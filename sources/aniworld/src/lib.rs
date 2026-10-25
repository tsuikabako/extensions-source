//! AniWorld (<https://aniworld.to>), an anime source plug-in for Kurobayomi: anime in German, dubbed and subbed (and
//! some with English subtitles). Ported from the yuzono `de/aniworld` extension (Apache-2.0). This file is the source's
//! ops and parsers; the video hosts are read by the shared `lib/extractors`, the glue to the app is the `kuro_guest`
//! toolkit.
//!
//! An episode lists its servers per language; each is a `/redirect/<id>` link of the site that leads to the host's
//! player page. Left out: the servers no extractor reads (VOE and Filemoon move to new domains; Vidoza and Streamtape
//! are no longer offered), the preferences (preferred language and server, excluded servers), and the DDoS-Guard cookie
//! step (the site does not ask for it today).

use kuro_guest::html::{ElementRef, Html, first, sel, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, Hoster, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

const BASE: &str = "https://aniworld.to";
const NAME: &str = "AniWorld";
/// The source id the Kotlin extension sets. Keep it: backups name it.
const ID: i64 = 8286900189409315836;
/// The servers an extractor reads, by the name the site gives them.
const SERVERS: &[&str] = &["Doodstream", "Vidmoly"];
/// The languages of a server, by its `data-lang-key`.
const LANGUAGES: &[(&str, &str)] = &[("3", "Deutscher Sub"), ("1", "Deutscher Dub"), ("2", "Englischer Sub")];

struct AniWorld;

impl Source for AniWorld {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "de", supports_latest: true, request_gap_ms: Some(500) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let get = |url: &str| Request::get(url).text(host, NAME);
        match op {
            "popular" => to_value(parse_list(&get(&format!("{BASE}/beliebte-animes"))?)),
            "latest" => to_value(parse_list(&get(&format!("{BASE}/neu"))?)),
            "search" => {
                let q: String = url::form_urlencoded::byte_serialize(input.query().trim().as_bytes()).collect();
                let answer = Request::get(format!("{BASE}/ajax/seriesSearch?keyword={q}"))
                    .header("Referer", format!("{BASE}/search"))
                    .header("Origin", BASE)
                    .header("X-Requested-With", "XMLHttpRequest")
                    .text(host, NAME)?;
                to_value(parse_search(&answer)?)
            }
            "details" => {
                let url = input.url("anime")?;
                let mut anime = parse_details(&get(&absolute(url)?)?)?;
                anime.url = url.to_owned();
                to_value(anime)
            }
            "episodes" => {
                let mut episodes = Vec::new();
                for season in parse_seasons(&get(&absolute(input.url("anime")?)?)?) {
                    episodes.extend(parse_episodes(&get(&absolute(&season)?)?));
                }
                // seasons and their episodes come oldest first
                episodes.reverse();
                to_value(episodes)
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
                            Err(error) => host.log(2, &format!("aniworld: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(AniWorld);

/// A stored path (`/anime/stream/x`, `/redirect/1`) as a full address, refused when it points elsewhere.
fn absolute(path: &str) -> Res<String> {
    let path = path.strip_prefix(BASE).unwrap_or(path);
    if path.starts_with('/') && !path.starts_with("//") { Ok(format!("{BASE}{path}")) } else { Err(format!("not a {NAME} address: {path}")) }
}

/// A link or image address of the site as a full address.
fn full(href: &str) -> String {
    if href.starts_with('/') && !href.starts_with("//") { format!("{BASE}{href}") } else { href.to_owned() }
}

/// The cards of the popular and new lists: `div.seriesListContainer > div` with a link, a lazy cover (`data-src`) and
/// an `h3` title. The lists are one page.
fn parse_list(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let animes = doc
        .select(&sel("div.seriesListContainer > div"))
        .filter_map(|card| {
            let link = first(card, "a")?;
            Some(Anime {
                url: link.value().attr("href")?.strip_prefix(BASE).unwrap_or(link.value().attr("href")?).to_owned(),
                title: first(card, "h3").map(text_of).filter(|t| !t.is_empty())?,
                thumbnail_url: first(link, "img").and_then(|img| img.value().attr("data-src")).map(full),
                ..Default::default()
            })
        })
        .collect();
    AnimesPage { animes, has_next_page: false }
}

/// The site's search answer: JSON `[{name, link, description, cover}]`, with HTML entities in the texts.
fn parse_search(answer: &str) -> Res<AnimesPage> {
    let results: Vec<Value> = serde_json::from_str(answer).map_err(|e| format!("{NAME}: unexpected search answer: {e}"))?;
    let animes = results
        .iter()
        .filter_map(|result| {
            Some(Anime {
                url: format!("/anime/stream/{}", result["link"].as_str()?),
                title: unescape(result["name"].as_str()?),
                thumbnail_url: result["cover"].as_str().map(|c| full(&c.replace("150x225", "220x330"))),
                description: result["description"].as_str().map(unescape),
                ..Default::default()
            })
        })
        .collect();
    Ok(AnimesPage { animes, has_next_page: false })
}

/// The text of an HTML fragment (`&#8230;` → `…`).
fn unescape(text: &str) -> String {
    Html::parse_fragment(text).root_element().text().collect()
}

fn parse_details(html: &str) -> Res<Anime> {
    let doc = Html::parse_document(html);
    let title = doc.select(&sel("div.series-title h1 span")).next().map(text_of).filter(|t| !t.is_empty()).ok_or("the title page has no title")?;
    let genres: Vec<String> = doc.select(&sel("div.genres ul li")).map(text_of).collect();
    // the producers are the list in `div.cast` whose label is "Produzent:"
    let producers: Vec<String> = doc
        .select(&sel("div.cast li"))
        .find(|li| first(*li, "strong").is_some_and(|label| text_of(label).contains("Produzent:")))
        .map(|li| li.select(&sel("ul li")).map(text_of).collect())
        .unwrap_or_default();
    Ok(Anime {
        title,
        thumbnail_url: doc.select(&sel("div.seriesCoverBox img")).next().and_then(|img| img.value().attr("data-src")).map(full),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        description: doc.select(&sel("p.seri_des")).next().and_then(|p| p.value().attr("data-full-description")).map(str::to_owned),
        author: Some(producers.join(", ")).filter(|p| !p.is_empty()),
        ..Default::default()
    })
}

/// The season pages of a title page (the films first when it has some), in the site's order.
fn parse_seasons(html: &str) -> Vec<String> {
    Html::parse_document(html).select(&sel("#stream > ul:nth-child(1) > li > a")).filter_map(|a| a.value().attr("href").map(str::to_owned)).collect()
}

/// The episodes of a season (or films) page, oldest first.
fn parse_episodes(html: &str) -> Vec<Episode> {
    Html::parse_document(html).select(&sel("table.seasonEpisodesList tbody tr")).filter_map(episode_of).collect()
}

/// One row: "Staffel 2 Folge 3 : <title>" numbered by its `meta`, or "Film 1 : <title>".
fn episode_of(row: ElementRef) -> Option<Episode> {
    let num = row.value().attr("data-episode-season-id")?;
    let link = first(row, "td.seasonEpisodeTitle a")?;
    let url = link.value().attr("href")?.to_owned();
    let title = link.select(&sel("span")).map(text_of).collect::<Vec<_>>().join(" ");
    let (name, number) = if url.contains("/filme") {
        (format!("Film {num} : {title}"), num.parse().ok())
    } else {
        let season = url.split("staffel-").nth(1)?.split("/episode").next()?;
        (format!("Staffel {season} Folge {num} : {title}"), first(row, "td meta").and_then(|m| m.value().attr("content")?.parse().ok()))
    };
    Some(Episode { url, name, episode_number: number.unwrap_or(0.0), ..Default::default() })
}

/// The servers of an episode page that an extractor reads, in the site's order. Each is the site's `/redirect/<id>`
/// link; its language ("Deutscher Sub") is in `internal_data` and the server's name.
fn parse_hosters(html: &str) -> Vec<Hoster> {
    Html::parse_document(html)
        .select(&sel("ul.row li[data-lang-key]"))
        .filter_map(|item| {
            let server_text = first(item, "a h4").map(text_of)?;
            let server = SERVERS.iter().find(|s| server_text.to_lowercase().contains(&s.to_lowercase()))?;
            let key = item.value().attr("data-lang-key")?;
            let language = LANGUAGES.iter().find(|(k, _)| key.contains(k)).map_or("?", |(_, l)| l);
            Some(Hoster {
                hoster_url: full(first(item, "a.watchEpisode")?.value().attr("href")?),
                hoster_name: format!("{server} ({language})"),
                internal_data: language.to_owned(),
                ..Default::default()
            })
        })
        .collect()
}

/// The videos of one server: its redirect leads to the host's player page, which an extractor reads. The first video
/// is the preferred one when `preferred`.
fn hoster_videos(host: &dyn Host, hoster: &Hoster, preferred: bool) -> Res<Vec<Video>> {
    let redirect = absolute(&hoster.hoster_url)?;
    let response = Request::get(&redirect).send(host)?;
    if !response.is_success() {
        return Err(response.error(NAME));
    }
    let embed = if response.url.is_empty() { redirect } else { response.url };
    let mut videos = kuro_extractors::videos(host, &embed, &hoster.internal_data).ok_or_else(|| format!("{} is not supported", hoster.hoster_name))??;
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
    const SEARCH: &str = include_str!("../fixtures/search.json");
    const ANIME: &str = include_str!("../fixtures/anime.html");
    const SEASON: &str = include_str!("../fixtures/season.html");
    const FILMS: &str = include_str!("../fixtures/films.html");
    const EPISODE: &str = include_str!("../fixtures/episode.html");
    const MANIFEST: &str = include_str!("../manifest.toml");

    #[test]
    fn the_popular_list_has_cards() {
        let page = parse_list(POPULAR);
        assert_eq!(page.animes.len(), 45);
        let first = &page.animes[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/anime/stream/overgeared", "Overgeared"));
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://aniworld.to/public/img/cover/overgeared-stream-cover-"));
        assert!(!page.has_next_page);
    }

    #[test]
    fn a_search_reads_the_json_answer() {
        let page = parse_search(SEARCH).unwrap();
        assert_eq!(page.animes.len(), 2);
        let first = &page.animes[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/anime/stream/black-clover", "Black Clover"));
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://aniworld.to/public/img/cover/") && first.thumbnail_url.as_deref().unwrap().ends_with("_220x330.jpg"));
        assert!(first.description.as_deref().unwrap().ends_with("unzertrennlich,…"));
        assert_eq!(unescape("Tom &amp; Jerry"), "Tom & Jerry");
    }

    #[test]
    fn the_title_page_has_details_and_seasons() {
        let anime = parse_details(ANIME).unwrap();
        assert_eq!(anime.title, "Black Clover");
        assert!(anime.genre.as_deref().unwrap().starts_with("Fighting-Shounen, Abenteuer, Action"));
        assert!(anime.author.as_deref().unwrap().starts_with("Pierrot, Shueisha"));
        assert!(anime.description.unwrap().starts_with("[S1-S4 zählt als gesamte 1. Staffel"));
        assert!(anime.thumbnail_url.unwrap().ends_with("_220x330.jpg"));
        let seasons = parse_seasons(ANIME);
        assert_eq!(seasons[0], "/anime/stream/black-clover/filme");
        assert_eq!(seasons[1..], ["/anime/stream/black-clover/staffel-1", "/anime/stream/black-clover/staffel-2", "/anime/stream/black-clover/staffel-3", "/anime/stream/black-clover/staffel-4", "/anime/stream/black-clover/staffel-5"]);
    }

    #[test]
    fn season_and_film_rows_become_episodes() {
        let episodes = parse_episodes(SEASON);
        assert_eq!(episodes.len(), 5);
        assert_eq!(episodes[0].url, "/anime/stream/overgeared/staffel-1/episode-1");
        assert_eq!((episodes[0].name.as_str(), episodes[0].episode_number), ("Staffel 1 Folge 1 : Legendary Class", 1.0));
        let films = parse_episodes(FILMS);
        assert_eq!(films.len(), 1);
        assert_eq!((films[0].name.as_str(), films[0].episode_number), ("Film 1 : Sword of the Wizard King [Movie]", 1.0));
        assert_eq!(films[0].url, "/anime/stream/black-clover/filme/film-1");
    }

    #[test]
    fn an_episode_offers_only_the_servers_that_play_in_each_language() {
        let hosters = parse_hosters(EPISODE);
        let names: Vec<&str> = hosters.iter().map(|h| h.hoster_name.as_str()).collect();
        assert_eq!(
            names,
            ["Doodstream (Deutscher Dub)", "Vidmoly (Deutscher Dub)", "Doodstream (Englischer Sub)", "Vidmoly (Englischer Sub)", "Doodstream (Deutscher Sub)", "Vidmoly (Deutscher Sub)"]
        );
        assert_eq!(hosters[0].hoster_url, "https://aniworld.to/redirect/4240421");
        assert_eq!(hosters[0].internal_data, "Deutscher Dub");
    }

    /// A test host whose `/redirect/` answers land on Vidmoly's player page, as the site's redirect does.
    struct Redirecting(TestHost);

    impl Host for Redirecting {
        fn http(&self, request_json: &str) -> Res<String> {
            let mut answer: Value = serde_json::from_str(&self.0.http(request_json)?).unwrap();
            if answer["url"].as_str().is_some_and(|u| u.contains("/redirect/")) {
                answer["url"] = "https://vidmoly.net/embed-ka28nfechsnx.html".into();
            }
            Ok(answer.to_string())
        }
        fn unix_seconds(&self) -> u64 {
            0
        }
        fn pref_get(&self, _key: &str) -> String {
            String::new()
        }
        fn pref_set(&self, _key: &str, _value_json: &str) -> Res<()> {
            Ok(())
        }
        fn log(&self, _level: u8, _message: &str) {}
    }

    #[test]
    fn a_server_follows_its_redirect_to_the_extractor() {
        let host = Redirecting(TestHost::with_body("<script>player.setup({ sources: [{ file: 'https://v.example/master.m3u8' }], })</script>"));
        let hoster = &parse_hosters(EPISODE)[1];
        let videos = hoster_videos(&host, hoster, true).unwrap();
        assert_eq!((videos[0].video_url.as_str(), videos[0].video_title.as_str(), videos[0].preferred), ("https://v.example/master.m3u8", "Deutscher Dub Auto", true));
        let sent: Vec<String> = host.0.sent().into_iter().map(|r| r.url).collect();
        assert_eq!(sent, ["https://aniworld.to/redirect/4240419", "https://vidmoly.biz/embed-ka28nfechsnx.html"]);
        let elsewhere = Hoster { hoster_url: "https://evil.example/x".into(), ..hoster.clone() };
        assert!(hoster_videos(&host, &elsewhere, true).is_err());
    }

    #[test]
    fn the_manifest_lists_the_site_and_every_extractor_host() {
        assert!(MANIFEST.contains("\"aniworld.to\""));
        for host in kuro_extractors::HOSTS {
            assert!(MANIFEST.contains(&format!("\"{host}\"")), "manifest.toml misses {host}");
        }
    }

    #[test]
    fn ops_run_through_the_toolkit() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            url if url.ends_with("/beliebte-animes") || url.ends_with("/neu") => (200, POPULAR.into()),
            url if url.contains("/ajax/seriesSearch?keyword=black+clover") => (200, SEARCH.into()),
            url if url.ends_with("/filme") => (200, FILMS.into()),
            url if url.contains("/staffel-") && url.contains("/episode-") => (200, EPISODE.into()),
            url if url.contains("/staffel-") => (200, SEASON.into()),
            url if url.ends_with("/anime/stream/black-clover") => (200, ANIME.into()),
            _ => (404, String::new()),
        });
        let call = |op: &str, args: Value| kuro_guest::invoke(&AniWorld, &host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str::<Value>(&o).unwrap());
        assert_eq!(call("info", json!({})).unwrap()["id"].as_i64(), Some(ID));
        assert_eq!(call("latest", json!({ "page": 1 })).unwrap()["animes"].as_array().unwrap().len(), 45);
        assert_eq!(call("search", json!({ "query": "black clover", "page": 1 })).unwrap()["animes"].as_array().unwrap().len(), 2);
        let episodes = call("episodes", json!({ "anime": { "url": "/anime/stream/black-clover" } })).unwrap();
        // five season pages of five rows each (the same fixture), then the film, newest first
        let episodes = episodes.as_array().unwrap();
        assert_eq!(episodes.len(), 26);
        assert_eq!(episodes[25]["name"], "Film 1 : Sword of the Wizard King [Movie]");
        assert_eq!(episodes[0]["url"], "/anime/stream/overgeared/staffel-1/episode-5");
        let hosters = call("hosters", json!({ "episode": { "url": "/anime/stream/overgeared/staffel-1/episode-1" } })).unwrap();
        assert_eq!(hosters.as_array().unwrap().len(), 6);
        assert!(call("details", json!({ "anime": { "url": "https://evil.example/x" } })).is_err());
    }
}
