//! Page « Découvrir » : recommandations personnalisées, sans compte ni serveur.
//!
//! 1. [`profile`] résume tes goûts à partir de ce qui est sur l'appareil : temps d'écoute (le
//!    récent compte plus), titres likés, playlists, morceaux sautés. On en tire un score par
//!    artiste, les morceaux déjà connus, ceux à éviter, et des « redécouvertes ».
//! 2. [`plan`] choisit les sections du jour (mix, « Parce que tu écoutes X », nouveautés de tes
//!    artistes, artistes à découvrir, redécouvertes, tendances). Le choix tourne chaque jour.
//! 3. [`Discover::build`] va chercher le contenu sur Deezer (radios d'artistes, artistes proches,
//!    sorties, top du pays), en parallèle, puis assemble les sections une à une : chaque morceau
//!    n'apparaît qu'une fois sur la page, au plus deux ou trois titres par artiste et par section.
//!
//! Les sections sont gardées en cache (mémoire et disque) : la page s'affiche aussitôt au
//! lancement, puis se met à jour en arrière-plan quand le cache a vieilli.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::{
    ArtistRef, CollectionData, ListenLog, Track,
    collection::save_json,
    model::split_artists,
    sources::{
        DeezerSource,
        deezer::{Album, ArtistCard},
    },
    stats::{Listen, artists_of, civil_from_days},
};

const DAY: u64 = 86_400_000;
/// Durée de vie du cache : top du pays une heure, le reste six heures.
const TTL_CHARTS: u64 = 3_600_000;
const TTL: u64 = 6 * 3_600_000;
/// Un morceau écouté depuis moins longtemps est « récent » (pas reproposé dans les tendances).
const RECENT: u64 = 14 * DAY;
/// Une sortie de moins de 60 jours est une nouveauté.
const NEW_RELEASE_DAYS: i64 = 60;

// ---------- Ce que voit l'interface ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// Mix du jour : découvertes et titres que tu aimes, mélangés.
    Mix,
    /// « Parce que tu écoutes X ».
    Because,
    /// Nouveautés de tes artistes.
    Releases,
    /// Artistes à découvrir.
    Artists,
    /// Titres aimés que tu n'as pas écoutés depuis longtemps.
    Rediscover,
    /// Top du pays.
    Charts,
    /// Nouvel utilisateur : artistes et albums du moment.
    ChartArtists,
    ChartAlbums,
}

/// Une sortie d'album, EP ou single.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    #[serde(flatten)]
    pub album: Album,
    pub artist: String,
    /// Date de sortie complète (« 2026-11-07 »), vide si inconnue.
    #[serde(default)]
    pub date: String,
}

/// Un artiste proposé, et l'artiste que tu écoutes dont il est proche.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub id: String,
    pub name: String,
    pub picture: Option<String>,
    #[serde(default)]
    pub near: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    /// Identifiant stable (`mix:<jour>`, `because:<artiste>`, `releases`…).
    pub id: String,
    pub kind: Kind,
    /// « Parce que tu écoutes X » : l'artiste X.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<ArtistRef>,
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub albums: Vec<Release>,
    #[serde(default)]
    pub artists: Vec<Suggestion>,
    /// Contenu présent ; sinon la section est en cours de chargement.
    pub ready: bool,
    /// Calcul du contenu (millisecondes depuis 1970).
    #[serde(default)]
    pub at: u64,
}

impl Section {
    fn stub(id: impl Into<String>, kind: Kind) -> Self {
        Self { id: id.into(), kind, artist: None, tracks: Vec::new(), albums: Vec::new(), artists: Vec::new(), ready: false, at: 0 }
    }

    fn ttl(&self) -> u64 {
        match self.kind {
            Kind::Charts | Kind::ChartArtists | Kind::ChartAlbums => TTL_CHARTS,
            _ => TTL,
        }
    }

    fn is_empty(&self) -> bool {
        self.tracks.is_empty() && self.albums.is_empty() && self.artists.is_empty()
    }
}

// ---------- Profil ----------

/// Un artiste que tu écoutes, et à quel point (score sans unité, plus haut = plus écouté).
#[derive(Debug, Clone, PartialEq)]
pub struct Fav {
    pub key: String,
    pub name: String,
    pub id: Option<String>,
    pub score: f64,
}

