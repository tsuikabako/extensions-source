//! The Atsumaru API (<https://atsu.moe>) as pure functions: URLs and one parser per answer. Ported from the keiyoushi
//! `en/atsumaru` extension (Apache-2.0), at its defaults (no excluded genres or scanlators, every upload of a chapter).

use chrono::{DateTime, Datelike};
use kuro_guest::Res;
use kuro_guest::wire::{Chapter, Manga, MangasPage, status};
use serde::Deserialize;
use serde_json::Value;

const BASE: &str = "https://atsu.moe";
/// Where covers and page images live; `atsu.moe/static/...` redirects there (covers) or is gone (pages).
const CDN: &str = "https://cdn.atsu.moe/static/";
pub const BROWSE_LIMIT: i32 = 40;
const SEARCH_LIMIT: i32 = 40;
const TYPES: &str = "&types=Manga,Manwha,Manhua,OEL&mediums=Comic";

// ================================ URLs =================================

fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn adult(show_adult: bool) -> &'static str {
    if show_adult { "&adult=1" } else { "" }
}

fn offset(page: i32) -> i32 {
    (page.max(1) - 1) * BROWSE_LIMIT
}

/// Today's most read titles.
pub fn popular_url(page: i32, show_adult: bool) -> String {
    format!("{BASE}/api/home2/popular?offset={}&limit={BROWSE_LIMIT}{TYPES}&timeframe=daily{}", offset(page), adult(show_adult))
}

pub fn latest_url(page: i32, show_adult: bool) -> String {
    format!("{BASE}/api/home2/recentlyUpdated?offset={}&limit={BROWSE_LIMIT}{TYPES}{}", offset(page), adult(show_adult))
}

/// The site's Typesense search, with the Kotlin's fixed filters (no hidden titles, no novels, nothing explicit).
pub fn search_url(page: i32, query: &str, show_adult: bool) -> String {
    let query = query.trim();
    let mut filter = vec!["hidden:!=true"];
    if !show_adult {
        filter.push("isAdult:=false");
    }
    filter.extend(["(mbContentRating:=[`Safe`,`Suggestive`,`Erotica`] || mbContentRating:!=*)", "medium:!=[`Novel`]", "views:>0"]);
    let mut url = format!(
        "{BASE}/collections/manga/documents/search?q={}&filter_by={}",
        if query.is_empty() { "*".to_owned() } else { encode(query) },
        encode(&filter.join(" && "))
    );
    if !query.is_empty() {
        url.push_str("&query_by=title%2CenglishTitle%2CotherNames%2Cauthors&query_by_weights=4%2C3%2C2%2C1&num_typos=4%2C3%2C2%2C1");
    }
    url.push_str(&format!("&page={}&per_page={SEARCH_LIMIT}", page.max(1)));
    url
}

/// The stored manga address is the site's id (`2VgNt`), as the Kotlin keeps it.
pub fn details_url(manga_url: &str) -> Res<String> {
    Ok(format!("{BASE}/api/manga/page?id={}", id(manga_url, "title")?))
}

pub fn chapters_url(manga_url: &str) -> Res<String> {
    Ok(format!("{BASE}/api/manga/allChapters?mangaId={}", id(manga_url, "title")?))
}

/// The stored chapter address is `<manga id>/<chapter id>`, as the Kotlin keeps it.
pub fn pages_url(chapter_url: &str) -> Res<String> {
    let (manga, chapter) = chapter_url.split_once('/').ok_or_else(|| format!("not an Atsumaru chapter: {chapter_url}"))?;
    Ok(format!("{BASE}/api/read/chapter?mangaId={}&chapterId={}", id(manga, "chapter")?, id(chapter, "chapter")?))
}

/// Ids are short URL-safe texts (`2VgNt`, `_-n2`, `prIIkU`).
fn id<'a>(text: &'a str, kind: &str) -> Res<&'a str> {
    let ok = !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok { Ok(text) } else { Err(format!("not an Atsumaru {kind}: {text}")) }
}

/// An image path (`posters/x.jpg`, `/static/pages/...`, `//host/...` or a full address) on the CDN.
fn image_url(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("//") {
        return format!("https://{rest}");
    }
    if path.starts_with("http") {
        return path.to_owned();
    }
    format!("{CDN}{}", path.trim_start_matches('/').trim_start_matches("static/"))
}

