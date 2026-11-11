use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use serde::Serialize;

use crate::{Error, Result, Track};

/// Ce qu'une source renvoie pour qu'on puisse jouer un morceau.
#[derive(Debug, Clone)]
pub enum StreamData {
    File(PathBuf),
    /// `ranged_len` : taille connue, à télécharger par plages (obligatoire pour YouTube).
    /// `hint` : extension du format ('m4a', 'mp3'…) pour aider le décodeur.
    /// `user_agent` : certains serveurs exigent celui du client qui a obtenu l'URL.
    Http { url: String, ranged_len: Option<u64>, hint: Option<&'static str>, user_agent: Option<String> },
    /// Téléchargement déjà lancé par la source (ex. anticipé), lu depuis la mémoire.
    Buffered { download: crate::stream::Download, hint: Option<&'static str> },
}

/// Une source de musique : bibliothèque locale, service en ligne, etc.
///
/// Ajouter une source = implémenter ce trait et l'enregistrer dans le [`SourceRegistry`].
#[async_trait]
pub trait MusicSource: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>>;

    /// Morceaux mis en avant (tendances…). Vide par défaut.
    async fn featured(&self, _limit: usize) -> Result<Vec<Track>> {
        Ok(Vec::new())
    }

    /// Prépare un morceau à l'avance (ex. résoudre son URL) pour qu'il démarre sans attendre.
    /// Ne fait rien par défaut.
    async fn prefetch(&self, _track: &Track) {}

    /// Préparation légère d'un morceau affiché (ex. résoudre son adresse, sans rien
    /// télécharger), pour qu'un clic démarre vite. Ne fait rien par défaut.
    async fn warm(&self, _track: &Track) {}

    /// Donne l'emplacement réel du son.
    async fn resolve(&self, track: &Track) -> Result<StreamData>;
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceInfo {
    pub id: &'static str,
    pub name: &'static str,
}

#[derive(Default)]
pub struct SourceRegistry {
    sources: Vec<Arc<dyn MusicSource>>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, source: Arc<dyn MusicSource>) {
        self.sources.push(source);
    }

    pub fn list(&self) -> Vec<SourceInfo> {
        self.sources
            .iter()
            .map(|s| SourceInfo { id: s.id(), name: s.name() })
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<Arc<dyn MusicSource>> {
        self.sources
            .iter()
            .find(|s| s.id() == id)
            .cloned()
            .ok_or_else(|| Error::UnknownSource(id.to_owned()))
    }

    /// Cherche dans toutes les sources en parallèle. Une source en panne ne bloque pas les autres.
    pub async fn search_all(&self, query: &str, limit: usize) -> Vec<Track> {
        let mut set = tokio::task::JoinSet::new();
        for (i, source) in self.sources.iter().cloned().enumerate() {
            let q = query.to_owned();
            set.spawn(async move { (i, source.search(&q, limit).await) });
        }
        let mut results: Vec<(usize, Vec<Track>)> = Vec::new();
        while let Some(joined) = set.join_next().await {
            if let Ok((i, Ok(tracks))) = joined {
                results.push((i, tracks));
            }
        }
        // Ordre stable : celui d'enregistrement des sources.
        results.sort_by_key(|(i, _)| *i);
        results.into_iter().flat_map(|(_, t)| t).collect()
    }
}
