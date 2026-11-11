//! Couche Tauri : expose le cœur (`yato-core`) à l'interface. Aucune logique métier ici.

#[cfg(target_os = "android")]
mod android_media;
mod cover;
#[cfg(desktop)]
mod discord;
#[cfg(desktop)]
mod media;
mod settings;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use yato_core::{
    ArtistRef, Collection, CollectionData, Controller, Error, ListenLog, ListenTracker, Period, RecentSearches, Repeat, Session,
    Snapshot, SourceInfo, SourceRegistry, Stats, Track,
    discover::{self, Discover, Section},
    lyrics::{Lyrics, LyricsClient},
    sources::{
        DeezerSource, LocalLibrary, YouTubeMusicSource,
        deezer::{AlbumPage, ArtistPage},
        read_cover,
    },
    ytdlp::{YtDlp, YtDlpStatus},
};
use percent_encoding::percent_decode_str;
use parking_lot::Mutex;
use settings::Settings;
use tauri::{Emitter, Manager, RunEvent, State, http};

type Res<T> = Result<T, Error>;

struct AppState {
    controller: Arc<Controller>,
    lyrics: LyricsClient,
    ytdlp: Arc<YtDlp>,
    library: Arc<LocalLibrary>,
    /// Source d'infos : fiches d'artistes et albums.
    deezer: Arc<DeezerSource>,
    collection: Collection,
    /// Journal des écoutes (page Stats, récap de l'année).
    listens: ListenLog,
    /// Page Découvrir : sections en cache.
    discover: Arc<Discover>,
    /// Suivi de l'écoute en cours, pour noter sa durée réelle.
    tracker: Mutex<ListenTracker>,
    searches: RecentSearches,
    youtube: Arc<YouTubeMusicSource>,
    client: yato_core::reqwest::Client,
    cache_path: PathBuf,
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    /// Écoute en cours, gardée pour le prochain lancement.
    session_path: PathBuf,
}

impl AppState {
    fn save_session(&self) {
        if self.settings.lock().resume {
            let _ = settings::save(&self.session_path, &self.controller.session());
        }
    }
}

const SEARCH_LIMIT: usize = 50;

// ---------- Bibliothèque ----------

#[tauri::command]
fn list_sources(state: State<'_, AppState>) -> Vec<SourceInfo> {
    state.controller.sources().list()
}

#[tauri::command]
fn library_tracks(state: State<'_, AppState>) -> Vec<Track> {
    state.library.tracks()
}

#[tauri::command]
fn library_folders(state: State<'_, AppState>) -> Vec<PathBuf> {
    state.library.folders()
}

/// Lance une opération de scan hors du thread de l'interface, puis sauvegarde le cache.
async fn with_library<F>(state: &AppState, f: F) -> Res<usize>
where
    F: FnOnce(&LocalLibrary) -> usize + Send + 'static,
{
    let lib = state.library.clone();
    let cache = state.cache_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let n = f(&lib);
        lib.save(&cache)?;
        Ok(n)
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))?
}

#[tauri::command]
async fn add_folder(state: State<'_, AppState>, path: PathBuf) -> Res<usize> {
    with_library(&state, move |lib| lib.add_folder(path)).await
}

#[tauri::command]
async fn remove_folder(state: State<'_, AppState>, path: PathBuf) -> Res<usize> {
    with_library(&state, move |lib| lib.remove_folder(&path)).await
}

#[tauri::command]
async fn rescan(state: State<'_, AppState>) -> Res<usize> {
    with_library(&state, |lib| lib.rescan()).await
}

// ---------- Collection : likes et playlists ----------
// Chaque commande renvoie la collection entière : l'interface la remplace d'un bloc.

#[tauri::command]
fn collection(state: State<'_, AppState>) -> CollectionData {
    state.collection.get()
}

#[tauri::command]
fn toggle_like(state: State<'_, AppState>, track: Track) -> Res<CollectionData> {
    state.collection.toggle_like(track)
}

#[tauri::command]
fn create_playlist(state: State<'_, AppState>, name: String, tracks: Option<Vec<Track>>) -> Res<CollectionData> {
    state.collection.create_playlist(name, tracks.unwrap_or_default())
}

