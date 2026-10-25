//! The MangaThemesia pages as pure functions: URLs and one parser per page. Ported from the keiyoushi
//! `lib-multisrc/mangathemesia` theme (Apache-2.0).

use std::collections::HashMap;

use base64::Engine;
use chrono::NaiveDate;
use kuro_guest::Res;
use kuro_guest::html::{ElementRef, Html, first, own_text, sel, text_of};
use kuro_guest::wire::{Chapter, Manga, MangasPage, status};
use url::Url;

use crate::Site;

// ================================ URLs =================================

/// The search page (`/manga/?title=...&page=N&order=...`), which is also the popular and latest list.
pub fn list_url(site: &Site, page: i32, order: &str, query: &str) -> String {
    let mut url = format!("{}{}/?", site.base, site.manga_dir);
    if !query.trim().is_empty() {
        let q: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
        url.push_str(&format!("title={q}&"));
    }
    url.push_str(&format!("page={}", page.max(1)));
    if !order.is_empty() {
        url.push_str(&format!("&order={order}"));
    }
    url
}

/// A page of the site from a stored address, refused when it is another site's.
pub fn page(site: &Site, path: &str) -> Res<String> {
    let base = Url::parse(site.base).map_err(|e| e.to_string())?;
    let url = base.join(path).map_err(|e| format!("invalid address {path}: {e}"))?;
    if url.host_str() != base.host_str() {
        return Err(format!("not a {} address: {path}", site.name));
    }
    Ok(url.to_string())
}

/// The last segment of a title address without the site's number prefix (`/manga/2072267132-a-title/` → `a-title`).
pub fn permanent_slug(url: &str) -> &str {
    let slug = url.split('#').next().unwrap_or(url).trim_end_matches('/').rsplit('/').next().unwrap_or("");
    match slug.split_once('-') {
        Some((number, rest)) if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) => rest,
        _ => slug,
    }
}

// =============================== Parsers ===============================

/// The path of an address on the site (`/manga/a-title/`).
fn path_of(site: &Site, href: &str) -> String {
    Url::parse(site.base).and_then(|base| base.join(href.trim())).map(|u| u.path().to_owned()).unwrap_or_else(|_| href.trim().to_owned())
}

/// The address of an image (the Kotlin `imgAttr`): the lazy-load attributes first, then `src`; a placeholder `data:`
/// address is no image.
fn image_of(site: &Site, img: ElementRef) -> Option<String> {
    let found = ["data-lazy-src", "data-src", "data-cfsrc", "src"].iter().find_map(|name| img.value().attr(name).map(str::trim).filter(|v| !v.is_empty()))?;
    if found.starts_with("data:") {
        return None;
    }
    Url::parse(site.base).and_then(|base| base.join(found)).map(|u| u.to_string()).ok()
}

/// The cards of a list or search page.
pub fn parse_list(site: &Site, html: &str) -> MangasPage {
    let doc = Html::parse_document(html);
    let mangas = doc
        .select(&sel(site.list_card))
        .filter_map(|card| {
            let link = first(card, "a")?;
            let href = link.value().attr("href")?;
            let named = Some(site.list_title).filter(|s| !s.is_empty()).and_then(|s| first(card, s)).map(text_of);
            let title = named.filter(|t| !t.is_empty()).or_else(|| link.value().attr("title").map(|t| t.trim().to_owned())).filter(|t| !t.is_empty())?;
            let thumbnail_url = first(card, "img").and_then(|img| image_of(site, img));
            Some(Manga { url: path_of(site, href), title, thumbnail_url, ..Default::default() })
        })
        .collect();
    MangasPage { mangas, has_next_page: doc.select(&sel(site.next_page)).next().is_some() }
}

/// The text list (`list-mode`) as permanent slug → current slug, for sites with random slugs.
pub fn parse_url_map(html: &str) -> HashMap<String, String> {
    Html::parse_document(html)
        .select(&sel("div#content div.soralist ul li a.series"))
        .filter_map(|link| {
            let slug = link.value().attr("href")?.trim_end_matches('/').rsplit('/').next()?.to_owned();
            Some((permanent_slug(&slug).to_owned(), slug))
        })
        .collect()
}