/// Tes goûts, résumés.
#[derive(Debug, Clone, Default)]
pub struct Profile {
    /// Artistes, du plus écouté au moins écouté.
    pub artists: Vec<Fav>,
    /// Tous les artistes déjà croisés (écoutés, likés, en playlist, sautés).
    pub known_artists: HashSet<String>,
    /// Morceaux déjà connus : likés, en playlist ou déjà écoutés (clé [`track_key`]).
    pub known: HashSet<String>,
    pub liked: HashSet<String>,
    /// Écoutés ces 14 derniers jours.
    pub recent: HashSet<String>,
    /// Sautés récemment : on ne les repropose pas.
    pub skipped: HashSet<String>,
    /// Tes titres préférés, du plus au moins écouté (part « connue » du mix).
    pub familiar: Vec<Track>,
    /// Titres aimés pas écoutés depuis longtemps.
    pub rediscover: Vec<Track>,
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Clé d'un morceau, indépendante de sa source et des variantes de titre : « Titre (Radio
/// Edit) » de Deezer et « Titre » de YouTube ont la même.
pub fn track_key(t: &Track) -> String {
    let base = t.title.split(" (").next().unwrap_or(&t.title);
    let base = base.split(" [").next().unwrap_or(base);
    let base = base.split(" - ").next().unwrap_or(base);
    let title = match norm(base) {
        n if n.is_empty() => t.title.to_lowercase(),
        n => n,
    };
    let main = t.artists.first().map(|a| a.name.clone()).or_else(|| split_artists(&t.artist).into_iter().next()).unwrap_or_default();
    format!("{title}|{}", norm(&main))
}

/// Clé de l'artiste principal d'un morceau.
fn main_artist(t: &Track) -> String {
    artists_of(t).into_iter().next().map(|a| a.0).unwrap_or_default()
}

fn decay(now: u64, at: u64, half_life_days: f64) -> f64 {
    0.5f64.powf(now.saturating_sub(at) as f64 / DAY as f64 / half_life_days)
}

#[derive(Default)]
struct TrackTally {
    plays: u32,
    last: u64,
    weight: f64,
}

/// Résume tes goûts. `listens` : écoutes triées ; `skips` : sauts ; `tracks` : description des
/// morceaux du journal.
pub fn profile(listens: &[Listen], skips: &[Listen], tracks: &HashMap<String, Track>, coll: &CollectionData, now: u64) -> Profile {
    let mut p = Profile::default();
    // Score brut par artiste : (nom affiché, identifiant, score).
    let mut raw: HashMap<String, (String, Option<String>, f64)> = HashMap::new();
    let mut add = |t: &Track, w: f64, known_artists: &mut HashSet<String>| {
        for (i, (key, name, id)) in artists_of(t).into_iter().enumerate() {
            // Les invités comptent moitié moins que l'artiste principal.
            let w = if i == 0 { w } else { w * 0.5 };
            known_artists.insert(key.clone());
            let e = raw.entry(key).or_insert_with(|| (name, None, 0.0));
            if e.1.is_none() {
                e.1 = id;
            }
            e.2 += w;
        }
    };
    let mut tally: HashMap<String, TrackTally> = HashMap::new();
    let mut by_key: HashMap<String, &Track> = HashMap::new();

    // Écoutes : temps écouté (3 min = 1), le récent compte plus (demi-vie de 30 jours).
    for l in listens {
        let Some(t) = tracks.get(&l.id) else { continue };
        add(t, l.secs.min(600.0) / 180.0 * decay(now, l.at, 30.0), &mut p.known_artists);
        let key = track_key(t);
        if now.saturating_sub(l.at) < RECENT {
            p.recent.insert(key.clone());
        }
        let e = tally.entry(key.clone()).or_default();
        e.plays += 1;
        e.last = e.last.max(l.at);
        e.weight += decay(now, l.at, 90.0);
        p.known.insert(key.clone());
        by_key.insert(key, t);
    }
    // Sauts : pénalité pour l'artiste ; un saut de moins d'un mois (ou deux de moins de deux
    // mois) écarte le morceau.
    let mut skip_weight: HashMap<String, f64> = HashMap::new();
    for s in skips {
        let Some(t) = tracks.get(&s.id) else { continue };
        let d = decay(now, s.at, 30.0);
        add(t, -0.4 * d, &mut p.known_artists);
        *skip_weight.entry(track_key(t)).or_default() += d;
    }
    p.skipped = skip_weight.into_iter().filter(|(_, w)| *w >= 0.5).map(|(k, _)| k).collect();
    // Likes : les plus récents (en tête) comptent plus. Un import de centaines de likes ne
    // noie pas le reste : le score final est logarithmique.
    for (i, t) in coll.liked.iter().enumerate() {
        let w = 1.0 + 1.5 * 0.97f64.powi(i.min(200) as i32);
        add(t, w, &mut p.known_artists);
        let key = track_key(t);
        p.liked.insert(key.clone());
        p.known.insert(key.clone());
        tally.entry(key.clone()).or_default().weight += 1.5 * 0.97f64.powi(i.min(200) as i32);
        by_key.entry(key).or_insert(t);
    }
    for pl in &coll.playlists {
        for t in &pl.tracks {
            add(t, 0.4, &mut p.known_artists);
            p.known.insert(track_key(t));
        }
    }

    let mut artists: Vec<Fav> = raw
        .into_iter()
        .filter(|(_, (_, _, s))| *s > 0.05)
        .map(|(key, (name, id, s))| Fav { key, name, id, score: s.ln_1p() })
        .collect();
    artists.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.key.cmp(&b.key)));
    p.artists = artists;

    // Préférés : les plus écoutés et likés, sauf ceux écoutés ces deux derniers jours.
    let mut fav: Vec<(&String, &TrackTally)> = tally
        .iter()
        .filter(|(k, t)| !p.skipped.contains(*k) && now.saturating_sub(t.last) > 2 * DAY && by_key.contains_key(*k))
        .collect();
    fav.sort_by(|a, b| b.1.weight.total_cmp(&a.1.weight).then_with(|| a.0.cmp(b.0)));
    p.familiar = fav.iter().take(60).map(|(k, _)| by_key[*k].clone()).collect();

    // Redécouvertes : beaucoup écoutés mais pas depuis un mois, ou likés et pas écoutés
    // depuis deux mois (ou jamais : likes importés).
    let mut old: Vec<(&String, f64)> = tally
        .iter()
        .filter(|(k, _)| !p.skipped.contains(*k) && by_key.contains_key(*k))
        .filter_map(|(k, t)| {
            let age = now.saturating_sub(t.last);
            if t.plays >= 3 && age > 30 * DAY {
                Some((k, f64::from(t.plays)))
            } else if p.liked.contains(k) && (t.plays == 0 || age > 60 * DAY) {
                Some((k, 1.0 + t.weight))
            } else {
                None
            }
        })
        .collect();
    old.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    p.rediscover = old.iter().take(80).map(|(k, _)| by_key[*k].clone()).collect();
    p
}

