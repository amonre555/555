//! Couche Tauri : expose le cœur (`ondes-core`) à l'interface. Aucune logique métier ici.

mod media;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use ondes_core::{
    ArtistRef, Controller, Error, Snapshot, SourceInfo, SourceRegistry, Track,
    lyrics::{Lyrics, LyricsClient},
    sources::{
        DeezerSource, LocalLibrary, YouTubeMusicSource,
        deezer::{AlbumPage, ArtistPage},
        read_cover,
    },
    ytdlp::{YtDlp, YtDlpStatus},
};
use percent_encoding::percent_decode_str;
use tauri::{Emitter, Manager, State, http};

type Res<T> = Result<T, Error>;

struct AppState {
    controller: Arc<Controller>,
    lyrics: LyricsClient,
    ytdlp: Arc<YtDlp>,
    library: Arc<LocalLibrary>,
    /// Source d'infos : fiches d'artistes et albums.
    deezer: Arc<DeezerSource>,
    cache_path: PathBuf,
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

#[tauri::command]
fn set_volume(state: State<'_, AppState>, volume: f32) -> Snapshot {
    state.controller.set_volume(volume);
    state.controller.snapshot()
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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("cover", |_ctx, request, responder| {
            let path = request.uri().path().to_owned();
            tauri::async_runtime::spawn_blocking(move || responder.respond(cover_response(&path)));
        })
        .setup(|app| {
            let cache_path = app.path().app_data_dir()?.join("library.json");
            let library = Arc::new(LocalLibrary::load(&cache_path));
            let client = ondes_core::http_client();

            // yt-dlp géré par Ondes : installé, vérifié (signature) et mis à jour automatiquement.
            let ytdlp = YtDlp::new(app.path().app_data_dir()?.join("bin").join("yt-dlp"), client.clone());
            let youtube = Arc::new(YouTubeMusicSource::new(app.path().app_cache_dir()?.join("youtube"), ytdlp.clone())?);
            // Deezer donne les infos (titres, albums, pochettes, top), YouTube Music le son.
            let deezer = Arc::new(DeezerSource::new(client.clone(), youtube.clone()));

            // L'ordre d'enregistrement = l'ordre des résultats de recherche.
            // YouTube reste cherchable : versions absentes de Deezer (remixes, lives…).
            let mut sources = SourceRegistry::new();
            sources.register(library.clone());
            sources.register(deezer.clone());
            sources.register(youtube);

            let controller = Controller::new(Arc::new(sources), client.clone())?;
            app.manage(AppState {
                controller: controller.clone(),
                lyrics: LyricsClient::new(client),
                ytdlp: ytdlp.clone(),
                library,
                deezer,
                cache_path,
            });

            // Touches multimédia et panneau multimédia du système.
            #[cfg(windows)]
            let hwnd = app
                .get_webview_window("main")
                .and_then(|w| w.hwnd().ok())
                .map(|h| h.0 as *mut std::ffi::c_void);
            #[cfg(not(windows))]
            let hwnd = None;
            let media = media::spawn(hwnd, app.path().app_cache_dir()?.join("now-playing"), controller.clone());

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

            // Boucle d'état : enchaîne les morceaux et pousse l'état à l'interface 4 fois par seconde.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(250));
                let mut last: Option<Snapshot> = None;
                loop {
                    interval.tick().await;
                    if controller.tick() {
                        let _ = controller.next().await;
                    }
                    let snap = controller.snapshot();
                    if last.as_ref() != Some(&snap) {
                        if let Some(media) = &media {
                            let _ = media.send(snap.clone());
                        }
                        let _ = handle.emit("player", &snap);
                        last = Some(snap);
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
            play_queue,
            play_index,
            toggle,
            next,
            previous,
            seek,
            set_volume,
            prefetch,
            lyrics,
            artist,
            album,
            track_artists,
            ytdlp_status,
            player_state,
            queue,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement d'Ondes");
}
