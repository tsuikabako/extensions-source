//! The LeerCapitulo site (<https://www.leercapitulo.co>) as pure functions: URLs and one parser per page. Ported from
//! the keiyoushi `es/leercapitulo` extension (Apache-2.0). Without the filters (genre, theme, type, status, order).

use chrono::NaiveDate;
use kuro_guest::Res;
use kuro_guest::html::{ElementRef, Html, attr, first, sel, text_of};
use kuro_guest::wire::{Chapter, Manga, MangasPage, status};
use url::Url;

pub const BASE: &str = "https://www.leercapitulo.co";
const HOST: &str = "www.leercapitulo.co";

// ================================ URLs =================================

pub fn home_url() -> String {
    format!("{BASE}/")
}

pub fn search_url(page: i32, query: &str) -> String {
    let mut url = format!("{BASE}/manga/");
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    if !query.trim().is_empty() {
        params.append_pair("q", query.trim());
    }
    if page > 1 {
        params.append_pair("page", &page.to_string());
    }
    let params = params.finish();
    if !params.is_empty() {
        url = format!("{url}?{params}");
    }
    url
}

/// A page of the site from a stored address (`/manga/...`, `/leer/...`), refused when it is another site's.
pub fn page(path: &str) -> Res<String> {
    let url = Url::parse(BASE).and_then(|base| base.join(path)).map_err(|e| format!("invalid address {path}: {e}"))?;
    if url.host_str() != Some(HOST) {
        return Err(format!("not a LeerCapitulo address: {path}"));
    }
    Ok(url.to_string())
}

// =============================== Parsers ===============================

fn absolute(href: &str) -> Option<Url> {
    Url::parse(BASE).and_then(|base| base.join(href)).ok()
}

/// A card: a link (`link` selector) with the title and the stored path, and a cover.
fn card(element: ElementRef, link: &str) -> Option<Manga> {
    let a = first(element, link)?;
    Some(Manga {
        url: absolute(a.value().attr("href")?)?.path().to_owned(),
        title: text_of(a),
        thumbnail_url: attr(element, "img", "src").and_then(|src| absolute(&src)).map(String::from),
        ..Default::default()
    })
}

fn cards(html: &str, item: &str, link: &str) -> Vec<Manga> {
    Html::parse_document(html).select(&sel(item)).filter_map(|e| card(e, link)).collect()
}

/// The "Tendencias" slider of the home page (one page).
pub fn parse_popular(html: &str) -> MangasPage {
    MangasPage { mangas: cards(html, ".lc-slide", "a.lc-slide-name"), has_next_page: false }
}

/// The latest releases of the home page (one page).
pub fn parse_latest(html: &str) -> MangasPage {
    MangasPage { mangas: cards(html, "article.lc-release", "a.lc-release-title"), has_next_page: false }
}

pub fn parse_search(html: &str) -> MangasPage {
    let has_next_page = Html::parse_document(html).select(&sel("a.page-link[rel=next]")).next().is_some();
    MangasPage { mangas: cards(html, "article.lc-card", "a.lc-card-name"), has_next_page }
}

/// The value of a fact of the title (`<li><span class="k">Autor</span><span>...</span></li>`): its last child.
fn fact(doc: &Html, label: &str) -> Option<String> {
    doc.select(&sel(".lc-facts li"))
        .find(|li| first(*li, ".k").map(text_of).as_deref() == Some(label))
        .and_then(|li| li.children().filter_map(ElementRef::wrap).last())
        .map(text_of)
        .filter(|v| !v.is_empty())
}

fn status_of(text: &str) -> i32 {
    match text {
        "Ongoing" => status::ONGOING,
        "Paused" => status::ON_HIATUS,
        "Completed" => status::COMPLETED,
        "Cancelled" => status::CANCELLED,
        _ => status::UNKNOWN,
    }
}

pub fn parse_details(html: &str) -> Res<Manga> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let title = first(root, "h1").map(text_of).ok_or("the manga page has no title")?;
    let synopsis = first(root, "#sinopsis p:not(.lc-muted)")
        .map(|p| p.text().collect::<String>().lines().map(str::trim).collect::<Vec<_>>().join("\n").trim().to_owned())
        .unwrap_or_default();
    let alt_names: Vec<String> = first(root, "h1 + p.lc-muted")
        .map(text_of)
        .unwrap_or_default()
        .split(" · ")
        .map(str::trim)
        .filter(|n| !n.is_empty() && *n != title)
        .map(str::to_owned)
        .collect();
    let mut description = synopsis;
    if !alt_names.is_empty() {
        if !description.is_empty() {
            description.push_str("\n\n");
        }
        description.push_str(&format!("Alt name(s): {}", alt_names.join(" · ")));
    }
    let genres: Vec<String> = doc.select(&sel("a.badge[href^='/manga/?genre='], a.badge[href^='/manga/?theme=']")).map(text_of).collect();
    Ok(Manga {
        url: String::new(),
        title,
        author: fact(&doc, "Autor"),
        artist: fact(&doc, "Dibujo"),
        description: Some(description).filter(|d| !d.is_empty()),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status: status_of(&fact(&doc, "Estado").unwrap_or_default()),
        thumbnail_url: attr(root, ".lc-cover-lg img", "src").and_then(|src| absolute(&src)).map(String::from),
    })
}

