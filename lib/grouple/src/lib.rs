//! The GroupLe theme, shared by the Russian sites of the GroupLe network (Usagi, ReadManga, SelfManga). Ported from the
//! keiyoushi `lib-multisrc/grouple` theme (Apache-2.0, base version 43). A site is one `Site` value and one line,
//! `kuro_guest::plugin!(SITE);`; this crate holds the ops, and the URLs and parsers are in `site.rs`.
//!
//! Left out: the filters, the sign-in and the user hash, the User-Agent setting and related titles.

mod site;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// One GroupLe site.
pub struct Site {
    /// The source id the Kotlin extension has. Keep it: backups and the library name it.
    pub id: i64,
    pub name: &'static str,
    pub lang: &'static str,
    /// The site root without a trailing slash (`https://web.usagi.one`).
    pub base: &'static str,
}

/// The Kotlin's default User-Agent. Usagi answers 404 to title and chapter pages for a browser User-Agent in some
/// countries, which is why GroupLe sends this one.
const USER_AGENT: &str = "arora";

impl Site {
    /// The headers of a page image and of every request: the site as Referer and the Kotlin's User-Agent.
    fn headers(&self) -> Vec<(String, String)> {
        vec![("Referer".into(), format!("{}/", self.base)), ("User-Agent".into(), USER_AGENT.into())]
    }

    /// Fetches a page of the site.
    fn get(&self, host: &dyn Host, url: &str) -> Res<String> {
        let response = Request::get(url).headers(self.headers()).send(host)?;
        if response.status == 404 && url.contains("mtr=") {
            return Err("Для просмотра главы необходима авторизация на сайте (ошибка 404).".into());
        }
        if !response.is_success() {
            return Err(response.error(self.name));
        }
        // the Kotlin refuses a page that redirected to another site (a moved title)
        if !response.url.is_empty() && !site::on_site(self, &response.url) {
            return Err(format!("{url} moved to another site: {}", response.url));
        }
        response.text(self.name)
    }
}

impl Source for Site {
    fn info(&self) -> Info {
        // The Kotlin's `rateLimit(2)`: two requests a second.
        Info { id: self.id, name: self.name, lang: self.lang, supports_latest: true, request_gap_ms: Some(500) }
    }

    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(self.headers())
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let s = self;
        let get = |url: &str| s.get(host, url);
        match op {
            "popular" => to_value(site::parse_catalog(s, &get(&site::catalog_url(s, "RATING", input.page(), ""))?)?),
            "latest" => to_value(site::parse_catalog(s, &get(&site::catalog_url(s, "DATE_UPDATE", input.page(), ""))?)?),
            "search" => to_value(site::parse_catalog(s, &get(&site::catalog_url(s, "RATING", input.page(), input.query()))?)?),
            "details" => {
                let url = input.url("manga")?;
                let mut found = site::parse_details(s, &get(&site::page(s, url)?)?)?;
                found.url = url.to_owned();
                to_value(found)
            }
            "chapters" => to_value(site::parse_chapters(s, &get(&site::page(s, input.url("manga")?)?)?)?),
            "pages" => to_value(site::parse_pages(s, &get(&site::page(s, input.url("chapter")?)?)?)?.into_iter().map(Page::image).collect::<Vec<_>>()),
            _ => Err(unknown_op(op)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;
    use serde_json::json;

    const SITE: Site = Site { id: 1, name: "Example", lang: "ru", base: "https://example.org" };

    fn call(op: &str, args: Value, host: &TestHost) -> Res<Value> {
        kuro_guest::invoke(&SITE, host, op, &json!({ "args": args }).to_string()).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info = call("info", json!({}), &TestHost::default()).unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(1), Some(500)));
        let headers = call("request-headers", json!({}), &TestHost::default()).unwrap();
        assert_eq!(headers, json!([["Referer", "https://example.org/"], ["User-Agent", "arora"]]));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(call("hosters", json!({}), &TestHost::default()), Err("unknown op: hosters".to_owned()));
    }

    #[test]
    fn requests_go_to_the_site_with_its_headers() {
        let host = TestHost::with_body(include_str!("../fixtures/popular.json"));
        assert_eq!(call("popular", json!({ "page": 1 }), &host).unwrap()["mangas"].as_array().unwrap().len(), 6);
        let sent = host.sent();
        assert_eq!(sent[0].url, "https://example.org/api/catalog/search?offset=0&sortType=RATING");
        assert!(sent[0].headers.contains(&("User-Agent".to_owned(), "arora".to_owned())));
    }

    #[test]
    fn a_missing_chapter_asks_for_a_sign_in_and_other_errors_name_the_site() {
        let chapter = json!({ "chapter": { "url": "/a/vol1/1?mtr=true" } });
        assert_eq!(call("pages", chapter, &TestHost::default()), Err("Для просмотра главы необходима авторизация на сайте (ошибка 404).".to_owned()));
        assert_eq!(call("details", json!({ "manga": { "url": "/a" } }), &TestHost::default()), Err("HTTP 404 from Example".to_owned()));
    }
}
