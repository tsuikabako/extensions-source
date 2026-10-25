//! MangaDex (<https://mangadex.org>), a manga source plug-in for Kurobayomi. Ported from the keiyoushi `all/mangadex`
//! extension (Apache-2.0), English only. This file is the source's ops; the URLs and parsers are in `api.rs`, the glue to
//! the app is the `kuro_guest` toolkit.

mod api;

use kuro_guest::wire::{MangasPage, Page};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The source id the app derives for it: `generate_id("MangaDex", "en", 1)`, the MD5 of `mangadex/en/1`, as the Kotlin
/// extension has it. Keep it: backups and the library name it.
const ID: i64 = 2499283573021220255;
const NAME: &str = "MangaDex";

/// Fetches an API address. A non-2xx answer is an error: the API's own message when it gives one.
fn get(host: &dyn Host, url: &str) -> Res<String> {
    let response = Request::get(url).header("Accept", "application/json").send(host)?;
    if !response.is_success() && response.status != 429 {
        if let Some(message) = api::error_message(&response.body) {
            return Err(message);
        }
    }
    response.text(NAME)
}

fn popular(host: &dyn Host, page: i32) -> Res<Value> {
    to_value(api::parse_mangas(&get(host, &api::popular_url(page, &api::thirty_days_before(host.unix_seconds())))?)?)
}

/// The newest chapters name the manga; their titles and covers come from one more request.
fn latest(host: &dyn Host, page: i32) -> Res<Value> {
    let (ids, more) = api::parse_latest_ids(&get(host, &api::latest_url(page))?)?;
    if ids.is_empty() {
        return to_value(MangasPage::default());
    }
    to_value(api::parse_in_order(&get(host, &api::by_ids_url(&ids))?, &ids, more)?)
}

/// Every page of the chapter feed, newest first.
fn chapters(host: &dyn Host, manga: &str) -> Res<Value> {
    let mut all = Vec::new();
    let mut offset = 0;
    loop {
        let page = api::parse_feed(&get(host, &api::feed_url(manga, offset)?)?)?;
        all.extend(page.chapters);
        match page.next_offset {
            Some(next) => offset = next,
            None => return to_value(all),
        }
    }
}

struct MangaDex;

impl Source for MangaDex {
    fn info(&self) -> Info {
        // The API allows about 5 requests a second; the Kotlin keeps to 3 (`rateLimit(3)`).
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: Some(350) }
    }

    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(vec![("Accept".into(), "image/avif,image/webp,*/*".into())])
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => popular(host, input.page()),
            "latest" => latest(host, input.page()),
            "search" => to_value(api::parse_mangas(&get(host, &api::search_url(input.page(), input.query()))?)?),
            "details" => to_value(api::parse_details(&get(host, &api::details_url(input.url("manga")?)?)?)?),
            "chapters" => chapters(host, input.url("manga")?),
            "pages" => to_value(api::parse_pages(&get(host, &api::at_home_url(input.url("chapter")?)?)?)?.into_iter().map(Page::image).collect::<Vec<_>>()),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(MangaDex);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn call(op: &str) -> Res<Value> {
        kuro_guest::invoke(&MangaDex, &TestHost::default(), op, "{}").map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info = call("info").unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(350)));
        assert_eq!(info["supportsLatest"], true);
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(call("hosters"), Err("unknown op: hosters".to_owned()));
    }
}
