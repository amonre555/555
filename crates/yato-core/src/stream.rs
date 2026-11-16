//! Lecture progressive.
//!
//! Un [`Download`] remplit un tampon en mémoire en arrière-plan, depuis HTTP ou depuis la
//! sortie d'un programme (yt-dlp). Un [`StreamReader`] lit ce tampon au fur et à mesure :
//! la lecture démarre dès les premières centaines de Ko reçus.
//!
//! Un `Download` peut être lancé à l'avance (anticipation) puis lu plus tard, plusieurs fois.
//! Il est arrêté quand plus personne ne le garde (ni cache, ni lecteur).
//!
//! Plusieurs sources peuvent remplir le même tampon en même temps, si elles livrent exactement
//! le même fichier (ex. yt-dlp, et l'adresse directe du flux YouTube) : chacune écrit à sa
//! propre position. Si l'une échoue, l'autre continue.
//!
//! Le tampon est fait de plages : une source HTTP qui accepte `Range` peut télécharger
//! n'importe quel endroit du fichier. Quand le lecteur attend un octet loin de ce qui arrive
//! (avance rapide, reprise au milieu d'un morceau), elle repart de cet octet ; ce qui est déjà
//! reçu est gardé, et les trous sont remplis ensuite. Une source sans `Range` (yt-dlp, serveur
//! qui renvoie tout le fichier) remplit le tampon du début à la fin, comme avant.

