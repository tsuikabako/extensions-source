//! The MangaBuff site (<https://mangabuff.ru>) as pure functions: URLs and one parser per page. Ported from the
//! keiyoushi `ru/mangabuff` extension (Apache-2.0). Without the filters, the address deep link and the GIF fix-up.

use chrono::NaiveDate;
use kuro_guest::Res;
use kuro_guest::html::{ElementRef, Html, Selector, attr, first, sel, text_of};
use kuro_guest::wire::{Chapter, Manga, MangasPage, status};
use serde::Deserialize;
use url::Url;

pub const BASE: &str = "https://mangabuff.ru";
const HOST: &str = "mangabuff.ru";

// ================================ URLs =================================

/// The catalog in one order: `real_views` (popular) or `updated_at` (latest).
pub fn catalog_url(page: i32, sort: &str) -> String {
    let mut url = format!("{BASE}/manga?sort={sort}");
    if page > 1 {
        url.push_str(&format!("&page={page}"));
    }
    url
}

/// The site's search. Like the Kotlin, a text shorter than 3 characters is refused before any request.
pub fn search_url(page: i32, query: &str) -> Res<String> {
    let query = query.trim();
    if query.chars().count() < 3 {
        return Err("Запрос должен содержать не менее 3 символов. / The query must contain at least 3 characters".into());
    }
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("type", "manga").append_pair("q", query);
    if page > 1 {
        params.append_pair("page", &page.to_string());
    }
    Ok(format!("{BASE}/search?{}", params.finish()))
}

pub fn chapters_load_url() -> String {
    format!("{BASE}/chapters/load")
}

/// A page of the site from a stored address (`/manga/<slug>`, `/manga/<slug>/<volume>/<chapter>`), refused when it is
/// another site's.
pub fn page(path: &str) -> Res<String> {
    let url = absolute(path).ok_or_else(|| format!("invalid address {path}"))?;
    if url.host_str() != Some(HOST) {
        return Err(format!("not a MangaBuff address: {path}"));
    }
    Ok(url.into())
}

fn absolute(href: &str) -> Option<Url> {
    Url::parse(BASE).ok()?.join(href.trim()).ok()
}

// =============================== Parsers ===============================

/// A list page (catalog or search): the cards and whether a "Вперёд" (next) button follows.
pub fn parse_list(html: &str) -> MangasPage {
    let doc = Html::parse_document(html);
    let mangas = doc
        .select(&sel(".cards .cards__item"))
        .filter_map(|card| {
            // The card is the link itself; Jsoup's `selectFirst("a")` finds it, `scraper` does not look at the element.
            let href = card.value().attr("href").map(str::to_owned).or_else(|| attr(card, "a", "href"))?;
            let url = absolute(&href)?;
            let slug = url.path_segments()?.next_back()?.to_owned();
            Some(Manga {
                url: url.path().to_owned(),
                title: first(card, ".cards__name").map(text_of)?,
                thumbnail_url: Some(format!("{BASE}/img/manga/posters/{slug}.jpg")),
                ..Default::default()
            })
        })
        .collect();
    let has_next_page = doc.select(&sel(".pagination .pagination__button a")).any(|a| text_of(a).contains("Вперёд"));
    MangasPage { mangas, has_next_page }
}

