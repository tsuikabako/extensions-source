//! Video-host extractors: from a player page on a file host ("embed") to the video files the app can play. Shared by
//! every anime source, so a source only finds the embeds of an episode and hands each to [`videos`]. Ported from the
//! extractor libraries of the yuzono anime-extensions repository (Apache-2.0).
//!
//! An extractor requests only its host's pages, so a source lists those hosts in its manifest (see [`HOSTS`]); the
//! videos themselves load in the app, from any host. Hosts that move to new domains all the time (VOE, StreamWish,
//! Filemoon) are not here: no manifest can list them.

mod dood;
mod hexload;
mod mixdrop;
mod mp4upload;
pub mod unpacker;

use kuro_guest::wire::Video;
use kuro_guest::{Host, Res};

/// Every host the extractors request. A source that uses them lists these in its manifest's `hosts`.
pub const HOSTS: &[&str] = &[
    "www.mp4upload.com",
    "mp4upload.com",
    "hexload.com",
    "playmogo.com",
    "dood.wf",
    "dood.so",
    "dood.re",
    "dood.pm",
    "dood.yt",
    "dood.li",
    "d000d.com",
    "d0000d.com",
    "ds2play.com",
    "doodstream.com",
    "dooood.com",
    "mixdrop.co",
    "mixdrop.to",
    "mixdrop.ag",
    "mixdrop.top",
    "mixdrop.ps",
    "mixdrop.sx",
    "mixdrop.bz",
    "mxdrop.click",
];

/// The videos of one embed page, by the host in its address. `None` when no extractor knows the host (the caller
/// skips that server); `Some(Err)` when the host is known but its page did not give a video. `label` starts each
/// video's title ("Latino", "Sub"), and may be empty.
pub fn videos(host: &dyn Host, embed_url: &str, label: &str) -> Option<Res<Vec<Video>>> {
    let name = host_of(embed_url)?;
    let extract: fn(&dyn Host, &str, &str) -> Res<Vec<Video>> = match name {
        "www.mp4upload.com" | "mp4upload.com" => mp4upload::videos,
        "hexload.com" => hexload::videos,
        "mixdrop.co" | "mixdrop.to" | "mixdrop.ag" | "mixdrop.top" | "mixdrop.ps" | "mixdrop.sx" | "mixdrop.bz" | "mxdrop.click" => mixdrop::videos,
        _ if is_dood(name) => dood::videos,
        _ => return None,
    };
    Some(extract(host, embed_url, label))
}

/// The host part of an address (`https://host:port/path` → `host`).
fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let end = rest.find(['/', '?', '#', ':']).unwrap_or(rest.len());
    Some(&rest[..end]).filter(|h| !h.is_empty())
}

/// `https://host` of an address.
fn origin_of(url: &str) -> Option<String> {
    let scheme = url.split_once("://")?.0;
    Some(format!("{scheme}://{}", host_of(url)?))
}

fn is_dood(host: &str) -> bool {
    HOSTS.contains(&host) && (host.contains("dood") || host.starts_with("d000") || host == "ds2play.com" || host == "playmogo.com")
}

/// A video's title: its height ("1080p") when the host says it, else "Auto". The player lists heights in its Quality
/// menu and the server (the hoster's name) in its Server menu, so the title does not repeat the host's name; videos
/// without a height all read "Auto" and merge into one Quality entry. `label` ("Latino") starts it when not empty.
fn title(label: &str, height: Option<i32>) -> String {
    let quality = height.map_or_else(|| "Auto".to_owned(), |h| format!("{h}p"));
    if label.is_empty() { quality } else { format!("{label} {quality}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    #[test]
    fn addresses_split_into_host_and_origin() {
        assert_eq!(host_of("https://www.mp4upload.com/embed-x.html"), Some("www.mp4upload.com"));
        assert_eq!(host_of("https://a3.mp4upload.com:183/d/x/video.mp4"), Some("a3.mp4upload.com"));
        assert_eq!(origin_of("https://mxdrop.click/e/1?x=2"), Some("https://mxdrop.click".to_owned()));
        assert_eq!(host_of("nope"), None);
        assert_eq!(title("", None), "Auto");
        assert_eq!(title("", Some(1080)), "1080p");
        assert_eq!(title("Latino", Some(720)), "Latino 720p");
    }

    #[test]
    fn hosts_pick_their_extractor_and_others_are_skipped() {
        let host = TestHost::default();
        assert!(videos(&host, "https://voe.sx/e/x", "").is_none(), "VOE is not supported");
        assert!(videos(&host, "https://mega.nz/embed/x", "").is_none());
        assert!(is_dood("playmogo.com") && is_dood("d0000d.com") && !is_dood("mixdrop.co"));
        // a known host whose page fails gives an error, not None
        assert!(matches!(videos(&host, "https://playmogo.com/e/x", ""), Some(Err(_))));
    }
}
