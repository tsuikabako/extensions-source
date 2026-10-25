//! Atsumaru (<https://atsu.moe>), a manga source plug-in for Kurobayomi. Ported from the keiyoushi `en/atsumaru`
//! extension (Apache-2.0). This file is the source's ops; the URLs and parsers are in `api.rs`, the glue to the app is
//! the `kuro_guest` toolkit.

mod api;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The source id the app derives for it: `generate_id("Atsumaru", "en", 2)`, the MD5 of `atsumaru/en/2`, as the Kotlin
/// extension has it. Keep it: backups and the library name it.
const ID: i64 = 2327480808438768017;
const NAME: &str = "Atsumaru";

/// Fetches an API address with the headers the Kotlin sends.
fn get(host: &dyn Host, url: &str) -> Res<String> {
    Request::get(url).headers([("Accept", "*/*"), ("Content-Type", "application/json")]).text(host, NAME)
}

/// The chapters, with their groups' names from the title's details.
fn chapters(host: &dyn Host, manga: &str) -> Res<Value> {
    let groups = api::parse_scanlators(&get(host, &api::details_url(manga)?)?)?;
    to_value(api::parse_chapters(&get(host, &api::chapters_url(manga)?)?, manga, &groups)?)
}

struct Atsumaru;

impl Source for Atsumaru {
    fn info(&self) -> Info {
        // The Kotlin keeps to 2 requests a second (`rateLimit(2)`).
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: Some(500) }
    }

    /// Page images are AVIF on cdn.atsu.moe; the Kotlin asks for them with this Accept.
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(vec![("Accept".into(), "image/avif,image/webp,*/*".into())])
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let adult = input.show_adult;
        match op {
            "popular" => to_value(api::parse_browse(&get(host, &api::popular_url(input.page(), adult))?)?),
            "latest" => to_value(api::parse_browse(&get(host, &api::latest_url(input.page(), adult))?)?),
            "search" => to_value(api::parse_search(&get(host, &api::search_url(input.page(), input.query(), adult))?)?),
            "details" => to_value(api::parse_details(&get(host, &api::details_url(input.url("manga")?)?)?)?),
            "chapters" => chapters(host, input.url("manga")?),
            "pages" => to_value(api::parse_pages(&get(host, &api::pages_url(input.url("chapter")?)?)?)?.into_iter().map(Page::image).collect::<Vec<_>>()),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Atsumaru);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn fixture(url: &str) -> (u16, String) {
        let body = match url {
            u if u.contains("/home2/popular?") => include_str!("../fixtures/popular.json"),
            u if u.contains("/home2/recentlyUpdated?") => include_str!("../fixtures/latest.json"),
            u if u.contains("/documents/search?") => include_str!("../fixtures/search.json"),
            u if u.ends_with("/manga/page?id=2VgNt") => include_str!("../fixtures/details.json"),
            u if u.ends_with("/allChapters?mangaId=2VgNt") => include_str!("../fixtures/chapters.json"),
            u if u.ends_with("mangaId=2VgNt&chapterId=prIIkU") => include_str!("../fixtures/pages.json"),
            _ => return (404, String::new()),
        };
        (200, body.to_owned())
    }

    fn call(op: &str, input: &str) -> Res<Value> {
        let host = TestHost::answering(|request| fixture(&request.url));
        kuro_guest::invoke(&Atsumaru, &host, op, input).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info = call("info", "{}").unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(500)));
        assert_eq!(info["supportsLatest"], true);
    }

    #[test]
    fn every_op_reads_its_answer() {
        assert_eq!(call("popular", r#"{"args":{"page":1}}"#).unwrap()["mangas"].as_array().unwrap().len(), 40);
        assert_eq!(call("latest", r#"{"args":{"page":1}}"#).unwrap()["mangas"][0]["url"], "P-fE");
        assert_eq!(call("search", r#"{"args":{"page":1,"query":"witch hat"}}"#).unwrap()["mangas"][0]["url"], "2VgNt");
        assert_eq!(call("details", r#"{"args":{"manga":{"url":"2VgNt"}}}"#).unwrap()["title"], "Witch Hat Atelier");
        let chapters = call("chapters", r#"{"args":{"manga":{"url":"2VgNt"}}}"#).unwrap();
        assert_eq!((chapters[0]["url"].as_str(), chapters[0]["scanlator"].as_str()), (Some("2VgNt/prIIkU"), Some("Thunder")));
        let pages = call("pages", r#"{"args":{"chapter":{"url":"2VgNt/prIIkU"}}}"#).unwrap();
        assert_eq!(pages.as_array().unwrap().len(), 3);
        assert_eq!(call("hosters", "{}"), Err("unknown op: hosters".to_owned()));
    }
}
