//! The MangaThemesia WordPress theme (once WPMangaStream and WPMangaReader), shared by every MangaThemesia site. Ported
//! from the keiyoushi `lib-multisrc/mangathemesia` theme (`MangaThemesia`, `MangaThemesiaAlt`; Apache-2.0). A site is one
//! `Site` value, `Site { id, name, lang, base, ..Site::THEME }` plus the selectors it changes, and one line,
//! `kuro_guest::plugin!(SITE);`; this crate holds the ops, the URLs and the parsers (`site.rs`).
//!
//! Left out: the filters, the view counter (`dynamic_view_ajax`), related titles, and chapters whose images come in
//! halves to be joined side by side (Hijala's `#chapter-pages-js-before`), which fail with a clear error.

mod site;

use std::collections::HashMap;

use kuro_guest::wire::{MangasPage, Page};
use kuro_guest::{Host, Info, Input, Request, Res, Source, read_pref, to_value, unknown_op, write_pref};
use serde_json::Value;

/// One MangaThemesia site. The selectors are the Kotlin's `open val`s and `open fun`s a site overrides.
pub struct Site {
    /// The source id: the Kotlin's `id`, or `generate_id(name, lang, versionId)`.
    pub id: i64,
    pub name: &'static str,
    pub lang: &'static str,
    /// The site root without a trailing slash (`https://example.org`).
    pub base: &'static str,
    /// The folder of the title list (`/manga`). Title pages may live elsewhere (Hijala's are at the root).
    pub manga_dir: &'static str,
    /// Title slugs carry a changing number prefix (`/manga/2072267132-a-title/`): titles are stored without it and found
    /// through the site's text list, `<manga_dir>/list-mode/` (the Kotlin `MangaThemesiaAlt`).
    pub random_slugs: bool,
    /// A title card of a list.
    pub list_card: &'static str,
    /// The title's text in a card; "" takes the `title` of the card's link, which is also the fallback.
    pub list_title: &'static str,
    pub next_page: &'static str,
    /// The block of a title page that holds the details below.
    pub details: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub genre: &'static str,
    /// The status text; "" looks for a row labelled "Status" (in the theme's languages).
    pub status: &'static str,
    pub thumbnail: &'static str,
    pub chapter_item: &'static str,
    /// The chapter's name in an item; the item's link text when absent.
    pub chapter_name: &'static str,
    pub chapter_date: &'static str,
    /// The page images of a chapter.
    pub pages: &'static str,
    /// The least gap between two requests to the site.
    pub request_gap_ms: u64,
}

impl Site {
    /// The theme's defaults; a site sets its own id, name, lang and base and what it changes.
    pub const THEME: Site = Site {
        id: 0,
        name: "",
        lang: "",
        base: "",
        manga_dir: "/manga",
        random_slugs: false,
        list_card: ".utao .uta .imgu, .listupd .bs .bsx, .listo .bs .bsx",
        list_title: "",
        next_page: "div.pagination .next, div.hpage .r",
        details: "div.bigcontent, div.animefull, div.main-info, div.postbody",
        title: ".entry-title, .ts-breadcrumb li:last-child span",
        description: ".desc, .entry-content[itemprop=description]",
        genre: "div.gnr a, .mgen a, .seriestugenre a",
        status: "",
        thumbnail: ".infomanga > div[itemprop=image] img, .thumb img",
        chapter_item: "div.bxcl li, div.cl li, #chapterlist li, ul li:has(div.chbox):has(div.eph-num)",
        chapter_name: ".lch a, .chapternum",
        chapter_date: ".chapterdate",
        pages: "div#readerarea img",
        // the theme has no stated limit; one request a second keeps well under any
        request_gap_ms: 1000,
    };
}

/// The preference that keeps the random-slug map (permanent slug to the site's current slug).
const URL_MAP: &str = "url_map";

impl Source for Site {
    fn info(&self) -> Info {
        Info { id: self.id, name: self.name, lang: self.lang, supports_latest: true, request_gap_ms: Some(self.request_gap_ms) }
    }

    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(vec![("Referer".into(), format!("{}/", self.base))])
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            // the Kotlin's popular and latest are the search page in an order
            "popular" => to_value(self.list(host, input.page(), "popular", "")?),
            "latest" => to_value(self.list(host, input.page(), "update", "")?),
            "search" => to_value(self.list(host, input.page(), "", input.query())?),
            "details" => {
                let url = input.url("manga")?;
                let mut found = site::parse_details(self, &self.title_page(host, url)?)?;
                found.url = url.to_owned();
                to_value(found)
            }
            "chapters" => to_value(site::parse_chapters(self, &self.title_page(host, input.url("manga")?)?)),
            "pages" => {
                let html = self.get(host, &site::page(self, input.url("chapter")?)?)?;
                to_value(site::parse_pages(self, &html)?.into_iter().map(Page::image).collect::<Vec<_>>())
            }
            _ => Err(unknown_op(op)),
        }
    }
}

impl Site {
    fn get(&self, host: &dyn Host, url: &str) -> Res<String> {
        Request::get(url).header("Referer", format!("{}/", self.base)).text(host, self.name)
    }

    fn list(&self, host: &dyn Host, page: i32, order: &str, query: &str) -> Res<MangasPage> {
        let mut found = site::parse_list(self, &self.get(host, &site::list_url(self, page, order, query))?);
        if self.random_slugs {
            for manga in &mut found.mangas {
                manga.url = format!("{}/{}/", self.manga_dir, site::permanent_slug(&manga.url));
            }
        }
        Ok(found)
    }

