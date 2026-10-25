//! AnimeWorld (<https://www.animeworld.ac>), an anime source plug-in for Kurobayomi: anime in Italian, subbed (SUB ITA)
//! and dubbed (ITA). Ported from the yuzono `it/animeworld` extension (Apache-2.0). This file is the source's ops and
//! parsers; the glue to the app is the `kuro_guest` toolkit.
//!
//! An episode's servers are tabs of its page; the site's API answers each with a file address ("grabber"). The site's
//! own server ("AnimeWorld Server") gives an mp4 that plays as is; a DoodStream tab goes through the shared
//! `lib/extractors`. Left out: the search filters, the preferences (preferred quality and server), and the servers no
//! extractor reads (Streamtape, StreamHide, VidGuard; the site shows none of them today).

use kuro_guest::html::{Html, attr, sel, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, Hoster, Video, status};
use kuro_guest::{Host, Info, Input, Request, Res, Response, Source, to_value, unknown_op};
use serde_json::Value;

const BASE: &str = "https://www.animeworld.ac";
const NAME: &str = "ANIMEWORLD.tv";
/// The source id: the MD5 of `animeworld.tv/it/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 6601315231151625432;
/// The name of the site's own server, whose file plays without an extractor.
const OWN_SERVER: &str = "animeworld";

struct AnimeWorld;

