//! Paroles synchronisées, via [LRCLIB](https://lrclib.net) : base de paroles libre et ouverte.
//!
//! 1. Recherche exacte (artiste, titre, album, durée).
//! 2. Sinon, recherche large, en préférant les paroles synchronisées dont la durée est la plus
//!    proche (±4 s), pour éviter les versions « sped up », live, etc.
//! 3. Sinon, paroles simples (non synchronisées).
//!
//! Les résultats (y compris « pas de paroles ») sont gardés en cache pour la session.

use std::collections::HashMap;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::{Result, Track};

const API: &str = "https://lrclib.net/api";
const CACHE_MAX: usize = 300;
/// Écart de durée toléré entre le morceau et les paroles.
const DURATION_TOLERANCE: f64 = 4.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    /// Lignes horodatées, triées. Vide si seules les paroles simples existent.
    pub lines: Vec<Line>,
    /// Paroles simples, si pas de version synchronisée.
    pub plain: Option<String>,
    pub instrumental: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    /// Début de la ligne, en secondes.
    pub time: f64,
    pub text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    duration: Option<f64>,
    #[serde(default)]
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

pub struct LyricsClient {
    client: reqwest::Client,
    cache: Mutex<HashMap<String, Option<Lyrics>>>,
}

impl LyricsClient {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client, cache: Mutex::new(HashMap::new()) }
    }

    pub async fn fetch(&self, track: &Track) -> Result<Option<Lyrics>> {
        if let Some(hit) = self.cache.lock().get(&track.id) {
            return Ok(hit.clone());
        }
        let found = self.lookup(track).await?;
        let mut cache = self.cache.lock();
        if cache.len() >= CACHE_MAX {
            cache.clear();
        }
        cache.insert(track.id.clone(), found.clone());
        Ok(found)
    }

    async fn lookup(&self, track: &Track) -> Result<Option<Lyrics>> {
        let title = clean_title(&track.title);
        let artist = main_artist(&track.artist);
        let duration = track.duration;

        // 1. Recherche exacte.
        let mut params = vec![("artist_name", artist.clone()), ("track_name", title.clone())];
        if let Some(album) = &track.album {
            params.push(("album_name", album.clone()));
        }
        if let Some(d) = duration {
            params.push(("duration", format!("{}", d.round() as u64)));
        }
        let exact = self.client.get(format!("{API}/get")).query(&params).send().await?;
        if exact.status().is_success() {
            let rec: Record = exact.json().await?;
            if rec.synced_lyrics.is_some() || rec.instrumental {
                return Ok(Some(to_lyrics(rec)));
            }
        }

        // 2. Recherche large, meilleur candidat.
        let candidates: Vec<Record> = self
            .client
            .get(format!("{API}/search"))
            .query(&[("track_name", title.as_str()), ("artist_name", artist.as_str())])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let close = |r: &Record| match (duration, r.duration) {
            (Some(a), Some(b)) => (a - b).abs() <= DURATION_TOLERANCE,
            _ => true,
        };
        let gap = |r: &Record| match (duration, r.duration) {
            (Some(a), Some(b)) => (a - b).abs(),
            _ => f64::MAX / 2.0,
        };
        let best_synced = candidates
            .iter()
            .filter(|r| r.synced_lyrics.is_some() && close(r))
            .min_by(|a, b| gap(a).total_cmp(&gap(b)));
        let best = best_synced.or_else(|| {
            candidates.iter().filter(|r| r.plain_lyrics.is_some() && close(r)).min_by(|a, b| gap(a).total_cmp(&gap(b)))
        });
        Ok(best.map(|r| {
            to_lyrics(Record {
                duration: r.duration,
                instrumental: r.instrumental,
                plain_lyrics: r.plain_lyrics.clone(),
                synced_lyrics: r.synced_lyrics.clone(),
            })
        }))
    }
}

fn to_lyrics(r: Record) -> Lyrics {
    let lines = r.synced_lyrics.as_deref().map(parse_lrc).unwrap_or_default();
    Lyrics {
        plain: if lines.is_empty() { r.plain_lyrics } else { None },
        lines,
        instrumental: r.instrumental,
    }
}

/// Lit le format LRC : `[mm:ss.xx] texte`, avec éventuellement plusieurs horodatages par ligne.
pub fn parse_lrc(lrc: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for raw in lrc.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            match parse_time(&stripped[..end]) {
                Some(t) => times.push(t),
                None => break, // balise de métadonnées ([ar:…], [ti:…]) : ligne ignorée
            }
            rest = stripped[end + 1..].trim_start();
        }
        let text = rest.trim().to_owned();
        lines.extend(times.into_iter().map(|time| Line { time, text: text.clone() }));
    }
    lines.sort_by(|a, b| a.time.total_cmp(&b.time));
    lines
}

fn parse_time(s: &str) -> Option<f64> {
    let (min, sec) = s.split_once(':')?;
    let min: f64 = min.trim().parse().ok()?;
    let sec: f64 = sec.trim().replace(',', ".").parse().ok()?;
    Some(min * 60.0 + sec)
}

/// Retire ce qui gêne la recherche : « (feat. X) », « - Remastered 2011 », « [Clip officiel] »…
/// On garde « (sped up version) » & co : ce sont d'autres versions, avec d'autres paroles/durées.
fn clean_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let mut cut = title.len();
    for marker in [" (feat", " [feat", " feat.", " ft.", " - remaster", " (remaster", " [official", " (official", " (clip", " [clip"] {
        if let Some(i) = lower.find(marker) {
            cut = cut.min(i);
        }
    }
    title[..cut].trim().to_owned()
}

/// Premier artiste d'une liste « A, B & C feat. D ».
fn main_artist(artist: &str) -> String {
    let lower = artist.to_lowercase();
    let mut cut = artist.len();
    for sep in [", ", " & ", " feat", " ft.", " x ", " et "] {
        if let Some(i) = lower.find(sep) {
            cut = cut.min(i);
        }
    }
    artist[..cut].trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lrc() {
        let l = parse_lrc("[ar:Test]\n[00:09.66] Bonjour\n[01:02.5][00:01.00]Refrain\n[00:20.00]");
        assert_eq!(l.len(), 4);
        assert_eq!(l[0], Line { time: 1.0, text: "Refrain".into() });
        assert_eq!(l[1].text, "Bonjour");
        assert!((l[1].time - 9.66).abs() < 1e-9);
        assert_eq!(l[2].text, ""); // ligne vide = pause instrumentale
        assert!((l[3].time - 62.5).abs() < 1e-9);
    }

    #[test]
    fn cleans_names() {
        assert_eq!(main_artist("Mattyeux, Videoclub"), "Mattyeux");
        assert_eq!(main_artist("Videoclub & Adèle Castillon"), "Videoclub");
        assert_eq!(clean_title("Roi (feat. Mattyeux)"), "Roi");
        assert_eq!(clean_title("VHS (sped up version)"), "VHS (sped up version)");
    }
}
