//! Minutage des étapes du chargement d'un morceau (diagnostic).
//!
//! Désactivé par défaut. Avec la variable d'environnement `YATO_TIMING` :
//! - `YATO_TIMING=1` : les étapes s'affichent sur la sortie d'erreur ;
//! - `YATO_TIMING=<chemin de fichier>` : elles y sont ajoutées (pratique sous Windows, où
//!   l'appli n'a pas de console).

use std::{io::Write, sync::OnceLock, time::Instant};

enum Out {
    Off,
    Stderr,
    File(std::path::PathBuf),
}

fn out() -> &'static Out {
    static OUT: OnceLock<Out> = OnceLock::new();
    OUT.get_or_init(|| match std::env::var_os("YATO_TIMING") {
        None => Out::Off,
        Some(v) if v.is_empty() || v == "1" => Out::Stderr,
        Some(v) => Out::File(v.into()),
    })
}

/// Référence commune pour dater les étapes (premier appel = 0).
fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

pub fn enabled() -> bool {
    !matches!(out(), Out::Off)
}

/// Note l'étape `what`, commencée à `since`.
pub fn step(what: &str, since: Instant) {
    if !enabled() {
        return;
    }
    let line = format!(
        "[yato ⏱ {:>7.0} ms] {what} : {:.0} ms",
        epoch().elapsed().as_secs_f64() * 1000.0,
        since.elapsed().as_secs_f64() * 1000.0
    );
    match out() {
        Out::Off => {}
        Out::Stderr => eprintln!("{line}"),
        Out::File(p) => {
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                let _ = writeln!(f, "{line}");
            }
        }
    }
}

/// Démarre le chronomètre global (à appeler tôt, au lancement).
pub fn start() {
    let _ = epoch();
}
