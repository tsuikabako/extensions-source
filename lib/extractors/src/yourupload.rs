//! YourUpload: the embed page sets `jwplayerOptions` with `file: '<mp4>'`. The video answers only with the site's
//! Referer. Ported from `YourUploadExtractor` (yuzono lib/youruploadextractor).

use kuro_guest::wire::Video;
use kuro_guest::{Host, Request, Res};

use crate::title;

const REFERER: &str = "https://www.yourupload.com/";

pub fn videos(host: &dyn Host, embed_url: &str, label: &str) -> Res<Vec<Video>> {
    let page = Request::get(embed_url).header("Referer", REFERER).text(host, "YourUpload")?;
    parse(&page, label).map(|video| vec![video]).ok_or_else(|| "YourUpload: no video in the page".to_owned())
}

pub fn parse(page: &str, label: &str) -> Option<Video> {
    let options = &page[page.find("jwplayerOptions")?..];
    let after = &options[options.find("file: '")? + 7..];
    let url = &after[..after.find('\'')?];
    Some(Video {
        video_url: url.to_owned(),
        video_title: title(label, None),
        headers: vec![("Referer".into(), REFERER.into())],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_options_give_the_mp4() {
        let video = parse(include_str!("../fixtures/yourupload.html"), "").unwrap();
        assert_eq!(video.video_url, "https://vidcache.net:8161/a20261025kI265K4cQdE/video.mp4");
        assert_eq!(video.video_title, "Auto");
        assert_eq!(video.headers, vec![("Referer".to_owned(), REFERER.to_owned())]);
        assert!(parse("<html>File not found</html>", "").is_none());
    }
}
