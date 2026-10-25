//! AllAnime (<https://allmanga.to>, API <https://api.allanime.day>), an anime source plug-in for Kurobayomi. Ported
//! from the `en/allanime` Aniyomi extension (Apache-2.0). Every op is one GraphQL POST to the API; the servers of an
//! episode are the site's own ("internal", a `/apivtwo/clock` link hidden by a hex/xor code), direct player files, and
//! embeds on file hosts read by the shared `lib/extractors`.
//!
//! Since 2026 the API answers the episode's server list only to the site's own app (`AA_CRYPTO_MISSING`: the query is
//! signed and the answer encrypted in the browser). The plug-in then says so; see README.md.
//!
//! Left out: the filters and the settings (title style: romaji; sub or dub: sub; quality, server choice). Servers on
//! hosts no extractor reads (Ok.ru, Filemoon, StreamWish, StreamLare, Vidstreaming) are not offered.

use kuro_guest::html::{Html, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, Hoster, Track, Video, status};
use kuro_guest::{Host, Info, Input, Request, Res, Source, to_value, unknown_op};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const API: &str = "https://api.allanime.day/api";
const SITE: &str = "https://allmanga.to";
/// Where the internal servers' `/apivtwo/clock` links are answered.
const PLAYER: &str = "https://allanime.day";
const NAME: &str = "AllAnime";
/// The source id: the MD5 of `allanime/en/1`, as the Kotlin extension uses. Keep it: backups name it.
const ID: i64 = 4709139914729853090;
const PAGE_SIZE: usize = 26;
/// The separator of a stored title address: `<id><&sep><slugTime><&sep><slug>` (the Kotlin's).
const SEP: &str = "<&sep>";
/// The internal servers the Kotlin offers by default.
const INTERNAL: &[&str] = &["default", "ac", "ak", "kir", "luf-mp4", "si-hls", "s-mp4", "ac-hls"];

const POPULAR_QUERY: &str = "query($type: VaildPopularTypeEnumType! $size: Int! $page: Int $dateRange: Int) { queryPopular(type: $type size: $size dateRange: $dateRange page: $page) { total recommendations { anyCard { _id name thumbnail englishName nativeName slugTime } } } }";
const SEARCH_QUERY: &str = "query($search: SearchInput $limit: Int $page: Int $translationType: VaildTranslationTypeEnumType $countryOrigin: VaildCountryOriginEnumType) { shows(search: $search limit: $limit page: $page translationType: $translationType countryOrigin: $countryOrigin) { pageInfo { total } edges { _id name thumbnail englishName nativeName slugTime } } }";
const DETAILS_QUERY: &str = "query ($_id: String!) { show(_id: $_id) { thumbnail description type season score genres status studios } }";
const EPISODES_QUERY: &str = "query ($_id: String!) { show(_id: $_id) { _id availableEpisodesDetail } }";
/// The streams query exactly as the Kotlin writes it: it is part of every stored episode address.
const STREAMS_QUERY: &str = "query(\n    $showId: String!,\n    $translationType: VaildTranslationTypeEnumType!,\n    $episodeString: String!\n) {\n    episode(\n        showId: $showId\n        translationType: $translationType\n        episodeString: $episodeString\n    ) {\n        sourceUrls\n    }\n}";

struct AllAnime;

