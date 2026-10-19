//! `/scenario`: a list of steps that runs by itself, is checked before it starts and can be stopped.
mod common;

use common::*;
use serde_json::Value;
use std::time::Duration;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(10))).build().into()
}

// What the server answered to a request with a method other than GET, as its status and text
fn send(server: &Server, method: &str, body: &str, token: Option<&str>) -> (u16, String) {
    let url = format!("{}/scenario", server.url());
    let authorization = token.map(|token| format!("Bearer {token}"));
    let mut response = if method == "POST" {
        let mut request = agent().post(&url);
        if let Some(authorization) = &authorization {
            request = request.header("authorization", authorization);
        }
        request.send(body.as_bytes()).unwrap()
    } else {
        let mut request = agent().delete(&url);
        if let Some(authorization) = &authorization {
            request = request.header("authorization", authorization);
        }
        request.call().unwrap()
    };
    (response.status().as_u16(), response.body_mut().read_to_string().unwrap())
}

fn state(server: &Server) -> Value {
    server.json("/scenario")
}

#[test]
fn the_steps_run_one_after_the_other_by_themselves() {
    let server = Server::start();
    assert_eq!(state(&server), serde_json::json!({"state": "idle"}));

    let (status, body) = send(&server, "POST", r#"[{"cpu":50,"seconds":1},{"ready":"off","seconds":1},{"seconds":1}]"#, None);
    assert_eq!(status, 202, "{body}");
    let started: Value = serde_json::from_str(&body).unwrap();
    assert_eq!((started["state"].as_str(), started["step"].as_u64(), started["steps"].as_u64()), (Some("running"), Some(1), Some(3)));
    assert_eq!(started["plan"][0]["state"], "running");
    assert_eq!(started["plan"][2]["state"], "pending");
    wait_for("the first step to burn the CPU", 3000, || server.info()["stress"]["burn"].is_object());
    assert_eq!(server.info()["scenario"]["state"], "running", "/info says it too");

    wait_for("the second step", 3000, || state(&server)["step"] == 2);
    wait_for("the second step to make it not ready", 3000, || server.status("/ready") == 503);
    wait_for("the end", 5000, || state(&server)["state"] == "finished");
    assert_eq!(server.status("/ready"), 200, "and the step ends by giving it back");
    assert!(server.info()["stress"]["burn"].is_null());
    let done = state(&server);
    assert!(done["plan"].as_array().unwrap().iter().all(|step| step["state"] == "done"));
    assert!(server.output().contains("Scenario step 2 of 3: not ready for 1 s"), "{}", server.output());
}

#[test]
fn a_wrong_scenario_is_refused_whole_and_says_which_step() {
    let server = Server::start();
    for (body, message) in [
        (r#"[{"cpu":50,"seconds":1},{"memory":301,"seconds":1}]"#, "step 2: memory must be between 1 and 300"),
        (r#"[{"crash":1,"seconds":1}]"#, "unknown key 'crash'"),
        (r#"[{"oom":1,"seconds":1}]"#, "unknown key 'oom'"),
        (r#"[]"#, "no steps"),
        ("not json", "not JSON"),
    ] {
        let (status, text) = send(&server, "POST", body, None);
        assert_eq!(status, 400, "{body}");
        assert!(text.contains(message), "{body}: {text}");
    }
    assert_eq!(state(&server)["state"], "idle", "nothing started");
    assert!(server.info()["stress"]["burn"].is_null());
}

#[test]
fn only_one_runs_and_delete_stops_it_and_undoes_the_readiness() {
    let server = Server::start();
    assert_eq!(send(&server, "DELETE", "", None).0, 404, "nothing to stop");

    assert_eq!(send(&server, "POST", r#"[{"ready":"off","seconds":20},{"seconds":5}]"#, None).0, 202);
    wait_for("the step to make it not ready", 3000, || server.status("/ready") == 503);
    let (status, text) = send(&server, "POST", r#"[{"seconds":1}]"#, None);
    assert_eq!(status, 409);
    assert!(text.contains("at step 1 of 2"), "{text}");

    let (status, text) = send(&server, "DELETE", "", None);
    assert_eq!(status, 200, "{text}");
    let stopped: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(stopped["plan"][0]["state"], "stopped");
    assert_eq!(server.status("/ready"), 200, "stopping it undoes the step");
    sleep(300);
    assert_eq!(state(&server)["state"], "stopped", "and nothing goes on");

    assert_eq!(send(&server, "POST", r#"[{"seconds":1}]"#, None).0, 202, "a new one can start");
}

#[test]
fn a_step_that_does_not_fit_stops_the_scenario_and_says_why() {
    let server = Server::start();
    // the whole budget of memory is held by someone else when the step comes
    assert_eq!(server.status("/memory?mb=300&hold=10"), 200);
    assert_eq!(send(&server, "POST", r#"[{"memory":100,"seconds":1},{"seconds":1}]"#, None).0, 202);
    wait_for("the failure", 3000, || state(&server)["state"] == "failed");
    let failed = state(&server);
    assert_eq!(failed["step"], 1);
    // the reason is what the endpoint answers to the same request, with the step in front
    let endpoint = server.get("/memory?mb=100&hold=1");
    assert_eq!(endpoint.status, 429);
    assert_eq!(failed["error"], format!("step 1 (memory): {}", endpoint.body), "{failed}");
    assert_eq!(failed["plan"][1]["state"], "pending");
}

#[test]
fn the_token_protects_all_three() {
    let server = Server::with(&[("TOKEN", "s3cret")]);
    assert_eq!(server.status("/scenario"), 401);
    assert_eq!(send(&server, "POST", r#"[{"seconds":1}]"#, None).0, 401);
    assert_eq!(send(&server, "DELETE", "", None).0, 401);
    assert_eq!(send(&server, "POST", r#"[{"seconds":1}]"#, Some("s3cret")).0, 202);
    assert_eq!(server.info()["scenario"]["state"], "running", "/info stays open");
}