// The labels of the info rows, in the theme's languages (the Kotlin's `:contains(...)` lists).
const ARTIST: &[&str] = &["artist", "Artiste", "Artista", "الرسام", "الناشر", "İllüstratör", "Çizer", "Sanatçı", "นักวาด"];
const AUTHOR: &[&str] = &["Author", "Auteur", "autor", "المؤلف", "Mangaka", "seniman", "Pengarang", "Yazar", "ผู้แต่ง", "นักเขียน"];
const TYPE: &[&str] = &["type", "ประเภท", "النوع", "tipe", "Türü"];
const STATUS: &[&str] = &["status", "Statut", "Durum", "連載状況", "Estado", "الحالة", "حالة العمل", "สถานะ", "stato", "Statüsü"];
const ALTERNATIVE: &[&str] = &["Alternative", "Alternatif", "الأسماء الثانوية"];

/// The value of an info row whose label holds one of `labels` (case does not matter): a table row's last cell, a
/// `.tsinfo .imptdt` item's `<i>` or link, or the `<span>` after a `.fmed` heading. Placeholders ("-", "N/A") are none.
fn labelled(root: ElementRef, labels: &[&str]) -> Option<String> {
    let has = |element: ElementRef| {
        let text = text_of(element).to_lowercase();
        labels.iter().any(|label| text.contains(&label.to_lowercase()))
    };
    let value = |element: ElementRef| Some(own_text(element)).filter(|t| !t.is_empty()).unwrap_or_else(|| text_of(element));
    root.select(&sel(".infotable tr"))
        .find(|row| has(*row))
        .and_then(|row| row.select(&sel("td")).last())
        .or_else(|| root.select(&sel(".tsinfo .imptdt")).find(|item| has(*item)).and_then(|item| first(item, "i, a")))
        .or_else(|| root.select(&sel(".fmed")).find(|item| first(*item, "b").is_some_and(has)).and_then(|item| first(item, "span")))
        .map(value)
        .filter(|v| !matches!(v.as_str(), "" | "-" | "N/A" | "n/a" | "Unknown"))
}

