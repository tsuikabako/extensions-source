//! MangaTime (<https://mangatime.org>), an Arabic manga source plug-in for Kurobayomi. Ported from the keiyoushi
//! `ar/mangatime` extension (Apache-2.0). This file is the source's ops; the API's URLs and parsers are in `api.rs`, the
//! glue to the app is the `kuro_guest` toolkit.

mod api;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The source id the app derives for it: `generate_id("MangaTime", "ar", 1)`, the MD5 of `mangatime/ar/1`, as the
/// Kotlin extension has it. Keep it: backups and the library name it.
const ID: i64 = 215553151312092548;
const NAME: &str = "MangaTime";

fn get(host: &dyn Host, url: &str) -> Res<String> {
    Request::get(url).text(host, NAME)
}

struct MangaTime;

impl Source for MangaTime {
    fn info(&self) -> Info {
        // The site has no stated limit; one request a second keeps well under any.
        Info { id: ID, name: NAME, lang: "ar", supports_latest: true, request_gap_ms: Some(1000) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => to_value(api::parse_list(&get(host, &api::list_url(input.page(), "popularity", ""))?)?),
            "latest" => to_value(api::parse_list(&get(host, &api::list_url(input.page(), "recent", ""))?)?),
            "search" => to_value(api::parse_list(&get(host, &api::list_url(input.page(), "popularity", input.query()))?)?),
            "details" => {
                let url = input.url("manga")?;
                let mut found = api::parse_details(&get(host, &api::details_url(url)?)?)?;
                found.url = url.to_owned();
                to_value(found)
            }
            "chapters" => {
                let url = input.url("manga")?;
                to_value(api::parse_chapters(&get(host, &api::chapters_url(url)?)?, url)?)
            }
            "pages" => to_value(api::parse_pages(&get(host, &api::pages_url(input.url("chapter")?)?)?)?.into_iter().map(Page::image).collect::<Vec<_>>()),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(MangaTime);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn fixture(url: &str) -> (u16, String) {
        let body = match url {
            u if u.contains("searchSeries") && u.contains("query") => include_str!("../fixtures/search.json"),
            u if u.contains("searchSeries") => include_str!("../fixtures/popular.json"),
            u if u.contains("getSeriesBySlug") => include_str!("../fixtures/series.json"),
            u if u.contains("getChapters") => include_str!("../fixtures/chapters.json"),
            u if u.contains("getChapterPages") => include_str!("../fixtures/pages.json"),
            _ => return (404, String::new()),
        };
        (200, body.to_owned())
    }

    fn call(op: &str, input: &str) -> Res<Value> {
        let host = TestHost::answering(|request| fixture(&request.url));
        kuro_guest::invoke(&MangaTime, &host, op, input).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info = call("info", "{}").unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(1000)));
        assert_eq!(call("hosters", "{}"), Err("unknown op: hosters".to_owned()));
    }

    #[test]
    fn every_op_reads_its_answer() {
        let popular = call("popular", r#"{"args":{"page":1}}"#).unwrap();
        assert_eq!((popular["mangas"].as_array().unwrap().len(), popular["hasNextPage"].as_bool()), (6, Some(true)));
        assert_eq!(popular["mangas"][0]["url"], "/manga/blue-lock#697df7820c5d340ac154519f");
        assert_eq!(call("search", r#"{"args":{"page":1,"query":"blue"}}"#).unwrap()["mangas"].as_array().unwrap().len(), 3);
        let manga = r#"{"args":{"manga":{"url":"/manga/blue-lock#697df7820c5d340ac154519f"}}}"#;
        assert_eq!(call("details", manga).unwrap()["title"], "بلو لوك");
        let chapters = call("chapters", manga).unwrap();
        assert_eq!(chapters[0]["url"], "/manga/blue-lock/chapter/365");
        let pages = call("pages", r#"{"args":{"chapter":{"url":"/manga/blue-lock/chapter/365"}}}"#).unwrap();
        assert_eq!(pages.as_array().unwrap().len(), 22);
    }
}
