//! The MangaTuk API (<https://api.mangatuk.com/api/catalog/>) as pure functions: URLs and one parser per answer. Ported
//! from the keiyoushi `ar/mangatuk` extension (Apache-2.0).
//!
//! A title is stored as its path on the site, `/series/<slug>`; a chapter as `/series/<slug>/<chapter slug>#<chapter id>`
//! (the id for its pages). The Kotlin's stored forms, the bare series and chapter ids, are read too.

use chrono::{DateTime, NaiveDateTime};
use kuro_guest::Res;
use kuro_guest::wire::{Chapter, Manga, MangasPage, status};
use serde::Deserialize;
use serde::de::DeserializeOwned;

const API: &str = "https://api.mangatuk.com/api";
const PAGE_SIZE: i32 = 30;

// ================================ URLs =================================

fn encode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

/// The catalog in an order (`popular`, `latest_updates`), with the search text when there is one. `page` starts at 1.
pub fn list_url(page: i32, sort: &str, text: &str) -> String {
    let q = if text.trim().is_empty() { String::new() } else { format!("q={}&", encode(text.trim())) };
    format!("{API}/catalog/search?{q}sort={sort}&limit={PAGE_SIZE}&offset={}", (page - 1) * PAGE_SIZE)
}

/// A bare id (the Kotlin's stored form): letters, digits and `_`.
fn is_id(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The series with its chapters: by slug from `/series/<slug>`, by id from a bare id.
pub fn series_url(manga_url: &str) -> Res<String> {
    if is_id(manga_url) {
        return Ok(format!("{API}/catalog/series/{manga_url}"));
    }
    match manga_url.trim_end_matches('/').strip_prefix("/series/") {
        Some(slug) if !slug.is_empty() && !slug.contains('/') => Ok(format!("{API}/catalog/series/by-slug/{}", encode(slug))),
        _ => Err(format!("not a MangaTuk title: {manga_url}")),
    }
}

pub fn pages_url(chapter_url: &str) -> Res<String> {
    let id = chapter_url.rsplit_once('#').map_or(chapter_url, |(_, id)| id);
    if is_id(id) { Ok(format!("{API}/catalog/chapters/{id}/pages")) } else { Err(format!("not a MangaTuk chapter: {chapter_url}")) }
}

// =============================== Parsers ===============================

fn parse<T: DeserializeOwned>(text: &str) -> Res<T> {
    serde_json::from_str(text).map_err(|e| format!("unexpected answer from MangaTuk: {e}"))
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoinAccess {
    #[serde(default)]
    locked: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChapterDto {
    id: String,
    number: String,
    title: Option<String>,
    slug: String,
    published_at: Option<String>,
    coin_access: Option<CoinAccess>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeriesDto {
    slug: String,
    title: String,
    cover_image: Option<String>,
    associated_names: Option<String>,
    description: Option<String>,
    author: Option<String>,
    status: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    genres: Vec<Named>,
    #[serde(default)]
    chapters: Vec<ChapterDto>,
}

#[derive(Deserialize)]
struct ListDto {
    data: Vec<SeriesDto>,
    total: i32,
}

pub fn parse_list(text: &str, page: i32) -> Res<MangasPage> {
    let list: ListDto = parse(text)?;
    let mangas = list.data.into_iter().map(|s| Manga { url: format!("/series/{}", s.slug), title: s.title, thumbnail_url: s.cover_image, ..Default::default() }).collect();
    Ok(MangasPage { mangas, has_next_page: page * PAGE_SIZE < list.total })
}

pub fn parse_details(text: &str) -> Res<Manga> {
    let s: SeriesDto = parse(text)?;
    let genres: Vec<String> = s.kind.into_iter().chain(s.genres.into_iter().map(|g| g.name)).collect();
    // "Other names:" in the Kotlin's words
    let other = s.associated_names.filter(|n| !n.trim().is_empty()).map(|n| format!("أسماء أخرى:\n{n}"));
    let description = [s.description, other].into_iter().flatten().collect::<Vec<_>>().join("\n\n");
    let status = match s.status.as_deref() {
        Some("ongoing") => status::ONGOING,
        Some("completed") => status::COMPLETED,
        Some("hiatus") => status::ON_HIATUS,
        Some("cancelled") => status::CANCELLED,
        _ => status::UNKNOWN,
    };
    Ok(Manga {
        url: format!("/series/{}", s.slug),
        title: s.title,
        author: s.author.filter(|a| !a.is_empty()),
        description: Some(description).filter(|d| !d.is_empty()),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status,
        thumbnail_url: s.cover_image,
        ..Default::default()
    })
}

/// "2026-10-17T09:43:43.358Z", or the same with a space and no zone (UTC), as the API writes some.
fn date_of(text: &str) -> i64 {
    DateTime::parse_from_rfc3339(text)
        .map(|d| d.timestamp_millis())
        .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f").map(|d| d.and_utc().timestamp_millis()))
        .unwrap_or(0)
}

/// The free chapters, newest first (the API lists them oldest first). Coin-locked chapters are left out, as in the Kotlin.
pub fn parse_chapters(text: &str) -> Res<Vec<Chapter>> {
    let s: SeriesDto = parse(text)?;
    Ok(s.chapters
        .into_iter()
        .rev()
        .filter(|c| !c.coin_access.as_ref().is_some_and(|a| a.locked))
        .map(|c| {
            let name = match c.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                Some(title) => format!("الفصل {} - {title}", c.number),
                None => format!("الفصل {}", c.number),
            };
            Chapter {
                url: format!("/series/{}/{}#{}", s.slug, c.slug, c.id),
                name,
                date_upload: c.published_at.as_deref().map_or(0, date_of),
                chapter_number: c.number.parse().unwrap_or(-1.0),
                scanlator: None,
            }
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageDto {
    image_url: String,
}

pub fn parse_pages(text: &str) -> Res<Vec<String>> {
    Ok(parse::<Vec<PageDto>>(text)?.into_iter().map(|p| p.image_url).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_follow_the_api() {
        assert_eq!(list_url(1, "popular", ""), "https://api.mangatuk.com/api/catalog/search?sort=popular&limit=30&offset=0");
        assert_eq!(list_url(3, "popular", "solo leveling"), "https://api.mangatuk.com/api/catalog/search?q=solo+leveling&sort=popular&limit=30&offset=60");
        assert_eq!(series_url("/series/a-title").unwrap(), "https://api.mangatuk.com/api/catalog/series/by-slug/a-title");
        assert_eq!(series_url("wpser_1").unwrap(), "https://api.mangatuk.com/api/catalog/series/wpser_1");
        assert!(series_url("https://evil.test/x").is_err());
        assert_eq!(pages_url("/series/a/chapter-2#abc").unwrap(), "https://api.mangatuk.com/api/catalog/chapters/abc/pages");
        assert!(pages_url("/series/a/chapter-2#").is_err());
    }

    #[test]
    fn the_list_pages_by_total() {
        let page = parse_list(include_str!("../fixtures/search.json"), 1).unwrap();
        assert!(page.has_next_page);
        assert!(!parse_list(include_str!("../fixtures/search.json"), 100).unwrap().has_next_page);
        assert!(page.mangas[0].thumbnail_url.as_deref().unwrap().starts_with("https://content.mangatuk.com/"));
    }

    #[test]
    fn a_title_has_its_details() {
        let manga = parse_details(include_str!("../fixtures/series.json")).unwrap();
        assert_eq!(manga.status, status::ONGOING);
        assert!(manga.genre.unwrap().starts_with("manhwa, "));
        assert!(manga.description.unwrap().contains("أسماء أخرى:\nدموع على زهرة ذابلة"));
    }

    #[test]
    fn chapters_are_newest_first_without_locked_ones() {
        let chapters = parse_chapters(include_str!("../fixtures/series.json")).unwrap();
        assert_eq!(chapters.len(), 5);
        assert_eq!((chapters[0].name.as_str(), chapters[0].chapter_number), ("الفصل 123", 123.0));
        assert_eq!(chapters[0].url, "/series/tears-on-a-withered-flower/chapter-123#pbc5nky0kt5fwbseyh8gy3nj");
        assert!(chapters.iter().all(|c| c.chapter_number != 25.0));
        assert_eq!(chapters[0].date_upload, date_of("2026-10-17T09:43:43.358Z"));
        assert_eq!(date_of("2026-10-17 09:43:43.358"), date_of("2026-10-17T09:43:43.358Z"));
    }
}
