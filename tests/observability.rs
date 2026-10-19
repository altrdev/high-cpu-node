//! What the server says about itself: `/info`, `/metrics`, `/log`, `/echo`, `/help` and `/config`.
mod common;

use common::*;
use serde_json::{Value, json};
use std::collections::HashSet;

#[test]
fn info_describes_the_server() {
    let server = Server::start();
    let info = server.info();
    assert_eq!(info["name"], "torchio");
    assert_eq!(info["version"], VERSION);
    assert_eq!(info["auth"], false);
    assert_eq!(info["stress"]["workers"], 2);
    assert_eq!(info["stress"]["perWorker"], json!([0, 0]));
    assert!(info["stress"]["burn"].is_null());
    assert_eq!(info["memory"]["limitMb"], 300);
    assert!(info["memory"]["processRssMb"].as_i64().unwrap() > 0);
    assert_eq!(info["disk"]["limitMb"], 300);
    assert_eq!(info["disk"]["dir"], server.disk_dir.path().to_str().unwrap());
    assert!(info["disk"]["freeMb"].as_i64().unwrap() > 0);
    assert_eq!(info["fd"], json!({"held": 0, "limit": 10000}));
    assert!(info["log"].is_null());
    assert_eq!(
        info["limits"],
        json!({"maxNumber": 50, "maxDelayMs": 5000, "maxHoldSeconds": 30, "maxLogLines": 100000, "maxLogRate": 10000})
    );
}

#[test]
fn info_counts_the_uptime() {
    let server = Server::start();
    sleep(1100);
    assert!(server.info()["uptimeSeconds"].as_i64().unwrap() >= 1);
}

#[test]
fn info_follows_what_the_server_holds() {
    let server = Server::start();
    server.get("/memory?mb=100&hold=5");
    server.get("/disk?mb=10&hold=5");
    server.get("/fd?count=20&hold=5");
    let info = server.info();
    assert_eq!(info["memory"]["heldMb"], 100);
    assert!(info["memory"]["processRssMb"].as_i64().unwrap() >= 100);
    assert_eq!(info["disk"]["heldMb"], 10);
    assert_eq!(info["fd"]["held"], 20);
}

// Every sample must come after the TYPE of its metric, and be a name and a number
fn check_prometheus_text(body: &str) {
    assert!(body.ends_with('\n'));
    let mut declared = HashSet::new();
    for line in body.trim_end().lines() {
        if let Some(rest) = line.strip_prefix("# TYPE ") {
            let mut parts = rest.split(' ');
            declared.insert(parts.next().unwrap().to_string());
            assert!(matches!(parts.next(), Some("gauge" | "counter")), "{line}");
        } else if !line.starts_with("# HELP ") {
            let (series, value) = line.rsplit_once(' ').unwrap_or_else(|| panic!("not a sample: {line}"));
            value.parse::<f64>().unwrap_or_else(|_| panic!("not a number: {line}"));
            let name = series.split('{').next().unwrap();
            assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':'), "bad name: {line}");
            assert!(declared.contains(name), "no TYPE before {name}");
            if let Some(labels) = series.strip_prefix(name).filter(|l| !l.is_empty()) {
                assert!(labels.starts_with('{') && labels.ends_with('}'), "bad labels: {line}");
            }
        }
    }
}