/// Profil à partir du journal des écoutes et de la collection.
pub fn profile_from(log: &ListenLog, coll: &CollectionData, now: u64) -> Profile {
    log.with_data(|listens, skips, tracks| profile(listens, skips, tracks, coll, now))
}

// ---------- Hasard reproductible (le même toute la journée) ----------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // SplitMix64.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn shuffle<T>(&mut self, list: &mut [T]) {
        for i in (1..list.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            list.swap(i, j);
        }
    }
}

// ---------- Plan du jour ----------

/// Les sections du jour, et les artistes qui les nourrissent.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub sections: Vec<Section>,
    because: Vec<Fav>,
    mix_seeds: Vec<Fav>,
    related_seeds: Vec<Fav>,
    release_artists: Vec<Fav>,
}

/// Choisit les sections du jour. `day` : numéro du jour local (change la rotation).
pub fn plan(p: &Profile, day: i64) -> Plan {
    let mut plan = Plan::default();
    let mut rng = Rng(day as u64);
    let rediscover = || {
        let mut pool: Vec<Track> = p.rediscover.iter().take(60).cloned().collect();
        Rng(day as u64 ^ 0x5EED).shuffle(&mut pool);
        let mut seen = HashSet::new();
        let mut per_artist: HashMap<String, usize> = HashMap::new();
        let tracks: Vec<Track> = pool
            .into_iter()
            .filter(|t| seen.insert(track_key(t)))
            .filter(|t| {
                let n = per_artist.entry(main_artist(t)).or_default();
                *n += 1;
                *n <= 2
            })
            .take(15)
            .collect();
        Section { tracks, ready: true, ..Section::stub("rediscover", Kind::Rediscover) }
    };
    if p.artists.is_empty() {
        // Nouvel utilisateur : ce qui marche en ce moment. Dès le premier artiste écouté ou le
        // premier like, les sections personnelles apparaissent.
        plan.sections = vec![
            Section::stub("charts", Kind::Charts),
            Section::stub("chart-artists", Kind::ChartArtists),
            Section::stub("chart-albums", Kind::ChartAlbums),
        ];
        let r = rediscover();
        if r.tracks.len() >= 4 {
            plan.sections.push(r);
        }
        return plan;
    }
    let top = &p.artists;
    // « Parce que tu écoutes » : tourne chaque jour parmi tes six artistes préférés.
    let pool = &top[..top.len().min(6)];
    let count = match top.len() {
        1 => 1,
        2..=5 => 2,
        _ => 3,
    };
    let step = if pool.len() >= 6 { 2 } else { 1 };
    let start = day.rem_euclid(pool.len() as i64) as usize;
    for k in 0..count.min(pool.len()) {
        let fav = &pool[(start + k * step) % pool.len()];
        if !plan.because.iter().any(|f| f.key == fav.key) {
            plan.because.push(fav.clone());
        }
    }
    // Mix : tes trois artistes préférés, et un autre tiré au sort parmi les suivants.
    plan.mix_seeds = top.iter().take(3).cloned().collect();
    if top.len() > 3 {
        let rest = &top[3..top.len().min(10)];
        plan.mix_seeds.push(rest[(rng.next() % rest.len() as u64) as usize].clone());
    }
    plan.related_seeds = top.iter().take(5).cloned().collect();
    plan.release_artists = top.iter().take(8).cloned().collect();

    plan.sections.push(Section::stub(format!("mix:{day}"), Kind::Mix));
    for fav in &plan.because {
        let mut s = Section::stub(format!("because:{}", fav.key), Kind::Because);
        s.artist = Some(ArtistRef { name: fav.name.clone(), id: fav.id.clone() });
        plan.sections.push(s);
    }
    plan.sections.push(Section::stub("releases", Kind::Releases));
    plan.sections.push(Section::stub("artists", Kind::Artists));
    let r = rediscover();
    if r.tracks.len() >= 4 {
        plan.sections.push(r);
    }
    plan.sections.push(Section::stub("charts", Kind::Charts));
    plan
}

// ---------- Assemblage ----------

/// Quels morceaux une section accepte.
#[derive(Clone, Copy, PartialEq)]
enum Filter {
    /// Découverte : rien de connu, rien de sauté.
    New,
    /// Assoupli (trop peu de découvertes) : ni liké, ni écouté récemment, ni sauté.
    NotRecent,
    /// Top du pays : seulement ni liké, ni récent, ni sauté.
    Charts,
    /// Tout (part connue du mix : déjà filtrée par le profil).
    Any,
}

/// Retient au plus `max` morceaux, `per_artist` par artiste principal, jamais deux fois le même
/// sur la page (`seen`), en écartant ceux d'artistes `banned`.
fn pick(
    candidates: &[Track],
    p: &Profile,
    filter: Filter,
    seen: &mut HashSet<String>,
    per_artist: usize,
    max: usize,
    banned: &HashSet<String>,
) -> Vec<Track> {
    pick_after(&[], candidates, p, filter, seen, per_artist, max, banned)
}

