//! Deezer comme source d'**infos** : recherche, top du moment, titres propres, albums,
//! pochettes carrées en 1000 × 1000 et fiches d'artistes. API publique, sans clé.
//!
//! Deezer ne fournit pas le son (seulement des extraits de 30 s). Au moment de jouer, on cherche
//! le même enregistrement sur YouTube Music ([`YouTubeMusicSource::find_match`]) et c'est lui qu'on
//! lit. Comme dans Nuclear : une source pour les infos, une source pour le son.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    ArtistRef, Error, Location, MusicSource, Result, StreamData, Track,
    model::guests_in_title,
    sources::YouTubeMusicSource,
};

const API: &str = "https://api.deezer.com";
/// Correspondances Deezer → YouTube gardées en mémoire.
const MATCHES_MAX: usize = 2000;

pub struct DeezerSource {
    client: reqwest::Client,
    youtube: Arc<YouTubeMusicSource>,
    /// Identifiant Deezer → identifiant YouTube déjà trouvé.
    matches: Mutex<HashMap<String, String>>,
    /// Artistes complets d'un morceau (avec les invités), déjà demandés.
    credits: Mutex<HashMap<String, Vec<ArtistRef>>>,
}

// ---------- Ce que renvoie l'API ----------

#[derive(Deserialize)]
struct Page<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
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
    /// Présent dans le top d'un artiste et le détail d'un morceau, pas dans la recherche.
    contributors: Option<Vec<DzArtist>>,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct DzArtist {
    id: Option<u64>,
    name: String,
    picture_xl: Option<String>,
    picture_big: Option<String>,
}

#[derive(Deserialize)]
struct DzAlbum {
    id: Option<u64>,
    title: String,
    cover_xl: Option<String>,
    cover_big: Option<String>,
    release_date: Option<String>,
    record_type: Option<String>,
}

#[derive(Deserialize)]
struct DzAlbumFull {
    title: String,
    cover_xl: Option<String>,
    cover_big: Option<String>,
    tracks: Page<DzTrack>,
}

// ---------- Ce qu'on renvoie à l'interface ----------

/// Fiche d'un artiste.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistPage {
    pub id: String,
    pub name: String,
    pub picture: Option<String>,
    /// Morceaux les plus écoutés.
    pub top: Vec<Track>,
    /// Albums, singles et EP, du plus récent au plus ancien.
    pub albums: Vec<Album>,
    pub related: Vec<ArtistCard>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub title: String,
    pub cover: Option<String>,
    /// Année de sortie (« 2013 »).
    pub year: Option<String>,
    /// `album`, `single`, `ep` ou `compile`.
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistCard {
    pub id: String,
    pub name: String,
    pub picture: Option<String>,
}

fn artist_ref(a: &DzArtist) -> ArtistRef {
    ArtistRef { name: a.name.clone(), id: a.id.map(|i| i.to_string()) }
}

/// Artistes d'un morceau : les crédits complets s'ils sont fournis, sinon l'artiste principal
/// plus les invités cités dans le titre (« feat. », « with »…), sans doublon.
fn credits(t: &DzTrack) -> Vec<ArtistRef> {
    let listed: Vec<ArtistRef> = match &t.contributors {
        Some(c) if !c.is_empty() => c.iter().map(artist_ref).collect(),
        _ => vec![artist_ref(&t.artist)],
    };
    let mut out: Vec<ArtistRef> = Vec::new();
    // Deezer peut citer un artiste plusieurs fois (un rôle par mention).
    for a in listed.into_iter().chain(guests_in_title(&t.title).into_iter().map(ArtistRef::named)) {
        if !out.iter().any(|o| o.name.eq_ignore_ascii_case(&a.name)) {
            out.push(a);
        }
    }
    out
}

fn to_track(t: DzTrack, album: Option<(&str, Option<&str>)>) -> Track {
    let id = t.id.to_string();
    let artists = credits(&t);
    let (album_title, artwork) = match (album, t.album) {
        (Some((title, cover)), _) => (Some(title.to_owned()), cover.map(str::to_owned)),
        (None, Some(a)) => (Some(a.title), a.cover_xl.or(a.cover_big)),
        (None, None) => (None, None),
    };
    Track {
        id: format!("deezer:{id}"),
        source: "deezer".into(),
        artist: artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
        title: t.title,
        album: album_title,
        duration: t.duration.filter(|d| *d > 0.0),
        track_number: None,
        disc_number: None,
        artwork,
        has_cover: false,
        location: Location::Remote { remote_id: id },
        artists,
    }
}

impl DeezerSource {
    pub fn new(client: reqwest::Client, youtube: Arc<YouTubeMusicSource>) -> Self {
        Self { client, youtube, matches: Mutex::new(HashMap::new()), credits: Mutex::new(HashMap::new()) }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let body: serde_json::Value = self
            .client
            .get(format!("{API}{path}"))
            .query(query)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        // Deezer répond 200 même en cas d'erreur (quota, introuvable…), avec un champ `error`.
        if let Some(e) = body.get("error") {
            let msg = e.get("message").and_then(|m| m.as_str()).unwrap_or("erreur");
            return Err(Error::Other(format!("Infos indisponibles : {msg}")));
        }
        serde_json::from_value(body).map_err(|e| Error::Other(format!("Infos indisponibles : réponse inattendue ({e})")))
    }

