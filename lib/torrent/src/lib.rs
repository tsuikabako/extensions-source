//! What every torrent source shares: reading a `.torrent` file (its files and trackers) and writing the magnet link a
//! video plays from.
//!
//! The convention every torrent source follows (Aniyomi's): a video's `videoUrl` is
//! `magnet:?xt=urn:btih:<hash>&dn=<name>&tr=<tracker>...&index=<N>`, where the non-standard `index` names the file to
//! play inside the torrent, 0-based, in the order the torrent lists its files. The app streams that file with its own
//! torrent engine (Kurobayomi 0.2.6 and newer; older apps refuse magnet links).
//!
//! No SHA-1 here: the info hash comes from the site (its magnet link or its API), never from hashing the file.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use kuro_guest::{Res, Response};
use url::form_urlencoded;

/// One file of a torrent.
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    /// The path inside the torrent, folders joined with `/` (the torrent's own name is not part of it in a multi-file
    /// torrent; a single-file torrent's path is its name).
    pub path: String,
    pub size: u64,
    /// The file's place in the torrent's list, 0-based: the magnet's `index`.
    pub index: usize,
}

impl File {
    /// The file name without its folders.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// What a source needs from a `.torrent` file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Torrent {
    pub name: String,
    pub files: Vec<File>,
    /// `announce-list` then `announce`, without duplicates.
    pub trackers: Vec<String>,
}

/// Reads a `.torrent` file. Only the files and trackers are read; a v2-only torrent (no `length` and no `files`) is an
/// error.
pub fn parse(bytes: &[u8]) -> Res<Torrent> {
    let (root, _) = Value::decode(bytes, 0, 0)?;
    let info = root.get(b"info").ok_or("the torrent file has no info")?;
    let name = info.get(b"name.utf-8").or_else(|| info.get(b"name")).and_then(Value::text).unwrap_or_default();
    let files = match info.get(b"files").and_then(Value::list) {
        Some(list) => list
            .iter()
            .enumerate()
            .map(|(index, file)| {
                let parts = file.get(b"path.utf-8").or_else(|| file.get(b"path")).and_then(Value::list).unwrap_or(&[]);
                let path = parts.iter().filter_map(Value::text).collect::<Vec<_>>().join("/");
                File { path, size: file.get(b"length").and_then(Value::int).unwrap_or(0), index }
            })
            .collect(),
        None => {
            let size = info.get(b"length").and_then(Value::int).ok_or("the torrent file lists no files")?;
            vec![File { path: name.clone(), size, index: 0 }]
        }
    };
    let mut trackers: Vec<String> = Vec::new();
    let tiers = root.get(b"announce-list").and_then(Value::list).unwrap_or(&[]);
    let listed = tiers.iter().flat_map(|tier| tier.list().unwrap_or(&[])).chain(root.get(b"announce"));
    for tracker in listed.filter_map(Value::text) {
        let tracker = tracker.trim().to_owned();
        if !tracker.is_empty() && !trackers.contains(&tracker) {
            trackers.push(tracker);
        }
    }
    Ok(Torrent { name, files, trackers })
}

/// The bytes of a `.torrent` answer: the app sends a body that is not UTF-8 text as base64.
pub fn body_bytes(response: Response, site: &str) -> Res<Vec<u8>> {
    if !response.is_success() {
        return Err(response.error(site));
    }
    match response.body_encoding.as_deref() {
        Some("base64") => STANDARD.decode(response.body.trim()).map_err(|e| format!("{site} sent a bad body: {e}")),
        _ => Ok(response.body.into_bytes()),
    }
}

/// The magnet link of one file: `magnet:?xt=urn:btih:<hash>&dn=<name>&tr=...&index=<N>`. The name and trackers are
/// percent-encoded; `index` is left out when `None` (the app then plays the largest video file).
pub fn magnet(hash: &str, name: &str, trackers: &[String], index: Option<usize>) -> String {
    let mut link = format!("magnet:?xt=urn:btih:{}", hash.trim());
    let mut query = form_urlencoded::Serializer::new(String::new());
    if !name.is_empty() {
        query.append_pair("dn", name);
    }
    for tracker in trackers {
        query.append_pair("tr", tracker);
    }
    let query = query.finish();
    if !query.is_empty() {
        link.push('&');
        link.push_str(&query);
    }
    if let Some(index) = index {
        link.push_str(&format!("&index={index}"));
    }
    link
}

/// The parts of a magnet link: its info hash (`xt=urn:btih:`), its name (`dn`) and its trackers (`tr`), decoded.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Magnet {
    pub hash: String,
    pub name: String,
    pub trackers: Vec<String>,
}

/// Reads a magnet link, `None` when it has no BitTorrent info hash.
pub fn read_magnet(link: &str) -> Option<Magnet> {
    let query = link.trim().strip_prefix("magnet:?")?;
    let mut magnet = Magnet::default();
    for (key, value) in form_urlencoded::parse(query.as_bytes()) {
        match &*key {
            "xt" => {
                if let Some(hash) = value.strip_prefix("urn:btih:") {
                    magnet.hash = hash.to_owned();
                }
            }
            "dn" => magnet.name = value.into_owned(),
            "tr" if !magnet.trackers.iter().any(|t| *t == value) => magnet.trackers.push(value.into_owned()),
            _ => {}
        }
    }
    (!magnet.hash.is_empty()).then_some(magnet)
}

