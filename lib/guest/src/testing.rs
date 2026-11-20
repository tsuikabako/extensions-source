//! Hosts for `cargo test`. [`TestHost`] answers requests from a function (usually with saved pages from `fixtures/`),
//! keeps preferences in memory and records what was sent. [`LiveHost`] sends real requests, for the `#[ignore]`d tests
//! that run a source against its live site.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::json;

use crate::{Host, Request, Res};

type Answer = Box<dyn Fn(&Request) -> (u16, String)>;

pub struct TestHost {
    answer: Answer,
    /// What `unix_seconds` returns.
    pub now: u64,
    prefs: RefCell<HashMap<String, String>>,
    sent: RefCell<Vec<Request>>,
}

impl Default for TestHost {
    /// A host that answers every request with 404.
    fn default() -> Self {
        Self::answering(|_| (404, String::new()))
    }
}

impl TestHost {
    /// A host that answers each request with `(status, body)` from `answer`.
    pub fn answering(answer: impl Fn(&Request) -> (u16, String) + 'static) -> Self {
        Self { answer: Box::new(answer), now: 0, prefs: RefCell::default(), sent: RefCell::default() }
    }

    /// A host that answers every request with this body.
    pub fn with_body(body: &str) -> Self {
        let body = body.to_owned();
        Self::answering(move |_| (200, body.clone()))
    }

    /// The requests sent so far, in order.
    pub fn sent(&self) -> Vec<Request> {
        self.sent.borrow().clone()
    }
}

#[derive(serde::Deserialize)]
struct Sent {
    method: String,
    url: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    body: Option<String>,
}

impl Host for TestHost {
    fn http(&self, request_json: &str) -> Res<String> {
        let sent: Sent = serde_json::from_str(request_json).map_err(|e| format!("invalid request: {e}"))?;
        let request = Request { method: sent.method, url: sent.url, headers: sent.headers, body: sent.body };
        let (status, body) = (self.answer)(&request);
        let url = request.url.clone();
        self.sent.borrow_mut().push(request);
        Ok(json!({ "status": status, "url": url, "headers": [], "body": body }).to_string())
    }

    fn unix_seconds(&self) -> u64 {
        self.now
    }

    fn pref_get(&self, key: &str) -> String {
        self.prefs.borrow().get(key).cloned().unwrap_or_default()
    }

    fn pref_set(&self, key: &str, value_json: &str) -> Res<()> {
        self.prefs.borrow_mut().insert(key.to_owned(), value_json.to_owned());
        Ok(())
    }

    fn log(&self, _level: u8, _message: &str) {}
}

/// The user agent [`LiveHost`] sends when a request names none: a desktop browser's, as the app's client does.
#[cfg(not(target_arch = "wasm32"))]
const BROWSER_UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";

