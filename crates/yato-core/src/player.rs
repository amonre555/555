//! Moteur de lecture et file d'attente.
//!
//! La sortie audio vit dans son propre thread (certaines API audio du système ne peuvent pas
//! changer de thread). Le décodage est fait par symphonia, en Rust pur.
//!
//! Deux lecteurs sont branchés sur la même sortie : pendant un fondu enchaîné, le morceau qui
//! se termine baisse sur l'un pendant que le suivant monte sur l'autre. Le reste du temps, un
//! seul joue (le lecteur « actif »).
//!
//! L'ordre de lecture est une liste d'indices de la file : dans l'ordre, ou mélangée en mode
//! aléatoire (le morceau en cours d'abord). La file elle-même ne bouge pas.

use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use parking_lot::Mutex;
use rodio::{Decoder, Source};
use serde::{Deserialize, Serialize};

use crate::{Error, Result, SourceRegistry, StreamData, Track, stream::Download};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Idle,
    Loading,
    Playing,
    Paused,
    Error,
}

/// Répétition : rien, toute la file, ou le morceau en cours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

/// État du lecteur envoyé à l'interface.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub track: Option<Track>,
    pub status: Status,
    pub position: f64,
    pub duration: Option<f64>,
    pub volume: f32,
    pub index: Option<usize>,
    pub queue_len: usize,
    pub error: Option<String>,
    pub shuffle: bool,
    pub repeat: Repeat,
}

/// Ce qu'il faut pour reprendre l'écoute au prochain lancement.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Session {
    pub queue: Vec<Track>,
    pub index: Option<usize>,
    pub position: f64,
    pub volume: Option<f32>,
    pub shuffle: bool,
    pub repeat: Repeat,
}

struct State {
    queue: Vec<Track>,
    index: Option<usize>,
    status: Status,
    volume: f32,
    duration: Option<f64>,
    error: Option<String>,
    shuffle: bool,
    repeat: Repeat,
    /// Ordre de lecture : indices de `queue`, mélangés en mode aléatoire.
    order: Vec<usize>,
    /// Session reprise : position où repartir au premier « lecture ». Pendant un chargement,
    /// position où le morceau démarrera.
    resume_at: Option<f64>,
    /// Position où le morceau en cours a démarré, en secondes : un morceau repris est ouvert
    /// déjà avancé, ce que `get_pos` de rodio ignore (il compte depuis l'ouverture).
    offset: f64,
    /// Durée du fondu enchaîné, en secondes (0 : pas de fondu).
    crossfade: f64,
    /// Un fondu enchaîné est en cours (ou en préparation).
    fading: bool,
    /// Morceau dont le fondu a échoué (impossible à charger à l'avance) : il sera chargé
    /// normalement à la fin du morceau en cours, sans retenter le fondu à chaque tick.
    no_fade_for: Option<usize>,
}

impl State {
    /// Refait l'ordre de lecture : dans l'ordre, ou mélangé avec le morceau en cours d'abord.
    fn rebuild_order(&mut self) {
        let mut order: Vec<usize> = (0..self.queue.len()).collect();
        if self.shuffle {
            shuffle(&mut order);
            if let Some(i) = self.index
                && let Some(p) = order.iter().position(|&x| x == i)
            {
                order.swap(0, p);
            }
        }
        self.order = order;
    }

    fn order_pos(&self) -> Option<usize> {
        self.index.and_then(|i| self.order.iter().position(|&x| x == i))
    }

    /// Morceau suivant dans l'ordre de lecture. `wrap` : repartir du début à la fin de la file.
    fn next_index(&self, wrap: bool) -> Option<usize> {
        match self.order_pos() {
            Some(p) if p + 1 < self.order.len() => Some(self.order[p + 1]),
            Some(_) if wrap => self.order.first().copied(),
            Some(_) => None,
            None => self.order.first().copied(),
        }
    }

    fn previous_index(&self, wrap: bool) -> Option<usize> {
        match self.order_pos()? {
            0 if wrap => self.order.last().copied(),
            0 => None,
            p => Some(self.order[p - 1]),
        }
    }
}

