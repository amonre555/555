//! Test : paroles synchronisées des premiers résultats YouTube Music d'une recherche.
use yato_core::{MusicSource, lyrics::LyricsClient, sources::YouTubeMusicSource, ytdlp::YtDlp};

#[tokio::main]
async fn main() {
    let q = std::env::args().nth(1).unwrap_or_else(|| "mattyeux videoclub".into());
    let client = yato_core::http_client();
    let yt = YouTubeMusicSource::new(std::env::temp_dir().join("yato-yt"), YtDlp::new(std::env::temp_dir().join("yato-ytdlp"), client.clone())).unwrap();
    let lyrics = LyricsClient::new(client);
    for t in yt.search(&q, 6).await.unwrap() {
        let t0 = std::time::Instant::now();
        match lyrics.fetch(&t).await {
            Ok(Some(l)) if !l.lines.is_empty() => println!("✓ {} — {} : {} lignes synchro en {:?}, 1re : [{:.1}s] {}", t.title, t.artist, l.lines.len(), t0.elapsed(), l.lines.iter().find(|x| !x.text.is_empty()).map(|x| x.time).unwrap_or(0.0), l.lines.iter().find(|x| !x.text.is_empty()).map(|x| x.text.as_str()).unwrap_or("")),
            Ok(Some(l)) => println!("~ {} — {} : paroles simples ({} car.), instrumental={}", t.title, t.artist, l.plain.map(|p| p.len()).unwrap_or(0), l.instrumental),
            Ok(None) => println!("✗ {} — {} : aucune parole", t.title, t.artist),
            Err(e) => println!("! {} : {e}", t.title),
        }
    }
}
