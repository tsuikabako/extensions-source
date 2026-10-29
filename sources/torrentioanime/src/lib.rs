//! Torrentio Anime as an anime source plug-in for Kurobayomi. Ported from the yuzono `all/torrentioanime` extension
//! (Apache-2.0), torrents only: the titles come from AniList, the episodes from ani.zip (with Kitsu for the id Torrentio
//! names shows by), and the videos from Torrentio's stream lists (<https://torrentio.strem.fun>), each a magnet link that
//! names its file with `&index=N` (`kuro-torrent`). Torrent playback needs Kurobayomi 0.2.6 or newer.

use kuro_guest::wire::{Anime, AnimesPage, Episode, Video, status};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::{Value, json};

/// The source id: the MD5 of `torrentio anime (torrent / debrid)/all/1`, the Kotlin extension's name. Keep it: backups
/// name it.
const ID: i64 = 7490206640756700883;
const NAME: &str = "Torrentio Anime";
const TORRENTIO: &str = "https://torrentio.strem.fun";
const ANILIST: &str = "https://graphql.anilist.co";

/// Torrentio's options, the Kotlin defaults: its default providers, without 720p, 480p, rips, screeners, cams and
/// unknown quality, sorted by quality then seeders.
const OPTIONS: &str = "providers=yts,eztv,rarbg,1337x,ext,thepiratebay,kickasstorrents,torrentgalaxy,magnetdl,horriblesubs,nyaasi,tokyotosho,anidex,nekobt|qualityfilter=720p,480p,other,scr,cam,unknown|sort=quality";

/// Trackers added to every magnet link besides the ones Torrentio names for the torrent: the Kotlin's anime list. The
/// Kotlin also fetched a public tracker list from GitHub for every video; a fixed list saves that request.
const TRACKERS: &[&str] = &[
    "http://nyaa.tracker.wf:7777/announce",
    "http://anidex.moe:6969/announce",
    "http://tracker.anirena.com:80/announce",
    "udp://tracker.uw0.xyz:6969/announce",
    "http://share.camoe.cn:8080/announce",
    "http://t.nyaatracker.com:80/announce",
    "udp://exodus.desync.com:6969/announce",
    "udp://explodie.org:6969/announce",
    "udp://open.stealth.si:80/announce",
    "udp://opentor.org:2710/announce",
    "udp://retracker.lanta-net.ru:2710/announce",
    "udp://tracker.cyberia.is:6969/announce",
    "udp://tracker.dler.org:6969/announce",
    "udp://tracker.ds.is:6969/announce",
    "udp://tracker.openbittorrent.com:6969/announce",
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://tracker.tiny-vps.com:6969/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://www.torrent.eu.org:451/announce",
];

/// The fields of an AniList title every list and the details ask for.
const MEDIA: &str = "id title { romaji english native } coverImage { extraLarge large } description status tags { name } \
                     genres studios { nodes { name } } countryOfOrigin isAdult";

fn list_query() -> String {
    format!(
        "query ($page: Int, $perPage: Int, $sort: [MediaSort], $search: String, $status: [MediaStatus]) {{ \
         Page(page: $page, perPage: $perPage) {{ pageInfo {{ hasNextPage }} \
         media(type: ANIME, sort: $sort, search: $search, status_in: $status, isAdult: false) {{ {MEDIA} }} }} }}"
    )
}

fn latest_query() -> String {
    format!(
        "query ($page: Int, $perPage: Int, $sort: [AiringSort], $before: Int) {{ \
         Page(page: $page, perPage: $perPage) {{ pageInfo {{ hasNextPage }} \
         airingSchedules(airingAt_greater: 0, airingAt_lesser: $before, sort: $sort) {{ media {{ {MEDIA} }} }} }} }}"
    )
}

fn details_query() -> String {
    format!("query ($id: Int) {{ Media(id: $id, isAdult: false) {{ {MEDIA} season seasonYear format episodes }} }}")
}

fn anilist(host: &dyn Host, query: String, variables: Value) -> Res<Value> {
    let body = json!({ "query": query, "variables": variables }).to_string();
    let text = Request::post(ANILIST, body)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Referer", "https://anilist.co")
        .text(host, "AniList")?;
    let answer: Value = serde_json::from_str(&text).map_err(|e| format!("AniList sent a bad answer: {e}"))?;
    match answer["errors"][0]["message"].as_str() {
        Some(error) => Err(format!("AniList: {error}")),
        None => Ok(answer),
    }
}

