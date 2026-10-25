//! KickAssAnime (<https://kaa.lt>), an anime source plug-in for Kurobayomi, anime ops only. The logic is ported from the
//! yuzono `en/kickassanime` extension (Apache-2.0); see README.md. This file is the source's ops; the API addresses and
//! parsers are in `api.rs`, the video extraction in `extract.rs`, the glue to the app is the `kuro_guest` toolkit.

mod api;
mod extract;

use kuro_guest::wire::{AnimesPage, Hoster, Track, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde::Deserialize;
use serde_json::Value;

/// The source id: the MD5 of `kickassanime/en/1`, as the Kotlin extension and Kurobayomi's built-in id use. Keep it.
const ID: i64 = 2081771513481539599;
const NAME: &str = "KickAssAnime";
/// The Kotlin settings at their defaults. The app has no settings store for plug-ins yet.
const AUDIO_FIRST: &str = "ja-JP";
const AUDIO_SECOND: &str = "en-US";
const QUALITY: &str = "1080p";
/// The preferred server (the first of the Kotlin `SERVERS`).
const SERVER: &str = "VidStreaming";

// ================================ HTTP =================================

/// One GET through the host. A non-2xx status is an error, as in the Kotlin client.
fn get(host: &dyn Host, url: &str, headers: &[(String, String)]) -> Res<String> {
    Request::get(url).headers(headers.iter().cloned()).text(host, NAME)
}

fn warn(host: &dyn Host, message: String) {
    host.log(2, &message);
}

// ================================ Ops ==================================

/// The part of a video the `subtitles` op reads.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoIn {
    #[serde(default)]
    subtitle_tracks: Vec<Track>,
}

/// A search (Kotlin `getSearchAnime`): a slug or a link names one show; a sub-page filter replaces the search; no text
/// browses with the filters; text posts them with the query. The Referer is the search page of the query.
fn search(host: &dyn Host, page: i32, query: &str, filters: &[Value]) -> Res<AnimesPage> {
    if let Some(slug) = api::slug_of_query(query)? {
        let anime = details(host, &format!("/{slug}"))?;
        return Ok(AnimesPage { animes: vec![anime], has_next_page: false });
    }
    let headers = [
        ("Accept".to_owned(), "application/json, text/plain, */*".to_owned()),
        ("Referer".to_owned(), api::search_referer(query)),
        ("Content-Type".to_owned(), "application/json".to_owned()),
    ];
    if let Some(subpage) = api::subpage(filters) {
        let body = get(host, &api::subpage_url(subpage, page), &headers)?;
        return api::parse_subpage(subpage, &body, page);
    }
    let filters = api::encoded_filters(filters);
    if query.trim().is_empty() {
        let body = get(host, &api::browse_url(page, filters.as_deref()), &headers)?;
        api::parse_search(&body, page)
    } else {
        let request = Request::post(api::FSEARCH_URL, api::search_body(page, query, filters.as_deref())).headers(headers);
        api::parse_search(&request.text(host, NAME)?, page)
    }
}

fn languages(host: &dyn Host, anime_url: &str) -> Res<Vec<String>> {
    api::parse_languages(&get(host, &api::language_url(anime_url), &[])?)
}

/// The details of one show, read for a search by slug or a link.
fn details(host: &dyn Host, anime_url: &str) -> Res<kuro_guest::wire::Anime> {
    let body = get(host, &api::details_url(anime_url), &[])?;
    api::parse_details(&body, &languages(host, anime_url)?)
}

/// The episodes in the first language that has any: the preferred one, the second, then the site's order. Newest
/// first. A page after the first that fails is skipped.
fn episodes(host: &dyn Host, anime: &str) -> Res<Vec<kuro_guest::wire::Episode>> {
    for lang in api::language_order(languages(host, anime)?, AUDIO_FIRST, AUDIO_SECOND) {
        let first = match get(host, &api::episodes_url(anime, 1, &lang), &[]).and_then(|b| api::parse_episode_page(&b)) {
            Ok(page) if !page.is_empty() => page,
            _ => continue,
        };
        let mut list = first.episodes(anime, &lang);
        for number in 2..=first.page_count() {
            match get(host, &api::episodes_url(anime, number, &lang), &[]).and_then(|b| api::parse_episode_page(&b)) {
                Ok(page) => list.extend(page.episodes(anime, &lang)),
                Err(err) => warn(host, format!("kickassanime: episode page {number} failed: {err}")),
            }
        }
        list.reverse();
        return Ok(list);
    }
    Ok(Vec::new())
}

