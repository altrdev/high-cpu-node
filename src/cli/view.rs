//! The view of `torchio watch`: bars, a short history of each figure and a few lines of text.
use super::info::{Info, Ready, Scenario};

const HISTORY: usize = 30;
const BAR_WIDTH: usize = 20;
const SPARK: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// The last samples of each figure
#[derive(Default)]
pub struct History {
    busy: Vec<i64>,
    held: Vec<i64>,
    rss: Vec<i64>,
    disk: Vec<i64>,
    fd: Vec<i64>,
    connections: Vec<i64>,
}

impl History {
    pub fn record(&mut self, info: &Info) {
        let samples = [
            (&mut self.busy, cpu_load(info)),
            (&mut self.held, info.memory.held_mb),
            (&mut self.rss, info.memory.process_rss_mb),
            (&mut self.disk, info.disk.held_mb),
            (&mut self.fd, info.fd.as_ref().map_or(0, |fd| fd.held)),
            (&mut self.connections, info.connections.as_ref().map_or(0, |c| c.held)),
        ];
        for (values, sample) in samples {
            values.push(sample);
            if values.len() > HISTORY {
                values.remove(0);
            }
        }
    }
}

/// The load of the workers in percent of one worker: a worker that runs a `/stress` is at 100, and
/// the others are at the percent of `/cpu` if it is burning them, which it does for the first ones
fn cpu_load(info: &Info) -> i64 {
    let burn = info.stress.burn.as_ref();
    let percent = burn.map_or(0, |burn| burn.percent.clamp(0, 100));
    let burned = burn.map_or(0, |burn| burn.workers.unwrap_or(info.stress.workers));
    (0..info.stress.workers)
        .map(|index| match info.stress.per_worker.get(index as usize) {
            Some(n) if *n > 0 => 100,
            _ if index < burned => percent,
            _ => 0,
        })
        .sum()
}

/// Which workers `/cpu` loads: every one, or how many of them
fn burned(info: &Info, burn: &super::info::Burn) -> String {
    match burn.workers {
        Some(n) if n < info.stress.workers => format!("{n} of {} workers", info.stress.workers),
        _ => "every worker".to_string(),
    }
}

/// Colours (256-colour palette), or plain text when `color` is off
pub struct Style {
    color: bool,
}

/// From green to red, for a bar that gets close to a limit
const HEAT: [u8; 10] = [35, 41, 77, 113, 149, 185, 221, 215, 209, 203];
/// From blue to cyan, for a bar that is only a size reference
const COOL: [u8; 6] = [27, 33, 39, 45, 51, 87];
/// The history, from low to high
const TRACE: [u8; 8] = [24, 30, 36, 42, 43, 79, 116, 159];
/// Partial cells of a bar, from one eighth to seven eighths
const EIGHTHS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];
/// The width of the separators, as wide as the rows
const WIDTH: usize = 92;

impl Style {
    pub fn new(color: bool) -> Style {
        Style { color }
    }

    pub fn is_color(&self) -> bool {
        self.color
    }

    /// `text` with the SGR parameters `sgr` (like `38;5;215` or `1`)
    pub fn paint(&self, sgr: &str, text: &str) -> String {
        if self.color && !text.is_empty() { format!("\x1b[{sgr}m{text}\x1b[0m") } else { text.to_string() }
    }

    fn fg(&self, code: u8, text: &str) -> String {
        self.paint(&format!("38;5;{code}"), text)
    }

    fn dim(&self, text: &str) -> String {
        self.fg(244, text)
    }

    /// Text on a coloured background, like a badge
    fn badge(&self, bg: u8, fg: u8, text: &str) -> String {
        self.paint(&format!("1;48;5;{bg};38;5;{fg}"), &format!(" {text} "))
    }

    /// A key and what it does, for the line of keys
    pub fn key(&self, key: &str, name: &str) -> String {
        if !self.color {
            return format!("{key} {name}");
        }
        format!("{} {}", self.badge(238, 215, key), self.dim(name))
    }

