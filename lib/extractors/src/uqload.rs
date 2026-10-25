//! Uqload: every mirror domain redirects to the current one (uqload.vc), whose embed page holds packed JS that sets up
//! the player with `sources:[{file:"<master.m3u8>"}]` and names the height in `qualityLabels`. Ported from
//! `UqloadExtractor` (yuzono lib/uqloadextractor); the master playlist is handed to the player whole.

use kuro_guest::wire::Video;
use kuro_guest::{Host, Request, Res};

use crate::{origin_of, title, unpacker};

pub fn videos(host: &dyn Host, embed_url: &str, label: &str) -> Res<Vec<Video>> {
    let response = Request::get(embed_url).send(host)?;
    let landed = if response.url.is_empty() { embed_url.to_owned() } else { response.url.clone() };
    let origin = origin_of(&landed).ok_or("Uqload: bad address")?;
    let page = response.text("Uqload")?;
    parse(&page, &origin, label).map(|video| vec![video]).ok_or_else(|| "Uqload: no video in the page".to_owned())
}

/// `origin` is the domain the page came from: the video wants it as Referer.
pub fn parse(page: &str, origin: &str, label: &str) -> Option<Video> {
    let unpacked = unpacker::unpack_and_combine(page);
    let script = unpacked.as_deref().unwrap_or(page);
    let sources = &script[script.find("sources:")?..];
    let start = sources.find("file:\"").map(|i| i + 6).or_else(|| sources.find("[\"").map(|i| i + 2))?;
    let url = &sources[start..start + sources[start..].find('"')?];
    if !url.starts_with("http") {
        return None;
    }
    // 'qualityLabels':{"945":"1080p (946 kbps)"}
    let height = script.find("qualityLabels").and_then(|i| {
        let label = &script[i..];
        let label = &label[label.find(":\"")? + 2..];
        label[..label.find('p')?].parse::<i32>().ok()
    });
    Some(Video {
        video_url: url.to_owned(),
        video_title: title(label, height),
        resolution: height,
        headers: vec![("Referer".into(), format!("{origin}/"))],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_packed_player_gives_the_playlist_and_height() {
        let video = parse(include_str!("../fixtures/uqload.html"), "https://uqload.vc", "").unwrap();
        assert!(video.video_url.starts_with("https://strm6.uqload.vc/hls2/") && video.video_url.contains("/master.m3u8?"), "{}", video.video_url);
        assert_eq!((video.video_title.as_str(), video.resolution), ("1080p", Some(1080)));
        assert_eq!(video.headers, vec![("Referer".to_owned(), "https://uqload.vc/".to_owned())]);
        assert!(parse("<html>File was deleted</html>", "https://uqload.vc", "").is_none());
    }
}
