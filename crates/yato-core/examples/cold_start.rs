//! Mesure du premier chargement, comme dans l'appli : lancement, top Deezer affiché, clic sur
//! le premier morceau (à froid), puis un morceau anticipé.
//!
//! `YATO_TIMING=1 cargo run --release -p yato-core --example cold_start -- <dossier>`
//! - dossier vide ou absent : premier lancement (yt-dlp pas encore installé, caches vides) ;
//! - même dossier une 2e fois : relancement (yt-dlp installé, jetons YouTube en cache).
use std::{sync::Arc, time::{Duration, Instant}};
use rodio::{Decoder, Source};
use yato_core::{MusicSource, StreamData, Track, sources::{DeezerSource, YouTubeMusicSource}, ytdlp::YtDlp};

/// Du clic au premier son décodé.
async fn play(src: &DeezerSource, t: &Track, label: &str) -> Duration {
    let t0 = Instant::now();
    let StreamData::Buffered { download, hint } = src.resolve(t).await.expect("resolve") else { panic!() };
    let mut reader = download.reader().await.expect("flux");
    let len = reader.total_len();
    let quick = std::env::var_os("YATO_NO_QUICK_OPEN").is_none();
    let total = tokio::task::spawn_blocking(move || {
        let _q = quick.then(|| reader.quick_open_mp4());
        let mut b = Decoder::builder().with_data(reader).with_seekable(true).with_hint(hint.unwrap());
        if let Some(l) = len {
            b = b.with_byte_len(l);
        }
        let mut dec = b.build().expect("décodeur");
        let _ = dec.next();
        let total = t0.elapsed();
        // Avance rapide pendant le téléchargement, puis retour au début.
        let ts = Instant::now();
        let fwd = dec.try_seek(Duration::from_secs(60)).is_ok() && dec.next().is_some();
        let back = dec.try_seek(Duration::from_secs(1)).is_ok() && dec.next().is_some();
        let n = (&mut dec).take(48_000).filter(|s| *s != 0.0).count();
        println!("    avance à 1:00 {fwd}, retour à 0:01 {back} ({:?}), {n} échantillons non nuls", ts.elapsed());
        total
    })
    .await
    .unwrap();
    println!("==> {label} : « {} » — premier son en {total:?}", t.title);
    total
}

#[tokio::main]
async fn main() {
    yato_core::timing::start();
    let dir = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("yato-cold-{}", std::process::id())));
    println!("dossier : {}", dir.display());
    let launch = Instant::now();
    let client = yato_core::http_client();
    let ytdlp = YtDlp::new(dir.join("bin").join("yt-dlp"), client.clone());
    let yt = Arc::new(YouTubeMusicSource::new(dir.join("youtube"), ytdlp.clone()).unwrap());
    let dz = Arc::new(DeezerSource::new(client, yt.clone()));
    let warm = std::env::var_os("YATO_NO_WARM").is_none();
    // Comme l'appli : entretien de yt-dlp (et préparation) en arrière-plan dès le lancement.
    {
        let y = ytdlp.clone();
        tokio::spawn(async move { y.maintain().await });
    }
    if warm {
        let yt = yt.clone();
        tokio::spawn(async move { yt.warm_up().await });
    }
    let t = Instant::now();
    let featured = dz.featured(10).await.expect("top");
    println!("top Deezer affiché en {:?}", t.elapsed());
    if warm {
        // Comme l'interface : préparation légère des premiers morceaux du top.
        let (dz, list) = (dz.clone(), featured.clone());
        tokio::spawn(async move {
            for t in list.iter().take(8) {
                dz.warm(t).await;
            }
        });
    }
    // Délai avant le clic (s) : par défaut tout de suite, le pire cas.
    let wait: f64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    tokio::time::sleep(Duration::from_secs_f64(wait)).await;
    println!("clic à {:?} après le lancement", launch.elapsed());
    play(&dz, &featured[0], "1er clic (à froid)").await;
    play(&dz, &featured[2], "2e morceau, pas anticipé").await;
    play(&dz, &featured[9], "morceau hors des 8 préparés").await;
    dz.prefetch(&featured[1]).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    play(&dz, &featured[1], "morceau anticipé").await;

    // Le vrai lecteur (volume 0 : rien ne s'entend), sur un morceau préparé puis un autre.
    let mut reg = yato_core::SourceRegistry::new();
    reg.register(dz.clone());
    let c = yato_core::Controller::new(Arc::new(reg), yato_core::http_client()).expect("sortie audio");
    c.set_volume(0.0);
    for i in [3usize, 4] {
        let t0 = Instant::now();
        c.play_queue(featured.clone(), i).await.expect("lecture");
        let s = c.snapshot();
        println!("==> lecteur : « {} » {:?} en {:?}", featured[i].title, s.status, t0.elapsed());
        tokio::time::sleep(Duration::from_secs(2)).await;
        let s = c.snapshot();
        println!("    2 s plus tard : {:?} à {:.1} s / {:?}", s.status, s.position, s.duration);
        c.seek(100.0).expect("avance");
        tokio::time::sleep(Duration::from_millis(500)).await;
        println!("    après avance à 1:40 : {:.1} s", c.snapshot().position);
    }
}
