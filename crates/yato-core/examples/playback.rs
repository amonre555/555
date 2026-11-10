//! Test du moteur de lecture, à volume zéro (rien ne s'entend) : fondu enchaîné, répétition,
//! aléatoire et reprise de session, sur deux sons générés de 10 s.
//! `cargo run -p yato-core --example playback`
use std::{io::Write, path::Path, sync::Arc, time::{Duration, Instant}};
use yato_core::{Controller, Location, Repeat, SourceRegistry, Status, Track, sources::LocalLibrary};

/// Écrit un fichier WAV mono 16 bits : une note de `freq` Hz pendant `secs` secondes.
fn tone(path: &Path, freq: f32, secs: f32) {
    let rate = 44_100u32;
    let n = (rate as f32 * secs) as u32;
    let mut f = std::fs::File::create(path).unwrap();
    let data = n * 2;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&(rate * 2).to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();
    for i in 0..n {
        let v = ((i as f32 / rate as f32 * freq * std::f32::consts::TAU).sin() * 8000.0) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
    }
}

fn track(path: &Path, name: &str) -> Track {
    Track {
        id: format!("local:{name}"), source: "local".into(), title: name.into(), artist: "Test".into(), album: None, album_id: None,
        duration: Some(10.0), track_number: None, disc_number: None, artwork: None, has_cover: false,
        location: Location::File { path: path.to_owned() }, artists: Vec::new(),
    }
}

async fn run_for(c: &Arc<Controller>, secs: f64, label: &str) {
    let t0 = Instant::now();
    let mut last = String::new();
    while t0.elapsed().as_secs_f64() < secs {
        c.tick().await;
        let s = c.snapshot();
        let now = format!("{:?} {}", s.status, s.track.as_ref().map_or("—", |t| t.title.as_str()));
        if now != last {
            println!("  [{label} {:>5.2}s] {now} (position {:.2})", t0.elapsed().as_secs_f64(), s.position);
            last = now;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::main]
async fn main() {
    let dir = std::env::temp_dir().join("yato-playback-test");
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("a.wav"), dir.join("b.wav"));
    tone(&a, 440.0, 10.0);
    tone(&b, 660.0, 10.0);
    let mut sources = SourceRegistry::new();
    sources.register(Arc::new(LocalLibrary::new()));
    let c = Controller::new(Arc::new(sources), yato_core::http_client()).unwrap();
    c.set_volume(0.0); // silencieux

    println!("1. Fondu enchaîné de 3 s : B doit devenir le morceau en cours vers 7 s, pas à 10 s.");
    c.set_crossfade(3.0);
    c.play_queue(vec![track(&a, "A"), track(&b, "B")], 0).await.unwrap();
    run_for(&c, 9.0, "fondu").await;

    println!("2. Répéter ce morceau, sans fondu : B recommence à la fin.");
    c.set_crossfade(0.0);
    c.set_repeat(Repeat::One);
    run_for(&c, 9.0, "répétition").await;

    println!("3. Session : on l'enregistre, puis on la reprend sur un lecteur neuf.");
    c.seek(5.0).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let session = c.session();
    println!("  enregistrée : morceau {:?}, position {:.1} s, répétition {:?}", session.index, session.position, session.repeat);
    c.stop();
    let mut sources = SourceRegistry::new();
    sources.register(Arc::new(LocalLibrary::new()));
    let d = Controller::new(Arc::new(sources), yato_core::http_client()).unwrap();
    d.restore(session.clone());
    let s = d.snapshot();
    println!("  reprise : {:?} {} à {:.1} s, volume {}", s.status, s.track.as_ref().unwrap().title, s.position, s.volume);
    d.toggle().await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let s = d.snapshot();
    println!("  après « lecture » : {:?} à {:.1} s (attendu : vers {:.1} s)", s.status, s.position, session.position);
    assert_eq!(s.status, Status::Playing);
    assert!((s.position - session.position).abs() < 1.5);
    println!("OK");
}
