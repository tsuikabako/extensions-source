//! A complete manga source for an imaginary site, `https://manga.example`, to copy when you write your own. Every part
//! you change is marked; the guide is `CONTRIBUTING.md` and https://tsuikabako.github.io/make/plugin.
//!
//! The site has:
//! - `/popular?page=N`: a grid of `div.card` (link `a.title`, cover `img`), and `a.next` when there is a next page;
//! - `/search?q=TEXT&page=N`: the same grid;
//! - `/manga/<slug>`: `h1`, `p.summary`, `span.genre` links, `span.status`, `img.cover`, and the chapters as
//!   `ul.chapters a` with a `data-date` (YYYY-MM-DD);
//! - `/chapter/<slug>/<n>`: the page images as `div.reader img`.

use kuro_guest::html::{Html, attr, first, sel, text_of};
use kuro_guest::wire::{Chapter, Manga, MangasPage, Page, status};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

// CHANGE: the site's address, as it appears in `hosts` of manifest.toml.
const BASE: &str = "https://manga.example";
const NAME: &str = "Example";

struct Example;

impl Source for Example {
    fn info(&self) -> Info {
        Info {
            // CHANGE: a new number, never changed after the source is published (the library and backups name it).
            // A port of a Mihon/Aniyomi extension keeps that extension's id: the MD5 of "<name lower-case>/<lang>/1",
            // first 8 bytes, big-endian, top bit cleared.
            id: 1,
            name: NAME,
            lang: "en",
            supports_latest: false,
            // the app keeps at least this gap between two requests to the site
            request_gap_ms: Some(1000),
        }
    }

    // The headers for page images: many image hosts want the site as Referer.
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(vec![("Referer".into(), format!("{BASE}/"))])
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => to_value(parse_list(&get(host, &format!("{BASE}/popular?page={}", input.page().max(1)))?)),
            "search" => to_value(parse_list(&get(host, &search_url(input.query(), input.page()))?)),
            "details" => {
                let url = input.url("manga")?;
                let mut manga = parse_details(&get(host, &absolute(url)?)?)?;
                manga.url = url.to_owned();
                to_value(manga)
            }
            "chapters" => to_value(parse_chapters(&get(host, &absolute(input.url("manga")?)?)?)),
            "pages" => to_value(parse_pages(&get(host, &absolute(input.url("chapter")?)?)?)),
            // "latest" is not offered (supports_latest: false); any op not listed answers "unknown op"
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Example);

/// One page of the site, as text. The error texts are the same in every source.
fn get(host: &dyn Host, url: &str) -> Res<String> {
    Request::get(url).header("Referer", format!("{BASE}/")).text(host, NAME)
}

fn search_url(query: &str, page: i32) -> String {
    let q: String = query.trim().chars().map(|c| if c == ' ' { '+' } else { c }).collect();
    format!("{BASE}/search?q={q}&page={}", page.max(1))
}

/// A stored path (`/manga/blue-lake`) as a full address, refused when it points elsewhere.
fn absolute(path: &str) -> Res<String> {
    if path.starts_with('/') && !path.starts_with("//") {
        Ok(format!("{BASE}{path}"))
    } else {
        Err(format!("not an {NAME} address: {path}"))
    }
}

/// The path of a link on the site: what the app stores for a title or chapter.
fn path_of(href: &str) -> String {
    href.strip_prefix(BASE).unwrap_or(href).to_owned()
}

fn parse_list(html: &str) -> MangasPage {
    let doc = Html::parse_document(html);
    let mangas = doc
        .select(&sel("div.card"))
        .filter_map(|card| {
            let link = first(card, "a.title")?;
            Some(Manga {
                url: path_of(link.value().attr("href")?),
                title: text_of(link),
                thumbnail_url: attr(card, "img", "src"),
                ..Default::default()
            })
        })
        .collect();
    MangasPage { mangas, has_next_page: doc.select(&sel("a.next")).next().is_some() }
}