/// Nombre de morceaux préparés par [`Controller::warm`].
pub const WARM_MAX: usize = 8;

pub struct Controller {
    players: [rodio::Player; 2],
    /// Lecteur qui joue le morceau en cours (0 ou 1).
    active: AtomicUsize,
    sources: Arc<SourceRegistry>,
    client: reqwest::Client,
    state: Mutex<State>,
    /// Incrémenté à chaque chargement : un chargement lent ne peut pas écraser un plus récent.
    generation: AtomicU64,
    /// Incrémenté pour arrêter le fondu enchaîné en cours.
    fade_generation: AtomicU64,
}

impl Controller {
    pub fn new(sources: Arc<SourceRegistry>, client: reqwest::Client) -> Result<Arc<Self>> {
        let players = open_output()?;
        let volume = 0.8;
        players[0].set_volume(perceptual(volume));
        Ok(Arc::new(Self {
            players,
            active: AtomicUsize::new(0),
            sources,
            client,
            state: Mutex::new(State {
                queue: Vec::new(),
                index: None,
                status: Status::Idle,
                volume,
                duration: None,
                error: None,
                shuffle: false,
                repeat: Repeat::Off,
                order: Vec::new(),
                resume_at: None,
                offset: 0.0,
                crossfade: 0.0,
                fading: false,
                no_fade_for: None,
            }),
            generation: AtomicU64::new(0),
            fade_generation: AtomicU64::new(0),
        }))
    }

    pub fn sources(&self) -> &Arc<SourceRegistry> {
        &self.sources
    }

    fn active(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }

    /// Lecteur du morceau en cours.
    fn player(&self) -> &rodio::Player {
        &self.players[self.active()]
    }

    /// Position dans le morceau en cours, en secondes.
    fn position(&self, st: &State) -> f64 {
        st.offset + self.player().get_pos().as_secs_f64()
    }

    pub fn snapshot(&self) -> Snapshot {
        let st = self.state.lock();
        let track = st.index.and_then(|i| st.queue.get(i)).cloned();
        let position = match st.status {
            Status::Playing | Status::Paused => self.position(&st),
            _ => st.resume_at.unwrap_or(0.0),
        };
        Snapshot {
            track,
            status: st.status,
            position,
            duration: st.duration,
            volume: st.volume,
            index: st.index,
            queue_len: st.queue.len(),
            error: st.error.clone(),
            shuffle: st.shuffle,
            repeat: st.repeat,
        }
    }

    pub fn queue(&self) -> Vec<Track> {
        self.state.lock().queue.clone()
    }

    /// Remplace la file d'attente et joue le morceau `index`.
    pub async fn play_queue(&self, tracks: Vec<Track>, index: usize) -> Result<()> {
        {
            let mut st = self.state.lock();
            st.queue = tracks;
            st.index = Some(index);
            st.rebuild_order();
        }
        self.load(index).await
    }

    /// Joue un morceau de la file. En mode aléatoire, la suite est remélangée à partir de lui.
    pub async fn play_index(&self, index: usize) -> Result<()> {
        {
            let mut st = self.state.lock();
            if st.shuffle {
                st.index = Some(index);
                st.rebuild_order();
            }
        }
        self.load(index).await
    }

    pub fn set_shuffle(&self, on: bool) {
        let mut st = self.state.lock();
        if st.shuffle != on {
            st.shuffle = on;
            st.rebuild_order();
        }
        self.prefetch_next(&st);
    }

    pub fn set_repeat(&self, repeat: Repeat) {
        let mut st = self.state.lock();
        st.repeat = repeat;
        self.prefetch_next(&st);
    }

    /// Durée du fondu enchaîné entre deux morceaux (0 : aucun, jusqu'à 12 s).
    pub fn set_crossfade(&self, seconds: f64) {
        self.state.lock().crossfade = seconds.clamp(0.0, 12.0);
    }

