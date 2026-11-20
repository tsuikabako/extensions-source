//! What Anikoto's pages and AJAX answers say: the lists, a title page, its episode list and an episode's servers. Pure
//! parsing, no requests. The selectors are the ones of the yuzono `anikototheme` (Apache-2.0).

use kuro_guest::Res;
use kuro_guest::html::{ElementRef, Html, attr, first, own_text, sel, text_of};
use kuro_guest::wire::{Anime, AnimesPage, Episode, Hoster, status};
use serde::Deserialize;

use crate::{BASE, NAME};

/// What separates the audio type from the episode page in a hoster's `internalData` (the Kotlin's separator).
pub const DATA_SEPARATOR: char = '\0';

/// A site path from a link of the site: the domain, the query and a trailing `/ep-<n>` are dropped. `None` for a link
/// that does not lead to a title.
pub fn title_path(href: &str) -> Option<String> {
    let path = href.trim().strip_prefix(BASE).unwrap_or(href.trim());
    let path = path.split(['?', '#']).next()?;
    let path = match path.rsplit_once("/ep-") {
        Some((title, number)) if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) => title,
        _ => path,
    };
    (path.starts_with("/watch/") && path.len() > "/watch/".len()).then(|| path.to_owned())
}

/// The English title of a `.d-title` element, else its romaji one (`data-jp`).
fn title_of(element: ElementRef) -> String {
    let english = text_of(element);
    if !english.is_empty() { english } else { element.value().attr("data-jp").unwrap_or_default().trim().to_owned() }
}

/// One page of a list (`most-viewed`, `latest-updated`, `filter`): its titles, and whether a page follows the active
/// one in the pager.
pub fn list(html: &str) -> AnimesPage {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let animes = root
        .select(&sel("div.ani.items > div.item"))
        .filter_map(|item| {
            let name = first(item, "a.name")?;
            let url = title_path(name.value().attr("href")?)?;
            let thumbnail_url = attr(item, "div.poster img", "data-src").or_else(|| attr(item, "div.poster img", "src"));
            Some(Anime { url, title: title_of(name), thumbnail_url, ..Default::default() })
        })
        .collect();
    AnimesPage { animes, has_next_page: first(root, "nav > ul.pagination > li.active ~ li").is_some() }
}

/// The label of a `div.meta > div` row of the info block ("Status: ..." gives "Status").
fn label_of(row: ElementRef) -> String {
    own_text(row).trim_end_matches([':', ' ']).to_owned()
}

