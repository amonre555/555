//! YouTube Music, comme la source principale de Nuclear : le plus gros catalogue disponible.
//!
//! - **Recherche** : API interne de YouTube Music via `rustypipe` (Rust pur, ~0,6 s). Elle renvoie
//!   de vrais morceaux (titre, artistes, album, pochette) plutôt que des vidéos.
//! - **Son** : `yt-dlp` (géré par [`YtDlp`]) télécharge lui-même le m4a et nous l'envoie sur sa
//!   sortie standard. On le lit au fil de l'eau.
//! - **Démarrage rapide** : l'adresse directe du flux (rustypipe) remplit le début du tampon
//!   pendant que yt-dlp démarre (ou s'installe) ; yt-dlp prend le relais si YouTube coupe ce
//!   flux direct.
//! - **Anticipation** : démarrer yt-dlp prend ~1 à 3 s. [`MusicSource::prefetch`] lance donc le
//!   téléchargement à l'avance (morceau suivant, premier résultat, survol) dans un cache mémoire
//!   limité : au clic, le son est déjà là. [`YouTubeMusicSource::warm_id`] est la version
//!   légère (l'adresse directe seulement), pour les morceaux affichés.
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
/// Durée pendant laquelle une adresse directe est réutilisée (YouTube les garde ~6 h).
const DIRECT_TTL: std::time::Duration = std::time::Duration::from_secs(3600);
/// Nombre d'adresses directes gardées.
const DIRECT_MAX: usize = 200;

/// Identifiant → (obtenue à, adresse directe du m4a, taille).
type DirectUrls = Arc<Mutex<std::collections::HashMap<String, (Instant, String, u64)>>>;

/// Demande à YouTube l'adresse directe du m4a (itag 140) et la garde.
///
/// Sur Android (pas de yt-dlp), on demande d'abord au client « visionOS » : ses adresses se
/// téléchargent en entier par plages de 1 Mo. Celles du client iOS de rustypipe sont refusées
/// après le premier Mo.
async fn fetch_direct(rp: &RustyPipe, http: &reqwest::Client, direct: &DirectUrls, id: &str) -> Option<(String, u64)> {
    let t0 = Instant::now();
    let mut found = None;
    if cfg!(target_os = "android") {
        found = visionos_direct(rp, http, id).await;
        crate::timing::step("flux direct : adresse obtenue (visionOS)", t0);
    }
    if found.is_none() {
        let player = rp.query().player(id).await;
        crate::timing::step("flux direct : adresse obtenue (rustypipe player)", t0);
        let a = player.ok()?.audio_streams.into_iter().find(|a| a.itag == 140)?;
        found = Some((a.url, a.size));
    }
    let (url, size) = found?;
    let mut direct = direct.lock();
    if direct.len() >= DIRECT_MAX {
        direct.retain(|_, (at, _, _)| at.elapsed() < DIRECT_TTL);
        if direct.len() >= DIRECT_MAX {
            direct.clear();
        }
    }
    direct.insert(id.to_owned(), (Instant::now(), url.clone(), size));
    Some((url, size))
}

const VISIONOS_UA: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

/// Le client « visionOS » de YouTube (celui qu'utilise yt-dlp sans moteur JavaScript) : ses
/// adresses sont déjà signées, rien à déchiffrer. Il exige un identifiant de visiteur.
async fn visionos_direct(rp: &RustyPipe, http: &reqwest::Client, id: &str) -> Option<(String, u64)> {
    let visitor = rp.query().get_visitor_data(false).await.ok()?;
    let body = serde_json::json!({
        "context": {"client": {
            "clientName": "VISIONOS", "clientVersion": "1.02", "deviceMake": "Apple",
            "deviceModel": "RealityDevice17,1", "userAgent": VISIONOS_UA, "osName": "visionOS",
            "osVersion": "26.5.23O471", "hl": "en", "timeZone": "UTC", "utcOffsetMinutes": 0
        }},
        "videoId": id,
        "contentCheckOk": true,
        "racyCheckOk": true
    });
    let v: serde_json::Value = http
        .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
        .header("User-Agent", VISIONOS_UA)
        .header("X-Youtube-Client-Name", "101")
        .header("X-Youtube-Client-Version", "1.02")
        .header("Origin", "https://www.youtube.com")
        .header("X-Goog-Visitor-Id", visitor)
        .json(&body)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let f = v["streamingData"]["adaptiveFormats"].as_array()?.iter().find(|f| f["itag"] == 140)?;
    Some((f["url"].as_str()?.to_owned(), f["contentLength"].as_str()?.parse().ok()?))
}

