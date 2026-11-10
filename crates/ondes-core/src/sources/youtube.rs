//! YouTube Music, comme la source principale de Nuclear : le plus gros catalogue disponible.
//!
//! - **Recherche** : API interne de YouTube Music via `rustypipe` (Rust pur, ~0,6 s). Elle renvoie
//!   de vrais morceaux (titre, artistes, album, pochette) plutôt que des vidéos.
//! - **Son** : `yt-dlp` (géré par [`YtDlp`]) télécharge lui-même le m4a et nous l'envoie sur sa
//!   sortie standard. On le lit au fil de l'eau.
//! - **Anticipation** : démarrer yt-dlp prend ~1 à 3 s. [`MusicSource::prefetch`] lance donc le
//!   téléchargement à l'avance (morceau suivant, premier résultat, survol) dans un cache mémoire
//!   limité : au clic, le son est déjà là.
//!
//! ⚠️ Lire YouTube en dehors de son lecteur est contraire à ses conditions d'utilisation.

use std::{path::PathBuf, sync::Arc, time::Instant};

use async_trait::async_trait;
use parking_lot::Mutex;
use rustypipe::{
    client::RustyPipe,
    model::{Thumbnail, TrackItem},
};

use crate::{Error, Location, MusicSource, Result, StreamData, Track, stream::Download, ytdlp::YtDlp};

/// Nombre de morceaux gardés en mémoire (un morceau ≈ 3 à 8 Mo).
const CACHE_TRACKS: usize = 8;

pub struct YouTubeMusicSource {
    rp: RustyPipe,
    ytdlp: Arc<YtDlp>,
    /// Téléchargements récents ou anticipés : `(id, dernier accès, téléchargement)`.
    cache: Mutex<Vec<(String, Instant, Download)>>,
}

impl YouTubeMusicSource {
    /// `storage_dir` : dossier où rustypipe garde ses jetons en cache.
    /// `ytdlp` : gestionnaire de yt-dlp (installé automatiquement au premier besoin).
    pub fn new(storage_dir: PathBuf, ytdlp: Arc<YtDlp>) -> Result<Self> {
        std::fs::create_dir_all(&storage_dir)?;
        let rp = RustyPipe::builder()
            .storage_dir(storage_dir)
            .build()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(Self { rp, ytdlp, cache: Mutex::new(Vec::new()) })
    }

    fn cached(&self, id: &str) -> Option<Download> {
        let mut cache = self.cache.lock();
        let entry = cache.iter_mut().find(|(k, _, d)| k == id && !d.failed())?;
        entry.1 = Instant::now();
        Some(entry.2.clone())
    }

    /// Le téléchargement de `id` : celui du cache, ou un nouveau.
    async fn download(&self, id: &str) -> Result<Download> {
        if let Some(d) = self.cached(id) {
            return Ok(d);
        }
        let program = self.ytdlp.path().await?;
        // Re-vérifier : une autre demande a pu le lancer pendant l'attente de yt-dlp.
        if let Some(d) = self.cached(id) {
            return Ok(d);
        }
        let base: Vec<String> = [
            // m4a/AAC : décodé nativement par symphonia, sans conversion.
            "-f", "140/bestaudio[ext=m4a]",
            "-o", "-",
            "--quiet", "--no-warnings", "--no-playlist", "--no-part",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let url = format!("https://music.youtube.com/watch?v={id}");
        // 1er essai sans moteur JavaScript (~2 s), 2e avec (~8 s) si YouTube l'exige.
        let mut attempts = vec![[base.clone(), vec![url.clone()]].concat()];
        let js = self.ytdlp.js_runtime_args();
        if !js.is_empty() {
            attempts.push([base, js, vec![url]].concat());
        }
        let download = Download::process(&program, attempts)?;

        // En parallèle : la taille du flux, pour l'avance rapide avant la fin du téléchargement.
        let (rp, dl, vid) = (self.rp.clone(), download.clone(), id.to_owned());
        tokio::spawn(async move {
            if let Ok(player) = rp.query().player(&vid).await
                && let Some(a) = player.audio_streams.iter().find(|a| a.itag == 140)
            {
                dl.set_len_hint(a.size);
            }
        });

        let mut cache = self.cache.lock();
        cache.retain(|(k, _, d)| k != id && !d.failed());
        if cache.len() >= CACHE_TRACKS
            && let Some(oldest) = cache.iter().enumerate().min_by_key(|(_, e)| e.1).map(|(i, _)| i)
        {
            cache.remove(oldest);
        }
        cache.push((id.to_owned(), Instant::now(), download.clone()));
        Ok(download)
    }
}

fn yt_err(e: rustypipe::error::Error) -> Error {
    Error::Other(format!("YouTube : {e}"))
}

/// La plus petite vignette d'au moins 300 px, sinon la plus grande.
fn best_cover(thumbs: &[Thumbnail]) -> Option<String> {
    thumbs
        .iter()
        .filter(|t| t.width >= 300)
        .min_by_key(|t| t.width)
        .or_else(|| thumbs.iter().max_by_key(|t| t.width))
        .map(|t| t.url.clone())
}

impl From<TrackItem> for Track {
    fn from(t: TrackItem) -> Self {
        let artist = if t.artists.is_empty() {
            "Artiste inconnu".to_owned()
        } else {
            t.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
        };
        Track {
            id: format!("youtube:{}", t.id),
            source: "youtube".into(),
            title: t.name,
            artist,
            album: t.album.map(|a| a.name),
            duration: t.duration.map(f64::from),
            track_number: t.track_nr.map(u32::from),
            disc_number: None,
            artwork: best_cover(&t.cover),
            has_cover: false,
            location: Location::Remote { remote_id: t.id },
        }
    }
}

fn remote_id(track: &Track) -> Result<&str> {
    match &track.location {
        Location::Remote { remote_id } => Ok(remote_id),
        Location::File { .. } => Err(Error::Other("ce morceau ne vient pas de YouTube".into())),
    }
}

#[async_trait]
impl MusicSource for YouTubeMusicSource {
    fn id(&self) -> &'static str {
        "youtube"
    }

    fn name(&self) -> &'static str {
        "YouTube Music"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let res = self.rp.query().music_search_tracks(query).await.map_err(yt_err)?;
        Ok(res.items.items.into_iter().take(limit).map(Into::into).collect())
    }

    async fn prefetch(&self, track: &Track) {
        if let Ok(id) = remote_id(track) {
            let _ = self.download(id).await;
        }
    }

    async fn resolve(&self, track: &Track) -> Result<StreamData> {
        let download = self.download(remote_id(track)?).await?;
        Ok(StreamData::Buffered { download, hint: Some("m4a") })
    }
}
