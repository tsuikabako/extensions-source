//! The Desu API (<https://desu.uno/api/manga/>) as pure functions: URLs and one parser per answer. Ported from the
//! keiyoushi `ru/desu` extension (Apache-2.0), at its defaults (the English title first).

use kuro_guest::Res;
use kuro_guest::html::{Html, attr, first, sel, text_of};
use kuro_guest::wire::{Chapter, Manga, MangasPage, status};
use serde::Deserialize;

pub const BASE: &str = "https://desu.uno";
const API: &str = "https://desu.uno/api/manga";
const LIMIT: i32 = 20;

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

/// The catalog, newest updates first. It reads no search text, sort or filter (the site ignores them).
pub fn catalog_url(page: i32) -> String {
    format!("{API}/catalog/?limit={LIMIT}&page={}", page.max(1))
}

/// The site's quick search (HTML, at most 20 titles, no pages): the API's catalog ignores search text.
pub fn search_url(query: &str) -> String {
    format!("{BASE}/manga/search/?q={}", encode(query.trim()))
}

/// The stored manga address is `/<id>`, as the Kotlin keeps it.
pub fn details_url(manga_url: &str) -> Res<String> {
    Ok(format!("{API}/{}/", manga_id(manga_url)?))
}

pub fn chapters_url(manga_url: &str) -> Res<String> {
    Ok(format!("{API}/{}/chapters", manga_id(manga_url)?))
}

/// The stored chapter address is `/<manga id>/chapters/<chapter id>`, the API path of its pages.
pub fn pages_url(chapter_url: &str) -> Res<String> {
    let ok = chapter_url.strip_prefix('/').and_then(|rest| rest.split_once("/chapters/")).is_some_and(|(manga, chapter)| is_id(manga) && is_id(chapter));
    if ok { Ok(format!("{API}{chapter_url}")) } else { Err(format!("not a Desu chapter: {chapter_url}")) }
}

