//! Nyaa (<https://nyaa.si>), the anime torrent tracker, as an anime source plug-in for Kurobayomi. Ported from the yuzono
//! `all/nyaatorrent` extension (Apache-2.0), nyaa.si only (its adult sister site is left out). A title is one torrent;
//! its episodes are the video files inside it, each played from a magnet link that names the file with `&index=N`
//! (`kuro-torrent`). Torrent playback needs Kurobayomi 0.2.6 or newer.

use kuro_guest::html::{ElementRef, Html, attr, first, sel, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;
use url::form_urlencoded;

/// The source id: the MD5 of `nyaa (torrent)/all/1`, as the Kotlin extension has it. Keep it: backups name it.
const ID: i64 = 5310001068353260432;
const NAME: &str = "Nyaa (Torrent)";
const BASE: &str = "https://nyaa.si";
/// The anime category (all of its sub-categories).
const CATEGORY: &str = "1_0";

fn get(host: &dyn Host, url: &str) -> Res<String> {
    Request::get(url).header("Referer", format!("{BASE}/")).text(host, NAME)
}

fn absolute(path: &str) -> String {
    if path.starts_with("http") { path.to_owned() } else { format!("{BASE}{path}") }
}

fn popular_url(page: i32) -> String {
    format!("{BASE}/?f=0&c={CATEGORY}&s=seeders&o=desc&p={}", page.max(1))
}

fn latest_url(page: i32) -> String {
    format!("{BASE}/?f=0&c={CATEGORY}&p={}", page.max(1))
}

fn search_url(query: &str, page: i32) -> String {
    let query: String = form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("{BASE}/?f=0&c={CATEGORY}&q={query}&p={}", page.max(1))
}

/// The torrent id of a search text that names one (`id:123`, or a `https://nyaa.si/view/123` link), as the Kotlin
/// reads it.
fn searched_id(query: &str) -> Option<&str> {
    let query = query.trim();
    let id = query.strip_prefix("id:").or_else(|| query.strip_prefix(BASE)?.strip_prefix("/view/"))?;
    let id = id.split(['#', '?', '/']).next()?;
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then_some(id)
}

/// One page of the torrent table.
fn parse_list(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let animes = doc
        .select(&sel("table.torrent-list tbody tr"))
        .filter_map(|row| {
            let link = first(row, "td:nth-child(2) a:not(.comments)[title]")?;
            let url = link.value().attr("href")?.split('#').next()?.to_owned();
            let title = link.value().attr("title")?.trim().to_owned();
            Some(Anime { url, title, ..Default::default() })
        })
        .collect();
    let has_next_page = doc.select(&sel("ul.pagination a[rel=next]")).next().is_some();
    AnimesPage { animes, has_next_page }
}

/// The text of the `col`th cell of the `row`th row of the torrent's info panel (both from 1).
fn panel_cell(doc: &Html, row: usize, col: usize) -> String {
    let css = format!("div.panel-body > div.row:nth-child({row}) > div:nth-child({col})");
    doc.select(&sel(&css)).next().map(text_of).unwrap_or_default()
}

/// The first image link in a description (the Kotlin's image regex, without one): the poster most uploaders put there.
fn first_image(text: &str) -> Option<String> {
    const IMAGES: &[&str] = &[".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp", ".tiff"];
    text.split(|c: char| c.is_whitespace() || "()[]<>\"'".contains(c))
        .find(|word| word.starts_with("http") && IMAGES.iter().any(|ext| word.to_ascii_lowercase().ends_with(ext)))
        .map(str::to_owned)
}

fn parse_details(html: &str, url: &str) -> Anime {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let title = first(root, "h3.panel-title").map(text_of).unwrap_or_default();
    let genre = [
        ("Category", panel_cell(&doc, 1, 2)),
        ("Seeders", panel_cell(&doc, 2, 4)),
        ("Leechers", panel_cell(&doc, 3, 4)),
        ("File Size", panel_cell(&doc, 4, 2)),
    ]
    .iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(label, value)| format!("{label}: {value}"))
    .collect::<Vec<_>>()
    .join(", ");
    let description = first(root, "#torrent-description").map(|e| e.text().collect::<String>().trim().to_owned());
    let author = Some(panel_cell(&doc, 2, 2)).filter(|s| !s.is_empty());
    Anime {
        url: url.to_owned(),
        title,
        thumbnail_url: description.as_deref().and_then(first_image),
        description: description.filter(|d| !d.is_empty()),
        author,
        genre: Some(genre).filter(|g| !g.is_empty()),
        status: kuro_guest::wire::status::COMPLETED,
        ..Default::default()
    }
}

