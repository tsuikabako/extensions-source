//! What Comix's pages show once a browser has loaded them: a list of titles, a title's chapter list, a chapter's
//! page images. Pure reading of the rendered HTML (as the app's page read hands it back), no requests. The stored
//! addresses are the ones of the keiyoushi extension (and of the first plug-in): `/<id>-<slug>` for a title,
//! `title/<id>-<slug>/<chapter id>-chapter-<n>` for a chapter.

use kuro_guest::Res;
use kuro_guest::wire::{Chapter, Manga, MangasPage, Page};

use crate::dates::upload_date;
use crate::tags::{attr, elements, first, text};

pub const BASE: &str = "https://comix.to";
const NAME: &str = "Comix";

/// `/<id>-<slug>` from a title link of the site (`/title/<id>-<slug>`): `None` for any other link.
pub fn title_address(href: &str) -> Option<String> {
    let path = href.trim().strip_prefix(BASE).unwrap_or(href.trim()).split(['?', '#']).next()?;
    let slug = path.strip_prefix("/title/")?;
    let ok = !slug.is_empty() && slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    ok.then(|| format!("/{slug}"))
}

/// `title/<id>-<slug>/<chapter id>-chapter-<n>` from a chapter link of the site, with the chapter's number.
pub fn chapter_address(href: &str) -> Option<(String, f32)> {
    let path = href.trim().strip_prefix(BASE).unwrap_or(href.trim()).split(['?', '#']).next()?;
    let (slug, chapter) = path.strip_prefix("/title/")?.split_once('/')?;
    let (id, number) = chapter.split_once("-chapter-")?;
    let ok = !slug.is_empty()
        && slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !id.is_empty()
        && id.bytes().all(|b| b.is_ascii_digit())
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit() || b == b'.');
    ok.then(|| (format!("title/{slug}/{chapter}"), number.parse().unwrap_or(-1.0)))
}

/// A cover or page address as a full one.
fn absolute(src: &str) -> Option<String> {
    let src = src.trim();
    if src.starts_with("https://") {
        Some(src.to_owned())
    } else if src.starts_with('/') && !src.starts_with("//") {
        Some(format!("{BASE}{src}"))
    } else {
        None
    }
}

/// Whether the page's pager (`nav.npager`) has a page after the current one: a number after the active one, or its
/// "Next page" button. A page without a pager is the only one.
fn has_next_page(html: &str) -> bool {
    let Some(pager) = first(html, "nav", "npager") else { return false };
    let next_button = elements(pager.inner, "button", "npager__nav").iter().any(|b| attr(b.start, "aria-label").as_deref() == Some("Next page") && attr(b.start, "disabled").is_none());
    let numbers = elements(pager.inner, "button", "npager__num");
    let active = numbers.iter().position(|b| attr(b.start, "class").is_some_and(|c| c.split_whitespace().any(|c| c == "is-active")));
    next_button || active.is_some_and(|active| active + 1 < numbers.len())
}

/// The titles of a browse page (`/browse?...&page=N`): its `div.list-grid` and its pager.
pub fn list(html: &str) -> Res<MangasPage> {
    if first(html, "div", "list-grid").is_none() && first(html, "div", "lrow").is_none() {
        return Err(format!("{NAME}: this page shows no list"));
    }
    let mangas = elements(html, "div", "lrow")
        .into_iter()
        .filter_map(|row| {
            let poster = first(row.inner, "a", "lrow__poster");
            let link = first(row.inner, "a", "lrow__title-link").or(poster)?;
            let url = title_address(&attr(link.start, "href")?)?;
            let title = first(row.inner, "h3", "lrow__title").map(|h| text(h.inner)).filter(|t| !t.is_empty()).or_else(|| poster.and_then(|p| attr(p.start, "aria-label")))?;
            let thumbnail_url = poster.and_then(|p| first(p.inner, "img", "")).and_then(|img| attr(img.start, "src")).and_then(|src| absolute(&src));
            Some(Manga { url, title, thumbnail_url, ..Default::default() })
        })
        .collect();
    Ok(MangasPage { mangas, has_next_page: has_next_page(html) })
}

