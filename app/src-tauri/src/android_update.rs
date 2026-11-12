//! Android : mises à jour intégrées (le plugin updater de Tauri n'existe que sur ordinateur).
//!
//! 1. `apk_update_check` lit la dernière version publiée dans les Releases GitHub ;
//! 2. `apk_update_install` télécharge son APK (`555_x.y.z_android.apk`, ajouté par le workflow de
//!    publication) puis le confie à Android (gen/android/…/UpdatePlugin.kt), qui vérifie qu'il
//!    est signé avec la même clé que l'appli installée avant de la remplacer.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Manager, Runtime, State,
    ipc::Channel,
    plugin::{Builder, PluginHandle, TauriPlugin},
};
use tokio::io::AsyncWriteExt;
use yato_core::{Error, reqwest};

use crate::{AppState, Res};

const LATEST: &str = "https://api.github.com/repos/amonre555/555/releases/latest";

struct Handle<R: Runtime>(PluginHandle<R>);

/// Le greffon Tauri qui charge la partie Kotlin (`UpdatePlugin`).
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("apkupdate")
        .setup(|app, api| {
            let handle = api.register_android_plugin("app.yato.player", "UpdatePlugin")?;
            app.manage(Handle(handle));
            Ok(())
        })
        .build()
}

/// Une version plus récente que celle installée.
#[derive(Serialize, Deserialize, Clone)]
pub struct ApkUpdate {
    version: String,
    url: String,
    size: u64,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Serialize, Clone)]
pub struct Progress {
    done: u64,
    total: u64,
}

/// `0.3.10` > `0.3.9` : comparaison nombre par nombre.
fn newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| v.trim_start_matches('v').split('.').map(|n| n.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parse(candidate) > parse(current)
}

fn update_dir<R: Runtime>(app: &AppHandle<R>) -> Res<PathBuf> {
    Ok(app.path().app_cache_dir().map_err(|e| Error::Other(e.to_string()))?.join("update"))
}

/// La dernière version publiée, si elle est plus récente et que son APK est déjà en ligne
/// (le workflow l'ajoute quelques minutes après les versions pour ordinateur).
#[tauri::command]
pub async fn apk_update_check<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> Res<Option<ApkUpdate>> {
    let release: Release = state
        .client
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let current = app.package_info().version.to_string();
    let version = release.tag_name.trim_start_matches('v').to_owned();
    let apk = release.assets.into_iter().find(|a| a.name.ends_with("_android.apk"));
    match apk {
        Some(a) if newer(&version, &current) => Ok(Some(ApkUpdate { version, url: a.browser_download_url, size: a.size })),
        _ => {
            // À jour : l'APK de la mise à jour précédente ne sert plus.
            if let Ok(dir) = update_dir(&app) {
                let _ = tokio::fs::remove_dir_all(dir).await;
            }
            Ok(None)
        }
    }
}

/// Télécharge l'APK (sauf s'il l'est déjà : nouvel essai après l'autorisation d'installer),
/// puis lance l'installation. Ne revient qu'en cas d'échec ou d'annulation : réussie, la mise à
/// jour remplace l'appli. Erreurs à part : `permission` (réglage ouvert, réessayer au retour) et
/// `cancelled` (« Annuler » dans la fenêtre d'Android).
#[tauri::command]
pub async fn apk_update_install<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    update: ApkUpdate,
    on_progress: Channel<Progress>,
) -> Res<()> {
    let dir = update_dir(&app)?;
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(format!("555_{}.apk", update.version));
    if !downloaded(&path, update.size).await {
        download(&state.client, &update, &path, &on_progress).await?;
    }
    let _ = on_progress.send(Progress { done: update.size, total: update.size });

    let handle = app.try_state::<Handle<R>>().ok_or_else(|| Error::Other("updater unavailable".into()))?.0.clone();
    #[derive(Serialize)]
    struct Install {
        path: String,
    }
    let payload = Install { path: path.to_string_lossy().into_owned() };
    tauri::async_runtime::spawn_blocking(move || handle.run_mobile_plugin::<serde_json::Value>("install", payload))
        .await
        .map_err(|e| Error::Other(e.to_string()))?
        .map_err(|e| match e {
            tauri::plugin::mobile::PluginInvokeError::InvokeRejected(r) => Error::Other(r.message.unwrap_or_default()),
            e => Error::Other(e.to_string()),
        })?;
    Ok(())
}

async fn downloaded(path: &Path, size: u64) -> bool {
    tokio::fs::metadata(path).await.is_ok_and(|m| m.len() == size)
}

async fn download(client: &reqwest::Client, update: &ApkUpdate, path: &Path, progress: &Channel<Progress>) -> Res<()> {
    let part = path.with_extension("part");
    let mut response = client
        .get(&update.url)
        .send()
        .await?
        .error_for_status()?;
    let total = response.content_length().unwrap_or(update.size);
    let mut file = tokio::fs::File::create(&part).await?;
    let (mut done, mut shown) = (0u64, 0u64);
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        // Au plus une centaine de messages vers l'interface.
        if done - shown >= total / 100 {
            shown = done;
            let _ = progress.send(Progress { done, total });
        }
    }
    file.flush().await?;
    drop(file);
    if done != update.size {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(Error::Other(format!("incomplete download ({done} / {} bytes)", update.size)));
    }
    tokio::fs::rename(&part, path).await?;
    Ok(())
}
