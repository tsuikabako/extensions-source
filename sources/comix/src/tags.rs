//! A small reader of HTML as a browser writes it out (`outerHTML`): elements by tag and class, their attributes and
//! their text. It stands in for the `scraper` crate, which with the image decoder would take this plug-in over the
//! size limit. It relies on what a browser's own serializer guarantees: every tag is closed, attribute values are
//! quoted, and a `<` in text is written `&lt;`. It is not a parser for hand-written HTML.

/// One element: its start tag (`<a class="x" href="/y">`) and what is inside it ("" for a tag without content).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Element<'a> {
    pub start: &'a str,
    pub inner: &'a str,
}

/// Where the start tag that begins at `html[at..]` ends (the index after its `>`); a `>` inside a quoted value does
/// not end it.
fn tag_end(html: &str, at: usize) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (offset, byte) in html.as_bytes()[at..].iter().enumerate() {
        match (quote, byte) {
            (Some(open), b) if *b == open => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(*byte),
            (None, b'>') => return Some(at + offset + 1),
            (None, _) => {}
        }
    }
    None
}

/// Whether `html[at..]` starts with `<name` as a whole tag name (`<a ` and `<a>`, not `<abbr`).
fn opens(html: &str, at: usize, name: &str) -> bool {
    let rest = &html.as_bytes()[at..];
    rest.len() > name.len() + 1
        && rest[0] == b'<'
        && rest[1..=name.len()].eq_ignore_ascii_case(name.as_bytes())
        && matches!(rest[name.len() + 1], b' ' | b'>' | b'/' | b'\n' | b'\t' | b'\r')
}

/// Whether `html[at..]` starts with the end tag `close` (`</li`) as a whole name (`</li>`, not `</line>`).
fn closes(html: &str, at: usize, close: &str) -> bool {
    let rest = &html.as_bytes()[at..];
    rest.len() > close.len() && rest[..close.len()].eq_ignore_ascii_case(close.as_bytes()) && (rest[close.len()] == b'>' || rest[close.len()].is_ascii_whitespace())
}

/// Every `<name>` element whose class list has `class` ("" for any), in document order, nested ones included.
pub fn elements<'a>(html: &'a str, name: &str, class: &str) -> Vec<Element<'a>> {
    let close = format!("</{name}");
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = html[at..].find('<') {
        let open = at + offset;
        at = open + 1;
        if !opens(html, open, name) {
            continue;
        }
        let Some(start_end) = tag_end(html, open) else { break };
        let start = &html[open..start_end];
        if !class.is_empty() && !attr(start, "class").is_some_and(|classes| classes.split_whitespace().any(|c| c == class)) {
            continue;
        }
        // the matching end tag: count the same-name tags opened and closed after the start tag
        let (mut depth, mut scan, mut inner_end) = (1_usize, start_end, None);
        while let Some(offset) = html[scan..].find('<') {
            let tag = scan + offset;
            if opens(html, tag, name) {
                depth += 1;
            } else if closes(html, tag, &close) {
                depth -= 1;
                if depth == 0 {
                    inner_end = Some(tag);
                    break;
                }
            }
            scan = tag + 1;
        }
        // a tag that is never closed has no content (`<img>`)
        found.push(Element { start, inner: inner_end.map_or("", |end| &html[start_end..end]) });
    }
    found
}

/// The first such element.
pub fn first<'a>(html: &'a str, name: &str, class: &str) -> Option<Element<'a>> {
    elements(html, name, class).into_iter().next()
}

