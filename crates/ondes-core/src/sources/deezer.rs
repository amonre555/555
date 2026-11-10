//! Deezer comme source d'**infos** : recherche, top du moment, titres propres, albums et
//! pochettes carrées en 1000 × 1000. API publique, sans clé.
//!
//! Deezer ne fournit pas le son (seulement des extraits de 30 s). Au moment de jouer, on cherche
//! le même enregistrement sur YouTube Music ([`YouTubeMusicSource::find_match`]) et c'est lui qu'on
//! lit. Comme dans Nuclear : une source pour les infos, une source pour le son.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use parking_lot::Mutex;
use serde::Deserialize;

use crate::{Error, Location, MusicSource, Result, StreamData, Track, sources::YouTubeMusicSource};

const API: &str = "https://api.deezer.com";
/// Correspondances Deezer → YouTube gardées en mémoire.
const MATCHES_MAX: usize = 2000;

pub struct DeezerSource {
    client: reqwest::Client,
    youtube: Arc<YouTubeMusicSource>,
    /// Identifiant Deezer → identifiant YouTube déjà trouvé.
    matches: Mutex<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    data: Vec<DzTrack>,
    error: Option<DzError>,
}

#[derive(Deserialize)]
struct DzError {
    message: Option<String>,
}

#[derive(Deserialize)]
struct DzTrack {
    id: u64,
    title: String,
    duration: Option<f64>,
    #[serde(default = "yes")]
    readable: bool,
    artist: DzArtist,
    album: Option<DzAlbum>,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct DzArtist {
    name: String,
}

#[derive(Deserialize)]
struct DzAlbum {
    title: String,
    cover_xl: Option<String>,
    cover_big: Option<String>,
}

impl From<DzTrack> for Track {
    fn from(t: DzTrack) -> Self {
        let id = t.id.to_string();
        let (album, artwork) = match t.album {
            Some(a) => (Some(a.title), a.cover_xl.or(a.cover_big)),
            None => (None, None),
        };
        Track {
            id: format!("deezer:{id}"),
            source: "deezer".into(),
            title: t.title,
            artist: t.artist.name,
            album,
            duration: t.duration.filter(|d| *d > 0.0),
            track_number: None,
            disc_number: None,
            artwork,
            has_cover: false,
            location: Location::Remote { remote_id: id },
        }
    }
}

impl DeezerSource {
    pub fn new(client: reqwest::Client, youtube: Arc<YouTubeMusicSource>) -> Self {
        Self { client, youtube, matches: Mutex::new(HashMap::new()) }
    }

    async fn tracks(&self, path: &str, query: &[(&str, String)]) -> Result<Vec<Track>> {
        let page: Page = self
            .client
            .get(format!("{API}{path}"))
            .query(query)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if let Some(e) = page.error {
            return Err(Error::Other(format!("Deezer : {}", e.message.unwrap_or_else(|| "erreur".into()))));
        }
        Ok(page.data.into_iter().filter(|t| t.readable).map(Into::into).collect())
    }

    /// L'identifiant YouTube du même morceau (trouvé une fois, puis gardé).
    async fn youtube_id(&self, track: &Track) -> Result<String> {
        if let Some(id) = self.matches.lock().get(&track.id) {
            return Ok(id.clone());
        }
        let id = self.youtube.find_match(&track.title, &track.artist, track.duration).await?;
        let mut matches = self.matches.lock();
        if matches.len() >= MATCHES_MAX {
            matches.clear();
        }
        matches.insert(track.id.clone(), id.clone());
        Ok(id)
    }
}

#[async_trait]
impl MusicSource for DeezerSource {
    fn id(&self) -> &'static str {
        "deezer"
    }

    fn name(&self) -> &'static str {
        "Titres"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        self.tracks("/search", &[("q", query.to_owned()), ("limit", limit.to_string())]).await
    }

    /// Top Deezer du pays de l'utilisateur (déduit de son adresse IP par Deezer).
    async fn featured(&self, limit: usize) -> Result<Vec<Track>> {
        self.tracks("/chart/0/tracks", &[("limit", limit.to_string())]).await
    }

    async fn prefetch(&self, track: &Track) {
        if let Ok(id) = self.youtube_id(track).await {
            self.youtube.prefetch_id(&id).await;
        }
    }

    async fn resolve(&self, track: &Track) -> Result<StreamData> {
        let id = self.youtube_id(track).await?;
        self.youtube.stream_id(&id).await
    }
}
