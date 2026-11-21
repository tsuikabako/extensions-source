//! The site's server functions (TanStack Start) take and give seroval node trees: the `payload` query value of a call
//! and the body of its answer. Ported from the keiyoushi `inkstory` theme's `Seroval.kt` (Apache-2.0). No network here.

use kuro_guest::Res;
use serde_json::{Map, Value, json};

/// The `payload` query value for a call with input `data`, before url encoding.
pub fn payload(data: &Value) -> String {
    let mut next = 0;
    let root = node(&json!({ "data": data }), &mut next);
    json!({ "t": root, "f": 127, "m": [] }).to_string()
}

fn node(value: &Value, next: &mut u32) -> Value {
    match value {
        Value::Object(map) => {
            let id = take(next);
            let values: Vec<Value> = map.values().map(|v| node(v, next)).collect();
            json!({ "t": 10, "i": id, "p": { "k": map.keys().collect::<Vec<_>>(), "v": values }, "o": 0 })
        }
        Value::Array(items) => {
            let id = take(next);
            json!({ "t": 9, "i": id, "a": items.iter().map(|v| node(v, next)).collect::<Vec<_>>(), "o": 0 })
        }
        Value::Null => json!({ "t": 2, "s": 0 }),
        Value::Bool(b) => json!({ "t": 2, "s": if *b { 2 } else { 3 } }),
        Value::String(s) => json!({ "t": 1, "s": escape(s) }),
        Value::Number(n) => json!({ "t": 0, "s": n }),
    }
}

fn take(next: &mut u32) -> u32 {
    *next += 1;
    *next - 1
}

/// seroval escapes strings for embedding in a script: these characters, and back on the way in.
const ESCAPES: [(char, &str); 10] = [
    ('"', "\\\""),
    ('\\', "\\\\"),
    ('\n', "\\n"),
    ('\r', "\\r"),
    ('\u{8}', "\\b"),
    ('\t', "\\t"),
    ('\u{c}', "\\f"),
    ('<', "\\x3C"),
    ('\u{2028}', "\\u2028"),
    ('\u{2029}', "\\u2029"),
];

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match ESCAPES.iter().find(|(from, _)| *from == c) {
            Some((_, to)) => out.push_str(to),
            None => out.push(c),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'outer: while !rest.is_empty() {
        if rest.starts_with('\\') {
            for (to, from) in ESCAPES {
                if let Some(after) = rest.strip_prefix(from) {
                    out.push(to);
                    rest = after;
                    continue 'outer;
                }
            }
        }
        let c = rest.chars().next().unwrap_or_default();
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// The `result` of a call's answer as plain JSON; `None` when the site reports the item as not found.
pub fn result(body: &str) -> Res<Option<Value>> {
    let top: Value = serde_json::from_str(body).map_err(|e| format!("InkStory: the answer is not a server function result ({e})"))?;
    let (keys, values) = properties(&top).ok_or("InkStory: the answer has no properties")?;
    let at = |name: &str| keys.iter().position(|k| k.as_str() == Some(name)).and_then(|i| values.get(i));
    let mut refs = Map::new();
    let found = match at("result") {
        Some(result) => decode(result, &mut refs)?,
        None => Value::Null,
    };
    if !found.is_null() {
        return Ok(Some(found));
    }
    let not_found = at("error")
        .filter(|e| e["t"] == 10)
        .and_then(properties)
        .is_some_and(|(keys, _)| keys.iter().any(|k| k.as_str() == Some("isNotFound")));
    if not_found { Ok(None) } else { Err("InkStory: the site answered with an error".to_owned()) }
}

/// An object node's keys and values.
fn properties(node: &Value) -> Option<(&Vec<Value>, &Vec<Value>)> {
    let p = node.get("p")?;
    Some((p.get("k")?.as_array()?, p.get("v")?.as_array()?))
}

fn decode(node: &Value, refs: &mut Map<String, Value>) -> Res<Value> {
    let bad = || format!("InkStory: unreadable server function node {node}");
    let value = match node["t"].as_i64().ok_or_else(bad)? {
        0 => node["s"].clone(),
        1 => Value::String(unescape(node["s"].as_str().ok_or_else(bad)?)),
        2 => match node["s"].as_i64() {
            Some(2) => Value::Bool(true),
            Some(3) => Value::Bool(false),
            _ => Value::Null,
        },
        4 => return refs.get(&node["i"].to_string()).cloned().ok_or_else(bad),
        9 => Value::Array(node["a"].as_array().ok_or_else(bad)?.iter().map(|v| decode(v, refs)).collect::<Res<_>>()?),
        10 | 11 => {
            let (keys, values) = properties(node).ok_or_else(bad)?;
            let mut map = Map::new();
            for (k, v) in keys.iter().zip(values) {
                map.insert(k.as_str().ok_or_else(bad)?.to_owned(), decode(v, refs)?);
            }
            Value::Object(map)
        }
        other => return Err(format!("InkStory: unsupported server function node type {other}")),
    };
    if let Some(id) = node.get("i") {
        refs.insert(id.to_string(), value.clone());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payload_is_a_node_tree_under_data() {
        let tree: Value = serde_json::from_str(&payload(&json!({ "slug": "a<b", "createView": false }))).unwrap();
        assert_eq!(tree["f"], 127);
        let data = &tree["t"]["p"]["v"][0];
        assert_eq!((tree["t"]["i"].as_i64(), data["i"].as_i64()), (Some(0), Some(1)));
        assert_eq!(data["p"]["k"], json!(["createView", "slug"]));
        assert_eq!(data["p"]["v"], json!([{ "t": 2, "s": 3 }, { "t": 1, "s": "a\\x3Cb" }]));
        let list: Value = serde_json::from_str(&payload(&json!(["x", 1, null]))).unwrap();
        assert_eq!(list["t"]["p"]["v"][0], json!({ "t": 9, "i": 1, "a": [{ "t": 1, "s": "x" }, { "t": 0, "s": 1 }, { "t": 2, "s": 0 }], "o": 0 }));
    }

    #[test]
    fn strings_read_back_as_written() {
        let text = "a\"b\\c\nd<e\u{2028}ф";
        assert_eq!(unescape(&escape(text)), text);
    }

    #[test]
    fn a_result_reads_objects_arrays_and_references() {
        let body = r#"{"t":10,"i":0,"p":{"k":["result","error"],"v":[{"t":10,"i":1,"p":{"k":["a","b","c"],"v":[{"t":9,"i":2,"a":[{"t":0,"s":1.5},{"t":2,"s":2}],"o":0},{"t":4,"i":2},{"t":1,"s":"x\\x3Cy"}]},"o":0},{"t":2,"s":0}]},"o":0}"#;
        assert_eq!(result(body), Ok(Some(json!({ "a": [1.5, true], "b": [1.5, true], "c": "x<y" }))));
    }

    #[test]
    fn not_found_is_none_and_another_error_is_an_error() {
        let not_found = r#"{"t":10,"i":0,"p":{"k":["result","error"],"v":[{"t":2,"s":0},{"t":10,"i":1,"p":{"k":["isNotFound"],"v":[{"t":2,"s":2}]},"o":0}]},"o":0}"#;
        assert_eq!(result(not_found), Ok(None));
        let failed = r#"{"t":10,"i":0,"p":{"k":["result","error"],"v":[{"t":2,"s":0},{"t":10,"i":1,"p":{"k":["message"],"v":[{"t":1,"s":"boom"}]},"o":0}]},"o":0}"#;
        assert!(result(failed).is_err());
    }
}
