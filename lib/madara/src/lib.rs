//! The Madara WordPress theme, shared by every Madara site plug-in. Ported from the keiyoushi `lib-multisrc/madara` theme
//! (`Madara`, `MadaraNoAjax`, `MadaraBase`; Apache-2.0). A site plug-in holds the WIT glue and one `Site`; this crate holds
//! the ops, the URLs and the parsers (`site.rs`), and reaches the network only through the `http` closure it is handed.
//!
//! Left out: the filters, the chapter protector (encrypted page lists: such a chapter fails with a clear error), the
//! view counter and related titles. Titles are stored by their path (`/manga/<slug>/`), not by WordPress post id.

mod site;

use serde::Deserialize;
use serde_json::{Value, json};

pub use site::Res;

/// One Madara site.
pub struct Site {
    /// The source id: `generate_id(name, lang, 1)`, the MD5 of `<name lower-case>/<lang>/1`, as the Kotlin extension has it.
    pub id: i64,
    pub name: &'static str,
    pub lang: &'static str,
    /// The site root without a trailing slash (`https://example.org`).
    pub base: &'static str,
    /// The folder of the title pages (`manga`; some sites use `manhua` or `series`).
    pub manga_dir: &'static str,
    pub browse: Browse,
    pub chapters: Chapters,
    /// The least gap between two requests to the site.
    pub request_gap_ms: u64,
}

/// How the site lists titles.
pub enum Browse {
    /// `admin-ajax.php` with `madara_load_more` (the Kotlin `Madara`).
    Ajax,
    /// Paged archive pages and the WordPress search page (the Kotlin `MadaraNoAjax`).
    Pages,
}

/// Where the chapter list of a title comes from (the Kotlin `ChapterMode`).
pub enum Chapters {
    /// The title page itself.
    MangaPage,
    /// A POST to `<title page>/ajax/chapters/`.
    MangaAjax,
    /// A POST to `admin-ajax.php` with `manga_get_chapters` and the title's post id.
    AdminAjax,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Args {
    page: i32,
    query: String,
    manga: Option<Ref>,
    chapter: Option<Ref>,
}

#[derive(Deserialize)]
struct Ref {
    url: String,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    args: Args,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponse {
    status: u16,
    body: String,
    #[serde(default)]
    body_encoding: Option<String>,
}

impl Site {
    fn descriptor(&self) -> Value {
        json!({ "id": self.id, "name": self.name, "lang": self.lang, "supportsLatest": true, "requestGapMs": self.request_gap_ms })
    }

    /// Answers one op of the ABI. `http` is the host's `http` import, `now` its `unix-seconds` (for "2 days ago").
    pub fn invoke(&self, op: &str, input_json: &str, http: impl Fn(&str) -> Res<String>, now: impl Fn() -> u64) -> Res<String> {
        let out = match op {
            "info" => self.descriptor(),
            "create-sources" => Value::Array(vec![self.descriptor()]),
            // ABI 1.5: the page-image headers. Image hosts of Madara sites commonly refuse a request without the site as Referer.
            "request-headers" => json!([["Referer", format!("{}/", self.base)]]),
            "popular" | "latest" | "search" | "details" | "chapters" | "pages" => {
                let envelope: Envelope = serde_json::from_str(input_json).map_err(|e| format!("invalid input: {e}"))?;
                Net { site: self, http: &http }.run(op, envelope.args, &now)?
            }
            _ => return Err(format!("unknown op: {op}")),
        };
        Ok(out.to_string())
    }
}

struct Net<'a, F> {
    site: &'a Site,
    http: &'a F,
}

impl<F: Fn(&str) -> Res<String>> Net<'_, F> {
    /// Fetches a page of the site through the host, which refuses any host outside `hosts` in the plug-in's manifest.
    /// A body makes it a form POST with the XHR header, as the theme's ajax endpoints want.
    fn fetch(&self, url: &str, form: Option<String>) -> Res<String> {
        let referer = format!("{}/", self.site.base);
        let request = match form {
            None => json!({ "method": "GET", "url": url, "headers": [["Referer", referer]] }),
            Some(body) => json!({
                "method": "POST",
                "url": url,
                "headers": [
                    ["Referer", referer],
                    ["X-Requested-With", "XMLHttpRequest"],
                    ["Content-Type", "application/x-www-form-urlencoded; charset=UTF-8"],
                ],
                "body": body,
            }),
        };
        let response: HttpResponse = serde_json::from_str(&(self.http)(&request.to_string())?).map_err(|e| format!("invalid http reply: {e}"))?;
        if !(200..300).contains(&response.status) {
            return Err(format!("HTTP {} from {url}", response.status));
        }
        if response.body_encoding.is_some() {
            return Err(format!("{url} is not UTF-8 text"));
        }
        Ok(response.body)
    }

    fn get(&self, url: &str) -> Res<String> {
        self.fetch(url, None)
    }

    fn list(&self, page: i32, order: site::Order, query: &str) -> Res<site::MangasPage> {
        let s = self.site;
        match s.browse {
            Browse::Ajax => {
                let html = self.fetch(&site::ajax_url(s), Some(site::archive_form(page, order, query)))?;
                Ok(site::ajax_page(s, &html))
            }
            Browse::Pages if !query.trim().is_empty() => {
                // the Kotlin answers only the first page of a search: the search pages of these sites do not page reliably
                if page > 1 {
                    return Ok(site::MangasPage { mangas: Vec::new(), has_next_page: false });
                }
                let html = self.get(&site::search_url(s, query))?;
                Ok(site::MangasPage { mangas: site::parse_archive(s, &html), has_next_page: false })
            }
            Browse::Pages => Ok(site::archive_page(s, &self.get(&site::archive_url(s, page, order))?)),
        }
    }