/// A [`Host`] that sends real requests with the `curl` program, for `#[ignore]`d tests that run a source against its
/// live site (`cargo test -p kuro-<id> -- --ignored`). It keeps the manifest's `hosts` rule the way the app does, on
/// the address asked for and on the one the answer came from, so a host missing from the manifest fails here and not
/// in the app. Requests are spaced by `gap`; the clock is the real one.
#[cfg(not(target_arch = "wasm32"))]
pub struct LiveHost {
    hosts: Vec<String>,
    gap: std::time::Duration,
    prefs: RefCell<HashMap<String, String>>,
    sent: RefCell<Vec<Request>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl LiveHost {
    /// `hosts` is the manifest's `hosts` list (a name, or `*.suffix` for its subdomains).
    pub fn new(hosts: &[&str]) -> Self {
        Self { hosts: hosts.iter().map(|h| h.to_ascii_lowercase()).collect(), gap: std::time::Duration::from_millis(300), prefs: RefCell::default(), sent: RefCell::default() }
    }

    /// The requests sent so far, in order.
    pub fn sent(&self) -> Vec<Request> {
        self.sent.borrow().clone()
    }

    fn check(&self, url: &str) -> Res<()> {
        let host = host_of(url).ok_or_else(|| "host not allowed: (none)".to_owned())?;
        if self.hosts.iter().any(|entry| host_matches(entry, &host)) { Ok(()) } else { Err(format!("host not allowed: {host}")) }
    }
}

/// The app's allow-list rule (`check_host` in its `kuro-source-wasm` crate): a name matched exactly, or `*.suffix`
/// matching any subdomain of the suffix (not the bare suffix).
#[cfg(not(target_arch = "wasm32"))]
fn host_matches(entry: &str, host: &str) -> bool {
    match entry.strip_prefix("*.") {
        Some(suffix) => host.ends_with(&format!(".{suffix}")),
        None => host == entry,
    }
}

/// The host name of an http(s) address, lower case, without user, password or port.
#[cfg(not(target_arch = "wasm32"))]
fn host_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

#[cfg(not(target_arch = "wasm32"))]
impl Host for LiveHost {
    fn http(&self, request_json: &str) -> Res<String> {
        let sent: Sent = serde_json::from_str(request_json).map_err(|e| format!("invalid request: {e}"))?;
        let request = Request { method: sent.method, url: sent.url, headers: sent.headers, body: sent.body };
        self.check(&request.url)?;
        std::thread::sleep(self.gap);
        let mut curl = std::process::Command::new("curl");
        // the status and the final address come after the body, on a line of their own
        curl.args(["-sS", "-L", "--compressed", "--max-time", "60", "-X", &request.method, "-w", "\n%{http_code} %{url_effective}"]);
        if !request.headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("user-agent")) {
            curl.args(["-A", BROWSER_UA]);
        }
        for (name, value) in &request.headers {
            curl.arg("-H").arg(format!("{name}: {value}"));
        }
        if let Some(body) = &request.body {
            curl.arg("--data-binary").arg(body);
        }
        let out = curl.arg(&request.url).output().map_err(|e| format!("curl did not start: {e}"))?;
        if !out.status.success() {
            return Err(format!("curl failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        let text = String::from_utf8(out.stdout).map_err(|_| format!("{} sent a binary body", request.url))?;
        let (body, tail) = text.rsplit_once('\n').ok_or("curl gave no status line")?;
        let (status, url) = tail.split_once(' ').ok_or("curl gave no status line")?;
        let status: u16 = status.parse().map_err(|_| format!("curl gave the status {status:?}"))?;
        self.check(url)?;
        let answer = json!({ "status": status, "url": url, "headers": [], "body": body }).to_string();
        self.sent.borrow_mut().push(request);
        Ok(answer)
    }

    fn unix_seconds(&self) -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
    }

    fn pref_get(&self, key: &str) -> String {
        self.prefs.borrow().get(key).cloned().unwrap_or_default()
    }

    fn pref_set(&self, key: &str, value_json: &str) -> Res<()> {
        self.prefs.borrow_mut().insert(key.to_owned(), value_json.to_owned());
        Ok(())
    }

    fn log(&self, level: u8, message: &str) {
        eprintln!("[log {level}] {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_live_host_keeps_the_manifest_hosts_rule() {
        let host = LiveHost::new(&["Example.org", "*.cdn.test"]);
        assert_eq!(host.check("https://example.org/a?b"), Ok(()));
        assert_eq!(host.check("https://user:pw@EXAMPLE.org:8443/a"), Ok(()));
        assert_eq!(host.check("https://a.b.cdn.test"), Ok(()));
        assert_eq!(host.check("https://cdn.test/"), Err("host not allowed: cdn.test".into()));
        assert_eq!(host.check("https://example.org.evil.test/"), Err("host not allowed: example.org.evil.test".into()));
        assert_eq!(host.check("ftp://example.org/"), Err("host not allowed: (none)".into()));
        // a refused address is never sent
        assert_eq!(Request::get("https://other.test/").send(&host), Err("host not allowed: other.test".into()));
        assert!(host.sent().is_empty());
    }
}
