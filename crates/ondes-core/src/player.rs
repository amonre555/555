//! Moteur de lecture et file d'attente.
//!
//! La sortie audio vit dans son propre thread (certaines API audio du système ne peuvent pas
//! changer de thread). Le décodage est fait par symphonia, en Rust pur.

use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use parking_lot::Mutex;
use rodio::{Decoder, Source};
use serde::Serialize;

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
}

struct State {
    queue: Vec<Track>,
    index: Option<usize>,
    status: Status,
    volume: f32,
    duration: Option<f64>,
    error: Option<String>,
}

pub struct Controller {
    player: rodio::Player,
    sources: Arc<SourceRegistry>,
    client: reqwest::Client,
    state: Mutex<State>,
    /// Incrémenté à chaque chargement : un chargement lent ne peut pas écraser un plus récent.
    generation: AtomicU64,
}

impl Controller {
    pub fn new(sources: Arc<SourceRegistry>, client: reqwest::Client) -> Result<Arc<Self>> {
        let player = open_output()?;
        let volume = 0.8;
        player.set_volume(perceptual(volume));
        Ok(Arc::new(Self {
            player,
            sources,
            client,
            state: Mutex::new(State {
                queue: Vec::new(),
                index: None,
                status: Status::Idle,
                volume,
                duration: None,
                error: None,
            }),
            generation: AtomicU64::new(0),
        }))
    }

    pub fn sources(&self) -> &Arc<SourceRegistry> {
        &self.sources
    }

    pub fn snapshot(&self) -> Snapshot {
        let st = self.state.lock();
        let track = st.index.and_then(|i| st.queue.get(i)).cloned();
        let position = match st.status {
            Status::Playing | Status::Paused => self.player.get_pos().as_secs_f64(),
            _ => 0.0,
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
        }
        self.load(index).await
    }

    pub async fn play_index(&self, index: usize) -> Result<()> {
        self.load(index).await
    }

    /// Prépare un morceau en arrière-plan (sans bloquer). Sans runtime tokio, ne fait rien.
    pub fn prefetch(&self, track: Track) {
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        if let Ok(source) = self.sources.get(&track.source) {
            rt.spawn(async move { source.prefetch(&track).await });
        }
    }

    async fn load(&self, index: usize) -> Result<()> {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let track = {
            let mut st = self.state.lock();
            let Some(track) = st.queue.get(index).cloned() else {
                return Err(Error::Other("morceau introuvable dans la file".into()));
            };
            st.index = Some(index);
            st.status = Status::Loading;
            st.duration = track.duration;
            st.error = None;
            track
        };
        self.player.stop();

        match self.open(&track, generation).await {
            Ok(()) => {
                // Préparer le morceau suivant pendant que celui-ci joue : enchaînement instantané.
                let next = self.state.lock().queue.get(index + 1).cloned();
                if let Some(next) = next {
                    self.prefetch(next);
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

    async fn open(&self, track: &Track, generation: u64) -> Result<()> {
        let data = self.sources.get(&track.source)?.resolve(track).await?;
        let is_current = || self.generation.load(Ordering::SeqCst) == generation;

        // Construire le décodeur peut lire le début du fichier : on le fait hors du thread async.
        let duration = match data {
            StreamData::File(path) => {
                let decoder = tokio::task::spawn_blocking(move || decode_file(&path))
                    .await
                    .map_err(|e| Error::Other(e.to_string()))??;
                if !is_current() {
                    return Ok(());
                }
                let d = decoder.total_duration();
                self.player.append(decoder);
                d
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
                    return Ok(());
                }
                let decoder = tokio::task::spawn_blocking(move || {
                    let len = reader.total_len();
                    let mut b = Decoder::builder().with_data(reader).with_seekable(true);
                    if let Some(len) = len {
                        b = b.with_byte_len(len);
                    }
                    if let Some(hint) = hint {
                        b = b.with_hint(hint);
                    }
                    b.build().map_err(|e| Error::Decode(e.to_string()))
                })
                .await
                .map_err(|e| Error::Other(e.to_string()))??;
                if !is_current() {
                    return Ok(());
                }
                let d = decoder.total_duration();
                self.player.append(decoder);
                d
            }
        };

        self.player.play();
        let mut st = self.state.lock();
        st.status = Status::Playing;
        if let Some(d) = duration {
            st.duration = Some(d.as_secs_f64());
        }
        Ok(())
    }

    pub async fn toggle(&self) -> Result<()> {
        let (status, index) = {
            let st = self.state.lock();
            (st.status, st.index)
        };
        match status {
            Status::Playing => {
                self.player.pause();
                self.state.lock().status = Status::Paused;
            }
            Status::Paused => {
                self.player.play();
                self.state.lock().status = Status::Playing;
            }
            Status::Idle | Status::Error => {
                if let Some(i) = index {
                    self.load(i).await?;
                } else if !self.state.lock().queue.is_empty() {
                    self.load(0).await?;
                }
            }
            Status::Loading => {}
        }
        Ok(())
    }

    pub async fn next(&self) -> Result<()> {
        let (index, len) = {
            let st = self.state.lock();
            (st.index, st.queue.len())
        };
        match index {
            Some(i) if i + 1 < len => self.load(i + 1).await,
            _ => {
                self.stop();
                Ok(())
            }
        }
    }

    pub async fn previous(&self) -> Result<()> {
        // Comme sur tous les lecteurs : au-delà de 3 s, « précédent » revient au début du morceau.
        if self.player.get_pos() > Duration::from_secs(3) {
            return self.seek(0.0);
        }
        let index = self.state.lock().index;
        match index {
            Some(i) if i > 0 => self.load(i - 1).await,
            _ => self.seek(0.0),
        }
    }

    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.player.stop();
        self.state.lock().status = Status::Idle;
    }

    pub fn seek(&self, seconds: f64) -> Result<()> {
        self.player
            .try_seek(Duration::from_secs_f64(seconds.max(0.0)))
            .map_err(|e| Error::Decode(e.to_string()))
    }

    pub fn set_volume(&self, volume: f32) {
        let v = volume.clamp(0.0, 1.0);
        self.player.set_volume(perceptual(v));
        self.state.lock().volume = v;
    }

    /// À appeler régulièrement. Renvoie `true` quand le morceau en cours vient de se terminer.
    pub fn tick(&self) -> bool {
        let mut st = self.state.lock();
        if st.status == Status::Playing && self.player.empty() {
            st.status = Status::Idle;
            return true;
        }
        false
    }
}

/// L'oreille perçoit le volume de façon logarithmique : une courbe au carré rend le curseur naturel.
fn perceptual(v: f32) -> f32 {
    v * v
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

/// Ouvre la sortie audio par défaut dans un thread dédié qui la garde en vie.
fn open_output() -> Result<rodio::Player> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("ondes-audio".into())
        .spawn(move || match open_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                let player = rodio::Player::connect_new(sink.mixer());
                let _ = tx.send(Ok(player));
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
