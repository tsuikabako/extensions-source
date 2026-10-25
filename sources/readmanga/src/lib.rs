//! ReadManga (<https://a.zazaza.me>), a Russian manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`). A
//! GroupLe site: the ops, URLs and parsers are the shared `lib/grouple` crate; this file is only the site.

use kuro_grouple::Site;

const SITE: Site = Site {
    // The id the Kotlin extension sets in its build.gradle.kts (`id = 5L`, the old built-in ReadManga source), not one
    // derived from the name. Keep it: backups and the library name it.
    id: 5,
    name: "ReadManga",
    lang: "ru",
    base: "https://a.zazaza.me",
};

kuro_guest::plugin!(SITE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info: serde_json::Value = serde_json::from_str(&kuro_guest::invoke(&SITE, &kuro_guest::testing::TestHost::default(), "info", "{}").unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["requestGapMs"].as_u64()), (Some(5), Some("ReadManga"), Some(500)));
    }
}
