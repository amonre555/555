//! Test rapide : cherche sur Audius, ouvre le flux et décode quelques secondes (sans jouer de son).
use ondes_core::{MusicSource, StreamData, sources::AudiusSource, stream::Download};
use rodio::{Decoder, Source};

#[tokio::main]
async fn main() {
    let client = reqwest::Client::new();
    let audius = AudiusSource::new(client.clone());
    let t0 = std::time::Instant::now();
    let tracks = audius.featured(5).await.expect("tendances");
    println!("{} morceaux en {:?}", tracks.len(), t0.elapsed());
    let track = &tracks[0];
    println!("-> {} — {} ({:?} s)", track.title, track.artist, track.duration);
    let StreamData::Http { url, .. } = audius.resolve(track).await.unwrap() else { panic!() };
    let t1 = std::time::Instant::now();
    let reader = Download::http(&client, &url, None, None).await.expect("flux").reader().await.expect("flux");
    println!("pré-chargement prêt en {:?} (taille {:?})", t1.elapsed(), reader.total_len());
    let n = tokio::task::spawn_blocking(move || {
        let len = reader.total_len();
        let mut b = Decoder::builder().with_data(reader).with_seekable(true);
        if let Some(l) = len { b = b.with_byte_len(l); }
        let dec = b.build().expect("décodeur");
        println!("format : {} Hz, {} canaux, durée {:?}", dec.sample_rate(), dec.channels(), dec.total_duration());
        let rate = dec.sample_rate().get() as usize * dec.channels().get() as usize;
        dec.take(rate * 5).count()
    }).await.unwrap();
    println!("{n} échantillons décodés (5 s), total {:?}", t1.elapsed());
}