fn servers(host: &dyn Host, episode_url: &str) -> Res<Vec<api::Server>> {
    api::parse_servers(&get(host, &api::servers_url(episode_url), &[])?)
}

/// One hoster per server, in the site's order. Its videos are read later, with the `videos` op.
fn hosters(host: &dyn Host, episode: &str) -> Res<Vec<Hoster>> {
    Ok(servers(host, episode)?.into_iter().map(|s| Hoster { hoster_url: s.src, hoster_name: s.name, ..Default::default() }).collect())
}

/// The videos of one server: its player page, then the manifest it names, split per quality with the subtitles.
fn server_videos(host: &dyn Host, name: &str, src: &str) -> Res<Vec<Video>> {
    let player = extract::player_url(src)?;
    let html = get(host, player.as_str(), &[])?;
    let Some(data) = extract::parse_new_player(&html, &player) else {
        warn(host, format!("kickassanime: {name} is not the current player; no videos"));
        return Ok(Vec::new());
    };
    // the request headers of a video: the same ones the player sends
    let headers = extract::video_headers(&player);
    let dash = !data.manifest.contains(".m3u8");
    let text = match get(host, &data.manifest, &headers) {
        Ok(text) => text,
        Err(err) => {
            warn(host, format!("kickassanime: manifest of {name} could not be fetched: {err}"));
            return Ok(Vec::new());
        }
    };
    Ok(if dash {
        extract::dash_videos(name, &data.manifest, &text, &data.subtitles, &headers)
    } else {
        extract::hls_videos(name, &data.manifest, &text, &data.subtitles, &headers)
    })
}

/// The videos of one hoster, sorted; or, for an episode, every server's videos sorted the same way (a failing server is
/// skipped). The Kotlin sorts the videos of an episode in `sortVideos` too.
fn videos(host: &dyn Host, input: &Input) -> Res<Vec<Video>> {
    if let Some(hoster) = input.get::<Hoster>("hoster")? {
        let found = server_videos(host, &hoster.hoster_name, &hoster.hoster_url)?;
        return Ok(extract::sort_videos(found, SERVER, QUALITY));
    }
    let Ok(episode) = input.url("episode") else {
        return Err("videos needs a hoster or an episode".into());
    };
    let mut all = Vec::new();
    for server in servers(host, episode)? {
        match server_videos(host, &server.name, &server.src) {
            Ok(found) => all.extend(found),
            Err(err) => warn(host, format!("kickassanime: server {} failed: {err}", server.name)),
        }
    }
    Ok(extract::sort_videos(all, SERVER, QUALITY))
}

struct KickAssAnime;

impl Source for KickAssAnime {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: None }
    }

    /// The headers of this source's video requests. The Origin is left out: it is the player's host, which each video
    /// carries in its own `headers` (`extract::video_headers`).
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        Some(extract::common_headers())
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => {
                let page = input.require("page")?;
                to_value(api::parse_popular(&get(host, &api::popular_url(page), &[])?, page)?)
            }
            "latest" => to_value(api::parse_latest(&get(host, &api::latest_url(input.require("page")?), &[])?)?),
            "search" => to_value(search(host, input.require("page")?, &input.require::<String>("query")?, input.filters())?),
            "details" => to_value(details(host, input.url("anime")?)?),
            "episodes" => to_value(episodes(host, input.url("anime")?)?),
            "hosters" => to_value(hosters(host, input.url("episode")?)?),
            "videos" => to_value(videos(host, input)?),
            "subtitles" => to_value(input.require::<VideoIn>("video")?.subtitle_tracks),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(KickAssAnime);

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    fn call(op: &str) -> Res<String> {
        kuro_guest::invoke(&KickAssAnime, &TestHost::default(), op, "{}")
    }

    /// ABI 1.5 `request-headers`: the player's headers, with neither Referer nor Origin (each video has its Origin).
    #[test]
    fn the_request_headers_are_the_player_headers_without_referer_or_origin() {
        let headers = call("request-headers").unwrap();
        let pairs: Vec<(String, String)> = serde_json::from_str(&headers).unwrap();
        assert!(pairs.iter().all(|(k, _)| !k.eq_ignore_ascii_case("origin") && !k.eq_ignore_ascii_case("referer")));
        assert!(pairs.iter().any(|(k, v)| k == "User-Agent" && v == extract::VIDEO_USER_AGENT));
    }

    #[test]
    fn an_op_the_plug_in_does_not_answer_is_unknown() {
        assert_eq!(call("chapters"), Err("unknown op: chapters".to_owned()));
    }
}