// =============================== Parsers ===============================

fn parse<'a, T: Deserialize<'a>>(json: &'a str) -> Res<T> {
    serde_json::from_str(json).map_err(|e| format!("unexpected answer from Atsumaru: {e}"))
}

/// A text or number as text.
fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Names from a list of texts or of `{name, type}` objects, with their type.
fn names(value: &Value) -> Vec<(String, Option<String>)> {
    let items = value.as_array().map_or(&[][..], Vec::as_slice);
    items
        .iter()
        .filter_map(|item| match item {
            Value::Object(o) => Some((text(o.get("name")?)?, o.get("type").and_then(Value::as_str).map(str::to_owned))),
            other => Some((text(other)?, None)),
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MangaDto {
    id: String,
    title: String,
    /// `poster` (a path, or an object of sizes) or `image`.
    #[serde(alias = "image")]
    poster: Option<Value>,
    #[serde(default)]
    authors: Value,
    synopsis: Option<String>,
    #[serde(default)]
    genres: Value,
    #[serde(default)]
    tags: Value,
    released: Option<Value>,
    status: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    views: Option<Value>,
    other_names: Option<Vec<String>>,
    avg_rating: Option<f64>,
    #[serde(default)]
    scanlators: Vec<ScanlatorDto>,
}

#[derive(Deserialize)]
struct ScanlatorDto {
    id: String,
    name: String,
}

impl MangaDto {
    /// The original poster (jpg or webp; the Kotlin takes the large AVIF copy when there is one).
    fn cover(&self) -> Option<String> {
        let path = match self.poster.as_ref()? {
            Value::Object(sizes) => sizes.get("image").or_else(|| sizes.get("largeImage"))?.as_str()?,
            Value::String(path) => path,
            _ => return None,
        };
        Some(image_url(path))
    }

    fn into_manga(self) -> Manga {
        let mut description = Vec::new();
        if let Some(rating) = self.avg_rating.filter(|r| *r > 0.0) {
            description.push(format!("**Rating**: {rating:.2}/10"));
        }
        let released = self.released.as_ref().and_then(|r| r.as_i64().or_else(|| r.as_str()?.parse().ok()));
        if let Some(date) = released.filter(|ms| *ms > 0).and_then(DateTime::from_timestamp_millis) {
            description.push(format!("**Year**: {}", date.year()));
        }
        if let Some(views) = self.views.as_ref().and_then(text) {
            description.push(format!("**Views**: {views}"));
        }
        if let Some(synopsis) = self.synopsis.as_deref().filter(|s| !s.trim().is_empty()) {
            description.push(format!("\n**Synopsis**: {synopsis}"));
        }
        let other: Vec<String> = self.other_names.iter().flatten().filter(|n| **n != self.title).map(|n| format!("- {n}")).collect();
        if !other.is_empty() {
            description.push(format!("\n**Alternative Names**:\n{}", other.join("\n")));
        }
        let genres = if self.genres.is_array() { &self.genres } else { &self.tags };
        let genre: Vec<String> = self.kind.clone().into_iter().chain(names(genres).into_iter().map(|(name, _)| name)).collect();
        let people = names(&self.authors);
        let join = |wanted: &dyn Fn(Option<&str>) -> bool| {
            let list: Vec<&str> = people.iter().filter(|(_, kind)| wanted(kind.as_deref())).map(|(name, _)| name.as_str()).collect();
            (!list.is_empty()).then(|| list.join(", "))
        };
        Manga {
            thumbnail_url: self.cover(),
            author: join(&|kind| matches!(kind, None | Some("Author"))),
            artist: join(&|kind| kind == Some("Artist")),
            description: (!description.is_empty()).then(|| description.join("\n")),
            genre: (!genre.is_empty()).then(|| genre.join(", ")),
            status: match self.status.as_deref().map(|s| s.trim().to_lowercase()).as_deref() {
                Some("ongoing") => status::ONGOING,
                Some("completed") => status::COMPLETED,
                Some("hiatus") => status::ON_HIATUS,
                Some("canceled") => status::CANCELLED,
                _ => status::UNKNOWN,
            },
            url: self.id,
            title: self.title,
        }
    }
}

#[derive(Deserialize)]
struct BrowseDto {
    items: Vec<MangaDto>,
}

/// A popular or latest page. The API gives no total; a full page means there may be more.
pub fn parse_browse(json: &str) -> Res<MangasPage> {
    let items = parse::<BrowseDto>(json)?.items;
    let has_next_page = items.len() as i32 >= BROWSE_LIMIT;
    Ok(MangasPage { mangas: items.into_iter().map(MangaDto::into_manga).collect(), has_next_page })
}

#[derive(Deserialize)]
struct Hit {
    document: MangaDto,
}

#[derive(Deserialize)]
struct RequestParams {
    per_page: i32,
}

#[derive(Deserialize)]
struct SearchDto {
    page: i32,
    found: i32,
    hits: Vec<Hit>,
    request_params: RequestParams,
}

pub fn parse_search(json: &str) -> Res<MangasPage> {
    let result: SearchDto = parse(json)?;
    Ok(MangasPage {
        has_next_page: result.page * result.request_params.per_page < result.found,
        mangas: result.hits.into_iter().map(|hit| hit.document.into_manga()).collect(),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailsDto {
    manga_page: MangaDto,
}

pub fn parse_details(json: &str) -> Res<Manga> {
    Ok(parse::<DetailsDto>(json)?.manga_page.into_manga())
}

/// The scanlation groups of a title (id, name), read from its details for the chapter list.
pub fn parse_scanlators(json: &str) -> Res<Vec<(String, String)>> {
    Ok(parse::<DetailsDto>(json)?.manga_page.scanlators.into_iter().map(|s| (s.id, s.name)).collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChapterDto {
    id: String,
    number: f32,
    title: String,
    scanlation_manga_id: Option<String>,
    created_at: Option<Value>,
}

#[derive(Deserialize)]
struct ChaptersDto {
    chapters: Vec<ChapterDto>,
}

/// Epoch milliseconds, or an ISO 8601 time; 0 when it does not read.
fn millis(value: &Value) -> i64 {
    match value {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => DateTime::parse_from_rfc3339(s).map_or(0, |d| d.timestamp_millis()),
        _ => 0,
    }
}

/// Every upload of every chapter, highest number first, then by group, then newest first (as the Kotlin sorts them).
pub fn parse_chapters(json: &str, manga_url: &str, scanlators: &[(String, String)]) -> Res<Vec<Chapter>> {
    let manga = id(manga_url, "title")?;
    let mut chapters: Vec<Chapter> = parse::<ChaptersDto>(json)?
        .chapters
        .into_iter()
        .map(|c| Chapter {
            url: format!("{manga}/{}", c.id),
            name: c.title,
            date_upload: c.created_at.as_ref().map_or(0, millis),
            chapter_number: c.number,
            scanlator: c.scanlation_manga_id.and_then(|group| scanlators.iter().find(|(id, _)| *id == group).map(|(_, name)| name.clone())),
        })
        .collect();
    chapters.sort_by(|a, b| {
        b.chapter_number.total_cmp(&a.chapter_number).then_with(|| a.scanlator.cmp(&b.scanlator)).then_with(|| b.date_upload.cmp(&a.date_upload))
    });
    Ok(chapters)
}

#[derive(Deserialize)]
struct PageDto {
    image: String,
}

#[derive(Deserialize)]
struct PagesDto {
    pages: Vec<PageDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadDto {
    read_chapter: PagesDto,
}

/// The page images, on the CDN (`atsu.moe/static/pages/...` answers 410).
pub fn parse_pages(json: &str) -> Res<Vec<String>> {
    Ok(parse::<ReadDto>(json)?.read_chapter.pages.into_iter().map(|p| image_url(&p.image)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_carry_the_kotlin_parameters() {
        assert_eq!(
            popular_url(2, false),
            "https://atsu.moe/api/home2/popular?offset=40&limit=40&types=Manga,Manwha,Manhua,OEL&mediums=Comic&timeframe=daily"
        );
        assert!(latest_url(1, true).ends_with("offset=0&limit=40&types=Manga,Manwha,Manhua,OEL&mediums=Comic&adult=1"));
        let search = search_url(1, "witch hat", false);
        assert!(search.starts_with("https://atsu.moe/collections/manga/documents/search?q=witch%20hat&filter_by=hidden%3A%21%3Dtrue%20%26%26%20isAdult%3A%3Dfalse%20%26%26"));
        assert!(search.contains("&query_by=title%2CenglishTitle") && search.ends_with("&page=1&per_page=40"));
        let browse = search_url(2, "", true);
        assert!(browse.contains("?q=*&") && !browse.contains("isAdult") && !browse.contains("query_by"));
        assert_eq!(details_url("2VgNt").unwrap(), "https://atsu.moe/api/manga/page?id=2VgNt");
        assert_eq!(chapters_url("_-n2").unwrap(), "https://atsu.moe/api/manga/allChapters?mangaId=_-n2");
        assert_eq!(pages_url("2VgNt/prIIkU").unwrap(), "https://atsu.moe/api/read/chapter?mangaId=2VgNt&chapterId=prIIkU");
        assert!(pages_url("prIIkU").is_err());
        assert!(details_url("a&b=c").is_err());
    }

    #[test]
    fn browse_pages_read_their_items() {
        let page = parse_browse(include_str!("../fixtures/popular.json")).unwrap();
        assert_eq!(page.mangas.len(), 40);
        assert!(page.has_next_page);
        let first = &page.mangas[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("z871", "Copying Skills with Affinity!"));
        assert_eq!(first.thumbnail_url.as_deref(), Some("https://cdn.atsu.moe/static/posters/FMEmnwUea6Dh0uba.jpg"));
        assert_eq!(first.genre.as_deref(), Some("Manwha"));
        assert_eq!(parse_browse(include_str!("../fixtures/latest.json")).unwrap().mangas[0].url, "P-fE");
    }

    #[test]
    fn search_reads_the_hits() {
        let page = parse_search(include_str!("../fixtures/search.json")).unwrap();
        assert_eq!(page.mangas.len(), 4);
        assert!(!page.has_next_page);
        assert_eq!(page.mangas[0].url, "2VgNt");
        assert_eq!(page.mangas[0].thumbnail_url.as_deref(), Some("https://cdn.atsu.moe/static/posters/N3HO30V76uWZV9iG.jpg"));
    }

    #[test]
    fn details_carry_people_genres_and_status() {
        let manga = parse_details(include_str!("../fixtures/details.json")).unwrap();
        assert_eq!((manga.url.as_str(), manga.title.as_str()), ("2VgNt", "Witch Hat Atelier"));
        assert_eq!((manga.author.as_deref(), manga.artist.as_deref()), (Some("Kamome Shirahama"), Some("Kamome Shirahama")));
        assert_eq!(manga.status, status::ONGOING);
        assert!(manga.genre.as_deref().unwrap().starts_with("Manga, Adventure, Boys Love, Comedy"));
        let description = manga.description.unwrap();
        assert!(description.starts_with("**Rating**: 8.48/10\n**Year**: 2016\n**Views**: 17M\n\n**Synopsis**: In a world"), "{description}");
        assert!(description.contains("**Alternative Names**:\n- Atelier of Witch Hat"));
        assert_eq!(manga.thumbnail_url.as_deref(), Some("https://cdn.atsu.moe/static/posters/N3HO30V76uWZV9iG.jpg"));
    }

    #[test]
    fn chapters_name_their_group_and_keep_the_manga() {
        let groups = parse_scanlators(include_str!("../fixtures/details.json")).unwrap();
        let chapters = parse_chapters(include_str!("../fixtures/chapters.json"), "2VgNt", &groups).unwrap();
        assert_eq!(chapters.len(), 4);
        let first = &chapters[0];
        assert_eq!((first.url.as_str(), first.name.as_str(), first.chapter_number), ("2VgNt/prIIkU", "Chapter 99", 99.0));
        assert_eq!((first.date_upload, first.scanlator.as_deref()), (1_788_464_782_000, Some("Thunder")));
        assert!(chapters.windows(2).all(|w| w[0].chapter_number >= w[1].chapter_number));
        assert_eq!(millis(&Value::from("2026-10-01T12:00:00.000Z")), 1_790_856_000_000);
    }

    #[test]
    fn pages_are_on_the_cdn() {
        let pages = parse_pages(include_str!("../fixtures/pages.json")).unwrap();
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0], "https://cdn.atsu.moe/static/pages/cmrsjj6s100002eqvm319u8nr/prIIkU/2fcc8efe01d30477.avif");
    }
}
