//! AniZone (<https://anizone.to>), an anime source plug-in for Kurobayomi: anime with many audio languages and
//! subtitles in one HLS stream. Ported from the yuzono `all/anizone` extension (Apache-2.0). This file is the source's
//! ops and its Livewire calls; `site.rs` reads the pages, the glue to the app is the `kuro_guest` toolkit.
//!
//! The site's lists load through Livewire: the first page is in the HTML, every further page is a `loadPage(cursor)`
//! call to `/livewire/update` with the page's snapshot and CSRF token (the session cookie is kept by the app). The
//! state of the last list page read is saved as a preference so the next page is one call.
//!
//! Left out: the sort filter (lists use the site's A-Z order, as the Kotlin's default) and the settings (title
//! language, preferred quality, audio and subtitle language, subtitle count): titles are English, then romaji; every
//! subtitle is offered; the player picks quality and audio from the playlist.

mod site;

use kuro_guest::wire::{AnimesPage, Hoster, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Response, Source, read_pref, to_value, unknown_op, write_pref};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(crate) const BASE: &str = "https://anizone.to";
const NAME: &str = "AniZone";
/// The source id: the MD5 of `anizone/all/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 954848840440936266;
/// The list the app shows as Popular: the whole index, A-Z (the Kotlin's default sort).
const POPULAR: &str = "/anime?sort=title-asc";
/// A safety cap on the episode pages of one title (the Kotlin's).
const MAX_PAGES: usize = 50;

struct AniZone;

