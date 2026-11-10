//! Paroles synchronisées, au mot près quand c'est possible.
//!
//! Deux sources interrogées en même temps :
//! - **NetEase Cloud Music** (format « YRC ») : début et durée de chaque mot, pour une grande
//!   partie des titres connus, y compris français. Morceau retrouvé par titre, artiste et durée
//!   (±3 s), pour ne pas prendre un remix ou une autre version.
//! - **[LRCLIB](https://lrclib.net)**, base de paroles libre et ouverte : paroles ligne par ligne,
//!   parfois au mot près (format « Enhanced LRC », `<mm:ss.xx>` devant chaque mot).
//!   1. Recherche exacte (artiste, titre, album, durée).
//!   2. Sinon, recherche large, en préférant les paroles synchronisées dont la durée est la plus
//!      proche (±4 s), pour éviter les versions « sped up », live, etc.
//!   3. Sinon, paroles simples (non synchronisées).
//!
//! NetEase gagne quand il a le mot par mot. Sinon, chaque ligne est découpée en mots dont le
//! minutage est estimé (longueur des mots, durée de chant plausible) : l'effet karaoké suit la
//! voix au lieu de s'étaler jusqu'à la ligne suivante pendant les pauses.
//!
//! Les résultats (y compris « pas de paroles ») sont gardés en cache pour la session.

use std::collections::HashMap;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::{Result, Track, model::split_artists};

