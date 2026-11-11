//! Faisabilité Android : peut-on lire un morceau YouTube Music EN ENTIER sans yt-dlp ?
//!
//! Pour chaque morceau et chaque client YouTube de rustypipe, on obtient l'adresse directe du
//! son puis on la télécharge de plusieurs façons, jusqu'au bout. Les fichiers complets sont
//! ensuite décodés en entier (symphonia), pour vérifier que le son est bon jusqu'à la fin.
//!
//! `cargo run --release -p yato-core --example no_ytdlp -- "requête 1" "requête 2"`
use std::{io::Cursor, time::{Duration, Instant}};

use rodio::{Decoder, Source};
use rustypipe::{client::{ClientType, RustyPipe}, model::AudioStream};

const CHUNK: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug)]
enum Method {
    /// En-tête `Range`, morceaux de 1 Mo, agent utilisateur de yato (ce que fait l'appli).
    RangeHeaderYatoUa,
    /// En-tête `Range`, morceaux de 1 Mo, agent utilisateur du client YouTube.
    RangeHeaderClientUa,
    /// Paramètre `&range=a-b` dans l'adresse (comme le lecteur web), agent du client.
    RangeParamClientUa,
    /// Une seule requête pour tout le fichier, agent du client.
    SingleGet,
}

async fn fetch(http: &reqwest::Client, url: &str, size: u64, ua: &str, m: Method) -> (Vec<u8>, String) {
    let mut buf = Vec::with_capacity(size as usize);
    let ua = match m { Method::RangeHeaderYatoUa => concat!("yato/", env!("CARGO_PKG_VERSION")), _ => ua };
    while (buf.len() as u64) < size {
        let start = buf.len() as u64;
        let end = (start + CHUNK).min(size) - 1;
        let req = match m {
            Method::RangeHeaderYatoUa | Method::RangeHeaderClientUa => {
                http.get(url).header("Range", format!("bytes={start}-{end}"))
            }
            Method::RangeParamClientUa => http.get(format!("{url}&range={start}-{end}")),
            Method::SingleGet => http.get(url),
        };
        let resp = match req.header("User-Agent", ua).send().await {
            Ok(r) => r,
            Err(e) => return (buf, format!("réseau à {} Ko : {e}", start / 1024)),
        };
        let status = resp.status();
        if !status.is_success() {
            return (buf, format!("HTTP {} à {} Ko", status.as_u16(), start / 1024));
        }
        let mut resp = resp;
        let before = buf.len();
        loop {
            match resp.chunk().await {
                Ok(Some(b)) => buf.extend_from_slice(&b),
                Ok(None) => break,
                Err(e) => { let n = buf.len() / 1024; return (buf, format!("coupé à {n} Ko : {e}")) }
            }
        }
        if buf.len() == before {
            return (buf, format!("réponse vide à {} Ko", start / 1024));
        }
    }
    buf.truncate(size as usize);
    (buf, "complet".into())
}

/// Durée réellement décodée (en secondes), en lisant tout le fichier.
fn decode_all(data: Vec<u8>, hint: &str) -> Result<f64, String> {
    let len = data.len() as u64;
    let dec = Decoder::builder()
        .with_data(Cursor::new(data))
        .with_hint(hint)
        .with_byte_len(len)
        .with_seekable(true)
        .build()
        .map_err(|e| e.to_string())?;
    let per_sec = dec.sample_rate().get() as f64 * dec.channels().get() as f64;
    Ok(dec.count() as f64 / per_sec)
}

fn pick(streams: &[AudioStream]) -> Option<&AudioStream> {
    streams.iter().find(|a| a.itag == 140).or_else(|| streams.iter().find(|a| a.mime.contains("mp4")))
}

const VISIONOS_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

