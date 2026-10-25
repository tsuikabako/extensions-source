//! The MangaTime tRPC API (<https://mangatime.org/api/trpc/>) as pure functions: URLs and one parser per answer. Ported
//! from the keiyoushi `ar/mangatime` extension (Apache-2.0).
//!
//! A title is stored as `/<type>/<slug>#<series id>` (the Kotlin's form: the slug for the details, the id for the
//! chapters), a chapter as `/<type>/<slug>/chapter/<number>`.

use chrono::DateTime;
use kuro_guest::Res;
use kuro_guest::wire::{Chapter, Manga, MangasPage, status};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Number, Value, json};

pub const BASE: &str = "https://mangatime.org";
const LIMIT: i32 = 24;

// ================================ URLs =================================

/// A batched tRPC query: `?batch=1&input={"0":{"json":<input>}}`.
fn query(endpoint: &str, input: Value) -> String {
    let input = json!({ "0": { "json": input } }).to_string();
    let encoded: String = url::form_urlencoded::byte_serialize(input.as_bytes()).collect();
    format!("{BASE}/api/trpc/{endpoint}?batch=1&input={encoded}")
}

/// The catalog in an order (`popularity`, `recent`), with the search text when there is one: the API refuses a null
/// `query`, so it is left out when empty.
pub fn list_url(page: i32, sort_by: &str, text: &str) -> String {
    let mut input = json!({ "page": page.max(1), "limit": LIMIT, "sortBy": sort_by, "sortOrder": "desc" });
    if !text.trim().is_empty() {
        input["query"] = json!(text.trim());
    }
    query("search.searchSeries", input)
}

/// (type, slug, series id) of a stored title.
fn title_parts(url: &str) -> Res<(&str, &str, &str)> {
    let (path, id) = url.split_once('#').ok_or_else(|| format!("not a MangaTime title: {url}"))?;
    let mut segments = path.trim_matches('/').split('/');
    match (segments.next(), segments.next(), segments.next()) {
        (Some(kind), Some(slug), None) if !kind.is_empty() && !slug.is_empty() && !id.is_empty() => Ok((kind, slug, id)),
        _ => Err(format!("not a MangaTime title: {url}")),
    }
}

pub fn details_url(manga_url: &str) -> Res<String> {
    let (_, slug, _) = title_parts(manga_url)?;
    Ok(query("content.getSeriesBySlug", json!({ "slug": slug })))
}

pub fn chapters_url(manga_url: &str) -> Res<String> {
    let (_, _, id) = title_parts(manga_url)?;
    Ok(query("content.getChapters", json!({ "seriesId": id, "limit": -1 })))
}

/// The pages of `/<type>/<slug>/chapter/<number>`; the number stays as written (`364.5`).
pub fn pages_url(chapter_url: &str) -> Res<String> {
    let segments: Vec<&str> = chapter_url.trim_matches('/').split('/').collect();
    let number = match segments.as_slice() {
        [_, _, "chapter", number] => number.parse::<Number>().ok(),
        _ => None,
    };
    let number = number.ok_or_else(|| format!("not a MangaTime chapter: {chapter_url}"))?;
    Ok(query("content.getChapterPages", json!({ "seriesSlug": segments[1], "chapterNumber": number })))
}

/// An image address from the API, which gives some as paths on the site.
fn image(url: &str) -> String {
    let url = url.replace(' ', "%20");
    if url.starts_with("http") { url } else { format!("{BASE}{url}") }
}

// =============================== Parsers ===============================