#[test]
fn metrics_are_valid_prometheus_text() {
    let server = Server::start();
    let response = server.get("/metrics");
    assert_eq!(response.header("content-type"), Some("text/plain; version=0.0.4; charset=utf-8"));
    check_prometheus_text(&response.body);

    let body = response.body;
    assert_eq!(metric(&body, &format!(r#"torchio_info{{version="{VERSION}"}}"#)), Some(1.0));
    assert_eq!(metric(&body, "torchio_workers"), Some(2.0));
    assert_eq!(metric(&body, "torchio_memory_limit_bytes"), Some((300 * MB) as f64));
    assert_eq!(metric(&body, "torchio_disk_limit_bytes"), Some((300 * MB) as f64));
    assert_eq!(metric(&body, "torchio_fd_limit"), Some(10000.0));
    assert_eq!(metric(&body, "torchio_ready"), Some(1.0));
    assert!(metric(&body, "process_resident_memory_bytes").unwrap() > 0.0);
    assert!(metric(&body, "torchio_disk_free_bytes").unwrap() > 0.0);
    assert!(metric(&body, "torchio_start_time_seconds").unwrap() > 1.6e9);
    assert!(!body.contains("torchio_container_memory_limit_bytes") || cfg!(target_os = "linux"), "only a container has a limit");
}

#[test]
fn metrics_follow_the_server() {
    let server = Server::start();
    server.get("/memory?mb=100&hold=2");
    server.get("/disk?mb=10&hold=2");
    let body = server.text("/metrics");
    assert_eq!(metric(&body, "torchio_memory_held_bytes"), Some((100 * MB) as f64));
    assert_eq!(metric(&body, "torchio_disk_held_bytes"), Some((10 * MB) as f64));
    wait_for("the memory to be released", 4000, || server.info()["memory"]["heldMb"] == 0);
    assert_eq!(metric(&server.text("/metrics"), "torchio_memory_held_bytes"), Some(0.0));
}

#[test]
fn metrics_count_the_requests_by_route_and_code() {
    let server = Server::start();
    let count = |series: &str| metric(&server.text("/metrics"), series).unwrap_or(0.0);
    let teapot = r#"torchio_http_requests_total{route="/status/:code",code="418"}"#;
    let before = count(teapot);
    server.get("/status/418");
    server.get("/status/418");
    assert_eq!(count(teapot), before + 2.0);

    server.get("/nowhere");
    assert_eq!(count(r#"torchio_http_requests_total{route="other",code="404"}"#), 1.0);
    server.get("/delay?ms=99999");
    assert_eq!(count(r#"torchio_http_requests_total{route="/delay",code="400"}"#), 1.0);
}

#[test]
fn metrics_count_the_requests_that_the_token_rejected() {
    let server = Server::with(&[("TOKEN", "s3cret")]);
    server.get("/crash");
    assert_eq!(metric(&server.text("/metrics"), r#"torchio_http_requests_total{route="/crash",code="401"}"#), Some(1.0));
}

#[test]
fn log_writes_the_lines_at_the_given_rate_then_stops() {
    let server = Server::start();
    let started = std::time::Instant::now();
    let answer = server.text("/log?lines=100&rate=200");
    assert_eq!(answer, "Writing 100 lines at 200 lines/s (1 s)");

    wait_for("the lines", 5000, || logged(&server).len() >= 100);
    let lines = logged(&server);
    assert_eq!(lines.len(), 100);
    let first = &lines[0];
    assert!(first.ends_with(" torchio-log line 1 of 100"), "{first}");
    // 2026-10-18T14:00:00.000Z
    let stamp = first.split(' ').next().unwrap();
    assert_eq!(stamp.len(), 24, "{stamp}");
    assert!(stamp.ends_with('Z') && stamp.as_bytes()[10] == b'T' && stamp.as_bytes()[19] == b'.', "{stamp}");
    assert!(lines[99].ends_with("line 100 of 100"));
    assert!(started.elapsed().as_millis() >= 400, "the rate is respected");

    sleep(100);
    assert!(server.info()["log"].is_null());
    assert_eq!(logged(&server).len(), 100, "nothing after the last line");
}

#[test]
fn log_is_in_info_while_it_writes_and_a_new_request_replaces_it() {
    let server = Server::start();
    server.get("/log?lines=1000&rate=10");
    sleep(300);
    let log = server.info()["log"].clone();
    assert_eq!((log["lines"].as_i64(), log["rate"].as_i64()), (Some(1000), Some(10)));
    let sent = log["sent"].as_i64().unwrap();
    assert!((1..1000).contains(&sent), "{sent}");

    server.get("/log?lines=5&rate=1000");
    sleep(300);
    assert!(server.info()["log"].is_null());
    let lines = logged(&server);
    assert!(lines.iter().any(|line| line.ends_with("line 5 of 5")));
    assert!(!lines.iter().any(|line| line.ends_with("of 1000") && line.contains("line 500 ")));
}

#[test]
fn log_takes_its_limits_from_the_configuration() {
    let server = Server::with(&[("MAX_LOG_LINES", "50"), ("MAX_LOG_RATE", "20")]);
    assert_eq!(server.status("/log?lines=51"), 400);
    assert_eq!(server.status("/log?rate=21"), 400);
    assert_eq!(server.status("/log?lines=0"), 400);
    assert_eq!(server.status("/log?lines=10&rate=20"), 200);
}

#[test]
fn echo_returns_the_request_and_masks_the_credentials() {
    let server = Server::start();
    let headers = [("authorization", "Bearer s3cret"), ("cookie", "sid=abc"), ("x-forwarded-for", "203.0.113.7"), ("x-custom", "yes")];
    let response = server.get_with("/echo?a=1", &headers);
    assert!(!response.body.contains("s3cret") && !response.body.contains("sid=abc"));
    let echo = response.json();
    assert_eq!(echo["method"], "GET");
    assert_eq!(echo["url"], "/echo?a=1");
    assert_eq!(echo["protocol"], "http");
    assert_eq!(echo["secure"], false);
    assert_eq!(echo["httpVersion"], "1.1");
    assert_eq!(echo["hostname"], "127.0.0.1");
    assert_eq!(echo["remoteAddress"], "127.0.0.1");
    assert_eq!(echo["headers"]["x-custom"], "yes");
    assert_eq!(echo["headers"]["authorization"], "[masked]");
    assert_eq!(echo["headers"]["cookie"], "[masked]");
    assert_eq!(echo["ip"], "127.0.0.1", "X-Forwarded-For is not trusted by default");
    assert_eq!(echo["ips"], json!([]));
}

#[test]
fn echo_follows_the_proxy_when_it_is_trusted() {
    let server = Server::with(&[("TRUST_PROXY", "true")]);
    let headers = [("x-forwarded-for", "203.0.113.7"), ("x-forwarded-proto", "https"), ("x-forwarded-host", "app.example.com")];
    let echo = server.get_with("/echo", &headers).json();
    assert_eq!(echo["ip"], "203.0.113.7");
    assert_eq!(echo["ips"], json!(["203.0.113.7"]));
    assert_eq!(echo["protocol"], "https");
    assert_eq!(echo["secure"], true);
    assert_eq!(echo["hostname"], "app.example.com");
    assert_eq!(server.json("/config")["trustProxy"], true);
}

#[test]
fn echo_trusts_only_the_proxies_it_is_told_to() {
    let headers = [("x-forwarded-for", "1.2.3.4, 203.0.113.7, 127.0.0.2"), ("x-forwarded-proto", "https")];

    let loopback = Server::with(&[("TRUST_PROXY", "loopback")]);
    let echo = loopback.get_with("/echo", &headers).json();
    assert_eq!(echo["ip"], "203.0.113.7", "the first address that is not a proxy, walking back from the socket");
    assert_eq!(echo["ips"], json!(["203.0.113.7", "127.0.0.2"]));
    assert_eq!(echo["protocol"], "https");

    let elsewhere = Server::with(&[("TRUST_PROXY", "10.0.0.0/8")]);
    let echo = elsewhere.get_with("/echo", &headers).json();
    assert_eq!(echo["ip"], "127.0.0.1", "the socket is not a trusted proxy");
    assert_eq!(echo["protocol"], "http");

    let hops = Server::with(&[("TRUST_PROXY", "1")]);
    assert_eq!(hops.get_with("/echo", &headers).json()["ip"], "127.0.0.2");
    assert_eq!(hops.json("/config")["trustProxy"], 1);
    assert_eq!(loopback.json("/config")["trustProxy"], "loopback");
}

#[test]
fn echo_shows_a_host_header_with_a_port_or_brackets_without_it() {
    let server = Server::start();
    let answer = raw(server.port, "GET /echo HTTP/1.1\r\nHost: [::1]:3000\r\nConnection: close\r\n\r\n");
    let body = &answer[answer.find('{').unwrap()..];
    let echo: Value = serde_json::from_str(body).unwrap();
    assert_eq!(echo["hostname"], "[::1]");
}

#[test]
fn help_lists_every_endpoint_with_its_parameters_defaults_and_limits() {
    let server = Server::with(&[("MAX_NUMBER", "40")]);
    let help = server.json("/help");
    assert_eq!(help["name"], "torchio");
    assert_eq!(help["version"], VERSION);
    let endpoints = help["endpoints"].as_array().unwrap();
    let by_path = |path: &str| endpoints.iter().find(|e| e["path"] == path).unwrap_or_else(|| panic!("{path} is not listed"));

    assert_eq!(by_path("/stress")["params"]["number"], json!({"fallback": 45, "min": 0, "max": 40}));
    assert_eq!(by_path("/memory")["params"]["mb"], json!({"fallback": 100, "min": 1, "max": 300}));
    assert_eq!(by_path("/status/:code")["params"]["code"], json!({"min": 200, "max": 599, "in": "path"}));
    assert_eq!(by_path("/crash")["auth"], true);
    assert_eq!(by_path("/info")["auth"], false);
    assert!(endpoints.iter().all(|e| e["path"] != "/ui"), "the panel is off");
    for path in ["/", "/delay", "/ready", "/metrics", "/help", "/config", "/echo", "/log"] {
        by_path(path);
    }
}

#[test]
fn help_answers_with_plain_text_to_a_browser() {
    let server = Server::with(&[("MAX_NUMBER", "50")]);
    let response = server.get_with("/help", &[("accept", "text/html,application/xhtml+xml,*/*;q=0.8")]);
    assert!(response.header("content-type").unwrap().starts_with("text/plain"));
    assert!(response.body.starts_with(&format!("torchio {VERSION}\n\n/\n")));
    assert!(response.body.contains("/stress  [needs TOKEN]\n  Runs a Fibonacci"));
    assert!(response.body.contains("  number: default 45, 0..50\n"));
    assert!(response.body.contains("/crash  [needs TOKEN]"));
    assert!(response.body.contains("  code: required, 200..599\n"));

    let json = server.get_with("/help", &[("accept", "application/json")]);
    assert!(json.header("content-type").unwrap().starts_with("application/json"));
}

#[test]
fn help_marks_as_protected_exactly_the_endpoints_the_token_protects() {
    let server = Server::with(&[("TOKEN", "s3cret"), ("UI", "1")]);
    let help = server.json("/help");
    let endpoints = help["endpoints"].as_array().unwrap();
    assert!(endpoints.iter().any(|e| e["path"] == "/ui"));
    for endpoint in endpoints {
        let path = endpoint["path"].as_str().unwrap();
        if path == "/ui" || path.contains(':') {
            continue;
        }
        let route = if path == "/crash" { "/crash?after=60" } else { path };
        assert_eq!(server.status(route) == 401, endpoint["auth"] == true, "{path}");
    }
}

#[test]
fn config_shows_the_effective_configuration_and_never_the_token() {
    let server = Server::with(&[("TOKEN", "s3cret"), ("MAX_DELAY_MS", "1234")]);
    let response = server.get("/config");
    assert!(!response.body.contains("s3cret"));
    let config = response.json();
    assert_eq!(config["auth"], true);
    assert!(config.get("token").is_none());
    assert_eq!(config["maxDelayMs"], 1234);
    assert_eq!(config["workers"], 2);
    assert_eq!(config["maxMemoryMb"], 300);
    assert_eq!(config["diskDir"], server.disk_dir.path().to_str().unwrap());
    assert_eq!(config["ui"], false);
    assert_eq!(config["trustProxy"], false);
    assert_eq!(config["port"].as_u64(), Some(server.port as u64));
}
