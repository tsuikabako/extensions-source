//! The Madara pages as pure functions: URLs, form bodies and one parser per page. Ported from the keiyoushi
//! `lib-multisrc/madara` theme (Apache-2.0).

use kuro_guest::Res;
use kuro_guest::html::{ElementRef, Html, own_text, sel, text_of};
use kuro_guest::wire::{Chapter, Manga, MangasPage};
use url::Url;

use crate::Site;

/// The theme's page size for the ajax lists: a full page means there may be more.
const PAGE_SIZE: usize = 25;

// ================================ URLs =================================

#[derive(Clone, Copy)]
pub enum Order {
    Views,
    Latest,
    Relevance,
}

pub fn ajax_url(site: &Site) -> String {
    format!("{}/wp-admin/admin-ajax.php", site.base)
}

/// The `madara_load_more` form of the Kotlin `Madara.ajaxList`, without the filters. `page` starts at 1.
pub fn archive_form(page: i32, order: Order, query: &str) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.append_pair("action", "madara_load_more")
        .append_pair("page", &(page.max(1) - 1).to_string())
        .append_pair("template", "madara-core/content/content-archive")
        .append_pair("vars[paged]", "1")
        .append_pair("vars[template]", "archive")
        .append_pair("vars[posts_per_page]", &PAGE_SIZE.to_string())
        .append_pair("vars[post_type]", "wp-manga")
        .append_pair("vars[post_status]", "publish")
        .append_pair("vars[manga_archives_item_layout]", "big_thumbnail")
        // titles only, not novels
        .append_pair("vars[meta_query][0][key]", "_wp_manga_chapter_type")
        .append_pair("vars[meta_query][0][value]", "manga");
    let sort_key = match order {
        Order::Views => Some("_wp_manga_views"),
        Order::Latest => Some("_latest_update"),
        Order::Relevance => None,
    };
    if let Some(key) = sort_key {
        form.append_pair("vars[orderby]", "meta_value_num").append_pair("vars[meta_key]", key).append_pair("vars[order]", "DESC");
    }
    if !query.trim().is_empty() {
        form.append_pair("vars[s]", query.trim());
    }
    form.finish()
}

/// A paged archive (`/manga/page/2?m_orderby=views`). Without a trailing slash: some sites redirect `/manga/` to plain
/// http, while WordPress redirects the bare form to the slashed one on the same scheme.
pub fn archive_url(site: &Site, page: i32, order: Order) -> String {
    let mut url = format!("{}/{}", site.base, site.manga_dir);
    if page > 1 {
        url.push_str(&format!("/page/{page}"));
    }
    match order {
        Order::Views => url.push_str("?m_orderby=views"),
        Order::Latest => url.push_str("?m_orderby=latest"),
        Order::Relevance => {}
    }
    url
}

pub fn search_url(site: &Site, query: &str) -> String {
    let q: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("{}/?s={q}&post_type=wp-manga", site.base)
}

/// A page of the site from a stored address, refused when it is another site's. A bare number is a WordPress post id
/// (what the newer keiyoushi extension stores for a title), which the site redirects to the title page.
pub fn page(site: &Site, path: &str) -> Res<String> {
    let base = Url::parse(site.base).map_err(|e| e.to_string())?;
    if !path.is_empty() && path.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(format!("{}/?p={path}", site.base));
    }
    let url = base.join(path).map_err(|e| format!("invalid address {path}: {e}"))?;
    if url.host_str() != base.host_str() {
        return Err(format!("not a {} address: {path}", site.name));
    }
    Ok(url.to_string())
}

pub fn chapters_ajax_url(title_url: &str) -> String {
    format!("{}/ajax/chapters/", title_url.trim_end_matches('/'))
}

pub fn list_style_url(chapter_url: &str) -> String {
    match Url::parse(chapter_url) {
        Ok(mut url) => {
            url.query_pairs_mut().append_pair("style", "list");
            url.to_string()
        }
        Err(_) => chapter_url.to_owned(),
    }
}

// =============================== Parsers ===============================

/// "Updating" stands in for an unknown author or genre on many sites.
fn is_updating(text: &str) -> bool {
    let text = text.to_lowercase();
    text.contains("updating") || text.contains("atualizando")
}

