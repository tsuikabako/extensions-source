//! The MangaGoku site (<https://www.mangagoku.com>) as pure functions: URLs and one parser per page. MangaGoku is the
//! Mangakakalot catalogue under a new name and a new layout (November 2026): the same titles, addresses, chapter API and
//! image servers. The selectors follow the keiyoushi MangaBox base's "modern" layout (Apache-2.0). Without the genre,
//! status and sort filters, the alternative CDNs and the option that merges strip images.

use kuro_guest::Res;
use kuro_guest::html::{ElementRef, Html, own_text, sel, text_of};
use kuro_guest::wire::{Chapter, Manga, MangasPage};
use serde::Deserialize;
use url::Url;

pub const BASE: &str = "https://www.mangagoku.com";
pub const HOST: &str = "www.mangagoku.com";

// ================================ URLs =================================

pub fn popular_url(page: i32) -> String {
    format!("{BASE}/manga-list/hot-manga?page={}", page.max(1))
}

pub fn latest_url(page: i32) -> String {
    format!("{BASE}/manga-list/latest-manga?page={}", page.max(1))
}

/// A title search: the query as typed, in `q`.
pub fn search_url(page: i32, query: &str) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("{BASE}/search?q={encoded}&page={}", page.max(1))
}

pub fn chapters_url(manga_url: &str) -> Res<String> {
    Ok(format!("{BASE}/api/manga/{}/chapters?limit=-1", slug_of(manga_url)?))
}

/// A page of the site from a stored address (`/manga/...`), refused when it is another site's.
pub fn page(path: &str) -> Res<String> {
    let url = Url::parse(BASE).and_then(|base| base.join(path)).map_err(|e| format!("invalid address {path}: {e}"))?;
    if url.host_str() != Some(HOST) {
        return Err(format!("not a MangaGoku address: {path}"));
    }
    Ok(url.to_string())
}

/// The slug in `/manga/<slug>` (or `/manga/<slug>/<chapter>`).
fn slug_of(manga_url: &str) -> Res<&str> {
    let slug = manga_url.strip_prefix("/manga/").unwrap_or(manga_url).split(['/', '?']).next().unwrap_or_default();
    let ok = !slug.is_empty() && slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok { Ok(slug) } else { Err(format!("not a MangaGoku manga: {manga_url}")) }
}

// =============================== Parsers ===============================

/// The path of an address on the site (`/manga/naruto`).
fn path_of(href: &str) -> String {
    Url::parse(BASE).and_then(|base| base.join(href)).map(|u| u.path().to_owned()).unwrap_or_else(|_| href.to_owned())
}

fn absolute(src: &str) -> String {
    Url::parse(BASE).and_then(|base| base.join(src)).map(|u| u.to_string()).unwrap_or_else(|_| src.to_owned())
}

/// A card of a listing: the link and title in its `h3`, the cover in its image.
fn card(element: ElementRef) -> Option<Manga> {
    let link = element.select(&sel("h3 a")).next()?;
    let title = text_of(link);
    if title.is_empty() {
        return None;
    }
    let cover = element.select(&sel("img")).next().and_then(|i| i.value().attr("src")).map(absolute);
    Some(Manga { url: path_of(link.value().attr("href")?), title, thumbnail_url: cover, ..Default::default() })
}

