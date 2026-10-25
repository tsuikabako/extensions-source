//! Manga Ai Land (<https://manga-ai-land.blogspot.com>), a manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! A ZeistManga site: the ops, URLs and parsers are the shared `lib/zeistmanga` crate; this file is only the site.

use kuro_zeistmanga::{DEFAULT, Site};

const SITE: Site = Site {
    // `generate_id("Manga Ai Land", "ar", 1)`, the MD5 of `manga ai land/ar/1`, as the Kotlin extension has it. Keep it:
    // backups and the library name it.
    id: 7109397520501699498,
    name: "Manga Ai Land",
    base: "https://manga-ai-land.blogspot.com",
    // the chapter posts are labelled in Arabic
    chapter_label: "فصل",
    ..DEFAULT
};

kuro_guest::plugin!(SITE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_carries_the_id_and_the_name() {
        let info: serde_json::Value =
            serde_json::from_str(&kuro_guest::invoke(&SITE, &kuro_guest::testing::TestHost::default(), "info", "{}").unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["lang"].as_str()), (Some(7109397520501699498), Some("Manga Ai Land"), Some("ar")));
    }
}
