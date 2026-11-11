//! Import de playlists depuis un lien : Deezer, YouTube / YouTube Music, Spotify.
//!
//! - Deezer et YouTube : la playlist est lue directement, ses morceaux sont jouables tels quels.
//! - Spotify : sans compte ni clé, on lit la page d'intégration publique de la playlist (ou de
//!   l'album), qui donne titre, artistes et durée des 100 premiers morceaux ; chacun est ensuite
//!   retrouvé sur Deezer (titre, artiste, durée à ±5 s), sinon sur YouTube Music. Ceux qui ne
//!   sont trouvés nulle part sont comptés, pas inventés.
//!
//! - Liste de liens de morceaux Spotify (Cmd+C sur une sélection dans Spotify, par exemple
//!   tous les « Titres likés ») : chaque morceau est lu sur sa page d'intégration publique, puis
//!   retrouvé de la même façon. Ces morceaux vont dans les titres likés.
//!
//! L'image de la playlist d'origine (personnalisée ou non) est gardée comme pochette.

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
    /// Image de la playlist d'origine.
    pub cover: Option<String>,
    pub tracks: Vec<Track>,
    /// Morceaux de la playlist d'origine introuvables ici.
    pub missing: usize,
    /// Morceaux à ranger dans les titres likés plutôt que dans une nouvelle playlist.
    pub liked: bool,
}

/// D'où vient un lien.
#[derive(Debug, PartialEq, Eq)]
enum Link {
    Deezer(String),
    /// Lien court Deezer (`link.deezer.com/s/…`) : à suivre pour trouver la playlist.
    DeezerShort(String),
    YouTube(String),
    Spotify { kind: &'static str, id: String },
    /// Plusieurs morceaux Spotify, dans l'ordre (doublons retirés).
    SpotifyTracks(Vec<String>),
}

/// Identifiants des morceaux Spotify cités dans un texte (liens `open.spotify.com/track/…` ou
/// `spotify:track:…`, un par ligne ou collés bout à bout).
fn spotify_tracks(text: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for marker in ["/track/", "spotify:track:"] {
        for (i, _) in text.match_indices(marker) {
            // Un identifiant Spotify fait toujours 22 caractères (base 62).
            let id: String = text[i + marker.len()..].chars().take(22).collect();
            if id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
                ids.push(id);
            }
        }
    }
    // Ordre du texte (les deux formes peuvent être mélangées).
    ids.sort_by_key(|id| text.find(id.as_str()));
    let mut seen = std::collections::HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));
    ids
}

fn parse(url: &str) -> Option<Link> {
    let url = url.trim();
    if url.contains("spotify") && (url.contains("/track/") || url.contains("spotify:track:")) {
        let ids = spotify_tracks(url);
        if !ids.is_empty() {
            return Some(Link::SpotifyTracks(ids));
        }
    }
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
    let unknown = || {
        Error::Other("Unrecognized link: paste the link of a Spotify, Deezer or YouTube Music playlist, or Spotify song links.".into())
    };
    let mut link = parse(url).ok_or_else(unknown)?;
    if let Link::DeezerShort(short) = &link {
        // Le lien court redirige vers la vraie adresse de la playlist.
        let res = client.get(short).send().await?;
        link = parse(res.url().as_str()).filter(|l| matches!(l, Link::Deezer(_))).ok_or_else(unknown)?;
    }
    match link {
        Link::Deezer(id) => {
            let (name, cover, tracks) = deezer.playlist(&id).await?;
            Ok(Imported { name, cover, tracks, missing: 0, liked: false })
        }
        Link::YouTube(id) => {
            let (name, cover, tracks) = youtube.playlist(&id).await?;
            Ok(Imported { name, cover, tracks, missing: 0, liked: false })
        }
        Link::SpotifyTracks(ids) => {
            let total = ids.len();
            progress(0, total);
            let mut tracks = Vec::with_capacity(total);
            let mut missing = 0;
            // Quatre à la fois : la page Spotify, puis la recherche (Deezer se règle seul sur
            // son quota).
            for (k, chunk) in ids.chunks(4).enumerate() {
                let found = futures_join(chunk.iter().map(|id| async move {
                    let wanted = spotify_track(client, id).await.ok()?;
                    find(deezer, youtube, &wanted).await
                }))
                .await;
                for f in found {
                    match f {
                        Some(t) => tracks.push(t),
                        None => missing += 1,
                    }
                }
                progress(((k + 1) * 4).min(total), total);
            }
            Ok(Imported { name: "Spotify".into(), cover: None, tracks, missing, liked: true })
        }
        Link::Spotify { kind, id } => {
            let (name, cover, wanted) = spotify(client, kind, &id).await?;
            let total = wanted.len();
            progress(0, total);
            let mut tracks = Vec::with_capacity(total);
            let mut missing = 0;
            // Deux recherches à la fois : Deezer limite à 50 requêtes par 5 s.
            for (k, chunk) in wanted.chunks(2).enumerate() {
                let found = match chunk {
                    [a, b] => {
                        let (a, b) = tokio::join!(find(deezer, youtube, a), find(deezer, youtube, b));
                        vec![a, b]
                    }
                    [a] => vec![find(deezer, youtube, a).await],
                    _ => Vec::new(),
                };
                for f in found {
                    match f {
                        Some(t) => tracks.push(t),
                        None => missing += 1,
                    }
                }
                progress(((k + 1) * 2).min(total), total);
            }
            Ok(Imported { name, cover, tracks, missing, liked: false })
        }
        Link::DeezerShort(_) => Err(unknown()),
    }
}

