//! Okanime (<https://ww3.okanime.xyz>), an anime source plug-in for Kurobayomi: anime with Arabic subtitles. Ported
//! from the yuzono `ar/okanime` extension (Apache-2.0). This file is the source's ops and parsers; the video hosts are
//! read by the shared `lib/extractors`, the glue to the app is the `kuro_guest` toolkit.
//!
//! The site was redesigned since the Kotlin: the popular list is `/top-anime`, the latest `/recently-uploaded-episodes`,
//! the search `/search?q=`. An episode lists its servers as `a.ep-link`, each with its embed address in `@click` and
//! its quality (SD, HD, FHD) in a `span`. Left out: the servers no extractor reads (VOE, Vidbom, VK, MEGA, share4max, Sendvid),
//! the preferences (preferred quality, enabled hosts), and the `id:` search.

use kuro_guest::html::{ElementRef, Html, first, own_text, sel, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, Hoster, Video, status};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

const BASE: &str = "https://ww3.okanime.xyz";
const NAME: &str = "Okanime";
/// The source id: the MD5 of `okanime/ar/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 2275740574829569178;

struct Okanime;

impl Source for Okanime {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "ar", supports_latest: true, request_gap_ms: Some(500) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let get = |url: &str| Request::get(url).header("Referer", format!("{BASE}/")).text(host, NAME);
        let page = input.page().max(1);
        match op {
            "popular" => to_value(parse_list(&get(&format!("{BASE}/top-anime?page={page}"))?)),
            "latest" => to_value(parse_list(&get(&format!("{BASE}/recently-uploaded-episodes?page={page}"))?)),
            "search" if input.query().trim().is_empty() => to_value(parse_list(&get(&format!("{BASE}/top-anime?page={page}"))?)),
            "search" => {
                let q: String = url::form_urlencoded::byte_serialize(input.query().trim().as_bytes()).collect();
                to_value(parse_list(&get(&format!("{BASE}/search?q={q}&page={page}"))?))
            }
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
                            Err(error) => host.log(2, &format!("okanime: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Okanime);

/// A stored path (`/anime/x`, `/episode/x`) as a full address, refused when it points elsewhere.
fn absolute(path: &str) -> Res<String> {
    if path.starts_with('/') && !path.starts_with("//") { Ok(format!("{BASE}{path}")) } else { Err(format!("not a {NAME} address: {path}")) }
}

/// The path of a link on the site (`https://ww3.okanime.xyz/anime/x` → `/anime/x`).
fn path_of(href: &str) -> String {
    href.strip_prefix(BASE).unwrap_or(href).to_owned()
}

/// The cards of a list or a search: `div.anime-card` with a cover and the title's link in `div.anime-title > h4`. The
/// latest list has episode cards, which link their title the same way.
fn parse_list(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let animes = doc
        .select(&sel("div.anime-card"))
        .filter_map(|card| {
            let link = first(card, "div.anime-title > h4 > a")?;
            Some(Anime {
                url: path_of(link.value().attr("href")?),
                title: Some(text_of(link)).filter(|t| !t.is_empty())?,
                thumbnail_url: first(card, "img").and_then(|img| img.value().attr("src")).map(str::to_owned),
                ..Default::default()
            })
        })
        .collect();
    let has_next_page = doc.select(&sel("ul.pagination > li:last-child:not(.disabled)")).next().is_some();
    AnimesPage { animes, has_next_page }
}

fn parse_details(html: &str) -> Res<Anime> {
    let doc = Html::parse_document(html);
    let title = doc.select(&sel("h1.animepage-h1")).next().map(text_of).filter(|t| !t.is_empty()).ok_or("the title page has no title")?;
    let genres: Vec<String> = doc.select(&sel("div.animepage-genres a.genre-tag")).map(text_of).collect();
    // the state is a link to `/state/currently-airing` or `/state/finished-airing`
    let state = doc.select(&sel("dl.animepage-meta a[href*='/state/']")).next().and_then(|a| a.value().attr("href")).unwrap_or("");
    Ok(Anime {
        title,
        thumbnail_url: doc.select(&sel("img.animepage-poster")).next().and_then(|img| img.value().attr("src")).map(str::to_owned),
        description: doc.select(&sel("div.animepage-synopsis div.synopsis-text")).next().map(text_of).filter(|d| !d.is_empty()),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status: if state.ends_with("/currently-airing") {
            status::ONGOING
        } else if state.ends_with("/finished-airing") {
            status::COMPLETED
        } else {
            status::UNKNOWN
        },
        ..Default::default()
    })
}

/// The episodes of a title page, newest first (the site lists the first episode first; all on one page).
fn parse_episodes(html: &str) -> Vec<Episode> {
    let mut episodes: Vec<Episode> = Html::parse_document(html).select(&sel("a.ep-compact-btn")).filter_map(episode_of).collect();
    episodes.reverse();
    episodes
}

/// One episode button: its title ("الحلقة 3") and number.
fn episode_of(link: ElementRef) -> Option<Episode> {
    let number: f32 = link.value().attr("data-umami-event-episode_number").and_then(|n| n.trim().parse().ok()).unwrap_or(0.0);
    let name = link.value().attr("title").map(str::trim).filter(|t| !t.is_empty()).map_or_else(|| text_of(link), str::to_owned);
    Some(Episode { url: path_of(link.value().attr("href")?), name, episode_number: number, ..Default::default() })
}

/// The servers of an episode page that an extractor reads, in the site's order, named with their quality
/// ("Mp4upload (HD)"). Each `a.ep-link` holds its embed in `@click="setServer('<embed>')"`.
fn parse_hosters(html: &str) -> Vec<Hoster> {
    Html::parse_document(html)
        .select(&sel("a.ep-link"))
        .filter_map(|link| {
            let embed = uqload_embed(link.value().attr("@click")?.split_once("setServer('")?.1.split_once('\'')?.0);
            let quality = first(link, "span").map(text_of).unwrap_or_default();
            let server = own_text(link);
            supported(&embed).then(|| Hoster {
                hoster_url: embed,
                hoster_name: if quality.is_empty() { server } else { format!("{server} ({quality})") },
                ..Default::default()
            })
        })
        .collect()
}

/// Uqload's `/e/<id>?...` link is a click-to-play page; the player is on `/embed-<id>.html`, which the extractor
/// reads. Other links are kept.
fn uqload_embed(url: &str) -> String {
    let Some((host, id)) = url.strip_prefix("https://").and_then(|rest| rest.split_once("/e/")).filter(|(host, _)| host.starts_with("uqload.")) else {
        return url.to_owned();
    };
    let id = id.split(['?', '/', '#']).next().unwrap_or(id);
    format!("https://{host}/embed-{id}.html")
}

/// Whether an extractor reads this embed's host.
fn supported(embed: &str) -> bool {
    embed.split_once("://").and_then(|(_, rest)| rest.split(['/', '?', ':']).next()).is_some_and(|host| kuro_extractors::HOSTS.contains(&host))
}

/// The videos of one server; the first video is the preferred one when `preferred`. The quality is in the server's
/// name (the Server menu), so the title is only the height or "Auto".
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
    const LATEST: &str = include_str!("../fixtures/latest.html");
    const SEARCH: &str = include_str!("../fixtures/search.html");
    const ANIME: &str = include_str!("../fixtures/anime.html");
    const EPISODE: &str = include_str!("../fixtures/episode.html");
    const MANIFEST: &str = include_str!("../manifest.toml");

    #[test]
    fn the_popular_list_has_cards_and_more_pages() {
        let page = parse_list(POPULAR);
        assert_eq!(page.animes.len(), 30);
        let first = &page.animes[0];
        assert_eq!(first.url, "/anime/rezero-kara-hajimeru-isekai-seikatsu-4th-season");
        assert!(first.title.starts_with("Re:Zero"), "{}", first.title);
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://ww3.okanime.xyz/media/posters/"));
        assert!(page.has_next_page);
    }

    #[test]
    fn the_latest_list_links_the_titles_of_its_episodes() {
        let page = parse_list(LATEST);
        assert_eq!(page.animes.len(), 30);
        assert_eq!((page.animes[0].url.as_str(), page.animes[0].title.as_str()), ("/anime/dragon-ball-super-beerus", "Dragon Ball Super: Beerus"));
        assert!(page.has_next_page);
    }

    #[test]
    fn a_search_lists_its_results_on_one_page() {
        let page = parse_list(SEARCH);
        assert_eq!(page.animes.len(), 5);
        assert!(page.animes.iter().all(|a| a.url.starts_with("/anime/") && a.title.to_lowercase().contains("naruto")));
        assert!(!page.has_next_page);
    }

    #[test]
    fn the_title_page_has_details_and_episodes_newest_first() {
        let anime = parse_details(ANIME).unwrap();
        assert_eq!(anime.title, "Fullmetal Alchemist: Brotherhood");
        assert_eq!(anime.genre.as_deref(), Some("أكشن, مغامرة, دراما, فانتازيا"));
        assert_eq!(anime.status, status::COMPLETED);
        assert!(anime.description.unwrap().starts_with("قصة انمي Fullmetal"));
        assert!(anime.thumbnail_url.unwrap().starts_with("https://ww3.okanime.xyz/media/posters/fullmetal-alchemist-brotherhood-"));
        let episodes = parse_episodes(ANIME);
        assert_eq!(episodes.len(), 64);
        assert_eq!(episodes[63].url, "/episode/fullmetal-alchemist-brotherhood-episode-1");
        assert_eq!((episodes[63].name.as_str(), episodes[63].episode_number), ("الحلقة 1", 1.0));
        assert_eq!(episodes[0].episode_number, 64.0);
    }

    #[test]
    fn an_episode_offers_only_the_servers_that_play() {
        let hosters = parse_hosters(EPISODE);
        let urls: Vec<&str> = hosters.iter().map(|h| h.hoster_url.as_str()).collect();
        assert_eq!(urls, ["https://mp4upload.com/embed-0cbwkrh9diar.html", "https://uqload.is/embed-o8g0kr56zuv0.html"]);
        assert!(hosters[0].hoster_name.starts_with("Mp4upload ("), "{}", hosters[0].hoster_name);
        assert!(!supported("https://mega.nz/embed/x") && !supported("https://vkvideo.ru/video_ext.php?oid=1"));
        assert_eq!(uqload_embed("https://uqload.vc/e/m35orgufe2ae?e19.uqload.vc/i/02/06071/m35orgufe2ae.html"), "https://uqload.vc/embed-m35orgufe2ae.html");
        assert_eq!(uqload_embed("https://mp4upload.com/embed-x.html"), "https://mp4upload.com/embed-x.html");
    }

    #[test]
    fn the_manifest_lists_the_site_and_every_extractor_host() {
        assert!(MANIFEST.contains("\"ww3.okanime.xyz\""));
        for host in kuro_extractors::HOSTS {
            assert!(MANIFEST.contains(&format!("\"{host}\"")), "manifest.toml misses {host}");
        }
    }

    #[test]
    fn ops_run_through_the_toolkit() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            url if url.contains("/top-anime") => (200, POPULAR.into()),
            url if url.contains("/search?q=naruto&page=1") => (200, SEARCH.into()),
            url if url.contains("/episode/") => (200, EPISODE.into()),
            _ => (404, String::new()),
        });
        let call = |op: &str, args: Value| kuro_guest::invoke(&Okanime, &host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str::<Value>(&o).unwrap());
        assert_eq!(call("info", json!({})).unwrap()["id"].as_i64(), Some(ID));
        assert_eq!(call("popular", json!({ "page": 1 })).unwrap()["animes"].as_array().unwrap().len(), 30);
        assert_eq!(call("search", json!({ "page": 1, "query": " naruto " })).unwrap()["animes"].as_array().unwrap().len(), 5);
        let hosters = call("hosters", json!({ "episode": { "url": "/episode/fullmetal-alchemist-brotherhood-episode-1" } })).unwrap();
        assert_eq!(hosters.as_array().unwrap().len(), 2);
        assert!(call("details", json!({ "anime": { "url": "https://evil.example/x" } })).is_err());
    }
}
