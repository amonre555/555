use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("erreur fichier : {0}")]
    Io(#[from] std::io::Error),
    #[error("erreur réseau : {0}")]
    Http(#[from] reqwest::Error),
    #[error("impossible de lire ce morceau : {0}")]
    Decode(String),
    #[error("sortie audio indisponible : {0}")]
    Audio(String),
    #[error("source inconnue : {0}")]
    UnknownSource(String),
    #[error("{0}")]
    Other(String),
}

/// Les erreurs arrivent dans l'interface sous forme de simple texte.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
