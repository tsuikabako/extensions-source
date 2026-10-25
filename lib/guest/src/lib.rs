//! The toolkit every Tsuikabako source is built on. It holds the part that is the same in every source, so a source's
//! own code is only "how to read this one website":
//!
//! - [`plugin!`]: the glue to the app (the WIT bindings and the exported `invoke`), one line per source.
//! - [`Source`] and [`invoke`]: the ops `info`, `create-sources` and `request-headers` answered the same way everywhere,
//!   every other op handed to the source with its parsed [`Input`].
//! - [`Host`]: what the app lends a source (network, clock, saved preferences, log). A source only reaches the app
//!   through it, so its ops also run in plain `cargo test` with a [`testing::TestHost`].
//! - [`http`]: one request type and one response type, with the same error texts in every source.
//! - [`wire`]: the JSON shapes the app reads (manga, chapters, pages, anime, episodes, hosters, videos).
//! - [`html`] (feature `html`): small CSS-selector helpers.
//!
//! The contract with the app is `wit/kuro-source.wit` at the repository root and `docs/source-abi.md` in the app's
//! repository.

pub mod http;
pub mod testing;
pub mod wire;

#[cfg(feature = "html")]
pub mod html;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub use http::{Request, Response};

/// The result of every op and helper: an error is a short text the app shows.
pub type Res<T> = Result<T, String>;

/// What the app lends a source. In the built plug-in these are the WIT imports (see [`plugin!`]); in tests,
/// [`testing::TestHost`].
pub trait Host {
    /// One HTTP request as JSON (see [`Request`]); the answer is a [`Response`] as JSON. The app refuses any host the
    /// source's manifest does not list.
    fn http(&self, request_json: &str) -> Res<String>;
    /// The clock, in whole seconds since 1970. For dates only ("2 days ago").
    fn unix_seconds(&self) -> u64;
    /// A saved preference as JSON text, "" when unset.
    fn pref_get(&self, key: &str) -> String;
    /// Saves a preference; the value is JSON text.
    fn pref_set(&self, key: &str, value_json: &str) -> Res<()>;
    /// Writes to the app's log: 0 debug, 1 info, 2 warning, 3 error.
    fn log(&self, level: u8, message: &str);
}

/// Reads a saved preference, `None` when it is unset or not a `T`.
pub fn read_pref<T: DeserializeOwned>(host: &dyn Host, key: &str) -> Option<T> {
    serde_json::from_str(&host.pref_get(key)).ok()
}

/// Saves a preference. A failure is logged, not returned: a preference is a cache, never needed to answer.
pub fn write_pref<T: Serialize>(host: &dyn Host, key: &str, value: &T) {
    let saved = serde_json::to_string(value).map_err(|e| e.to_string()).and_then(|text| host.pref_set(key, &text));
    if let Err(error) = saved {
        host.log(2, &format!("the preference {key} could not be saved: {error}"));
    }
}

/// Who a source is. The id must never change: the library, history and backups name it.
#[derive(Debug, Clone, PartialEq)]
pub struct Info {
    pub id: i64,
    pub name: &'static str,
    pub lang: &'static str,
    pub supports_latest: bool,
    /// The least gap between two requests to one host, which the app keeps. `None`: no spacing.
    pub request_gap_ms: Option<u64>,
}

impl Info {
    fn to_json(&self) -> Value {
        let mut info = json!({ "id": self.id, "name": self.name, "lang": self.lang, "supportsLatest": self.supports_latest });
        if let Some(gap) = self.request_gap_ms {
            info["requestGapMs"] = json!(gap);
        }
        info
    }
}

/// One source. [`invoke`] answers `info`, `create-sources` and `request-headers` from it and hands every other op to
/// [`Source::run`].
pub trait Source {
    fn info(&self) -> Info;

    /// The headers the app sends with this source's page images and videos (they replace the app's default set).
    /// `None` keeps the app's default.
    fn request_headers(&self) -> Option<Vec<(String, String)>> {
        None
    }

