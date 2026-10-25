//! Weeb Central (<https://weebcentral.com>), a manga source plug-in for Kurobayomi. Port of `rust-sources/weebcentral`
//! (keiyoushi `en/weebcentral`, Apache-2.0). This file is the source's ops and its own manga shapes; the URLs and parsers
//! are in `site.rs`, the glue to the app is the `kuro_guest` toolkit.

mod site;

use kuro_guest::wire::Page;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

/// The source id the app has always derived for this source: `generate_id("Weeb Central", "en", 1)`.
const ID: i64 = 2131019126180322627;
const NAME: &str = "Weeb Central";

// The manga shapes stay this source's own (not `kuro_guest::wire`): they carry `updateStrategy` and `initialized`, and
// send every unset field as `null`, as they always did. Chapters and pages use the toolkit's shapes.

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireManga {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub status: i32,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    /// The stored ordinal of the app's `UpdateStrategy`: 0 always updates, 1 fetches once.
    #[serde(default)]
    pub update_strategy: i64,
    #[serde(default)]
    pub initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireMangasPage {
    #[serde(default)]
    pub mangas: Vec<WireManga>,
    #[serde(default)]
    pub has_next_page: bool,
}

/// Fetches a page of the site. The answer is the body and the final address after redirects, which the links of the
/// page are resolved against (the Kotlin reads `document.location()`). The app spaces the requests (`requestGapMs`).
fn get(host: &dyn Host, url: &Url) -> Res<(String, Url)> {
    let response = Request::get(url.as_str()).header("Referer", "https://weebcentral.com/").send(host)?;
    let final_url = Url::parse(&response.url).unwrap_or_else(|_| url.clone());
    Ok((response.text(NAME)?, final_url))
}

fn listing(host: &dyn Host, input: &Input, query: &str, sort: Option<&str>, filters: &[Value]) -> Res<Value> {
    let url = site::listing_url(input.page(), query, sort, filters, input.show_adult)?;
    let (html, base) = get(host, &url)?;
    to_value(site::parse_listing(&html, &base)?)
}

struct WeebCentral;

impl Source for WeebCentral {
    fn info(&self) -> Info {
        // The site's own spacing (Kotlin `rateLimit(1, 2.seconds)`).
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: Some(2000) }
    }

    /// The headers of a page image: the Kotlin `imageRequest` (Accept, and Origin removed by `configureHeaders`).
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(vec![("Accept".into(), "image/avif,image/webp,*/*".into())])
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => listing(host, input, "", Some("Popularity"), &[]),
            "latest" => listing(host, input, "", Some("Latest Updates"), &[]),
            "search" => listing(host, input, input.query(), None, input.filters()),
            "details" => {
                let (html, at) = get(host, &site::page(input.url("manga")?)?)?;
                to_value(site::parse_details(&html, &at)?)
            }
            "chapters" => {
                let (html, base) = get(host, &site::chapters_url(input.url("manga")?)?)?;
                to_value(site::parse_chapters(&html, &base)?)
            }
            "pages" => {
                let (html, base) = get(host, &site::pages_url(input.url("chapter")?)?)?;
                to_value(site::parse_pages(&html, &base).into_iter().map(Page::image).collect::<Vec<_>>())
            }
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(WeebCentral);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn call(op: &str) -> Res<String> {
        kuro_guest::invoke(&WeebCentral, &TestHost::default(), op, "{}")
    }

    #[test]
    fn info_carries_the_site_spacing_and_the_image_headers_are_accept_only() {
        let info: Value = serde_json::from_str(&call("info").unwrap()).unwrap();
        assert_eq!(info["requestGapMs"], 2000);
        assert_eq!(info["supportsLatest"], true);
        let headers = call("request-headers").unwrap();
        assert_eq!(headers, r#"[["Accept","image/avif,image/webp,*/*"]]"#, "no Origin, no Referer");
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(call("hosters"), Err("unknown op: hosters".to_owned()));
    }
}
