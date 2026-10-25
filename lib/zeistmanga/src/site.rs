//! The ZeistManga pages and feeds as pure functions: URLs and one parser per page. Ported from the keiyoushi
//! `lib-multisrc/zeistmanga` theme (`ZeistManga.kt`, `ZeistMangaDto.kt`; Apache-2.0).

use chrono::DateTime;
use kuro_guest::Res;
use kuro_guest::html::{ElementRef, Html, attr, first, own_text, sel, text_of};
use kuro_guest::wire::{Chapter, Manga, MangasPage, status};
use regex_lite::Regex;
use serde::Deserialize;
use url::Url;

use crate::Site;

/// The label of the title posts (the Kotlin `mangaCategory`).
const SERIES: &str = "Series";
/// Posts in these labels are not manga, even when they are labelled Series.
const EXCLUDED: [&str; 3] = ["Anime", "Novel", "Novela"];
/// A list page: one more than the Kotlin's 20 asks whether there is a next page.
const LIST_SIZE: usize = 20;
/// The chapters asked for at once; Blogger may answer fewer.
const CHAPTER_PAGE: usize = 150;

// ================================ Feeds ================================

#[derive(Deserialize, Default)]
#[serde(default)]
struct Feed {
    feed: Body,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Body {
    #[serde(rename = "openSearch$totalResults")]
    total: Text,
    entry: Vec<Entry>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Text {
    #[serde(rename = "$t")]
    t: String,
}

/// One post of a feed.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Entry {
    title: Text,
    published: Text,
    updated: Text,
    category: Vec<Term>,
    link: Vec<Link>,
    content: Text,
    #[serde(rename = "media$thumbnail")]
    thumbnail: Option<Thumbnail>,
}

#[derive(Deserialize, Default)]
struct Term {
    term: String,
}

#[derive(Deserialize, Default)]
struct Link {
    rel: String,
    href: String,
}

#[derive(Deserialize, Default)]
struct Thumbnail {
    url: String,
}

impl Entry {
    fn labelled(&self, label: &str) -> bool {
        self.category.iter().any(|c| c.term == label)
    }

