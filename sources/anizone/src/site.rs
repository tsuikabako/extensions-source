//! AniZone's pages read: the Alpine/Livewire state in the HTML (`x-data` with `JSON.parse('...')`, `wire:snapshot`, the
//! CSRF token) and the JSON answers of `/livewire/update`.

use kuro_guest::Res;
use kuro_guest::html::{ElementRef, Html, attr, first, sel, text_of};
use kuro_guest::wire::{Anime, Episode, Hoster, Track, Video, status};
use regex_lite::Regex;
use serde::Deserialize;
use serde_json::Value;

use crate::BASE;

/// The state a Livewire call needs, and the list a page or call gave.
#[derive(Debug, Default)]
pub struct Page {
    pub token: String,
    pub snapshot: String,
    pub items: Vec<Value>,
    /// The cursor of the next page; empty when there is none.
    pub cursor: String,
}

/// A page's Livewire state and the first part of its list (`items`, `nextCursor`, `hasMore` in the `x-data`).
pub fn parse_page(html: &str) -> Page {
    let doc = Html::parse_document(html);
    let mut page = Page {
        token: attr(doc.root_element(), "script[data-csrf]", "data-csrf").unwrap_or_default(),
        snapshot: attr(doc.root_element(), "main > [wire\\:snapshot]", "wire:snapshot").unwrap_or_default(),
        ..Default::default()
    };
    if let Some(data) = x_data_with(&doc, "items") {
        page.items = js_json(&data, "items").and_then(|v| v.as_array().cloned()).unwrap_or_default();
        let has_more = Regex::new(r"hasMore:\s*true").unwrap().is_match(&data);
        page.cursor = Regex::new(r"nextCursor:\s*'([^']*)'").unwrap().captures(&data).filter(|_| has_more).map(|c| c[1].to_owned()).unwrap_or_default();
    }
    page
}

/// The answer of a `/livewire/update` call: the component's new snapshot, its HTML and, for `loadPage`, the next items.
pub fn parse_update(json: &str) -> Res<(Page, String)> {
    #[derive(Deserialize)]
    struct Update {
        components: Vec<Component>,
    }
    #[derive(Deserialize)]
    struct Component {
        snapshot: String,
        effects: Effects,
    }
    #[derive(Deserialize)]
    struct Effects {
        #[serde(default)]
        html: String,
        #[serde(default)]
        dispatches: Vec<Value>,
    }
    let update: Update = serde_json::from_str(json).map_err(|e| format!("AniZone sent an unexpected answer: {e}"))?;
    let component = update.components.into_iter().next().ok_or("AniZone sent no component")?;
    let mut page = Page { snapshot: component.snapshot, ..Default::default() };
    if let Some(loaded) = component.effects.dispatches.iter().find(|d| d["name"] == "items-loaded") {
        let params = &loaded["params"];
        page.items = params["items"].as_array().cloned().unwrap_or_default();
        page.cursor = params["nextCursor"].as_str().filter(|_| params["hasMore"] == true).unwrap_or_default().to_owned();
    }
    Ok((page, component.effects.html))
}

/// The first `x-data` that sets `key` from `JSON.parse(...)`.
fn x_data_with(doc: &Html, key: &str) -> Option<String> {
    let needle = format!("{key}: JSON.parse(");
    doc.select(&sel("[x-data]")).filter_map(|e| e.value().attr("x-data")).find(|d| d.contains(&needle)).map(str::to_owned)
}

/// The JSON of `key: JSON.parse('...')` in an `x-data`.
fn js_json(data: &str, key: &str) -> Option<Value> {
    let re = Regex::new(&format!(r"\b{key}:\s*JSON\.parse\('((?:[^'\\]|\\.)*)'\)")).unwrap();
    serde_json::from_str(&js_unescape(&re.captures(data)?[1])).ok()
}

/// A JavaScript single-quoted string's text. The site writes JSON into one (`\u0022` for its quotes, `\\` before the
/// JSON's own escapes), so undoing the JavaScript escapes gives the JSON exactly.
pub fn js_unescape(s: &str) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(s.len());
    let mut chars = s.chars();
    let push = |units: &mut Vec<u16>, c: char| units.extend(c.encode_utf16(&mut [0; 2]).iter());
    while let Some(c) = chars.next() {
        if c != '\\' {
            push(&mut units, c);
            continue;
        }
        match chars.next() {
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                units.push(u16::from_str_radix(&hex, 16).unwrap_or(0xFFFD));
            }
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                units.push(u16::from_str_radix(&hex, 16).unwrap_or(0xFFFD));
            }
            Some('n') => units.push(10),
            Some('r') => units.push(13),
            Some('t') => units.push(9),
            Some(other) => push(&mut units, other),
            None => {}
        }
    }
    String::from_utf16_lossy(&units)
}