impl Source for AniZone {
    fn info(&self) -> Info {
        // the Kotlin allows three requests a second
        Info { id: ID, name: NAME, lang: "all", supports_latest: true, request_gap_ms: Some(350) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => to_value(list(host, POPULAR, input.page().max(1))?),
            // the home page's "Latest Anime": one page
            "latest" => to_value(AnimesPage { animes: site::animes(&site::parse_page(&get(host, "/")?).items), has_next_page: false }),
            "search" if input.query().trim().is_empty() => to_value(list(host, POPULAR, input.page().max(1))?),
            "search" => to_value(list(host, &search_path(input.query()), input.page().max(1))?),
            "details" => {
                let url = input.url("anime")?;
                let mut anime = site::details(&get(host, url)?)?;
                anime.url = url.to_owned();
                to_value(anime)
            }
            "episodes" => to_value(site::episodes(&all_episodes(host, input.url("anime")?)?)),
            "hosters" => {
                let episode = input.url("episode")?;
                to_value(site::hosters(&get(host, episode)?, episode))
            }
            "videos" => match input.get::<Hoster>("hoster")? {
                Some(hoster) => to_value(vec![server_video(host, &hoster.hoster_url, &hoster.internal_data, true)?]),
                None => {
                    let episode = input.url("episode")?;
                    let mut all: Vec<Video> = Vec::new();
                    for hoster in site::hosters(&get(host, episode)?, episode) {
                        // one failing server does not hide the others
                        match server_video(host, episode, &hoster.internal_data, all.is_empty()) {
                            Ok(video) => all.push(video),
                            Err(error) => host.log(2, &format!("anizone: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(AniZone);

fn search_path(query: &str) -> String {
    let q: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("/anime?search={q}&sort=title-asc")
}

/// A stored path (`/anime/x`, `/anime/x/1`) as a full address, refused when it points elsewhere.
fn absolute(path: &str) -> Res<String> {
    if path.starts_with('/') && !path.starts_with("//") { Ok(format!("{BASE}{path}")) } else { Err(format!("not an {NAME} address: {path}")) }
}

fn get(host: &dyn Host, path: &str) -> Res<String> {
    Request::get(absolute(path)?).header("Referer", format!("{BASE}/")).text(host, NAME)
}

/// One Livewire call on the page at `path` (its snapshot and token), e.g. `loadPage(cursor)`. A 419 is an expired
/// session or token; the caller then reads the page again.
fn livewire(host: &dyn Host, path: &str, state: &site::Page, method: &str, params: Value) -> Res<Response> {
    let body = json!({
        "_token": state.token,
        "components": [{ "snapshot": state.snapshot, "updates": {}, "calls": [{ "path": "", "method": method, "params": params }] }],
    });
    Request::post(format!("{BASE}/livewire/update"), body.to_string())
        .headers([("Accept", "*/*"), ("Content-Type", "application/json"), ("X-Livewire", "")])
        .header("Origin", BASE)
        .header("Referer", absolute(path)?)
        .send(host)
}

/// The answer of a Livewire call, read again from a fresh page once when the token expired (419).
fn livewire_fresh(host: &dyn Host, path: &str, state: &mut site::Page, method: &str, params: Value) -> Res<(site::Page, String)> {
    let mut response = livewire(host, path, state, method, params.clone())?;
    if response.status == 419 {
        let fresh = site::parse_page(&get(host, path)?);
        (state.token, state.snapshot) = (fresh.token, fresh.snapshot);
        response = livewire(host, path, state, method, params)?;
    }
    site::parse_update(&response.text(NAME)?)
}

/// Where the last list page read left off: enough for the next page to be one Livewire call.
#[derive(Serialize, Deserialize)]
struct ListState {
    path: String,
    page: i32,
    cursor: String,
    token: String,
    snapshot: String,
}

const LIST_STATE: &str = "list_state";

/// Page `page` of the list at `path`: page 1 from the HTML, the others by `loadPage` from the saved state of the page
/// before (read again from page 1 when that state is gone).
fn list(host: &dyn Host, path: &str, page: i32) -> Res<AnimesPage> {
    let (items, mut state) = if page <= 1 {
        let mut first = site::parse_page(&get(host, path)?);
        (std::mem::take(&mut first.items), first)
    } else {
        let saved = read_pref::<ListState>(host, LIST_STATE).filter(|s| s.path == path && s.page == page - 1);
        let saved = match saved {
            Some(saved) => saved,
            None => {
                list(host, path, page - 1)?;
                read_pref::<ListState>(host, LIST_STATE).ok_or("AniZone list state was not saved")?
            }
        };
        if saved.cursor.is_empty() {
            return Ok(AnimesPage::default());
        }
        let mut state = site::Page { token: saved.token, snapshot: saved.snapshot, items: Vec::new(), cursor: saved.cursor };
        let cursor = state.cursor.clone();
        let (next, _) = livewire_fresh(host, path, &mut state, "loadPage", json!([cursor]))?;
        state.snapshot = next.snapshot;
        state.cursor = next.cursor;
        (next.items, state)
    };
    let has_next_page = !state.cursor.is_empty();
    let saved = ListState { path: path.to_owned(), page, cursor: std::mem::take(&mut state.cursor), token: state.token, snapshot: state.snapshot };
    write_pref(host, LIST_STATE, &saved);
    Ok(AnimesPage { animes: site::animes(&items), has_next_page })
}

/// Every episode item of a title: the page's first part, then `loadPage` until the site has no more.
fn all_episodes(host: &dyn Host, anime: &str) -> Res<Vec<Value>> {
    let mut state = site::parse_page(&get(host, anime)?);
    let mut items = std::mem::take(&mut state.items);
    for _ in 0..MAX_PAGES {
        if state.cursor.is_empty() {
            break;
        }
        let cursor = std::mem::take(&mut state.cursor);
        let (next, _) = livewire_fresh(host, anime, &mut state, "loadPage", json!([cursor]))?;
        state.snapshot = next.snapshot;
        state.cursor = next.cursor;
        for item in next.items {
            if !items.iter().any(|seen| seen["url"] == item["url"]) {
                items.push(item);
            }
        }
    }
    Ok(items)
}

/// The video of server `index` of an episode: the page's own player when it already plays that server, else the
/// player `setVideo(index)` gives.
fn server_video(host: &dyn Host, episode: &str, index: &str, preferred: bool) -> Res<Video> {
    let html = get(host, episode)?;
    let mut video = if site::is_playing(&html, index) {
        site::video(&html)?
    } else {
        let number: u32 = index.parse().map_err(|_| format!("not an {NAME} server: {index}"))?;
        let mut state = site::parse_page(&html);
        let (_, component) = livewire_fresh(host, episode, &mut state, "setVideo", json!([number]))?;
        site::video(&component)?
    };
    video.preferred = preferred;
    Ok(video)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    const LIST: &str = include_str!("../fixtures/list.html");
    const LIST_PAGE2: &str = include_str!("../fixtures/list-page2.json");
    const HOME: &str = include_str!("../fixtures/home.html");
    const SEARCH: &str = include_str!("../fixtures/search.html");
    const TITLE: &str = include_str!("../fixtures/title.html");
    const EPISODES_PAGE2: &str = include_str!("../fixtures/episodes-page2.json");
    const EPISODE: &str = include_str!("../fixtures/episode.html");
    const EPISODE_TWO: &str = include_str!("../fixtures/episode-two-servers.html");
    const SET_VIDEO: &str = include_str!("../fixtures/set-video.json");
    const MANIFEST: &str = include_str!("../manifest.toml");

    /// The site as saved on 2026-10-25: pages by address, Livewire calls by their method.
    fn site() -> TestHost {
        TestHost::answering(|request| {
            let body = request.body.as_deref().unwrap_or("");
            match request.url.as_str() {
                "https://anizone.to/livewire/update" if body.contains("\"setVideo\"") => (200, SET_VIDEO.into()),
                "https://anizone.to/livewire/update" if body.contains("pages.anime-detail") => (200, EPISODES_PAGE2.into()),
                "https://anizone.to/livewire/update" if body.contains("pages.anime-index") => (200, LIST_PAGE2.into()),
                "https://anizone.to/anime?sort=title-asc" => (200, LIST.into()),
                "https://anizone.to/" => (200, HOME.into()),
                url if url.starts_with("https://anizone.to/anime?search=") => (200, SEARCH.into()),
                "https://anizone.to/anime/mdkytdqp" => (200, TITLE.into()),
                "https://anizone.to/anime/mdkytdqp/1" => (200, EPISODE.into()),
                "https://anizone.to/anime/ltayl97j/1" => (200, EPISODE_TWO.into()),
                _ => (404, String::new()),
            }
        })
    }

    fn call(host: &TestHost, op: &str, args: Value) -> Res<Value> {
        kuro_guest::invoke(&AniZone, host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str(&o).unwrap())
    }

    #[test]
    fn popular_is_the_index_and_its_next_page_one_livewire_call() {
        let host = site();
        let first = call(&host, "popular", json!({ "page": 1 })).unwrap();
        assert_eq!(first["animes"].as_array().unwrap().len(), 24);
        assert_eq!(first["animes"][0]["title"], "\"Eiji\"");
        assert_eq!(first["animes"][0]["url"], "/anime/qtskwpje");
        assert!(first["animes"][0]["thumbnailUrl"].as_str().unwrap().starts_with("https://anizone.to/images/anime/"));
        assert_eq!(first["hasNextPage"], true);
        let second = call(&host, "popular", json!({ "page": 2 })).unwrap();
        assert_eq!(second["animes"][0]["title"], "Aharen-san wa Hakarenai Season 2");
        assert_eq!(second["hasNextPage"], true);
        let sent = host.sent();
        assert_eq!(sent.len(), 2);
        let body: Value = serde_json::from_str(sent[1].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["_token"], "FmJOku78DmCQGPvtVgpskFp3Rcbf34JvosEclx9y");
        assert_eq!(body["components"][0]["calls"][0]["method"], "loadPage");
        assert!(body["components"][0]["calls"][0]["params"][0].as_str().unwrap().starts_with("eyJtYWluX3RpdGxlIjoiQWhhcmVu"));
        assert!(body["components"][0]["snapshot"].as_str().unwrap().contains("pages.anime-index"));
    }

    #[test]
    fn a_later_page_without_saved_state_walks_from_page_one() {
        let host = site();
        let second = call(&host, "popular", json!({ "page": 2 })).unwrap();
        assert_eq!(second["animes"].as_array().unwrap().len(), 24);
        assert_eq!(host.sent().len(), 2);
    }

    #[test]
    fn an_expired_token_reads_the_page_again_once() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            "https://anizone.to/livewire/update" if request.body.as_deref().unwrap().contains("\"stale\"") => (419, String::new()),
            "https://anizone.to/livewire/update" => (200, LIST_PAGE2.into()),
            _ => (200, LIST.into()),
        });
        let state = ListState { path: POPULAR.into(), page: 1, cursor: "c".into(), token: "stale".into(), snapshot: "s".into() };
        write_pref(&host, LIST_STATE, &state);
        let second = call(&host, "popular", json!({ "page": 2 })).unwrap();
        assert_eq!(second["animes"].as_array().unwrap().len(), 24);
        let urls: Vec<String> = host.sent().iter().map(|r| r.url.clone()).collect();
        assert_eq!(urls, ["https://anizone.to/livewire/update", "https://anizone.to/anime?sort=title-asc", "https://anizone.to/livewire/update"]);
    }

    #[test]
    fn latest_and_search_read_their_pages() {
        let host = site();
        let latest = call(&host, "latest", json!({ "page": 1 })).unwrap();
        assert_eq!(latest["animes"].as_array().unwrap().len(), 24);
        assert_eq!(latest["animes"][0]["title"], "Cyberpunk: Edgerunners 2");
        assert_eq!(latest["hasNextPage"], false);
        let search = call(&host, "search", json!({ "page": 1, "query": "frieren" })).unwrap();
        assert_eq!(search["animes"][0]["title"], "Frieren: Beyond Journey's End");
        assert_eq!(search["animes"][0]["url"], "/anime/mdkytdqp");
        assert_eq!(search["hasNextPage"], false);
        assert_eq!(search_path(" sousou no "), "/anime?search=sousou+no&sort=title-asc");
    }

    #[test]
    fn a_title_has_details_and_all_episodes_newest_first() {
        let host = site();
        let anime = call(&host, "details", json!({ "anime": { "url": "/anime/mdkytdqp" } })).unwrap();
        assert_eq!(anime["title"], "Frieren: Beyond Journey's End");
        assert_eq!(anime["status"], 2);
        assert_eq!(anime["genre"], "Manga, Shounen, Elf, Magic, Demon, Violence, Fantasy");
        assert!(anime["thumbnailUrl"].as_str().unwrap().ends_with("38c0dc13-dbdf-4e7d-9590-f079497c2842.jpg"));
        let description = anime["description"].as_str().unwrap();
        assert!(description.starts_with("* Based on an action-adventure fantasy manga"), "{description}");
        assert!(description.contains("Abe Tsukasa.\n\nElf mage Frieren"), "{description}");
        let episodes = call(&host, "episodes", json!({ "anime": { "url": "/anime/mdkytdqp" } })).unwrap();
        let episodes = episodes.as_array().unwrap();
        assert_eq!(episodes.len(), 28);
        assert_eq!(episodes[0]["episodeNumber"], 28.0);
        assert_eq!(episodes[27]["url"], "/anime/mdkytdqp/1");
        assert_eq!(episodes[27]["name"], "Episode 1 - The Journey's End");
        assert_eq!(episodes[27]["dateUpload"], 1695945600000_i64);
    }

    #[test]
    fn an_episode_offers_its_servers_and_each_plays_its_playlist() {
        let host = site();
        let hosters = call(&host, "hosters", json!({ "episode": { "url": "/anime/ltayl97j/1" } })).unwrap();
        let names: Vec<&str> = hosters.as_array().unwrap().iter().map(|h| h["hosterName"].as_str().unwrap()).collect();
        assert_eq!(names, ["VARYG", "VARYG (2)"]);
        let first = call(&host, "videos", json!({ "hoster": hosters[0] })).unwrap();
        assert_eq!(first[0]["videoUrl"], "https://suzaku.xin-cdn.xyz/b5b9aaae-8e23-48e2-af41-3ec5c3ae2229/master.m3u8");
        let second = call(&host, "videos", json!({ "hoster": hosters[1] })).unwrap();
        assert_eq!(second[0]["videoUrl"], "https://suzaku.xin-cdn.xyz/cc63965f-b21b-4803-a2de-da16b0c8cd34/master.m3u8");
        assert!(host.sent().iter().any(|r| r.body.as_deref().is_some_and(|b| b.contains("\"setVideo\""))));
    }

    #[test]
    fn a_video_carries_its_subtitles_default_first() {
        let host = site();
        let videos = call(&host, "videos", json!({ "episode": { "url": "/anime/mdkytdqp/1" } })).unwrap();
        let video = &videos[0];
        assert_eq!(video["videoUrl"], "https://seiryuu.vid-cdn.xyz/a7305a5d-5bc8-4944-843a-f8675561b940/master.m3u8");
        assert_eq!((video["videoTitle"].as_str(), video["preferred"].as_bool()), (Some("Auto"), Some(true)));
        assert_eq!(video["headers"], json!([["Referer", "https://anizone.to/"]]));
        let tracks = video["subtitleTracks"].as_array().unwrap();
        assert!(tracks.len() > 5);
        assert_eq!(tracks[0], json!({ "url": "https://seiryuu.vid-cdn.xyz/a7305a5d-5bc8-4944-843a-f8675561b940/subtitles/3_en.ass", "lang": "English (US)" }));
    }

    #[test]
    fn info_manifest_and_foreign_addresses() {
        let host = site();
        let info = call(&host, "info", json!({})).unwrap();
        assert_eq!((info["id"].as_i64(), info["lang"].as_str()), (Some(ID), Some("all")));
        assert!(MANIFEST.contains("hosts = [\"anizone.to\"]"));
        assert!(call(&host, "details", json!({ "anime": { "url": "https://evil.example/x" } })).is_err());
        assert_eq!(call(&host, "pages", json!({})), Err("unknown op: pages".into()));
    }
}
