//! Test : importe une playlist depuis un lien (Spotify, Deezer, YouTube Music).
//! `cargo run -p yato-core --example import -- "https://open.spotify.com/playlist/…"`
use std::sync::Arc;
use yato_core::{import::import, sources::{DeezerSource, YouTubeMusicSource}, ytdlp::YtDlp};

#[tokio::main]
async fn main() {
    let client = yato_core::http_client();
    let yt = Arc::new(YouTubeMusicSource::new(std::env::temp_dir().join("yato-yt"), YtDlp::new(std::env::temp_dir().join("yato-ytdlp"), client.clone())).unwrap());
    let dz = Arc::new(DeezerSource::new(client.clone(), yt.clone()));
    for url in std::env::args().skip(1) {
        let t0 = std::time::Instant::now();
        match import(&url, &client, &dz, &yt, |_, _| {}).await {
            Ok(p) => println!(
                "✓ « {} » : {} morceaux, {} introuvables, en {:?}\n    1er : {} — {}",
                p.name, p.tracks.len(), p.missing, t0.elapsed(),
                p.tracks.first().map_or("", |t| t.title.as_str()), p.tracks.first().map_or("", |t| t.artist.as_str()),
            ),
            Err(e) => println!("✗ {url} : {e}"),
        }
    }
}