/// The path of a site address (`http://anizone.to/anime/x` → `/anime/x`).
fn path_of(url: &str) -> String {
    match url.split_once("://") {
        Some((_, rest)) => rest.find('/').map_or("/".to_owned(), |i| rest[i..].to_owned()),
        None => url.to_owned(),
    }
}

/// The English title, else the romaji one, else `fallback` (the site's own order). The site writes ` for '.
fn title_of(titles: &Value, fallback: &str) -> String {
    let pick = ["1", "5"].iter().find_map(|k| titles[*k].as_str().filter(|t| !t.trim().is_empty()));
    pick.unwrap_or(fallback).trim().replace('`', "'")
}

/// The anime of a list's items.
pub fn animes(items: &[Value]) -> Vec<Anime> {
    items
        .iter()
        .filter_map(|item| {
            let url = path_of(item["url"].as_str()?);
            let title = title_of(&item["title_list"], item["main_title"].as_str().unwrap_or(""));
            (url.len() > 1 && !title.is_empty()).then(|| Anime { url, title, thumbnail_url: item["cover"].as_str().map(str::to_owned), ..Default::default() })
        })
        .collect()
}

/// The episodes of a title's items, specials first, each part newest first (as the Kotlin sorts them).
pub fn episodes(items: &[Value]) -> Vec<Episode> {
    let mut episodes: Vec<Episode> = items
        .iter()
        .filter_map(|item| {
            let slug = item["slug"].as_str()?;
            let title = title_of(&item["title_list"], "");
            let base = format!("Episode {slug}");
            Some(Episode {
                url: path_of(item["url"].as_str()?),
                name: if title.is_empty() || title == "Unknown" { base } else { format!("{base} - {title}") },
                episode_number: slug.parse().unwrap_or(-1.0),
                date_upload: item["air_date"].as_str().and_then(date_ms).unwrap_or(0),
                ..Default::default()
            })
        })
        .collect();
    let special = Regex::new(r"(?i)s\d+|special|recap").unwrap();
    episodes.sort_by(|a, b| {
        let (sa, sb) = (special.is_match(a.name.split(" - ").next().unwrap_or("")), special.is_match(b.name.split(" - ").next().unwrap_or("")));
        sb.cmp(&sa).then(b.episode_number.total_cmp(&a.episode_number))
    });
    episodes
}

/// `2023-09-29` as milliseconds since 1970.
fn date_ms(date: &str) -> Option<i64> {
    let day = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some(day.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis())
}

/// A title page: titles, cover, status, tags and synopsis.
pub fn details(html: &str) -> Res<Anime> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let data = x_data_with(&doc, "anmTitles").unwrap_or_default();
    let fallback = Regex::new(r"getTitle\([^,]+,\s*'([^']+)'\)").unwrap().captures(&data).map(|c| c[1].to_owned());
    let title = title_of(&js_json(&data, "anmTitles").unwrap_or(Value::Null), fallback.as_deref().unwrap_or(""));
    if title.is_empty() {
        return Err("the title page has no title".into());
    }
    let state = doc.select(&sel("span.inline-block")).map(|s| text_of(s).to_lowercase()).find_map(|s| match s.as_str() {
        "completed" => Some(status::COMPLETED),
        "ongoing" => Some(status::ONGOING),
        "cancelled" => Some(status::CANCELLED),
        "upcoming" => Some(status::UNKNOWN),
        _ => None,
    });
    let tags: Vec<String> = doc.select(&sel("a[href*='/tag/']")).map(text_of).collect();
    Ok(Anime {
        title,
        thumbnail_url: attr(root, "div.flex.items-start img", "src"),
        status: state.unwrap_or(status::UNKNOWN),
        genre: Some(tags.join(", ")).filter(|g| !g.is_empty()),
        description: synopsis(&doc),
        ..Default::default()
    })
}

/// The text after the "Synopsis" heading, its `<br>`s as line breaks.
fn synopsis(doc: &Html) -> Option<String> {
    let heading = doc.select(&sel("h3")).find(|h| text_of(*h) == "Synopsis")?;
    let body = heading.next_siblings().filter_map(ElementRef::wrap).find(|e| e.value().name() == "div")?;
    let mut text = String::new();
    for node in body.descendants() {
        if let Some(t) = node.value().as_text() {
            text.push_str(&t.split_whitespace().collect::<Vec<_>>().join(" "));
            if t.ends_with(char::is_whitespace) {
                text.push(' ');
            }
        } else if node.value().as_element().is_some_and(|e| e.name() == "br") {
            text.push('\n');
        }
    }
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    Some(lines.join("\n").trim().replace('`', "'")).filter(|t| !t.is_empty())
}

