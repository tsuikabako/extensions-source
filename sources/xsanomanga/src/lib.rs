//! XSano Manga (<https://www.xsano-manga.com>), a manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! A ZeistManga site: the ops, URLs and parsers are the shared `lib/zeistmanga` crate; this file is only the site.

use kuro_zeistmanga::{DEFAULT, Details, Site};

const SITE: Site = Site {
    // `generate_id("XSano Manga", "ar", 1)`, the MD5 of `xsano manga/ar/1`, as the Kotlin extension has it. Keep it:
    // backups and the library name it.
    id: 6170348197605244575,
    name: "XSano Manga",
    base: "https://www.xsano-manga.com",
    // no popular list on the home page: Popular is the newest titles, no Latest
    popular: None,
    details: Details { root: "main", genres: "dl a[rel=tag]", info: "#extra-info dl", info_title: "dt", info_value: "dd" },
    pages: "#reader div.separator",
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
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["lang"].as_str()), (Some(6170348197605244575), Some("XSano Manga"), Some("ar")));
    }
}