/// What the episode list needs from a torrent's page.
#[derive(Debug, PartialEq)]
struct ViewPage {
    magnet: kuro_torrent::Magnet,
    /// The `.torrent` file's path on the site.
    torrent: String,
    /// The upload time, in milliseconds.
    date: i64,
    /// The file names the page lists, in the page's order (not the torrent's), with their sizes as the page shows them.
    listed: Vec<(String, String)>,
}

fn parse_view(html: &str) -> Res<ViewPage> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let link = attr(root, "div.panel-footer a[href^='magnet:']", "href").ok_or("this torrent page has no magnet link")?;
    let magnet = kuro_torrent::read_magnet(&link).ok_or("this torrent page's magnet link has no info hash")?;
    let torrent = attr(root, "div.panel-footer a[href$='.torrent']", "href").unwrap_or_default();
    let date = attr(root, "div.panel-body [data-timestamp]", "data-timestamp").and_then(|s| s.parse::<i64>().ok()).map_or(0, |s| s * 1000);
    let listed = doc
        .select(&sel("div.torrent-file-list li"))
        // a file's own icon, not one of a folder's files
        .filter(|li| li.children().filter_map(ElementRef::wrap).next().is_some_and(|i| i.value().classes().any(|c| c == "fa-file")))
        .map(|li: ElementRef| {
            let size = first(li, "span.file-size").map(text_of).unwrap_or_default();
            let name = kuro_guest::html::own_text(li);
            (name, size.trim_matches(['(', ')']).to_owned())
        })
        .collect();
    Ok(ViewPage { magnet, torrent, date, listed })
}

/// The episodes of one torrent: one per video file, newest (last in the torrent) first. Each url is the file's magnet
/// link, so `videos` needs no request.
fn episodes(view: &ViewPage, torrent: &kuro_torrent::Torrent) -> Vec<Episode> {
    let mut trackers = view.magnet.trackers.clone();
    for tracker in &torrent.trackers {
        if !trackers.contains(tracker) {
            trackers.push(tracker.clone());
        }
    }
    let name = if view.magnet.name.is_empty() { &torrent.name } else { &view.magnet.name };
    let mut list: Vec<Episode> = torrent
        .files
        .iter()
        .filter(|file| kuro_torrent::is_video(&file.path))
        .enumerate()
        .map(|(n, file)| Episode {
            url: kuro_torrent::magnet(&view.magnet.hash, name, &trackers, Some(file.index)),
            name: file.name().to_owned(),
            episode_number: (n + 1) as f32,
            scanlator: Some(kuro_torrent::readable_size(file.size)),
            date_upload: view.date,
            ..Default::default()
        })
        .collect();
    list.reverse();
    list
}

/// The episodes when the `.torrent` file cannot be had: a torrent of one file plays at index 0, so the page's list
/// is enough. A torrent of several files needs the file's order, which only the `.torrent` gives.
fn single_file_episode(view: &ViewPage) -> Option<Episode> {
    let [(name, size)] = view.listed.as_slice() else { return None };
    kuro_torrent::is_video(name).then(|| Episode {
        url: kuro_torrent::magnet(&view.magnet.hash, &view.magnet.name, &view.magnet.trackers, Some(0)),
        name: name.clone(),
        episode_number: 1.0,
        scanlator: Some(size.clone()).filter(|s| !s.is_empty()),
        date_upload: view.date,
        ..Default::default()
    })
}

fn episode_list(host: &dyn Host, url: &str) -> Res<Vec<Episode>> {
    let view = parse_view(&get(host, &absolute(url))?)?;
    let fetched = if view.torrent.is_empty() {
        Err("this torrent page has no .torrent file".to_owned())
    } else {
        Request::get(absolute(&view.torrent))
            .header("Referer", absolute(url))
            .send(host)
            .and_then(|response| kuro_torrent::body_bytes(response, NAME))
            .and_then(|bytes| kuro_torrent::parse(&bytes))
    };
    match fetched {
        Ok(torrent) => Ok(episodes(&view, &torrent)),
        Err(error) => match single_file_episode(&view) {
            Some(episode) => {
                host.log(2, &format!("nyaa: the .torrent file failed ({error}); one file, so index 0"));
                Ok(vec![episode])
            }
            None => Err(error),
        },
    }
}