/// The file extensions a torrent source offers as episodes (the Kotlin `validExtensions`).
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "avi", "wmv", "mkv", "flv", "webm", "ogg", "mpeg", "mpg", "mts", "vob", "ts", "m4v"];

/// Whether a file name is a video by its extension.
pub fn is_video(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, ext)| VIDEO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// A size as people read it: "1.37 GB", "350.00 MB", "12.00 KB" (powers of 1024, as the Kotlin writes it).
pub fn readable_size(bytes: u64) -> String {
    let kb = bytes as f64 / 1024.0;
    let mb = kb / 1024.0;
    let gb = mb / 1024.0;
    if gb >= 1.0 {
        format!("{gb:.2} GB")
    } else if mb >= 1.0 {
        format!("{mb:.2} MB")
    } else {
        format!("{kb:.2} KB")
    }
}

/// The video height a release name states ("... (1080p) ..." → 1080; "2160p", "4K" → 2160), `None` when it states none.
pub fn height(name: &str) -> Option<i32> {
    let lower = name.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for (at, _) in lower.match_indices('p') {
        let digits = bytes[..at].iter().rev().take_while(|b| b.is_ascii_digit()).count();
        let follows = bytes.get(at + 1).is_none_or(|b| !b.is_ascii_alphanumeric());
        let before = at - digits;
        let starts = before == 0 || !bytes[before - 1].is_ascii_alphanumeric();
        if (3..=4).contains(&digits) && follows && starts {
            return lower[before..at].parse().ok();
        }
    }
    lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|word| word == "4k").then_some(2160)
}

// ================================ Bencode ================================

