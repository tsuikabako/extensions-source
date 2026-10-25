//! Latanime (<https://latanime.org>), an anime source plug-in for Kurobayomi: anime in Spanish, dubbed (Latino,
//! Castellano) and subbed. Ported from the yuzono `es/latanime` extension (Apache-2.0). This file is the source's ops
//! and parsers; the video hosts are read by the shared `lib/extractors`, the glue to the app is the `kuro_guest`
//! toolkit.
//!
//! Left out: the search filters (year, genre, letter), and every video host the extractors do not cover (VOE,
//! Filemoon, StreamWish, MEGA and others): an episode offers only the servers that can play.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use kuro_guest::html::{ElementRef, Html, attr, first, own_text, sel, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, Hoster, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

const BASE: &str = "https://latanime.org";
const NAME: &str = "Latanime";
/// The source id: the MD5 of `latanime/es/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 7257892387451890645;

struct Latanime;

impl Source for Latanime {
    fn info(&self) -> Info {
        // the Kotlin has no latest list; one request a second keeps well under any limit
        Info { id: ID, name: NAME, lang: "es", supports_latest: false, request_gap_ms: Some(1000) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let get = |url: &str| Request::get(url).header("Referer", format!("{BASE}/")).text(host, NAME);
        match op {
            "popular" => to_value(parse_list(&get(&format!("{BASE}/emision?p={}", input.page().max(1)))?)),
            // without text the Kotlin lists the airing titles too (its filters are left out)
            "search" if input.query().trim().is_empty() => to_value(parse_list(&get(&format!("{BASE}/emision?p={}", input.page().max(1)))?)),
            "search" => to_value(parse_list(&get(&search_url(input.query(), input.page()))?)),
            "details" => {
                let url = input.url("anime")?;
                let mut anime = parse_details(&get(&absolute(url)?)?)?;
                anime.url = url.to_owned();
                to_value(anime)
            }
            "episodes" => to_value(parse_episodes(&get(&absolute(input.url("anime")?)?)?)),
            "hosters" => to_value(parse_hosters(&get(&absolute(input.url("episode")?)?)?)),
            "videos" => match input.get::<Hoster>("hoster")? {
                Some(hoster) => to_value(hoster_videos(host, &hoster, true)?),
                None => {
                    let hosters = parse_hosters(&get(&absolute(input.url("episode")?)?)?);
                    let mut all = Vec::new();
                    for hoster in &hosters {
                        // one failing server does not hide the others
                        match hoster_videos(host, hoster, all.is_empty()) {
                            Ok(videos) => all.extend(videos),
                            Err(error) => host.log(2, &format!("latanime: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Latanime);

fn search_url(query: &str, page: i32) -> String {
    let q: String = query.trim().split_whitespace().collect::<Vec<_>>().join("+");
    format!("{BASE}/buscar?q={q}&p={}", page.max(1))
}

/// A stored path (`/anime/x`, `/ver/x`) as a full address, refused when it points elsewhere.
fn absolute(path: &str) -> Res<String> {
    if path.starts_with('/') && !path.starts_with("//") { Ok(format!("{BASE}{path}")) } else { Err(format!("not a {NAME} address: {path}")) }
}

/// The path of a link on the site (`https://latanime.org/anime/x` → `/anime/x`).
fn path_of(href: &str) -> String {
    href.strip_prefix(BASE).unwrap_or(href).to_owned()
}

/// The cards of the airing list or a search: `div.row > div` with a link, a cover and `div.seriedetails > h3`.
fn parse_list(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let animes = doc
        .select(&sel("div.row > div"))
        .filter_map(|card| {
            let href = attr(card, "a", "href")?;
            Some(Anime {
                url: path_of(&href),
                title: first(card, "div.seriedetails > h3").map(text_of).filter(|t| !t.is_empty())?,
                thumbnail_url: attr(card, "img", "src"),
                ..Default::default()
            })
        })
        .collect();
    AnimesPage { animes, has_next_page: has_next(&doc) }
}

/// Whether the pager has a page link after the active page (the Kotlin's `li.active ~ li:has(a)`).
fn has_next(doc: &Html) -> bool {
    doc.select(&sel("ul.pagination > li.active"))
        .next()
        .is_some_and(|active| active.next_siblings().filter_map(ElementRef::wrap).any(|li| li.select(&sel("a")).next().is_some()))
}

fn parse_details(html: &str) -> Res<Anime> {
    let doc = Html::parse_document(html);
    let title = doc.select(&sel("div.row > div > h2")).next().map(text_of).filter(|t| !t.is_empty()).ok_or("the title page has no title")?;
    let genres: Vec<String> = doc.select(&sel("div.row > div > a div.btn")).map(text_of).collect();
    Ok(Anime {
        title,
        description: doc.select(&sel("div.row > div > p.my-2")).next().map(text_of).filter(|d| !d.is_empty()),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        ..Default::default()
    })
}

/// The episodes of a title page, newest first (the site lists the first episode first).
fn parse_episodes(html: &str) -> Vec<Episode> {
    let doc = Html::parse_document(html);
    let mut episodes: Vec<Episode> = doc
        .select(&sel("div.row > div > a"))
        .filter_map(|link| {
            let href = link.value().attr("href").filter(|h| h.contains("/ver/"))?;
            let text = text_of(link);
            Some(Episode {
                url: path_of(href),
                // "Show - Capitulo 3" → "Show Capitulo 3", number 3, as the Kotlin does
                episode_number: text.rsplit("Capitulo ").next().filter(|_| text.contains("Capitulo ")).and_then(|n| n.trim().parse().ok()).unwrap_or(0.0),
                name: text.replace("- ", ""),
                ..Default::default()
            })
        })
        .collect();
    episodes.reverse();
    episodes
}

/// The servers of an episode page that the extractors can play, in the site's order. Each player link carries its
/// embed address in base64 (`data-player`) and the server's name as its text.
fn parse_hosters(html: &str) -> Vec<Hoster> {
    Html::parse_document(html)
        .select(&sel("li#play-video > a.play-video"))
        .filter_map(|link| {
            let embed = STANDARD.decode(link.value().attr("data-player")?.trim()).ok().and_then(|b| String::from_utf8(b).ok())?;
            supported(&embed).then(|| Hoster { hoster_url: embed, hoster_name: own_text(link), ..Default::default() })
        })
        .collect()
}

/// Whether an extractor reads this embed's host.
fn supported(embed: &str) -> bool {
    embed.split_once("://").and_then(|(_, rest)| rest.split(['/', '?', ':']).next()).is_some_and(|host| kuro_extractors::HOSTS.contains(&host))
}

/// The videos of one server; the first video is the preferred one when `preferred`.
fn hoster_videos(host: &dyn Host, hoster: &Hoster, preferred: bool) -> Res<Vec<Video>> {
    let mut videos = kuro_extractors::videos(host, &hoster.hoster_url, "").ok_or_else(|| format!("{} is not supported", hoster.hoster_name))??;
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

    const POPULAR: &str = include_str!("../fixtures/popular.html");
    const SEARCH: &str = include_str!("../fixtures/search.html");
    const ANIME: &str = include_str!("../fixtures/anime.html");
    const EPISODE: &str = include_str!("../fixtures/episode.html");
    const MANIFEST: &str = include_str!("../manifest.toml");

    #[test]
    fn the_airing_list_has_cards_and_more_pages() {
        let page = parse_list(POPULAR);
        assert_eq!(page.animes.len(), 30);
        let first = &page.animes[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/anime/helluva-boss-temporada-3", "Helluva Boss S3 Latino AMZ"));
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://latanime.org/thumbs/"));
        assert!(page.has_next_page);
    }

    #[test]
    fn a_search_lists_its_results_on_one_page() {
        let page = parse_list(SEARCH);
        assert_eq!(page.animes.len(), 5);
        assert!(page.animes.iter().all(|a| a.url.starts_with("/anime/") && a.title.to_lowercase().contains("helluva")));
        assert!(!page.has_next_page);
        assert_eq!(search_url("  helluva  boss ", 0), "https://latanime.org/buscar?q=helluva+boss&p=1");
    }

    #[test]
    fn the_title_page_has_details_and_episodes_newest_first() {
        let anime = parse_details(ANIME).unwrap();
        assert_eq!((anime.title.as_str(), anime.genre.as_deref()), ("Helluva Boss S3 Latino AMZ", Some("Comedia, Drama")));
        assert!(anime.description.unwrap().starts_with("En el infierno"));
        let episodes = parse_episodes(ANIME);
        assert_eq!(episodes.len(), 5);
        assert_eq!(episodes[4].url, "/ver/helluva-boss-temporada-3-episodio-1");
        assert_eq!((episodes[4].name.as_str(), episodes[4].episode_number), ("Helluva Boss S3 Latino AMZ Capitulo 1", 1.0));
        assert_eq!(episodes[0].episode_number, 5.0);
    }

    #[test]
    fn an_episode_offers_only_the_servers_that_play() {
        let hosters = parse_hosters(EPISODE);
        let hosts: Vec<&str> = hosters.iter().map(|h| h.hoster_url.split('/').nth(2).unwrap()).collect();
        assert_eq!(hosts, ["playmogo.com", "hexload.com", "mixdrop.top", "www.mp4upload.com"]);
        assert_eq!(hosters[0].hoster_name, "dsvplay");
        assert!(!supported("https://voe.sx/e/x") && !supported("https://mega.nz/embed/x"));
    }

    #[test]
    fn the_manifest_lists_the_site_and_every_extractor_host() {
        assert!(MANIFEST.contains("\"latanime.org\""));
        for host in kuro_extractors::HOSTS {
            assert!(MANIFEST.contains(&format!("\"{host}\"")), "manifest.toml misses {host}");
        }
    }

    #[test]
    fn ops_run_through_the_toolkit() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            url if url.contains("/emision") => (200, POPULAR.into()),
            url if url.contains("/ver/") => (200, EPISODE.into()),
            _ => (404, String::new()),
        });
        let call = |op: &str, args: Value| kuro_guest::invoke(&Latanime, &host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str::<Value>(&o).unwrap());
        assert_eq!(call("info", json!({})).unwrap()["id"].as_i64(), Some(ID));
        assert_eq!(call("popular", json!({ "page": 1 })).unwrap()["animes"].as_array().unwrap().len(), 30);
        let hosters = call("hosters", json!({ "episode": { "url": "/ver/helluva-boss-temporada-3-episodio-1" } })).unwrap();
        assert_eq!(hosters.as_array().unwrap().len(), 4);
        assert_eq!(call("latest", json!({ "page": 1 })), Err("unknown op: latest".into()));
        assert!(call("details", json!({ "anime": { "url": "https://evil.example/x" } })).is_err());
    }
}
