//! Anikoto (<https://anikototv.bz>), an anime source plug-in for Kurobayomi: anime subbed and dubbed in English, each
//! server one HLS stream. Ported from the yuzono `en/anikoto` extension and its `anikototheme` (Apache-2.0); see
//! README.md. This file is the source's ops and requests; `site.rs` reads the site's pages and AJAX answers,
//! `megaplay.rs` turns a server's player page into its playlist; the glue to the app is the `kuro_guest` toolkit.
//!
//! Left out: the search filters, the settings (title language, preferred quality, server and audio type, hidden
//! servers and types, where the score goes), the related titles, the extra servers of the third-party "mapper" API,
//! the mirrors, and the `vrf` parameter (the site answers without it, checked 2026-11-20).
//!
//! Not offered: a server on MegaPlay's image CDN (`s=tcdn`, the site's "HD-1"). Its segments are video behind a PNG
//! image header, on an image host; the app's player (ffmpeg) reads them as pictures. See README.md.

mod megaplay;
mod site;

use kuro_guest::wire::{Hoster, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

pub(crate) const BASE: &str = "https://anikototv.bz";
pub(crate) const NAME: &str = "Anikoto";
/// The source id: the MD5 of `anikoto/en/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 4697393375201558791;

struct Anikoto;

impl Source for Anikoto {
    fn info(&self) -> Info {
        // the Kotlin allows five requests a second
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: Some(200) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let page = input.page().max(1);
        match op {
            "popular" => to_value(site::list(&get(host, &format!("/most-viewed/?page={page}"))?)),
            "latest" => to_value(site::list(&get(host, &format!("/latest-updated/?page={page}"))?)),
            "search" if input.query().trim().is_empty() => to_value(site::list(&get(host, &format!("/most-viewed/?page={page}"))?)),
            "search" => to_value(site::list(&get(host, &search_path(input.query(), page))?)),
            "details" => {
                let url = input.url("anime")?;
                let mut anime = site::details(&get(host, &title_path(url)?)?)?;
                anime.url = url.to_owned();
                to_value(anime)
            }
            "episodes" => {
                let url = input.url("anime")?;
                let path = title_path(url)?;
                // an address from an Aniyomi backup carries the site's id after '#'; else the title page names it
                let id = match url.split_once('#').map(|(_, id)| id).filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric())) {
                    Some(id) => id.to_owned(),
                    None => site::anime_id(&get(host, &path)?)?,
                };
                to_value(site::episodes(&ajax(host, &format!("/ajax/episode/list/{id}"), &path)?, &path)?)
            }
            "hosters" => to_value(hosters(host, input.url("episode")?)?),
            "videos" => match input.get::<Hoster>("hoster")? {
                Some(hoster) => to_value(video(host, &hoster, true)?.into_iter().collect::<Vec<Video>>()),
                None => {
                    let mut all: Vec<Video> = Vec::new();
                    for hoster in hosters(host, input.url("episode")?)? {
                        // one failing server does not hide the others
                        match video(host, &hoster, all.is_empty()) {
                            Ok(Some(video)) => all.push(Video { video_title: hoster.hoster_name, ..video }),
                            Ok(None) => {}
                            Err(error) => host.log(2, &format!("anikoto: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Anikoto);

fn search_path(query: &str, page: i32) -> String {
    let keyword: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("/filter?keyword={keyword}&page={page}")
}

/// The site path of a stored title address (`/watch/<slug>`, with `#<id>` after it in an Aniyomi backup), refused
/// when it points elsewhere.
fn title_path(url: &str) -> Res<String> {
    Some(url).filter(|url| url.starts_with('/')).and_then(site::title_path).ok_or_else(|| format!("not an {NAME} address: {url}"))
}

/// One page of the site (`path` starts with '/').
fn get(host: &dyn Host, path: &str) -> Res<String> {
    Request::get(format!("{BASE}{path}")).header("Referer", format!("{BASE}/")).text(host, NAME)
}

/// One AJAX call of the site, sent as the page at `referer` (a site path) sends it.
fn ajax(host: &dyn Host, path: &str, referer: &str) -> Res<String> {
    Request::get(format!("{BASE}{path}"))
        .headers([("Accept", "application/json, text/javascript, */*; q=0.01"), ("X-Requested-With", "XMLHttpRequest")])
        .header("Referer", format!("{BASE}{referer}"))
        .text(host, NAME)
}

/// The servers of an episode, one per server and audio type.
fn hosters(host: &dyn Host, episode: &str) -> Res<Vec<Hoster>> {
    let (ids, page) = site::episode_parts(episode)?;
    site::hosters(&ajax(host, &format!("/ajax/server/list?servers={ids}"), page)?, page)
}

/// The video of one server: its player page, found through `ajax/server`, then MegaPlay's playlist. `None` for a
/// server on the image CDN, which the player cannot read (the app then leaves the server out). A server on a player
/// this source does not know is an error (the app shows the server as failed).
fn video(host: &dyn Host, hoster: &Hoster, preferred: bool) -> Res<Option<Video>> {
    let page = hoster.internal_data.split_once(site::DATA_SEPARATOR).map(|(_, page)| page).filter(|page| site::title_path(page).is_some());
    let page = page.ok_or_else(|| format!("not an {NAME} server: {}", hoster.hoster_name))?;
    let link = &hoster.hoster_url;
    let embed = if link.starts_with("https://") {
        link.clone()
    } else if !link.is_empty() && link.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'=' | b'+' | b'/')) {
        site::embed(&ajax(host, &format!("/ajax/server?get={link}"), page)?)?
    } else {
        return Err(format!("not an {NAME} server: {}", hoster.hoster_name));
    };
    if !embed.contains("/stream/") {
        return Err(format!("{NAME}: no player for {} ({embed})", hoster.hoster_name));
    }
    if megaplay::on_image_cdn(&embed) {
        return Ok(None);
    }
    let mut video = megaplay::video(host, &embed, "Auto")?;
    video.preferred = preferred;
    Ok(Some(video))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::{LiveHost, TestHost};
    use serde_json::json;

    const POPULAR: &str = include_str!("../fixtures/popular.html");
    const LATEST: &str = include_str!("../fixtures/latest.html");
    const SEARCH: &str = include_str!("../fixtures/search.html");
    const TITLE: &str = include_str!("../fixtures/title.html");
    const EPISODES: &str = include_str!("../fixtures/episodes.json");
    const SERVERS: &str = include_str!("../fixtures/servers.json");
    const SERVER_HD1: &str = include_str!("../fixtures/server-hd1.json");
    const SERVER_VIDSTREAM: &str = include_str!("../fixtures/server-vidstream.json");
    const PLAYER: &str = include_str!("../fixtures/megaplay.html");
    const SOURCES: &str = include_str!("../fixtures/sources.json");
    const MANIFEST: &str = include_str!("../manifest.toml");
    const HOSTS: &[&str] = &["anikototv.bz", "megaplay.buzz", "vidtube.site"];
    const FRIEREN: &str = "/watch/frieren-beyond-journey-s-end-c6fbj";
    const PLAYLIST: &str = "https://fetch.nexabloom.top/anime/bb6d2babd7797d94d8f4a8600bc9b44e/b7d51fb7e838ee9b60dcdb34b953bc07/master.m3u8";

    /// The site as saved on 2026-11-20, by address. Of an episode's servers, "HD-1" (the first of each audio type)
    /// answers with its player on the image CDN, every other one with the plain player.
    fn site() -> TestHost {
        let links: Vec<String> = site::hosters(SERVERS, "/watch/x/ep-1").unwrap().into_iter().filter(|h| h.hoster_name.starts_with("HD-1")).map(|h| h.hoster_url).collect();
        TestHost::answering(move |request| {
            let url = request.url.as_str();
            let body = match url {
                "https://anikototv.bz/most-viewed/?page=1" => POPULAR,
                "https://anikototv.bz/latest-updated/?page=2" => LATEST,
                "https://anikototv.bz/filter?keyword=sousou+no&page=1" => SEARCH,
                "https://anikototv.bz/watch/frieren-beyond-journey-s-end-c6fbj" => TITLE,
                "https://anikototv.bz/ajax/episode/list/6351" => EPISODES,
                "https://megaplay.buzz/stream/s-2/107257/sub" => PLAYER,
                "https://megaplay.buzz/stream/getSources?id=13461&st=other" => SOURCES,
                _ if url.starts_with("https://anikototv.bz/ajax/server/list?servers=") => SERVERS,
                _ if links.iter().any(|link| url == format!("https://anikototv.bz/ajax/server?get={link}")) => SERVER_HD1,
                _ if url.starts_with("https://anikototv.bz/ajax/server?get=") => SERVER_VIDSTREAM,
                _ => return (404, String::new()),
            };
            (200, body.to_owned())
        })
    }

    fn call(host: &dyn Host, op: &str, args: Value) -> Res<Value> {
        kuro_guest::invoke(&Anikoto, host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str(&o).unwrap())
    }

    #[test]
    fn lists_read_their_pages() {
        let host = site();
        let popular = call(&host, "popular", json!({ "page": 1 })).unwrap();
        assert_eq!(popular["animes"].as_array().unwrap().len(), 30);
        assert_eq!(popular["animes"][0]["title"], "Solo Leveling Season 2: Arise from the Shadow");
        assert_eq!(popular["animes"][0]["url"], "/watch/solo-leveling-season-2-arise-from-the-shadow-3eukp");
        assert_eq!(popular["animes"][0]["thumbnailUrl"], "https://cdn.anipixcdn.co/thumbnail/4b5ed938de41e4ff532c02c27dfd143a.jpg");
        assert_eq!(popular["hasNextPage"], true);
        let latest = call(&host, "latest", json!({ "page": 2 })).unwrap();
        assert_eq!(latest["animes"].as_array().unwrap().len(), 30);
        assert_eq!(latest["hasNextPage"], true);
        let search = call(&host, "search", json!({ "page": 1, "query": " sousou no " })).unwrap();
        let titles: Vec<&str> = search["animes"].as_array().unwrap().iter().map(|a| a["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["Frieren: Beyond Journey's End", "Frieren: Beyond Journey's End Season 2", "Sousou no Frieren - Marumaru no Mahou (Mini Anime)"]);
        assert_eq!(search["animes"][0]["url"], FRIEREN);
        assert_eq!(search["hasNextPage"], false);
        // an empty search is the popular list
        assert_eq!(call(&host, "search", json!({ "page": 1, "query": " " })).unwrap(), popular);
    }

    #[test]
    fn a_title_has_details() {
        let host = site();
        let anime = call(&host, "details", json!({ "anime": { "url": FRIEREN } })).unwrap();
        assert_eq!(anime["url"], FRIEREN);
        assert_eq!(anime["title"], "Frieren: Beyond Journey's End");
        assert_eq!(anime["status"], 2);
        assert_eq!(anime["genre"], "Adventure, Fantasy, Shounen, Drama");
        assert_eq!(anime["author"], "Madhouse");
        assert_eq!(anime["thumbnailUrl"], "https://cdn.anipixcdn.co/thumbnail/bb6d2babd7797d94d8f4a8600bc9b44e.jpg");
        let description = anime["description"].as_str().unwrap();
        let parts: Vec<&str> = description.split("\n\n").collect();
        assert_eq!(parts.len(), 5, "{description}");
        assert_eq!(parts[0], "★★★★★ 9.36");
        assert!(parts[1].starts_with("During their decade-long quest") && parts[1].ends_with("a new tale is about to begin."), "{}", parts[1]);
        assert_eq!(parts[2], "Type: TV | Premiered: FALL 2023 | Aired: Sep 29, 2023 to Mar 22, 2024 | Duration: 25 min | Episodes: 28");
        assert_eq!(parts[3], "**Studio:** Madhouse (**Producers:** TOHO animation, Shogakukan)");
        assert_eq!(parts[4], "**Other name(s):** Sousou no Frieren");
    }

    #[test]
    fn a_title_lists_its_episodes_newest_first() {
        let host = site();
        let episodes = call(&host, "episodes", json!({ "anime": { "url": FRIEREN } })).unwrap();
        let episodes = episodes.as_array().unwrap();
        assert_eq!(episodes.len(), 28);
        assert_eq!(episodes[0]["episodeNumber"], 28.0);
        let first = &episodes[27];
        assert_eq!(first["name"], "Episode 1: The Journey's End");
        assert_eq!(first["scanlator"], "Sub & Dub");
        assert!(first["dateUpload"].as_i64().unwrap() > 1_600_000_000_000);
        let url = first["url"].as_str().unwrap();
        let (ids, page) = site::episode_parts(url).unwrap();
        assert!(ids.len() > 40 && page == "/watch/frieren-beyond-journey-s-end-c6fbj/ep-1", "{url}");
        assert!(url.contains("&mal=") && url.contains("&slug=1&ts="), "{url}");
        let sent = host.sent();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1].url, "https://anikototv.bz/ajax/episode/list/6351");
        assert!(sent[1].headers.contains(&("Referer".into(), format!("{BASE}{FRIEREN}"))));
        // an Aniyomi address names the id, so the title page is not read
        let host = site();
        let again = call(&host, "episodes", json!({ "anime": { "url": format!("{FRIEREN}#6351") } })).unwrap();
        assert_eq!((again.as_array().unwrap().len(), host.sent().len()), (28, 1));
    }

    #[test]
    fn an_episode_offers_its_servers_and_each_plays_its_playlist() {
        let host = site();
        let episodes = call(&host, "episodes", json!({ "anime": { "url": FRIEREN } })).unwrap();
        let episode = &episodes[27];
        let hosters = call(&host, "hosters", json!({ "episode": episode })).unwrap();
        let names: Vec<&str> = hosters.as_array().unwrap().iter().map(|h| h["hosterName"].as_str().unwrap()).collect();
        assert_eq!(names, ["HD-1 (Sub)", "Vidstream-2 (Sub)", "Vidstream-1 (Sub)", "VidPlay (Sub)", "HD-1 (Dub)", "Vidstream-2 (Dub)", "Vidstream-1 (Dub)", "VidPlay (Dub)"]);
        assert_eq!(hosters[0]["internalData"], "Sub\u{0}/watch/frieren-beyond-journey-s-end-c6fbj/ep-1");
        // the first server is on the image CDN: it has no video for this player, and its player page is not asked
        assert_eq!(call(&host, "videos", json!({ "hoster": hosters[0] })).unwrap(), json!([]));
        assert!(!host.sent().iter().any(|r| r.url.starts_with("https://megaplay.buzz/")));
        let videos = call(&host, "videos", json!({ "hoster": hosters[1] })).unwrap();
        let video = &videos[0];
        assert_eq!((video["videoUrl"].as_str(), video["videoTitle"].as_str(), video["preferred"].as_bool()), (Some(PLAYLIST), Some("Auto"), Some(true)));
        assert_eq!(video["headers"], json!([["Referer", "https://megaplay.buzz/"], ["Origin", "https://megaplay.buzz"]]));
        assert_eq!(video["subtitleTracks"].as_array().unwrap().len(), 9);
        let sent = host.sent();
        let player = sent.iter().find(|r| r.url.starts_with("https://megaplay.buzz/stream/s-2/")).unwrap();
        assert!(player.headers.contains(&("Referer".into(), "https://anikototv.bz/".into())));
        let sources = sent.last().unwrap();
        assert!(sources.headers.contains(&("Referer".into(), "https://megaplay.buzz/stream/s-2/107257/sub".into())));
    }

    #[test]
    fn videos_of_an_episode_are_one_per_server_named_by_it() {
        let host = site();
        let episodes = call(&host, "episodes", json!({ "anime": { "url": FRIEREN } })).unwrap();
        let videos = call(&host, "videos", json!({ "episode": episodes[27] })).unwrap();
        let videos = videos.as_array().unwrap();
        let titles: Vec<&str> = videos.iter().map(|v| v["videoTitle"].as_str().unwrap()).collect();
        assert_eq!(titles, ["Vidstream-2 (Sub)", "Vidstream-1 (Sub)", "VidPlay (Sub)", "Vidstream-2 (Dub)", "Vidstream-1 (Dub)", "VidPlay (Dub)"]);
        let preferred: Vec<bool> = videos.iter().map(|v| v["preferred"].as_bool().unwrap()).collect();
        assert_eq!(preferred, [true, false, false, false, false, false]);
    }

    #[test]
    fn info_manifest_and_foreign_addresses() {
        let host = site();
        let info = call(&host, "info", json!({})).unwrap();
        assert_eq!((info["id"].as_i64(), info["lang"].as_str(), info["supportsLatest"].as_bool()), (Some(ID), Some("en"), Some(true)));
        assert!(MANIFEST.contains(&format!("hosts = {HOSTS:?}")), "the manifest lists the hosts the tests use");
        assert!(call(&host, "details", json!({ "anime": { "url": "https://evil.example/watch/x" } })).is_err());
        assert!(call(&host, "details", json!({ "anime": { "url": "//evil.example/watch/x" } })).is_err());
        assert!(call(&host, "hosters", json!({ "episode": { "url": "abc&epurl=https://evil.example/" } })).is_err());
        let foreign = json!({ "hosterUrl": "https://megaplay.buzz/other/1", "hosterName": "X", "internalData": "Sub\u{0}/watch/x/ep-1" });
        assert_eq!(call(&host, "videos", json!({ "hoster": foreign })), Err("Anikoto: no player for X (https://megaplay.buzz/other/1)".into()));
        assert_eq!(call(&host, "pages", json!({})), Err("unknown op: pages".into()));
        assert!(host.sent().is_empty());
    }

    /// The whole chain against the live site, through the manifest's hosts: `cargo test -p kuro-anikoto -- --ignored`.
    /// Needs the `curl` program and the network.
    #[test]
    #[ignore = "asks the live site"]
    fn live_from_the_lists_to_a_playlist() {
        let host = LiveHost::new(HOSTS);
        let popular = call(&host, "popular", json!({ "page": 1 })).unwrap();
        assert!(popular["animes"].as_array().unwrap().len() >= 20 && popular["hasNextPage"] == true, "{popular}");
        let latest = call(&host, "latest", json!({ "page": 2 })).unwrap();
        assert!(latest["animes"].as_array().unwrap().len() >= 20, "{latest}");
        let search = call(&host, "search", json!({ "page": 1, "query": "frieren" })).unwrap();
        let anime = search["animes"].as_array().unwrap().iter().find(|a| a["title"] == "Frieren: Beyond Journey's End").expect("the search finds the title").clone();
        let details = call(&host, "details", json!({ "anime": anime })).unwrap();
        assert_eq!(details["status"], 2, "{details}");
        assert!(details["description"].as_str().unwrap().contains("Demon King"), "{details}");
        let episodes = call(&host, "episodes", json!({ "anime": anime })).unwrap();
        let episodes = episodes.as_array().unwrap();
        assert!(episodes.len() >= 28, "{} episodes", episodes.len());
        let hosters = call(&host, "hosters", json!({ "episode": episodes.last().unwrap() })).unwrap();
        let hosters = hosters.as_array().unwrap();
        assert!(hosters.len() >= 2, "{hosters:?}");
        let (mut played, mut left_out) = (0, 0);
        for hoster in hosters {
            match call(&host, "videos", json!({ "hoster": hoster })) {
                Ok(videos) if videos.as_array().unwrap().is_empty() => {
                    println!("{}: not offered (image CDN)", hoster["hosterName"]);
                    left_out += 1;
                }
                Ok(videos) => {
                    let url = videos[0]["videoUrl"].as_str().unwrap();
                    assert!(url.starts_with("https://") && url.contains(".m3u8"), "{url}");
                    println!("{}: {url} ({} subtitles)", hoster["hosterName"], videos[0]["subtitleTracks"].as_array().unwrap().len());
                    played += 1;
                }
                Err(error) => println!("{}: FAILED: {error}", hoster["hosterName"]),
            }
        }
        assert_eq!(played + left_out, hosters.len(), "every server gives a playlist or is left out");
        assert!(played >= 2, "{played} servers play");
    }
}