/// Upper-cases the first letter of a lower-cased genre, as the Kotlin does.
fn capitalized(genre: &str) -> String {
    let lower = genre.trim().to_lowercase();
    let mut chars = lower.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

pub fn parse_details(site: &Site, html: &str) -> Res<Manga> {
    let doc = Html::parse_document(html);
    let root = doc.select(&sel(site.details)).next().ok_or("the title page has no details")?;
    let title = first(root, site.title).map(text_of).filter(|t| !t.is_empty()).ok_or("the title page has no title")?;
    let summary = first(root, site.description).map(text_of).filter(|d| !d.is_empty());
    let alternative = first(root, ".alternative, .alter, .seriestualt").map(own_text).filter(|a| !a.is_empty()).or_else(|| labelled(root, ALTERNATIVE)).map(|names| {
        let names: Vec<String> = names.split(['|', '/', '•', ',', ';']).map(str::trim).filter(|n| !n.is_empty()).map(|n| format!("- {n}")).collect();
        format!("Alternative Names:\n{}", names.join("\n"))
    });
    let description = [summary, alternative].into_iter().flatten().collect::<Vec<_>>().join("\n\n");
    let mut genres: Vec<String> = root.select(&sel(site.genre)).map(text_of).filter(|g| !g.is_empty()).collect();
    genres.extend(labelled(root, TYPE).or_else(|| first(root, "a[href*='type=']").map(own_text)).filter(|t| !t.is_empty()));
    let status = match site.status {
        "" => labelled(root, STATUS),
        css => first(root, css).map(text_of),
    };
    Ok(Manga {
        url: String::new(),
        title,
        artist: labelled(root, ARTIST),
        author: labelled(root, AUTHOR),
        description: Some(description).filter(|d| !d.is_empty()),
        genre: Some(genres.iter().map(|g| capitalized(g)).collect::<Vec<_>>().join(", ")).filter(|g| !g.is_empty()),
        status: status.map_or(status::UNKNOWN, |s| status_of(&s)),
        thumbnail_url: first(root, site.thumbnail).and_then(|img| image_of(site, img)),
    })
}

/// A title's status from the theme's words (the Kotlin `parseStatus`, ongoing checked first).
fn status_of(text: &str) -> i32 {
    let text = text.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| text.contains(w));
    if any(&[
        "مستمرة", "en curso", "ongoing", "on going", "new season", "mass released", "ativo", "en cours", "đang tiến hành", "em lançamento", "онгоінг", "publishing",
        "devam ediyor", "em andamento", "in corso", "güncel", "berjalan", "продолжается", "updating", "lançando", "in arrivo", "emision", "en emision", "مستمر",
        "curso", "en marcha", "publicandose", "publicando", "连载中", "devam etmekte", "連載中",
    ]) {
        status::ONGOING
    } else if any(&[
        "completed", "completo", "complété", "fini", "achevé", "terminé", "tamamlandı", "đã hoàn thành", "hoàn thành", "مكتملة", "завершено", "finished",
        "finalizad", "completata", "one-shot", "bitti", "tamat", "completado", "concluído", "完結", "concluido", "已完结", "bitmiş",
    ]) {
        status::COMPLETED
    } else if any(&["canceled", "cancelled", "cancelado", "cancellato", "cancelados", "dropped", "discontinued", "abandonné"]) {
        status::CANCELLED
    } else if any(&["hiatus", "on hold", "season end", "pausado", "en espera", "en pause", "en attente", "hiato"]) {
        status::ON_HIATUS
    } else {
        status::UNKNOWN
    }
}

/// The chapters of a title page, newest first as the site lists them. Items whose link is a script template
/// (`#/chapter-{{number}}`) are left out.
pub fn parse_chapters(site: &Site, html: &str) -> Vec<Chapter> {
    let doc = Html::parse_document(html);
    let mut chapters: Vec<Chapter> = doc
        .select(&sel(site.chapter_item))
        .filter_map(|item| {
            let link = first(item, "a")?;
            let href = link.value().attr("href").map(str::trim).filter(|h| !h.is_empty() && !h.starts_with('#') && !h.contains("{{"))?;
            let name = first(item, site.chapter_name).map(text_of).filter(|n| !n.is_empty()).unwrap_or_else(|| text_of(link));
            let date_upload = first(item, site.chapter_date).map_or(0, |d| parse_date(&text_of(d)));
            Some(Chapter { url: path_of(site, href), name, date_upload, ..Default::default() })
        })
        .collect();
    // a site without chapter dates gets at least the newest one's, from the title's "Updated on"
    if let Some(newest) = chapters.first_mut().filter(|c| c.date_upload == 0) {
        let updated = doc.select(&sel("time[itemprop=dateModified]")).next().and_then(|t| t.value().attr("datetime"));
        newest.date_upload = updated.map_or(0, |d| parse_date(d.get(..10).unwrap_or(d)));
    }
    chapters
}