/// The details of a title page, as the Kotlin's `parseMangaDetails`.
pub fn parse_details(html: &str, manga_url: &str) -> Res<Manga> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let title = first(root, "h1, .manga__name, .manga-mobile__name").map(text_of).ok_or("MangaBuff: no title on the page")?;

    let mut parts = Vec::new();
    if let Some(text) = first(root, ".manga__description").map(text_of) {
        parts.push(text);
    }
    if let Some(rating) = first(root, ".manga__rating").and_then(|e| text_of(e).parse::<f64>().ok()) {
        parts.push(format!("Рейтинг: {:.0}%", rating * 10.0));
    }
    if let Some(views) = first(root, ".manga__views").and_then(|e| text_of(e).replace(' ', "").parse::<i64>().ok()) {
        parts.push(format!("Просмотров: {}", grouped(views)));
    }
    if let Some(favs) = first(root, ".manga").and_then(|e| e.value().attr("data-fav-count")).and_then(|v| v.trim().parse::<i64>().ok()) {
        parts.push(format!("Избранное: {}", grouped(favs)));
    }
    let alt: Vec<String> = all_text(&doc, ".manga__name-alt > span, .manga-mobile__name-alt > span");
    if !alt.is_empty() {
        parts.push(format!("Альтернативные названия:\n{}", alt.iter().map(|name| format!("• {name}")).collect::<Vec<_>>().join("\n")));
    }

    let mut genres = all_text(&doc, ".manga__middle-links > a:not(:last-child)");
    genres.extend(all_text(&doc, ".manga-mobile__info > a:not(:last-child)"));
    genres.extend(all_text(&doc, ".tags > .tags__item"));

    let status_text = all_text(&doc, ".manga__middle-links > a:last-child, .manga-mobile__info > a:last-child").join(" ");

    Ok(Manga {
        url: manga_url.to_owned(),
        title,
        description: Some(parts.join("\n\n")).filter(|d| !d.is_empty()),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status: status_of(&status_text),
        thumbnail_url: first(root, ".manga__img img, img.manga-mobile__image")
            .and_then(|img| img.value().attr("src"))
            .and_then(absolute)
            .map(String::from),
        ..Default::default()
    })
}

/// What a title page needs for the rest of its chapters: the manga id and the page's CSRF token, when the page shows
/// only its first 100 chapters (`.load-chapters-trigger`). `None` when the page lists them all.
pub fn more_chapters(html: &str) -> Res<Option<(String, String)>> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    if first(root, ".load-chapters-trigger").is_none() {
        return Ok(None);
    }
    let id = attr(root, ".manga", "data-id").ok_or("Не удалось найти ID манги")?;
    let token = attr(root, "head meta[name*=csrf-token]", "content").ok_or("Unable to find CSRF token")?;
    Ok(Some((id, token)))
}

/// The chapters of a title page (or of the `chapters/load` fragment), newest first as the site lists them.
pub fn parse_chapters(html: &str) -> Vec<Chapter> {
    Html::parse_document(html).select(&sel("a.chapters__item")).filter_map(chapter_of).collect()
}

#[derive(Deserialize)]
struct Load {
    content: String,
}

/// The chapters of a `chapters/load` answer: JSON whose `content` is the HTML of the older chapters.
pub fn parse_load(json: &str) -> Res<Vec<Chapter>> {
    let load: Load = serde_json::from_str(json).map_err(|e| format!("MangaBuff chapter list: {e}"))?;
    Ok(parse_chapters(&load.content))
}

fn chapter_of(a: ElementRef) -> Option<Chapter> {
    let url = absolute(a.value().attr("href")?)?;
    let name = a.select(&sel(".chapters__volume, .chapters__value, .chapters__name")).map(text_of).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
    let value = first(a, ".chapters__value").map(text_of).unwrap_or_default();
    Some(Chapter {
        url: url.path().to_owned(),
        name,
        date_upload: first(a, ".chapters__add-date").map_or(0, |d| date_of(&text_of(d))),
        chapter_number: value.split_once(' ').and_then(|(_, n)| n.trim().parse().ok()).unwrap_or(-1.0),
        ..Default::default()
    })
}

/// The page images of a reader page, in order: `data-src` for the lazy ones, `src` otherwise.
pub fn parse_pages(html: &str) -> Vec<String> {
    Html::parse_document(html)
        .select(&sel(".reader__pages img"))
        .filter_map(|img| img.value().attr("data-src").or_else(|| img.value().attr("src")))
        .filter_map(absolute)
        .map(String::from)
        .collect()
}

// ================================ Helpers ==============================

fn all_text(doc: &Html, css: &str) -> Vec<String> {
    let selector: Selector = sel(css);
    doc.select(&selector).map(text_of).filter(|t| !t.is_empty()).collect()
}

/// A `dd.MM.yyyy` date as milliseconds since 1970 (UTC), 0 when it does not parse.
fn date_of(text: &str) -> i64 {
    NaiveDate::parse_from_str(text.trim(), "%d.%m.%Y").ok().and_then(|d| d.and_hms_opt(0, 0, 0)).map_or(0, |t| t.and_utc().timestamp_millis())
}