use std::{
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

use parking_lot::{Condvar, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{Error, Result};

/// Quantité minimum reçue avant de lancer le décodeur.
const PREBUFFER: usize = 128 * 1024;
/// Taille d'une plage pour [`Download::http`].
pub const CHUNK: u64 = 8 * 1024 * 1024;
/// Taille d'une plage pour la source HTTP ajoutée par [`Download::add_http`] : petite, pour
/// que le premier morceau arrive vite et qu'un refus en cours de route coûte peu.
const HTTP_CHUNK: u64 = 1024 * 1024;
/// Un octet attendu à moins de cette distance devant la requête en cours arrivera bientôt :
/// on ne la coupe pas.
const NEAR: u64 = 256 * 1024;
/// MP4 fragmenté : octets lus au début de chaque fragment (l'atome `moof` et le début du son)
/// pendant que le démuxeur parcourt les fragments jusqu'au point d'arrivée d'un saut.
const PEEK: u64 = 32 * 1024;
/// Nombre de fragments lus à l'avance pendant ce parcours.
const PEEK_AHEAD: usize = 8;

/// Plages reçues : triées, disjointes, et jamais contiguës (deux plages qui se touchent sont
/// fusionnées).
#[derive(Default)]
struct Parts(Vec<(u64, Vec<u8>)>);

impl Parts {
    fn end(&self, i: usize) -> u64 {
        self.0[i].0 + self.0[i].1.len() as u64
    }

    /// Octets reçus d'affilée depuis le début du fichier.
    fn prefix(&self) -> &[u8] {
        match self.0.first() {
            Some((0, b)) => b,
            _ => &[],
        }
    }

    /// Plage qui contient `pos`.
    fn find(&self, pos: u64) -> Option<usize> {
        let i = self.0.partition_point(|(s, b)| s + b.len() as u64 <= pos);
        (i < self.0.len() && self.0[i].0 <= pos).then_some(i)
    }

    /// Octets reçus d'affilée à partir de `pos` (vide s'il manque).
    fn at(&self, pos: u64) -> &[u8] {
        match self.find(pos) {
            Some(i) => &self.0[i].1[(pos - self.0[i].0) as usize..],
            None => &[],
        }
    }

    /// Premier octet manquant à partir de `from`, avant `len`.
    fn first_missing(&self, from: u64, len: u64) -> Option<u64> {
        let p = self.find(from).map_or(from, |i| self.end(i));
        (p < len).then_some(p)
    }

    /// Début de la première plage après `pos` (ou `len`).
    fn next_start(&self, pos: u64, len: u64) -> u64 {
        let i = self.0.partition_point(|(s, _)| *s <= pos);
        self.0.get(i).map_or(len, |(s, _)| (*s).min(len))
    }

    /// Écrit `data` à la position `off`, sans toucher aux octets déjà reçus. `reserve` : place
    /// à prévoir pour la plage du début. Renvoie `true` si quelque chose a été ajouté.
    fn insert(&mut self, off: u64, data: &[u8], reserve: usize) -> bool {
        let end = off + data.len() as u64;
        let (mut cur, mut changed) = (off, false);
        while cur < end {
            let i = self.0.partition_point(|(s, b)| s + b.len() as u64 <= cur);
            if i < self.0.len() && self.0[i].0 <= cur {
                cur = self.end(i); // déjà reçu
                continue;
            }
            let stop = self.0.get(i).map_or(end, |(s, _)| (*s).min(end));
            let bytes = &data[(cur - off) as usize..(stop - off) as usize];
            let j = if i > 0 && self.end(i - 1) == cur {
                self.0[i - 1].1.extend_from_slice(bytes);
                i - 1
            } else {
                let mut v = Vec::with_capacity(if cur == 0 { reserve.max(bytes.len()) } else { bytes.len() });
                v.extend_from_slice(bytes);
                self.0.insert(i, (cur, v));
                i
            };
            if j + 1 < self.0.len() && self.end(j) == self.0[j + 1].0 {
                let (_, next) = self.0.remove(j + 1);
                self.0[j].1.extend_from_slice(&next);
            }
            cur = stop;
            changed = true;
        }
        changed
    }
}

#[derive(Default)]
struct State {
    parts: Parts,
    done: bool,
    error: Option<String>,
    /// Taille totale attendue, si connue.
    total: Option<u64>,
    /// Octet qu'un lecteur attend (voir [`Fetcher`]).
    want: Option<u64>,
}

impl State {
    /// Tout le fichier est là.
    fn full(&self) -> bool {
        self.total.is_some_and(|t| t > 0 && self.parts.prefix().len() as u64 >= t)
    }
}

struct Shared {
    state: Mutex<State>,
    cond: Condvar,
    /// Réveille la source par plages quand un lecteur attend un nouvel octet.
    wake: tokio::sync::Notify,
    cancelled: AtomicBool,
    /// Sources encore en train d'écrire (voir [`Shared::writer_done`]).
    writers: AtomicUsize,
    /// Création du téléchargement (minutage).
    born: std::time::Instant,
}

impl Shared {
    /// Écrit `bytes`, qui commencent à la position `offset` du fichier. Seuls les octets pas
    /// encore reçus sont ajoutés (une autre source a peut-être déjà livré le reste).
    fn push_at(&self, offset: u64, bytes: &[u8]) {
        let mut st = self.state.lock();
        let reserve = st.total.unwrap_or(4 << 20) as usize;
        if st.parts.insert(offset, bytes, reserve) {
            drop(st);
            self.cond.notify_all();
        }
    }

    /// Tout le fichier est là (ou le téléchargement est terminé) : les autres sources s'arrêtent.
    fn complete(&self) -> bool {
        let st = self.state.lock();
        st.done || st.full()
    }

    /// Une source a fini. Le téléchargement se termine quand le fichier est complet, ou quand la
    /// dernière source s'arrête (son erreur, s'il y en a une, est alors celle du téléchargement).
    fn writer_done(&self, result: std::result::Result<(), String>) {
        let left = self.writers.fetch_sub(1, Ordering::AcqRel) - 1;
        if self.state.lock().full() {
            self.finish(Ok(()));
        } else if left == 0 {
            self.finish(result);
        }
    }

    fn finish(&self, result: std::result::Result<(), String>) {
        let mut st = self.state.lock();
        if let Err(e) = result {
            st.error = Some(e);
        }
        st.done = true;
        drop(st);
        self.cond.notify_all();
    }

    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Arrête le téléchargement quand le dernier [`Download`] / [`StreamReader`] disparaît.
/// (La tâche de téléchargement ne garde que `Shared`, pas ce garde.)
struct CancelOnDrop(Arc<Shared>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancelled.store(true, Ordering::Relaxed);
        self.0.wake.notify_one();
    }
}

impl std::fmt::Debug for Download {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let st = self.shared.state.lock();
        write!(f, "Download({} bytes, done: {})", st.parts.prefix().len(), st.done)
    }
}

/// Un téléchargement en cours (ou terminé) vers la mémoire.
#[derive(Clone)]
pub struct Download {
    shared: Arc<Shared>,
    guard: Arc<CancelOnDrop>,
}

impl Download {
    fn new(total: Option<u64>) -> (Self, Arc<Shared>) {
        let shared = Arc::new(Shared {
            state: Mutex::new(State { total, ..Default::default() }),
            cond: Condvar::new(),
            wake: tokio::sync::Notify::new(),
            cancelled: AtomicBool::new(false),
            writers: AtomicUsize::new(0),
            born: std::time::Instant::now(),
        });
        let guard = Arc::new(CancelOnDrop(shared.clone()));
        (Self { shared: shared.clone(), guard }, shared)
    }

    /// Téléchargement HTTP. `ranged_len` : si la taille est connue, téléchargement par plages
    /// de [`CHUNK`] octets (certains serveurs refusent un seul bloc), qui suit les sauts.
    pub async fn http(
        client: &reqwest::Client,
        url: &str,
        ranged_len: Option<u64>,
        user_agent: Option<String>,
    ) -> Result<Self> {
        if let Some(len) = ranged_len {
            let (dl, _) = Self::new(Some(len));
            dl.spawn_ranged(client, url, len, user_agent, CHUNK);
            return Ok(dl);
        }
        let mut resp = with_ua(client.get(url), user_agent.as_deref()).send().await?.error_for_status()?;
        let (dl, shared) = Self::new(resp.content_length());
        tokio::spawn(async move {
            let result = async {
                let mut pos = 0u64;
                while let Some(bytes) = resp.chunk().await.map_err(|e| e.to_string())? {
                    if shared.cancelled() {
                        return Ok(());
                    }
                    shared.push_at(pos, &bytes);
                    pos += bytes.len() as u64;
                }
                Ok(())
            }
            .await;
            shared.finish(result);
        });
        Ok(dl)
    }

    /// Adresse HTTP directe d'un fichier de `len` octets, seule source (Android : pas de
    /// yt-dlp). Téléchargée par plages de 1 Mo, comme [`Download::add_http`].
    pub fn direct(client: &reqwest::Client, url: &str, len: u64) -> Self {
        let (dl, _) = Self::new(Some(len));
        dl.add_http(client, url, len);
        dl
    }

    /// Sortie standard d'un programme (ex. `yt-dlp -o -`). Le programme est tué si le
    /// téléchargement est abandonné.
    ///
    /// `attempts` : jeux d'arguments essayés dans l'ordre. Si un essai échoue **avant d'avoir
    /// envoyé la moindre donnée**, on passe au suivant (ex. mode rapide, puis mode complet).
    pub fn process(program: &Path, attempts: Vec<Vec<String>>) -> Result<Self> {
        if attempts.is_empty() {
            return Err(Error::Other("no arguments for the program".into()));
        }
        let program = program.to_owned();
        Ok(Self::process_when_ready(async move { Ok((program, attempts)) }))
    }

    /// Comme [`Download::process`], mais le programme n'est connu qu'une fois `prepare` terminé
    /// (ex. yt-dlp en cours d'installation). Le téléchargement existe tout de suite : une autre
    /// source ([`Download::add_http`]) peut le remplir en attendant.
    pub fn process_when_ready<F>(prepare: F) -> Self
    where
        F: std::future::Future<Output = Result<(std::path::PathBuf, Vec<Vec<String>>)>> + Send + 'static,
    {
        let (dl, shared) = Self::new(None);
        shared.writers.fetch_add(1, Ordering::AcqRel);
        tokio::spawn(async move {
            let (program, attempts) = match prepare.await {
                Ok(p) => p,
                Err(e) => return shared.writer_done(Err(e.to_string())),
            };
            let mut result = Err("no attempt".to_owned());
            let mut written = 0u64;
            for args in attempts {
                if shared.cancelled() || shared.complete() {
                    result = Ok(());
                    break;
                }
                result = run_process(&shared, &program, &args, &mut written).await;
                if result.is_ok() || written > 0 || shared.cancelled() || shared.complete() {
                    break;
                }
            }
            // Fin de flux : la taille exacte est maintenant connue.
            if result.is_ok() && !shared.complete() {
                let mut st = shared.state.lock();
                if written >= st.parts.prefix().len() as u64 {
                    st.total = Some(written);
                }
            }
            shared.writer_done(result);
        });
        dl
    }

    /// Ajoute une deuxième source : l'adresse HTTP directe du **même** fichier (`len` octets).
    /// Elle remplit le tampon en parallèle de la source actuelle, à partir de ce qui est déjà
    /// reçu, et suit les sauts du lecteur (voir [`Fetcher`]). Un échec de cette source (refus
    /// du serveur en cours de route…) ne compte pas tant que l'autre continue.
    pub fn add_http(&self, client: &reqwest::Client, url: &str, len: u64) {
        if self.shared.complete() || len == 0 {
            return;
        }
        self.set_len_hint(len);
        self.spawn_ranged(client, url, len, None, HTTP_CHUNK);
    }

    fn spawn_ranged(&self, client: &reqwest::Client, url: &str, len: u64, ua: Option<String>, chunk: u64) {
        let shared = self.shared.clone();
        shared.writers.fetch_add(1, Ordering::AcqRel);
        let fetcher = Fetcher {
            shared: shared.clone(),
            client: client.clone(),
            url: url.to_owned(),
            ua,
            len,
            chunk,
            no_range: AtomicBool::new(false),
            no_peek: Arc::new(AtomicBool::new(false)),
            first: AtomicBool::new(true),
        };
        tokio::spawn(async move {
            let result = fetcher.run().await;
            if crate::timing::enabled() {
                let have = shared.state.lock().parts.prefix().len();
                crate::timing::step(&format!("flux direct : fin ({result:?}, tampon {} Ko / {} Ko)", have / 1024, len / 1024), shared.born);
            }
            shared.writer_done(result);
        });
    }

    /// Indique la taille attendue (permet l'avance rapide avant la fin du téléchargement).
    pub fn set_len_hint(&self, len: u64) {
        let mut st = self.shared.state.lock();
        if !st.done {
            st.total = Some(len);
        }
    }

    pub fn total_len(&self) -> Option<u64> {
        self.shared.state.lock().total
    }

    /// Échec sans aucune donnée reçue.
    pub fn failed(&self) -> bool {
        let st = self.shared.state.lock();
        st.done && st.error.is_some() && st.parts.0.is_empty()
    }

    /// Attend d'avoir assez de données pour démarrer, puis renvoie un lecteur.
    pub async fn reader(&self) -> Result<StreamReader> {
        let t0 = std::time::Instant::now();
        let waiter = self.shared.clone();
        tokio::task::spawn_blocking(move || {
            let mut st = waiter.state.lock();
            while st.parts.prefix().len() < PREBUFFER && !st.done {
                waiter.cond.wait(&mut st);
            }
        })
        .await
        .ok();
        crate::timing::step("tampon : attente des 128 premiers Ko", t0);
        let st = self.shared.state.lock();
        if st.parts.prefix().is_empty() {
            return Err(Error::Other(st.error.clone().unwrap_or_else(|| "empty stream".into())));
        }
        Ok(StreamReader { shared: self.shared.clone(), _guard: self.guard.clone(), pos: 0, quick_open: None })
    }
}

/// Source HTTP par plages. Elle télécharge le fichier par requêtes `Range` de `chunk` octets,
/// en partant du premier octet manquant après sa position. Quand un lecteur attend un octet
/// manquant loin de la requête en cours, elle la coupe et repart de cet octet. Arrivée à la
/// fin du fichier, elle revient remplir les trous laissés derrière.
///
/// MP4 fragmenté (m4a de YouTube) : pour un saut, le démuxeur lit tous les atomes `moof` un
/// par un jusqu'au point d'arrivée, en sautant les `mdat` (le son). Quand il attend un `moof`
/// (positions lues dans l'atome `sidx`), on lit les [`PEEK_AHEAD`] fragments suivants en
/// parallèle, quelques Ko chacun, au lieu de tout télécharger.
///
/// Si le serveur ignore `Range` (réponse 200), le fichier entier arrive du début : on le lit
/// jusqu'au bout, sans suivre les sauts.
struct Fetcher {
    shared: Arc<Shared>,
    client: reqwest::Client,
    url: String,
    ua: Option<String>,
    len: u64,
    chunk: u64,
    no_range: AtomicBool,
    /// Une lecture de fragment a échoué : on ne lit plus les fragments à part.
    no_peek: Arc<AtomicBool>,
    first: AtomicBool,
}

/// Fragments d'un MP4 (début de chaque `moof`, d'après `sidx`), et ceux déjà demandés.
#[derive(Default)]
struct Peeks {
    index: Option<Vec<u64>>,
    asked: Vec<bool>,
}

impl Fetcher {
    async fn run(&self) -> std::result::Result<(), String> {
        let mut cursor = 0u64;
        let mut peeks = Peeks::default();
        loop {
            if self.shared.cancelled() || self.shared.complete() {
                return Ok(());
            }
            if let Some(w) = self.on_want(&mut peeks, cursor) {
                cursor = w;
            }
            let (start, end) = {
                let st = self.shared.state.lock();
                let Some(start) = st.parts.first_missing(cursor, self.len).or_else(|| st.parts.first_missing(0, self.len))
                else {
                    return Ok(());
                };
                (start, (start + self.chunk).min(st.parts.next_start(start, self.len)))
            };
            let pos = AtomicU64::new(start);
            let restart = {
                let fetch = self.fetch(start, end, &pos);
                tokio::pin!(fetch);
                loop {
                    tokio::select! {
                        r = &mut fetch => {
                            r?;
                            break None;
                        }
                        _ = self.shared.wake.notified() => {
                            if self.shared.cancelled() {
                                return Ok(());
                            }
                            if let Some(w) = self.on_want(&mut peeks, pos.load(Ordering::Relaxed)) {
                                break Some(w);
                            }
                        }
                    }
                }
            };
            cursor = restart.unwrap_or_else(|| pos.load(Ordering::Relaxed));
        }
    }

    /// Un lecteur attend peut-être un octet manquant. Renvoie la position d'où repartir, si la
    /// requête en cours (à la position `pos`) ne l'apportera pas bientôt.
    fn on_want(&self, peeks: &mut Peeks, pos: u64) -> Option<u64> {
        if self.no_range.load(Ordering::Relaxed) {
            return None;
        }
        let st = self.shared.state.lock();
        let w = st.want?;
        if w >= self.len || !st.parts.at(w).is_empty() || (pos <= w && w < pos + NEAR) {
            return None;
        }
        if !self.no_peek.load(Ordering::Relaxed) {
            if peeks.index.is_none() {
                peeks.index = mp4_fragments(st.parts.prefix());
                peeks.asked = vec![false; peeks.index.as_ref().map_or(0, Vec::len)];
            }
            if let Some(k) = peeks.index.as_ref().and_then(|ix| ix.binary_search(&w).ok()) {
                let index = peeks.index.as_ref().expect("index");
                for j in k..(k + PEEK_AHEAD).min(index.len()) {
                    let start = index[j];
                    if std::mem::replace(&mut peeks.asked[j], true) || !st.parts.at(start).is_empty() {
                        continue;
                    }
                    self.spawn_peek(start, (start + PEEK).min(self.len));
                }
                return None;
            }
        }
        Some(w)
    }

    /// Lit `start..end` à part (début d'un fragment MP4).
    fn spawn_peek(&self, start: u64, end: u64) {
        let (shared, client, url, ua) = (self.shared.clone(), self.client.clone(), self.url.clone(), self.ua.clone());
        let no_peek = self.no_peek.clone();
        tokio::spawn(async move {
            let result = async {
                let mut resp = range_request(&client, &url, ua.as_deref(), start, end).await?;
                if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                    return Err(Error::Other("no range support".into()));
                }
                let mut pos = start;
                while let Some(bytes) = resp.chunk().await? {
                    if shared.cancelled() {
                        break;
                    }
                    shared.push_at(pos, &bytes);
                    pos += bytes.len() as u64;
                }
                Ok(())
            }
            .await;
            if result.is_err() {
                // La source principale prendra le relais.
                no_peek.store(true, Ordering::Relaxed);
                shared.wake.notify_one();
            }
        });
    }

    /// Télécharge `start..end` dans le tampon. `pos` : position atteinte.
    async fn fetch(&self, start: u64, end: u64, pos: &AtomicU64) -> std::result::Result<(), String> {
        let mut resp = range_request(&self.client, &self.url, self.ua.as_deref(), start, end)
            .await
            .map_err(|e| e.to_string())?;
        if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            // `Range` ignoré : tout le fichier arrive, depuis le début.
            self.no_range.store(true, Ordering::Relaxed);
            pos.store(0, Ordering::Relaxed);
        }
        while let Some(bytes) = resp.chunk().await.map_err(|e| e.to_string())? {
            if self.shared.cancelled() || self.shared.complete() {
                return Ok(());
            }
            if self.first.swap(false, Ordering::Relaxed) {
                crate::timing::step("flux direct : premiers octets (depuis le début du téléchargement)", self.shared.born);
            }
            let at = pos.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            self.shared.push_at(at, &bytes);
        }
        Ok(())
    }
}

