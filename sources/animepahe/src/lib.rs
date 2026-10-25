//! AnimePahe (<https://animepahe.pw>) as a Kurobayomi source plug-in, ABI 1.5 (`wit/kuro-source.wit`), anime ops only.
//! Ported from the yuzono `en/animepahe` extension (Apache-2.0; the Kwik decode is MIT, CloudStream); see README.md. The
//! site answers plain clients with Cloudflare's browser check: the app then sends its requests from a hidden page, so
//! nothing here deals with it. Every network call goes through the host's `http` import, limited to `manifest.toml`.

mod api;
mod dto;
mod kwik;
mod unpacker;

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use api::{AnimesPage, Hoster, Res};
use kuro::source::host;

/// The source id: the MD5 of `animepahe/en/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 9163675745699695675;
const NAME: &str = "AnimePahe";
const LANG: &str = "en";
/// The site answers too many quick requests with 429; the Kotlin waits 3 s between pages.
const REQUEST_GAP_MS: u64 = 1500;

// ================================ HTTP =================================

#[derive(Serialize)]
struct HttpRequest<'a> {
    method: &'a str,
    url: &'a str,
    headers: &'a [(&'a str, &'a str)],
    body: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponse {
    status: u16,
    #[serde(default)]
    url: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    body: String,
    #[serde(default)]
    body_encoding: Option<String>,
}

/// The start of the error a 429 gives, which tells the stream lookups to stop.
const LIMITED: &str = "AnimePahe is limiting requests";

fn limited(wait: Option<u64>) -> String {
    match wait {
        Some(seconds) => format!("{LIMITED}; wait {seconds} seconds and try again"),
        None => format!("{LIMITED}; wait a few seconds and try again"),
    }
}

/// The seconds of a `Retry-After` header. ponytail: the HTTP-date form is ignored (no wait kept); parse it if a host sends it.
fn retry_after(headers: &[(String, String)]) -> Option<u64> {
    headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("retry-after")).and_then(|(_, value)| value.trim().parse().ok())
}

/// The pref that holds until when a host asked us to wait (unix seconds), so the next op does not ask it again early.
fn limited_key(url: &str) -> String {
    format!("limited_until_{}", url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default())
}

/// One GET through the host: the body and the address it ended at. A non-2xx status is an error. A host that answered
/// 429 with a `Retry-After` is not asked again until that time has passed.
fn fetch(url: &str, headers: &[(&str, &str)]) -> Res<(String, String)> {
    let now = unix_seconds();
    if let Some(until) = serde_json::from_str::<u64>(&host::pref_get(&limited_key(url))).ok().filter(|until| *until > now) {
        return Err(limited(Some(until - now)));
    }
    let request = serde_json::to_string(&HttpRequest { method: "GET", url, headers, body: None }).map_err(|e| e.to_string())?;
    let response: HttpResponse = serde_json::from_str(&host::http(&request)?).map_err(|e| format!("invalid response: {e}"))?;
    if response.status == 429 {
        let wait = retry_after(&response.headers);
        if let Some(seconds) = wait {
            let _ = host::pref_set(&limited_key(url), &json!(now + seconds).to_string());
        }
        return Err(limited(wait));
    }
    if !(200..300).contains(&response.status) {
        return Err(format!("HTTP error {}", response.status));
    }
    if response.body_encoding.is_some() {
        return Err(format!("{url} sent a binary body"));
    }
    Ok((response.body, response.url))
}

fn get(url: &str) -> Res<String> {
    fetch(url, &[("Referer", "https://animepahe.pw/")]).map(|(body, _)| body)
}

// =========================== Session cache =============================

fn session_key(id: i64) -> String {
    format!("session_{id}")
}

fn remember(sessions: &[(i64, String)]) {
    for (id, session) in sessions {
        if cached_session(*id).as_deref() != Some(session) {
            let _ = host::pref_set(&session_key(*id), &json!(session).to_string());
        }
    }
}

fn cached_session(id: i64) -> Option<String> {
    serde_json::from_str(&host::pref_get(&session_key(id))).ok()
}

fn unix_seconds() -> u64 {
    host::unix_seconds()
}

