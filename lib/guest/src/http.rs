//! One HTTP request and its response, as the app's `http` import takes and gives them (JSON). The app sends the request
//! only to a host the source's manifest lists, follows redirects, and keeps the source's request gap.

use serde::{Deserialize, Serialize};

use crate::{Host, Res};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl Request {
    pub fn get(url: impl Into<String>) -> Self {
        Self { method: "GET".into(), url: url.into(), headers: Vec::new(), body: None }
    }

    pub fn post(url: impl Into<String>, body: impl Into<String>) -> Self {
        Self { method: "POST".into(), url: url.into(), headers: Vec::new(), body: Some(body.into()) }
    }

    /// A form POST (`application/x-www-form-urlencoded`), the body already encoded.
    pub fn form(url: impl Into<String>, body: impl Into<String>) -> Self {
        Self::post(url, body).header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn headers<N: Into<String>, V: Into<String>>(mut self, headers: impl IntoIterator<Item = (N, V)>) -> Self {
        self.headers.extend(headers.into_iter().map(|(name, value)| (name.into(), value.into())));
        self
    }

    /// Sends the request through the app. An `Err` means no answer at all (refused host, network down); any HTTP
    /// status is an `Ok` [`Response`].
    pub fn send(&self, host: &dyn Host) -> Res<Response> {
        let request = serde_json::to_string(self).map_err(|e| e.to_string())?;
        serde_json::from_str(&host.http(&request)?).map_err(|e| format!("invalid http reply: {e}"))
    }

    /// Sends the request and returns the body of a successful text answer; see [`Response::text`].
    pub fn text(&self, host: &dyn Host, site: &str) -> Res<String> {
        self.send(host)?.text(site)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub status: u16,
    /// The final address, after redirects.
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// The body as text, or base64 when `body_encoding` is set.
    pub body: String,
    /// `Some("base64")` for a body that is not UTF-8 text.
    #[serde(default)]
    pub body_encoding: Option<String>,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The first header of that name (case does not matter).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The body of a successful text answer. Otherwise the same error texts in every source: "<site> is limiting
    /// requests; ..." for 429, "HTTP <status> from <site>", "<site> sent a binary body".
    pub fn text(self, site: &str) -> Res<String> {
        if !self.is_success() {
            return Err(self.error(site));
        }
        if self.body_encoding.is_some() {
            return Err(format!("{site} sent a binary body"));
        }
        Ok(self.body)
    }

    /// The error text of an unsuccessful answer.
    pub fn error(&self, site: &str) -> String {
        match self.status {
            429 => format!("{site} is limiting requests; wait a few seconds and try again"),
            status => format!("HTTP {status} from {site}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHost;
    use serde_json::json;

    #[test]
    fn a_request_is_the_json_the_app_reads() {
        let request = Request::form("https://example.org/a", "x=1").header("Referer", "https://example.org/");
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "method": "POST",
                "url": "https://example.org/a",
                "headers": [["Content-Type", "application/x-www-form-urlencoded; charset=UTF-8"], ["Referer", "https://example.org/"]],
                "body": "x=1",
            })
        );
        assert_eq!(serde_json::to_value(Request::get("u")).unwrap()["body"], json!(null));
    }

    #[test]
    fn responses_give_text_or_one_kind_of_error() {
        let host = TestHost::answering(|request| match request.url.as_str() {
            "https://ok.test/" => (200, "hello".into()),
            "https://slow.test/" => (429, String::new()),
            _ => (404, String::new()),
        });
        assert_eq!(Request::get("https://ok.test/").text(&host, "Ok").unwrap(), "hello");
        assert_eq!(Request::get("https://slow.test/").text(&host, "Slow"), Err("Slow is limiting requests; wait a few seconds and try again".into()));
        assert_eq!(Request::get("https://gone.test/").text(&host, "Gone"), Err("HTTP 404 from Gone".into()));
        assert_eq!(host.sent().len(), 3);
        let binary = Response { status: 200, url: String::new(), headers: vec![("Retry-After".into(), "5".into())], body: "AA==".into(), body_encoding: Some("base64".into()) };
        assert_eq!(binary.header("retry-after"), Some("5"));
        assert_eq!(binary.text("Bin"), Err("Bin sent a binary body".into()));
    }
}