    async fn list<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<Vec<T>> {
        let page: Page<T> = self.get(path, query).await?;
        if let Some(e) = page.error {
            return Err(Error::Other(format!("Infos indisponibles : {}", e.message.unwrap_or_else(|| "erreur".into()))));
        }
        Ok(page.data)
    }

    async fn tracks(&self, path: &str, query: &[(&str, String)]) -> Result<Vec<Track>> {
        let data: Vec<DzTrack> = self.list(path, query).await?;
        Ok(data.into_iter().filter(|t| t.readable).map(|t| to_track(t, None)).collect())
    }

    /// L'identifiant YouTube du même morceau (trouvé une fois, puis gardé).
    async fn youtube_id(&self, track: &Track) -> Result<String> {
        if let Some(id) = self.matches.lock().get(&track.id) {
            return Ok(id.clone());
        }
        // Artiste principal seulement : les invités sont souvent dans le titre côté YouTube.
        let artist = track.artists.first().map_or(track.artist.as_str(), |a| a.name.as_str());
        let id = self.youtube.find_match(&track.title, artist, track.duration).await?;
        let mut matches = self.matches.lock();
        if matches.len() >= MATCHES_MAX {
            matches.clear();
        }
        matches.insert(track.id.clone(), id.clone());
        Ok(id)
    }

    /// Tous les artistes d'un morceau, avec leurs identifiants. La recherche Deezer ne donne que
    /// l'artiste principal : pour un morceau Deezer, on demande ses crédits complets (une fois).
    /// Pour les autres sources, on garde ce que le morceau connaît déjà.
    pub async fn track_artists(&self, track: &Track) -> Result<Vec<ArtistRef>> {
        let (Location::Remote { remote_id }, "deezer") = (&track.location, track.source.as_str()) else {
            return Ok(track.artists.clone());
        };
        if let Some(c) = self.credits.lock().get(remote_id) {
            return Ok(c.clone());
        }
        let full: DzTrack = self.get(&format!("/track/{remote_id}"), &[]).await?;
        let mut found = credits(&full);
        // Les invités déjà repérés dans le titre restent, même si Deezer ne les crédite pas.
        for a in &track.artists {
            if !found.iter().any(|f| f.name.eq_ignore_ascii_case(&a.name)) {
                found.push(a.clone());
            }
        }
        self.credits.lock().insert(remote_id.clone(), found.clone());
        Ok(found)
    }

    /// Fiche d'un artiste : par son identifiant Deezer, sinon retrouvé par son nom.
    pub async fn artist(&self, id: Option<&str>, name: &str) -> Result<ArtistPage> {
        let id = match id {
            Some(id) => id.to_owned(),
            None => {
                let found: Vec<DzArtist> = self.list("/search/artist", &[("q", name.to_owned()), ("limit", "5".into())]).await?;
                // Nom identique de préférence, sinon le premier (le plus populaire).
                let best = found.iter().find(|a| a.name.eq_ignore_ascii_case(name)).or(found.first());
                best.and_then(|a| a.id)
                    .map(|i| i.to_string())
                    .ok_or_else(|| Error::Other(format!("Aucun artiste « {name} » trouvé")))?
            }
        };
        let (info_path, top_path, albums_path, related_path) =
            (format!("/artist/{id}"), format!("/artist/{id}/top"), format!("/artist/{id}/albums"), format!("/artist/{id}/related"));
        let (top_q, albums_q, related_q) =
            ([("limit", "10".to_owned())], [("limit", "100".to_owned())], [("limit", "12".to_owned())]);
        let (info, top, albums, related) = tokio::join!(
            self.get::<DzArtist>(&info_path, &[]),
            self.tracks(&top_path, &top_q),
            self.list::<DzAlbum>(&albums_path, &albums_q),
            self.list::<DzArtist>(&related_path, &related_q),
        );
        let info = info?;
        let mut albums = albums.unwrap_or_default();
        // Du plus récent au plus ancien (date complète « 2013-05-17 »).
        albums.sort_by(|a, b| b.release_date.cmp(&a.release_date));
        let albums: Vec<Album> = albums
            .into_iter()
            .filter_map(|a| {
                Some(Album {
                    id: a.id?.to_string(),
                    title: a.title,
                    cover: a.cover_xl.or(a.cover_big),
                    year: a.release_date.as_deref().and_then(|d| d.get(..4)).map(str::to_owned),
                    kind: a.record_type.unwrap_or_else(|| "album".into()),
                })
            })
            .collect();
        Ok(ArtistPage {
            id,
            picture: info.picture_xl.or(info.picture_big),
            name: info.name,
            top: top.unwrap_or_default(),
            albums,
            related: related
                .unwrap_or_default()
                .into_iter()
                .filter_map(|a| Some(ArtistCard { id: a.id?.to_string(), name: a.name, picture: a.picture_xl.or(a.picture_big) }))
                .collect(),
        })
    }

    /// Les morceaux d'un album, dans l'ordre, avec sa pochette.
    pub async fn album_tracks(&self, id: &str) -> Result<Vec<Track>> {
        let album: DzAlbumFull = self.get(&format!("/album/{id}"), &[]).await?;
        let cover = album.cover_xl.or(album.cover_big);
        Ok(album
            .tracks
            .data
            .into_iter()
            .filter(|t| t.readable)
            .enumerate()
            .map(|(i, t)| {
                let mut track = to_track(t, Some((&album.title, cover.as_deref())));
                track.track_number = Some(i as u32 + 1);
                track
            })
            .collect())
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
