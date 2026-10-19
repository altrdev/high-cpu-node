//! What `/info` says, as far as the view needs it. The fields that older servers do not report are optional.
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
    pub fd: Option<Fd>,
    pub ready: Option<Ready>,
    pub log: Option<Log>,
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
    /// How many workers are loaded; a server that does not say loads all of them
    pub workers: Option<i64>,
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub held_mb: i64,
    pub limit_mb: i64,
    pub dir: String,
    pub free_mb: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct Fd {
    pub held: i64,
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ready {
    pub ready: bool,
    pub seconds_left: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct Log {
    pub sent: i64,
    pub lines: i64,
    pub rate: i64,
}

/// Asks the server that `base` points to how it is
pub fn fetch(base: &str) -> Result<Info, String> {
    let (status, body) = http::get(&http::info_url(base)?, Duration::from_secs(2))?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    serde_json::from_str(&body).map_err(|e| format!("not the /info of torchio: {e}"))
}
