//! The control panel at `/ui`.
mod common;

use common::*;

#[test]
fn it_is_not_there_unless_ui_is_on() {
    let server = Server::start();
    assert_eq!(server.status("/ui"), 404);
    assert!(server.json("/help")["endpoints"].as_array().unwrap().iter().all(|e| e["path"] != "/ui"));
}

#[test]
fn it_serves_the_control_panel_with_ui_on() {
    let server = Server::with(&[("UI", "1")]);
    let response = server.get("/ui");
    assert_eq!(response.status, 200);
    assert!(response.header("content-type").unwrap().starts_with("text/html"));
    assert!(response.body.contains("<title>torchio</title>"));
    assert!(response.body.contains(r#"<link rel="icon""#));
    assert_eq!(response.body.matches("<script>").count(), 1, "the page is a single static file");
}

#[test]
fn the_page_sends_the_token_and_calls_every_endpoint_of_the_tests() {
    let server = Server::with(&[("UI", "1")]);
    let page = server.text("/ui");
    assert!(page.contains("Authorization"), "the page does not send the token");
    for path in [
        "/info",
        "/metrics",
        "/cpu",
        "/leak",
        "/memory",
        "/disk",
        "/fd",
        "/ready/off",
        "/ready/on",
        "/delay",
        "/crash",
        "/block",
        "/scenario",
        "/stress",
        "/connections",
        "/flaky",
        "/status/",
        "/payload",
    ] {
        let used = [format!("'{path}"), format!("path: '{path}"), format!("`{path}")].iter().any(|text| page.contains(text.as_str()));
        assert!(used, "the page does not use {path}");
    }
    // the one that ends the instance for memory has no button: it is not safe to be a click away
    assert!(!page.contains("/oom"), "the page must not offer /oom");
}

#[test]
fn ui_accepts_the_value_true_too() {
    assert_eq!(Server::with(&[("UI", "true")]).status("/ui"), 200);
    assert_eq!(Server::with(&[("UI", "yes")]).status("/ui"), 404, "only 1 and true turn it on");
}
