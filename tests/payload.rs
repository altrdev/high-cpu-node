//! `/payload`: data streamed to the client, and an upload that is thrown away.
mod common;

use common::*;
use std::io::Read;
use std::time::Duration;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(20))).build().into()
}

#[test]
fn get_streams_exactly_the_megabytes_asked_for() {
    let server = Server::start();
    let mut response = agent().get(&format!("{}/payload?mb=3", server.url())).call().unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.headers()["content-type"], "application/octet-stream");
    assert_eq!(response.headers()["content-length"], (3 * MB).to_string());
    let mut size = 0;
    let mut buffer = [0u8; 64 * 1024];
    let mut reader = response.body_mut().as_reader();
    loop {
        let read = reader.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        size += read as u64;
    }
    assert_eq!(size, 3 * MB);
}

#[test]
fn get_has_a_limit_and_a_default() {
    let server = Server::with(&[("MAX_PAYLOAD_MB", "2")]);
    assert_eq!(server.status("/payload?mb=0"), 400);
    assert_eq!(server.status("/payload?mb=3"), 400);
    assert_eq!(server.json("/config")["maxPayloadMb"], 2);
    let response = agent().get(&format!("{}/payload", server.url())).call().unwrap();
    assert_eq!(response.headers()["content-length"], (2 * MB).to_string(), "the default is cut to the limit");
}

#[test]
fn post_discards_the_upload_and_reports_its_size() {
    let server = Server::start();
    let body = vec![7u8; 2 * MB as usize + 123];
    let mut response = agent().post(&format!("{}/payload", server.url())).send(&body[..]).unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let report: serde_json::Value = serde_json::from_str(&response.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(report["bytes"], body.len());
    assert!(report["seconds"].as_f64().unwrap() >= 0.0);
    assert!(report["mbPerSecond"].as_f64().is_some());
    assert!(server.rss_kb() < 200 * 1024, "the upload must not be kept: {} kB", server.rss_kb());
}

#[test]
fn post_over_the_limit_is_a_413() {
    let server = Server::with(&[("MAX_PAYLOAD_MB", "1")]);
    let body = vec![0u8; 2 * MB as usize];
    // the server answers as soon as the limit is passed and stops reading, so a client that is still
    // sending may see the connection closed before it reads the 413: both are the limit at work
    match agent().post(&format!("{}/payload", server.url())).send(&body[..]) {
        Ok(mut response) => {
            assert_eq!(response.status().as_u16(), 413);
            assert!(response.body_mut().read_to_string().unwrap().contains("the limit is 1 MB"));
        }
        Err(error) => eprintln!("the connection was closed before the 413 was read: {error}"),
    }
    assert_eq!(server.status("/"), 200, "the server is still there");
}

#[test]
fn the_token_protects_both_directions() {
    let server = Server::with(&[("TOKEN", "s3cret")]);
    assert_eq!(server.status("/payload?mb=1"), 401);
    assert_eq!(agent().post(&format!("{}/payload", server.url())).send(&b"x"[..]).unwrap().status().as_u16(), 401);
    let allowed = agent().post(&format!("{}/payload", server.url())).header("authorization", "Bearer s3cret").send(&b"x"[..]).unwrap();
    assert_eq!(allowed.status().as_u16(), 200);
}
