//! `install.sh`, against a release made of local files: the right binary is installed when the
//! checksum matches, and nothing is written when it does not.
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

fn available(program: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {program}")]).output().is_ok_and(|output| output.status.success())
}

/// The name the release gives to the binary for this system
fn asset() -> String {
    let os = if cfg!(target_os = "macos") { "macos" } else { "linux" };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    format!("torchio-{os}-{arch}")
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A release with a fake binary for this system and the others, and its checksums
fn release(dir: &Path, binary: &[u8], checksum_of: &[u8]) {
    fs::create_dir_all(dir).unwrap();
    let mut sums = String::new();
    for name in ["torchio-linux-amd64", "torchio-linux-arm64", "torchio-macos-amd64", "torchio-macos-arm64"] {
        fs::write(dir.join(name), binary).unwrap();
        sums.push_str(&format!("{}  {name}\n", hex(checksum_of)));
    }
    fs::write(dir.join("SHA256SUMS"), sums).unwrap();
}

fn install(release: &Path, into: &Path) -> Output {
    Command::new("sh")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh"))
        .env("TORCHIO_BASE_URL", format!("file://{}", release.display()))
        .env("TORCHIO_INSTALL_DIR", into)
        .output()
        .unwrap()
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("torchio-install-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[test]
fn installs_the_binary_of_this_system_when_the_checksum_matches() {
    if !available("curl") {
        return;
    }
    let (release_dir, into) = (temp("ok-release"), temp("ok-bin"));
    release(&release_dir, b"#!/bin/sh\necho fake torchio\n", b"#!/bin/sh\necho fake torchio\n");
    let output = install(&release_dir, &into);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains(&format!("Downloading {}", asset())));

    let installed = into.join("torchio");
    assert_eq!(fs::read(&installed).unwrap(), b"#!/bin/sh\necho fake torchio\n");
    assert_eq!(fs::metadata(&installed).unwrap().permissions().mode() & 0o755, 0o755);
    assert_eq!(String::from_utf8_lossy(&Command::new(&installed).output().unwrap().stdout), "fake torchio\n");
}

#[test]
fn installs_nothing_when_the_checksum_does_not_match() {
    if !available("curl") {
        return;
    }
    let (release_dir, into) = (temp("bad-release"), temp("bad-bin"));
    release(&release_dir, b"what was published", b"something else");
    let output = install(&release_dir, &into);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains(&format!("checksum mismatch for {}", asset())));
    assert!(!into.join("torchio").exists(), "a binary that fails the check is never installed");
}

#[test]
fn says_when_the_release_cannot_be_downloaded() {
    if !available("curl") {
        return;
    }
    let output = install(&temp("missing-release"), &temp("missing-bin"));
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("torchio install: cannot download "));
}

#[test]
fn help_prints_the_variables() {
    let output = Command::new("sh").arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh")).arg("--help").output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success() && text.contains("TORCHIO_VERSION") && text.contains("TORCHIO_INSTALL_DIR"), "{text}");
}
