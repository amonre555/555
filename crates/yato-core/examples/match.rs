//! Test : relie des morceaux Deezer (top + recherches) à YouTube Music et affiche le résultat.
use std::sync::Arc;
use yato_core::{MusicSource, sources::{DeezerSource, YouTubeMusicSource}, ytdlp::YtDlp};

#[tokio::main]
async fn main() {
    let client = yato_core::http_client();
    let yt = Arc::new(YouTubeMusicSource::new(std::env::temp_dir().join("yato-yt"), YtDlp::new(std::env::temp_dir().join("yato-ytdlp"), client.clone())).unwrap());
    let dz = DeezerSource::new(client.clone(), yt.clone());
    let rp = rustypipe::client::RustyPipe::builder().storage_dir(std::env::temp_dir().join("yato-yt")).build().unwrap();
    // Recherches données en argument, sinon le top Deezer et quelques recherches connues.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut tracks = if args.is_empty() { dz.featured(12).await.unwrap() } else { Vec::new() };
    let queries: Vec<String> = if args.is_empty() {
        ["serebro мало тебя", "sweat snoop dogg david guetta", "daft punk get lucky", "stromae alors on danse", "bohemian rhapsody"].map(String::from).to_vec()
    } else {
        args
    };
    for q in &queries {
        tracks.extend(dz.search(q, 1).await.unwrap());
    }
    let (mut ok, n) = (0, tracks.len());
    for t in &tracks {
        let t0 = std::time::Instant::now();
        match yt.find_match(&t.title, &t.artist, t.duration).await {
            Ok(id) => {
                ok += 1;
                let d = rp.query().music_details(&id).await.ok();
                let (name, artists, dur) = d.map(|d| (d.track.name, d.track.artists.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", "), d.track.duration)).unwrap_or_default();
                println!("✓ {} - {} ({:?}s)\n    → {} - {} ({:?}s) en {:?}", t.title, t.artist, t.duration, name, artists, dur, t0.elapsed());
            }
            Err(e) => println!("✗ {} - {} : {e}", t.title, t.artist),
        }
    }
    println!("{ok}/{n} trouvés");
    println!("pochette exemple : {:?}", tracks[0].artwork);
}
