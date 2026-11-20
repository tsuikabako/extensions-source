//! MegaPlay (megaplay.buzz and its mirrors, e.g. vidtube.site): from a player page to its HLS playlist and subtitles.
//! Ported from `AnikotoExtractor.kt` of the yuzono `anikototheme` (Apache-2.0).
//!
//! The player page names a file id; `stream/getSources?id=<id>` answers with the subtitle tracks and `enc`, the
//! playlist address encrypted with AES-256-CBC under a key and IV that are constants of the player's own script
//! (`lib/e1-player.min.js`, read 2026-11-20). An answer may also carry the address in clear (`sources`).

use aes::cipher::block_padding::Pkcs7;
use aes::cipher::{BlockModeDecrypt, KeyIvInit};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use kuro_guest::wire::{Track, Video};
use kuro_guest::{Host, Request, Res};
use serde::Deserialize;
use serde_json::Value;

use crate::BASE;

const NAME: &str = "MegaPlay";
/// The key and IV of `enc`. The player pads the key text with zero bytes to 32; the IV text is 16 bytes.
const ENC_KEY: &[u8] = b"i?LMTAx0Q6,:}50U";
const ENC_IV: &[u8; 16] = b"W0;27ToaUpl_P%'c";

#[derive(Deserialize)]
struct Sources {
    #[serde(default)]
    enc: Option<String>,
    #[serde(default)]
    sources: Value,
    #[serde(default)]
    tracks: Vec<SourceTrack>,
}

#[derive(Deserialize)]
struct SourceTrack {
    #[serde(default)]
    file: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    kind: String,
}

/// The value of a player address's `s` parameter that picks MegaPlay's image CDN.
const IMAGE_CDN: &str = "tcdn";

/// Whether a player page serves its video from the image CDN: each segment is a one-pixel PNG image followed by the
/// MPEG-TS data, stored on an image host. ffmpeg, which the app's player is built on, reads such a segment as a
/// picture (checked 2026-11-20 with ffprobe 9.0.2: the stream's codec is "png"), so that video cannot play.
pub fn on_image_cdn(embed: &str) -> bool {
    url::Url::parse(embed).is_ok_and(|address| address.query_pairs().any(|(name, value)| name == "s" && value == IMAGE_CDN))
}

/// The video of a player page: the master playlist (its qualities are inside it), every subtitle track, and the
/// headers the CDN wants (it answers 403 without the player's Referer).
pub fn video(host: &dyn Host, embed: &str, title: &str) -> Res<Video> {
    let address = url::Url::parse(embed).ok().filter(|u| u.scheme() == "https").ok_or_else(|| format!("not a {NAME} address: {embed}"))?;
    let origin = format!("https://{}", address.host_str().unwrap_or_default());
    let page = Request::get(embed)
        .headers([("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"), ("X-Requested-With", "XMLHttpRequest")])
        .header("Referer", format!("{BASE}/"))
        .text(host, NAME)?;
    let id = media_id(&page).ok_or_else(|| format!("{NAME}: the player page names no file"))?;
    // `st=other` adds the ASS subtitle tracks; `s` is the CDN the server chose (tcdn, bcdn)
    let mut sources_url = format!("{origin}/stream/getSources?id={id}&st=other");
    if let Some((_, cdn)) = address.query_pairs().find(|(name, _)| name == "s") {
        if !cdn.is_empty() && cdn.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            sources_url.push_str(&format!("&s={cdn}"));
        }
    }
    let body = Request::get(sources_url).headers([("Accept", "application/json,*/*"), ("X-Requested-With", "XMLHttpRequest")]).header("Referer", embed).text(host, NAME)?;
    let sources: Sources = serde_json::from_str(&body).map_err(|e| format!("{NAME}: the sources could not be read: {e}"))?;
    let playlist = playlist(&sources).ok_or_else(|| format!("{NAME}: the answer has no playlist"))?;
    Ok(Video {
        video_url: playlist,
        video_title: title.to_owned(),
        headers: vec![("Referer".into(), format!("{origin}/")), ("Origin".into(), origin)],
        subtitle_tracks: subtitles(&sources.tracks),
        ..Default::default()
    })
}

/// The file id of a player page: its `data-id`, else the "File <n>" of its title.
fn media_id(page: &str) -> Option<String> {
    let after = |marker: &str| page.split_once(marker).map(|(_, rest)| rest);
    let id = after("data-id=\"")
        .or_else(|| after("data-id='"))
        .map(|rest| rest.split(['"', '\'']).next().unwrap_or_default().trim())
        .filter(|id| !id.is_empty())
        .or_else(|| after("File ").map(|rest| rest.split(|c: char| !c.is_ascii_digit()).next().unwrap_or_default()).filter(|id| !id.is_empty()))?;
    id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_').then(|| id.to_owned())
}

/// The playlist address of an answer: `enc` decrypted, else `sources` in clear (a text, an object or a list of
/// objects with `file`).
fn playlist(sources: &Sources) -> Option<String> {
    let clear = || match &sources.sources {
        Value::String(file) => Some(file.clone()),
        Value::Object(source) => source.get("file").and_then(Value::as_str).map(str::to_owned),
        Value::Array(list) => list.iter().find_map(|source| source.get("file").and_then(Value::as_str)).map(str::to_owned),
        _ => None,
    };
    sources.enc.as_deref().and_then(decrypt).or_else(clear).filter(|file| file.starts_with("https://"))
}

