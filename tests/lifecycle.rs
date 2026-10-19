//! How the process starts, is configured and ends.
mod common;

use common::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

#[test]
fn it_says_where_it_listens() {
    let server = Server::start();
    assert!(server.output().contains(&format!("torchio running on 0.0.0.0:{} with 2 workers", server.port)), "{}", server.output());
}

#[test]
fn a_value_that_is_not_a_positive_integer_is_reported_and_replaced_by_the_default() {
    let server = Server::with(&[("MAX_NUMBER", "abc"), ("MAX_DELAY_MS", "0"), ("WORKERS", "-2"), ("MAX_HOLD_SECONDS", "12")]);
    let info = server.info();
    assert_eq!(info["limits"]["maxNumber"], 50);
    assert_eq!(info["limits"]["maxDelayMs"], 60000);
    assert_eq!(info["limits"]["maxHoldSeconds"], 12, "a valid value is used");
    let cpus = std::thread::available_parallelism().unwrap().get();
    assert_eq!(info["stress"]["workers"].as_u64(), Some(cpus as u64));

    let output = server.output();
    assert!(output.contains("MAX_NUMBER=abc is not a positive integer, using 50"), "{output}");
    assert!(output.contains("MAX_DELAY_MS=0 is not a positive integer, using 60000"), "{output}");
    assert!(output.contains("WORKERS=-2 is not a positive integer"), "{output}");
    assert!(!output.contains("MAX_HOLD_SECONDS"), "a valid value is not reported");
}

// What the server wrote as JSON objects, one for each line
fn json_lines(server: &Server) -> Vec<serde_json::Value> {
    server.output().lines().map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("not JSON: {line}"))).collect()
}

#[test]
fn with_json_every_line_is_an_object_with_the_time_the_level_and_the_message() {
    let server = Server::with(&[("LOG_FORMAT", "json"), ("MAX_NUMBER", "abc")]);
    wait_for("the start line", 5000, || server.output().contains("running on"));
    let lines = json_lines(&server);
    let warning = lines.iter().find(|l| l["message"].as_str().is_some_and(|m| m.starts_with("MAX_NUMBER=abc"))).unwrap();
    assert_eq!(warning["level"], "warn");
    let started = lines.iter().find(|l| l["message"].as_str().is_some_and(|m| m.contains("running on"))).unwrap();
    assert_eq!(started["level"], "info");
    let timestamp = started["timestamp"].as_str().unwrap();
    assert!(timestamp.len() == 24 && timestamp.ends_with('Z'), "{timestamp}");
    assert!(server.text("/config").contains("\"logFormat\":\"json\""));
}

#[test]
fn with_json_each_request_has_a_line_with_method_path_status_and_duration() {
    let server = Server::with(&[("LOG_FORMAT", "json")]);
    server.get("/status/418?token=secret");
    server.get("/nowhere");
    wait_for("the access lines", 5000, || server.output().contains("/nowhere"));
    let lines = json_lines(&server);
    let teapot = lines.iter().find(|l| l["path"] == "/status/418").unwrap();
    assert_eq!(teapot["message"], "GET /status/418 418");
    assert_eq!((teapot["method"].as_str(), teapot["status"].as_u64()), (Some("GET"), Some(418)));
    assert!(teapot["duration_ms"].as_f64().unwrap() >= 0.0);
    assert!(!server.output().contains("secret"), "the query is not logged");
    assert_eq!(lines.iter().find(|l| l["path"] == "/nowhere").unwrap()["status"], 404);
}

#[test]
fn with_json_the_lines_of_log_are_objects_too() {
    let server = Server::with(&[("LOG_FORMAT", "json")]);
    server.get("/log?lines=3&rate=100");
    wait_for("the lines", 5000, || logged(&server).len() >= 3);
    let line: serde_json::Value = serde_json::from_str(&logged(&server)[2]).unwrap();
    assert_eq!(line["message"], "torchio-log line 3 of 3");
    assert_eq!(line["level"], "info");
}

#[test]
fn the_default_is_plain_text_without_access_lines() {
    let server = Server::start();
    server.get("/status/418");
    server.get("/");
    let output = server.output();
    assert!(output.starts_with("torchio running on"), "{output}");
    assert!(!output.contains("/status/418"), "{output}");
    assert!(!output.contains('{'), "{output}");
}