/// The path of an address on the site (`/manga/one-piece/`).
fn path_of(site: &Site, href: &str) -> String {
    Url::parse(site.base).and_then(|base| base.join(href.trim())).map(|u| u.path().to_owned()).unwrap_or_else(|_| href.trim().to_owned())
}

/// The address of an image element (the Kotlin `imageFromElement`): lazy-load attributes first, then the largest
/// `srcset` candidate, then `src`.
fn image_of(site: &Site, img: ElementRef) -> Option<String> {
    let attr = |name: &str| img.value().attr(name).map(str::trim).filter(|v| !v.is_empty());
    let found = ["data-src", "data-lazy-src", "data-lzl-src", "data-cfsrc", "data-manga-src"]
        .iter()
        .find_map(|name| attr(name).map(str::to_owned))
        .or_else(|| attr("srcset").and_then(largest_in_srcset))
        .or_else(|| attr("src").map(str::to_owned))?;
    Url::parse(site.base).and_then(|base| base.join(&found)).map(|u| u.to_string()).ok()
}

/// The candidate of a `srcset` with the largest width or density.
fn largest_in_srcset(srcset: &str) -> Option<String> {
    srcset
        .split(',')
        .filter_map(|candidate| {
            let mut parts = candidate.split_whitespace();
            let url = parts.next()?;
            let size = parts.next().and_then(|d| d.trim_end_matches(['w', 'x']).parse::<f32>().ok()).unwrap_or(0.0);
            Some((url.to_owned(), size))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(url, _)| url)
}

/// The title cards of an archive, an ajax list or a search page (the Kotlin `parseArchive`).
pub fn parse_archive(site: &Site, html: &str) -> Vec<Manga> {
    let doc = Html::parse_document(html);
    let mut mangas: Vec<Manga> = Vec::new();
    for card in doc.select(&sel("div.page-item-detail, .manga__item, .c-tabs-item__content")) {
        let Some(link) = card.select(&sel(".post-title a")).next() else { continue };
        let (Some(href), title) = (link.value().attr("href"), text_of(link)) else { continue };
        let url = path_of(site, href);
        if title.is_empty() || mangas.iter().any(|m| m.url == url) {
            continue;
        }
        let thumbnail_url = card.select(&sel("img")).next().and_then(|img| image_of(site, img));
        mangas.push(Manga { url, title, author: None, artist: None, description: None, genre: None, status: 0, thumbnail_url });
    }
    mangas
}

pub fn ajax_page(site: &Site, html: &str) -> MangasPage {
    let mangas = parse_archive(site, html);
    MangasPage { has_next_page: mangas.len() == PAGE_SIZE, mangas }
}

pub fn archive_page(site: &Site, html: &str) -> MangasPage {
    let has_next_page = Html::parse_document(html).select(&sel("div.nav-previous, a.nextpostslink, #navigation-ajax")).next().is_some();
    MangasPage { mangas: parse_archive(site, html), has_next_page }
}

/// The value of a row of the details table whose heading contains `label` (`Status`, `Type`, `Alternative`).
fn row(doc: &Html, label: &str) -> Option<String> {
    doc.select(&sel(".post-content_item"))
        .find(|item| item.select(&sel(".summary-heading")).next().is_some_and(|h| text_of(h).contains(label)))
        .and_then(|item| item.select(&sel(".summary-content")).next())
        .map(text_of)
        .filter(|v| !v.is_empty() && !is_updating(v))
}

fn joined(doc: &Html, css: &str) -> Option<String> {
    let names: Vec<String> = doc.select(&sel(css)).map(text_of).filter(|t| !t.is_empty() && !is_updating(t)).collect();
    Some(names.join(", ")).filter(|n| !n.is_empty())
}

pub fn parse_details(site: &Site, html: &str) -> Res<Manga> {
    let doc = Html::parse_document(html);
    let heading = doc.select(&sel("div.post-title h3, div.post-title h1, #manga-title > h1")).next().ok_or("the title page has no title")?;
    let title = Some(own_text(heading)).filter(|t| !t.is_empty()).unwrap_or_else(|| text_of(heading));
    let summary = doc
        .select(&sel("div.description-summary div.summary__content, div.summary_content div.post-content_item > h5 + div, div.summary_content div.manga-excerpt"))
        .next()
        .map(|element| {
            let paragraphs: Vec<String> = element.select(&sel("p")).map(text_of).filter(|p| !p.is_empty()).collect();
            if paragraphs.is_empty() { text_of(element) } else { paragraphs.join("\n\n") }
        })
        .filter(|d| !d.is_empty());
    let alternative = row(&doc, "Alt").map(|names| format!("Alternative names: {names}"));
    let description = [summary, alternative].into_iter().flatten().collect::<Vec<_>>().join("\n\n");
    let status = row(&doc, "Status").or_else(|| doc.select(&sel("div.summary-content")).last().map(text_of)).map_or(0, |s| status_of(&s));
    let mut genres: Vec<String> = doc.select(&sel("div.genres-content a, div.tags-content a")).map(text_of).collect();
    genres.extend(row(&doc, "Type"));
    let mut seen = Vec::new();
    genres.retain(|g| !g.is_empty() && !is_updating(g) && !seen.contains(&g.to_lowercase()) && {
        seen.push(g.to_lowercase());
        true
    });
    Ok(Manga {
        url: String::new(),
        title,
        author: joined(&doc, "div.author-content > a, div.manga-authors > a"),
        artist: joined(&doc, "div.artist-content > a"),
        description: Some(description).filter(|d| !d.is_empty()),
        genre: Some(genres.join(", ")).filter(|g| !g.is_empty()),
        status,
        thumbnail_url: doc.select(&sel("div.summary_image img")).next().and_then(|img| image_of(site, img)),
    })
}

/// The status of the details table, in the words the Kotlin knows (English, Spanish and Portuguese here).
fn status_of(text: &str) -> i32 {
    let text = text.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| text.contains(w));
    if any(&["completed", "completo", "completado", "concluído", "concluido", "finalizado", "terminé", "complété"]) {
        2
    } else if any(&["ongoing", "on going", "updating", "em lançamento", "em andamento", "en curso", "emision", "en emisión", "publicándose", "publicandose", "lançando"]) {
        1
    } else if any(&["on hold", "hiatus", "pausado", "en espera"]) {
        6
    } else if any(&["canceled", "cancelled", "cancelado"]) {
        5
    } else {
        0
    }
}

