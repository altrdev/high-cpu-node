//! The configuration: every setting comes from an environment variable and has a default.
use crate::logging::LogFormat;
use serde::Serialize;
use std::env;

/// `TRUST_PROXY`: `true`, a number of hops, or addresses and names (`loopback`, `10.0.0.0/8`)
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum TrustProxy {
    Bool(bool),
    Hops(i64),
    List(String),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub name: &'static str,
    pub port: i64,
    pub host: &'static str,
    pub workers: i64,
    pub default_number: i64,
    pub max_number: i64,
    pub default_delay_ms: i64,
    pub max_delay_ms: i64,
    pub default_memory_mb: i64,
    pub max_memory_mb: i64,
    pub default_hold_seconds: i64,
    pub max_hold_seconds: i64,
    pub default_disk_mb: i64,
    pub max_disk_mb: i64,
    #[serde(skip)]
    pub token: String,
    pub ui: bool,
    pub default_cpu_percent: i64,
    pub default_leak_rate_mb: i64,
    pub start_delay_seconds: i64,
    pub shutdown_grace_seconds: i64,
    pub default_fd: i64,
    pub max_fd: i64,
    pub default_connections: i64,
    pub max_connections: i64,
    pub max_block_ms: i64,
    pub allow_oom: bool,
    pub default_not_ready_seconds: i64,
    pub default_log_lines: i64,
    pub max_log_lines: i64,
    pub default_log_rate: i64,
    pub max_log_rate: i64,
    pub trust_proxy: TrustProxy,
    pub disk_dir: String,
    pub log_format: LogFormat,
}

/// A number as it may be written in an environment variable: with spaces around it, in
/// hexadecimal (`0x10`) or with an exponent (`1e3`). An empty text is 0.
fn js_number(raw: &str) -> Option<f64> {
    let text = raw.trim();
    if text.is_empty() {
        return Some(0.0);
    }
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).ok().map(|n| n as f64);
    }
    // Rust also reads "inf" and "nan", which are not integers either way
    text.parse::<f64>().ok()
}

/// An integer from the environment, at least `min`. A value that is not one is reported and
/// replaced by the default, instead of being silently ignored.
fn positive_int(get: &impl Fn(&str) -> Option<String>, warn: &mut impl FnMut(String), name: &str, fallback: i64, min: i64) -> i64 {
    let Some(raw) = get(name).filter(|raw| !raw.is_empty()) else {
        return fallback;
    };
    match js_number(&raw) {
        Some(value) if value.is_finite() && value.fract() == 0.0 && value >= min as f64 => value as i64,
        _ => {
            let kind = if min == 0 { "a non-negative" } else { "a positive" };
            warn(format!("{name}={raw} is not {kind} integer, using {fallback}"));
            fallback
        }
    }
}

fn log_format(get: &impl Fn(&str) -> Option<String>, warn: &mut impl FnMut(String)) -> LogFormat {
    match get("LOG_FORMAT") {
        None => LogFormat::default(),
        Some(value) => LogFormat::parse(&value).unwrap_or_else(|| {
            warn(format!("LOG_FORMAT={value} is not text or json, using text"));
            LogFormat::default()
        }),
    }
}

fn trust_proxy(get: &impl Fn(&str) -> Option<String>) -> TrustProxy {
    match get("TRUST_PROXY").as_deref() {
        None | Some("") | Some("false") => TrustProxy::Bool(false),
        Some("true") => TrustProxy::Bool(true),
        Some(raw) if raw.bytes().all(|b| b.is_ascii_digit()) => {
            raw.parse().map(TrustProxy::Hops).unwrap_or_else(|_| TrustProxy::List(raw.to_string()))
        }
        Some(raw) => TrustProxy::List(raw.to_string()),
    }
}

fn temp_dir() -> String {
    let dir = env::temp_dir().to_string_lossy().into_owned();
    match dir.trim_end_matches('/') {
        "" => "/".to_string(),
        trimmed => trimmed.to_string(),
    }
}

impl Config {
    pub fn from_env() -> Config {
        // the format of the warnings is the one the variable asks for, even if it is not read yet
        let format = env::var("LOG_FORMAT").ok().and_then(|value| LogFormat::parse(&value)).unwrap_or_default();
        Config::read(|name| env::var(name).ok(), |message| format.warn(&message))
    }

