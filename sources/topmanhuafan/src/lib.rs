//! TopManhua.fan (<https://www.topmanhua.fan>), a manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! A Madara site: the ops, URLs and parsers are the shared `lib/madara` crate; this file is only the site.

use kuro_madara::{Browse, Chapters, Site};

const SITE: Site = Site {
    // `generate_id("TopManhua.fan", "en", 1)`, the MD5 of `topmanhua.fan/en/1`, as the Kotlin extension has it. Keep it:
    // backups and the library name it.
    id: 6293890187960663957,
    name: "TopManhua.fan",
    lang: "en",
    base: "https://www.topmanhua.fan",
    manga_dir: "manhua",
    // the site answers 404 to admin-ajax.php and to ajax/chapters/: paged archives, chapters on the title page
    browse: Browse::Pages,
    chapters: Chapters::MangaPage,
    // the site has no stated limit; one request a second keeps well under any
    request_gap_ms: 1000,
};

kuro_guest::plugin!(SITE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info: serde_json::Value = serde_json::from_str(&kuro_guest::invoke(&SITE, &kuro_guest::testing::TestHost::default(), "info", "{}").unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["requestGapMs"].as_u64()), (Some(6293890187960663957), Some("TopManhua.fan"), Some(1000)));
    }
}
