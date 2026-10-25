//! The NovelCool site as pure functions: the app API's lists, and one parser per HTML page. Ported from the keiyoushi
//! `all/novelcool` extension (Apache-2.0).

use chrono::NaiveDate;
use kuro_guest::Res;
use kuro_guest::html::{Html, attr, first, sel, text_of};
use kuro_guest::wire::{Chapter, Manga, MangasPage, status};
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

pub const API: &str = "https://api.novelcool.com";
/// The app's key for the API, as the Kotlin sends it.
const APP_ID: &str = "202201290625004";
const APP_SECRET: &str = "c73a8590641781f203660afca1d37ada";
/// Titles per page of an API list; a full page means there may be more.
const PAGE_SIZE: usize = 20;
/// How many images one reader page shows (the site offers 1, 3, 6 or 10).
const IMAGES_PER_PAGE: u32 = 10;

// ================================ API =================================

/// The body of an API list: `elite/hot/`, `elite/latest/` or `book/search/` (with `keyword`). Manga only.
pub fn api_body(lang: &str, page: i32, keyword: Option<&str>) -> String {
    let mut body = json!({
        "appId": APP_ID,
        "lang": lang,
        "lc_type": "manga",
        "page": page.max(1).to_string(),
        "page_size": PAGE_SIZE.to_string(),
        "secret": APP_SECRET,
    });
    if let Some(keyword) = keyword {
        body["keyword"] = json!(keyword.trim());
    }
    body.to_string()
}

#[derive(Deserialize)]
struct ApiList {
    #[serde(default)]
    list: Option<Vec<ApiManga>>,
}

#[derive(Deserialize)]
struct ApiManga {
    url: String,
    name: String,
    #[serde(default)]
    cover: String,
    /// "1" on titles the site marks for adults.
    #[serde(default)]
    warning: Value,
}

/// An API list. Titles marked for adults are left out unless `show_adult`.
pub fn parse_api_list(json: &str, show_adult: bool) -> Res<MangasPage> {
    let list = serde_json::from_str::<ApiList>(json).map_err(|e| format!("NovelCool sent an unreadable list: {e}"))?.list.unwrap_or_default();
    let has_next_page = list.len() == PAGE_SIZE;
    let mangas = list
        .into_iter()
        .filter(|m| show_adult || m.warning.as_str() != Some("1"))
        .filter_map(|m| {
            Some(Manga {
                url: Url::parse(&m.url).ok()?.path().to_owned(),
                title: m.name,
                thumbnail_url: Some(m.cover).filter(|c| !c.is_empty()),
                ..Default::default()
            })
        })
        .collect();
    Ok(MangasPage { mangas, has_next_page })
}

// ================================ URLs =================================

/// A page of the site from a stored address (`/novel/...`, `/chapter/...`), refused when it is another site's.
pub fn page(base: &str, path: &str) -> Res<String> {
    let base = Url::parse(base).map_err(|e| e.to_string())?;
    let url = base.join(path).map_err(|e| format!("invalid address {path}: {e}"))?;
    if url.host_str() != base.host_str() {
        return Err(format!("not a NovelCool address: {path}"));
    }
    Ok(url.to_string())
}

/// The stored form of a chapter address: its path, ending in `.html` (the site writes both `/chapter/x/1/` and
/// `/chapter/x/1.html`).
fn chapter_path(href: &str) -> Option<String> {
    let path = Url::parse(href).ok()?.path().trim_end_matches('/').trim_end_matches(".html").to_owned();
    path.starts_with("/chapter/").then(|| format!("{path}.html"))
}

/// The first reader page of a chapter with ten images on each page (`/chapter/x/1.html` → `/chapter/x/1-10-1.html`).
pub fn reader_url(base: &str, chapter: &str) -> Res<String> {
    page(base, &format!("{}-{IMAGES_PER_PAGE}-1.html", chapter.trim_end_matches('/').trim_end_matches(".html")))
}

// =============================== Parsers ===============================

pub fn parse_details(html: &str) -> Res<Manga> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let genres: Vec<String> = first(root, ".bk-cates").map(|c| c.select(&sel(".bk-cate-item a[title]")).map(text_of).collect()).unwrap_or_default();
    let status = match attr(root, ".bk-cate-type1 a", "href").as_deref().and_then(|h| h.rsplit('/').next()) {
        Some("updated.html") => status::ONGOING,
        Some("completed.html") => status::COMPLETED,
        _ => status::UNKNOWN,
    };
    Ok(Manga {
        url: String::new(),
        title: first(root, "h1.bookinfo-title").map(text_of).ok_or("the title page has no title")?,
        author: attr(root, ".bookinfo-author > a", "title"),
        description: first(root, "div.bk-summary-txt").map(text_of).filter(|d| !d.is_empty()),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status,
        thumbnail_url: attr(root, ".bookinfo-pic-img", "src"),
        ..Default::default()
    })
}