    /// A title page. With random slugs the stored `/manga/<slug>/` is looked up in the saved map, and the map fetched
    /// again when the title is not in it or its address no longer answers.
    fn title_page(&self, host: &dyn Host, stored: &str) -> Res<String> {
        if !self.random_slugs {
            return self.get(host, &site::page(self, stored)?);
        }
        let slug = site::permanent_slug(stored);
        let title_url = |current: &str| format!("{}{}/{current}/", self.base, self.manga_dir);
        let saved: HashMap<String, String> = read_pref(host, URL_MAP).unwrap_or_default();
        if let Some(current) = saved.get(slug) {
            let response = Request::get(title_url(current)).header("Referer", format!("{}/", self.base)).send(host)?;
            if response.status != 404 {
                return response.text(self.name);
            }
        }
        let map = site::parse_url_map(&self.get(host, &format!("{}{}/list-mode/", self.base, self.manga_dir))?);
        write_pref(host, URL_MAP, &map);
        self.get(host, &title_url(map.get(slug).map_or(slug, String::as_str)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;
    use serde_json::json;

    const SITE: Site = Site { id: 1, name: "Example", lang: "ar", base: "https://example.org", ..Site::THEME };
    const RANDOM: Site = Site { random_slugs: true, ..SITE };

    fn call(site: &Site, host: &TestHost, op: &str, args: Value) -> Res<Value> {
        kuro_guest::invoke(site, host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str(&o).unwrap())
    }

    #[test]
    fn info_carries_the_id_spacing_and_referer() {
        let host = TestHost::default();
        let info = call(&SITE, &host, "info", json!({})).unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(1), Some(1000)));
        assert_eq!(call(&SITE, &host, "request-headers", json!({})).unwrap(), json!([["Referer", "https://example.org/"]]));
        assert_eq!(call(&SITE, &host, "hosters", json!({})), Err("unknown op: hosters".to_owned()));
    }

    #[test]
    fn lists_are_the_search_page_in_an_order() {
        let host = TestHost::with_body(include_str!("../fixtures/hijala/popular.html"));
        let page = call(&SITE, &host, "popular", json!({ "page": 2 })).unwrap();
        assert!(page["hasNextPage"].as_bool().unwrap());
        call(&SITE, &host, "latest", json!({ "page": 1 })).unwrap();
        call(&SITE, &host, "search", json!({ "page": 1, "query": "solo leveling" })).unwrap();
        let urls: Vec<String> = host.sent().into_iter().map(|r| r.url).collect();
        assert_eq!(
            urls,
            [
                "https://example.org/manga/?page=2&order=popular",
                "https://example.org/manga/?page=1&order=update",
                "https://example.org/manga/?title=solo+leveling&page=1",
            ]
        );
    }

    #[test]
    fn random_slugs_are_stored_without_their_prefix() {
        let host = TestHost::with_body(include_str!("../fixtures/lavascans/popular.html"));
        let site = Site { list_card: ".listupd .manga-card-v", list_title: "h3 a", ..RANDOM };
        let page = call(&site, &host, "popular", json!({ "page": 1 })).unwrap();
        assert_eq!(page["mangas"][0]["url"], "/manga/logging-10-000-years-into-the-future/");
    }

    #[test]
    fn a_random_slug_title_is_found_through_the_list_and_the_map_is_kept() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            "https://example.org/manga/list-mode/" => (200, include_str!("../fixtures/lavascans/list-mode.html").into()),
            "https://example.org/manga/2072267132-after-merging-with-the-goddess-i-am-invincible/" => (200, include_str!("../fixtures/lavascans/manga.html").into()),
            _ => (404, String::new()),
        });
        let manga = json!({ "manga": { "url": "/manga/after-merging-with-the-goddess-i-am-invincible/" } });
        let site = Site { details: "div.lh-container", title: ".lh-title", ..RANDOM };
        assert_eq!(call(&site, &host, "details", manga.clone()).unwrap()["title"], "after merging with the goddess i am invincible");
        // the second time the saved map answers at once
        call(&site, &host, "chapters", manga).unwrap();
        let urls: Vec<String> = host.sent().into_iter().map(|r| r.url).collect();
        assert_eq!(urls.iter().filter(|u| u.ends_with("/list-mode/")).count(), 1, "{urls:?}");
        assert_eq!(urls.len(), 3);
    }

    #[test]
    fn a_stale_saved_slug_fetches_the_list_again() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            "https://example.org/manga/list-mode/" => (200, include_str!("../fixtures/lavascans/list-mode.html").into()),
            "https://example.org/manga/2072267132-after-merging-with-the-goddess-i-am-invincible/" => (200, "<div class='main-info'><h1 class='entry-title'>A</h1></div>".into()),
            _ => (404, String::new()),
        });
        write_pref(&host, URL_MAP, &HashMap::from([("after-merging-with-the-goddess-i-am-invincible", "111-after-merging-with-the-goddess-i-am-invincible")]));
        let manga = json!({ "manga": { "url": "/manga/after-merging-with-the-goddess-i-am-invincible/" } });
        assert_eq!(call(&RANDOM, &host, "details", manga).unwrap()["title"], "A");
        assert_eq!(host.sent().len(), 3);
    }
}
