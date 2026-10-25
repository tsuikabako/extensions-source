//! AnimeToast (<https://www.animetoast.cc>), an anime source plug-in for Kurobayomi: anime in German, dubbed and
//! subbed. Ported from the yuzono `de/animetoast` extension (Apache-2.0). This file is the source's ops and parsers;
//! the video hosts are read by the shared `lib/extractors`, the glue to the app is the `kuro_guest` toolkit.
//!
//! A title's page has one tab per server, each listing the episodes as links (`?link=<n>`) to a page with that
//! server's player. Long series list season packs instead ("S01:E01-61"), each leading to a page of its own episodes.
//! Left out: the servers no extractor reads (VOE, Filemoon, PlayN, ToastP, VidGuard) and the preferences (preferred
//! and excluded servers).

use kuro_guest::html::{ElementRef, Html, attr, first, sel, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, Hoster, Video, status};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

const BASE: &str = "https://www.animetoast.cc";
const NAME: &str = "AnimeToast";
/// The source id: the MD5 of `animetoast/de/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 2386616842414617638;

struct AnimeToast;

impl Source for AnimeToast {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "de", supports_latest: false, request_gap_ms: Some(500) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let get = |url: &str| Request::get(url).header("Referer", format!("{BASE}/")).text(host, NAME);
        match op {
            // the home page's titles; it has no further pages
            "popular" => to_value(parse_popular(&get(&format!("{BASE}/"))?)),
            "search" => {
                let q: String = url::form_urlencoded::byte_serialize(input.query().trim().as_bytes()).collect();
                to_value(parse_search(&get(&format!("{BASE}/page/{}/?s={q}", input.page().max(1)))?))
            }
            "details" => {
                let url = input.url("anime")?;
                let mut anime = parse_details(&get(&absolute(url)?)?)?;
                anime.url = url.to_owned();
                to_value(anime)
            }
            "episodes" => {
                let page = get(&absolute(input.url("anime")?)?)?;
                let episodes = match parse_episodes(&page, false) {
                    Listing::Episodes(episodes) => episodes,
                    Listing::Packs(packs) => {
                        // a pack's link page points (`#player-embed a`) to the page of its episodes
                        let mut episodes = Vec::new();
                        for pack in packs {
                            let Some(target) = player_link(&get(&absolute(&pack)?)?) else { continue };
                            if let Listing::Episodes(more) = parse_episodes(&get(&absolute(&path_of(&target))?)?, true) {
                                episodes.extend(more);
                            }
                        }
                        episodes
                    }
                };
                // the site lists the first episode first
                to_value(episodes.into_iter().rev().collect::<Vec<_>>())
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
                            Err(error) => host.log(2, &format!("animetoast: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(AnimeToast);

/// A stored path (`/x-ger-dub/?link=3`) as a full address, refused when it points elsewhere.
fn absolute(path: &str) -> Res<String> {
    if path.starts_with('/') && !path.starts_with("//") { Ok(format!("{BASE}{path}")) } else { Err(format!("not a {NAME} address: {path}")) }
}

/// The path of a link on the site (`https://www.animetoast.cc/x/` → `/x/`).
fn path_of(href: &str) -> String {
    href.strip_prefix(BASE).unwrap_or(href).to_owned()
}

/// A title of a card's thumbnail link: its address, `title` and cover.
fn anime_of(link: ElementRef) -> Option<Anime> {
    Some(Anime {
        url: path_of(link.value().attr("href")?),
        title: link.value().attr("title").map(str::trim).filter(|t| !t.is_empty())?.to_owned(),
        thumbnail_url: attr(link, "img", "src"),
        ..Default::default()
    })
}

fn parse_popular(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let animes = doc.select(&sel("div.col-md-4 > div.video-item div.item-thumbnail > a")).filter_map(anime_of).collect();
    AnimesPage { animes, has_next_page: false }
}

fn parse_search(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let animes = doc.select(&sel("div.item-thumbnail > a[href]")).filter_map(anime_of).collect();
    AnimesPage { animes, has_next_page: doc.select(&sel(".nextpostslink")).next().is_some() }
}

/// The post's categories ("Ger Dub", "Serie", "Airing", ...).
fn categories(doc: &Html) -> Vec<String> {
    doc.select(&sel("a[rel='category tag']")).map(text_of).collect()
}

fn parse_details(html: &str) -> Res<Anime> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let title = first(root, "h1.light-title.entry-title").map(text_of).filter(|t| !t.is_empty()).ok_or("the title page has no title")?;
    let genres: Vec<String> = doc.select(&sel("a[rel=tag]")).map(text_of).collect();
    let description: Vec<String> = doc.select(&sel("div.item-content div + p")).map(text_of).filter(|t| !t.is_empty()).collect();
    Ok(Anime {
        title,
        thumbnail_url: attr(root, ".item-content p img", "src"),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        description: Some(description.join("\n")).filter(|d| !d.is_empty()),
        status: if categories(&doc).iter().any(|c| c.to_lowercase().contains("airing")) { status::ONGOING } else { status::COMPLETED },
        ..Default::default()
    })
}

/// What a title page lists.
#[derive(Debug, PartialEq)]
enum Listing {
    Episodes(Vec<Episode>),
    /// The link pages of its season packs.
    Packs(Vec<String>),
}

/// The servers of a page: each tab's name (`a[href=#multi_link_tabN]`) and its episode links.
fn tabs(doc: &Html) -> Vec<(String, Vec<ElementRef<'_>>)> {
    doc.select(&sel("ul.nav-tabs a[href^='#multi_link_tab']"))
        .filter_map(|tab| {
            let id = tab.value().attr("href")?.trim_start_matches('#');
            let pane = doc.select(&sel(&format!("div#{id}"))).next()?;
            Some((text_of(tab), pane.select(&sel("a[href]")).collect()))
        })
        .collect()
}

/// Whether an extractor reads this server, by the name the site gives its tab.
fn supported(server: &str) -> bool {
    let server = server.to_lowercase();
    server.contains("dood") || server.contains("mp4u")
}

/// A title page's episodes, oldest first, from the first server that plays (else the first with links). A film is its
/// own one episode; a season pack's page (`pack`) is not in the "Serie" category but lists episodes.
fn parse_episodes(html: &str, pack: bool) -> Listing {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    if !pack && !categories(&doc).iter().any(|c| c.contains("Serie")) {
        let url = attr(root, "link[rel=canonical]", "href").map(|h| path_of(&h)).unwrap_or_default();
        let name = first(root, "h1.light-title").map(text_of).unwrap_or_default();
        return Listing::Episodes(vec![Episode { url, name, episode_number: 1.0, ..Default::default() }]);
    }
    let tabs = tabs(&doc);
    let Some((_, links)) = tabs.iter().find(|(name, links)| supported(name) && !links.is_empty()).or_else(|| tabs.iter().find(|(_, links)| !links.is_empty())) else {
        return Listing::Episodes(Vec::new());
    };
    let labels: Vec<String> = links.iter().map(|link| text_of(*link)).collect();
    if labels.iter().any(|label| label.contains(':') || label.contains('-')) {
        return Listing::Packs(links.iter().filter_map(|link| link.value().attr("href")).map(path_of).collect());
    }
    Listing::Episodes(
        links
            .iter()
            .zip(labels)
            .filter_map(|(link, label)| {
                Some(Episode {
                    url: path_of(link.value().attr("href")?),
                    episode_number: label.trim_start_matches("Ep.").trim().parse().unwrap_or(-1.0),
                    name: label,
                    ..Default::default()
                })
            })
            .collect(),
    )
}

/// The servers of an episode that the extractors can play: in every server tab, the link labelled like the current
/// episode (`a.current-link`, "Ep. 3").
fn parse_hosters(html: &str) -> Vec<Hoster> {
    let doc = Html::parse_document(html);
    let Some(current) = doc.select(&sel("a.current-link")).next().map(text_of) else { return Vec::new() };
    tabs(&doc)
        .into_iter()
        .filter(|(name, _)| supported(name))
        .filter_map(|(name, links)| {
            let link = links.into_iter().find(|link| text_of(*link) == current)?;
            Some(Hoster { hoster_url: link.value().attr("href")?.to_owned(), hoster_name: name, ..Default::default() })
        })
        .collect()
}

/// The player of a link page: `#player-embed iframe`.
fn player_frame(html: &str) -> Option<String> {
    attr(Html::parse_document(html).root_element(), "#player-embed iframe", "src")
}

/// The page a link page points to instead of a player: `#player-embed a`.
fn player_link(html: &str) -> Option<String> {
    attr(Html::parse_document(html).root_element(), "#player-embed a", "href")
}

/// The embed address an extractor reads, `None` when none does. DoodStream moves between domains (dooodster.com, ...)
/// that all serve the same ids, so any Dood domain is asked at playmogo.com.
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

/// The videos of one server: its link page's player, or the player of the page it points to. The first video is the
/// preferred one when `preferred`.
fn hoster_videos(host: &dyn Host, hoster: &Hoster, preferred: bool) -> Res<Vec<Video>> {
    let get = |url: &str| Request::get(url).header("Referer", format!("{BASE}/")).text(host, NAME);
    let page = get(&absolute(&path_of(&hoster.hoster_url))?)?;
    let frame = match player_frame(&page) {
        Some(frame) => frame,
        None => {
            let target = player_link(&page).ok_or_else(|| format!("{}: the page has no player", hoster.hoster_name))?;
            player_frame(&get(&absolute(&path_of(&target))?)?).ok_or_else(|| format!("{}: the page has no player", hoster.hoster_name))?
        }
    };
    let embed = playable(&frame).ok_or_else(|| format!("{}: {frame} is not supported", hoster.hoster_name))?;
    let mut videos = kuro_extractors::videos(host, &embed, "").ok_or_else(|| format!("{} is not supported", hoster.hoster_name))??;
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
    const MOVIE: &str = include_str!("../fixtures/movie.html");
    const MOVIE_LINK: &str = include_str!("../fixtures/movie-link.html");
    const MOVIE_PLAYER: &str = include_str!("../fixtures/movie-player.html");
    const BUNDLES: &str = include_str!("../fixtures/bundles.html");
    const BUNDLE_LINK: &str = include_str!("../fixtures/bundle-link.html");
    const SUB: &str = include_str!("../fixtures/sub.html");
    const MANIFEST: &str = include_str!("../manifest.toml");

    #[test]
    fn the_home_page_and_a_search_list_titles() {
        let popular = parse_popular(POPULAR);
        assert!(popular.animes.len() == 60 && !popular.has_next_page);
        let first = &popular.animes[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/kanata-kara-ger-sub/", "Kanata kara Ger Sub"));
        assert!(first.thumbnail_url.as_deref().unwrap().starts_with("https://www.animetoast.cc/wp-content/uploads/"));
        let search = parse_search(SEARCH);
        assert_eq!(search.animes.len(), 11);
        assert_eq!(search.animes[3].url, "/one-piece-ger-dub/");
    }

    #[test]
    fn the_title_page_has_details_and_episodes() {
        let anime = parse_details(ANIME).unwrap();
        assert_eq!(anime.title, "Yoru no Kurage wa Oyogenai Ger Dub");
        assert_eq!(anime.genre.as_deref(), Some("Slice of Life"));
        assert_eq!(anime.status, status::COMPLETED);
        assert!(anime.thumbnail_url.unwrap().ends_with("/63102.jpg"));
        let Listing::Episodes(episodes) = parse_episodes(ANIME, false) else { panic!("episodes") };
        // from the Mp4Upload tab: the first server that plays
        assert_eq!(episodes.len(), 12);
        assert_eq!((episodes[0].url.as_str(), episodes[0].name.as_str(), episodes[0].episode_number), ("/yoru-no-kurage-wa-oyogenai-ger-dub/?link=48", "Ep. 1", 1.0));
    }

    #[test]
    fn a_film_is_one_episode_and_packs_lead_to_their_episodes() {
        let Listing::Episodes(film) = parse_episodes(MOVIE, false) else { panic!("a film") };
        assert_eq!((film.len(), film[0].url.as_str()), (1, "/op-stampede-ger-dub/"));
        let Listing::Packs(packs) = parse_episodes(BUNDLES, false) else { panic!("packs") };
        assert_eq!(packs[0], "/one-piece-ger-dub/?link=0");
        assert_eq!(player_link(BUNDLE_LINK).as_deref(), Some("https://www.animetoast.cc/16876-2/?link=0"));
        let Listing::Episodes(pack) = parse_episodes(SUB, true) else { panic!("a pack's episodes") };
        assert_eq!((pack[0].url.as_str(), pack[0].name.as_str()), ("/16876-2/?link=0", "Ep. 1"));
    }

    #[test]
    fn an_episode_offers_only_the_servers_that_play() {
        let hosters = parse_hosters(EPISODE);
        assert_eq!(hosters.len(), 1);
        assert_eq!((hosters[0].hoster_name.as_str(), hosters[0].hoster_url.as_str()), ("Mp4Upload", "https://www.animetoast.cc/yoru-no-kurage-wa-oyogenai-ger-dub/?link=48"));
        assert_eq!(player_frame(EPISODE).as_deref(), Some("https://www.mp4upload.com/embed-aa20pbhtnf36.html"));
        let film = parse_hosters(MOVIE);
        assert_eq!((film.len(), film[0].hoster_name.as_str()), (1, "DoodStream"));
        assert_eq!(player_frame(MOVIE_LINK), None);
        assert_eq!(player_frame(MOVIE_PLAYER).as_deref(), Some("https://dooodster.com/e/qo8jat52jphu"));
        assert_eq!(playable("https://dooodster.com/e/qo8jat52jphu").as_deref(), Some("https://playmogo.com/e/qo8jat52jphu"));
        assert_eq!(playable("https://listeamed.net/e/x"), None);
    }

    #[test]
    fn the_manifest_lists_the_site_and_every_extractor_host() {
        assert!(MANIFEST.contains("\"www.animetoast.cc\""));
        for host in kuro_extractors::HOSTS {
            assert!(MANIFEST.contains(&format!("\"{host}\"")), "manifest.toml misses {host}");
        }
    }

    #[test]
    fn ops_run_through_the_toolkit() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            "https://www.animetoast.cc/" => (200, POPULAR.into()),
            "https://www.animetoast.cc/one-piece-ger-dub/" => (200, BUNDLES.into()),
            url if url.starts_with("https://www.animetoast.cc/one-piece-ger-dub/?link=") => (200, BUNDLE_LINK.into()),
            url if url.starts_with("https://www.animetoast.cc/16876-2/") => (200, SUB.into()),
            url if url.contains("/yoru-no-kurage") => (200, EPISODE.into()),
            "https://www.animetoast.cc/op-stampede-ger-dub/" => (200, MOVIE.into()),
            "https://www.animetoast.cc/op-stampede-ger-dub/?link=1" => (200, MOVIE_LINK.into()),
            "https://www.animetoast.cc/16858-2/?link=1" => (200, MOVIE_PLAYER.into()),
            _ => (404, String::new()),
        });
        let call = |op: &str, args: Value| kuro_guest::invoke(&AnimeToast, &host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str::<Value>(&o).unwrap());
        assert_eq!(call("info", json!({})).unwrap()["id"].as_i64(), Some(ID));
        assert!(call("popular", json!({ "page": 1 })).unwrap()["animes"].as_array().unwrap().len() > 20);
        // every pack leads (in this test) to the same page of episodes, newest first
        let episodes = call("episodes", json!({ "anime": { "url": "/one-piece-ger-dub/" } })).unwrap();
        assert!(episodes.as_array().unwrap().len() > 1);
        assert_eq!(call("hosters", json!({ "episode": { "url": "/yoru-no-kurage-wa-oyogenai-ger-dub/?link=48" } })).unwrap().as_array().unwrap().len(), 1);
        // a film's Dood link page points to another page, whose player is on a Dood domain read at playmogo.com
        assert!(call("videos", json!({ "hoster": { "hosterUrl": "https://www.animetoast.cc/op-stampede-ger-dub/?link=1", "hosterName": "DoodStream" } })).is_err());
        assert!(host.sent().iter().any(|r| r.url == "https://playmogo.com/e/qo8jat52jphu"));
        assert!(call("details", json!({ "anime": { "url": "https://evil.example/x" } })).is_err());
    }
}
