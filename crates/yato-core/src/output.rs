//! Sortie audio : liste des appareils du système et bascule de l'un à l'autre.
//!
//! Les deux lecteurs (fondu enchaîné) sont branchés sur un mélangeur à nous, qui ne dépend
//! d'aucun appareil. Ce mélangeur est lu par l'appareil ouvert ; pour changer de sortie, on
//! ferme l'appareil et on en ouvre un autre qui lit le même mélangeur : la lecture continue,
//! au même endroit, sans toucher aux lecteurs.
//!
//! L'appareil vit dans un thread dédié (certaines API audio ne peuvent pas changer de thread).
//! Toutes les 2 s, ce thread vérifie que la sortie voulue existe toujours : si elle a disparu,
//! il repasse sur la sortie par défaut (et revient sur elle si elle réapparaît). Sans choix,
//! il suit la sortie par défaut du système.

use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

use parking_lot::Mutex;
use rodio::{
    Source,
    cpal::{
        BufferSize, SupportedBufferSize,
        traits::{DeviceTrait, HostTrait},
    },
    mixer::MixerSource,
};
use serde::Serialize;

use crate::{Error, Result};

/// Une sortie audio connue du système.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    /// Identifiant stable (à garder dans les réglages).
    pub id: String,
    pub name: String,
    /// `builtIn`, `bluetooth`, `usb`, `hdmi`, `network` (AirPlay…) ou `other`.
    pub kind: &'static str,
    /// Sortie par défaut du système.
    pub is_default: bool,
}

/// Sorties audio du système, la sortie par défaut d'abord.
pub fn output_devices() -> Vec<OutputDevice> {
    use rodio::cpal::InterfaceType as I;
    let host = rodio::cpal::default_host();
    let default_id = host.default_output_device().and_then(|d| d.id().ok()).map(|i| i.to_string());
    let mut list: Vec<OutputDevice> = host
        .output_devices()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            let id = d.id().ok()?.to_string();
            let desc = d.description().ok()?;
            let kind = match desc.interface_type() {
                I::BuiltIn => "builtIn",
                I::Bluetooth => "bluetooth",
                I::Usb => "usb",
                I::Hdmi | I::DisplayPort => "hdmi",
                I::Network => "network",
                _ => "other",
            };
            Some(OutputDevice { is_default: default_id.as_deref() == Some(id.as_str()), id, name: desc.name().to_owned(), kind })
        })
        .collect();
    list.sort_by_key(|d| !d.is_default);
    list
}

/// Android et iOS : une seule sortie, celle que le système choisit.
const PHONE: bool = cfg!(any(target_os = "android", target_os = "ios"));

enum Cmd {
    /// Sortie voulue (`None` : celle du système) ; répond une fois la sortie ouverte.
    Use(Option<String>, mpsc::Sender<std::result::Result<(), String>>),
}

/// Sortie audio ouverte, et de quoi en changer.
pub(crate) struct Output {
    tx: Mutex<mpsc::Sender<Cmd>>,
    /// Sortie qui joue vraiment (identifiant).
    active: Arc<Mutex<Option<String>>>,
}

impl Output {
    /// Ouvre la sortie par défaut et y branche deux lecteurs.
    pub(crate) fn open() -> Result<(Self, [rodio::Player; 2])> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let active = Arc::new(Mutex::new(None));
        let shown = active.clone();
        std::thread::Builder::new().name("yato-audio".into()).spawn(move || {
            let first = match open_sink(None) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };
            let cfg = first.0.config();
            let (mixer, source) = rodio::mixer::mixer(cfg.channel_count(), cfg.sample_rate());
            let source = Arc::new(Mutex::new(source));
            let players = [rodio::Player::connect_new(&mixer), rodio::Player::connect_new(&mixer)];
            let mut current = Some(attach(first, &source));
            *shown.lock() = current.as_ref().map(|c| c.1.clone());
            let _ = ready_tx.send(Ok(players));
            run(rx, current.take(), &source, &shown);
        })?;
        let players = ready_rx.recv().map_err(|e| Error::Audio(e.to_string()))?.map_err(Error::Audio)?;
        Ok((Self { tx: Mutex::new(tx), active }, players))
    }

    /// Passe sur la sortie `id` (`None` : celle du système). Bloque jusqu'à ce qu'elle soit
    /// ouverte : à appeler hors du runtime async.
    pub(crate) fn set(&self, id: Option<String>) -> Result<()> {
        let (reply, answer) = mpsc::channel();
        self.tx.lock().send(Cmd::Use(id, reply)).map_err(|e| Error::Audio(e.to_string()))?;
        answer.recv().map_err(|e| Error::Audio(e.to_string()))?.map_err(Error::Audio)
    }

    pub(crate) fn active(&self) -> Option<String> {
        self.active.lock().clone()
    }
}

type Sink = (rodio::MixerDeviceSink, String);

/// Branche le mélangeur des lecteurs sur un appareil ouvert.
fn attach(mut sink: Sink, source: &Arc<Mutex<MixerSource>>) -> Sink {
    sink.0.log_on_drop(false);
    let channels = source.lock().channels();
    let rate = source.lock().sample_rate();
    sink.0.mixer().add(Tap { source: source.clone(), buf: Vec::new(), pos: 0, channels, rate });
    sink
}