/// A `yyyy-MM-dd` date as milliseconds since 1970 (UTC), 0 when it does not parse.
fn date_of(text: &str) -> i64 {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)).map_or(0, |t| t.and_utc().timestamp_millis())
}

/// The chapters of a manga page, newest first as the site lists them.
pub fn parse_chapters(html: &str) -> Vec<Chapter> {
    Html::parse_document(html)
        .select(&sel("a.lc-chapter-row"))
        .filter_map(|a| {
            Some(Chapter {
                url: absolute(a.value().attr("href")?)?.path().to_owned(),
                name: first(a, ".n").map(text_of)?,
                date_upload: first(a, ".d").map_or(0, |d| date_of(&text_of(d))),
                ..Default::default()
            })
        })
        .collect()
}

/// The page images of a chapter, in order.
pub fn parse_pages(html: &str) -> Vec<String> {
    Html::parse_document(html)
        .select(&sel("#lcPages img[data-src]"))
        .filter_map(|img| absolute(img.value().attr("data-src")?.trim()))
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = include_str!("../fixtures/home.html");
    const SEARCH: &str = include_str!("../fixtures/search.html");
    const MANGA: &str = include_str!("../fixtures/manga.html");
    const CHAPTER: &str = include_str!("../fixtures/chapter.html");

    #[test]
    fn the_home_page_gives_the_trending_and_the_latest_cards() {
        let popular = parse_popular(HOME);
        assert_eq!(popular.mangas.len(), 4);
        assert_eq!((popular.mangas[0].url.as_str(), popular.mangas[0].title.as_str()), ("/manga/psvkfbmjgo/one-piece/", "One Piece"));
        assert!(popular.mangas[0].thumbnail_url.as_deref().unwrap().starts_with("https://www.leercapitulo.co/covers/"));
        let latest = parse_latest(HOME);
        assert_eq!(latest.mangas.len(), 5);
        assert_eq!(latest.mangas[0].title, "In The Name Of Marriage");
        assert!(latest.mangas.iter().all(|m| m.url.starts_with("/manga/") && m.thumbnail_url.is_some()));
        assert!(!popular.has_next_page && !latest.has_next_page);
    }

    #[test]
    fn a_search_lists_cards_and_a_next_link_means_more() {
        let page = parse_search(SEARCH);
        assert_eq!(page.mangas.len(), 4);
        assert!(page.has_next_page);
        assert!(page.mangas.iter().all(|m| m.url.starts_with("/manga/") && !m.title.is_empty()));
        assert!(!parse_search("<article class=\"lc-card\"></article>").has_next_page);
        assert_eq!(search_url(1, " one piece "), "https://www.leercapitulo.co/manga/?q=one+piece");
        assert_eq!(search_url(3, ""), "https://www.leercapitulo.co/manga/?page=3");
        assert_eq!(search_url(0, ""), "https://www.leercapitulo.co/manga/");
    }

    #[test]
    fn the_manga_page_has_title_summary_genres_facts_and_cover() {
        let manga = parse_details(MANGA).unwrap();
        assert_eq!(manga.title, "One Piece");
        assert_eq!(manga.status, status::ONGOING);
        assert!(manga.author.unwrap().starts_with("Oda Eiichirou"));
        assert!(manga.genre.unwrap().starts_with("Action, Adventure, Comedy"));
        let description = manga.description.unwrap();
        assert!(description.starts_with("Hace veintidós años"), "{description}");
        assert!(description.contains("\n\nAlt name(s): Đảo Hải Tặc · "));
        assert!(manga.thumbnail_url.unwrap().starts_with("https://www.leercapitulo.co/covers/4f/"));
    }

    #[test]
    fn the_chapters_are_listed_newest_first_with_dates() {
        let chapters = parse_chapters(MANGA);
        assert_eq!(chapters.len(), 5);
        assert_eq!((chapters[0].url.as_str(), chapters[0].name.as_str()), ("/leer/psvkfbmjgo/one-piece/1196/", "Capitulo 1196"));
        assert_eq!(chapters[0].date_upload, 1_792_195_200_000); // 2026-10-17
        assert_eq!(date_of("not a date"), 0);
    }

    #[test]
    fn the_chapter_page_gives_its_images_in_order() {
        let pages = parse_pages(CHAPTER);
        assert_eq!(pages.len(), 17);
        assert!(pages.iter().all(|p| p.starts_with("https://")), "{pages:?}");
    }

    #[test]
    fn only_addresses_of_the_site_are_fetched() {
        assert_eq!(page("/manga/psvkfbmjgo/one-piece/").unwrap(), "https://www.leercapitulo.co/manga/psvkfbmjgo/one-piece/");
        assert!(page("https://evil.test/x").is_err());
        assert_eq!(status_of("Paused"), status::ON_HIATUS);
    }
}
