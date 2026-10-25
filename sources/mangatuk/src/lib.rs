//! MangaTuk (<https://mangatuk.com>), an Arabic manga source plug-in for Kurobayomi. Ported from the keiyoushi
//! `ar/mangatuk` extension (Apache-2.0). This file is the source's ops; the API's URLs and parsers are in `api.rs`, the
//! glue to the app is the `kuro_guest` toolkit.

mod api;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The source id the app derives for it: `generate_id("MangaTuk", "ar", 2)`, the MD5 of `mangatuk/ar/2` (the Kotlin's
/// `versionId = 2`). Keep it: backups and the library name it.
const ID: i64 = 6924751245735287034;
const NAME: &str = "MangaTuk";

fn get(host: &dyn Host, url: &str) -> Res<String> {
    Request::get(url).text(host, NAME)
}

struct MangaTuk;

impl Source for MangaTuk {
    fn info(&self) -> Info {
        // The site has no stated limit; one request a second keeps well under any.
        Info { id: ID, name: NAME, lang: "ar", supports_latest: true, request_gap_ms: Some(1000) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let page = input.page().max(1);
        match op {
            "popular" => to_value(api::parse_list(&get(host, &api::list_url(page, "popular", ""))?, page)?),
            "latest" => to_value(api::parse_list(&get(host, &api::list_url(page, "latest_updates", ""))?, page)?),
            "search" => to_value(api::parse_list(&get(host, &api::list_url(page, "popular", input.query()))?, page)?),
            "details" => {
                let url = input.url("manga")?;
                let mut found = api::parse_details(&get(host, &api::series_url(url)?)?)?;
                found.url = url.to_owned();
                to_value(found)
            }
            "chapters" => to_value(api::parse_chapters(&get(host, &api::series_url(input.url("manga")?)?)?)?),
            "pages" => to_value(api::parse_pages(&get(host, &api::pages_url(input.url("chapter")?)?)?)?.into_iter().map(Page::image).collect::<Vec<_>>()),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(MangaTuk);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn fixture(url: &str) -> (u16, String) {
        let body = match url {
            u if u.contains("/catalog/search?") => include_str!("../fixtures/search.json"),
            u if u.ends_with("/catalog/series/by-slug/tears-on-a-withered-flower") => include_str!("../fixtures/series.json"),
            u if u.ends_with("/catalog/series/wpser_8063be9ecbddf5cb92a38a1bcc") => include_str!("../fixtures/series.json"),
            u if u.ends_with("/catalog/chapters/wpchap_1db3d2687bb7ee500aa8961957/pages") => include_str!("../fixtures/pages.json"),
            _ => return (404, String::new()),
        };
        (200, body.to_owned())
    }

    fn call(op: &str, input: &str) -> Res<Value> {
        let host = TestHost::answering(|request| fixture(&request.url));
        kuro_guest::invoke(&MangaTuk, &host, op, input).map(|out| serde_json::from_str(&out).unwrap())
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
        assert_eq!((popular["mangas"].as_array().unwrap().len(), popular["hasNextPage"].as_bool()), (5, Some(true)));
        assert_eq!(popular["mangas"][0]["url"], "/series/tears-on-a-withered-flower");
        let manga = r#"{"args":{"manga":{"url":"/series/tears-on-a-withered-flower"}}}"#;
        assert_eq!(call("details", manga).unwrap()["title"], "Tears on a Withered Flower");
        // the Kotlin's stored form, the series id, still answers
        assert_eq!(call("details", r#"{"args":{"manga":{"url":"wpser_8063be9ecbddf5cb92a38a1bcc"}}}"#).unwrap()["author"], "Gae");
        let chapters = call("chapters", manga).unwrap();
        let first = &chapters.as_array().unwrap().last().unwrap()["url"];
        assert_eq!(first, "/series/tears-on-a-withered-flower/1#wpchap_1db3d2687bb7ee500aa8961957");
        let pages = call("pages", &format!(r#"{{"args":{{"chapter":{{"url":{first}}}}}}}"#)).unwrap();
        assert_eq!(pages.as_array().unwrap().len(), 4);
        assert_eq!(call("pages", r#"{"args":{"chapter":{"url":"wpchap_1db3d2687bb7ee500aa8961957"}}}"#).unwrap().as_array().unwrap().len(), 4);
    }
}
