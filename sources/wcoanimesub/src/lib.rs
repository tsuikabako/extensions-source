//! WcoAnimeSub (<https://www.wcoanimesub.tv>), an anime source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`).
//! A WCO site: the ops, the player and the parsers are the shared `lib/wco` crate; this file is only the site. The site
//! sends a title of the other kind to www.wco.tv or www.wcoanimedub.tv (a 301); the manifest lists them.

use kuro_wco::Site;

const SITE: Site = Site {
    // the MD5 of `wcoanimesub/en/1`, as the Kotlin extension has it. Keep it: backups and the library name it.
    id: 665629163594938512,
    name: "WcoAnimeSub",
    base: "https://www.wcoanimesub.tv",
    supports_latest: true,
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
            (Some(665629163594938512), Some("WcoAnimeSub"), Some("en"))
        );
        let manifest = include_str!("../manifest.toml");
        assert!(manifest.contains("\"www.wcoanimesub.tv\"") && manifest.contains("\"embed.wcostream.com\""));
    }
}
