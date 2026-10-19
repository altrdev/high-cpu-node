//! `torchio completions <bash|zsh|fish>`: the script that makes the shell complete the commands and
//! the options of torchio. The commands that start a test come from the table of `action`, so a new
//! one is completed without touching this file.
use super::action;
use std::io::Write;
use std::process::ExitCode;

/// An option of a command; `value` is true when it is followed by a value
struct Opt {
    name: &'static str,
    value: bool,
}

/// A command with what can follow it
struct Entry {
    name: &'static str,
    about: &'static str,
    options: Vec<Opt>,
    /// Words that are not options: `on` and `off`, or the names of the shells
    words: Vec<&'static str>,
}

fn opt(name: &'static str, value: bool) -> Opt {
    Opt { name, value }
}

/// Every command of the binary and what it takes, in the order the shells list them: the server and
/// the view, the commands that start a test, then the rest
fn entries() -> Vec<Entry> {
    let mut entries = vec![
        Entry { name: "serve", about: "Start the server", options: vec![], words: vec![] },
        Entry {
            name: "watch",
            about: "Watch a server, with keys that start tests",
            options: vec![
                opt("url", true),
                opt("interval", true),
                opt("token", true),
                opt("once", false),
                opt("read-only", false),
                opt("help", false),
            ],
            words: vec![],
        },
    ];
    for command in action::commands() {
        let mut options: Vec<Opt> = command.params.iter().map(|param| opt(param, true)).collect();
        options.extend([opt("url", true), opt("token", true), opt("help", false)]);
        let words = if command.name == "ready" { vec!["on", "off"] } else { vec![] };
        entries.push(Entry { name: command.name, about: command.about, options, words });
    }
    for (name, about, words) in [
        ("scenario", "Run a scenario from a file, or see or stop it", vec!["status", "stop"]),
        ("payload", "Download or upload a large body and report the speed", vec!["get", "post"]),
    ] {
        let mut options = vec![opt("url", true), opt("token", true), opt("help", false)];
        if name == "scenario" {
            options.insert(0, opt("wait", false));
        }
        entries.push(Entry { name, about, options, words });
    }
    entries.push(Entry { name: "version", about: "Print the version", options: vec![], words: vec![] });
    entries.push(Entry { name: "healthcheck", about: "Exit with 0 when the server answers", options: vec![], words: vec![] });
    entries.push(Entry {
        name: "completions",
        about: "Print the completion script of a shell",
        options: vec![],
        words: vec!["bash", "zsh", "fish"],
    });
    entries.push(Entry { name: "help", about: "Show the usage", options: vec![], words: vec![] });
    entries
}

fn dashed(options: &[Opt]) -> Vec<String> {
    options.iter().map(|option| format!("--{}", option.name)).collect()
}

pub fn bash() -> String {
    let entries = entries();
    let names: Vec<&str> = entries.iter().map(|entry| entry.name).collect();
    let mut script = String::from(
        "# bash completion for torchio: eval \"$(torchio completions bash)\" in ~/.bashrc, or save it in your bash-completion folder\n_torchio() {\n",
    );
    script.push_str("    local cur=\"${COMP_WORDS[COMP_CWORD]}\" cmd=\"${COMP_WORDS[1]}\" opts words\n");
    script.push_str(&format!(
        "    if [ \"$COMP_CWORD\" -eq 1 ]; then\n        COMPREPLY=($(compgen -W \"{}\" -- \"$cur\"))\n        return\n    fi\n",
        names.join(" ")
    ));
    script.push_str("    case \"$cmd\" in\n");
    for entry in &entries {
        script.push_str(&format!(
            "        {}) opts=\"{}\"; words=\"{}\" ;;\n",
            entry.name,
            dashed(&entry.options).join(" "),
            entry.words.join(" ")
        ));
    }
    script.push_str("        *) return ;;\n    esac\n");
    script.push_str("    case \"$cur\" in\n        -*) COMPREPLY=($(compgen -W \"$opts\" -- \"$cur\")) ;;\n        *) COMPREPLY=($(compgen -W \"$words\" -- \"$cur\")) ;;\n    esac\n}\n");
    script.push_str("complete -F _torchio torchio\n");
    script
}