#[test]
fn a_log_format_that_is_not_text_or_json_is_reported_and_text_is_used() {
    let server = Server::with(&[("LOG_FORMAT", "xml")]);
    assert!(server.output().contains("LOG_FORMAT=xml is not text or json, using text"), "{}", server.output());
    server.get("/");
    assert!(!server.output().contains("GET /"));
}

#[test]
fn the_start_can_be_delayed() {
    let started = Instant::now();
    let server = Server::with(&[("START_DELAY_SECONDS", "2")]);
    assert!(started.elapsed().as_millis() >= 1800, "it answered before the delay");
    assert!(server.output().contains("will start listening in 2 s"));
    assert_eq!(server.text("/"), "Status: OK");
}

#[test]
fn a_start_delay_that_is_not_a_number_is_reported() {
    let server = Server::with(&[("START_DELAY_SECONDS", "-1")]);
    assert!(server.output().contains("START_DELAY_SECONDS=-1 is not a non-negative integer, using 0"));
}

#[test]
fn sigterm_ends_it_with_success() {
    let mut server = Server::start();
    let status = server.terminate();
    assert_eq!(status.code(), Some(0));
}

#[test]
fn sigint_ends_it_with_success_too() {
    let mut server = Server::start();
    server.signal(libc::SIGINT);
    assert_eq!(server.wait_exit(5000).code(), Some(0));
}

// A request on a connection that stays open, and its answer: the headers and the body by Content-Length
fn exchange(stream: &mut TcpStream, path: &str) -> String {
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(stream, "GET {path} HTTP/1.1\r\nHost: torchio\r\n\r\n").unwrap();
    let mut answer = Vec::new();
    let mut byte = [0u8; 1];
    while !answer.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        answer.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&answer).to_lowercase();
    let length: usize = head.lines().find_map(|line| line.strip_prefix("content-length: ")).unwrap().trim().parse().unwrap();
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body).unwrap();
    String::from_utf8(body).unwrap()
}

// Waits until the server counts at least this many requests and connections as open. The count is
// of what the server has accepted, and a client that has connected is not that yet, so a signal sent
// right after would find nothing open and end the drain at once. The request that asks counts as
// one of each, which is why the callers add it.
fn wait_open(server: &Server, requests: i64, connections: i64) {
    wait_for("the server to count what is open", 5000, || {
        let open = server.info()["shutdown"].clone();
        open["openRequests"].as_i64() >= Some(requests) && open["openConnections"].as_i64() >= Some(connections)
    });
}

#[test]
fn the_signal_is_logged_when_it_ends_it_right_away() {
    let mut server = Server::start();
    assert!(server.terminate().success());
    assert!(server.output().contains("SIGTERM received, exiting"), "{}", server.output());
}

#[test]
fn with_a_grace_period_it_serves_the_request_in_progress_and_refuses_new_connections() {
    let mut server = Server::with(&[("SHUTDOWN_GRACE_SECONDS", "10")]);
    let url = server.url();
    let waiting = std::thread::spawn(move || try_get(&format!("{url}/delay?ms=1500"), &[]));
    wait_open(&server, 2, 2);

    server.signal(libc::SIGTERM);
    sleep(300);
    assert!(server.exited().is_none(), "it ended with a request in progress");
    assert!(TcpStream::connect(("127.0.0.1", server.port)).is_err(), "a new connection was accepted while draining");

    assert_eq!(waiting.join().unwrap().unwrap().body, "Answered after 1500 ms");
    assert!(server.wait_exit(5000).success());
    let output = server.output();
    assert!(output.contains("SIGTERM received, draining for up to 10 s"), "{output}");
    assert!(output.contains("Drained after"), "{output}");
}

