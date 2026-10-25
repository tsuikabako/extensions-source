//! LeerCapitulo (<https://www.leercapitulo.co>), a Spanish manga source plug-in for Kurobayomi. Ported from the
//! keiyoushi `es/leercapitulo` extension (Apache-2.0). This file is the source's ops; the URLs and parsers are in
//! `site.rs`, the glue to the app is the `kuro_guest` toolkit.

mod site;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The source id the app derives for it: `generate_id("LeerCapitulo", "es", 1)`, the MD5 of `leercapitulo/es/1`, as the
/// Kotlin extension has it. Keep it: backups and the library name it.
const ID: i64 = 5621901596522308538;
const NAME: &str = "LeerCapitulo";

struct LeerCapitulo;

impl Source for LeerCapitulo {
    fn info(&self) -> Info {
        // The Kotlin allows one request every 3 s to the site.
        Info { id: ID, name: NAME, lang: "es", supports_latest: true, request_gap_ms: Some(3000) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let get = |url: &str| Request::get(url).text(host, NAME);
        match op {
            "popular" => to_value(site::parse_popular(&get(&site::home_url())?)),
            "latest" => to_value(site::parse_latest(&get(&site::home_url())?)),
            "search" => to_value(site::parse_search(&get(&site::search_url(input.page(), input.query()))?)),
            "details" => {
                let url = input.url("manga")?;
                let mut found = site::parse_details(&get(&site::page(url)?)?)?;
                found.url = url.to_owned();
                to_value(found)
            }
            "chapters" => to_value(site::parse_chapters(&get(&site::page(input.url("manga")?)?)?)),
            "pages" => to_value(site::parse_pages(&get(&site::page(input.url("chapter")?)?)?).into_iter().map(Page::image).collect::<Vec<_>>()),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(LeerCapitulo);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn call(host: &TestHost, op: &str, input: &str) -> Res<Value> {
        kuro_guest::invoke(&LeerCapitulo, host, op, input).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info = call(&TestHost::default(), "info", "{}").unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(3000)));
    }

    #[test]
    fn the_ops_fetch_the_site_pages() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            "https://www.leercapitulo.co/" => (200, include_str!("../fixtures/home.html").into()),
            "https://www.leercapitulo.co/manga/?q=love&page=2" => (200, include_str!("../fixtures/search.html").into()),
            "https://www.leercapitulo.co/manga/psvkfbmjgo/one-piece/" => (200, include_str!("../fixtures/manga.html").into()),
            "https://www.leercapitulo.co/leer/psvkfbmjgo/one-piece/1196/" => (200, include_str!("../fixtures/chapter.html").into()),
            _ => (404, String::new()),
        });
        assert_eq!(call(&host, "popular", r#"{"args":{"page":1}}"#).unwrap()["mangas"].as_array().unwrap().len(), 4);
        assert_eq!(call(&host, "latest", r#"{"args":{"page":1}}"#).unwrap()["mangas"].as_array().unwrap().len(), 5);
        assert_eq!(call(&host, "search", r#"{"args":{"page":2,"query":"love"}}"#).unwrap()["hasNextPage"], true);
        let manga = r#"{"args":{"manga":{"url":"/manga/psvkfbmjgo/one-piece/"}}}"#;
        let details = call(&host, "details", manga).unwrap();
        assert_eq!((details["url"].as_str(), details["title"].as_str()), (Some("/manga/psvkfbmjgo/one-piece/"), Some("One Piece")));
        assert_eq!(call(&host, "chapters", manga).unwrap().as_array().unwrap().len(), 5);
        let pages = call(&host, "pages", r#"{"args":{"chapter":{"url":"/leer/psvkfbmjgo/one-piece/1196/"}}}"#).unwrap();
        assert_eq!(pages.as_array().unwrap().len(), 17);
        assert_eq!(call(&host, "hosters", "{}"), Err("unknown op: hosters".to_owned()));
    }
}
