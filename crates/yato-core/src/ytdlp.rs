//! Gestion automatique de yt-dlp : installation, vérification et mises à jour.
//!
//! - **Source** : les versions « nightly » officielles de yt-dlp, qui reçoivent les correctifs
//!   YouTube en premier.
//! - **Sécurité** : le fichier d'empreintes `SHA2-256SUMS` doit porter une signature OpenPGP
//!   valide de la clé officielle de yt-dlp (intégrée à yato), puis l'archive téléchargée doit
//!   avoir l'empreinte SHA-256 annoncée. Sinon, rien n'est installé.
//! - **Rapidité** : on prend la version « dossier » (zip), qui démarre en ~0,5 s au lieu de ~2 s
//!   pour l'exécutable unique (qui doit se décompresser à chaque lancement).
//! - **Mises à jour sans coupure** : chaque version va dans son propre sous-dossier, et
//!   `current` indique la version active. Un yt-dlp en cours d'exécution n'est jamais écrasé.
//! - **Secours** : si l'installation échoue (hors ligne…), on utilise un yt-dlp déjà présent
//!   sur le système (PATH, winget).

use std::{
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use parking_lot::Mutex;
use pgp::composed::{Deserializable, DetachedSignature, SignedPublicKey};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::watch;

use crate::{Error, Result};

const RELEASES: &str = "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download";
/// Clé publique officielle de yt-dlp (« yt-dlp signing key », id 57CF65933B5A7581).
const SIGNING_KEY: &str = include_str!("ytdlp_signing_key.asc");
/// Fréquence de vérification des mises à jour.
const UPDATE_EVERY: Duration = Duration::from_secs(12 * 3600);

/// État affiché dans l'interface.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum YtDlpStatus {
    /// Pas encore vérifié.
    Unknown,
    /// Téléchargement en cours (`progress` de 0 à 1).
    Installing { progress: f32 },
    /// Version gérée par yato, prête.
    Ready,
    /// Pas de version gérée, mais une version du système est utilisée.
    System { path: PathBuf },
    /// Échec, et aucun yt-dlp utilisable.
    Error { message: String },
}

pub struct YtDlp {
    /// Dossier géré : `<dir>/<empreinte>/…` et `<dir>/current`.
    dir: PathBuf,
    client: reqwest::Client,
    status: watch::Sender<YtDlpStatus>,
    /// Une seule installation à la fois.
    install_lock: tokio::sync::Mutex<()>,
    js_runtime: Mutex<Option<Option<Vec<String>>>>,
}

impl YtDlp {
    pub fn new(dir: PathBuf, client: reqwest::Client) -> Arc<Self> {
        let (status, _) = watch::channel(YtDlpStatus::Unknown);
        Arc::new(Self {
            dir,
            client,
            status,
            install_lock: tokio::sync::Mutex::new(()),
            js_runtime: Mutex::new(None),
        })
    }