impl Source for AllAnime {
    fn info(&self) -> Info {
        Info { id: ID, name: NAME, lang: "en", supports_latest: true, request_gap_ms: None }
    }

    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value> {
        let page = input.page().max(1);
        match op {
            "popular" => to_value(cards(&api(host, POPULAR_QUERY, json!({ "type": "anime", "size": PAGE_SIZE, "dateRange": 7, "page": page }))?["queryPopular"]["recommendations"], "anyCard")),
            "latest" => to_value(search(host, "", page)?),
            "search" => to_value(search(host, input.query().trim(), page)?),
            "details" => {
                let url = input.url("anime")?;
                let mut anime = details(&api(host, DETAILS_QUERY, json!({ "_id": show_id(url)? }))?["show"]);
                anime.url = url.to_owned();
                to_value(anime)
            }
            "episodes" => to_value(episodes(&api(host, EPISODES_QUERY, json!({ "_id": show_id(input.url("anime")?)? }))?["show"])?),
            "hosters" => to_value(hosters(host, input.url("episode")?)?),
            "videos" => match input.get::<Hoster>("hoster")? {
                Some(hoster) => to_value(hoster_videos(host, &hoster, true)?),
                None => {
                    let mut all = Vec::new();
                    for hoster in hosters(host, input.url("episode")?)? {
                        // one failing server does not hide the others
                        match hoster_videos(host, &hoster, all.is_empty()) {
                            Ok(videos) => all.extend(videos),
                            Err(error) => host.log(2, &format!("allanime: {} failed: {error}", hoster.hoster_name)),
                        }
                    }
                    to_value(all)
                }
            },
            _ => Err(unknown_op(op)),
        }
    }
}

kuro_guest::plugin!(AllAnime);

/// One GraphQL call; its `data`, or the API's first error.
fn api(host: &dyn Host, query: &str, variables: Value) -> Res<Value> {
    let body = json!({ "variables": variables, "query": query }).to_string();
    let text = Request::post(API, body)
        .headers([("Accept", "*/*"), ("Content-Type", "application/json; charset=utf-8"), ("Origin", SITE)])
        .header("Referer", format!("{SITE}/"))
        .text(host, NAME)?;
    answer(&text)
}

fn answer(text: &str) -> Res<Value> {
    let mut reply: Value = serde_json::from_str(text).map_err(|e| format!("{NAME} sent an unexpected answer: {e}"))?;
    match reply["errors"][0]["message"].as_str() {
        Some("AA_CRYPTO_MISSING") => Err(format!("{NAME} now gives its servers only to its own site (AA_CRYPTO_MISSING); this source cannot read them")),
        Some(message) => Err(format!("{NAME}: {message}")),
        None => Ok(reply["data"].take()),
    }
}

fn search(host: &dyn Host, query: &str, page: i32) -> Res<AnimesPage> {
    let mut filter = json!({ "allowAdult": false, "allowUnknown": false });
    if !query.is_empty() {
        filter["query"] = json!(query);
    }
    let variables = json!({ "search": filter, "limit": PAGE_SIZE, "page": page, "translationType": "sub", "countryOrigin": "ALL" });
    Ok(cards(&api(host, SEARCH_QUERY, variables)?["shows"]["edges"], ""))
}

/// The titles of a list (`inner`: the field that holds each card, "" when the item is the card). A full page means
/// there may be more.
fn cards(list: &Value, inner: &str) -> AnimesPage {
    let items = list.as_array().map_or(&[][..], Vec::as_slice);
    let animes: Vec<Anime> = items
        .iter()
        .map(|item| if inner.is_empty() { item } else { &item[inner] })
        .filter_map(|card| {
            let (id, name) = (card["_id"].as_str()?, card["name"].as_str()?);
            Some(Anime {
                url: [id, card["slugTime"].as_str().unwrap_or(""), &slugify(name)].join(SEP),
                title: name.to_owned(),
                thumbnail_url: card["thumbnail"].as_str().map(str::to_owned),
                ..Default::default()
            })
        })
        .collect();
    AnimesPage { has_next_page: items.len() == PAGE_SIZE, animes }
}

/// The Kotlin's slug: every run of characters other than ASCII letters and digits one `-`, lower case.
fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        let c = if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' };
        if !(c == '-' && slug.ends_with('-')) {
            slug.push(c);
        }
    }
    slug
}

fn show_id(url: &str) -> Res<&str> {
    url.split(SEP).next().filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())).ok_or_else(|| format!("not an {NAME} title: {url}"))
}

