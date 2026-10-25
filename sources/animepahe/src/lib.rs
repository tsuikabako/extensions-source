//! AnimePahe (<https://animepahe.pw>), an anime source plug-in for Kurobayomi, anime ops only. Ported from the yuzono
//! `en/animepahe` extension (Apache-2.0; the Kwik decode is MIT, CloudStream); see README.md. The site answers plain
//! clients with Cloudflare's browser check: the app then sends its requests from a hidden page, so nothing here deals
//! with it. This file is the source's ops, its session cache and its handling of 429; the URLs and parsers are in
//! `api.rs`, the glue to the app is the `kuro_guest` toolkit.

mod api;
mod dto;
mod kwik;
mod unpacker;

use kuro_guest::wire::{Anime, AnimesPage, Episode, Hoster, Video};
use kuro_guest::{Host, Info, Input, Request, Res, Source, read_pref, to_value, unknown_op, write_pref};
use serde::Deserialize;
use serde_json::Value;

/// The source id: the MD5 of `animepahe/en/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 9163675745699695675;
const NAME: &str = "AnimePahe";

// ================================ HTTP =================================

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

/// The error of a 404, after which a cached session is found again.
fn not_found() -> String {
    format!("HTTP 404 from {NAME}")
}

/// One GET through the host: the body and the address it ended at. A non-2xx status is an error. A host that answered
/// 429 with a `Retry-After` is not asked again until that time has passed.
fn fetch(host: &dyn Host, url: &str) -> Res<(String, String)> {
    let now = host.unix_seconds();
    if let Some(until) = read_pref::<u64>(host, &limited_key(url)).filter(|until| *until > now) {
        return Err(limited(Some(until - now)));
    }
    let response = Request::get(url).header("Referer", "https://animepahe.pw/").send(host)?;
    if response.status == 429 {
        let wait = retry_after(&response.headers);
        if let Some(seconds) = wait {
            write_pref(host, &limited_key(url), &(now + seconds));
        }
        return Err(limited(wait));
    }
    let landed = response.url.clone();
    Ok((response.text(NAME)?, landed))
}

fn get(host: &dyn Host, url: &str) -> Res<String> {
    fetch(host, url).map(|(body, _)| body)
}

// =========================== Session cache =============================

fn session_key(id: i64) -> String {
    format!("session_{id}")
}

fn remember(host: &dyn Host, sessions: &[(i64, String)]) {
    for (id, session) in sessions {
        if cached_session(host, *id).as_deref() != Some(session) {
            write_pref(host, &session_key(*id), session);
        }
    }
}

fn cached_session(host: &dyn Host, id: i64) -> Option<String> {
    read_pref(host, &session_key(id))
}

fn poster_key(id: i64) -> String {
    format!("poster_{id}")
}

fn title_key(id: i64) -> String {
    format!("title_{id}")
}

fn cached_poster(host: &dyn Host, id: i64) -> Option<String> {
    read_pref::<String>(host, &poster_key(id)).filter(|p| !p.is_empty())
}

fn remember_posters(host: &dyn Host, hits: &[api::Hit]) {
    for hit in hits {
        if cached_poster(host, hit.id).as_deref() != Some(hit.poster.as_str()) {
            write_pref(host, &poster_key(hit.id), &hit.poster);
        }
    }
}

/// Finds an anime's current session by searching its title and matching its id (Kotlin `fetchSessionAndId`).
fn find_session(host: &dyn Host, id: i64, title: &str) -> Res<String> {
    let query = api::query_of(title);
    for page in 1..=3 {
        let hits = api::search_hits(&get(host, &api::search_url(&query, page, host.unix_seconds()))?)?;
        remember_posters(host, &hits);
        if let Some(hit) = hits.iter().find(|hit| hit.id == id) {
            remember(host, &[(id, hit.session.clone())]);
            return Ok(hit.session.clone());
        }
        if hits.is_empty() {
            break;
        }
    }
    Err(format!("AnimePahe no longer lists \"{title}\"; search for it again"))
}

/// The session of an anime: the cached one, or a fresh one when `stale` (the cached one failed) or none is cached.
fn session(host: &dyn Host, anime: &AnimeRef, stale: bool) -> Res<String> {
    if let Some(session) = api::session_of(&anime.url) {
        return Ok(session.to_owned());
    }
    let id = api::id_of(&anime.url).ok_or_else(|| format!("not an AnimePahe entry: {}", anime.url))?;
    match cached_session(host, id).filter(|_| !stale) {
        Some(session) => Ok(session),
        None => find_session(host, id, &anime.title),
    }
}

