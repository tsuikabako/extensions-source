//! Vidmoly: its embed page sets up the player with `sources: [{ file: '<master.m3u8>' }]`. The older domains redirect
//! to vidmoly.biz (vidmoly.to to an ad), so the page is always asked of vidmoly.biz, as the Kotlin does. Ported from
//! `VidMolyExtractor` (yuzono lib/vidmolyextractor); the master playlist is handed to the player whole.

use kuro_guest::wire::Video;
use kuro_guest::{Host, Request, Res};

use crate::{host_of, title};

const BASE: &str = "https://vidmoly.biz";

pub fn videos(host: &dyn Host, embed_url: &str, label: &str) -> Res<Vec<Video>> {
    let page = Request::get(on_base(embed_url).ok_or("Vidmoly: bad address")?).header("Referer", format!("{BASE}/")).text(host, "Vidmoly")?;
    parse(&page, label).map(|video| vec![video]).ok_or_else(|| "Vidmoly: no video in the page".to_owned())
}

/// The embed address on vidmoly.biz.
fn on_base(embed_url: &str) -> Option<String> {
    let host = host_of(embed_url)?;
    let path = &embed_url[embed_url.find(host)? + host.len()..];
    Some(format!("{BASE}{path}"))
}

pub fn parse(page: &str, label: &str) -> Option<Video> {
    let sources = &page[page.find("sources:")?..];
    let after = &sources[sources.find("file:")? + 5..];
    let quote = after.trim_start().chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let after = &after[after.find(quote)? + 1..];
    let url = &after[..after.find(quote)?];
    Some(Video {
        video_url: url.to_owned(),
        video_title: title(label, None),
        headers: vec![("Referer".into(), format!("{BASE}/")), ("Origin".into(), BASE.into())],
        ..Default::default()
    })
    .filter(|v| v.video_url.starts_with("http"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_sources_give_the_playlist() {
        let video = parse(include_str!("../fixtures/vidmoly.html"), "Deutscher Sub").unwrap();
        assert!(video.video_url.starts_with("https://") && video.video_url.contains("/master.m3u8?"), "{}", video.video_url);
        assert_eq!(video.video_title, "Deutscher Sub Auto");
        assert!(parse("<html>Video not found</html>", "").is_none());
    }

    #[test]
    fn every_domain_is_asked_on_vidmoly_biz() {
        assert_eq!(on_base("https://vidmoly.to/embed-ka28nfechsnx.html").as_deref(), Some("https://vidmoly.biz/embed-ka28nfechsnx.html"));
        assert_eq!(on_base("https://vidmoly.biz/embed-x.html").as_deref(), Some("https://vidmoly.biz/embed-x.html"));
    }
}