/// Attend toutes ces tâches (lancées ensemble) et renvoie leurs résultats dans l'ordre.
async fn futures_join<F: std::future::Future>(tasks: impl Iterator<Item = F>) -> Vec<F::Output> {
    let mut tasks: Vec<_> = tasks.map(Box::pin).collect();
    let mut out: Vec<Option<F::Output>> = (0..tasks.len()).map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut pending = false;
        for (task, slot) in tasks.iter_mut().zip(out.iter_mut()) {
            if slot.is_none() {
                match task.as_mut().poll(cx) {
                    std::task::Poll::Ready(v) => *slot = Some(v),
                    std::task::Poll::Pending => pending = true,
                }
            }
        }
        if pending { std::task::Poll::Pending } else { std::task::Poll::Ready(()) }
    })
    .await;
    out.into_iter().map(|v| v.expect("tâche terminée")).collect()
}

/// Lit le JSON de la page d'intégration publique Spotify (`kind` : playlist, album, track).
/// Spotify répond parfois 429 quand on enchaîne les pages : on attend et on réessaie.
async fn spotify_embed(client: &reqwest::Client, kind: &str, id: &str) -> Result<serde_json::Value> {
    let bad = || Error::Other("Spotify: playlist not found or private.".into());
    let mut attempt = 0;
    let html = loop {
        let res = client
            .get(format!("https://open.spotify.com/embed/{kind}/{id}"))
            .header(reqwest::header::USER_AGENT, "Mozilla/5.0")
            .send()
            .await?;
        if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt < 4 {
            attempt += 1;
            let wait = res
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()?.parse::<u64>().ok())
                .unwrap_or(2)
                .min(30);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            continue;
        }
        break res.error_for_status()?.text().await?;
    };
    let start = html.find(r#"<script id="__NEXT_DATA__" type="application/json">"#).ok_or_else(bad)?;
    let json = &html[start..];
    let json = &json[json.find('>').ok_or_else(bad)? + 1..];
    let json = &json[..json.find("</script>").ok_or_else(bad)?];
    let mut data: serde_json::Value = serde_json::from_str(json).map_err(|_| bad())?;
    data.pointer_mut("/props/pageProps/state/data/entity").map(serde_json::Value::take).ok_or_else(bad)
}

/// Titre, artiste principal et durée d'un morceau Spotify.
async fn spotify_track(client: &reqwest::Client, id: &str) -> Result<Wanted> {
    #[derive(Deserialize)]
    struct Entity {
        name: String,
        #[serde(default)]
        artists: Vec<Artist>,
        duration: Option<f64>,
    }
    #[derive(Deserialize)]
    struct Artist {
        name: String,
    }
    let entity: Entity = serde_json::from_value(spotify_embed(client, "track", id).await?)
        .map_err(|_| Error::Other("Spotify: song not found.".into()))?;
    Ok(Wanted {
        artist: entity.artists.into_iter().next().map(|a| a.name).unwrap_or_default(),
        title: entity.name,
        duration: entity.duration.map(|ms| ms / 1000.0),
    })
}

/// Un morceau Spotify retrouvé : sur Deezer d'abord (fiches artiste / album complètes), sinon
/// sur YouTube Music, au plus gros catalogue. Une erreur passagère (réseau, quota) est
/// réessayée une fois avant d'abandonner.
async fn find(deezer: &DeezerSource, youtube: &YouTubeMusicSource, w: &Wanted) -> Option<Track> {
    for attempt in 0..2 {
        match deezer.find_track(&w.title, &w.artist, w.duration).await {
            Ok(Some(t)) => return Some(t),
            Ok(None) => break,
            Err(_) if attempt == 0 => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
            Err(_) => break,
        }
    }
    let title = clean_title(&w.title);
    for attempt in 0..2 {
        match youtube.find_track(&title, &w.artist, w.duration).await {
            Ok(found) => return found,
            Err(_) if attempt == 0 => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
            Err(_) => {}
        }
    }
    None
}

