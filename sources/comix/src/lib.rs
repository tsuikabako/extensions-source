//! Comix (<https://comix.to>), a manga source plug-in for Kurobayomi. Ported from the keiyoushi `en/comix` extension
//! (Apache-2.0); see README.md.
//!
//! The site serves its lists, chapter lists and page lists only through API calls that its own script signs, so they
//! cannot be asked for with plain requests. Like the Kotlin, this source reads what the site's pages show once a
//! browser has loaded them: the app opens the page in its hidden browser page (`kuro_guest::pages`) and hands back the
//! HTML. This file is the ops and what each asks the app to read; `page.rs` reads the rendered pages, `data.rs` a
//! title's details, `descramble.rs` restores the page images the site hands out scrambled, `dates.rs` the upload times.
//!
//! Left out: the search filters and the settings (content rating, types, demographics, hidden groups, chapter
//! deduplication), related titles and deep links.

mod data;
mod dates;
mod descramble;
mod page;
mod tags;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use kuro_guest::wire::{Chapter, Manga, MangasPage, Page};
use kuro_guest::{Host, Info, Input, PageRead, Request, Res, Source, to_value, unknown_op};
use page::BASE;
use serde::Deserialize;
use serde_json::{Value, json};

const NAME: &str = "Comix";
/// The source id: the MD5 of `comix/en/1`, as the Kotlin extension and the first plug-in use. Keep it: libraries and
/// backups name it.
const ID: i64 = 7537715367149829912;
/// A safety cap on the pages of one title's chapter list (20 chapters a page).
const MAX_CHAPTER_PAGES: i32 = 150;
/// The reader fills a page's address only once its slot has come into view, so a long chapter takes a while.
const READER_TIMEOUT_MS: u64 = 55_000;

struct Comix;

impl Source for Comix {
    fn info(&self) -> Info {
        // the Kotlin allows five requests a second
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: Some(200) }
    }

    /// The page images are asked without Origin or Referer: the image hosts answer 403 to the site's Referer.
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(vec![("Accept".into(), "*/*".into())])
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let page = input.page().max(1);
        match op {
            "popular" => to_value(list(host, &format!("sort=score:desc&page={page}"))?),
            "latest" => to_value(list(host, &format!("sort=chapter_updated_at:desc&page={page}"))?),
            "search" if input.query().trim().is_empty() => to_value(list(host, &format!("sort=score:desc&page={page}"))?),
            "search" => {
                let query: String = url::form_urlencoded::byte_serialize(input.query().trim().as_bytes()).collect();
                to_value(list(host, &format!("q={query}&sort=relevance:desc&page={page}"))?)
            }
            "details" => to_value(details(host, input.url("manga")?)?),
            "chapters" => to_value(chapters(host, input.url("manga")?)?),
            "pages" => to_value(pages(host, input.url("chapter")?)?),
            "unscramble" => {
                #[derive(Deserialize)]
                struct Image {
                    body: String,
                    #[serde(default)]
                    headers: Vec<(String, String)>,
                }
                let image: Image = input.args()?;
                let body = STANDARD.decode(&image.body).map_err(|e| format!("body is not base64: {e}"))?;
                let fixed = descramble::unscramble(&body, &image.headers)?;
                Ok(json!({ "body": fixed.map(|bytes| STANDARD.encode(bytes)) }))
            }
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Comix, pages);

/// One page of the site's browse list: its titles, or none when the site says nothing matches.
fn list(host: &dyn Host, query: &str) -> Res<MangasPage> {
    let read = PageRead::new(format!("{BASE}/browse?{query}")).wait("div.list-grid > div.lrow, div.list-empty").read("div.list-grid, nav.npager").send(host)?;
    if read.html().trim().is_empty() { Ok(MangasPage::default()) } else { page::list(read.html()) }
}

/// The site address of a stored title address (`/<id>-<slug>`), refused when it is anything else.
fn title_url(manga: &str) -> Res<String> {
    let url = format!("{BASE}/title{manga}");
    Some(url).filter(|url| page::title_address(url).as_deref() == Some(manga)).ok_or_else(|| format!("not a {NAME} address: {manga}"))
}

/// A title's details: its page carries them for a plain request; when the site refuses that, the page is read.
fn details(host: &dyn Host, manga: &str) -> Res<Manga> {
    let url = title_url(manga)?;
    let plain = Request::get(&url).header("Accept", "text/html,application/xhtml+xml").text(host, NAME).and_then(|html| data::details(&html));
    let mut details = match plain {
        Ok(details) => details,
        Err(error) => {
            host.log(1, &format!("comix: the title page was not readable by a plain request ({error}); reading it in a page"));
            data::details(PageRead::new(&url).wait("script#initial-data").read("script#initial-data").send(host)?.html())?
        }
    };
    details.url = manga.to_owned();
    Ok(details)
}

