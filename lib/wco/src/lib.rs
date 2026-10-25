//! The WCO anime theme, shared by the WCO family of sites (English dubbed and subbed anime and cartoons). Ported from
//! the yuzono `lib-multisrc/wcotheme` theme (`WcoTheme`; Apache-2.0). A site is one `Site` value and one line,
//! `kuro_guest::plugin!(SITE);`; this crate holds the ops and the site's own player, `site.rs` the parsers.
//!
//! Every episode plays from the sites' own player, `embed.wcostream.com`: its page, a "clear" ad-check beacon, a wait
//! the server counts (a 12 s countdown in the browser), then the player page and its JSON with up to three mp4 files.
//!
//! Left out: the genre filter, the settings (preferred quality, the countdown length), the premium player
//! (`vhs.watchanimesub`, on no site's episodes on 2026-10-25).

mod site;

use kuro_guest::wire::{AnimesPage, Hoster, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// One WCO site.
pub struct Site {
    /// The source id: the MD5 of `<name lower-case>/en/1`, as the Kotlin extension has it.
    pub id: i64,
    pub name: &'static str,
    /// The site root without a trailing slash (`https://www.wco.tv`).
    pub base: &'static str,
    /// WCOStream has no "Recently Added Series": its Popular is the recent releases, and it offers no Latest.
    pub supports_latest: bool,
}

/// Whole seconds to wait between the player's beacon and its player page. The server refuses the page before its
/// 12 s countdown has run; the clock reads whole seconds, so 13 makes at least 12.
const PLAYER_WAIT_S: u64 = if cfg!(test) { 0 } else { 13 };
/// How often a "busy" player page is asked again, and the whole seconds between two asks (2 to 3 s on a clock of whole
/// seconds). With the countdown that is at most about 19 s of the call's 30 s of run time.
const BUSY_RETRIES: u32 = 2;
const BUSY_WAIT_S: u64 = if cfg!(test) { 0 } else { 3 };

impl Source for Site {
    fn info(&self) -> Info {
        Info {
            id: self.id,
            name: self.name,
            lang: "en",
            supports_latest: self.supports_latest,
            request_gap_ms: Some(500),
        }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let s = self;
        match op {
            "popular" => to_value(AnimesPage {
                animes: site::popular(s, &s.get(host, &format!("{}/", s.base))?),
                has_next_page: false,
            }),
            "latest" => to_value(AnimesPage {
                animes: site::latest(s, &s.get(host, &format!("{}/", s.base))?),
                has_next_page: false,
            }),
            "search" if input.query().trim().is_empty() => s.run("popular", input, host),
            // the site's search has one page
            "search" if input.page() > 1 => to_value(AnimesPage::default()),
            "search" => {
                let form = format!(
                    "catara={}&konuara=series",
                    url::form_urlencoded::byte_serialize(input.query().trim().as_bytes()).collect::<String>()
                );
                let html = Request::form(format!("{}/search", s.base), form)
                    .header("Referer", format!("{}/", s.base))
                    .text(host, s.name)?;
                to_value(AnimesPage { animes: site::search(s, &html), has_next_page: false })
            }
            "details" => {
                let url = input.url("anime")?;
                let mut anime = site::details(s, &s.get(host, &s.absolute(url)?)?)?;
                anime.url = url.to_owned();
                to_value(anime)
            }
            "episodes" => {
                let url = input.url("anime")?;
                to_value(site::episodes(&s.get(host, &s.absolute(url)?)?, url))
            }
            "hosters" => {
                let url = s.absolute(input.url("episode")?)?;
                to_value(site::hosters(&s.get(host, &url)?, &url))
            }
            "videos" => match input.get::<Hoster>("hoster")? {
                Some(hoster) => to_value(player_videos(host, &hoster.hoster_url, &hoster.internal_data, true)?),
                None => {
                    let url = s.absolute(input.url("episode")?)?;
                    let mut all = Vec::new();
                    for hoster in site::hosters(&s.get(host, &url)?, &url) {
                        // one failing server does not hide the others
                        match player_videos(host, &hoster.hoster_url, &hoster.internal_data, all.is_empty()) {
                            Ok(videos) => all.extend(videos),
                            Err(error) => host.log(2, &format!("{}: {} failed: {error}", s.name, hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

impl Site {
    fn get(&self, host: &dyn Host, url: &str) -> Res<String> {
        Request::get(url).header("Referer", format!("{}/", self.base)).text(host, self.name)
    }

    /// A stored path (`/anime/x`, `/x-episode-1-english-dubbed`) as a full address, refused when it points elsewhere.
    fn absolute(&self, path: &str) -> Res<String> {
        if path.starts_with('/') && !path.starts_with("//") {
            Ok(format!("{}{path}", self.base))
        } else {
            Err(format!("not a {} address: {path}", self.name))
        }
    }
}

const PLAYER: &str = "the WCO player";
/// The file server plays a video only to the User-Agent that asked the player for it, so the player's requests and the
/// videos carry this one (the Kotlin's).
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36";

/// The videos of one player (`https://embed.wcostream.com/inc/embed/index.php?...&pid=...`) opened from the episode
/// page `referer`: the steps the player's own scripts take, as the Kotlin does them.
fn player_videos(host: &dyn Host, iframe: &str, referer: &str, preferred: bool) -> Res<Vec<Video>> {
    let parsed = url::Url::parse(iframe).map_err(|_| format!("not a player address: {iframe}"))?;
    let origin = parsed.origin().ascii_serialization();
    let query = parsed.query().unwrap_or("");
    let pid = parsed
        .query_pairs()
        .find(|(key, _)| key == "pid")
        .map(|(_, value)| value.into_owned())
        .ok_or("the player has no video id")?;
    // the player page; its session cookie is kept by the app
    Request::get(iframe).header("User-Agent", USER_AGENT).header("Referer", referer).text(host, PLAYER)?;
    let started = host.unix_seconds();
    let nonce = site::nonce(started, iframe);
    let beacon = serde_json::json!({ "nonce": nonce, "status": "clear", "id": pid }).to_string();
    let answer = Request::post(format!("{origin}/ad-verify"), beacon)
        .headers([
            ("User-Agent", USER_AGENT),
            ("Content-Type", "application/json"),
            ("Origin", &origin),
            ("Referer", iframe),
        ])
        .send(host)?;
    if !answer.is_success() {
        return Err(answer.error(PLAYER));
    }
    // ponytail: the ABI has no sleep, so the countdown is waited out on the clock (about 13 s of the call's 30 s of run
    // time); a host wait import would free the CPU.
    while host.unix_seconds() < started + PLAYER_WAIT_S {}
    let player = |path: &str| -> Res<(u16, Option<String>)> {
        let page = Request::get(format!("{origin}{path}?{query}&n={nonce}"))
            .headers([("User-Agent", USER_AGENT), ("Referer", iframe)])
            .send(host)?;
        let ok = page.is_success() && page.body_encoding.is_none();
        Ok((page.status, if ok { site::json_link(&page.body) } else { None }))
    };
    // the newer player first, as the browser opens it; the Kotlin asks the older one first, but on 2026-10-25 that
    // answered 403 and the newer one asked right after it mostly answered "busy". A "busy" (503) is asked again a few
    // seconds later, which played every time it was tried; the countdown need not run again.
    let mut link = None;
    for attempt in 0..=BUSY_RETRIES {
        if attempt > 0 {
            let until = host.unix_seconds() + BUSY_WAIT_S;
            while host.unix_seconds() < until {}
        }
        let (status, found) = player("/inc/embed/video-js.php")?;
        link = found;
        if link.is_some() || status != 503 {
            break;
        }
    }
    if link.is_none() {
        link = player("/inc/embed/video-js-old.php")?.1;
    }
    let link = format!("{origin}{}", link.ok_or("the WCO player is busy or refused the episode; try again later")?);
    let json = Request::get(&link)
        .headers([
            ("User-Agent", USER_AGENT),
            ("X-Requested-With", "XMLHttpRequest"),
            ("Referer", &link),
            ("Origin", &origin),
        ])
        .text(host, PLAYER)?;
    let mut videos = site::videos(&json)?;
    for video in &mut videos {
        video.headers = vec![("User-Agent".into(), USER_AGENT.into())];
    }
    if let Some(first) = videos.first_mut() {
        first.preferred = preferred;
    }
    Ok(videos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;
    use serde_json::json;

    const WCOTV: Site =
        Site { id: 648702763126013078, name: "WcoTv", base: "https://www.wco.tv", supports_latest: true };
    const WCOSTREAM: Site =
        Site { id: 4144024127778211557, name: "WCOStream", base: "https://www.wcostream.tv", supports_latest: false };

    /// The two sites and the player as saved on 2026-10-25.
    fn web() -> TestHost {
        TestHost::answering(|request| {
            let page = |name: &str| {
                (200, std::fs::read_to_string(format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap())
            };
            match request.url.as_str() {
                "https://www.wco.tv/" => page("wcotv-home.html"),
                "https://www.wco.tv/search" => page("wcotv-search.html"),
                "https://www.wco.tv/anime/overgeared" => page("wcotv-title.html"),
                "https://www.wco.tv/overgeared-episode-5-english-dubbed" => page("wcotv-episode.html"),
                "https://www.wcostream.tv/" => page("wcostream-home.html"),
                "https://www.wcostream.tv/search" => page("wcostream-search.html"),
                "https://www.wcostream.tv/anime/naruto-movies" => page("wcostream-title.html"),
                "https://www.wcostream.tv/overgeared-episode-5-english-dubbed" => page("wcostream-episode.html"),
                "https://embed.wcostream.com/ad-verify" => (204, String::new()),
                url if url.starts_with("https://embed.wcostream.com/inc/embed/index.php?") => {
                    (200, "<html></html>".into())
                }
                // on 2026-10-25 the older player answered 403 with an announcement, the newer one played
                url if url.starts_with("https://embed.wcostream.com/inc/embed/video-js-old.php?") => {
                    (403, page("player-refused.html").1)
                }
                url if url.starts_with("https://embed.wcostream.com/inc/embed/video-js.php?") => page("player.html"),
                url if url.starts_with("https://embed.wcostream.com/inc/embed/getvidlink.php?") => {
                    page("getvidlink.json")
                }
                _ => (404, String::new()),
            }
        })
    }

    fn call(site: &Site, host: &TestHost, op: &str, args: Value) -> Res<Value> {
        kuro_guest::invoke(site, host, op, &json!({ "args": args }).to_string())
            .map(|o| serde_json::from_str(&o).unwrap())
    }

    #[test]
    fn popular_is_recently_added_series_and_latest_the_recent_releases() {
        let host = web();
        let popular = call(&WCOTV, &host, "popular", json!({ "page": 1 })).unwrap();
        let animes = popular["animes"].as_array().unwrap();
        assert_eq!(animes.len(), 16);
        assert_eq!(
            animes[0],
            json!({ "url": "/anime/piggy-builders", "title": "Piggy Builders", "thumbnailUrl": "https://images.wcostream.com/catimg/934442.jpg", "status": 0 })
        );
        assert_eq!(popular["hasNextPage"], false);
        let latest = call(&WCOTV, &host, "latest", json!({ "page": 1 })).unwrap();
        assert_eq!(latest["animes"].as_array().unwrap().len(), 16);
        assert_eq!(latest["animes"][0]["url"], "/tales-of-herding-gods-episode-106-english-subbed");
        // the cover's link comes first and has no text: the title is the cover's alt, the series name
        assert_eq!(latest["animes"][0]["title"], "Tales of Herding Gods");
    }

    #[test]
    fn wcostream_lists_its_recent_releases_as_popular_and_has_no_latest() {
        let host = web();
        assert_eq!(call(&WCOSTREAM, &host, "info", json!({})).unwrap()["supportsLatest"], false);
        let popular = call(&WCOSTREAM, &host, "popular", json!({ "page": 1 })).unwrap();
        assert_eq!(popular["animes"].as_array().unwrap().len(), 16);
        assert_eq!(popular["animes"][0]["url"], "/tales-of-herding-gods-episode-106-english-subbed");
        assert_eq!(popular["animes"][0]["thumbnailUrl"], "https://images.wcostream.com/catimg/934041.jpg");
    }

    #[test]
    fn search_posts_the_form_and_reads_either_layout() {
        let host = web();
        let found = call(&WCOTV, &host, "search", json!({ "page": 1, "query": "naruto shippuden" })).unwrap();
        let titles: Vec<&str> =
            found["animes"].as_array().unwrap().iter().map(|a| a["title"].as_str().unwrap()).collect();
        assert_eq!(
            titles,
            ["Naruto", "Naruto Movies", "Naruto Shippuden", "Naruto Kai", "Boruto: Naruto Next Generations"]
        );
        assert_eq!(found["animes"][2]["url"], "/anime/naruto-shippuden");
        assert_eq!(host.sent()[0].body.as_deref(), Some("catara=naruto+shippuden&konuara=series"));
        let stream = call(&WCOSTREAM, &host, "search", json!({ "page": 1, "query": "naruto" })).unwrap();
        assert_eq!(stream["animes"].as_array().unwrap().len(), 5);
        assert_eq!(
            stream["animes"][0],
            json!({ "url": "/anime/naruto", "title": "Naruto", "thumbnailUrl": "https://images.wcostream.com/catimg/113.jpg", "status": 0 })
        );
        assert_eq!(
            call(&WCOTV, &host, "search", json!({ "page": 2, "query": "naruto" })).unwrap()["animes"],
            json!([])
        );
    }

    #[test]
    fn details_read_both_layouts() {
        let host = web();
        let tv = call(&WCOTV, &host, "details", json!({ "anime": { "url": "/anime/overgeared" } })).unwrap();
        assert_eq!(tv["title"], "Overgeared");
        assert!(tv["thumbnailUrl"].as_str().unwrap().starts_with("https://images.wcostream.com/catimg/"));
        assert!(tv["genre"].as_str().is_some_and(|g| !g.is_empty()), "{tv}");
        assert!(tv["description"].as_str().is_some_and(|d| !d.is_empty()), "{tv}");
        let stream = call(&WCOSTREAM, &host, "details", json!({ "anime": { "url": "/anime/naruto-movies" } })).unwrap();
        assert_eq!(stream["title"], "Naruto Movies");
        assert_eq!(stream["genre"], "Action, Adventure");
        assert_eq!(stream["thumbnailUrl"], "https://images.wcostream.com/catimg/69568.jpg");
        assert!(stream["description"].as_str().unwrap().starts_with("Plot Summary: Kakashi orders Naruto"));
        // a Latest entry is an episode page: the series name stands in for the title
        let episode =
            call(&WCOTV, &host, "details", json!({ "anime": { "url": "/overgeared-episode-5-english-dubbed" } }))
                .unwrap();
        assert_eq!(episode["title"], "Overgeared");
        let url = json!({ "anime": { "url": "/overgeared-episode-5-english-dubbed" } });
        assert_eq!(call(&WCOSTREAM, &host, "details", url).unwrap()["title"], "Overgeared");
        assert!(call(&WCOTV, &host, "details", json!({ "anime": { "url": "https://evil.example/x" } })).is_err());
    }

    #[test]
    fn episodes_are_numbered_from_their_names_newest_first() {
        let host = web();
        let episodes = call(&WCOTV, &host, "episodes", json!({ "anime": { "url": "/anime/overgeared" } })).unwrap();
        let episodes = episodes.as_array().unwrap();
        assert_eq!(episodes.len(), 10);
        assert_eq!(
            episodes[0],
            json!({ "url": "/overgeared-episode-5-english-subbed", "name": "Episode 5: English Subbed", "episodeNumber": 5.0, "fillermark": false, "dateUpload": 0 })
        );
        assert_eq!(episodes[1]["name"], "Episode 5: English Dubbed");
        let movies =
            call(&WCOSTREAM, &host, "episodes", json!({ "anime": { "url": "/anime/naruto-movies" } })).unwrap();
        assert_eq!(movies.as_array().unwrap().len(), 18);
        assert_eq!(
            movies[0]["name"],
            "Naruto Shippuden Movie 5 Special Chūnin Exam on Fire! Naruto vs. Konohamaru! English Dubbed"
        );
        assert_eq!(movies[0]["episodeNumber"], -1.0);
        // an episode page lists no episodes: it is its own one episode
        let single =
            call(&WCOTV, &host, "episodes", json!({ "anime": { "url": "/overgeared-episode-5-english-dubbed" } }))
                .unwrap();
        assert_eq!(
            single,
            json!([{ "url": "/overgeared-episode-5-english-dubbed", "name": "Episode 5: English Dubbed", "episodeNumber": 5.0, "fillermark": false, "dateUpload": 0 }])
        );
    }

    #[test]
    fn an_oldest_first_list_is_turned() {
        let html = r#"<nav id="sidebarEpisodeList"><a class="sidebar-episode-item" href="/s-episode-1-a"><span>S Episode 1 A</span></a><a class="sidebar-episode-item" href="/s-episode-2-b"><span>S Episode 2 B</span></a></nav>"#;
        let numbers: Vec<f32> = site::episodes(html, "/x").iter().map(|e| e.episode_number).collect();
        assert_eq!(numbers, [2.0, 1.0]);
    }

    #[test]
    fn episode_names_follow_the_kotlin_pattern() {
        assert_eq!(
            site::episode_name("Show Season 2 Episode 3 The Return"),
            ("Season 2 - Episode 3: The Return".into(), 103.0)
        );
        assert_eq!(site::episode_name("Show Episode 12"), ("Episode 12".into(), 12.0));
        assert_eq!(site::episode_name("Show Movie"), ("Show Movie".into(), -1.0));
    }

    #[test]
    fn the_player_hides_in_an_iframe_or_in_a_script() {
        let host = web();
        for (site, episode) in
            [(&WCOTV, "/overgeared-episode-5-english-dubbed"), (&WCOSTREAM, "/overgeared-episode-5-english-dubbed")]
        {
            let hosters = call(site, &host, "hosters", json!({ "episode": { "url": episode } })).unwrap();
            let hosters = hosters.as_array().unwrap();
            assert_eq!(hosters.len(), 1, "{}", site.name);
            assert_eq!(hosters[0]["hosterName"], "WCO");
            assert!(
                hosters[0]["hosterUrl"].as_str().unwrap().starts_with(
                    "https://embed.wcostream.com/inc/embed/index.php?file=neptun2%2Fongoing2%2FOvergeared."
                )
            );
            assert_eq!(hosters[0]["internalData"], format!("{}{episode}", site.base));
        }
    }

    #[test]
    fn a_player_gives_its_three_mp4_files_best_first() {
        let host = web();
        let hosters =
            call(&WCOTV, &host, "hosters", json!({ "episode": { "url": "/overgeared-episode-5-english-dubbed" } }))
                .unwrap();
        let videos = call(&WCOTV, &host, "videos", json!({ "hoster": hosters[0] })).unwrap();
        let titles: Vec<&str> = videos.as_array().unwrap().iter().map(|v| v["videoTitle"].as_str().unwrap()).collect();
        assert_eq!(titles, ["1080p", "720p", "480p"]);
        assert_eq!((videos[0]["resolution"].as_i64(), videos[0]["preferred"].as_bool()), (Some(1080), Some(true)));
        assert!(
            videos[0]["videoUrl"].as_str().unwrap().starts_with("https://neptun.wcostream.com/getvid?evid=qUP-ALy6")
        );
        // the file plays only to the User-Agent that asked for it
        assert_eq!(videos[2]["headers"], json!([["User-Agent", USER_AGENT]]));
        let asked = &host.sent()[4];
        assert!(
            asked.url.contains("/getvidlink.php?") && asked.headers.contains(&("User-Agent".into(), USER_AGENT.into()))
        );
        let sent: Vec<(String, String)> =
            host.sent().into_iter().skip(1).map(|r| (r.method, r.url.split('?').next().unwrap().to_owned())).collect();
        let steps: Vec<(&str, &str)> = sent.iter().map(|(m, u)| (m.as_str(), u.as_str())).collect();
        assert_eq!(
            steps,
            [
                ("GET", "https://embed.wcostream.com/inc/embed/index.php"),
                ("POST", "https://embed.wcostream.com/ad-verify"),
                ("GET", "https://embed.wcostream.com/inc/embed/video-js.php"),
                ("GET", "https://embed.wcostream.com/inc/embed/getvidlink.php"),
            ]
        );
        let beacon: Value = serde_json::from_str(host.sent()[2].body.as_deref().unwrap()).unwrap();
        assert_eq!((beacon["status"].as_str(), beacon["id"].as_str()), (Some("clear"), Some("1075584")));
        let nonce = beacon["nonce"].as_str().unwrap();
        assert_eq!(nonce.len(), 32);
        assert!(host.sent()[3].url.ends_with(&format!("&embed=neptun&n={nonce}")));
    }

    #[test]
    fn a_busy_player_says_so() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            "https://www.wco.tv/x" => (
                200,
                std::fs::read_to_string(format!("{}/fixtures/wcotv-episode.html", env!("CARGO_MANIFEST_DIR"))).unwrap(),
            ),
            "https://embed.wcostream.com/ad-verify" => (204, String::new()),
            url if url.contains("/video-js") => (503, "All Servers Are At Full Capacity".into()),
            _ => (200, String::new()),
        });
        let error = call(&WCOTV, &host, "videos", json!({ "episode": { "url": "/x" } })).unwrap();
        assert_eq!(error, json!([]));
        let hoster = json!({ "hosterUrl": "https://embed.wcostream.com/inc/embed/index.php?pid=1", "hosterName": "WCO", "internalData": "https://www.wco.tv/x" });
        assert_eq!(
            call(&WCOTV, &host, "videos", json!({ "hoster": hoster })),
            Err("the WCO player is busy or refused the episode; try again later".into())
        );
        // the newer player three times, then the older one
        let asked = |path: &str| host.sent().iter().rev().take(4).filter(|r| r.url.contains(path)).count();
        assert_eq!((asked("/video-js.php?"), asked("/video-js-old.php?")), (3, 1));
    }

    #[test]
    fn a_busy_player_is_asked_again() {
        let busy = std::cell::Cell::new(true);
        let host = TestHost::answering(move |request| {
            let page = |name: &str| {
                std::fs::read_to_string(format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
            };
            match request.url.as_str() {
                "https://embed.wcostream.com/ad-verify" => (204, String::new()),
                url if url.contains("/video-js.php?") && busy.replace(false) => (503, "Servers Busy".into()),
                url if url.contains("/video-js.php?") => (200, page("player.html")),
                url if url.contains("/getvidlink.php?") => (200, page("getvidlink.json")),
                _ => (200, String::new()),
            }
        });
        let hoster = json!({ "hosterUrl": "https://embed.wcostream.com/inc/embed/index.php?pid=1", "hosterName": "WCO", "internalData": "https://www.wco.tv/x" });
        let videos = call(&WCOTV, &host, "videos", json!({ "hoster": hoster })).unwrap();
        assert_eq!(videos.as_array().unwrap().len(), 3);
        assert_eq!(host.sent().iter().filter(|r| r.url.contains("/video-js.php?")).count(), 2);
    }
}
