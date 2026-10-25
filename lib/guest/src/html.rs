//! Small helpers for reading HTML pages with CSS selectors (the `scraper` crate, re-exported).

pub use scraper::{ElementRef, Html, Selector};

/// A selector from a CSS text that is known to be valid (a literal in the source's code).
pub fn sel(css: &str) -> Selector {
    Selector::parse(css).unwrap_or_else(|e| panic!("invalid selector {css:?}: {e}"))
}

/// The text of an element and its children, with runs of whitespace made one space.
pub fn text_of(element: ElementRef) -> String {
    element.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text directly inside an element, without its children's (Jsoup's `ownText`).
pub fn own_text(element: ElementRef) -> String {
    let text: String = element.children().filter_map(|node| node.value().as_text().map(|t| t.to_string())).collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The first element below `element` that matches `css`.
pub fn first<'a>(element: ElementRef<'a>, css: &str) -> Option<ElementRef<'a>> {
    element.select(&sel(css)).next()
}

/// An attribute of the first element below `element` that matches `css`, trimmed; `None` when absent or empty.
pub fn attr(element: ElementRef, css: &str, name: &str) -> Option<String> {
    first(element, css)?.value().attr(name).map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_read_text_and_attributes() {
        let doc = Html::parse_fragment(r#"<div><h1> Name <span>HOT</span></h1><a href=" /x "> a  b </a></div>"#);
        let root = doc.root_element();
        assert_eq!(own_text(first(root, "h1").unwrap()), "Name");
        assert_eq!(text_of(first(root, "h1").unwrap()), "Name HOT");
        assert_eq!(attr(root, "a", "href").as_deref(), Some("/x"));
        assert_eq!(attr(root, "a", "title"), None);
    }
}
