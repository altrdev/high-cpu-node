//! The view of `torchio watch`: bars, a short history of each figure and a few lines of text.
use super::info::{Info, Ready};

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
}

impl History {
    pub fn record(&mut self, info: &Info) {
        let samples = [
            (&mut self.busy, info.stress.per_worker.iter().filter(|n| **n > 0).count() as i64),
            (&mut self.held, info.memory.held_mb),
            (&mut self.rss, info.memory.process_rss_mb),
            (&mut self.disk, info.disk.held_mb),
            (&mut self.fd, info.fd.as_ref().map_or(0, |fd| fd.held)),
        ];
        for (values, sample) in samples {
            values.push(sample);
            if values.len() > HISTORY {
                values.remove(0);
            }
        }
    }
}

/// With colours or without
pub struct Style {
    color: bool,
}

impl Style {
    pub fn new(color: bool) -> Style {
        Style { color }
    }

    fn paint(&self, code: u8, text: &str) -> String {
        if self.color { format!("\x1b[{code}m{text}\x1b[0m") } else { text.to_string() }
    }

    /// Red, yellow or green, by how full a bar is
    fn level(ratio: f64) -> u8 {
        if ratio >= 0.85 {
            31
        } else if ratio >= 0.6 {
            33
        } else {
            32
        }
    }

    /// `neutral` draws the bar in blue instead of the green, yellow or red of a real limit
    fn bar(&self, value: i64, max: i64, neutral: bool) -> String {
        let ratio = if max > 0 { (value as f64 / max as f64).min(1.0) } else { 0.0 };
        let filled = (ratio * BAR_WIDTH as f64).round() as usize;
        let code = if neutral { 34 } else { Style::level(ratio) };
        self.paint(code, &"█".repeat(filled)) + &"░".repeat(BAR_WIDTH - filled)
    }

    /// A label, a bar of `value` against `max`, the text, and the history of the figure
    fn row(&self, label: &str, value: i64, max: i64, text: &str, history: &[i64], neutral: bool) -> String {
        format!("{label:<14}{}  {text:<26} {}", self.bar(value, max, neutral), self.paint(36, &spark(history, max)))
    }
}

/// One character per sample, scaled between 0 and `max`
fn spark(history: &[i64], max: i64) -> String {
    let max = if max == 0 { 1 } else { max } as f64;
    history.iter().map(|value| SPARK[(((*value as f64 / max) * 7.0).floor().max(0.0) as usize).min(7)]).collect()
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

fn readiness(style: &Style, ready: &Ready) -> String {
    if ready.ready {
        return format!("readiness: {}", style.paint(32, "ready"));
    }
    let back = ready.seconds_left.map_or("until /ready/on".to_string(), |s| format!("ready again in {s} s"));
    format!("readiness: {} ({back})", style.paint(31, "NOT READY"))
}

/// The view of the last sample, with the history of the previous ones
pub fn render(style: &Style, url: &str, info: &Info, history: &History) -> String {
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
    let free = info.disk.free_mb.map_or("not usable".to_string(), |mb| format!("{mb} MB free"));

    let mut lines = vec![
        format!("{} {}  {url}  up {}", style.paint(1, &info.name), info.version, duration(info.uptime_seconds)),
        String::new(),
        style.row("CPU workers", busy, workers, &format!("{busy} busy / {workers}"), &history.busy, false),
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
        lines.push(style.row("FDs held", fd.held, fd.limit, &format!("{} / {}", fd.held, fd.limit), &history.fd, false));
    }
    lines.push(String::new());
    lines.push(format!("/stress in progress: {}   /delay waiting: {}", info.stress.in_flight, info.delay.in_flight));
    if let Some(burn) = &info.stress.burn {
        lines.push(format!("/cpu: {}% on every worker, {} s left", burn.percent, burn.seconds_left));
    }
    if let Some(log) = &info.log {
        lines.push(format!("/log: {} of {} lines, {} lines/s", log.sent, log.lines, log.rate));
    }
    if let Some(ready) = &info.ready {
        lines.push(readiness(style, ready));
    }
    lines.push(format!("disk dir: {}   {free}", shorten(&info.disk.dir, 40)));
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
    fn the_view_has_every_figure() {
        let view = plain(INFO);
        assert!(view.starts_with("torchio 3.5.0  http://x:3000  up 1h2m5s\n\n"));
        assert!(view.contains("CPU workers   █████░░░░░░░░░░░░░░░  1 busy / 4"));
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
        assert_eq!(readiness(&Style::new(false), &ready), "readiness: ready");
        let off = Ready { ready: false, seconds_left: None };
        assert_eq!(readiness(&Style::new(false), &off), "readiness: NOT READY (until /ready/on)");
        assert_eq!(readiness(&Style::new(true), &ready), "readiness: \x1b[32mready\x1b[0m");
    }

    #[test]
    fn a_container_limit_of_zero_is_no_limit() {
        let view = plain(&INFO.replace("\"containerLimitMb\": 256", "\"containerLimitMb\": 0"));
        assert!(view.contains("180 MB (no limit)"));
    }

    #[test]
    fn bars_are_coloured_by_how_full_they_are() {
        let style = Style::new(true);
        assert_eq!(style.bar(50, 100, false), "\x1b[32m██████████\x1b[0m░░░░░░░░░░");
        assert!(style.bar(70, 100, false).starts_with("\x1b[33m"));
        assert!(style.bar(90, 100, false).starts_with("\x1b[31m"));
        assert!(style.bar(90, 100, true).starts_with("\x1b[34m"));
        assert_eq!(Style::new(false).bar(0, 0, false), "░".repeat(20));
        assert_eq!(Style::new(false).bar(500, 100, false), "█".repeat(20), "a bar never goes past its end");
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
