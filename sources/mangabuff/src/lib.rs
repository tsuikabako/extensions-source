//! MangaBuff (<https://mangabuff.ru>), a Russian manga source plug-in for Kurobayomi. Ported from the keiyoushi
//! `ru/mangabuff` extension (Apache-2.0). This file is the source's ops; the URLs and parsers are in `site.rs`, the glue
//! to the app is the `kuro_guest` toolkit.

mod site;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// `MD5("mangabuff/ru/1")`, the id the Kotlin extension gets from its name, language and version. Keep it: backups and
/// the library name it.
const ID: i64 = 4008409601887986192;
const NAME: &str = "MangaBuff";

fn get(host: &dyn Host, url: &str) -> Res<String> {
    Request::get(url).text(host, NAME)
}

/// A title page. A 404 is what the site answers for a title it shows only to signed-in readers.
fn title_page(host: &dyn Host, manga_url: &str) -> Res<String> {
    let response = Request::get(site::page(manga_url)?).send(host)?;
    if response.status == 404 {
        return Err("Контент не доступен. / This title is only shown to signed-in readers.".into());
    }
    response.text(NAME)
}

/// The chapters past the first 100, from `chapters/load` with the page's CSRF token. A 419 (expired token) is asked
/// again once with a fresh token, as the Kotlin's token interceptor does.
fn load_more(host: &dyn Host, manga_url: &str, id: &str, token: &str) -> Res<String> {
    let send = |token: &str| {
        Request::form(site::chapters_load_url(), format!("manga_id={id}"))
            .header("X-Requested-With", "XMLHttpRequest")
            .header("X-CSRF-TOKEN", token)
            .header("Referer", site::page(manga_url)?)
            .send(host)
    };
    let response = send(token)?;
    if response.status != 419 {
        return response.text(NAME);
    }
    let fresh = site::more_chapters(&title_page(host, manga_url)?)?.ok_or("Unable to find CSRF token")?.1;
    send(&fresh)?.text(NAME)
}

struct MangaBuff;

impl Source for MangaBuff {
    fn info(&self) -> Info {
        // The Kotlin allows one request a second to title pages and the chapter list; the gap covers every request.
        Info { id: ID, name: NAME, lang: "ru", supports_latest: true, request_gap_ms: Some(1000) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => to_value(site::parse_list(&get(host, &site::catalog_url(input.page(), "real_views"))?)),
            "latest" => to_value(site::parse_list(&get(host, &site::catalog_url(input.page(), "updated_at"))?)),
            "search" if input.query().trim().is_empty() => to_value(site::parse_list(&get(host, &site::catalog_url(input.page(), "real_views"))?)),
            "search" => to_value(site::parse_list(&get(host, &site::search_url(input.page(), input.query())?)?)),
            "details" => {
                let manga = input.url("manga")?;
                to_value(site::parse_details(&title_page(host, manga)?, manga)?)
            }
            "chapters" => {
                let manga = input.url("manga")?;
                let html = title_page(host, manga)?;
                let mut chapters = site::parse_chapters(&html);
                if let Some((id, token)) = site::more_chapters(&html)? {
                    chapters.extend(site::parse_load(&load_more(host, manga, &id, &token)?)?);
                }
                to_value(chapters)
            }
            "pages" => to_value(site::parse_pages(&get(host, &site::page(input.url("chapter")?)?)?).into_iter().map(Page::image).collect::<Vec<_>>()),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(MangaBuff);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn fixture(method: &str, url: &str) -> (u16, String) {
        let body = match url {
            u if method == "POST" && u.ends_with("/chapters/load") => include_str!("../fixtures/load.json"),
            u if u.contains("/manga?sort=") => include_str!("../fixtures/catalog.html"),
            u if u.contains("/search?") => include_str!("../fixtures/search.html"),
            u if u.ends_with("/manga/bezgranichnyi-volshebnik") => include_str!("../fixtures/details.html"),
            u if u.ends_with("/manga/bezgranichnyi-volshebnik/1/1") => include_str!("../fixtures/reader.html"),
            _ => return (404, String::new()),
        };
        (200, body.to_owned())
    }

    fn call(op: &str, input: &str) -> Res<Value> {
        let host = TestHost::answering(|request| fixture(&request.method, &request.url));
        kuro_guest::invoke(&MangaBuff, &host, op, input).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info = call("info", "{}").unwrap();
        assert_eq!((info["id"].as_i64(), info["requestGapMs"].as_u64()), (Some(ID), Some(1000)));
    }

    #[test]
    fn every_op_reads_its_answer() {
        assert_eq!(call("popular", r#"{"args":{"page":1}}"#).unwrap()["mangas"].as_array().unwrap().len(), 30);
        assert_eq!(call("latest", r#"{"args":{"page":2}}"#).unwrap()["mangas"].as_array().unwrap().len(), 30);
        assert_eq!(call("search", r#"{"args":{"page":1,"query":""}}"#).unwrap()["mangas"].as_array().unwrap().len(), 30);
        assert!(!call("search", r#"{"args":{"page":1,"query":"башня"}}"#).unwrap()["mangas"].as_array().unwrap().is_empty());
        assert!(call("search", r#"{"args":{"page":1,"query":"ба"}}"#).is_err());
        let manga = r#"{"args":{"manga":{"url":"/manga/bezgranichnyi-volshebnik"}}}"#;
        assert_eq!(call("details", manga).unwrap()["title"], "Безграничный маг");
        let chapters = call("chapters", manga).unwrap();
        assert_eq!(chapters.as_array().unwrap().len(), 192);
        assert_eq!(chapters[0]["url"], "/manga/bezgranichnyi-volshebnik/1/192");
        let pages = call("pages", r#"{"args":{"chapter":{"url":"/manga/bezgranichnyi-volshebnik/1/1"}}}"#).unwrap();
        assert_eq!(pages.as_array().unwrap().len(), 22);
        assert!(call("details", r#"{"args":{"manga":{"url":"/manga/hidden"}}}"#).unwrap_err().contains("signed-in"));
        assert_eq!(call("episodes", "{}"), Err("unknown op: episodes".to_owned()));
    }
}
