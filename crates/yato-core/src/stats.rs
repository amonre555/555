//! Statistiques d'écoute : chaque écoute est notée dans un journal sur disque, puis résumée sur
//! une période (un mois, une année, depuis toujours) pour la page Stats et le récap de l'année.
//!
//! - [`ListenTracker`] suit le lecteur et dit quand une écoute est terminée, avec la durée
//!   réellement écoutée (pauses et avances rapides exclues).
//! - [`ListenLog`] garde les écoutes dans un fichier JSON Lines (`listens.jsonl`) : une ligne par
//!   écoute, ajoutée à la fin sans réécrire le fichier. Chaque morceau n'y est décrit qu'une fois
//!   (ligne `track`), les écoutes ne gardent que son identifiant : le journal reste léger, même
//!   après des années (~70 octets par écoute).
//! - [`compute`] calcule les statistiques d'une période.

use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::{Played, Result, Track, model::split_artists};

/// Une écoute : quand elle a commencé (millisecondes depuis 1970), quel morceau, et combien de
/// secondes ont vraiment été écoutées.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listen {
    pub at: u64,
    pub id: String,
    /// Secondes réellement écoutées.
    pub secs: f64,
}

/// Seuil au-delà duquel une écoute compte : 30 s, ou la moitié du morceau s'il est court.
pub fn threshold(duration: Option<f64>) -> f64 {
    match duration {
        Some(d) if d > 0.0 => (d / 2.0).min(30.0),
        _ => 30.0,
    }
}

// ---------- Suivi du lecteur ----------

/// Plus grand pas d'avance compté entre deux relevés (ils arrivent toutes les 250 ms) : au-delà,
/// c'est une avance rapide, pas de l'écoute.
const MAX_STEP: f64 = 2.0;

/// Un passage d'un morceau, du moment où il apparaît dans le lecteur à celui où il en sort.
#[derive(Debug, Clone)]
struct Passage {
    track: Track,
    duration: Option<f64>,
    started: u64,
    listened: f64,
    last: f64,
}

/// Au-delà de ce temps d'écoute, quitter un morceau n'est plus un saut.
const SKIP_MAX: f64 = 15.0;

/// Suit l'état du lecteur et repère les écoutes terminées.
#[derive(Debug, Default)]
pub struct ListenTracker {
    current: Option<Passage>,
    /// Dernier morceau sauté (quitté pour un autre après quelques secondes), pas encore relevé.
    skipped: Option<(Track, Listen)>,
}

impl ListenTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// À appeler à chaque relevé de l'état du lecteur. Renvoie l'écoute qui vient de se terminer
    /// (changement de morceau, morceau relancé depuis le début, lecteur vidé), si elle compte.
    pub fn observe(&mut self, track: Option<&Track>, playing: bool, position: f64, duration: Option<f64>, now: u64) -> Option<(Track, Listen)> {
        let same = match (&self.current, track) {
            (Some(p), Some(t)) => p.track.id == t.id,
            _ => false,
        };
        // Répétition du morceau (ou retour au tout début) : une nouvelle écoute commence.
        let restarted = same && self.current.as_ref().is_some_and(|p| position < 3.0 && p.last - position > 5.0);
        if same && !restarted {
            let p = self.current.as_mut().expect("passage en cours");
            let step = position - p.last;
            if playing && step > 0.0 && step <= MAX_STEP {
                p.listened += step;
            }
            p.last = position;
            if duration.is_some() {
                p.duration = duration;
            }
            return None;
        }
        // Passage à un autre morceau après quelques secondes seulement : un saut (signal pour les
        // recommandations). Un lecteur vidé ou un morceau relancé n'en sont pas.
        if !same && track.is_some() {
            self.skipped = self.current.as_ref().filter(|p| is_skip(p)).map(|p| {
                let secs = (p.listened * 10.0).round() / 10.0;
                (p.track.clone(), Listen { at: p.started, id: p.track.id.clone(), secs })
            });
        }
        let done = self.finish();
        self.current = track.map(|t| Passage {
            track: t.clone(),
            duration: duration.or(t.duration),
            started: now,
            listened: 0.0,
            last: position,
        });
        done
    }

    /// Le dernier morceau sauté, s'il y en a un (une seule fois).
    pub fn take_skip(&mut self) -> Option<(Track, Listen)> {
        self.skipped.take()
    }

    /// Termine l'écoute en cours (fermeture de l'app) ; la renvoie si elle compte.
    pub fn finish(&mut self) -> Option<(Track, Listen)> {
        let p = self.current.take()?;
        if p.listened < threshold(p.duration) {
            return None;
        }
        let secs = (p.listened * 10.0).round() / 10.0;
        let listen = Listen { at: p.started, id: p.track.id.clone(), secs };
        Some((p.track, listen))
    }
}