/// What a title page gives for the chapter list: the newest chapter's reader page (the cover links to it), and the
/// names and dates of the listed chapters, newest first. The listed links go through ad redirectors, so the chapter
/// addresses come from the reader page's chapter menu instead.
pub struct TitlePage {
    pub newest: Option<String>,
    pub dated: Vec<(String, i64)>,
}

pub fn parse_title_page(html: &str) -> TitlePage {
    let doc = Html::parse_document(html);
    let newest = doc.select(&sel(".bookinfo-pic a[href]")).find_map(|a| chapter_path(a.value().attr("href")?));
    let dated = doc
        .select(&sel(".chapter-item-list a"))
        .map(|a| {
            let name = a.value().attr("title").unwrap_or_default().trim().to_owned();
            (name, first(a, ".chapter-item-time").map_or(0, |t| date_of(&text_of(t))))
        })
        .collect();
    TitlePage { newest, dated }
}

/// An `Oct 18, 2026` date as milliseconds since 1970 (UTC), 0 when it does not parse.
fn date_of(text: &str) -> i64 {
    NaiveDate::parse_from_str(text.trim(), "%b %d, %Y").ok().and_then(|d| d.and_hms_opt(0, 0, 0)).map_or(0, |t| t.and_utc().timestamp_millis())
}

/// The chapters from the reader page of `newest` and the title page's dates. The reader's chapter menu lists the same
/// chapters as the title page, newest first, but sometimes without the newest one, which is then put first.
pub fn parse_chapters(reader_html: &str, newest: &str, dated: &[(String, i64)]) -> Vec<Chapter> {
    let doc = Html::parse_document(reader_html);
    let root = doc.root_element();
    let mut chapters: Vec<Chapter> = first(root, ".mangaread-pagenav select")
        .map(|menu| {
            menu.select(&sel("option"))
                .filter_map(|o| Some(Chapter { url: chapter_path(o.value().attr("value")?)?, name: text_of(o), ..Default::default() }))
                .collect()
        })
        .unwrap_or_default();
    if !chapters.iter().any(|c| c.url == newest) {
        let name = first(root, ".mangaread-title h1").map(text_of).unwrap_or_else(|| "Newest".into());
        chapters.insert(0, Chapter { url: newest.to_owned(), name, ..Default::default() });
    }
    // the two lists end with the same oldest chapter: pair them from the end, and only where the names agree
    let offset = chapters.len() as isize - dated.len() as isize;
    for (i, chapter) in chapters.iter_mut().enumerate() {
        if let Some((name, date)) = usize::try_from(i as isize - offset).ok().and_then(|j| dated.get(j)) {
            if *name == chapter.name {
                chapter.date_upload = *date;
            }
        }
    }
    chapters
}