/// Comme [`pick`], à la suite de morceaux déjà retenus pour la section (`before`, comptés dans
/// la limite par artiste).
#[allow(clippy::too_many_arguments)]
fn pick_after(
    before: &[Track],
    candidates: &[Track],
    p: &Profile,
    filter: Filter,
    seen: &mut HashSet<String>,
    per_artist: usize,
    max: usize,
    banned: &HashSet<String>,
) -> Vec<Track> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for t in before {
        *counts.entry(main_artist(t)).or_default() += 1;
    }
    let mut out = Vec::new();
    for t in candidates {
        if out.len() >= max {
            break;
        }
        let key = track_key(t);
        if seen.contains(&key) || p.skipped.contains(&key) {
            continue;
        }
        let ok = match filter {
            Filter::New => !p.known.contains(&key),
            Filter::NotRecent | Filter::Charts => !p.liked.contains(&key) && !p.recent.contains(&key),
            Filter::Any => true,
        };
        if !ok {
            continue;
        }
        let artists = artists_of(t);
        if artists.iter().any(|a| banned.contains(&a.0)) {
            continue;
        }
        let n = counts.entry(artists.first().map(|a| a.0.clone()).unwrap_or_default()).or_default();
        if *n >= per_artist {
            continue;
        }
        *n += 1;
        seen.insert(key);
        out.push(t.clone());
    }
    out
}

/// Découvertes d'abord, complétées si besoin par des titres connus mais pas récents.
fn pick_new(candidates: &[Track], p: &Profile, seen: &mut HashSet<String>, per_artist: usize, max: usize, banned: &HashSet<String>) -> Vec<Track> {
    let mut out = pick(candidates, p, Filter::New, seen, per_artist, max, banned);
    if out.len() < max.min(8) {
        let more = pick_after(&out, candidates, p, Filter::NotRecent, seen, per_artist, max - out.len(), banned);
        out.extend(more);
    }
    out
}

/// « Parce que tu écoutes X » : la radio de X, sans les titres de X lui-même.
fn because(radio: &[Track], fav: &Fav, p: &Profile, seen: &mut HashSet<String>) -> Vec<Track> {
    let banned = HashSet::from([fav.key.clone()]);
    pick_new(radio, p, seen, 2, 15, &banned)
}

/// Artistes proches de tes préférés que tu n'as jamais écoutés ; proches de plusieurs d'entre
/// eux = plus haut dans la liste.
fn suggestions(related: &[(Fav, Vec<ArtistCard>)], p: &Profile) -> Vec<Suggestion> {
    let mut scored: Vec<(Suggestion, f64, usize)> = Vec::new();
    for (fav, cards) in related {
        for (rank, c) in cards.iter().enumerate() {
            let key = c.name.trim().to_lowercase();
            if p.known_artists.contains(&key) {
                continue;
            }
            // Les premiers artistes liés sont les plus proches.
            let w = fav.score / (1.0 + rank as f64 * 0.15);
            match scored.iter_mut().find(|s| s.0.id == c.id) {
                Some(s) => s.1 += w,
                None => scored.push((
                    Suggestion { id: c.id.clone(), name: c.name.clone(), picture: c.picture.clone(), near: Some(fav.name.clone()) },
                    w,
                    scored.len(),
                )),
            }
        }
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.2.cmp(&b.2)));
    scored.into_iter().take(12).map(|s| s.0).collect()
}

/// Date « AAAA-MM-JJ » d'un jour (numéro de jour depuis 1970).
fn date_of(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Sorties de moins de 60 jours de tes artistes, de la plus récente à la plus ancienne.
fn new_releases(found: &[(Fav, Vec<(Album, String)>)], today: i64) -> Vec<Release> {
    let since = date_of(today - NEW_RELEASE_DAYS);
    let until = date_of(today + 1);
    let mut out: Vec<Release> = Vec::new();
    for (fav, albums) in found {
        for (a, date) in albums {
            if date.as_str() < since.as_str() || date.as_str() > until.as_str() {
                continue;
            }
            // Même sortie en plusieurs éditions (explicite / clean…) : une seule.
            let dup = out.iter().any(|r| r.album.id == a.id || (r.artist == fav.name && norm(&r.album.title) == norm(&a.title)));
            if !dup {
                out.push(Release { album: a.clone(), artist: fav.name.clone(), date: date.clone() });
            }
        }
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.album.title.cmp(&b.album.title)));
    out.truncate(12);
    out
}

/// Mix du jour : environ 40 titres, dont un tiers que tu aimes déjà, le reste à découvrir,
/// mélangés (le même toute la journée).
fn mix(radios: &[Vec<Track>], tops: &[Vec<Track>], p: &Profile, seen: &mut HashSet<String>, day: i64) -> Vec<Track> {
    let mut rng = Rng(day as u64 ^ 0x00D1_5C0F);
    // Radios entremêlées (un titre de chaque à tour de rôle), puis les artistes à découvrir.
    let mut pool: Vec<Track> = Vec::new();
    let longest = radios.iter().chain(tops).map(Vec::len).max().unwrap_or(0);
    for i in 0..longest {
        for list in radios.iter().chain(tops) {
            if let Some(t) = list.get(i) {
                pool.push(t.clone());
            }
        }
    }
    let mut fresh = pick_new(&pool, p, seen, 3, 28, &HashSet::new());
    let mut known: Vec<Track> = p.familiar.iter().take(40).cloned().collect();
    rng.shuffle(&mut known);
    let mut known = pick(&known, p, Filter::Any, seen, 2, 12, &HashSet::new());
    rng.shuffle(&mut fresh);
    // Un titre connu tous les trois titres environ, pas tout de suite au début.
    let mut out = Vec::with_capacity(fresh.len() + known.len());
    let mut fresh = fresh.into_iter();
    let mut known_it = known.drain(..);
    let mut i = 0;
    loop {
        let next = if i % 3 == 2 { known_it.next().or_else(|| fresh.next()) } else { fresh.next().or_else(|| known_it.next()) };
        match next {
            Some(t) => out.push(t),
            None => break,
        }
        i += 1;
    }
    out
}

// ---------- Cache et calcul ----------