/// Runs `op` with the anime's session; when the site no longer knows it, finds it again and retries once.
fn with_session<T>(host: &dyn Host, anime: &AnimeRef, op: impl Fn(&str) -> Res<T>) -> Res<T> {
    match op(&session(host, anime, false)?) {
        Err(error) if error == not_found() && api::id_of(&anime.url).is_some() => op(&session(host, anime, true)?),
        result => result,
    }
}

// ================================ Ops ==================================

#[derive(Deserialize)]
struct AnimeRef {
    url: String,
    #[serde(default)]
    title: String,
}

/// The marker a list gives as an anime's picture while its poster is unknown: the app asks the `cover` op for the real one
/// when the card is on screen, so a list never waits on lookups.
const COVER_MARKER: &str = "kb-cover:";

/// The airing list: the site has no popular list, and the Kotlin shows this one. It carries a screenshot of the latest
/// episode, not the anime's poster (the Kotlin shows the screenshot too). A poster seen before (a search, a details page,
/// an earlier lookup) replaces it; the others carry the marker.
fn popular(host: &dyn Host, page: i32) -> Res<AnimesPage> {
    let (mut list, sessions) = api::parse_airing(&get(host, &api::airing_url(page))?)?;
    remember(host, &sessions);
    for anime in &mut list.animes {
        let id = api::id_of(&anime.url);
        anime.thumbnail_url = Some(match id.and_then(|id| cached_poster(host, id)) {
            Some(poster) => poster,
            None => {
                // the cover op gets only the url, and finds the poster by the title kept here
                if let Some(id) = id {
                    write_pref(host, &title_key(id), &anime.title);
                }
                format!("{COVER_MARKER}{}", anime.url)
            }
        });
    }
    Ok(list)
}

/// The poster of one anime: the remembered one, else found by searching its title.
fn cover(host: &dyn Host, anime: AnimeRef) -> Res<Option<String>> {
    let Some(id) = api::id_of(&anime.url) else { return Ok(None) };
    if let Some(poster) = cached_poster(host, id) {
        return Ok(Some(poster));
    }
    let title = Some(anime.title).filter(|t| !t.is_empty()).or_else(|| read_pref(host, &title_key(id)));
    let Some(title) = title else { return Ok(None) };
    let hits = api::search_hits(&get(host, &api::search_url(&api::query_of(&title), 1, host.unix_seconds()))?)?;
    remember_posters(host, &hits);
    Ok(hits.iter().find(|hit| hit.id == id).map(|hit| hit.poster.clone()))
}

fn search(host: &dyn Host, page: i32, query: &str) -> Res<AnimesPage> {
    if query.trim().is_empty() {
        return popular(host, page);
    }
    let body = get(host, &api::search_url(query.trim(), page, host.unix_seconds()))?;
    let (list, sessions) = api::parse_search(&body)?;
    remember(host, &sessions);
    remember_posters(host, &api::search_hits(&body)?);
    Ok(list)
}

fn details(host: &dyn Host, anime: AnimeRef) -> Res<Anime> {
    let mut found = with_session(host, &anime, |session| api::parse_details(&get(host, &api::details_url(session))?))?;
    if found.url.is_empty() {
        found.url = anime.url;
    }
    Ok(found)
}