    /// Red, yellow or green, by how full a bar is
    fn level(ratio: f64) -> u8 {
        HEAT[((ratio.clamp(0.0, 1.0) * 9.0).round() as usize).min(9)]
    }

    /// A bar of `value` against `max`: with colours it is smooth, one eighth of a cell at a time, and
    /// each cell has the colour of its place (green to red, or blue to cyan when `neutral`); without
    /// them it is whole cells. `neutral` is for a size reference and not for a real limit.
    fn bar(&self, value: i64, max: i64, neutral: bool) -> String {
        let ratio = ratio(value, max);
        if !self.color {
            let filled = (ratio * BAR_WIDTH as f64).round() as usize;
            return "█".repeat(filled) + &"░".repeat(BAR_WIDTH - filled);
        }
        let eighths = (ratio * BAR_WIDTH as f64 * 8.0).round() as usize;
        let palette: &[u8] = if neutral { &COOL } else { &HEAT };
        let color_of = |cell: usize| palette[cell * (palette.len() - 1) / (BAR_WIDTH - 1)];
        let mut bar = String::new();
        for cell in 0..BAR_WIDTH {
            let glyph = match eighths.saturating_sub(cell * 8) {
                0 => self.fg(238, "·"),
                1..=7 => self.fg(color_of(cell), &EIGHTHS[eighths - cell * 8 - 1].to_string()),
                _ => self.fg(color_of(cell), "█"),
            };
            bar.push_str(&glyph);
        }
        bar
    }

    /// The history of a figure: one character per sample, taller and brighter when it is higher
    fn trace(&self, history: &[i64], max: i64) -> String {
        history
            .iter()
            .map(|value| {
                let step = step(*value, max);
                self.fg(TRACE[step], &SPARK[step].to_string())
            })
            .collect()
    }

    /// A label, a bar of `value` against `max`, the share, the text, and the history of the figure
    fn row(&self, label: &str, value: i64, max: i64, text: &str, history: &[i64], neutral: bool) -> String {
        if !self.color {
            return format!("{label:<14}{}  {text:<26} {}", self.bar(value, max, neutral), spark(history, max));
        }
        let ratio = ratio(value, max);
        let share = if neutral { self.dim("    ") } else { self.fg(Style::level(ratio), &format!("{:>3.0}%", ratio * 100.0)) };
        format!(
            " {}  {} {share}  {text:<26} {}",
            self.paint("1", &format!("{label:<13}")),
            self.bar(value, max, neutral),
            self.trace(history, max)
        )
    }

    fn rule(&self) -> String {
        self.fg(238, &"─".repeat(WIDTH))
    }
}

/// One character per sample, scaled between 0 and `max`
fn spark(history: &[i64], max: i64) -> String {
    history.iter().map(|value| SPARK[step(*value, max)]).collect()
}

/// How full `value` is against `max`, from 0 to 1; 0 when there is no limit
fn ratio(value: i64, max: i64) -> f64 {
    if max > 0 { (value as f64 / max as f64).clamp(0.0, 1.0) } else { 0.0 }
}

/// Which of the eight heights of a history `value` has, between 0 and `max`
fn step(value: i64, max: i64) -> usize {
    let max = if max == 0 { 1 } else { max } as f64;
    (((value as f64 / max) * 7.0).floor().max(0.0) as usize).min(7)
}

fn duration(seconds: i64) -> String {
    let (h, m, s) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if h > 0 {
        format!("{h}h{m}m{s}s")
    } else if m > 0 {
        format!("{m}m{s}s")
    } else {
        format!("{s}s")
    }
}

/// Keeps the end of a long path, which is the part that tells directories apart
fn shorten(text: &str, max: usize) -> String {
    let length = text.chars().count();
    if length > max { format!("…{}", text.chars().skip(length - max + 1).collect::<String>()) } else { text.to_string() }
}

