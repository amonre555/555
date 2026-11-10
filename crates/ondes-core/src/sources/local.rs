//! Bibliothèque de fichiers locaux.
//!
//! - Scan en parallèle (rayon) : les tags sont lus sur tous les cœurs du processeur.
//! - Cache JSON sur disque : au démarrage, la bibliothèque s'affiche sans rescanner.
//! - Recherche en mémoire sur un texte pré-calculé en minuscules : instantanée même sur 50 000 morceaux.

use std::{
    fs,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use lofty::{picture::PictureType, prelude::*};
use parking_lot::RwLock;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::{ArtistRef, Location, MusicSource, Result, StreamData, Track};

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "m4a", "mp4", "aac", "wav", "aif", "aiff", "caf", "mka", "webm",
];
const FOLDER_COVERS: &[&str] = &[
    "cover.jpg", "cover.png", "folder.jpg", "folder.png", "front.jpg", "front.png", "album.jpg",
];
const CACHE_VERSION: u32 = 1;

#[derive(Default)]
pub struct LocalLibrary {
    inner: RwLock<Inner>,
}

#[derive(Default)]
struct Inner {
    folders: Vec<PathBuf>,
    tracks: Vec<Track>,
    /// `haystacks[i]` = titre + artiste + album de `tracks[i]`, en minuscules.
    haystacks: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    folders: Vec<PathBuf>,
    tracks: Vec<Track>,
}

impl LocalLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Charge le cache. Un cache absent ou illisible donne une bibliothèque vide.
    pub fn load(path: &Path) -> Self {
        let lib = Self::new();
        if let Ok(bytes) = fs::read(path)
            && let Ok(cache) = serde_json::from_slice::<Cache>(&bytes)
            && cache.version == CACHE_VERSION
        {
            lib.replace(cache.folders, cache.tracks);
        }
        lib
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let inner = self.inner.read();
        let cache = Cache {
            version: CACHE_VERSION,
            folders: inner.folders.clone(),
            tracks: inner.tracks.clone(),
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        // Écriture atomique : on ne corrompt jamais le cache si l'app est coupée en plein milieu.
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(&cache).map_err(|e| crate::Error::Other(e.to_string()))?)?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn folders(&self) -> Vec<PathBuf> {
        self.inner.read().folders.clone()
    }

    pub fn tracks(&self) -> Vec<Track> {
        self.inner.read().tracks.clone()
    }

    pub fn len(&self) -> usize {
        self.inner.read().tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Ajoute un dossier puis rescanne tout. Bloquant : à lancer hors du thread de l'interface.
    pub fn add_folder(&self, folder: PathBuf) -> usize {
        let mut folders = self.folders();
        if !folders.contains(&folder) {
            folders.push(folder);
        }
        self.scan(folders)
    }

    pub fn remove_folder(&self, folder: &Path) -> usize {
        let folders = self.folders().into_iter().filter(|f| f != folder).collect();
        self.scan(folders)
    }

    pub fn rescan(&self) -> usize {
        self.scan(self.folders())
    }

    fn scan(&self, folders: Vec<PathBuf>) -> usize {
        let tracks = scan_folders(&folders);
        let n = tracks.len();
        self.replace(folders, tracks);
        n
    }

    fn replace(&self, folders: Vec<PathBuf>, tracks: Vec<Track>) {
        let haystacks = tracks.iter().map(haystack).collect();
        *self.inner.write() = Inner { folders, tracks, haystacks };
    }

    /// Tous les mots de la recherche doivent apparaître (dans n'importe quel ordre).
    pub fn find(&self, query: &str, limit: usize) -> Vec<Track> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let inner = self.inner.read();
        if words.is_empty() {
            return inner.tracks.iter().take(limit).cloned().collect();
        }
        inner
            .haystacks
            .iter()
            .zip(&inner.tracks)
            .filter(|(h, _)| words.iter().all(|w| h.contains(w.as_str())))
            .map(|(_, t)| t.clone())
            .take(limit)
            .collect()
    }
}