/// Début de chaque fragment d'un MP4 fragmenté, d'après l'atome `sidx` (s'il est dans `buf`,
/// avant le premier fragment).
fn mp4_fragments(buf: &[u8]) -> Option<Vec<u64>> {
    let mut pos = 0usize;
    while pos + 8 <= buf.len() {
        let size = u32::from_be_bytes(buf[pos..pos + 4].try_into().ok()?) as usize;
        let kind = &buf[pos + 4..pos + 8];
        if size < 8 || pos + size > buf.len() || kind == b"moof" || kind == b"mdat" {
            return None;
        }
        if kind == b"sidx" {
            return parse_sidx(&buf[pos + 8..pos + size], (pos + size) as u64);
        }
        pos += size;
    }
    None
}

/// Corps d'un atome `sidx` qui finit à la position `end` du fichier.
fn parse_sidx(b: &[u8], end: u64) -> Option<Vec<u64>> {
    let u16_at = |i: usize| Some(u16::from_be_bytes(b.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |i: usize| Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?));
    let u64_at = |i: usize| Some(u64::from_be_bytes(b.get(i..i + 8)?.try_into().ok()?));
    // version, drapeaux, reference_ID, timescale, earliest_pts, first_offset, réservé, nombre.
    let (first_offset, count_at) = match *b.first()? {
        0 => (u64::from(u32_at(16)?), 22),
        _ => (u64_at(20)?, 30),
    };
    let count = usize::from(u16_at(count_at)?);
    let mut at = end + first_offset;
    let mut out = Vec::with_capacity(count);
    for r in 0..count {
        let reference = u32_at(count_at + 2 + r * 12)?;
        if reference >> 31 != 0 {
            return None; // renvoie vers un autre index : pas le cas de YouTube
        }
        out.push(at);
        at += u64::from(reference & 0x7fff_ffff);
    }
    Some(out)
}

