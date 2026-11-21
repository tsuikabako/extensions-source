//! MangaGoku (<https://www.mangagoku.com>), a manga source plug-in for Kurobayomi: the Mangakakalot catalogue under its
//! new name and layout. The selectors follow the keiyoushi MangaBox base (Apache-2.0). This file is the source's ops; the
//! URLs and parsers are in `site.rs`, the glue to the app is the `kuro_guest` toolkit.

mod site;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The source id: `generate_id("MangaGoku", "en", 1)`, the MD5 of `mangagoku/en/1`. Keep it: backups and the library
/// name it.
const ID: i64 = 8821297132723982231;
const NAME: &str = "MangaGoku";
const REFERER: &str = "https://www.mangagoku.com/";

struct MangaGoku;

impl Source for MangaGoku {
    fn info(&self) -> Info {
        // 2 requests a second, as the Kotlin MangaBox sources allow (`rateLimit(2, 1.seconds)`)
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: Some(500) }
    }

    /// The headers of a page image and a cover: the CDN answers 403 without a site as Referer.
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(vec![("Referer".into(), REFERER.into())])
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let get = |url: &str| Request::get(url).header("Referer", REFERER).text(host, NAME);
        match op {
            "popular" => to_value(site::parse_list(&get(&site::popular_url(input.page()))?)),
            "latest" => to_value(site::parse_list(&get(&site::latest_url(input.page()))?)),
            // without text the hot list shows (the Kotlin lists by its filters, which are not ported)
            "search" if input.query().trim().is_empty() => to_value(site::parse_list(&get(&site::popular_url(input.page()))?)),
            "search" => to_value(site::parse_list(&get(&site::search_url(input.page(), input.query()))?)),
            "details" => {
                let url = input.url("manga")?;
                let mut found = site::parse_details(&get(&site::page(url)?)?)?;
                found.url = url.to_owned();
                to_value(found)
            }
            "chapters" => {
                let url = input.url("manga")?;
                to_value(site::parse_chapters(&get(&site::chapters_url(url)?)?, url)?)
            }
            "pages" => to_value(site::parse_pages(&get(&site::page(input.url("chapter")?)?)?).into_iter().map(Page::image).collect::<Vec<_>>()),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(MangaGoku);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn call(op: &str) -> Res<Value> {
        kuro_guest::invoke(&MangaGoku, &TestHost::default(), op, "{}").map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info = call("info").unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(500)));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(call("hosters"), Err("unknown op: hosters".to_owned()));
    }
}
