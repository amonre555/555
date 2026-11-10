//! Test : paroles des premiers résultats Deezer de chaque recherche, et source du minutage des
//! mots (NetEase / Enhanced LRC, ou estimé).
//! `cargo run -p yato-core --example lyrics -- "blinding lights" "alors on danse stromae"`
use std::sync::Arc;
use yato_core::{MusicSource, lyrics::LyricsClient, sources::{DeezerSource, YouTubeMusicSource}, ytdlp::YtDlp};

#[tokio::main]
async fn main() {
    let mut queries: Vec<String> = std::env::args().skip(1).collect();
    if queries.is_empty() {
        queries.push("mattyeux videoclub".into());
    }
    let client = yato_core::http_client();
    let ytdlp = YtDlp::new(std::env::temp_dir().join("yato-ytdlp"), client.clone());
    let yt = Arc::new(YouTubeMusicSource::new(std::env::temp_dir().join("yato-yt"), ytdlp).unwrap());
    let deezer = DeezerSource::new(client.clone(), yt);
    let lyrics = LyricsClient::new(client);
    for q in queries {
        for t in deezer.search(&q, 2).await.unwrap() {
            let t0 = std::time::Instant::now();
            match lyrics.fetch(&t).await {
                Ok(Some(l)) if !l.lines.is_empty() => {
                    let first = l.lines.iter().find(|x| !x.words.is_empty());
                    println!(
                        "✓ {} — {} : {} lignes, mots {} en {:?}",
                        t.title, t.artist, l.lines.len(), if l.word_synced { "SYNCHRONISÉS" } else { "estimés" }, t0.elapsed(),
                    );
                    if let Some(line) = first {
                        let words: Vec<String> = line.words.iter().map(|w| format!("{}@{:.2}", w.text.trim(), w.time)).collect();
                        println!("    [{:.2}–{:.2}] {}", line.time, line.end, words.join(" "));
                    }
                }
                Ok(Some(l)) => println!("~ {} — {} : paroles simples ({} car.), instrumental={}", t.title, t.artist, l.plain.map(|p| p.len()).unwrap_or(0), l.instrumental),
                Ok(None) => println!("✗ {} — {} : aucune parole", t.title, t.artist),
                Err(e) => println!("! {} : {e}", t.title),
            }
        }
    }
}