fn with_ua(req: reqwest::RequestBuilder, ua: Option<&str>) -> reqwest::RequestBuilder {
    match ua {
        Some(ua) => req.header(reqwest::header::USER_AGENT, ua),
        None => req,
    }
}

/// Requête des octets `start..end` (`end` exclu).
async fn range_request(
    client: &reqwest::Client,
    url: &str,
    ua: Option<&str>,
    start: u64,
    end: u64,
) -> Result<reqwest::Response> {
    Ok(with_ua(client.get(url), ua)
        .header(reqwest::header::RANGE, format!("bytes={start}-{}", end - 1))
        .send()
        .await?
        .error_for_status()?)
}

/// Lance le programme et copie sa sortie dans le tampon.
/// `written` : octets déjà écrits par ce programme (sa position dans le fichier).
async fn run_process(shared: &Shared, program: &Path, args: &[String], written: &mut u64) -> std::result::Result<(), String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW : pas de console qui clignote
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", program.display()))?;
    let mut stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let err_task = tokio::spawn(async move { read_all(&mut stderr).await });
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if shared.cancelled() || shared.complete() {
            return Ok(()); // `child` est tué en sortant (kill_on_drop)
        }
        let n = stdout.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if *written == 0 {
            crate::timing::step("yt-dlp : premiers octets (depuis le début du téléchargement)", shared.born);
        }
        shared.push_at(*written, &buf[..n]);
        *written += n as u64;
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    if status.success() {
        return Ok(());
    }
    let err = err_task.await.unwrap_or_default();
    let text = String::from_utf8_lossy(&err);
    let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("unknown error");
    Err(last.trim().to_owned())
}

