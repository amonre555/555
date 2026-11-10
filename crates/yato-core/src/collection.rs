//! Ta collection : titres likés et playlists, gardés sur disque dans un petit fichier JSON.
//!
//! Les morceaux sont enregistrés en entier (titre, pochette, emplacement du son…) : une playlist
//! se lit sans refaire de recherche, et mélange librement bibliothèque locale, Deezer et YouTube.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::{Error, Result, Track};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tracks: Vec<Track>,
    /// Création, en millisecondes depuis 1970.
    #[serde(default)]
    pub created: u64,
}

/// Tout ce que l'interface affiche : likes (le plus récent en premier) et playlists.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionData {
    #[serde(default)]
    pub liked: Vec<Track>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}

pub struct Collection {
    path: PathBuf,
    data: Mutex<CollectionData>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Identifiant unique, même pour deux playlists créées dans la même milliseconde.
fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("pl-{:x}-{:x}", now_ms(), COUNTER.fetch_add(1, Ordering::Relaxed))
}

impl Collection {
    /// Charge la collection (vide si le fichier n'existe pas ou est illisible).
    pub fn load(path: &Path) -> Self {
        let data = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { path: path.to_owned(), data: Mutex::new(data) }
    }

    pub fn get(&self) -> CollectionData {
        self.data.lock().clone()
    }

    /// Applique une modification, enregistre (écriture atomique) et renvoie l'état à jour.
    fn update<T>(&self, f: impl FnOnce(&mut CollectionData) -> Result<T>) -> Result<CollectionData> {
        let mut data = self.data.lock();
        let mut next = data.clone();
        f(&mut next)?;
        save(&self.path, &next)?;
        *data = next;
        Ok(data.clone())
    }

    /// Like ou retire le like d'un morceau.
    pub fn toggle_like(&self, track: Track) -> Result<CollectionData> {
        self.update(|d| {
            if let Some(i) = d.liked.iter().position(|t| t.id == track.id) {
                d.liked.remove(i);
            } else {
                d.liked.insert(0, track);
            }
            Ok(())
        })
    }

    /// Nouvelle playlist, éventuellement déjà remplie. Son identifiant est celui de la dernière.
    pub fn create_playlist(&self, name: String, tracks: Vec<Track>) -> Result<CollectionData> {
        self.update(|d| {
            let name = clean_name(&name, d.playlists.len());
            let mut p = Playlist { id: new_id(), name, tracks: Vec::new(), created: now_ms() };
            push_unique(&mut p.tracks, tracks);
            d.playlists.push(p);
            Ok(())
        })
    }

    pub fn rename_playlist(&self, id: &str, name: String) -> Result<CollectionData> {
        self.update(|d| {
            let i = index_of(d, id)?;
            d.playlists[i].name = clean_name(&name, i);
            Ok(())
        })
    }

    pub fn delete_playlist(&self, id: &str) -> Result<CollectionData> {
        self.update(|d| {
            let i = index_of(d, id)?;
            d.playlists.remove(i);
            Ok(())
        })
    }

    /// Ajoute des morceaux à la fin (ceux déjà présents sont ignorés).
    pub fn add_to_playlist(&self, id: &str, tracks: Vec<Track>) -> Result<CollectionData> {
        self.update(|d| {
            let i = index_of(d, id)?;
            push_unique(&mut d.playlists[i].tracks, tracks);
            Ok(())
        })
    }

    pub fn remove_from_playlist(&self, id: &str, track_id: &str) -> Result<CollectionData> {
        self.update(|d| {
            let i = index_of(d, id)?;
            d.playlists[i].tracks.retain(|t| t.id != track_id);
            Ok(())
        })
    }

    /// Déplace un morceau dans une playlist (glisser-déposer).
    pub fn move_in_playlist(&self, id: &str, from: usize, to: usize) -> Result<CollectionData> {
        self.update(|d| {
            let i = index_of(d, id)?;
            let tracks = &mut d.playlists[i].tracks;
            if from >= tracks.len() {
                return Ok(());
            }
            let t = tracks.remove(from);
            tracks.insert(to.min(tracks.len()), t);
            Ok(())
        })
    }
}

fn index_of(d: &CollectionData, id: &str) -> Result<usize> {
    d.playlists.iter().position(|p| p.id == id).ok_or_else(|| Error::Other("cette playlist n'existe plus".into()))
}

fn clean_name(name: &str, n: usize) -> String {
    let name = name.trim();
    if name.is_empty() { format!("Ma playlist n° {}", n + 1) } else { name.to_owned() }
}

fn push_unique(list: &mut Vec<Track>, tracks: Vec<Track>) {
    for t in tracks {
        if !list.iter().any(|x| x.id == t.id) {
            list.push(t);
        }
    }
}

fn save(path: &Path, data: &CollectionData) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec(data).map_err(|e| Error::Other(e.to_string()))?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Location;

    fn track(id: &str) -> Track {
        Track {
            id: id.into(), source: "deezer".into(), title: id.into(), artist: "A".into(), album: None, album_id: None,
            duration: None, track_number: None, disc_number: None, artwork: None, has_cover: false,
            location: Location::Remote { remote_id: id.into() }, artists: Vec::new(),
        }
    }

    #[test]
    fn likes_and_playlists_survive_a_reload() {
        let dir = std::env::temp_dir().join(format!("yato-collection-{}", new_id()));
        let path = dir.join("collection.json");
        let c = Collection::load(&path);
        c.toggle_like(track("a")).unwrap();
        c.toggle_like(track("b")).unwrap();
        let d = c.toggle_like(track("a")).unwrap();
        assert_eq!(d.liked.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["b"]);

        let d = c.create_playlist("  ".into(), vec![track("x"), track("x"), track("y")]).unwrap();
        let id = d.playlists[0].id.clone();
        assert_eq!(d.playlists[0].name, "Ma playlist n° 1");
        assert_eq!(d.playlists[0].tracks.len(), 2);
        c.add_to_playlist(&id, vec![track("y"), track("z")]).unwrap();
        c.move_in_playlist(&id, 2, 0).unwrap();
        c.remove_from_playlist(&id, "x").unwrap();
        c.rename_playlist(&id, "Été".into()).unwrap();

        let d = Collection::load(&path).get();
        assert_eq!(d.liked.len(), 1);
        assert_eq!(d.playlists[0].name, "Été");
        assert_eq!(d.playlists[0].tracks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["z", "y"]);
        c.delete_playlist(&id).unwrap();
        assert!(Collection::load(&path).get().playlists.is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}