    fn run(&self, op: &str, args: Args, now: &impl Fn() -> u64) -> Res<Value> {
        let s = self.site;
        match op {
            "popular" => to_value(self.list(args.page, site::Order::Views, "")?),
            "latest" => to_value(self.list(args.page, site::Order::Latest, "")?),
            "search" => to_value(self.list(args.page, site::Order::Relevance, &args.query)?),
            "details" => {
                let manga = args.manga.ok_or("details needs args.manga")?;
                let mut found = site::parse_details(s, &self.get(&site::page(s, &manga.url)?)?)?;
                found.url = manga.url;
                to_value(found)
            }
            "chapters" => {
                let manga = args.manga.ok_or("chapters needs args.manga")?;
                let url = site::page(s, &manga.url)?;
                let html = match s.chapters {
                    Chapters::MangaPage => self.get(&url)?,
                    Chapters::MangaAjax => self.fetch(&site::chapters_ajax_url(&url), Some(String::new()))?,
                    Chapters::AdminAjax => {
                        let id = site::post_id(&self.get(&url)?).ok_or("the title page has no post id")?;
                        self.fetch(&site::ajax_url(s), Some(format!("action=manga_get_chapters&manga={id}")))?
                    }
                };
                to_value(site::parse_chapters(s, &html, now()))
            }
            "pages" => {
                let chapter = args.chapter.ok_or("pages needs args.chapter")?;
                let url = site::page(s, &chapter.url)?;
                let mut html = self.get(&url)?;
                // a chapter shown one image at a time has a list view with every image
                if site::is_single_pager(&html) {
                    html = self.get(&site::list_style_url(&url))?;
                }
                let pages = site::parse_pages(s, &html)?;
                to_value(pages.into_iter().map(|image| json!({ "url": "", "imageUrl": image })).collect::<Vec<_>>())
            }
            _ => Err(format!("unknown op: {op}")),
        }
    }
}

fn to_value<T: serde::Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const SITE: Site = Site {
        id: 1,
        name: "Example",
        lang: "en",
        base: "https://example.org",
        manga_dir: "manga",
        browse: Browse::Ajax,
        chapters: Chapters::MangaAjax,
        request_gap_ms: 500,
    };

    /// Runs one op against a fake host that answers every request with `body` and records the requests.
    fn call(site: &Site, op: &str, args: Value, body: &str) -> (Res<Value>, Vec<Value>) {
        let sent = RefCell::new(Vec::new());
        let http = |request: &str| {
            sent.borrow_mut().push(serde_json::from_str::<Value>(request).unwrap());
            Ok(json!({ "status": 200, "body": body }).to_string())
        };
        let out = site.invoke(op, &json!({ "args": args }).to_string(), http, || 0);
        (out.map(|o| serde_json::from_str(&o).unwrap()), sent.into_inner())
    }

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let (info, _) = call(&SITE, "info", json!({}), "");
        let info = info.unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(1), Some(500)));
        let (headers, _) = call(&SITE, "request-headers", json!({}), "");
        assert_eq!(headers.unwrap(), json!([["Referer", "https://example.org/"]]));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(SITE.invoke("hosters", "{}", |_| Ok(String::new()), || 0), Err("unknown op: hosters".to_owned()));
    }

    #[test]
    fn ajax_browsing_posts_the_load_more_form() {
        let (page, sent) = call(&SITE, "popular", json!({ "page": 2 }), include_str!("../fixtures/ajax/popular.html"));
        assert_eq!(page.unwrap()["mangas"].as_array().unwrap().len(), 25);
        assert_eq!((sent[0]["method"].as_str(), sent[0]["url"].as_str()), (Some("POST"), Some("https://example.org/wp-admin/admin-ajax.php")));
        let body = sent[0]["body"].as_str().unwrap();
        assert!(body.starts_with("action=madara_load_more&page=1&") && body.contains("vars%5Bmeta_key%5D=_wp_manga_views"), "{body}");
    }

    #[test]
    fn ajax_chapters_post_to_the_title_page() {
        let manga = json!({ "manga": { "url": "/manga/a-title/" } });
        let (chapters, sent) = call(&SITE, "chapters", manga, include_str!("../fixtures/ajax/chapters.html"));
        assert!(chapters.unwrap().as_array().unwrap().len() > 300);
        assert_eq!(sent[0]["url"], "https://example.org/manga/a-title/ajax/chapters/");
    }

    #[test]
    fn a_page_search_answers_only_its_first_page() {
        let pages = Site { browse: Browse::Pages, chapters: Chapters::MangaPage, ..SITE };
        let (first, sent) = call(&pages, "search", json!({ "page": 1, "query": "magic" }), include_str!("../fixtures/pages/search.html"));
        let first = first.unwrap();
        assert_eq!((first["mangas"].as_array().unwrap().len(), first["hasNextPage"].as_bool()), (18, Some(false)));
        assert_eq!(sent[0]["url"], "https://example.org/?s=magic&post_type=wp-manga");
        let (second, sent) = call(&pages, "search", json!({ "page": 2, "query": "magic" }), "");
        assert!(second.unwrap()["mangas"].as_array().unwrap().is_empty() && sent.is_empty());
    }

    #[test]
    fn an_error_status_is_an_error() {
        let http = |_: &str| Ok(json!({ "status": 404, "body": "" }).to_string());
        let out = SITE.invoke("details", r#"{"args":{"manga":{"url":"/manga/x/"}}}"#, http, || 0);
        assert_eq!(out, Err("HTTP 404 from https://example.org/manga/x/".to_owned()));
    }
}
