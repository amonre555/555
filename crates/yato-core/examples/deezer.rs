//! Test sans interface : recherche Deezer, puis lecture du son trouvé sur YouTube Music
//! (au 1er clic, puis pour un morceau anticipé).
use std::{sync::Arc, time::{Duration, Instant}};
use yato_core::{MusicSource, StreamData, sources::{DeezerSource, YouTubeMusicSource}, ytdlp::YtDlp};
use rodio::{Decoder, Source};

async fn play(src: &DeezerSource, t: &yato_core::Track, label: &str) {
    let t0 = Instant::now();
    let StreamData::Buffered { download, hint } = src.resolve(t).await.expect("resolve") else { panic!() };
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
        let ok = dec.try_seek(Duration::from_secs(90)).is_ok() && dec.take(rate).count() > 0;
        (n, if ok { format!("{:?}", ts.elapsed()) } else { "échec".into() })
    }).await.unwrap();
    println!("{label} : « {} » de {} prêt en {ready:?} ({n} échantillons), avance à 1:30 : {seek}", t.title, t.artist);
}

#[tokio::main]
async fn main() {
    let q = std::env::args().nth(1).unwrap_or_else(|| "stromae alors on danse".into());
    let client = yato_core::http_client();
    let ytdlp = YtDlp::new(std::env::temp_dir().join("yato-ytdlp"), client.clone());
    let yt = Arc::new(YouTubeMusicSource::new(std::env::temp_dir().join("yato-yt"), ytdlp).unwrap());
    let dz = DeezerSource::new(client, yt);
    let t = Instant::now();
    let tracks = dz.search(&q, 5).await.expect("recherche");
    println!("recherche Deezer « {q} » : {} résultats en {:?}, pochette {:?}", tracks.len(), t.elapsed(), tracks[0].artwork);
    play(&dz, &tracks[0], "1er clic, sans anticipation").await;
    dz.prefetch(&tracks[1]).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    play(&dz, &tracks[1], "morceau anticipé").await;
}
