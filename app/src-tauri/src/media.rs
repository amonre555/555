//! Contrôles multimédia du système : touches du clavier (lecture, suivant…), panneau multimédia
//! de Windows (avec titre, artiste et pochette), MPRIS sous Linux, « À l'écoute » sous macOS et
//! iOS (écran verrouillé, Dynamic Island, Centre de contrôle, écouteurs).
//!
//! Les contrôles vivent dans leur propre thread (l'API Windows n'aime pas changer de thread).
//! Il reçoit les états du lecteur et renvoie les commandes de l'utilisateur au contrôleur.

use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Duration,
};

use yato_core::{Controller, Snapshot, Status};

use crate::cover::cover_url;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};

/// Lance les contrôles. Renvoie le canal où envoyer chaque nouvel état du lecteur.
/// `hwnd` : fenêtre principale (obligatoire sous Windows).
pub fn spawn(hwnd: Option<*mut c_void>, cover_dir: PathBuf, controller: Arc<Controller>) -> Option<mpsc::Sender<Snapshot>> {
    let (tx, rx) = mpsc::channel::<Snapshot>();
    let (events_tx, mut events_rx) = tokio::sync::mpsc::unbounded_channel::<MediaControlEvent>();
    let hwnd = hwnd.map(|h| h as usize); // un pointeur brut ne peut pas changer de thread
    let config = move || PlatformConfig {
        display_name: "555",
        dbus_name: "yato",
        hwnd: hwnd.map(|h| h as *mut c_void),
    };

    // iPhone : les commandes (lecture, suivant…) s'enregistrent depuis le fil principal, comme
    // Apple le demande (on y est : appelé pendant le démarrage de l'appli).
    #[cfg(target_os = "ios")]
    let attached = {
        let mut controls = MediaControls::new(config()).ok()?;
        controls.attach(move |e| { let _ = events_tx.send(e); }).ok()?;
        controls
    };

    std::thread::Builder::new()
        .name("yato-media-controls".into())
        .spawn(move || {
            #[cfg(target_os = "ios")]
            let mut controls = attached;
            #[cfg(not(target_os = "ios"))]
            let mut controls = {
                let Ok(mut controls) = MediaControls::new(config()) else { return };
                if controls.attach(move |e| { let _ = events_tx.send(e); }).is_err() {
                    return;
                }
                controls
            };
            let mut shown = Shown::default();
            for snap in rx {
                shown.update(&mut controls, &snap, &cover_dir);
            }
        })
        .ok()?;

    // Commandes de l'utilisateur → contrôleur.
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            let snap = controller.snapshot();
            let _ = match event {
                MediaControlEvent::Toggle => controller.toggle().await,
                MediaControlEvent::Play if snap.status != Status::Playing => controller.toggle().await,
                MediaControlEvent::Pause if snap.status == Status::Playing => controller.toggle().await,
                MediaControlEvent::Next => controller.next().await,
                MediaControlEvent::Previous => controller.previous().await,
                MediaControlEvent::Stop => {
                    controller.stop();
                    Ok(())
                }
                MediaControlEvent::SetPosition(MediaPosition(p)) => seek(&controller, p.as_secs_f64()).await,
                MediaControlEvent::SeekBy(dir, d) => seek(&controller, offset(snap.position, dir, d)).await,
                MediaControlEvent::Seek(dir) => {
                    seek(&controller, offset(snap.position, dir, Duration::from_secs(10))).await
                }
                _ => Ok(()),
            };
        }
    });
    Some(tx)
}

fn offset(pos: f64, dir: SeekDirection, d: Duration) -> f64 {
    match dir {
        SeekDirection::Forward => pos + d.as_secs_f64(),
        SeekDirection::Backward => (pos - d.as_secs_f64()).max(0.0),
    }
}

async fn seek(c: &Arc<Controller>, to: f64) -> yato_core::Result<()> {
    let c = c.clone();
    tauri::async_runtime::spawn_blocking(move || c.seek(to))
        .await
        .map_err(|e| yato_core::Error::Other(e.to_string()))?
}

/// Ce qui est actuellement affiché par le système, pour ne lui envoyer que les changements.
#[derive(Default)]
struct Shown {
    track_id: Option<String>,
    /// Durée envoyée : souvent inconnue au changement de morceau, elle arrive un peu après.
    duration: Option<f64>,
    /// Pochette du morceau en cours (sur iPhone, un fichier téléchargé : on ne le refait pas).
    cover: Option<String>,
    status: Option<Status>,
    /// Position attendue (position affichée + temps écoulé) pour repérer les sauts.
    position: f64,
}

