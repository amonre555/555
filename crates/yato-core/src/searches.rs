//! Recherches récentes : les dernières recherches lancées, la plus récente en tête, sans
//! doublon, gardées dans un petit fichier JSON.

use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use crate::{Result, collection::save_json};

/// Nombre de recherches gardées.
pub const RECENT_MAX: usize = 10;

pub struct RecentSearches {
    path: PathBuf,
    list: Mutex<Vec<String>>,
}

/// Espaces superflus retirés : « daft   punk » et « daft punk » sont la même recherche.
fn clean(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

impl RecentSearches {
    /// Charge la liste (vide si le fichier n'existe pas ou est illisible).
    pub fn load(path: &Path) -> Self {
        let list: Vec<String> = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path: path.to_owned(), list: Mutex::new(list) }
    }

    pub fn get(&self) -> Vec<String> {
        self.list.lock().clone()
    }

    fn update(&self, f: impl FnOnce(&mut Vec<String>)) -> Result<Vec<String>> {
        let mut list = self.list.lock();
        let mut next = list.clone();
        f(&mut next);
        if next != *list {
            save_json(&self.path, &next)?;
            *list = next;
        }
        Ok(list.clone())
    }

    /// Note une recherche, en tête ; une recherche identique (casse comprise) remonte.
    pub fn add(&self, query: &str) -> Result<Vec<String>> {
        let q = clean(query);
        self.update(|list| {
            if q.is_empty() {
                return;
            }
            list.retain(|x| !same(x, &q));
            list.insert(0, q);
            list.truncate(RECENT_MAX);
        })
    }

    pub fn remove(&self, query: &str) -> Result<Vec<String>> {
        let q = clean(query);
        self.update(|list| list.retain(|x| !same(x, &q)))
    }

    pub fn clear(&self) -> Result<Vec<String>> {
        self.update(Vec::clear)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_ten_without_duplicates() {
        let dir = std::env::temp_dir().join(format!("yato-searches-{}", std::process::id()));
        let path = dir.join("searches.json");
        let s = RecentSearches::load(&path);
        for i in 0..12 {
            s.add(&format!("recherche {i}")).unwrap();
        }
        s.add("  ").unwrap();
        let list = s.add("  Recherche   5 ").unwrap();
        assert_eq!(list.len(), RECENT_MAX);
        assert_eq!(list[0], "Recherche 5");
        assert_eq!(list[1], "recherche 11");
        assert_eq!(list.iter().filter(|x| x.to_lowercase() == "recherche 5").count(), 1);
        assert!(!list.contains(&"recherche 1".to_owned()));

        let list = s.remove("recherche 11").unwrap();
        assert_eq!(list.len(), RECENT_MAX - 1);
        assert_eq!(RecentSearches::load(&path).get(), list);
        assert!(s.clear().unwrap().is_empty());
        assert!(RecentSearches::load(&path).get().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