/// Le client « visionOS » qu'utilise yt-dlp sans moteur JavaScript : adresses déjà signées.
async fn visionos(http: &reqwest::Client, id: &str, visitor: Option<&str>) -> Result<(String, u64), String> {
    let body = serde_json::json!({
        "context": {"client": {
            "clientName": "VISIONOS", "clientVersion": "1.02", "deviceMake": "Apple",
            "deviceModel": "RealityDevice17,1", "userAgent": VISIONOS_UA, "osName": "visionOS",
            "osVersion": "26.5.23O471", "hl": "en", "timeZone": "UTC", "utcOffsetMinutes": 0
        }},
        "videoId": id,
        "contentCheckOk": true, "racyCheckOk": true
    });
    let mut req = http.post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false")
        .header("User-Agent", VISIONOS_UA)
        .header("X-Youtube-Client-Name", "101")
        .header("X-Youtube-Client-Version", "1.02")
        .header("Origin", "https://www.youtube.com")
        .json(&body);
    if let Some(v) = visitor { req = req.header("X-Goog-Visitor-Id", v); }
    let v: serde_json::Value = req.send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
    let status = v["playabilityStatus"]["status"].as_str().unwrap_or("?").to_owned();
    let fmts = v["streamingData"]["adaptiveFormats"].as_array().ok_or(format!("pas de flux (statut {status})"))?;
    let f = fmts.iter().find(|f| f["itag"] == 140).ok_or("pas d'itag 140")?;
    let url = f["url"].as_str().ok_or("adresse chiffrée (signatureCipher)")?.to_owned();
    let size = f["contentLength"].as_str().and_then(|s| s.parse().ok()).ok_or("taille inconnue")?;
    Ok((url, size))
}

#[tokio::main]
async fn main() {
    let mut queries: Vec<String> = std::env::args().skip(1).collect();
    if queries.is_empty() {
        queries = ["vhs mattyeux", "daft punk one more time", "aya nakamura djadja"].map(String::from).to_vec();
    }
    let rp = RustyPipe::builder().storage_dir(std::env::temp_dir().join("yato-noytdlp")).build().unwrap();
    let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(120)).build().unwrap();
    let clients = [ClientType::Ios, ClientType::Tv, ClientType::Android, ClientType::DesktopMusic, ClientType::Desktop, ClientType::Mobile];
    let methods = [Method::RangeHeaderYatoUa, Method::RangeHeaderClientUa, Method::RangeParamClientUa];
    let _ = Method::SingleGet; // une seule requête : YouTube la bride (~35 Ko/s), testé à part

    for q in &queries {
        let res = rp.query().music_search_tracks(q).await.expect("recherche");
        let Some(t) = res.items.items.first() else { println!("« {q} » : aucun résultat"); continue };
        println!("\n=== « {} » – {} ({}, {} s) ===", t.name, t.artists.first().map(|a| a.name.as_str()).unwrap_or("?"), t.id, t.duration.unwrap_or(0));
        // (nom, adresse, taille, agent utilisateur)
        let mut candidates: Vec<(String, String, u64, String)> = Vec::new();
        for c in clients {
            let t0 = Instant::now();
            match rp.query().player_from_client(&t.id, c).await {
                Ok(p) => match pick(&p.audio_streams) {
                    Some(a) => {
                        println!("{c:?}: adresse en {:?}, itag {} {} Ko", t0.elapsed(), a.itag, a.size / 1024);
                        candidates.push((format!("{c:?}"), a.url.clone(), a.size, rp.query().user_agent(c).into_owned()));
                    }
                    None => println!("{c:?}: pas de flux m4a ({} flux audio)", p.audio_streams.len()),
                },
                Err(e) => println!("{c:?}: player refusé ({e})"),
            }
        }
        for with_visitor in [false, true] {
            let visitor = if with_visitor { rp.query().get_visitor_data(false).await.ok() } else { None };
            let t0 = Instant::now();
            match visionos(&http, &t.id, visitor.as_deref()).await {
                Ok((url, size)) => {
                    println!("VisionOS (visitor {with_visitor}): adresse en {:?}, {} Ko", t0.elapsed(), size / 1024);
                    candidates.push((format!("VisionOS/visitor={with_visitor}"), url, size, VISIONOS_UA.into()));
                }
                Err(e) => println!("VisionOS (visitor {with_visitor}): refusé ({e})"),
            }
        }
        for (name, url, size, ua) in candidates {
            println!("  {name}");
            for m in methods {
                let t1 = Instant::now();
                let (data, outcome) = fetch(&http, &url, size, &ua, m).await;
                let got = data.len();
                let decoded = if outcome == "complet" {
                    match tokio::task::spawn_blocking(move || decode_all(data, "m4a")).await.unwrap() {
                        Ok(s) => format!(", décodé {s:.1} s"),
                        Err(e) => format!(", DÉCODAGE ÉCHOUÉ : {e}"),
                    }
                } else { String::new() };
                println!("    {m:?}: {outcome} ({} / {} Ko en {:.1?}){decoded}", got / 1024, size / 1024, t1.elapsed());
            }
        }
    }
}