impl Shown {
    fn update(&mut self, controls: &mut MediaControls, snap: &Snapshot, cover_dir: &Path) {
        let id = snap.track.as_ref().map(|t| t.id.clone());
        let duration = snap.duration.or(snap.track.as_ref().and_then(|t| t.duration)).filter(|d| *d > 0.0);
        let new_track = id != self.track_id;
        if new_track || duration != self.duration {
            match &snap.track {
                Some(t) => {
                    if new_track {
                        let cover = cover_url(t, cover_dir);
                        #[cfg(target_os = "ios")]
                        let cover = cover.and_then(|url| crate::cover::local_cover(&url, cover_dir));
                        self.cover = cover;
                    }
                    let _ = controls.set_metadata(MediaMetadata {
                        title: Some(&t.title),
                        artist: Some(&t.artist),
                        album: t.album.as_deref(),
                        cover_url: self.cover.as_deref(),
                        duration: duration.map(Duration::from_secs_f64),
                    });
                }
                None => {
                    self.cover = None;
                    let _ = controls.set_metadata(MediaMetadata::default());
                }
            }
            self.track_id = id;
            self.duration = duration;
            self.status = None; // forcer la mise à jour de l'état
        }

        // L'état envoyé ~4 fois par seconde : on ne prévient le système qu'en cas de
        // changement d'état ou de saut (avance rapide).
        let jumped = (snap.position - self.position).abs() > 1.5;
        if Some(snap.status) != self.status || jumped {
            let progress = Some(MediaPosition(Duration::from_secs_f64(snap.position.max(0.0))));
            let playback = match snap.status {
                Status::Playing => MediaPlayback::Playing { progress },
                Status::Paused | Status::Loading => MediaPlayback::Paused { progress },
                Status::Idle | Status::Error => MediaPlayback::Stopped,
            };
            let _ = controls.set_playback(playback);
            #[cfg(target_os = "ios")]
            ios::set_rate(snap.status == Status::Playing, snap.position.max(0.0), duration);
            self.status = Some(snap.status);
        }
        self.position = snap.position;
    }
}

/// iPhone : souvlaki donne l'état « lecture / pause » par `playbackState`, qui n'existe que sur
/// macOS. iOS le déduit de la vitesse de lecture : sans elle, pas de boutons sur l'écran
/// verrouillé et le temps n'avance pas. On l'ajoute à ce que souvlaki a déjà envoyé.
#[cfg(target_os = "ios")]
mod ios {
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };

    unsafe extern "C" {
        static MPNowPlayingInfoPropertyPlaybackRate: *mut AnyObject;
        static MPNowPlayingInfoPropertyElapsedPlaybackTime: *mut AnyObject;
        static MPMediaItemPropertyPlaybackDuration: *mut AnyObject;
    }

    pub fn set_rate(playing: bool, position: f64, duration: Option<f64>) {
        unsafe {
            let (Some(center_class), Some(dict_class), Some(number_class)) = (
                AnyClass::get(c"MPNowPlayingInfoCenter"),
                AnyClass::get(c"NSMutableDictionary"),
                AnyClass::get(c"NSNumber"),
            ) else {
                return;
            };
            let number = |v: f64| -> *mut AnyObject { msg_send![number_class, numberWithDouble: v] };
            let center: *mut AnyObject = msg_send![center_class, defaultCenter];
            let info: *mut AnyObject = msg_send![dict_class, dictionary];
            let previous: *mut AnyObject = msg_send![&*center, nowPlayingInfo];
            if !previous.is_null() {
                let _: () = msg_send![&*info, addEntriesFromDictionary: previous];
            }
            let rate = number(if playing { 1.0 } else { 0.0 });
            let _: () = msg_send![&*info, setObject: rate, forKey: MPNowPlayingInfoPropertyPlaybackRate];
            let elapsed = number(position);
            let _: () = msg_send![&*info, setObject: elapsed, forKey: MPNowPlayingInfoPropertyElapsedPlaybackTime];
            if let Some(d) = duration {
                let d = number(d);
                let _: () = msg_send![&*info, setObject: d, forKey: MPMediaItemPropertyPlaybackDuration];
            }
            let _: () = msg_send![&*center, setNowPlayingInfo: info];
        }
    }
}