pub struct YouTubeMusicSource {
    rp: RustyPipe,
    ytdlp: Arc<YtDlp>,
    /// Téléchargements récents ou anticipés : `(id, dernier accès, téléchargement)`.
    cache: Mutex<Vec<(String, Instant, Download)>>,
    /// Adresses directes déjà obtenues (voir [`Self::warm_id`]).
    direct: DirectUrls,
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
        Ok(Self { rp, ytdlp, cache: Mutex::new(Vec::new()), direct: DirectUrls::default() })
    }

    fn cached(&self, id: &str) -> Option<Download> {
        let mut cache = self.cache.lock();
        let entry = cache.iter_mut().find(|(k, _, d)| k == id && !d.failed())?;
        entry.1 = Instant::now();
        Some(entry.2.clone())
    }

    /// Le téléchargement de `id` : celui du cache, ou un nouveau.
    ///
    /// Deux sources remplissent le même tampon, lancées tout de suite et sans s'attendre :
    /// - l'adresse directe du fichier (itag 140), lue par rustypipe en ~0,1 à 0,3 s (ou déjà
    ///   connue, voir [`Self::warm_id`]) : le son démarre sans attendre yt-dlp. YouTube refuse
    ///   en général ce flux direct après le premier Mo (≈ 1 min de son) ;
    /// - yt-dlp, qui prend alors le relais sans coupure. Le lancer prend 1 à 3 s (bien plus au
    ///   tout premier lancement, surtout sous Windows : installation, antivirus, Python), d'où
    ///   l'intérêt de ne pas l'attendre pour démarrer.
    ///
    /// L'adresse directe donne aussi la taille du flux, pour l'avance rapide avant la fin du
    /// téléchargement.
    async fn download(&self, id: &str) -> Result<Download> {
        if let Some(d) = self.cached(id) {
            crate::timing::step("youtube : déjà dans le cache (anticipé)", Instant::now());
            return Ok(d);
        }
        if cfg!(target_os = "android") {
            return self.download_direct_only(id).await;
        }
        let ytdlp = self.ytdlp.clone();
        let url = format!("https://music.youtube.com/watch?v={id}");
        let download = Download::process_when_ready(async move {
            let t0 = Instant::now();
            let program = ytdlp.path().await?;
            crate::timing::step("yt-dlp : chemin prêt (installation si absent)", t0);
            let base: Vec<String> = [
                // m4a/AAC : décodé nativement par symphonia, sans conversion.
                "-f", "140/bestaudio[ext=m4a]",
                "-o", "-",
                "--quiet", "--no-warnings", "--no-playlist", "--no-part",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect();
            // 1er essai sans moteur JavaScript (~2 s), 2e avec (~8 s) si YouTube l'exige.
            let mut attempts = vec![[base.clone(), vec![url.clone()]].concat()];
            let js = ytdlp.js_runtime_args();
            if !js.is_empty() {
                attempts.push([base, js, vec![url]].concat());
            }
            Ok((program, attempts))
        });

        let this_direct = self.direct_known(id);
        let (rp, dl, vid, client) = (self.rp.clone(), download.clone(), id.to_owned(), self.ytdlp.client().clone());
        let direct = self.direct.clone();
        tokio::spawn(async move {
            let found = match this_direct {
                Some(d) => {
                    crate::timing::step("flux direct : adresse déjà connue", Instant::now());
                    Some(d)
                }
                None => fetch_direct(&rp, &client, &direct, &vid).await,
            };
            if let Some((url, size)) = found {
                dl.add_http(&client, &url, size);
            }
        });

        self.remember(id, download.clone());
        Ok(download)
    }

    /// Android : l'adresse directe seule, sans yt-dlp (voir [`fetch_direct`]).
    async fn download_direct_only(&self, id: &str) -> Result<Download> {
        let client = self.ytdlp.client();
        let (url, size) = match self.direct_known(id) {
            Some(d) => d,
            None => fetch_direct(&self.rp, client, &self.direct, id)
                .await
                .ok_or_else(|| Error::Other(format!("YouTube: no audio stream for {id}")))?,
        };
        let download = Download::direct(client, &url, size);
        self.remember(id, download.clone());
        Ok(download)
    }

    fn remember(&self, id: &str, download: Download) {
        let mut cache = self.cache.lock();
        cache.retain(|(k, _, d)| k != id && !d.failed());
        if cache.len() >= CACHE_TRACKS
            && let Some(oldest) = cache.iter().enumerate().min_by_key(|(_, e)| e.1).map(|(i, _)| i)
        {
            cache.remove(oldest);
        }
        cache.push((id.to_owned(), Instant::now(), download));
    }

    /// Adresse directe déjà connue et encore valable.
    fn direct_known(&self, id: &str) -> Option<(String, u64)> {
        let direct = self.direct.lock();
        direct.get(id).filter(|(at, _, _)| at.elapsed() < DIRECT_TTL).map(|(_, u, s)| (u.clone(), *s))
    }

    /// Préparation légère : l'adresse directe du son, sans rien télécharger. Au clic, il ne
    /// reste qu'à lire les premiers Ko.
    pub async fn warm_id(&self, id: &str) {
        if self.cached(id).is_none() && self.direct_known(id).is_none() {
            fetch_direct(&self.rp, self.ytdlp.client(), &self.direct, id).await;
        }
    }

    /// Chauffe le client YouTube Music (connexion, jetons) au lancement de l'appli, pour que
    /// la première recherche de correspondance ne paie pas ce coût.
    pub async fn warm_up(&self) {
        let t0 = Instant::now();
        let _ = self.rp.query().music_search_tracks("a").await;
        crate::timing::step("youtube : client chauffé au lancement", t0);
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
        let t0 = Instant::now();
        let res = self.rp.query().music_search_tracks(&query).await.map_err(yt_err)?;
        crate::timing::step("deezer → youtube : recherche de la correspondance", t0);
        res.items
            .items
            .iter()
            .take(10)
            .map(|c| (match_score(c, title, artist, duration), c))
            .filter(|(s, _)| *s >= MATCH_MIN_SCORE)
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, c)| c.id.clone())
            .ok_or_else(|| Error::Other(format!("\"{title}\" by {artist} not found on YouTube Music")))
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
            "Unknown artist".to_owned()
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
        Location::File { .. } => Err(Error::Other("this track is not from YouTube".into())),
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
            return Err(Error::Other("YouTube Music: empty charts".into()));
        }
        tracks.truncate(limit);
        Ok(tracks)
    }

    async fn prefetch(&self, track: &Track) {
        if let Ok(id) = remote_id(track) {
            let _ = self.download(id).await;
        }
    }

    async fn warm(&self, track: &Track) {
        if let Ok(id) = remote_id(track) {
            self.warm_id(id).await;
        }
    }

    async fn resolve(&self, track: &Track) -> Result<StreamData> {
        let download = self.download(remote_id(track)?).await?;
        Ok(StreamData::Buffered { download, hint: Some("m4a") })
    }
}