#[tauri::command]
fn rename_playlist(state: State<'_, AppState>, id: String, name: String) -> Res<CollectionData> {
    state.collection.rename_playlist(&id, name)
}

#[tauri::command]
fn set_playlist_cover(state: State<'_, AppState>, id: String, cover: Option<String>) -> Res<CollectionData> {
    state.collection.set_playlist_cover(&id, cover)
}

#[tauri::command]
fn delete_playlist(state: State<'_, AppState>, id: String) -> Res<CollectionData> {
    state.collection.delete_playlist(&id)
}

#[tauri::command]
fn add_to_playlist(state: State<'_, AppState>, id: String, tracks: Vec<Track>) -> Res<CollectionData> {
    state.collection.add_to_playlist(&id, tracks)
}

#[tauri::command]
fn remove_from_playlist(state: State<'_, AppState>, id: String, track_id: String) -> Res<CollectionData> {
    state.collection.remove_from_playlist(&id, &track_id)
}

#[tauri::command]
fn clear_history(state: State<'_, AppState>) -> Res<CollectionData> {
    state.collection.clear_history()
}

/// Résultat d'un import : la collection à jour, la nouvelle playlist, et ce qui manque.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    collection: CollectionData,
    id: Option<String>,
    added: usize,
    missing: usize,
    /// Les morceaux sont allés dans les titres likés (liens de morceaux Spotify).
    liked: bool,
}

/// Importe une playlist depuis un lien (Spotify, Deezer, YouTube Music) et la crée ici.
/// L'avancement (recherche des morceaux Spotify) arrive par l'événement `import-progress`.
#[tauri::command]
async fn import_playlist(app: tauri::AppHandle, state: State<'_, AppState>, url: String) -> Res<ImportResult> {
    let progress = |done: usize, total: usize| {
        let _ = app.emit("import-progress", (done, total));
    };
    let found = yato_core::import::import(&url, &state.client, &state.deezer, &state.youtube, progress).await?;
    if found.tracks.is_empty() {
        return Err(Error::Other("None of the songs in this playlist could be found.".into()));
    }
    if found.liked {
        let before = state.collection.get().liked.len();
        let collection = state.collection.like_all(found.tracks)?;
        let added = collection.liked.len() - before;
        return Ok(ImportResult { collection, id: None, added, missing: found.missing, liked: true });
    }
    let added = found.tracks.len();
    let mut collection = state.collection.create_playlist(found.name, found.tracks)?;
    let id = collection.playlists.last().map(|p| p.id.clone());
    if let (Some(id), Some(cover)) = (&id, found.cover) {
        // Une image refusée n'empêche pas l'import : la mosaïque reste.
        if let Ok(c) = state.collection.set_playlist_cover(id, Some(cover)) {
            collection = c;
        }
    }
    Ok(ImportResult { collection, id, added, missing: found.missing, liked: false })
}

// ---------- Statistiques d'écoute ----------

/// Statistiques d'une période. `offset` : décalage de l'heure locale sur UTC, en minutes.
#[tauri::command]
fn listening_stats(state: State<'_, AppState>, period: Period, offset: i32) -> Stats {
    state.listens.stats(period, offset)
}

#[tauri::command]
fn clear_stats(state: State<'_, AppState>) -> Res<()> {
    state.listens.clear()
}

// ---------- Découvrir ----------

/// Sections de la page Découvrir, tout de suite (contenu en cache, éventuellement un peu ancien,
/// ou vide en attendant). Si le cache a vieilli, elles sont recalculées en arrière-plan et
/// arrivent une à une par l'événement `discover-section`. `offset` : décalage de l'heure locale
/// sur UTC, en minutes (le choix des sections change à minuit).
#[tauri::command]
fn discover(app: tauri::AppHandle, state: State<'_, AppState>, force: bool, offset: i32) -> Vec<Section> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let day = (now as i64 + i64::from(offset) * 60_000).div_euclid(86_400_000);
    let profile = discover::profile_from(&state.listens, &state.collection.get(), now);
    let plan = discover::plan(&profile, day);
    let (sections, stale) = state.discover.page(&plan, now, force);
    if stale && state.discover.claim(&plan) {
        let (engine, deezer) = (state.discover.clone(), state.deezer.clone());
        tauri::async_runtime::spawn(async move {
            engine
                .build(deezer, plan, &profile, now, day, |s| {
                    let _ = app.emit("discover-section", s);
                })
                .await;
        });
    }
    sections
}