fn poster_key(id: i64) -> String {
    format!("poster_{id}")
}

fn title_key(id: i64) -> String {
    format!("title_{id}")
}

fn cached_poster(id: i64) -> Option<String> {
    serde_json::from_str(&host::pref_get(&poster_key(id))).ok().filter(|p: &String| !p.is_empty())
}

fn remember_posters(hits: &[api::Hit]) {
    for hit in hits {
        if cached_poster(hit.id).as_deref() != Some(hit.poster.as_str()) {
            let _ = host::pref_set(&poster_key(hit.id), &json!(hit.poster).to_string());
        }
    }
}

/// Finds an anime's current session by searching its title and matching its id (Kotlin `fetchSessionAndId`).
fn find_session(id: i64, title: &str) -> Res<String> {
    let query = api::query_of(title);
    for page in 1..=3 {
        let hits = api::search_hits(&get(&api::search_url(&query, page, unix_seconds()))?)?;
        remember_posters(&hits);
        if let Some(hit) = hits.iter().find(|hit| hit.id == id) {
            remember(&[(id, hit.session.clone())]);
            return Ok(hit.session.clone());
        }
        if hits.is_empty() {
            break;
        }
    }
    Err(format!("AnimePahe no longer lists \"{title}\"; search for it again"))
}

/// The session of an anime: the cached one, or a fresh one when `stale` (the cached one failed) or none is cached.
fn session(anime: &AnimeRef, stale: bool) -> Res<String> {
    if let Some(session) = api::session_of(&anime.url) {
        return Ok(session.to_owned());
    }
    let id = api::id_of(&anime.url).ok_or_else(|| format!("not an AnimePahe entry: {}", anime.url))?;
    match cached_session(id).filter(|_| !stale) {
        Some(session) => Ok(session),
        None => find_session(id, &anime.title),
    }
}

/// Runs `op` with the anime's session; when the site no longer knows it, finds it again and retries once.
fn with_session<T>(anime: &AnimeRef, op: impl Fn(&str) -> Res<T>) -> Res<T> {
    match op(&session(anime, false)?) {
        Err(error) if error.starts_with("HTTP error 404") && api::id_of(&anime.url).is_some() => op(&session(anime, true)?),
        result => result,
    }
}

// ================================ Ops ==================================

#[derive(Deserialize)]
struct Envelope<T> {
    args: T,
}

fn args<T: DeserializeOwned>(input: &str) -> Res<T> {
    serde_json::from_str::<Envelope<T>>(input).map(|e| e.args).map_err(|e| format!("invalid args: {e}"))
}

#[derive(Deserialize)]
struct PageArgs {
    page: i32,
}

#[derive(Deserialize)]
struct SearchArgs {
    page: i32,
    query: String,
}