    /// Prépare un morceau en arrière-plan (sans bloquer). Sans runtime tokio, ne fait rien.
    pub fn prefetch(&self, track: Track) {
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        if let Ok(source) = self.sources.get(&track.source) {
            rt.spawn(async move { source.prefetch(&track).await });
        }
    }

    /// Préparation légère des morceaux affichés (les premiers [`WARM_MAX`]), un par un, en
    /// arrière-plan. Sans runtime tokio, ne fait rien.
    pub fn warm(&self, tracks: Vec<Track>) {
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        let sources = self.sources.clone();
        rt.spawn(async move {
            for track in tracks.into_iter().take(WARM_MAX) {
                if let Ok(source) = sources.get(&track.source) {
                    source.warm(&track).await;
                }
            }
        });
    }

    /// Prépare le morceau qui suivra : enchaînement (ou fondu) instantané.
    fn prefetch_next(&self, st: &State) {
        let next = if st.repeat == Repeat::One { st.index } else { st.next_index(st.repeat == Repeat::All) };
        if let Some(t) = next.and_then(|i| st.queue.get(i)).cloned() {
            self.prefetch(t);
        }
    }

    /// Arrête le fondu en cours : le morceau qui s'éteignait s'arrête, le nouveau passe au
    /// volume normal.
    fn cancel_fade(&self) {
        self.fade_generation.fetch_add(1, Ordering::SeqCst);
        let mut st = self.state.lock();
        if st.fading {
            st.fading = false;
            self.players[1 - self.active()].stop();
        }
        self.player().set_volume(perceptual(st.volume));
    }

    async fn load(&self, index: usize) -> Result<()> {
        self.load_from(index, None).await
    }

    /// Charge le morceau `index` et le lance à `start` secondes (au début si `None`).
    async fn load_from(&self, index: usize, start: Option<f64>) -> Result<()> {
        self.cancel_fade();
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let track = {
            let mut st = self.state.lock();
            let Some(track) = st.queue.get(index).cloned() else {
                return Err(Error::Other("track not found in the queue".into()));
            };
            st.index = Some(index);
            st.status = Status::Loading;
            st.duration = track.duration;
            st.error = None;
            st.resume_at = start;
            st.offset = 0.0;
            st.no_fade_for = None;
            track
        };
        self.player().stop();

        let slot = self.active();
        match self.open(&track, generation, slot, true).await {
            Ok(opened) => {
                if self.generation.load(Ordering::SeqCst) == generation {
                    let mut st = self.state.lock();
                    st.status = Status::Playing;
                    st.resume_at = None;
                    st.offset = opened.start;
                    if let Some(d) = opened.duration {
                        st.duration = Some(d.as_secs_f64());
                    }
                    self.prefetch_next(&st);
                }
                Ok(())
            }
            Err(e) => {
                if self.generation.load(Ordering::SeqCst) == generation {
                    let mut st = self.state.lock();
                    st.status = Status::Error;
                    st.error = Some(e.to_string());
                }
                Err(e)
            }
        }
    }

