//! The endpoints a probe, a load balancer or a retry policy talks to: `/`, `/status/:code`,
//! `/ready` and `/delay`.
mod common;

use common::*;
use std::io::Write;
use std::net::TcpStream;
use std::thread;
use std::time::Instant;

#[test]
fn root_answers_as_a_health_check() {
    let server = Server::start();
    assert_eq!(server.text("/"), "Status: OK");
}

#[test]
fn status_answers_with_the_requested_code() {
    let server = Server::start();
    assert_eq!(server.status("/status/503"), 503);
    assert_eq!(server.status("/status/204"), 204);
    assert_eq!(server.status("/status/599"), 599);
    let ok = server.get("/status/200");
    assert_eq!((ok.status, ok.body.as_str()), (200, "Status: 200"));
}

#[test]
fn status_rejects_what_is_not_a_code() {
    let server = Server::start();
    for code in ["199", "600", "abc", "2.5", "-1", "0"] {
        let response = server.get(&format!("/status/{code}"));
        assert_eq!(response.status, 400, "{code}");
        assert_eq!(response.body, "code must be an integer between 200 and 599");
    }
}

#[test]
fn ready_switches_off_for_a_while_and_comes_back_by_itself() {
    let server = Server::start();
    assert_eq!(server.text("/ready"), "Ready");

    assert_eq!(server.text("/ready/off?seconds=1"), "Not ready for 1 s");
    let down = server.get("/ready");
    assert_eq!(down.status, 503);
    assert!(down.body.starts_with("Not ready, ready again in "), "{}", down.body);
    assert_eq!(server.info()["ready"]["ready"], false);
    assert_eq!(metric(&server.text("/metrics"), "torchio_ready"), Some(0.0));
    assert_eq!(server.status("/"), 200, "the liveness check must keep answering");

    wait_for("the server to be ready again", 3000, || server.status("/ready") == 200);
    assert_eq!(server.info()["ready"], serde_json::json!({"ready": true, "secondsLeft": null}));
}

#[test]
fn ready_stays_off_until_it_is_switched_on() {
    let server = Server::start();
    assert_eq!(server.text("/ready/off?seconds=0"), "Not ready until /ready/on");
    sleep(1200);
    assert_eq!(server.text("/ready"), "Not ready");
    assert_eq!(server.info()["ready"], serde_json::json!({"ready": false, "secondsLeft": null}));

    assert_eq!(server.text("/ready/on"), "Ready");
    assert_eq!(server.text("/ready"), "Ready");
    assert_eq!(metric(&server.text("/metrics"), "torchio_ready"), Some(1.0));
}

#[test]
fn ready_off_again_restarts_the_countdown() {
    let server = Server::start();
    server.get("/ready/off?seconds=2");
    sleep(1200);
    server.get("/ready/off?seconds=2");
    sleep(1200);
    assert_eq!(server.status("/ready"), 503, "the first countdown must not end it");
    wait_for("the second countdown to end", 3000, || server.status("/ready") == 200);
}

#[test]
fn ready_off_checks_its_seconds() {
    let server = Server::start();
    assert_eq!(server.status("/ready/off?seconds=31"), 400);
    assert_eq!(server.status("/ready/off?seconds=-1"), 400);
    assert_eq!(server.status("/ready"), 200, "a rejected request changes nothing");
}

#[test]
fn delay_waits_without_blocking_the_server() {
    let server = Server::start();
    let url = server.url();
    let started = Instant::now();
    let slow = thread::spawn(move || try_get(&format!("{url}/delay?ms=300"), &[]).unwrap());
    sleep(50);

    let quick = Instant::now();
    assert_eq!(server.text("/"), "Status: OK");
    assert!(quick.elapsed().as_millis() < 200, "another request was blocked by the delay");

    assert_eq!(slow.join().unwrap().body, "Answered after 300 ms");
    assert!(started.elapsed().as_millis() >= 290);
}

#[test]
fn flaky_waits_only_before_a_failure() {
    let server = Server::start();
    let started = Instant::now();
    assert_eq!(server.status("/flaky?rate=0&ms=2000"), 200);
    assert!(started.elapsed().as_millis() < 1000, "a request that succeeds waited");

    let started = Instant::now();
    assert_eq!(server.status("/flaky?rate=100&code=502&ms=300"), 502);
    assert!(started.elapsed().as_millis() >= 290, "a failure did not wait");
}

#[test]
fn flaky_requests_are_counted_by_status() {
    let server = Server::start();
    server.get("/flaky?rate=100&code=500");
    server.get("/flaky?rate=100&code=500");
    server.get("/flaky?rate=0");
    let metrics = server.text("/metrics");
    assert_eq!(metric(&metrics, r#"torchio_http_requests_total{route="/flaky",code="500"}"#), Some(2.0));
    assert_eq!(metric(&metrics, r#"torchio_http_requests_total{route="/flaky",code="200"}"#), Some(1.0));
}

#[test]
fn delay_checks_its_limits() {
    let server = Server::start();
    assert_eq!(server.status("/delay?ms=6000"), 400);
    assert_eq!(server.status("/delay?ms=-1"), 400);
    assert_eq!(server.text("/delay?ms=0"), "Answered after 0 ms");
}

#[test]
fn delay_stops_counting_a_request_the_client_gave_up_on() {
    let server = Server::start();
    let mut client = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    client.write_all(b"GET /delay?ms=4000 HTTP/1.1\r\nHost: test\r\n\r\n").unwrap();
    wait_for("the delay to be counted", 2000, || server.info()["delay"]["inFlight"] == 1);

    drop(client);
    wait_for("the delay to be forgotten", 2000, || server.info()["delay"]["inFlight"] == 0);
}
