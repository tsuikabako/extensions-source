//! MonosChinos (<https://monoschinos.st>), an anime source plug-in for Kurobayomi: anime subbed in Spanish. Ported
//! from the yuzono `es/monoschinos` extension (Apache-2.0), adapted to the site's new layout (October 2026). This file
//! is the source's ops and parsers; the video hosts are read by the shared `lib/extractors`, the glue to the app is the
//! `kuro_guest` toolkit.
//!
//! Left out: the search filters, the preferences (preferred server and quality), and every video host the extractors
//! do not cover (VOE, Filemoon, StreamWish, LuluStream, MEGA, savefiles and others): an episode offers only the
//! servers that can play.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use kuro_guest::html::{ElementRef, Html, attr, first, sel, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, Hoster, Video, status};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

const BASE: &str = "https://monoschinos.st";
const NAME: &str = "MonosChinos";
/// The source id the Kotlin extension sets. Keep it: backups name it.
const ID: i64 = 6957694006954649296;

struct MonosChinos;

impl Source for MonosChinos {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "es", supports_latest: true, request_gap_ms: Some(500) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let get = |url: &str| Request::get(url).header("Referer", format!("{BASE}/")).text(host, NAME);
        let popular = || get(&format!("{BASE}/animes?p={}", input.page().max(1)));
        match op {
            "popular" => to_value(parse_list(&popular()?)),
            // the home page's newest episodes; it has no further pages
            "latest" => to_value(parse_latest(&get(BASE)?)),
            // without text the Kotlin lists the popular titles (its filters are left out)
            "search" if input.query().trim().is_empty() => to_value(parse_list(&popular()?)),
            "search" => to_value(parse_list(&get(&search_url(input.query(), input.page()))?)),
            "details" => {
                let url = input.url("anime")?;
                let mut anime = parse_details(&get(&absolute(url)?)?)?;
                anime.url = url.to_owned();
                to_value(anime)
            }
            "episodes" => {
                let url = input.url("anime")?;
                let page = get(&absolute(url)?)?;
                let Some((ajax, slug)) = episode_list_of(&page, url) else { return to_value(Vec::<Episode>::new()) };
                let answer = Request::get(ajax)
                    .header("Referer", absolute(url)?)
                    .header("X-Requested-With", "XMLHttpRequest")
                    .header("Accept", "application/json, text/javascript, */*; q=0.01")
                    .text(host, NAME)?;
                to_value(parse_episodes(&answer, &slug)?)
            }
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
                            Err(error) => host.log(2, &format!("monoschinos: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(MonosChinos);

fn search_url(query: &str, page: i32) -> String {
    let q: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("{BASE}/buscar?q={q}&p={}", page.max(1))
}

/// A stored path (`/anime/x`, `/ver/x`) as a full address, refused when it points elsewhere.
fn absolute(path: &str) -> Res<String> {
    if path.starts_with('/') && !path.starts_with("//") { Ok(format!("{BASE}{path}")) } else { Err(format!("not a {NAME} address: {path}")) }
}

/// The path of a link on the site (`https://monoschinos.st/anime/x` → `/anime/x`).
fn path_of(href: &str) -> String {
    href.strip_prefix(BASE).unwrap_or(href).to_owned()
}

/// A lazy image's address: `data-src`, unless it is the site's placeholder.
fn image(element: ElementRef) -> Option<String> {
    let img = first(element, "img")?.value();
    img.attr("data-src").or(img.attr("src")).filter(|src| !src.is_empty() && !src.contains("anime.png")).map(str::to_owned)
}

/// The cards of the anime list or a search: `article > a.card-wrap` with a cover and an `h3`.
fn parse_list(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let animes = doc
        .select(&sel("article > a.card-wrap[href*='/anime/']"))
        .filter_map(|card| {
            Some(Anime {
                url: path_of(card.value().attr("href")?),
                title: first(card, "h3").map(text_of).filter(|t| !t.is_empty())?,
                thumbnail_url: image(card),
                ..Default::default()
            })
        })
        .collect();
    AnimesPage { animes, has_next_page: doc.select(&sel("a[rel=next]")).next().is_some() }
}

/// The home page's newest episodes as their titles, once each. An episode links `/ver/<slug>-episodio-<n>`; its title
/// is `/anime/<slug>-sub-espanol`, as the Kotlin builds it.
fn parse_latest(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let mut animes: Vec<Anime> = Vec::new();
    for card in doc.select(&sel("article > a.card-wrap[href*='/ver/']")) {
        let Some(slug) = card.value().attr("href").and_then(|h| h.split("/ver/").nth(1)).and_then(|s| s.rsplit_once("-episodio-")).map(|(slug, _)| slug) else { continue };
        let url = format!("/anime/{slug}-sub-espanol");
        let Some(title) = first(card, "h3").map(text_of).filter(|t| !t.is_empty()) else { continue };
        if !animes.iter().any(|a| a.url == url) {
            animes.push(Anime { url, title, thumbnail_url: image(card), ..Default::default() });
        }
    }
    AnimesPage { animes, has_next_page: false }
}

fn parse_details(html: &str) -> Res<Anime> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let title = first(root, "h1").map(text_of).filter(|t| !t.is_empty()).ok_or("the title page has no title")?;
    let mut genres: Vec<String> = Vec::new();
    for genre in doc.select(&sel("a[href*='/genero/']")).map(text_of) {
        if !genres.contains(&genre) {
            genres.push(genre);
        }
    }
    // the details list: <dt>Estado</dt><dd>Finalizado</dd>
    let state = doc
        .select(&sel("dt"))
        .find(|dt| text_of(*dt) == "Estado")
        .and_then(|dt| dt.next_siblings().filter_map(ElementRef::wrap).next())
        .map(text_of);
    Ok(Anime {
        title,
        description: first(root, "#tab-info p").map(text_of).filter(|d| !d.is_empty()),
        thumbnail_url: attr(root, "img.lazy[data-src*='/thumbs/']", "data-src"),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status: match state.as_deref() {
            Some("Estreno" | "En emisión") => status::ONGOING,
            Some("Finalizado") => status::COMPLETED,
            _ => status::UNKNOWN,
        },
        ..Default::default()
    })
}

/// The address of the title's episode list (`section.caplist[data-ajax]`) and the slug its episodes are named by: the
/// one in the page's first `/ver/` link, else the title's own without `-sub-espanol`.
fn episode_list_of(html: &str, anime_url: &str) -> Option<(String, String)> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let ajax = attr(root, "section.caplist", "data-ajax")?;
    let ajax = if ajax.starts_with("http") { ajax } else { format!("{BASE}{ajax}") };
    let slug = attr(root, "a[href*='/ver/']", "href")
        .and_then(|h| Some(h.split("/ver/").nth(1)?.split("-episodio-").next()?.to_owned()))
        .unwrap_or_else(|| anime_url.trim_start_matches("/anime/").trim_end_matches("-sub-espanol").to_owned());
    Some((ajax, slug)).filter(|(_, slug)| !slug.is_empty())
}

/// The episode list (`{"eps":[{"num":14},...]}`, every episode at once) as episodes, newest first.
fn parse_episodes(json: &str, slug: &str) -> Res<Vec<Episode>> {
    let list: Value = serde_json::from_str(json).map_err(|e| format!("invalid episode list: {e}"))?;
    let mut episodes: Vec<Episode> = list["eps"]
        .as_array()
        .ok_or("the episode list has no eps")?
        .iter()
        .filter_map(|ep| {
            // a number (14) or a text ("12.5")
            let text = match &ep["num"] {
                Value::String(s) => s.trim().to_owned(),
                other => other.to_string(),
            };
            let number: f32 = text.parse().ok()?;
            let shown = if number.fract() == 0.0 { (number as i64).to_string() } else { text };
            Some(Episode { url: format!("/ver/{slug}-episodio-{shown}"), name: format!("Episodio {shown}"), episode_number: number, ..Default::default() })
        })
        .collect();
    episodes.sort_by(|a, b| b.episode_number.total_cmp(&a.episode_number));
    Ok(episodes)
}

/// The servers of an episode page that the extractors can play, in the site's order. Each button carries its embed
/// address in base64 (`data-player`) and the server's name as its text.
fn parse_hosters(html: &str) -> Vec<Hoster> {
    Html::parse_document(html)
        .select(&sel("button.play-video[data-player]"))
        .filter_map(|button| {
            let embed = STANDARD.decode(button.value().attr("data-player")?.trim()).ok().and_then(|b| String::from_utf8(b).ok())?;
            let name = button.value().attr("data-server").map(str::to_owned).filter(|n| !n.is_empty()).unwrap_or_else(|| text_of(button));
            Some(Hoster { hoster_url: playable(&embed)?, hoster_name: capitalized(&name), ..Default::default() })
        })
        .collect()
}

fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The embed address an extractor reads, `None` when none does. DoodStream moves between domains (dsvplay.com,
/// dooodster.com, ...) that all serve the same ids, so any Dood domain is asked at playmogo.com.
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
    const LATEST: &str = include_str!("../fixtures/latest.html");
    const ANIME: &str = include_str!("../fixtures/anime.html");
    const EPISODES: &str = include_str!("../fixtures/eps.json");
    const EPISODE: &str = include_str!("../fixtures/episode.html");
    const MANIFEST: &str = include_str!("../manifest.toml");

    #[test]
    fn the_anime_list_has_cards_and_more_pages() {
        let page = parse_list(POPULAR);
        assert_eq!(page.animes.len(), 24);
        let first = &page.animes[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/anime/watashi-no-shiawase-na-kekkon-s2-sub-espanol", "Watashi no Shiawase na Kekkon S2"));
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://monoschinos.st/assets/thumbs/"));
        assert!(page.has_next_page);
    }

    #[test]
    fn a_search_lists_its_results_on_one_page() {
        let page = parse_list(SEARCH);
        assert_eq!(page.animes.len(), 4);
        assert!(page.animes.iter().all(|a| a.url.starts_with("/anime/") && a.title.to_lowercase().contains("kekkon")));
        assert!(!page.has_next_page);
        assert_eq!(search_url(" shiawase  kekkon ", 0), "https://monoschinos.st/buscar?q=shiawase++kekkon&p=1");
    }

    #[test]
    fn the_latest_episodes_name_their_titles_once() {
        let page = parse_latest(LATEST);
        assert!(page.animes.len() > 20 && !page.has_next_page);
        let urls: Vec<&str> = page.animes.iter().map(|a| a.url.as_str()).collect();
        assert_eq!(urls[0], "/anime/dark-summoner-to-dekiteiru-sub-espanol");
        assert_eq!(urls.iter().filter(|u| **u == "/anime/watashi-no-shiawase-na-kekkon-s2-sub-espanol").count(), 1);
    }

    #[test]
    fn the_title_page_has_details_and_its_episode_list() {
        let anime = parse_details(ANIME).unwrap();
        assert_eq!(anime.title, "Watashi no Shiawase na Kekkon S2");
        assert_eq!(anime.genre.as_deref(), Some("Drama, Fantasía, Histórico, Romance"));
        assert_eq!(anime.status, status::COMPLETED);
        assert_eq!(anime.description.as_deref(), Some("La segunda temporada de Watashi no Shiawase na Kekkon."));
        assert!(anime.thumbnail_url.unwrap().contains("/thumbs/imagen/watashi"));
        let (ajax, slug) = episode_list_of(ANIME, "/anime/watashi-no-shiawase-na-kekkon-s2-sub-espanol").unwrap();
        assert_eq!((ajax.as_str(), slug.as_str()), ("https://monoschinos.st/ajax/ajax_pagination/6353", "watashi-no-shiawase-na-kekkon-s2"));
        let episodes = parse_episodes(EPISODES, &slug).unwrap();
        assert_eq!(episodes.len(), 3);
        assert_eq!((episodes[0].url.as_str(), episodes[0].name.as_str()), ("/ver/watashi-no-shiawase-na-kekkon-s2-episodio-16", "Episodio 16"));
        let half = parse_episodes(r#"{"eps":[{"num":"12.5"}]}"#, "x").unwrap();
        assert_eq!((half[0].url.as_str(), half[0].episode_number), ("/ver/x-episodio-12.5", 12.5));
    }

    #[test]
    fn an_episode_offers_only_the_servers_that_play() {
        let hosters = parse_hosters(EPISODE);
        let urls: Vec<&str> = hosters.iter().map(|h| h.hoster_url.as_str()).collect();
        assert_eq!(urls, ["https://playmogo.com/e/gtnwykefr3nj", "https://mixdrop.top/e/l77w8zw1i13gq8", "https://www.mp4upload.com/embed-93e16mggc0dk.html"]);
        assert_eq!(hosters[0].hoster_name, "Doodstream");
        assert_eq!(playable("https://voe.sx/e/x"), None);
        assert_eq!(playable("https://dooodster.com/e/x").as_deref(), Some("https://playmogo.com/e/x"));
    }

    #[test]
    fn the_manifest_lists_the_site_and_every_extractor_host() {
        assert!(MANIFEST.contains("\"monoschinos.st\""));
        for host in kuro_extractors::HOSTS {
            assert!(MANIFEST.contains(&format!("\"{host}\"")), "manifest.toml misses {host}");
        }
    }

    #[test]
    fn ops_run_through_the_toolkit() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            url if url.contains("/animes?p=") => (200, POPULAR.into()),
            url if url.contains("/ajax/") => (200, EPISODES.into()),
            url if url.contains("/anime/") => (200, ANIME.into()),
            url if url.contains("/ver/") => (200, EPISODE.into()),
            _ => (404, String::new()),
        });
        let call = |op: &str, args: Value| kuro_guest::invoke(&MonosChinos, &host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str::<Value>(&o).unwrap());
        assert_eq!(call("info", json!({})).unwrap()["id"].as_i64(), Some(ID));
        assert_eq!(call("popular", json!({ "page": 1 })).unwrap()["animes"].as_array().unwrap().len(), 24);
        let anime = json!({ "anime": { "url": "/anime/watashi-no-shiawase-na-kekkon-s2-sub-espanol" } });
        assert_eq!(call("episodes", anime).unwrap().as_array().unwrap().len(), 3);
        let hosters = call("hosters", json!({ "episode": { "url": "/ver/watashi-no-shiawase-na-kekkon-s2-episodio-14" } })).unwrap();
        assert_eq!(hosters.as_array().unwrap().len(), 3);
        assert!(call("details", json!({ "anime": { "url": "https://evil.example/x" } })).is_err());
    }
}