fn status_of(text: &str) -> i32 {
    match text.trim().to_lowercase().as_str() {
        "завершен" => status::COMPLETED,
        "продолжается" => status::ONGOING,
        "заморожен" => status::ON_HIATUS,
        "заброшен" => status::CANCELLED,
        _ => status::UNKNOWN,
    }
}

/// A number with its thousands apart, as Java's `%,d` in the Russian locale (a no-break space).
fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('\u{a0}');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DETAILS: &str = include_str!("../fixtures/details.html");

    #[test]
    fn a_catalog_page_reads_cards_with_poster_covers_and_a_next_page() {
        let page = parse_list(include_str!("../fixtures/catalog.html"));
        assert_eq!(page.mangas.len(), 30);
        let first = &page.mangas[0];
        assert_eq!(first.url, "/manga/slezy-na-uvyadshih-cvetah");
        assert_eq!(first.title, "Слезы на увядших цветах");
        assert_eq!(first.thumbnail_url.as_deref(), Some("https://mangabuff.ru/img/manga/posters/slezy-na-uvyadshih-cvetah.jpg"));
        assert!(page.has_next_page);
    }

    #[test]
    fn a_search_page_reads_its_cards() {
        let page = parse_list(include_str!("../fixtures/search.html"));
        assert!(!page.mangas.is_empty());
        assert!(page.mangas.iter().all(|m| m.url.starts_with("/manga/") && !m.title.is_empty()));
    }

    #[test]
    fn urls_follow_the_kotlin() {
        assert_eq!(catalog_url(1, "real_views"), "https://mangabuff.ru/manga?sort=real_views");
        assert_eq!(catalog_url(3, "updated_at"), "https://mangabuff.ru/manga?sort=updated_at&page=3");
        assert_eq!(search_url(2, " башня ").unwrap(), "https://mangabuff.ru/search?type=manga&q=%D0%B1%D0%B0%D1%88%D0%BD%D1%8F&page=2");
        assert!(search_url(1, "ab").is_err());
        assert_eq!(page("/manga/x").unwrap(), "https://mangabuff.ru/manga/x");
        assert!(page("https://example.com/manga/x").is_err());
    }

    #[test]
    fn details_carry_title_description_genres_status_and_cover() {
        let manga = parse_details(DETAILS, "/manga/bezgranichnyi-volshebnik").unwrap();
        assert_eq!(manga.title, "Безграничный маг");
        let description = manga.description.unwrap();
        assert!(description.starts_with("Как-то раз в горном поселении"));
        assert!(description.contains("Просмотров: 30\u{a0}474\u{a0}060"));
        assert!(description.contains("• Infinite Mage"));
        let genre = manga.genre.unwrap();
        assert!(genre.starts_with("Манхва, 2022"), "{genre}");
        assert!(genre.contains("Приключения"));
        assert_eq!(manga.status, status::ONGOING);
        assert_eq!(manga.thumbnail_url.as_deref(), Some("https://mangabuff.ru/img/manga/posters/bezgranichnyi-volshebnik.jpg?1782856666"));
    }

    #[test]
    fn a_long_title_lists_100_chapters_and_asks_for_the_rest() {
        let chapters = parse_chapters(DETAILS);
        assert_eq!(chapters.len(), 100);
        let newest = &chapters[0];
        assert_eq!(newest.url, "/manga/bezgranichnyi-volshebnik/1/192");
        assert_eq!(newest.name, "Том 1 Глава 192");
        assert_eq!(newest.chapter_number, 192.0);
        assert_eq!(newest.date_upload, 1_793_059_200_000); // 27.10.2026
        let (id, token) = more_chapters(DETAILS).unwrap().unwrap();
        assert_eq!(id, "24830");
        assert!(!token.is_empty());
        let older = parse_load(include_str!("../fixtures/load.json")).unwrap();
        assert_eq!(older.len(), 92);
    }

    #[test]
    fn the_reader_gives_every_page_image() {
        let pages = parse_pages(include_str!("../fixtures/reader.html"));
        assert_eq!(pages.len(), 22);
        assert_eq!(pages[0], "https://c3.mangabuff.ru/img2/chapters/muhan-ui-mabeobsa/1/1/1.jpg");
    }

    #[test]
    fn numbers_are_grouped_as_in_russian() {
        assert_eq!(grouped(30_474_060), "30\u{a0}474\u{a0}060");
        assert_eq!(grouped(999), "999");
    }
}