async fn read_all<R: AsyncRead + Unpin>(r: &mut R) -> Vec<u8> {
    let mut v = Vec::new();
    let _ = r.read_to_end(&mut v).await;
    v
}

/// Lecteur `Read + Seek` sur un [`Download`].
pub struct StreamReader {
    shared: Arc<Shared>,
    _guard: Arc<CancelOnDrop>,
    pos: u64,
    /// Ouverture rapide d'un MP4 fragmenté, voir [`StreamReader::quick_open_mp4`].
    quick_open: Option<QuickOpen>,
}

/// Pendant l'ouverture du décodeur : au-delà de `cutoff`, le fichier est présenté comme un
/// seul atome `free` jusqu'à la fin (`len`), sans attendre le téléchargement.
struct QuickOpen {
    cutoff: u64,
    len: u64,
    /// Le faux atome a été lu au moins une fois.
    served: bool,
    /// Mis à `true` quand le décodeur est ouvert (voir [`QuickOpenGuard`]).
    over: Arc<AtomicBool>,
}

/// Termine l'ouverture rapide quand il disparaît (à garder pendant l'ouverture du décodeur).
pub struct QuickOpenGuard(Option<Arc<AtomicBool>>);

impl Drop for QuickOpenGuard {
    fn drop(&mut self) {
        if let Some(over) = &self.0 {
            over.store(true, Ordering::Relaxed);
        }
    }
}