/// A hot, latest or search listing: the cards of its grid, more when the pager has a "Next page" link.
pub fn parse_list(html: &str) -> MangasPage {
    let doc = Html::parse_document(html);
    let mangas = doc.select(&sel("div.grid > div.group")).filter_map(card).collect();
    let has_next_page = doc.select(&sel(r#"nav[aria-label="Pagination"] a[aria-label="Next page"][href]"#)).next().is_some();
    MangasPage { mangas, has_next_page }
}

/// The value next to a label of the Details box (`<span>Status</span><span>Ongoing</span>`).
fn detail(doc: &Html, label: &str) -> Option<String> {
    doc.select(&sel("aside span"))
        .find(|span| own_text(*span).eq_ignore_ascii_case(label))
        .and_then(|span| span.next_siblings().find_map(ElementRef::wrap))
        .map(text_of)
        .filter(|v| !v.is_empty())
}

/// The block that holds the title and its genre chips: the nearest parent of the `h1` with a genre link (the menu's
/// "Genres" link is elsewhere).
fn info_block(h1: ElementRef) -> Option<ElementRef> {
    h1.ancestors().filter_map(ElementRef::wrap).find(|e| e.select(&sel(r#"a[href^="/genre/"]"#)).next().is_some())
}

pub fn parse_details(html: &str) -> Res<Manga> {
    let doc = Html::parse_document(html);
    let h1 = doc.select(&sel("h1")).next().ok_or("the manga page has no title")?;
    let title = text_of(h1);
    let status = match detail(&doc, "Status").as_deref() {
        Some(s) if s.contains("Ongoing") => 1,
        Some(s) if s.contains("Completed") => 2,
        _ => 0,
    };
    let genres: Vec<String> = info_block(h1).map(|b| b.select(&sel(r#"a[href^="/genre/"]"#)).map(text_of).collect()).unwrap_or_default();
    // the summary keeps its paragraphs: each run of whitespace becomes one space, each blank line a paragraph break
    let description = doc
        .select(&sel("div.prose-summary"))
        .next()
        .map(|d| d.text().collect::<String>().split("\n\n").map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n"))
        .filter(|d| !d.is_empty());
    let cover = h1.ancestors().filter_map(ElementRef::wrap).find_map(|e| e.select(&sel("img")).next()).and_then(|i| i.value().attr("src")).map(absolute);
    Ok(Manga {
        title,
        author: detail(&doc, "Author").filter(|a| a != "Unknown"),
        description,
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status,
        thumbnail_url: cover,
        ..Default::default()
    })
}

#[derive(Deserialize)]
struct ChaptersAnswer {
    #[serde(default)]
    success: bool,
    data: Option<ChaptersData>,
}

#[derive(Deserialize)]
struct ChaptersData {
    #[serde(default)]
    chapters: Vec<ApiChapter>,
}

#[derive(Deserialize)]
struct ApiChapter {
    chapter_name: Option<String>,
    chapter_slug: Option<String>,
    chapter_num: Option<f32>,
    updated_at: Option<String>,
}

/// The chapters of the API answer, in the order it gives (newest first).
pub fn parse_chapters(body: &str, manga_url: &str) -> Res<Vec<Chapter>> {
    let slug = slug_of(manga_url)?;
    let answer: ChaptersAnswer = serde_json::from_str(body).map_err(|e| format!("unexpected answer: {e}"))?;
    if !answer.success {
        return Ok(Vec::new());
    }
    Ok(answer
        .data
        .map(|d| d.chapters)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| {
            let chapter_slug = c.chapter_slug?;
            Some(Chapter {
                url: format!("/manga/{slug}/{chapter_slug}"),
                name: c.chapter_name.unwrap_or_else(|| "Chapter".to_owned()),
                date_upload: c.updated_at.as_deref().map_or(0, millis),
                chapter_number: c.chapter_num.unwrap_or(0.0),
                scanlator: Some("mangagoku.com".to_owned()),
            })
        })
        .collect())
}

/// A UTC time as the API writes it (`2025-12-22 19:57:22`, or ISO 8601 `2025-12-22T19:57:22.000000Z`) as epoch
/// milliseconds; 0 when it does not read.
pub fn millis(text: &str) -> i64 {
    let parse = || -> Option<i64> {
        let (date, rest) = text.split_once(['T', ' '])?;
        let d: Vec<i64> = date.split('-').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        let [y, m, day] = d.as_slice().try_into().ok()?;
        let t: Vec<i64> = rest.get(..8)?.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        let [hh, mm, ss] = t.as_slice().try_into().ok()?;
        let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * m + 2) / 5 + day - 1;
        let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
        Some(((days * 24 + hh) * 3600 + mm * 60 + ss) * 1000)
    };
    parse().unwrap_or(0)
}

/// The page images of a chapter, in order: the image of each `div[data-page]` of the reader.
pub fn parse_pages(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    doc.select(&sel("div[data-page] > img")).filter_map(|i| i.value().attr("src")).map(absolute).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const POPULAR: &str = include_str!("../fixtures/popular.html");
    const LATEST: &str = include_str!("../fixtures/latest.html");
    const SEARCH: &str = include_str!("../fixtures/search.html");
    const MANGA: &str = include_str!("../fixtures/manga.html");
    const CHAPTERS: &str = include_str!("../fixtures/chapters.json");
    const CHAPTER: &str = include_str!("../fixtures/chapter.html");

    #[test]
    fn the_hot_list_has_cards_and_a_next_page() {
        let page = parse_list(POPULAR);
        assert_eq!(page.mangas.len(), 5);
        assert!(page.has_next_page);
        assert_eq!((page.mangas[0].url.as_str(), page.mangas[0].title.as_str()), ("/manga/martial-peak", "Martial Peak"));
        assert_eq!(page.mangas[0].thumbnail_url.as_deref(), Some("https://img-r1.2xstorage.com/thumb/martial-peak.webp"));
        assert_eq!(parse_list(LATEST).mangas.len(), 4);
    }

    #[test]
    fn a_search_lists_the_grid() {
        let page = parse_list(SEARCH);
        assert!(page.mangas.iter().any(|m| (m.url.as_str(), m.title.as_str()) == ("/manga/naruto", "Naruto")), "{:?}", page.mangas);
        assert_eq!(search_url(2, " One Piece: Gold! "), "https://www.mangagoku.com/search?q=One+Piece%3A+Gold%21&page=2");
    }

    #[test]
    fn only_a_next_page_link_means_more() {
        let more = r#"<nav aria-label="Pagination"><span aria-current="page">1</span><a href="/x?page=2">2</a><a aria-label="Next page" href="/x?page=2">›</a></nav>"#;
        let last = r#"<nav aria-label="Pagination"><a aria-label="Previous page" href="/x?page=8">‹</a><span aria-current="page">9</span></nav>"#;
        assert!(parse_list(more).has_next_page);
        assert!(!parse_list(last).has_next_page);
    }

    #[test]
    fn the_manga_page_has_title_author_status_genres_summary_and_cover() {
        let manga = parse_details(MANGA).unwrap();
        assert_eq!(manga.title, "Martial Peak");
        assert_eq!((manga.status, manga.author.as_deref()), (1, Some("Momo (Ii)")));
        assert_eq!(manga.genre.as_deref(), Some("Fantasy, Action, Adventure, Historical, Martial arts"), "the menu's Genres link stays out");
        let description = manga.description.unwrap();
        assert!(description.contains("\n\nThe journey to the martial peak is a lonely"), "{description}");
        assert_eq!(manga.thumbnail_url.as_deref(), Some("https://img-r1.2xstorage.com/thumb/martial-peak.webp"));
    }

    #[test]
    fn the_chapters_come_from_the_api_with_dates() {
        let chapters = parse_chapters(CHAPTERS, "/manga/martial-peak").unwrap();
        assert_eq!(chapters.len(), 5);
        assert_eq!(chapters[0].name, "Chapter 3862");
        assert_eq!(chapters[0].url, "/manga/martial-peak/chapter-3862");
        assert_eq!(chapters[0].chapter_number, 3862.0);
        assert_eq!(chapters[0].date_upload, millis("2025-12-22T19:57:22.000000Z"));
        assert_eq!(millis("1970-01-02 00:00:01"), 86_401_000);
        assert_eq!(chapters_url("/manga/martial-peak").unwrap(), "https://www.mangagoku.com/api/manga/martial-peak/chapters?limit=-1");
        assert!(parse_chapters(r#"{"success":false}"#, "/manga/x").unwrap().is_empty());
    }

    #[test]
    fn the_pages_are_the_readers_images_in_order() {
        let pages = parse_pages(CHAPTER);
        assert_eq!(pages.len(), 6);
        assert_eq!(pages[0], "https://img-r1.2xstorage.com/martial-peak/3862/0.webp");
        assert_eq!(pages[5], "https://img-r1.2xstorage.com/martial-peak/3862/5.webp");
    }

    #[test]
    fn only_addresses_of_the_site_are_fetched() {
        assert_eq!(page("/manga/naruto").unwrap(), "https://www.mangagoku.com/manga/naruto");
        assert!(page("https://evil.test/x").is_err());
        assert!(chapters_url("/manga/../x").is_err());
    }
}
