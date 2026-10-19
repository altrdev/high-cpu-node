//! How the process starts, is configured and ends.
mod common;

use common::*;
use std::net::TcpListener;
use std::time::Instant;

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