/// The free space of the directory of `/disk`, or that it can't be used
fn free_space(info: &Info) -> String {
    info.disk.free_mb.map_or("not usable".to_string(), |mb| format!("{mb} MB free"))
}

/// The directory of `/disk`, short enough for the line
fn disk_dir(info: &Info) -> String {
    shorten(&info.disk.dir, 40)
}

fn readiness(ready: &Ready) -> String {
    if ready.ready {
        return "readiness: ready".to_string();
    }
    let back = ready.seconds_left.map_or("until /ready/on".to_string(), |s| format!("ready again in {s} s"));
    format!("readiness: NOT READY ({back})")
}

/// What the scenario is doing, or why it ended; nothing when there is none
fn scenario_line(scenario: &Scenario) -> Option<String> {
    let (step, steps) = (scenario.step.unwrap_or(0), scenario.steps.unwrap_or(0));
    match scenario.state.as_str() {
        "running" => Some(format!(
            "step {step} of {steps}, {} s left in it, {} s in all",
            scenario.seconds_left.unwrap_or(0),
            scenario.remaining_seconds.unwrap_or(0)
        )),
        "failed" => Some(format!("failed: {}", scenario.error.as_deref().unwrap_or("no reason given"))),
        "finished" => Some(format!("finished, {steps} steps")),
        "stopped" => Some(format!("stopped at step {step} of {steps}")),
        _ => None,
    }
}

/// The rows of figures, the same with colours and without
fn rows(style: &Style, info: &Info, history: &History) -> Vec<String> {
    let busy = info.stress.per_worker.iter().filter(|n| **n > 0).count() as i64;
    let workers = info.stress.workers;
    let memory = &info.memory;
    let container = memory.container_limit_mb.filter(|limit| *limit > 0);
    let rss_text = match container {
        Some(limit) => format!("{} / {limit} MB (container)", memory.process_rss_mb),
        None => format!("{} MB (no limit)", memory.process_rss_mb),
    };
    // without a container limit the bar is only a size reference: the memory budget of /memory
    let rss_max = container.unwrap_or_else(|| history.rss.iter().copied().fold(memory.limit_mb, i64::max));

    // the bar is the load of the workers: what /stress keeps busy and what /cpu burns
    let load = cpu_load(info);
    let cpu_text = match &info.stress.burn {
        Some(_) if workers > 0 => format!("{}% load, {busy} busy / {workers}", load / workers),
        _ => format!("{busy} busy / {workers}"),
    };
    let mut rows = vec![
        style.row("CPU workers", load, workers * 100, &cpu_text, &history.busy, false),
        style.row(
            "Memory held",
            memory.held_mb,
            memory.limit_mb,
            &format!("{} / {} MB", memory.held_mb, memory.limit_mb),
            &history.held,
            false,
        ),
        style.row("Process RSS", memory.process_rss_mb, rss_max, &rss_text, &history.rss, container.is_none()),
        style.row(
            "Disk held",
            info.disk.held_mb,
            info.disk.limit_mb,
            &format!("{} / {} MB", info.disk.held_mb, info.disk.limit_mb),
            &history.disk,
            false,
        ),
    ];
    if let Some(fd) = &info.fd {
        let (ceiling, by_process) = fd.ceiling();
        let text = format!("{} / {}{}", fd.held, ceiling, if by_process { " (ulimit)" } else { "" });
        rows.push(style.row("FDs held", fd.held, ceiling, &text, &history.fd, false));
    }
    if let Some(c) = &info.connections {
        rows.push(style.row("Connections", c.held, c.limit, &format!("{} / {}", c.held, c.limit), &history.connections, false));
    }
    rows
}

