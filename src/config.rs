use serde::Serialize;
use std::env;

// TRUST_PROXY as Express reads it: true, a number of hops, or addresses/names (loopback, 10.0.0.0/8)
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
    pub default_fd: i64,
    pub max_fd: i64,
    pub default_not_ready_seconds: i64,
    pub default_log_lines: i64,
    pub max_log_lines: i64,
    pub default_log_rate: i64,
    pub max_log_rate: i64,
    pub trust_proxy: TrustProxy,
    pub disk_dir: String,
}

// What JavaScript's Number() makes of a string, as far as an integer setting is concerned
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

// An integer from the environment, at least `min` (a positive one by default). A value that
// is not one is reported and replaced by the default, instead of being silently ignored.
fn positive_int(
    get: &impl Fn(&str) -> Option<String>,
    warn: &mut impl FnMut(String),
    name: &str,
    fallback: i64,
    min: i64,
) -> i64 {
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
        Config::read(|name| env::var(name).ok(), |message| eprintln!("{message}"))
    }

    // `get` reads a variable and `warn` reports a setting that was replaced, so tests can supply both
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
        let max_log_lines = int("MAX_LOG_LINES", 100000);
        let max_log_rate = int("MAX_LOG_RATE", 10000);
        let start_delay_seconds = positive_int(&get, &mut warn, "START_DELAY_SECONDS", 0, 0);

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
            default_fd: 100,
            max_fd,
            default_not_ready_seconds: 30,
            default_log_lines: 1000,
            max_log_lines,
            default_log_rate: 100,
            max_log_rate,
            trust_proxy: trust_proxy(&get),
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
        let (config, warnings) = read(&[("MAX_NUMBER", "abc"), ("MAX_DELAY_MS", "0"), ("MAX_HOLD_SECONDS", "12"), ("START_DELAY_SECONDS", "-1")]);
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
    fn numbers_are_read_like_javascript_does() {
        let (config, warnings) = read(&[("PORT", " 8080 "), ("MAX_FD", "1e3"), ("MAX_LOG_RATE", "2.5")]);
        assert_eq!((config.port, config.max_fd, config.max_log_rate), (8080, 1000, 10000));
        assert_eq!(warnings.len(), 1);
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
    fn the_token_is_never_serialized() {
        let (config, _) = read(&[("TOKEN", "s3cret")]);
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("s3cret") && !json.contains("token"));
        assert!(json.contains("\"maxDelayMs\":60000"));
        assert!(json.contains("\"trustProxy\":false"));
    }
}