const API: &str = "https://lrclib.net/api";
const NETEASE: &str = "https://music.163.com/api";
const CACHE_MAX: usize = 300;
/// Écart de durée toléré entre le morceau et les paroles.
const DURATION_TOLERANCE: f64 = 4.0;
/// Écart toléré pour NetEase, plus strict : son catalogue a beaucoup de versions proches.
const NETEASE_TOLERANCE: f64 = 3.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    /// Lignes horodatées, triées. Vide si seules les paroles simples existent.
    pub lines: Vec<Line>,
    /// Paroles simples, si pas de version synchronisée.
    pub plain: Option<String>,
    pub instrumental: bool,
    /// Le minutage des mots vient de la source (sinon, il est estimé).
    pub word_synced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    /// Début de la ligne, en secondes.
    pub time: f64,
    /// Fin du chant de la ligne (dernier mot), en secondes.
    pub end: f64,
    pub text: String,
    /// Les mots, dans l'ordre. Vide pour une ligne vide (pause instrumentale).
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Word {
    pub time: f64,
    pub end: f64,
    /// Le mot, avec l'espace qui le suit s'il y en a une (« I've »).
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
        let (ne, lr) = tokio::join!(self.netease(track), self.lrclib(track));
        // Une source en panne ne doit pas priver de l'autre.
        let ne = ne.unwrap_or_default();
        let found = match ne.words {
            Some(words) => Some(words),
            None => {
                let lr = match lr {
                    Ok(lr) => lr,
                    Err(e) if ne.lrc.is_none() => return Err(e),
                    Err(_) => LrcFound::default(),
                };
                choose(lr, ne.lrc)
            }
        };
        let mut cache = self.cache.lock();
        if cache.len() >= CACHE_MAX {
            cache.clear();
        }
        cache.insert(track.id.clone(), found.clone());
        Ok(found)
    }

    /// Toutes les paroles LRCLIB compatibles : recherche exacte et recherche large, en parallèle.
    async fn lrclib(&self, track: &Track) -> Result<LrcFound> {
        let title = clean_title(&track.title);
        let artist = main_artist(&track.artist);
        let duration = track.duration;

        let mut params = vec![("artist_name", artist.clone()), ("track_name", title.clone())];
        if let Some(album) = &track.album {
            params.push(("album_name", album.clone()));
        }
        if let Some(d) = duration {
            params.push(("duration", format!("{}", d.round() as u64)));
        }
        let exact = async {
            let res = self.client.get(format!("{API}/get")).query(&params).send().await?;
            if !res.status().is_success() {
                return Ok(None);
            }
            Ok::<_, crate::Error>(Some(res.json::<Record>().await?))
        };
        let search = async {
            let found: Vec<Record> = self
                .client
                .get(format!("{API}/search"))
                .query(&[("track_name", title.as_str()), ("artist_name", artist.as_str())])
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            Ok::<_, crate::Error>(found)
        };
        let (exact, search) = tokio::join!(exact, search);
        let exact = exact.ok().flatten();
        let mut search = match (search, &exact) {
            (Ok(s), _) => s,
            (Err(_), Some(_)) => Vec::new(),
            (Err(e), None) => return Err(e),
        };

        let close = |r: &Record| match (duration, r.duration) {
            (Some(a), Some(b)) => (a - b).abs() <= DURATION_TOLERANCE,
            _ => true,
        };
        let gap = |r: &Record| match (duration, r.duration) {
            (Some(a), Some(b)) => (a - b).abs(),
            _ => f64::MAX / 2.0,
        };
        search.retain(|r| close(r));
        search.sort_by(|a, b| gap(a).total_cmp(&gap(b)));

        let mut out = LrcFound::default();
        // La recherche exacte d'abord : à égalité, c'est elle qui l'emporte.
        for r in exact.iter().chain(search.iter()) {
            if let Some(text) = &r.synced_lyrics
                && !out.synced.contains(text)
            {
                out.synced.push(text.clone());
            }
            out.instrumental |= r.instrumental && r.synced_lyrics.is_none() && r.plain_lyrics.is_none();
            if out.plain.is_none() {
                out.plain = r.plain_lyrics.clone();
            }
        }
        if let Some(e) = &exact {
            out.instrumental |= e.instrumental;
        }
        Ok(out)
    }

    /// NetEase Cloud Music, si le même enregistrement y est trouvé : paroles au mot près (YRC)
    /// quand il y en a, et toujours ses paroles ligne par ligne (pour départager les versions).
    async fn netease(&self, track: &Track) -> Result<NeFound> {
        let title = clean_title(&track.title);
        let artists: Vec<String> = if track.artists.is_empty() {
            split_artists(&track.artist)
        } else {
            track.artists.iter().map(|a| a.name.clone()).collect()
        };
        let query = format!("{title} {}", artists.first().map(String::as_str).unwrap_or(""));
        let found: NeSearch = self
            .client
            .get(format!("{NETEASE}/search/get"))
            .header(reqwest::header::REFERER, "https://music.163.com")
            .query(&[("s", query.as_str()), ("type", "1"), ("limit", "10")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let songs = found.result.map(|r| r.songs).unwrap_or_default();
        let Some(song) = songs.iter().find(|s| same_song(s, &title, &artists, track.duration)) else {
            return Ok(NeFound::default());
        };
        let lyric: NeLyric = self
            .client
            .get(format!("{NETEASE}/song/lyric"))
            .header(reqwest::header::REFERER, "https://music.163.com")
            .query(&[("id", song.id.to_string().as_str()), ("lv", "1"), ("yv", "1")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let lrc = lyric.lrc.map(|l| l.lyric).filter(|l| !l.trim().is_empty());
        let mut lines = lyric.yrc.map(|y| parse_yrc(&y.lyric)).unwrap_or_default();
        // NetEase ouvre parfois par une ligne « Titre - Artiste » : ce n'est pas chanté.
        let (t, a) = (norm(&title), artists.iter().map(|a| norm(a)).collect::<Vec<_>>());
        let is_heading = |l: &Line| {
            let n = norm(&l.text);
            n.contains(&t) && a.iter().any(|a| !a.is_empty() && n.contains(a.as_str()))
        };
        if lines.first().is_some_and(is_heading) {
            lines.remove(0);
        }
        let words = (!lines.is_empty()).then_some(Lyrics { lines, plain: None, instrumental: false, word_synced: true });
        Ok(NeFound { words, lrc })
    }
}

/// Ce que NetEase a pour ce morceau.
#[derive(Default)]
struct NeFound {
    /// Paroles au mot près (YRC).
    words: Option<Lyrics>,
    /// Paroles ligne par ligne (LRC).
    lrc: Option<String>,
}

/// Ce que LRCLIB a pour ce morceau : toutes les versions synchronisées compatibles (la
/// recherche exacte en premier), et à défaut des paroles simples.
#[derive(Default)]
struct LrcFound {
    synced: Vec<String>,
    plain: Option<String>,
    instrumental: bool,
}

/// Choisit les paroles synchronisées les plus fiables parmi les versions LRCLIB et celle de
/// NetEase. Les bases de paroles se contredisent souvent d'une seconde ou plus (même texte,
/// horodatages décalés) : on garde la version qui s'accorde le mieux avec toutes les autres,
/// ce qui écarte les versions mal calées.
fn choose(lr: LrcFound, netease: Option<String>) -> Option<Lyrics> {
    let texts: Vec<String> = lr.synced.into_iter().chain(netease).collect();
    let parsed: Vec<Vec<Line>> = texts
        .iter()
        .map(|t| parse_lrc(t).into_iter().filter(|l| !is_credit(&l.text)).collect())
        .collect();
    let candidates: Vec<usize> = (0..parsed.len()).filter(|&i| parsed[i].iter().any(|l| !l.text.is_empty())).collect();
    let Some(&first) = candidates.first() else {
        if lr.instrumental {
            return Some(Lyrics { lines: Vec::new(), plain: None, instrumental: true, word_synced: false });
        }
        return lr.plain.map(|p| Lyrics { lines: Vec::new(), plain: Some(p), instrumental: false, word_synced: false });
    };
    let best = consensus(&candidates.iter().map(|&i| parsed[i].as_slice()).collect::<Vec<_>>())
        .map_or(first, |k| candidates[k]);
    Some(Lyrics {
        word_synced: has_word_tags(&texts[best]),
        lines: parsed.into_iter().nth(best).unwrap_or_default(),
        plain: None,
        instrumental: false,
    })
}

/// Indice de la version la plus centrale : la plus petite moyenne d'écart avec les autres.
/// Chaque écart est plafonné à 2 s : une version complètement à côté (autre montage) ne doit
/// pas peser plus que les autres. À égalité, la première (recherche exacte). Rien si aucune
/// comparaison n'est possible.
fn consensus(versions: &[&[Line]]) -> Option<usize> {
    let cost = |i: usize| {
        let gaps: Vec<f64> = (0..versions.len())
            .filter(|&j| j != i)
            .filter_map(|j| offset(versions[i], versions[j]))
            .map(|o| o.abs().min(2.0))
            .collect();
        (!gaps.is_empty()).then(|| gaps.iter().sum::<f64>() / gaps.len() as f64)
    };
    (0..versions.len())
        .filter_map(|i| cost(i).map(|c| (i, c)))
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(i, _)| i)
}

/// Décalage de `b` par rapport à `a` (médiane, en secondes), d'après les lignes au même texte.
/// Une ligne peut être coupée en deux d'une version à l'autre : le début d'une ligne compte si
/// l'un des textes commence par l'autre. Rien si moins de 3 lignes se correspondent.
fn offset(a: &[Line], b: &[Line]) -> Option<f64> {
    const MIN_LEN: usize = 6;
    let bn: Vec<(String, f64)> = b.iter().map(|l| (norm(&l.text), l.time)).filter(|(n, _)| n.len() >= MIN_LEN).collect();
    let mut diffs = Vec::new();
    let mut from = 0;
    for l in a {
        let n = norm(&l.text);
        if n.len() < MIN_LEN {
            continue;
        }
        // Dans l'ordre, et pas trop loin : un refrain répété ne doit pas être pris pour un autre.
        let window = from..bn.len().min(from + 6);
        if let Some(j) = window.into_iter().find(|&j| bn[j].0.starts_with(&n) || n.starts_with(&bn[j].0)) {
            diffs.push(bn[j].1 - l.time);
            from = j + 1;
        }
    }
    if diffs.len() < 3 {
        return None;
    }
    diffs.sort_by(f64::total_cmp);
    Some(diffs[diffs.len() / 2])
}

#[derive(Deserialize)]
struct NeSearch {
    result: Option<NeResult>,
}
#[derive(Deserialize)]
struct NeResult {
    #[serde(default)]
    songs: Vec<NeSong>,
}
#[derive(Deserialize)]
struct NeSong {
    id: u64,
    name: String,
    /// Millisecondes.
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    artists: Vec<NeArtist>,
}
#[derive(Deserialize)]
struct NeArtist {
    name: String,
}
#[derive(Deserialize)]
struct NeLyric {
    yrc: Option<NeText>,
    lrc: Option<NeText>,
}
#[derive(Deserialize)]
struct NeText {
    lyric: String,
}

/// Lettres et chiffres en minuscules : « Dernière danse » et « dernière-danse » se valent.
fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn overlaps(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a.contains(b) || b.contains(a))
}

/// Même enregistrement : durée proche, titre et au moins un artiste en commun.
fn same_song(s: &NeSong, title: &str, artists: &[String], duration: Option<f64>) -> bool {
    if let Some(d) = duration
        && s.duration > 0.0
        && (s.duration / 1000.0 - d).abs() > NETEASE_TOLERANCE
    {
        return false;
    }
    let artist_ok = artists.is_empty()
        || s.artists.iter().any(|a| artists.iter().any(|b| overlaps(&norm(&a.name), &norm(b))));
    overlaps(&norm(&s.name), &norm(title)) && artist_ok
}

/// Format YRC de NetEase : `[début,durée](début,durée,0)mot (début,durée,0)mot…`, en millisecondes.
/// Les lignes de crédits (paroles, musique…) et de métadonnées JSON sont ignorées.
pub fn parse_yrc(yrc: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for raw in yrc.lines() {
        let Some(rest) = raw.trim().strip_prefix('[') else { continue };
        let Some(close) = rest.find(']') else { continue };
        let Some((start, dur)) = rest[..close].split_once(',') else { continue };
        let (Ok(start), Ok(dur)) = (start.trim().parse::<f64>(), dur.trim().parse::<f64>()) else { continue };
        let words = yrc_words(&rest[close + 1..]);
        let text = words.iter().map(|w| w.text.as_str()).collect::<String>().trim_end().to_owned();
        if text.is_empty() || is_credit(&text) {
            continue;
        }
        lines.push(Line { time: start / 1000.0, end: (start + dur) / 1000.0, text, words });
    }
    lines.sort_by(|a, b| a.time.total_cmp(&b.time));
    lines
}

/// Mots d'une ligne YRC. Un mot peut contenir des parenthèses (« (oh) ») : seules celles de la
/// forme `(nombre,nombre,nombre)` sont des horodatages.
fn yrc_words(s: &str) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if let Some((time, end, len)) = yrc_tag(&s[i..]) {
            words.push(Word { time, end, text: String::new() });
            i += len;
            continue;
        }
        let c = s[i..].chars().next().expect("indice sur une frontière de caractère");
        if let Some(w) = words.last_mut() {
            w.text.push(c);
        }
        i += c.len_utf8();
    }
    words.retain(|w| !w.text.trim().is_empty());
    words
}