/// What `/memory`, `/leak`, `/disk`, `/fd` and `/connections` hold right now and for how long more:
/// the colour of its line, its command, and the text
fn holds(info: &Info) -> Vec<(u8, &'static str, String)> {
    let mut holds = Vec::new();
    if let Some(left) = info.memory.seconds_left {
        holds.push((114, "/memory", format!("{} MB, {left} s left", info.memory.held_mb)));
    }
    if let Some(left) = info.disk.seconds_left {
        holds.push((180, "/disk", format!("{} MB, {left} s left", info.disk.held_mb)));
    }
    if let Some(fd) = &info.fd
        && let Some(left) = fd.seconds_left
    {
        holds.push((141, "/fd", format!("{}, {left} s left", fd.held)));
    }
    if let Some(connections) = &info.connections
        && let Some(left) = connections.seconds_left
    {
        holds.push((75, "/connections", format!("{}, {left} s left", connections.held)));
    }
    holds
}

/// The view of the last sample, with the history of the previous ones
pub fn render(style: &Style, url: &str, info: &Info, history: &History) -> String {
    if style.color {
        return render_color(style, url, info, history);
    }
    let free = free_space(info);
    let mut lines = vec![format!("{} {}  {url}  up {}", info.name, info.version, duration(info.uptime_seconds)), String::new()];
    lines.extend(rows(style, info, history));
    lines.push(String::new());
    lines.push(format!("/stress in progress: {}   /delay waiting: {}", info.stress.in_flight, info.delay.in_flight));
    if let Some(burn) = &info.stress.burn {
        lines.push(format!("/cpu: {}% on {}, {} s left", burn.percent, burned(info, burn), burn.seconds_left));
    }
    for (_, command, text) in holds(info) {
        lines.push(format!("{command}: {text}"));
    }
    if let Some(log) = &info.log {
        lines.push(format!("/log: {} of {} lines, {} lines/s", log.sent, log.lines, log.rate));
    }
    if let Some(text) = info.scenario.as_ref().and_then(scenario_line) {
        lines.push(format!("/scenario: {text}"));
    }
    if let Some(ready) = &info.ready {
        lines.push(readiness(ready));
    }
    lines.push(format!("disk dir: {}   {free}", disk_dir(info)));
    lines.join("\n")
}

