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
//! propre position, et la plus avancée fait grandir le tampon. Si l'une échoue, l'autre continue.

use std::{
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use parking_lot::{Condvar, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{Error, Result};

/// Quantité minimum reçue avant de lancer le décodeur.
const PREBUFFER: usize = 128 * 1024;
/// Taille d'une plage en mode par plages.
pub const CHUNK: u64 = 8 * 1024 * 1024;
/// Taille d'une plage pour la source HTTP ajoutée par [`Download::add_http`] : petite, pour
/// que le premier morceau arrive vite et qu'un refus en cours de route coûte peu.
const HTTP_CHUNK: u64 = 1024 * 1024;

#[derive(Default)]
struct State {
    buf: Vec<u8>,
    done: bool,
    error: Option<String>,
    /// Taille totale attendue, si connue.
    total: Option<u64>,
}

struct Shared {
    state: Mutex<State>,
    cond: Condvar,
    cancelled: AtomicBool,
    /// Sources encore en train d'écrire (voir [`Shared::writer_done`]).
    writers: AtomicUsize,
}

impl Shared {
    fn push(&self, bytes: &[u8]) {
        self.state.lock().buf.extend_from_slice(bytes);
        self.cond.notify_all();
    }

    /// Écrit `bytes`, qui commencent à la position `offset` du fichier. Seule la partie qui
    /// dépasse le tampon actuel est ajoutée (une autre source a peut-être déjà livré le reste).
    fn push_at(&self, offset: u64, bytes: &[u8]) {
        let mut st = self.state.lock();
        let have = st.buf.len() as u64;
        let end = offset + bytes.len() as u64;
        if offset <= have && end > have {
            st.buf.extend_from_slice(&bytes[(have - offset) as usize..]);
            drop(st);
            self.cond.notify_all();
        }
    }

    /// Tout le fichier est là (ou le téléchargement est terminé) : les autres sources s'arrêtent.
    fn complete(&self) -> bool {
        let st = self.state.lock();
        st.done || st.total.is_some_and(|t| t > 0 && st.buf.len() as u64 >= t)
    }

    /// Une source a fini. Le téléchargement se termine quand le fichier est complet, ou quand la
    /// dernière source s'arrête (son erreur, s'il y en a une, est alors celle du téléchargement).
    fn writer_done(&self, result: std::result::Result<(), String>) {
        let left = self.writers.fetch_sub(1, Ordering::AcqRel) - 1;
        let complete = {
            let st = self.state.lock();
            st.total.is_some_and(|t| t > 0 && st.buf.len() as u64 >= t)
        };
        if complete {
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
    }
}

impl std::fmt::Debug for Download {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let st = self.shared.state.lock();
        write!(f, "Download({} bytes, done: {})", st.buf.len(), st.done)
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
            state: Mutex::new(State {
                buf: Vec::with_capacity(total.unwrap_or(4 << 20) as usize),
                total,
                ..Default::default()
            }),
            cond: Condvar::new(),
            cancelled: AtomicBool::new(false),
            writers: AtomicUsize::new(0),
        });
        let guard = Arc::new(CancelOnDrop(shared.clone()));
        (Self { shared: shared.clone(), guard }, shared)
    }

    /// Téléchargement HTTP. `ranged_len` : si la taille est connue, téléchargement par plages
    /// successives de [`CHUNK`] octets (certains serveurs refusent un seul bloc).
    pub async fn http(
        client: &reqwest::Client,
        url: &str,
        ranged_len: Option<u64>,
        user_agent: Option<String>,
    ) -> Result<Self> {
        let ua = user_agent.as_deref();
        let (first, total) = match ranged_len {
            Some(len) => (range_request(client, url, ua, 0, len, CHUNK).await?, Some(len)),
            None => {
                let resp = with_ua(client.get(url), ua).send().await?.error_for_status()?;
                let len = resp.content_length();
                (resp, len)
            }
        };
        let (dl, shared) = Self::new(total);
        let client = client.clone();
        let url = url.to_owned();
        tokio::spawn(async move {
            let mut resp = first;
            let result = async {
                loop {
                    while let Some(bytes) = resp.chunk().await.map_err(|e| e.to_string())? {
                        if shared.cancelled() {
                            return Ok(());
                        }
                        shared.push(&bytes);
                    }
                    let Some(len) = ranged_len else { return Ok(()) };
                    let have = shared.state.lock().buf.len() as u64;
                    if have >= len {
                        return Ok(());
                    }
                    resp = range_request(&client, &url, user_agent.as_deref(), have, len, CHUNK)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            .await;
            shared.finish(result);
        });
        Ok(dl)
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
        let (dl, shared) = Self::new(None);
        shared.writers.fetch_add(1, Ordering::AcqRel);
        let program = program.to_owned();
        tokio::spawn(async move {
            let mut result = Err("no attempt".to_owned());
            let mut written = 0u64;
            for args in attempts {
                result = run_process(&shared, &program, &args, &mut written).await;
                if result.is_ok() || written > 0 || shared.cancelled() || shared.complete() {
                    break;
                }
            }
            // Fin de flux : la taille exacte est maintenant connue.
            if result.is_ok() && !shared.complete() {
                let mut st = shared.state.lock();
                if written >= st.buf.len() as u64 {
                    st.total = Some(written);
                }
            }
            shared.writer_done(result);
        });
        Ok(dl)
    }

    /// Ajoute une deuxième source : l'adresse HTTP directe du **même** fichier (`len` octets).
    /// Elle remplit le tampon en parallèle de la source actuelle, à partir de ce qui est déjà
    /// reçu ; la plus rapide l'emporte. Un échec de cette source (refus du serveur en cours de
    /// route…) ne compte pas tant que l'autre continue.
    pub fn add_http(&self, client: &reqwest::Client, url: &str, len: u64) {
        let shared = self.shared.clone();
        if shared.complete() || len == 0 {
            return;
        }
        self.set_len_hint(len);
        shared.writers.fetch_add(1, Ordering::AcqRel);
        let (client, url) = (client.clone(), url.to_owned());
        tokio::spawn(async move {
            let result = async {
                let mut pos = shared.state.lock().buf.len() as u64;
                while pos < len {
                    if shared.cancelled() || shared.complete() {
                        return Ok(());
                    }
                    let mut resp = range_request(&client, &url, None, pos, len, HTTP_CHUNK)
                        .await
                        .map_err(|e| e.to_string())?;
                    while let Some(bytes) = resp.chunk().await.map_err(|e| e.to_string())? {
                        if shared.cancelled() || shared.complete() {
                            return Ok(());
                        }
                        shared.push_at(pos, &bytes);
                        pos += bytes.len() as u64;
                    }
                    // Repartir de la position la plus avancée (l'autre source a pu nous dépasser).
                    pos = pos.max(shared.state.lock().buf.len() as u64);
                }
                Ok(())
            }
            .await;
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
        st.done && st.error.is_some() && st.buf.is_empty()
    }

    /// Attend d'avoir assez de données pour démarrer, puis renvoie un lecteur.
    pub async fn reader(&self) -> Result<StreamReader> {
        let waiter = self.shared.clone();
        tokio::task::spawn_blocking(move || {
            let mut st = waiter.state.lock();
            while st.buf.len() < PREBUFFER && !st.done {
                waiter.cond.wait(&mut st);
            }
        })
        .await
        .ok();
        let st = self.shared.state.lock();
        if st.buf.is_empty() {
            return Err(Error::Other(st.error.clone().unwrap_or_else(|| "empty stream".into())));
        }
        Ok(StreamReader { shared: self.shared.clone(), _guard: self.guard.clone(), pos: 0 })
    }
}

fn with_ua(req: reqwest::RequestBuilder, ua: Option<&str>) -> reqwest::RequestBuilder {
    match ua {
        Some(ua) => req.header(reqwest::header::USER_AGENT, ua),
        None => req,
    }
}

async fn range_request(
    client: &reqwest::Client,
    url: &str,
    ua: Option<&str>,
    start: u64,
    len: u64,
    chunk: u64,
) -> Result<reqwest::Response> {
    let end = (start + chunk).min(len) - 1;
    Ok(with_ua(client.get(url), ua)
        .header(reqwest::header::RANGE, format!("bytes={start}-{end}"))
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
}

impl StreamReader {
    pub fn total_len(&self) -> Option<u64> {
        self.shared.state.lock().total
    }
}

impl Read for StreamReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let mut st = self.shared.state.lock();
        // Attendre que les données demandées soient arrivées.
        while (st.buf.len() as u64) <= self.pos && !st.done {
            self.shared.cond.wait(&mut st);
        }
        let start = self.pos as usize;
        if start >= st.buf.len() {
            return match &st.error {
                Some(e) => Err(io::Error::other(e.clone())),
                None => Ok(0),
            };
        }
        let n = out.len().min(st.buf.len() - start);
        out[..n].copy_from_slice(&st.buf[start..start + n]);
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
                    st.buf.len() as u64
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
        Ok(self.pos)
    }
}
