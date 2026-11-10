//! Import de playlists depuis un lien : Deezer, YouTube / YouTube Music, Spotify.
//!
//! - Deezer et YouTube : la playlist est lue directement, ses morceaux sont jouables tels quels.
//! - Spotify : sans compte ni clé, on lit la page d'intégration publique de la playlist (ou de
//!   l'album), qui donne titre, artistes et durée des 100 premiers morceaux ; chacun est ensuite
//!   retrouvé sur Deezer (titre, artiste, durée à ±5 s). Ceux qui ne sont pas trouvés sont
//!   comptés, pas inventés.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    Error, Result, Track,
    sources::{DeezerSource, YouTubeMusicSource},
};

/// Une playlist importée, prête à être enregistrée.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub name: String,
    pub tracks: Vec<Track>,
    /// Morceaux de la playlist d'origine introuvables ici.
    pub missing: usize,
}

/// D'où vient un lien.
#[derive(Debug, PartialEq, Eq)]
enum Link {
    Deezer(String),
    /// Lien court Deezer (`link.deezer.com/s/…`) : à suivre pour trouver la playlist.
    DeezerShort(String),
    YouTube(String),
    Spotify { kind: &'static str, id: String },
}

fn parse(url: &str) -> Option<Link> {
    let url = url.trim();
    let after = |marker: &str| -> Option<String> {
        let i = url.find(marker)? + marker.len();
        let id: String = url[i..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
        (!id.is_empty()).then_some(id)
    };
    if url.contains("link.deezer.com") || url.contains("deezer.page.link") {
        return Some(Link::DeezerShort(url.to_owned()));
    }
    if url.contains("deezer.com") {
        return after("/playlist/").map(Link::Deezer);
    }
    if url.contains("youtube.com") || url.contains("youtu.be") {
        return after("list=").map(Link::YouTube);
    }
    if url.contains("spotify.com") {
        if let Some(id) = after("/playlist/") {
            return Some(Link::Spotify { kind: "playlist", id });
        }
        return after("/album/").map(|id| Link::Spotify { kind: "album", id });
    }
    None
}

/// Lit la playlist d'un lien. `progress(fait, total)` est appelé pendant la recherche des
/// morceaux Spotify (la seule étape longue).
pub async fn import(
    url: &str,
    client: &reqwest::Client,
    deezer: &Arc<DeezerSource>,
    youtube: &Arc<YouTubeMusicSource>,
    progress: impl Fn(usize, usize),
) -> Result<Imported> {
    let unknown = || Error::Other("Lien non reconnu : colle le lien d'une playlist Spotify, Deezer ou YouTube Music.".into());
    let mut link = parse(url).ok_or_else(unknown)?;
    if let Link::DeezerShort(short) = &link {
        // Le lien court redirige vers la vraie adresse de la playlist.
        let res = client.get(short).send().await?;
        link = parse(res.url().as_str()).filter(|l| matches!(l, Link::Deezer(_))).ok_or_else(unknown)?;
    }
    match link {
        Link::Deezer(id) => {
            let (name, tracks) = deezer.playlist(&id).await?;
            Ok(Imported { name, tracks, missing: 0 })
        }
        Link::YouTube(id) => {
            let (name, tracks) = youtube.playlist(&id).await?;
            Ok(Imported { name, tracks, missing: 0 })
        }
        Link::Spotify { kind, id } => {
            let (name, wanted) = spotify(client, kind, &id).await?;
            let total = wanted.len();
            progress(0, total);
            let mut tracks = Vec::with_capacity(total);
            let mut missing = 0;
            // Deux recherches à la fois : Deezer limite à 50 requêtes par 5 s.
            for (k, chunk) in wanted.chunks(2).enumerate() {
                let found = match chunk {
                    [a, b] => {
                        let (a, b) = tokio::join!(
                            deezer.find_track(&a.title, &a.artist, a.duration),
                            deezer.find_track(&b.title, &b.artist, b.duration),
                        );
                        vec![a, b]
                    }
                    [a] => vec![deezer.find_track(&a.title, &a.artist, a.duration).await],
                    _ => Vec::new(),
                };
                for f in found {
                    match f {
                        Ok(Some(t)) => tracks.push(t),
                        _ => missing += 1,
                    }
                }
                progress(((k + 1) * 2).min(total), total);
            }
            Ok(Imported { name, tracks, missing })
        }
        Link::DeezerShort(_) => Err(unknown()),
    }
}

struct Wanted {
    title: String,
    artist: String,
    duration: Option<f64>,
}

/// Titre, artistes et durée des morceaux d'une playlist ou d'un album Spotify, d'après sa page
/// d'intégration publique.
async fn spotify(client: &reqwest::Client, kind: &str, id: &str) -> Result<(String, Vec<Wanted>)> {
    #[derive(Deserialize)]
    struct Entity {
        name: String,
        #[serde(rename = "trackList", default)]
        track_list: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        title: String,
        #[serde(default)]
        subtitle: String,
        duration: Option<f64>,
    }
    let html = client
        .get(format!("https://open.spotify.com/embed/{kind}/{id}"))
        .header(reqwest::header::USER_AGENT, "Mozilla/5.0")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let bad = || Error::Other("Spotify : playlist introuvable ou privée.".into());
    let start = html.find(r#"<script id="__NEXT_DATA__" type="application/json">"#).ok_or_else(bad)?;
    let json = &html[start..];
    let json = &json[json.find('>').ok_or_else(bad)? + 1..];
    let json = &json[..json.find("</script>").ok_or_else(bad)?];
    let data: serde_json::Value = serde_json::from_str(json).map_err(|_| bad())?;
    let entity = data.pointer("/props/pageProps/state/data/entity").ok_or_else(bad)?;
    let entity: Entity = serde_json::from_value(entity.clone()).map_err(|_| bad())?;
    let wanted = entity
        .track_list
        .into_iter()
        .map(|t| Wanted {
            // Premier artiste : les invités sont souvent écrits autrement d'un service à l'autre.
            artist: t.subtitle.split(',').next().unwrap_or("").trim().to_owned(),
            title: t.title,
            duration: t.duration.map(|ms| ms / 1000.0),
        })
        .collect();
    Ok((entity.name, wanted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_links() {
        assert_eq!(parse("https://www.deezer.com/fr/playlist/1109890291?utm=x"), Some(Link::Deezer("1109890291".into())));
        assert_eq!(
            parse("https://open.spotify.com/intl-fr/playlist/37i9dQZF1DXcBWIGoYBM5M?si=abc"),
            Some(Link::Spotify { kind: "playlist", id: "37i9dQZF1DXcBWIGoYBM5M".into() })
        );
        assert_eq!(
            parse("https://music.youtube.com/playlist?list=PL4fGSI1pDJn6puJdseH2Rt9sMvt9E2M4i"),
            Some(Link::YouTube("PL4fGSI1pDJn6puJdseH2Rt9sMvt9E2M4i".into()))
        );
        assert_eq!(parse("https://link.deezer.com/s/30abc"), Some(Link::DeezerShort("https://link.deezer.com/s/30abc".into())));
        assert_eq!(parse("bonjour"), None);
    }
}
