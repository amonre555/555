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
    /// Nom, image et morceaux d'une playlist YouTube / YouTube Music (jusqu'à 5000 morceaux).
    pub async fn playlist(&self, id: &str) -> Result<(String, Option<String>, Vec<Track>)> {
        let mut pl = self.rp.query().music_playlist(id).await.map_err(yt_err)?;
        pl.tracks.extend_limit(self.rp.query(), 5000).await.map_err(yt_err)?;
        let cover = pl.thumbnail.iter().max_by_key(|t| t.width).map(|t| t.url.clone());
        Ok((pl.name, cover, pl.tracks.items.into_iter().map(Track::from).collect()))
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
    /// On cherche « artiste titre », puis on note les premiers résultats (voir [`match_score`]) :
    /// même titre, même artiste, durée proche, et exactement les mêmes variantes (live,
    /// instrumentale, remix…) que l'original.
    pub async fn find_match(&self, title: &str, artist: &str, duration: Option<f64>) -> Result<String> {
        self.find_track(title, artist, duration)
            .await?
            .and_then(|t| match t.location {
                Location::Remote { remote_id } => Some(remote_id),
                Location::File { .. } => None,
            })
            .ok_or_else(|| Error::Other(format!("\"{title}\" by {artist} not found on YouTube Music")))
    }

    /// Comme [`Self::find_match`], mais renvoie le morceau entier (ou rien s'il ne ressemble
    /// pas assez) : sert aussi de repli à l'import Spotify pour ce que Deezer n'a pas.
    pub async fn find_track(&self, title: &str, artist: &str, duration: Option<f64>) -> Result<Option<Track>> {
        let query = format!("{artist} {title}");
        let wanted = Wanted::new(title, artist, duration);
        let t0 = Instant::now();
        let songs = self.rp.query().music_search_tracks(&query).await.map_err(yt_err)?;
        crate::timing::step("deezer → youtube : recherche de la correspondance", t0);
        let mut best = best_match(songs.items.items, &wanted);
        // Certains enregistrements (instrumentales, titres peu connus…) n'existent sur YouTube
        // Music qu'en vidéo : on ne les cherche que si aucun morceau officiel ne convient.
        if best.as_ref().is_none_or(|(s, _)| *s < MATCH_GOOD_SCORE) {
            let t0 = Instant::now();
            if let Ok(videos) = self.rp.query().music_search_videos(&query).await {
                crate::timing::step("deezer → youtube : repli sur les vidéos", t0);
                if let Some(v) = best_match(videos.items.items, &wanted)
                    && best.as_ref().is_none_or(|(s, _)| v.0 > *s)
                {
                    best = Some(v);
                }
            }
        }
        Ok(best.filter(|(s, _)| *s >= MATCH_MIN_SCORE).map(|(_, c)| Track::from(c)))
    }
}

/// En dessous, le résultat ressemble trop peu au morceau cherché : mieux vaut une erreur
/// claire qu'un autre morceau.
const MATCH_MIN_SCORE: f64 = 4.5;
/// Au-dessus, le morceau trouvé convient : inutile de chercher aussi parmi les vidéos.
/// (Note maximale : 9,5.)
const MATCH_GOOD_SCORE: f64 = 7.5;

