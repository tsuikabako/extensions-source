//! DoodStream and its many domains (playmogo.com, d000d.com, ...): the embed page names `/pass_md5/<...>/<token>`;
//! that address answers the start of the video address, which is completed with any 10 letters, the token and an
//! expiry time. Ported from `DoodExtractor` (yuzono lib/doodextractor).

use kuro_guest::wire::Video;
use kuro_guest::{Host, Request, Res};

use crate::{origin_of, title};

pub fn videos(host: &dyn Host, embed_url: &str, label: &str) -> Res<Vec<Video>> {
    let response = Request::get(embed_url).send(host)?;
    // the embed may redirect to another Dood domain; its pass_md5 lives there
    let landed = if response.url.is_empty() { embed_url.to_owned() } else { response.url.clone() };
    let page = response.text("DoodStream")?;
    let pass = pass_md5(&page).ok_or("DoodStream: no video in the page")?;
    let origin = origin_of(&landed).ok_or("DoodStream: bad address")?;
    let start = Request::get(format!("{origin}{pass}")).header("Referer", landed.as_str()).text(host, "DoodStream")?;
    Ok(vec![video(&start, pass, &origin, quality(&page), host.unix_seconds(), label)])
}

/// The `/pass_md5/...` path of the embed page.
pub fn pass_md5(page: &str) -> Option<&str> {
    let start = page.find("/pass_md5/")?;
    let end = page[start..].find(['\'', '"']).map_or(page.len(), |e| start + e);
    Some(&page[start..end])
}

/// "720p" from the page title, when it names one.
fn quality(page: &str) -> Option<String> {
    let title = &page[page.find("<title>")? + 7..];
    let title = &title[..title.find("</title>")?];
    title.split(|c: char| !c.is_ascii_alphanumeric()).find(|w| w.len() >= 4 && w.ends_with('p') && w[..w.len() - 1].chars().all(|c| c.is_ascii_digit())).map(str::to_owned)
}

/// The playable address: the start the server gave, 10 letters (the server only wants 10; the Kotlin picks them at
/// random, here they come from the token), the token, and an expiry in milliseconds.
pub fn video(start: &str, pass: &str, origin: &str, quality: Option<String>, now: u64, label: &str) -> Video {
    let token = pass.rsplit('/').next().unwrap_or_default();
    let filler: String = token.chars().chain("abcdefghij".chars()).filter(char::is_ascii_alphanumeric).take(10).collect();
    let name = format!("DoodStream {}", quality.as_deref().unwrap_or("mirror"));
    Video {
        video_url: format!("{}{filler}?token={token}&expiry={}", start.trim(), now * 1000),
        video_title: title(label, &name),
        resolution: quality.and_then(|q| q.trim_end_matches('p').parse().ok()),
        headers: vec![("Referer".into(), format!("{origin}/"))],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    const PAGE: &str = include_str!("../fixtures/dood.html");
    const START: &str = include_str!("../fixtures/dood-pass.txt");

    #[test]
    fn the_page_names_its_pass_md5() {
        let pass = pass_md5(PAGE).unwrap();
        assert!(pass.starts_with("/pass_md5/") && pass.ends_with("/tfnvc5m7rf59fe8nkmeysjgk"), "{pass}");
        assert_eq!(quality(PAGE), None, "the title names no quality");
        assert_eq!(quality("<title>Show 01 1080p - DoodStream</title>").as_deref(), Some("1080p"));
    }

    #[test]
    fn the_video_address_is_start_filler_token_and_expiry() {
        let video = video(START, "/pass_md5/1-2/tfnvc5m7rf59fe8nkmeysjgk", "https://playmogo.com", None, 1_792_943_929, "");
        assert!(video.video_url.starts_with("https://gv675rt.cloudatacdn.com/"), "{}", video.video_url);
        assert!(video.video_url.ends_with("~tfnvc5m7rf?token=tfnvc5m7rf59fe8nkmeysjgk&expiry=1792943929000"), "{}", video.video_url);
        assert_eq!(video.video_title, "DoodStream mirror");
        assert_eq!(video.headers, vec![("Referer".to_owned(), "https://playmogo.com/".to_owned())]);
    }

    #[test]
    fn both_requests_go_to_the_embed_s_domain() {
        let host = TestHost::answering(|request| if request.url.contains("/pass_md5/") { (200, START.into()) } else { (200, PAGE.into()) });
        let videos = videos(&host, "https://playmogo.com/e/59zpn0j4eoqz", "Latino").unwrap();
        assert_eq!(videos[0].video_title, "Latino DoodStream mirror");
        let sent = host.sent();
        assert!(sent[1].url.starts_with("https://playmogo.com/pass_md5/"));
        assert_eq!(sent[1].headers, vec![("Referer".to_owned(), "https://playmogo.com/e/59zpn0j4eoqz".to_owned())]);
    }
}