/// `(27360,240,0)` → (27,36 s, 27,60 s, longueur de la balise).
fn yrc_tag(s: &str) -> Option<(f64, f64, usize)> {
    let inner = s.strip_prefix('(')?;
    let close = inner.find(')')?;
    let mut parts = inner[..close].split(',');
    let start: f64 = parts.next()?.trim().parse().ok()?;
    let dur: f64 = parts.next()?.trim().parse().ok()?;
    parts.next()?.trim().parse::<f64>().ok()?;
    parts.next().is_none().then_some((start / 1000.0, (start + dur) / 1000.0, close + 2))
}

/// Ligne de crédits ajoutée par NetEase (« 作词 : … », « Composer：… »).
fn is_credit(text: &str) -> bool {
    const KEYS: &[&str] = &["作词", "作曲", "编曲", "制作", "混音", "母带", "监制", "出品", "词：", "曲："];
    KEYS.iter().any(|k| text.contains(k)) || text.contains('：')
}

/// Lit le format LRC : `[mm:ss.xx] texte`, avec éventuellement plusieurs horodatages par ligne,
/// et des horodatages par mot (`<mm:ss.xx>mot`, « Enhanced LRC »). Sans ces derniers, le
/// minutage des mots est estimé.
pub fn parse_lrc(lrc: &str) -> Vec<Line> {
    let mut parsed: Vec<(f64, String, Vec<Word>)> = Vec::new();
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
        let (text, words) = enhanced_words(rest.trim());
        parsed.extend(times.into_iter().map(|time| (time, text.clone(), words.clone())));
    }
    parsed.sort_by(|a, b| a.0.total_cmp(&b.0));

    let starts: Vec<f64> = parsed.iter().map(|p| p.0).collect();
    parsed
        .into_iter()
        .enumerate()
        .map(|(i, (time, text, mut words))| {
            let next = starts.get(i + 1).copied();
            if words.is_empty() {
                words = estimate_words(&text, time, next);
            } else if let Some(last) = words.last_mut() {
                // Fin du dernier mot : inconnue en Enhanced LRC, estimée d'après sa longueur.
                let guess = last.time + 0.25 + letters(&last.text) as f64 * 0.09;
                last.end = next.map_or(guess, |n| guess.min(n));
            }
            let end = words.last().map_or(time, |w| w.end);
            Line { time, end, text, words }
        })
        .collect()
}

