//! The parsers of the WCO pages: the home page's grids, the search page, a title page (two layouts: WCOStream's and
//! the others'), an episode page and its player, and the player's JSON.

use std::hash::{DefaultHasher, Hash, Hasher};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use kuro_guest::Res;
use kuro_guest::html::{ElementRef, Html, attr, first, own_text, sel, text_of};
use kuro_guest::wire::{Anime, Episode, Hoster, Video};
use serde_json::Value;

use crate::Site;

/// "Recently Added Series"; WCOStream has none and shows its recent releases.
pub fn popular(site: &Site, html: &str) -> Vec<Anime> {
    let doc = Html::parse_document(html);
    let added = grid(site, doc.select(&sel("div#sidebar_right2 ul.items > li")));
    if added.is_empty() { latest(site, html) } else { added }
}

/// "Recent Releases": the page's first grid. Its links are episode pages, stored as titles as the Kotlin does.
pub fn latest(site: &Site, html: &str) -> Vec<Anime> {
    let doc = Html::parse_document(html);
    doc.select(&sel("ul.items")).next().map(|list| grid(site, list.select(&sel(":scope > li")))).unwrap_or_default()
}

pub fn search(site: &Site, html: &str) -> Vec<Anime> {
    grid(site, Html::parse_document(html).select(&sel("div#sidebar_right2 li, div#blog div.iccerceve")))
}

/// The cards of a grid: a link, its own text as the title (without the Dub/Sub badges), else the cover's alt text.
fn grid<'a>(site: &Site, items: impl Iterator<Item = ElementRef<'a>>) -> Vec<Anime> {
    items
        .filter_map(|item| {
            let anchor = first(item, ".recent-release-episodes a, .img a").or_else(|| first(item, "a"))?;
            let url = path_of(anchor.value().attr("href")?);
            let title = [own_text(anchor), attr(item, "img[alt]", "alt").unwrap_or_default(), text_of(item)]
                .into_iter()
                .map(|t| t.trim().to_owned())
                .find(|t| !t.is_empty())?;
            // WcoTv lists entries without a page
            (url.len() > 1).then(|| Anime {
                url,
                title,
                thumbnail_url: attr(item, "img[src]", "src").map(|src| absolute(site, &src)),
                ..Default::default()
            })
        })
        .collect()
}

pub fn details(site: &Site, html: &str) -> Res<Anime> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    // a title page, else an episode page (a Latest entry): the series it belongs to, else its heading; WCOStream's
    // layout last
    let title = [
        "div.video-title a",
        "div.header-tag h2 a",
        "div.video-title h1",
        "div.baslikCell h1",
        "td.vsbaslik h2",
        "a[href*='/anime/'] > span.film-name",
        "td.ilxbaslik8 h1",
    ]
    .into_iter()
    .filter_map(|css| first(root, css).map(text_of))
    .find(|t| !t.is_empty())
    .ok_or("the page has no title")?;
    let description = first(root, "div#sidebar_cat p")
        .map(text_of)
        .or_else(|| {
            Some(root.select(&sel("div#content div.katcont div.iltext p")).map(text_of).collect::<Vec<_>>().join(" "))
        })
        .filter(|d| !d.is_empty());
    let genre = root
        .select(&sel("div#sidebar_cat > a, div#cat-genre > div.wcobtn"))
        .map(text_of)
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Anime {
        title,
        description,
        thumbnail_url: attr(root, "div#sidebar_cat img, #cat-img-desc img", "src").map(|src| absolute(site, &src)),
        genre: (!genre.is_empty()).then_some(genre),
        ..Default::default()
    })
}

/// The episodes of a title page, newest first. An episode page lists none: it is its own one
/// episode, named by its heading.
pub fn episodes(html: &str, url: &str) -> Vec<Episode> {
    let doc = Html::parse_document(html);
    let css = "div.cat-eps, div#catlist-listview > ul > li, div#episodeList a.dark-episode-item, nav#sidebarEpisodeList a.sidebar-episode-item";
    let mut episodes = doc
        .select(&sel(css))
        .filter_map(|item| {
            let anchor = if item.value().name() == "a" { item } else { first(item, "a")? };
            let title = first(anchor, "span").map(text_of).unwrap_or_else(|| text_of(item));
            let (name, episode_number) = episode_name(&title);
            Some(Episode { url: path_of(anchor.value().attr("href")?), name, episode_number, ..Default::default() })
        })
        .collect::<Vec<_>>();
    if !episodes.is_empty() {
        // WCOStream's episode pages list oldest first
        let ascending = episodes
            .first()
            .zip(episodes.last())
            .is_some_and(|(a, z)| (0.0..z.episode_number).contains(&a.episode_number));
        if ascending {
            episodes.reverse();
        }
        return episodes;
    }
    let heading =
        first(doc.root_element(), ".video-title, .baslikCell, td.ilxbaslik8 h1").map(text_of).unwrap_or_default();
    let (name, episode_number) = episode_name(&heading);
    vec![Episode { url: url.to_owned(), name, episode_number, ..Default::default() }]
}

