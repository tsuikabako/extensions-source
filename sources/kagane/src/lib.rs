//! Kagane (<https://kagane.si>), a manga source plug-in for Kurobayomi. Ported from the keiyoushi `all/kagane` extension
//! (Apache-2.0), English only. This file is the source's ops, its requests and the integrity token; the URLs and parsers
//! are in `api.rs`, the glue to the app is the `kuro_guest` toolkit.

mod api;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, read_pref, to_value, unknown_op, write_pref};
use serde_json::Value;

/// The source id: `generate_id("Kagane", "en", 1)`, the MD5 of `kagane/en/1`, as the Kotlin extension has it. Keep it:
/// backups and the library name it.
const ID: i64 = 4024736764982024684;
const NAME: &str = "Kagane";
/// Where the integrity token is kept between calls, with the time it runs out.
const INTEGRITY_PREF: &str = "integrity";
/// A token this close to its end is fetched again, so a page list never starts with one about to expire.
const INTEGRITY_MARGIN_S: u64 = 60;

/// A request to the site with the API's headers; a POST carries a JSON body.
fn request(url: &str, body: Option<&str>) -> Request {
    let request = match body {
        Some(body) => Request::post(url, body),
        None => Request::get(url),
    };
    let request = request.header("Accept", "application/json").header("Referer", format!("{}/", api::BASE)).header("Origin", api::BASE);
    if body.is_some() { request.header("Content-Type", "application/json") } else { request }
}

/// Sends a request through the host. A non-2xx answer is an error: the API's own message when it gives one.
fn send(host: &dyn Host, request: Request) -> Res<String> {
    let response = request.send(host)?;
    match response.status {
        200..=299 | 429 => response.text(NAME),
        _ => Err(api::error_message(&response.body).unwrap_or_else(|| response.error(NAME))),
    }
}

/// The list sorted by `sort` (empty: relevance), filtered by `query`; adult titles only when `show_adult`.
fn list(host: &dyn Host, page: i32, query: &str, sort: &str, show_adult: bool) -> Res<Value> {
    to_value(api::parse_search(&send(host, request(&api::search_url(page, sort), Some(&api::search_body(query, show_adult))))?)?)
}

/// The integrity token the page challenge needs: the kept one while it has time left, else a new one, kept for the next
/// chapter.
fn integrity(host: &dyn Host) -> Res<String> {
    if let Some(kept) = read_pref::<api::Integrity>(host, INTEGRITY_PREF)
        && kept.exp > host.unix_seconds() + INTEGRITY_MARGIN_S
    {
        return Ok(kept.token);
    }
    let fresh = api::parse_integrity(&send(host, request(api::INTEGRITY_URL, Some("")))?)?;
    write_pref(host, INTEGRITY_PREF, &fresh);
    Ok(fresh.token)
}

/// The page images of a chapter: the challenge with the integrity token names the cache host, the access token and the
/// pages.
fn pages(host: &dyn Host, chapter_url: &str) -> Res<Value> {
    let book = api::book_of(chapter_url)?;
    let token = integrity(host)?;
    let images = api::parse_pages(book, &send(host, request(&api::challenge_url(book), Some("{}")).header("x-integrity-token", token))?)?;
    to_value(images.into_iter().map(Page::image).collect::<Vec<_>>())
}

struct Kagane;

impl Source for Kagane {
    fn info(&self) -> Info {
        // The Kotlin keeps to 3 requests a second (`rateLimit(3)`).
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: Some(350) }
    }

    /// The page-image headers. The cache host sends JPEG or JPEG XL; the app reads both.
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(vec![("Accept".into(), "image/jxl,image/avif,image/webp,image/*,*/*".into()), ("Referer".into(), format!("{}/", api::BASE))])
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            // the Kotlin's Popular is "Popular (Total Views)", its Latest "Latest" (`updated_at`)
            "popular" => list(host, input.page(), "", "total_views", input.show_adult),
            "latest" => list(host, input.page(), "", "updated_at", input.show_adult),
            "search" => list(host, input.page(), input.query(), "", input.show_adult),
            "details" => {
                let url = input.url("manga")?;
                to_value(api::parse_details(url, &send(host, request(&api::details_url(url), None))?)?)
            }
            "chapters" => {
                let url = input.url("manga")?;
                to_value(api::parse_books(url, &send(host, request(&api::books_url(url), None))?)?)
            }
            "pages" => pages(host, input.url("chapter")?),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Kagane);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn call(op: &str) -> Res<Value> {
        kuro_guest::invoke(&Kagane, &TestHost::default(), op, "{}").map(|out| serde_json::from_str(&out).unwrap())
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