#[test]
fn the_grace_period_ends_the_drain_even_if_a_request_is_still_open() {
    let mut server = Server::with(&[("SHUTDOWN_GRACE_SECONDS", "1")]);
    let url = server.url();
    let waiting = std::thread::spawn(move || try_get(&format!("{url}/delay?ms=4000"), &[]));
    wait_open(&server, 2, 2);

    let started = Instant::now();
    server.signal(libc::SIGTERM);
    assert!(server.wait_exit(5000).success());
    assert!((900..3000).contains(&(started.elapsed().as_millis() as u64)), "{:?}", started.elapsed());
    assert!(
        server.output().contains("The grace period of 1 s is over with 1 requests and 1 connections still open"),
        "{}",
        server.output()
    );
    assert!(waiting.join().unwrap().is_err(), "the request was cut when the time was up");
}

#[test]
fn while_draining_info_and_metrics_say_so_on_a_connection_that_was_open() {
    let mut server = Server::with(&[("SHUTDOWN_GRACE_SECONDS", "20")]);
    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    let before: serde_json::Value = serde_json::from_str(&exchange(&mut stream, "/info")).unwrap();
    assert_eq!(before["shutdown"]["draining"], false);
    assert_eq!(before["shutdown"]["graceSeconds"], 20);
    assert!(before["shutdown"]["drainSeconds"].is_null());

    server.signal(libc::SIGTERM);
    sleep(1500);
    assert!(server.exited().is_none(), "it ended with a connection open");
    let during: serde_json::Value = serde_json::from_str(&exchange(&mut stream, "/info")).unwrap();
    assert_eq!(during["shutdown"]["draining"], true);
    assert!(during["shutdown"]["drainSeconds"].as_f64().unwrap() >= 1.0);
    assert_eq!(during["shutdown"]["openConnections"], 1);
    assert_eq!(during["shutdown"]["openRequests"], 1, "the request for /info itself");
    let metrics = exchange(&mut stream, "/metrics");
    assert_eq!(metric(&metrics, "torchio_draining"), Some(1.0));
    assert_eq!(metric(&metrics, "torchio_open_connections"), Some(1.0));

    drop(stream);
    assert!(server.wait_exit(5000).success(), "it ends when the last connection is closed");
    assert!(server.output().contains("Drained after"), "{}", server.output());
}

#[test]
fn a_second_signal_ends_the_drain_at_once() {
    let mut server = Server::with(&[("SHUTDOWN_GRACE_SECONDS", "60")]);
    let _stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    wait_open(&server, 1, 2);
    server.signal(libc::SIGTERM);
    sleep(300);
    assert!(server.exited().is_none());

    let started = Instant::now();
    server.signal(libc::SIGTERM);
    assert!(server.wait_exit(5000).success());
    assert!(started.elapsed().as_millis() < 2500);
    assert!(server.output().contains("SIGTERM received again, exiting now"), "{}", server.output());
}

#[test]
fn held_connections_keep_the_drain_open_until_the_grace_period_is_over() {
    let mut server = Server::with(&[("SHUTDOWN_GRACE_SECONDS", "2")]);
    assert!(server.text("/connections?count=3&hold=30").starts_with("Opened 3 connections"));
    wait_open(&server, 1, 4);
    server.signal(libc::SIGTERM);
    assert!(server.wait_exit(8000).success());
    let output = server.output();
    assert!(output.contains("with 0 requests and 3 connections still open"), "{output}");
}

#[test]
fn it_ends_right_away_even_with_a_request_in_progress() {
    let mut server = Server::start();
    let url = server.url();
    let waiting = std::thread::spawn(move || try_get(&format!("{url}/delay?ms=4000"), &[]));
    sleep(200);

    let started = Instant::now();
    assert!(server.terminate().success());
    assert!(started.elapsed().as_millis() < 1000, "it waited for the request");
    assert!(waiting.join().unwrap().is_err(), "the request was cut");
}