    /// Charge `track` dans le lecteur `slot` et le lance. `resume` : démarrer à la position
    /// `resume_at` de l'état (relue au dernier moment : l'utilisateur a pu la changer).
    /// Ne touche pas à l'état : c'est à l'appelant de dire ce qui joue.
    ///
    /// L'avance se fait sur le décodeur, avant de le confier au lecteur : un saut demandé au
    /// lecteur s'exécute dans le fil audio, qui attendrait (tout le son figé, les deux lecteurs
    /// compris) que le téléchargement atteigne ce point.
    async fn open(&self, track: &Track, generation: u64, slot: usize, resume: bool) -> Result<Opened> {
        let t0 = std::time::Instant::now();
        let data = self.sources.get(&track.source)?.resolve(track).await?;
        crate::timing::step("lecteur : source résolue", t0);
        let is_current = || self.generation.load(Ordering::SeqCst) == generation;
        let player = &self.players[slot];
        let stale = Opened { duration: None, start: 0.0 };
        let start = if resume { self.state.lock().resume_at.filter(|&p| p > 1.0) } else { None };

        // Construire le décodeur peut lire le début du fichier : on le fait hors du thread async.
        let (duration, start) = match data {
            StreamData::File(path) => {
                let (decoder, started) = tokio::task::spawn_blocking(move || decode_file(&path).map(|d| seek_to(d, start)))
                    .await
                    .map_err(|e| Error::Other(e.to_string()))??;
                if !is_current() {
                    return Ok(stale);
                }
                let d = decoder.total_duration();
                player.append(decoder);
                (d, started)
            }
            StreamData::Http { .. } | StreamData::Buffered { .. } => {
                let (download, hint) = match data {
                    StreamData::Http { url, ranged_len, hint, user_agent } => {
                        (Download::http(&self.client, &url, ranged_len, user_agent).await?, hint)
                    }
                    StreamData::Buffered { download, hint } => (download, hint),
                    StreamData::File(_) => unreachable!(),
                };
                let reader = download.reader().await?;
                if !is_current() {
                    return Ok(stale);
                }
                let (decoder, started) = tokio::task::spawn_blocking(move || {
                    let mut reader = reader;
                    let len = reader.total_len();
                    let decoder = {
                        // m4a de YouTube : ouvrir sans attendre la fin du téléchargement.
                        let _quick = reader.quick_open_mp4();
                        let mut b = Decoder::builder().with_data(reader).with_seekable(true);
                        if let Some(len) = len {
                            b = b.with_byte_len(len);
                        }
                        if let Some(hint) = hint {
                            b = b.with_hint(hint);
                        }
                        b.build().map_err(|e| Error::Decode(e.to_string()))?
                    };
                    // Ouverture rapide terminée : l'avance lit le vrai fichier (et attend, ici,
                    // que le téléchargement y arrive).
                    Ok::<_, Error>(seek_to(decoder, start))
                })
                .await
                .map_err(|e| Error::Other(e.to_string()))??;
                crate::timing::step("lecteur : décodeur prêt (depuis le clic)", t0);
                if !is_current() {
                    return Ok(stale);
                }
                let d = decoder.total_duration();
                player.append(decoder);
                (d, started)
            }
        };
        if !is_current() {
            player.stop();
            return Ok(stale);
        }
        player.play();
        crate::timing::step(&format!("lecteur : « {} » lancé (total depuis le clic)", track.title), t0);
        Ok(Opened { duration, start })
    }

    pub async fn toggle(&self) -> Result<()> {
        let (status, index) = {
            let st = self.state.lock();
            (st.status, st.index)
        };
        match status {
            Status::Playing => {
                self.cancel_fade();
                self.player().pause();
                self.state.lock().status = Status::Paused;
            }
            Status::Paused => {
                self.player().play();
                self.state.lock().status = Status::Playing;
            }
            Status::Idle | Status::Error => {
                // Session reprise : on repart où l'on s'était arrêté.
                let resume = self.state.lock().resume_at.take();
                let start = index.or_else(|| self.state.lock().order.first().copied());
                if let Some(i) = start {
                    self.load_from(i, resume).await?;
                }
            }
            Status::Loading => {}
        }
        Ok(())
    }

    /// « Suivant » demandé par l'utilisateur : en répétition, la file reboucle.
    pub async fn next(&self) -> Result<()> {
        let next = {
            let st = self.state.lock();
            st.next_index(st.repeat != Repeat::Off)
        };
        match next {
            Some(i) => self.load(i).await,
            None => {
                self.stop();
                Ok(())
            }
        }
    }

    /// Fin naturelle du morceau : répétition du morceau, suivant, ou fin de la file.
    async fn advance(&self) -> Result<()> {
        let next = {
            let st = self.state.lock();
            match st.repeat {
                Repeat::One => st.index,
                r => st.next_index(r == Repeat::All),
            }
        };
        match next {
            Some(i) => self.load(i).await,
            None => {
                self.stop();
                Ok(())
            }
        }
    }

