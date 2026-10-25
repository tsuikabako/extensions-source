//! NovelCool (<https://www.novelcool.com> and a subdomain per language), shared by its language sources. Ported from the
//! keiyoushi `all/novelcool` extension (Apache-2.0), which makes one source per language. A language is one `Site` value
//! and one line, `kuro_guest::plugin!(SITE);`; this crate holds the ops, the URLs and the parsers (`site.rs`).
//!
//! Manga only, as the Kotlin: the site also hosts novels. The lists come from the app API (the Kotlin's default); the
//! Kotlin's HTML lists and its filters (author, genres, status, rating), which only work without the API, are left out.
//! Chapters and pages are read on the site itself, not through the partner reader domains the chapter links lead to.

mod site;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// One language of NovelCool.
pub struct Site {
    /// The source id: `generate_id("NovelCool", lang, 1)`, the MD5 of `novelcool/<lang>/1`, as the Kotlin extension has it.
    pub id: i64,
    pub name: &'static str,
    /// The app's language, also the API's (the Kotlin sends `br` for `pt-BR`; that language is not ported).
    pub lang: &'static str,
    /// The site root without a trailing slash (`https://es.novelcool.com`).
    pub base: &'static str,
}

impl Site {
    fn get(&self, host: &dyn Host, url: &str) -> Res<String> {
        Request::get(url).text(host, self.name)
    }

    fn list(&self, host: &dyn Host, input: &Input, endpoint: &str, keyword: Option<&str>) -> Res<Value> {
        let body = site::api_body(self.lang, input.page(), keyword);
        let json = Request::post(format!("{}/{endpoint}/", site::API), body).header("Content-Type", "application/json").text(host, self.name)?;
        to_value(site::parse_api_list(&json, input.show_adult)?)
    }
}

impl Source for Site {
    fn info(&self) -> Info {
        // The Kotlin allows one request a second.
        Info { id: self.id, name: self.name, lang: self.lang, supports_latest: true, request_gap_ms: Some(1000) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let s = self;
        match op {
            "popular" => s.list(host, input, "elite/hot", None),
            "latest" => s.list(host, input, "elite/latest", None),
            "search" => s.list(host, input, "book/search", Some(input.query())),
            "details" => {
                let url = input.url("manga")?;
                let mut found = site::parse_details(&s.get(host, &site::page(s.base, url)?)?)?;
                found.url = url.to_owned();
                to_value(found)
            }
            "chapters" => {
                let title = site::parse_title_page(&s.get(host, &site::page(s.base, input.url("manga")?)?)?);
                let Some(newest) = title.newest else { return to_value(Vec::<Value>::new()) };
                to_value(site::parse_chapters(&s.get(host, &site::page(s.base, &newest)?)?, &newest, &title.dated))
            }
            "pages" => {
                let (mut images, others) = site::parse_reader(&s.get(host, &site::reader_url(s.base, input.url("chapter")?)?)?);
                for other in others {
                    images.extend(site::parse_reader(&s.get(host, &site::page(s.base, &other)?)?).0);
                }
                to_value(images.into_iter().map(Page::image).collect::<Vec<_>>())
            }
            _ => Err(unknown_op(op)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    const SITE: Site = Site { id: 7570101320206504111, name: "NovelCool", lang: "en", base: "https://www.novelcool.com" };

    fn call(host: &TestHost, op: &str, input: &str) -> Res<Value> {
        kuro_guest::invoke(&SITE, host, op, input).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn the_ops_use_the_api_and_the_site_pages() {
        let host = TestHost::answering(|request| match (request.method.as_str(), request.url.as_str()) {
            ("POST", "https://api.novelcool.com/elite/hot/" | "https://api.novelcool.com/elite/latest/" | "https://api.novelcool.com/book/search/") => {
                (200, include_str!("../fixtures/hot.json").into())
            }
            ("GET", "https://www.novelcool.com/novel/Sakamoto-Days.html") => (200, include_str!("../fixtures/manga.html").into()),
            ("GET", "https://www.novelcool.com/chapter/Ch-280/14940329.html") => (200, include_str!("../fixtures/reader.html").into()),
            ("GET", "https://www.novelcool.com/chapter/Ch-280/14940329-10-1.html" | "https://www.novelcool.com/chapter/Ch-280/14940329-10-2.html" | "https://www.novelcool.com/chapter/Ch-280/14940329-10-3.html") => {
                (200, include_str!("../fixtures/reader_10.html").into())
            }
            _ => (404, String::new()),
        });
        for op in ["popular", "latest", "search"] {
            assert_eq!(call(&host, op, r#"{"showAdult":true,"args":{"page":1,"query":"x"}}"#).unwrap()["mangas"].as_array().unwrap().len(), 20, "{op}");
        }
        let sent = host.sent();
        assert_eq!(sent[2].headers, [("Content-Type".to_owned(), "application/json".to_owned())]);
        assert!(sent[2].body.as_deref().unwrap().contains(r#""keyword":"x""#));
        let manga = r#"{"args":{"manga":{"url":"/novel/Sakamoto-Days.html"}}}"#;
        assert_eq!(call(&host, "details", manga).unwrap()["title"], "Sakamoto Days");
        assert_eq!(call(&host, "chapters", manga).unwrap().as_array().unwrap().len(), 6);
        // three reader pages of ten images (the fixture answers each of them)
        let pages = call(&host, "pages", r#"{"args":{"chapter":{"url":"/chapter/Ch-280/14940329.html"}}}"#).unwrap();
        assert_eq!(pages.as_array().unwrap().len(), 30);
        assert_eq!(call(&host, "hosters", "{}"), Err("unknown op: hosters".to_owned()));
    }
}
