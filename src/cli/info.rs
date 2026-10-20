//! What `/info` says, as far as the view needs it. A field is optional only when the server reports it
//! as `null` at times (nothing held, no limit); a server too old to have the others is refused.
use super::http;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub name: String,
    pub version: String,
    pub uptime_seconds: i64,
    pub stress: Stress,
    pub delay: Delay,
    pub memory: Memory,
    pub disk: Disk,
    pub fd: Fd,
    pub connections: Connections,
    pub ready: Ready,
    pub log: Option<Log>,
    pub scenario: Scenario,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stress {
    pub workers: i64,
    pub in_flight: i64,
    pub per_worker: Vec<i64>,
    pub burn: Option<Burn>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Burn {
    pub percent: i64,
    pub seconds_left: i64,
    /// How many workers are loaded, the first ones
    pub workers: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Delay {
    pub in_flight: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub held_mb: i64,
    pub limit_mb: i64,
    pub process_rss_mb: i64,
    pub container_limit_mb: Option<i64>,
    /// Seconds until the last `/memory` or `/leak` lets go; a server that does not say, or holds nothing, has none
    pub seconds_left: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub held_mb: i64,
    pub limit_mb: i64,
    pub dir: String,
    pub free_mb: Option<i64>,
    pub seconds_left: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connections {
    pub held: i64,
    pub limit: i64,
    pub seconds_left: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fd {
    pub held: i64,
    pub limit: i64,
    pub process_limit: Option<i64>,
    pub seconds_left: Option<i64>,
}

impl Fd {
    /// What a bar should compare with: the budget of `/fd`, or the limit of the process when it is lower,
    /// which is where `EMFILE` comes first. The flag says it is the process
    pub fn ceiling(&self) -> (i64, bool) {
        match self.process_limit {
            Some(process) if process < self.limit => (process, true),
            _ => (self.limit, false),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ready {
    pub ready: bool,
    pub seconds_left: Option<i64>,
}

/// Where `/scenario` is: `idle` has nothing else, a running one has its step and the seconds left
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub state: String,
    pub step: Option<i64>,
    pub steps: Option<i64>,
    pub seconds_left: Option<i64>,
    pub remaining_seconds: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Log {
    pub sent: i64,
    pub lines: i64,
    pub rate: i64,
}

/// The oldest server whose `/info` has every field that `watch` reads (`/scenario` came with 4.5.0)
const OLDEST_SERVER: (u32, u32, u32) = (4, 5, 0);

/// `4.5.0` as numbers; `None` for anything that is not three of them
fn version_of(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.split('.').map(|part| part.parse().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

/// Why an answer is not an `/info` that `watch` can read: a server that is too old says so, with
/// its version, and anything else is described by the error
fn unreadable(body: &str, error: &serde_json::Error) -> String {
    let version = serde_json::from_str::<serde_json::Value>(body).ok().and_then(|value| value["version"].as_str().map(String::from));
    match version {
        Some(version) if version_of(&version).is_some_and(|found| found < OLDEST_SERVER) => {
            let (major, minor, patch) = OLDEST_SERVER;
            format!("This server is torchio {version}, and watch needs {major}.{minor}.{patch} or newer")
        }
        _ => format!("not the /info of torchio: {error}"),
    }
}

/// Why there is no `/info` to show
#[derive(Debug, PartialEq)]
pub enum FetchError {
    /// The server did not answer, or not with a success
    Unreachable(String),
    /// It answered, but with something `watch` can't read: not torchio, or too old
    Unreadable(String),
}

impl FetchError {
    /// What to tell about the server at `url`
    pub fn describe(&self, url: &str) -> String {
        match self {
            FetchError::Unreachable(reason) => format!("Cannot reach {url}: {reason}"),
            FetchError::Unreadable(reason) => format!("{url}: {reason}"),
        }
    }
}

/// Asks the server that `base` points to how it is
pub fn fetch(base: &str) -> Result<Info, FetchError> {
    let url = http::info_url(base).map_err(FetchError::Unreachable)?;
    let (status, body) = http::get(&url, Duration::from_secs(2)).map_err(FetchError::Unreachable)?;
    if !(200..300).contains(&status) {
        return Err(FetchError::Unreachable(format!("HTTP {status}")));
    }
    serde_json::from_str(&body).map_err(|e| FetchError::Unreadable(unreadable(&body, &e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_of(body: &str) -> String {
        let error = serde_json::from_str::<Info>(body).unwrap_err();
        unreadable(body, &error)
    }

    #[test]
    fn an_error_says_whether_the_server_was_reached() {
        assert_eq!(FetchError::Unreachable("HTTP 500".into()).describe("http://h:1"), "Cannot reach http://h:1: HTTP 500");
        assert_eq!(FetchError::Unreadable("too old".into()).describe("http://h:1"), "http://h:1: too old");
    }

    #[test]
    fn versions_are_three_numbers() {
        assert_eq!(version_of("4.5.0"), Some((4, 5, 0)));
        assert_eq!(version_of("4.10.2"), Some((4, 10, 2)));
        for text in ["", "4.5", "4.5.0.1", "4.x.0", "dev"] {
            assert_eq!(version_of(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_server_older_than_watch_needs_says_its_version() {
        assert_eq!(error_of(r#"{"name":"torchio","version":"4.2.1"}"#), "This server is torchio 4.2.1, and watch needs 4.5.0 or newer");
        assert_eq!(error_of(r#"{"version":"3.9.0"}"#), "This server is torchio 3.9.0, and watch needs 4.5.0 or newer");
    }

    #[test]
    fn anything_else_that_cannot_be_read_is_described_by_the_error() {
        // a current version with a missing field, a version we can't read, and no JSON at all
        assert!(error_of(r#"{"version":"4.6.1"}"#).starts_with("not the /info of torchio: missing field"));
        assert!(error_of(r#"{"version":"dev"}"#).starts_with("not the /info of torchio: "));
        assert!(error_of("<html>").starts_with("not the /info of torchio: "));
    }
}
