//! InkStory (<https://inkstory.net>), a Russian manga source plug-in for Kurobayomi. Ported from the keiyoushi
//! `ru/inkstory` extension and its `inkstory` theme (Apache-2.0). This file is the source's ops, its requests and the
//! page image fix-up; the URLs and parsers are in `api.rs`, the glue to the app is the `kuro_guest` toolkit.

mod api;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::{Value, json};

/// The source id: `generate_id("InkStory", "ru", 2)`, the MD5 of `inkstory/ru/2` (the Kotlin's `versionId = 2`). Keep
/// it: backups and the library name it.
const ID: i64 = 2026387314200968454;
const NAME: &str = "InkStory";

/// A GET of an API address with the Kotlin's headers: the API wants its own User-Agent and the service name.
fn request(url: &str) -> Request {
    Request::get(url)
        .header("User-Agent", "Tachiyomi (+https://github.com/keiyoushi/extensions-source)")
        .header("Accept", "application/json, text/plain, */*")
        .header("X-Service-Name", api::SERVICE)
}

/// The body of an API address; a non-2xx answer is an error.
fn get(host: &dyn Host, url: &str) -> Res<String> {
    request(url).text(host, NAME)
}

fn books(host: &dyn Host, page: i32, query: &str) -> Res<Value> {
    let response = request(&api::books_url(page, query)).send(host)?;
    let total = response.header("x-estimated-total-hits").and_then(|v| v.trim().parse().ok());
    to_value(api::parse_books(&response.text(NAME)?, page, total)?)
}

/// The chapter list needs the book id, which the slug in the manga url does not carry: the book is read first.
fn chapters(host: &dyn Host, slug: &str) -> Res<Value> {
    let id = api::book_id(&get(host, &api::details_url(slug))?)?;
    let branches = get(host, &api::branches_url(&id))?;
    to_value(api::parse_chapters(&branches, &get(host, &api::chapters_url(&id))?)?)
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
            "popular" => books(host, input.page(), ""),
            "search" => books(host, input.page(), input.query()),
            "latest" => to_value(api::parse_feed(&get(host, &api::feed_url(input.page()))?)?),
            "details" => to_value(api::parse_details(&get(host, &api::details_url(input.url("manga")?))?)?),
            "chapters" => chapters(host, input.url("manga")?),
            "pages" => {
                let images = api::parse_pages(&get(host, &api::pages_url(input.url("chapter")?))?)?;
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
