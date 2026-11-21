//! InkStory (<https://inkstory.net>), a Russian manga source plug-in for Kurobayomi. Ported from the keiyoushi
//! `ru/inkstory` extension and its `inkstory` theme (Apache-2.0). This file is the source's ops, its requests and the
//! page image fix-up; the addresses and parsers are in `api.rs`, the server function encoding in `seroval.rs`, the glue
//! to the app is the `kuro_guest` toolkit.

mod api;
mod seroval;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::{Value, json};

/// The source id: `generate_id("InkStory", "ru", 2)`, the MD5 of `inkstory/ru/2` (the Kotlin's `versionId = 2`). Keep
/// it: backups and the library name it.
const ID: i64 = 2026387314200968454;
const NAME: &str = "InkStory";

/// The User-Agent the Kotlin sends ("required by source").
const USER_AGENT: &str = "Tachiyomi (+https://github.com/keiyoushi/extensions-source)";

/// A server function call on the site, with the headers its server functions want. The site's Anubis check answers a
/// plain client with its own page; the app then sends the call again from its hidden browser page, where the check
/// clears (an app newer than 0.4.0, the first to know Anubis), and the page's browser adds `Sec-Fetch-Site`.
fn call(host: &dyn Host, url: &str) -> Res<String> {
    Request::get(url).header("User-Agent", USER_AGENT).header("Accept", "application/json").header("x-tsr-serverFn", "true").text(host, NAME)
}

/// The shared API's update feed, with the Kotlin's headers: its own User-Agent and the service name.
fn feed(host: &dyn Host, page: i32) -> Res<Value> {
    let body = Request::get(api::feed_url(page))
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json, text/plain, */*")
        .header("X-Service-Name", api::SERVICE)
        .text(host, NAME)?;
    to_value(api::parse_feed(&body)?)
}

/// The chapter list needs the book id, which the slug in the manga url does not carry: the book is read first.
fn chapters(host: &dyn Host, slug: &str) -> Res<Value> {
    let id = api::book_id(&call(host, &api::book_url(slug))?)?;
    to_value(api::parse_chapters(&call(host, &api::chapters_url(&id))?)?)
}

/// ABI 1.5 image hook: XOR-ciphered page images read back; anything else shown as fetched.
fn unscramble(body: &str) -> Res<Value> {
    let bytes = STANDARD.decode(body).map_err(|e| format!("body is not base64: {e}"))?;
    Ok(json!({ "body": api::unxor(&bytes).map(|plain| STANDARD.encode(plain)) }))
}

struct InkStory;

impl Source for InkStory {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "ru", supports_latest: true, request_gap_ms: None }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => to_value(api::parse_catalogue(&call(host, &api::catalogue_url(input.page(), ""))?, input.page())?),
            "search" => to_value(api::parse_catalogue(&call(host, &api::catalogue_url(input.page(), input.query()))?, input.page())?),
            "latest" => feed(host, input.page()),
            "details" => to_value(api::parse_details(&call(host, &api::book_url(input.url("manga")?))?)?),
            "chapters" => chapters(host, input.url("manga")?),
            "pages" => {
                let images = api::parse_pages(&call(host, &api::chapter_url(input.url("chapter")?))?)?;
                to_value(images.into_iter().map(Page::image).collect::<Vec<_>>())
            }
            // `args.body`: the fetched image, base64.
            "unscramble" => unscramble(&input.get::<String>("body")?.unwrap_or_default()),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(InkStory);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn call(op: &str, input: &str) -> Res<String> {
        kuro_guest::invoke(&InkStory, &TestHost::default(), op, input)
    }

    #[test]
    fn info_carries_the_kotlin_id() {
        let info: Value = serde_json::from_str(&call("info", "{}").unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["lang"].as_str()), (Some(ID), Some("ru")));
    }

    #[test]
    fn the_unscramble_op_reads_a_xor_page_and_leaves_a_plain_one() {
        let ciphered = STANDARD.encode(include_bytes!("../fixtures/page-start.bin"));
        let out: Value = serde_json::from_str(&call("unscramble", &json!({"args": {"body": ciphered, "headers": []}}).to_string()).unwrap()).unwrap();
        let plain = STANDARD.decode(out["body"].as_str().unwrap()).unwrap();
        assert!(plain.starts_with(&[0xFF, 0xD8, 0xFF]));
        let again = json!({"args": {"body": STANDARD.encode(&plain), "headers": []}}).to_string();
        assert_eq!(call("unscramble", &again), Ok(r#"{"body":null}"#.to_owned()));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(call("hosters", "{}"), Err("unknown op: hosters".to_owned()));
    }
}