/// Titre sans les mentions de réédition propres à un service (« Funeralopolis - 2004
/// Remaster » sur Spotify, « Funeralopolis » sur Deezer). Les versions live, remix… restent.
pub(crate) fn clean_title(title: &str) -> String {
    let noise = |part: &str| {
        let p = part.to_lowercase();
        ["remaster", "mono", "stereo", "mix 20", "deluxe", "bonus track", "single version", "radio edit"]
            .iter()
            .any(|w| p.contains(w))
    };
    let mut t = title.trim().to_owned();
    // « Titre - 2004 Remaster »
    if let Some(i) = t.rfind(" - ") {
        if noise(&t[i + 3..]) {
            t.truncate(i);
        }
    }
    // « Titre (Remastered 2011) », « Titre [Mono] »
    while let Some(open) = t.rfind(['(', '[']) {
        if open == 0 || !noise(&t[open..]) {
            break;
        }
        t.truncate(open);
        t = t.trim_end().to_owned();
    }
    t
}

struct Wanted {
    title: String,
    artist: String,
    duration: Option<f64>,
}

/// Nom, image, et titre, artistes et durée des morceaux d'une playlist ou d'un album Spotify,
/// d'après sa page d'intégration publique.
async fn spotify(client: &reqwest::Client, kind: &str, id: &str) -> Result<(String, Option<String>, Vec<Wanted>)> {
    #[derive(Deserialize)]
    struct Entity {
        name: String,
        #[serde(rename = "visualIdentity", default)]
        visual: Option<Visual>,
        #[serde(rename = "coverArt", default)]
        cover_art: Option<CoverArt>,
        #[serde(rename = "trackList", default)]
        track_list: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Visual {
        #[serde(default)]
        image: Vec<Image>,
    }
    #[derive(Deserialize)]
    struct CoverArt {
        #[serde(default)]
        sources: Vec<Image>,
    }
    #[derive(Deserialize)]
    struct Image {
        url: String,
        #[serde(rename = "maxWidth", default)]
        max_width: Option<u32>,
    }
    #[derive(Deserialize)]
    struct Item {
        title: String,
        #[serde(default)]
        subtitle: String,
        duration: Option<f64>,
    }
    let entity: Entity = serde_json::from_value(spotify_embed(client, kind, id).await?)
        .map_err(|_| Error::Other("Spotify: playlist not found or private.".into()))?;
    // La plus grande image (640 px) ; sinon celle de la pochette.
    let cover = entity
        .visual
        .and_then(|v| v.image.into_iter().max_by_key(|i| i.max_width.unwrap_or(0)))
        .or_else(|| entity.cover_art.and_then(|c| c.sources.into_iter().next()))
        .map(|i| i.url);
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
    Ok((entity.name, cover, wanted))
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
        // Cmd+C sur des morceaux dans Spotify : un lien par ligne, ou collés bout à bout par
        // un champ d'une seule ligne.
        let a = "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC";
        let b = "https://open.spotify.com/track/7GhIk7Il098yCjg4BQjzvb?si=x";
        let want = Link::SpotifyTracks(vec!["4uLU6hMCjMI75M1A2tKUQC".into(), "7GhIk7Il098yCjg4BQjzvb".into()]);
        assert_eq!(parse(&format!("{a}\n{b}\n{a}")), Some(want));
        assert_eq!(
            parse(&format!("{a}{b}")),
            Some(Link::SpotifyTracks(vec!["4uLU6hMCjMI75M1A2tKUQC".into(), "7GhIk7Il098yCjg4BQjzvb".into()]))
        );
        assert_eq!(parse("spotify:track:4uLU6hMCjMI75M1A2tKUQC"), Some(Link::SpotifyTracks(vec!["4uLU6hMCjMI75M1A2tKUQC".into()])));
    }

    #[test]
    fn cleans_titles() {
        assert_eq!(clean_title("Funeralopolis - 2004 Remaster"), "Funeralopolis");
        assert_eq!(clean_title("Help! (Remastered 2009)"), "Help!");
        assert_eq!(clean_title("Paranoid [Mono]"), "Paranoid");
        assert_eq!(clean_title("Electric Machine (Live)"), "Electric Machine (Live)");
        assert_eq!(clean_title("Spare No Man"), "Spare No Man");
        assert_eq!(clean_title("Song - Radio Edit"), "Song");
    }
}