impl StreamReader {
    pub fn total_len(&self) -> Option<u64> {
        self.shared.state.lock().total
    }

    /// À appeler juste avant d'ouvrir le décodeur, pour un MP4 fragmenté (le m4a de YouTube :
    /// `ftyp`, `moov`, `sidx`, puis des dizaines de paires `moof` + `mdat` sur tout le fichier).
    ///
    /// Quand la source est « seekable », le démuxeur MP4 de symphonia parcourt **tous** les
    /// atomes du fichier avant de rendre la main : il faudrait attendre le téléchargement
    /// complet avant le premier son. Ici, tant qu'il n'est pas revenu au début du fichier (ce
    /// qu'il fait à la fin de ce parcours), tout ce qui suit les atomes déjà reçus en entier lui
    /// apparaît comme un atome `free` (vide) jusqu'à la fin. Il ne retient de ce parcours que
    /// `ftyp` et `moov` (les fragments sont relus ensuite, au fil de la lecture) : rien n'est
    /// perdu, et l'avance rapide reste possible.
    ///
    /// Sans effet si le fichier est déjà complet, n'est pas un MP4, ou si `moov` manque encore.
    /// Le garde renvoyé est à lâcher une fois le décodeur ouvert.
    #[must_use]
    pub fn quick_open_mp4(&mut self) -> QuickOpenGuard {
        let st = self.shared.state.lock();
        let prefix = st.parts.prefix();
        let Some(len) = st.total.filter(|&t| t > prefix.len() as u64) else { return QuickOpenGuard(None) };
        match complete_mp4_prefix(prefix) {
            Some(cutoff) if len - cutoff >= 8 && len - cutoff <= u64::from(u32::MAX) => {
                let over = Arc::new(AtomicBool::new(false));
                self.quick_open = Some(QuickOpen { cutoff, len, served: false, over: over.clone() });
                QuickOpenGuard(Some(over))
            }
            _ => QuickOpenGuard(None),
        }
    }

    fn quick_open_active(&mut self) -> Option<&mut QuickOpen> {
        if self.quick_open.as_ref().is_some_and(|q| q.over.load(Ordering::Relaxed)) {
            self.quick_open = None;
        }
        self.quick_open.as_mut()
    }
}

/// Fin des atomes de premier niveau d'un MP4 entièrement présents dans `buf`, si `ftyp` et
/// `moov` en font partie.
fn complete_mp4_prefix(buf: &[u8]) -> Option<u64> {
    let (mut pos, mut ftyp, mut moov) = (0usize, false, false);
    while pos + 8 <= buf.len() {
        let size = u32::from_be_bytes(buf[pos..pos + 4].try_into().ok()?) as usize;
        let kind = &buf[pos + 4..pos + 8];
        if size < 8 || pos + size > buf.len() {
            break; // atome incomplet (ou taille sur 64 bits, rare : on s'arrête là)
        }
        ftyp |= kind == b"ftyp";
        moov |= kind == b"moov";
        pos += size;
    }
    (ftyp && moov).then_some(pos as u64)
}

