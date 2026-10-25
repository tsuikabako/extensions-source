//! Okru (ok.ru): the embed page's player `div` carries its options as JSON in `data-options`; `flashvars.metadata`
//! (an object, or JSON text) lists one mp4 address per quality ("mobile" to "full"). The addresses are signed for the
//! browser that asked ("srcAg=CHROME"), so the page and the videos go with one fixed User-Agent. Ported from
//! `OkruExtractor` (yuzono lib/okruextractor); live streams (`ondemandHls`/`ondemandDash`) are left out.

use kuro_guest::wire::Video;
use kuro_guest::{Host, Request, Res};
use serde_json::Value;

use crate::{USER_AGENT, title};

pub fn videos(host: &dyn Host, embed_url: &str, label: &str) -> Res<Vec<Video>> {
    let url = embed_url.replacen("http://", "https://", 1);
    let page = Request::get(url).header("User-Agent", USER_AGENT).text(host, "Okru")?;
    let videos = parse(&page, label);
    if videos.is_empty() { Err("Okru: no video in the page (removed or blocked)".into()) } else { Ok(videos) }
}

/// The videos of the page, highest first.
pub fn parse(page: &str, label: &str) -> Vec<Video> {
    let Some(options) = data_options(page) else { return Vec::new() };
    let metadata = &options["flashvars"]["metadata"];
    let parsed;
    let metadata = match metadata.as_str() {
        Some(text) => {
            parsed = serde_json::from_str::<Value>(text).unwrap_or_default();
            &parsed
        }
        None => metadata,
    };
    let mut videos: Vec<Video> = metadata["videos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| {
            let url = v["url"].as_str().filter(|u| u.starts_with("https://"))?;
            let height = height_of(v["name"].as_str()?);
            Some(Video {
                video_url: url.to_owned(),
                video_title: title(label, height),
                resolution: height,
                headers: vec![("User-Agent".into(), USER_AGENT.into())],
                ..Default::default()
            })
        })
        .collect();
    videos.reverse();
    videos
}

fn data_options(page: &str) -> Option<Value> {
    let start = page.find("data-options=\"")? + 14;
    let raw = &page[start..start + page[start..].find('"')?];
    serde_json::from_str(&raw.replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")).ok()
}

/// The height of Okru's quality names.
fn height_of(name: &str) -> Option<i32> {
    Some(match name {
        "ultra" => 2160,
        "quad" => 1440,
        "full" => 1080,
        "hd" => 720,
        "sd" => 480,
        "low" => 360,
        "lowest" => 240,
        "mobile" => 144,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_options_give_one_mp4_per_quality() {
        let videos = parse(include_str!("../fixtures/okru.html"), "");
        let titles: Vec<&str> = videos.iter().map(|v| v.video_title.as_str()).collect();
        assert_eq!(titles, ["1080p", "720p", "480p", "360p", "240p", "144p"]);
        assert!(videos.iter().all(|v| v.video_url.starts_with("https://") && v.video_url.contains("vkuser.net/") && !v.video_url.contains("\\u0026")));
        assert_eq!(videos[0].headers, vec![("User-Agent".to_owned(), USER_AGENT.to_owned())]);
        assert!(parse("<div class=\"vp_video_stub_txt\">Video not found</div>", "").is_empty());
    }

    #[test]
    fn metadata_may_be_json_text() {
        let page = r#"<div data-options="{&quot;flashvars&quot;:{&quot;metadata&quot;:&quot;{\&quot;videos\&quot;:[{\&quot;name\&quot;:\&quot;hd\&quot;,\&quot;url\&quot;:\&quot;https://v.example/1\&quot;}]}&quot;}}"></div>"#;
        let videos = parse(page, "Sub");
        assert_eq!((videos[0].video_url.as_str(), videos[0].video_title.as_str()), ("https://v.example/1", "Sub 720p"));
    }
}