/// The post id of a title page, for the `manga_get_chapters` request (the Kotlin `Document.mangaId`).
pub fn post_id(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let attr = |css: &str, name: &str| doc.select(&sel(css)).next().and_then(|e| e.value().attr(name)).map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    attr("[id^=manga-chapters-holder]", "data-id").or_else(|| attr("input.rating-post-id", "value")).or_else(|| attr("a[data-post]", "data-post")).or_else(|| {
        let link = attr("link[rel=shortlink]", "href")?;
        Url::parse(&link).ok()?.query_pairs().find(|(k, _)| k == "p").map(|(_, v)| v.into_owned())
    })
}

/// The chapters of a title page or a chapter list reply, newest first as the site lists them.
pub fn parse_chapters(site: &Site, html: &str, now: u64) -> Vec<Chapter> {
    let doc = Html::parse_document(html);
    doc.select(&sel(".wp-manga-chapter"))
        .filter_map(|item| {
            let link = item.select(&sel("a")).next()?;
            let href = link.value().attr("href").filter(|h| !h.trim().is_empty() && !h.starts_with('#'))?;
            let attr = |css: &str, name: &str| item.select(&sel(css)).next().and_then(|e| e.value().attr(name)).map(str::to_owned).filter(|v| !v.trim().is_empty());
            // the Kotlin takes the first of these that is present; some sites put a non-date `alt` on an icon, so the
            // first that reads as a date wins here
            let release = item.select(&sel("span.chapter-release-date")).next().map(text_of);
            let date_upload = [attr("img:not(.thumb)", "alt"), attr("span a", "title"), release].into_iter().flatten().map(|d| parse_date(&d, now)).find(|&ms| ms != 0).unwrap_or(0);
            Some(Chapter { url: path_of(site, href), name: text_of(link), date_upload, ..Default::default() })
        })
        .collect()
}

pub fn is_single_pager(html: &str) -> bool {
    Html::parse_document(html).select(&sel("#single-pager")).next().is_some()
}

