//! The commands of the binary besides the server: `watch`, `healthcheck` and the ones that start a test.
mod common;

use common::*;

fn watch_once(server: &Server) -> String {
    let output = run(&["watch", "--once", "--url", &server.url()], &[]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

// The line of the view that starts with `label`
fn line<'a>(view: &'a str, label: &str) -> &'a str {
    view.lines().find(|line| line.starts_with(label)).unwrap_or_else(|| panic!("no {label} in\n{view}"))
}

#[test]
fn watch_once_prints_the_figures_of_a_running_server() {
    let server = Server::start();
    server.get("/memory?mb=100&hold=5");
    let view = watch_once(&server);

    assert!(view.starts_with(&format!("torchio {VERSION}  {}  up ", server.url())), "{view}");
    assert!(
        line(&view, "CPU workers").ends_with(&format!("{}{}", "0 busy / 2", " ".repeat(16)))
            || line(&view, "CPU workers").contains("0 busy / 2")
    );
    assert!(line(&view, "Memory held").contains("100 / 300 MB"));
    assert!(line(&view, "Disk held").contains("0 / 300 MB"));
    assert!(line(&view, "FDs held").contains("0 / 10000"));
    assert!(line(&view, "Process RSS").contains("MB (no limit)") || line(&view, "Process RSS").contains("(container)"));
    assert!(view.contains("/stress in progress: 0   /delay waiting: 0"));
    assert!(view.contains("readiness: ready"));
    assert!(view.contains(&format!("disk dir: {}", server.disk_dir.path().display())) || view.contains("disk dir: …"));
}

#[test]
fn watch_once_shows_when_the_server_is_not_ready_and_the_descriptors_held() {
    let server = Server::start();
    server.get("/ready/off?seconds=20");
    server.get("/fd?count=40&hold=5");
    let view = watch_once(&server);
    assert!(line(&view, "FDs held").contains("40 / 10000"));
    assert!(view.contains("readiness: NOT READY (ready again in "), "{view}");
}

#[test]
fn watch_once_shows_a_server_that_stays_not_ready() {
    let server = Server::start();
    server.get("/ready/off?seconds=0");
    assert!(watch_once(&server).contains("readiness: NOT READY (until /ready/on)"));
}

#[test]
fn watch_once_shows_what_cpu_is_doing() {
    let server = Server::start();
    server.get("/cpu?percent=30&hold=20");
    let view = watch_once(&server);
    let cpu = view.lines().find(|l| l.starts_with("/cpu: ")).unwrap_or_else(|| panic!("{view}"));
    assert!(cpu.starts_with("/cpu: 30% on every worker, "), "{cpu}");
}

#[test]
fn watch_once_shows_what_log_is_doing() {
    let server = Server::start();
    server.get("/log?lines=1000&rate=10");
    let view = watch_once(&server);
    let log = view.lines().find(|l| l.starts_with("/log: ")).unwrap_or_else(|| panic!("{view}"));
    assert!(log.ends_with(" of 1000 lines, 10 lines/s"), "{log}");
}

#[test]
fn watch_once_shows_the_requests_in_progress() {
    let server = Server::start();
    let url = server.url();
    let slow = std::thread::spawn(move || try_get(&format!("{url}/delay?ms=1500"), &[]));
    wait_for("the delay to be counted", 2000, || server.info()["delay"]["inFlight"] == 1);
    assert!(watch_once(&server).contains("/delay waiting: 1"));
    slow.join().unwrap().unwrap();
}

#[test]
fn watch_once_works_with_a_token_because_info_is_open() {
    let server = Server::with(&[("TOKEN", "s3cret")]);
    assert!(watch_once(&server).contains("torchio"));
}

#[test]
fn watch_once_has_no_colours_when_it_is_not_a_terminal() {
    let server = Server::start();
    assert!(!watch_once(&server).contains('\x1b'));
}

#[test]
fn watch_once_exits_with_1_when_the_server_cannot_be_reached() {
    let url = format!("http://127.0.0.1:{}", free_port());
    let output = run(&["watch", "--once", "--url", &url], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with(&format!("Cannot reach {url}: ")));
    assert!(output.stdout.is_empty());
}