/// A title page: the title, cover, genres, studios, status and a description (score, synopsis, the other facts and
/// names), in the Kotlin's layout. `url` is left empty: the caller keeps the stored address.
pub fn details(html: &str) -> Res<Anime> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let heading = first(root, "h1.title, h2.title").ok_or_else(|| format!("{NAME}: this page is not a title page"))?;
    let rows: Vec<(String, ElementRef)> = root.select(&sel("div.bmeta div.meta > div")).map(|row| (label_of(row), row)).collect();
    let row = |label: &str| rows.iter().find(|(name, _)| name.eq_ignore_ascii_case(label)).map(|(_, row)| *row);
    let links = |label: &str| row(label).map(|row| row.select(&sel("span > a")).map(text_of).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    let value = |row: ElementRef| first(row, "span").map(text_of).unwrap_or_default();

    let (studios, producers) = (links("Studios"), links("Producers"));
    let mut parts: Vec<String> = Vec::new();
    if let Some(score) = row("MAL").map(value).and_then(|score| fancy_score(&score)) {
        parts.push(score);
    }
    if let Some(synopsis) = first(root, "div.synopsis > div.shorting > div.content").map(text_of).filter(|t| !t.is_empty()) {
        parts.push(synopsis);
    }
    let facts: Vec<String> = rows
        .iter()
        .filter(|(label, _)| !label.is_empty() && !["Genres", "Status", "Studios", "Producers", "MAL"].contains(&label.as_str()))
        .filter_map(|(label, row)| {
            let mut text = value(*row);
            if label.eq_ignore_ascii_case("Duration") {
                let minutes: String = text.chars().filter(char::is_ascii_digit).collect();
                if !minutes.is_empty() {
                    text = format!("{minutes} min");
                }
            }
            (!text.is_empty()).then(|| format!("{label}: {text}"))
        })
        .collect();
    if !facts.is_empty() {
        parts.push(facts.join(" | "));
    }
    match (studios.is_empty(), producers.is_empty()) {
        (false, false) => parts.push(format!("**Studio:** {studios} (**Producers:** {producers})")),
        (false, true) => parts.push(format!("**Studio:** {studios}")),
        (true, false) => parts.push(format!("**Producers:** {producers}")),
        (true, true) => {}
    }
    let english = text_of(heading);
    let romaji = heading.value().attr("data-jp").unwrap_or_default().trim().to_owned();
    let mut names: Vec<String> = Vec::new();
    if !romaji.is_empty() && romaji != english {
        names.push(romaji.clone());
    }
    for alias in first(root, "div.names.font-italic").map(text_of).unwrap_or_default().split(';') {
        let alias = alias.trim();
        if !alias.is_empty() && alias != romaji && alias != english && !names.iter().any(|n| n == alias) {
            names.push(alias.to_owned());
        }
    }
    if !names.is_empty() {
        parts.push(format!("**Other name(s):** {}", names.join(", ")));
    }

    Ok(Anime {
        url: String::new(),
        title: title_of(heading),
        description: (!parts.is_empty()).then(|| parts.join("\n\n")),
        thumbnail_url: attr(root, "div.binfo div.poster img", "src"),
        author: (!studios.is_empty()).then_some(studios),
        artist: None,
        genre: Some(links("Genres")).filter(|g| !g.is_empty()),
        status: match row("Status").map(value).unwrap_or_default().to_lowercase().as_str() {
            "ongoing anime" | "currently airing" => status::ONGOING,
            "finished airing" | "completed" => status::COMPLETED,
            _ => status::UNKNOWN,
        },
    })
}

/// A score out of ten as five stars and the number ("★★★★★ 9.36"); `None` when the site has none ("?", 0).
fn fancy_score(score: &str) -> Option<String> {
    let value: f64 = score.trim().parse().ok().filter(|v: &f64| *v > 0.0)?;
    let stars = ((value / 2.0).round() as usize).min(5);
    Some(format!("{}{} {}", "★".repeat(stars), "☆".repeat(5 - stars), score.trim()))
}

/// The site's id of the title on its page: what its own script reads (`#watch-main`), else the first `data-id`.
pub fn anime_id(html: &str) -> Res<String> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let id = attr(root, "#watch-main", "data-id").or_else(|| first(root, "[data-id]").and_then(|e| e.value().attr("data-id")).map(|id| id.trim().to_owned()));
    id.filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric())).ok_or_else(|| format!("{NAME}: the title page has no id"))
}

/// The answer of every AJAX call: `{"status": 200, "result": ...}`, or a status with a `message`.
#[derive(Deserialize)]
struct Answer<T> {
    status: i64,
    #[serde(default)]
    message: Option<String>,
    result: Option<T>,
}

fn result<T: for<'de> Deserialize<'de>>(body: &str, what: &str) -> Res<T> {
    let answer: Answer<T> = serde_json::from_str(body).map_err(|e| format!("{NAME}: the {what} could not be read: {e}"))?;
    match answer.result {
        Some(result) if answer.status == 200 => Ok(result),
        _ => Err(format!("{NAME}: {}", answer.message.filter(|m| !m.trim().is_empty()).unwrap_or_else(|| format!("no {what} (status {})", answer.status)))),
    }
}