/// The same view with colours: a header, smooth bars, badges and lit-up activity
fn render_color(style: &Style, url: &str, info: &Info, history: &History) -> String {
    let free = free_space(info);
    let ready = info.ready.as_ref().map(|ready| {
        if ready.ready {
            style.badge(35, 16, "● READY")
        } else {
            let back = ready.seconds_left.map_or("until /ready/on".to_string(), |s| format!("back in {s} s"));
            style.badge(160, 15, &format!("● NOT READY · {back}"))
        }
    });
    let mut lines = vec![
        format!(
            " {} {}  {}  {}",
            style.paint("1;38;5;215", &info.name),
            style.dim(&info.version),
            style.fg(75, url),
            style.fg(42, &format!("● up {}", duration(info.uptime_seconds)))
        ),
        style.rule(),
    ];
    lines.extend(rows(style, info, history));
    lines.push(style.rule());

    // what is happening lights up, what is not stays dim
    let activity = |on: bool, color: u8, text: String| {
        if on {
            format!(" {}", style.paint(&format!("1;38;5;{color}"), &format!("◆ {text}")))
        } else {
            format!(" {}", style.dim(&format!("◇ {text}")))
        }
    };
    let (running, waiting) = (info.stress.in_flight, info.delay.in_flight);
    lines.push(activity(running + waiting > 0, 221, format!("/stress {running} running   /delay {waiting} waiting")));
    match &info.stress.burn {
        Some(burn) => {
            lines.push(activity(true, 209, format!("/cpu {}% on {}, {} s left", burn.percent, burned(info, burn), burn.seconds_left)))
        }
        None => lines.push(activity(false, 209, "/cpu idle".into())),
    }
    for (color, command, text) in holds(info) {
        lines.push(activity(true, color, format!("{command} {text}")));
    }
    if let Some(log) = &info.log {
        lines.push(activity(true, 177, format!("/log {} of {} lines, {} lines/s", log.sent, log.lines, log.rate)));
    }
    if let Some(scenario) = &info.scenario
        && let Some(text) = scenario_line(scenario)
    {
        lines.push(activity(scenario.state == "running", 117, format!("/scenario {text}")));
    }
    let place = format!("{}   {}", style.dim(&format!("disk dir {}", disk_dir(info))), style.dim(&free));
    lines.push(match ready {
        Some(ready) => format!(" {ready}   {place}"),
        None => format!(" {place}"),
    });
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &str = r#"{
        "name": "torchio", "version": "3.5.0", "uptimeSeconds": 3725, "auth": false,
        "stress": {"workers": 4, "inFlight": 1, "perWorker": [1, 0, 0, 0], "burn": {"percent": 30, "secondsLeft": 12}},
        "delay": {"inFlight": 2},
        "memory": {"heldMb": 150, "limitMb": 300, "processRssMb": 180, "containerLimitMb": 256},
        "disk": {"heldMb": 0, "limitMb": 300, "dir": "/tmp", "freeMb": null},
        "fd": {"held": 40, "limit": 10000},
        "ready": {"ready": false, "secondsLeft": 20},
        "log": {"sent": 10, "lines": 1000, "rate": 10}
    }"#;

    fn plain(info: &str) -> String {
        let info: Info = serde_json::from_str(info).unwrap();
        let mut history = History::default();
        history.record(&info);
        render(&Style::new(false), "http://x:3000", &info, &history)
    }

    #[test]
    fn the_fd_bar_compares_with_the_lower_of_the_budget_and_the_process_limit() {
        let low =
            INFO.replace("\"fd\": {\"held\": 40, \"limit\": 10000}", "\"fd\": {\"held\": 2549, \"limit\": 10000, \"processLimit\": 2560}");
        assert!(plain(&low).contains("FDs held      ████████████████████  2549 / 2560 (ulimit)"), "{}", plain(&low));
        let high =
            INFO.replace("\"fd\": {\"held\": 40, \"limit\": 10000}", "\"fd\": {\"held\": 40, \"limit\": 10000, \"processLimit\": 1048576}");
        assert!(
            plain(&high).contains("FDs held      ░░░░░░░░░░░░░░░░░░░░  40 / 10000") && !plain(&high).contains("ulimit"),
            "a higher limit of the process changes nothing"
        );
        let none =
            INFO.replace("\"fd\": {\"held\": 40, \"limit\": 10000}", "\"fd\": {\"held\": 40, \"limit\": 10000, \"processLimit\": null}");
        assert!(plain(&none).contains("40 / 10000") && !plain(&none).contains("ulimit"), "no limit of the process changes nothing");
    }

    #[test]
    fn every_hold_that_runs_has_a_line_with_the_seconds_left() {
        let busy = INFO
            .replace("\"containerLimitMb\": 256", "\"containerLimitMb\": 256, \"secondsLeft\": 18")
            .replace("\"freeMb\": null", "\"freeMb\": null, \"secondsLeft\": 9")
            .replace("\"fd\": {\"held\": 40, \"limit\": 10000}", "\"fd\": {\"held\": 40, \"limit\": 10000, \"secondsLeft\": 5}");
        let view = plain(&busy);
        assert!(view.contains("\n/memory: 150 MB, 18 s left\n"), "{view}");
        assert!(view.contains("\n/disk: 0 MB, 9 s left\n"), "{view}");
        assert!(view.contains("\n/fd: 40, 5 s left\n"), "{view}");
        assert!(!view.contains("/connections:"), "what is not held has no line");
        let idle = plain(INFO);
        assert!(!idle.contains("/memory:") && !idle.contains("/disk:") && !idle.contains("/fd:"), "{idle}");
    }

    #[test]
    fn the_view_has_every_figure() {
        let view = plain(INFO);
        assert!(view.starts_with("torchio 3.5.0  http://x:3000  up 1h2m5s\n\n"));
        assert!(view.contains("CPU workers   ██████████░░░░░░░░░░  47% load, 1 busy / 4"), "{view}");
        assert!(view.contains("Memory held   ██████████░░░░░░░░░░  150 / 300 MB"));
        assert!(view.contains("Process RSS   ██████████████░░░░░░  180 / 256 MB (container)"));
        assert!(view.contains("FDs held      ░░░░░░░░░░░░░░░░░░░░  40 / 10000"));
        assert!(view.contains("/stress in progress: 1   /delay waiting: 2\n"));
        assert!(view.contains("/cpu: 30% on every worker, 12 s left\n"));
        assert!(view.contains("/log: 10 of 1000 lines, 10 lines/s\n"));
        assert!(view.contains("readiness: NOT READY (ready again in 20 s)\n"));
        assert!(view.ends_with("disk dir: /tmp   not usable"));
    }

    #[test]
    fn the_cpu_row_counts_what_stress_keeps_busy_and_what_cpu_burns() {
        let info = |stress: &str| -> Info {
            let json = INFO.replace(
                r#""stress": {"workers": 4, "inFlight": 1, "perWorker": [1, 0, 0, 0], "burn": {"percent": 30, "secondsLeft": 12}}"#,
                stress,
            );
            serde_json::from_str(&json).unwrap()
        };
        let idle = info(r#""stress": {"workers": 4, "inFlight": 0, "perWorker": [0, 0, 0, 0], "burn": null}"#);
        let burning =
            info(r#""stress": {"workers": 4, "inFlight": 0, "perWorker": [0, 0, 0, 0], "burn": {"percent": 80, "secondsLeft": 9}}"#);
        let both = info(r#""stress": {"workers": 4, "inFlight": 1, "perWorker": [1, 0, 0, 0], "burn": {"percent": 80, "secondsLeft": 9}}"#);
        assert_eq!((cpu_load(&idle), cpu_load(&burning), cpu_load(&both)), (0, 320, 340));
        // /cpu on 2 of the 4 workers: a worker that runs a /stress counts 100 wherever it is
        let some = info(
            r#""stress": {"workers": 4, "inFlight": 0, "perWorker": [0, 0, 0, 0], "burn": {"percent": 80, "secondsLeft": 9, "workers": 2}}"#,
        );
        let some_and_stress = info(
            r#""stress": {"workers": 4, "inFlight": 1, "perWorker": [0, 0, 0, 1], "burn": {"percent": 80, "secondsLeft": 9, "workers": 2}}"#,
        );
        assert_eq!((cpu_load(&some), cpu_load(&some_and_stress)), (160, 260));
        let view = |info: &Info| {
            let mut history = History::default();
            history.record(info);
            render(&Style::new(false), "u", info, &history)
        };
        assert!(view(&some).contains("/cpu: 80% on 2 of 4 workers, 9 s left"));
        assert!(view(&burning).contains("/cpu: 80% on every worker, 9 s left"));
        assert!(view(&idle).contains("CPU workers   ░░░░░░░░░░░░░░░░░░░░  0 busy / 4"));
        assert!(view(&burning).contains("CPU workers   ████████████████░░░░  80% load, 0 busy / 4"));
        assert!(view(&both).contains("85% load, 1 busy / 4"));
    }

    #[test]
    fn connections_have_a_row_when_the_server_reports_them() {
        let view = |json: &str| {
            let info: Info = serde_json::from_str(json).unwrap();
            let mut history = History::default();
            history.record(&info);
            render(&Style::new(false), "u", &info, &history)
        };
        let with = INFO.replace(
            r#""fd": {"held": 40, "limit": 10000},"#,
            r#""fd": {"held": 40, "limit": 10000}, "connections": {"held": 25, "limit": 500},"#,
        );
        assert!(view(&with).contains("25 / 500"));
        assert!(!view(INFO).contains("Connections"), "a server that does not report them has no row");
    }

    #[test]
    fn the_scenario_is_a_line_while_it_runs_and_after_it_ends() {
        let with = |scenario: &str| INFO.replace(r#""log": {"#, &format!(r#""scenario": {scenario}, "log": {{"#));
        let running = plain(&with(r#"{"state": "running", "step": 2, "steps": 3, "secondsLeft": 12, "remainingSeconds": 40}"#));
        assert!(running.contains("/scenario: step 2 of 3, 12 s left in it, 40 s in all\n"), "{running}");
        let failed = plain(&with(r#"{"state": "failed", "step": 1, "steps": 2, "error": "step 1: no memory"}"#));
        assert!(failed.contains("/scenario: failed: step 1: no memory\n"), "{failed}");
        assert!(plain(&with(r#"{"state": "stopped", "step": 2, "steps": 3}"#)).contains("/scenario: stopped at step 2 of 3\n"));
        assert!(plain(&with(r#"{"state": "finished", "step": 3, "steps": 3}"#)).contains("/scenario: finished, 3 steps\n"));
        assert!(!plain(&with(r#"{"state": "idle"}"#)).contains("/scenario"), "nothing to say when there is none");
        assert!(!plain(INFO).contains("/scenario"), "an older server has none");
    }

    #[test]
    fn an_older_server_has_no_fd_ready_or_log() {
        let older = r#"{"name":"torchio","version":"3.0.0","uptimeSeconds":5,
            "stress":{"workers":2,"inFlight":0,"perWorker":[0,0],"burn":null},"delay":{"inFlight":0},
            "memory":{"heldMb":0,"limitMb":512,"processRssMb":60,"containerLimitMb":null},
            "disk":{"heldMb":0,"limitMb":1024,"dir":"/tmp","freeMb":1000}}"#;
        let view = plain(older);
        assert!(!view.contains("FDs held") && !view.contains("readiness") && !view.contains("/log:"));
        assert!(view.contains("60 MB (no limit)"));
        assert!(view.contains("1000 MB free"));
    }

    #[test]
    fn a_server_that_is_ready_or_stays_not_ready() {
        let ready = Ready { ready: true, seconds_left: None };
        assert_eq!(readiness(&ready), "readiness: ready");
        let off = Ready { ready: false, seconds_left: None };
        assert_eq!(readiness(&off), "readiness: NOT READY (until /ready/on)");
    }

    #[test]
    fn a_container_limit_of_zero_is_no_limit() {
        let view = plain(&INFO.replace("\"containerLimitMb\": 256", "\"containerLimitMb\": 0"));
        assert!(view.contains("180 MB (no limit)"));
    }

    // what is left of a coloured view when the escape sequences are taken away
    fn text(view: &str) -> String {
        let mut out = String::new();
        let mut chars = view.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn coloured(info: &str) -> String {
        let info: Info = serde_json::from_str(info).unwrap();
        let mut history = History::default();
        history.record(&info);
        render(&Style::new(true), "http://x:3000", &info, &history)
    }

    #[test]
    fn a_plain_bar_is_whole_cells_and_never_goes_past_its_end() {
        let style = Style::new(false);
        assert_eq!(style.bar(50, 100, false), "█".repeat(10) + &"░".repeat(10));
        assert_eq!(style.bar(0, 0, false), "░".repeat(20));
        assert_eq!(style.bar(500, 100, false), "█".repeat(20));
        assert!(!style.bar(90, 100, false).contains('\x1b'));
    }

    #[test]
    fn a_coloured_bar_is_smooth_and_each_cell_has_the_colour_of_its_place() {
        let style = Style::new(true);
        // 20 cells of 8 parts: 55% is 88 parts, 11 whole cells; 56% is 90, two more parts
        assert_eq!(text(&style.bar(55, 100, false)), "█".repeat(11) + &"·".repeat(9));
        assert_eq!(text(&style.bar(56, 100, false)), "█".repeat(11) + "▎" + &"·".repeat(8));
        assert_eq!(text(&style.bar(100, 100, false)), "█".repeat(20));
        let bar = style.bar(100, 100, false);
        assert!(bar.contains("38;5;35m") && bar.contains("38;5;203m"), "green at the start, red at the end");
        assert!(style.bar(100, 100, true).contains("38;5;27m"), "a size reference is blue");
        assert_eq!(Style::level(0.0), 35);
        assert_eq!(Style::level(1.0), 203);
    }

    #[test]
    fn the_coloured_view_has_the_same_figures() {
        let view = text(&coloured(INFO));
        assert!(view.contains("torchio 3.5.0  http://x:3000  ● up 1h2m5s"), "{view}");
        for figure in ["1 busy / 4", "150 / 300 MB", "180 / 256 MB (container)", "40 / 10000", "50%", "NOT READY · back in 20 s"] {
            assert!(view.contains(figure), "{figure} in\n{view}");
        }
        assert!(view.contains("◆ /stress 1 running   /delay 2 waiting"));
        assert!(view.contains("◆ /cpu 30% on every worker, 12 s left"));
        assert!(view.contains("◆ /log 10 of 1000 lines, 10 lines/s"));
        assert!(view.contains("disk dir /tmp"));
    }

    #[test]
    fn what_is_not_happening_is_dim() {
        let idle = INFO
            .replace("\"inFlight\": 1", "\"inFlight\": 0")
            .replace("\"inFlight\": 2", "\"inFlight\": 0")
            .replace(", \"burn\": {\"percent\": 30, \"secondsLeft\": 12}", "")
            .replace("\"ready\": false, \"secondsLeft\": 20", "\"ready\": true");
        let view = coloured(&idle);
        assert!(text(&view).contains("◇ /stress 0 running   /delay 0 waiting") && text(&view).contains("◇ /cpu idle"));
        assert!(text(&view).contains("● READY"));
        assert!(!view.contains("1;38;5;221m"), "nothing lights up");
    }

    #[test]
    fn the_keys_are_badges_with_colours_and_words_without() {
        assert_eq!(Style::new(false).key("c", "cpu"), "c cpu");
        assert_eq!(text(&Style::new(true).key("c", "cpu")), " c  cpu");
    }

    #[test]
    fn the_history_is_drawn_between_zero_and_the_maximum() {
        assert_eq!(spark(&[0, 4, 8], 8), "▁▄█");
        assert_eq!(spark(&[3], 0), "█");
        assert_eq!(spark(&[100], 8), "█", "a value over the maximum is the highest bar");
        assert_eq!(spark(&[], 8), "");
    }

    #[test]
    fn durations_use_the_units_that_are_needed() {
        assert_eq!((duration(5), duration(65), duration(3725), duration(0)), ("5s".into(), "1m5s".into(), "1h2m5s".into(), "0s".into()));
    }

    #[test]
    fn a_long_path_keeps_its_end() {
        assert_eq!(shorten("/short", 40), "/short");
        assert_eq!(shorten("/a/very/long/path/to/some/directory/that/goes/on", 20), "…ectory/that/goes/on");
        assert_eq!(shorten("/ünïcode/dïrectory/with/accents/ünder/here", 20).chars().count(), 20);
    }

    #[test]
    fn the_history_keeps_the_last_thirty() {
        let info: Info = serde_json::from_str(INFO).unwrap();
        let mut history = History::default();
        for _ in 0..40 {
            history.record(&info);
        }
        assert_eq!(history.held.len(), HISTORY);
    }
}
