//! InkStory's data: the site's server functions (`https://inkstory.net/_serverFn/<id>`, TanStack Start) for the catalogue,
//! a book, its chapters and a chapter's pages, and the shared API's update feed (`https://api.inuko.me/v2`) for Latest.
//! The addresses, the parsers and the page image key; no network here, `lib.rs` sends the requests through the host.

use kuro_guest::Res;
use kuro_guest::wire::status::{COMPLETED, ON_HIATUS, ONGOING};
use kuro_guest::wire::{Chapter, Manga, MangasPage};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::seroval;

pub const SITE: &str = "https://inkstory.net";
pub const API: &str = "https://api.inuko.me/v2";
/// The `serviceName` the shared API knows this site by (the Kotlin's `serviceName`).
pub const SERVICE: &str = "inkstory";
/// Titles a catalogue page, as the Kotlin asks for.
const PAGE_SIZE: u32 = 40;
/// Titles a feed page.
const FEED_SIZE: u32 = 30;

/// Server function ids, from the Kotlin: SHA-256 of the function's place in the site's code, the same for every
/// inuko.me site.
const FN_CATALOGUE: &str = "5e920c6f2538a89f74b8cc297001d862340f690c4684e8ab6c14522960998b2e";
const FN_BOOK: &str = "f1eb190aebb53f98ebd70df7ef5ec57105819b52503dba8d8943811d2bced2f2";
const FN_CHAPTERS: &str = "1c9d8e028fe5eab8c867ee5fc39b0a75acc20270c6cecf479cfd582404e947bc";
const FN_CHAPTER: &str = "c342d30c43b5931eec428542eb9d2bf68431a41a046ae931929f8f018d7f5505";
/// The reading restrictions the catalogue lists, as the Kotlin asks for.
const READING_RESTRICTIONS: [&str; 3] = ["NONE", "AUTHORIZED_ONLY", "LIBRARY_ONLY"];

fn parse<'a, T: Deserialize<'a>>(body: &'a str, what: &str) -> Res<T> {
    serde_json::from_str(body).map_err(|e| format!("unexpected InkStory {what}: {e}"))
}

fn from_value<T: DeserializeOwned>(value: Value, what: &str) -> Res<T> {
    serde_json::from_value(value).map_err(|e| format!("unexpected InkStory {what}: {e}"))
}

