//! Murim (<https://murim-manga.blogspot.com>), a manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! A ZeistManga site: the ops, URLs and parsers are the shared `lib/zeistmanga` crate; this file is only the site.

use kuro_zeistmanga::{DEFAULT, Site};

const SITE: Site = Site {
    // `generate_id("Murim", "ar", 1)`, the MD5 of `murim/ar/1`, as the Kotlin extension has it. Keep it:
    // backups and the library name it.
    id: 4966914277555229831,
    name: "Murim",
    // www.murim.site sends every feed address to its home page; the blog itself answers them
    base: "https://murim-manga.blogspot.com",
    // no popular list on the home page: Popular is the newest titles, no Latest
    popular: None,
    // the Kotlin's `rateLimit(2)`
    request_gap_ms: 500,
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
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["lang"].as_str()), (Some(4966914277555229831), Some("Murim"), Some("ar")));
    }
}
