//! Desu (<https://desu.uno>), a Russian manga source plug-in for Kurobayomi. Ported from the keiyoushi `ru/desu`
//! extension (Apache-2.0). This file is the source's ops; the URLs and parsers are in `api.rs`, the glue to the app is
//! the `kuro_guest` toolkit.

mod api;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The source id the Kotlin extension sets (`id = 6684416167758830305L`), which is also `MD5("desu/ru/1")`. Keep it:
/// backups and the library name it.
const ID: i64 = 6684416167758830305;
const NAME: &str = "Desu";

/// Fetches an address with the User-Agent the Kotlin sends.
fn get(host: &dyn Host, url: &str) -> Res<String> {
    Request::get(url).header("User-Agent", "Mihon (+https://github.com/keiyoushi/extensions-source)").text(host, NAME)
}

struct Desu;

impl Source for Desu {
    fn info(&self) -> Info {
        // The Kotlin allows one request every 2 seconds to the site.
        Info { id: ID, name: NAME, lang: "ru", supports_latest: true, request_gap_ms: Some(2000) }
    }

    /// The image hosts (img*.desu.uno) refuse a browser without the site as Referer.
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(vec![("Referer".into(), format!("{}/", api::BASE))])
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            // The catalog has one order (the latest updates), which the Kotlin shows for both lists.
            "popular" | "latest" => to_value(api::parse_catalog(&get(host, &api::catalog_url(input.page()))?, input.show_adult)?),
            "search" if input.query().trim().is_empty() => to_value(api::parse_catalog(&get(host, &api::catalog_url(input.page()))?, input.show_adult)?),
            "search" => to_value(api::parse_search(&get(host, &api::search_url(input.query()))?)),
            "details" => to_value(api::parse_details(&get(host, &api::details_url(input.url("manga")?)?)?)?),
            "chapters" => {
                let manga = input.url("manga")?;
                to_value(api::parse_chapters(&get(host, &api::chapters_url(manga)?)?, manga)?)
            }
            "pages" => to_value(api::parse_pages(&get(host, &api::pages_url(input.url("chapter")?)?)?)?.into_iter().map(Page::image).collect::<Vec<_>>()),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Desu);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn fixture(url: &str) -> (u16, String) {
        let body = match url {
            u if u.contains("/catalog/") => include_str!("../fixtures/catalog.json"),
            u if u.contains("/manga/search/") => include_str!("../fixtures/search.html"),
            u if u.ends_with("/29/") => include_str!("../fixtures/details.json"),
            u if u.ends_with("/29/chapters") => include_str!("../fixtures/chapters.json"),
            u if u.ends_with("/29/chapters/52364") => include_str!("../fixtures/pages.json"),
            _ => return (404, String::new()),
        };
        (200, body.to_owned())
    }

    fn call(op: &str, input: &str) -> Res<Value> {
        let host = TestHost::answering(|request| fixture(&request.url));
        kuro_guest::invoke(&Desu, &host, op, input).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info = call("info", "{}").unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(2000)));
        assert_eq!(call("request-headers", "{}").unwrap(), serde_json::json!([["Referer", "https://desu.uno/"]]));
    }

    #[test]
    fn every_op_reads_its_answer() {
        assert_eq!(call("popular", r#"{"args":{"page":1}}"#).unwrap()["mangas"].as_array().unwrap().len(), 15);
        assert_eq!(call("latest", r#"{"showAdult":true,"args":{"page":1}}"#).unwrap()["mangas"].as_array().unwrap().len(), 20);
        assert_eq!(call("search", r#"{"args":{"page":1,"query":""}}"#).unwrap()["mangas"].as_array().unwrap().len(), 15);
        assert_eq!(call("search", r#"{"args":{"page":1,"query":"naruto"}}"#).unwrap()["mangas"][0]["url"], "/29");
        assert_eq!(call("details", r#"{"args":{"manga":{"url":"/29"}}}"#).unwrap()["title"], "Naruto");
        assert_eq!(call("chapters", r#"{"args":{"manga":{"url":"/29"}}}"#).unwrap()[0]["url"], "/29/chapters/52364");
        let pages = call("pages", r#"{"args":{"chapter":{"url":"/29/chapters/52364"}}}"#).unwrap();
        assert_eq!(pages.as_array().unwrap().len(), 3);
        assert_eq!(call("hosters", "{}"), Err("unknown op: hosters".to_owned()));
    }
}