/// An attribute of a start tag, its entities decoded. `Some("")` for an attribute without a value (`disabled`).
pub fn attr(start: &str, name: &str) -> Option<String> {
    let bytes = start.as_bytes();
    // skip the tag name
    let mut at = start.find(|c: char| c.is_ascii_whitespace())?;
    while at < bytes.len() {
        while at < bytes.len() && (bytes[at].is_ascii_whitespace() || bytes[at] == b'/') {
            at += 1;
        }
        let name_start = at;
        while at < bytes.len() && !matches!(bytes[at], b'=' | b'>' | b'/') && !bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        let found = &start[name_start..at];
        if found.is_empty() {
            break;
        }
        let mut value: &str = "";
        if at < bytes.len() && bytes[at] == b'=' {
            at += 1;
            if at < bytes.len() && matches!(bytes[at], b'"' | b'\'') {
                let quote = bytes[at];
                let value_start = at + 1;
                at = value_start + start[value_start..].bytes().position(|b| b == quote)?;
                value = &start[value_start..at];
                at += 1;
            } else {
                let value_start = at;
                while at < bytes.len() && bytes[at] != b'>' && !bytes[at].is_ascii_whitespace() {
                    at += 1;
                }
                value = &start[value_start..at];
            }
        }
        if found.eq_ignore_ascii_case(name) {
            return Some(decode(value));
        }
    }
    None
}

/// The text inside an element: tags dropped, entities decoded, runs of white space made one space.
pub fn text(inner: &str) -> String {
    let mut plain = String::new();
    let mut rest = inner;
    while let Some(open) = rest.find('<') {
        plain.push_str(&rest[..open]);
        plain.push(' ');
        match tag_end(rest, open) {
            Some(end) => rest = &rest[end..],
            None => {
                rest = "";
                break;
            }
        }
    }
    plain.push_str(rest);
    decode(&plain).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The entities a browser writes (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&nbsp;`) and the numeric ones.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest[..rest.len().min(12)].find(';');
        let decoded = end.and_then(|end| match &rest[1..end] {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            code => {
                let number = code.strip_prefix('#')?;
                let value = match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                    None => number.parse().ok()?,
                };
                char::from_u32(value)
            }
        });
        match (decoded, end) {
            (Some(c), Some(end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"<ul class="list big"><li class="item"><a class="link main" href="/a?x=1&amp;y=2" title="a > b">One <b>&lt;1&gt;</b>
  &amp; more</a><div class="item-note"><div>deep</div></div></li><li class="item last"><img class="pic" src="/p.jpg" alt=""><button class="go" disabled="">Go</button><abbr class="item">no</abbr></li></ul>"#;

    #[test]
    fn elements_are_found_by_tag_and_class_token() {
        let items = elements(HTML, "li", "item");
        assert_eq!(items.len(), 2);
        assert!(items[0].inner.starts_with("<a class=\"link main\"") && items[0].inner.ends_with("</div></div>"));
        assert_eq!(elements(HTML, "li", "last").len(), 1);
        // a class is a whole token, and a tag name is a whole name
        assert_eq!(elements(HTML, "div", "item").len(), 0);
        assert_eq!(elements(HTML, "a", "item").len(), 0);
        assert_eq!(elements(HTML, "div", "").len(), 2);
        let note = first(HTML, "div", "item-note").unwrap();
        assert_eq!(note.inner, "<div>deep</div>");
        assert_eq!(first(HTML, "img", "pic").unwrap().inner, "");
        assert!(first(HTML, "table", "").is_none());
        // an end tag is a whole name too: `</line>` does not close an `<li>`
        let svg = "<li class=\"row\"><svg><line x1=\"0\"></line></svg><span class=\"t\">late</span></li>";
        assert!(first(svg, "li", "row").unwrap().inner.ends_with("<span class=\"t\">late</span>"));
    }

    #[test]
    fn attributes_and_text_are_decoded() {
        let link = first(HTML, "a", "link").unwrap();
        assert_eq!(attr(link.start, "href").as_deref(), Some("/a?x=1&y=2"));
        assert_eq!(attr(link.start, "title").as_deref(), Some("a > b"));
        assert_eq!(attr(link.start, "id"), None);
        assert_eq!(text(link.inner), "One <1> & more");
        let button = first(HTML, "button", "go").unwrap();
        assert_eq!(attr(button.start, "disabled").as_deref(), Some(""));
        assert_eq!(attr("<button class=go disabled>", "disabled").as_deref(), Some(""));
        assert_eq!(attr("<button class=go disabled>", "class").as_deref(), Some("go"));
        assert_eq!(text("caf&#233; &#x1F600; &unknown; a&b"), "café 😀 &unknown; a&b");
    }
}
