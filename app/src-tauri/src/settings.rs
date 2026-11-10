//! Réglages de l'app et écoute en cours, gardés dans de petits fichiers JSON.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Ce que l'utilisateur règle dans la page Réglages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Fondu enchaîné entre deux morceaux, en secondes (0 : aucun).
    pub crossfade: f64,
    /// Reprendre l'écoute (file, morceau, position) au lancement.
    pub resume: bool,
    /// Afficher « Écoute … » dans le statut Discord.
    pub discord: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { crossfade: 0.0, resume: true, discord: true }
    }
}

/// Lit un fichier JSON ; valeur par défaut s'il n'existe pas ou est illisible.
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Écrit un fichier JSON d'un coup (fichier temporaire puis renommage : jamais à moitié écrit).
pub fn save<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec(value).map_err(std::io::Error::other)?)?;
    fs::rename(tmp, path)
}