/// The page images of a chapter, in order.
pub fn parse_pages(site: &Site, html: &str) -> Res<Vec<String>> {
    let doc = Html::parse_document(html);
    if doc.select(&sel("#chapter-protector-data")).next().is_some() {
        return Err("this chapter's page list is encrypted, which the plug-in cannot read yet".to_owned());
    }
    Ok(doc
        .select(&sel("div.page-break, li.blocks-gallery-item"))
        .filter_map(|block| block.select(&sel("img")).next())
        .filter_map(|img| image_of(site, img))
        .collect())
}

// ================================ Dates ================================

const DAY_MS: i64 = 86_400_000;

/// A chapter date as epoch milliseconds, 0 when unknown: "today", "yesterday", "2 days ago" (against `now`, Unix
/// seconds), "January 5, 2024" or "01/05/2024" (month first, as the sites that use it do).
pub fn parse_date(text: &str, now: u64) -> i64 {
    let lower = text.trim().to_lowercase();
    let now_ms = now as i64 * 1000;
    let today = now_ms - now_ms.rem_euclid(DAY_MS);
    if ["today", "hoy", "hoje"].iter().any(|w| lower.starts_with(w)) {
        return today;
    }
    if ["yesterday", "ayer", "ontem"].iter().any(|w| lower.starts_with(w)) {
        return today - DAY_MS;
    }
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    if let Some(amount) = words.iter().find_map(|w| w.parse::<i64>().ok()) {
        let unit_ms = words.iter().find_map(|w| relative_unit(w));
        if let Some(unit_ms) = unit_ms {
            return now_ms - amount * unit_ms;
        }
    }
    calendar_date(&words).map_or(0, |(y, m, d)| days_from_civil(y, m, d) * DAY_MS)
}

/// The length of a relative-date unit word ("days", "horas", "min").
fn relative_unit(word: &str) -> Option<i64> {
    let units: [(&[&str], i64); 7] = [
        (&["year", "año", "ano"], 365 * DAY_MS),
        (&["month", "mes"], 30 * DAY_MS),
        (&["week", "semana"], 7 * DAY_MS),
        (&["day", "día", "dia"], DAY_MS),
        (&["hour", "hora"], 3_600_000),
        (&["min"], 60_000),
        (&["sec", "segundo"], 1_000),
    ];
    units.iter().find(|(prefixes, _)| prefixes.iter().any(|p| word.starts_with(p))).map(|(_, ms)| *ms)
}

/// The prefixes of the month names, in English, Spanish, Portuguese, Italian and French. `juil` comes before `jun` and
/// `juin` so July is not read as June.
const MONTHS: [(&str, i64); 32] = [
    ("jan", 1), ("ene", 1), ("gen", 1), ("feb", 2), ("fev", 2), ("fév", 2), ("mar", 3), ("apr", 4), ("abr", 4), ("avr", 4),
    ("may", 5), ("mai", 5), ("mag", 5), ("juil", 7), ("jun", 6), ("juin", 6), ("giu", 6), ("jul", 7), ("lug", 7),
    ("aug", 8), ("ago", 8), ("aoû", 8), ("sep", 9), ("set", 9), ("oct", 10), ("out", 10), ("ott", 10), ("nov", 11),
    ("dec", 12), ("dic", 12), ("dez", 12), ("déc", 12),
];

