//! JKAnime (<https://jkanime.net>), an anime source plug-in for Kurobayomi: anime in Japanese with Spanish subtitles,
//! some dubbed (Latino). Ported from the Kohi-den `es/jkanime` extension (Apache-2.0). This file is the source's ops and
//! the site's own players (Desu, Desuka, Nozomi); the pages are read in `site.rs`, the other video hosts by the shared
//! `lib/extractors`.
//!
//! The site is behind Cloudflare's browser check; the app passes it (hidden pages), so nothing here deals with it.
//! Left out: the search filters, the preferences, and every video host the extractors do not cover (VOE, Filemoon,
//! StreamWish, StreamTape and the Kotlin's catch-all WebView extractor): an episode offers only the servers that play.

mod site;

use kuro_guest::wire::{Hoster, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;
use site::{BASE, OwnPlayer};

const NAME: &str = "JKAnime";
/// The id the Kotlin extension sets (`id = 6007374093317538838`). Keep it: backups name it.
const ID: i64 = 6007374093317538838;
/// The episode list is paged by the site; a title longer than this many pages is cut (a runaway guard).
const MAX_EPISODE_PAGES: u32 = 300;

fn get(host: &dyn Host, url: &str) -> Res<String> {
    Request::get(url).header("Referer", format!("{BASE}/")).text(host, NAME)
}

fn form_value(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

struct Jkanime;

impl Source for Jkanime {
    fn info(&self) -> Info {
        // the Kotlin has no rate limit; a long title's episode list takes one request per page, so the gap stays short
        Info { id: ID, name: NAME, lang: "es", supports_latest: true, request_gap_ms: Some(250) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => to_value(site::parse_directory(&get(host, &site::popular_url(input.page()))?)),
            "latest" => to_value(site::parse_directory(&get(host, &site::latest_url(input.page()))?)),
            "search" if input.query().trim().is_empty() => to_value(site::parse_directory(&get(host, &site::popular_url(input.page()))?)),
            // one page of results: the site's search has no pages
            "search" if input.page() > 1 => to_value(kuro_guest::wire::AnimesPage::default()),
            "search" => {
                let response = Request::get(site::search_url(input.query())).header("Referer", format!("{BASE}/")).send(host)?;
                let final_url = response.url.clone();
                to_value(site::parse_search(&response.text(NAME)?, &final_url))
            }
            "details" => {
                let url = input.url("anime")?;
                let mut anime = site::parse_details(&get(host, &site::absolute(url)?)?);
                anime.url = url.to_owned();
                to_value(anime)
            }
            "episodes" => to_value(episodes(host, input.url("anime")?)?),
            "hosters" => to_value(hosters(host, input.url("episode")?)?),
            "videos" => match input.get::<Hoster>("hoster")? {
                Some(hoster) => to_value(hoster_videos(host, &hoster, true)?),
                None => {
                    let mut all = Vec::new();
                    for hoster in hosters(host, input.url("episode")?)? {
                        // one failing server does not hide the others
                        match hoster_videos(host, &hoster, all.is_empty()) {
                            Ok(videos) => all.extend(videos),
                            Err(error) => host.log(2, &format!("jkanime: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Jkanime);

/// Every episode of a title, newest first: the title page gives the anime id and the CSRF token, then the episodes
/// API is posted page by page.
fn episodes(host: &dyn Host, anime_url: &str) -> Res<Vec<kuro_guest::wire::Episode>> {
    let page_url = site::absolute(anime_url)?;
    let keys = site::episode_keys(&get(host, &page_url)?)?;
    let mut all = Vec::new();
    for page in 1..=MAX_EPISODE_PAGES {
        let answer = Request::form(site::episodes_url(&keys.anime_id, page), format!("_token={}", form_value(&keys.token)))
            .header("Referer", page_url.clone())
            .header("X-Requested-With", "XMLHttpRequest")
            .text(host, NAME)?;
        let (episodes, more) = site::parse_episodes(&answer, &keys.base)?;
        all.extend(episodes);
        if !more {
            break;
        }
    }
    all.reverse();
    Ok(all)
}

/// The servers of an episode that can play: the site's own players and the hosts the extractors know.
fn hosters(host: &dyn Host, episode_url: &str) -> Res<Vec<Hoster>> {
    let servers = site::parse_servers(&get(host, &site::absolute(episode_url)?)?);
    Ok(servers
        .into_iter()
        .filter(|server| OwnPlayer::of(&server.url).is_some() || extractor_host(&server.url))
        .map(|server| {
            let name = OwnPlayer::of(&server.url).map_or_else(|| server.name.clone(), |player| player.name().to_owned());
            Hoster {
                hoster_name: format!("{} {name}", server.lang).trim().to_owned(),
                hoster_url: server.url,
                internal_data: server.lang.to_owned(),
                ..Default::default()
            }
        })
        .collect())
}

/// Whether an extractor reads this player's host.
fn extractor_host(url: &str) -> bool {
    url.split_once("://").and_then(|(_, rest)| rest.split(['/', '?', ':']).next()).is_some_and(|host| kuro_extractors::HOSTS.contains(&host))
}

/// The videos of one server; the first video is the preferred one when `preferred`.
fn hoster_videos(host: &dyn Host, hoster: &Hoster, preferred: bool) -> Res<Vec<Video>> {
    let label = hoster.internal_data.as_str();
    let mut videos = match OwnPlayer::of(&hoster.hoster_url) {
        Some(player) => own_videos(host, player, &hoster.hoster_url, label)?,
        None => kuro_extractors::videos(host, &hoster.hoster_url, label).ok_or_else(|| format!("{} is not supported", hoster.hoster_name))??,
    };
    if let Some(first) = videos.first_mut() {
        first.preferred = preferred;
    }
    Ok(videos)
}

/// The video of one of the site's own players, as the Kotlin's `JkanimeExtractor`.
fn own_videos(host: &dyn Host, player: OwnPlayer, url: &str, label: &str) -> Res<Vec<Video>> {
    let video_url = match player {
        OwnPlayer::Desu => site::script_url(&get(host, url)?, "var parts = {"),
        OwnPlayer::Desuka => {
            // the address may be the video itself: ask for its first byte only, so a video is not read whole
            let response = Request::get(url).header("Referer", format!("{BASE}/")).header("Range", "bytes=0-0").send(host)?;
            if response.header("Content-Type").is_some_and(|t| t.starts_with("video/")) {
                Some(if response.url.is_empty() { url.to_owned() } else { response.url.clone() })
            } else {
                site::script_url(&response.text(NAME)?, "new DPlayer({")
            }
        }
        OwnPlayer::Nozomi => nozomi(host, url)?,
    };
    let video_url = video_url.ok_or_else(|| format!("{}: no video in the player", player.name()))?;
    let video_title = if label.is_empty() { "Auto".to_owned() } else { format!("{label} Auto") };
    Ok(vec![Video { video_url, video_title, ..Default::default() }])
}

/// Nozomi: the player page's form key is posted to `gsplay/redirect_post.php`, which redirects to
/// `player.html#<key>`; that key posted to `gsplay/api.php` gives `{"file": ...}`.
fn nozomi(host: &dyn Host, url: &str) -> Res<Option<String>> {
    let Some(key) = site::nozomi_key(&Request::get(url).header("Referer", url).text(host, NAME)?) else { return Ok(None) };
    let redirect = Request::form(format!("{BASE}/gsplay/redirect_post.php"), format!("data={}", form_value(&key))).header("Referer", url).send(host)?;
    let Some(post_key) = redirect.url.split_once("player.html#").map(|(_, k)| k.to_owned()).filter(|k| !k.is_empty()) else {
        return Err("Nozomi: the player did not give its key".into());
    };
    let answer = Request::form(format!("{BASE}/gsplay/api.php"), format!("v={}", form_value(&post_key))).text(host, NAME)?;
    Ok(site::nozomi_file(&answer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;
    use serde_json::json;

    const MANIFEST: &str = include_str!("../manifest.toml");

    fn fixture(url: &str) -> (u16, String) {
        let body = match url {
            u if u.contains("/directorio") => include_str!("../fixtures/directory.html"),
            u if u.contains("/ajax/episodes/21/") => include_str!("../fixtures/episodes_last.json"),
            u if u.ends_with("/one-piece/") => include_str!("../fixtures/anime.html"),
            u if u.ends_with("/one-piece/1/") => include_str!("../fixtures/episode.html"),
            _ => return (404, String::new()),
        };
        (200, body.to_owned())
    }

    fn call(op: &str, args: Value) -> Res<Value> {
        let host = TestHost::answering(|request| fixture(&request.url));
        kuro_guest::invoke(&Jkanime, &host, op, &json!({ "args": args }).to_string()).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_kotlin_id() {
        assert_eq!(call("info", json!({})).unwrap()["id"].as_i64(), Some(ID));
    }

    #[test]
    fn ops_run_through_the_toolkit() {
        assert_eq!(call("popular", json!({ "page": 1 })).unwrap()["animes"].as_array().unwrap().len(), 2);
        assert_eq!(call("latest", json!({ "page": 2 })).unwrap()["animes"].as_array().unwrap().len(), 2);
        assert_eq!(call("details", json!({ "anime": { "url": "/one-piece/" } })).unwrap()["title"], "One Piece");
        let episodes = call("episodes", json!({ "anime": { "url": "/one-piece/" } })).unwrap();
        assert_eq!(episodes[0]["url"], "/one-piece/3/");
        let hosters = call("hosters", json!({ "episode": { "url": "/one-piece/1/" } })).unwrap();
        let names: Vec<&str> = hosters.as_array().unwrap().iter().map(|h| h["hosterName"].as_str().unwrap()).collect();
        // the VOE server is left out: no extractor reads it
        assert_eq!(names, ["[JAP] Okru", "[JAP] Desu"]);
        assert!(call("details", json!({ "anime": { "url": "https://evil.example/x" } })).is_err());
    }

    #[test]
    fn the_manifest_lists_the_site_and_every_extractor_host() {
        assert!(MANIFEST.contains("\"jkanime.net\""));
        for host in kuro_extractors::HOSTS {
            assert!(MANIFEST.contains(&format!("\"{host}\"")), "manifest.toml misses {host}");
        }
    }
}