// ---------- Recherches récentes ----------

#[tauri::command]
fn recent_searches(state: State<'_, AppState>) -> Vec<String> {
    state.searches.get()
}

#[tauri::command]
fn add_recent_search(state: State<'_, AppState>, query: String) -> Res<Vec<String>> {
    state.searches.add(&query)
}

#[tauri::command]
fn remove_recent_search(state: State<'_, AppState>, query: String) -> Res<Vec<String>> {
    state.searches.remove(&query)
}

#[tauri::command]
fn clear_recent_searches(state: State<'_, AppState>) -> Res<Vec<String>> {
    state.searches.clear()
}

// ---------- Réglages ----------

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().clone()
}

#[tauri::command]
fn set_settings(state: State<'_, AppState>, value: Settings) -> Res<Settings> {
    state.controller.set_crossfade(value.crossfade);
    *state.settings.lock() = value.clone();
    settings::save(&state.settings_path, &value)?;
    if !value.resume {
        let _ = std::fs::remove_file(&state.session_path);
    }
    Ok(value)
}

#[tauri::command]
fn move_in_playlist(state: State<'_, AppState>, id: String, from: usize, to: usize) -> Res<CollectionData> {
    state.collection.move_in_playlist(&id, from, to)
}

// ---------- Recherche ----------

#[tauri::command]
async fn search(state: State<'_, AppState>, query: String, source: Option<String>) -> Res<Vec<Track>> {
    let sources = state.controller.sources();
    match source {
        Some(id) => sources.get(&id)?.search(&query, SEARCH_LIMIT).await,
        None => Ok(sources.search_all(&query, SEARCH_LIMIT).await),
    }
}

#[tauri::command]
async fn featured(state: State<'_, AppState>, source: String) -> Res<Vec<Track>> {
    state.controller.sources().get(&source)?.featured(60).await
}

// ---------- Lecture ----------
// Chaque commande renvoie l'état à jour : l'interface réagit sans attendre le prochain tick.

#[tauri::command]
async fn play_queue(state: State<'_, AppState>, tracks: Vec<Track>, index: usize) -> Res<Snapshot> {
    state.controller.play_queue(tracks, index).await?;
    Ok(state.controller.snapshot())
}

#[tauri::command]
async fn play_index(state: State<'_, AppState>, index: usize) -> Res<Snapshot> {
    state.controller.play_index(index).await?;
    Ok(state.controller.snapshot())
}

#[tauri::command]
async fn toggle(state: State<'_, AppState>) -> Res<Snapshot> {
    state.controller.toggle().await?;
    Ok(state.controller.snapshot())
}

#[tauri::command]
async fn next(state: State<'_, AppState>) -> Res<Snapshot> {
    state.controller.next().await?;
    Ok(state.controller.snapshot())
}

#[tauri::command]
async fn previous(state: State<'_, AppState>) -> Res<Snapshot> {
    state.controller.previous().await?;
    Ok(state.controller.snapshot())
}

#[tauri::command]
async fn seek(state: State<'_, AppState>, position: f64) -> Res<Snapshot> {
    // `try_seek` attend le thread audio : on ne bloque pas l'interface pendant ce temps.
    let c = state.controller.clone();
    tauri::async_runtime::spawn_blocking(move || c.seek(position))
        .await
        .map_err(|e| Error::Other(e.to_string()))??;
    Ok(state.controller.snapshot())
}

/// Prépare un morceau à l'avance (survol, premier résultat de recherche).
// `async` : la commande tourne alors dans le runtime tokio, qui exécute la préparation.
#[tauri::command]
async fn prefetch(state: State<'_, AppState>, track: Track) -> Res<()> {
    state.controller.prefetch(track);
    Ok(())
}

/// Préparation légère des morceaux affichés (top, résultats) : un clic démarre aussitôt.
#[tauri::command]
async fn warm(state: State<'_, AppState>, tracks: Vec<Track>) -> Res<()> {
    state.controller.warm(tracks);
    Ok(())
}

#[tauri::command]
fn set_shuffle(state: State<'_, AppState>, on: bool) -> Snapshot {
    state.controller.set_shuffle(on);
    state.controller.snapshot()
}

