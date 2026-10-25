//! A [`Host`] for `cargo test`: it answers requests from a function (usually with saved pages from `fixtures/`), keeps
//! preferences in memory and records what was sent.

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
