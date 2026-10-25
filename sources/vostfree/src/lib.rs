//! Vostfree (<https://vostfree.ws>), an anime source plug-in for Kurobayomi: anime in French, subbed (VOSTFR) and
//! dubbed (VF). Ported from the yuzono `fr/vostfree` extension (Apache-2.0). This file is the source's ops and parsers;
//! the video hosts are read by the shared `lib/extractors`, the glue to the app is the `kuro_guest` toolkit.
//!
//! A title's page holds every episode and, per episode, its servers: a button each, whose video id (or embed address)
//! sits in a hidden `div#content_<button id>`. Left out: the search filters (genre, type), the preferred-quality
//! setting, and the servers no extractor reads (Sibnet, Vudeo, VOE, Myvi, MyTV and others).

use kuro_guest::html::{Html, first, own_text, sel, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, Hoster, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The site sends every visitor of vostfree.ws to this host.
const BASE: &str = "https://ipv4.vostfree.ws";
const NAME: &str = "Vostfree";
/// The source id: the MD5 of `vostfree/fr/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 8596049504378147571;

struct Vostfree;

impl Source for Vostfree {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "fr", supports_latest: false, request_gap_ms: Some(500) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let get = |url: &str| Request::get(url).header("Referer", format!("{BASE}/")).text(host, NAME);
        let page = input.page().max(1);
        match op {
            // the Kotlin's popular list is the films
            "popular" => to_value(parse_list(&get(&format!("{BASE}/films-vf-vostfr/page/{page}/"))?)),
            "search" if input.query().trim().is_empty() => to_value(parse_list(&get(&format!("{BASE}/animes-vostfr/page/{page}/"))?)),
            "search" => {
                let story: String = url::form_urlencoded::byte_serialize(input.query().trim().as_bytes()).collect();
                let body = format!("do=search&subaction=search&search_start=0&full_search=0&result_from=1&story={story}");
                let answer = Request::form(format!("{BASE}/index.php?do=search"), body).header("Referer", format!("{BASE}/")).text(host, NAME)?;
                // the search answers one page
                to_value(AnimesPage { has_next_page: false, ..parse_list(&answer) })
            }
            "details" => {
                let url = input.url("anime")?;
                let mut anime = parse_details(&get(&absolute(url)?)?)?;
                anime.url = url.to_owned();
                to_value(anime)
            }
            "episodes" => {
                let url = input.url("anime")?;
                to_value(parse_episodes(&get(&absolute(url)?)?, url))
            }
            "hosters" => {
                let (page, number) = episode_page(input.url("episode")?)?;
                to_value(parse_hosters(&get(&page)?, number))
            }
            "videos" => match input.get::<Hoster>("hoster")? {
                Some(hoster) => to_value(hoster_videos(host, &hoster, true)?),
                None => {
                    let (page, number) = episode_page(input.url("episode")?)?;
                    let hosters = parse_hosters(&get(&page)?, number);
                    let mut all = Vec::new();
                    for hoster in &hosters {
                        // one failing server does not hide the others
                        match hoster_videos(host, hoster, all.is_empty()) {
                            Ok(videos) => all.extend(videos),
                            Err(error) => host.log(2, &format!("vostfree: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Vostfree);

/// A stored path (`/275-x.html`) as a full address, refused when it points elsewhere.
fn absolute(path: &str) -> Res<String> {
    if path.starts_with('/') && !path.starts_with("//") { Ok(format!("{BASE}{path}")) } else { Err(format!("not a {NAME} address: {path}")) }
}

/// The path of a link on the site, on either of its hosts.
fn path_of(href: &str) -> String {
    href.strip_prefix(BASE).or_else(|| href.strip_prefix("https://vostfree.ws")).unwrap_or(href).to_owned()
}

/// An episode's stored address (`/275-x.html?episode=3`) as its title page and its number.
fn episode_page(url: &str) -> Res<(String, u32)> {
    let (path, number) = url.split_once("?episode=").ok_or_else(|| format!("not a {NAME} episode: {url}"))?;
    Ok((absolute(path)?, number.parse().map_err(|_| format!("not a {NAME} episode: {url}"))?))
}

/// The posters of a list (`div.movie-poster`) or a search (`div.search-result`): the first link is the title's.
fn parse_list(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let animes = doc
        .select(&sel("div#dle-content div.search-result, div#dle-content div.movie-poster"))
        .filter_map(|card| {
            let link = first(card, "a[href]")?;
            Some(Anime {
                url: path_of(link.value().attr("href")?),
                title: Some(text_of(link)).filter(|t| !t.is_empty())?,
                thumbnail_url: first(card, "span.image img").and_then(|img| img.value().attr("src")).map(str::to_owned),
                ..Default::default()
            })
        })
        .collect();
    AnimesPage { animes, has_next_page: doc.select(&sel("a span.next-page")).next().is_some() }
}

fn parse_details(html: &str) -> Res<Anime> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let title = first(root, "div.slide-middle h1").map(text_of).filter(|t| !t.is_empty()).ok_or("the title page has no title")?;
    let genres: Vec<String> = doc.select(&sel("li.right b.fa-bookmark-o ~ a")).map(text_of).collect();
    Ok(Anime {
        title,
        description: first(root, "div.slide-desc").map(own_text).filter(|d| !d.is_empty()),
        thumbnail_url: first(root, "div.slide-poster img").and_then(|img| img.value().attr("src")).map(str::to_owned),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        ..Default::default()
    })
}

/// The episodes of a title page, newest first: one per option of its episode selector ("Episode 03", or "Film"),
/// whose value names the episode's server buttons (`buttons_3`).
fn parse_episodes(html: &str, anime_url: &str) -> Vec<Episode> {
    let doc = Html::parse_document(html);
    let mut episodes: Vec<Episode> = doc
        .select(&sel("select.new_player_selector option"))
        .filter_map(|option| {
            let number: u32 = option.value().attr("value")?.strip_prefix("buttons_")?.parse().ok()?;
            let text = text_of(option);
            Some(Episode {
                url: format!("{anime_url}?episode={number}"),
                name: if text == "Film" { text } else { format!("Épisode {number}") },
                episode_number: number as f32,
                ..Default::default()
            })
        })
        .collect();
    episodes.reverse();
    episodes
}

/// The servers of one episode that the extractors can play, in the site's order. Uqload and Okru buttons hold a video
/// id, the others an embed address.
fn parse_hosters(html: &str, number: u32) -> Vec<Hoster> {
    let doc = Html::parse_document(html);
    doc.select(&sel(&format!("div#buttons_{number} > div[id]")))
        .filter_map(|button| {
            let id = button.value().attr("id")?;
            let content = doc.select(&sel(&format!("div#content_{id}"))).next().map(text_of)?;
            let name = text_of(button);
            let embed = match name.to_lowercase().as_str() {
                "uqload" => format!("https://uqload.io/embed-{content}.html"),
                "ok" => format!("https://ok.ru/videoembed/{content}"),
                _ => content,
            };
            Some(Hoster { hoster_url: playable(&embed)?, hoster_name: name, ..Default::default() })
        })
        .collect()
}

/// The embed address an extractor reads, `None` when none does. DoodStream moves between domains that all serve the
/// same ids, so any Dood domain is asked at playmogo.com.
fn playable(embed: &str) -> Option<String> {
    let rest = embed.split_once("://")?.1;
    let end = rest.find(['/', '?', '#', ':']).unwrap_or(rest.len());
    let host = &rest[..end];
    // "dood", "dooodster", "d000d", ...
    let dood = host.match_indices('d').any(|(i, _)| {
        let rest = &host[i + 1..];
        let os = rest.len() - rest.trim_start_matches(['o', '0']).len();
        os >= 2 && rest[os..].starts_with('d')
    });
    if dood || ["ds2play.com", "ds2video.com", "dsvplay.com"].contains(&host) {
        return Some(format!("https://playmogo.com{}", &rest[end..]));
    }
    kuro_extractors::HOSTS.contains(&host).then(|| embed.to_owned())
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
    const FILM: &str = include_str!("../fixtures/film.html");
    const MANIFEST: &str = include_str!("../manifest.toml");
    const BLACK_CLOVER: &str = "/275-black-clover-vostfr-ddl-streaming-1fichier-uptobox.html";

    #[test]
    fn a_list_has_posters_and_more_pages() {
        let page = parse_list(POPULAR);
        assert_eq!(page.animes.len(), 15);
        let first = &page.animes[0];
        assert_eq!(first.url, "/1404-helck-ddl-streaming-1fichier-uptobox.html");
        assert!(first.title.starts_with("Helck"));
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://vostfree.ws/uploads/"));
        assert!(page.has_next_page);
    }

    #[test]
    fn a_search_lists_its_results() {
        let page = parse_list(SEARCH);
        assert_eq!(page.animes.len(), 7);
        assert_eq!((page.animes[0].url.as_str(), page.animes[0].title.as_str()), ("/1315-uniteup-vostfr-ddl-streaming-1fichier-uptobox.html", "UniteUp! VOSTFR"));
        assert!(page.animes.iter().all(|a| a.thumbnail_url.is_some()));
    }

    #[test]
    fn the_title_page_has_details_and_episodes_newest_first() {
        let anime = parse_details(ANIME).unwrap();
        assert_eq!(anime.title, "Black Clover VOSTFR");
        assert!(anime.description.unwrap().starts_with("Il s'agit d'une série animée"));
        assert!(anime.genre.unwrap().contains("Shonen"));
        let episodes = parse_episodes(ANIME, BLACK_CLOVER);
        assert_eq!(episodes.len(), 170);
        assert_eq!((episodes[169].url.as_str(), episodes[169].name.as_str()), (&*format!("{BLACK_CLOVER}?episode=1"), "Épisode 1"));
        assert_eq!(episodes[0].episode_number, 170.0);
        let film = parse_episodes(FILM, "/794-x.html");
        assert_eq!((film.len(), film[0].name.as_str(), film[0].url.as_str()), (1, "Film", "/794-x.html?episode=1"));
        assert_eq!(episode_page(&format!("{BLACK_CLOVER}?episode=150")).unwrap(), (format!("{BASE}{BLACK_CLOVER}"), 150));
        assert!(episode_page("/x.html").is_err());
    }

    #[test]
    fn an_episode_offers_only_the_servers_that_play() {
        let hosters = parse_hosters(ANIME, 150);
        assert_eq!(hosters.len(), 1);
        assert_eq!((hosters[0].hoster_name.as_str(), hosters[0].hoster_url.as_str()), ("Uqload", "https://uqload.io/embed-c5o96tyvsy6s.html"));
        let film = parse_hosters(FILM, 1);
        assert_eq!(film.iter().map(|h| h.hoster_url.as_str()).collect::<Vec<_>>(), ["https://uqload.io/embed-25v0w3yzigbu.html"]);
        assert_eq!(playable("https://dood.so/e/a").as_deref(), Some("https://playmogo.com/e/a"));
        assert_eq!(playable("https://mixdrop.sx/e/n0").as_deref(), Some("https://mixdrop.sx/e/n0"));
        assert_eq!(playable("https://vudeo.co/x"), None);
    }

    #[test]
    fn the_manifest_lists_the_site_and_every_extractor_host() {
        assert!(MANIFEST.contains("\"ipv4.vostfree.ws\""));
        for host in kuro_extractors::HOSTS {
            assert!(MANIFEST.contains(&format!("\"{host}\"")), "manifest.toml misses {host}");
        }
    }

    #[test]
    fn ops_run_through_the_toolkit() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            url if url.contains("/animes-vostfr/") => (200, POPULAR.into()),
            url if url.contains("do=search") && request.method == "POST" => (200, SEARCH.into()),
            url if url.contains("/275-") => (200, ANIME.into()),
            _ => (404, String::new()),
        });
        let call = |op: &str, args: Value| kuro_guest::invoke(&Vostfree, &host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str::<Value>(&o).unwrap());
        assert_eq!(call("info", json!({})).unwrap()["id"].as_i64(), Some(ID));
        assert_eq!(call("search", json!({ "page": 1, "query": "" })).unwrap()["animes"].as_array().unwrap().len(), 15);
        let search = call("search", json!({ "page": 1, "query": "clover" })).unwrap();
        assert_eq!((search["animes"].as_array().unwrap().len(), search["hasNextPage"].as_bool()), (7, Some(false)));
        assert_eq!(call("episodes", json!({ "anime": { "url": BLACK_CLOVER } })).unwrap().as_array().unwrap().len(), 170);
        let hosters = call("hosters", json!({ "episode": { "url": format!("{BLACK_CLOVER}?episode=150") } })).unwrap();
        assert_eq!(hosters.as_array().unwrap().len(), 1);
        assert_eq!(call("latest", json!({ "page": 1 })), Err("unknown op: latest".into()));
    }
}
