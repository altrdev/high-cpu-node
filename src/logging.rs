//! What the server writes about itself, in plain text or as one JSON object a line (`LOG_FORMAT`).
use crate::app::AppState;
use crate::resources::log::iso;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

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