/// The page images of a chapter: the reader's `<img>`s, else those in its `<noscript>` (which the HTML parser reads as
/// text), else the `images` list of the reader's script (`ts_reader.run({...})`, plain or base64).
pub fn parse_pages(site: &Site, html: &str) -> Res<Vec<String>> {
    let doc = Html::parse_document(html);
    if doc.select(&sel("#chapter-pages-js-before")).next().is_some() {
        return Err("this chapter's images come in halves to be joined side by side, which the plug-in cannot do yet".to_owned());
    }
    let images: Vec<String> = doc.select(&sel(site.pages)).filter_map(|img| image_of(site, img)).collect();
    if !images.is_empty() {
        return Ok(images);
    }
    let images: Vec<String> = doc
        .select(&sel("#readerarea noscript"))
        .flat_map(|noscript| {
            let fragment = Html::parse_fragment(&noscript.text().collect::<String>());
            fragment.select(&sel("img")).filter_map(|img| image_of(site, img)).collect::<Vec<_>>()
        })
        .collect();
    if !images.is_empty() {
        return Ok(images);
    }
    // "ts_reader.run({" in base64
    let script = doc
        .select(&sel(r#"script[src^="data:text/javascript;base64,dHNfcmVhZGVyLnJ1bih7"]"#))
        .next()
        .and_then(|s| s.value().attr("src")?.split_once("base64,").map(|(_, data)| data.to_owned()))
        .and_then(|data| base64::engine::general_purpose::STANDARD.decode(data).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok());
    let text = script.as_deref().unwrap_or(html);
    let list = regex_lite::Regex::new(r#"["']?(?:images|imageUrls)["']?\s*[:=]\s*(\[.*?\])"#)
        .ok()
        .and_then(|re| re.captures(text)?.get(1).and_then(|m| serde_json::from_str::<Vec<String>>(m.as_str()).ok()))
        .unwrap_or_default();
    let images: Vec<String> = list.iter().filter_map(|url| page(site, url).ok().or_else(|| Some(url.clone()).filter(|u| u.starts_with("http")))).collect();
    if images.is_empty() { Err("the chapter page lists no images".to_owned()) } else { Ok(images) }
}

// ================================ Dates ================================

/// The month names, English by prefix and Arabic (the Java `ar` locale) in full.
const MONTHS: [&[&str]; 12] = [
    &["jan", "يناير"],
    &["feb", "فبراير"],
    &["mar", "مارس"],
    &["apr", "أبريل", "إبريل"],
    &["may", "مايو"],
    &["jun", "يونيو"],
    &["jul", "يوليو"],
    &["aug", "أغسطس"],
    &["sep", "سبتمبر"],
    &["oct", "أكتوبر"],
    &["nov", "نوفمبر"],
    &["dec", "ديسمبر"],
];

/// A chapter date as epoch milliseconds, 0 when unknown: a named month with day and year ("October 22, 2026",
/// "أكتوبر 22, 2026"), or numbers year first ("2026/10/19", "2026-10-22") or month first ("10/22/2026").
// ponytail: English and Arabic month names and month-first numbers only; a site in another language or with
// day-first numbers needs its months here or a date-order setting on `Site`.
pub fn parse_date(text: &str) -> i64 {
    let lower = text.trim().to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let numbers: Vec<u32> = words.iter().filter_map(|w| w.parse().ok()).collect();
    let named = words.iter().find_map(|w| MONTHS.iter().position(|names| names.iter().any(|n| w.starts_with(n)))).map(|i| i as u32 + 1);
    let (y, m, d) = match (named, numbers.as_slice()) {
        (Some(m), &[y, d]) if y > 31 => (y, m, d),
        (Some(m), &[d, y]) => (y, m, d),
        (None, &[y, m, d]) if y > 31 => (y, m, d),
        (None, &[m, d, y]) => (y, m, d),
        _ => return 0,
    };
    NaiveDate::from_ymd_opt(y as i32, m, d).and_then(|date| date.and_hms_opt(0, 0, 0)).map_or(0, |t| t.and_utc().timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HIJALA: Site = Site { id: 0, name: "Hijala", lang: "ar", base: "https://hijala.com", ..Site::THEME };
    const LAVA: Site = Site {
        base: "https://lavascans.com",
        list_card: ".listupd .manga-card-v",
        list_title: ".bigor .tt, h3 a",
        details: "div.lh-container",
        title: ".lh-title",
        description: "#manga-story",
        genre: ".lh-genres a",
        status: ".status-badge-lux",
        thumbnail: ".lh-poster img",
        chapter_item: "#chapters-list-container .ch-item:not(.locked)",
        chapter_name: ".ch-num",
        chapter_date: ".ch-date",
        ..HIJALA
    };

    fn ms(y: i32, m: u32, d: u32) -> i64 {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis()
    }

    #[test]
    fn list_urls_and_addresses() {
        assert_eq!(list_url(&HIJALA, 0, "popular", ""), "https://hijala.com/manga/?page=1&order=popular");
        assert_eq!(list_url(&HIJALA, 3, "", " عمل "), "https://hijala.com/manga/?title=%D8%B9%D9%85%D9%84&page=3");
        assert_eq!(page(&HIJALA, "/lookism-630/").unwrap(), "https://hijala.com/lookism-630/");
        assert!(page(&HIJALA, "https://evil.test/x/").is_err());
        assert_eq!(permanent_slug("/manga/2072267132-the-eternal-supreme/"), "the-eternal-supreme");
        assert_eq!(permanent_slug("/manga/what-can-i-do-alone/"), "what-can-i-do-alone");
    }

    #[test]
    fn the_theme_list_has_cards_with_lazy_covers_and_a_next_link() {
        let page = parse_list(&HIJALA, include_str!("../fixtures/hijala/popular.html"));
        assert!(page.has_next_page);
        assert!(page.mangas.len() >= 5, "{}", page.mangas.len());
        let first = &page.mangas[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/lookism/", "Lookism"));
        let cover = first.thumbnail_url.as_deref().unwrap();
        assert!(cover.starts_with("https://hijala.com/wp-content/uploads/2024/08/") && cover.ends_with("-225x300.gif"), "{cover}");
        let search = parse_list(&HIJALA, include_str!("../fixtures/hijala/search.html"));
        assert!(search.mangas.iter().any(|m| m.title == "Lookism"), "{:?}", search.mangas);
    }

    #[test]
    fn a_site_with_its_own_cards_reads_their_titles() {
        let page = parse_list(&LAVA, include_str!("../fixtures/lavascans/popular.html"));
        assert_eq!(page.mangas.len(), 32);
        assert!(page.has_next_page);
        assert_eq!(page.mangas[0].url, "/manga/2072267132-logging-10-000-years-into-the-future/");
        assert_eq!(page.mangas[0].title, "Logging 10.000 Years into the Future");
        assert!(page.mangas.iter().all(|m| m.thumbnail_url.as_deref().is_some_and(|t| t.starts_with("https://lavascans.com/"))));
    }

    #[test]
    fn the_text_list_maps_permanent_slugs() {
        let map = parse_url_map(include_str!("../fixtures/lavascans/list-mode.html"));
        assert!(map.len() > 400, "{}", map.len());
        assert_eq!(map["what-can-i-do-alone"], "what-can-i-do-alone");
        assert_eq!(map["after-merging-with-the-goddess-i-am-invincible"], "2072267132-after-merging-with-the-goddess-i-am-invincible");
    }

    #[test]
    fn a_theme_title_page_has_its_details() {
        let manga = parse_details(&HIJALA, include_str!("../fixtures/hijala/manga.html")).unwrap();
        assert_eq!(manga.title, "Lookism");
        assert_eq!((manga.author.as_deref(), manga.artist.as_deref()), (Some("Taejun Pak"), Some("Taejun Pak")));
        assert_eq!(manga.status, status::ONGOING);
        let genre = manga.genre.unwrap();
        assert!(genre.starts_with("أكشن, حياة مدرسية") && genre.ends_with(", Manhwa"), "{genre}");
        let description = manga.description.unwrap();
        assert!(description.starts_with("بارك هيونج سوك") && description.contains("Alternative Names:\n- "), "{description}");
        assert!(manga.thumbnail_url.unwrap().starts_with("https://hijala.com/wp-content/uploads/"));
    }

    #[test]
    fn a_site_with_its_own_title_page_has_its_details() {
        let manga = parse_details(&LAVA, include_str!("../fixtures/lavascans/manga.html")).unwrap();
        assert_eq!(manga.title, "after merging with the goddess i am invincible");
        assert_eq!(manga.status, status::ONGOING);
        assert!(manga.genre.is_some() && manga.description.is_some() && manga.thumbnail_url.is_some(), "{manga:?}");
    }

    #[test]
    fn theme_chapters_skip_the_script_template() {
        let chapters = parse_chapters(&HIJALA, include_str!("../fixtures/hijala/manga.html"));
        assert!(chapters.len() > 600, "{}", chapters.len());
        assert_eq!((chapters[0].url.as_str(), chapters[0].name.as_str()), ("/lookism-630/", "فصل 630"));
        assert_eq!(chapters[0].date_upload, ms(2026, 10, 22));
        assert!(chapters.iter().all(|c| !c.url.contains('{') && !c.url.contains('#')));
    }

    #[test]
    fn site_chapters_read_their_own_name_and_date() {
        let chapters = parse_chapters(&LAVA, include_str!("../fixtures/lavascans/manga.html"));
        assert_eq!(chapters.len(), 10);
        assert!(chapters[0].name.starts_with("فصل ") && chapters[0].date_upload > ms(2020, 1, 1), "{:?}", chapters[0]);
        let locked = r#"<div id="chapters-list-container"><div class="ch-item locked"><a href="/x/"><span class="ch-num">1</span></a></div></div>"#;
        assert!(parse_chapters(&LAVA, locked).is_empty());
    }

    #[test]
    fn pages_come_from_the_reader_or_its_noscript_or_its_script() {
        let hijala = parse_pages(&HIJALA, include_str!("../fixtures/hijala/chapter.html")).unwrap();
        assert_eq!(hijala.len(), 30);
        assert_eq!(hijala[0], "https://hijala.com/wp-content/uploads/manga/27446e4e/01.webp");
        let lava = parse_pages(&LAVA, include_str!("../fixtures/lavascans/chapter.html")).unwrap();
        assert_eq!(lava[0], "https://lavascans.com/wp-content/uploads/2026/10/00-2.jpg");
        let script = r#"<div id="readerarea"></div><script>ts_reader.run({"sources":[{"images":["https:\/\/c.test\/1.jpg","\/up\/2.jpg"]}]});</script>"#;
        assert_eq!(parse_pages(&HIJALA, script).unwrap(), ["https://c.test/1.jpg", "https://hijala.com/up/2.jpg"]);
        let encoded = format!(r#"<script src="data:text/javascript;base64,{}"></script>"#, base64::engine::general_purpose::STANDARD.encode(r#"ts_reader.run({"images":["https://c.test/a.png"]})"#));
        assert_eq!(parse_pages(&HIJALA, &encoded).unwrap(), ["https://c.test/a.png"]);
        assert!(parse_pages(&HIJALA, "<div id='chapter-pages-js-before'></div>").is_err());
        assert!(parse_pages(&HIJALA, "<div id='readerarea'></div>").is_err());
    }

    #[test]
    fn dates_in_english_arabic_and_numbers() {
        assert_eq!(parse_date("October 22, 2026"), ms(2026, 10, 22));
        assert_eq!(parse_date("أكتوبر 22, 2026"), ms(2026, 10, 22));
        assert_eq!(parse_date("مايو 3, 2025"), ms(2025, 5, 3));
        assert_eq!(parse_date("2026/10/19"), ms(2026, 10, 19));
        assert_eq!(parse_date("2026-10-22"), ms(2026, 10, 22));
        assert_eq!(parse_date("10/22/2026"), ms(2026, 10, 22));
        assert_eq!(parse_date("منذ يومين"), 0);
    }

    #[test]
    fn statuses_in_the_theme_words() {
        assert_eq!((status_of("Ongoing"), status_of("مستمر"), status_of("مكتملة"), status_of("Dropped"), status_of("On Hold")), (1, 1, 2, 5, 6));
    }
}
