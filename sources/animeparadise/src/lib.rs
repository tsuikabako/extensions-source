//! AnimeParadise (<https://www.animeparadise.moe>), an anime source plug-in for Kurobayomi. Ported from the Kohi-den
//! `en/animeparadise` extension (Apache-2.0) and brought up to the site's current API (see `api.rs`). This file is the
//! source's ops; the addresses and parsers are in `api.rs`, the glue to the app is the `kuro_guest` toolkit.

mod api;

use kuro_guest::wire::Hoster;
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde_json::Value;

/// The source id: the MD5 of `animeparadise/en/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 3319373396687010408;
const NAME: &str = "AnimeParadise";

/// One GET to the API through the host, which refuses any host outside `hosts` in `manifest.toml`. A non-2xx answer is
/// an error: the API's own message when it gives one.
fn get(host: &dyn Host, url: &str) -> Res<String> {
    let response = Request::get(url)
        .header("Accept", "application/json, text/plain, */*")
        .header("Origin", api::SITE)
        .header("Referer", format!("{}/", api::SITE))
        .send(host)?;
    if !response.is_success()
        && response.status != 429
        && let Some(message) = api::error_message(&response.body)
    {
        return Err(message);
    }
    response.text(NAME)
}

/// The episode's one hoster, its video already read (`videoList`), so the app asks nothing more.
fn hoster_of(host: &dyn Host, episode_url: &str) -> Res<Hoster> {
    let video = api::parse_video(&get(host, &api::episode_api_url(episode_url)?)?)?;
    Ok(Hoster { hoster_url: episode_url.to_owned(), hoster_name: NAME.to_owned(), video_list: Some(vec![video]), ..Default::default() })
}

struct AnimeParadise;

impl Source for AnimeParadise {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: Some(300) }
    }

    /// The headers of the stream and subtitle requests.
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(api::media_headers())
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let adult = input.show_adult;
        match op {
            // the site's own lists: by popularity, and by when an anime was posted
            "popular" => to_value(api::parse_list(&get(host, &api::search_url(input.page(), "", "POPULARITY"))?, adult)?),
            "latest" => to_value(api::parse_list(&get(host, &api::search_url(input.page(), "", "POST_DATE"))?, adult)?),
            "search" => to_value(api::parse_list(&get(host, &api::search_url(input.page(), input.query(), ""))?, adult)?),
            "details" => to_value(api::parse_details(&get(host, &api::details_url(&api::link_of(input.url("anime")?)?.slug))?)?),
            "episodes" => to_value(api::parse_episodes(&get(host, &api::episodes_url(&api::link_of(input.url("anime")?)?.id))?)?),
            "hosters" => to_value(vec![hoster_of(host, input.url("episode")?)?]),
            "videos" => {
                let url = match input.get::<Hoster>("hoster")? {
                    Some(hoster) => hoster.hoster_url,
                    None => input.url("episode").map_err(|_| "videos needs a hoster or an episode")?.to_owned(),
                };
                to_value(hoster_of(host, &url)?.video_list.unwrap_or_default())
            }
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(AnimeParadise);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;
    use serde_json::json;

    fn call(op: &str) -> Res<Value> {
        kuro_guest::invoke(&AnimeParadise, &TestHost::default(), op, "{}").map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_carries_the_kotlin_id() {
        let info = call("info").unwrap();
        assert_eq!(info["id"].as_i64(), Some(ID));
        assert_eq!(info["supportsLatest"], true);
    }

    #[test]
    fn the_media_headers_name_the_site() {
        assert_eq!(call("request-headers").unwrap(), json!([["Referer", "https://www.animeparadise.moe/"], ["Origin", "https://www.animeparadise.moe"]]));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(call("pages"), Err("unknown op: pages".to_owned()));
    }
}
