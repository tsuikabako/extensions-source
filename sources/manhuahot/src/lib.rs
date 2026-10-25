//! ManhuaHot (<https://manhuahot.com>), a manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`). A Madara site: the
//! ops, URLs and parsers are the shared `lib/madara` crate; this file is only the site.

use kuro_madara::{Browse, Chapters, Site};

const SITE: Site = Site {
    // `generate_id("ManhuaHot", "en", 1)`, the MD5 of `manhuahot/en/1`, as the Kotlin extension has it. Keep
    // it: backups and the library name it.
    id: 7638481723868333116,
    name: "ManhuaHot",
    lang: "en",
    base: "https://manhuahot.com",
    manga_dir: "manga",
    browse: Browse::Ajax,
    chapters: Chapters::MangaPage,
    request_gap_ms: 1000,
};

kuro_guest::plugin!(SITE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info: serde_json::Value = serde_json::from_str(&kuro_guest::invoke(&SITE, &kuro_guest::testing::TestHost::default(), "info", "{}").unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["requestGapMs"].as_u64()), (Some(7638481723868333116), Some("ManhuaHot"), Some(1000)));
    }
}