/// One bencoded value. Byte strings borrow the file.
#[derive(Debug, PartialEq)]
enum Value<'a> {
    Int(i64),
    Bytes(&'a [u8]),
    List(Vec<Value<'a>>),
    Dict(Vec<(&'a [u8], Value<'a>)>),
}

/// Deeper than this is not a torrent file.
const MAX_DEPTH: usize = 64;

impl<'a> Value<'a> {
    /// The value at `at` and the position after it.
    fn decode(data: &'a [u8], at: usize, depth: usize) -> Res<(Self, usize)> {
        if depth > MAX_DEPTH {
            return Err("the torrent file is nested too deep".into());
        }
        let bad = || format!("the torrent file is damaged at byte {at}");
        match data.get(at).ok_or_else(bad)? {
            b'i' => {
                let end = find(data, at + 1, b'e').ok_or_else(bad)?;
                let number = std::str::from_utf8(&data[at + 1..end]).ok().and_then(|n| n.parse().ok()).ok_or_else(bad)?;
                Ok((Value::Int(number), end + 1))
            }
            b'l' => {
                let (mut items, mut next) = (Vec::new(), at + 1);
                while data.get(next).ok_or_else(bad)? != &b'e' {
                    let (item, after) = Value::decode(data, next, depth + 1)?;
                    items.push(item);
                    next = after;
                }
                Ok((Value::List(items), next + 1))
            }
            b'd' => {
                let (mut entries, mut next) = (Vec::new(), at + 1);
                while data.get(next).ok_or_else(bad)? != &b'e' {
                    let (Value::Bytes(key), after) = Value::decode(data, next, depth + 1)? else { return Err(bad()) };
                    let (value, after) = Value::decode(data, after, depth + 1)?;
                    entries.push((key, value));
                    next = after;
                }
                Ok((Value::Dict(entries), next + 1))
            }
            b'0'..=b'9' => {
                let colon = find(data, at, b':').ok_or_else(bad)?;
                let len: usize = std::str::from_utf8(&data[at..colon]).ok().and_then(|n| n.parse().ok()).ok_or_else(bad)?;
                let end = colon.checked_add(1 + len).filter(|end| *end <= data.len()).ok_or_else(bad)?;
                Ok((Value::Bytes(&data[colon + 1..end]), end))
            }
            _ => Err(bad()),
        }
    }

    fn get(&self, key: &[u8]) -> Option<&Value<'a>> {
        match self {
            Value::Dict(entries) => entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn list(&self) -> Option<&[Value<'a>]> {
        match self {
            Value::List(items) => Some(items),
            _ => None,
        }
    }

    fn int(&self) -> Option<u64> {
        match self {
            Value::Int(n) => u64::try_from(*n).ok(),
            _ => None,
        }
    }

    /// A byte string as text (invalid UTF-8 replaced).
    fn text(&self) -> Option<String> {
        match self {
            Value::Bytes(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        }
    }
}

fn find(data: &[u8], from: usize, byte: u8) -> Option<usize> {
    data.get(from..)?.iter().position(|b| *b == byte).map(|i| from + i)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Nyaa batch saved on 2026-10-29, its piece hashes blanked to keep it small.
    const BATCH: &[u8] = include_bytes!("../fixtures/batch.torrent");

    #[test]
    fn a_multi_file_torrent_lists_its_files_in_order_and_its_trackers() {
        let torrent = parse(BATCH).unwrap();
        assert_eq!(torrent.name, "[SubsPlease] Sousou no Frieren S2 (01-10) (1080p) [Batch]");
        assert_eq!(torrent.files.len(), 10);
        let first = &torrent.files[0];
        assert_eq!((first.path.as_str(), first.size, first.index), ("[SubsPlease] Sousou no Frieren S2 - 01v2 (1080p) [4149A222].mkv", 1469186575, 0));
        assert_eq!(torrent.files[9].index, 9);
        assert!(torrent.files[9].path.contains(" - 10 "));
        assert_eq!(torrent.trackers.len(), 13);
        assert_eq!(torrent.trackers[0], "http://nyaa.tracker.wf:7777/announce");
        assert_eq!(torrent.trackers[12], "wss://tracker.openwebtorrent.com");
    }

    #[test]
    fn a_single_file_torrent_is_its_name_at_index_0() {
        let data = b"d8:announce21:udp://t.example:1/ann4:infod6:lengthi2048e4:name6:ep.mkv12:piece lengthi1e6:pieces0:ee";
        let torrent = parse(data).unwrap();
        assert_eq!(torrent.files, vec![File { path: "ep.mkv".into(), size: 2048, index: 0 }]);
        assert_eq!(torrent.trackers, vec!["udp://t.example:1/ann".to_owned()]);
    }

    #[test]
    fn folders_join_and_names_drop_them() {
        let data = b"d4:infod5:filesld6:lengthi5e4:pathl3:sub5:a.mkveed6:lengthi1e10:path.utf-8l5:b.srteee4:name1:xee";
        let torrent = parse(data).unwrap();
        assert_eq!(torrent.files[0].path, "sub/a.mkv");
        assert_eq!(torrent.files[0].name(), "a.mkv");
        assert_eq!((torrent.files[1].path.as_str(), torrent.files[1].index), ("b.srt", 1));
    }

    #[test]
    fn damaged_files_are_errors_not_panics() {
        for data in [&b""[..], b"d4:info", b"d4:infod5:filesl", b"i12", b"5:ab", b"d4:infod4:name1:xee", b"x", b"d1:ai1ee"] {
            assert!(parse(data).is_err(), "{:?}", String::from_utf8_lossy(data));
        }
        let deep = format!("{}{}", "l".repeat(100), "e".repeat(100));
        assert!(parse(deep.as_bytes()).is_err());
    }

    #[test]
    fn a_magnet_names_the_file_and_reads_back() {
        let trackers = vec!["http://nyaa.tracker.wf:7777/announce".to_owned(), "udp://open.stealth.si:80/announce".to_owned()];
        let link = magnet("bc25231f", "A & B [1080p]", &trackers, Some(3));
        assert_eq!(link, "magnet:?xt=urn:btih:bc25231f&dn=A+%26+B+%5B1080p%5D&tr=http%3A%2F%2Fnyaa.tracker.wf%3A7777%2Fannounce&tr=udp%3A%2F%2Fopen.stealth.si%3A80%2Fannounce&index=3");
        let back = read_magnet(&link).unwrap();
        assert_eq!((back.hash.as_str(), back.name.as_str(), back.trackers), ("bc25231f", "A & B [1080p]", trackers));
        assert_eq!(magnet("h", "", &[], None), "magnet:?xt=urn:btih:h");
        assert_eq!(read_magnet("magnet:?dn=x"), None);
        assert_eq!(read_magnet("https://x"), None);
    }

    #[test]
    fn a_base64_body_is_decoded() {
        let response = |body: &str, encoding: Option<&str>, status| Response {
            status,
            url: String::new(),
            headers: Vec::new(),
            body: body.into(),
            body_encoding: encoding.map(str::to_owned),
        };
        assert_eq!(body_bytes(response("ZDFlZQ==", Some("base64"), 200), "S").unwrap(), b"d1ee");
        assert_eq!(body_bytes(response("de", None, 200), "S").unwrap(), b"de");
        assert_eq!(body_bytes(response("", None, 404), "S"), Err("HTTP 404 from S".into()));
    }

    #[test]
    fn helpers_read_names() {
        assert!(is_video("a/b.MKV") && is_video("x.ts") && !is_video("x.srt") && !is_video("mkv"));
        assert_eq!(readable_size(1469186575), "1.37 GB");
        assert_eq!(readable_size(350 * 1024 * 1024), "350.00 MB");
        assert_eq!(readable_size(12 * 1024), "12.00 KB");
        assert_eq!(height("[SubsPlease] Frieren - 01 (1080p) [4149A222].mkv"), Some(1080));
        assert_eq!(height("Show.S01E01.720p.WEB.x264"), Some(720));
        assert_eq!(height("Show 4K HDR"), Some(2160));
        assert_eq!(height("Show 2160p"), Some(2160));
        assert_eq!(height("[Group] Show - 01 [ABCD1080p]"), None);
        assert_eq!(height("Show - 01.mkv"), None);
    }
}
