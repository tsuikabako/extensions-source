//! Usagi (<https://web.usagi.one>), a Russian manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`). A
//! GroupLe site: the ops, URLs and parsers are the shared `lib/grouple` crate; this file is only the site.

use kuro_grouple::Site;

const SITE: Site = Site {
    // `generate_id("Usagi", "ru", 1)`, the MD5 of `usagi/ru/1`, as the Kotlin extension has it. Keep it: backups and
    // the library name it.
    id: 5705163226489175947,
    name: "Usagi",
    lang: "ru",
    base: "https://web.usagi.one",
};

kuro_guest::plugin!(SITE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info: serde_json::Value = serde_json::from_str(&kuro_guest::invoke(&SITE, &kuro_guest::testing::TestHost::default(), "info", "{}").unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["requestGapMs"].as_u64()), (Some(5705163226489175947), Some("Usagi"), Some(500)));
    }
}
