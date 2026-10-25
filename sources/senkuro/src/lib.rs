//! Senkuro (<https://senkuro.com>), a Russian manga source plug-in for Kurobayomi. Ported from the keiyoushi `ru/senkuro`
//! extension (Apache-2.0). This file is the source's ops and its one request path, which picks the mirror that answers;
//! the queries and parsers are in `api.rs`, the glue to the app is the `kuro_guest` toolkit.

mod api;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, read_pref, to_value, unknown_op, write_pref};
use serde_json::Value;

/// The source id: `generate_id("Senkuro", "ru", 1)`, the MD5 of `senkuro/ru/1`, as the Kotlin extension has it. Keep it:
/// backups and the library name it.
const ID: i64 = 1452500215874655811;
const NAME: &str = "Senkuro";
/// The mirror that answered last, tried first next time.
const MIRROR_PREF: &str = "mirror";

/// Posts one GraphQL request to one mirror. Anything but a JSON answer (a refused address, DDoS-Guard's geo-block page,
/// an HTTP error) is an error.
fn post(host: &dyn Host, base: &str, body: &str) -> Res<String> {
    let response = Request::post(api::api_url(base), body)
        .header("Content-Type", "application/json")
        .header("User-Agent", "Tachiyomi (+https://github.com/keiyoushi/extensions-source)")
        .header("App-Id", "4026531840100")
        .header("App-Version", "060626")
        .send(host)?;
    match response.status {
        200..=299 if response.body.trim_start().starts_with('{') => Ok(response.body),
        200..=299 => Err(format!("{base} sent no JSON (blocked from this network?)")),
        429 => Err(response.error(NAME)),
        status => Err(format!("HTTP {status} from {base}")),
    }
}

/// Sends a GraphQL request to the mirror that answered last, then to the other one, and keeps the one that answers.
fn send(host: &dyn Host, body: &str) -> Res<String> {
    let mut mirrors = api::MIRRORS;
    if read_pref::<String>(host, MIRROR_PREF).is_some_and(|m| m == mirrors[1]) {
        mirrors.reverse();
    }
    let mut errors = Vec::new();
    for (i, base) in mirrors.iter().enumerate() {
        match post(host, base, body) {
            Ok(answer) => {
                if i > 0 {
                    write_pref(host, MIRROR_PREF, base);
                }
                return Ok(answer);
            }
            Err(error) => errors.push(error),
        }
    }
    Err(errors.join("; "))
}

struct Senkuro;

impl Source for Senkuro {
    fn info(&self) -> Info {
        // The Kotlin keeps to 3 requests a second (`rateLimit(3)`).
        Info { id: ID, name: NAME, lang: "ru", supports_latest: false, request_gap_ms: Some(350) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            // the Kotlin's Popular is the search sorted by popularity, which search keeps
            "popular" => to_value(api::parse_search(&send(host, &api::search_body(input.page(), "", input.show_adult))?)?),
            "search" => to_value(api::parse_search(&send(host, &api::search_body(input.page(), input.query(), input.show_adult))?)?),
            "details" => to_value(api::parse_details(&send(host, &api::details_body(input.url("manga")?))?)?),
            "chapters" => {
                let url = input.url("manga")?;
                to_value(api::parse_chapters(url, &send(host, &api::chapters_body(url))?)?)
            }
            "pages" => {
                let images = api::parse_pages(&send(host, &api::pages_body(input.url("chapter")?)?)?)?;
                to_value(images.into_iter().map(Page::image).collect::<Vec<_>>())
            }
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Senkuro);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn call(op: &str) -> Res<Value> {
        kuro_guest::invoke(&Senkuro, &TestHost::default(), op, "{}").map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info = call("info").unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(350)));
        assert_eq!((info["lang"].as_str(), info["supportsLatest"].as_bool()), (Some("ru"), Some(false)));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(call("latest"), Err("unknown op: latest".to_owned()));
    }
}
