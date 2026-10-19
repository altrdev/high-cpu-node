//! What the server holds on purpose: memory (`/memory`, `/leak`), disk (`/disk`) and file
//! descriptors (`/fd`), each against its budget.
mod common;

use common::*;

#[test]
fn memory_is_held_within_its_budget_and_released() {
    let server = Server::start();
    assert_eq!(server.text("/memory?mb=100&hold=1"), "Allocated 100 MB, held for 1 s");
    assert_eq!(server.info()["memory"]["heldMb"], 100);

    let over = server.get("/memory?mb=250");
    assert_eq!(over.status, 429, "over the budget left");
    assert_eq!(over.body, "100 MB already held, limit is 300 MB");

    wait_for("the memory to be released", 4000, || server.info()["memory"]["heldMb"] == 0);
    assert_eq!(server.status("/memory?mb=250&hold=0"), 200, "the budget is free again");
}

#[test]
fn memory_checks_its_parameters() {
    let server = Server::start();
    assert_eq!(server.status("/memory?mb=301"), 400);
    assert_eq!(server.status("/memory?mb=0"), 400);
    assert_eq!(server.status("/memory?hold=31"), 400);
    assert_eq!(server.info()["memory"]["heldMb"], 0, "a rejected request holds nothing");
}

#[test]
fn memory_with_no_hold_is_released_right_away() {
    let server = Server::start();
    assert_eq!(server.status("/memory?mb=200&hold=0"), 200);
    wait_for("the memory to be released", 1000, || server.info()["memory"]["heldMb"] == 0);
}

#[test]
fn memory_counts_as_used_and_goes_back_to_the_os() {
    let server = Server::with(&[("WORKERS", "1")]);
    let base = server.rss_kb();
    assert_eq!(server.status("/memory?mb=200&hold=1"), 200);
    assert!(server.rss_kb() - base > 150_000, "the pages must be written, or the OS does not count them");

    wait_for("the memory to go back to the OS", 4000, || server.rss_kb().saturating_sub(base) < 20_000);
}

#[test]
fn the_budget_of_memory_is_shared_with_leak() {
    let server = Server::start();
    server.get("/memory?mb=250&hold=4");
    server.get("/leak?rate=40&mb=200&hold=3");
    sleep(1500);
    // 250 + 40 fits in the 300 MB budget, the next 40 does not: it stops there
    assert_eq!(server.info()["memory"]["heldMb"], 290);
    wait_for("everything to be released", 6000, || server.info()["memory"]["heldMb"] == 0);

    server.get("/memory?mb=300&hold=2");
    assert_eq!(server.status("/leak?mb=10"), 429);
}

#[test]
fn leak_grows_by_itself_and_releases() {
    let server = Server::start();
    assert_eq!(server.text("/leak?rate=20&mb=60&hold=1"), "Leaking 20 MB/s up to 60 MB (3 s), then held for 1 s");
    assert_eq!(server.info()["memory"]["heldMb"], 20, "the first step is taken right away");

    wait_for("the leak to reach its ceiling", 4000, || server.info()["memory"]["heldMb"] == 60);
    wait_for("the leak to be released", 4000, || server.info()["memory"]["heldMb"] == 0);
}

#[test]
fn leak_checks_its_parameters() {
    let server = Server::start();
    assert_eq!(server.status("/leak?mb=301"), 400);
    assert_eq!(server.status("/leak?rate=0"), 400);
    assert_eq!(server.status("/leak?hold=31"), 400);
}

#[test]
fn disk_writes_a_file_keeps_it_and_removes_it() {
    let server = Server::start();
    let dir = server.disk_dir.path().display().to_string();
    assert_eq!(server.text("/disk?mb=10&hold=1"), format!("Wrote 10 MB to {dir}, held for 1 s"));

    let files = server.disk_files();
    assert_eq!(files.len(), 1);
    assert_eq!(std::fs::metadata(server.disk_dir.path().join(&files[0])).unwrap().len(), 10 * MB);
    assert_eq!(server.info()["disk"]["heldMb"], 10);

    wait_for("the file to be removed", 4000, || server.disk_files().is_empty());
    assert_eq!(server.info()["disk"]["heldMb"], 0);
}

#[test]
fn disk_answers_429_when_the_budget_is_used_up() {
    let server = Server::start();
    assert_eq!(server.status("/disk?mb=200&hold=1"), 200);
    let over = server.get("/disk?mb=200");
    assert_eq!(over.status, 429);
    assert_eq!(over.body, "200 MB already held, limit is 300 MB");
    wait_for("the file to be removed", 4000, || server.disk_files().is_empty());
}

#[test]
fn disk_checks_its_parameters() {
    let server = Server::start();
    assert_eq!(server.status("/disk?mb=301"), 400);
    assert_eq!(server.status("/disk?mb=0"), 400);
    assert!(server.disk_files().is_empty(), "a rejected request writes nothing");
}

#[test]
fn disk_answers_507_when_the_directory_is_not_usable() {
    let server = Server::with(&[("DISK_DIR", "/dev/null/stress")]);
    assert!(server.output().contains("DISK_DIR /dev/null/stress is not usable, /disk will answer 507"));

    let response = server.get("/disk?mb=1");
    assert_eq!(response.status, 507);
    assert!(response.body.starts_with("Could not write 1 MB to "), "{}", response.body);
    let info = server.info();
    assert!(info["disk"]["freeMb"].is_null());
    assert_eq!(info["disk"]["heldMb"], 0, "the budget is given back");
    assert_eq!(server.text("/"), "Status: OK");
}

#[test]
fn disk_removes_the_leftovers_of_a_previous_run_and_keeps_other_files() {
    let server = Options::default()
        .prepare(|dir| {
            std::fs::write(dir.join("torchio-disk-old"), "leftover").unwrap();
            std::fs::write(dir.join("keep.txt"), "mine").unwrap();
        })
        .start();
    let names: Vec<_> = std::fs::read_dir(server.disk_dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["keep.txt"]);
}

#[test]
fn disk_creates_its_directory() {
    let parent = TempDir::new();
    let nested = parent.path().join("a").join("b");
    let server = Server::with(&[("DISK_DIR", &nested.to_string_lossy())]);
    assert!(nested.is_dir());
    assert_eq!(server.status("/disk?mb=1&hold=0"), 200);
}

#[test]
fn disk_removes_the_files_it_still_holds_on_shutdown() {
    let mut server = Server::start();
    assert_eq!(server.status("/disk?mb=5&hold=30"), 200);
    assert_eq!(server.disk_files().len(), 1);

    assert!(server.terminate().success());
    assert!(server.disk_files().is_empty());
}

#[test]
fn fd_holds_descriptors_within_its_budget_and_releases_them() {
    let server = Server::with(&[("MAX_FD", "50")]);
    assert_eq!(server.text("/fd?count=30&hold=1"), "Opened 30 descriptors, held for 1 s");
    assert_eq!(server.info()["fd"]["held"], 30);

    let over = server.get("/fd?count=30&hold=1");
    assert_eq!(over.status, 429);
    assert_eq!(over.body, "30 descriptors already held, limit is 50");
    assert_eq!(server.status("/fd?count=51"), 400);
    assert_eq!(server.status("/fd?count=0"), 400);

    wait_for("the descriptors to be released", 3000, || server.info()["fd"]["held"] == 0);
    assert_eq!(metric(&server.text("/metrics"), "torchio_fd_limit"), Some(50.0));
}
