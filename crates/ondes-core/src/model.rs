use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Un morceau, quelle que soit sa source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Identifiant unique, de la forme `"<source>:<id>"`.
    pub id: String,
    /// Identifiant de la source qui sait jouer ce morceau (`"local"`, `"audius"`…).
    pub source: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    /// Durée en secondes, si connue.
    pub duration: Option<f64>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    /// URL d'une pochette distante.
    pub artwork: Option<String>,
    /// Pour un fichier local : une pochette est intégrée au fichier ou présente dans le dossier.
    #[serde(default)]
    pub has_cover: bool,
    pub location: Location,
}

/// Où se trouve le son.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Location {
    File { path: PathBuf },
    Remote { remote_id: String },
}