fn get_json(host: &dyn Host, url: &str, site: &str) -> Res<Value> {
    serde_json::from_str(&Request::get(url).header("Accept", "application/json").text(host, site)?)
        .map_err(|e| format!("{site} sent a bad answer: {e}"))
}

/// AniList's HTML description as plain text.
fn plain_text(html: &str) -> String {
    let html = html.replace("<br>\n", "\n").replace("<br />\n", "\n").replace("<br>", "\n").replace("<br />", "\n");
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.replace("&amp;", "&").replace("&quot;", "\"").replace("&#039;", "'").replace("&lt;", "<").replace("&gt;", ">").trim().to_owned()
}

fn names(list: &Value) -> Vec<String> {
    list.as_array().map(|items| items.iter().filter_map(|i| i["name"].as_str().or(i.as_str()).map(str::to_owned)).collect()).unwrap_or_default()
}

/// One AniList title (romaji, the Kotlin's default), stored by its AniList id as the Kotlin does.
fn parse_media(media: &Value) -> Option<Anime> {
    let id = media["id"].as_i64()?;
    let mut genres: Vec<String> = names(&media["tags"]).into_iter().chain(names(&media["genres"])).collect();
    genres.sort();
    genres.dedup();
    let mut studios = names(&media["studios"]["nodes"]);
    studios.sort();
    Some(Anime {
        url: id.to_string(),
        title: media["title"]["romaji"].as_str().or(media["title"]["english"].as_str()).unwrap_or_default().to_owned(),
        description: media["description"].as_str().map(plain_text),
        thumbnail_url: media["coverImage"]["extraLarge"].as_str().or(media["coverImage"]["large"].as_str()).map(str::to_owned),
        author: Some(studios.join(", ")).filter(|s| !s.is_empty()),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status: match media["status"].as_str() {
            Some("RELEASING") => status::ONGOING,
            Some("FINISHED") => status::COMPLETED,
            Some("HIATUS") => status::ON_HIATUS,
            Some("NOT_YET_RELEASED") => status::LICENSED,
            _ => status::UNKNOWN,
        },
        ..Default::default()
    })
}