    /// `get` reads a variable and `warn` reports a setting that was replaced, so tests can supply both
    pub fn read(get: impl Fn(&str) -> Option<String>, mut warn: impl FnMut(String)) -> Config {
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as i64;
        let mut int = |name: &str, fallback: i64| positive_int(&get, &mut warn, name, fallback, 1);
        let port = int("PORT", 3000);
        let workers = int("WORKERS", cores);
        let max_number = int("MAX_NUMBER", 50);
        let max_delay_ms = int("MAX_DELAY_MS", 60000);
        let max_memory_mb = int("MAX_MEMORY_MB", 512);
        let max_hold_seconds = int("MAX_HOLD_SECONDS", 300);
        let max_disk_mb = int("MAX_DISK_MB", 1024);
        let max_fd = int("MAX_FD", 10000);
        let max_connections = int("MAX_CONNECTIONS", 500);
        let max_block_ms = int("MAX_BLOCK_MS", 30000);
        let max_log_lines = int("MAX_LOG_LINES", 100000);
        let max_log_rate = int("MAX_LOG_RATE", 10000);
        let start_delay_seconds = positive_int(&get, &mut warn, "START_DELAY_SECONDS", 0, 0);
        let shutdown_grace_seconds = positive_int(&get, &mut warn, "SHUTDOWN_GRACE_SECONDS", 0, 0);

        Config {
            name: "torchio",
            port,
            host: "0.0.0.0",
            workers,
            default_number: 45,
            max_number,
            default_delay_ms: 1000,
            max_delay_ms,
            default_memory_mb: 100,
            max_memory_mb,
            default_hold_seconds: 10,
            max_hold_seconds,
            default_disk_mb: 100,
            max_disk_mb,
            token: get("TOKEN").unwrap_or_default(),
            ui: matches!(get("UI").as_deref(), Some("1") | Some("true")),
            default_cpu_percent: 50,
            default_leak_rate_mb: 10,
            start_delay_seconds,
            shutdown_grace_seconds,
            default_fd: 100,
            max_fd,
            default_connections: 10,
            max_connections,
            max_block_ms,
            allow_oom: matches!(get("ALLOW_OOM").as_deref(), Some("1") | Some("true")),
            default_not_ready_seconds: 30,
            default_log_lines: 1000,
            max_log_lines,
            default_log_rate: 100,
            max_log_rate,
            trust_proxy: trust_proxy(&get),
            log_format: log_format(&get, &mut warn),
            disk_dir: get("DISK_DIR").filter(|dir| !dir.is_empty()).unwrap_or_else(temp_dir),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn read(vars: &[(&str, &str)]) -> (Config, Vec<String>) {
        let vars: HashMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let mut warnings = Vec::new();
        let config = Config::read(|name| vars.get(name).cloned(), |message| warnings.push(message));
        (config, warnings)
    }

    #[test]
    fn defaults_without_environment() {
        let (config, warnings) = read(&[]);
        assert_eq!(config.port, 3000);
        assert_eq!(config.max_number, 50);
        assert_eq!(config.trust_proxy, TrustProxy::Bool(false));
        assert!(!config.ui);
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_value_that_is_not_a_positive_integer_is_reported_and_replaced() {
        let (config, warnings) =
            read(&[("MAX_NUMBER", "abc"), ("MAX_DELAY_MS", "0"), ("MAX_HOLD_SECONDS", "12"), ("START_DELAY_SECONDS", "-1")]);
        assert_eq!(config.max_number, 50);
        assert_eq!(config.max_delay_ms, 60000);
        assert_eq!(config.max_hold_seconds, 12);
        assert_eq!(config.start_delay_seconds, 0);
        assert_eq!(
            warnings,
            [
                "MAX_NUMBER=abc is not a positive integer, using 50",
                "MAX_DELAY_MS=0 is not a positive integer, using 60000",
                "START_DELAY_SECONDS=-1 is not a non-negative integer, using 0",
            ]
        );
    }

    #[test]
    fn numbers_may_have_spaces_an_exponent_or_be_hexadecimal() {
        let (config, warnings) = read(&[("PORT", " 8080 "), ("MAX_FD", "1e3"), ("MAX_LOG_RATE", "2.5")]);
        assert_eq!((config.port, config.max_fd, config.max_log_rate), (8080, 1000, 10000));
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn log_format_is_text_unless_json_is_asked() {
        assert_eq!(read(&[]).0.log_format, LogFormat::Text);
        assert_eq!(read(&[("LOG_FORMAT", "json")]).0.log_format, LogFormat::Json);
        let (config, warnings) = read(&[("LOG_FORMAT", "xml")]);
        assert_eq!(config.log_format, LogFormat::Text);
        assert_eq!(warnings, ["LOG_FORMAT=xml is not text or json, using text"]);
    }

    #[test]
    fn trust_proxy_forms() {
        let proxy = |value: &str| read(&[("TRUST_PROXY", value)]).0.trust_proxy;
        assert_eq!(proxy("true"), TrustProxy::Bool(true));
        assert_eq!(proxy("false"), TrustProxy::Bool(false));
        assert_eq!(proxy("2"), TrustProxy::Hops(2));
        assert_eq!(proxy("loopback, 10.0.0.0/8"), TrustProxy::List("loopback, 10.0.0.0/8".into()));
    }

    #[test]
    fn every_variable_is_read() {
        let (config, warnings) = read(&[
            ("PORT", "8080"),
            ("WORKERS", "3"),
            ("MAX_NUMBER", "40"),
            ("MAX_DELAY_MS", "1500"),
            ("MAX_MEMORY_MB", "256"),
            ("MAX_HOLD_SECONDS", "90"),
            ("MAX_DISK_MB", "128"),
            ("MAX_FD", "500"),
            ("MAX_LOG_LINES", "5000"),
            ("MAX_LOG_RATE", "250"),
            ("START_DELAY_SECONDS", "7"),
            ("TOKEN", "s3cret"),
            ("UI", "1"),
            ("DISK_DIR", "/data"),
            ("TRUST_PROXY", "2"),
        ]);
        assert!(warnings.is_empty());
        assert_eq!((config.port, config.workers, config.max_number, config.max_delay_ms), (8080, 3, 40, 1500));
        assert_eq!((config.max_memory_mb, config.max_hold_seconds, config.max_disk_mb, config.max_fd), (256, 90, 128, 500));
        assert_eq!((config.max_log_lines, config.max_log_rate, config.start_delay_seconds), (5000, 250, 7));
        assert_eq!(
            (config.token.as_str(), config.ui, config.disk_dir.as_str(), config.trust_proxy),
            ("s3cret", true, "/data", TrustProxy::Hops(2))
        );
    }

    #[test]
    fn the_panel_is_on_with_one_or_true_only() {
        let ui = |value: &str| read(&[("UI", value)]).0.ui;
        assert!(ui("1") && ui("true"));
        assert!(!ui("0") && !ui("false") && !ui("yes") && !ui("") && !ui("TRUE"));
    }

    #[test]
    fn empty_variables_count_as_unset() {
        let (config, warnings) = read(&[("PORT", ""), ("DISK_DIR", ""), ("TRUST_PROXY", ""), ("START_DELAY_SECONDS", "")]);
        assert_eq!((config.port, config.trust_proxy), (3000, TrustProxy::Bool(false)));
        assert!(!config.disk_dir.is_empty() && !config.disk_dir.ends_with('/') || config.disk_dir == "/");
        assert!(warnings.is_empty());
    }

    #[test]
    fn workers_default_to_the_cpus_of_the_process() {
        let cpus = std::thread::available_parallelism().unwrap().get() as i64;
        assert_eq!(read(&[]).0.workers, cpus);
    }

    #[test]
    fn numbers_that_are_not_whole_or_not_finite_are_replaced() {
        for value in ["1.5", "-3", "0", "inf", "NaN", "abc", "1,000", "0x", "--1", "1e400"] {
            let (config, warnings) = read(&[("PORT", value)]);
            assert_eq!(config.port, 3000, "{value}");
            assert_eq!(warnings.len(), 1, "{value}");
        }
    }

    #[test]
    fn the_token_is_never_serialized() {
        let (config, _) = read(&[("TOKEN", "s3cret")]);
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("s3cret") && !json.contains("token"));
        assert!(json.contains("\"maxDelayMs\":60000"));
        assert!(json.contains("\"trustProxy\":false"));
    }
}
