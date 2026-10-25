//! EShadow (<https://eshadow.net>), an Arabic manga source plug-in for Kurobayomi. Ported from the keiyoushi
//! `ar/eshadow` extension (Apache-2.0). This file is the source's ops; the API's URLs and parsers are in `api.rs`, the
//! glue to the app is the `kuro_guest` toolkit.

mod api;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The source id the app derives for it: `generate_id("EShadow", "ar", 1)`, the MD5 of `eshadow/ar/1`, as the Kotlin
/// extension has it. Keep it: backups and the library name it.
const ID: i64 = 8617573161604126293;
const NAME: &str = "EShadow";

fn get(host: &dyn Host, url: &str) -> Res<String> {
    Request::get(url).text(host, NAME)
}

struct EShadow;

impl Source for EShadow {
    fn info(&self) -> Info {
        // The site has one list (no latest, as in the Kotlin) and no stated limit; one request a second.
        Info { id: ID, name: NAME, lang: "ar", supports_latest: false, request_gap_ms: Some(1000) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => to_value(api::parse_list(&get(host, &api::list_url(input.page(), ""))?)?),
            "search" => to_value(api::parse_list(&get(host, &api::list_url(input.page(), input.query()))?)?),
            "details" => {
                let url = input.url("manga")?;
                let mut found = api::parse_details(&get(host, &api::manga_url(url)?)?)?;
                found.url = url.to_owned();
                to_value(found)
            }
            "chapters" => to_value(api::parse_chapters(&get(host, &api::manga_url(input.url("manga")?)?)?)?),
            "pages" => to_value(api::parse_pages(&get(host, &api::pages_url(input.url("chapter")?)?)?)?.into_iter().map(Page::image).collect::<Vec<_>>()),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(EShadow);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    const ONE_PIECE: &str = "90a69c73-7adf-4f51-9344-2ce469c40e11";

    fn fixture(url: &str) -> (u16, String) {
        let body = match url {
            u if u.starts_with("https://eshadow.net/api/manga?") => include_str!("../fixtures/list.json"),
            u if u.ends_with(ONE_PIECE) => include_str!("../fixtures/manga.json"),
            u if u.ends_with("/api/chapters/cmpdynfr40003c01s1i5jxb6r") => include_str!("../fixtures/chapter.json"),
            _ => return (404, String::new()),
        };
        (200, body.to_owned())
    }

    fn call(op: &str, input: &str) -> Res<Value> {
        let host = TestHost::answering(|request| fixture(&request.url));
        kuro_guest::invoke(&EShadow, &host, op, input).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_says_there_is_no_latest() {
        let info = call("info", "{}").unwrap();
        assert_eq!((info["id"].as_i64(), info["supportsLatest"].as_bool()), (Some(ID), Some(false)));
        assert_eq!(call("latest", "{}"), Err("unknown op: latest".to_owned()));
    }

    #[test]
    fn every_op_reads_its_answer() {
        let popular = call("popular", r#"{"args":{"page":1}}"#).unwrap();
        assert_eq!((popular["mangas"].as_array().unwrap().len(), popular["hasNextPage"].as_bool()), (7, Some(false)));
        assert_eq!(popular["mangas"][5]["url"], format!("/manga/one-piece#{ONE_PIECE}"));
        let manga = format!(r#"{{"args":{{"manga":{{"url":"/manga/one-piece#{ONE_PIECE}"}}}}}}"#);
        assert_eq!(call("details", &manga).unwrap()["title"], "ONE PIECE || ون بيس");
        // the Kotlin's stored form, the bare id, still answers
        assert_eq!(call("details", &format!(r#"{{"args":{{"manga":{{"url":"{ONE_PIECE}"}}}}}}"#)).unwrap()["status"], 1);
        let chapters = call("chapters", &manga).unwrap();
        assert_eq!(chapters[0]["chapterNumber"], 1185.0);
        let last = chapters.as_array().unwrap().last().unwrap();
        assert_eq!(last["url"], "/read/cmpdynfr40003c01s1i5jxb6r");
        let pages = call("pages", r#"{"args":{"chapter":{"url":"/read/cmpdynfr40003c01s1i5jxb6r"}}}"#).unwrap();
        assert!(pages.as_array().unwrap().len() > 10);
        assert_eq!(call("pages", r#"{"args":{"chapter":{"url":"cmpdynfr40003c01s1i5jxb6r"}}}"#).unwrap(), pages);
    }
}
