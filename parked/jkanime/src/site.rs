//! The JKAnime site (<https://jkanime.net>) as pure functions: URLs and one parser per page. Ported from the Kohi-den
//! `es/jkanime` extension (Apache-2.0). Without the filters and the preferences (language, quality, server).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::NaiveDateTime;
use kuro_guest::Res;
use kuro_guest::html::{ElementRef, Html, attr, first, own_text, sel, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, status};
use serde::Deserialize;
use serde::de::DeserializeOwned;

pub const BASE: &str = "https://jkanime.net";

// ================================ URLs =================================

pub fn popular_url(page: i32) -> String {
    format!("{BASE}/directorio?filtro=popularidad&p={}", page.max(1))
}

pub fn latest_url(page: i32) -> String {
    format!("{BASE}/directorio?estado=emision&p={}", page.max(1))
}

/// The search: the text is a path segment (`/buscar/<text>`), as the Kotlin sends it. The site answers with its own
/// result cards, or redirects to the directory.
pub fn search_url(query: &str) -> String {
    let mut segment = String::new();
    for b in query.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => segment.push(b as char),
            _ => segment.push_str(&format!("%{b:02X}")),
        }
    }
    format!("{BASE}/buscar/{segment}")
}

pub fn episodes_url(anime_id: &str, page: u32) -> String {
    format!("{BASE}/ajax/episodes/{anime_id}/{page}")
}

/// A stored path (`/one-piece/`, `/one-piece/1/`) as a full address, refused when it points elsewhere.
pub fn absolute(path: &str) -> Res<String> {
    if path.starts_with('/') && !path.starts_with("//") { Ok(format!("{BASE}{path}")) } else { Err(format!("not a JKAnime address: {path}")) }
}

/// The path of an address on the site (`https://jkanime.net/one-piece/` → `/one-piece/`); other addresses unchanged.
pub fn path_of(url: &str) -> String {
    url.strip_prefix(BASE).filter(|p| p.starts_with('/')).unwrap_or(url).to_owned()
}

// =============================== Parsers ===============================

/// The first JSON value after `marker` in the page (`var animes = {...};`, `var servers = [...];`): read up to the end
/// of that value, whatever follows it.
fn json_after<T: DeserializeOwned>(html: &str, marker: &str) -> Option<T> {
    let rest = &html[html.find(marker)? + marker.len()..];
    let start = rest.find(['{', '['])?;
    serde_json::Deserializer::from_str(&rest[start..]).into_iter::<T>().next()?.ok()
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Item {
    number: Option<i64>,
    title: Option<String>,
    synopsis: Option<String>,
    image: Option<String>,
    studios: Option<String>,
    url: Option<String>,
    timestamp: Option<String>,
}

/// A page of the site's paginator (the directory's `var animes` and the episodes API).
#[derive(Deserialize, Default)]
#[serde(default)]
struct Paged {
    data: Vec<Item>,
    next_page_url: Option<String>,
}

impl Paged {
    fn has_next(&self) -> bool {
        self.next_page_url.as_deref().is_some_and(|u| !u.trim().is_empty())
    }
}

/// A directory page (popular, latest, a search that landed there): the titles are JSON in `var animes = {...};`.
pub fn parse_directory(html: &str) -> AnimesPage {
    let Some(paged) = json_after::<Paged>(html, "var animes =") else { return AnimesPage::default() };
    let has_next_page = paged.has_next();
    let animes = paged
        .data
        .into_iter()
        .filter_map(|item| {
            Some(Anime {
                url: path_of(item.url.as_deref().filter(|u| !u.is_empty())?),
                title: item.title.unwrap_or_default(),
                thumbnail_url: item.image,
                description: item.synopsis,
                author: item.studios,
                ..Default::default()
            })
        })
        .collect();
    AnimesPage { animes, has_next_page }
}

/// A search answer. `final_url` is where the request ended after redirects: the directory, or the site's result page
/// (`.anime__item` cards, one page).
pub fn parse_search(html: &str, final_url: &str) -> AnimesPage {
    if final_url.contains("directorio") {
        return parse_directory(html);
    }
    let doc = Html::parse_document(html);
    let animes = doc
        .select(&sel(".anime__item"))
        .filter_map(|card| {
            Some(Anime {
                url: path_of(&attr(card, "a", "href")?),
                title: first(card, "h5 a").map(text_of).unwrap_or_default(),
                thumbnail_url: attr(card, ".set-bg", "data-setbg"),
                ..Default::default()
            })
        })
        .collect();
    AnimesPage { animes, has_next_page: false }
}

/// The details of a title page.
pub fn parse_details(html: &str) -> Anime {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    // the Kotlin's `li:has(span:matchesOwn(^Generos:))`: the list item whose span starts with "Generos:"
    let genres: Vec<String> = doc
        .select(&sel("li"))
        .find(|li| li.select(&sel("span")).any(|span| own_text(span).starts_with("Generos:")))
        .map(|li| li.select(&sel("a")).map(text_of).collect())
        .unwrap_or_default();
    Anime {
        title: first(root, ".anime_info h3").map(text_of).unwrap_or_default(),
        thumbnail_url: first(root, ".anime_info img").and_then(image_url),
        description: first(root, ".scroll").map(text_of).filter(|d| !d.is_empty()),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status: if first(root, ".finished").is_some() { status::COMPLETED } else { status::ONGOING },
        ..Default::default()
    }
}

/// The image of an `img`, from the first of its lazy-loading attributes that is set and is not the placeholder.
fn image_url(img: ElementRef) -> Option<String> {
    ["data-src", "data-lazy-src", "srcset", "src"].iter().find_map(|name| {
        let value = img.value().attr(name)?.trim();
        if value.is_empty() || value.contains("anime.png") {
            return None;
        }
        let value = if *name == "srcset" { value.split(' ').next().unwrap_or(value) } else { value };
        Some(if value.starts_with('/') && !value.starts_with("//") { format!("{BASE}{value}") } else { value.to_owned() })
    })
}

/// What a title page needs for its episodes: the anime id (`[data-anime]`), the page's CSRF token and its own address
/// (`og:url`), which every episode address starts with.
pub struct EpisodeKeys {
    pub anime_id: String,
    pub token: String,
    pub base: String,
}

pub fn episode_keys(html: &str) -> Res<EpisodeKeys> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let anime_id = attr(root, "[data-anime]", "data-anime").ok_or("JKAnime: the title page has no anime id")?;
    let token = attr(root, "meta[name=csrf-token]", "content").unwrap_or_default();
    let base = attr(root, "[property=\"og:url\"]", "content").ok_or("JKAnime: the title page has no address")?;
    Ok(EpisodeKeys { anime_id, token, base })
}

