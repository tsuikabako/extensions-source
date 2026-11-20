//! Re:ANIME (<https://reanime.cz>), an anime source plug-in for Kurobayomi: anime subbed and dubbed, each episode
//! played from its original MKV file (1080p for most, every audio track and subtitle inside). Ported from the yuzono
//! `en/reanime` extension (Apache-2.0); see README.md. This file is the source's ops and requests; `api.rs` reads the
//! answers; the glue to the app is the `kuro_guest` toolkit.
//!
//! The site's own player streams the same file in HLS from its video host (FlixCloud), with every playlist scrambled
//! by a key that a small program sent with each player page computes. A plug-in can only hand the app an address, and
//! the app's player cannot read a scrambled playlist, so this source offers the host's direct download instead (the
//! Kotlin offers both). Also left out: the search filters, the settings, the related titles, the episode previews and
//! the mirrors.

mod api;

use api::{Details, Meta};
use kuro_guest::wire::{AnimesPage, Hoster, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, read_pref, to_value, unknown_op, write_pref};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const BASE: &str = "https://reanime.cz";
pub(crate) const FLIXCLOUD: &str = "https://flixcloud.cc";
pub(crate) const NAME: &str = "Re:ANIME";
/// The source id: the MD5 of `re:anime/en/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 7105156973687362431;
/// Titles a page of the popular list and of a search.
const PAGE: i32 = 36;
/// Titles a page of the latest list.
const LATEST_PAGE: i32 = 12;

struct ReAnime;

