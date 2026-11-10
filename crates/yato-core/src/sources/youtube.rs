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
    param::Country,
};

use crate::{ArtistRef, Error, Location, MusicSource, Result, StreamData, Track, stream::Download, ytdlp::YtDlp};

/// Nombre de morceaux gardés en mémoire (un morceau ≈ 3 à 8 Mo).
const CACHE_TRACKS: usize = 8;

pub struct YouTubeMusicSource {
    rp: RustyPipe,
    ytdlp: Arc<YtDlp>,
    /// Téléchargements récents ou anticipés : `(id, dernier accès, téléchargement)`.
    cache: Mutex<Vec<(String, Instant, Download)>>,
}

impl YouTubeMusicSource {
    /// Nom et morceaux d'une playlist YouTube / YouTube Music (jusqu'à 5000 morceaux).
    pub async fn playlist(&self, id: &str) -> Result<(String, Vec<Track>)> {
        let mut pl = self.rp.query().music_playlist(id).await.map_err(yt_err)?;
        pl.tracks.extend_limit(self.rp.query(), 5000).await.map_err(yt_err)?;
        Ok((pl.name, pl.tracks.items.into_iter().map(Track::from).collect()))
    }

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

    /// Le son d'une vidéo YouTube Music, par son identifiant.
    pub async fn stream_id(&self, id: &str) -> Result<StreamData> {
        Ok(StreamData::Buffered { download: self.download(id).await?, hint: Some("m4a") })
    }

    /// Lance à l'avance le téléchargement d'une vidéo YouTube Music.
    pub async fn prefetch_id(&self, id: &str) {
        let _ = self.download(id).await;
    }

    /// Trouve sur YouTube Music l'enregistrement d'un morceau décrit par une autre source
    /// (Deezer…) et renvoie son identifiant.
    ///
    /// On cherche « artiste titre », puis on note les premiers résultats : titre et artiste
    /// ressemblants, durée proche, et pas de version live / remix / accélérée… si l'original
    /// n'en est pas une.
    pub async fn find_match(&self, title: &str, artist: &str, duration: Option<f64>) -> Result<String> {
        let query = format!("{artist} {title}");
        let res = self.rp.query().music_search_tracks(&query).await.map_err(yt_err)?;
        res.items
            .items
            .iter()
            .take(10)
            .map(|c| (match_score(c, title, artist, duration), c))
            .filter(|(s, _)| *s >= MATCH_MIN_SCORE)
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, c)| c.id.clone())
            .ok_or_else(|| Error::Other(format!("« {title} » de {artist} introuvable sur YouTube Music")))
    }
}

/// En dessous, le résultat ressemble trop peu au morceau cherché : mieux vaut une erreur
/// claire qu'un autre morceau.
const MATCH_MIN_SCORE: f64 = 2.0;

/// Mots qui signalent une autre version que l'originale.
const VERSION_WORDS: &[&str] = &[
    "live", "remix", "sped", "slowed", "reverb", "nightcore", "karaoke", "cover", "instrumental", "acoustic",
    "acoustique", "8d", "edit", "extended", "version",
];

/// Mots en minuscules, sans ponctuation (fonctionne aussi en cyrillique, en japonais…).
fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Part des mots de `wanted` présents dans `got` (0 à 1).
fn overlap(wanted: &[String], got: &[String]) -> f64 {
    if wanted.is_empty() {
        return 0.0;
    }
    wanted.iter().filter(|w| got.contains(w)).count() as f64 / wanted.len() as f64
}

fn match_score(c: &TrackItem, title: &str, artist: &str, duration: Option<f64>) -> f64 {
    let (want_title, got_title) = (words(title), words(&c.name));
    let want_artist = words(artist);
    let got_artist: Vec<String> = c.artists.iter().flat_map(|a| words(&a.name)).collect();

    let mut score = overlap(&want_title, &got_title) * 3.0 + overlap(&want_artist, &got_artist) * 2.0;
    if let (Some(d), Some(cd)) = (duration, c.duration) {
        let gap = (d - f64::from(cd)).abs();
        score += match gap {
            g if g <= 3.0 => 2.0,
            g if g <= 8.0 => 1.0,
            g if g <= 20.0 => 0.0,
            _ => -2.0,
        };
    }
    for w in VERSION_WORDS {
        let w = (*w).to_owned();
        if got_title.contains(&w) && !want_title.contains(&w) {
            score -= 1.5;
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_handles_any_script() {
        assert_eq!(words("Мало тебя (Remix)"), vec!["мало", "тебя", "remix"]);
        assert_eq!(words("Sweat - Snoop Dogg vs. David Guetta"), vec!["sweat", "snoop", "dogg", "vs", "david", "guetta"]);
    }

    #[test]
    fn overlap_counts_wanted_words() {
        let want = words("Dai Dai");
        assert_eq!(overlap(&want, &words("Dai Dai (Official)")), 1.0);
        assert_eq!(overlap(&want, &words("Hips Don't Lie")), 0.0);
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
            album_id: None,
            duration: t.duration.map(f64::from),
            track_number: t.track_nr.map(u32::from),
            disc_number: None,
            artwork: best_cover(&t.cover),
            has_cover: false,
            location: Location::Remote { remote_id: t.id },
            // Identifiants YouTube inutiles pour les fiches (Deezer) : retrouvées par le nom.
            artists: t.artists.iter().map(|a| ArtistRef::named(&a.name)).collect(),
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

    /// Classements YouTube Music en France.
    ///
    /// YouTube ne met plus les morceaux directement dans les classements (`top_tracks` et
    /// `trending_tracks` arrivent vides) : ils sont dans des playlists (« Trending 20 France »,
    /// « Top 100 Music Videos France »…), lues dans l'ordre, sans doublons.
    async fn featured(&self, limit: usize) -> Result<Vec<Track>> {
        let charts = self.rp.query().music_charts(Some(Country::Fr)).await.map_err(yt_err)?;
        let mut seen = std::collections::HashSet::new();
        let mut tracks: Vec<Track> = charts
            .top_tracks
            .into_iter()
            .chain(charts.trending_tracks)
            .filter(|t| seen.insert(t.id.clone()))
            .map(Into::into)
            .collect();
        for playlist in &charts.playlists {
            if tracks.len() >= limit {
                break;
            }
            let Ok(pl) = self.rp.query().music_playlist(&playlist.id).await else { continue };
            tracks.extend(pl.tracks.items.into_iter().filter(|t| seen.insert(t.id.clone())).map(Track::from));
        }
        if tracks.is_empty() {
            return Err(Error::Other("YouTube Music : classements vides".into()));
        }
        tracks.truncate(limit);
        Ok(tracks)
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