    pub async fn previous(&self) -> Result<()> {
        // Comme sur tous les lecteurs : au-delà de 3 s, « précédent » revient au début du morceau.
        if self.position(&self.state.lock()) > 3.0 {
            return self.seek(0.0);
        }
        let prev = {
            let st = self.state.lock();
            st.previous_index(st.repeat != Repeat::Off)
        };
        match prev {
            Some(i) => self.load(i).await,
            None => self.seek(0.0),
        }
    }

    pub fn stop(&self) {
        self.cancel_fade();
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.player().stop();
        self.state.lock().status = Status::Idle;
    }

    pub fn seek(&self, seconds: f64) -> Result<()> {
        {
            let mut st = self.state.lock();
            // Rien ne joue encore : le morceau démarrera à cette position.
            if st.status == Status::Loading
                || (matches!(st.status, Status::Idle | Status::Error) && st.resume_at.is_some())
            {
                st.resume_at = Some(seconds.max(0.0));
                return Ok(());
            }
        }
        self.cancel_fade();
        self.player()
            .try_seek(Duration::from_secs_f64(seconds.max(0.0)))
            .map_err(|e| Error::Decode(e.to_string()))?;
        // Après un saut, rodio compte depuis le début du morceau.
        self.state.lock().offset = 0.0;
        Ok(())
    }

    pub fn set_volume(&self, volume: f32) {
        let v = volume.clamp(0.0, 1.0);
        let mut st = self.state.lock();
        st.volume = v;
        // Pendant un fondu, c'est lui qui règle les deux lecteurs (il relit ce volume).
        if !st.fading {
            self.player().set_volume(perceptual(v));
        }
    }

    /// À appeler régulièrement (4 fois par seconde) : enchaîne les morceaux, et lance le fondu
    /// enchaîné quelques secondes avant la fin.
    pub async fn tick(self: &Arc<Self>) {
        enum Action {
            Ended,
            Fade(usize),
        }
        let action = {
            let mut st = self.state.lock();
            if st.status != Status::Playing || st.fading {
                None
            } else if self.player().empty() {
                st.status = Status::Idle;
                Some(Action::Ended)
            } else if st.crossfade > 0.0 && st.repeat != Repeat::One {
                let remaining = st.duration.map(|d| d - self.position(&st));
                let long_enough = st.duration.is_some_and(|d| d > st.crossfade * 3.0);
                match (remaining, st.next_index(st.repeat == Repeat::All)) {
                    (Some(r), Some(n)) if long_enough && r <= st.crossfade && st.no_fade_for != Some(n) => {
                        st.fading = true;
                        Some(Action::Fade(n))
                    }
                    _ => None,
                }
            } else {
                None
            }
        };
        match action {
            Some(Action::Ended) => {
                let _ = self.advance().await;
            }
            Some(Action::Fade(next)) => self.crossfade_to(next).await,
            None => {}
        }
    }

    /// Fondu enchaîné : le morceau `next` démarre sur l'autre lecteur, monte pendant que
    /// l'actuel baisse (courbes à puissance constante : le volume perçu ne creuse pas), puis
    /// l'ancien s'arrête. Il devient aussitôt « le morceau en cours ».
    async fn crossfade_to(self: &Arc<Self>, next: usize) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (from, to) = (self.active(), 1 - self.active());
        let Some(track) = self.state.lock().queue.get(next).cloned() else { return };
        self.players[to].stop();
        self.players[to].set_volume(0.0);

        let opened = self.open(&track, generation, to, false).await;
        if self.generation.load(Ordering::SeqCst) != generation {
            return; // l'utilisateur a changé de morceau entre-temps
        }
        let duration = match opened {
            Ok(o) => o.duration,
            Err(_) => {
                let mut st = self.state.lock();
                st.fading = false;
                st.no_fade_for = Some(next);
                return;
            }
        };
        let seconds = {
            let mut st = self.state.lock();
            self.active.store(to, Ordering::SeqCst);
            st.index = Some(next);
            st.offset = 0.0;
            st.duration = duration.map(|d| d.as_secs_f64()).or(track.duration);
            st.status = Status::Playing;
            st.error = None;
            st.no_fade_for = None;
            self.prefetch_next(&st);
            st.crossfade.max(0.5)
        };