    /// Answers one op: `popular`, `latest`, `search`, `details`, `chapters`, `pages` for manga; `popular`, `latest`,
    /// `search`, `details`, `episodes`, `hosters`, `videos`, `subtitles` for anime. An op the source does not offer
    /// answers `Err(unknown_op(op))`.
    fn run(&self, op: &str, input: &Input, host: &dyn Host) -> Res<Value>;
}

/// The error of an op a source does not offer. The app reads this exact text as "not offered".
pub fn unknown_op(op: &str) -> String {
    format!("unknown op: {op}")
}

/// Turns any answer into the JSON value an op returns.
pub fn to_value<T: Serialize>(value: T) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// Answers one call from the app. The built plug-in calls this from [`plugin!`]; tests call it directly.
pub fn invoke(source: &dyn Source, host: &dyn Host, op: &str, input_json: &str) -> Res<String> {
    let out = match op {
        "info" => source.info().to_json(),
        "create-sources" => json!([source.info().to_json()]),
        "request-headers" => match source.request_headers() {
            Some(headers) => to_value(headers)?,
            None => return Err(unknown_op(op)),
        },
        _ => source.run(op, &Input::parse(op, input_json)?, host)?,
    };
    Ok(out.to_string())
}

/// What the app sends with an op: `{"sourceId", "showAdult", "args": {...}}`.
#[derive(Debug, Clone)]
pub struct Input {
    op: String,
    /// The user's "Show adult (18+) titles" switch. A source that knows a title's rating leaves adult titles out of
    /// its lists while this is `false`.
    pub show_adult: bool,
    args: Value,
}

impl Input {
    pub fn parse(op: &str, input_json: &str) -> Res<Self> {
        let envelope: Value = serde_json::from_str(input_json).map_err(|e| format!("invalid input: {e}"))?;
        Ok(Self {
            op: op.to_owned(),
            show_adult: envelope["showAdult"].as_bool().unwrap_or(false),
            args: envelope.get("args").cloned().filter(Value::is_object).unwrap_or_else(|| json!({})),
        })
    }

    /// The page of a list, from 1 (0 when the app sent none).
    pub fn page(&self) -> i32 {
        self.args["page"].as_i64().unwrap_or(0) as i32
    }

    /// The search text, "" when none.
    pub fn query(&self) -> &str {
        self.args["query"].as_str().unwrap_or("")
    }

    /// The search filters (ABI 1.5), empty when the app holds none.
    pub fn filters(&self) -> &[Value] {
        self.args["filters"].as_array().map_or(&[], Vec::as_slice)
    }

    /// The stored address of `args.<key>` (`manga`, `chapter`, `anime`, `episode`).
    pub fn url(&self, key: &str) -> Res<&str> {
        self.args[key]["url"].as_str().ok_or_else(|| format!("{} needs args.{key}", self.op))
    }

    /// `args.<key>` as a `T`, `None` when it is absent or null.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Res<Option<T>> {
        match self.args.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => T::deserialize(value).map(Some).map_err(|e| format!("invalid args.{key}: {e}")),
        }
    }

    /// `args.<key>` as a `T`; an error when it is absent.
    pub fn require<T: DeserializeOwned>(&self, key: &str) -> Res<T> {
        self.get(key)?.ok_or_else(|| format!("{} needs args.{key}", self.op))
    }

    /// All of `args` as a `T`.
    pub fn args<T: DeserializeOwned>(&self) -> Res<T> {
        T::deserialize(&self.args).map_err(|e| format!("invalid args: {e}"))
    }
}

