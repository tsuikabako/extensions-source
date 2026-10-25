//! Lava Scans (<https://lavascans.com>), an Arabic manga source plug-in for Kurobayomi, ABI 1.5
//! (`wit/kuro-source.wit`). A MangaThemesia site with its own cards, title page and chapter list: the ops, URLs and
//! parsers are the shared `lib/mangathemesia` crate; this file is only the site.

use kuro_mangathemesia::Site;

const SITE: Site = Site {
    // The Kotlin extension sets `id = 3209001028102012989L`. Keep it: backups and the library name it.
    id: 3209001028102012989,
    name: "Lava Scans",
    lang: "ar",
    base: "https://lavascans.com",
    // the site puts a changing number before many slugs (`/manga/2072267132-a-title/`)
    random_slugs: true,
    list_card: ".listupd .manga-card-v",
    list_title: ".bigor .tt, h3 a",
    details: "div.lh-container",
    title: ".lh-title",
    description: "#manga-story",
    genre: ".lh-genres a",
    status: ".status-badge-lux",
    thumbnail: ".lh-poster img",
    // paid chapters are left out, as the Kotlin's default
    chapter_item: "#chapters-list-container .ch-item:not(.locked)",
    chapter_name: ".ch-num",
    chapter_date: ".ch-date",
    ..Site::THEME
};

kuro_guest::plugin!(SITE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info: serde_json::Value = serde_json::from_str(&kuro_guest::invoke(&SITE, &kuro_guest::testing::TestHost::default(), "info", "{}").unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["requestGapMs"].as_u64()), (Some(3209001028102012989), Some("Lava Scans"), Some(1000)));
    }
}