/// The name and number of an episode from its title, as the Kotlin's `(Season (\d+) )?Episode (\d+) (.*)`: "Season 2 -
/// Episode 3: Rest", numbered (season - 1) * 100 + episode. A title without "Episode N" keeps its text, number unknown.
pub fn episode_name(title: &str) -> (String, f32) {
    let found = title
        .match_indices("Episode ")
        .map(|(at, _)| at)
        .find(|&at| title[at + 8..].starts_with(|c: char| c.is_ascii_digit()));
    let Some(at) = found else {
        return (title.to_owned(), -1.0);
    };
    let rest = &title[at + 8..];
    let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    let episode: u32 = rest[..digits].parse().unwrap_or(1);
    let season = title[..at]
        .strip_suffix(' ')
        .and_then(|before| before.rsplit_once("Season "))
        .and_then(|(_, n)| n.parse::<u32>().ok());
    let mut name = season.map(|s| format!("Season {s} - ")).unwrap_or_default();
    name += &format!("Episode {episode}");
    let tail = rest[digits..].trim();
    if !tail.is_empty() {
        name += &format!(": {tail}");
    }
    (name, (season.unwrap_or(1).saturating_sub(1) * 100 + episode) as f32)
}

/// The players of an episode page: its iframes, or, where the page writes the iframe from a script, the decoded
/// script's. Each hoster carries the episode page, the player's Referer.
pub fn hosters(html: &str, episode_url: &str) -> Vec<Hoster> {
    let doc = Html::parse_document(html);
    let mut players = iframes(&doc);
    if players.is_empty() {
        for script in doc.select(&sel("script")) {
            let code: String = script.text().collect();
            if code.contains("atob") && code.contains("decodeURIComponent") {
                players.extend(
                    decode_script(&code).map(|written| iframes(&Html::parse_fragment(&written))).unwrap_or_default(),
                );
            }
        }
    }
    let count = players.len();
    players
        .into_iter()
        .enumerate()
        .map(|(i, player)| Hoster {
            hoster_url: player,
            hoster_name: if count > 1 { format!("WCO {}", i + 1) } else { "WCO".into() },
            internal_data: episode_url.to_owned(),
            ..Default::default()
        })
        .collect()
}

/// The WCO player iframes of a page (the only player the sites use).
fn iframes(doc: &Html) -> Vec<String> {
    doc.select(&sel("iframe[src]"))
        .filter_map(|frame| frame.value().attr("src"))
        .map(|src| if src.starts_with("//") { format!("https:{src}") } else { src.trim().to_owned() })
        .filter(|src| src.starts_with("https://embed.wcostream."))
        .collect()
}

/// What the page's script writes: a list of base64 texts, each a number hidden among letters; the number less the
/// script's shift is one byte of the HTML (`... - 27611262); ...`).
fn decode_script(code: &str) -> Option<String> {
    let list = code.split_once('[')?.1.split_once(']')?.0.trim().trim_end_matches(',');
    let parts: Vec<String> = serde_json::from_str(&format!("[{list}]")).ok()?;
    let shift: i64 = code.rsplit_once("- ")?.1.split_once(");")?.0.trim().parse().ok()?;
    let bytes = parts
        .iter()
        .map(|part| {
            let digits: String =
                String::from_utf8(STANDARD.decode(part).ok()?).ok()?.chars().filter(char::is_ascii_digit).collect();
            u8::try_from(digits.parse::<i64>().ok()? - shift).ok()
        })
        .collect::<Option<Vec<u8>>>()?;
    // `decodeURIComponent(escape(...))`: the bytes are UTF-8
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// A 32-hex nonce for the player's beacon. The player makes a random one; any fresh value does.
pub fn nonce(now: u64, player: &str) -> String {
    let half = |salt: u8| {
        let mut hasher = DefaultHasher::new();
        (now, player, salt).hash(&mut hasher);
        hasher.finish()
    };
    format!("{:016x}{:016x}", half(0), half(1))
}

/// The JSON address in the player page's script: `$.getJSON("/inc/embed/getvidlink.php?...", ...)`.
pub fn json_link(page: &str) -> Option<String> {
    let link = page.split_once("$.getJSON(\"")?.1.split_once('"')?.0;
    link.starts_with('/').then(|| link.to_owned())
}

/// The player's JSON: the file server and one token per quality (`enc` 480p, `hd` 720p, `fhd` 1080p), best first.
pub fn videos(json: &str) -> Res<Vec<Video>> {
    let data: Value = serde_json::from_str(json).map_err(|_| "the WCO player sent no video list")?;
    let server =
        data["server"].as_str().filter(|s| s.starts_with("https://")).ok_or("the WCO player sent no video server")?;
    let videos: Vec<Video> = [("fhd", 1080), ("hd", 720), ("enc", 480)]
        .into_iter()
        .filter_map(|(key, height)| {
            let token = data[key].as_str().filter(|t| !t.trim().is_empty())?;
            Some(Video {
                video_url: format!("{server}/getvid?evid={token}"),
                video_title: format!("{height}p"),
                resolution: Some(height),
                ..Default::default()
            })
        })
        .collect();
    if videos.is_empty() { Err("the WCO player has no file for this episode".into()) } else { Ok(videos) }
}

/// The path of a link on the site (`https://www.wco.tv/anime/x` → `/anime/x`); the sites link to each other, so any
/// host's path is kept.
fn path_of(href: &str) -> String {
    let href = href.trim();
    match url::Url::parse(href) {
        Ok(url) => match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_owned(),
        },
        Err(_) => href.to_owned(),
    }
}

/// A cover address as a full one (`//images.wcostream.com/...`, `/wp-content/...`).
fn absolute(site: &Site, src: &str) -> String {
    if src.starts_with("//") {
        format!("https:{src}")
    } else if src.starts_with('/') {
        format!("{}{src}", site.base)
    } else {
        src.to_owned()
    }
}
