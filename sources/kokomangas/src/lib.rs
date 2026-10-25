//! KokoMangas (<https://kokomangas.com>), a manga source plug-in for Kurobayomi, ABI 1.5 (`wit/kuro-source.wit`). A Madara site: the
//! ops, URLs and parsers are the shared `lib/madara` crate; this file is the site and the ABI glue.

use kuro_madara::{Browse, Chapters, Site};

wit_bindgen::generate!({
    path: "wit",
    world: "source",
});

use kuro::source::host;

const SITE: Site = Site {
    // `generate_id("KokoMangas", "en", 2)`, the MD5 of `kokomangas/en/2`, as the Kotlin extension has it. Keep
    // it: backups and the library name it.
    id: 58992897252080929,
    name: "KokoMangas",
    lang: "en",
    base: "https://kokomangas.com",
    manga_dir: "manga",
    browse: Browse::Ajax,
    chapters: Chapters::MangaAjax,
    request_gap_ms: 1000,
};

struct Plugin;

impl Guest for Plugin {
    fn invoke(op: String, input_json: String) -> Result<String, String> {
        SITE.invoke(&op, &input_json, host::http, host::unix_seconds)
    }
}

export!(Plugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_carries_the_id_and_the_spacing() {
        let info: serde_json::Value = serde_json::from_str(&<Plugin as Guest>::invoke("info".into(), "{}".into()).unwrap()).unwrap();
        assert_eq!((info["id"].as_i64(), info["name"].as_str(), info["requestGapMs"].as_u64()), (Some(58992897252080929), Some("KokoMangas"), Some(1000)));
    }
}