/// The episodes of `ajax/episode/list/<id>`, newest first. An episode's stored address is the Kotlin's:
/// `<server ids>&epurl=<title path>/ep-<n>`, then `&mal=`, `&slug=` and `&ts=` when the site gives them.
pub fn episodes(body: &str, anime_path: &str) -> Res<Vec<Episode>> {
    let html: String = result(body, "episode list")?;
    let doc = Html::parse_fragment(&html);
    let mut episodes: Vec<Episode> = doc
        .root_element()
        .select(&sel("div.episodes ul > li > a"))
        .filter_map(|link| {
            let data = |name: &str| link.value().attr(name).unwrap_or_default().trim();
            let (number, ids) = (data("data-num"), data("data-ids"));
            if ids.is_empty() {
                return None;
            }
            let tooltip = link.parent().and_then(ElementRef::wrap).and_then(|li| li.value().attr("title")).unwrap_or_default();
            let mut title = first(link, "span.d-title").map(text_of).unwrap_or_default();
            if title.is_empty() {
                title = tooltip.split("Release:").next().unwrap_or_default().split("Softsub").next().unwrap_or_default().trim().to_owned();
            }
            let numbered = format!("Episode {number}");
            let name = if title.is_empty() || title == numbered { numbered } else { format!("{numbered}: {title}") };
            let timestamp: Option<i64> = data("data-timestamp").parse().ok();
            let mut url = format!("{ids}&epurl={anime_path}/ep-{number}");
            for (key, value) in [("mal", data("data-mal")), ("slug", data("data-slug"))] {
                if !value.is_empty() {
                    url.push_str(&format!("&{key}={value}"));
                }
            }
            if let Some(timestamp) = timestamp {
                url.push_str(&format!("&ts={timestamp}"));
            }
            let softsub = tooltip.to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|word| word == "softsub");
            let audio: Vec<&str> = [(data("data-sub") == "1", "Sub"), (softsub, "SoftSub"), (data("data-dub") == "1", "Dub")].into_iter().filter(|(has, _)| *has).map(|(_, tag)| tag).collect();
            Some(Episode {
                url,
                name,
                episode_number: number.parse().unwrap_or(-1.0),
                fillermark: link.value().classes().any(|class| class == "filler"),
                scanlator: (!audio.is_empty()).then(|| audio.join(" & ")),
                date_upload: timestamp.map_or(0, |seconds| seconds * 1000),
                summary: None,
            })
        })
        .collect();
    episodes.reverse();
    Ok(episodes)
}

/// The two parts of an episode's stored address this source needs: the server ids and the episode's page path.
pub fn episode_parts(url: &str) -> Res<(&str, &str)> {
    let ids = url.split('&').next().unwrap_or_default();
    let page = url.split_once("epurl=").map(|(_, rest)| rest.split('&').next().unwrap_or_default()).unwrap_or_default();
    let ids_ok = !ids.is_empty() && ids.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'=' | b'+' | b'/'));
    if ids_ok && title_path(page).is_some() { Ok((ids, page)) } else { Err(format!("not an {NAME} episode: {url}")) }
}

/// The audio type of a server group, as the Kotlin names it: by its label, else by its `data-type`.
fn type_label(group: ElementRef) -> String {
    let label = first(group, "label").map(text_of).unwrap_or_default();
    let known = |text: &str| match text.to_lowercase().as_str() {
        "sub" => Some("Sub"),
        "h-sub" => Some("H-Sub"),
        "hsub" => Some("HSub"),
        "dub" => Some("Dub"),
        "a-dub" | "adub" => Some("A-Dub"),
        "s-sub" => Some("S-Sub"),
        _ => None,
    };
    let capital = |text: &str| text.chars().next().map(|c| c.to_uppercase().chain(text.chars().skip(1)).collect::<String>()).unwrap_or_default();
    let data_type = group.value().attr("data-type").unwrap_or_default().trim();
    known(&label).or_else(|| known(data_type)).map(str::to_owned).unwrap_or_else(|| if data_type.is_empty() { capital(&label) } else { capital(data_type) })
}

/// The servers of `ajax/server/list`: one hoster per server and audio type, "HD-1 (Sub)". `hosterUrl` is the
/// server's link id; `internalData` is the audio type and the episode's page, for the `videos` op.
pub fn hosters(body: &str, episode_page: &str) -> Res<Vec<Hoster>> {
    let html: String = result(body, "server list")?;
    let doc = Html::parse_fragment(&html);
    let mut hosters: Vec<Hoster> = Vec::new();
    for group in doc.root_element().select(&sel("div.servers > div.type")) {
        let audio = type_label(group);
        for server in group.select(&sel("li")) {
            let link = server.value().attr("data-link-id").unwrap_or_default().trim();
            let name = text_of(server);
            if link.is_empty() || name.is_empty() || server.value().classes().any(|class| class == "download-icon") {
                continue;
            }
            let name = name.trim_end_matches(['-', ' ']);
            let hoster_name = if audio.is_empty() { name.to_owned() } else { format!("{name} ({audio})") };
            hosters.push(Hoster { hoster_url: link.to_owned(), hoster_name, video_list: None, internal_data: format!("{audio}{DATA_SEPARATOR}{episode_page}"), lazy: false });
        }
    }
    Ok(hosters)
}

