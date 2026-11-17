//! Pochette du morceau en cours pour le système (panneau multimédia, notification Android).

use std::path::Path;

use yato_core::{Location, Track, sources::read_cover};

/// Pochette pour le système : URL distante, ou fichier extrait pour les morceaux locaux.
pub fn cover_url(t: &Track, cover_dir: &Path) -> Option<String> {
    if let Some(url) = &t.artwork {
        return Some(url.clone());
    }
    let Location::File { path } = &t.location else { return None };
    if !t.has_cover {
        return None;
    }
    let (bytes, mime) = read_cover(path)?;
    let ext = if mime.contains("png") { "png" } else { "jpg" };
    // Un nom par morceau (le système garde parfois l'ancienne image en cache), en ne gardant
    // que la pochette en cours.
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.id.hash(&mut h);
    let file = cover_dir.join(format!("cover-{:016x}.{ext}", h.finish()));
    let _ = std::fs::remove_dir_all(cover_dir);
    std::fs::create_dir_all(cover_dir).ok()?;
    std::fs::write(&file, bytes).ok()?;
    Some(format!("file://{}", file.display().to_string().replace('\\', "/")))
}

/// iPhone : l'écran verrouillé n'affiche que des pochettes en fichier local. Une pochette distante
/// est donc téléchargée dans `cover_dir` (en ne gardant que celle en cours). Appelé depuis le fil
/// des contrôles multimédia, jamais depuis le fil principal.
#[cfg(target_os = "ios")]
pub fn local_cover(url: &str, cover_dir: &Path) -> Option<String> {
    if !url.starts_with("http") {
        return Some(url.to_owned());
    }
    let bytes = tauri::async_runtime::block_on(async {
        let response = yato_core::http_client()
            .get(url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        response.bytes().await.ok()
    })?;
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    let file = cover_dir.join(format!("remote-{:016x}.img", h.finish()));
    let _ = std::fs::remove_dir_all(cover_dir);
    std::fs::create_dir_all(cover_dir).ok()?;
    std::fs::write(&file, bytes).ok()?;
    Some(format!("file://{}", file.display()))
}