/// The one video of an episode: its magnet link, labelled with the height its file name states.
fn video(episode: &Episode) -> Res<Video> {
    if kuro_torrent::read_magnet(&episode.url).is_none() {
        return Err("this episode is not a magnet link".into());
    }
    let height = kuro_torrent::height(&episode.name);
    Ok(Video {
        video_url: episode.url.clone(),
        video_title: height.map_or_else(|| "Auto".to_owned(), |h| format!("{h}p")),
        resolution: height,
        preferred: true,
        ..Default::default()
    })
}

struct Nyaa;

impl Source for Nyaa {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "all", supports_latest: true, request_gap_ms: Some(500) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => to_value(parse_list(&get(host, &popular_url(input.page()))?)),
            "latest" => to_value(parse_list(&get(host, &latest_url(input.page()))?)),
            "search" => match searched_id(input.query()) {
                Some(id) => {
                    let url = format!("/view/{id}");
                    let anime = parse_details(&get(host, &absolute(&url))?, &url);
                    to_value(AnimesPage { animes: vec![anime], has_next_page: false })
                }
                None => to_value(parse_list(&get(host, &search_url(input.query(), input.page()))?)),
            },
            "details" => {
                let url = input.url("anime")?;
                to_value(parse_details(&get(host, &absolute(url))?, url))
            }
            "episodes" => to_value(episode_list(host, input.url("anime")?)?),
            "videos" => to_value(vec![video(&input.require::<Episode>("episode")?)?]),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Nyaa);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;
    use serde_json::json;

    // Saved from nyaa.si on 2026-10-29. The .torrent is the batch of /view/2115493 with its piece hashes blanked.
    const POPULAR: &str = include_str!("../fixtures/popular.html");
    const SEARCH: &str = include_str!("../fixtures/search.html");
    const VIEW: &str = include_str!("../fixtures/view.html");
    const TORRENT: &[u8] = include_bytes!("../fixtures/batch.torrent");

    /// The site. TestHost answers text only, so this wraps it to send the .torrent as the app sends a binary body:
    /// base64, with `bodyEncoding`.
    struct Site(TestHost);

    impl Host for Site {
        fn http(&self, request_json: &str) -> Res<String> {
            let mut answer: Value = serde_json::from_str(&self.0.http(request_json)?).unwrap();
            if answer["url"].as_str().is_some_and(|u| u.ends_with(".torrent")) {
                use base64::Engine;
                answer["body"] = json!(base64::engine::general_purpose::STANDARD.encode(TORRENT));
                answer["bodyEncoding"] = json!("base64");
            }
            Ok(answer.to_string())
        }
        fn unix_seconds(&self) -> u64 {
            0
        }
        fn pref_get(&self, key: &str) -> String {
            self.0.pref_get(key)
        }
        fn pref_set(&self, key: &str, value_json: &str) -> Res<()> {
            self.0.pref_set(key, value_json)
        }
        fn log(&self, _level: u8, _message: &str) {}
    }

    fn site(torrent_status: u16) -> Site {
        Site(TestHost::answering(move |request| match request.url.as_str() {
            "https://nyaa.si/view/2115493" => (200, VIEW.into()),
            "https://nyaa.si/download/2115493.torrent" => (torrent_status, String::new()),
            url if url.contains("&q=") => (200, SEARCH.into()),
            _ => (200, POPULAR.into()),
        }))
    }

    fn run(host: &dyn Host, op: &str, args: Value) -> Res<Value> {
        kuro_guest::invoke(&Nyaa, host, op, &json!({ "args": args }).to_string()).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_kotlin_id() {
        let info = run(&TestHost::default(), "info", json!({})).unwrap();
        assert_eq!(info, json!({ "id": ID, "name": "Nyaa (Torrent)", "lang": "all", "supportsLatest": true, "requestGapMs": 500 }));
    }

    #[test]
    fn lists_read_the_torrent_table() {
        let host = TestHost::with_body(POPULAR);
        let page = run(&host, "popular", json!({ "page": 1 })).unwrap();
        assert_eq!(page["hasNextPage"], json!(true));
        let animes = page["animes"].as_array().unwrap();
        assert_eq!(animes.len(), 75);
        assert_eq!(animes[0]["url"], json!("/view/2176423"));
        assert!(animes[0]["title"].as_str().unwrap().starts_with("Cyberpunk Edgerunners S02"));
        assert_eq!(host.sent()[0].url, "https://nyaa.si/?f=0&c=1_0&s=seeders&o=desc&p=1");
        run(&host, "latest", json!({ "page": 2 })).unwrap();
        assert_eq!(host.sent()[1].url, "https://nyaa.si/?f=0&c=1_0&p=2");
    }

    #[test]
    fn search_sends_the_text_and_reads_ids() {
        let host = site(200);
        let page = run(&host, "search", json!({ "page": 1, "query": "frieren batch" })).unwrap();
        assert!(page["animes"].as_array().unwrap().iter().any(|a| a["url"] == json!("/view/2115493")));
        assert_eq!(host.0.sent()[0].url, "https://nyaa.si/?f=0&c=1_0&q=frieren+batch&p=1");
        let one = run(&host, "search", json!({ "query": "https://nyaa.si/view/2115493" })).unwrap();
        assert_eq!(one["animes"][0]["url"], json!("/view/2115493"));
        assert_eq!(searched_id("id:2115493"), Some("2115493"));
        assert_eq!(searched_id("id:abc"), None);
    }

    #[test]
    fn details_read_the_info_panel() {
        let anime = parse_details(VIEW, "/view/2115493");
        assert_eq!(anime.title, "[SubsPlease] Sousou no Frieren S2 (01-10) (1080p) [Batch]");
        assert_eq!(anime.genre.as_deref(), Some("Category: Anime - English-translated, Seeders: 194, Leechers: 4, File Size: 13.7 GiB"));
        assert_eq!(anime.author.as_deref(), Some("subsplease"));
        assert!(anime.description.unwrap().starts_with("**Visit our new [website]"));
        assert_eq!(first_image("x ![](https://i.example/a.PNG) y"), Some("https://i.example/a.PNG".into()));
    }

    #[test]
    fn episodes_are_the_video_files_with_their_index() {
        let host = site(200);
        let episodes = run(&host, "episodes", json!({ "anime": { "url": "/view/2115493" } })).unwrap();
        let episodes = episodes.as_array().unwrap();
        assert_eq!(episodes.len(), 10);
        let newest = &episodes[0];
        assert_eq!(newest["name"], json!("[SubsPlease] Sousou no Frieren S2 - 10 (1080p) [7D35515E].mkv"));
        assert_eq!(newest["episodeNumber"], json!(10.0));
        assert_eq!(newest["dateUpload"], json!(1780074337000_i64));
        assert!(newest["scanlator"].as_str().unwrap().ends_with(" GB"));
        let url = newest["url"].as_str().unwrap();
        assert!(url.starts_with("magnet:?xt=urn:btih:bc25231f657d115e61e68d0e124d3ff5ec101f69&dn="));
        assert!(url.ends_with("&index=9"));
        let magnet = kuro_torrent::read_magnet(url).unwrap();
        assert_eq!(magnet.trackers.len(), 13);
        assert_eq!(episodes[9]["url"].as_str().unwrap().rsplit('=').next(), Some("0"));
        assert_eq!(host.0.sent()[1].url, "https://nyaa.si/download/2115493.torrent");

        let video = run(&host, "videos", json!({ "episode": newest })).unwrap();
        assert_eq!(video[0]["videoUrl"], newest["url"]);
        assert_eq!((video[0]["videoTitle"].clone(), video[0]["resolution"].clone()), (json!("1080p"), json!(1080)));
        let auto = run(&host, "videos", json!({ "episode": { "url": url, "name": "ep.mkv" } })).unwrap();
        assert_eq!(auto[0]["videoTitle"], json!("Auto"));
        assert!(run(&host, "videos", json!({ "episode": { "url": "/view/1" } })).is_err());
    }

    #[test]
    fn without_the_torrent_file_a_batch_fails_and_a_single_file_plays_at_0() {
        assert_eq!(run(&site(503), "episodes", json!({ "anime": { "url": "/view/2115493" } })), Err("HTTP 503 from Nyaa (Torrent)".into()));
        let mut view = parse_view(VIEW).unwrap();
        assert_eq!(view.listed.len(), 10);
        assert_eq!(view.listed[0].1, "1.4 GiB");
        view.listed.truncate(1);
        let episode = single_file_episode(&view).unwrap();
        assert!(episode.url.ends_with("&index=0"));
        assert_eq!(episode.name, "[SubsPlease] Sousou no Frieren S2 - 01v2 (1080p) [4149A222].mkv");
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(run(&TestHost::default(), "hosters", json!({})), Err("unknown op: hosters".to_owned()));
    }
}