#[test]
fn watch_once_says_when_the_address_is_not_a_server_of_torchio() {
    let server = Server::start();
    let url = format!("{}/status/200", server.url());
    // the path of the url is ignored: /info is asked to the server
    assert!(run(&["watch", "--once", "--url", &url], &[]).status.success());

    let other = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = other.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = other.accept() {
            let mut buffer = [0u8; 1024];
            let _ = std::io::Read::read(&mut stream, &mut buffer);
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}");
        }
    });
    let output = run(&["watch", "--once", "--url", &format!("http://127.0.0.1:{port}")], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not the /info of torchio"));
}

#[test]
fn watch_reads_the_port_from_the_environment() {
    let server = Server::start();
    let output = run(&["watch", "--once"], &[("PORT", &server.port.to_string())]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn watch_prints_a_frame_each_interval() {
    let server = Server::start();
    let mut child = std::process::Command::new(BIN)
        .args(["watch", "--interval", "0.2", "--url", &server.url()])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    sleep(1100);
    child.kill().unwrap();
    let output = child.wait_with_output().unwrap();
    let frames = String::from_utf8_lossy(&output.stdout).matches("\ntorchio ").count();
    assert!((3..=7).contains(&frames), "{frames} frames");
}

#[test]
fn unknown_commands_and_options_print_the_usage_and_exit_with_1() {
    for args in [&["nope"][..], &["watch", "--nope"], &["watch", "--url"]] {
        let output = run(args, &[]);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("Usage: torchio watch"), "{args:?}");
    }
}

#[test]
fn help_prints_the_usage_and_exits_with_0() {
    for args in [&["help"][..], &["--help"], &["-h"], &["watch", "--help"], &["watch", "-h"]] {
        let output = run(args, &[]);
        assert_eq!(output.status.code(), Some(0), "{args:?}");
        let usage = String::from_utf8_lossy(&output.stdout);
        assert!(usage.starts_with("Usage: torchio watch") && usage.contains("torchio healthcheck"), "{args:?}");
    }
}

#[test]
fn healthcheck_exits_with_0_when_the_server_answers_and_with_1_when_it_does_not() {
    let mut server = Server::start();
    let port = server.port.to_string();
    assert_eq!(run(&["healthcheck"], &[("PORT", &port)]).status.code(), Some(0));

    server.get("/crash?code=0");
    server.wait_exit(3000);
    let down = run(&["healthcheck"], &[("PORT", &port)]);
    assert_eq!(down.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&down.stderr).starts_with("unhealthy: "));
}

#[test]
fn healthcheck_does_not_depend_on_readiness() {
    let server = Server::start();
    server.get("/ready/off?seconds=0");
    assert_eq!(
        run(&["healthcheck"], &[("PORT", &server.port.to_string())]).status.code(),
        Some(0),
        "an instance that is not ready is alive"
    );
}

#[test]
fn an_action_command_starts_the_test_on_the_server() {
    let server = Server::start();
    let output = run(&["memory", "100", "--hold", "5", "--url", &server.url()], &[]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("100"));
    assert_eq!(server.info()["memory"]["heldMb"], 100);

    assert!(run(&["cpu", "30", "--hold=20", "--url", &server.url()], &[]).status.success());
    assert!(run(&["ready", "off", "20", "--url", &server.url()], &[]).status.success());
    assert_eq!(server.status("/ready"), 503);
    assert!(run(&["ready", "on", "--url", &server.url()], &[]).status.success());
    assert_eq!(server.status("/ready"), 200);
}

#[test]
fn an_action_command_fails_with_what_the_server_said() {
    let server = Server::start();
    let output = run(&["memory", "99999", "--url", &server.url()], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("HTTP 400: "));

    let output = run(&["status", "503", "--url", &server.url()], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("HTTP 503: "));
}

#[test]
fn an_action_command_sends_the_token() {
    let server = Server::with(&[("TOKEN", "s3cret")]);
    let output = run(&["memory", "10", "--hold", "5", "--url", &server.url()], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("asks for a token"));

    assert!(run(&["memory", "10", "--hold", "5", "--url", &server.url(), "--token", "s3cret"], &[]).status.success());
    assert!(run(&["memory", "10", "--hold", "5", "--url", &server.url()], &[("TOKEN", "s3cret")]).status.success());
}

#[test]
fn an_action_command_exits_with_1_when_the_server_cannot_be_reached() {
    let url = format!("http://127.0.0.1:{}", free_port());
    let output = run(&["cpu", "10", "--url", &url], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with(&format!("Cannot reach {url}: ")));
}