#[tauri::command]
fn set_repeat(state: State<'_, AppState>, repeat: Repeat) -> Snapshot {
    state.controller.set_repeat(repeat);
    state.controller.snapshot()
}

#[tauri::command]
fn set_volume(state: State<'_, AppState>, volume: f32) -> Snapshot {
    state.controller.set_volume(volume);
    state.controller.snapshot()
}

/// L'app s'appelait « Ondes » (identifiant `app.ondes.player`) : au premier lancement de yato,
/// on reprend ses données (dossiers de la bibliothèque, yt-dlp) plutôt que de repartir de zéro.
fn migrate_old_data(data_dir: &Path) {
    if data_dir.exists() {
        return;
    }
    let Some(old) = data_dir.parent().map(|p| p.join("app.ondes.player")) else { return };
    if old.is_dir() {
        let _ = std::fs::rename(&old, data_dir);
    }
}

// ---------- Artistes et albums (Deezer) ----------

/// Fiche d'un artiste, par son identifiant Deezer ou, à défaut, par son nom.
#[tauri::command]
async fn artist(state: State<'_, AppState>, id: Option<String>, name: String) -> Res<ArtistPage> {
    state.deezer.artist(id.as_deref(), &name).await
}

/// Fiche d'un album, par son identifiant ou, à défaut, par son titre et son artiste.
#[tauri::command]
async fn album(state: State<'_, AppState>, id: Option<String>, title: String, artist: Option<String>) -> Res<AlbumPage> {
    state.deezer.album(id.as_deref(), &title, artist.as_deref()).await
}

/// Tous les artistes d'un morceau, invités compris.
#[tauri::command]
async fn track_artists(state: State<'_, AppState>, track: Track) -> Res<Vec<ArtistRef>> {
    state.deezer.track_artists(&track).await
}

/// Paroles (synchronisées si possible) d'un morceau.
#[tauri::command]
async fn lyrics(state: State<'_, AppState>, track: Track) -> Res<Option<Lyrics>> {
    state.lyrics.fetch(&track).await
}

/// État de yt-dlp (installation, mise à jour…).
#[tauri::command]
fn ytdlp_status(state: State<'_, AppState>) -> YtDlpStatus {
    state.ytdlp.status()
}

#[tauri::command]
fn player_state(state: State<'_, AppState>) -> Snapshot {
    state.controller.snapshot()
}

#[tauri::command]
fn queue(state: State<'_, AppState>) -> Vec<Track> {
    state.controller.queue()
}

// ---------- Pochettes locales ----------
// Servies via le protocole `cover://` : pas de base64, et le webview les met en cache.