#[async_trait]
impl MusicSource for LocalLibrary {
    fn id(&self) -> &'static str {
        "local"
    }

    fn name(&self) -> &'static str {
        "Ma bibliothèque"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        Ok(self.find(query, limit))
    }

    async fn resolve(&self, track: &Track) -> Result<StreamData> {
        match &track.location {
            Location::File { path } => Ok(StreamData::File(path.clone())),
            Location::Remote { .. } => Err(crate::Error::Other("ce morceau n'est pas local".into())),
        }
    }
}

fn haystack(t: &Track) -> String {
    let mut s = String::with_capacity(t.title.len() + t.artist.len() + 32);
    s.push_str(&t.title);
    s.push(' ');
    s.push_str(&t.artist);
    if let Some(album) = &t.album {
        s.push(' ');
        s.push_str(album);
    }
    s.to_lowercase()
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

pub fn scan_folders(folders: &[PathBuf]) -> Vec<Track> {
    // 1. Lister les fichiers (rapide, limité par le disque).
    let paths: Vec<PathBuf> = folders
        .iter()
        .flat_map(|f| WalkDir::new(f).follow_links(true).into_iter().filter_map(|e| e.ok()))
        .filter(|e| e.file_type().is_file() && is_audio(e.path()))
        .map(|e| e.into_path())
        .collect();

    // 2. Lire les tags en parallèle (le vrai coût).
    let mut tracks: Vec<Track> = paths.par_iter().filter_map(|p| read_track(p)).collect();

    // 3. Trier : artiste, album, disque, numéro de piste, titre.
    tracks.par_sort_by_cached_key(|t| {
        (
            t.artist.to_lowercase(),
            t.album.as_deref().unwrap_or("").to_lowercase(),
            t.disc_number.unwrap_or(0),
            t.track_number.unwrap_or(0),
            t.title.to_lowercase(),
        )
    });
    tracks.dedup_by(|a, b| a.id == b.id);
    tracks
}

fn read_track(path: &Path) -> Option<Track> {
    let file = lofty::read_from_path(path).ok()?;
    let tag = file.primary_tag().or_else(|| file.first_tag());

    let clean = |s: std::borrow::Cow<'_, str>| {
        let s = s.trim();
        (!s.is_empty()).then(|| s.to_owned())
    };
    let title = tag
        .and_then(|t| t.title())
        .and_then(clean)
        .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))?;
    let artist = tag
        .and_then(|t| t.artist())
        .and_then(clean)
        .unwrap_or_else(|| "Artiste inconnu".to_owned());
    let album = tag.and_then(|t| t.album()).and_then(clean);
    let duration = file.properties().duration().as_secs_f64();
    let has_cover = tag.is_some_and(|t| !t.pictures().is_empty()) || folder_cover(path).is_some();
    let artists = crate::model::split_artists(&artist).into_iter().map(ArtistRef::named).collect();

    Some(Track {
        id: format!("local:{}", path.display()),
        source: "local".to_owned(),
        title,
        artist,
        album,
        duration: (duration > 0.0).then_some(duration),
        track_number: tag.and_then(|t| t.track()),
        disc_number: tag.and_then(|t| t.disk()),
        artwork: None,
        has_cover,
        location: Location::File { path: path.to_owned() },
        artists,
    })
}

fn folder_cover(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    FOLDER_COVERS.iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

/// Pochette d'un fichier local : celle intégrée au fichier, sinon une image du dossier.
/// Renvoie `(octets, type MIME)`.
pub fn read_cover(path: &Path) -> Option<(Vec<u8>, String)> {
    if let Ok(file) = lofty::read_from_path(path)
        && let Some(tag) = file.primary_tag().or_else(|| file.first_tag())
    {
        let pics = tag.pictures();
        let pic = pics
            .iter()
            .find(|p| p.pic_type() == PictureType::CoverFront)
            .or_else(|| pics.first());
        if let Some(pic) = pic {
            let mime = pic.mime_type().map(|m| m.as_str().to_owned()).unwrap_or_else(|| "image/jpeg".into());
            return Some((pic.data().to_vec(), mime));
        }
    }
    let cover = folder_cover(path)?;
    let mime = if cover.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        "image/png"
    } else {
        "image/jpeg"
    };
    Some((fs::read(cover).ok()?, mime.to_owned()))
}
