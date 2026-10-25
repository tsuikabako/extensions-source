//! WCOStream (<https://www.wcostream.tv>), an anime source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`). A
//! WCO site: the ops, the player and the parsers are the shared `lib/wco` crate; this file is only the site.

use kuro_wco::Site;

const SITE: Site = Site {
    // the MD5 of `wcostream/en/1`, as the Kotlin extension has it. Keep it: backups and the library name it.
    id: 4144024127778211557,
    name: "WCOStream",
    base: "https://www.wcostream.tv",
    // the site has no "Recently Added Series": Popular is its recent releases, and there is no Latest (the Kotlin's)
    supports_latest: false,
};

kuro_guest::plugin!(SITE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_and_manifest_name_the_site() {
        let info: serde_json::Value = serde_json::from_str(
            &kuro_guest::invoke(&SITE, &kuro_guest::testing::TestHost::default(), "info", "{}").unwrap(),
        )
        .unwrap();
        assert_eq!(
            (info["id"].as_i64(), info["name"].as_str(), info["lang"].as_str()),
            (Some(4144024127778211557), Some("WCOStream"), Some("en"))
        );
        let manifest = include_str!("../manifest.toml");
        assert!(manifest.contains("\"www.wcostream.tv\"") && manifest.contains("\"embed.wcostream.com\""));
    }
}