/// Un passage quitté tôt : écouté au moins une seconde (pas juste survolé), moins de 15 s (et
/// moins que le seuil d'une écoute), et pas arrivé à la fin du morceau.
fn is_skip(p: &Passage) -> bool {
    let near_end = p.duration.is_some_and(|d| p.last >= d - 5.0);
    p.listened >= 1.0 && p.listened < SKIP_MAX.min(threshold(p.duration)) && !near_end
}

// ---------- Journal sur disque ----------

/// Une ligne du journal : la description d'un morceau, ou une écoute.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Line {
    Track(Track),
    Listen(Listen),
    /// Morceau sauté. Les versions qui ne connaissent pas cette ligne l'ignorent.
    Skip(Listen),
}

#[derive(Debug, Default)]
struct LogData {
    listens: Vec<Listen>,
    /// Sauts, dans l'ordre d'arrivée.
    skips: Vec<Listen>,
    tracks: HashMap<String, Track>,
    /// Le fichier ne finit pas par un retour à la ligne (écriture interrompue) : on en ajoute un
    /// avant la prochaine ligne, pour ne pas la coller à la ligne abîmée.
    broken_end: bool,
}

/// Le journal des écoutes, gardé en mémoire et sur disque.
pub struct ListenLog {
    path: PathBuf,
    data: Mutex<LogData>,
}

impl ListenLog {
    /// Charge le journal (vide s'il n'existe pas). Les lignes illisibles sont ignorées.
    pub fn load(path: &Path) -> Self {
        let mut data = LogData::default();
        if let Ok(text) = fs::read_to_string(path) {
            data.broken_end = !text.is_empty() && !text.ends_with('\n');
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                match serde_json::from_str::<Line>(line) {
                    Ok(Line::Track(t)) => {
                        data.tracks.insert(t.id.clone(), t);
                    }
                    Ok(Line::Listen(l)) => data.listens.push(l),
                    Ok(Line::Skip(l)) => data.skips.push(l),
                    Err(_) => {}
                }
            }
        }
        data.listens.sort_by_key(|l| l.at);
        Self { path: path.to_owned(), data: Mutex::new(data) }
    }

    /// Le journal existe sur disque (sinon, c'est le tout premier lancement avec les stats).
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn len(&self) -> usize {
        self.data.lock().listens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Ajoute une écoute à la fin du journal (et la description du morceau, si elle est nouvelle
    /// ou a changé).
    pub fn record(&self, track: &Track, listen: Listen) -> Result<()> {
        self.append(track, Line::Listen(listen))
    }

    /// Note un morceau sauté (recommandations : on évite de le reproposer).
    pub fn record_skip(&self, track: &Track, skip: Listen) -> Result<()> {
        self.append(track, Line::Skip(skip))
    }

    fn append(&self, track: &Track, entry: Line) -> Result<()> {
        let mut data = self.data.lock();
        let mut out = String::new();
        if data.broken_end {
            out.push('\n');
        }
        if data.tracks.get(&track.id) != Some(track) {
            out += &line(&Line::Track(track.clone()))?;
        }
        out += &line(&entry)?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        OpenOptions::new().create(true).append(true).open(&self.path)?.write_all(out.as_bytes())?;
        data.broken_end = false;
        data.tracks.insert(track.id.clone(), track.clone());
        match entry {
            Line::Listen(listen) => {
                // Les écoutes arrivent dans l'ordre : on garde la liste triée sans tout retrier.
                let i = data.listens.partition_point(|l| l.at <= listen.at);
                data.listens.insert(i, listen);
            }
            Line::Skip(skip) => data.skips.push(skip),
            Line::Track(_) => {}
        }
        Ok(())
    }

    /// Donne accès aux écoutes (triées), aux sauts et aux morceaux décrits, sans les copier.
    pub fn with_data<R>(&self, f: impl FnOnce(&[Listen], &[Listen], &HashMap<String, Track>) -> R) -> R {
        let data = self.data.lock();
        f(&data.listens, &data.skips, &data.tracks)
    }

    /// Premier lancement avec les stats : reprend l'historique existant (une écoute par lecture,
    /// comptée pour la durée du morceau, faute de mieux). Ne fait rien si le journal existe.
    pub fn import_history(&self, history: &[Played]) -> Result<()> {
        if self.exists() {
            return Ok(());
        }
        let mut data = self.data.lock();
        let mut out = String::new();
        for p in history.iter().rev() {
            if !data.tracks.contains_key(&p.track.id) {
                out += &line(&Line::Track(p.track.clone()))?;
                data.tracks.insert(p.track.id.clone(), p.track.clone());
            }
            let secs = p.track.duration.unwrap_or(0.0).round();
            let listen = Listen { at: p.at, id: p.track.id.clone(), secs };
            out += &line(&Line::Listen(listen.clone()))?;
            data.listens.push(listen);
        }
        data.listens.sort_by_key(|l| l.at);
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        fs::write(&tmp, out)?;
        fs::rename(tmp, &self.path)?;
        Ok(())
    }

    /// Efface toutes les écoutes.
    pub fn clear(&self) -> Result<()> {
        let mut data = self.data.lock();
        fs::write(&self.path, "")?;
        *data = LogData::default();
        Ok(())
    }

    /// Statistiques d'une période. `offset` : décalage de l'heure locale sur UTC, en minutes
    /// (Paris en hiver : 60).
    pub fn stats(&self, period: Period, offset: i32) -> Stats {
        let data = self.data.lock();
        let listens: Vec<(&Listen, &Track)> =
            data.listens.iter().filter_map(|l| data.tracks.get(&l.id).map(|t| (l, t))).collect();
        compute(&listens, period, offset)
    }
}

fn line(l: &Line) -> Result<String> {
    let mut s = serde_json::to_string(l).map_err(|e| crate::Error::Other(e.to_string()))?;
    s.push('\n');
    Ok(s)
}

// ---------- Dates (calendrier grégorien, sans dépendance) ----------

const DAY_MS: i64 = 86_400_000;

/// Nombre de jours depuis le 1er janvier 1970 (algorithme de Howard Hinnant).
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Date (année, mois, jour) d'un nombre de jours depuis le 1er janvier 1970.
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y as i32, m, d)
}