/// One page of the episodes API, in the site's order (oldest first), and whether another page follows.
pub fn parse_episodes(json: &str, base: &str) -> Res<(Vec<Episode>, bool)> {
    let paged: Paged = serde_json::from_str(json).map_err(|e| format!("JKAnime episode list: {e}"))?;
    let base = if base.ends_with('/') { base.to_owned() } else { format!("{base}/") };
    let episodes = paged
        .data
        .iter()
        .filter_map(|item| {
            let number = item.number?;
            Some(Episode {
                url: path_of(&format!("{base}{number}/")),
                name: format!("Episodio {number}"),
                episode_number: number as f32,
                date_upload: item.timestamp.as_deref().map_or(0, date_of),
                ..Default::default()
            })
        })
        .collect();
    Ok((episodes, paged.has_next()))
}

/// A `yyyy-MM-dd HH:mm:ss` time as milliseconds since 1970 (read as UTC), 0 when it does not parse.
fn date_of(text: &str) -> i64 {
    NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%d %H:%M:%S").map_or(0, |t| t.and_utc().timestamp_millis())
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Server {
    remote: Option<String>,
    server: Option<String>,
    lang: Option<i64>,
}

/// One server of an episode: its player address (decoded), the site's name for it and its language label.
#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeServer {
    pub url: String,
    pub name: String,
    pub lang: &'static str,
}

/// The servers of an episode page: JSON in `var servers = [...];`, each player address in base64.
pub fn parse_servers(html: &str) -> Vec<EpisodeServer> {
    json_after::<Vec<Server>>(html, "var servers =")
        .unwrap_or_default()
        .into_iter()
        .filter_map(|server| {
            let url = String::from_utf8(STANDARD.decode(server.remote?.trim()).ok()?).ok()?;
            let lang = match server.lang {
                Some(1) => "[JAP]",
                Some(3) => "[LAT]",
                Some(4) => "[CHIN]",
                _ => "",
            };
            Some(EpisodeServer { url, name: server.server.unwrap_or_default(), lang })
        })
        .collect()
}

/// The site's own players, by a part of their address (the Kotlin's `conventions`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OwnPlayer {
    Desuka,
    Nozomi,
    Desu,
}

impl OwnPlayer {
    pub fn of(url: &str) -> Option<Self> {
        let url = url.to_lowercase();
        if url.contains("stream/jkmedia") {
            Some(Self::Desuka)
        } else if url.contains("um2.php") || url.contains("nozomi") {
            Some(Self::Nozomi)
        } else if url.contains("um.php") {
            Some(Self::Desu)
        } else {
            None
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Desuka => "Desuka",
            Self::Nozomi => "Nozomi",
            Self::Desu => "Desu",
        }
    }
}

/// The address in a player script: the text between `url: '` and the next `'`, in the first script that holds
/// `marker` (`var parts = {` for Desu, `new DPlayer({` for Desuka).
pub fn script_url(html: &str, marker: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    doc.select(&sel("script")).map(|s| s.text().collect::<String>()).find(|text| text.contains(marker)).and_then(|text| {
        let after = text.split_once("url: '")?.1;
        Some(after.split_once('\'')?.0.to_owned()).filter(|u| !u.is_empty())
    })
}

