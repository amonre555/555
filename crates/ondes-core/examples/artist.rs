//! Test : artistes d'un morceau, fiche d'un artiste (par nom) et morceaux d'un album.
use std::sync::Arc;
use ondes_core::{MusicSource, sources::{DeezerSource, YouTubeMusicSource}, ytdlp::YtDlp};

#[tokio::main]
async fn main() {
    let q = std::env::args().nth(1).unwrap_or_else(|| "sweat snoop dogg david guetta".into());
    let client = ondes_core::http_client();
    let yt = Arc::new(YouTubeMusicSource::new(std::env::temp_dir().join("ondes-yt"), YtDlp::new(std::env::temp_dir().join("ondes-ytdlp"), client.clone())).unwrap());
    let dz = DeezerSource::new(client, yt);
    let t = &dz.search(&q, 1).await.unwrap()[0];
    println!("« {} » — recherche : {:?}", t.title, t.artists);
    let full = dz.track_artists(t).await.unwrap();
    println!("crédits complets : {full:?}");
    for a in &full {
        let t0 = std::time::Instant::now();
        let p = dz.artist(a.id.as_deref(), &a.name).await.unwrap();
        println!("\n{} (id {}) : {:?} fans, {} titres populaires, {} albums, {} similaires, en {:?}", p.name, p.id, p.fans, p.top.len(), p.albums.len(), p.related.len(), t0.elapsed());
        for x in p.top.iter().take(3) { println!("  ♪ {} — {:?}", x.title, x.artists.iter().map(|a| &a.name).collect::<Vec<_>>()); }
        for al in p.albums.iter().take(3) { println!("  ◼ {} ({:?}, {})", al.title, al.year, al.kind); }
        if let Some(al) = p.albums.first() {
            let tr = dz.album_tracks(&al.id).await.unwrap();
            println!("  album « {} » : {} morceaux, 1er : {} (pochette {})", al.title, tr.len(), tr.first().map(|x| x.title.as_str()).unwrap_or("-"), tr.first().and_then(|x| x.artwork.as_ref()).is_some());
        }
    }
}