impl Source for AnimeWorld {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "it", supports_latest: true, request_gap_ms: Some(500) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let page = input.page().max(1);
        match op {
            "popular" => to_value(parse_list(&get(host, &format!("{BASE}/filter?sort=6&page={page}"), &[])?)),
            "latest" => to_value(parse_list(&get(host, &format!("{BASE}/updated?page={page}"), &[])?)),
            "search" => to_value(parse_list(&get(host, &search_url(input.query(), page), &[])?)),
            "details" => {
                let url = input.url("anime")?;
                let mut anime = parse_details(&get(host, &absolute(url)?, &[])?)?;
                anime.url = url.to_owned();
                to_value(anime)
            }
            "episodes" => to_value(parse_episodes(&get(host, &absolute(input.url("anime")?)?, &[])?)),
            "hosters" => to_value(hosters(host, input.url("episode")?)?),
            "videos" => match input.get::<Hoster>("hoster")? {
                Some(hoster) => to_value(hoster_videos(host, &hoster, true)?),
                None => {
                    let mut all = Vec::new();
                    for hoster in &hosters(host, input.url("episode")?)? {
                        // one failing server does not hide the others
                        match hoster_videos(host, hoster, all.is_empty()) {
                            Ok(videos) => all.extend(videos),
                            Err(error) => host.log(2, &format!("animeworld: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(AnimeWorld);

/// A page of the site. The site sometimes answers 202 with a script that sets a cookie and reloads; the request is then
/// sent again with that cookie (the Kotlin's `ShittyRedirectionInterceptor`).
fn get(host: &dyn Host, url: &str, headers: &[(&str, &str)]) -> Res<String> {
    let request = Request::get(url).headers(headers.iter().copied());
    let response = request.send(host)?;
    match (response.status, challenge_cookie(&response)) {
        (202, Some(cookie)) => request.header("Cookie", cookie).text(host, NAME),
        _ => response.text(NAME),
    }
}

/// `name=value` of the challenge page's `document.cookie="name=value; path=/"`.
fn challenge_cookie(response: &Response) -> Option<String> {
    let rest = &response.body[response.body.find("document.cookie=\"")? + 17..];
    let cookie = &rest[..rest.find('"')?];
    Some(cookie.split(';').next()?.trim().to_owned()).filter(|c| c.contains('='))
}

fn search_url(query: &str, page: i32) -> String {
    let q: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("{BASE}/filter?keyword={q}&page={page}")
}

/// A stored path (`/play/x.id`, `/play/x.id/ep`) as a full address, refused when it points elsewhere.
fn absolute(path: &str) -> Res<String> {
    if path.starts_with('/') && !path.starts_with("//") { Ok(format!("{BASE}{path}")) } else { Err(format!("not a {NAME} address: {path}")) }
}

/// The path of a link on the site.
fn path_of(href: &str) -> String {
    href.strip_prefix(BASE).unwrap_or(href).to_owned()
}

/// The cards of a list (popular, latest, a search): `div.film-list div.item div.inner a.poster` with the cover as its
/// `img` (whose `alt` is the title).
fn parse_list(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let animes = doc
        .select(&sel("div.film-list div.item div.inner a.poster"))
        .filter_map(|poster| {
            Some(Anime {
                url: path_of(poster.value().attr("href")?),
                title: attr(poster, "img", "alt")?,
                thumbnail_url: attr(poster, "img", "src"),
                ..Default::default()
            })
        })
        .collect();
    AnimesPage { animes, has_next_page: doc.select(&sel("div.paging-wrapper a#go-next-page")).next().is_some() }
}

fn parse_details(html: &str) -> Res<Anime> {
    let doc = Html::parse_document(html);
    let title = doc.select(&sel("div.c1 h2.title")).next().map(text_of).filter(|t| !t.is_empty()).ok_or("the title page has no title")?;
    // the facts are links in `div.info dl.meta dd`, told apart by their address
    let links: Vec<(String, String)> = doc
        .select(&sel("div.info dl.meta dd a"))
        .filter_map(|a| Some((a.value().attr("href")?.to_owned(), text_of(a))))
        .collect();
    let joined = |keys: &[&str]| Some(links.iter().filter(|(href, _)| keys.iter().any(|k| href.contains(k))).map(|(_, text)| text.as_str()).collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty());
    Ok(Anime {
        title,
        thumbnail_url: doc.select(&sel("div.thumb img")).next().and_then(|img| img.value().attr("src")).map(str::to_owned),
        description: doc.select(&sel("div.desc")).next().map(text_of).filter(|d| !d.is_empty()),
        genre: joined(&["language", "genre"]),
        author: joined(&["studio"]),
        status: match joined(&["status"]).as_deref() {
            Some("In corso") => status::ONGOING,
            Some("Finito") => status::COMPLETED,
            _ => status::UNKNOWN,
        },
        ..Default::default()
    })
}

/// The episodes of a title page (the active server's list), newest first.
fn parse_episodes(html: &str) -> Vec<Episode> {
    let doc = Html::parse_document(html);
    let mut episodes: Vec<Episode> = doc
        .select(&sel("div.server.active ul.episodes li.episode a"))
        .filter_map(|link| {
            let text = text_of(link);
            Some(Episode {
                url: path_of(link.value().attr("href")?),
                name: format!("Episode: {text}"),
                episode_number: text.parse().unwrap_or(1.0),
                ..Default::default()
            })
        })
        .collect();
    episodes.reverse();
    episodes
}

/// The servers of an episode page that can play: the site's own and DoodStream. Each is the API address of this
/// episode on that server; `internal_data` keeps the episode page for the API's Referer.
fn hosters(host: &dyn Host, episode: &str) -> Res<Vec<Hoster>> {
    let url = absolute(episode)?;
    parse_hosters(&get(host, &url, &[])?, &url)
}

fn parse_hosters(html: &str, page_url: &str) -> Res<Vec<Hoster>> {
    let doc = Html::parse_document(html);
    if let Some(alert) = doc.select(&sel("div.alert.alert-primary")).map(text_of).find(|t| t.contains("Copyright")) {
        return Err(alert);
    }
    let Some(episode_id) = doc.select(&sel("div#player[data-episode-id]")).next().and_then(|p| p.value().attr("data-episode-id")) else {
        return Ok(Vec::new());
    };
    Ok(doc
        .select(&sel("div.servers > div.widget-title span.server-tab"))
        .filter_map(|tab| {
            let name = text_of(tab);
            let lower = name.to_lowercase();
            if !lower.contains(OWN_SERVER) && !lower.contains("dood") {
                return None;
            }
            let server = tab.value().attr("data-name")?;
            let data_id = doc
                .select(&sel(&format!("div.server[data-name=\"{server}\"] li.episode a[data-episode-id=\"{episode_id}\"]")))
                .next()?
                .value()
                .attr("data-id")
                .filter(|id| !id.trim().is_empty())?;
            Some(Hoster {
                hoster_url: format!("{BASE}/api/episode/info?id={data_id}&alt=0"),
                hoster_name: name,
                internal_data: page_url.to_owned(),
                ..Default::default()
            })
        })
        .collect())
}

/// The file address the API gives for one server of an episode.
fn grabber(answer: &str) -> Res<String> {
    let answer: Value = serde_json::from_str(answer).map_err(|e| format!("{NAME}: unexpected answer: {e}"))?;
    answer["grabber"].as_str().filter(|g| g.starts_with("http")).map(str::to_owned).ok_or_else(|| format!("{NAME}: the server has no file"))
}

/// The videos of one server; the first video is the preferred one when `preferred`.
fn hoster_videos(host: &dyn Host, hoster: &Hoster, preferred: bool) -> Res<Vec<Video>> {
    if !hoster.hoster_url.starts_with(&format!("{BASE}/api/")) {
        return Err(format!("not a {NAME} server: {}", hoster.hoster_url));
    }
    let referer = if hoster.internal_data.starts_with(BASE) { hoster.internal_data.as_str() } else { BASE };
    let answer = get(host, &hoster.hoster_url, &[("Accept", "application/json, text/javascript, */*; q=0.01"), ("Referer", referer), ("X-Requested-With", "XMLHttpRequest")])?;
    let file = grabber(&answer)?;
    let mut videos = if hoster.hoster_name.to_lowercase().contains(OWN_SERVER) {
        vec![Video { video_url: file, video_title: "Auto".into(), ..Default::default() }]
    } else {
        kuro_extractors::videos(host, &file, "").ok_or_else(|| format!("{} is not supported", hoster.hoster_name))??
    };
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
    const INFO: &str = include_str!("../fixtures/info.json");
    const MANIFEST: &str = include_str!("../manifest.toml");
    const EPISODE: &str = "/play/black-clover.MaMIb/fpBDR";

    #[test]
    fn the_lists_have_cards_and_a_next_page() {
        let page = parse_list(POPULAR);
        assert_eq!(page.animes.len(), 40);
        let first = &page.animes[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/play/one-piece-subita.qzG-LE", "One Piece"));
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://img.animeworld.ac/locandine/"));
        assert!(page.has_next_page);
        let search = parse_list(SEARCH);
        assert_eq!(search.animes.len(), 13);
        assert!(search.animes.iter().any(|a| a.url == "/play/black-clover.MaMIb" && a.title == "Black Clover"));
        assert!(!search.has_next_page);
        assert_eq!(search_url(" città di notte ", 2), "https://www.animeworld.ac/filter?keyword=citt%C3%A0+di+notte&page=2");
    }

    #[test]
    fn the_title_page_has_details_and_episodes_newest_first() {
        let anime = parse_details(ANIME).unwrap();
        assert_eq!(anime.title, "Black Clover");
        assert_eq!(anime.genre.as_deref(), Some("Giapponese, Azione, Commedia, Fantasy, Magia, Shounen"));
        assert_eq!((anime.author.as_deref(), anime.status), (Some("Studio Pierrot"), status::COMPLETED));
        assert_eq!(anime.thumbnail_url.as_deref(), Some("https://img.animeworld.ac/locandine/TSAUM.jpg?0"));
        assert!(anime.description.unwrap().starts_with("In un mondo dove la magia"));
        let episodes = parse_episodes(ANIME);
        assert_eq!(episodes.len(), 170);
        assert_eq!((episodes[169].url.as_str(), episodes[169].name.as_str(), episodes[169].episode_number), (EPISODE, "Episode: 1", 1.0));
        assert_eq!(episodes[0].episode_number, 170.0);
    }

    #[test]
    fn an_episode_offers_the_site_s_server() {
        let hosters = parse_hosters(ANIME, &format!("{BASE}{EPISODE}")).unwrap();
        assert_eq!(hosters.len(), 1);
        assert_eq!(hosters[0].hoster_name, "AnimeWorld Server");
        assert_eq!(hosters[0].hoster_url, "https://www.animeworld.ac/api/episode/info?id=fpBDR&alt=0");
        assert_eq!(hosters[0].internal_data, "https://www.animeworld.ac/play/black-clover.MaMIb/fpBDR");
        let blocked = r#"<div class="alert alert-primary">Copyright: episodio rimosso</div>"#;
        assert_eq!(parse_hosters(blocked, BASE), Err("Copyright: episodio rimosso".into()));
        assert_eq!(grabber(INFO).unwrap(), "https://srv15-parrocchia.sweetpixel.org/DDL/ANIME/BlackClover/BlackClover_Ep_001_SUB_ITA.mp4");
    }

    #[test]
    fn a_cookie_challenge_is_answered_once() {
        let host = TestHost::answering(|request| match request.headers.iter().any(|(n, v)| n == "Cookie" && v == "SecurityAW-gl=abc") {
            true => (200, POPULAR.into()),
            false => (202, r#"<script>document.cookie="SecurityAW-gl=abc; path=/";location.reload();</script>"#.into()),
        });
        assert!(get(&host, BASE, &[]).unwrap().contains("film-list"));
        assert_eq!(host.sent().len(), 2);
    }

    #[test]
    fn the_manifest_lists_the_site_and_every_extractor_host() {
        assert!(MANIFEST.contains("\"www.animeworld.ac\""));
        for host in kuro_extractors::HOSTS {
            assert!(MANIFEST.contains(&format!("\"{host}\"")), "manifest.toml misses {host}");
        }
    }

    #[test]
    fn ops_run_through_the_toolkit() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            url if url.contains("/filter?sort=6") || url.contains("/updated") => (200, POPULAR.into()),
            url if url.contains("/api/episode/info") => (200, INFO.into()),
            url if url.contains("/play/") => (200, ANIME.into()),
            _ => (404, String::new()),
        });
        let call = |op: &str, args: Value| kuro_guest::invoke(&AnimeWorld, &host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str::<Value>(&o).unwrap());
        assert_eq!(call("info", json!({})).unwrap()["id"].as_i64(), Some(ID));
        assert_eq!(call("latest", json!({ "page": 1 })).unwrap()["animes"].as_array().unwrap().len(), 40);
        let hosters = call("hosters", json!({ "episode": { "url": EPISODE } })).unwrap();
        let videos = call("videos", json!({ "hoster": hosters[0] })).unwrap();
        assert_eq!(videos[0]["videoUrl"], "https://srv15-parrocchia.sweetpixel.org/DDL/ANIME/BlackClover/BlackClover_Ep_001_SUB_ITA.mp4");
        assert_eq!((videos[0]["videoTitle"].as_str(), videos[0]["preferred"].as_bool()), (Some("Auto"), Some(true)));
        let api = host.sent().into_iter().find(|r| r.url.contains("/api/")).unwrap();
        assert!(api.headers.contains(&("Referer".to_owned(), format!("{BASE}{EPISODE}"))));
        assert!(call("details", json!({ "anime": { "url": "https://evil.example/x" } })).is_err());
        assert!(call("videos", json!({ "hoster": { "hosterUrl": "https://evil.example/x", "hosterName": "AnimeWorld Server" } })).is_err());
    }
}