/// The images of one reader page, and the other reader pages of the chapter (from its page menu).
pub fn parse_reader(html: &str) -> (Vec<String>, Vec<String>) {
    let doc = Html::parse_document(html);
    let images = doc.select(&sel("img.mangaread-manga-pic")).filter_map(|img| img.value().attr("src")).map(|s| s.trim().to_owned()).collect();
    let others = doc
        .select(&sel(".sl-page"))
        .next()
        .map(|menu| menu.select(&sel("option:not([selected])")).filter_map(|o| o.value().attr("value")).map(str::to_owned).collect())
        .unwrap_or_default();
    (images, others)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOT: &str = include_str!("../fixtures/hot.json");
    const HOT_ES: &str = include_str!("../fixtures/hot_es.json");
    const MANGA: &str = include_str!("../fixtures/manga.html");
    const READER: &str = include_str!("../fixtures/reader.html");
    const READER_10: &str = include_str!("../fixtures/reader_10.html");
    const MANGA_ES: &str = include_str!("../fixtures/manga_es.html");
    const READER_ES: &str = include_str!("../fixtures/reader_es.html");

    #[test]
    fn an_api_list_gives_manga_paths_and_hides_adult_titles() {
        let page = parse_api_list(HOT, true).unwrap();
        assert_eq!(page.mangas.len(), 20);
        assert!(page.has_next_page);
        assert_eq!((page.mangas[0].url.as_str(), page.mangas[0].title.as_str()), ("/novel/Martial-Lord.html", "Martial Lord"));
        assert!(page.mangas[0].thumbnail_url.as_deref().unwrap().starts_with("https://img.novelcool.com/"));
        let es = parse_api_list(HOT_ES, true).unwrap().mangas.len();
        let es_safe = parse_api_list(HOT_ES, false).unwrap().mangas;
        assert!(es_safe.len() < es && es_safe.iter().all(|m| m.title != "BJ Alex"));
        assert!(!parse_api_list(r#"{"list":null}"#, false).unwrap().has_next_page);
        let body: Value = serde_json::from_str(&api_body("es", 0, Some(" love "))).unwrap();
        assert_eq!((body["lang"].as_str(), body["page"].as_str(), body["keyword"].as_str(), body["lc_type"].as_str()), (Some("es"), Some("1"), Some("love"), Some("manga")));
        assert!(serde_json::from_str::<Value>(&api_body("en", 2, None)).unwrap().get("keyword").is_none());
    }

    #[test]
    fn the_title_page_has_title_author_summary_genres_status_and_cover() {
        let manga = parse_details(MANGA).unwrap();
        assert_eq!(manga.title, "Sakamoto Days");
        assert_eq!(manga.author.as_deref(), Some("Suzuki Yuuto"));
        assert_eq!(manga.status, status::ONGOING);
        assert!(manga.genre.unwrap().starts_with("Action"));
        assert!(manga.description.unwrap().starts_with("Taro Sakamoto was the ultimate assassin"));
        assert_eq!(manga.thumbnail_url.as_deref(), Some("https://img.novelcool.com/logo/202109/72/Sakamoto_Days5579.jpg"));
        assert_eq!(parse_details(MANGA_ES).unwrap().author, None, "a blank author is none");
    }

    #[test]
    fn the_chapters_come_from_the_reader_menu_with_the_title_page_dates() {
        let title = parse_title_page(MANGA);
        assert_eq!(title.newest.as_deref(), Some("/chapter/Ch-280/14940329.html"));
        assert_eq!(title.dated[0], ("Ch.279".to_owned(), 1_792_281_600_000)); // Oct 18, 2026
        // here the menu lacks the newest chapter: it comes first, without a date
        let chapters = parse_chapters(READER, title.newest.as_deref().unwrap(), &title.dated);
        assert_eq!(chapters.len(), 6);
        assert_eq!((chapters[0].url.as_str(), chapters[0].name.as_str(), chapters[0].date_upload), ("/chapter/Ch-280/14940329.html", "Ch.280", 0));
        assert_eq!((chapters[1].url.as_str(), chapters[1].name.as_str()), ("/chapter/Ch-279/14925419.html", "Ch.279"));
        assert!(chapters[1..].iter().all(|c| c.date_upload > 0));
        // here it has it
        let title = parse_title_page(MANGA_ES);
        let chapters = parse_chapters(READER_ES, title.newest.as_deref().unwrap(), &title.dated);
        assert_eq!(chapters.len(), 5);
        assert_eq!((chapters[0].url.as_str(), chapters[0].name.as_str()), ("/chapter/Capitulo-103/13979247.html", "Capitulo 103"));
        assert!(chapters.iter().all(|c| c.date_upload > 0));
        assert_eq!(date_of("Jun 02, 2026"), 1_780_358_400_000);
    }

    #[test]
    fn a_reader_page_gives_its_images_and_the_other_pages() {
        let (images, others) = parse_reader(READER_10);
        assert_eq!(images.len(), 10);
        assert!(images.iter().all(|i| i.starts_with("https://")));
        assert_eq!(others, ["https://www.novelcool.com/chapter/Ch-280/14940329-10-2.html", "https://www.novelcool.com/chapter/Ch-280/14940329-10-3.html"]);
        assert_eq!(reader_url("https://www.novelcool.com", "/chapter/Ch-280/14940329.html").unwrap(), "https://www.novelcool.com/chapter/Ch-280/14940329-10-1.html");
    }

    #[test]
    fn only_addresses_of_the_site_are_fetched() {
        assert_eq!(page("https://es.novelcool.com", "/novel/Jinx.html").unwrap(), "https://es.novelcool.com/novel/Jinx.html");
        assert!(page("https://es.novelcool.com", "https://www.novelcool.com/novel/Jinx.html").is_err());
        assert_eq!(chapter_path("https://es.novelcool.com/chapter/Capitulo-103/13979247/").as_deref(), Some("/chapter/Capitulo-103/13979247.html"));
        assert_eq!(chapter_path("https://explorefunctionality.com/go/rds/?data=00"), None);
    }
}
