//! What the server writes about itself, in plain text or as one JSON object a line (`LOG_FORMAT`).
use crate::app::AppState;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// `LOG_FORMAT`: `text`, the default, or `json`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }
}

impl LogFormat {
    /// `None` for a value that is neither `text` nor `json`
    pub fn parse(value: &str) -> Option<LogFormat> {
        match value.trim().to_ascii_lowercase().as_str() {
            "text" => Some(LogFormat::Text),
            "json" => Some(LogFormat::Json),
            _ => None,
        }
    }

    /// The line for `message`, without the newline. Text is the message alone; JSON adds the time
    /// and the level, then `fields`.
    pub fn render(self, level: Level, message: &str, fields: &[(&str, Value)], at: SystemTime) -> String {
        match self {
            LogFormat::Text => message.to_string(),
            LogFormat::Json => {
                let mut line = Map::new();
                line.insert("timestamp".into(), json!(iso(at)));
                line.insert("level".into(), json!(level.name()));
                line.insert("message".into(), json!(message));
                for (key, value) in fields {
                    line.insert((*key).to_string(), value.clone());
                }
                Value::Object(line).to_string()
            }
        }
    }

    /// Writes a line: warnings and errors to stderr, the rest to stdout
    pub fn log(self, level: Level, message: &str, fields: &[(&str, Value)]) {
        let line = self.render(level, message, fields, SystemTime::now());
        if level == Level::Info {
            println!("{line}");
        } else {
            eprintln!("{line}");
        }
    }

    pub fn info(self, message: &str) {
        self.log(Level::Info, message, &[]);
    }

    pub fn warn(self, message: &str) {
        self.log(Level::Warn, message, &[]);
    }

    pub fn error(self, message: &str) {
        self.log(Level::Error, message, &[]);
    }
}

/// `2026-10-19T12:34:56.789Z`: ISO 8601, in UTC, with milliseconds
pub fn iso(time: SystemTime) -> String {
    let since_epoch = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let days = (since_epoch.as_secs() / 86400) as i64;
    let seconds = since_epoch.as_secs() % 86400;
    // days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let day_of_era = z.rem_euclid(146097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60,
        since_epoch.subsec_millis()
    )
}

/// Goes around the whole router: with `LOG_FORMAT=json`, one line for each request answered, with
/// the method, the path (without the query, which can hold a token), the status and the time taken.
/// With the text format nothing is written, as before.
pub async fn access(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let format = state.config.log_format;
    if format != LogFormat::Json {
        return next.run(request).await;
    }
    let started = Instant::now();
    let (method, path) = (request.method().to_string(), request.uri().path().to_string());
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let fields = [
        ("method", json!(method)),
        ("path", json!(path)),
        ("status", json!(status)),
        ("duration_ms", json!((started.elapsed().as_secs_f64() * 1e6).round() / 1e3)),
    ];
    format.log(Level::Info, &format!("{method} {path} {status}"), &fields);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    const AT: SystemTime = UNIX_EPOCH;

    #[test]
    fn dates_are_iso_8601_in_utc_with_milliseconds() {
        let at = |ms: u64| iso(UNIX_EPOCH + Duration::from_millis(ms));
        assert_eq!(at(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(at(1_700_000_000_123), "2023-11-14T22:13:20.123Z");
        assert_eq!(at(951_782_400_000), "2000-02-29T00:00:00.000Z", "a leap day");
        assert_eq!(at(951_868_800_000), "2000-03-01T00:00:00.000Z");
        assert_eq!(at(4_102_444_799_999), "2099-12-31T23:59:59.999Z");
    }

    #[test]
    fn the_format_is_read_without_caring_for_case() {
        assert_eq!(LogFormat::parse("json"), Some(LogFormat::Json));
        assert_eq!(LogFormat::parse(" JSON "), Some(LogFormat::Json));
        assert_eq!(LogFormat::parse("text"), Some(LogFormat::Text));
        assert_eq!(LogFormat::parse("xml"), None);
        assert_eq!(LogFormat::parse(""), None);
    }

    #[test]
    fn text_is_the_message_alone() {
        let line = LogFormat::Text.render(Level::Warn, "something odd", &[("status", json!(200))], AT);
        assert_eq!(line, "something odd");
    }

    #[test]
    fn json_has_the_time_the_level_the_message_and_the_fields_in_order() {
        let fields = [("method", json!("GET")), ("status", json!(200))];
        let line = LogFormat::Json.render(Level::Info, "GET /", &fields, AT + Duration::from_millis(1500));
        assert_eq!(line, r#"{"timestamp":"1970-01-01T00:00:01.500Z","level":"info","message":"GET /","method":"GET","status":200}"#);
    }

    #[test]
    fn json_escapes_the_message_so_that_a_line_stays_a_line() {
        let line = LogFormat::Json.render(Level::Error, "a \"b\"\nc", &[], AT);
        assert!(!line.contains('\n'));
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["message"], "a \"b\"\nc");
        assert_eq!(parsed["level"], "error");
    }
}
