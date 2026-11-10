//! Test sans interface : yt-dlp géré (installé si besoin), recherche YouTube Music,
//! lecture au 1er clic puis lecture d'un morceau anticipé.
use yato_core::{MusicSource, StreamData, sources::YouTubeMusicSource, ytdlp::YtDlp};
use rodio::{Decoder, Source};
use std::time::{Duration, Instant};

async fn play(yt: &YouTubeMusicSource, t: &yato_core::Track, label: &str) {
    let t0 = Instant::now();
    let StreamData::Buffered { download, hint } = yt.resolve(t).await.expect("resolve") else { panic!() };
    let reader = download.reader().await.expect("flux");
    let ready = t0.elapsed();
    let len = reader.total_len();
    let (n, seek) = tokio::task::spawn_blocking(move || {
        let mut b = Decoder::builder().with_data(reader).with_seekable(true).with_hint(hint.unwrap());
        if let Some(l) = len { b = b.with_byte_len(l); }
        let mut dec = b.build().expect("décodeur");
        let rate = dec.sample_rate().get() as usize * dec.channels().get() as usize;
        let n = (&mut dec).take(rate * 3).count();
        let ts = Instant::now();
        let ok = dec.try_seek(Duration::from_secs(150)).is_ok() && dec.take(rate).count() > 0;
        (n, if ok { format!("{:?}", ts.elapsed()) } else { "échec".into() })
    }).await.unwrap();
    println!("{label} : « {} » prêt en {ready:?} ({n} échantillons), avance à 2:30 : {seek}", t.title);
}

#[tokio::main]
async fn main() {
    let q = std::env::args().nth(1).unwrap_or_else(|| "vhs mattyeux".into());
    let client = yato_core::http_client();
    let dir = std::env::args().nth(2).map(Into::into).unwrap_or_else(|| std::env::temp_dir().join("yato-ytdlp"));
    let ytdlp = YtDlp::new(dir, client.clone());
    let t = Instant::now();
    let path = ytdlp.path().await.expect("yt-dlp");
    println!("yt-dlp prêt en {:?} : {} ({:?})", t.elapsed(), path.display(), ytdlp.status());
    let yt = YouTubeMusicSource::new(std::env::temp_dir().join("yato-yt"), ytdlp).unwrap();
    let t = Instant::now();
    let tracks = yt.search(&q, 5).await.expect("recherche");
    println!("recherche « {q} » : {} résultats en {:?}", tracks.len(), t.elapsed());
    play(&yt, &tracks[0], "1er clic, sans anticipation").await;
    yt.prefetch(&tracks[2]).await;
    tokio::time::sleep(Duration::from_secs(3)).await; // l'utilisateur écoute pendant ce temps
    play(&yt, &tracks[2], "morceau anticipé").await;
}
