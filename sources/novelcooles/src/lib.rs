//! NovelCool in Spanish (<https://es.novelcool.com>), a manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! The ops, URLs and parsers are the shared `lib/novelcool` crate; this file is only the language.

use kuro_novelcool::Site;

const SITE: Site = Site {
    // `generate_id("NovelCool", "es", 1)`, the MD5 of `novelcool/es/1`, as the Kotlin extension has it. Keep it: backups
    // and the library name it.
    id: 438359803442833644,
    name: "NovelCool",
    lang: "es",
    base: "https://es.novelcool.com",
};

kuro_guest::plugin!(SITE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info: serde_json::Value = serde_json::from_str(&kuro_guest::invoke(&SITE, &kuro_guest::testing::TestHost::default(), "info", "{}").unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["lang"].as_str(), info["requestGapMs"].as_u64()), (Some(438359803442833644), Some("es"), Some(1000)));
    }
}
