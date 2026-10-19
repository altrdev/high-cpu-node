//! What keeps the CPU busy: `/stress` runs a calculation, `/cpu` holds a level.
mod common;

use common::*;
use std::thread;

#[test]
fn stress_calculates() {
    let server = Server::start();
    let response = server.text("/stress?number=20");
    assert_eq!(response, "I'm still alive after fibonacci calculation with result: 6765");
}

#[test]
fn stress_uses_its_default_and_checks_its_range() {
    let server = Server::with(&[("MAX_NUMBER", "30")]);
    assert_eq!(server.status("/stress?number=31"), 400);
    assert_eq!(server.status("/stress?number=-1"), 400);
    assert_eq!(server.status("/stress?number=999"), 400);
    assert!(server.text("/stress?number=0").ends_with("result: 1"));
    assert_eq!(server.status("/stress"), 400, "the default (45) is above the limit of this instance");
}

#[test]
fn stress_requests_go_to_different_workers() {
    let server = Server::start();
    let url = server.url();
    let requests: Vec<_> = (0..2)
        .map(|_| {
            let url = url.clone();
            thread::spawn(move || try_get(&format!("{url}/stress?number=42"), &[]).unwrap().body)
        })
        .collect();

    wait_for("both workers to be busy", 3000, || server.info()["stress"]["perWorker"] == serde_json::json!([1, 1]));
    let info = server.info();
    assert_eq!(info["stress"]["inFlight"], 2);
    assert_eq!(metric(&server.text("/metrics"), r#"torchio_worker_requests_in_flight{worker="0"}"#), Some(1.0));

    for request in requests {
        assert!(request.join().unwrap().ends_with("result: 267914296"));
    }
    wait_for("the requests to be forgotten", 1000, || server.info()["stress"]["inFlight"] == 0);
}

#[test]
fn stress_stops_counting_a_client_that_left() {
    let server = Server::with(&[("WORKERS", "1")]);
    let mut client = std::net::TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    std::io::Write::write_all(&mut client, b"GET /stress?number=43 HTTP/1.1\r\nHost: test\r\n\r\n").unwrap();
    wait_for("the request to be counted", 2000, || server.info()["stress"]["inFlight"] == 1);

    drop(client);
    wait_for("the request to be forgotten", 2000, || server.info()["stress"]["inFlight"] == 0);
}

#[test]
fn cpu_keeps_the_workers_busy_for_a_while_and_can_be_stopped() {
    let server = Server::start();
    assert_eq!(server.text("/cpu?percent=40&hold=20"), "Burning 40% of 2 workers for 20 s");

    let burn = server.info()["stress"]["burn"].clone();
    assert_eq!(burn["percent"], 40);
    let left = burn["secondsLeft"].as_i64().unwrap();
    assert!((15..=20).contains(&left), "{left}");
    assert_eq!(metric(&server.text("/metrics"), "torchio_cpu_burn_percent"), Some(40.0));
    assert!(server.text("/stress?number=20").ends_with("result: 6765"), "/stress still works while /cpu runs");

    assert_eq!(server.text("/cpu?percent=0"), "Stopped burning CPU");
    assert!(server.info()["stress"]["burn"].is_null());
    assert_eq!(metric(&server.text("/metrics"), "torchio_cpu_burn_percent"), Some(0.0));
}

#[test]
fn cpu_can_burn_only_some_of_the_workers() {
    let server = Server::start();
    assert_eq!(server.text("/cpu?percent=40&hold=20&workers=1"), "Burning 40% of 1 workers for 20 s");
    assert_eq!(server.info()["stress"]["burn"]["workers"], 1);
    assert_eq!(metric(&server.text("/metrics"), "torchio_cpu_burn_workers"), Some(1.0));

    assert_eq!(server.text("/cpu?percent=40&hold=20"), "Burning 40% of 2 workers for 20 s", "all of them by default");
    assert_eq!(server.info()["stress"]["burn"]["workers"], 2);
    assert_eq!(server.status("/cpu?workers=3"), 400, "more than there are");
    assert_eq!(server.status("/cpu?workers=0"), 400);

    server.get("/cpu?percent=0");
    assert_eq!(metric(&server.text("/metrics"), "torchio_cpu_burn_workers"), Some(0.0));
}

#[test]
fn cpu_with_no_hold_stops_too() {
    let server = Server::start();
    server.get("/cpu?percent=40&hold=20");
    assert_eq!(server.text("/cpu?percent=40&hold=0"), "Stopped burning CPU");
    assert!(server.info()["stress"]["burn"].is_null());
}

#[test]
fn cpu_checks_its_parameters() {
    let server = Server::start();
    assert_eq!(server.status("/cpu?percent=101"), 400);
    assert_eq!(server.status("/cpu?percent=-1"), 400);
    assert_eq!(server.status("/cpu?hold=31"), 400);
    assert!(server.info()["stress"]["burn"].is_null(), "a rejected request changes nothing");
}

#[test]
fn a_new_cpu_request_replaces_the_previous_one() {
    let server = Server::start();
    server.get("/cpu?percent=30&hold=20");
    server.get("/cpu?percent=60&hold=5");
    let burn = server.info()["stress"]["burn"].clone();
    assert_eq!(burn["percent"], 60);
    assert!(burn["secondsLeft"].as_i64().unwrap() <= 5);
}

#[test]
fn cpu_really_uses_the_cpu() {
    let server = Server::with(&[("WORKERS", "1")]);
    server.get("/cpu?percent=100&hold=5");
    sleep(500);
    let before = server.cpu_seconds();
    sleep(2000);
    let used = server.cpu_seconds() - before;
    assert!(used >= 1.0, "the worker should have spent at least a second of CPU in two, it spent {used}");
}

#[test]
fn cpu_ends_by_itself() {
    let server = Server::with(&[("WORKERS", "1")]);
    server.get("/cpu?percent=100&hold=1");
    sleep(1500);
    assert!(server.info()["stress"]["burn"].is_null());
    let before = server.cpu_seconds();
    sleep(1500);
    assert!(server.cpu_seconds() - before < 0.5, "the worker must be idle once the burn is over");
}