#[derive(Deserialize)]
struct AnimeRef {
    url: String,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
struct AnimeArgs {
    anime: AnimeRef,
}

#[derive(Deserialize)]
struct EpisodeRef {
    url: String,
}

#[derive(Deserialize)]
struct EpisodeArgs {
    episode: EpisodeRef,
}

#[derive(Deserialize)]
struct VideosArgs {
    hoster: Option<Hoster>,
    episode: Option<EpisodeRef>,
}

fn descriptor() -> serde_json::Value {
    json!({ "id": ID, "name": NAME, "lang": LANG, "supportsLatest": false, "requestGapMs": REQUEST_GAP_MS })
}

/// The airing list: the site has no popular list, and the Kotlin shows this one.
/// The marker a list gives as an anime's picture while its poster is unknown: the app asks the `cover` op for the real one
/// when the card is on screen, so a list never waits on lookups.
const COVER_MARKER: &str = "kb-cover:";

/// The airing list carries a screenshot of the latest episode, not the anime's poster (the Kotlin shows the screenshot too).
/// A poster seen before (a search, a details page, an earlier lookup) replaces it; the others carry the marker.
fn popular(input: &str) -> Res<AnimesPage> {
    let page = args::<PageArgs>(input)?.page;
    let (mut list, sessions) = api::parse_airing(&get(&api::airing_url(page))?)?;
    remember(&sessions);
    for anime in &mut list.animes {
        let id = api::id_of(&anime.url);
        anime.thumbnail_url = Some(match id.and_then(cached_poster) {
            Some(poster) => poster,
            None => {
                // the cover op gets only the url, and finds the poster by the title kept here
                if let Some(id) = id {
                    let _ = host::pref_set(&title_key(id), &json!(anime.title).to_string());
                }
                format!("{COVER_MARKER}{}", anime.url)
            }
        });
    }
    Ok(list)
}

/// The poster of one anime: the remembered one, else found by searching its title.
fn cover(input: &str) -> Res<Option<String>> {
    let anime = args::<AnimeArgs>(input)?.anime;
    let Some(id) = api::id_of(&anime.url) else { return Ok(None) };
    if let Some(poster) = cached_poster(id) {
        return Ok(Some(poster));
    }
    let title = Some(anime.title).filter(|t| !t.is_empty()).or_else(|| serde_json::from_str(&host::pref_get(&title_key(id))).ok());
    let Some(title) = title else { return Ok(None) };
    let hits = api::search_hits(&get(&api::search_url(&api::query_of(&title), 1, unix_seconds()))?)?;
    remember_posters(&hits);
    Ok(hits.iter().find(|hit| hit.id == id).map(|hit| hit.poster.clone()))
}

fn search(input: &str) -> Res<AnimesPage> {
    let a: SearchArgs = args(input)?;
    if a.query.trim().is_empty() {
        return popular(&json!({ "args": { "page": a.page } }).to_string());
    }
    let body = get(&api::search_url(a.query.trim(), a.page, unix_seconds()))?;
    let (list, sessions) = api::parse_search(&body)?;
    remember(&sessions);
    remember_posters(&api::search_hits(&body)?);
    Ok(list)
}

fn details(input: &str) -> Res<api::Anime> {
    let anime = args::<AnimeArgs>(input)?.anime;
    let mut found = with_session(&anime, |session| api::parse_details(&get(&api::details_url(session))?))?;
    if found.url.is_empty() {
        found.url = anime.url;
    }
    Ok(found)
}

/// Every page of the episode list, newest first.
fn episodes(input: &str) -> Res<Vec<api::Episode>> {
    let anime = args::<AnimeArgs>(input)?.anime;
    let mut list = with_session(&anime, |session| {
        let mut all = Vec::new();
        for page in 1.. {
            let (episodes, more) = api::parse_release(&get(&api::release_url(session, page))?, session)?;
            all.extend(episodes);
            if !more {
                break;
            }
        }
        Ok(all)
    })?;
    list.reverse();
    Ok(list)
}

fn hosters(input: &str) -> Res<Vec<Hoster>> {
    let play = args::<EpisodeArgs>(input)?.episode.url;
    let url = format!("{}{}", api::BASE_URL, play.split('?').next().unwrap_or(&play));
    Ok(api::parse_hosters(&get(&url)?, &play))
}

/// The videos of a hoster: each stream's Kwik page, unpacked to its HLS playlist. A stream that fails is skipped.
fn hoster_videos(hoster: &Hoster, first: bool) -> Res<Vec<api::Video>> {
    let fetch_kwik = |kwik: &str| fetch(kwik, &[("Referer", "https://animepahe.pw/")]);
    let log = |message: &str| host::log(2, &format!("animepahe: {message} of {}", hoster.hoster_name));
    resolve_streams(api::streams(&hoster.internal_data), first, fetch_kwik, log)
}

/// Resolves each stream with `fetch`, skipping one that fails. A 429 stops the lookups, since each further one counts
/// against the limit too: what was found is returned, or the 429 error when nothing was.
fn resolve_streams(
    streams: Vec<(String, String)>,
    first: bool,
    mut fetch: impl FnMut(&str) -> Res<(String, String)>,
    log: impl Fn(&str),
) -> Res<Vec<api::Video>> {
    let mut videos = Vec::new();
    for (kwik, title) in streams {
        let found = fetch(&kwik)
            .and_then(|(html, landed)| api::kwik_playlist(&html).map(|p| (p, landed)).ok_or_else(|| "no playlist in the Kwik page".to_owned()));
        match found {
            Ok((playlist, landed)) => {
                let preferred = first && videos.is_empty();
                videos.push(api::video(playlist, &title, if landed.is_empty() { &kwik } else { &landed }, preferred));
            }
            Err(error) => {
                log(&format!("{title} failed: {error}"));
                if error.starts_with(LIMITED) {
                    return if videos.is_empty() { Err(error) } else { Ok(videos) };
                }
            }
        }
    }
    Ok(videos)
}

fn videos(input: &str) -> Res<Vec<api::Video>> {
    let a: VideosArgs = args(input)?;
    if let Some(hoster) = a.hoster {
        return hoster_videos(&hoster, true);
    }
    let episode = a.episode.ok_or("videos needs a hoster or an episode")?;
    let hosters = hosters(&json!({ "args": { "episode": { "url": episode.url } } }).to_string())?;
    let mut all = Vec::new();
    for (i, hoster) in hosters.iter().enumerate() {
        match hoster_videos(hoster, i == 0) {
            Ok(videos) => all.extend(videos),
            Err(error) if all.is_empty() => return Err(error),
            Err(_) => break,
        }
    }
    Ok(all)
}

fn json_out<T: Serialize>(result: Res<T>) -> Res<String> {
    result.and_then(|value| serde_json::to_string(&value).map_err(|e| e.to_string()))
}

struct Plugin;

impl Guest for Plugin {
    fn invoke(op: String, input_json: String) -> Result<String, String> {
        match op.as_str() {
            "info" => Ok(descriptor().to_string()),
            "create-sources" => Ok(json!([descriptor()]).to_string()),
            "popular" => json_out(popular(&input_json)),
            "search" => json_out(search(&input_json)),
            "details" => json_out(details(&input_json)),
            "cover" => json_out(cover(&input_json)),
            "episodes" => json_out(episodes(&input_json)),
            "hosters" => json_out(hosters(&input_json)),
            "videos" => json_out(videos(&input_json)),
            _ => Err(format!("unknown op: {op}")),
        }
    }
}

export!(Plugin);

#[cfg(test)]
mod tests {
    use super::*;