    /// Client HTTP partagé.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub fn status(&self) -> YtDlpStatus {
        self.status.borrow().clone()
    }

    /// Pour suivre les changements d'état (installation, progression…).
    pub fn subscribe(&self) -> watch::Receiver<YtDlpStatus> {
        self.status.subscribe()
    }

    fn set(&self, s: YtDlpStatus) {
        self.status.send_replace(s);
    }

    /// Chemin de yt-dlp, en l'installant si besoin.
    pub async fn path(&self) -> Result<PathBuf> {
        if let Some(p) = self.managed() {
            if *self.status.borrow() == YtDlpStatus::Unknown {
                self.set(YtDlpStatus::Ready);
            }
            return Ok(p);
        }
        let _guard = self.install_lock.lock().await;
        if let Some(p) = self.managed() {
            return Ok(p);
        }
        match self.install(None).await {
            Ok(p) => Ok(p),
            Err(e) => self.fallback(e),
        }
    }

    /// À appeler au démarrage, en arrière-plan : installe yt-dlp s'il manque, sinon vérifie
    /// les mises à jour (au plus toutes les 12 h).
    pub async fn maintain(&self) {
        if self.managed().is_none() {
            if let Ok(p) = self.path().await {
                warm_exe(&p).await;
            }
            return;
        }
        self.set(YtDlpStatus::Ready);
        if !self.update_due() {
            if let Some(p) = self.managed() {
                warm_exe(&p).await;
            }
            return;
        }
        let _guard = self.install_lock.lock().await;
        let current = self.current_hash();
        if let Ok(sums) = self.signed_sums().await {
            self.touch_last_check();
            if let Some(latest) = hash_for(&sums, asset_name())
                && Some(latest.as_str()) != current.as_deref()
                && self.install(Some(sums)).await.is_err()
            {
                // Échec de la mise à jour : l'ancienne version reste en place.
                self.set(YtDlpStatus::Ready);
            }
        }
        self.cleanup();
        if let Some(p) = self.managed() {
            warm_exe(&p).await;
        }
    }

    /// Arguments à ajouter pour que yt-dlp utilise un moteur JavaScript du système
    /// (exigé par YouTube pour certains formats). Vide si aucun n'est trouvé.
    pub fn js_runtime_args(&self) -> Vec<String> {
        let mut cache = self.js_runtime.lock();
        cache
            .get_or_insert_with(|| {
                let found = find_program("deno", &winget_package_dirs("DenoLand.Deno_"))
                    .map(|p| ("deno", p))
                    .or_else(|| find_program("node", &[]).map(|p| ("node", p)))
                    .or_else(|| find_program("bun", &[]).map(|p| ("bun", p)));
                found.map(|(name, path)| vec!["--js-runtimes".into(), format!("{name}:{}", path.display())])
            })
            .clone()
            .unwrap_or_default()
    }

    // ---------- Installation ----------

    fn managed(&self) -> Option<PathBuf> {
        let hash = self.current_hash()?;
        find_executable(&self.dir.join(&hash[..16]))
    }

    fn current_hash(&self) -> Option<String> {
        let h = std::fs::read_to_string(self.dir.join("current")).ok()?;
        let h = h.trim();
        (h.len() == 64).then(|| h.to_owned())
    }

    async fn install(&self, sums: Option<String>) -> Result<PathBuf> {
        self.set(YtDlpStatus::Installing { progress: 0.0 });
        let t0 = std::time::Instant::now();
        let result = self.download_and_extract(sums).await;
        crate::timing::step("yt-dlp : téléchargement + vérification + extraction", t0);
        match &result {
            Ok(_) => {
                self.touch_last_check();
                self.set(YtDlpStatus::Ready);
                self.cleanup();
            }
            Err(e) => self.set(YtDlpStatus::Error { message: e.to_string() }),
        }
        result
    }

    async fn download_and_extract(&self, sums: Option<String>) -> Result<PathBuf> {
        let asset = asset_name();
        let sums = match sums {
            Some(s) => s,
            None => self.signed_sums().await?,
        };
        let expected = hash_for(&sums, asset)
            .ok_or_else(|| Error::Other(format!("yt-dlp: no checksum for {asset}")))?;

        // Téléchargement, avec progression.
        let mut resp = self.client.get(format!("{RELEASES}/{asset}")).send().await?.error_for_status()?;
        let total = resp.content_length().unwrap_or(0);
        let mut data = Vec::with_capacity(total as usize);
        while let Some(chunk) = resp.chunk().await? {
            data.extend_from_slice(&chunk);
            if total > 0 {
                self.set(YtDlpStatus::Installing { progress: data.len() as f32 / total as f32 });
            }
        }

        let actual = hex(&Sha256::digest(&data));
        if actual != expected {
            return Err(Error::Other(format!(
                "yt-dlp: wrong checksum, download rejected (expected {expected}, got {actual})"
            )));
        }

        // Décompression dans un dossier propre à cette version, puis activation.
        let dir = self.dir.clone();
        tokio::task::spawn_blocking(move || -> Result<PathBuf> {
            let target = dir.join(&expected[..16]);
            let tmp = dir.join(format!("{}.tmp", &expected[..16]));
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(&tmp)?;
            zip::ZipArchive::new(Cursor::new(data))
                .and_then(|mut z| z.extract(&tmp))
                .map_err(|e| Error::Other(format!("yt-dlp: unreadable archive ({e})")))?;
            let _ = std::fs::remove_dir_all(&target);
            std::fs::rename(&tmp, &target)?;
            let exe = find_executable(&target)
                .ok_or_else(|| Error::Other("yt-dlp: executable not found in the archive".into()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))?;
            }
            // Écriture atomique du pointeur de version.
            let cur_tmp = dir.join("current.tmp");
            std::fs::write(&cur_tmp, &expected)?;
            std::fs::rename(cur_tmp, dir.join("current"))?;
            Ok(exe)
        })
        .await
        .map_err(|e| Error::Other(e.to_string()))?
    }

    /// Télécharge `SHA2-256SUMS` et vérifie sa signature avec la clé officielle de yt-dlp.
    async fn signed_sums(&self) -> Result<String> {
        let get = |name: &str| {
            let url = format!("{RELEASES}/{name}");
            let client = self.client.clone();
            async move {
                Ok::<_, Error>(client.get(url).send().await?.error_for_status()?.bytes().await?)
            }
        };
        let (sums, sig) = tokio::try_join!(get("SHA2-256SUMS"), get("SHA2-256SUMS.sig"))?;
        verify_signature(&sums, &sig)?;
        String::from_utf8(sums.to_vec()).map_err(|e| Error::Other(e.to_string()))
    }

    fn fallback(&self, err: Error) -> Result<PathBuf> {
        match find_system_ytdlp() {
            Some(path) => {
                self.set(YtDlpStatus::System { path: path.clone() });
                Ok(path)
            }
            None => {
                self.set(YtDlpStatus::Error { message: err.to_string() });
                Err(err)
            }
        }
    }

    fn update_due(&self) -> bool {
        std::fs::metadata(self.dir.join("last_check"))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_none_or(|age| age > UPDATE_EVERY)
    }

    fn touch_last_check(&self) {
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(self.dir.join("last_check"), b"");
    }

    /// Supprime les anciennes versions (échoue sans gravité si l'une tourne encore).
    fn cleanup(&self) {
        let Some(hash) = self.current_hash() else { return };
        let keep = &hash[..16];
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for e in entries.flatten() {
                if e.path().is_dir() && e.file_name() != keep {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
    }
}

/// Lance une fois `yt-dlp --version`, en arrière-plan, pour que le premier vrai lancement soit
/// rapide : le tout premier démarrage d'un yt-dlp fraîchement installé est très lent (analyse
/// antivirus de chaque fichier sous Windows ou macOS, fichiers à charger depuis le disque,
/// Python à initialiser) : jusqu'à ~13 s mesurées sous macOS, contre ~1,5 s ensuite.
async fn warm_exe(program: &Path) {
    let t0 = std::time::Instant::now();
    let mut cmd = tokio::process::Command::new(program);
    cmd.arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let _ = tokio::time::timeout(Duration::from_secs(60), cmd.status()).await;
    crate::timing::step("yt-dlp : préchauffage (--version)", t0);
}

fn verify_signature(data: &[u8], sig: &[u8]) -> Result<()> {
    let bad = |e: pgp::errors::Error| Error::Other(format!("yt-dlp: invalid signature ({e})"));
    let (key, _) = SignedPublicKey::from_string(SIGNING_KEY).map_err(bad)?;
    let sig = DetachedSignature::from_bytes(sig).map_err(bad)?;
    sig.verify(&key, data).map_err(bad)
}

fn hash_for(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == asset).then(|| hash.to_ascii_lowercase())
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Version « dossier » de yt-dlp pour cette plateforme.
fn asset_name() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "aarch64") => "yt-dlp_win_arm64.zip",
        ("windows", "x86") => "yt-dlp_win_x86.zip",
        ("windows", _) => "yt-dlp_win.zip",
        ("macos", _) => "yt-dlp_macos.zip",
        ("linux", "aarch64") => "yt-dlp_linux_aarch64.zip",
        _ => "yt-dlp_linux.zip",
    }
}

