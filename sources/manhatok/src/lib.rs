//! Manhatok (<https://manhatok.blogspot.com>), a manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! A ZeistManga site: the ops, URLs and parsers are the shared `lib/zeistmanga` crate; this file is only the site.

use kuro_zeistmanga::{DEFAULT, Site};

const SITE: Site = Site {
    // `generate_id("Manhatok", "ar", 1)`, the MD5 of `manhatok/ar/1`, as the Kotlin extension has it. Keep it:
    // backups and the library name it.
    id: 743599002989616408,
    name: "Manhatok",
    base: "https://manhatok.blogspot.com",
    // the Kotlin's `rateLimit(3)`: three requests a second
    request_gap_ms: 334,
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
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["lang"].as_str()), (Some(743599002989616408), Some("Manhatok"), Some("ar")));
    }
}