/// Every chapter of a title, newest first: the title page lists twenty a page, and the page number goes in its
/// address.
fn chapters(host: &dyn Host, manga: &str) -> Res<Vec<Chapter>> {
    let url = title_url(manga)?;
    let now = i64::try_from(host.unix_seconds()).unwrap_or(0);
    let mut all: Vec<Chapter> = Vec::new();
    for number in 1..=MAX_CHAPTER_PAGES {
        let read = PageRead::new(format!("{url}?page={number}")).wait("ul.mchap-list > li.mchap-item").read("ul.mchap-list, nav.npager").send(host)?;
        if read.html().trim().is_empty() {
            break;
        }
        let (found, more) = page::chapters(read.html(), now)?;
        let before = all.len();
        for chapter in found {
            if !all.iter().any(|seen| seen.url == chapter.url) {
                all.push(chapter);
            }
        }
        // a page that brings nothing new means the site gave the same page again: stop instead of going round
        if !more || all.len() == before {
            break;
        }
    }
    Ok(all)
}

/// The page images of a chapter: its reader page, every slot scrolled into view so the reader names its picture.
fn pages(host: &dyn Host, chapter: &str) -> Res<Vec<Page>> {
    let url = format!("{BASE}/{chapter}");
    if page::chapter_address(&url).map(|(stored, _)| stored).as_deref() != Some(chapter) {
        return Err(format!("not a {NAME} chapter: {chapter}"));
    }
    let read = PageRead::new(url)
        .wait("div.rpage-page")
        .scroll("div.rpage-page")
        .wait_gone("div.rpage-page:not(:has(img.rpage-page__img[src]))")
        .read("div.rpage-page")
        .timeout_ms(READER_TIMEOUT_MS)
        .send(host)?;
    let reader = page::reader(read.html())?;
    if !reader.missing.is_empty() {
        return Err(format!("{NAME}: {} of {} pages did not load; open the chapter again", reader.missing.len(), reader.pages.len()));
    }
    Ok(reader.pages.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::Page as Read;
    use kuro_guest::testing::TestHost;
    use kuro_guest::wire::status;

    const BROWSE: &str = include_str!("../fixtures/browse.html");
    const TITLE: &str = include_str!("../fixtures/title.html");
    const TITLE_PAGE2: &str = include_str!("../fixtures/title-page2.html");
    const READER: &str = include_str!("../fixtures/reader.html");
    const MANIFEST: &str = include_str!("../manifest.toml");
    const MEAL: &str = "/02l05-after-a-meal";

    fn read(url: &str, html: &str) -> Res<Read> {
        Ok(Read { url: url.to_owned(), title: String::new(), reads: vec![html.to_owned()] })
    }

    /// The site as saved on 2026-11-20 by a headless browser: pages by address. The title has two chapter pages here
    /// (the second one's pager is cut), and its reader is filled the way the app's scroll step fills it.
    fn site() -> TestHost {
        let reader_filled = READER.replace("<div class=\"rpage-page__loader\" aria-label=\"Page ", "<img class=\"rpage-page__img\" src=\"https://447.example.site/hi/x\"><div class=\"rpage-page__loader\" aria-label=\"Page ");
        let last_page = TITLE_PAGE2.split("<nav class=\"npager\"").next().unwrap().to_owned();
        TestHost::answering(|request| match request.url.as_str() {
            "https://comix.to/title/02l05-after-a-meal" => (200, TITLE.to_owned()),
            _ => (404, String::new()),
        })
        .reading(move |page| match page.url.as_str() {
            "https://comix.to/browse?sort=score:desc&page=1" | "https://comix.to/browse?sort=chapter_updated_at:desc&page=2" | "https://comix.to/browse?q=solo+leveling&sort=relevance:desc&page=1" => read(&page.url, BROWSE),
            "https://comix.to/browse?q=nothing&sort=relevance:desc&page=1" => read(&page.url, ""),
            "https://comix.to/title/02l05-after-a-meal?page=1" => read(&page.url, TITLE),
            "https://comix.to/title/02l05-after-a-meal?page=2" => read(&page.url, &last_page),
            "https://comix.to/title/02l05-after-a-meal/11725043-chapter-32" => read(&page.url, &reader_filled),
            "https://comix.to/title/02l05-after-a-meal/11723807-chapter-32" => read(&page.url, READER),
            other => Err(format!("nothing matched in time (the page is \"Security check\" at {other})")),
        })
    }

    fn call(host: &dyn Host, op: &str, args: Value) -> Res<Value> {
        kuro_guest::invoke(&Comix, host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str(&o).unwrap())
    }

    #[test]
    fn lists_are_read_from_the_browse_page() {
        let host = site();
        let popular = call(&host, "popular", json!({ "page": 1 })).unwrap();
        assert_eq!((popular["mangas"].as_array().unwrap().len(), popular["hasNextPage"].as_bool()), (28, Some(true)));
        assert_eq!((popular["mangas"][0]["url"].as_str(), popular["mangas"][0]["title"].as_str()), (Some("/g2rk-on-the-way-to-meet-mom"), Some("On the Way to Meet Mom")));
        assert_eq!(call(&host, "latest", json!({ "page": 2 })).unwrap()["mangas"].as_array().unwrap().len(), 28);
        assert_eq!(call(&host, "search", json!({ "page": 1, "query": " solo leveling " })).unwrap()["mangas"].as_array().unwrap().len(), 28);
        assert_eq!(call(&host, "search", json!({ "page": 1, "query": "nothing" })).unwrap(), json!({ "mangas": [], "hasNextPage": false }));
        assert_eq!(call(&host, "search", json!({ "page": 1, "query": "" })).unwrap(), popular);
        let first = &host.pages_read()[0];
        assert_eq!(serde_json::to_value(first).unwrap()["steps"], json!([{ "wait": "div.list-grid > div.lrow, div.list-empty" }, { "read": "div.list-grid, nav.npager" }]));
        assert!(host.sent().is_empty(), "lists ask no plain request");
    }

    #[test]
    fn details_come_from_a_plain_request_and_from_a_page_read_when_that_fails() {
        let host = site();
        let manga = call(&host, "details", json!({ "manga": { "url": MEAL } })).unwrap();
        assert_eq!((manga["url"].as_str(), manga["title"].as_str(), manga["status"].as_i64()), (Some(MEAL), Some("After a Meal"), Some(i64::from(status::ONGOING))));
        assert!(host.pages_read().is_empty());
        let refused = TestHost::answering(|_| (403, String::new())).reading(|page| read(&page.url, TITLE));
        let manga = call(&refused, "details", json!({ "manga": { "url": MEAL } })).unwrap();
        assert_eq!(manga["title"], "After a Meal");
        assert_eq!(refused.pages_read()[0].url, "https://comix.to/title/02l05-after-a-meal");
    }

    #[test]
    fn chapters_follow_the_pages_of_the_list() {
        let host = site();
        let chapters = call(&host, "chapters", json!({ "manga": { "url": MEAL } })).unwrap();
        let chapters = chapters.as_array().unwrap();
        assert_eq!(chapters.len(), 40);
        assert_eq!((chapters[0]["url"].as_str(), chapters[0]["name"].as_str()), (Some("title/02l05-after-a-meal/11725043-chapter-32"), Some("Chapter 32")));
        assert!(chapters[0]["chapterNumber"].as_f64().unwrap() >= chapters[39]["chapterNumber"].as_f64().unwrap());
        let urls: Vec<String> = host.pages_read().iter().map(|read| read.url.clone()).collect();
        assert_eq!(urls, ["https://comix.to/title/02l05-after-a-meal?page=1", "https://comix.to/title/02l05-after-a-meal?page=2"]);
    }

    #[test]
    fn a_chapter_gives_every_page_or_says_how_many_are_missing() {
        let host = site();
        let pages = call(&host, "pages", json!({ "chapter": { "url": "title/02l05-after-a-meal/11725043-chapter-32" } })).unwrap();
        assert_eq!(pages.as_array().unwrap().len(), 50);
        assert!(pages[0]["imageUrl"].as_str().unwrap().starts_with("https://447.alyssablog.site/hi/"), "{}", pages[0]);
        assert_eq!(pages[3], json!({ "url": "", "imageUrl": "https://447.example.site/hi/x" }));
        let read = serde_json::to_value(&host.pages_read()[0]).unwrap();
        assert_eq!(read["steps"][1], json!({ "scroll": "div.rpage-page" }));
        assert_eq!(read["timeoutMs"], 55_000);
        let half = call(&host, "pages", json!({ "chapter": { "url": "title/02l05-after-a-meal/11723807-chapter-32" } }));
        assert_eq!(half, Err("Comix: 47 of 50 pages did not load; open the chapter again".into()));
    }

    #[test]
    fn the_image_ops_answer() {
        let host = site();
        assert_eq!(call(&host, "request-headers", json!({})).unwrap(), json!([["Accept", "*/*"]]));
        assert_eq!(call(&host, "unscramble", json!({ "body": "AAEC", "headers": [["content-type", "image/webp"]] })).unwrap(), json!({ "body": null }));
        assert!(call(&host, "unscramble", json!({ "body": "not base64!" })).is_err());
    }

    #[test]
    fn info_manifest_and_foreign_addresses() {
        let host = site();
        let info = call(&host, "info", json!({})).unwrap();
        assert_eq!((info["id"].as_i64(), info["lang"].as_str(), info["supportsLatest"].as_bool()), (Some(ID), Some("en"), Some(true)));
        assert!(MANIFEST.contains("hosts = [\"comix.to\"]"));
        for manga in ["https://evil.example/x", "/../browse", "02l05-after-a-meal", "/a/b"] {
            assert!(call(&host, "details", json!({ "manga": { "url": manga } })).is_err(), "{manga}");
            assert!(call(&host, "chapters", json!({ "manga": { "url": manga } })).is_err(), "{manga}");
        }
        assert!(call(&host, "pages", json!({ "chapter": { "url": "https://evil.example/title/a/1-chapter-1" } })).is_err());
        assert!(call(&host, "pages", json!({ "chapter": { "url": "title/a/../../browse" } })).is_err());
        assert_eq!(call(&host, "episodes", json!({})), Err("unknown op: episodes".into()));
        assert!(host.sent().is_empty() && host.pages_read().is_empty());
        // the site's check page is the app's error, passed on
        assert!(call(&host, "popular", json!({ "page": 9 })).unwrap_err().contains("Security check"));
    }
}
