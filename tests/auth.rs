//! `TOKEN`: the endpoints that change the state of the instance need it, the others stay open.
mod common;

use common::*;

const PROTECTED: [&str; 14] = [
    "/stress?number=1",
    "/cpu?percent=0",
    "/memory?mb=1&hold=1",
    "/leak?rate=1&mb=1&hold=1",
    "/disk?mb=1&hold=1",
    "/fd?count=1&hold=1",
    "/log?lines=1&rate=1",
    "/ready/off?seconds=1",
    "/ready/on",
    "/crash?after=60",
    "/block?ms=1",
    "/oom",
    "/payload?mb=1",
    "/scenario",
];
const OPEN: [&str; 9] = ["/", "/ready", "/info", "/metrics", "/delay?ms=1", "/status/204", "/echo", "/help", "/config"];

#[test]
fn without_a_token_nothing_is_checked_and_info_says_so() {
    let server = Server::start();
    assert_eq!(server.status("/ready/on"), 200);
    assert_eq!(server.info()["auth"], false);
}

#[test]
fn with_a_token_the_endpoints_that_change_the_state_need_it() {
    let server = Server::with(&[("TOKEN", "s3cret")]);
    for route in PROTECTED {
        let response = server.get(route);
        assert_eq!(response.status, 401, "{route}");
        assert_eq!(response.header("www-authenticate"), Some("Bearer"));
        assert!(response.body.starts_with("Unauthorized: send the token as"));
    }
    assert_eq!(server.info()["auth"], true);
}

#[test]
fn with_a_token_the_others_stay_open() {
    let server = Server::with(&[("TOKEN", "s3cret")]);
    for route in OPEN {
        assert_ne!(server.status(route), 401, "{route}");
    }
}

#[test]
fn a_wrong_token_is_rejected_in_every_form() {
    let mut server = Server::with(&[("TOKEN", "s3cret")]);
    for route in ["/ready/on", "/crash"] {
        for authorization in ["Bearer wrong", "s3cret", "Basic s3cret", "Bearer", "Bearer  s3cret", "Bearer s3cre", "Bearer s3cret2", ""] {
            assert_eq!(server.get_with(route, &[("authorization", authorization)]).status, 401, "{route} with {authorization:?}");
        }
    }
    assert!(server.exited().is_none(), "a rejected /crash must not end the process");
}

#[test]
fn the_right_token_is_accepted_whatever_the_case_of_the_scheme() {
    let server = Server::with(&[("TOKEN", "s3cret")]);
    for authorization in ["Bearer s3cret", "bearer s3cret", "BEARER s3cret"] {
        assert_eq!(server.get_with("/ready/on", &[("authorization", authorization)]).status, 200, "{authorization}");
    }
}

#[test]
fn a_request_with_the_token_does_the_work() {
    let server = Server::with(&[("TOKEN", "s3cret")]);
    let response = server.get_with("/ready/off?seconds=0", &[("authorization", "Bearer s3cret")]);
    assert_eq!(response.status, 200);
    assert_eq!(server.status("/ready"), 503);
}

#[test]
fn the_token_is_not_echoed_nor_shown() {
    let server = Server::with(&[("TOKEN", "s3cret")]);
    let echo = server.get_with("/echo", &[("authorization", "Bearer s3cret")]);
    for text in [echo.body, server.text("/config"), server.text("/info"), server.text("/help"), server.text("/metrics"), server.output()] {
        assert!(!text.contains("s3cret"));
    }
}
