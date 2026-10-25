//! The Madara WordPress theme, shared by every Madara site. Ported from the keiyoushi `lib-multisrc/madara` theme
//! (`Madara`, `MadaraNoAjax`, `MadaraBase`; Apache-2.0). A site is one `Site` value and one line,
//! `kuro_guest::plugin!(SITE);`; this crate holds the ops, the URLs and the parsers (`site.rs`).
//!
//! Left out: the filters, the chapter protector (encrypted page lists: such a chapter fails with a clear error), the
//! view counter and related titles. Titles are stored by their path (`/manga/<slug>/`), not by WordPress post id.

mod site;

use kuro_guest::wire::{MangasPage, Page};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

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

impl Source for Site {
    fn info(&self) -> Info {
        Info { id: self.id, name: self.name, lang: self.lang, supports_latest: true, request_gap_ms: Some(self.request_gap_ms) }
    }

    // Image hosts of Madara sites commonly refuse a request without the site as Referer.
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(vec![("Referer".into(), format!("{}/", self.base))])
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let s = self;
        match op {
            "popular" => to_value(s.list(host, input.page(), site::Order::Views, "")?),
            "latest" => to_value(s.list(host, input.page(), site::Order::Latest, "")?),
            "search" => to_value(s.list(host, input.page(), site::Order::Relevance, input.query())?),
            "details" => {
                let url = input.url("manga")?;
                let mut found = site::parse_details(s, &s.get(host, &site::page(s, url)?)?)?;
                found.url = url.to_owned();
                to_value(found)
            }
            "chapters" => {
                let url = site::page(s, input.url("manga")?)?;
                let html = match s.chapters {
                    Chapters::MangaPage => s.get(host, &url)?,
                    Chapters::MangaAjax => s.ajax(host, &site::chapters_ajax_url(&url), "")?,
                    Chapters::AdminAjax => {
                        let id = site::post_id(&s.get(host, &url)?).ok_or("the title page has no post id")?;
                        s.ajax(host, &site::ajax_url(s), &format!("action=manga_get_chapters&manga={id}"))?
                    }
                };
                to_value(site::parse_chapters(s, &html, host.unix_seconds()))
            }
            "pages" => {
                let url = site::page(s, input.url("chapter")?)?;
                let mut html = s.get(host, &url)?;
                // a chapter shown one image at a time has a list view with every image
                if site::is_single_pager(&html) {
                    html = s.get(host, &site::list_style_url(&url))?;
                }
                to_value(site::parse_pages(s, &html)?.into_iter().map(Page::image).collect::<Vec<_>>())
            }
            _ => Err(unknown_op(op)),
        }
    }
}

impl Site {
    fn referer(&self) -> String {
        format!("{}/", self.base)
    }

    fn get(&self, host: &dyn Host, url: &str) -> Res<String> {
        Request::get(url).header("Referer", self.referer()).text(host, self.name)
    }

    /// A form POST with the XHR header, as the theme's ajax endpoints want.
    fn ajax(&self, host: &dyn Host, url: &str, form: &str) -> Res<String> {
        Request::form(url, form).header("Referer", self.referer()).header("X-Requested-With", "XMLHttpRequest").text(host, self.name)
    }

    fn list(&self, host: &dyn Host, page: i32, order: site::Order, query: &str) -> Res<MangasPage> {
        match self.browse {
            Browse::Ajax => Ok(site::ajax_page(self, &self.ajax(host, &site::ajax_url(self), &site::archive_form(page, order, query))?)),
            Browse::Pages if !query.trim().is_empty() => {
                // the Kotlin answers only the first page of a search: the search pages of these sites do not page reliably
                if page > 1 {
                    return Ok(MangasPage::default());
                }
                Ok(MangasPage { mangas: site::parse_archive(self, &self.get(host, &site::search_url(self, query))?), has_next_page: false })
            }
            Browse::Pages => Ok(site::archive_page(self, &self.get(host, &site::archive_url(self, page, order))?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;
    use serde_json::json;

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

    /// Runs one op against a host that answers every request with `body`; returns the answer and the requests sent.
    fn call(site: &Site, op: &str, args: Value, body: &str) -> (Res<Value>, Vec<Request>) {
        let host = TestHost::with_body(body);
        let out = kuro_guest::invoke(site, &host, op, &json!({ "args": args }).to_string());
        (out.map(|o| serde_json::from_str(&o).unwrap()), host.sent())
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
        assert_eq!(call(&SITE, "hosters", json!({}), "").0, Err("unknown op: hosters".to_owned()));
    }

    #[test]
    fn ajax_browsing_posts_the_load_more_form() {
        let (page, sent) = call(&SITE, "popular", json!({ "page": 2 }), include_str!("../fixtures/ajax/popular.html"));
        assert_eq!(page.unwrap()["mangas"].as_array().unwrap().len(), 25);
        assert_eq!((sent[0].method.as_str(), sent[0].url.as_str()), ("POST", "https://example.org/wp-admin/admin-ajax.php"));
        let body = sent[0].body.as_deref().unwrap();
        assert!(body.starts_with("action=madara_load_more&page=1&") && body.contains("vars%5Bmeta_key%5D=_wp_manga_views"), "{body}");
    }

    #[test]
    fn ajax_chapters_post_to_the_title_page() {
        let manga = json!({ "manga": { "url": "/manga/a-title/" } });
        let (chapters, sent) = call(&SITE, "chapters", manga, include_str!("../fixtures/ajax/chapters.html"));
        assert!(chapters.unwrap().as_array().unwrap().len() > 300);
        assert_eq!(sent[0].url, "https://example.org/manga/a-title/ajax/chapters/");
    }

    #[test]
    fn a_page_search_answers_only_its_first_page() {
        let pages = Site { browse: Browse::Pages, chapters: Chapters::MangaPage, ..SITE };
        let (first, sent) = call(&pages, "search", json!({ "page": 1, "query": "magic" }), include_str!("../fixtures/pages/search.html"));
        let first = first.unwrap();
        assert_eq!((first["mangas"].as_array().unwrap().len(), first["hasNextPage"].as_bool()), (18, Some(false)));
        assert_eq!(sent[0].url, "https://example.org/?s=magic&post_type=wp-manga");
        let (second, sent) = call(&pages, "search", json!({ "page": 2, "query": "magic" }), "");
        assert!(second.unwrap()["mangas"].as_array().unwrap().is_empty() && sent.is_empty());
    }

    #[test]
    fn an_error_status_is_an_error() {
        let host = TestHost::default();
        let out = kuro_guest::invoke(&SITE, &host, "details", r#"{"args":{"manga":{"url":"/manga/x/"}}}"#);
        assert_eq!(out, Err("HTTP 404 from Example".to_owned()));
    }
}