fn run(rx: mpsc::Receiver<Cmd>, mut current: Option<Sink>, source: &Arc<Mutex<MixerSource>>, shown: &Mutex<Option<String>>) {
    let mut wanted: Option<String> = None;
    // Sortie qu'on n'a pas réussi à ouvrir : on ne réessaie pas toutes les 2 s.
    let mut failed: Option<String> = None;
    loop {
        let reply = match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Cmd::Use(id, reply)) => {
                wanted = id;
                failed = None;
                Some(reply)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Téléphone : une seule sortie, gérée par le système.
                if PHONE {
                    continue;
                }
                None
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let target = target_id(wanted.as_deref());
        let open_now = current.as_ref().map(|c| &c.1);
        if target.is_some() && target.as_ref() == open_now {
            if let Some(r) = reply {
                let _ = r.send(Ok(()));
            }
            continue;
        }
        if reply.is_none() && (target.is_none() || target == failed) {
            continue;
        }
        // L'ancien appareil d'abord : deux appareils ne lisent jamais le mélangeur en même temps.
        current = None;
        let result = open_sink(target.as_deref()).map(|s| attach(s, source));
        let result = match result {
            Ok(s) => {
                if Some(&s.1) != target.as_ref() {
                    failed = target.clone();
                }
                Ok(s)
            }
            Err(e) => {
                failed = target.clone();
                Err(e.to_string())
            }
        };
        let answer = match result {
            Ok(s) => {
                current = Some(s);
                Ok(())
            }
            Err(e) => Err(e),
        };
        *shown.lock() = current.as_ref().map(|c| c.1.clone());
        if let Some(r) = reply {
            let _ = r.send(answer);
        }
    }
}

/// Sortie à utiliser : celle voulue si elle existe, sinon celle par défaut du système.
fn target_id(wanted: Option<&str>) -> Option<String> {
    let wanted = wanted.filter(|_| !PHONE);
    let host = rodio::cpal::default_host();
    if let Some(w) = wanted
        && host.output_devices().into_iter().flatten().any(|d| d.id().is_ok_and(|i| i.to_string() == w))
    {
        return Some(w.to_owned());
    }
    host.default_output_device().and_then(|d| d.id().ok()).map(|i| i.to_string())
}

/// Ouvre `wanted` si elle existe, sinon la sortie par défaut, sinon la première autre sortie
/// qui accepte de s'ouvrir.
///
/// On n'utilise pas `open_default_sink` de rodio : il impose un tampon d'environ 50 ms
/// (2048 trames à 48 kHz), que certains périphériques refusent (AirPods : 960 au plus),
/// et sous macOS son repli écarte toutes les sorties car elles n'annoncent pas de pilote.
fn open_sink(wanted: Option<&str>) -> std::result::Result<Sink, rodio::DeviceSinkError> {
    let wanted = wanted.filter(|_| !PHONE);
    let host = rodio::cpal::default_host();
    let id_of = |d: &rodio::cpal::Device| d.id().ok().map(|i| i.to_string());
    let all: Vec<rodio::cpal::Device> = host.output_devices().into_iter().flatten().collect();
    let mut ordered: Vec<rodio::cpal::Device> = Vec::new();
    if let Some(w) = wanted
        && let Some(d) = all.iter().find(|d| id_of(d).as_deref() == Some(w))
    {
        ordered.push(d.clone());
    }
    // La sortie par défaut telle que le système la donne, même si elle n'est pas dans la
    // liste : sur Android, c'est « laisser le système choisir » (id -1), absent de la liste.
    // Sans ça, on ouvrait le premier appareil listé : l'écouteur qui sert aux appels.
    ordered.extend(host.default_output_device());
    // Téléphone : pas de repli sur un appareil précis (écouteur, sortie des appels…), le
    // système route lui-même vers le haut-parleur, le casque ou le Bluetooth.
    if !PHONE {
        ordered.extend(all.iter().cloned());
    }

    let mut first_err = None;
    let mut tried: Vec<Option<String>> = Vec::new();
    for device in ordered {
        let id = id_of(&device);
        if tried.contains(&id) {
            continue;
        }
        tried.push(id.clone());
        let attempt = device
            .default_output_config()
            .map_err(rodio::DeviceSinkError::DefaultSinkConfigError)
            .and_then(|config| {
                // Environ 50 ms, ramenés dans ce que le périphérique accepte.
                let mut frames = config.sample_rate() / 20;
                if let SupportedBufferSize::Range { min, max } = *config.buffer_size() {
                    frames = frames.clamp(min, max);
                }
                rodio::DeviceSinkBuilder::from_device(device).map(|b| b.with_buffer_size(BufferSize::Fixed(frames)))
            })
            .and_then(|b| b.open_sink_or_fallback());
        match attempt {
            Ok(sink) => return Ok((sink, id.unwrap_or_default())),
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    Err(first_err.unwrap_or(rodio::DeviceSinkError::NoDevice))
}

/// Lit le mélangeur des lecteurs pour un appareil, par paquets de trames entières (un verrou
/// par paquet, pas par échantillon). Ne s'arrête jamais : sans son, il rend du silence.
struct Tap {
    source: Arc<Mutex<MixerSource>>,
    buf: Vec<rodio::Sample>,
    pos: usize,
    channels: rodio::ChannelCount,
    rate: rodio::SampleRate,
}

/// Trames lues d'un coup (environ 5 ms).
const TAP_FRAMES: usize = 256;

impl Iterator for Tap {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<rodio::Sample> {
        if self.pos >= self.buf.len() {
            self.buf.clear();
            self.pos = 0;
            let n = TAP_FRAMES * usize::from(self.channels.get());
            let mut src = self.source.lock();
            self.buf.extend((0..n).map(|_| src.next().unwrap_or(0.0)));
        }
        let s = self.buf[self.pos];
        self.pos += 1;
        Some(s)
    }
}

impl Source for Tap {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