fn details(show: &Value) -> Anime {
    let strings = |key: &str| show[key].as_array().map(|v| v.iter().filter_map(Value::as_str).collect::<Vec<_>>()).unwrap_or_default();
    let text = show["description"].as_str().unwrap_or("").split("<br>").map(|part| text_of(Html::parse_fragment(part).root_element())).collect::<Vec<_>>().join("\n");
    let or_dash = |v: &Value| v.as_str().map(str::to_owned).or_else(|| v.as_f64().map(|n| n.to_string())).unwrap_or_else(|| "-".into());
    let description = format!(
        "{}\n\nType: {}\nAired: {} {}\nScore: {}★",
        text.trim(),
        show["type"].as_str().unwrap_or("Unknown"),
        or_dash(&show["season"]["quarter"]),
        or_dash(&show["season"]["year"]),
        or_dash(&show["score"]),
    );
    Anime {
        genre: Some(strings("genres").join(", ")),
        status: match show["status"].as_str() {
            Some("Releasing" | "Not Yet Released") => status::ONGOING,
            Some("Finished") => status::COMPLETED,
            _ => status::UNKNOWN,
        },
        author: strings("studios").first().map(|s| s.to_string()),
        thumbnail_url: show["thumbnail"].as_str().map(str::to_owned),
        description: Some(description),
        ..Default::default()
    }
}

/// A stored episode address: the Kotlin's, the whole streams request as JSON (fields in its order).
#[derive(Serialize, Deserialize)]
struct EpisodeUrl {
    variables: EpisodeVars,
    query: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EpisodeVars {
    show_id: String,
    translation_type: String,
    episode_string: String,
}

/// The subbed episodes, in the API's order (newest first).
fn episodes(show: &Value) -> Res<Vec<Episode>> {
    let id = show["_id"].as_str().ok_or("the title has no id")?;
    let list = show["availableEpisodesDetail"]["sub"].as_array().map_or(&[][..], Vec::as_slice);
    list.iter()
        .filter_map(Value::as_str)
        .map(|ep| {
            let url = EpisodeUrl { variables: EpisodeVars { show_id: id.into(), translation_type: "sub".into(), episode_string: ep.into() }, query: STREAMS_QUERY.into() };
            let number: Option<f32> = ep.parse().ok();
            Ok(Episode {
                url: serde_json::to_string(&url).map_err(|e| e.to_string())?,
                // "Episode 12 (sub)", "Episode 12.5 (sub)"
                name: format!("Episode {} (sub)", number.map_or("1".to_owned(), |n| n.to_string())),
                episode_number: number.unwrap_or(0.0),
                ..Default::default()
            })
        })
        .collect()
}

/// The servers of an episode that this plug-in can play, highest priority first.
fn hosters(host: &dyn Host, episode: &str) -> Res<Vec<Hoster>> {
    let url: EpisodeUrl = serde_json::from_str(episode).map_err(|_| format!("not an {NAME} episode: {episode}"))?;
    let vars = serde_json::to_value(&url.variables).map_err(|e| e.to_string())?;
    Ok(servers(&api(host, STREAMS_QUERY, vars)?["episode"]["sourceUrls"]))
}

/// `sourceUrls` as hosters: `internal_data` says how to read each (`internal`, `player`, `embed`).
fn servers(sources: &Value) -> Vec<Hoster> {
    let mut list: Vec<(f64, Hoster)> = Vec::new();
    for source in sources.as_array().map_or(&[][..], Vec::as_slice) {
        let (Some(raw), Some(name)) = (source["sourceUrl"].as_str(), source["sourceName"].as_str()) else { continue };
        let url = decode(raw);
        let lower = name.to_lowercase();
        let kind = if url.starts_with("/apivtwo/") && INTERNAL.iter().any(|n| has_word(&lower, n)) {
            "internal"
        } else if source["type"] == "player" && url.starts_with("http") {
            "player"
        } else if kuro_extractors::HOSTS.contains(&host_of(&url)) {
            "embed"
        } else {
            continue;
        };
        if list.iter().any(|(_, h)| h.hoster_url == url) {
            continue;
        }
        let same = list.iter().filter(|(_, h)| h.hoster_name == name || h.hoster_name.starts_with(&format!("{name} ("))).count();
        let hoster_name = if same > 0 { format!("{name} ({})", same + 1) } else { name.to_owned() };
        list.push((source["priority"].as_f64().unwrap_or(0.0), Hoster { hoster_url: url, hoster_name, internal_data: kind.into(), ..Default::default() }));
    }
    list.sort_by(|a, b| b.0.total_cmp(&a.0));
    list.into_iter().map(|(_, h)| h).collect()
}

/// Whether `word` stands alone in `text` (the Kotlin's `\bword\b`; a `-` is a boundary).
fn has_word(text: &str, word: &str) -> bool {
    let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '_');
    text.match_indices(word).any(|(i, _)| edge(text[..i].chars().next_back()) && edge(text[i + word.len()..].chars().next()))
}

