//! The HTTP behaviour of the router, without a socket: every endpoint of the catalog, the limits of
//! every parameter, the methods, CORS and the shape of what `/info`, `/config` and `/metrics` say.
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request, StatusCode};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::Arc;
use torchio::app::{AppState, router};
use torchio::config::Config;
use torchio::endpoints::{STATUS_MAX, STATUS_MIN, catalog};
use tower::ServiceExt;

/// A router with its own state, configured by `env`. Its `DISK_DIR` is private to the test.
fn app(env: &[(&str, &str)]) -> Router {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "torchio-api-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let get = |name: &str| match name {
        "DISK_DIR" => Some(dir.to_string_lossy().into_owned()),
        "WORKERS" => Some("1".into()),
        _ => env.iter().find(|(key, _)| *key == name).map(|(_, value)| value.to_string()),
    };
    router(Arc::new(AppState::new(Config::read(get, |_| {})).unwrap()))
}

struct Answer {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Answer {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.body))
    }

    fn header(&self, name: &str) -> &str {
        self.headers.get(name).map_or("", |value| value.to_str().unwrap())
    }
}

async fn send(app: &Router, request: Request<Body>) -> Answer {
    let response = app.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let body = String::from_utf8_lossy(&to_bytes(body, 1 << 20).await.unwrap()).into_owned();
    Answer { status: parts.status, headers: parts.headers, body }
}

async fn request(app: &Router, method: &str, uri: &str, headers: &[(&str, &str)]) -> Answer {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut request = builder.body(Body::empty()).unwrap();
    request.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4000))));
    send(app, request).await
}

async fn get(app: &Router, uri: &str) -> Answer {
    request(app, "GET", uri, &[]).await
}

async fn get_with(app: &Router, uri: &str, headers: &[(&str, &str)]) -> Answer {
    request(app, "GET", uri, headers).await
}

#[tokio::test]
async fn every_endpoint_of_the_catalog_is_routed_and_the_token_protects_the_ones_it_says() {
    let app = app(&[("TOKEN", "s3cret"), ("UI", "1")]);
    let config = Config::read(|name| (name == "UI").then(|| "1".to_string()), |_| {});
    let endpoints = catalog(&config);
    assert!(endpoints.len() >= 19);

    for endpoint in endpoints {
        // the cheapest request that goes through the endpoint
        let uri = match endpoint.path {
            "/status/:code" => "/status/204".to_string(),
            "/delay" => "/delay?ms=0".to_string(),
            "/crash" => "/crash?after=60".to_string(),
            path => path.to_string(),
        };
        let answer = get(&app, &uri).await;
        assert_ne!(answer.status, StatusCode::NOT_FOUND, "{uri} is in the catalog and not routed");
        assert_ne!(answer.status, StatusCode::METHOD_NOT_ALLOWED, "{uri}");
        assert_eq!(answer.status == StatusCode::UNAUTHORIZED, endpoint.auth, "{uri}: the catalog says auth={}", endpoint.auth);
        if endpoint.auth {
            assert_eq!(answer.header("www-authenticate"), "Bearer", "{uri}");
        }
    }
}

#[tokio::test]
async fn every_parameter_is_rejected_one_step_outside_its_range() {
    let app = app(&[("TOKEN", "s3cret")]);
    // the same workers as the app, which has one: that is the range of /cpu?workers=
    let config = Config::read(|name| (name == "WORKERS").then(|| "1".to_string()), |_| {});
    let token = [("authorization", "Bearer s3cret")];
    let mut checked = 0;
    for endpoint in catalog(&config) {
        for (name, param) in &endpoint.params {
            let message = format!("{name} must be between {} and {}", param.min, param.max);
            for value in [param.min - 1, param.max + 1] {
                let uri = if endpoint.location == Some("path") {
                    endpoint.path.replace(&format!(":{name}"), &value.to_string())
                } else {
                    format!("{}?{name}={value}", endpoint.path)
                };
                let answer = get_with(&app, &uri, &token).await;
                assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{uri}");
                if endpoint.location != Some("path") {
                    assert_eq!(answer.body, message, "{uri}");
                }
                checked += 1;
            }
        }
    }
    assert!(checked >= 30, "only {checked} cases");
}

