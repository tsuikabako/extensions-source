//! The JSON shapes the app reads from a source, one type per shape. They mirror `wire.rs` and `anime_wire.rs` of the
//! app's `kuro-source-wasm` crate: every field the app reads, camelCase. A field left at its default is the app's
//! default too, so a source fills only what its site gives (`Manga { url, title, ..Default::default() }`).

use serde::{Deserialize, Serialize};

/// A title's status, as the app numbers it.
pub mod status {
    pub const UNKNOWN: i32 = 0;
    pub const ONGOING: i32 = 1;
    pub const COMPLETED: i32 = 2;
    pub const LICENSED: i32 = 3;
    pub const PUBLISHING_FINISHED: i32 = 4;
    pub const CANCELLED: i32 = 5;
    pub const ON_HIATUS: i32 = 6;
}

// ================================ Manga ================================

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Manga {
    /// The stored address of the title, usually its path on the site. Never change its form once a source is
    /// published: libraries and backups keep it.
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Genres joined with ", ".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// See [`status`].
    pub status: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

/// One page of a list (`popular`, `latest`, `search`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MangasPage {
    pub mangas: Vec<Manga>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Chapter {
    /// The stored address of the chapter; the same rule as [`Manga::url`].
    pub url: String,
    pub name: String,
    /// Milliseconds since 1970, 0 when unknown.
    pub date_upload: i64,
    /// -1 when unknown.
    pub chapter_number: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scanlator: Option<String>,
}

impl Default for Chapter {
    fn default() -> Self {
        Self { url: String::new(), name: String::new(), date_upload: 0, chapter_number: -1.0, scanlator: None }
    }
}

/// One page image of a chapter.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Page {
    pub url: String,
    pub image_url: Option<String>,
}

impl Page {
    /// A page that is just its image address.
    pub fn image(image_url: impl Into<String>) -> Self {
        Self { url: String::new(), image_url: Some(image_url.into()) }
    }
}

// ================================ Anime ================================

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Anime {
    /// The stored address of the title; the same rule as [`Manga::url`].
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    /// The studio, for most sites.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// See [`status`].
    pub status: i32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnimesPage {
    pub animes: Vec<Anime>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Episode {
    /// The stored address of the episode; the same rule as [`Manga::url`].
    pub url: String,
    pub name: String,
    /// -1 when unknown.
    pub episode_number: f32,
    pub fillermark: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scanlator: Option<String>,
    /// Milliseconds since 1970, 0 when unknown.
    pub date_upload: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl Default for Episode {
    fn default() -> Self {
        Self { url: String::new(), name: String::new(), episode_number: -1.0, fillermark: false, scanlator: None, date_upload: 0, summary: None }
    }
}

/// One server of an episode. With `video_list` the videos come at once; without it the app asks the `videos` op with
/// this hoster later (and gives it back as the source returned it, `internal_data` included).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Hoster {
    pub hoster_url: String,
    pub hoster_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_list: Option<Vec<Video>>,
    /// Anything the source needs to find the videos later.
    pub internal_data: String,
    pub lazy: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Video {
    /// The stream: an mp4 file or an HLS playlist. It loads in the app, on any host.
    pub video_url: String,
    /// The label in the quality menu ("Mp4Upload - 1080p").
    pub video_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i32>,
    /// Sent with the player's requests for this video (Referer, Origin, ...).
    pub headers: Vec<(String, String)>,
    /// The one the player starts with.
    pub preferred: bool,
    pub subtitle_tracks: Vec<Track>,
    pub audio_tracks: Vec<Track>,
    pub internal_data: String,
}

/// A subtitle or audio track.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Track {
    pub url: String,
    pub lang: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unset_fields_are_left_out_and_numbers_default_to_unknown() {
        let manga = Manga { url: "/m".into(), title: "M".into(), ..Default::default() };
        assert_eq!(serde_json::to_value(manga).unwrap(), json!({ "url": "/m", "title": "M", "status": 0 }));
        assert_eq!(Chapter::default().chapter_number, -1.0);
        assert_eq!(Episode::default().episode_number, -1.0);
        assert_eq!(serde_json::to_value(Page::image("i")).unwrap(), json!({ "url": "", "imageUrl": "i" }));
    }

    #[test]
    fn a_hoster_comes_back_as_it_went_out() {
        let hoster = Hoster { hoster_url: "u".into(), hoster_name: "n".into(), internal_data: "d".into(), ..Default::default() };
        let back: Hoster = serde_json::from_value(serde_json::to_value(&hoster).unwrap()).unwrap();
        assert_eq!(back, hoster);
        let video = Video { video_url: "v".into(), headers: vec![("Referer".into(), "r".into())], ..Default::default() };
        assert_eq!(serde_json::to_value(&video).unwrap()["headers"], json!([["Referer", "r"]]));
    }
}