/// A `--` address: hex bytes, each xor 56 (the Kotlin's `decryptSource`). Others are plain.
fn decode(source: &str) -> String {
    let Some(hex) = source.strip_prefix('-').map(|s| s.rsplit('-').next().unwrap_or(s)) else { return source.to_owned() };
    let bytes: Option<Vec<u8>> = (0..hex.len() / 2).map(|i| u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok().map(|b| b ^ 56)).collect();
    bytes.and_then(|b| String::from_utf8(b).ok()).unwrap_or_else(|| source.to_owned())
}

fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or("", |(_, r)| r);
    &rest[..rest.find(['/', '?', '#', ':']).unwrap_or(rest.len())]
}

fn hoster_videos(host: &dyn Host, hoster: &Hoster, preferred: bool) -> Res<Vec<Video>> {
    let mut videos = match hoster.internal_data.as_str() {
        "internal" => {
            let url = format!("{PLAYER}{}", hoster.hoster_url.replacen("/clock?", "/clock.json?", 1));
            clock_videos(&Request::get(url).text(host, NAME)?)?
        }
        "player" => vec![Video { video_url: hoster.hoster_url.clone(), video_title: "Auto".into(), headers: vec![("Referer".into(), format!("{PLAYER}/"))], ..Default::default() }],
        _ => kuro_extractors::videos(host, &hoster.hoster_url, "").ok_or_else(|| format!("{} is not supported", hoster.hoster_name))??,
    };
    if let Some(first) = videos.first_mut() {
        first.preferred = preferred;
    }
    Ok(videos)
}