/// The chapters of one page of a title's chapter list (`/title/<id>-<slug>?page=N`), in the site's order (newest
/// first), and whether a page follows. `now` is the clock in Unix seconds, for the relative upload times. A chapter
/// uploaded by several groups is listed once per group, as the site lists it.
pub fn chapters(html: &str, now: i64) -> Res<(Vec<Chapter>, bool)> {
    if first(html, "ul", "mchap-list").is_none() {
        return Err(format!("{NAME}: this page shows no chapter list"));
    }
    let chapters = elements(html, "li", "mchap-item")
        .into_iter()
        .filter_map(|item| {
            let link = first(item.inner, "a", "mchap-row__primary")?;
            let (url, chapter_number) = chapter_address(&attr(link.start, "href")?)?;
            let number = url.rsplit_once("-chapter-").map(|(_, n)| n.to_owned()).unwrap_or_default();
            let title = first(link.inner, "span", "mchap-row__title").map(|t| text(t.inner)).unwrap_or_default();
            let name = if title.is_empty() { format!("Chapter {number}") } else { format!("Chapter {number}: {title}") };
            let group = first(item.inner, "a", "mchap-row__group");
            let scanlator = match group.map(|g| text(g.inner)).filter(|g| !g.is_empty()) {
                Some(group) => group,
                None if group.is_some_and(|g| attr(g.start, "class").is_some_and(|c| c.split_whitespace().any(|c| c == "is-official"))) => "Official".to_owned(),
                None => "Unknown".to_owned(),
            };
            let date_upload = first(item.inner, "span", "mchap-row__time").map(|time| upload_date(&text(time.inner), now)).unwrap_or(0);
            Some(Chapter { url, name, date_upload, chapter_number, scanlator: Some(scanlator) })
        })
        .collect();
    Ok((chapters, has_next_page(html)))
}

/// The page images of a chapter's reader page, in order. The reader gives a slot its picture only when the slot
/// comes near the window, so a slot may still be without one: `missing` lists those page numbers (from 1).
#[derive(Debug)]
pub struct Reader {
    pub pages: Vec<Option<Page>>,
    pub missing: Vec<usize>,
}

