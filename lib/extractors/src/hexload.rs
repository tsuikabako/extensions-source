//! Hexload (an XFileSharing host): the embed page asks `POST /download` with `op=download3` for the file's address,
//! and the answer is JSON with `result.url`. Written from the site's own script (no Aniyomi extractor exists).

use kuro_guest::wire::Video;
use kuro_guest::{Host, Request, Res};
use serde::Deserialize;

use crate::title;

#[derive(Deserialize)]
struct Answer {
    msg: String,
    result: Option<File>,
}

#[derive(Deserialize)]
struct File {
    url: String,
}

pub fn videos(host: &dyn Host, embed_url: &str, label: &str) -> Res<Vec<Video>> {
    let id = file_id(embed_url).ok_or("Hexload: no file id in the address")?;
    let form = format!("op=download3&id={id}&ajax=1&method_free=1&dataType=json");
    let answer = Request::form("https://hexload.com/download", form)
        .header("Referer", embed_url)
        .header("X-Requested-With", "XMLHttpRequest")
        .text(host, "Hexload")?;
    parse(&answer, label).map(|video| vec![video])
}

/// The file id of `https://hexload.com/embed-<id>` or `https://hexload.com/<id>`.
pub fn file_id(embed_url: &str) -> Option<&str> {
    let last = embed_url.split('?').next()?.trim_end_matches('/').rsplit('/').next()?;
    let id = last.strip_prefix("embed-").unwrap_or(last).trim_end_matches(".html");
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())).then_some(id)
}

pub fn parse(answer: &str, label: &str) -> Res<Video> {
    let answer: Answer = serde_json::from_str(answer).map_err(|e| format!("Hexload: unexpected answer: {e}"))?;
    let file = answer.result.filter(|_| answer.msg == "OK").ok_or("Hexload: the file is not available")?;
    Ok(Video {
        video_url: file.url,
        video_title: title(label, None),
        headers: vec![("Referer".into(), "https://hexload.com/".into())],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuro_guest::testing::TestHost;

    #[test]
    fn the_download_answer_gives_the_mp4() {
        let video = parse(include_str!("../fixtures/hexload-download.json"), "").unwrap();
        assert!(video.video_url.ends_with("/HB3-01.mp4"), "{}", video.video_url);
        assert_eq!(video.video_title, "Auto");
        assert!(parse(r#"{"msg":"File not found","result":null}"#, "").is_err());
    }

    #[test]
    fn the_request_is_the_page_s_own() {
        assert_eq!(file_id("https://hexload.com/embed-6lvybdub3dse"), Some("6lvybdub3dse"));
        assert_eq!(file_id("https://hexload.com/6lvybdub3dse.html"), Some("6lvybdub3dse"));
        assert_eq!(file_id("https://hexload.com/"), None);
        let host = TestHost::with_body(include_str!("../fixtures/hexload-download.json"));
        let videos = videos(&host, "https://hexload.com/embed-6lvybdub3dse", "Sub").unwrap();
        assert_eq!(videos[0].video_title, "Sub Auto");
        let sent = host.sent();
        assert_eq!((sent[0].method.as_str(), sent[0].url.as_str()), ("POST", "https://hexload.com/download"));
        assert_eq!(sent[0].body.as_deref(), Some("op=download3&id=6lvybdub3dse&ajax=1&method_free=1&dataType=json"));
    }
}