impl Read for StreamReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let pos = self.pos;
        if let Some(q) = self.quick_open_active()
            && pos >= q.cutoff
        {
            // Faux atome `free` : en-tête (taille, type), puis des zéros.
            if pos >= q.len || out.is_empty() {
                return Ok(0);
            }
            q.served = true;
            let mut header = [0u8; 8];
            header[..4].copy_from_slice(&((q.len - q.cutoff) as u32).to_be_bytes());
            header[4..].copy_from_slice(b"free");
            let n = out.len().min((q.len - pos) as usize);
            for (i, b) in out[..n].iter_mut().enumerate() {
                let at = pos - q.cutoff + i as u64;
                *b = if at < 8 { header[at as usize] } else { 0 };
            }
            self.pos += n as u64;
            return Ok(n);
        }
        // Pendant l'ouverture rapide, ne jamais déborder sur le faux atome.
        let limit = self.quick_open.as_ref().map_or(usize::MAX, |q| (q.cutoff - self.pos) as usize);
        let mut st = self.shared.state.lock();
        // Attendre que les données demandées soient arrivées, en disant à la source par plages
        // où l'on en est.
        loop {
            if st.total.is_some_and(|t| t > 0 && self.pos >= t) {
                return Ok(0);
            }
            if st.done || !st.parts.at(self.pos).is_empty() {
                break;
            }
            if st.want != Some(self.pos) {
                st.want = Some(self.pos);
                self.shared.wake.notify_one();
            }
            self.shared.cond.wait(&mut st);
        }
        let avail = st.parts.at(self.pos);
        if avail.is_empty() {
            return match &st.error {
                Some(e) => Err(io::Error::other(e.clone())),
                None => Ok(0),
            };
        }
        let n = out.len().min(avail.len()).min(limit);
        out[..n].copy_from_slice(&avail[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for StreamReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let end = if let SeekFrom::End(_) = from {
            let mut st = self.shared.state.lock();
            match st.total {
                Some(t) => t,
                None => {
                    // Taille inconnue : il faut attendre la fin pour connaître la fin.
                    while !st.done {
                        self.shared.cond.wait(&mut st);
                    }
                    st.parts.prefix().len() as u64
                }
            }
        } else {
            0
        };
        let new = match from {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::Current(d) => self.pos as i64 + d,
            SeekFrom::End(d) => end as i64 + d,
        };
        if new < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "negative position"));
        }
        self.pos = new as u64;
        // Le démuxeur revient au début après son parcours : le vrai fichier reprend sa place.
        let pos = self.pos;
        if self.quick_open_active().is_some_and(|q| q.served && pos < q.cutoff) {
            self.quick_open = None;
        }
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(kind: &[u8; 4], body: usize) -> Vec<u8> {
        let mut a = ((body + 8) as u32).to_be_bytes().to_vec();
        a.extend_from_slice(kind);
        a.resize(body + 8, 7);
        a
    }

    #[test]
    fn mp4_prefix_stops_at_the_first_incomplete_atom() {
        let mut buf = [atom(b"ftyp", 16), atom(b"moov", 100), atom(b"moof", 50)].concat();
        let complete = buf.len() as u64;
        buf.extend_from_slice(&atom(b"mdat", 1000)[..300]);
        assert_eq!(complete_mp4_prefix(&buf), Some(complete));
        // Sans `moov`, pas d'ouverture rapide.
        assert_eq!(complete_mp4_prefix(&[atom(b"ftyp", 16), atom(b"moof", 50)].concat()), None);
        assert_eq!(complete_mp4_prefix(b"ID3\x04 pas un mp4"), None);
    }

    #[test]
    fn quick_open_hides_the_missing_part_until_the_demuxer_rewinds() {
        let head = [atom(b"ftyp", 16), atom(b"moov", 100)].concat();
        let mdat = atom(b"mdat", 1000);
        let len = (head.len() + mdat.len()) as u64;
        let (dl, shared) = Download::new(Some(len));
        shared.writers.fetch_add(1, Ordering::AcqRel);
        shared.push_at(0, &head);
        shared.push_at(head.len() as u64, &mdat[..100]);
        let mut r = StreamReader { shared: shared.clone(), _guard: dl.guard.clone(), pos: 0, quick_open: None };
        let guard = r.quick_open_mp4();

        // Au-delà des atomes complets : un atome `free` jusqu'à la fin, lu sans attendre.
        r.seek(SeekFrom::Start(head.len() as u64)).unwrap();
        let mut h = [0u8; 8];
        r.read_exact(&mut h).unwrap();
        assert_eq!(u32::from_be_bytes(h[..4].try_into().unwrap()) as usize, mdat.len());
        assert_eq!(&h[4..], b"free");

        // Retour au début : le vrai fichier reprend sa place.
        r.seek(SeekFrom::Start(0)).unwrap();
        drop(guard);
        let mut all = vec![0u8; head.len() + 100];
        r.read_exact(&mut all).unwrap();
        assert_eq!(&all[head.len()..], &mdat[..100]);
    }

    #[test]
    fn parts_merge_and_keep_received_bytes() {
        let mut p = Parts::default();
        assert!(p.insert(10, &[1; 5], 0));
        assert!(p.insert(0, &[2; 5], 0));
        assert_eq!(p.0.len(), 2);
        assert_eq!(p.first_missing(0, 100), Some(5));
        assert_eq!(p.next_start(5, 100), 10);
        // Comble le trou en débordant des deux côtés : rien de déjà reçu n'est remplacé.
        assert!(p.insert(3, &[3; 14], 0));
        assert_eq!(p.0.len(), 1);
        assert_eq!(p.prefix(), &[2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 1, 1, 1, 1, 1, 3, 3]);
        assert!(!p.insert(0, &[9; 17], 0));
        assert_eq!(p.at(15), &[3, 3]);
        assert_eq!(p.first_missing(4, 17), None);
    }

    #[test]
    fn sidx_gives_the_fragment_positions() {
        let mut body = vec![0u8; 4]; // version 0, drapeaux
        body.extend_from_slice(&1u32.to_be_bytes()); // reference_ID
        body.extend_from_slice(&44100u32.to_be_bytes()); // timescale
        body.extend_from_slice(&0u32.to_be_bytes()); // earliest_pts
        body.extend_from_slice(&0u32.to_be_bytes()); // first_offset
        body.extend_from_slice(&[0, 0, 0, 2]); // réservé, 2 fragments
        for size in [1000u32, 2000] {
            body.extend_from_slice(&size.to_be_bytes());
            body.extend_from_slice(&[0; 8]);
        }
        let sidx = [((body.len() + 8) as u32).to_be_bytes().to_vec(), b"sidx".to_vec(), body].concat();
        let buf = [atom(b"ftyp", 16), atom(b"moov", 100), sidx].concat();
        let end = buf.len() as u64;
        assert_eq!(mp4_fragments(&buf), Some(vec![end, end + 1000]));
        assert_eq!(mp4_fragments(&[atom(b"ftyp", 16), atom(b"moof", 10)].concat()), None);
    }

    /// Faux serveur HTTP local. `ranges` : gère l'en-tête `Range` (sinon renvoie tout le
    /// fichier). Note chaque requête (`début..fin` servi).
    struct Server {
        url: String,
        log: Arc<Mutex<Vec<(u64, u64)>>>,
    }

    impl Server {
        fn start(data: Arc<Vec<u8>>, ranges: bool) -> Self {
            use std::io::{BufRead, BufReader, Write};
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/song.m4a", listener.local_addr().unwrap());
            let log = Arc::new(Mutex::new(Vec::new()));
            let log2 = log.clone();
            std::thread::spawn(move || {
                for conn in listener.incoming().flatten() {
                    let (data, log) = (data.clone(), log2.clone());
                    std::thread::spawn(move || {
                        let mut reader = BufReader::new(conn.try_clone().unwrap());
                        let mut range = None;
                        loop {
                            let mut line = String::new();
                            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                                break;
                            }
                            if let Some(r) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                                let (a, b) = r.trim().split_once('-').unwrap();
                                range = Some((a.parse::<u64>().unwrap(), b.parse::<u64>().unwrap() + 1));
                            }
                        }
                        let len = data.len() as u64;
                        let (start, end, head) = match range.filter(|_| ranges) {
                            Some((a, b)) => {
                                let b = b.min(len);
                                (a, b, format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {a}-{}/{len}\r\n", b - 1))
                            }
                            None => (0, len, "HTTP/1.1 200 OK\r\n".to_owned()),
                        };
                        log.lock().push((start, end));
                        let mut conn = conn;
                        let head = format!("{head}Content-Length: {}\r\nConnection: close\r\n\r\n", end - start);
                        if conn.write_all(head.as_bytes()).is_err() {
                            return;
                        }
                        // Lent : 16 Ko toutes les 2 ms (≈ 8 Mo/s).
                        for piece in data[start as usize..end as usize].chunks(16 * 1024) {
                            if conn.write_all(piece).is_err() {
                                return;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        }
                    });
                }
            });
            Self { url, log }
        }

        /// Nombre de requêtes qui ont servi l'octet `at`.
        fn served(&self, at: u64) -> usize {
            self.log.lock().iter().filter(|(a, b)| *a <= at && at < *b).count()
        }
    }

    const SIZE: usize = 8 << 20;

    fn song() -> Arc<Vec<u8>> {
        Arc::new((0..SIZE).map(|i| (i % 251) as u8).collect())
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    /// Lit `n` octets à `at` (dans un fil à part : la lecture attend le réseau).
    async fn read_at(r: StreamReader, at: SeekFrom, n: usize) -> (StreamReader, Vec<u8>) {
        tokio::task::spawn_blocking(move || {
            let mut r = r;
            r.seek(at).unwrap();
            let mut out = vec![0u8; n];
            r.read_exact(&mut out).unwrap();
            (r, out)
        })
        .await
        .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn jump_forward_downloads_from_the_target() {
        let data = song();
        let server = Server::start(data.clone(), true);
        let dl = Download::direct(&client(), &server.url, SIZE as u64);
        let r = dl.reader().await.unwrap();
        let at = 6 * (1 << 20) + 123;
        let (_r, got) = read_at(r, SeekFrom::Start(at as u64), 1000).await;
        assert_eq!(got, data[at..at + 1000]);
        // Une requête part de l'octet demandé, et le milieu n'a pas été téléchargé avant.
        assert!(server.log.lock().iter().any(|(a, _)| *a == at as u64), "{:?}", server.log.lock());
        assert_eq!(server.served(5 << 20), 0, "{:?}", server.log.lock());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn going_back_reuses_what_was_received() {
        let data = song();
        let server = Server::start(data.clone(), true);
        let dl = Download::direct(&client(), &server.url, SIZE as u64);
        let r = dl.reader().await.unwrap();
        let (r, _) = read_at(r, SeekFrom::Start(0), 200 * 1024).await;
        let far = 6 << 20;
        let (r, got) = read_at(r, SeekFrom::Start(far), 1000).await;
        assert_eq!(got, data[far as usize..far as usize + 1000]);
        let (r, got) = read_at(r, SeekFrom::Start(100 * 1024), 50 * 1024).await;
        assert_eq!(got, data[100 * 1024..150 * 1024]);
        // Tout le fichier finit par arriver (les trous sont remplis), sans rien retélécharger.
        let (_r, all) = read_at(r, SeekFrom::Start(0), SIZE).await;
        assert!(all == *data);
        assert_eq!(server.served(100 * 1024), 1, "{:?}", server.log.lock());
        assert_eq!(server.served(far), 1, "{:?}", server.log.lock());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn end_of_file() {
        let data = song();
        let server = Server::start(data.clone(), true);
        let dl = Download::direct(&client(), &server.url, SIZE as u64);
        let r = dl.reader().await.unwrap();
        let (r, got) = read_at(r, SeekFrom::End(-100), 100).await;
        assert_eq!(got, data[SIZE - 100..]);
        tokio::task::spawn_blocking(move || {
            let mut r = r;
            let mut buf = [0u8; 10];
            assert_eq!(r.read(&mut buf).unwrap(), 0);
            r.seek(SeekFrom::Start(SIZE as u64 + 50)).unwrap();
            assert_eq!(r.read(&mut buf).unwrap(), 0);
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn source_without_range_reads_from_the_start() {
        let data = song();
        let server = Server::start(data.clone(), false);
        let dl = Download::direct(&client(), &server.url, SIZE as u64);
        let r = dl.reader().await.unwrap();
        let at = 6 << 20;
        let (_r, got) = read_at(r, SeekFrom::Start(at as u64), 1000).await;
        assert_eq!(got, data[at..at + 1000]);
        // Une seule requête, tout le fichier depuis le début.
        assert_eq!(*server.log.lock(), vec![(0, SIZE as u64)]);
    }
}