/// Every page of the episode list, newest first.
fn episodes(host: &dyn Host, anime: AnimeRef) -> Res<Vec<Episode>> {
    let mut list = with_session(host, &anime, |session| {
        let mut all = Vec::new();
        for page in 1.. {
            let (episodes, more) = api::parse_release(&get(host, &api::release_url(session, page))?, session)?;
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

fn hosters(host: &dyn Host, play: &str) -> Res<Vec<Hoster>> {
    let url = format!("{}{}", api::BASE_URL, play.split('?').next().unwrap_or(play));
    Ok(api::parse_hosters(&get(host, &url)?, play))
}

/// The videos of a hoster: each stream's Kwik page, unpacked to its HLS playlist. A stream that fails is skipped.
fn hoster_videos(host: &dyn Host, hoster: &Hoster, first: bool) -> Res<Vec<Video>> {
    let fetch_kwik = |kwik: &str| fetch(host, kwik);
    let log = |message: &str| host.log(2, &format!("animepahe: {message} of {}", hoster.hoster_name));
    resolve_streams(api::streams(&hoster.internal_data), first, fetch_kwik, log)
}

/// Resolves each stream with `fetch`, skipping one that fails. A 429 stops the lookups, since each further one counts
/// against the limit too: what was found is returned, or the 429 error when nothing was.
fn resolve_streams(
    streams: Vec<(String, String)>,
    first: bool,
    mut fetch: impl FnMut(&str) -> Res<(String, String)>,
    log: impl Fn(&str),
) -> Res<Vec<Video>> {
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

fn videos(host: &dyn Host, hoster: Option<Hoster>, episode: Option<String>) -> Res<Vec<Video>> {
    if let Some(hoster) = hoster {
        return hoster_videos(host, &hoster, true);
    }
    let episode = episode.ok_or("videos needs a hoster or an episode")?;
    let hosters = hosters(host, &episode)?;
    let mut all = Vec::new();
    for (i, hoster) in hosters.iter().enumerate() {
        match hoster_videos(host, hoster, i == 0) {
            Ok(videos) => all.extend(videos),
            Err(error) if all.is_empty() => return Err(error),
            Err(_) => break,
        }
    }
    Ok(all)
}

#[derive(Deserialize)]
struct EpisodeRef {
    url: String,
}

struct AnimePahe;

impl Source for AnimePahe {
    fn info(&self) -> Info {
        // The site answers too many quick requests with 429; the Kotlin waits 3 s between pages.
        Info { id: ID, name: NAME, lang: "en", supports_latest: false, request_gap_ms: Some(1500) }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        match op {
            "popular" => to_value(popular(host, input.require("page")?)?),
            "search" => to_value(search(host, input.require("page")?, &input.require::<String>("query")?)?),
            "details" => to_value(details(host, input.require("anime")?)?),
            "cover" => to_value(cover(host, input.require("anime")?)?),
            "episodes" => to_value(episodes(host, input.require("anime")?)?),
            "hosters" => to_value(hosters(host, &input.require::<EpisodeRef>("episode")?.url)?),
            "videos" => to_value(videos(host, input.get("hoster")?, input.get::<EpisodeRef>("episode")?.map(|e| e.url))?),
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(AnimePahe);

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
            if count == 1 { Err("HTTP 500 from AnimePahe".into()) } else { Ok((KWIK.to_owned(), String::new())) }
        }, |_| {})
        .unwrap();
        assert_eq!((count, found.len()), (3, 2));
    }

    #[test]
    fn a_404_finds_the_session_again_and_retries_once() {
        const SEARCH: &str = include_str!("../fixtures/search.json");
        const ANIME: &str = include_str!("../fixtures/anime.html");
        let host = kuro_guest::testing::TestHost::answering(|request| {
            if request.url.contains("m=search") {
                (200, SEARCH.to_owned())
            } else if request.url.ends_with("/anime/bf48dc58-16f3-99a1-7238-c82c2d74fa95") {
                (200, ANIME.to_owned())
            } else {
                (404, String::new())
            }
        });
        write_pref(&host, &session_key(5319), &"stale");
        let input = r#"{"args":{"anime":{"url":"/a/5319","title":"Frieren"}}}"#;
        let out: Value = serde_json::from_str(&kuro_guest::invoke(&AnimePahe, &host, "details", input).unwrap()).unwrap();
        assert_eq!(out["title"], "A Returner's Magic Should Be Special Season 2");
        assert_eq!(cached_session(&host, 5319).as_deref(), Some("bf48dc58-16f3-99a1-7238-c82c2d74fa95"));
        let urls: Vec<String> = host.sent().into_iter().map(|r| r.url).collect();
        assert_eq!(urls.len(), 3, "{urls:?}");
        assert!(urls[0].ends_with("/anime/stale"));
    }

    #[test]
    fn retry_after_reads_seconds_in_any_case() {
        assert_eq!(retry_after(&[("Retry-After".into(), " 42 ".into())]), Some(42));
        assert_eq!(retry_after(&[("retry-after".into(), "Wed, 21 Oct 2015 07:28:00 GMT".into())]), None);
        assert_eq!(retry_after(&[]), None);
        assert_eq!(limited_key("https://kwik.cx/e/abc"), "limited_until_kwik.cx");
    }
}
