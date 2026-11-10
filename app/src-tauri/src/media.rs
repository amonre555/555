//! Contrôles multimédia du système : touches du clavier (lecture, suivant…), panneau multimédia
//! de Windows (avec titre, artiste et pochette), MPRIS sous Linux, « À l'écoute » sous macOS.
//!
//! Les contrôles vivent dans leur propre thread (l'API Windows n'aime pas changer de thread).
//! Il reçoit les états du lecteur et renvoie les commandes de l'utilisateur au contrôleur.

use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Duration,
};

use ondes_core::{Controller, Location, Snapshot, Status, Track, sources::read_cover};
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

    std::thread::Builder::new()
        .name("ondes-media-controls".into())
        .spawn(move || {
            let config = PlatformConfig {
                display_name: "Ondes",
                dbus_name: "ondes",
                hwnd: hwnd.map(|h| h as *mut c_void),
            };
            let Ok(mut controls) = MediaControls::new(config) else { return };
            if controls.attach(move |e| { let _ = events_tx.send(e); }).is_err() {
                return;
            }
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

async fn seek(c: &Arc<Controller>, to: f64) -> ondes_core::Result<()> {
    let c = c.clone();
    tauri::async_runtime::spawn_blocking(move || c.seek(to))
        .await
        .map_err(|e| ondes_core::Error::Other(e.to_string()))?
}

/// Ce qui est actuellement affiché par le système, pour ne lui envoyer que les changements.
#[derive(Default)]
struct Shown {
    track_id: Option<String>,
    status: Option<Status>,
    /// Position attendue (position affichée + temps écoulé) pour repérer les sauts.
    position: f64,
}

impl Shown {
    fn update(&mut self, controls: &mut MediaControls, snap: &Snapshot, cover_dir: &Path) {
        let id = snap.track.as_ref().map(|t| t.id.clone());
        if id != self.track_id {
            match &snap.track {
                Some(t) => {
                    let cover = cover_url(t, cover_dir);
                    let _ = controls.set_metadata(MediaMetadata {
                        title: Some(&t.title),
                        artist: Some(&t.artist),
                        album: t.album.as_deref(),
                        cover_url: cover.as_deref(),
                        duration: snap.duration.map(Duration::from_secs_f64),
                    });
                }
                None => {
                    let _ = controls.set_metadata(MediaMetadata::default());
                }
            }
            self.track_id = id;
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
            self.status = Some(snap.status);
        }
        self.position = snap.position;
    }
}

/// Pochette pour le système : URL distante, ou fichier extrait pour les morceaux locaux.
fn cover_url(t: &Track, cover_dir: &Path) -> Option<String> {
    if let Some(url) = &t.artwork {
        return Some(url.clone());
    }
    let Location::File { path } = &t.location else { return None };
    if !t.has_cover {
        return None;
    }
    let (bytes, mime) = read_cover(path)?;
    let ext = if mime.contains("png") { "png" } else { "jpg" };
    // Un nom par morceau (le système garde parfois l'ancienne image en cache), en ne gardant
    // que la pochette en cours.
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.id.hash(&mut h);
    let file = cover_dir.join(format!("cover-{:016x}.{ext}", h.finish()));
    let _ = std::fs::remove_dir_all(cover_dir);
    std::fs::create_dir_all(cover_dir).ok()?;
    std::fs::write(&file, bytes).ok()?;
    Some(format!("file://{}", file.display().to_string().replace('\\', "/")))
}