    /// The path of the post on the blog.
    fn path(&self) -> Option<String> {
        self.link.iter().find(|l| l.rel == "alternate").map(|l| path_of(&l.href))
    }
}

// ================================ URLs =================================

/// A feed of posts in `label` (`Chapter/Blue Lock` is the posts in both labels), with `alt=json`.
fn feed_url(site: &Site, label: &str) -> String {
    let mut url = Url::parse(site.base).expect("the site root is a valid address");
    url.path_segments_mut().expect("an http address").extend(["feeds", "posts", "default", "-"]).extend(label.split('/'));
    url.query_pairs_mut().append_pair("alt", "json");
    url.to_string()
}

/// A page of the title posts: the newest first, or the ones that match `query` (the Kotlin's `q=label:Series+...`).
pub fn list_url(site: &Site, page: i32, query: &str) -> String {
    let mut url = Url::parse(&feed_url(site, SERIES)).expect("a feed address");
    let start = LIST_SIZE * (page.max(1) as usize - 1) + 1;
    let mut pairs = url.query_pairs_mut();
    if query.trim().is_empty() {
        pairs.append_pair("orderby", "published");
    } else {
        pairs.append_pair("q", &format!("label:{SERIES} {}", query.trim()));
    }
    pairs.append_pair("max-results", &(LIST_SIZE + 1).to_string()).append_pair("start-index", &start.to_string());
    drop(pairs);
    url.to_string()
}

/// The chapter feed from `start` (from 1). The summary feed: the same posts without their bodies, which the list
/// does not need.
pub fn feed_range(feed: &str, start: usize) -> String {
    let Ok(mut url) = Url::parse(&feed.replacen("/feeds/posts/default/", "/feeds/posts/summary/", 1)) else {
        return feed.to_owned();
    };
    let kept: Vec<(String, String)> = url.query_pairs().filter(|(k, _)| k != "start-index" && k != "max-results").map(|(k, v)| (k.into(), v.into())).collect();
    url.query_pairs_mut().clear().extend_pairs(kept).append_pair("start-index", &start.to_string()).append_pair("max-results", &CHAPTER_PAGE.to_string());
    url.to_string()
}

/// The path of an address (`/2023/05/blue-lock.html`). An address that is already a path stays as it is.
pub fn path_of(href: &str) -> String {
    match Url::parse(href.trim()) {
        Ok(url) => url.path().to_owned(),
        Err(_) => href.trim().to_owned(),
    }
}

/// A page of the blog from a stored address: its path on the site root. A whole address (what the Kotlin stores when
/// a post links to another domain of the blog) is read by its path too.
pub fn page(site: &Site, stored: &str) -> Res<String> {
    let path = path_of(stored);
    if !path.starts_with('/') {
        return Err(format!("not a {} address: {stored}", site.name));
    }
    Ok(format!("{}{path}", site.base))
}

fn absolute(site: &Site, src: &str) -> Option<String> {
    Url::parse(site.base).and_then(|base| base.join(src.trim())).map(|u| u.to_string()).ok()
}

// =============================== Parsers ===============================

fn parse_feed_json(json: &str) -> Res<Body> {
    serde_json::from_str::<Feed>(json).map(|f| f.feed).map_err(|e| format!("invalid feed: {e}"))
}

/// A larger cover than the feed's 72-pixel square (the Kotlin `getThumbnail`).
fn cover(url: &str) -> String {
    let url = Regex::new(r"/s.+?-c/").expect("a valid pattern").replace_all(url, "/w600/").into_owned();
    match url.rfind("=s") {
        Some(i) if url.ends_with("-c") => format!("{}=w600", &url[..i]),
        _ => url,
    }
}

/// A page of the title posts (the Kotlin `parseSearchManga`).
pub fn parse_list(json: &str) -> Res<MangasPage> {
    let entries = parse_feed_json(json)?.entry;
    let mut mangas: Vec<Manga> = entries
        .iter()
        .filter(|e| e.labelled(SERIES) && !EXCLUDED.iter().any(|x| e.labelled(x)))
        .filter_map(|e| {
            let thumbnail_url = match &e.thumbnail {
                Some(t) => Some(cover(&t.url)),
                None => attr(Html::parse_fragment(&e.content.t).root_element(), "img", "src"),
            };
            Some(Manga { url: e.path()?, title: e.title.t.clone(), thumbnail_url, ..Default::default() })
        })
        .collect();
    let has_next_page = mangas.len() > LIST_SIZE;
    mangas.truncate(LIST_SIZE);
    Ok(MangasPage { mangas, has_next_page })
}

/// The home page's popular titles (the Kotlin `parsePopularManga`).
pub fn parse_popular(site: &Site, cards: &str, html: &str) -> MangasPage {
    let doc = Html::parse_document(html);
    let mangas = doc
        .select(&sel(cards))
        .filter_map(|figure| {
            let link = first(figure, "figcaption > a")?;
            let thumbnail_url = attr(figure, "img", "src").and_then(|src| absolute(site, &src));
            Some(Manga { url: path_of(link.value().attr("href")?), title: text_of(link), thumbnail_url, ..Default::default() })
        })
        .collect();
    MangasPage { mangas, has_next_page: false }
}

fn status_of(text: &str) -> i32 {
    match text.trim().to_lowercase().as_str() {
        "ongoing" | "en curso" | "en emisión" | "em lançamento" | "activo" | "ativo" | "lançando" | "مستمر" | "مستمرة" => status::ONGOING,
        "completed" | "completo" | "finalizado" | "مكتمل" | "مكتملة" => status::COMPLETED,
        "hiatus" | "pausado" => status::ON_HIATUS,
        "cancelled" | "dropped" | "dropado" | "abandonado" | "cancelado" => status::CANCELLED,
        _ => status::UNKNOWN,
    }
}

fn some(text: String) -> Option<String> {
    Some(text).filter(|t| !t.is_empty())
}

/// A title page (the Kotlin `mangaDetailsParse`; the title from the page's heading).
pub fn parse_details(site: &Site, html: &str) -> Res<Manga> {
    let doc = Html::parse_document(html);
    let root = doc.select(&sel(site.details.root)).next().ok_or("the title page has no details")?;
    let title = first(root, "h1")
        .or_else(|| doc.select(&sel("h1")).next())
        .map(text_of)
        .filter(|t| !t.is_empty())
        .or_else(|| {
            doc.select(&sel("meta[property='og:title']"))
                .next()
                .and_then(|m| m.value().attr("content"))
                .map(|t| t.split('-').next().unwrap_or("").trim().to_owned())
        })
        .ok_or("the title page has no title")?;
    let mut description = root.select(&sel("#synopsis")).map(text_of).collect::<Vec<_>>().join(" ");
    if let Some(alternative) = first(root, "header > p").map(text_of).filter(|t| !t.is_empty()) {
        let label = if site.lang == "ar" { "أسماء أخرى: " } else { "Alternative names: " };
        description = format!("{description}\n\n{label}{alternative}");
    }
    let genres: Vec<String> = root.select(&sel(site.details.genres)).map(text_of).collect();
    let mut author = first(root, "span#author").map(text_of);
    let mut artist = first(root, "span#artist").map(text_of);
    let mut state = first(root, "span[data-status]").map_or(status::UNKNOWN, |e| status_of(&text_of(e)));
    for row in root.select(&sel(site.details.info)) {
        let heading = some(own_text(row)).or_else(|| first(row, site.details.info_title).map(text_of)).unwrap_or_default();
        let value = row.select(&sel(site.details.info_value)).map(text_of).collect::<Vec<_>>().join(" ");
        let has = |names: &[&str]| names.iter().any(|n| heading.contains(n));
        if state == status::UNKNOWN && has(&["Status", "Estado", "الحالة"]) {
            state = status_of(&value);
        } else if author.is_none() && has(&["Author", "Autor", "Mangaka", "الكاتب", "Yazar"]) {
            author = Some(value);
        } else if artist.is_none() && has(&["Artist", "Artista", "الرسام", "Çizer"]) {
            artist = Some(value);
        }
    }
    Ok(Manga {
        url: String::new(),
        title,
        author: author.and_then(some),
        artist: artist.and_then(some),
        description: some(description.trim().to_owned()),
        genre: some(genres.join(", ")),
        status: state,
        thumbnail_url: attr(root, "img", "src").and_then(|src| absolute(site, &src)),
    })
}

fn script_text(element: ElementRef) -> String {
    element.text().collect()
}

/// The chapter feed a title page names (the Kotlin `getChapterFeedUrl`): the label in its `clwd.run('...')` script,
/// else the feed address in its `#myUL` script, else the title's name (or its `data-label`) as the label.
pub fn chapter_feed(site: &Site, html: &str) -> String {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let label = match first(root, "#clwd > script") {
        Some(script) => Regex::new(r#"clwd\.run\(["'](.*?)["']\)"#).expect("a valid pattern").captures(&script_text(script)).map(|c| c[1].to_owned()),
        None => {
            if let Some(src) = attr(root, "#myUL > script", "src").filter(|s| s.contains('?'))
                && let Some(url) = absolute(site, src.split('?').next().unwrap_or(""))
            {
                return format!("{url}?alt=json");
            }
            None
        }
    };
    let label = label.or_else(|| match site.data_label {
        true => attr(root, "[data-label]", "data-label"),
        false => parse_details(site, html).ok().map(|m| m.title),
    });
    feed_url(site, &format!("{}/{}", site.chapter_label, label.unwrap_or_default()))
}

/// One page of a chapter feed: the number of posts in all and this page's posts.
pub fn parse_feed(json: &str) -> Res<(usize, Vec<Entry>)> {
    let body = parse_feed_json(json)?;
    Ok((body.total.t.parse().unwrap_or(0), body.entry))
}

fn millis(date: &str) -> i64 {
    DateTime::parse_from_rfc3339(date.trim()).map_or(0, |d| d.timestamp_millis())
}

/// The chapter posts of a feed, newest first (the Kotlin `getChapterList`).
pub fn chapters(site: &Site, entries: Vec<Entry>) -> Vec<Chapter> {
    let arabic = Regex::new(r"الفصل\s*(\d+(?:\.\d+)?)").expect("a valid pattern");
    entries
        .into_iter()
        .filter(|e| e.labelled(site.chapter_label))
        .filter_map(|e| {
            let (first, second) = if site.yokai { (&e.updated.t, &e.published.t) } else { (&e.published.t, &e.updated.t) };
            let date = if first.trim().is_empty() { second } else { first };
            let mut chapter = Chapter { url: e.path()?, name: e.title.t.trim().to_owned(), date_upload: millis(date), ..Default::default() };
            if site.yokai {
                let number = match chapter.name.get(..7) {
                    Some(head) if head.eq_ignore_ascii_case("chapter") => {
                        let number = chapter.name[7..].split_whitespace().next().unwrap_or("").to_owned();
                        chapter.name = format!("الفصل {number}");
                        Some(number)
                    }
                    _ => arabic.captures(&chapter.name).map(|c| c[1].to_owned()),
                };
                if let Some(n) = number.and_then(|n| n.parse().ok()) {
                    chapter.chapter_number = n;
                }
            }
            Some(chapter)
        })
        .collect()
}

/// Yokai: the title page's download links after the feed's chapters, each address once.
pub fn add_downloads(chapters: &mut Vec<Chapter>, html: &str) {
    let doc = Html::parse_document(html);
    for link in doc.select(&sel("div#download > div.index-list > a")) {
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let name = text_of(link);
        let chapter_number = name.split(' ').next().and_then(|n| n.parse().ok()).unwrap_or(1.0);
        let chapter = Chapter { url: path_of(href), name, chapter_number, ..Default::default() };
        let key = |c: &Chapter| c.url.split('?').next().unwrap_or("").to_owned();
        if !chapters.iter().any(|c| key(c) == key(&chapter)) {
            chapters.push(chapter);
        }
    }
}

/// The images that `css` selects, each once and in page order.
fn images(site: &Site, doc: &Html, css: &str) -> Vec<String> {
    doc.select(&sel(css)).filter_map(|img| absolute(site, img.value().attr("src")?)).collect()
}

/// A chapter page's images (the Kotlin `pageListParse`).
pub fn parse_pages(site: &Site, html: &str) -> Vec<String> {
    images(site, &Html::parse_document(html), &format!("{} img[src]", site.pages))
}

/// The Blogger post id of a page.
pub fn post_id(html: &str) -> Option<String> {
    Regex::new(r"'postId':\s*'(\d+)'").expect("a valid pattern").captures(html).map(|c| c[1].to_owned())
}

/// The images in the body of a post's feed entry (`/feeds/posts/default/<id>?alt=json`).
pub fn parse_post(site: &Site, json: &str) -> Res<Vec<String>> {
    #[derive(Deserialize)]
    struct Post {
        entry: Entry,
    }
    let post: Post = serde_json::from_str(json).map_err(|e| format!("invalid post: {e}"))?;
    Ok(images(site, &Html::parse_fragment(&post.entry.content.t), "img[src]"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_are_asked_larger() {
        assert_eq!(cover("https://blogger.googleusercontent.com/img/b/R29/AVv/s72-c/x.jpg"), "https://blogger.googleusercontent.com/img/b/R29/AVv/w600/x.jpg");
        assert_eq!(cover("https://blogger.googleusercontent.com/img/a/AVvX=s72-c"), "https://blogger.googleusercontent.com/img/a/AVvX=w600");
        assert_eq!(cover("https://x.test/a.jpg"), "https://x.test/a.jpg");
    }

    #[test]
    fn statuses_are_read_in_each_language() {
        assert_eq!((status_of(" مستمرة "), status_of("Completed"), status_of("Dropped"), status_of("?")), (1, 2, 5, 0));
    }

    #[test]
    fn a_range_replaces_the_old_one() {
        let feed = "https://b.test/feeds/posts/default/-/Usogui?orderby=published&alt=json&max-results=999";
        assert_eq!(feed_range(feed, 151), "https://b.test/feeds/posts/summary/-/Usogui?orderby=published&alt=json&start-index=151&max-results=150");
    }
}