/// `enc` (URL-safe base64 of AES-256-CBC, PKCS#7) to the `file` of the JSON inside it.
fn decrypt(enc: &str) -> Option<String> {
    let data = URL_SAFE_NO_PAD.decode(enc.trim().trim_end_matches('=')).ok()?;
    let mut key = [0_u8; 32];
    key[..ENC_KEY.len()].copy_from_slice(ENC_KEY);
    let plain = cbc::Decryptor::<aes::Aes256>::new(&key.into(), ENC_IV.into()).decrypt_padded_vec::<Pkcs7>(&data).ok()?;
    let json: Value = serde_json::from_slice(&plain).ok()?;
    json.get("file").and_then(Value::as_str).map(str::to_owned)
}

/// The subtitle tracks: everything but the seek-bar thumbnails; of two tracks with one label the styled ASS wins.
fn subtitles(tracks: &[SourceTrack]) -> Vec<Track> {
    let is_ass = |track: &&SourceTrack| track.file.split('?').next().unwrap_or_default().to_lowercase().ends_with(".ass");
    let wanted = || tracks.iter().filter(|t| !t.label.trim().is_empty() && !t.file.is_empty() && t.kind != "thumbnails");
    let mut out: Vec<Track> = Vec::new();
    for track in wanted().filter(is_ass).chain(wanted().filter(|t| !is_ass(t))) {
        if !out.iter().any(|seen| seen.lang.eq_ignore_ascii_case(&track.label)) {
            out.push(Track { url: track.file.clone(), lang: track.label.clone() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = include_str!("../fixtures/megaplay.html");
    const SOURCES: &str = include_str!("../fixtures/sources.json");

    #[test]
    fn the_player_page_names_its_file() {
        assert_eq!(media_id(PAGE).as_deref(), Some("13461"));
        assert_eq!(media_id("<title>File 77 - MegaPlay</title>").as_deref(), Some("77"));
        assert_eq!(media_id("<div data-id=''></div>"), None);
        assert_eq!(media_id("<div data-id=\"1&x=2\"></div>"), None);
    }

    #[test]
    fn enc_decrypts_to_the_playlist() {
        let sources: Sources = serde_json::from_str(SOURCES).unwrap();
        assert_eq!(
            playlist(&sources).as_deref(),
            Some("https://fetch.nexabloom.top/anime/bb6d2babd7797d94d8f4a8600bc9b44e/b7d51fb7e838ee9b60dcdb34b953bc07/master.m3u8")
        );
        assert_eq!(decrypt("not base64 !"), None);
        assert_eq!(decrypt("AAAAAAAAAAAAAAAAAAAAAA"), None);
    }

    #[test]
    fn the_image_cdn_is_told_by_the_player_address() {
        assert!(on_image_cdn("https://megaplay.buzz/stream/s-2/107257/sub?s=tcdn"));
        assert!(!on_image_cdn("https://megaplay.buzz/stream/s-2/107257/sub?s=bcdn"));
        assert!(!on_image_cdn("https://megaplay.buzz/stream/s-2/107257/sub"));
        assert!(!on_image_cdn("https://megaplay.buzz/stream/s-2/tcdn/sub?x=tcdn"));
    }

    #[test]
    fn a_clear_answer_is_read_too() {
        for sources in [r#""https://a.test/m.m3u8""#, r#"{"file":"https://a.test/m.m3u8"}"#, r#"[{"file":"https://a.test/m.m3u8"}]"#] {
            let sources: Sources = serde_json::from_str(&format!(r#"{{"sources":{sources}}}"#)).unwrap();
            assert_eq!(playlist(&sources).as_deref(), Some("https://a.test/m.m3u8"));
        }
        let none: Sources = serde_json::from_str(r#"{"sources":"javascript:alert(1)"}"#).unwrap();
        assert_eq!(playlist(&none), None);
    }

    #[test]
    fn subtitles_leave_out_thumbnails_and_prefer_ass() {
        let sources: Sources = serde_json::from_str(SOURCES).unwrap();
        let tracks = subtitles(&sources.tracks);
        assert_eq!(tracks.len(), 9);
        assert!(tracks.iter().any(|t| t.lang == "English" && t.url.ends_with(".vtt")));
        assert!(tracks.iter().all(|t| !t.url.contains("thumbnails")));
        let two = [
            SourceTrack { file: "https://a.test/en.vtt".into(), label: "English".into(), kind: "captions".into() },
            SourceTrack { file: "https://a.test/en.ass?x=1".into(), label: "english".into(), kind: "captions".into() },
            SourceTrack { file: "https://a.test/fr.vtt".into(), label: "French".into(), kind: "captions".into() },
        ];
        let urls: Vec<String> = subtitles(&two).into_iter().map(|t| t.url).collect();
        assert_eq!(urls, ["https://a.test/en.ass?x=1", "https://a.test/fr.vtt"]);
    }
}