/// Le meilleur des premiers résultats d'une recherche, avec sa note. À égalité, le mieux
/// classé par YouTube l'emporte.
fn best_match(items: Vec<TrackItem>, wanted: &Wanted) -> Option<(f64, TrackItem)> {
    items
        .into_iter()
        .take(10)
        .enumerate()
        .map(|(rank, c)| {
            let artists: Vec<&str> = c.artists.iter().map(|a| a.name.as_str()).collect();
            let found = Candidate { title: &c.name, artists: &artists, duration: c.duration, official: c.track_type.is_track() };
            (match_score(wanted, &found) - rank as f64 * 0.05, c)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
}

/// Mots qui signalent un autre enregistrement que l'original, avec leur forme commune
/// (« rmx » = « remix »…). Un résultat doit avoir exactement les mêmes que le titre cherché.
/// Les variantes « légères » ([`SOFT_VARIANTS`]) comptent moins : « Radio Edit » ou
/// « Album Version » désignent souvent l'enregistrement habituel.
const VARIANT_WORDS: &[(&str, &str)] = &[
    ("live", "live"),
    ("remix", "remix"),
    ("rmx", "remix"),
    ("mix", "mix"),
    ("bootleg", "bootleg"),
    ("rework", "rework"),
    ("mashup", "mashup"),
    ("sped", "sped"),
    ("speed", "sped"),
    ("slowed", "slowed"),
    ("reverb", "reverb"),
    ("nightcore", "nightcore"),
    ("karaoke", "karaoke"),
    ("cover", "cover"),
    ("instrumental", "instrumental"),
    ("instru", "instrumental"),
    ("acapella", "acapella"),
    ("acoustic", "acoustic"),
    ("acoustique", "acoustic"),
    ("unplugged", "acoustic"),
    ("8d", "8d"),
    ("edit", "edit"),
    ("extended", "extended"),
    ("version", "version"),
    ("demo", "demo"),
    ("reprise", "reprise"),
    ("orchestral", "orchestral"),
    ("drumless", "drumless"),
    ("432hz", "432hz"),
];

/// Variantes qui changent rarement l'enregistrement (la durée départage les autres cas).
const SOFT_VARIANTS: &[&str] = &["edit", "version", "mix", "extended"];

/// Mots sans importance dans un titre YouTube (« Official Video », « Remastered 2009 »…),
/// ignorés s'ils ne sont pas dans le titre cherché.
const NOISE_WORDS: &[&str] = &[
    "official", "officiel", "audio", "video", "clip", "lyrics", "lyric", "paroles", "visualizer", "visualiser", "hd",
    "hq", "4k", "mv", "explicit", "remastered", "remaster", "music", "musique", "original",
];

/// Mots en minuscules, sans accents ni ponctuation (fonctionne aussi en cyrillique, en
/// japonais…). Les titres épelés (« D.A.N.C.E. », « D>E>A>T>H ») sont recollés en un seul
/// mot, tant que le séparateur ne change pas.
fn words(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // Lettres seules en cours de recollage, et ce qui les sépare.
    let (mut letters, mut sep) = (String::new(), None::<String>);
    let flush = |letters: &mut String, out: &mut Vec<String>| {
        if letters.chars().count() >= 3 {
            out.push(std::mem::take(letters));
        } else {
            out.extend(letters.chars().map(String::from));
            letters.clear();
        }
    };
    let lower: String = s.to_lowercase().chars().map(unaccent).collect();
    let mut rest = lower.as_str();
    while !rest.is_empty() {
        let start = rest.find(char::is_alphanumeric).unwrap_or(rest.len());
        let gap = &rest[..start];
        rest = &rest[start..];
        let end = rest.find(|c: char| !c.is_alphanumeric()).unwrap_or(rest.len());
        let w = &rest[..end];
        rest = &rest[end..];
        if w.is_empty() {
            break;
        }
        if w.chars().count() == 1 {
            if !letters.is_empty() {
                match &sep {
                    None => sep = Some(gap.to_owned()),
                    Some(s) if s != gap => {
                        flush(&mut letters, &mut out);
                        sep = None;
                    }
                    Some(_) => {}
                }
            }
            letters.push_str(w);
        } else {
            flush(&mut letters, &mut out);
            sep = None;
            out.push(w.to_owned());
        }
    }
    flush(&mut letters, &mut out);
    out
}

/// Lettre latine sans son accent (« é » → « e »).
fn unaccent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        c => c,
    }
}

/// Retire les invités d'un titre : « (feat. X) », « [ft. X] », « (with X) », « feat. X - … ».
fn strip_featuring(s: &str) -> String {
    let s = s.to_lowercase();
    let mut out = String::new();
    let mut rest = s.as_str();
    'scan: loop {
        for (i, _) in rest.char_indices() {
            let before = rest[..i].trim_end();
            let opened = before.ends_with(['(', '[']);
            let boundary = i == 0 || rest[..i].ends_with([' ', '(', '[']);
            let after = &rest[i..];
            let marker = ["feat.", "feat ", "ft.", "ft ", "featuring "].iter().any(|m| after.starts_with(m))
                || (opened && after.starts_with("with "));
            if !(boundary && marker) {
                continue;
            }
            if opened {
                let close = after.find([')', ']']).map_or(rest.len(), |j| i + j + 1);
                out.push_str(&before[..before.len() - 1]);
                rest = &rest[close..];
            } else {
                let stop = [after.find(['(', '[', ')', ']']), after.find(" - ")].into_iter().flatten().min();
                out.push_str(&rest[..i]);
                rest = stop.map_or("", |j| &rest[i + j..]);
            }
            continue 'scan;
        }
        out.push_str(rest);
        return out;
    }
}