/// A page of titles. The latest list (recent airings) leaves out Chinese and adult titles, as the Kotlin does, and shows
/// each title once.
fn parse_page(answer: &Value, latest: bool) -> AnimesPage {
    let page = &answer["data"]["Page"];
    let media: Vec<&Value> = if latest {
        page["airingSchedules"].as_array().map(|a| a.iter().map(|s| &s["media"]).collect()).unwrap_or_default()
    } else {
        page["media"].as_array().map(|a| a.iter().collect()).unwrap_or_default()
    };
    let mut animes: Vec<Anime> = Vec::new();
    for media in media {
        if latest && (media["countryOfOrigin"] == "CN" || media["isAdult"] == true) {
            continue;
        }
        if let Some(anime) = parse_media(media).filter(|a| !animes.iter().any(|b| b.url == a.url)) {
            animes.push(anime);
        }
    }
    AnimesPage { animes, has_next_page: page["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false) }
}

fn parse_details(answer: &Value, url: &str) -> Res<Anime> {
    let media = &answer["data"]["Media"];
    let mut anime = parse_media(media).ok_or("AniList has no such title")?;
    anime.url = url.to_owned();
    let mut extra = Vec::new();
    let season = [media["season"].as_str().map(str::to_owned), media["seasonYear"].as_i64().map(|y| y.to_string())];
    let season: Vec<String> = season.into_iter().flatten().collect();
    if !season.is_empty() {
        extra.push(format!("Release: {}", season.join(" ")));
    }
    if let Some(format) = media["format"].as_str() {
        extra.push(format!("Type: {format}"));
    }
    if let Some(count) = media["episodes"].as_i64() {
        extra.push(format!("Total Episode Count: {count}"));
    }
    if !extra.is_empty() {
        let text = anime.description.take().unwrap_or_default();
        anime.description = Some(format!("{text}\n\n{}", extra.join("\n")).trim().to_owned());
    }
    Ok(anime)
}

/// "2023-09-29" in milliseconds (midnight UTC), 0 when unreadable.
fn day(text: Option<&str>) -> i64 {
    let mut parts = text.unwrap_or_default().splitn(3, '-').map(|p| p.parse::<u32>().ok());
    let (Some(Some(y)), Some(Some(m)), Some(Some(d))) = (parts.next(), parts.next(), parts.next()) else { return 0 };
    chrono::NaiveDate::from_ymd_opt(y as i32, m, d).and_then(|date| date.and_hms_opt(0, 0, 0)).map_or(0, |t| t.and_utc().timestamp_millis())
}

/// The Kitsu id Torrentio names a title by: ani.zip's mapping, else Kitsu's own AniList mapping.
fn kitsu_id(host: &dyn Host, zip: &Value, anilist_id: &str) -> Res<Option<i64>> {
    if let Some(id) = zip["mappings"]["kitsu_id"].as_i64() {
        return Ok(Some(id));
    }
    let url = format!("https://kitsu.io/api/edge/mappings?filter[externalSite]=anilist/anime&filter[externalId]={anilist_id}&include=item");
    let answer = get_json(host, &url, "Kitsu")?;
    Ok(answer["data"][0]["relationships"]["item"]["data"]["id"].as_str().and_then(|id| id.parse().ok()))
}

/// The episodes of an ani.zip answer, newest first: every aired episode of a series (the specials, numbered "S1", are
/// left out as in the Kotlin), or the one "Movie". Other formats have none.
fn parse_episodes(zip: &Value, kitsu: i64, now_ms: i64) -> Vec<Episode> {
    let aired = |e: &Value| day(e["airDate"].as_str().or(e["airdate"].as_str()));
    match zip["mappings"]["type"].as_str() {
        Some("TV" | "ONA" | "OVA") => {
            let mut list: Vec<Episode> = zip["episodes"]
                .as_object()
                .map(|episodes| episodes.values().collect::<Vec<_>>())
                .unwrap_or_default()
                .into_iter()
                .filter(|e| aired(e) <= now_ms)
                .filter_map(|e| {
                    let label = e["episode"].as_str()?;
                    let number: f32 = label.parse().ok()?;
                    let name = match e["title"]["en"].as_str().filter(|t| !t.is_empty()) {
                        Some(title) => format!("Episode {label}: {title}"),
                        None => format!("Episode {label}"),
                    };
                    Some(Episode {
                        url: format!("/stream/series/kitsu:{kitsu}:{number:.0}.json"),
                        name,
                        episode_number: number,
                        date_upload: aired(e),
                        ..Default::default()
                    })
                })
                .collect();
            list.sort_by(|a, b| b.episode_number.total_cmp(&a.episode_number));
            list
        }
        Some("MOVIE") => vec![Episode {
            url: format!("/stream/movie/kitsu:{kitsu}.json"),
            name: "Movie".into(),
            episode_number: 1.0,
            date_upload: zip["episodes"]["1"].as_object().map_or(0, |e| day(e.get("airDate").or(e.get("airdate")).and_then(Value::as_str))),
            ..Default::default()
        }],
        _ => Vec::new(),
    }
}

/// One video per Torrentio stream, in Torrentio's order (best quality first): the magnet link of its torrent and file.
fn parse_streams(answer: &Value) -> Vec<Video> {
    let streams = answer["streams"].as_array().map(Vec::as_slice).unwrap_or_default();
    streams
        .iter()
        .filter_map(|stream| {
            let hash = stream["infoHash"].as_str().filter(|h| !h.is_empty())?;
            let mut trackers: Vec<String> = stream["sources"]
                .as_array()
                .map(|s| s.iter().filter_map(|t| t.as_str()?.strip_prefix("tracker:").map(str::to_owned)).collect())
                .unwrap_or_default();
            for tracker in TRACKERS {
                if !trackers.iter().any(|t| t == tracker) {
                    trackers.push((*tracker).to_owned());
                }
            }
            let file = stream["behaviorHints"]["filename"].as_str().unwrap_or(hash);
            let index = stream["fileIdx"].as_u64().map(|i| i as usize);
            let quality = stream["name"].as_str().unwrap_or_default().trim_start_matches("Torrentio").trim().replace('\n', " ");
            let title = stream["title"].as_str().unwrap_or_default().lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" · ");
            Some(Video {
                video_url: kuro_torrent::magnet(hash, file, &trackers, index),
                video_title: [quality.as_str(), title.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · "),
                resolution: kuro_torrent::height(&quality),
                ..Default::default()
            })
        })
        .enumerate()
        .map(|(i, video)| Video { preferred: i == 0, ..video })
        .collect()
}

struct TorrentioAnime;

impl Source for TorrentioAnime {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "all", supports_latest: true, request_gap_ms: Some(300) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let page = input.page().max(1);
        match op {
            "popular" => {
                let variables = json!({ "page": page, "perPage": 30, "sort": "TRENDING_DESC", "status": ["FINISHED", "RELEASING"] });
                to_value(parse_page(&anilist(host, list_query(), variables)?, false))
            }
            "latest" => {
                // airings that started at least ~3 hours ago, as the Kotlin asks
                let before = host.unix_seconds() as i64 - 10_000;
                let variables = json!({ "page": page, "perPage": 30, "sort": "TIME_DESC", "before": before });
                to_value(parse_page(&anilist(host, latest_query(), variables)?, true))
            }
            "search" => {
                let mut variables = json!({ "page": page, "perPage": 30, "sort": "POPULARITY_DESC" });
                if !input.query().trim().is_empty() {
                    variables["search"] = json!(input.query().trim());
                }
                to_value(parse_page(&anilist(host, list_query(), variables)?, false))
            }
            "details" => {
                let url = input.url("anime")?;
                let id: i64 = url.trim().parse().map_err(|_| format!("not an AniList id: {url}"))?;
                to_value(parse_details(&anilist(host, details_query(), json!({ "id": id }))?, url)?)
            }
            "episodes" => {
                let id = input.url("anime")?.trim();
                let zip = get_json(host, &format!("https://api.ani.zip/mappings?anilist_id={id}"), "ani.zip")?;
                let Some(kitsu) = kitsu_id(host, &zip, id)? else { return to_value(Vec::<Episode>::new()) };
                to_value(parse_episodes(&zip, kitsu, host.unix_seconds() as i64 * 1000))
            }
            "videos" => {
                let url = input.url("episode")?;
                if !url.starts_with("/stream/") {
                    return Err(format!("not a Torrentio episode: {url}"));
                }
                to_value(parse_streams(&get_json(host, &format!("{TORRENTIO}/{OPTIONS}{url}"), "Torrentio")?))
            }
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(TorrentioAnime);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    // Saved on 2026-10-29: AniList (popular, latest, search "frieren", details of 154587), ani.zip of 154587 (Kitsu
    // 46474) and Torrentio's first six streams of its episode 1.
    const POPULAR: &str = include_str!("../fixtures/popular.json");
    const LATEST: &str = include_str!("../fixtures/latest.json");
    const SEARCH: &str = include_str!("../fixtures/search.json");
    const DETAILS: &str = include_str!("../fixtures/details.json");
    const ANIZIP: &str = include_str!("../fixtures/anizip.json");
    const STREAMS: &str = include_str!("../fixtures/streams.json");

    /// 2026-10-29 in seconds.
    const NOW: u64 = 1_793_260_000;

    fn site() -> TestHost {
        let mut host = TestHost::answering(|request| {
            let url = request.url.as_str();
            let body = request.body.as_deref().unwrap_or_default();
            let answer = if url == ANILIST && body.contains("airingSchedules") {
                LATEST
            } else if url == ANILIST && body.contains("Media(id") {
                DETAILS
            } else if url == ANILIST && body.contains("\"search\"") {
                SEARCH
            } else if url == ANILIST {
                POPULAR
            } else if url == "https://api.ani.zip/mappings?anilist_id=154587" {
                ANIZIP
            } else if url == "https://api.ani.zip/mappings?anilist_id=1" {
                r#"{"mappings":{"type":"MOVIE"},"episodes":{"1":{"episode":"1","airDate":"2001-07-20"}}}"#
            } else if url.contains("kitsu.io") && url.contains("externalId]=1&") {
                r#"{"data":[{"relationships":{"item":{"data":{"type":"anime","id":"176"}}}}]}"#
            } else if url.starts_with(TORRENTIO) {
                STREAMS
            } else {
                return (404, String::new());
            };
            (200, answer.into())
        });
        host.now = NOW;
        host
    }

    fn run(host: &TestHost, op: &str, args: Value) -> Res<Value> {
        kuro_guest::invoke(&TorrentioAnime, host, op, &json!({ "args": args }).to_string()).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_kotlin_id() {
        assert_eq!(run(&TestHost::default(), "info", json!({})).unwrap()["id"].as_i64(), Some(ID));
    }

    #[test]
    fn lists_are_anilist_titles_by_id() {
        let host = site();
        let popular = run(&host, "popular", json!({ "page": 1 })).unwrap();
        assert_eq!(popular["hasNextPage"], json!(true));
        let first = &popular["animes"][0];
        assert_eq!((first["url"].clone(), first["title"].clone()), (json!("195539"), json!("Cyberpunk: Edgerunners 2")));
        assert!(first["thumbnailUrl"].as_str().unwrap().starts_with("https://s4.anilist.co/"));
        let sent: Value = serde_json::from_str(host.sent()[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(sent["variables"]["sort"], json!("TRENDING_DESC"));

        let latest = run(&host, "latest", json!({ "page": 1 })).unwrap();
        let urls: Vec<_> = latest["animes"].as_array().unwrap().iter().map(|a| a["url"].clone()).collect();
        assert!(!urls.is_empty());
        assert_eq!(urls.len(), urls.iter().collect::<std::collections::HashSet<_>>().len());
        let sent: Value = serde_json::from_str(host.sent()[1].body.as_deref().unwrap()).unwrap();
        assert_eq!(sent["variables"]["before"], json!(NOW - 10_000));

        let found = run(&host, "search", json!({ "query": "frieren" })).unwrap();
        assert_eq!(found["animes"][0]["title"], json!("Sousou no Frieren"));
        assert_eq!(found["animes"][0]["url"], json!("154587"));
    }

    #[test]
    fn details_add_the_season_and_type() {
        let anime = run(&site(), "details", json!({ "anime": { "url": "154587" } })).unwrap();
        assert_eq!(anime["title"], json!("Sousou no Frieren"));
        assert_eq!(anime["status"], json!(status::COMPLETED));
        let description = anime["description"].as_str().unwrap();
        assert!(!description.contains("<br>") && description.contains("Type: TV"), "{description}");
        assert_eq!(plain_text("A<br>\nB <i>c</i> &amp; d"), "A\nB c & d");
    }

    #[test]
    fn episodes_are_the_aired_episodes_newest_first() {
        let episodes = run(&site(), "episodes", json!({ "anime": { "url": "154587" } })).unwrap();
        let episodes = episodes.as_array().unwrap();
        assert_eq!(episodes.len(), 28);
        assert_eq!(episodes[0]["url"], json!("/stream/series/kitsu:46474:28.json"));
        let last = &episodes[27];
        assert_eq!(last["url"], json!("/stream/series/kitsu:46474:1.json"));
        assert_eq!(last["name"], json!("Episode 1: The Journey`s End"));
        assert_eq!(last["dateUpload"], json!(1695945600000_i64));
    }

    #[test]
    fn a_movie_is_one_episode_and_kitsu_fills_a_missing_id() {
        let host = site();
        let episodes = run(&host, "episodes", json!({ "anime": { "url": "1" } })).unwrap();
        assert_eq!(episodes, json!([{ "url": "/stream/movie/kitsu:176.json", "name": "Movie", "episodeNumber": 1.0, "fillermark": false, "dateUpload": 995587200000_i64 }]));
    }

    #[test]
    fn videos_are_magnets_naming_their_file() {
        let host = site();
        let videos = run(&host, "videos", json!({ "episode": { "url": "/stream/series/kitsu:46474:1.json" } })).unwrap();
        assert_eq!(host.sent()[0].url, format!("{TORRENTIO}/{OPTIONS}/stream/series/kitsu:46474:1.json"));
        let videos = videos.as_array().unwrap();
        assert_eq!(videos.len(), 6);
        let first = &videos[0];
        assert_eq!(first["preferred"], json!(true));
        assert_eq!(first["resolution"], json!(2160));
        assert!(first["videoTitle"].as_str().unwrap().starts_with("4k · [Feibanyama]"));
        let url = first["videoUrl"].as_str().unwrap();
        assert!(url.starts_with("magnet:?xt=urn:btih:091b1b30d24c4d41759da1b0e24ddfffc89925c4&dn=%5BFeibanyama%5D"));
        assert!(url.ends_with("&index=15"));
        let magnet = kuro_torrent::read_magnet(url).unwrap();
        assert_eq!(magnet.trackers[0], "udp://tracker.opentrackr.org:1337/announce");
        assert!(magnet.trackers.iter().any(|t| t == "udp://www.torrent.eu.org:451/announce"));
        assert_eq!(videos[3]["resolution"], json!(1080));
        assert!(videos[4]["videoUrl"].as_str().unwrap().ends_with("&index=0"));
        assert!(run(&host, "videos", json!({ "episode": { "url": "https://x/" } })).is_err());
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(run(&TestHost::default(), "hosters", json!({})), Err("unknown op: hosters".to_owned()));
    }
}