        let fade = self.fade_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let this = self.clone();
        tokio::spawn(async move {
            const STEP: Duration = Duration::from_millis(30);
            let steps = ((seconds / STEP.as_secs_f64()) as u32).max(1);
            for k in 1..=steps {
                tokio::time::sleep(STEP).await;
                if this.fade_generation.load(Ordering::SeqCst) != fade {
                    return; // annulé : `cancel_fade` a déjà remis les lecteurs en ordre
                }
                let t = f64::from(k) / f64::from(steps);
                let v = perceptual(this.state.lock().volume);
                let angle = t * std::f64::consts::FRAC_PI_2;
                this.players[from].set_volume(v * angle.cos() as f32);
                this.players[to].set_volume(v * angle.sin() as f32);
            }
            let mut st = this.state.lock();
            if this.fade_generation.load(Ordering::SeqCst) == fade {
                st.fading = false;
                this.players[from].stop();
                this.players[to].set_volume(perceptual(st.volume));
            }
        });
    }

    /// L'écoute en cours, à garder pour le prochain lancement.
    pub fn session(&self) -> Session {
        let snap = self.snapshot();
        let st = self.state.lock();
        Session {
            queue: st.queue.clone(),
            index: st.index,
            position: snap.position,
            volume: Some(st.volume),
            shuffle: st.shuffle,
            repeat: st.repeat,
        }
    }

    /// Reprend une écoute enregistrée, sans lancer le son : le morceau s'affiche, et
    /// « lecture » repart à la position enregistrée.
    pub fn restore(&self, s: Session) {
        if let Some(v) = s.volume {
            self.set_volume(v);
        }
        let mut st = self.state.lock();
        st.index = s.index.filter(|&i| i < s.queue.len());
        st.duration = st.index.and_then(|i| s.queue.get(i)).and_then(|t| t.duration);
        st.queue = s.queue;
        st.shuffle = s.shuffle;
        st.repeat = s.repeat;
        st.resume_at = st.index.map(|_| s.position.max(0.0));
        st.status = Status::Idle;
        st.rebuild_order();
    }
}

/// Mélange de Fisher-Yates, avec un petit générateur (xorshift) amorcé par l'horloge : pas
/// besoin d'un hasard cryptographique pour mélanger une playlist.
fn shuffle(v: &mut [usize]) {
    let mut x = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        | 1;
    for i in (1..v.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.swap(i, (x % (i as u64 + 1)) as usize);
    }
}

/// L'oreille perçoit le volume de façon logarithmique : une courbe au carré rend le curseur naturel.
fn perceptual(v: f32) -> f32 {
    v * v
}

/// Ce que [`Controller::open`] a lancé.
struct Opened {
    /// Durée lue dans le fichier.
    duration: Option<Duration>,
    /// Position de départ, en secondes.
    start: f64,
}

/// Avance le décodeur à `start` secondes. Renvoie la position de départ (0 si l'avance échoue :
/// le morceau part du début plutôt que pas du tout).
fn seek_to<S: Source>(mut decoder: S, start: Option<f64>) -> (S, f64) {
    let started = match start {
        Some(p) if decoder.try_seek(Duration::from_secs_f64(p)).is_ok() => p,
        Some(_) => {
            let _ = decoder.try_seek(Duration::ZERO);
            0.0
        }
        None => 0.0,
    };
    (decoder, started)
}

fn decode_file(path: &Path) -> Result<Decoder<BufReader<File>>> {
    let file = File::open(path)?;
    let len = file.metadata()?.len();
    let mut b = Decoder::builder()
        .with_data(BufReader::with_capacity(256 * 1024, file))
        .with_byte_len(len)
        .with_seekable(true);
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        b = b.with_hint(ext);
    }
    b.build().map_err(|e| Error::Decode(e.to_string()))
}