/// Part des mots de `wanted` présents dans `got` (0 à 1).
fn overlap(wanted: &[String], got: &[String]) -> f64 {
    if wanted.is_empty() {
        return 0.0;
    }
    wanted.iter().filter(|w| got.contains(w)).count() as f64 / wanted.len() as f64
}

/// Variantes (forme commune) présentes dans des mots.
fn variants(words: &[String]) -> Vec<&'static str> {
    let mut v: Vec<&str> = VARIANT_WORDS.iter().filter(|(w, _)| words.iter().any(|x| x == w)).map(|(_, c)| *c).collect();
    v.dedup();
    v
}

/// Le morceau cherché, déjà découpé en mots.
struct Wanted {
    title: Vec<String>,
    artist: Vec<String>,
    duration: Option<f64>,
}

impl Wanted {
    fn new(title: &str, artist: &str, duration: Option<f64>) -> Self {
        Self { title: words(&strip_featuring(title)), artist: words(artist), duration }
    }
}

/// Un résultat YouTube Music.
struct Candidate<'a> {
    title: &'a str,
    artists: &'a [&'a str],
    duration: Option<u32>,
    /// Morceau d'album (« song ») plutôt que vidéo.
    official: bool,
}

/// Ressemblance entre le morceau cherché et un résultat (au mieux 9,5).
fn match_score(want: &Wanted, c: &Candidate) -> f64 {
    let got_artists: Vec<Vec<String>> = c.artists.iter().map(|a| words(a)).collect();
    // Titre du résultat sans le bruit : « Official Video », année, noms d'artistes (les vidéos
    // sont souvent titrées « Artiste - Titre »)…
    let raw_title = words(&strip_featuring(c.title));
    let got_title: Vec<String> = raw_title
        .iter()
        .filter(|w| {
            want.title.contains(w)
                || !(NOISE_WORDS.contains(&w.as_str())
                    || (w.len() == 4 && w.chars().all(|c| c.is_ascii_digit()))
                    || want.artist.contains(w)
                    || got_artists.iter().flatten().any(|a| a == *w))
        })
        .cloned()
        .collect();

    // Titre : tous les mots cherchés (3 points), et rien de plus (1,5 point). Un autre titre
    // ne doit pas passer grâce à l'artiste et à la durée.
    let found = overlap(&want.title, &got_title);
    let mut score = found * 3.0 + overlap(&got_title, &want.title) * 1.5;
    if found < 0.5 {
        score -= 4.0;
    }

    // Variantes : jamais une que l'original n'a pas, toujours celles qu'il a.
    let (want_v, got_v) = (variants(&want.title), variants(&got_title));
    let weight = |v: &&str, hard: f64| if SOFT_VARIANTS.contains(v) { 1.0 } else { hard };
    score -= got_v.iter().filter(|v| !want_v.contains(v)).map(|v| weight(v, 3.0)).sum::<f64>();
    score -= want_v.iter().filter(|v| !got_v.contains(v)).map(|v| weight(v, 2.0)).sum::<f64>();

    // Artiste : le même nom, sinon une partie, sinon cité dans le titre (vidéo d'une autre
    // chaîne : moins sûr). Un autre artiste est presque toujours une reprise.
    let want_compact = want.artist.concat();
    score += if !want_compact.is_empty() && got_artists.iter().any(|a| a.concat() == want_compact) {
        2.5
    } else {
        let part = overlap(&want.artist, &got_artists.concat());
        if part > 0.0 {
            2.0 * part
        } else if !want.artist.is_empty() && want.artist.iter().all(|w| raw_title.contains(w)) {
            1.5
        } else {
            -3.0
        }
    };

    if let (Some(d), Some(cd)) = (want.duration, c.duration) {
        score += match (d - f64::from(cd)).abs() {
            g if g <= 2.0 => 2.0,
            g if g <= 5.0 => 1.5,
            g if g <= 10.0 => 0.5,
            g if g <= 20.0 => -0.5,
            g if g <= 40.0 => -2.0,
            _ => -4.0,
        };
    }
    if c.official {
        score += 0.5;
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

    #[test]
    fn words_joins_spelled_titles() {
        assert_eq!(words("D>E>A>T>H>M>E>T>A>L"), vec!["deathmetal"]);
        assert_eq!(words("D.A.N.C.E."), vec!["dance"]);
        assert_eq!(words("D>E>A>T>H>M>E>T>A>L_R>O>T"), vec!["deathmetal", "rot"]);
        // Deux lettres seules ne suffisent pas à faire un mot épelé.
        assert_eq!(words("A Day In The Life"), vec!["a", "day", "in", "the", "life"]);
        assert_eq!(words("Beyoncé - Déjà Vu"), vec!["beyonce", "deja", "vu"]);
    }

    #[test]
    fn strip_featuring_removes_guests() {
        assert_eq!(words(&strip_featuring("Get Lucky (feat. Pharrell Williams & Nile Rodgers)")), vec!["get", "lucky"]);
        assert_eq!(words(&strip_featuring("Dracula (with JENNIE)")), vec!["dracula"]);
        assert_eq!(words(&strip_featuring("Icon [ft. Nia La Vey] (Remix)")), vec!["icon", "remix"]);
        assert_eq!(words(&strip_featuring("Get Lucky feat. Pharrell Williams - Radio Edit")), vec!["get", "lucky", "radio", "edit"]);
        // « with » hors parenthèses fait partie du titre.
        assert_eq!(words(&strip_featuring("Dancing with Myself")), vec!["dancing", "with", "myself"]);
        assert_eq!(words(&strip_featuring("Left Behind")), vec!["left", "behind"]);
    }

    /// Note d'un résultat (morceau officiel sauf `video`).
    fn score(want: (&str, &str, f64), title: &str, artists: &[&str], duration: u32, video: bool) -> f64 {
        let wanted = Wanted::new(want.0, want.1, Some(want.2));
        match_score(&wanted, &Candidate { title, artists, duration: Some(duration), official: !video })
    }

    /// Le résultat le mieux noté parmi `(titre, artistes, durée, vidéo)`.
    fn pick<'a>(want: (&str, &str, f64), found: &[(&'a str, &[&str], u32, bool)]) -> &'a str {
        found.iter().map(|&(t, a, d, v)| (score(want, t, a, d, v), t)).max_by(|a, b| a.0.total_cmp(&b.0)).unwrap().1
    }

    #[test]
    fn exact_match_is_best_possible() {
        let s = score(("Bohemian Rhapsody", "Queen", 354.0), "Bohemian Rhapsody", &["Queen"], 355, false);
        assert_eq!(s, 9.5);
        assert!(s >= MATCH_GOOD_SCORE);
    }

    #[test]
    fn extra_words_lose_to_exact_title() {
        // Cas signalé : les deux avaient la même note, et le dernier l'emportait.
        let want = ("D>E>A>T>H>M>E>T>A>L", "Panchiko", 261.0);
        let found: &[(&str, &[&str], u32, bool)] = &[
            ("D>E>A>T>H>M>E>T>A>L_R>O>T", &["Panchiko"], 261, false),
            ("D>E>A>T>H>M>E>T>A>L", &["Panchiko"], 262, false),
            ("DEATHMETAL (Live)", &["Panchiko"], 261, false),
        ];
        assert_eq!(pick(want, found), "D>E>A>T>H>M>E>T>A>L");
    }

    #[test]
    fn never_adds_a_variant() {
        for extra in ["Live", "Remix", "Sped Up", "Slowed + Reverb", "Instrumental", "Acoustic", "Nightcore", "Karaoke Version", "Cover"] {
            let other = format!("Bohemian Rhapsody ({extra})");
            let want = ("Bohemian Rhapsody", "Queen", 354.0);
            let found: &[(&str, &[&str], u32, bool)] = &[(&other, &["Queen"], 354, false), ("Bohemian Rhapsody", &["Queen"], 360, false)];
            assert_eq!(pick(want, found), "Bohemian Rhapsody", "{extra}");
            assert!(score(want, &other, &["Queen"], 354, false) < MATCH_GOOD_SCORE, "{extra}");
        }
    }

    #[test]
    fn keeps_the_wanted_variant() {
        // Cas signalé : l'instrumentale ne doit pas être remplacée par la version chantée.
        let want = ("Love Potions Instrumental", "BJ Lips", 156.0);
        let found: &[(&str, &[&str], u32, bool)] = &[
            ("Love Potions", &["BJ Lips"], 156, false),
            ("Love Potions (Instrumental)", &["BJ Lips"], 157, false),
            ("Love Potions (Instrumental Slowed)", &["BJ Lips"], 160, false),
        ];
        assert_eq!(pick(want, found), "Love Potions (Instrumental)");
        let want = ("DEATHMETAL (Live)", "Panchiko", 261.0);
        let found: &[(&str, &[&str], u32, bool)] = &[("D>E>A>T>H>M>E>T>A>L", &["Panchiko"], 262, false), ("DEATHMETAL (Live)", &["Panchiko"], 261, false)];
        assert_eq!(pick(want, found), "DEATHMETAL (Live)");
    }

    #[test]
    fn radio_edit_is_a_soft_variant() {
        // Sur YouTube Music, l'enregistrement habituel s'appelle parfois « Radio Edit ».
        let s = score(("Alors on danse", "Stromae", 206.0), "Alors on danse (Radio Edit)", &["Stromae"], 208, false);
        assert!(s >= MATCH_GOOD_SCORE, "{s}");
    }

    #[test]
    fn other_artist_is_a_cover() {
        let want = ("Love Potions Instrumental", "BJ Lips", 156.0);
        // Même titre, autre artiste : en dessous du minimum.
        assert!(score(want, "Love Potions (Instrumental)", &["Ren"], 156, false) < MATCH_MIN_SCORE);
        // Une vidéo d'une autre chaîne qui cite l'artiste dans son titre convient.
        let video = score(want, "(OFFICIAL INSTRUMENTAL) BJ Lips - Love Potions", &["luca.v"], 160, true);
        assert!(video >= MATCH_MIN_SCORE, "{video}");
        // … mais moins que le morceau officiel de l'artiste.
        assert!(video < score(want, "Love Potions (Instrumental)", &["BJ Lips"], 160, false));
    }

    #[test]
    fn artist_names_are_compared_loosely() {
        let want = ("Мало тебя", "Serebro", 227.0);
        assert_eq!(score(want, "Мало тебя", &["SEREBRO"], 226, false), 9.5);
        let want = ("Dai Dai", "Shakira", 223.0);
        assert_eq!(score(want, "Dai Dai", &["Shakira", "Burna Boy"], 224, false), 9.5);
        let want = ("Self Aware", "TemperCity", 181.0);
        assert_eq!(score(want, "Self Aware", &["Temper City"], 181, false), 9.5);
    }

    #[test]
    fn duration_separates_versions() {
        let want = ("Get Lucky", "Daft Punk", 369.0);
        let found: &[(&str, &[&str], u32, bool)] = &[("Get Lucky", &["Daft Punk"], 249, false), ("Get Lucky", &["Daft Punk"], 370, false)];
        let scores: Vec<f64> = found.iter().map(|&(t, a, d, v)| score(want, t, a, d, v)).collect();
        assert!(scores[1] - scores[0] >= 5.0, "{scores:?}");
    }

    #[test]
    fn prefers_official_songs_to_videos() {
        let want = ("Blinding Lights", "The Weeknd", 200.0);
        let song = score(want, "Blinding Lights", &["The Weeknd"], 200, false);
        let video = score(want, "Blinding Lights (Official Video)", &["The Weeknd"], 200, true);
        let lyrics = score(want, "The Weeknd - Blinding Lights (Lyrics)", &["7clouds"], 200, true);
        assert!(song > video && video > lyrics, "{song} {video} {lyrics}");
    }

    #[test]
    fn another_title_never_passes() {
        // Même artiste, même durée, autre morceau.
        assert!(score(("Мало тебя", "Serebro", 227.0), "Иум", &["SEREBRO"], 227, false) < MATCH_MIN_SCORE);
    }

    /// Affiche les candidats YouTube Music et leur note pour des morceaux Deezer signalés :
    /// `cargo test -p yato-core real_matches -- --ignored --nocapture`.
    /// Morceaux Deezer signalés comme mal reliés, contre le vrai YouTube Music :
    /// `cargo test -p yato-core real_matches -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn real_matches() {
        let dir = std::env::temp_dir().join("yato-match-test");
        let ytdlp = crate::ytdlp::YtDlp::new(dir.join("yt-dlp"), reqwest::Client::new());
        let yt = YouTubeMusicSource::new(dir.join("yt"), ytdlp).unwrap();
        // (titre, artiste, durée Deezer, titres YouTube acceptés ; aucun = rien ne doit être trouvé).
        let cases: &[(&str, &str, f64, &[&str])] = &[
            ("D>E>A>T>H>M>E>T>A>L", "Panchiko", 261.0, &["D>E>A>T>H>M>E>T>A>L"]),
            ("DEATHMETAL (Live)", "Panchiko", 261.0, &["DEATHMETAL (Live)"]),
            // Pas de morceau officiel : seulement des vidéos de l'instrumentale.
            ("Love Potions Instrumental", "BJ Lips", 156.0, &["BJ Lips - Love Potions (instrumental)", "(OFFICIAL INSTRUMENTAL) BJ Lips - Love Potions"]),
            // « BJLHDD » n'existe pas sur YouTube : mieux vaut rien que la version d'un autre.
            ("Love Potions (Instrumental)", "BJLHDD", 156.0, &[]),
            ("Get Lucky (feat. Pharrell Williams & Nile Rodgers)", "Daft Punk", 369.0, &["Get Lucky (feat. Pharrell Williams and Nile Rodgers)"]),
            ("Bohemian Rhapsody", "Queen", 354.0, &["Bohemian Rhapsody"]),
            ("Alors on danse", "Stromae", 206.0, &["Alors on danse (Radio Edit)"]),
            ("Мало тебя", "Serebro", 227.0, &["Мало тебя"]),
            ("Hips Don't Lie (feat. Wyclef Jean)", "Shakira", 218.0, &["Hips Don't Lie (feat. Wyclef Jean)"]),
            ("Smells Like Teen Spirit", "Nirvana", 301.0, &["Smells Like Teen Spirit"]),
            ("Blinding Lights", "The Weeknd", 200.0, &["Blinding Lights"]),
            ("Dernière danse", "Indila", 211.0, &["Dernière danse"]),
        ];
        let mut failed = Vec::new();
        for &(title, artist, d, expected) in cases {
            let wanted = Wanted::new(title, artist, Some(d));
            let query = format!("{artist} {title}");
            println!("== {artist} - {title} ({d}s)");
            let songs = yt.rp.query().music_search_tracks(&query).await.unwrap().items.items;
            let videos = yt.rp.query().music_search_videos(&query).await.unwrap().items.items;
            for c in songs.iter().take(10).chain(videos.iter().take(10)) {
                let artists: Vec<&str> = c.artists.iter().map(|a| a.name.as_str()).collect();
                let found = Candidate { title: &c.name, artists: &artists, duration: c.duration, official: c.track_type.is_track() };
                println!("  {:6.2}  {} - {} ({:?}s, {:?})", match_score(&wanted, &found), artists.join(", "), c.name, c.duration, c.track_type);
            }
            let got = yt.find_track(title, artist, Some(d)).await.unwrap();
            println!("  → {:?}", got.as_ref().map(|t| (&t.artist, &t.title, t.duration)));
            if !got.as_ref().map_or(expected.is_empty(), |t| expected.contains(&t.title.as_str())) {
                failed.push(format!("{artist} - {title}"));
            }
        }
        assert!(failed.is_empty(), "mal reliés : {failed:?}");
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