/// Nozomi's first page: the key of its form (`form input[value]`).
pub fn nozomi_key(html: &str) -> Option<String> {
    attr(Html::parse_document(html).root_element(), "form input[value]", "value")
}

#[derive(Deserialize)]
struct NozomiFile {
    file: Option<String>,
}

/// Nozomi's API answer: `{"file": "<video>"}`.
pub fn nozomi_file(json: &str) -> Option<String> {
    serde_json::from_str::<NozomiFile>(json).ok()?.file.filter(|f| !f.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIRECTORY: &str = include_str!("../fixtures/directory.html");
    const SEARCH: &str = include_str!("../fixtures/search.html");
    const ANIME: &str = include_str!("../fixtures/anime.html");
    const EPISODES: &str = include_str!("../fixtures/episodes.json");
    const EPISODE: &str = include_str!("../fixtures/episode.html");

    #[test]
    fn the_directory_reads_its_json_list() {
        let page = parse_directory(DIRECTORY);
        assert_eq!(page.animes.len(), 2);
        let first = &page.animes[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/one-piece/", "One Piece"));
        assert_eq!(first.author.as_deref(), Some("Toei Animation"));
        assert!(page.has_next_page);
        assert!(parse_directory("<html></html>").animes.is_empty());
    }

    #[test]
    fn a_search_reads_cards_or_the_directory_it_landed_on() {
        let page = parse_search(SEARCH, "https://jkanime.net/buscar/naruto");
        assert_eq!(page.animes.len(), 2);
        assert_eq!(page.animes[0].url, "/naruto/");
        assert_eq!(page.animes[0].thumbnail_url.as_deref(), Some("https://cdn.jkdesu.com/assets/images/animes/image/naruto.jpg"));
        assert!(!page.has_next_page);
        assert_eq!(parse_search(DIRECTORY, "https://jkanime.net/directorio?q=x").animes.len(), 2);
        assert_eq!(search_url(" one piece "), "https://jkanime.net/buscar/one%20piece");
    }

    #[test]
    fn details_carry_title_cover_genres_and_status() {
        let anime = parse_details(ANIME);
        assert_eq!(anime.title, "One Piece");
        assert_eq!(anime.thumbnail_url.as_deref(), Some("https://cdn.jkdesu.com/assets/images/animes/image/one-piece.jpg"));
        assert_eq!(anime.genre.as_deref(), Some("Acción, Aventura"));
        assert!(anime.description.unwrap().starts_with("Monkey D. Luffy"));
        assert_eq!(anime.status, status::ONGOING);
    }

    #[test]
    fn episodes_come_from_the_api_with_the_page_address() {
        let keys = episode_keys(ANIME).unwrap();
        assert_eq!((keys.anime_id.as_str(), keys.token.as_str(), keys.base.as_str()), ("21", "tok123", "https://jkanime.net/one-piece/"));
        let (episodes, more) = parse_episodes(EPISODES, &keys.base).unwrap();
        assert!(more);
        assert_eq!(episodes.len(), 2);
        assert_eq!((episodes[0].url.as_str(), episodes[0].name.as_str(), episodes[0].episode_number), ("/one-piece/1/", "Episodio 1", 1.0));
        assert_eq!(episodes[0].date_upload, 940_896_000_000); // 1999-10-26 00:00:00
    }

    #[test]
    fn an_episode_lists_its_servers_decoded() {
        let servers = parse_servers(EPISODE);
        assert_eq!(servers.len(), 3);
        assert_eq!(servers[0], EpisodeServer { url: "https://ok.ru/videoembed/123".into(), name: "Okru".into(), lang: "[JAP]" });
        assert_eq!(OwnPlayer::of(&servers[1].url), Some(OwnPlayer::Desu));
        assert_eq!(servers[2].lang, "[LAT]");
        assert_eq!(OwnPlayer::of("https://jkanime.net/um2.php?e=x"), Some(OwnPlayer::Nozomi));
        assert_eq!(OwnPlayer::of("https://jkanime.net/stream/jkmedia/x"), Some(OwnPlayer::Desuka));
    }

    #[test]
    fn player_pages_give_their_video() {
        let desu = "<script>var parts = { swarmId: 'x', url: 'https://cdn.example/v.m3u8', }</script>";
        assert_eq!(script_url(desu, "var parts = {").as_deref(), Some("https://cdn.example/v.m3u8"));
        assert_eq!(script_url(desu, "new DPlayer({"), None);
        assert_eq!(nozomi_key(r#"<form><input type="hidden" name="data" value="k1"></form>"#).as_deref(), Some("k1"));
        assert_eq!(nozomi_file(r#"{"file":"https://cdn.example/n.mp4"}"#).as_deref(), Some("https://cdn.example/n.mp4"));
        assert_eq!(nozomi_file(r#"{"file":null}"#), None);
    }
}
