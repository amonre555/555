//! Android : lecture écran éteint, notification du lecteur, écran verrouillé, casque.
//!
//! Le côté Android (gen/android/…/PlaybackService.kt et MediaSessionPlugin.kt) garde l'appli en
//! vie et parle au système. Ici, le pont avec le cœur : chaque changement de morceau ou d'état
//! part vers le service, et ses commandes (lecture, suivant, avance…) reviennent au contrôleur
//! par un canal. Rien ne passe par l'interface : écran éteint, la WebView est en pause, mais le
//! cœur (et donc ce pont) continue de tourner.

use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Manager, Runtime,
    ipc::{Channel, InvokeResponseBody},
    plugin::{Builder, PluginHandle, TauriPlugin},
};
use yato_core::{Controller, Snapshot, Status};

use crate::cover::cover_url;

struct Handle<R: Runtime>(PluginHandle<R>);

/// Le greffon Tauri qui charge la partie Kotlin (`MediaSessionPlugin`).
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("mediasession")
        .setup(|app, api| {
            let handle = api.register_android_plugin("app.yato.player", "MediaSessionPlugin")?;
            app.manage(Handle(handle));
            Ok(())
        })
        .build()
}

/// Commandes envoyées par le système (notification, écran verrouillé, casque, appel entrant…).
#[derive(Deserialize, Debug)]
#[serde(tag = "action", rename_all = "camelCase")]
enum Command {
    Play,
    Pause,
    Next,
    Previous,
    Stop,
    Seek { position: f64 },
}

#[derive(Serialize)]
struct Init<'a> {
    channel: &'a Channel,
}

#[derive(Serialize, Default, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Update {
    /// Un morceau est chargé : le service tourne (lecture ou pause).
    active: bool,
    title: String,
    artist: String,
    album: Option<String>,
    cover: Option<String>,
    duration: f64,
    position: f64,
    playing: bool,
}

/// Lance le pont. Renvoie le canal où envoyer chaque nouvel état du lecteur (comme `media::spawn`).
pub fn spawn<R: Runtime>(app: &AppHandle<R>, cover_dir: PathBuf, controller: Arc<Controller>) -> Option<mpsc::Sender<Snapshot>> {
    let handle = app.try_state::<Handle<R>>()?.0.clone();
    let (tx, rx) = mpsc::channel::<Snapshot>();
    let (commands_tx, mut commands_rx) = tokio::sync::mpsc::unbounded_channel::<Command>();

    std::thread::Builder::new()
        .name("yato-media-session".into())
        .spawn(move || {
            // Gardé en vie par ce fil : le service s'en sert pendant toute la vie de l'appli.
            let channel = Channel::new(move |body| {
                if let InvokeResponseBody::Json(json) = body
                    && let Ok(c) = serde_json::from_str::<Command>(&json)
                {
                    let _ = commands_tx.send(c);
                }
                Ok(())
            });
            if let Err(e) = handle.run_mobile_plugin::<serde_json::Value>("init", Init { channel: &channel }) {
                eprintln!("session multimédia indisponible : {e}");
                return;
            }
            let mut shown = Shown::default();
            for snap in rx {
                if let Some(update) = shown.next(&snap, &cover_dir) {
                    let _ = handle.run_mobile_plugin::<serde_json::Value>("update", update);
                }
            }
            drop(channel);
        })
        .ok()?;

    tauri::async_runtime::spawn(async move {
        while let Some(command) = commands_rx.recv().await {
            let playing = controller.snapshot().status == Status::Playing;
            let _ = match command {
                Command::Play if !playing => controller.toggle().await,
                Command::Pause if playing => controller.toggle().await,
                Command::Play | Command::Pause => Ok(()),
                Command::Next => controller.next().await,
                Command::Previous => controller.previous().await,
                Command::Stop => {
                    controller.stop();
                    Ok(())
                }
                Command::Seek { position } => {
                    let c = controller.clone();
                    tauri::async_runtime::spawn_blocking(move || c.seek(position.max(0.0)))
                        .await
                        .unwrap_or(Ok(()))
                }
            };
        }
    });
    Some(tx)
}

/// Dernier état envoyé : on ne prévient Android qu'aux changements (morceau, lecture/pause,
/// saut dans le morceau), pas aux 4 états par seconde du lecteur.
#[derive(Default)]
struct Shown {
    last: Option<Update>,
    track_id: Option<String>,
    cover: Option<String>,
}

impl Shown {
    fn next(&mut self, snap: &Snapshot, cover_dir: &std::path::Path) -> Option<Update> {
        let id = snap.track.as_ref().map(|t| t.id.clone());
        if id != self.track_id {
            self.cover = snap.track.as_ref().and_then(|t| cover_url(t, cover_dir));
            self.track_id = id;
        }
        let update = match &snap.track {
            Some(t) if !matches!(snap.status, Status::Idle) => Update {
                active: true,
                title: t.title.clone(),
                artist: t.artist.clone(),
                album: t.album.clone(),
                cover: self.cover.clone(),
                duration: snap.duration.or(t.duration).unwrap_or(0.0),
                position: snap.position.max(0.0),
                // En chargement : on montre déjà « lecture » (le son arrive).
                playing: matches!(snap.status, Status::Playing | Status::Loading),
            },
            _ => Update::default(),
        };
        let changed = match &self.last {
            None => true,
            Some(last) => {
                // Position attendue si rien n'a sauté : celle d'avant, plus le temps de lecture.
                let jumped = (update.position - last.position).abs() > 1.5;
                let same = Update { position: last.position, ..update.clone() } == *last;
                !same || jumped
            }
        };
        let last_position = update.position;
        if changed {
            self.last = Some(update.clone());
            return Some(update);
        }
        if let Some(last) = &mut self.last {
            last.position = last_position;
        }
        None
    }
}