#[test]
fn it_does_not_start_when_the_port_is_taken() {
    let taken = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = taken.local_addr().unwrap().port().to_string();
    let output = run(&[], &[("PORT", &port)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("Internal Server Error: "));
}

#[test]
fn it_does_not_start_on_a_port_that_does_not_exist() {
    let output = run(&[], &[("PORT", "70000")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("port 70000 is out of range"));
}

#[test]
fn it_does_not_start_with_a_trust_proxy_it_cannot_read() {
    let port = free_port().to_string();
    for (setting, message) in [
        ("10.0.0.0/8, nope", "invalid IP address: nope"),
        ("10.0.0.0/33", "invalid range on address: 10.0.0.0/33"),
        ("10.0.0.0/0", "invalid range on address: 10.0.0.0/0"),
    ] {
        let output = run(&[], &[("PORT", &port), ("TRUST_PROXY", setting)]);
        assert_eq!(output.status.code(), Some(1), "{setting}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(message), "{setting}: {stderr}");
    }
}

#[test]
fn crash_ends_the_process_with_the_exit_code_asked_for() {
    let mut server = Server::start();
    assert_eq!(server.text("/crash?code=42"), "Exiting with code 42");
    assert_eq!(server.wait_exit(3000).code(), Some(42));
}

#[test]
fn crash_defaults_to_one() {
    let mut server = Server::start();
    server.get("/crash");
    assert_eq!(server.wait_exit(3000).code(), Some(1));
}

#[test]
fn crash_can_wait_before_exiting() {
    let mut server = Server::start();
    assert_eq!(server.status("/crash?code=256"), 400);
    assert_eq!(server.status("/crash?after=31"), 400);

    let started = Instant::now();
    assert_eq!(server.text("/crash?code=3&after=1"), "Exiting with code 3 in 1 s");
    assert_eq!(server.status("/"), 200, "the server must still answer before the exit");
    assert_eq!(server.wait_exit(3000).code(), Some(3));
    assert!(started.elapsed().as_millis() >= 900);
}

#[test]
fn crash_does_not_clean_up_and_the_next_start_does() {
    let mut server = Server::start();
    server.get("/disk?mb=1&hold=30");
    server.get("/crash");
    server.wait_exit(3000);
    assert_eq!(server.disk_files().len(), 1, "like a real crash, nothing is cleaned up");

    let dir = server.disk_dir.path().to_string_lossy().into_owned();
    let next = Server::with(&[("DISK_DIR", &dir)]);
    assert!(next.disk_files().is_empty(), "leftovers are removed at startup");
}

#[test]
fn block_freezes_the_server_for_the_time_asked_for() {
    let server = Server::start();
    assert_eq!(server.status("/block?ms=0"), 400);
    assert_eq!(server.status("/block?ms=30001"), 400);
    assert_eq!(server.info()["block"]["count"], 0);

    assert_eq!(server.text("/block?ms=1500"), "Blocking for 1500 ms");
    // the answer left before the threads went to sleep, which they do a moment later
    sleep(200);
    let asked = Instant::now();
    assert_eq!(server.status("/"), 200);
    assert!(asked.elapsed().as_millis() >= 500, "the server answered after {:?}, in the middle of the block", asked.elapsed());

    let info = server.info();
    assert_eq!((info["block"]["count"].as_u64(), info["block"]["lastMs"].as_u64()), (Some(1), Some(1500)));
    let output = server.output();
    assert!(output.contains("Blocking every thread of the server for 1500 ms"), "{output}");
    wait_for("the end of the block in the log", 3000, || server.output().contains("The server answers again"));
}

#[test]
fn block_is_limited_by_the_configuration() {
    let server = Server::with(&[("MAX_BLOCK_MS", "100")]);
    assert_eq!(server.status("/block?ms=101"), 400);
    assert_eq!(server.json("/config")["maxBlockMs"], 100);
}

#[test]
fn oom_is_refused_unless_it_is_allowed_and_nothing_is_allocated() {
    let mut server = Server::start();
    let response = server.get("/oom");
    assert_eq!(response.status, 403);
    assert!(response.body.contains("ALLOW_OOM=1"), "{}", response.body);
    assert_eq!(server.json("/config")["allowOom"], false);
    sleep(300);
    assert!(server.exited().is_none());
    assert!(server.rss_kb() < 200 * 1024, "nothing may be allocated: {} kB", server.rss_kb());
}

#[test]
fn oom_needs_the_token_even_when_it_is_allowed() {
    let mut server = Server::with(&[("ALLOW_OOM", "1"), ("TOKEN", "s3cret")]);
    assert_eq!(server.json("/config")["allowOom"], true);
    assert_eq!(server.status("/oom"), 401);
    sleep(300);
    assert!(server.exited().is_none());
    assert!(server.rss_kb() < 200 * 1024, "{} kB", server.rss_kb());
}
