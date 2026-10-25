//! Usagi (<https://web.usagi.one>), a Russian manga source plug-in for Kurobayomi. Ported from the keiyoushi `ru/usagi`
//! extension and its GroupLe multisrc (Apache-2.0). This file is the source's ops; the URLs and parsers are in `site.rs`,
//! the glue to the app is the `kuro_guest` toolkit.

mod site;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The source id the app derives for it: `generate_id("Usagi", "ru", 1)`, the MD5 of `usagi/ru/1`, as the Kotlin
/// extension has it. Keep it: backups and the library name it.
const ID: i64 = 5705163226489175947;
const NAME: &str = "Usagi";
/// The Kotlin's default User-Agent. The site answers 404 to title and chapter pages for a browser User-Agent in some
/// countries, which is why GroupLe sends this one.
const USER_AGENT: &str = "arora";

/// The headers of a page image and of every request: the site as Referer and the Kotlin's User-Agent.
fn headers() -> Vec<(String, String)> {
    vec![("Referer".into(), "https://web.usagi.one/".into()), ("User-Agent".into(), USER_AGENT.into())]
}

/// Fetches a page of the site.
fn get(host: &dyn Host, url: &str) -> Res<String> {
    let response = Request::get(url).headers(headers()).send(host)?;
    if response.status == 404 && url.contains("mtr=") {
        return Err("Для просмотра главы необходима авторизация на сайте (ошибка 404).".into());
    }
    if !response.is_success() {
        return Err(response.error(NAME));
    }
    // the Kotlin refuses a page that redirected to another site (a moved title)
    if !response.url.is_empty() && !site::on_site(&response.url) {
        return Err(format!("{url} moved to another site: {}", response.url));
    }
    response.text(NAME)
}

struct Usagi;

impl Source for Usagi {
    fn info(&self) -> Info {
        // The Kotlin's `rateLimit(2)`: two requests a second.
        Info { id: ID, name: NAME, lang: "ru", supports_latest: true, request_gap_ms: Some(500) }
    }

    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(headers())
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let get = |url: &str| get(host, url);
        match op {
            "popular" => to_value(site::parse_catalog(&get(&site::catalog_url("RATING", input.page(), ""))?)?),
            "latest" => to_value(site::parse_catalog(&get(&site::catalog_url("DATE_UPDATE", input.page(), ""))?)?),
            "search" => to_value(site::parse_catalog(&get(&site::catalog_url("RATING", input.page(), input.query()))?)?),
            "details" => {
                let url = input.url("manga")?;
                let mut found = site::parse_details(&get(&site::page(url)?)?)?;
                found.url = url.to_owned();
                to_value(found)
            }
            "chapters" => to_value(site::parse_chapters(&get(&site::page(input.url("manga")?)?)?)?),
            "pages" => to_value(site::parse_pages(&get(&site::page(input.url("chapter")?)?)?)?.into_iter().map(Page::image).collect::<Vec<_>>()),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(Usagi);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn call(op: &str) -> Res<Value> {
        kuro_guest::invoke(&Usagi, &TestHost::default(), op, "{}").map(|out| serde_json::from_str(&out).unwrap())
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