impl Source for ReAnime {
    fn info(&self) -> Info {
        // the Kotlin allows five requests a second
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: Some(200) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let page = input.page().max(1);
        let offset = (page - 1) * PAGE;
        match op {
            "popular" => to_value(api::search(&api(host, &format!("/search?sort=popularity_desc&limit={PAGE}&offset={offset}"), "/search")?, input.show_adult)?),
            "latest" => to_value(latest(host, page, input.show_adult)?),
            "search" if input.query().trim().is_empty() => to_value(api::search(&api(host, &format!("/search?sort=popularity_desc&limit={PAGE}&offset={offset}"), "/search")?, input.show_adult)?),
            "search" => {
                let query: String = url::form_urlencoded::byte_serialize(input.query().trim().as_bytes()).collect();
                to_value(api::search(&api(host, &format!("/search?q={query}&limit={PAGE}&offset={offset}"), "/search")?, input.show_adult)?)
            }
            "details" => {
                let slug = slug(input.url("anime")?)?;
                to_value(details(host, slug)?.anime(slug))
            }
            "episodes" => {
                let slug = slug(input.url("anime")?)?;
                to_value(api::episodes(&api(host, &format!("/anime/{slug}/episodes?limit=2000"), &format!("/anime/{slug}"))?, slug)?)
            }
            "hosters" => to_value(hosters(host, input.url("episode")?)?),
            "videos" => match input.get::<Hoster>("hoster")? {
                Some(hoster) => to_value(vec![video(host, &hoster.hoster_url)?]),
                None => {
                    let mut all: Vec<Video> = Vec::new();
                    for hoster in hosters(host, input.url("episode")?)? {
                        // one failing file does not hide the others
                        match video(host, &hoster.hoster_url) {
                            Ok(video) => all.push(Video { preferred: all.is_empty(), video_title: format!("{} - {}", hoster.hoster_name, video.video_title), ..video }),
                            Err(error) => host.log(2, &format!("reanime: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(ReAnime);

/// A stored title address (the title's id on the site), refused when it is anything else.
fn slug(url: &str) -> Res<&str> {
    Some(url).filter(|url| api::is_slug(url)).ok_or_else(|| format!("not a {NAME} address: {url}"))
}

/// One call of the site's API (`path` under `/api/v1`), sent as the site's page at `referer` sends it.
fn api(host: &dyn Host, path: &str, referer: &str) -> Res<String> {
    get(host, &format!("{BASE}/api/v1{path}"), &format!("{BASE}{referer}"))
}

fn get(host: &dyn Host, url: &str, referer: &str) -> Res<String> {
    Request::get(url).headers([("Accept", "application/json, text/plain, */*"), ("Accept-Language", "en-US,en;q=0.9")]).header("Referer", referer).text(host, NAME)
}

/// The details of a title; its ids are kept for the title's next episode (see [`meta`]).
fn details(host: &dyn Host, slug: &str) -> Res<Details> {
    let details = Details::parse(&api(host, &format!("/anime/{slug}"), &format!("/anime/{slug}"))?)?;
    write_pref(host, META, &details.meta(slug));
    Ok(details)
}

const META: &str = "meta";

/// The ids the video servers of a title are asked by: kept from the last title asked about, else from its details.
fn meta(host: &dyn Host, slug: &str) -> Res<Meta> {
    match read_pref::<Meta>(host, META).filter(|meta| meta.slug == slug) {
        Some(meta) => Ok(meta),
        None => Ok(details(host, slug)?.meta(slug)),
    }
}

/// Where the latest list left off: the page last read and the cursor of the page after it.
#[derive(Serialize, Deserialize)]
struct LatestState {
    page: i32,
    cursor: String,
}

const LATEST_STATE: &str = "latest_state";

/// Page `page` of the latest list. The site pages it by a cursor each page gives for the next one; the cursor of the
/// page last read is saved, so the next page is one call (read again from page 1 when that state is gone).
fn latest(host: &dyn Host, page: i32, show_adult: bool) -> Res<AnimesPage> {
    let mut path = format!("/home/latest-aired?limit={LATEST_PAGE}&lang=");
    if page > 1 {
        let saved = read_pref::<LatestState>(host, LATEST_STATE).filter(|state| state.page == page - 1);
        let cursor = match saved {
            Some(state) => state.cursor,
            None => {
                latest(host, page - 1, show_adult)?;
                read_pref::<LatestState>(host, LATEST_STATE).filter(|state| state.page == page - 1).map(|state| state.cursor).unwrap_or_default()
            }
        };
        if cursor.is_empty() {
            return Ok(AnimesPage::default());
        }
        let cursor: String = url::form_urlencoded::byte_serialize(cursor.as_bytes()).collect();
        path.push_str(&format!("&cursor={cursor}"));
    }
    let (list, next) = api::latest(&api(host, &path, "/home")?, show_adult)?;
    write_pref(host, LATEST_STATE, &LatestState { page, cursor: next.unwrap_or_default() });
    Ok(list)
}

/// The files of an episode, one hoster each. The servers are asked by the title's AniList id (0 without one), with
/// its MyAnimeList id, and its TMDB id and season when it has no AniList id.
fn hosters(host: &dyn Host, episode: &str) -> Res<Vec<Hoster>> {
    let (slug, number) = api::episode_parts(episode)?;
    let meta = meta(host, slug)?;
    if meta.anilist <= 0 && meta.mal <= 0 && meta.tmdb <= 0 {
        return Err(format!("{NAME} names no id to find this title's videos by"));
    }
    let mut url = format!("{BASE}/api/flix/{}/{number}", meta.anilist.max(0));
    let mut params: Vec<String> = Vec::new();
    if meta.mal > 0 {
        params.push(format!("mal={}", meta.mal));
    }
    if meta.anilist <= 0 && meta.tmdb > 0 {
        params.push(format!("tmdb={}", meta.tmdb));
        params.push(format!("season={}", meta.season));
    }
    if !params.is_empty() {
        url.push_str(&format!("?{}", params.join("&")));
    }
    api::hosters(&get(host, &url, &format!("{BASE}/watch/{slug}?ep={number}"))?)
}

/// The video of a file: its download data on the video host, then the file's download address.
fn video(host: &dyn Host, download_data: &str) -> Res<Video> {
    if !api::is_download_data(download_data) {
        return Err(format!("not a {NAME} server: {download_data}"));
    }
    let body = Request::get(download_data).headers([("Accept", "*/*")]).header("Referer", format!("{FLIXCLOUD}/")).text(host, "FlixCloud")?;
    api::download(&body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::{LiveHost, TestHost};
    use serde_json::json;

    const POPULAR: &str = include_str!("../fixtures/popular.json");
    const LATEST: &str = include_str!("../fixtures/latest.json");
    const LATEST_PAGE2: &str = include_str!("../fixtures/latest-page2.json");
    const SEARCH: &str = include_str!("../fixtures/search.json");
    const DETAILS: &str = include_str!("../fixtures/details.json");
    const EPISODES: &str = include_str!("../fixtures/episodes.json");
    const SERVERS: &str = include_str!("../fixtures/servers.json");
    const DOWNLOAD: &str = include_str!("../fixtures/download.json");
    const MANIFEST: &str = include_str!("../manifest.toml");
    const HOSTS: &[&str] = &["reanime.cz", "flixcloud.cc"];
    const MUSHISHI: &str = "mushi-shi-the-next-passage-2-qazz9g";

    /// The site as saved on 2026-11-20, by address.
    fn site() -> TestHost {
        TestHost::answering(|request| {
            let body = match request.url.as_str() {
                "https://reanime.cz/api/v1/search?sort=popularity_desc&limit=36&offset=0" => POPULAR,
                "https://reanime.cz/api/v1/home/latest-aired?limit=12&lang=" => LATEST,
                "https://reanime.cz/api/v1/home/latest-aired?limit=12&lang=&cursor=2026-11-19T18%3A22%3A26.393Z" => LATEST_PAGE2,
                "https://reanime.cz/api/v1/search?q=mushishi&limit=36&offset=0" => SEARCH,
                "https://reanime.cz/api/v1/anime/mushi-shi-the-next-passage-2-qazz9g" => DETAILS,
                "https://reanime.cz/api/v1/anime/mushi-shi-the-next-passage-2-qazz9g/episodes?limit=2000" => EPISODES,
                "https://reanime.cz/api/flix/20751/7?mal=24701" => SERVERS,
                "https://flixcloud.cc/d/tj6w0yf402zn/__data.json?v=1" => DOWNLOAD,
                _ => return (404, String::new()),
            };
            (200, body.to_owned())
        })
    }

    fn call(host: &dyn Host, op: &str, args: Value) -> Res<Value> {
        kuro_guest::invoke(&ReAnime, host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str(&o).unwrap())
    }

    #[test]
    fn popular_and_search_read_the_search_api() {
        let host = site();
        let popular = call(&host, "popular", json!({ "page": 1 })).unwrap();
        assert_eq!(popular["animes"].as_array().unwrap().len(), 36);
        assert_eq!((popular["animes"][0]["title"].as_str(), popular["animes"][0]["url"].as_str()), (Some("Attack on Titan"), Some("attack-on-titan-p9y2p9")));
        assert!(popular["animes"][0]["thumbnailUrl"].as_str().unwrap().starts_with("https://"));
        assert_eq!(popular["hasNextPage"], true);
        let search = call(&host, "search", json!({ "page": 1, "query": " mushishi " })).unwrap();
        let found = search["animes"].as_array().unwrap();
        assert!(found.iter().any(|a| a["url"] == MUSHISHI && a["title"] == "MUSHI-SHI The Next Passage 2"), "{search}");
        assert_eq!(search["hasNextPage"], false);
        assert_eq!(call(&host, "search", json!({ "page": 1, "query": "" })).unwrap(), popular);
        let sent = host.sent();
        assert!(sent[0].headers.contains(&("Referer".into(), "https://reanime.cz/search".into())));
    }

    #[test]
    fn latest_follows_the_cursor_and_walks_from_page_one_without_it() {
        let host = site();
        let first = call(&host, "latest", json!({ "page": 1 })).unwrap();
        assert_eq!((first["animes"].as_array().unwrap().len(), first["hasNextPage"].as_bool()), (12, Some(true)));
        let second = call(&host, "latest", json!({ "page": 2 })).unwrap();
        assert_eq!(second["animes"].as_array().unwrap().len(), 12);
        assert_ne!(second["animes"][0]["url"], first["animes"][0]["url"]);
        assert_eq!(host.sent().len(), 2);
        // the app restarted: page 2 is asked first
        let host = site();
        assert_eq!(call(&host, "latest", json!({ "page": 2 })).unwrap(), second);
        assert_eq!(host.sent().len(), 2);
    }

    #[test]
    fn a_title_has_details() {
        let host = site();
        let anime = call(&host, "details", json!({ "anime": { "url": MUSHISHI } })).unwrap();
        assert_eq!((anime["url"].as_str(), anime["title"].as_str(), anime["status"].as_i64()), (Some(MUSHISHI), Some("MUSHI-SHI The Next Passage 2"), Some(2)));
        assert_eq!(anime["author"], "Artland");
        assert!(anime["genre"].as_str().unwrap().starts_with("Adventure, Fantasy, Mystery"), "{}", anime["genre"]);
        let description = anime["description"].as_str().unwrap();
        assert!(description.starts_with("★★★★☆ 86\n\nSecond season"), "{description}");
        for line in [
            "**Alternative Titles**: Mushishi Zoku Shou 2 • 蟲師 続章 2",
            "**Format**: TV",
            "**Source**: Manga • JP",
            "**Airing**: From October 19th, 2014 to December 22nd, 2014",
            "**Season**: Fall 2014",
            "**Duration**: 24m",
            "**Trackers**: [AniList](https://anilist.co/anime/20751) • [MAL](https://myanimelist.net/anime/24701)",
            "**Streaming**: [Crunchyroll](http://www.crunchyroll.com/mushi-shi)",
            "**Trailer**: [YouTube](https://www.youtube.com/watch?v=zuX3P6ynAgc)",
        ] {
            assert!(description.contains(line), "{line:?} is missing from:\n{description}");
        }
        assert!(!description.contains('<'), "{description}");
    }

    #[test]
    fn a_title_lists_its_episodes_newest_first() {
        let host = site();
        let episodes = call(&host, "episodes", json!({ "anime": { "url": MUSHISHI } })).unwrap();
        let episodes = episodes.as_array().unwrap();
        assert_eq!(episodes.len(), 10);
        assert_eq!(episodes[0]["episodeNumber"], 10.0);
        let seventh = &episodes[3];
        assert_eq!((seventh["url"].as_str(), seventh["name"].as_str()), (Some("mushi-shi-the-next-passage-2-qazz9g/ep-7"), Some("Episode 7 - Azure Waters")));
        assert_eq!((seventh["scanlator"].as_str(), seventh["dateUpload"].as_i64()), (Some("Sub"), Some(1_417_305_600_000)));
    }

    #[test]
    fn an_episode_plays_its_file() {
        let host = site();
        let episode = json!({ "url": "mushi-shi-the-next-passage-2-qazz9g/ep-7" });
        let hosters = call(&host, "hosters", json!({ "episode": episode })).unwrap();
        // three players of the site, one file
        assert_eq!(hosters.as_array().unwrap().len(), 1);
        assert_eq!((hosters[0]["hosterName"].as_str(), hosters[0]["hosterUrl"].as_str()), (Some("FlixCloud (Sub)"), Some("https://flixcloud.cc/d/tj6w0yf402zn/__data.json?v=1")));
        let videos = call(&host, "videos", json!({ "hoster": hosters[0] })).unwrap();
        let video = &videos[0];
        let url = video["videoUrl"].as_str().unwrap();
        assert!(url.starts_with("https://fetch8.flixcloud.cc/download/ce139d9d-ec9a-4d08-bd9d-984fc2b171eb?token=eyJ"), "{url}");
        assert_eq!((video["videoTitle"].as_str(), video["resolution"].as_i64(), video["preferred"].as_bool()), (Some("1080p MKV"), Some(1080), Some(true)));
        assert_eq!(video["headers"], json!([["Referer", "https://flixcloud.cc/"]]));
        let urls: Vec<String> = host.sent().iter().map(|r| r.url.clone()).collect();
        assert_eq!(urls, ["https://reanime.cz/api/v1/anime/mushi-shi-the-next-passage-2-qazz9g", "https://reanime.cz/api/flix/20751/7?mal=24701", "https://flixcloud.cc/d/tj6w0yf402zn/__data.json?v=1"]);
        // the title's ids are kept: its next episode asks the servers at once
        call(&host, "hosters", json!({ "episode": episode })).unwrap();
        assert_eq!(host.sent().len(), 4);
        let all = call(&host, "videos", json!({ "episode": episode })).unwrap();
        assert_eq!((all.as_array().unwrap().len(), all[0]["videoTitle"].as_str()), (1, Some("FlixCloud (Sub) - 1080p MKV")));
    }

    #[test]
    fn info_manifest_and_foreign_addresses() {
        let host = site();
        let info = call(&host, "info", json!({})).unwrap();
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["supportsLatest"].as_bool()), (Some(ID), Some("Re:ANIME"), Some(true)));
        assert!(MANIFEST.contains(&format!("hosts = {HOSTS:?}")), "the manifest lists the hosts the tests use");
        assert!(call(&host, "details", json!({ "anime": { "url": "https://evil.example/x" } })).is_err());
        assert!(call(&host, "episodes", json!({ "anime": { "url": "../admin" } })).is_err());
        assert!(call(&host, "hosters", json!({ "episode": { "url": "a/../../x" } })).is_err());
        assert!(call(&host, "videos", json!({ "hoster": { "hosterUrl": "https://evil.example/d/x/__data.json" } })).is_err());
        assert_eq!(call(&host, "pages", json!({})), Err("unknown op: pages".into()));
        assert!(host.sent().is_empty());
    }

    /// The whole chain against the live site, through the manifest's hosts: `cargo test -p kuro-reanime -- --ignored`.
    /// Needs the `curl` program and the network.
    #[test]
    #[ignore = "asks the live site"]
    fn live_from_the_lists_to_a_file() {
        let host = LiveHost::new(HOSTS);
        let popular = call(&host, "popular", json!({ "page": 1 })).unwrap();
        assert!(popular["animes"].as_array().unwrap().len() >= 30 && popular["hasNextPage"] == true, "{popular}");
        let latest = call(&host, "latest", json!({ "page": 1 })).unwrap();
        let later = call(&host, "latest", json!({ "page": 2 })).unwrap();
        assert!(!latest["animes"].as_array().unwrap().is_empty() && !later["animes"].as_array().unwrap().is_empty());
        assert_ne!(latest["animes"][0]["url"], later["animes"][0]["url"]);
        let search = call(&host, "search", json!({ "page": 1, "query": "mushishi" })).unwrap();
        let anime = search["animes"].as_array().unwrap().iter().find(|a| a["url"] == MUSHISHI).expect("the search finds the title").clone();
        let details = call(&host, "details", json!({ "anime": anime })).unwrap();
        assert_eq!(details["status"], 2, "{details}");
        let episodes = call(&host, "episodes", json!({ "anime": anime })).unwrap();
        let episodes = episodes.as_array().unwrap();
        assert_eq!(episodes.len(), 10);
        let hosters = call(&host, "hosters", json!({ "episode": episodes.last().unwrap() })).unwrap();
        let hosters = hosters.as_array().unwrap();
        assert!(!hosters.is_empty(), "the episode has a file");
        for hoster in hosters {
            let videos = call(&host, "videos", json!({ "hoster": hoster })).unwrap();
            let url = videos[0]["videoUrl"].as_str().unwrap();
            assert!(url.contains(".flixcloud.cc/download/") && url.contains("?token="), "{}", url.split('?').next().unwrap());
            println!("{}: {} at {}", hoster["hosterName"], videos[0]["videoTitle"], url.split('?').next().unwrap());
        }
    }
}