/// Ouvre la sortie audio par défaut dans un thread dédié qui la garde en vie, avec deux
/// lecteurs branchés dessus (pour le fondu enchaîné).
fn open_output() -> Result<[rodio::Player; 2]> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("yato-audio".into())
        .spawn(move || match open_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                let players = [rodio::Player::connect_new(sink.mixer()), rodio::Player::connect_new(sink.mixer())];
                let _ = tx.send(Ok(players));
                loop {
                    std::thread::park();
                }
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        })?;
    rx.recv()
        .map_err(|e| Error::Audio(e.to_string()))?
        .map_err(Error::Audio)
}

/// Sortie par défaut, sinon la première autre sortie qui accepte de s'ouvrir.
///
/// On n'utilise pas `open_default_sink` de rodio : il impose un tampon d'environ 50 ms
/// (2048 trames à 48 kHz), que certains périphériques refusent (AirPods : 960 au plus),
/// et sous macOS son repli écarte toutes les sorties car elles n'annoncent pas de pilote.
fn open_sink() -> std::result::Result<rodio::MixerDeviceSink, rodio::DeviceSinkError> {
    use rodio::cpal::{BufferSize, SupportedBufferSize, traits::{DeviceTrait, HostTrait}};

    let host = rodio::cpal::default_host();
    let default = host.default_output_device();
    let default_id = default.as_ref().and_then(|d| d.id().ok());
    let others = host
        .output_devices()
        .into_iter()
        .flatten()
        .filter(|d| default_id.is_none() || d.id().ok() != default_id);

    let mut first_err = None;
    for device in default.into_iter().chain(others) {
        let attempt = device
            .default_output_config()
            .map_err(rodio::DeviceSinkError::DefaultSinkConfigError)
            .and_then(|config| {
                // Environ 50 ms, ramenés dans ce que le périphérique accepte.
                let mut frames = config.sample_rate() / 20;
                if let SupportedBufferSize::Range { min, max } = *config.buffer_size() {
                    frames = frames.clamp(min, max);
                }
                rodio::DeviceSinkBuilder::from_device(device)
                    .map(|b| b.with_buffer_size(BufferSize::Fixed(frames)))
            })
            .and_then(|b| b.open_sink_or_fallback());
        match attempt {
            Ok(sink) => return Ok(sink),
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    Err(first_err.unwrap_or(rodio::DeviceSinkError::NoDevice))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(len: usize, index: usize, shuffle: bool) -> State {
        let track = |i: usize| Track {
            id: format!("t{i}"), source: "local".into(), title: String::new(), artist: String::new(), album: None, album_id: None,
            duration: None, track_number: None, disc_number: None, artwork: None, has_cover: false,
            location: crate::Location::Remote { remote_id: String::new() }, artists: Vec::new(),
        };
        let mut st = State {
            queue: (0..len).map(track).collect(), index: Some(index), status: Status::Idle, volume: 1.0, duration: None,
            error: None, shuffle, repeat: Repeat::Off, order: Vec::new(), resume_at: None, offset: 0.0, crossfade: 0.0, fading: false,
            no_fade_for: None,
        };
        st.rebuild_order();
        st
    }

    #[test]
    fn plays_in_order_and_wraps_only_when_asked() {
        let st = state(3, 1, false);
        assert_eq!(st.next_index(false), Some(2));
        assert_eq!(st.previous_index(false), Some(0));
        let st = state(3, 2, false);
        assert_eq!(st.next_index(false), None);
        assert_eq!(st.next_index(true), Some(0));
        let st = state(3, 0, false);
        assert_eq!(st.previous_index(true), Some(2));
    }

    #[test]
    fn shuffle_starts_from_the_current_track_and_visits_everything() {
        let mut st = state(50, 17, true);
        assert_eq!(st.order[0], 17);
        let mut seen = vec![17];
        while let Some(n) = st.next_index(false) {
            seen.push(n);
            st.index = Some(n);
        }
        seen.sort();
        assert_eq!(seen, (0..50).collect::<Vec<_>>());
    }
}
