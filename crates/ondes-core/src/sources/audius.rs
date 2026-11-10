//! Audius : plateforme de musique ouverte, avec une API publique officielle et gratuite.
//! Les artistes y publient eux-mêmes leurs morceaux : c'est une source 100 % légale.

use async_trait::async_trait;
use serde::Deserialize;

use crate::{Error, Location, MusicSource, Result, StreamData, Track};

const API: &str = "https://api.audius.co/v1";
const APP_NAME: &str = "ondes";

pub struct AudiusSource {
    client: reqwest::Client,
}

impl AudiusSource {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    async fn fetch(&self, path: &str, params: &[(&str, &str)]) -> Result<Vec<Track>> {
        let resp: Resp = self
            .client
            .get(format!("{API}{path}"))
            .query(&[("app_name", APP_NAME)])
            .query(params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp
            .data
            .into_iter()
            // Certains morceaux sont réservés (achat, abonnés) : on ne garde que ceux jouables.
            .filter(|t| t.is_streamable.unwrap_or(true))
            .map(Into::into)
            .collect())
    }
}

#[async_trait]
impl MusicSource for AudiusSource {
    fn id(&self) -> &'static str {
        "audius"
    }

    fn name(&self) -> &'static str {
        "Audius"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit.min(50).to_string();
        self.fetch("/tracks/search", &[("query", query), ("limit", &limit)]).await
    }

    async fn featured(&self, limit: usize) -> Result<Vec<Track>> {
        let limit = limit.min(100).to_string();
        self.fetch("/tracks/trending", &[("limit", &limit)]).await
    }

    async fn resolve(&self, track: &Track) -> Result<StreamData> {
        let Location::Remote { remote_id } = &track.location else {
            return Err(Error::Other("ce morceau ne vient pas d'Audius".into()));
        };
        Ok(StreamData::Http {
            url: format!("{API}/tracks/{remote_id}/stream?app_name={APP_NAME}"),
            ranged_len: None,
            hint: Some("mp3"),
            user_agent: None,
        })
    }
}

#[derive(Deserialize)]
struct Resp {
    data: Vec<AudiusTrack>,
}

#[derive(Deserialize)]
struct AudiusTrack {
    id: String,
    title: String,
    duration: Option<f64>,
    artwork: Option<serde_json::Value>,
    user: AudiusUser,
    is_streamable: Option<bool>,
}

#[derive(Deserialize)]
struct AudiusUser {
    name: String,
}

impl From<AudiusTrack> for Track {
    fn from(t: AudiusTrack) -> Self {
        let artwork = t.artwork.as_ref().and_then(|a| {
            ["480x480", "1000x1000", "150x150"]
                .iter()
                .find_map(|k| a.get(k).and_then(|v| v.as_str()).map(str::to_owned))
        });
        Track {
            id: format!("audius:{}", t.id),
            source: "audius".into(),
            title: t.title,
            artist: t.user.name,
            album: None,
            duration: t.duration,
            track_number: None,
            disc_number: None,
            artwork,
            has_cover: false,
            location: Location::Remote { remote_id: t.id },
        }
    }
}
