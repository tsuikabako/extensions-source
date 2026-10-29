//! SubsPlease (<https://subsplease.org>), the anime release group, as an anime source plug-in for Kurobayomi. Ported from
//! the yuzono `all/subsplease` extension (Apache-2.0), through the site's JSON API. Each episode is one torrent of one
//! file, so a video is the API's magnet link as it is (no `index` needed). Torrent playback needs Kurobayomi 0.2.6 or
//! newer.

use kuro_guest::html::{Html, attr, first, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::{Map, Value};
use url::form_urlencoded;

/// The source id: the MD5 of `subsplease/all/1`, as the Kotlin extension has it. Keep it: backups name it.
const ID: i64 = 8766955949256845198;
const NAME: &str = "SubsPlease";
const BASE: &str = "https://subsplease.org";
/// The time zone the API gives times in, as the Kotlin asks it (dates carry their offset, so it changes nothing here).
const TZ: &str = "Europe/Berlin";
const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

fn get(host: &dyn Host, url: &str) -> Res<String> {
    Request::get(url).header("Referer", format!("{BASE}/")).header("X-Requested-With", "XMLHttpRequest").text(host, NAME)
}

fn json(host: &dyn Host, url: &str) -> Res<Value> {
    let body = get(host, url)?;
    let value: Value = serde_json::from_str(&body).map_err(|e| format!("{NAME} sent a bad answer: {e}"))?;
    match value["error"].as_str() {
        Some(error) => Err(format!("{NAME}: {error}")),
        None => Ok(value),
    }
}

/// Release dates as the API writes them ("Fri, 27 Mar 2026 16:02:39 +0100"), in milliseconds; 0 when unreadable.
fn date(text: Option<&str>) -> i64 {
    text.and_then(|t| chrono::DateTime::parse_from_rfc2822(t.trim()).ok()).map_or(0, |d| d.timestamp_millis())
}

fn anime(title: Option<&str>, page: Option<&str>, image: Option<&str>) -> Option<Anime> {
    Some(Anime {
        url: format!("/shows/{}", page?),
        title: title?.to_owned(),
        thumbnail_url: image.filter(|i| !i.is_empty()).map(|i| format!("{BASE}{i}")),
        ..Default::default()
    })
}

/// The week's schedule, Monday first: the shows airing now (the Kotlin's popular list).
fn parse_schedule(value: &Value) -> AnimesPage {
    let days = &value["schedule"];
    let animes = WEEKDAYS
        .iter()
        .flat_map(|day| days[day].as_array().map_or(&[][..], Vec::as_slice))
        .filter_map(|item| anime(item["title"].as_str(), item["page"].as_str(), item["image_url"].as_str()))
        .collect();
    AnimesPage { animes, has_next_page: false }
}

/// The releases of a `latest` or `search` answer (an object keyed by release name, or `[]` when there is none), newest
/// first, one per show.
fn parse_releases(value: &Value, has_next_page: bool) -> AnimesPage {
    let empty = Map::new();
    let mut releases: Vec<&Value> = value.as_object().unwrap_or(&empty).values().collect();
    releases.sort_by_key(|item| std::cmp::Reverse(date(item["release_date"].as_str())));
    let mut animes: Vec<Anime> = Vec::new();
    for item in releases {
        if let Some(found) = anime(item["show"].as_str(), item["page"].as_str(), item["image_url"].as_str()) {
            if !animes.iter().any(|a| a.url == found.url) {
                animes.push(found);
            }
        }
    }
    AnimesPage { animes, has_next_page }
}

/// The API's latest list has two pages; a third answers "limit_reached".
fn latest_url(page: i32) -> String {
    match page {
        ..=1 => format!("{BASE}/api/?f=latest&tz={TZ}"),
        n => format!("{BASE}/api/?f=latest&tz={TZ}&p={}", n - 1),
    }
}

fn search_url(query: &str) -> String {
    let query: String = form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("{BASE}/api/?f=search&tz={TZ}&s={query}")
}

fn parse_details(html: &str, url: &str) -> Anime {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    Anime {
        url: url.to_owned(),
        title: first(root, "h1.entry-title").map(text_of).unwrap_or_default(),
        description: first(root, "div.series-syn p").map(text_of).filter(|d| !d.is_empty()),
        thumbnail_url: attr(root, "img.img-responsive.img-center", "src").map(|src| if src.starts_with('/') { format!("{BASE}{src}") } else { src }),
        ..Default::default()
    }
}

/// The show id the episode API takes, from the show page.
fn show_id(html: &str) -> Option<String> {
    attr(Html::parse_document(html).root_element(), "#show-release-table", "sid")
}

fn show_url(sid: &str) -> String {
    format!("{BASE}/api/?f=show&tz={TZ}&sid={sid}")
}

/// The episode releases of a show answer (the batches, under `batch`, are left out as the Kotlin does).
fn releases(value: &Value) -> Vec<&Value> {
    value["episode"].as_object().map(|m| m.values().collect()).unwrap_or_default()
}

/// The episodes of a show answer, newest first. The url is the Kotlin's: the show API with the episode's `num`.
fn parse_episodes(value: &Value, sid: &str) -> Vec<Episode> {
    let mut list: Vec<(i64, Episode)> = releases(value)
        .into_iter()
        .filter_map(|item| {
            let num = item["episode"].as_str()?;
            let lead: String = num.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            let uploaded = date(item["release_date"].as_str());
            let episode = Episode {
                url: format!("/api/?f=show&tz={TZ}&sid={sid}&num={num}"),
                name: format!("Episode {num}"),
                episode_number: lead.parse().unwrap_or(-1.0),
                date_upload: uploaded,
                ..Default::default()
            };
            Some((uploaded, episode))
        })
        .collect();
    list.sort_by(|(a_date, a), (b_date, b)| b.episode_number.total_cmp(&a.episode_number).then(b_date.cmp(a_date)));
    list.into_iter().map(|(_, episode)| episode).collect()
}

/// The `num` of an episode url.
fn episode_num(url: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    form_urlencoded::parse(query.as_bytes()).find(|(key, _)| key == "num").map(|(_, value)| value.into_owned())
}

/// One video per resolution of the episode `num`, best first; 1080p is the one the player starts with (the Kotlin's
/// default quality).
fn parse_videos(value: &Value, num: &str) -> Vec<Video> {
    let mut videos: Vec<Video> = releases(value)
        .into_iter()
        .filter(|item| item["episode"].as_str() == Some(num))
        .flat_map(|item| item["downloads"].as_array().cloned().unwrap_or_default())
        .filter_map(|download| {
            let res = download["res"].as_str()?.to_owned();
            let magnet = download["magnet"].as_str().filter(|m| m.starts_with("magnet:"))?;
            Some(Video {
                video_url: magnet.to_owned(),
                video_title: format!("{res}p"),
                resolution: res.parse().ok(),
                preferred: res == "1080",
                ..Default::default()
            })
        })
        .collect();
    videos.sort_by_key(|v| std::cmp::Reverse(v.resolution.unwrap_or(0)));
    videos
}

struct SubsPlease;

impl Source for SubsPlease {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "all", supports_latest: true, request_gap_ms: Some(300) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => to_value(parse_schedule(&json(host, &format!("{BASE}/api/?f=schedule&tz={TZ}"))?)),
            "latest" => to_value(parse_releases(&json(host, &latest_url(input.page()))?, input.page() < 2)),
            "search" if input.query().trim().is_empty() => to_value(parse_schedule(&json(host, &format!("{BASE}/api/?f=schedule&tz={TZ}"))?)),
            "search" => to_value(parse_releases(&json(host, &search_url(input.query()))?, false)),
            "details" => {
                let url = input.url("anime")?;
                to_value(parse_details(&get(host, &format!("{BASE}{url}/"))?, url))
            }
            "episodes" => {
                let page = get(host, &format!("{BASE}{}/", input.url("anime")?))?;
                let sid = show_id(&page).ok_or("this show page has no episode list")?;
                to_value(parse_episodes(&json(host, &show_url(&sid))?, &sid))
            }
            "videos" => {
                let url = input.url("episode")?;
                let num = episode_num(url).ok_or("this episode has no number")?;
                let videos = parse_videos(&json(host, &format!("{BASE}{url}"))?, &num);
                if videos.is_empty() {
                    return Err(format!("{NAME} lists no torrent for episode {num}"));
                }
                to_value(videos)
            }
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(SubsPlease);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;
    use serde_json::json;

    // Saved from subsplease.org on 2026-10-29.
    const SCHEDULE: &str = include_str!("../fixtures/schedule.json");
    const LATEST: &str = include_str!("../fixtures/latest.json");
    const SEARCH: &str = include_str!("../fixtures/search.json");
    const SHOW: &str = include_str!("../fixtures/show.html");
    const EPISODES: &str = include_str!("../fixtures/episodes.json");

    fn site() -> TestHost {
        TestHost::answering(|request| {
            let url = request.url.as_str();
            let body = if url.contains("f=schedule") {
                SCHEDULE
            } else if url.contains("f=latest&tz=Europe/Berlin&p=2") {
                r#"{"error":"limit_reached"}"#
            } else if url.contains("f=latest") {
                LATEST
            } else if url.contains("s=nothing") {
                "[]"
            } else if url.contains("f=search") {
                SEARCH
            } else if url.contains("f=show") {
                EPISODES
            } else if url == "https://subsplease.org/shows/sousou-no-frieren-s2/" {
                SHOW
            } else {
                return (404, String::new());
            };
            (200, body.into())
        })
    }

    fn run(host: &TestHost, op: &str, args: Value) -> Res<Value> {
        kuro_guest::invoke(&SubsPlease, host, op, &json!({ "args": args }).to_string()).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_kotlin_id() {
        assert_eq!(run(&TestHost::default(), "info", json!({})).unwrap()["id"].as_i64(), Some(ID));
    }

    #[test]
    fn popular_is_the_schedule_monday_first() {
        let page = run(&site(), "popular", json!({ "page": 1 })).unwrap();
        let animes = page["animes"].as_array().unwrap();
        assert!(animes.len() > 20);
        assert_eq!(animes[0]["url"], json!("/shows/doumo-suki-na-hito-ni-horegusuri-wo-irai-sareta-majo-desu"));
        assert_eq!(animes[0]["thumbnailUrl"], json!("https://subsplease.org/wp-content/uploads/2026/10/160033.jpg"));
        assert_eq!(page["hasNextPage"], json!(false));
    }

    #[test]
    fn latest_and_search_are_releases_newest_first_one_per_show() {
        let host = site();
        let page = run(&host, "latest", json!({ "page": 1 })).unwrap();
        let animes = page["animes"].as_array().unwrap();
        assert_eq!(animes[0]["title"], json!("Link Click S3"));
        let mut urls: Vec<_> = animes.iter().map(|a| a["url"].clone()).collect();
        urls.dedup();
        assert_eq!(urls.len(), animes.len());
        assert_eq!(page["hasNextPage"], json!(true));
        assert!(run(&host, "latest", json!({ "page": 3 })).unwrap_err().contains("limit_reached"));
        let found = run(&host, "search", json!({ "query": "frieren" })).unwrap();
        assert!(found["animes"].as_array().unwrap().iter().any(|a| a["url"] == json!("/shows/sousou-no-frieren-s2")));
        assert!(host.sent().iter().any(|r| r.url == "https://subsplease.org/api/?f=search&tz=Europe/Berlin&s=frieren"));
        assert_eq!(run(&host, "search", json!({ "query": "nothing" })).unwrap()["animes"], json!([]));
    }

    #[test]
    fn details_read_the_show_page() {
        let anime = parse_details(SHOW, "/shows/sousou-no-frieren-s2");
        assert_eq!(anime.title, "Sousou no Frieren S2");
        assert_eq!(anime.description.as_deref(), Some("Second season of Sousou no Frieren."));
        assert_eq!(anime.thumbnail_url.as_deref(), Some("https://subsplease.org/wp-content/uploads/2026/01/154528.jpg"));
    }

    #[test]
    fn episodes_and_videos_come_from_the_show_api() {
        let host = site();
        let episodes = run(&host, "episodes", json!({ "anime": { "url": "/shows/sousou-no-frieren-s2" } })).unwrap();
        let episodes = episodes.as_array().unwrap();
        assert_eq!(episodes.len(), 10);
        assert_eq!(episodes[0]["name"], json!("Episode 10"));
        assert_eq!(episodes[0]["episodeNumber"], json!(10.0));
        assert_eq!(episodes[0]["url"], json!("/api/?f=show&tz=Europe/Berlin&sid=1121&num=10"));
        assert_eq!(episodes[0]["dateUpload"], json!(1774623759000_i64));
        assert_eq!(episodes[9]["name"], json!("Episode 01"));
        assert_eq!(host.sent()[1].url, "https://subsplease.org/api/?f=show&tz=Europe/Berlin&sid=1121");

        let videos = run(&host, "videos", json!({ "episode": episodes[0] })).unwrap();
        let titles: Vec<_> = videos.as_array().unwrap().iter().map(|v| v["videoTitle"].as_str().unwrap().to_owned()).collect();
        assert_eq!(titles, ["1080p", "720p", "480p"]);
        assert_eq!(videos[0]["preferred"], json!(true));
        let url = videos[2]["videoUrl"].as_str().unwrap();
        assert!(url.starts_with("magnet:?xt=urn:btih:JS2UBQPDGNQW6IB2RGKIROFLNM6MBVIS&dn="));
        assert!(run(&host, "videos", json!({ "episode": { "url": "/api/?f=show&tz=Europe/Berlin&sid=1121&num=99" } })).is_err());
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(run(&TestHost::default(), "hosters", json!({})), Err("unknown op: hosters".to_owned()));
    }
}