/// The player page a server points to (`ajax/server?get=<link id>`).
pub fn embed(body: &str) -> Res<String> {
    #[derive(Deserialize)]
    struct Server {
        url: String,
    }
    let server: Server = result(body, "server")?;
    Ok(server.url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_paths_drop_the_domain_the_episode_and_the_query() {
        assert_eq!(title_path("https://anikototv.bz/watch/one-piece-odmau/ep-1").as_deref(), Some("/watch/one-piece-odmau"));
        assert_eq!(title_path("/watch/a-b?x=1").as_deref(), Some("/watch/a-b"));
        assert_eq!(title_path("/watch/a-b#77").as_deref(), Some("/watch/a-b"));
        assert_eq!(title_path("/watch/step-ep-x").as_deref(), Some("/watch/step-ep-x"));
        assert_eq!(title_path("https://evil.example/watch/a"), None);
        assert_eq!(title_path("/genre/action"), None);
        assert_eq!(title_path("/watch/"), None);
    }

    #[test]
    fn scores_become_stars() {
        assert_eq!(fancy_score(" 9.36 ").as_deref(), Some("★★★★★ 9.36"));
        assert_eq!(fancy_score("7.84").as_deref(), Some("★★★★☆ 7.84"));
        assert_eq!(fancy_score("?"), None);
        assert_eq!(fancy_score("0"), None);
    }

    #[test]
    fn an_answer_without_a_result_is_the_sites_message() {
        assert_eq!(embed(r#"{"status":404,"message":"Server not found"}"#), Err("Anikoto: Server not found".into()));
        assert_eq!(embed(r#"{"status":500,"result":null}"#), Err("Anikoto: no server (status 500)".into()));
        assert!(embed("<html>").unwrap_err().starts_with("Anikoto: the server could not be read"));
        assert_eq!(embed(r#"{"status":200,"result":{"url":"https://megaplay.buzz/stream/s-2/1/sub","skip_data":{}}}"#).as_deref(), Ok("https://megaplay.buzz/stream/s-2/1/sub"));
    }

    #[test]
    fn episode_addresses_give_their_ids_and_page() {
        assert_eq!(episode_parts("QUJD-_=&epurl=/watch/x/ep-3&mal=5&slug=3&ts=9"), Ok(("QUJD-_=", "/watch/x/ep-3")));
        assert!(episode_parts("QUJD&epurl=https://evil.example/a").is_err());
        assert!(episode_parts("a b&epurl=/watch/x/ep-1").is_err());
        assert!(episode_parts("/watch/x").is_err());
    }

    #[test]
    fn server_groups_are_named_by_label_then_data_type() {
        let body = serde_json::json!({ "status": 200, "result": r#"<div class="servers">
            <div class="type" data-type="sub"><label> SUB</label><ul><li data-link-id="a">HD-1</li><li class="download-icon" data-link-id="x">DL</li><li>none</li></ul></div>
            <div class="type" data-type="adub"><label>English</label><ul><li data-link-id="b">Vidstream-</li></ul></div>
            <div class="type" data-type=""><label>raw</label><ul><li data-link-id="c">HD-2</li></ul></div></div>"# })
        .to_string();
        let hosters = hosters(&body, "/watch/x/ep-1").unwrap();
        let names: Vec<&str> = hosters.iter().map(|h| h.hoster_name.as_str()).collect();
        assert_eq!(names, ["HD-1 (Sub)", "Vidstream (A-Dub)", "HD-2 (Raw)"]);
        assert_eq!((hosters[0].hoster_url.as_str(), hosters[0].internal_data.as_str()), ("a", "Sub\0/watch/x/ep-1"));
    }
}
