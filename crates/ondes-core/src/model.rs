use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Un morceau, quelle que soit sa source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Identifiant unique, de la forme `"<source>:<id>"`.
    pub id: String,
    /// Identifiant de la source qui sait jouer ce morceau (`"local"`, `"youtube"`…).
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
    /// Chaque artiste du morceau, séparément (principal, invités…). `artist` reste le texte
    /// affiché d'un bloc ; vide pour les morceaux gardés en cache avant l'ajout de ce champ.
    #[serde(default)]
    pub artists: Vec<ArtistRef>,
}

/// Un artiste cité par un morceau. `id` : identifiant Deezer quand on le connaît ; sinon, la
/// fiche de l'artiste est retrouvée par son nom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistRef {
    pub name: String,
    #[serde(default)]
    pub id: Option<String>,
}

impl ArtistRef {
    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into(), id: None }
    }
}

/// Sépare « A, B & C feat. D » en `["A", "B", "C", "D"]`.
pub fn split_artists(s: &str) -> Vec<String> {
    const SEPARATORS: &[&str] = &[", ", " & ", " feat. ", " feat ", " ft. ", " featuring ", " x ", " X ", " et ", " and ", " with ", " vs. ", " vs "];
    let mut parts = vec![s.to_owned()];
    for sep in SEPARATORS {
        parts = parts.iter().flat_map(|p| p.split(sep).map(str::to_owned).collect::<Vec<_>>()).collect();
    }
    let mut out: Vec<String> = Vec::new();
    for p in parts.into_iter().map(|p| p.trim().to_owned()).filter(|p| !p.is_empty()) {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&p)) {
            out.push(p);
        }
    }
    out
}

/// Artistes invités cités dans un titre : « Cuenta (feat. Meryl & Shay) » → `["Meryl", "Shay"]`.
pub fn guests_in_title(title: &str) -> Vec<String> {
    let lower = title.to_lowercase();
    for marker in ["(feat. ", "[feat. ", "(feat ", "(ft. ", "(featuring ", "(with ", " feat. ", " ft. "] {
        if let Some(i) = lower.find(marker) {
            // `lower` peut différer en longueur pour certains caractères : on ne coupe `title`
            // que si l'indice tombe sur une frontière de caractère.
            let start = i + marker.len();
            let Some(rest) = title.get(start..) else { return Vec::new() };
            let end = rest.find([')', ']']).unwrap_or(rest.len());
            return split_artists(&rest[..end]);
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_artist_lists() {
        assert_eq!(split_artists("Snoop Dogg, David Guetta"), ["Snoop Dogg", "David Guetta"]);
        assert_eq!(split_artists("Videoclub & Adèle Castillon feat. Mattyeux"), ["Videoclub", "Adèle Castillon", "Mattyeux"]);
        assert_eq!(split_artists("Serebro"), ["Serebro"]);
    }

    #[test]
    fn finds_guests_in_titles() {
        assert_eq!(guests_in_title("Cuenta (feat. Meryl & Shay)"), ["Meryl", "Shay"]);
        assert_eq!(guests_in_title("Dracula (with JENNIE)"), ["JENNIE"]);
        assert_eq!(guests_in_title("Get Lucky (Radio Edit - feat. Pharrell Williams and Nile Rodgers)"), ["Pharrell Williams", "Nile Rodgers"]);
        assert!(guests_in_title("Мало тебя").is_empty());
    }
}

/// Où se trouve le son.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Location {
    File { path: PathBuf },
    Remote { remote_id: String },
}