/// Une écoute vue à l'heure locale.
struct Local {
    day: i64,
    hour: usize,
    /// 0 = lundi.
    weekday: usize,
    year: i32,
    month: u32,
}

fn local(at: u64, offset: i32) -> Local {
    let ms = at as i64 + i64::from(offset) * 60_000;
    let day = ms.div_euclid(DAY_MS);
    let (year, month, _) = civil_from_days(day);
    Local {
        day,
        hour: (ms.rem_euclid(DAY_MS) / 3_600_000) as usize,
        // Le 1er janvier 1970 était un jeudi.
        weekday: (day + 3).rem_euclid(7) as usize,
        year,
        month,
    }
}

// ---------- Statistiques ----------

/// La période résumée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Period {
    Year { year: i32 },
    Month { year: i32, month: u32 },
    All,
}

impl Period {
    /// Jours locaux couverts : `[début, fin)`.
    fn days(self) -> (i64, i64) {
        match self {
            Period::Year { year } => (days_from_civil(year, 1, 1), days_from_civil(year + 1, 1, 1)),
            Period::Month { year, month } => {
                let (ny, nm) = if month >= 12 { (year + 1, 1) } else { (year, month + 1) };
                (days_from_civil(year, month, 1), days_from_civil(ny, nm, 1))
            }
            Period::All => (i64::MIN, i64::MAX),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopTrack {
    pub track: Track,
    pub plays: u32,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopArtist {
    pub name: String,
    /// Identifiant Deezer, quand on le connaît.
    pub id: Option<String>,
    pub plays: u32,
    pub seconds: f64,
    /// Son morceau le plus écouté (pour sa pochette, et pour le lancer).
    pub track: Track,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopAlbum {
    pub title: String,
    pub artist: String,
    pub id: Option<String>,
    pub plays: u32,
    pub seconds: f64,
    /// Son morceau le plus écouté (pour la pochette).
    pub track: Track,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayTotal {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub seconds: f64,
    pub plays: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceShare {
    pub source: String,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstListen {
    pub track: Track,
    pub at: u64,
}

/// Le résumé d'une période.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub period: Period,
    /// Nombre d'écoutes.
    pub plays: u32,
    /// Temps d'écoute total, en secondes.
    pub seconds: f64,
    /// Morceaux, artistes et albums différents.
    pub tracks: u32,
    pub artists: u32,
    pub albums: u32,
    /// Artistes écoutés pour la première fois pendant la période.
    pub new_artists: u32,
    /// Il y a des écoutes avant la période (sinon « nouveaux artistes » n'a pas de sens).
    pub has_earlier: bool,
    pub top_tracks: Vec<TopTrack>,
    pub top_artists: Vec<TopArtist>,
    pub top_albums: Vec<TopAlbum>,
    /// Secondes écoutées par mois (janvier en premier), par heure (0 h en premier) et par jour
    /// de la semaine (lundi en premier).
    pub by_month: Vec<f64>,
    pub by_hour: Vec<f64>,
    pub by_weekday: Vec<f64>,
    pub by_source: Vec<SourceShare>,
    pub busiest_day: Option<DayTotal>,
    /// Mois le plus écouté (1 à 12).
    pub busiest_month: Option<u32>,
    /// Heure préférée (0 à 23).
    pub favorite_hour: Option<u32>,
    /// Jours avec au moins une écoute, et plus longue série de jours consécutifs.
    pub active_days: u32,
    pub longest_streak: u32,
    /// Première écoute de la période.
    pub first: Option<FirstListen>,
    /// Années qui ont des écoutes, la plus récente en premier.
    pub years: Vec<i32>,
}

/// Nombre d'entrées de chaque classement.
const TOP: usize = 10;

#[derive(Default)]
struct Tally {
    plays: u32,
    seconds: f64,
    /// Écoutes de chaque morceau, pour retrouver le plus écouté.
    by_track: HashMap<String, u32>,
}

impl Tally {
    fn add(&mut self, l: &Listen) {
        self.plays += 1;
        self.seconds += l.secs;
        *self.by_track.entry(l.id.clone()).or_default() += 1;
    }
    fn best_track(&self) -> &str {
        self.by_track.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))).map(|(id, _)| id.as_str()).unwrap_or("")
    }
}

/// Les artistes d'un morceau : (clé de regroupement, nom affiché, identifiant).
pub(crate) fn artists_of(t: &Track) -> Vec<(String, String, Option<String>)> {
    let list: Vec<(String, Option<String>)> = if t.artists.is_empty() {
        split_artists(&t.artist).into_iter().map(|n| (n, None)).collect()
    } else {
        t.artists.iter().map(|a| (a.name.clone(), a.id.clone())).collect()
    };
    let mut out: Vec<(String, String, Option<String>)> = Vec::new();
    for (name, id) in list {
        let key = name.trim().to_lowercase();
        if !key.is_empty() && !out.iter().any(|o| o.0 == key) {
            out.push((key, name.trim().to_owned(), id));
        }
    }
    out
}

/// Ordre des classements : le plus écouté d'abord (temps ou écoutes), puis par nom.
fn rank<T>(list: &mut [(String, Tally, T)], by_time: bool) {
    list.sort_by(|a, b| {
        let (x, y) = (&a.1, &b.1);
        let primary = if by_time {
            y.seconds.total_cmp(&x.seconds).then(y.plays.cmp(&x.plays))
        } else {
            y.plays.cmp(&x.plays).then(y.seconds.total_cmp(&x.seconds))
        };
        primary.then_with(|| a.0.cmp(&b.0))
    });
}

/// Calcule les statistiques d'une période à partir des écoutes (triées ou non) et de leurs
/// morceaux. `offset` : décalage de l'heure locale sur UTC, en minutes.
pub fn compute(listens: &[(&Listen, &Track)], period: Period, offset: i32) -> Stats {
    let (from, to) = period.days();

    // Première écoute de chaque artiste (tout l'historique), et années couvertes.
    let mut first_day: HashMap<String, i64> = HashMap::new();
    let mut years: Vec<i32> = Vec::new();
    let mut has_earlier = false;
    for (l, t) in listens {
        let loc = local(l.at, offset);
        if !years.contains(&loc.year) {
            years.push(loc.year);
        }
        has_earlier |= loc.day < from;
        for (key, _, _) in artists_of(t) {
            let e = first_day.entry(key).or_insert(loc.day);
            *e = (*e).min(loc.day);
        }
    }
    years.sort_unstable_by(|a, b| b.cmp(a));

    let mut tracks: HashMap<String, (Tally, &Track)> = HashMap::new();
    let mut artists: HashMap<String, (Tally, String, Option<String>)> = HashMap::new();
    let mut albums: HashMap<String, (Tally, String, String, Option<String>)> = HashMap::new();
    let mut days: HashMap<i64, (f64, u32)> = HashMap::new();
    let mut sources: HashMap<String, f64> = HashMap::new();
    let (mut by_month, mut by_hour, mut by_weekday) = (vec![0.0; 12], vec![0.0; 24], vec![0.0; 7]);
    let (mut plays, mut seconds) = (0u32, 0.0f64);
    let mut first: Option<FirstListen> = None;

    for (l, t) in listens {
        let loc = local(l.at, offset);
        if loc.day < from || loc.day >= to {
            continue;
        }
        plays += 1;
        seconds += l.secs;
        if first.as_ref().is_none_or(|f| l.at < f.at) {
            first = Some(FirstListen { track: (*t).clone(), at: l.at });
        }
        tracks.entry(l.id.clone()).or_insert_with(|| (Tally::default(), t)).0.add(l);
        for (key, name, id) in artists_of(t) {
            let e = artists.entry(key).or_insert_with(|| (Tally::default(), name, None));
            e.0.add(l);
            if e.2.is_none() {
                e.2 = id;
            }
        }
        if let Some(album) = t.album.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
            let artist = artists_of(t).into_iter().next().map(|a| a.1).unwrap_or_else(|| t.artist.clone());
            let key = format!("{}\u{1}{}", album.to_lowercase(), artist.to_lowercase());
            let e = albums.entry(key).or_insert_with(|| (Tally::default(), album.to_owned(), artist, None));
            e.0.add(l);
            if e.3.is_none() {
                e.3 = t.album_id.clone();
            }
        }
        let d = days.entry(loc.day).or_default();
        d.0 += l.secs;
        d.1 += 1;
        *sources.entry(t.source.clone()).or_default() += l.secs;
        by_month[loc.month as usize - 1] += l.secs;
        by_hour[loc.hour] += l.secs;
        by_weekday[loc.weekday] += l.secs;
    }

    let track_of = |id: &str| tracks.get(id).map(|(_, t)| (*t).clone());

    let mut top_tracks: Vec<(String, Tally, &Track)> = tracks.iter().map(|(id, (tally, t))| {
        (format!("{}\u{1}{id}", t.title.to_lowercase()), Tally { plays: tally.plays, seconds: tally.seconds, by_track: HashMap::new() }, *t)
    }).collect();
    rank(&mut top_tracks, false);
    let top_tracks = top_tracks.into_iter().take(TOP).map(|(_, x, t)| TopTrack { track: t.clone(), plays: x.plays, seconds: x.seconds }).collect();

    let new_artists = artists.keys().filter(|k| first_day.get(*k).is_some_and(|d| *d >= from)).count() as u32;
    let n_artists = artists.len() as u32;
    let mut top_artists: Vec<(String, Tally, (String, Option<String>))> =
        artists.into_iter().map(|(_, (tally, name, id))| (name.to_lowercase(), tally, (name, id))).collect();
    rank(&mut top_artists, true);
    let top_artists = top_artists
        .into_iter()
        .take(TOP)
        .filter_map(|(_, x, (name, id))| {
            let track = track_of(x.best_track())?;
            Some(TopArtist { name, id, plays: x.plays, seconds: x.seconds, track })
        })
        .collect();

    let n_albums = albums.len() as u32;
    let mut top_albums: Vec<(String, Tally, (String, String, Option<String>))> =
        albums.into_iter().map(|(key, (tally, title, artist, id))| (key, tally, (title, artist, id))).collect();
    rank(&mut top_albums, true);
    let top_albums = top_albums
        .into_iter()
        .take(TOP)
        .filter_map(|(_, x, (title, artist, id))| {
            let track = track_of(x.best_track())?;
            Some(TopAlbum { title, artist, id, plays: x.plays, seconds: x.seconds, track })
        })
        .collect();

    let busiest_day = days
        .iter()
        .max_by(|a, b| a.1.0.total_cmp(&b.1.0).then(b.0.cmp(a.0)))
        .map(|(day, (secs, n))| {
            let (year, month, d) = civil_from_days(*day);
            DayTotal { year, month, day: d, seconds: *secs, plays: *n }
        });
    let busiest = |v: &[f64]| {
        v.iter().enumerate().filter(|(_, s)| **s > 0.0).max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(&a.0))).map(|(i, _)| i as u32)
    };

    let mut active: Vec<i64> = days.keys().copied().collect();
    active.sort_unstable();
    let (mut longest_streak, mut run) = (0u32, 0u32);
    for (i, d) in active.iter().enumerate() {
        run = if i > 0 && active[i - 1] + 1 == *d { run + 1 } else { 1 };
        longest_streak = longest_streak.max(run);
    }

    let mut by_source: Vec<SourceShare> = sources.into_iter().map(|(source, seconds)| SourceShare { source, seconds }).collect();
    by_source.sort_by(|a, b| b.seconds.total_cmp(&a.seconds).then_with(|| a.source.cmp(&b.source)));

    Stats {
        period,
        plays,
        seconds,
        tracks: tracks.len() as u32,
        artists: n_artists,
        albums: n_albums,
        new_artists,
        has_earlier,
        top_tracks,
        top_artists,
        top_albums,
        busiest_month: busiest(&by_month).map(|m| m + 1),
        favorite_hour: busiest(&by_hour),
        by_month,
        by_hour,
        by_weekday,
        by_source,
        busiest_day,
        active_days: active.len() as u32,
        longest_streak,
        first,
        years,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArtistRef, Location};

    fn track(id: &str, artist: &str, album: Option<&str>, duration: f64) -> Track {
        Track {
            id: id.into(),
            source: "deezer".into(),
            title: id.into(),
            artist: artist.into(),
            album: album.map(Into::into),
            album_id: None,
            duration: Some(duration),
            track_number: None,
            disc_number: None,
            artwork: None,
            has_cover: false,
            location: Location::Remote { remote_id: id.into() },
            artists: Vec::new(),
        }
    }

    /// Joue `t` pendant `secs` secondes (relevés toutes les 250 ms) à partir de `from`.
    fn play(tr: &mut ListenTracker, t: &Track, from: f64, secs: f64, now: &mut u64) -> Vec<Listen> {
        let mut out = Vec::new();
        let mut pos = from;
        while pos <= from + secs + 1e-9 {
            out.extend(tr.observe(Some(t), true, pos, t.duration, *now).map(|x| x.1));
            pos += 0.25;
            *now += 250;
        }
        out
    }

    #[test]
    fn a_listen_counts_after_30_seconds_or_half_the_song() {
        let mut tr = ListenTracker::new();
        let mut now = 1_000_000;
        let long = track("long", "A", None, 200.0);
        let short = track("short", "A", None, 40.0);
        let other = track("other", "B", None, 200.0);

        // 10 s d'un morceau de 3 min 20 : ne compte pas.
        assert!(play(&mut tr, &long, 0.0, 10.0, &mut now).is_empty());
        // Changement de morceau : la première écoute est trop courte, rien n'est rendu.
        assert!(play(&mut tr, &short, 0.0, 21.0, &mut now).is_empty());
        // 21 s d'un morceau de 40 s : plus de la moitié, compte (rendu au changement suivant).
        let done = play(&mut tr, &other, 0.0, 45.0, &mut now);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].id, "short");
        assert!((done[0].secs - 21.0).abs() < 0.3, "{}", done[0].secs);
        // Fermeture de l'app : l'écoute en cours (45 s) compte.
        let (t, l) = tr.finish().expect("écoute de 45 s");
        assert_eq!(t.id, "other");
        assert!((l.secs - 45.0).abs() < 0.3);
        assert!(tr.finish().is_none());
        assert_eq!(threshold(None), 30.0);
        assert_eq!(threshold(Some(20.0)), 10.0);
    }

    #[test]
    fn pauses_and_seeks_are_not_counted_and_repeats_count_twice() {
        let mut tr = ListenTracker::new();
        let mut now = 0;
        let t = track("t", "A", None, 180.0);
        play(&mut tr, &t, 0.0, 30.0, &mut now);
        // En pause : la position ne bouge pas, et rien ne compte même si elle bougeait.
        for _ in 0..40 {
            assert!(tr.observe(Some(&t), false, 30.0, t.duration, now).is_none());
            now += 250;
        }
        // Avance rapide de 30 s à 150 s : le saut n'est pas de l'écoute.
        play(&mut tr, &t, 150.0, 5.0, &mut now);
        let (_, l) = tr.finish().expect("35 s écoutées");
        assert!((l.secs - 35.0).abs() < 0.3, "{}", l.secs);

        // Répétition du morceau : deux écoutes distinctes.
        let mut tr = ListenTracker::new();
        let mut done = play(&mut tr, &t, 0.0, 179.0, &mut now);
        done.extend(play(&mut tr, &t, 0.0, 40.0, &mut now));
        assert_eq!(done.len(), 1);
        assert!(done[0].secs > 178.0);
        assert!(tr.finish().is_some());
        // Lecteur vidé : l'écoute en cours se termine.
        play(&mut tr, &t, 0.0, 35.0, &mut now);
        assert!(tr.observe(None, false, 0.0, None, now).is_some());
    }

    #[test]
    fn leaving_a_song_after_a_few_seconds_is_a_skip() {
        let mut tr = ListenTracker::new();
        let mut now = 0;
        let (a, b) = (track("a", "A", None, 200.0), track("b", "B", None, 200.0));
        // 5 s puis un autre morceau : saut.
        play(&mut tr, &a, 0.0, 5.0, &mut now);
        play(&mut tr, &b, 0.0, 0.5, &mut now);
        let (t, l) = tr.take_skip().expect("saut");
        assert_eq!(t.id, "a");
        assert!((l.secs - 5.0).abs() < 0.3);
        assert!(tr.take_skip().is_none());
        // À peine lancé (moins d'une seconde) : pas un saut.
        play(&mut tr, &a, 0.0, 20.0, &mut now);
        assert!(tr.take_skip().is_none());
        // 20 s : plus un saut.
        play(&mut tr, &b, 0.0, 1.0, &mut now);
        assert!(tr.take_skip().is_none());
        // Lecteur vidé après 5 s : fin de la file, pas un saut.
        play(&mut tr, &a, 0.0, 5.0, &mut now);
        assert_eq!(tr.take_skip().map(|s| s.0.id), Some("b".into()), "b quitté après 1 s");
        tr.observe(None, false, 0.0, None, now);
        assert!(tr.take_skip().is_none());
    }

    #[test]
    fn calendar_dates_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        for day in [-800_000, -1, 0, 59, 10_957, 20_000, 20_454, 30_000] {
            let (y, m, d) = civil_from_days(day);
            assert_eq!(days_from_civil(y, m, d), day);
        }
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        // Le 3 novembre 2026 est un mardi (0 = lundi).
        assert_eq!(local(days_from_civil(2026, 11, 3) as u64 * 86_400_000, 0).weekday, 1);
    }

    /// Milliseconde UTC d'une date et d'une heure.
    fn at(y: i32, m: u32, d: u32, h: u64) -> u64 {
        days_from_civil(y, m, d) as u64 * 86_400_000 + h * 3_600_000
    }

    #[test]
    fn stats_of_a_year() {
        let mut feat = track("duo", "Clara Vent & Orbite", Some("Satellites"), 200.0);
        feat.artists = vec![ArtistRef { name: "Clara Vent".into(), id: Some("12".into()) }, ArtistRef::named("Orbite")];
        let solo = track("solo", "Clara Vent", Some("Satellites"), 180.0);
        let other = track("autre", "Les Phares", None, 240.0);
        let old = track("ancien", "Les Phares", Some("Phares"), 100.0);
        let l = |id: &str, when: u64, secs: f64| Listen { at: when, id: id.into(), secs };
        let listens = [
            (l("ancien", at(2025, 12, 31, 23), 100.0), &old),
            (l("solo", at(2026, 1, 2, 9), 180.0), &solo),
            (l("solo", at(2026, 1, 3, 21), 180.0), &solo),
            (l("duo", at(2026, 1, 3, 22), 200.0), &feat),
            (l("solo", at(2026, 1, 4, 23), 90.0), &solo),
            (l("autre", at(2026, 6, 10, 21), 200.0), &other),
            (l("autre", at(2026, 6, 10, 23), 200.0), &other),
            (l("autre", at(2026, 6, 10, 23), 200.0), &other),
            (l("solo", at(2027, 1, 1, 10), 180.0), &solo),
        ];
        let refs: Vec<(&Listen, &Track)> = listens.iter().map(|(l, t)| (l, *t)).collect();
        let s = compute(&refs, Period::Year { year: 2026 }, 0);

        assert_eq!(s.plays, 7);
        assert_eq!(s.seconds, 1250.0);
        assert_eq!((s.tracks, s.artists, s.albums), (3, 3, 1));
        // Les Phares étaient déjà écoutés en 2025 : deux nouveaux artistes seulement.
        assert_eq!(s.new_artists, 2);
        assert!(s.has_earlier);
        assert_eq!(s.years, [2027, 2026, 2025]);

        // Morceaux : par nombre d'écoutes. Artistes et albums : par temps d'écoute.
        assert_eq!(s.top_tracks.iter().map(|t| (t.track.id.as_str(), t.plays)).collect::<Vec<_>>(), [("autre", 3), ("solo", 3), ("duo", 1)]);
        assert_eq!(s.top_artists[0].name, "Clara Vent");
        assert_eq!(s.top_artists[0].seconds, 650.0);
        assert_eq!(s.top_artists[0].id.as_deref(), Some("12"));
        assert_eq!(s.top_artists[0].track.id, "solo");
        assert_eq!(s.top_artists[1].name, "Les Phares");
        assert_eq!(s.top_artists[2].name, "Orbite");
        assert_eq!(s.top_albums.len(), 1);
        assert_eq!((s.top_albums[0].title.as_str(), s.top_albums[0].plays), ("Satellites", 4));

        assert_eq!(s.busiest_day.as_ref().map(|d| (d.year, d.month, d.day, d.plays)), Some((2026, 6, 10, 3)));
        assert_eq!(s.busiest_month, Some(1));
        assert_eq!(s.favorite_hour, Some(23));
        assert_eq!(s.by_month[0], 650.0);
        assert_eq!((s.active_days, s.longest_streak), (4, 3));
        assert_eq!(s.first.as_ref().map(|f| f.track.id.as_str()), Some("solo"));
        assert_eq!(s.by_source[0].source, "deezer");
        // Le 10 juin 2026 est un mercredi.
        assert_eq!(s.by_weekday[2], 600.0);

        // Heure locale : à Paris l'été (+120 min), l'écoute de 23 h UTC du 10 juin tombe le 11.
        let paris = compute(&refs, Period::Month { year: 2026, month: 6 }, 120);
        assert_eq!(paris.plays, 3);
        assert_eq!(paris.busiest_day.as_ref().map(|d| d.day), Some(11));
        assert_eq!(paris.favorite_hour, Some(1));
        // Et le 31 décembre à 23 h UTC est déjà le 1er janvier à Paris (+60 min) : 2026.
        assert_eq!(compute(&refs, Period::Year { year: 2026 }, 60).plays, 8);

        let all = compute(&refs, Period::All, 0);
        assert_eq!(all.plays, 9);
        assert!(!all.has_earlier);
        assert_eq!(compute(&refs, Period::Year { year: 2024 }, 0).top_tracks.len(), 0);
    }

    #[test]
    fn the_log_survives_a_reload_and_a_torn_line() {
        let dir = std::env::temp_dir().join(format!("yato-stats-{}-{}", std::process::id(), line!()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("listens.jsonl");
        let t = track("a", "A", Some("Album"), 200.0);
        let log = ListenLog::load(&path);
        assert!(!log.exists());
        log.import_history(&[Played { track: t.clone(), at: at(2026, 3, 1, 12) }]).unwrap();
        log.record(&t, Listen { at: at(2026, 3, 2, 12), id: "a".into(), secs: 60.0 }).unwrap();
        // Écriture interrompue (coupure de courant) : une ligne à moitié écrite à la fin.
        OpenOptions::new().append(true).open(&path).unwrap().write_all(b"{\"listen\":{\"at\":1").unwrap();

        let log = ListenLog::load(&path);
        assert_eq!(log.len(), 2);
        log.record(&t, Listen { at: at(2026, 3, 3, 12), id: "a".into(), secs: 30.0 }).unwrap();
        log.record_skip(&t, Listen { at: at(2026, 3, 4, 12), id: "a".into(), secs: 4.0 }).unwrap();
        // Import refusé : le journal existe déjà.
        log.import_history(&[Played { track: t.clone(), at: 5 }]).unwrap();
        let log = ListenLog::load(&path);
        let s = log.stats(Period::Year { year: 2026 }, 0);
        assert_eq!((s.plays, s.seconds), (3, 290.0), "un saut n'est pas une écoute");
        assert_eq!(log.with_data(|l, sk, _| (l.len(), sk.len())), (3, 1));
        // Le morceau n'est décrit qu'une fois dans le fichier.
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("\"track\"").count(), 1);
        log.clear().unwrap();
        assert!(ListenLog::load(&path).is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}
