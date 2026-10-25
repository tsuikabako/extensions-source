//! Hijala (<https://hijala.com>), an Arabic manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! A MangaThemesia site: the ops, URLs and parsers are the shared `lib/mangathemesia` crate; this file is only the site.

use kuro_mangathemesia::Site;

const SITE: Site = Site {
    // `generate_id("Hijala", "ar", 2)`, the MD5 of `hijala/ar/2` (the Kotlin's `versionId = 2`: the site moved from
    // ZeistManga back to MangaThemesia). Keep it: backups and the library name it.
    id: 917436262447415426,
    name: "Hijala",
    lang: "ar",
    base: "https://hijala.com",
    ..Site::THEME
};

kuro_guest::plugin!(SITE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info: serde_json::Value = serde_json::from_str(&kuro_guest::invoke(&SITE, &kuro_guest::testing::TestHost::default(), "info", "{}").unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["lang"].as_str()), (Some(917436262447415426), Some("Hijala"), Some("ar")));
    }
}
