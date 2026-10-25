//! Comic Verse (<https://arcomixverse.blogspot.com>), a manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! A ZeistManga site: the ops, URLs and parsers are the shared `lib/zeistmanga` crate; this file is only the site.

use kuro_zeistmanga::{DEFAULT, Site};

const SITE: Site = Site {
    // `generate_id("Comic Verse", "ar", 1)`, the MD5 of `comic verse/ar/1`, as the Kotlin extension has it. Keep it:
    // backups and the library name it.
    id: 7487319247633385575,
    name: "Comic Verse",
    base: "https://arcomixverse.blogspot.com",
    // the site's own popular widget: the theme's selector finds nothing on its home page
    popular: Some("div.PopularPosts div.cvh-popular-list > figure"),
    // the chapters are labelled with the title page's `data-label`, the reader is `#cvOriginal`
    data_label: true,
    pages: "#cvOriginal",
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
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["lang"].as_str()), (Some(7487319247633385575), Some("Comic Verse"), Some("ar")));
    }
}