/// The glue between a source and the app: the WIT bindings, a [`Host`] made of the WIT imports, and the exported
/// `invoke`. One line at the end of a source's `lib.rs`:
///
/// ```ignore
/// kuro_guest::plugin!(MySite);
/// ```
///
/// The argument is a value whose type implements [`Source`]. The WIT file is `wit/` at the repository root, two folders
/// up from `sources/<id>/`; a crate elsewhere passes its own path: `kuro_guest::plugin!(MySite, "../wit")`.
#[macro_export]
macro_rules! plugin {
    ($source:expr) => {
        $crate::plugin!($source, "../../wit");
    };
    ($source:expr, $wit:literal) => {
        wit_bindgen::generate!({
            path: $wit,
            world: "source",
        });

        /// The app's imports, lent to the source.
        struct WitHost;

        impl $crate::Host for WitHost {
            fn http(&self, request_json: &str) -> $crate::Res<String> {
                kuro::source::host::http(request_json)
            }
            fn unix_seconds(&self) -> u64 {
                kuro::source::host::unix_seconds()
            }
            fn pref_get(&self, key: &str) -> String {
                kuro::source::host::pref_get(key)
            }
            fn pref_set(&self, key: &str, value_json: &str) -> $crate::Res<()> {
                kuro::source::host::pref_set(key, value_json)
            }
            fn log(&self, level: u8, message: &str) {
                kuro::source::host::log(level, message)
            }
        }

        struct Plugin;

        impl Guest for Plugin {
            fn invoke(op: String, input_json: String) -> Result<String, String> {
                $crate::invoke(&$source, &WitHost, &op, &input_json)
            }
        }

        export!(Plugin);
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHost;

    struct Example;

    impl Source for Example {
        fn info(&self) -> Info {
            Info { id: 7, name: "Example", lang: "en", supports_latest: false, request_gap_ms: Some(500) }
        }
        fn run(&self, op: &str, input: &Input, _host: &dyn Host) -> Res<Value> {
            match op {
                "popular" => Ok(json!({ "page": input.page(), "adult": input.show_adult })),
                "details" => Ok(json!(input.url("manga")?)),
                _ => Err(unknown_op(op)),
            }
        }
    }

    fn call(op: &str, input: &str) -> Res<Value> {
        invoke(&Example, &TestHost::default(), op, input).map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn info_and_create_sources_describe_the_source() {
        let info = json!({ "id": 7, "name": "Example", "lang": "en", "supportsLatest": false, "requestGapMs": 500 });
        assert_eq!(call("info", "{}").unwrap(), info);
        assert_eq!(call("create-sources", "{}").unwrap(), json!([info]));
        let no_gap = Info { request_gap_ms: None, ..Example.info() };
        assert!(no_gap.to_json().get("requestGapMs").is_none());
    }

    #[test]
    fn ops_get_their_input_and_unknown_ops_say_so() {
        assert_eq!(call("popular", r#"{"showAdult":true,"args":{"page":3}}"#).unwrap(), json!({ "page": 3, "adult": true }));
        assert_eq!(call("details", r#"{"args":{"manga":{"url":"/m/1"}}}"#).unwrap(), json!("/m/1"));
        assert_eq!(call("details", r#"{"args":{}}"#), Err("details needs args.manga".to_owned()));
        assert_eq!(call("hosters", "{}"), Err("unknown op: hosters".to_owned()));
        assert_eq!(call("request-headers", "{}"), Err("unknown op: request-headers".to_owned()));
        assert!(call("popular", "not json").unwrap_err().starts_with("invalid input"));
    }

    #[test]
    fn input_reads_typed_args() {
        let input = Input::parse("videos", r#"{"args":{"query":"x","filters":[1],"hoster":{"hosterUrl":"u"},"episode":null}}"#).unwrap();
        assert_eq!((input.page(), input.query(), input.filters().len()), (0, "x", 1));
        let hoster: wire::Hoster = input.require("hoster").unwrap();
        assert_eq!(hoster.hoster_url, "u");
        assert_eq!(input.get::<wire::Episode>("episode").unwrap(), None);
        assert_eq!(input.require::<wire::Episode>("episode"), Err("videos needs args.episode".to_owned()));
    }

    #[test]
    fn preferences_round_trip() {
        let host = TestHost::default();
        assert_eq!(read_pref::<String>(&host, "mirror"), None);
        write_pref(&host, "mirror", &"b");
        assert_eq!(read_pref::<String>(&host, "mirror").as_deref(), Some("b"));
    }
}