/// (year, month, day) of "january 5 2024", "7 agosto 2023" or "01 05 2024" (the words of the date, split on punctuation).
fn calendar_date(words: &[&str]) -> Option<(i64, i64, i64)> {
    let numbers: Vec<i64> = words.iter().filter_map(|w| w.parse().ok()).collect();
    let named = words.iter().find_map(|w| MONTHS.iter().find(|(prefix, _)| w.starts_with(prefix)).map(|(_, month)| *month));
    let (y, m, d) = match (named, numbers.as_slice()) {
        (Some(month), [day, year]) => (*year, month, *day),
        (None, [month, day, year]) => (*year, *month, *day),
        _ => return None,
    };
    ((1..=12).contains(&m) && (1..=31).contains(&d) && y >= 1970).then_some((y, m, d))
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Browse, Chapters};

    /// TopManhua.fan: the paged layout, chapters on the title page, dates as MM/dd/yyyy.
    const PAGES: Site = Site {
        id: 0,
        name: "TopManhua.fan",
        lang: "en",
        base: "https://www.topmanhua.fan",
        manga_dir: "manhua",
        browse: Browse::Pages,
        chapters: Chapters::MangaPage,
        request_gap_ms: 0,
    };
    /// MangaSushi: the stock ajax layout, chapters from `ajax/chapters/`, dates as "October 13, 2026".
    const AJAX: Site = Site { base: "https://mangasushi.org", manga_dir: "manga", browse: Browse::Ajax, chapters: Chapters::MangaAjax, ..PAGES };

    /// 2026-10-25 12:00 UTC.
    const NOW: u64 = 1_792_929_600;

    #[test]
    fn the_ajax_form_and_the_page_urls_follow_the_kotlin() {
        let form = archive_form(1, Order::Latest, "");
        assert!(form.starts_with("action=madara_load_more&page=0&template=madara-core%2Fcontent%2Fcontent-archive&"), "{form}");
        assert!(form.ends_with("vars%5Borderby%5D=meta_value_num&vars%5Bmeta_key%5D=_latest_update&vars%5Border%5D=DESC"), "{form}");
        let search = archive_form(3, Order::Relevance, " one piece ");
        assert!(search.contains("page=2&") && search.ends_with("vars%5Bs%5D=one+piece") && !search.contains("orderby"), "{search}");
        assert_eq!(archive_url(&PAGES, 1, Order::Views), "https://www.topmanhua.fan/manhua?m_orderby=views");
        assert_eq!(archive_url(&PAGES, 2, Order::Latest), "https://www.topmanhua.fan/manhua/page/2?m_orderby=latest");
        assert_eq!(search_url(&PAGES, "solo leveling"), "https://www.topmanhua.fan/?s=solo+leveling&post_type=wp-manga");
        assert_eq!(chapters_ajax_url("https://mangasushi.org/manga/x/"), "https://mangasushi.org/manga/x/ajax/chapters/");
        assert_eq!(list_style_url("https://mangasushi.org/manga/x/chapter-1/"), "https://mangasushi.org/manga/x/chapter-1/?style=list");
    }

    #[test]
    fn only_addresses_of_the_site_are_fetched() {
        assert_eq!(page(&PAGES, "/manhua/solo-leveling").unwrap(), "https://www.topmanhua.fan/manhua/solo-leveling");
        assert_eq!(page(&AJAX, "4206").unwrap(), "https://mangasushi.org/?p=4206");
        assert!(page(&PAGES, "https://evil.test/manhua/x").is_err());
    }

    #[test]
    fn the_paged_archive_lists_cards_and_a_next_link() {
        let page = archive_page(&PAGES, include_str!("../fixtures/pages/popular.html"));
        assert_eq!(page.mangas.len(), 12);
        assert!(page.has_next_page);
        let first = &page.mangas[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/manhua/tales-of-demons-and-gods", "Tales of Demons and Gods"));
        assert!(first.thumbnail_url.as_deref().unwrap().ends_with("/thumb/tales-of-demons-and-gods.webp"), "{first:?}");
        assert!(!archive_page(&PAGES, "<div class='page-item-detail'></div>").has_next_page);
    }

    #[test]
    fn the_search_page_lists_its_cards() {
        let found = parse_archive(&PAGES, include_str!("../fixtures/pages/search.html"));
        assert_eq!(found.len(), 18);
        assert_eq!(found[0].url, "/manhua/global-class-transition-the-magic-king-s-skills-have-infinite-affixes");
        assert_eq!(found[0].title, "Global Class Transition: The Magic King's Skills Have Infinite Affixes");
    }

    #[test]
    fn the_ajax_list_is_full_and_has_srcset_covers() {
        let page = ajax_page(&AJAX, include_str!("../fixtures/ajax/popular.html"));
        assert_eq!(page.mangas.len(), 25);
        assert!(page.has_next_page);
        assert!(page.mangas.iter().all(|m| m.url.starts_with("/manga/") && m.url.ends_with('/') && !m.title.is_empty()));
        assert!(page.mangas.iter().all(|m| m.thumbnail_url.as_deref().is_some_and(|t| t.starts_with("https://"))));
        let search = ajax_page(&AJAX, include_str!("../fixtures/ajax/search.html"));
        assert!(!search.mangas.is_empty() && !search.has_next_page);
    }

    #[test]
    fn a_title_page_has_its_details() {
        let manga = parse_details(&PAGES, include_str!("../fixtures/pages/manga.html")).unwrap();
        assert_eq!(manga.title, "Solo Leveling");
        assert_eq!(manga.status, 2);
        assert!(manga.genre.as_deref().unwrap().starts_with("Action, Adventure, Demons"), "{:?}", manga.genre);
        assert!(manga.description.as_deref().unwrap().contains("Sung Jinwoo"));
        assert_eq!(manga.thumbnail_url.as_deref(), Some("https://img-r1.2xstorage.com/thumb/solo-leveling.webp"));

        let ajax = parse_details(&AJAX, include_str!("../fixtures/ajax/manga.html")).unwrap();
        assert!(!ajax.title.is_empty() && ajax.thumbnail_url.is_some() && ajax.description.is_some(), "{ajax:?}");
    }

    #[test]
    fn the_chapters_have_paths_names_and_dates() {
        let chapters = parse_chapters(&PAGES, include_str!("../fixtures/pages/manga.html"), NOW);
        assert_eq!(chapters.len(), 258);
        assert_eq!((chapters[0].url.as_str(), chapters[0].name.as_str()), ("/manhua/solo-leveling/chapter-200", "Chapter 200"));
        assert_eq!(chapters[0].date_upload, days_from_civil(2023, 5, 31) * DAY_MS);

        let ajax = parse_chapters(&AJAX, include_str!("../fixtures/ajax/chapters.html"), NOW);
        assert!(ajax.len() > 300);
        assert_eq!(ajax[0].url, "/manga/lonely-attack-on-the-different-world/chapter-335/");
        assert_eq!(ajax[0].date_upload, days_from_civil(2026, 10, 13) * DAY_MS);
    }

    #[test]
    fn the_chapter_pages_are_its_images_in_order() {
        let pages = parse_pages(&PAGES, include_str!("../fixtures/pages/chapter.html")).unwrap();
        assert_eq!(pages.len(), 17);
        assert!(pages[0].starts_with("https://cdn.") && pages[0].ends_with("/chapter-200/001.png"), "{}", pages[0]);
        let ajax = parse_pages(&AJAX, include_str!("../fixtures/ajax/chapter.html")).unwrap();
        assert!(!ajax.is_empty() && ajax[0].ends_with("/00.jpg") && !ajax[0].contains(char::is_whitespace), "{ajax:?}");
        assert!(parse_pages(&PAGES, "<div id='chapter-protector-data'></div>").is_err());
    }

    #[test]
    fn dates_read_relative_and_calendar_forms() {
        let today = (NOW as i64 * 1000) / DAY_MS * DAY_MS;
        assert_eq!(parse_date("Today", NOW), today);
        assert_eq!(parse_date("yesterday", NOW), today - DAY_MS);
        assert_eq!(parse_date("2 days ago", NOW), NOW as i64 * 1000 - 2 * DAY_MS);
        assert_eq!(parse_date("hace 3 horas", NOW), NOW as i64 * 1000 - 3 * 3_600_000);
        assert_eq!(parse_date("5 mins ago", NOW), NOW as i64 * 1000 - 5 * 60_000);
        assert_eq!(parse_date("January 1, 1970", NOW), 0);
        assert_eq!(parse_date("March 1, 2024", NOW), days_from_civil(2024, 3, 1) * DAY_MS);
        assert_eq!(parse_date("7 Agosto 2023", NOW), days_from_civil(2023, 8, 7) * DAY_MS);
        assert_eq!(parse_date("12 juillet 2025", NOW), days_from_civil(2025, 7, 12) * DAY_MS);
        assert_eq!(parse_date("3 juin 2025", NOW), days_from_civil(2025, 6, 3) * DAY_MS);
        assert_eq!(days_from_civil(2024, 3, 1), 19_783);
        assert_eq!(parse_date("not a date", NOW), 0);
    }

    #[test]
    fn statuses_and_updating_placeholders() {
        assert_eq!((status_of("OnGoing"), status_of("Completed"), status_of("On Hold"), status_of("Cancelado")), (1, 2, 6, 5));
        let doc = r#"<div class="post-title"><h1>Name <span class="hot">HOT</span></h1></div>
            <div class="author-content"><a>Updating</a></div>"#;
        let manga = parse_details(&PAGES, doc).unwrap();
        assert_eq!((manga.title.as_str(), manga.author), ("Name", None));
    }
}
