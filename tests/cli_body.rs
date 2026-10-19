//! `torchio scenario` and `torchio payload`, the commands that send or read a body, against a real server.
mod common;

use common::*;

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn scenario_starts_a_file_shows_where_it_is_and_stops_it() {
    let server = Server::start();
    let dir = TempDir::new();
    let file = dir.path().join("scenario.json");
    std::fs::write(&file, r#"[{"ready":"off","seconds":20},{"seconds":5}]"#).unwrap();
    let file = file.to_str().unwrap();
    let url = server.url();

    let started = run(&["scenario", file, "--url", &url], &[]);
    assert!(started.status.success(), "{}", text(&started.stderr));
    assert!(text(&started.stdout).contains("\"state\": \"running\""), "the answer is written out: {}", text(&started.stdout));
    wait_for("the step to make it not ready", 3000, || server.status("/ready") == 503);

    let status = run(&["scenario", "status", "--url", &url], &[]);
    assert!(text(&status.stdout).contains("\"step\": 1"), "{}", text(&status.stdout));

    let again = run(&["scenario", file, "--url", &url], &[]);
    assert_eq!(again.status.code(), Some(1), "one at a time");
    assert!(text(&again.stderr).starts_with("HTTP 409: A scenario is already running"), "{}", text(&again.stderr));

    let stopped = run(&["scenario", "stop", "--url", &url], &[]);
    assert!(stopped.status.success());
    assert!(text(&stopped.stdout).contains("\"state\": \"stopped\""));
    assert_eq!(server.status("/ready"), 200);
    assert_eq!(run(&["scenario", "stop", "--url", &url], &[]).status.code(), Some(1), "nothing to stop");
}

#[test]
fn scenario_refuses_a_wrong_file_with_the_step_and_a_missing_one() {
    let server = Server::start();
    let dir = TempDir::new();
    let file = dir.path().join("wrong.json");
    std::fs::write(&file, r#"[{"seconds":1},{"memory":9999,"seconds":1}]"#).unwrap();

    let refused = run(&["scenario", file.to_str().unwrap(), "--url", &server.url()], &[]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(text(&refused.stderr).contains("step 2: memory must be between 1 and 300"), "{}", text(&refused.stderr));

    let missing = run(&["scenario", "/no/such/scenario.json", "--url", &server.url()], &[]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(text(&missing.stderr).starts_with("Cannot read /no/such/scenario.json"));
    assert_eq!(run(&["scenario", "--url", &server.url()], &[]).status.code(), Some(1), "a word is needed");
}

#[test]
fn scenario_with_wait_ends_when_the_scenario_does_and_says_how() {
    let server = Server::start();
    let dir = TempDir::new();
    let file = dir.path().join("short.json");
    // two seconds each: the command asks once a second, and a step of one could pass between two questions
    std::fs::write(&file, r#"[{"seconds":2},{"seconds":2}]"#).unwrap();
    let done = run(&["scenario", file.to_str().unwrap(), "--wait", "--url", &server.url()], &[]);
    assert!(done.status.success(), "{}", text(&done.stderr));
    assert!(text(&done.stderr).contains("The scenario finished"), "{}", text(&done.stderr));
    assert!(text(&done.stderr).contains("step 2 of 2"), "the steps are followed: {}", text(&done.stderr));

    // a step that can't have its budget fails the scenario, and the command with it
    assert_eq!(server.status("/memory?mb=300&hold=10"), 200);
    let failing = dir.path().join("failing.json");
    std::fs::write(&failing, r#"[{"memory":100,"seconds":1}]"#).unwrap();
    let failed = run(&["scenario", failing.to_str().unwrap(), "--wait", "--url", &server.url()], &[]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(text(&failed.stderr).contains("The scenario failed: step 1 (memory): 300 MB already held"), "{}", text(&failed.stderr));
}

#[test]
fn scenario_takes_the_token_from_the_option_or_the_environment() {
    let server = Server::with(&[("TOKEN", "s3cret")]);
    let dir = TempDir::new();
    let file = dir.path().join("pause.json");
    std::fs::write(&file, r#"[{"seconds":1}]"#).unwrap();
    let file = file.to_str().unwrap();

    let without = run(&["scenario", file, "--url", &server.url()], &[]);
    assert_eq!(without.status.code(), Some(1));
    assert!(text(&without.stderr).contains("The server asks for a token"), "{}", text(&without.stderr));
    assert!(run(&["scenario", file, "--url", &server.url(), "--token", "s3cret"], &[]).status.success());
    assert!(run(&["scenario", "stop", "--url", &server.url()], &[("TOKEN", "s3cret")]).status.success());
}

#[test]
fn payload_get_downloads_and_reports_the_speed() {
    let server = Server::start();
    let output = run(&["payload", "get", "3", "--url", &server.url()], &[]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(text(&output.stdout).starts_with("Received 3.0 MB in "), "{}", text(&output.stdout));
    assert!(text(&output.stdout).contains("MB/s)"));
    assert!(run(&["payload", "get", "--url", &server.url()], &[]).status.success(), "the default size");

    let refused = run(&["payload", "get", "99999", "--url", &server.url()], &[]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(text(&refused.stderr).starts_with("HTTP 400: mb must be between 1 and"), "{}", text(&refused.stderr));
}

#[test]
fn payload_post_sends_a_file_and_says_what_the_server_counted() {
    let server = Server::with(&[("MAX_PAYLOAD_MB", "2")]);
    let dir = TempDir::new();
    let file = dir.path().join("body.bin");
    std::fs::write(&file, vec![7u8; 1024 * 1024 + 5]).unwrap();

    let output = run(&["payload", "post", file.to_str().unwrap(), "--url", &server.url()], &[]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(text(&output.stdout).contains("the server counted 1048581 bytes"), "{}", text(&output.stdout));

    // more than the limit: a 413, or a connection that closed before the answer was read
    let big = dir.path().join("big.bin");
    std::fs::write(&big, vec![0u8; 3 * 1024 * 1024]).unwrap();
    let refused = run(&["payload", "post", big.to_str().unwrap(), "--url", &server.url()], &[]);
    assert_eq!(refused.status.code(), Some(1));
    let message = text(&refused.stderr);
    assert!(message.starts_with("HTTP 413") || message.contains("The connection was closed while the body was being sent"), "{message}");
    assert_eq!(server.status("/"), 200);
}

#[test]
fn payload_needs_its_words() {
    let server = Server::start();
    let url = server.url();
    for args in [&["payload"][..], &["payload", "put"], &["payload", "post"], &["payload", "get", "1", "2"]] {
        let mut full = args.to_vec();
        full.extend(["--url", &url]);
        let output = run(&full, &[]);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(text(&output.stderr).starts_with("Usage: torchio payload"), "{args:?}: {}", text(&output.stderr));
    }
}
