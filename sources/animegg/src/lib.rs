//! AnimeGG (<https://www.animegg.org>), an anime source plug-in for Kurobayomi. Ported from the Kohi-den `en/animegg`
//! extension (Apache-2.0). This file is the source's ops; the selectors and parsers are in `api.rs`, the glue to the app
//! is the `kuro_guest` toolkit.

mod api;

use kuro_guest::wire::{Hoster, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The source id: the MD5 of `animegg/en/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 2247987965040486818;
const NAME: &str = "AnimeGG";

/// One GET of a site page through the host, which refuses any host outside `hosts` in `manifest.toml`.
fn get(host: &dyn Host, url: &str, referer: &str) -> Res<String> {
    Request::get(url).header("Referer", referer).text(host, NAME)
}

fn page(host: &dyn Host, url: &str) -> Res<String> {
    get(host, url, &format!("{}/", api::BASE))
}

/// The videos of one player. A player whose page fails gives none, so the others still play (the Kotlin's
/// `parallelCatchingFlatMap`).
fn hoster_videos(host: &dyn Host, hoster: &Hoster, episode_url: &str, preferred: bool) -> Vec<Video> {
    match get(host, &hoster.hoster_url, episode_url) {
        Ok(html) => api::parse_embed(&html, &hoster.hoster_url, &hoster.hoster_name, preferred),
        Err(error) => {
            host.log(2, &format!("animegg: {} failed: {error}", hoster.hoster_name));
            Vec::new()
        }
    }
}

struct AnimeGG;

impl Source for AnimeGG {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: Some(300) }
    }

    /// The video host answers 500 without the site as referer.
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(api::video_headers())
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => to_value(api::parse_list(&page(host, &api::popular_url())?)),
            "latest" => to_value(api::parse_list(&page(host, &api::latest_url())?)),
            // the Kotlin shows the popular list for an empty search
            "search" if input.query().trim().is_empty() => to_value(api::parse_list(&page(host, &api::popular_url())?)),
            "search" => to_value(api::parse_search(&page(host, &api::search_url(input.query()))?)),
            "details" => {
                let url = input.url("anime")?;
                to_value(api::parse_details(&page(host, &api::absolute(url))?, url))
            }
            "episodes" => to_value(api::parse_episodes(&page(host, &api::absolute(input.url("anime")?))?)),
            "hosters" => to_value(api::parse_hosters(&page(host, &api::absolute(input.url("episode")?))?)),
            "videos" => match input.get::<Hoster>("hoster")? {
                Some(hoster) => to_value(hoster_videos(host, &hoster, &format!("{}/", api::BASE), true)),
                None => {
                    let url = api::absolute(input.url("episode").map_err(|_| "videos needs a hoster or an episode")?);
                    let hosters = api::parse_hosters(&page(host, &url)?);
                    to_value(hosters.iter().enumerate().flat_map(|(i, h)| hoster_videos(host, h, &url, i == 0)).collect::<Vec<_>>())
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(AnimeGG);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;
    use serde_json::json;

    fn call(op: &str) -> Res<Value> {
        kuro_guest::invoke(&AnimeGG, &TestHost::default(), op, "{}").map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_kotlin_id() {
        assert_eq!(call("info").unwrap()["id"].as_i64(), Some(ID));
    }

    #[test]
    fn the_media_headers_name_the_site() {
        assert_eq!(call("request-headers").unwrap(), json!([["Referer", "https://www.animegg.org"]]));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(call("pages"), Err("unknown op: pages".to_owned()));
    }
}