/// The videos of an internal server's `clock.json`: mp4 files and HLS playlists (one video each), with subtitles.
/// ponytail: DASH and the old Crunchyroll-iframe links are skipped (the Kotlin splits them per stream); add them if
/// an internal server is seen giving only those.
fn clock_videos(text: &str) -> Res<Vec<Video>> {
    let reply: Value = serde_json::from_str(text).map_err(|e| format!("{NAME} sent an unexpected answer: {e}"))?;
    let links = reply["links"].as_array().map_or(&[][..], Vec::as_slice);
    let videos: Vec<Video> = links
        .iter()
        .filter(|link| link["mp4"] == true || link["hls"] == true)
        .filter_map(|link| {
            let tracks = link["subtitles"].as_array().map_or(&[][..], Vec::as_slice);
            let resolution = link["resolutionStr"].as_str().unwrap_or("");
            let is_height = resolution.strip_suffix('p').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            let headers = if link["hls"] == true { vec![("Origin".into(), PLAYER.into()), ("Referer".into(), format!("{PLAYER}/"))] } else { Vec::new() };
            Some(Video {
                video_url: link["link"].as_str()?.to_owned(),
                video_title: if is_height { resolution.to_owned() } else { "Auto".into() },
                headers,
                subtitle_tracks: tracks
                    .iter()
                    .filter_map(|t| {
                        let lang = t["label"].as_str().or(t["lang"].as_str()).unwrap_or("").to_owned();
                        Some(Track { url: t["src"].as_str()?.to_owned(), lang })
                    })
                    .collect(),
                ..Default::default()
            })
        })
        .collect();
    if videos.is_empty() { Err("the server gave no video this source can play".into()) } else { Ok(videos) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    const POPULAR: &str = include_str!("../fixtures/popular.json");
    const LATEST: &str = include_str!("../fixtures/latest.json");
    const SEARCH: &str = include_str!("../fixtures/search.json");
    const DETAILS: &str = include_str!("../fixtures/details.json");
    const EPISODES: &str = include_str!("../fixtures/episodes.json");
    const ENCRYPTED: &str = include_str!("../fixtures/streams-encrypted.json");
    const STREAMS: &str = include_str!("../fixtures/constructed-streams.json");
    const CLOCK: &str = include_str!("../fixtures/constructed-clock.json");
    const MANIFEST: &str = include_str!("../manifest.toml");

    /// The API as answered on 2026-10-25, picked by the query each POST carries.
    fn site(streams: &'static str) -> TestHost {
        TestHost::answering(move |request| {
            let body = request.body.as_deref().unwrap_or("");
            match request.url.as_str() {
                API if body.contains("queryPopular") => (200, POPULAR.into()),
                API if body.contains("shows(") && body.contains("\"query\":\"frieren\"") => (200, SEARCH.into()),
                API if body.contains("shows(") => (200, LATEST.into()),
                API if body.contains("availableEpisodesDetail") => (200, EPISODES.into()),
                API if body.contains("studios") => (200, DETAILS.into()),
                API if body.contains("sourceUrls") => (200, streams.into()),
                url if url.starts_with("https://allanime.day/apivtwo/clock.json?") => (200, CLOCK.into()),
                _ => (404, String::new()),
            }
        })
    }

    fn call(host: &TestHost, op: &str, args: Value) -> Res<Value> {
        kuro_guest::invoke(&AllAnime, host, op, &json!({ "args": args }).to_string()).map(|o| serde_json::from_str(&o).unwrap())
    }

    #[test]
    fn lists_are_graphql_pages_of_26() {
        let host = site(STREAMS);
        let popular = call(&host, "popular", json!({ "page": 1 })).unwrap();
        assert_eq!(popular["animes"].as_array().unwrap().len(), 26);
        assert_eq!(popular["animes"][0]["title"], "Cyberpunk: Edgerunners 2");
        assert_eq!(popular["animes"][0]["url"], "eELRbqmddmBn2R7on<&sep><&sep>cyberpunk-edgerunners-2");
        assert_eq!(popular["hasNextPage"], true);
        let latest = call(&host, "latest", json!({ "page": 1 })).unwrap();
        assert_eq!(latest["animes"].as_array().unwrap().len(), 26);
        let search = call(&host, "search", json!({ "page": 1, "query": "frieren" })).unwrap();
        assert_eq!(search["animes"].as_array().unwrap().len(), 5);
        assert_eq!(search["animes"][4]["url"], "ReHMC7TQnch3C6z8j<&sep><&sep>sousou-no-frieren");
        assert_eq!(search["hasNextPage"], false);
        let sent: Value = serde_json::from_str(host.sent()[2].body.as_deref().unwrap()).unwrap();
        assert_eq!(sent["variables"]["search"], json!({ "query": "frieren", "allowAdult": false, "allowUnknown": false }));
        assert_eq!(host.sent()[0].headers.iter().find(|(n, _)| n == "Origin").unwrap().1, SITE);
    }

    #[test]
    fn a_title_has_details_and_subbed_episodes() {
        let host = site(STREAMS);
        let title = json!({ "url": "ReHMC7TQnch3C6z8j<&sep><&sep>sousou-no-frieren" });
        let anime = call(&host, "details", json!({ "anime": title })).unwrap();
        assert_eq!(anime["status"], 2);
        assert_eq!(anime["author"], "MADHOUSE");
        assert_eq!(anime["genre"], "Adventure, Demons, Drama, Fantasy, Magic, Shounen, Yuri");
        let description = anime["description"].as_str().unwrap();
        assert!(description.ends_with("a new adventure…\n\n(Source: Crunchyroll)\n\nType: TV\nAired: Fall 2023\nScore: 9.26★"), "{description}");
        let episodes = call(&host, "episodes", json!({ "anime": title })).unwrap();
        assert_eq!(episodes.as_array().unwrap().len(), 28);
        assert_eq!((episodes[0]["name"].as_str(), episodes[0]["episodeNumber"].as_f64()), (Some("Episode 28 (sub)"), Some(28.0)));
        let url = episodes[27]["url"].as_str().unwrap();
        assert!(url.starts_with(r#"{"variables":{"showId":"ReHMC7TQnch3C6z8j","translationType":"sub","episodeString":"1"},"query":"query(\n    $showId"#), "{url}");
        assert!(call(&host, "details", json!({ "anime": { "url": "../x" } })).is_err());
    }

    #[test]
    fn the_encrypted_server_list_is_a_clear_error() {
        let host = site(ENCRYPTED);
        let episodes = call(&host, "episodes", json!({ "anime": { "url": "ReHMC7TQnch3C6z8j<&sep><&sep>x" } })).unwrap();
        let error = call(&host, "hosters", json!({ "episode": episodes[0] })).unwrap_err();
        assert!(error.contains("AA_CRYPTO_MISSING"), "{error}");
    }

    #[test]
    fn servers_are_the_internal_ones_players_and_known_embeds() {
        let host = site(STREAMS);
        let episodes = call(&host, "episodes", json!({ "anime": { "url": "ReHMC7TQnch3C6z8j<&sep><&sep>x" } })).unwrap();
        let hosters = call(&host, "hosters", json!({ "episode": episodes[27] })).unwrap();
        let names: Vec<&str> = hosters.as_array().unwrap().iter().map(|h| h["hosterName"].as_str().unwrap()).collect();
        // Rab is off by default; StreamLare and Filemoon have no extractor
        assert_eq!(names, ["S-mp4", "Default", "Sak", "Mp4"]);
        assert_eq!(hosters[1]["hosterUrl"], "/apivtwo/clock?id=7d2473746a4a5a6a6d7c6c4b2d2c");
        let sent: Value = serde_json::from_str(host.sent().last().unwrap().body.as_deref().unwrap()).unwrap();
        assert_eq!(sent["variables"], json!({ "showId": "ReHMC7TQnch3C6z8j", "translationType": "sub", "episodeString": "1" }));
        let videos = call(&host, "videos", json!({ "hoster": hosters[1] })).unwrap();
        assert_eq!(videos.as_array().unwrap().len(), 2);
        assert_eq!(videos[0]["videoUrl"], "https://cdn.example/master.m3u8");
        assert_eq!(videos[0]["headers"], json!([["Origin", "https://allanime.day"], ["Referer", "https://allanime.day/"]]));
        assert_eq!(videos[0]["subtitleTracks"], json!([{ "url": "https://cdn.example/en.vtt", "lang": "English" }]));
        assert_eq!((videos[1]["videoTitle"].as_str(), videos[1]["preferred"].as_bool()), (Some("1080p"), Some(false)));
        assert_eq!(host.sent().last().unwrap().url, "https://allanime.day/apivtwo/clock.json?id=7d2473746a4a5a6a6d7c6c4b2d2c");
        let player = call(&host, "videos", json!({ "hoster": hosters[2] })).unwrap();
        assert_eq!(player[0]["videoUrl"], "https://myanime.sharepoint.com/x/video.mp4");
    }

    #[test]
    fn helpers_follow_the_kotlin() {
        assert_eq!(slugify("Sousou no Frieren: ●● no Mahou"), "sousou-no-frieren-no-mahou");
        assert_eq!(decode("--175948514e4c4f57"), "/apivtwo");
        assert_eq!(decode("https://x"), "https://x");
        assert!(has_word("si-hls", "si-hls") && has_word("default", "default") && !has_word("ack", "ac"));
    }

    #[test]
    fn the_manifest_lists_the_api_player_and_every_extractor_host() {
        for host in ["api.allanime.day", "allanime.day"].iter().chain(kuro_extractors::HOSTS) {
            assert!(MANIFEST.contains(&format!("\"{host}\"")), "manifest.toml misses {host}");
        }
    }
}
