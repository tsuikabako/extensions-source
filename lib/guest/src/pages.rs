//! One web page read through the app's hidden browser page, for a site that only fills its pages from a script of its
//! own (so a plain request gets an empty shell). The app opens the address in a real web engine, its own script does
//! the steps named here, and the HTML the read steps found comes back as text for the source to parse.
//!
//! A source names steps from a fixed set and passes no script. The address must be https and on a host the manifest
//! lists; the page itself can ask those hosts only, and loads no image, media file or font (an `<img>` still has its
//! address in the HTML). A site that answers with a check the page does not clear by itself fails the read: the app
//! answers no check.
//!
//! A source that reads pages ends with `kuro_guest::plugin!(MySite, pages);` so the plug-in imports the app's
//! `pages` interface. It needs an app that has it (the app's `docs/source-abi.md`, "Page reads").
//!
//! ```ignore
//! let page = PageRead::new("https://example.org/title/1?page=2").wait("ul.chapters > li").read("ul.chapters").send(host)?;
//! let chapters_html = page.html();
//! ```

use serde::{Deserialize, Serialize};

use crate::{Host, Res};

/// The error of [`Host::page_read`] where the app (or the test host) reads no pages.
pub const NOT_OFFERED: &str = "page reads are not offered here";

/// One thing the app's script does in the page. Each takes a CSS selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Step {
    /// Waits until an element matches.
    Wait(String),
    /// Waits until no element matches (a loading mark, or "a slot without its picture").
    WaitGone(String),
    /// Brings each matching element into view in turn, as a reader who scrolls down would.
    Scroll(String),
    /// Hands back the HTML of every matching element, joined by line breaks (`html` matches the whole page).
    Read(String),
}

/// A page to read: the address, the steps in order, and how long they may take (the app's default is 20 s and its
/// limit 60 s). A wait that is not met in time fails the read with the page's title and address in the error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRead {
    pub url: String,
    pub steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

impl PageRead {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into(), steps: Vec::new(), timeout_ms: None }
    }

    /// Waits until an element matches `selector`.
    pub fn wait(self, selector: impl Into<String>) -> Self {
        self.step(Step::Wait(selector.into()))
    }

    /// Waits until no element matches `selector`.
    pub fn wait_gone(self, selector: impl Into<String>) -> Self {
        self.step(Step::WaitGone(selector.into()))
    }

    /// Brings each element matching `selector` into view in turn.
    pub fn scroll(self, selector: impl Into<String>) -> Self {
        self.step(Step::Scroll(selector.into()))
    }

    /// Reads the HTML of every element matching `selector`; each read is one entry of [`Page::reads`].
    pub fn read(self, selector: impl Into<String>) -> Self {
        self.step(Step::Read(selector.into()))
    }

    /// How long the steps may take, in milliseconds.
    pub fn timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    fn step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }

    /// Has the app read the page. An `Err` is a short text: a refused address, a wait that was not met, a check the
    /// page did not clear, or an app without page reads.
    pub fn send(&self, host: &dyn Host) -> Res<Page> {
        let request = serde_json::to_string(self).map_err(|e| e.to_string())?;
        serde_json::from_str(&host.page_read(&request)?).map_err(|e| format!("invalid page reply: {e}"))
    }
}

/// What a read gives: the address and title the page ended on, and the HTML of each read step, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub reads: Vec<String>,
}

impl Page {
    /// The HTML of the first read step, "" when there was none.
    pub fn html(&self) -> &str {
        self.reads.first().map_or("", String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHost;
    use serde_json::json;

    #[test]
    fn a_read_is_the_json_the_app_reads() {
        let read = PageRead::new("https://example.org/c/1").wait("div.page").scroll("div.page").wait_gone("div.page:not(:has(img))").read("div.page img").timeout_ms(30_000);
        assert_eq!(
            serde_json::to_value(&read).unwrap(),
            json!({
                "url": "https://example.org/c/1",
                "steps": [{ "wait": "div.page" }, { "scroll": "div.page" }, { "waitGone": "div.page:not(:has(img))" }, { "read": "div.page img" }],
                "timeoutMs": 30000,
            })
        );
        assert!(serde_json::to_value(PageRead::new("u").read("html")).unwrap().get("timeoutMs").is_none(), "no time named: the app's default");
    }

    #[test]
    fn a_read_gives_the_page_or_the_apps_error() {
        let host = TestHost::default().reading(|read| match read.url.as_str() {
            "https://example.org/list" => Ok(Page { url: read.url.clone(), title: "List".into(), reads: vec!["<li>1</li>\n<li>2</li>".into()] }),
            _ => Err("nothing matched ul > li in time (the page is \"Security check\" at https://example.org/check)".into()),
        });
        let page = PageRead::new("https://example.org/list").wait("ul > li").read("ul > li").send(&host).unwrap();
        assert_eq!((page.title.as_str(), page.html()), ("List", "<li>1</li>\n<li>2</li>"));
        assert!(PageRead::new("https://example.org/other").read("html").send(&host).unwrap_err().contains("Security check"));
        assert_eq!(host.pages_read().iter().map(|read| read.url.as_str()).collect::<Vec<_>>(), ["https://example.org/list", "https://example.org/other"]);
        // a host that reads no pages says so
        assert_eq!(PageRead::new("https://example.org/list").read("html").send(&TestHost::default()), Err(NOT_OFFERED.to_owned()));
        assert_eq!(Page::default().html(), "");
    }
}