/// L'exécutable principal d'une version extraite (`yt-dlp.exe`, `yt-dlp_linux`…).
fn find_executable(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name().is_some_and(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with("yt-dlp") && (!cfg!(windows) || n.ends_with(".exe"))
                })
        })
        .collect();
    files.sort();
    files.into_iter().next()
}

fn winget_package_dirs(prefix: &str) -> Vec<PathBuf> {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else { return Vec::new() };
    let winget = PathBuf::from(local).join("Microsoft").join("WinGet");
    let mut dirs = vec![winget.join("Links")];
    if let Ok(entries) = std::fs::read_dir(winget.join("Packages")) {
        dirs.extend(
            entries
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
                .map(|e| e.path()),
        );
    }
    dirs
}

/// Cherche un programme dans le PATH, puis dans les dossiers donnés.
fn find_program(name: &str, extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(extra_dirs.iter().cloned())
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

/// yt-dlp installé sur le système (PATH, ou winget sous Windows même avant que le PATH soit relu).
pub fn find_system_ytdlp() -> Option<PathBuf> {
    find_program("yt-dlp", &winget_package_dirs("yt-dlp.yt-dlp_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_key_parses() {
        SignedPublicKey::from_string(SIGNING_KEY).expect("clé valide");
    }

    /// Vraie vérification sur les fichiers officiels (réseau requis) : `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn official_sums_are_signed() {
        let y = YtDlp::new(std::env::temp_dir().join("yato-ytdlp-test"), reqwest::Client::new());
        let sums = y.signed_sums().await.expect("signature valide");
        assert!(hash_for(&sums, asset_name()).is_some());
    }

    #[tokio::test]
    #[ignore]
    async fn tampered_sums_are_rejected() {
        let c = reqwest::Client::new();
        let sums = c.get(format!("{RELEASES}/SHA2-256SUMS")).send().await.unwrap().bytes().await.unwrap();
        let sig = c.get(format!("{RELEASES}/SHA2-256SUMS.sig")).send().await.unwrap().bytes().await.unwrap();
        let mut tampered = sums.to_vec();
        tampered[0] ^= 1;
        assert!(verify_signature(&sums, &sig).is_ok());
        assert!(verify_signature(&tampered, &sig).is_err());
    }

    #[test]
    fn rejects_tampered_data() {
        assert!(verify_signature(b"faux contenu", b"pas une signature").is_err());
    }

    #[test]
    fn parses_sums() {
        let sums = "abc  yt-dlp.exe\nDEF  yt-dlp_win.zip\n";
        assert_eq!(hash_for(sums, "yt-dlp_win.zip").as_deref(), Some("def"));
        assert_eq!(hash_for(sums, "yt-dlp_macos.zip"), None);
    }
}