fn non_empty(text: Option<&str>) -> Option<String> {
    text.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// Encodes a query value.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The address of a server function call with input `data`.
fn call_url(id: &str, data: &Value) -> String {
    format!("{SITE}/_serverFn/{id}?payload={}", encode(&seroval::payload(data)))
}

/// The result of a call's answer; an error when the site reports the item as not found.
pub fn result(body: &str, what: &str) -> Res<Value> {
    seroval::result(body)?.ok_or_else(|| format!("InkStory: {what} not found"))
}

// ------------------------------------------------------------------ list and search

/// The catalogue, most viewed first (the Kotlin's Popular and search), filtered by `query`. Pages count from 0 on the
/// site.
pub fn catalogue_url(page: i32, query: &str) -> String {
    let mut input = json!({
        "size": PAGE_SIZE,
        "page": (page - 1).max(0),
        "sort": "viewsCount",
        "orderBy": "desc",
        "readingRestriction": READING_RESTRICTIONS,
    });
    if !query.trim().is_empty() {
        input["search"] = json!(query.trim());
    }
    call_url(FN_CATALOGUE, &input)
}

/// The chapter update feed, one row a title (Latest).
pub fn feed_url(page: i32) -> String {
    format!("{API}/chapter-update-feed?serviceName={SERVICE}&onlyBorderChapters=true&page={}&size={FEED_SIZE}", (page - 1).max(0))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct NameDto {
    ru: Option<String>,
    en: Option<String>,
    original: Option<String>,
}

impl NameDto {
    /// The Russian title, else the English, else the original, else the slug.
    fn resolve(&self, slug: &str) -> String {
        [&self.ru, &self.en, &self.original].into_iter().find_map(|n| non_empty(n.as_deref())).unwrap_or_else(|| slug.to_owned())
    }
}

#[derive(Deserialize)]
struct BookDto {
    slug: String,
    poster: Option<String>,
    #[serde(default)]
    name: NameDto,
}

impl BookDto {
    fn manga(self) -> Manga {
        Manga {
            title: self.name.resolve(&self.slug),
            url: self.slug,
            thumbnail_url: self.poster,
            ..Default::default()
        }
    }
}

#[derive(Deserialize)]
struct CatalogueDto {
    total: u64,
    books: Vec<BookDto>,
}

/// A catalogue page: its books, and whether `total` leaves more after page `page`.
pub fn parse_catalogue(body: &str, page: i32) -> Res<MangasPage> {
    let dto: CatalogueDto = from_value(result(body, "catalogue")?, "catalogue")?;
    let has_next_page = (page.max(1) as u64) * u64::from(PAGE_SIZE) < dto.total;
    Ok(MangasPage { mangas: dto.books.into_iter().map(BookDto::manga).collect(), has_next_page })
}

#[derive(Deserialize)]
struct FeedDto {
    book: BookDto,
}

pub fn parse_feed(body: &str) -> Res<MangasPage> {
    let rows: Vec<FeedDto> = parse(body, "update feed")?;
    let has_next_page = rows.len() >= FEED_SIZE as usize;
    Ok(MangasPage { mangas: rows.into_iter().map(|row| row.book.manga()).collect(), has_next_page })
}

// ------------------------------------------------------------------ details

/// A book by its slug, the manga url (as in the Kotlin, so Aniyomi backups match).
pub fn book_url(slug: &str) -> String {
    call_url(FN_BOOK, &json!({ "slug": slug, "createView": false }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailsDto {
    id: String,
    slug: String,
    #[serde(default)]
    name: NameDto,
    poster: Option<String>,
    description: Option<String>,
    status: Option<String>,
    #[serde(default)]
    labels: Vec<LabelDto>,
    #[serde(default)]
    formats: Vec<String>,
    #[serde(default)]
    relations: Vec<RelationDto>,
    #[serde(default)]
    external_links: Vec<String>,
    average_rating: Option<f64>,
    rating_votes_count: Option<i64>,
    views_count: Option<i64>,
    likes_count: Option<i64>,
    bookmarks_count: Option<i64>,
}

#[derive(Deserialize)]
struct LabelDto {
    name: Option<String>,
}

#[derive(Deserialize)]
struct RelationDto {
    #[serde(rename = "type")]
    kind: Option<String>,
    publisher: Option<PublisherDto>,
}

#[derive(Deserialize)]
struct PublisherDto {
    name: Option<String>,
}

fn status_of(text: &str) -> i32 {
    match text {
        "ONGOING" => ONGOING,
        "DONE" => COMPLETED,
        "FROZEN" => ON_HIATUS,
        _ => 0,
    }
}

/// 1234567 as "1.2M", 1234 as "1.2k" (the Kotlin's `formatInt`).
fn short(value: i64) -> String {
    match value {
        v if v >= 1_000_000 => format!("{:.1}M", v as f64 / 1_000_000.0),
        v if v >= 1_000 => format!("{:.1}k", v as f64 / 1_000.0),
        v => v.to_string(),
    }
}

/// The publishers of the relations of one type, once each, in order.
fn relation(relations: &[RelationDto], kind: &str) -> Option<String> {
    let mut names: Vec<String> = Vec::new();
    for r in relations.iter().filter(|r| r.kind.as_deref() == Some(kind)) {
        if let Some(name) = non_empty(r.publisher.as_ref().and_then(|p| p.name.as_deref()))
            && !names.contains(&name)
        {
            names.push(name);
        }
    }
    (!names.is_empty()).then(|| names.join(", "))
}

fn details(body: &str) -> Res<DetailsDto> {
    from_value(result(body, "book")?, "book")
}

/// The book id the chapter list asks for.
pub fn book_id(body: &str) -> Res<String> {
    Ok(details(body)?.id)
}

/// The details: summary, rating, counts and links in the description; labels, then formats, as genres; authors and
/// artists from the relations.
pub fn parse_details(body: &str) -> Res<Manga> {
    let dto = details(body)?;
    let mut lines: Vec<String> = Vec::new();
    if let Some(summary) = non_empty(dto.description.as_deref()) {
        lines.push(summary);
        lines.push(String::new());
    }
    if let Some(rating) = dto.average_rating.filter(|_| dto.rating_votes_count != Some(0)) {
        let votes = dto.rating_votes_count.map(|v| format!(" (оценок: {v})")).unwrap_or_default();
        lines.push(format!("Рейтинг: {rating:.2}{votes}"));
    }
    for (label, count) in [("Просмотры", dto.views_count), ("Лайки", dto.likes_count), ("Закладки", dto.bookmarks_count)] {
        if let Some(count) = count {
            lines.push(format!("{label}: {}", short(count)));
        }
    }
    if !dto.external_links.is_empty() {
        lines.push("Внешние ссылки:".to_owned());
        lines.extend(dto.external_links.iter().map(|link| format!("- {link}")));
    }
    let genre: Vec<String> = dto
        .labels
        .iter()
        .filter_map(|l| non_empty(l.name.as_deref()))
        .chain(dto.formats.iter().map(|f| f.to_lowercase().replace('_', " ")))
        .collect();
    Ok(Manga {
        title: dto.name.resolve(&dto.slug),
        author: relation(&dto.relations, "AUTHOR"),
        artist: relation(&dto.relations, "ARTIST"),
        description: non_empty(Some(&lines.join("\n"))),
        genre: (!genre.is_empty()).then(|| genre.join(", ")),
        status: dto.status.as_deref().map_or(0, status_of),
        thumbnail_url: dto.poster,
        url: dto.slug,
    })
}

// ------------------------------------------------------------------ chapters

/// Every approved chapter of a book, all branches, newest first.
pub fn chapters_url(book_id: &str) -> String {
    call_url(FN_CHAPTERS, &json!(book_id))
}

#[derive(Deserialize)]
struct ChapterListDto {
    #[serde(default)]
    chapters: Vec<ChapterDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChapterDto {
    id: String,
    title: Option<String>,
    number: Option<f64>,
    volume: Option<f64>,
    created_at: Option<String>,
    publisher_names: Option<String>,
}

/// 3.0 as "3", 0.5 as "0.5".
fn number(value: f64) -> String {
    let text = value.to_string();
    text.strip_suffix(".0").map(str::to_owned).unwrap_or(text)
}

/// "Том 1 Глава 84" as the Kotlin names a chapter: the volume, then the site's title ("Глава 84"); "Глава <n>" when the
/// site gives no title.
fn chapter_name(dto: &ChapterDto) -> String {
    let title = non_empty(dto.title.as_deref()).or_else(|| dto.number.map(|n| format!("Глава {}", number(n)))).unwrap_or_else(|| "Глава".to_owned());
    match dto.volume {
        Some(v) => format!("Том {} {title}", number(v)),
        None => title,
    }
}

/// Every chapter of every branch (the Kotlin's default), in the site's order (newest first); the chapter's teams are
/// the scanlator. The chapter url is its id, as in the Kotlin.
pub fn parse_chapters(body: &str) -> Res<Vec<Chapter>> {
    let list: ChapterListDto = from_value(result(body, "chapter list")?, "chapter list")?;
    Ok(list
        .chapters
        .iter()
        .map(|dto| Chapter {
            url: dto.id.clone(),
            name: chapter_name(dto),
            date_upload: dto.created_at.as_deref().map_or(0, iso_millis),
            chapter_number: dto.number.map_or(-1.0, |n| n as f32),
            scanlator: non_empty(dto.publisher_names.as_deref()),
        })
        .collect())
}

/// Epoch milliseconds of an ISO-8601 UTC timestamp (`2023-05-31T15:20:01.449Z`); 0 when it does not parse.
pub fn iso_millis(text: &str) -> i64 {
    fn num(s: &str) -> Option<i64> {
        s.parse().ok()
    }
    let parse = || -> Option<i64> {
        let (date, time) = text.trim().split_once('T')?;
        let mut d = date.splitn(3, '-');
        let (y, m, day) = (num(d.next()?)?, num(d.next()?)?, num(d.next()?)?);
        let time = time.trim_end_matches('Z');
        let (hms, frac) = time.split_once('.').unwrap_or((time, ""));
        let mut t = hms.splitn(3, ':');
        let (h, mi, s) = (num(t.next()?)?, num(t.next()?)?, num(t.next().unwrap_or("0"))?);
        let ms = format!("{frac:0<3}").get(..3)?.parse::<i64>().ok()?;
        // Days since 1970-01-01 (Hinnant's civil-from-days, inverted).
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
        let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
        Some(((days * 24 + h) * 60 + mi) * 60_000 + s * 1000 + ms).filter(|v| *v > 0)
    };
    parse().unwrap_or(0)
}

// ------------------------------------------------------------------ pages

pub fn chapter_url(chapter_id: &str) -> String {
    call_url(FN_CHAPTER, &json!({ "id": chapter_id, "createView": false }))
}

#[derive(Deserialize)]
struct PagesDto {
    #[serde(default)]
    pages: Vec<PageDto>,
}

#[derive(Deserialize)]
struct PageDto {
    index: Option<i32>,
    image: Option<String>,
}

/// The page images in page order, as the site gives them (an `x` image is XOR-ciphered; `unxor` reads it back).
pub fn parse_pages(body: &str) -> Res<Vec<String>> {
    let mut pages = from_value::<PagesDto>(result(body, "chapter")?, "page list")?.pages;
    pages.sort_by_key(|p| p.index.unwrap_or(i32::MAX));
    Ok(pages.iter().filter_map(|p| non_empty(p.image.as_deref())).collect())
}

/// The key of the XOR cipher on page images (the Kotlin's `IMAGE_KEY`).
const KEY: &[u8] = b"UySkp0BzPhwlvP2V";
/// The bytes the image signatures need.
const SIGNATURE: usize = 12;

/// Whether the bytes start like a JPEG, PNG, GIF, WebP or AVIF image.
fn looks_like_image(b: &[u8]) -> bool {
    b.len() >= SIGNATURE
        && (b.starts_with(&[0xFF, 0xD8, 0xFF])
            || b.starts_with(b"\x89PNG")
            || b.starts_with(b"GIF8")
            || (b.starts_with(b"RIFF") && &b[8..12] == b"WEBP")
            || &b[4..11] == b"ftypavi")
}

fn xor(bytes: &mut [u8]) {
    for (i, b) in bytes.iter_mut().enumerate() {
        *b ^= KEY[i % KEY.len()];
    }
}

/// The XOR-ciphered page image read back, or `None` when the bytes already are an image (or are not this cipher).
pub fn unxor(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < SIGNATURE || looks_like_image(bytes) {
        return None;
    }
    let mut head = bytes[..SIGNATURE].to_vec();
    xor(&mut head);
    if !looks_like_image(&head) {
        return None;
    }
    let mut plain = bytes.to_vec();
    xor(&mut plain);
    Some(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Saved from the site on 2026-11-21 through a WebView2 page (the site's Anubis check stops plain clients), untrimmed.
    const POPULAR: &str = include_str!("../fixtures/fn-catalogue-popular.json");
    const SEARCH: &str = include_str!("../fixtures/fn-catalogue-search.json");
    const BOOK: &str = include_str!("../fixtures/fn-book.json");
    const MISSING: &str = include_str!("../fixtures/fn-book-missing.json");
    const CHAPTERS: &str = include_str!("../fixtures/fn-chapters.json");
    const CHAPTER: &str = include_str!("../fixtures/fn-chapter.json");
    const FEED: &str = include_str!("../fixtures/feed.json");
    const PAGE_START: &[u8] = include_bytes!("../fixtures/page-start.bin");

    /// The input a call url carries, decoded back.
    fn input_of(url: &str) -> Value {
        let query = url.split_once("?payload=").unwrap().1;
        let bytes: Vec<u8> = {
            let b = query.as_bytes();
            let mut out = Vec::new();
            let mut i = 0;
            while i < b.len() {
                if b[i] == b'%' {
                    out.push(u8::from_str_radix(&query[i + 1..i + 3], 16).unwrap());
                    i += 3;
                } else {
                    out.push(b[i]);
                    i += 1;
                }
            }
            out
        };
        let tree: Value = serde_json::from_slice(&bytes).unwrap();
        let answer = json!({ "t": 10, "i": 99, "p": { "k": ["result"], "v": [tree["t"]] }, "o": 0 }).to_string();
        seroval::result(&answer).unwrap().unwrap()["data"].clone()
    }

    #[test]
    fn calls_go_to_the_site_with_the_kotlins_inputs() {
        let popular = catalogue_url(1, "");
        assert!(popular.starts_with(&format!("{SITE}/_serverFn/{FN_CATALOGUE}?payload=")));
        assert_eq!(input_of(&popular), json!({ "size": 40, "page": 0, "sort": "viewsCount", "orderBy": "desc", "readingRestriction": ["NONE", "AUTHORIZED_ONLY", "LIBRARY_ONLY"] }));
        assert_eq!(input_of(&catalogue_url(3, " solo лев "))["search"], "solo лев");
        assert_eq!(input_of(&catalogue_url(3, ""))["page"], 2);
        assert_eq!(input_of(&book_url("solo-leveling")), json!({ "slug": "solo-leveling", "createView": false }));
        assert_eq!(input_of(&chapters_url("baf2")), json!("baf2"));
        assert_eq!(input_of(&chapter_url("4fd0")), json!({ "id": "4fd0", "createView": false }));
        assert_eq!(feed_url(3), "https://api.inuko.me/v2/chapter-update-feed?serviceName=inkstory&onlyBorderChapters=true&page=2&size=30");
    }

    #[test]
    fn the_catalogue_gives_slugs_russian_titles_posters_and_more_pages() {
        let page = parse_catalogue(POPULAR, 1).unwrap();
        assert_eq!(page.mangas.len(), 3);
        assert_eq!((page.mangas[0].url.as_str(), page.mangas[0].title.as_str()), ("solo-leveling", "Поднятие уровня в одиночку"));
        assert!(page.mangas[0].thumbnail_url.as_deref().unwrap().starts_with("https://static.inuko.me/"));
        assert!(page.has_next_page, "21901 titles");
        let search = parse_catalogue(SEARCH, 1).unwrap();
        assert_eq!(search.mangas[0].url, "solo-leveling");
        assert!(!search.has_next_page, "28 titles fit on one page of 40");
    }

    #[test]
    fn the_feed_gives_the_books_of_the_updates() {
        let page = parse_feed(FEED).unwrap();
        assert_eq!(page.mangas[0].url, "a-genius-warrior-who-remembers-everything");
        assert_eq!(page.mangas[0].title, "Гений боевых искусств, помнящий всё");
    }

    #[test]
    fn details_read_the_status_genres_people_and_summary() {
        let manga = parse_details(BOOK).unwrap();
        assert_eq!((manga.url.as_str(), manga.title.as_str(), manga.status), ("there39s-a-mad-dog-living-in-the-duke39s-house", "Бешеный пес герцогского дома", ONGOING));
        let description = manga.description.as_deref().unwrap();
        assert!(description.starts_with("Прошлую жизнь я прожил как пёс на цепи."));
        assert!(description.contains("Рейтинг: 9.08 (оценок: 145)") && description.contains("Закладки: 2.5k"));
        assert!(manga.genre.as_deref().unwrap().starts_with("Аристократия"));
        assert!(manga.genre.as_deref().unwrap().ends_with("colored, webtoon"));
        assert_eq!((manga.author.as_deref(), manga.artist.as_deref()), (Some("Fanmison, Luminara"), Some("Luminara")));
        assert_eq!(book_id(BOOK).unwrap(), "baf22303-c97f-4582-8dd5-277aa065a15f");
    }

    #[test]
    fn a_missing_book_is_not_found() {
        assert_eq!(parse_details(MISSING).unwrap_err(), "InkStory: book not found");
    }

    #[test]
    fn chapters_carry_names_numbers_dates_and_teams() {
        let chapters = parse_chapters(CHAPTERS).unwrap();
        assert_eq!(chapters.len(), 84);
        let first = &chapters[0];
        assert_eq!((first.url.as_str(), first.name.as_str()), ("4fd0ac54-b8bd-401d-9dcd-207d67b052f8", "Том 1 Глава 84"));
        assert_eq!((first.chapter_number, first.date_upload), (84.0, 1_795_256_599_297));
        assert_eq!(first.scanlator.as_deref(), Some("F4keSm1le"));
        assert_eq!(chapters[83].name, "Том 1 Глава 1");
    }

    #[test]
    fn pages_come_in_order_as_the_site_names_them() {
        let pages = parse_pages(CHAPTER).unwrap();
        assert_eq!(pages[0], "https://static.inuko.me/chapters/4fd0ac54-b8bd-401d-9dcd-207d67b052f8/1bb7d105-49a2-xbc1-b031-8b91ea0e2da6.webp");
        assert_eq!(pages[1], "https://static.inuko.me/chapters/4fd0ac54-b8bd-401d-9dcd-207d67b052f8/d3a49c68-4b3d-x3ae-bee7-f35621da684f.webp");
    }

    #[test]
    fn a_xor_page_reads_back_as_a_jpeg_and_a_plain_image_is_left_alone() {
        let plain = unxor(PAGE_START).unwrap();
        assert!(plain.starts_with(&[0xFF, 0xD8, 0xFF]));
        assert_eq!(unxor(&plain), None);
        assert_eq!(unxor(b"not an image at all"), None);
    }

    #[test]
    fn dates_read_with_and_without_fractions() {
        assert_eq!(iso_millis("2018-11-17T06:58:18Z"), 1_542_437_898_000);
        assert_eq!(iso_millis("2024-03-13T16:49:08.530187Z"), 1_710_348_548_530);
        assert_eq!(iso_millis(""), 0);
    }
}
