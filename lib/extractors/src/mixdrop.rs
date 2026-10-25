//! MixDrop: the embed page (often after a redirect to another MixDrop domain) holds packed JS whose unpacked text sets
//! `MDCore.wurl` to the mp4 address and `MDCore.remotesub` to a subtitle. Ported from `MixDropExtractor` (yuzono
//! lib/mixdropextractor).

use kuro_guest::wire::{Track, Video};
use kuro_guest::{Host, Request, Res};

use crate::{title, unpacker};

const REFERER: &str = "https://mixdrop.co/";
/// The video address works only for the browser that opened the page, so the page is asked for, and the video played,
/// with this one User-Agent (as the Kotlin does).
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36";

pub fn videos(host: &dyn Host, embed_url: &str, label: &str) -> Res<Vec<Video>> {
    let page = Request::get(embed_url).header("Referer", REFERER).header("User-Agent", USER_AGENT).text(host, "MixDrop")?;
    parse(&page, label).map(|video| vec![video]).ok_or_else(|| "MixDrop: no video in the page".to_owned())
}

pub fn parse(page: &str, label: &str) -> Option<Video> {
    let unpacked = unpacker::unpack_and_combine(page)?;
    let url = value_of(&unpacked, "Core.wurl=\"")?;
    let subtitle = value_of(&unpacked, "Core.remotesub=\"").filter(|s| !s.is_empty()).map(|s| Track { url: percent_decode(s), lang: "sub".into() });
    Some(Video {
        video_url: if url.starts_with("//") { format!("https:{url}") } else { url.to_owned() },
        video_title: title(label, None),
        headers: vec![("Referer".into(), REFERER.into()), ("User-Agent".into(), USER_AGENT.into())],
        subtitle_tracks: subtitle.into_iter().collect(),
        ..Default::default()
    })
}

/// The text after `key` up to the next `"`.
fn value_of<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let rest = &text[text.find(key)? + key.len()..];
    Some(&rest[..rest.find('"')?])
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], bytes.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                i += 1;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_packed_script_gives_the_mp4() {
        let video = parse(include_str!("../fixtures/mixdrop.html"), "").unwrap();
        assert!(video.video_url.starts_with("https://") && video.video_url.contains(".mxcontent.net/v2/366pk6q4cv14w1.mp4?"), "{}", video.video_url);
        assert_eq!(video.video_title, "Auto");
        assert!(parse("<html>no player</html>", "").is_none());
    }

    #[test]
    fn subtitles_are_decoded() {
        assert_eq!(percent_decode("https%3A%2F%2Fs.example%2Fa+b.vtt"), "https://s.example/a b.vtt");
    }
}