fn parse_details(html: &str) -> Res<Manga> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let genres: Vec<String> = doc.select(&sel("span.genre")).map(text_of).collect();
    Ok(Manga {
        title: first(root, "h1").map(text_of).ok_or("the title page has no title")?,
        description: first(root, "p.summary").map(text_of),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status: match first(root, "span.status").map(text_of).as_deref() {
            Some("Ongoing") => status::ONGOING,
            Some("Completed") => status::COMPLETED,
            _ => status::UNKNOWN,
        },
        thumbnail_url: attr(root, "img.cover", "src"),
        ..Default::default()
    })
}

fn parse_chapters(html: &str) -> Vec<Chapter> {
    Html::parse_document(html)
        .select(&sel("ul.chapters a"))
        .filter_map(|link| {
            let name = text_of(link);
            Some(Chapter {
                url: path_of(link.value().attr("href")?),
                chapter_number: name.split_whitespace().last().and_then(|n| n.parse().ok()).unwrap_or(-1.0),
                date_upload: link.value().attr("data-date").map_or(0, date_millis),
                name,
                ..Default::default()
            })
        })
        .collect()
}

fn parse_pages(html: &str) -> Vec<Page> {
    Html::parse_document(html).select(&sel("div.reader img")).filter_map(|img| img.value().attr("src")).map(Page::image).collect()
}

/// "2024-03-01" as milliseconds since 1970 (0 when it does not read).
fn date_millis(date: &str) -> i64 {
    let parts: Vec<i64> = date.split('-').filter_map(|p| p.parse().ok()).collect();
    let [y, m, d] = parts[..] else { return 0 };
    // days from 1970-01-01 to the date (Howard Hinnant's days_from_civil)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    (era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468) * 86_400_000
}

// The tests read pages saved from the site (fixtures/) through a TestHost, so they run with plain `cargo test`.
#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;
    use serde_json::json;

    const LIST: &str = include_str!("../fixtures/popular.html");
    const MANGA: &str = include_str!("../fixtures/manga.html");
    const CHAPTER: &str = include_str!("../fixtures/chapter.html");

    /// Runs an op the way the app does, with a host that answers from the fixtures.
    fn call(op: &str, args: Value) -> Res<Value> {
        let host = TestHost::answering(|request| match request.url.as_str() {
            url if url.contains("/popular") || url.contains("/search") => (200, LIST.into()),
            url if url.contains("/manga/") => (200, MANGA.into()),
            url if url.contains("/chapter/") => (200, CHAPTER.into()),
            _ => (404, String::new()),
        });
        kuro_guest::invoke(&Example, &host, op, &json!({ "args": args }).to_string()).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn the_list_has_cards_and_a_next_page() {
        let page = call("popular", json!({ "page": 1 })).unwrap();
        assert_eq!(page["mangas"][0], json!({ "url": "/manga/blue-lake", "title": "Blue Lake", "status": 0, "thumbnailUrl": "https://img.manga.example/blue-lake.jpg" }));
        assert_eq!(page["hasNextPage"], json!(true));
    }

    #[test]
    fn details_chapters_and_pages() {
        let manga = call("details", json!({ "manga": { "url": "/manga/blue-lake" } })).unwrap();
        assert_eq!((manga["title"].as_str(), manga["status"].as_i64(), manga["genre"].as_str()), (Some("Blue Lake"), Some(1), Some("Drama, Mystery")));
        let chapters = call("chapters", json!({ "manga": { "url": "/manga/blue-lake" } })).unwrap();
        assert_eq!(chapters[0], json!({ "url": "/chapter/blue-lake/2", "name": "Chapter 2", "dateUpload": 1_709_251_200_000_i64, "chapterNumber": 2.0 }));
        let pages = call("pages", json!({ "chapter": { "url": "/chapter/blue-lake/2" } })).unwrap();
        assert_eq!(pages, json!([{ "url": "", "imageUrl": "https://img.manga.example/2/1.jpg" }, { "url": "", "imageUrl": "https://img.manga.example/2/2.jpg" }]));
    }

    #[test]
    fn other_sites_and_unknown_ops_are_refused() {
        assert_eq!(call("details", json!({ "manga": { "url": "https://evil.example/x" } })), Err("not an Example address: https://evil.example/x".into()));
        assert_eq!(call("latest", json!({ "page": 1 })), Err("unknown op: latest".into()));
        assert_eq!(search_url(" blue lake ", 0), "https://manga.example/search?q=blue+lake&page=1");
    }
}
