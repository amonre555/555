//! Statut Discord (« Rich Presence ») : « Écoute *titre* », l'artiste, la pochette et une barre
//! de progression, comme Spotify.
//!
//! Pensé pour ne rien coûter :
//! - un thread à part, endormi tant qu'il ne se passe rien ;
//! - Discord n'est prévenu qu'aux vrais changements (morceau, lecture/pause, saut dans le
//!   morceau), au plus une fois toutes les 2 s : la barre de progression avance toute seule
//!   côté Discord grâce aux horodatages ;
//! - si Discord n'est pas lancé, on réessaie discrètement toutes les 15 s.

use std::{
    sync::mpsc::{self, RecvTimeoutError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use discord_rich_presence::{
    DiscordIpc, DiscordIpcClient,
    activity::{Activity, ActivityType, Assets, StatusDisplayType, Timestamps},
};
use yato_core::{Snapshot, Status};

/// ID of the "555" Discord application (Discord developer portal).
/// Can be overridden at build time: `YATO_DISCORD_CLIENT_ID=… npm run tauri build`.
const CLIENT_ID: &str = match option_env!("YATO_DISCORD_CLIENT_ID") {
    Some(id) => id,
    None => "1569805294308167701",
};

/// Intervalle minimal entre deux envois (Discord limite à 5 mises à jour par 20 s).
const MIN_GAP: Duration = Duration::from_secs(2);
/// Délai entre deux tentatives de connexion quand Discord n'est pas lancé.
const RETRY: Duration = Duration::from_secs(15);
/// Écart de position (en s) au-delà duquel on considère que l'utilisateur a sauté.
const JUMP: f64 = 2.0;
/// Image de l'application (à téléverser sous ce nom dans le portail Discord), pour les morceaux
/// sans pochette en ligne.
const FALLBACK_IMAGE: &str = "logo";

/// Lance le statut Discord. Renvoie le canal où envoyer chaque nouvel état du lecteur, ou
/// `None` si aucun identifiant d'application n'est configuré.
pub fn spawn() -> Option<mpsc::Sender<Snapshot>> {
    if CLIENT_ID.is_empty() {
        return None;
    }
    let (tx, rx) = mpsc::channel::<Snapshot>();
    std::thread::Builder::new()
        .name("yato-discord".into())
        .spawn(move || Presence::new().run(rx))
        .ok()?;
    Some(tx)
}

/// Ce que Discord affiche, pour ne lui envoyer que les changements.
#[derive(Clone, PartialEq)]
struct Shown {
    track_id: String,
    playing: bool,
    /// Début du morceau en temps Unix (ms) : change quand on saute dans le morceau.
    started_ms: i64,
}

struct Presence {
    client: DiscordIpcClient,
    connected: bool,
    last_attempt: Option<Instant>,
    last_sent: Option<Instant>,
    /// Dernier état reçu du lecteur.
    latest: Option<Snapshot>,
    shown: Option<Shown>,
}

impl Presence {
    fn new() -> Self {
        Self {
            client: DiscordIpcClient::new(CLIENT_ID),
            connected: false,
            last_attempt: None,
            last_sent: None,
            latest: None,
            shown: None,
        }
    }

    fn run(mut self, rx: mpsc::Receiver<Snapshot>) {
        loop {
            // Dormir jusqu'au prochain état, ou jusqu'au moment où un envoi en attente sera permis.
            let wait = if self.pending() { self.until_allowed() } else { RETRY };
            match rx.recv_timeout(wait) {
                Ok(snap) => {
                    // Ne garder que le plus récent si plusieurs états attendent.
                    self.latest = Some(rx.try_iter().last().unwrap_or(snap));
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if self.pending() && self.until_allowed().is_zero() {
                self.flush();
            }
        }
        if self.connected {
            let _ = self.client.clear_activity();
            let _ = self.client.close();
        }
    }

    /// Ce qu'il faudrait afficher pour l'état actuel (`None` : rien, statut effacé).
    fn wanted(&self) -> Option<Shown> {
        let snap = self.latest.as_ref()?;
        let track = snap.track.as_ref()?;
        // Pendant un chargement, on garde l'affichage précédent plutôt que de clignoter.
        let playing = match snap.status {
            Status::Playing => true,
            Status::Paused => false,
            Status::Loading => return self.shown.clone(),
            Status::Idle | Status::Error => return None,
        };
        if !playing {
            // En pause, pas de minuteur : rien ne change tant que la pause dure.
            return Some(Shown { track_id: track.id.clone(), playing, started_ms: 0 });
        }
        let started_ms = now_ms() - (snap.position * 1000.0) as i64;
        // Si la lecture suit son cours, garder l'horodatage déjà envoyé (pas de mise à jour).
        let started_ms = match &self.shown {
            Some(s) if s.track_id == track.id && s.playing && (s.started_ms - started_ms).abs() < (JUMP * 1000.0) as i64 => {
                s.started_ms
            }
            _ => started_ms,
        };
        Some(Shown { track_id: track.id.clone(), playing, started_ms })
    }

    /// Un changement attend d'être envoyé à Discord.
    fn pending(&self) -> bool {
        self.wanted() != self.shown
    }

    fn until_allowed(&self) -> Duration {
        let since_send = self.last_sent.map_or(MIN_GAP, |t| t.elapsed());
        let gap = MIN_GAP.saturating_sub(since_send);
        if self.connected {
            gap
        } else {
            gap.max(RETRY.saturating_sub(self.last_attempt.map_or(RETRY, |t| t.elapsed())))
        }
    }

    fn flush(&mut self) {
        if !self.connected {
            self.last_attempt = Some(Instant::now());
            if self.client.connect().is_err() {
                return; // Discord fermé : on réessaiera plus tard.
            }
            self.connected = true;
            self.shown = None;
        }
        let wanted = self.wanted();
        let ok = match (&wanted, self.latest.as_ref()) {
            (Some(w), Some(snap)) => self.client.set_activity(activity(w, snap)).is_ok(),
            _ => self.client.clear_activity().is_ok(),
        };
        self.last_sent = Some(Instant::now());
        if ok {
            self.shown = wanted;
        } else {
            // Discord a été fermé entre-temps : tout renvoyer à la prochaine connexion.
            let _ = self.client.close();
            self.connected = false;
            self.shown = None;
        }
    }
}

fn activity<'a>(shown: &Shown, snap: &'a Snapshot) -> Activity<'a> {
    let track = snap.track.as_ref().expect("morceau présent");
    let mut assets = Assets::new().large_image(match &track.artwork {
        Some(url) if url.starts_with("https://") => url.as_str(),
        _ => FALLBACK_IMAGE,
    });
    if let Some(album) = track.album.as_deref().filter(|a| *a != track.title) {
        assets = assets.large_text(clip(album));
    }
    let mut a = Activity::new()
        .activity_type(ActivityType::Listening)
        .status_display_type(StatusDisplayType::Details)
        .details(clip(&track.title))
        .state(clip(&track.artist))
        .assets(assets);
    if shown.playing {
        let mut ts = Timestamps::new().start(shown.started_ms);
        if let Some(d) = snap.duration.filter(|d| *d > 0.0) {
            ts = ts.end(shown.started_ms + (d * 1000.0) as i64);
        }
        a = a.timestamps(ts);
    } else {
        a = a.state(format!("Paused · {}", clip(&track.artist)));
    }
    a
}

/// Discord exige des textes de 2 à 128 caractères.
fn clip(s: &str) -> String {
    let mut out: String = s.chars().take(128).collect();
    if out.chars().count() > 127 {
        out = out.chars().take(127).collect::<String>() + "…";
    }
    if out.chars().count() < 2 {
        out = format!("♪ {out}");
    }
    out
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(status: Status, position: f64) -> Snapshot {
        let track = yato_core::Track {
            id: "deezer:1".into(),
            source: "deezer".into(),
            title: "Dai Dai".into(),
            artist: "Shakira".into(),
            album: None,
            album_id: None,
            duration: Some(223.0),
            track_number: None,
            disc_number: None,
            artwork: None,
            has_cover: false,
            location: yato_core::Location::Remote { remote_id: "1".into() },
            artists: Vec::new(),
        };
        Snapshot { track: Some(track), status, position, duration: Some(223.0), volume: 1.0, index: Some(0), queue_len: 1, error: None }
    }

    /// Simule un envoi réussi : Discord affiche ce qui était voulu.
    fn sent(p: &mut Presence) {
        p.shown = p.wanted();
    }

    #[test]
    fn sends_only_real_changes() {
        let mut p = Presence::new();
        p.latest = Some(snap(Status::Playing, 10.0));
        assert!(p.pending(), "nouveau morceau");
        sent(&mut p);

        // La lecture avance normalement (état reçu 4 fois par seconde) : rien à renvoyer.
        std::thread::sleep(Duration::from_millis(300));
        p.latest = Some(snap(Status::Playing, 10.3));
        assert!(!p.pending(), "lecture normale");

        // Saut dans le morceau.
        p.latest = Some(snap(Status::Playing, 120.0));
        assert!(p.pending(), "saut");
        sent(&mut p);

        // Pause : un envoi, puis plus rien tant que la pause dure.
        p.latest = Some(snap(Status::Paused, 120.0));
        assert!(p.pending(), "pause");
        sent(&mut p);
        std::thread::sleep(Duration::from_millis(50));
        p.latest = Some(snap(Status::Paused, 120.0));
        assert!(!p.pending(), "pause qui dure");

        // Chargement : on garde l'affichage. Arrêt : on efface.
        p.latest = Some(snap(Status::Loading, 0.0));
        assert!(!p.pending(), "chargement");
        p.latest = Some(snap(Status::Idle, 0.0));
        assert!(p.pending(), "arrêt");
        sent(&mut p);
        assert!(p.shown.is_none());
    }

    #[test]
    fn clip_respects_discord_limits() {
        assert_eq!(clip("A"), "♪ A");
        assert_eq!(clip(&"x".repeat(300)).chars().count(), 128);
        assert_eq!(clip("Dai Dai"), "Dai Dai");
    }
}