/// The `json` of the first answer of a batch.
fn answer<T: DeserializeOwned>(text: &str) -> Res<T> {
    let mut batch: Vec<Value> = serde_json::from_str(text).map_err(|e| format!("unexpected answer from MangaTime: {e}"))?;
    let data = batch.first_mut().map(|first| first["result"]["data"]["json"].take()).unwrap_or(Value::Null);
    T::deserialize(data).map_err(|e| format!("unexpected answer from MangaTime: {e}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeriesCard {
    id: String,
    title: String,
    slug: String,
    cover_url: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListDto {
    results: Vec<SeriesCard>,
    has_more: bool,
}

pub fn parse_list(text: &str) -> Res<MangasPage> {
    let list: ListDto = answer(text)?;
    let mangas = list
        .results
        .into_iter()
        .map(|s| Manga { url: format!("/{}/{}#{}", s.kind, s.slug, s.id), title: s.title, thumbnail_url: Some(image(&s.cover_url)), ..Default::default() })
        .collect();
    Ok(MangasPage { mangas, has_next_page: list.has_more })
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeriesDto {
    title: String,
    cover_url: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    genres: Vec<Named>,
    description: Option<String>,
    status: Option<String>,
    author: Option<String>,
    artist: Option<String>,
}

pub fn parse_details(text: &str) -> Res<Manga> {
    let series: SeriesDto = answer(text)?;
    let genres: Vec<String> = series.genres.into_iter().map(|g| g.name).chain([series.kind]).filter(|g| !g.trim().is_empty()).collect();
    let status = match series.status.as_deref().map(str::to_lowercase).as_deref() {
        Some("ongoing") => status::ONGOING,
        Some("completed") => status::COMPLETED,
        Some("hiatus") => status::ON_HIATUS,
        Some("cancelled") => status::CANCELLED,
        _ => status::UNKNOWN,
    };
    Ok(Manga {
        url: String::new(),
        title: series.title,
        author: series.author.filter(|a| !a.is_empty()),
        artist: series.artist.filter(|a| !a.is_empty()),
        description: series.description,
        // the Arabic comma inside a genre name reads as a separator, as in the Kotlin
        genre: Some(genres.join(", ").replace('\u{060c}', ",")).filter(|g| !g.is_empty()),
        status,
        thumbnail_url: Some(image(&series.cover_url)),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChapterDto {
    number: Number,
    title: Option<String>,
    published_at: Option<String>,
}

#[derive(Deserialize)]
struct ChaptersDto {
    chapters: Vec<ChapterDto>,
}

/// The chapters of a title, newest first as the API lists them. A chapter without a title of its own is "Chapter N".
pub fn parse_chapters(text: &str, manga_url: &str) -> Res<Vec<Chapter>> {
    let (kind, slug, _) = title_parts(manga_url)?;
    let list: ChaptersDto = answer(text)?;
    Ok(list
        .chapters
        .into_iter()
        .map(|c| {
            let number = c.number.to_string();
            let name = match c.title.filter(|t| !t.trim().is_empty()) {
                Some(title) if title.contains(&number) => title,
                Some(title) => format!("Chapter {number} - {title}"),
                None => format!("Chapter {number}"),
            };
            Chapter {
                url: format!("/{kind}/{slug}/chapter/{number}"),
                name,
                date_upload: c.published_at.and_then(|d| DateTime::parse_from_rfc3339(&d).ok()).map_or(0, |d| d.timestamp_millis()),
                chapter_number: c.number.as_f64().unwrap_or(-1.0) as f32,
                scanlator: None,
            }
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PagesDto {
    pages: Vec<String>,
    is_unlocked: bool,
}

pub fn parse_pages(text: &str) -> Res<Vec<String>> {
    let chapter: PagesDto = answer(text)?;
    if !chapter.is_unlocked {
        return Err("this chapter is locked on MangaTime".to_owned());
    }
    Ok(chapter.pages.iter().map(|p| image(p)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_are_batched_trpc_inputs() {
        let popular = list_url(1, "popularity", "");
        assert!(popular.starts_with("https://mangatime.org/api/trpc/search.searchSeries?batch=1&input=%7B%220%22%3A%7B%22json%22%3A"), "{popular}");
        assert!(!popular.contains("query"));
        assert!(list_url(2, "popularity", " blue ").contains("%22query%22%3A%22blue%22"));
        assert!(pages_url("/manga/blue-lock/chapter/364.5").unwrap().contains("%22chapterNumber%22%3A364.5"));
        assert!(pages_url("/manga/blue-lock/chapter/365").unwrap().contains("%22chapterNumber%22%3A365%"));
        assert!(pages_url("/manga/blue-lock").is_err());
        assert!(details_url("/manga/blue-lock").is_err());
        assert_eq!(image("/uploads/a b.webp"), "https://mangatime.org/uploads/a%20b.webp");
    }

    #[test]
    fn the_list_has_cards_and_a_next_page() {
        let page = parse_list(include_str!("../fixtures/popular.json")).unwrap();
        assert!(page.has_next_page);
        assert_eq!(page.mangas[0].title, "بلو لوك");
        assert_eq!(page.mangas[0].thumbnail_url.as_deref(), Some("https://mangatime.org/uploads/series/697df7820c5d340ac154519f-be10685810f1255a.webp"));
    }

    #[test]
    fn a_title_has_its_details() {
        let manga = parse_details(include_str!("../fixtures/series.json")).unwrap();
        assert_eq!(manga.status, status::ONGOING);
        assert_eq!(manga.genre.as_deref(), Some("رياضة, إثارة, دراما, نفسي, شونين, manga"));
        assert!(manga.author.unwrap().starts_with("Muneyuki Kaneshiro"));
        assert!(manga.description.unwrap().starts_with("بعد إخفاق"));
    }

    #[test]
    fn chapters_have_names_numbers_and_dates() {
        let chapters = parse_chapters(include_str!("../fixtures/chapters.json"), "/manga/blue-lock#697df7820c5d340ac154519f").unwrap();
        assert_eq!((chapters[0].name.as_str(), chapters[0].chapter_number), ("Chapter 365", 365.0));
        assert_eq!(chapters[0].date_upload, DateTime::parse_from_rfc3339("2026-10-20T16:16:52.486Z").unwrap().timestamp_millis());
        assert_eq!(chapters[1].url, "/manga/blue-lock/chapter/364.5");
        assert_eq!(chapters[1].name, "Chapter 364.5 - نقطة اللاعودة (الجزء الثاني)");
    }

    #[test]
    fn pages_are_the_image_list_unless_locked() {
        let pages = parse_pages(include_str!("../fixtures/pages.json")).unwrap();
        assert!(pages.iter().all(|p| p.starts_with("https://mangatime.org/uploads/chapter/")));
        assert!(parse_pages(r#"[{"result":{"data":{"json":{"pages":[],"isUnlocked":false}}}}]"#).is_err());
    }
}
