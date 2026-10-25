//! The EShadow API (<https://eshadow.net/api/>) as pure functions: URLs and one parser per answer. Ported from the
//! keiyoushi `ar/eshadow` extension (Apache-2.0).
//!
//! A title is stored as `/manga/<slug>#<id>` (its page on the site; the API knows it only by id), a chapter as its page,
//! `/read/<chapter id>`. The Kotlin's stored forms, the bare ids, are read too.

use chrono::DateTime;
use kuro_guest::Res;
use kuro_guest::wire::{Chapter, Manga, MangasPage, status};
use serde::Deserialize;
use serde::de::DeserializeOwned;

const BASE: &str = "https://eshadow.net";

// ================================ URLs =================================

pub fn list_url(page: i32, text: &str) -> String {
    let mut url = format!("{BASE}/api/manga?page={}", page.max(1));
    if !text.trim().is_empty() {
        let q: String = url::form_urlencoded::byte_serialize(text.trim().as_bytes()).collect();
        url.push_str(&format!("&query={q}"));
    }
    url
}

/// An API id: letters, digits and `-`.
fn is_id(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The title with its chapters, from `/manga/<slug>#<id>` or a bare id.
pub fn manga_url(stored: &str) -> Res<String> {
    let id = stored.rsplit_once('#').map_or(stored, |(_, id)| id);
    if is_id(id) { Ok(format!("{BASE}/api/manga/{id}")) } else { Err(format!("not an EShadow title: {stored}")) }
}

/// A chapter with its images, from `/read/<id>` or a bare id.
pub fn pages_url(stored: &str) -> Res<String> {
    let id = stored.strip_prefix("/read/").unwrap_or(stored).trim_end_matches('/');
    if is_id(id) { Ok(format!("{BASE}/api/chapters/{id}")) } else { Err(format!("not an EShadow chapter: {stored}")) }
}

// =============================== Parsers ===============================

fn parse<T: DeserializeOwned>(text: &str) -> Res<T> {
    serde_json::from_str(text).map_err(|e| format!("unexpected answer from EShadow: {e}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChapterDto {
    // the list's titles carry chapters without ids
    #[serde(default)]
    id: String,
    title: Option<String>,
    #[serde(default)]
    number: f32,
    #[serde(default)]
    images: Vec<String>,
    published_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MangaDto {
    id: String,
    slug: String,
    title: String,
    description: Option<String>,
    cover_image: Option<String>,
    author: Option<String>,
    status: Option<String>,
    #[serde(default)]
    chapters: Vec<ChapterDto>,
}

impl MangaDto {
    fn into_manga(self) -> Manga {
        let status = match self.status.as_deref() {
            Some("ONGOING") => status::ONGOING,
            Some("COMPLETED") => status::COMPLETED,
            Some("HIATUS") => status::ON_HIATUS,
            _ => status::UNKNOWN,
        };
        Manga {
            url: format!("/manga/{}#{}", self.slug, self.id),
            title: self.title,
            author: self.author.filter(|a| !a.is_empty()),
            description: self.description,
            status,
            thumbnail_url: self.cover_image,
            ..Default::default()
        }
    }
}

#[derive(Deserialize)]
struct ListDto {
    data: Vec<MangaDto>,
    limit: i32,
    page: i32,
    total: i32,
}

pub fn parse_list(text: &str) -> Res<MangasPage> {
    let list: ListDto = parse(text)?;
    Ok(MangasPage { has_next_page: list.page * list.limit < list.total, mangas: list.data.into_iter().map(MangaDto::into_manga).collect() })
}

pub fn parse_details(text: &str) -> Res<Manga> {
    Ok(parse::<MangaDto>(text)?.into_manga())
}

/// The chapters, highest number first.
pub fn parse_chapters(text: &str) -> Res<Vec<Chapter>> {
    let manga: MangaDto = parse(text)?;
    let mut chapters: Vec<Chapter> = manga
        .chapters
        .into_iter()
        .filter(|c| !c.id.is_empty())
        .map(|c| Chapter {
            url: format!("/read/{}", c.id),
            name: match c.title {
                Some(title) => format!("الفصل {} - {title}", c.number),
                None => format!("الفصل {}", c.number),
            },
            date_upload: c.published_at.and_then(|d| DateTime::parse_from_rfc3339(&d).ok()).map_or(0, |d| d.timestamp_millis()),
            chapter_number: c.number,
            scanlator: None,
        })
        .collect();
    chapters.sort_by(|a, b| b.chapter_number.total_cmp(&a.chapter_number));
    Ok(chapters)
}

#[derive(Deserialize)]
struct ChapterAnswer {
    chapter: ChapterDto,
}

pub fn parse_pages(text: &str) -> Res<Vec<String>> {
    Ok(parse::<ChapterAnswer>(text)?.chapter.images)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_follow_the_api() {
        assert_eq!(list_url(0, ""), "https://eshadow.net/api/manga?page=1");
        assert_eq!(list_url(2, "one piece"), "https://eshadow.net/api/manga?page=2&query=one+piece");
        assert_eq!(manga_url("/manga/kingdom#7297d253-e25d").unwrap(), "https://eshadow.net/api/manga/7297d253-e25d");
        assert!(manga_url("/manga/kingdom#../x").is_err());
        assert_eq!(pages_url("/read/abc").unwrap(), "https://eshadow.net/api/chapters/abc");
    }

    #[test]
    fn a_title_has_its_details_and_chapters() {
        let manga = parse_details(include_str!("../fixtures/manga.json")).unwrap();
        assert_eq!((manga.status, manga.author.is_some()), (status::ONGOING, true));
        let chapters = parse_chapters(include_str!("../fixtures/manga.json")).unwrap();
        assert_eq!(chapters.len(), 8);
        assert_eq!(chapters[0].name, "الفصل 1185 - دعيهم وشأنهم");
        assert_eq!(chapters[7].date_upload, DateTime::parse_from_rfc3339("2026-05-20T11:10:29.248Z").unwrap().timestamp_millis());
    }

    #[test]
    fn the_chapter_answer_holds_its_images() {
        let pages = parse_pages(include_str!("../fixtures/chapter.json")).unwrap();
        assert!(pages[0].ends_with("/chapters/cmpdynfr40003c01s1i5jxb6r/001.webp"), "{}", pages[0]);
    }
}