fn cover_response(uri_path: &str) -> http::Response<Vec<u8>> {
    let decoded = percent_decode_str(uri_path.trim_start_matches('/')).decode_utf8_lossy();
    match read_cover(Path::new(decoded.as_ref())) {
        Some((bytes, mime)) => http::Response::builder()
            .header(http::header::CONTENT_TYPE, mime)
            .header(http::header::CACHE_CONTROL, "max-age=31536000, immutable")
            .body(bytes),
        None => http::Response::builder().status(404).body(Vec::new()),
    }
    .expect("réponse HTTP valide")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    yato_core::timing::start();
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    // Mises à jour automatiques, publiées dans les Releases GitHub (voir .github/workflows).
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build()).plugin(tauri_plugin_process::init());
    // Android : lecture écran éteint, notification et écran verrouillé.
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_media::plugin());
    let app = builder
        .register_asynchronous_uri_scheme_protocol("cover", |_ctx, request, responder| {
            let path = request.uri().path().to_owned();
            tauri::async_runtime::spawn_blocking(move || responder.respond(cover_response(&path)));
        })
        .setup(|app| {
            migrate_old_data(&app.path().app_data_dir()?);
            let cache_path = app.path().app_data_dir()?.join("library.json");
            let library = Arc::new(LocalLibrary::load(&cache_path));
            let collection = Collection::load(&app.path().app_data_dir()?.join("collection.json"));
            // Stats : au tout premier lancement, l'historique existant sert de point de départ.
            let listens = ListenLog::load(&app.path().app_data_dir()?.join("listens.jsonl"));
            if let Err(e) = listens.import_history(&collection.get().history) {
                eprintln!("stats : reprise de l'historique impossible : {e}");
            }
            let discover = Arc::new(Discover::load(&app.path().app_cache_dir()?.join("discover.json")));
            let searches = RecentSearches::load(&app.path().app_data_dir()?.join("searches.json"));
            let settings_path = app.path().app_data_dir()?.join("settings.json");
            let session_path = app.path().app_data_dir()?.join("session.json");
            let prefs: Settings = settings::load(&settings_path);
            let client = yato_core::http_client();

            // yt-dlp géré par yato : installé, vérifié (signature) et mis à jour automatiquement.
            let ytdlp = YtDlp::new(app.path().app_data_dir()?.join("bin").join("yt-dlp"), client.clone());
            let youtube = Arc::new(YouTubeMusicSource::new(app.path().app_cache_dir()?.join("youtube"), ytdlp.clone())?);
            // Deezer donne les infos (titres, albums, pochettes, top), YouTube Music le son.
            let deezer = Arc::new(DeezerSource::new(client.clone(), youtube.clone()));

            // L'ordre d'enregistrement = l'ordre des résultats de recherche.
            // YouTube reste cherchable : versions absentes de Deezer (remixes, lives…).
            let mut sources = SourceRegistry::new();
            sources.register(library.clone());
            sources.register(deezer.clone());
            sources.register(youtube.clone());

            let controller = Controller::new(Arc::new(sources), client.clone())?;
            controller.set_crossfade(prefs.crossfade);
            if prefs.resume {
                controller.restore(settings::load::<Session>(&session_path));
            }
            app.manage(AppState {
                controller: controller.clone(),
                lyrics: LyricsClient::new(client.clone()),
                ytdlp: ytdlp.clone(),
                library,
                deezer,
                collection,
                listens,
                discover,
                tracker: Mutex::new(ListenTracker::new()),
                searches,
                youtube: youtube.clone(),
                client: client.clone(),
                cache_path,
                settings: Mutex::new(prefs),
                settings_path,
                session_path,
            });

            // Touches multimédia et panneau multimédia du système.
            #[cfg(windows)]
            let hwnd = app
                .get_webview_window("main")
                .and_then(|w| w.hwnd().ok())
                .map(|h| h.0 as *mut std::ffi::c_void);
            #[cfg(all(desktop, not(windows)))]
            let hwnd = None;
            #[cfg(desktop)]
            let media = media::spawn(hwnd, app.path().app_cache_dir()?.join("now-playing"), controller.clone());
            // Statut Discord (« Écoute … »), s'il est configuré.
            #[cfg(desktop)]
            let discord = discord::spawn();
            // Android : notification du lecteur et lecture écran éteint ; pas de Discord.
            #[cfg(target_os = "android")]
            let media = android_media::spawn(app.handle(), app.path().app_cache_dir()?.join("now-playing"), controller.clone());
            #[cfg(target_os = "ios")]
            let media: Option<std::sync::mpsc::Sender<Snapshot>> = None;
            #[cfg(mobile)]
            let discord: Option<std::sync::mpsc::Sender<Snapshot>> = None;

            // Installation / mise à jour de yt-dlp en arrière-plan, état relayé à l'interface.
            let handle = app.handle().clone();
            let mut status = ytdlp.subscribe();
            tauri::async_runtime::spawn(async move {
                while status.changed().await.is_ok() {
                    let s = status.borrow_and_update().clone();
                    let _ = handle.emit("ytdlp", &s);
                }
            });
            tauri::async_runtime::spawn(async move { ytdlp.maintain().await });

            // Premier clic rapide : client YouTube Music chauffé (connexion, jetons), et le
            // morceau de la session reprise préparé.
            let (yt, c) = (youtube.clone(), controller.clone());
            let resumed: Vec<Track> = c.snapshot().track.into_iter().collect();
            tauri::async_runtime::spawn(async move {
                yt.warm_up().await;
                c.warm(resumed);
            });

            // Boucle d'état, 4 fois par seconde : enchaîne les morceaux (et les fondus), pousse
            // l'état à l'interface, note les écoutes et garde la session.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                let mut interval = tokio::time::interval(Duration::from_millis(250));
                let mut last: Option<Snapshot> = None;
                let mut last_discord: Option<Snapshot> = None;
                let mut recorded: Option<String> = None;
                let mut saved: Option<Session> = None;
                let mut saved_at = std::time::Instant::now();
                loop {
                    interval.tick().await;
                    controller.tick().await;
                    let snap = controller.snapshot();
                    if last.as_ref() != Some(&snap) {
                        if let Some(media) = &media {
                            let _ = media.send(snap.clone());
                        }
                        let _ = handle.emit("player", &snap);
                        last = Some(snap.clone());
                    }

                    // Discord : statut effacé quand il est désactivé dans les réglages.
                    if let Some(discord) = &discord {
                        let shown = if state.settings.lock().discord {
                            snap.clone()
                        } else {
                            Snapshot { track: None, status: yato_core::Status::Idle, ..snap.clone() }
                        };
                        if last_discord.as_ref() != Some(&shown) {
                            let _ = discord.send(shown.clone());
                            last_discord = Some(shown);
                        }
                    }

                    // Historique : un morceau compte comme écouté après 20 s (ou la moitié s'il
                    // est court), une fois par passage.
                    if let Some(track) = &snap.track {
                        if recorded.as_deref() != Some(track.id.as_str()) && snap.status == yato_core::Status::Playing {
                            let needed = snap.duration.map_or(20.0, |d| (d / 2.0).min(20.0));
                            if snap.position >= needed {
                                recorded = Some(track.id.clone());
                                if let Ok(c) = state.collection.record_play(track.clone()) {
                                    let _ = handle.emit("collection", &c);
                                }
                            }
                        }
                    }

                    // Stats : durée réellement écoutée de chaque passage ; l'écoute est notée quand
                    // le morceau change, s'il a été écouté 30 s (ou la moitié s'il est court).
                    let playing = snap.status == yato_core::Status::Playing;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis() as u64);
                    let done = state.tracker.lock().observe(snap.track.as_ref(), playing, snap.position, snap.duration, now);
                    if let Some((track, listen)) = done {
                        if let Err(e) = state.listens.record(&track, listen) {
                            eprintln!("stats : écoute non notée : {e}");
                        }
                    }
                    // Morceau sauté : noté pour ne pas le reproposer dans Découvrir.
                    let skipped = state.tracker.lock().take_skip();
                    if let Some((track, skip)) = skipped {
                        let _ = state.listens.record_skip(&track, skip);
                    }

                    // Session : enregistrée quand elle change, et toutes les 10 s pendant la lecture.
                    let session = controller.session();
                    let changed = saved.as_ref().is_none_or(|s| {
                        s.index != session.index || s.queue.len() != session.queue.len() || s.shuffle != session.shuffle
                            || s.repeat != session.repeat || s.volume != session.volume
                    });
                    if changed || (playing && saved_at.elapsed() >= Duration::from_secs(10)) {
                        if saved.as_ref() != Some(&session) {
                            state.save_session();
                        }
                        saved = Some(session);
                        saved_at = std::time::Instant::now();
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_sources,
            library_tracks,
            library_folders,
            add_folder,
            remove_folder,
            rescan,
            search,
            featured,
            discover,
            play_queue,
            play_index,
            toggle,
            next,
            previous,
            seek,
            set_volume,
            prefetch,
            warm,
            lyrics,
            artist,
            album,
            track_artists,
            ytdlp_status,
            player_state,
            queue,
            collection,
            toggle_like,
            create_playlist,
            rename_playlist,
            set_playlist_cover,
            delete_playlist,
            add_to_playlist,
            remove_from_playlist,
            move_in_playlist,
            clear_history,
            listening_stats,
            clear_stats,
            recent_searches,
            add_recent_search,
            remove_recent_search,
            clear_recent_searches,
            import_playlist,
            get_settings,
            set_settings,
            set_shuffle,
            set_repeat,
        ])
        .build(tauri::generate_context!())
        .expect("erreur au lancement de yato");
    app.run(|handle, event| {
        // En quittant : la position exacte, pour reprendre pile au même endroit.
        if let RunEvent::Exit = event {
            let state = handle.state::<AppState>();
            state.save_session();
            // L'écoute en cours compte aussi.
            let done = state.tracker.lock().finish();
            if let Some((track, listen)) = done {
                let _ = state.listens.record(&track, listen);
            }
        }
    });
}
