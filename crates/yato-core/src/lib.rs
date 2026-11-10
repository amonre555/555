//! Cœur de yato : tout ce qui ne dépend pas de l'interface.
//!
//! - [`model`]   : les types partagés (morceau, emplacement du son)
//! - [`source`]  : le trait [`source::MusicSource`] que chaque source de musique implémente
//! - [`sources`] : les sources fournies (bibliothèque locale, Deezer pour les infos, YouTube Music pour le son)
//! - [`stream`]  : lecteur HTTP progressif (la lecture démarre avant la fin du téléchargement)
//! - [`player`]  : moteur de lecture + file d'attente
//!
//! L'interface (Tauri aujourd'hui, autre chose demain) ne fait qu'appeler ce crate.

pub mod error;
pub mod lyrics;
pub mod model;
pub mod player;
pub mod source;
pub mod sources;
pub mod stream;
pub mod ytdlp;

pub use error::{Error, Result};
pub use model::{ArtistRef, Location, Track};
pub use player::{Controller, Snapshot, Status};
pub use source::{MusicSource, SourceInfo, SourceRegistry, StreamData};

/// Client HTTP partagé par toutes les sources (connexions réutilisées = requêtes plus rapides).
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("yato/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .build()
        .expect("client HTTP")
}
