//! Mp4Upload: the embed page holds the mp4 address in `player.src({... src: "..."})`, sometimes inside packed JS.
//! Ported from `Mp4uploadExtractor` (yuzono lib/mp4uploadextractor).

use kuro_guest::wire::Video;
use kuro_guest::{Host, Request, Res};

use crate::{title, unpacker};

const REFERER: &str = "https://mp4upload.com/";

pub fn videos(host: &dyn Host, embed_url: &str, label: &str) -> Res<Vec<Video>> {
    let page = Request::get(embed_url).header("Referer", REFERER).text(host, "Mp4Upload")?;
    parse(&page, label).map(|video| vec![video]).ok_or_else(|| "Mp4Upload: no video in the page".to_owned())
}

pub fn parse(page: &str, label: &str) -> Option<Video> {
    let unpacked = unpacker::unpack_and_combine(page);
    let script = unpacked.as_deref().unwrap_or(page);
    let after = &script[script.find(".src(")? + 5..];
    let after = &after[after.find("src:")? + 4..];
    let start = after.find('"')? + 1;
    let url = &after[start..start + after[start..].find('"')?];
    // the page names its height more than once, a 0 among them: the largest is the video's
    let height = script.match_indices("HEIGHT=").filter_map(|(i, _)| script[i + 7..].split(|c: char| !c.is_ascii_digit()).next()?.parse::<i32>().ok()).filter(|h| *h > 0).max();
    let quality = height.map_or_else(|| "Unknown resolution".to_owned(), |h| format!("{h}p"));
    Some(Video {
        video_url: url.to_owned(),
        video_title: title(label, &format!("Mp4Upload - {quality}")),
        resolution: height,
        headers: vec![("Referer".into(), REFERER.into())],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_gives_the_mp4_and_its_height() {
        let video = parse(include_str!("../fixtures/mp4upload.html"), "Latino").unwrap();
        assert!(video.video_url.starts_with("https://a3.mp4upload.com:183/d/") && video.video_url.ends_with("/video.mp4"));
        assert_eq!((video.video_title.as_str(), video.resolution), ("Latino Mp4Upload - 1080p", Some(1080)));
        assert_eq!(video.headers, vec![("Referer".to_owned(), REFERER.to_owned())]);
        assert!(parse("<html>removed</html>", "").is_none());
    }
}
