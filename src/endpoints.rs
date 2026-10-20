//! The definition of every endpoint: what it does, whether `TOKEN` protects it and the parameters
//! it takes with their defaults and limits. The routes read their parameters from `Specs` and
//! `/help` is built from `catalog`, so the two can't go out of date.
use crate::config::Config;
use crate::http::params::Param;
use serde_json::{Map, Value, json};

pub const STATUS_MIN: i64 = 200;
pub const STATUS_MAX: i64 = 599;
/// The lowest status that `/flaky` fails with: a failure is a 4xx or a 5xx
pub const FAILURE_MIN: i64 = 400;

type Specs1 = [(&'static str, Param); 1];
type Specs2 = [(&'static str, Param); 2];
type Specs3 = [(&'static str, Param); 3];

pub struct Specs {
    pub stress: Specs1,
    pub cpu: Specs3,
    pub delay: Specs1,
    pub flaky: Specs3,
    pub memory: Specs2,
    pub leak: Specs3,
    pub disk: Specs2,
    pub fd: Specs2,
    pub connections: Specs2,
    pub ready_off: Specs1,
    pub log: Specs2,
    pub crash: Specs2,
    pub block: Specs1,
    pub payload: Specs1,
}

pub fn specs(config: &Config) -> Specs {
    let hold = ("hold", Param::new(config.default_hold_seconds, 0, config.max_hold_seconds));
    Specs {
        stress: [("number", Param::new(config.default_number, 0, config.max_number))],
        cpu: [
            ("percent", Param::new(config.default_cpu_percent, 0, 100)),
            hold,
            ("workers", Param::new(config.workers, 1, config.workers)),
        ],
        delay: [("ms", Param::new(config.default_delay_ms, 0, config.max_delay_ms))],
        flaky: [
            ("rate", Param::new(50, 0, 100)),
            ("code", Param::new(503, FAILURE_MIN, STATUS_MAX)),
            ("ms", Param::new(0, 0, config.max_delay_ms)),
        ],
        memory: [("mb", Param::new(config.default_memory_mb, 1, config.max_memory_mb)), hold],
        leak: [
            ("rate", Param::new(config.default_leak_rate_mb, 1, config.max_memory_mb)),
            ("mb", Param::new(config.default_memory_mb, 1, config.max_memory_mb)),
            hold,
        ],
        disk: [("mb", Param::new(config.default_disk_mb, 1, config.max_disk_mb)), hold],
        fd: [("count", Param::new(config.default_fd, 1, config.max_fd)), hold],
        connections: [("count", Param::new(config.default_connections, 1, config.max_connections)), hold],
        ready_off: [("seconds", Param::new(config.default_not_ready_seconds, 0, config.max_hold_seconds))],
        log: [
            ("lines", Param::new(config.default_log_lines, 1, config.max_log_lines)),
            ("rate", Param::new(config.default_log_rate, 1, config.max_log_rate)),
        ],
        crash: [("code", Param::new(1, 0, 255)), ("after", Param::new(0, 0, config.max_hold_seconds))],
        payload: [("mb", Param::new(10.min(config.max_payload_mb), 1, config.max_payload_mb))],
        block: [("ms", Param::new(5000.min(config.max_block_ms), 1, config.max_block_ms))],
    }
}

pub struct Endpoint {
    pub path: &'static str,
    pub auth: bool,
    pub params: Vec<(&'static str, Param)>,
    // set for a parameter that is part of the path
    pub location: Option<&'static str>,
    pub description: &'static str,
}

fn endpoint(path: &'static str, auth: bool, params: &[(&'static str, Param)], description: &'static str) -> Endpoint {
    Endpoint { path, auth, params: params.to_vec(), location: None, description }
}

pub fn catalog(config: &Config) -> Vec<Endpoint> {
    let s = specs(config);
    let mut list = vec![
        endpoint("/", false, &[], "Health check, returns \"Status: OK\""),
        endpoint("/stress", true, &s.stress, "Runs a Fibonacci calculation in a worker, to keep a core busy"),
        endpoint(
            "/cpu",
            true,
            &s.cpu,
            "Keeps the workers (or the first ones, with workers) busy for a share of the time, percent=0 stops it",
        ),
        endpoint("/delay", false, &s.delay, "Answers after a delay, without blocking the server"),
        Endpoint {
            path: "/status/:code",
            auth: false,
            params: vec![("code", Param { fallback: None, min: STATUS_MIN, max: STATUS_MAX })],
            location: Some("path"),
            description: "Answers with the given HTTP status",
        },
        endpoint("/flaky", false, &s.flaky, "Fails rate percent of the requests with code, and answers 200 to the rest"),
        endpoint("/memory", true, &s.memory, "Allocates memory (MB) and holds it"),
        endpoint("/leak", true, &s.leak, "Memory that grows by itself, rate MB each second up to mb MB"),
        endpoint("/disk", true, &s.disk, "Writes a file (MB) in DISK_DIR and keeps it"),
        endpoint("/fd", true, &s.fd, "Opens file descriptors and holds them"),
        endpoint("/connections", true, &s.connections, "Opens TCP connections to the server itself and holds them"),
        endpoint("/ready", false, &[], "Readiness check: 200, or 503 after /ready/off"),
        endpoint("/ready/off", true, &s.ready_off, "Makes /ready answer 503 for seconds (0 until /ready/on)"),
        endpoint("/ready/on", true, &[], "Makes /ready answer 200 again"),
        endpoint("/crash", true, &s.crash, "Ends the process with the exit code, after the given seconds"),
        endpoint("/block", true, &s.block, "Freezes every thread of the server for ms, so it accepts and answers nothing"),
        endpoint("/oom", true, &[], "Allocates memory until the kernel kills the process, needs ALLOW_OOM=1"),
        endpoint(
            "/payload",
            true,
            &s.payload,
            "GET answers with mb MB of data, streamed; POST takes an upload, discards it and reports its size and time",
        ),
        endpoint("/scenario", true, &[], "POST a list of steps to run one after the other, GET shows where it is, DELETE stops it"),
        endpoint("/log", true, &s.log, "Writes lines to stdout at rate lines/s, then stops"),
        endpoint("/echo", false, &[], "The request as the server sees it: headers, client address, protocol (credentials masked)"),
        endpoint("/info", false, &[], "JSON with what the server is doing and the remaining budgets"),
        endpoint("/metrics", false, &[], "The same figures in the Prometheus text format"),
        endpoint("/help", false, &[], "This list"),
        endpoint("/config", false, &[], "The effective configuration, without the token"),
    ];
    if config.ui {
        let at = list.iter().position(|e| e.path == "/info").expect("/info is in the catalog");
        list.insert(at, endpoint("/ui", false, &[], "Control panel in the browser"));
    }
    list
}

fn param_json(param: &Param, location: Option<&str>) -> Value {
    let mut map = Map::new();
    if let Some(fallback) = param.fallback {
        map.insert("fallback".into(), json!(fallback));
    }
    map.insert("min".into(), json!(param.min));
    map.insert("max".into(), json!(param.max));
    if let Some(location) = location {
        map.insert("in".into(), json!(location));
    }
    Value::Object(map)
}

pub fn to_json(endpoints: &[Endpoint]) -> Value {
    endpoints
        .iter()
        .map(|e| {
            let mut map = Map::new();
            map.insert("path".into(), json!(e.path));
            map.insert("auth".into(), json!(e.auth));
            if !e.params.is_empty() {
                let params = e.params.iter().map(|(name, param)| (name.to_string(), param_json(param, e.location)));
                map.insert("params".into(), Value::Object(params.collect()));
            }
            map.insert("description".into(), json!(e.description));
            Value::Object(map)
        })
        .collect()
}

pub fn to_text(version: &str, endpoints: &[Endpoint]) -> String {
    let mut lines = vec![format!("torchio {version}"), String::new()];
    for e in endpoints {
        lines.push(format!("{}{}", e.path, if e.auth { "  [needs TOKEN]" } else { "" }));
        lines.push(format!("  {}", e.description));
        for (name, Param { fallback, min, max }) in &e.params {
            let default = fallback.map_or("required".to_string(), |n| format!("default {n}"));
            lines.push(format!("  {name}: {default}, {min}..{max}"));
        }
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(ui: bool) -> Config {
        Config::read(|name| (name == "UI" && ui).then(|| "1".to_string()), |_| {})
    }

    #[test]
    fn the_panel_is_listed_only_when_it_is_on() {
        assert!(!catalog(&config(false)).iter().any(|e| e.path == "/ui"));
        let on = catalog(&config(true));
        let paths: Vec<_> = on.iter().map(|e| e.path).collect();
        let ui = paths.iter().position(|p| *p == "/ui").unwrap();
        assert_eq!(paths[ui + 1], "/info");
    }

    #[test]
    fn the_path_parameter_is_marked_and_has_no_default() {
        let json = to_json(&catalog(&config(false)));
        let status = json.as_array().unwrap().iter().find(|e| e["path"] == "/status/:code").unwrap();
        assert_eq!(status["params"]["code"], json!({"min": 200, "max": 599, "in": "path"}));
        assert!(json[0].get("params").is_none(), "an endpoint without parameters has no params");
    }

    #[test]
    fn text_lists_the_defaults_and_limits() {
        let text = to_text("1.0.0", &catalog(&config(false)));
        assert!(text.starts_with("torchio 1.0.0\n\n/\n  Health check"));
        assert!(text.contains("/stress  [needs TOKEN]\n  Runs a Fibonacci"));
        assert!(text.contains("  number: default 45, 0..50\n"));
        assert!(text.contains("  code: required, 200..599\n"));
    }

    #[test]
    fn paths_are_unique_and_start_with_a_slash() {
        let catalog = catalog(&config(true));
        let mut paths: Vec<&str> = catalog.iter().map(|e| e.path).collect();
        assert!(paths.iter().all(|path| path.starts_with('/')));
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), catalog.len());
    }

    #[test]
    fn the_endpoints_that_change_the_state_are_the_protected_ones() {
        let protected: Vec<&str> = catalog(&config(false)).iter().filter(|e| e.auth).map(|e| e.path).collect();
        assert_eq!(
            protected,
            [
                "/stress",
                "/cpu",
                "/memory",
                "/leak",
                "/disk",
                "/fd",
                "/connections",
                "/ready/off",
                "/ready/on",
                "/crash",
                "/block",
                "/oom",
                "/payload",
                "/scenario",
                "/log"
            ]
        );
    }

    #[test]
    fn every_parameter_has_a_default_inside_its_range_but_the_code_of_status() {
        for endpoint in catalog(&config(false)) {
            for (name, param) in &endpoint.params {
                match param.fallback {
                    Some(fallback) => assert!((param.min..=param.max).contains(&fallback), "{} {name}", endpoint.path),
                    None => assert_eq!(endpoint.path, "/status/:code"),
                }
            }
        }
    }

    #[test]
    fn the_limits_follow_the_configuration() {
        let get = |name: &str| match name {
            "MAX_NUMBER" => Some("30".to_string()),
            "MAX_HOLD_SECONDS" => Some("60".to_string()),
            "MAX_LOG_RATE" => Some("500".to_string()),
            _ => None,
        };
        let specs = specs(&Config::read(get, |_| {}));
        assert_eq!(specs.stress[0].1.max, 30);
        assert_eq!((specs.cpu[1].0, specs.cpu[1].1.max), ("hold", 60));
        assert_eq!(specs.log[1].1.max, 500);
        assert_eq!(specs.crash[1].1.max, 60, "`after` is limited by the same hold");
        assert_eq!(specs.ready_off[0].1.max, 60);
    }
}
