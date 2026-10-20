//! `torchio completions`: the scripts parse in their shell, and bash really completes with them.
mod common;

use common::*;
use std::process::Command;

fn script(shell: &str) -> String {
    let output = run(&["completions", shell], &[]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

/// What bash offers after `torchio <words>`, the last word being the one being typed
fn bash_offers(words: &[&str]) -> Vec<String> {
    let array = words.iter().map(|word| format!("'{word}'")).collect::<Vec<_>>().join(" ");
    let program = format!(
        "eval \"$({BIN} completions bash)\"; COMP_WORDS=(torchio {array}); COMP_CWORD={}; _torchio; printf '%s\\n' \"${{COMPREPLY[@]}}\"",
        words.len()
    );
    let output = Command::new("bash").args(["-c", &program]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    // printf with no word prints an empty line
    String::from_utf8(output.stdout).unwrap().lines().filter(|line| !line.is_empty()).map(String::from).collect()
}

#[test]
fn each_shell_gets_a_script_and_anything_else_is_an_error() {
    assert!(script("bash").contains("complete -F _torchio torchio"));
    assert!(script("zsh").starts_with("#compdef torchio"));
    assert!(script("fish").contains("complete -c torchio"));

    for args in [&["completions"][..], &["completions", "powershell"]] {
        let output = run(args, &[]);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("use bash, zsh or fish"));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn the_scripts_are_valid_in_their_shells() {
    for (shell, flag) in [("bash", "-n"), ("zsh", "-n")] {
        if !available(shell) {
            continue;
        }
        let dir = std::env::temp_dir().join(format!("torchio-completions-{shell}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("script");
        std::fs::write(&file, script(shell)).unwrap();
        let output = Command::new(shell).arg(flag).arg(&file).output().unwrap();
        assert!(output.status.success(), "{shell}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

#[test]
fn the_commands_are_listed_with_the_server_the_view_and_the_tests_before_the_rest() {
    let script = script("zsh");
    let position = |name: &str| script.find(&format!("'{name}:")).unwrap_or_else(|| panic!("{name} in\n{script}"));
    let order = ["serve", "watch", "stress", "ready", "healthcheck", "completions", "help"];
    for pair in order.windows(2) {
        assert!(position(pair[0]) < position(pair[1]), "{} before {}", pair[0], pair[1]);
    }
}

#[test]
fn a_reader_that_goes_away_is_not_a_failure() {
    let mut child = Command::new(BIN).args(["completions", "bash"]).stdout(std::process::Stdio::piped()).spawn().unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "no panic on a closed pipe");
}

#[test]
fn bash_completes_the_commands() {
    if !available("bash") {
        return;
    }
    assert_eq!(bash_offers(&["cp"]), ["cpu"]);
    let all = bash_offers(&[""]);
    for name in [
        "watch",
        "healthcheck",
        "completions",
        "help",
        "stress",
        "cpu",
        "delay",
        "status",
        "memory",
        "leak",
        "disk",
        "fd",
        "log",
        "crash",
        "block",
        "scenario",
        "payload",
        "ready",
    ] {
        assert!(all.contains(&name.to_string()), "{name} in {all:?}");
    }
}

#[test]
fn bash_completes_the_options_and_the_words_of_a_command() {
    if !available("bash") {
        return;
    }
    assert_eq!(bash_offers(&["cpu", "--h"]), ["--hold", "--help"]);
    assert_eq!(bash_offers(&["memory", "--"]), ["--mb", "--hold", "--url", "--token", "--help"]);
    assert_eq!(bash_offers(&["watch", "--re"]), ["--read-only"]);
    assert_eq!(bash_offers(&["ready", ""]), ["on", "off"]);
    assert_eq!(bash_offers(&["ready", "o"]), ["on", "off"]);
    assert_eq!(bash_offers(&["completions", "z"]), ["zsh"]);
    assert!(bash_offers(&["cpu", ""]).is_empty(), "a value is not completed");
}

#[test]
fn zsh_registers_the_completion_when_the_script_is_sourced() {
    if !available("zsh") {
        return;
    }
    let program =
        format!("autoload -U compinit && compinit -u -d /dev/null; eval \"$({BIN} completions zsh)\"; print -r -- $_comps[torchio]");
    let output = Command::new("zsh").args(["-f", "-c", &program]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "_torchio", "{}", String::from_utf8_lossy(&output.stderr));
}
