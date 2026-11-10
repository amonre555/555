//! Test : artistes d'un morceau, fiche d'un artiste (par nom) et morceaux d'un album.
use std::sync::Arc;
use yato_core::{MusicSource, sources::{DeezerSource, YouTubeMusicSource}, ytdlp::YtDlp};

#[tokio::main]
async fn main() {
    let q = std::env::args().nth(1).unwrap_or_else(|| "sweat snoop dogg david guetta".into());
    let client = yato_core::http_client();
    let yt = Arc::new(YouTubeMusicSource::new(std::env::temp_dir().join("yato-yt"), YtDlp::new(std::env::temp_dir().join("yato-ytdlp"), client.clone())).unwrap());
    let dz = DeezerSource::new(client, yt);
    let t = &dz.search(&q, 1).await.unwrap()[0];
    println!("« {} » - recherche : {:?}", t.title, t.artists);
    let full = dz.track_artists(t).await.unwrap();
    println!("crédits complets : {full:?}");
    // Album retrouvé par son titre, comme pour un morceau YouTube ou local.
    let found = dz.album(None, "Discovery", Some("Daft Punk")).await.unwrap();
    println!("album par titre : « {} » de {:?}, {} morceaux, album_id du 1er : {:?}", found.title, found.artists.iter().map(|a| &a.name).collect::<Vec<_>>(), found.tracks.len(), found.tracks[0].album_id);
    println!("album_id du morceau cherché : {:?}", t.album_id);
    for a in &full {
        let t0 = std::time::Instant::now();
        let p = dz.artist(a.id.as_deref(), &a.name).await.unwrap();
        println!("\n{} (id {}) : {} titres populaires, {} albums, {} similaires, en {:?}", p.name, p.id, p.top.len(), p.albums.len(), p.related.len(), t0.elapsed());
        for x in p.top.iter().take(3) { println!("  ♪ {} - {:?}", x.title, x.artists.iter().map(|a| &a.name).collect::<Vec<_>>()); }
        for al in p.albums.iter().take(3) { println!("  ◼ {} ({:?}, {})", al.title, al.year, al.kind); }
        if let Some(al) = p.albums.first() {
            let al = dz.album(Some(&al.id), &al.title, None).await.unwrap();
            println!("  album « {} » ({:?}, {}, {:?}) : {} morceaux, {:?} s, artistes {:?}", al.title, al.year, al.kind, al.label, al.tracks.len(), al.duration, al.artists.iter().map(|a| &a.name).collect::<Vec<_>>());
        }
    }
}