pub fn reader(html: &str) -> Res<Reader> {
    let slots = elements(html, "div", "rpage-page");
    if slots.is_empty() {
        return Err(format!("{NAME}: this page shows no pages"));
    }
    let pages: Vec<Option<Page>> = slots.into_iter().map(|slot| first(slot.inner, "img", "rpage-page__img").and_then(|img| attr(img.start, "src")).and_then(|src| absolute(&src)).map(Page::image)).collect();
    let missing = pages.iter().enumerate().filter(|(_, page)| page.is_none()).map(|(index, _)| index + 1).collect();
    Ok(Reader { pages, missing })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BROWSE: &str = include_str!("../fixtures/browse.html");
    const TITLE: &str = include_str!("../fixtures/title.html");
    const TITLE_PAGE2: &str = include_str!("../fixtures/title-page2.html");
    const READER: &str = include_str!("../fixtures/reader.html");
    /// 2026-11-20T20:00:00Z, the day the pages were saved.
    const NOW: i64 = 1_795_204_800;

    #[test]
    fn links_become_stored_addresses() {
        assert_eq!(title_address("/title/02l05-after-a-meal").as_deref(), Some("/02l05-after-a-meal"));
        assert_eq!(title_address("https://comix.to/title/02l05-after-a-meal?x=1").as_deref(), Some("/02l05-after-a-meal"));
        assert_eq!(title_address("/title/02l05-after-a-meal/11725043-chapter-32"), None);
        assert_eq!(title_address("https://evil.example/title/a"), None);
        assert_eq!(title_address("/groups/11279"), None);
        assert_eq!(chapter_address("/title/02l05-after-a-meal/11686993-chapter-31.5"), Some(("title/02l05-after-a-meal/11686993-chapter-31.5".into(), 31.5)));
        assert_eq!(chapter_address("/title/02l05-after-a-meal/x-chapter-1"), None);
        assert_eq!(chapter_address("/title/02l05-after-a-meal"), None);
        assert_eq!(absolute("/images/covers/a@280.jpg").as_deref(), Some("https://comix.to/images/covers/a@280.jpg"));
        assert_eq!(absolute("//evil.example/a.jpg"), None);
        assert_eq!(absolute("data:image/png;base64,AA"), None);
    }

    #[test]
    fn a_browse_page_lists_its_titles() {
        let page = list(BROWSE).unwrap();
        assert_eq!(page.mangas.len(), 28);
        let first = &page.mangas[0];
        assert_eq!((first.url.as_str(), first.title.as_str()), ("/g2rk-on-the-way-to-meet-mom", "On the Way to Meet Mom"));
        assert_eq!(first.thumbnail_url.as_deref(), Some("https://comix.to/images/covers/38/3c/383c9dc1408fd98453eaccbd59754b976708b548@280.jpg"));
        assert!(page.mangas.iter().all(|m| m.url.starts_with('/') && !m.title.is_empty() && m.thumbnail_url.is_some()));
        assert!(page.has_next_page);
    }

    #[test]
    fn a_title_page_lists_twenty_chapters_and_the_next_page_follows() {
        let (chapters, more) = chapters(TITLE, NOW).unwrap();
        assert_eq!((chapters.len(), more), (20, true));
        let newest = &chapters[0];
        assert_eq!((newest.url.as_str(), newest.name.as_str(), newest.chapter_number), ("title/02l05-after-a-meal/11725043-chapter-32", "Chapter 32", 32.0));
        assert_eq!(newest.scanlator.as_deref(), Some("Dindin_"));
        // "1w ago"
        assert_eq!(newest.date_upload, (NOW - 7 * 86_400) * 1000);
        let named = chapters.iter().find(|c| c.url.ends_with("11686993-chapter-31.5")).unwrap();
        assert_eq!((named.name.as_str(), named.chapter_number), ("Chapter 31.5: Hiatus extension", 31.5));
        let (second, more) = super::chapters(TITLE_PAGE2, NOW).unwrap();
        assert_eq!((second.len(), more), (20, true));
        assert!(second[0].chapter_number < chapters[19].chapter_number || second[0].url != chapters[19].url);
        assert!(second.iter().all(|c| !chapters.iter().any(|seen| seen.url == c.url)), "page 2 repeats nothing of page 1");
    }

    #[test]
    fn a_reader_page_gives_the_addresses_it_has_and_names_the_slots_without_one() {
        let reader = reader(READER).unwrap();
        assert_eq!(reader.pages.len(), 50);
        assert_eq!(reader.missing.len(), 47);
        assert_eq!(reader.missing[0], 4);
        let first = reader.pages[0].as_ref().unwrap().image_url.as_deref().unwrap();
        assert!(first.starts_with("https://447.") && first.contains("/hi/"), "{first}");
    }

    #[test]
    fn another_page_is_an_error_not_an_empty_answer() {
        let other = "<html><head><title>404</title></head><body><p>nothing</p></body></html>";
        assert_eq!(list(other).unwrap_err(), "Comix: this page shows no list");
        assert_eq!(chapters(other, NOW).unwrap_err(), "Comix: this page shows no chapter list");
        assert_eq!(reader(other).unwrap_err(), "Comix: this page shows no pages");
        // a list that has run out is a real, empty page
        assert_eq!(list("<div class=\"list-grid\"></div>").unwrap(), MangasPage::default());
    }
}