/// The servers of an episode page: one per `setVideo(n)` button, `internal_data` the server's number. Two servers of
/// one name (the site has that) are told apart by a number.
pub fn hosters(html: &str, episode: &str) -> Vec<Hoster> {
    let doc = Html::parse_document(html);
    let number = Regex::new(r#"setVideo\(['"]?(\d+)"#).unwrap();
    let mut hosters: Vec<Hoster> = Vec::new();
    for button in doc.select(&sel("button[wire\\:click*=setVideo]")) {
        let Some(index) = button.value().attr("wire:click").and_then(|c| number.captures(c)).map(|c| c[1].to_owned()) else { continue };
        let mut name = first(button, "div.text-lg").map(text_of).filter(|n| !n.is_empty()).unwrap_or_else(|| text_of(button));
        let same = hosters.iter().filter(|h| h.hoster_name == name || h.hoster_name.starts_with(&format!("{name} ("))).count();
        if same > 0 {
            name = format!("{name} ({})", same + 1);
        }
        hosters.push(Hoster { hoster_url: episode.to_owned(), hoster_name: name, internal_data: index, ..Default::default() });
    }
    hosters
}

/// Whether the page already plays server `index` (its button is the disabled one).
pub fn is_playing(html: &str, index: &str) -> bool {
    Html::parse_document(html)
        .select(&sel("button[wire\\:click*=setVideo]"))
        .any(|b| b.value().attr("disabled").is_some() && b.value().attr("wire:click").is_some_and(|c| c.contains(&format!("setVideo({index})"))))
}

/// The video of a page or component HTML: the HLS master playlist of its player and the subtitle files, the site's
/// default subtitle first. One video: the playlist holds the qualities and the audio languages, and the player
/// offers them.
pub fn video(html: &str) -> Res<Video> {
    #[derive(Deserialize)]
    struct Player {
        src: String,
        #[serde(default)]
        subtitles: Vec<Subtitle>,
    }
    #[derive(Deserialize)]
    struct Subtitle {
        title: String,
        file: String,
        #[serde(default)]
        default: bool,
    }
    let doc = Html::parse_document(html);
    let data = doc.select(&sel("[x-data*=vidstackPlayer]")).find_map(|e| e.value().attr("x-data").map(str::to_owned)).ok_or("the episode has no player")?;
    let raw = Regex::new(r"JSON\.parse\('((?:[^'\\]|\\.)*)'\)").unwrap().captures(&data).map(|c| js_unescape(&c[1])).ok_or("the player has no video")?;
    let mut player: Player = serde_json::from_str(&raw).map_err(|e| format!("the player data is unreadable: {e}"))?;
    player.subtitles.sort_by_key(|s| !s.default);
    Ok(Video {
        video_url: player.src,
        video_title: "Auto".into(),
        headers: vec![("Referer".into(), format!("{BASE}/"))],
        subtitle_tracks: player.subtitles.into_iter().map(|s| Track { url: s.file, lang: s.title }).collect(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn javascript_escapes_give_the_json_back() {
        let raw = r#"{\u0022t\u0022:\u0022a \\u0022b\\u0022 \\u846c\u0022,\u0022u\u0022:\u0022http:\\\/\\\/x\u0022,\u0022q\u0022:\u0022it\'s\u0022}"#;
        let value: Value = serde_json::from_str(&js_unescape(raw)).unwrap();
        assert_eq!(value["t"], "a \"b\" 葬");
        assert_eq!(value["u"], "http://x");
        assert_eq!(value["q"], "it's");
    }

    #[test]
    fn titles_prefer_english_then_romaji() {
        let titles: Value = serde_json::json!({ "5": "Sousou no Frieren", "1": "Frieren: Beyond Journey`s End" });
        assert_eq!(title_of(&titles, "x"), "Frieren: Beyond Journey's End");
        assert_eq!(title_of(&serde_json::json!({ "5": "Romaji" }), "x"), "Romaji");
        assert_eq!(title_of(&serde_json::json!([]), "Main"), "Main");
        assert_eq!(path_of("http://anizone.to/anime/x/1"), "/anime/x/1");
    }
}