/// Enhanced LRC : `<00:12.00>Hello <00:12.50>world` → texte et mots horodatés (fins provisoires :
/// début du mot suivant). Rien si la ligne n'a pas d'horodatage par mot.
fn enhanced_words(s: &str) -> (String, Vec<Word>) {
    if !s.contains('<') {
        return (s.to_owned(), Vec::new());
    }
    let mut words: Vec<Word> = Vec::new();
    let mut plain = String::new();
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(inner) = rest.strip_prefix('<')
            && let Some(close) = inner.find('>')
            && let Some(t) = parse_time(&inner[..close])
        {
            if let Some(prev) = words.last_mut() {
                prev.end = t;
            }
            words.push(Word { time: t, end: t, text: String::new() });
            rest = &inner[close + 1..];
            continue;
        }
        let c = rest.chars().next().expect("chaîne non vide");
        plain.push(c);
        if let Some(w) = words.last_mut() {
            w.text.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
    words.retain(|w| !w.text.trim().is_empty());
    (plain.trim().to_owned(), words)
}

/// Le texte contient au moins un horodatage de mot `<mm:ss.xx>`.
fn has_word_tags(lrc: &str) -> bool {
    lrc.split('<').skip(1).any(|part| part.split_once('>').is_some_and(|(t, _)| parse_time(t).is_some()))
}

fn letters(s: &str) -> usize {
    s.chars().filter(|c| c.is_alphanumeric()).count()
}

/// Mots d'une ligne dont seul le début est connu. La ligne est chantée en un temps plausible
/// (environ 0,09 s par lettre), sans déborder sur la suivante ; chaque mot en prend une part
/// proportionnelle à sa longueur. Les pauses entre les lignes restent des pauses.
fn estimate_words(text: &str, time: f64, next: Option<f64>) -> Vec<Word> {
    let tokens: Vec<&str> = text.split_inclusive(char::is_whitespace).filter(|t| !t.trim().is_empty()).collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let weights: Vec<f64> = tokens.iter().map(|t| letters(t).max(1) as f64 + 1.0).collect();
    let total: f64 = weights.iter().sum();
    let sung = 0.3 + letters(text) as f64 * 0.09;
    let room = next.map_or(sung, |n| ((n - time) * 0.92).max(0.3));
    let duration = sung.min(room);
    let mut at = time;
    tokens
        .iter()
        .zip(weights)
        .map(|(t, w)| {
            let len = duration * w / total;
            let word = Word { time: at, end: at + len, text: (*t).to_owned() };
            at += len;
            word
        })
        .collect()
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
        assert_eq!(l[0].text, "Refrain");
        assert_eq!(l[0].time, 1.0);
        assert_eq!(l[1].text, "Bonjour");
        assert!((l[1].time - 9.66).abs() < 1e-9);
        assert_eq!(l[2].text, ""); // ligne vide = pause instrumentale
        assert!(l[2].words.is_empty());
        assert!((l[3].time - 62.5).abs() < 1e-9);
    }

    #[test]
    fn estimates_words_without_spilling_over_pauses() {
        // « Bonjour tout le monde » à 10 s, ligne suivante à 30 s : chantée en ~2 s, pas en 20.
        let l = parse_lrc("[00:10.00]Bonjour tout le monde\n[00:30.00]Suite");
        let w = &l[0].words;
        assert_eq!(w.iter().map(|w| w.text.as_str()).collect::<String>(), "Bonjour tout le monde");
        assert_eq!(w[0].time, 10.0);
        assert!(l[0].end < 12.5, "fin estimée : {}", l[0].end);
        assert!(w.windows(2).all(|p| p[0].end <= p[1].time + 1e-9));
        // Ligne suivante très proche : on ne déborde pas dessus.
        let l = parse_lrc("[00:10.00]Une très longue ligne chantée vite\n[00:11.00]Suite");
        assert!(l[0].end <= 11.0);
    }

    #[test]
    fn parses_enhanced_lrc() {
        let l = parse_lrc("[00:12.00]<00:12.00>Hello <00:12.50>world\n[00:20.00]Next");
        assert_eq!(l[0].text, "Hello world");
        assert_eq!(l[0].words.len(), 2);
        assert_eq!((l[0].words[0].time, l[0].words[0].end), (12.0, 12.5));
        assert_eq!(l[0].words[1].text, "world");
        assert!(l[0].words[1].end > 12.5 && l[0].words[1].end < 14.0);
        assert!(has_word_tags("[00:12.00]<00:12.00>Hello"));
        assert!(!has_word_tags("[00:12.00]Hello <3"));
    }

    #[test]
    fn parses_yrc() {
        let yrc = "{\"t\":0,\"c\":[{\"tx\":\"作词: \"}]}\n\
                   [0,2000](0,1000,0)作词 : (1000,1000,0)Someone\n\
                   [27360,1290](27360,240,0)I've (27600,90,0)been (27690,360,0)(oh) (28050,600,0)call";
        let l = parse_yrc(yrc);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].text, "I've been (oh) call");
        assert_eq!(l[0].words.len(), 4);
        assert_eq!(l[0].words[0].text, "I've ");
        assert_eq!(l[0].words[2].text, "(oh) ");
        assert!((l[0].time - 27.36).abs() < 1e-9 && (l[0].end - 28.65).abs() < 1e-9);
        assert!((l[0].words[3].end - 28.65).abs() < 1e-9);
    }

    #[test]
    fn picks_the_version_that_agrees_with_the_others() {
        let lrc = |shift: f64| {
            ["Dans mon esprit tout divague", "Je me noie dans la vague", "Je ne veux que ton âme", "Une fleur, une femme"]
                .iter()
                .enumerate()
                .map(|(i, t)| format!("[00:{:05.2}]{t}", 8.0 + i as f64 * 4.0 + shift))
                .collect::<Vec<_>>()
                .join("\n")
        };
        // Exacte (en premier) décalée de +0,9 s ; trois autres versions d'accord à ±0,1 s.
        let lr = LrcFound { synced: vec![lrc(0.9), lrc(-0.1), lrc(0.0)], plain: None, instrumental: false };
        let got = choose(lr, Some(lrc(0.1))).unwrap();
        // Une des versions d'accord (à égalité, peu importe laquelle), pas celle décalée.
        assert!((got.lines[0].time - 8.0).abs() <= 0.11, "choisi : {}", got.lines[0].time);
        // Une seule version : on la garde.
        let lr = LrcFound { synced: vec![lrc(0.9)], plain: None, instrumental: false };
        assert!((choose(lr, None).unwrap().lines[0].time - 8.9).abs() < 1e-9);
    }

    #[test]
    fn cleans_names() {
        assert_eq!(main_artist("Mattyeux, Videoclub"), "Mattyeux");
        assert_eq!(main_artist("Videoclub & Adèle Castillon"), "Videoclub");
        assert_eq!(clean_title("Roi (feat. Mattyeux)"), "Roi");
        assert_eq!(clean_title("VHS (sped up version)"), "VHS (sped up version)");
    }

    #[test]
    fn matches_the_same_recording() {
        let song = |name: &str, artist: &str, s: f64| NeSong { id: 1, name: name.into(), duration: s * 1000.0, artists: vec![NeArtist { name: artist.into() }] };
        let stromae = vec!["Stromae".to_owned()];
        assert!(same_song(&song("Alors On Danse", "Stromae", 206.0), "Alors on danse", &stromae, Some(207.0)));
        assert!(!same_song(&song("Alors On Danse", "Kanye West", 214.0), "Alors on danse", &stromae, Some(207.0)));
        assert!(!same_song(&song("Alors On Danse", "Stromae", 240.0), "Alors on danse", &stromae, Some(207.0)));
    }
}