fn is_id(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

fn manga_id(url: &str) -> Res<&str> {
    let id = url.trim_matches('/');
    if is_id(id) { Ok(id) } else { Err(format!("not a Desu title: {url}")) }
}

// =============================== Parsers ===============================

#[derive(Deserialize)]
struct Score {
    value: Option<f32>,
    votes: Option<i64>,
}

#[derive(Deserialize)]
struct Cover {
    preview: Option<String>,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct MangaDto {
    id: i64,
    name: String,
    russian: String,
    kind: Option<String>,
    description: Option<String>,
    score: Option<Score>,
    content_rating: Option<String>,
    synonyms: Option<Vec<String>>,
    cover: Cover,
    trans_status: Option<String>,
    status: Option<String>,
    genres: Option<Vec<Named>>,
    authors: Option<Vec<Named>>,
}

impl MangaDto {
    fn is_adult(&self) -> bool {
        self.content_rating.as_deref() == Some("18+")
    }

    /// As the Kotlin's `toSManga` with the English title: the Russian title, the rating, the other names and the
    /// summary in the description; kind, age rating and genres as genres.
    fn into_manga(self) -> Manga {
        let rating = self.score.as_ref().and_then(|s| s.value).unwrap_or(0.0);
        let votes = self.score.as_ref().and_then(|s| s.votes).unwrap_or(0);
        let stars = match rating {
            r if r > 9.5 => "★★★★★",
            r if r > 8.5 => "★★★★✬",
            r if r > 7.5 => "★★★★☆",
            r if r > 6.5 => "★★★✬☆",
            r if r > 5.5 => "★★★☆☆",
            r if r > 4.5 => "★★✬☆☆",
            r if r > 3.5 => "★★☆☆☆",
            r if r > 2.5 => "★✬☆☆☆",
            r if r > 1.5 => "★☆☆☆☆",
            r if r > 0.5 => "✬☆☆☆☆",
            _ => "☆☆☆☆☆",
        };
        let other_names = match self.synonyms.as_deref() {
            Some(names) if !names.is_empty() => format!("Альтернативные названия:\n{}\n\n", names.join(" / ")),
            _ => String::new(),
        };
        let description = format!("{}\n{stars} {rating} (голосов: {votes})\n{other_names}{}", self.russian, self.description.as_deref().unwrap_or(""));
        let category = match self.kind.as_deref() {
            Some("manhwa") => "Манхва",
            Some("manhua") => "Маньхуа",
            Some("comics") => "Комикс",
            Some("one_shot") => "Ваншот",
            _ => "Манга",
        };
        let age = match self.content_rating.as_deref() {
            Some("no") | None => String::new(),
            Some(rating) => rating.replace("_plus", "+"),
        };
        let genres = self.genres.iter().flatten().map(|g| g.name.trim().to_owned());
        let genre: Vec<String> = [category.to_owned(), age].into_iter().chain(genres).filter(|g| !g.is_empty()).collect();
        // The translation's state first, then the original's. "copyright" stays unknown, as in the Kotlin.
        let status = match (self.trans_status.as_deref(), self.status.as_deref()) {
            (Some("continued"), _) => status::ONGOING,
            (Some("completed"), _) => status::COMPLETED,
            (_, Some("ongoing")) => status::ONGOING,
            (_, Some("released")) => status::COMPLETED,
            _ => status::UNKNOWN,
        };
        let authors: Vec<String> = self.authors.into_iter().flatten().map(|a| a.name).collect();
        Manga {
            url: format!("/{}", self.id),
            title: self.name,
            author: (!authors.is_empty()).then(|| authors.join(", ")),
            description: Some(description),
            genre: Some(genre.join(", ")),
            status,
            thumbnail_url: self.cover.preview,
            ..Default::default()
        }
    }
}

#[derive(Deserialize)]
struct Pagination {
    current_page: i32,
    last_page: i32,
}

#[derive(Deserialize)]
struct CatalogDto {
    pagination: Pagination,
    mangas: Vec<MangaDto>,
}

fn parse<'a, T: Deserialize<'a>>(json: &'a str) -> Res<T> {
    serde_json::from_str(json).map_err(|e| format!("unexpected answer from Desu: {e}"))
}

/// One catalog page; titles rated 18+ are left out unless `show_adult`.
pub fn parse_catalog(json: &str, show_adult: bool) -> Res<MangasPage> {
    let page: CatalogDto = parse(json)?;
    Ok(MangasPage {
        mangas: page.mangas.into_iter().filter(|m| show_adult || !m.is_adult()).map(MangaDto::into_manga).collect(),
        has_next_page: page.pagination.last_page > page.pagination.current_page,
    })
}

/// The quick search's cards: `manga/<slug>.<id>/`, the English name under the Russian one, a small cover.
pub fn parse_search(html: &str) -> MangasPage {
    let doc = Html::parse_document(html);
    let mangas = doc
        .select(&sel("a.AniMangaSearchCard__link"))
        .filter_map(|card| {
            let id = card.value().attr("href")?.trim_end_matches('/').rsplit('.').next().filter(|id| is_id(id))?;
            let russian = first(card, ".AniMangaSearchCard__title").map(text_of);
            let title = first(card, ".AniMangaSearchCard__subtitle").map(text_of).filter(|t| !t.is_empty()).or(russian)?;
            // The card's 120 px cover; the catalog's is the larger `preview` one.
            let thumbnail_url = attr(card, "img", "src").map(|src| src.replace("/covers/x120/", "/covers/preview/"));
            Some(Manga { url: format!("/{id}"), title, thumbnail_url, ..Default::default() })
        })
        .collect();
    MangasPage { mangas, has_next_page: false }
}

#[derive(Deserialize)]
struct DetailsDto {
    manga: MangaDto,
}

pub fn parse_details(json: &str) -> Res<Manga> {
    Ok(parse::<DetailsDto>(json)?.manga.into_manga())
}

#[derive(Deserialize)]
struct ChapterDto {
    id: i64,
    volume: String,
    number: String,
    title: Option<String>,
    publish_date: i64,
}

#[derive(Deserialize)]
struct ChaptersDto {
    chapters: Vec<ChapterDto>,
}

/// The chapters as the API lists them (newest first), named as the Kotlin names them.
pub fn parse_chapters(json: &str, manga_url: &str) -> Res<Vec<Chapter>> {
    let manga = manga_id(manga_url)?;
    Ok(parse::<ChaptersDto>(json)?
        .chapters
        .into_iter()
        .map(|c| {
            let number = format!("{}. Глава {}", c.volume, c.number);
            Chapter {
                url: format!("/{manga}/chapters/{}", c.id),
                name: match c.title.filter(|t| !t.is_empty()) {
                    Some(title) => format!("{number} {title}"),
                    None => number,
                },
                date_upload: c.publish_date * 1000,
                chapter_number: c.number.parse().unwrap_or(-1.0),
                scanlator: None,
            }
        })
        .collect())
}

#[derive(Deserialize)]
struct PageDto {
    url: String,
}

#[derive(Deserialize)]
struct PagesDto {
    pages: Vec<PageDto>,
}

#[derive(Deserialize)]
struct ChapterPagesDto {
    chapter: PagesDto,
}

pub fn parse_pages(json: &str) -> Res<Vec<String>> {
    Ok(parse::<ChapterPagesDto>(json)?.chapter.pages.into_iter().map(|p| p.url).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_follow_the_stored_addresses() {
        assert_eq!(catalog_url(0), "https://desu.uno/api/manga/catalog/?limit=20&page=1");
        assert_eq!(search_url(" наруто 7 "), "https://desu.uno/manga/search/?q=%D0%BD%D0%B0%D1%80%D1%83%D1%82%D0%BE%207");
        assert_eq!(details_url("/29").unwrap(), "https://desu.uno/api/manga/29/");
        assert_eq!(chapters_url("/29").unwrap(), "https://desu.uno/api/manga/29/chapters");
        assert_eq!(pages_url("/29/chapters/52364").unwrap(), "https://desu.uno/api/manga/29/chapters/52364");
        assert!(pages_url("52364").is_err());
        assert!(details_url("/manga/naruto.29/").is_err());
    }

    #[test]
    fn the_catalog_hides_18_plus_titles_unless_asked() {
        let json = include_str!("../fixtures/catalog.json");
        let all = parse_catalog(json, true).unwrap();
        assert_eq!(all.mangas.len(), 20);
        assert!(all.has_next_page);
        let first = &all.mangas[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/6996", "There are no bad noble ladies in the world"));
        assert_eq!(first.thumbnail_url.as_deref(), Some("https://static.desu.uno/data/manga/covers/preview/6996.jpg?1774094022"));
        let safe = parse_catalog(json, false).unwrap();
        assert_eq!(safe.mangas.len(), 15);
        assert!(safe.mangas.iter().all(|m| !m.genre.as_deref().unwrap_or("").contains("18+")));
    }

    #[test]
    fn search_reads_the_quick_search_cards() {
        let page = parse_search(include_str!("../fixtures/search.html"));
        assert_eq!(page.mangas.len(), 4);
        let naruto = &page.mangas[0];
        assert_eq!((naruto.url.as_str(), naruto.title.as_str()), ("/29", "Naruto"));
        assert_eq!(naruto.thumbnail_url.as_deref(), Some("https://static.desu.uno/data/manga/covers/preview/29.jpg"));
        assert!(!page.has_next_page);
    }

    #[test]
    fn details_carry_the_russian_title_genres_and_status() {
        let manga = parse_details(include_str!("../fixtures/details.json")).unwrap();
        assert_eq!((manga.url.as_str(), manga.title.as_str()), ("/29", "Naruto"));
        assert_eq!(manga.author.as_deref(), Some("Masashi Kishimoto"));
        assert_eq!(manga.status, status::COMPLETED);
        assert!(manga.genre.as_deref().unwrap().starts_with("Манга, 16+, Экшен, Приключения"));
        let description = manga.description.unwrap();
        assert!(description.starts_with("Наруто\n★★★★✬ 9.08 (голосов: 322)\nДвенадцать лет назад"), "{description}");
    }

    #[test]
    fn chapters_keep_the_manga_in_their_address() {
        let chapters = parse_chapters(include_str!("../fixtures/chapters.json"), "/29").unwrap();
        assert_eq!(chapters.len(), 4);
        let last = &chapters[0];
        assert_eq!((last.url.as_str(), last.name.as_str()), ("/29/chapters/52364", "72. Глава 700 Узумаки Наруто"));
        assert_eq!((last.chapter_number, last.date_upload), (700.0, 1_415_294_434_000));
        assert_eq!(chapters[3].chapter_number, 0.0);
    }

    #[test]
    fn pages_are_the_listed_images() {
        let pages = parse_pages(include_str!("../fixtures/pages.json")).unwrap();
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0], "https://img2.desu.uno/manga/rus/naruto/vol72_ch700/naruto_vol72_ch700_p001.jpg?1415294434");
    }
}