#[derive(Default, Serialize, Deserialize)]
struct CacheFile {
    #[serde(default)]
    sections: Vec<Section>,
    /// Identifiants Deezer d'artistes retrouvés par leur nom.
    #[serde(default)]
    ids: HashMap<String, String>,
}

pub struct Discover {
    path: PathBuf,
    cache: Mutex<CacheFile>,
    /// Dernier calcul lancé (sections visées, heure) : pas deux fois le même à la suite.
    last_build: Mutex<Option<(Vec<String>, Instant)>>,
}

/// Ce que renvoie une requête Deezer lancée par [`Discover::build`].
enum Fetched {
    Radio(String, Vec<Track>),
    Related(String, Vec<ArtistCard>),
    Releases(String, Vec<(Album, String)>),
    Charts(Vec<Track>),
    ChartArtists(Vec<ArtistCard>),
    ChartAlbums(Vec<(Album, String)>),
    Top(Vec<Track>),
}

impl Discover {
    pub fn load(path: &Path) -> Self {
        let cache = fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path: path.to_owned(), cache: Mutex::new(cache), last_build: Mutex::new(None) }
    }

    fn save(&self) {
        let cache = self.cache.lock();
        if let Err(e) = save_json(&self.path, &*cache) {
            eprintln!("découvrir : cache non enregistré : {e}");
        }
    }

    /// La page à afficher tout de suite : chaque section prend son contenu en cache s'il en a un
    /// (même un peu ancien). Renvoie aussi s'il faut recalculer (contenu absent ou périmé).
    pub fn page(&self, plan: &Plan, now: u64, force: bool) -> (Vec<Section>, bool) {
        let cache = self.cache.lock();
        let mut stale = false;
        let sections = plan
            .sections
            .iter()
            .map(|s| {
                if s.ready {
                    return s.clone();
                }
                match cache.sections.iter().find(|c| c.id == s.id && c.ready) {
                    Some(c) => {
                        stale |= force || now.saturating_sub(c.at) > c.ttl();
                        Section { artist: s.artist.clone().or_else(|| c.artist.clone()), ..c.clone() }
                    }
                    None => {
                        stale = true;
                        s.clone()
                    }
                }
            })
            .collect();
        (sections, stale)
    }

    /// Un calcul identique vient d'être lancé (moins de 30 s) : inutile de le refaire.
    pub fn claim(&self, plan: &Plan) -> bool {
        let ids: Vec<String> = plan.sections.iter().map(|s| s.id.clone()).collect();
        let mut last = self.last_build.lock();
        if last.as_ref().is_some_and(|(l, at)| *l == ids && at.elapsed() < Duration::from_secs(30)) {
            return false;
        }
        *last = Some((ids, Instant::now()));
        true
    }

    /// Calcule les sections du plan (Deezer), et les donne une à une à `emit` dès qu'elles sont
    /// prêtes. Une requête qui échoue laisse sa section vide, sans bloquer les autres.
    pub async fn build(&self, deezer: Arc<DeezerSource>, plan: Plan, p: &Profile, now: u64, day: i64, emit: impl Fn(&Section) + Send + Sync) {
        // Identifiants Deezer manquants (artistes de fichiers locaux…) : retrouvés par le nom.
        let mut favs: Vec<Fav> = Vec::new();
        for f in plan.because.iter().chain(&plan.mix_seeds).chain(&plan.related_seeds).chain(&plan.release_artists) {
            if !favs.iter().any(|x| x.key == f.key) {
                favs.push(f.clone());
            }
        }
        let missing: Vec<Fav> = {
            let cache = self.cache.lock();
            for f in favs.iter_mut().filter(|f| f.id.is_none()) {
                f.id = cache.ids.get(&f.key).cloned();
            }
            favs.iter().filter(|f| f.id.is_none()).cloned().collect()
        };
        if !missing.is_empty() {
            let mut set = tokio::task::JoinSet::new();
            for f in missing {
                let dz = deezer.clone();
                set.spawn(async move { (f.key.clone(), dz.find_artist_id(&f.name).await) });
            }
            while let Some(Ok((key, found))) = set.join_next().await {
                if let Ok(Some(id)) = found {
                    self.cache.lock().ids.insert(key.clone(), id.clone());
                    if let Some(f) = favs.iter_mut().find(|f| f.key == key) {
                        f.id = Some(id);
                    }
                }
            }
        }
        let with_id = |list: &[Fav]| -> Vec<Fav> {
            list.iter().filter_map(|f| favs.iter().find(|x| x.key == f.key && x.id.is_some()).cloned()).collect()
        };
        let (because_favs, mix_favs, related_favs, release_favs) =
            (with_id(&plan.because), with_id(&plan.mix_seeds), with_id(&plan.related_seeds), with_id(&plan.release_artists));
        let has = |kind: Kind| plan.sections.iter().any(|s| s.kind == kind);

        // Phase 1 : toutes les requêtes en parallèle (une vingtaine, sous le quota de Deezer).
        let mut set = tokio::task::JoinSet::new();
        let mut radios_wanted: Vec<String> = Vec::new();
        for f in because_favs.iter().chain(&mix_favs) {
            let id = f.id.clone().unwrap_or_default();
            if !radios_wanted.contains(&id) {
                radios_wanted.push(id.clone());
                let dz = deezer.clone();
                set.spawn(async move { Fetched::Radio(id.clone(), dz.artist_radio(&id, 40).await.unwrap_or_default()) });
            }
        }
        if has(Kind::Artists) || has(Kind::Mix) {
            for f in &related_favs {
                let (dz, key, id) = (deezer.clone(), f.key.clone(), f.id.clone().unwrap_or_default());
                set.spawn(async move { Fetched::Related(key, dz.related_artists(&id, 10).await.unwrap_or_default()) });
            }
        }
        if has(Kind::Releases) {
            for f in &release_favs {
                let (dz, key, id) = (deezer.clone(), f.key.clone(), f.id.clone().unwrap_or_default());
                set.spawn(async move { Fetched::Releases(key, dz.artist_releases(&id).await.unwrap_or_default()) });
            }
        }
        if has(Kind::Charts) {
            let dz = deezer.clone();
            set.spawn(async move {
                use crate::MusicSource;
                Fetched::Charts(dz.featured(60).await.unwrap_or_default())
            });
        }
        if has(Kind::ChartArtists) {
            let dz = deezer.clone();
            set.spawn(async move { Fetched::ChartArtists(dz.chart_artists(16).await.unwrap_or_default()) });
        }
        if has(Kind::ChartAlbums) {
            let dz = deezer.clone();
            set.spawn(async move { Fetched::ChartAlbums(dz.chart_albums(16).await.unwrap_or_default()) });
        }
        let mut radios: HashMap<String, Vec<Track>> = HashMap::new();
        let mut related: HashMap<String, Vec<ArtistCard>> = HashMap::new();
        let mut releases: HashMap<String, Vec<(Album, String)>> = HashMap::new();
        let (mut charts, mut chart_artists, mut chart_albums) = (Vec::new(), Vec::new(), Vec::new());
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Fetched::Radio(id, t)) => drop(radios.insert(id, t)),
                Ok(Fetched::Related(k, a)) => drop(related.insert(k, a)),
                Ok(Fetched::Releases(k, a)) => drop(releases.insert(k, a)),
                Ok(Fetched::Charts(t)) => charts = t,
                Ok(Fetched::ChartArtists(a)) => chart_artists = a,
                Ok(Fetched::ChartAlbums(a)) => chart_albums = a,
                _ => {}
            }
        }

        // Assemblage, dans l'ordre où les sections se partagent les morceaux : chacun ne paraît
        // qu'une fois sur la page.
        let mut seen: HashSet<String> = HashSet::new();
        let finish = |mut s: Section| {
            s.ready = true;
            s.at = now;
            emit(&s);
            self.store(s);
        };
        let stub = |kind: Kind| plan.sections.iter().find(|s| s.kind == kind).cloned();
        if let Some(r) = stub(Kind::Rediscover) {
            seen.extend(r.tracks.iter().map(track_key));
        }
        for s in plan.sections.iter().filter(|s| s.kind == Kind::Because) {
            let key = s.id.trim_start_matches("because:");
            let tracks = match because_favs.iter().find(|f| f.key == key) {
                Some(f) => because(radios.get(f.id.as_deref().unwrap_or("")).map_or(&[][..], Vec::as_slice), f, p, &mut seen),
                None => Vec::new(),
            };
            let artist = because_favs.iter().find(|f| f.key == key).map(|f| ArtistRef { name: f.name.clone(), id: f.id.clone() });
            finish(Section { tracks, artist: artist.or_else(|| s.artist.clone()), ..s.clone() });
        }
        let related_list: Vec<(Fav, Vec<ArtistCard>)> =
            related_favs.iter().filter_map(|f| related.remove(&f.key).map(|a| (f.clone(), a))).collect();
        let suggested = suggestions(&related_list, p);
        if let Some(s) = stub(Kind::Artists) {
            finish(Section { artists: suggested.clone(), ..s });
        }
        if let Some(s) = stub(Kind::Releases) {
            let found: Vec<(Fav, Vec<(Album, String)>)> =
                release_favs.iter().filter_map(|f| releases.remove(&f.key).map(|a| (f.clone(), a))).collect();
            finish(Section { albums: new_releases(&found, (now / DAY) as i64), ..s });
        }
        if let Some(s) = stub(Kind::ChartArtists) {
            let artists = chart_artists.into_iter().map(|c| Suggestion { id: c.id, name: c.name, picture: c.picture, near: None }).collect();
            finish(Section { artists, ..s });
        }
        if let Some(s) = stub(Kind::ChartAlbums) {
            let albums = chart_albums.into_iter().map(|(album, artist)| Release { album, artist, date: String::new() }).collect();
            finish(Section { albums, ..s });
        }

        // Phase 2 : le mix, nourri aussi des titres phares des artistes à découvrir.
        if let Some(s) = stub(Kind::Mix) {
            let mut set = tokio::task::JoinSet::new();
            for a in suggested.iter().take(4) {
                let (dz, id) = (deezer.clone(), a.id.clone());
                set.spawn(async move { Fetched::Top(dz.artist_top(&id, 5).await.unwrap_or_default()) });
            }
            let mut tops = Vec::new();
            while let Some(joined) = set.join_next().await {
                if let Ok(Fetched::Top(t)) = joined {
                    tops.push(t);
                }
            }
            let mix_radios: Vec<Vec<Track>> = mix_favs.iter().filter_map(|f| radios.get(f.id.as_deref()?).cloned()).collect();
            finish(Section { tracks: mix(&mix_radios, &tops, p, &mut seen, day), ..s });
        }
        if let Some(s) = stub(Kind::Charts) {
            let tracks = if p.artists.is_empty() {
                pick(&charts, p, Filter::Any, &mut seen, 3, 50, &HashSet::new())
            } else {
                pick(&charts, p, Filter::Charts, &mut seen, 2, 20, &HashSet::new())
            };
            finish(Section { tracks, ..s });
        }
        self.save();
    }

    /// Garde une section calculée (une section vide après un échec ne remplace pas une pleine).
    fn store(&self, s: Section) {
        let mut cache = self.cache.lock();
        match cache.sections.iter_mut().find(|c| c.id == s.id) {
            Some(c) if s.is_empty() && !c.is_empty() => {}
            Some(c) => *c = s,
            None => cache.sections.push(s),
        }
        // Le cache ne garde que les sections récentes (le mix d'hier, un artiste d'il y a longtemps…).
        let now = cache.sections.iter().map(|s| s.at).max().unwrap_or(0);
        cache.sections.retain(|c| now.saturating_sub(c.at) < 3 * DAY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Location, Playlist};

    const NOW: u64 = 1_800_000_000_000;

    fn track(id: &str, title: &str, artist: &str) -> Track {
        Track {
            id: id.into(),
            source: "deezer".into(),
            title: title.into(),
            artist: artist.into(),
            album: None,
            album_id: None,
            duration: Some(200.0),
            track_number: None,
            disc_number: None,
            artwork: None,
            has_cover: false,
            location: Location::Remote { remote_id: id.into() },
            artists: vec![ArtistRef { name: artist.into(), id: Some(format!("id-{artist}")) }],
        }
    }

    fn listen(id: &str, days_ago: u64, secs: f64) -> Listen {
        Listen { at: NOW - days_ago * DAY, id: id.into(), secs }
    }

    fn library(list: &[Track]) -> HashMap<String, Track> {
        list.iter().map(|t| (t.id.clone(), t.clone())).collect()
    }

    #[test]
    fn keys_ignore_versions_and_sources() {
        let a = track("deezer:1", "Get Lucky (Radio Edit)", "Daft Punk");
        let mut b = track("youtube:x", "Get Lucky", "Daft Punk");
        b.artists.clear();
        b.artist = "Daft Punk, Pharrell Williams".into();
        assert_eq!(track_key(&a), track_key(&b));
        assert_ne!(track_key(&a), track_key(&track("deezer:2", "Get Lucky", "Justice")));
    }

    #[test]
    fn recent_listening_outweighs_old_listening() {
        let a = track("a", "A1", "Alpha");
        let b = track("b", "B1", "Beta");
        let tracks = library(&[a.clone(), b.clone()]);
        // Beta : beaucoup écouté il y a un an ; Alpha : moins, mais cette semaine.
        let mut listens: Vec<Listen> = (0..20).map(|_| listen("b", 365, 200.0)).collect();
        listens.extend((0..5).map(|d| listen("a", d, 200.0)));
        let p = profile(&listens, &[], &tracks, &CollectionData::default(), NOW);
        assert_eq!(p.artists[0].name, "Alpha");
        assert!(p.recent.contains(&track_key(&a)));
        assert!(!p.recent.contains(&track_key(&b)));
        // Beaucoup écouté et plus depuis un mois : à redécouvrir.
        assert_eq!(p.rediscover.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["b"]);
    }

    #[test]
    fn likes_playlists_and_skips_shape_the_profile() {
        let liked = track("l", "Liked", "Gamma");
        let skipped = track("s", "Skipped", "Delta");
        let in_pl = track("p", "Listed", "Epsilon");
        let tracks = library(&[skipped.clone()]);
        let coll = CollectionData {
            liked: vec![liked.clone()],
            playlists: vec![Playlist { id: "pl".into(), name: "x".into(), tracks: vec![in_pl.clone()], created: 0, cover: None }],
            history: Vec::new(),
        };
        let skips = vec![listen("s", 3, 4.0), listen("s", 2, 5.0)];
        let p = profile(&[], &skips, &tracks, &coll, NOW);
        let names: Vec<&str> = p.artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Gamma", "Epsilon"], "un artiste seulement sauté n'est pas un favori");
        assert!(p.skipped.contains(&track_key(&skipped)));
        assert!(p.known_artists.contains("delta"));
        assert!(p.known.contains(&track_key(&in_pl)) && p.liked.contains(&track_key(&liked)));
        // Liké mais jamais écouté (import) : proposé en redécouverte.
        assert_eq!(p.rediscover.len(), 1);
    }

    #[test]
    fn picks_skip_known_tracks_duplicates_and_cap_artists() {
        let known = track("k", "Known", "Alpha");
        let tracks = library(&[known.clone()]);
        let p = profile(&[listen("k", 40, 200.0)], &[], &tracks, &CollectionData::default(), NOW);
        let radio = vec![
            known.clone(),
            track("1", "One", "Alpha"),
            track("2", "Two", "Beta"),
            track("3", "Three", "Beta"),
            track("4", "Four", "Beta"),
            track("5", "One (Remastered)", "Alpha"),
            track("6", "Six", "Gamma"),
        ];
        let fav = p.artists[0].clone();
        let mut seen = HashSet::new();
        let got = because(&radio, &fav, &p, &mut seen);
        // Pas Alpha (l'artiste de la section), au plus deux Beta.
        assert_eq!(got.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["2", "3", "6"]);
        // Déjà sur la page : pas reproposé ailleurs.
        let again = pick(&radio, &p, Filter::New, &mut seen, 5, 10, &HashSet::new());
        assert_eq!(again.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["1", "4"]);
    }

    #[test]
    fn new_users_get_charts_then_personal_sections() {
        let empty = plan(&Profile::default(), 100);
        let kinds: Vec<Kind> = empty.sections.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [Kind::Charts, Kind::ChartArtists, Kind::ChartAlbums]);

        let names = ["A", "B", "C", "D", "E", "F", "G"];
        let list: Vec<Track> = names.iter().map(|n| track(n, n, n)).collect();
        let tracks = library(&list);
        let listens: Vec<Listen> =
            names.iter().enumerate().flat_map(|(i, n)| (0..(10 - i)).map(move |_| listen(n, 1, 200.0))).collect();
        let p = profile(&listens, &[], &tracks, &CollectionData::default(), NOW);
        let today = plan(&p, 100);
        let kinds: Vec<Kind> = today.sections.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [Kind::Mix, Kind::Because, Kind::Because, Kind::Because, Kind::Releases, Kind::Artists, Kind::Charts]);
        // La rotation change d'un jour à l'autre, jamais deux fois le même artiste.
        let tomorrow = plan(&p, 101);
        let ids = |pl: &Plan| pl.because.iter().map(|f| f.key.clone()).collect::<Vec<_>>();
        assert_ne!(ids(&today), ids(&tomorrow));
        for pl in [&today, &tomorrow] {
            let set: HashSet<_> = ids(pl).into_iter().collect();
            assert_eq!(set.len(), 3);
        }
        assert_eq!(today.mix_seeds.len(), 4);
    }

    #[test]
    fn suggestions_skip_known_artists_and_favor_shared_neighbours() {
        let p = Profile { known_artists: HashSet::from(["known".to_owned()]), ..Profile::default() };
        let fav = |n: &str, s: f64| Fav { key: n.to_lowercase(), name: n.into(), id: Some(n.into()), score: s };
        let card = |n: &str| ArtistCard { id: n.into(), name: n.into(), picture: None };
        let related = vec![
            (fav("A", 2.0), vec![card("Known"), card("X"), card("Y")]),
            (fav("B", 1.0), vec![card("Z"), card("Y")]),
        ];
        let got = suggestions(&related, &p);
        assert_eq!(got.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Y", "X", "Z"]);
        assert_eq!(got[0].near.as_deref(), Some("A"));
    }

    #[test]
    fn releases_keep_the_last_60_days_newest_first() {
        let today = crate::stats::days_from_civil(2026, 11, 11);
        let album = |id: &str, title: &str| Album { id: id.into(), title: title.into(), cover: None, year: None, kind: "album".into() };
        let fav = Fav { key: "a".into(), name: "A".into(), id: None, score: 1.0 };
        let found = vec![(
            fav,
            vec![
                (album("1", "Old"), "2026-01-01".to_owned()),
                (album("2", "New"), "2026-11-07".to_owned()),
                (album("3", "New"), "2026-11-07".to_owned()),
                (album("4", "Recent"), "2026-10-01".to_owned()),
                (album("5", "Future"), "2027-02-01".to_owned()),
            ],
        )];
        let got = new_releases(&found, today);
        assert_eq!(got.iter().map(|r| r.album.id.as_str()).collect::<Vec<_>>(), ["2", "4"]);
    }

    #[test]
    fn the_mix_blends_new_and_familiar_tracks() {
        let fam: Vec<Track> = (0..20).map(|i| track(&format!("f{i}"), &format!("Fav {i}"), &format!("Old{}", i % 10))).collect();
        let mut listens = Vec::new();
        for t in &fam {
            listens.push(Listen { at: NOW - 10 * DAY, id: t.id.clone(), secs: 200.0 });
        }
        let p = profile(&listens, &[], &library(&fam), &CollectionData::default(), NOW);
        let radio: Vec<Track> = (0..40).map(|i| track(&format!("r{i}"), &format!("New {i}"), &format!("N{}", i % 15))).collect();
        let mut seen = HashSet::new();
        let a = mix(&[radio.clone()], &[], &p, &mut seen, 7);
        assert_eq!(a.len(), 40);
        let familiar = a.iter().filter(|t| t.id.starts_with('f')).count();
        assert_eq!(familiar, 12);
        // Le même toute la journée.
        let b = mix(&[radio], &[], &p, &mut HashSet::new(), 7);
        assert_eq!(a, b);
    }

    /// Calcul complet contre la vraie API Deezer : `cargo test -p yato-core real_page -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn real_page() {
        let dir = std::env::temp_dir().join("yato-discover-test");
        let _ = fs::remove_file(dir.join("discover.json"));
        let ytdlp = crate::ytdlp::YtDlp::new(dir.join("yt-dlp"), reqwest::Client::new());
        let yt = Arc::new(crate::sources::YouTubeMusicSource::new(dir.join("yt"), ytdlp).unwrap());
        let dz = Arc::new(DeezerSource::new(crate::http_client(), yt));
        let liked = vec![track("deezer:1", "Get Lucky", "Daft Punk"), track("deezer:2", "D.A.N.C.E.", "Justice")];
        let mut liked = liked;
        liked[0].artists[0].id = Some("27".into());
        liked[1].artists[0].id = None;
        let coll = CollectionData { liked, ..CollectionData::default() };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        let p = profile(&[], &[], &HashMap::new(), &coll, now);
        let pl = plan(&p, (now / DAY) as i64);
        let engine = Discover::load(&dir.join("discover.json"));
        let started = Instant::now();
        let all = Mutex::new(HashSet::new());
        engine
            .build(dz, pl, &p, now, (now / DAY) as i64, |s| {
                println!("{:>5} ms  {:<24} {} titres, {} albums, {} artistes", started.elapsed().as_millis(), s.id, s.tracks.len(), s.albums.len(), s.artists.len());
                for t in &s.tracks {
                    assert!(all.lock().insert(track_key(t)), "doublon : {} - {}", t.artist, t.title);
                }
            })
            .await;
        let (sections, stale) = engine.page(&plan(&p, (now / DAY) as i64), now, false);
        assert!(!stale && sections.iter().all(|s| s.ready));
    }
}
