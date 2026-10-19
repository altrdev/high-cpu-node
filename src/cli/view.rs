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
        let ratio = if max > 0 { (value as f64 / max as f64).clamp(0.0, 1.0) } else { 0.0 };
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
        let max = if max == 0 { 1 } else { max } as f64;
        history
            .iter()
            .map(|value| {
                let level = (((*value as f64 / max) * 7.0).floor().max(0.0) as usize).min(7);
                self.fg(TRACE[level], &SPARK[level].to_string())
            })
            .collect()
    }

    /// A label, a bar of `value` against `max`, the share, the text, and the history of the figure
    fn row(&self, label: &str, value: i64, max: i64, text: &str, history: &[i64], neutral: bool) -> String {
        if !self.color {
            return format!("{label:<14}{}  {text:<26} {}", self.bar(value, max, neutral), spark(history, max));
        }
        let ratio = if max > 0 { (value as f64 / max as f64).clamp(0.0, 1.0) } else { 0.0 };
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

fn readiness(ready: &Ready) -> String {
    if ready.ready {
        return "readiness: ready".to_string();
    }
    let back = ready.seconds_left.map_or("until /ready/on".to_string(), |s| format!("ready again in {s} s"));
    format!("readiness: NOT READY ({back})")
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

    let mut rows = vec![
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
        rows.push(style.row("FDs held", fd.held, fd.limit, &format!("{} / {}", fd.held, fd.limit), &history.fd, false));
    }
    rows
}

/// The view of the last sample, with the history of the previous ones
pub fn render(style: &Style, url: &str, info: &Info, history: &History) -> String {
    if style.color {
        return render_color(style, url, info, history);
    }
    let free = info.disk.free_mb.map_or("not usable".to_string(), |mb| format!("{mb} MB free"));
    let mut lines = vec![format!("{} {}  {url}  up {}", info.name, info.version, duration(info.uptime_seconds)), String::new()];
    lines.extend(rows(style, info, history));
    lines.push(String::new());
    lines.push(format!("/stress in progress: {}   /delay waiting: {}", info.stress.in_flight, info.delay.in_flight));
    if let Some(burn) = &info.stress.burn {
        lines.push(format!("/cpu: {}% on every worker, {} s left", burn.percent, burn.seconds_left));
    }
    if let Some(log) = &info.log {
        lines.push(format!("/log: {} of {} lines, {} lines/s", log.sent, log.lines, log.rate));
    }
    if let Some(ready) = &info.ready {
        lines.push(readiness(ready));
    }
    lines.push(format!("disk dir: {}   {free}", shorten(&info.disk.dir, 40)));
    lines.join("\n")
}

/// The same view with colours: a header, smooth bars, badges and lit-up activity
fn render_color(style: &Style, url: &str, info: &Info, history: &History) -> String {
    let free = info.disk.free_mb.map_or("not usable".to_string(), |mb| format!("{mb} MB free"));
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
        Some(burn) => lines.push(activity(true, 209, format!("/cpu {}% on every worker, {} s left", burn.percent, burn.seconds_left))),
        None => lines.push(activity(false, 209, "/cpu idle".into())),
    }
    if let Some(log) = &info.log {
        lines.push(activity(true, 177, format!("/log {} of {} lines, {} lines/s", log.sent, log.lines, log.rate)));
    }
    let place = format!("{}   {}", style.dim(&format!("disk dir {}", shorten(&info.disk.dir, 40))), style.dim(&free));
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