    const KWIK: &str = include_str!("../fixtures/kwik.html");

    fn streams(n: usize) -> Vec<(String, String)> {
        (0..n).map(|i| (format!("https://kwik.cx/e/{i}"), format!("{i}p"))).collect()
    }

    #[test]
    fn a_429_stops_the_remaining_lookups() {
        // the first stream resolves, the second is limited: the third is never asked for, and the found one is kept
        let mut asked = Vec::new();
        let found = resolve_streams(streams(3), true, |url| {
            asked.push(url.to_owned());
            if asked.len() == 1 { Ok((KWIK.to_owned(), String::new())) } else { Err(limited(Some(30))) }
        }, |_| {})
        .unwrap();
        assert_eq!(asked.len(), 2);
        assert_eq!(found.len(), 1);
        assert!(found[0].preferred);

        // limited at the first stream: one request, and the 429 is the answer
        let mut count = 0;
        let error = resolve_streams(streams(3), true, |_| { count += 1; Err(limited(None)) }, |_| {}).unwrap_err();
        assert_eq!(count, 1);
        assert!(error.starts_with(LIMITED));
    }

    #[test]
    fn another_failure_skips_only_its_stream() {
        let mut count = 0;
        let found = resolve_streams(streams(3), false, |_| {
            count += 1;
            if count == 1 { Err("HTTP error 500".into()) } else { Ok((KWIK.to_owned(), String::new())) }
        }, |_| {})
        .unwrap();
        assert_eq!((count, found.len()), (3, 2));
    }

    #[test]
    fn retry_after_reads_seconds_in_any_case() {
        assert_eq!(retry_after(&[("Retry-After".into(), " 42 ".into())]), Some(42));
        assert_eq!(retry_after(&[("retry-after".into(), "Wed, 21 Oct 2015 07:28:00 GMT".into())]), None);
        assert_eq!(retry_after(&[]), None);
        assert_eq!(limited_key("https://kwik.cx/e/abc"), "limited_until_kwik.cx");
    }
}