pub fn zsh() -> String {
    let entries = entries();
    let mut script = String::from(
        "#compdef torchio\n# zsh completion for torchio: save it as _torchio in a folder of your fpath, or source it\n_torchio() {\n",
    );
    script.push_str("    local -a commands opts words\n    commands=(\n");
    for entry in &entries {
        script.push_str(&format!("        '{}:{}'\n", entry.name, entry.about));
    }
    script.push_str(
        "    )\n    if (( CURRENT == 2 )); then\n        _describe -t commands 'torchio command' commands\n        return\n    fi\n",
    );
    script.push_str("    case $words[2] in\n");
    for entry in &entries {
        script.push_str(&format!(
            "        {}) opts=({}); words=({}) ;;\n",
            entry.name,
            dashed(&entry.options).join(" "),
            entry.words.join(" ")
        ));
    }
    script.push_str("        *) return ;;\n    esac\n");
    script.push_str("    if [[ $words[CURRENT] == -* ]]; then\n        compadd -a opts\n    else\n        compadd -a words\n    fi\n}\n");
    script.push_str("if [ \"$funcstack[1]\" = \"_torchio\" ]; then\n    _torchio \"$@\"\nelse\n    compdef _torchio torchio\nfi\n");
    script
}

pub fn fish() -> String {
    let entries = entries();
    let mut script =
        String::from("# fish completion for torchio: save it in ~/.config/fish/completions/torchio.fish\ncomplete -c torchio -f\n");
    for entry in &entries {
        script.push_str(&format!("complete -c torchio -n __fish_use_subcommand -a {} -d '{}'\n", entry.name, entry.about));
    }
    for entry in &entries {
        let seen = format!("-n '__fish_seen_subcommand_from {}'", entry.name);
        for option in &entry.options {
            script.push_str(&format!("complete -c torchio {seen} -l {}{}\n", option.name, if option.value { " -r" } else { "" }));
        }
        if !entry.words.is_empty() {
            script.push_str(&format!("complete -c torchio {seen} -a '{}'\n", entry.words.join(" ")));
        }
    }
    script
}

/// `torchio completions <shell>`
pub fn run(args: &[String]) -> ExitCode {
    let script = match args.first().map(String::as_str) {
        Some("bash") => bash(),
        Some("zsh") => zsh(),
        Some("fish") => fish(),
        other => {
            let shell = other.map_or(String::new(), |shell| format!(" '{shell}'"));
            eprintln!("Unknown shell{shell}: use bash, zsh or fish");
            return ExitCode::FAILURE;
        }
    };
    // a reader that goes away, like `head`, is not an error
    let _ = std::io::stdout().write_all(script.as_bytes());
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_in_every_script() {
        for name in action::names().into_iter().chain(["serve", "watch", "healthcheck", "completions", "version", "help"]) {
            for (shell, script) in [("bash", bash()), ("zsh", zsh()), ("fish", fish())] {
                assert!(script.contains(name), "{name} is missing from {shell}");
            }
        }
    }

    #[test]
    fn the_options_of_a_command_are_its_parameters_and_the_common_ones() {
        let cpu = bash().lines().find(|line| line.trim_start().starts_with("cpu)")).unwrap().to_string();
        assert!(cpu.contains("--percent --hold --workers --url --token --help"), "{cpu}");
        let ready = bash().lines().find(|line| line.trim_start().starts_with("ready)")).unwrap().to_string();
        assert!(ready.contains("--seconds") && !ready.contains("--state") && ready.contains("words=\"on off\""), "{ready}");
        assert!(fish().contains("-n '__fish_seen_subcommand_from watch' -l read-only\n"), "a flag takes no value");
        assert!(fish().contains("-n '__fish_seen_subcommand_from watch' -l interval -r\n"), "an option that takes a value says so");
    }

    #[test]
    fn the_descriptions_do_not_break_the_quotes_of_the_scripts() {
        for entry in entries() {
            assert!(!entry.about.contains(['\'', ':', '"']), "{}", entry.about);
        }
    }
}