#[tokio::test]
async fn a_value_that_is_not_a_number_falls_back_to_the_default() {
    let app = app(&[]);
    assert_eq!(get(&app, "/cpu?percent=abc&hold=0").await.body, "Stopped burning CPU");
    assert_eq!(get(&app, "/ready/off?seconds=abc").await.body, "Not ready for 30 s");
    assert_eq!(get(&app, "/ready/off?seconds=").await.body, "Not ready for 30 s");
    assert_eq!(get(&app, "/ready/off?seconds=7days").await.body, "Not ready for 7 s", "the number at the start counts");
}

#[tokio::test]
async fn limits_come_from_the_configuration() {
    let app = app(&[("MAX_HOLD_SECONDS", "5"), ("MAX_MEMORY_MB", "20"), ("MAX_FD", "10"), ("MAX_DISK_MB", "3")]);
    assert_eq!(get(&app, "/memory?hold=6").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(get(&app, "/memory?mb=21").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(get(&app, "/fd?count=11").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(get(&app, "/disk?mb=4").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(get(&app, "/memory?mb=20&hold=0").await.status, StatusCode::OK, "the limit itself is allowed");
}

#[tokio::test]
async fn status_answers_every_code_it_promises() {
    let app = app(&[]);
    for code in STATUS_MIN..=STATUS_MAX {
        let answer = get(&app, &format!("/status/{code}")).await;
        assert_eq!(i64::from(answer.status.as_u16()), code);
        if !matches!(code, 204 | 205 | 304) {
            assert_eq!(answer.body, format!("Status: {code}"));
        }
    }
}

#[tokio::test]
async fn flaky_never_fails_at_rate_0_and_always_does_at_rate_100() {
    let app = app(&[]);
    for _ in 0..50 {
        let answer = get(&app, "/flaky?rate=0").await;
        assert_eq!((answer.status, answer.body.as_str()), (StatusCode::OK, "Status: OK"));
        let answer = get(&app, "/flaky?rate=100").await;
        assert_eq!((answer.status, answer.body.as_str()), (StatusCode::SERVICE_UNAVAILABLE, "Status: 503"), "503 is the default");
    }
    assert_eq!(get(&app, "/flaky?rate=100&code=429").await.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(get(&app, "/flaky?rate=100&code=599").await.status.as_u16(), 599);
}

#[tokio::test]
async fn flaky_fails_about_half_of_the_requests_by_default() {
    let app = app(&[]);
    let mut failed = 0;
    for _ in 0..400 {
        failed += i32::from(get(&app, "/flaky").await.status != StatusCode::OK);
    }
    assert!((120..=280).contains(&failed), "{failed} of 400 failed with the default rate of 50");
}

#[tokio::test]
async fn flaky_only_takes_a_code_that_is_a_failure() {
    let app = app(&[]);
    assert_eq!(get(&app, "/flaky?code=399").await.body, "code must be between 400 and 599");
    assert_eq!(get(&app, "/flaky?code=600").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(get(&app, "/flaky?rate=101").await.body, "rate must be between 0 and 100");
    assert_eq!(get(&app, "/flaky?ms=60001").await.status, StatusCode::BAD_REQUEST, "the limit of /delay");
}

#[tokio::test]
async fn head_answers_like_get_without_a_body() {
    let app = app(&[]);
    let answer = request(&app, "HEAD", "/", &[]).await;
    assert_eq!((answer.status, answer.body.as_str()), (StatusCode::OK, ""));
}

#[tokio::test]
async fn other_methods_are_not_allowed_and_unknown_paths_are_not_found() {
    let app = app(&[]);
    for method in ["POST", "PUT", "DELETE", "PATCH"] {
        assert_eq!(request(&app, method, "/", &[]).await.status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
    }
    for path in ["/nope", "/info/", "/INFO", "/status", "/status/200/more", "/ui"] {
        assert_eq!(get(&app, path).await.status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn any_origin_may_call_it_from_a_browser() {
    let app = app(&[]);
    let answer = get_with(&app, "/info", &[("origin", "http://panel.example")]).await;
    assert_eq!(answer.header("access-control-allow-origin"), "*");

    let preflight = request(
        &app,
        "OPTIONS",
        "/crash",
        &[
            ("origin", "http://panel.example"),
            ("access-control-request-method", "GET"),
            ("access-control-request-headers", "authorization"),
        ],
    )
    .await;
    assert!(preflight.status.is_success(), "{}", preflight.status);
    assert_eq!(preflight.header("access-control-allow-origin"), "*");
    assert!(
        preflight.header("access-control-allow-headers").contains("authorization")
            || preflight.header("access-control-allow-headers") == "*"
    );
}

#[tokio::test]
async fn help_chooses_json_or_text_by_the_accept_header() {
    let app = app(&[]);
    for (accept, text) in [
        ("text/html", true),
        ("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8", true),
        ("application/json", false),
        ("*/*", false),
        ("application/json, text/html;q=0.5", false),
        ("", false),
    ] {
        let answer = get_with(&app, "/help", &[("accept", accept)]).await;
        let content_type = answer.header("content-type");
        assert_eq!(content_type.starts_with("text/plain"), text, "{accept:?} -> {content_type}");
        assert_eq!(content_type.starts_with("application/json"), !text, "{accept:?} -> {content_type}");
    }
    assert!(get(&app, "/help").await.header("content-type").starts_with("application/json"), "no Accept header");
}

#[tokio::test]
async fn the_configuration_has_these_keys() {
    let config = get(&app(&[]), "/config").await.json();
    let mut keys: Vec<&str> = config.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "allowOom",
            "auth",
            "defaultConnections",
            "defaultCpuPercent",
            "defaultDelayMs",
            "defaultDiskMb",
            "defaultFd",
            "defaultHoldSeconds",
            "defaultLeakRateMb",
            "defaultLogLines",
            "defaultLogRate",
            "defaultMemoryMb",
            "defaultNotReadySeconds",
            "defaultNumber",
            "diskDir",
            "host",
            "logFormat",
            "maxBlockMs",
            "maxConnections",
            "maxDelayMs",
            "maxDiskMb",
            "maxFd",
            "maxHoldSeconds",
            "maxLogLines",
            "maxLogRate",
            "maxMemoryMb",
            "maxNumber",
            "maxPayloadMb",
            "name",
            "port",
            "shutdownGraceSeconds",
            "startDelaySeconds",
            "trustProxy",
            "ui",
            "workers"
        ]
    );
    assert_eq!(config["host"], "0.0.0.0");
    assert_eq!(config["port"], 3000);
    assert_eq!(config["maxNumber"], 50);
}

#[tokio::test]
async fn info_has_these_fields() {
    let info = get(&app(&[]), "/info").await.json();
    let keys = |value: &Value| {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(
        keys(&info),
        [
            "auth",
            "block",
            "connections",
            "delay",
            "disk",
            "fd",
            "limits",
            "log",
            "memory",
            "name",
            "ready",
            "scenario",
            "shutdown",
            "stress",
            "uptimeSeconds",
            "version"
        ]
    );
    assert_eq!(keys(&info["stress"]), ["burn", "inFlight", "perWorker", "workers"]);
    assert_eq!(keys(&info["memory"]), ["containerLimitMb", "heldMb", "limitMb", "processRssMb"]);
    assert_eq!(keys(&info["disk"]), ["dir", "freeMb", "heldMb", "limitMb"]);
    assert_eq!(keys(&info["ready"]), ["ready", "secondsLeft"]);
    assert_eq!(keys(&info["block"]), ["count", "lastMs", "msLeft"]);
    assert_eq!(
        keys(&info["limits"]),
        ["maxBlockMs", "maxDelayMs", "maxHoldSeconds", "maxLogLines", "maxLogRate", "maxNumber", "maxPayloadMb"]
    );
}

#[tokio::test]
async fn metrics_have_these_names() {
    let answer = get(&app(&[]), "/metrics").await;
    let mut names: Vec<&str> =
        answer.body.lines().filter_map(|line| line.strip_prefix("# TYPE ")).filter_map(|rest| rest.split(' ').next()).collect();
    names.sort_unstable();
    let expected = [
        "process_resident_memory_bytes",
        "torchio_connections_held",
        "torchio_connections_limit",
        "torchio_cpu_burn_percent",
        "torchio_cpu_burn_workers",
        "torchio_delay_requests_in_flight",
        "torchio_disk_free_bytes",
        "torchio_disk_held_bytes",
        "torchio_disk_limit_bytes",
        "torchio_draining",
        "torchio_fd_held",
        "torchio_fd_limit",
        "torchio_info",
        "torchio_memory_held_bytes",
        "torchio_memory_limit_bytes",
        "torchio_open_connections",
        "torchio_open_requests",
        "torchio_ready",
        "torchio_start_time_seconds",
        "torchio_stress_requests_in_flight",
        "torchio_worker_requests_in_flight",
        "torchio_workers",
    ];
    // the counter of requests appears with the first request answered, the limit of the container only in one
    for name in expected {
        assert!(names.contains(&name), "{name} is missing from {names:?}");
    }
    for name in &names {
        assert!(
            expected.contains(name) || ["torchio_container_memory_limit_bytes", "torchio_http_requests_total"].contains(name),
            "{name} is not expected"
        );
    }
    assert!(!answer.body.contains("torchio_http_requests_total"), "a fresh server has answered nothing yet");
}

#[tokio::test]
async fn the_counter_of_requests_starts_with_the_first_answer() {
    let app = app(&[]);
    get(&app, "/").await;
    get(&app, "/status/418").await;
    get(&app, "/status/418").await;
    get(&app, "/nope").await;
    let body = get(&app, "/metrics").await.body;
    for line in [
        r#"torchio_http_requests_total{route="/",code="200"} 1"#,
        r#"torchio_http_requests_total{route="/status/:code",code="418"} 2"#,
        r#"torchio_http_requests_total{route="other",code="404"} 1"#,
    ] {
        assert!(body.lines().any(|l| l == line), "{line} is missing from\n{body}");
    }
}

#[tokio::test]
async fn echo_follows_the_address_of_the_peer() {
    let app = app(&[("TRUST_PROXY", "loopback")]);
    let mut v6 = Request::builder().uri("/echo").header("x-forwarded-for", "2001:db8::7").body(Body::empty()).unwrap();
    v6.extensions_mut().insert(ConnectInfo(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 4000))));
    let echo = send(&app, v6).await.json();
    assert_eq!(echo["remoteAddress"], "::1");
    assert_eq!(echo["ip"], "2001:db8::7", "::1 is a loopback address, so it is a trusted proxy");

    let echo = get_with(&app, "/echo", &[("x-forwarded-for", "10.9.8.7")]).await.json();
    assert_eq!(echo["ip"], "10.9.8.7");
}

#[tokio::test]
async fn echo_lists_a_repeated_header_once() {
    let app = app(&[]);
    let mut request = Request::builder().uri("/echo").header("x-tag", "a").header("x-tag", "b").body(Body::empty()).unwrap();
    request.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4000))));
    assert_eq!(send(&app, request).await.json()["headers"]["x-tag"], json!("a, b"));
}

#[tokio::test]
async fn ready_follows_the_switch() {
    let app = app(&[("TOKEN", "t")]);
    let token = [("authorization", "Bearer t")];
    assert_eq!(get(&app, "/ready").await.status, StatusCode::OK);
    get_with(&app, "/ready/off?seconds=0", &token).await;
    assert_eq!(get(&app, "/ready").await.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(get(&app, "/").await.status, StatusCode::OK, "alive while not ready");
    get_with(&app, "/ready/on", &token).await;
    assert_eq!(get(&app, "/ready").await.status, StatusCode::OK);
}
