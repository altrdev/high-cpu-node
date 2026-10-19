// The commands of the binary besides the server:
//   torchio watch [--url http://host:3000] [--interval 1] [--once]   a terminal view of /info
//   torchio healthcheck                                             exit 0 when the server answers
use serde::Deserialize;
use std::io::{IsTerminal, Write};
use std::process::exit;
use std::time::{Duration, Instant};

const HISTORY: usize = 30;
const BAR_WIDTH: usize = 20;
const SPARK: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub const USAGE: &str = "Usage: torchio watch [--url <url>] [--interval <seconds>] [--once]

  --url       server to watch (default: http://127.0.0.1:$PORT or :3000)
  --interval  seconds between samples (default: 1)
  --once      print one sample and exit, with status 1 if the server can't be reached

Other commands:
  torchio              start the server
  torchio healthcheck  exit with status 0 when the server answers on $PORT (or 3000)";

// What /info says, as far as the view needs it. A server older than 3.1 has no `ready` in it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    name: String,
    version: String,
    uptime_seconds: i64,
    stress: Stress,
    delay: Delay,
    memory: Memory,
    disk: Disk,
    fd: Option<Fd>,
    ready: Option<Ready>,
    log: Option<Log>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stress {
    workers: i64,
    in_flight: i64,
    per_worker: Vec<i64>,
    burn: Option<Burn>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Burn {
    percent: i64,
    seconds_left: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Delay {
    in_flight: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Memory {
    held_mb: i64,
    limit_mb: i64,
    process_rss_mb: i64,
    container_limit_mb: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Disk {
    held_mb: i64,
    limit_mb: i64,
    dir: String,
    free_mb: Option<i64>,
}

#[derive(Deserialize)]
struct Fd {
    held: i64,
    limit: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ready {
    ready: bool,
    seconds_left: Option<i64>,
}

#[derive(Deserialize)]
struct Log {
    sent: i64,
    lines: i64,
    rate: i64,
}

#[derive(Default)]
struct History {
    busy: Vec<i64>,
    held: Vec<i64>,
    rss: Vec<i64>,
    disk: Vec<i64>,
    fd: Vec<i64>,
}

impl History {
    fn record(&mut self, info: &Info) {
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

struct Style {
    color: bool,
}

impl Style {
    fn paint(&self, code: u8, text: &str) -> String {
        if self.color { format!("\x1b[{code}m{text}\x1b[0m") } else { text.to_string() }
    }

    // red, yellow, green
    fn level(ratio: f64) -> u8 {
        if ratio >= 0.85 { 31 } else if ratio >= 0.6 { 33 } else { 32 }
    }

    // `neutral` draws the bar in blue instead of the green/yellow/red of a real limit
    fn bar(&self, value: i64, max: i64, neutral: bool) -> String {
        let ratio = if max > 0 { (value as f64 / max as f64).min(1.0) } else { 0.0 };
        let filled = (ratio * BAR_WIDTH as f64).round() as usize;
        let code = if neutral { 34 } else { Style::level(ratio) };
        self.paint(code, &"█".repeat(filled)) + &"░".repeat(BAR_WIDTH - filled)
    }

    fn row(&self, label: &str, value: i64, max: i64, text: &str, history: &[i64], spark_max: i64, neutral: bool) -> String {
        format!("{label:<14}{}  {text:<26} {}", self.bar(value, max, neutral), self.paint(36, &spark(history, spark_max)))
    }
}

// One character per sample, scaled between 0 and `max`
fn spark(history: &[i64], max: i64) -> String {
    let max = if max == 0 { 1 } else { max } as f64;
    history.iter().map(|value| SPARK[(((*value as f64 / max) * 7.0).floor().max(0.0) as usize).min(7)]).collect()
}

fn duration(seconds: i64) -> String {
    let (h, m, s) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if h > 0 { format!("{h}h{m}m{s}s") } else if m > 0 { format!("{m}m{s}s") } else { format!("{s}s") }
}

// Keeps the end of a long path, which is the part that tells directories apart
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

fn render(style: &Style, url: &str, info: &Info, history: &History) -> String {
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
        style.row("CPU workers", busy, workers, &format!("{busy} busy / {workers}"), &history.busy, workers, false),
        style.row("Memory held", memory.held_mb, memory.limit_mb, &format!("{} / {} MB", memory.held_mb, memory.limit_mb), &history.held, memory.limit_mb, false),
        style.row("Process RSS", memory.process_rss_mb, rss_max, &rss_text, &history.rss, rss_max, container.is_none()),
        style.row("Disk held", info.disk.held_mb, info.disk.limit_mb, &format!("{} / {} MB", info.disk.held_mb, info.disk.limit_mb), &history.disk, info.disk.limit_mb, false),
    ];
    if let Some(fd) = &info.fd {
        lines.push(style.row("FDs held", fd.held, fd.limit, &format!("{} / {}", fd.held, fd.limit), &history.fd, fd.limit, false));
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

// GET `url`: the status and the body. Only http is spoken; for https, reach the server through a port-forward.
fn get(url: &str, timeout: Duration) -> Result<(u16, String), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).build().into();
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    Ok((status, body))
}

// The address of /info on the server that `base` points to
fn info_url(base: &str) -> Result<String, String> {
    let (scheme, rest) = base.split_once("://").ok_or("Invalid URL")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if scheme.is_empty() || authority.is_empty() {
        return Err("Invalid URL".into());
    }
    Ok(format!("{scheme}://{authority}/info"))
}

fn fetch_info(base: &str) -> Result<Info, String> {
    let (status, body) = get(&info_url(base)?, Duration::from_secs(2))?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    serde_json::from_str(&body).map_err(|e| format!("not the /info of torchio: {e}"))
}

struct Options {
    url: String,
    interval: String,
    once: bool,
    help: bool,
}

fn port() -> String {
    std::env::var("PORT").ok().filter(|port| !port.is_empty()).unwrap_or_else(|| "3000".into())
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options { url: format!("http://127.0.0.1:{}", port()), interval: "1".into(), once: false, help: false };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        let mut value = |what: &str| inline.clone().or_else(|| args.next().cloned()).ok_or(format!("Option '{what}' argument missing"));
        match name {
            "--url" => options.url = value("--url <url>")?,
            "--interval" => options.interval = value("--interval <seconds>")?,
            "--once" => options.once = true,
            "--help" | "-h" => options.help = true,
            other => return Err(format!("Unknown option '{other}'")),
        }
    }
    Ok(options)
}

// Restores the terminal when Ctrl+C or SIGTERM ends the view: show the cursor, leave the alternate screen
extern "C" fn quit(_: libc::c_int) {
    const RESTORE: &[u8] = b"\x1b[?25h\x1b[?1049l";
    // SAFETY: write and _exit are async-signal-safe, and RESTORE is a valid buffer
    unsafe {
        libc::write(1, RESTORE.as_ptr().cast(), RESTORE.len());
        libc::_exit(0);
    }
}

pub fn watch(args: &[String]) -> ! {
    let options = parse_options(args).unwrap_or_else(|message| {
        eprintln!("{message}");
        println!("{USAGE}");
        exit(1);
    });
    if options.help {
        println!("{USAGE}");
        exit(0);
    }

    let stdout = std::io::stdout();
    let style = Style { color: stdout.is_terminal() && std::env::var("NO_COLOR").map_or(true, |v| v.is_empty()) };
    let mut history = History::default();

    if options.once {
        match fetch_info(&options.url) {
            Ok(info) => {
                history.record(&info);
                println!("{}", render(&style, &options.url, &info, &history));
                exit(0);
            }
            Err(message) => {
                eprintln!("Cannot reach {}: {message}", options.url);
                exit(1);
            }
        }
    }

    let interval = Duration::from_secs_f64(options.interval.trim().parse::<f64>().ok().filter(|n| *n != 0.0 && n.is_finite()).unwrap_or(1.0).max(0.2));
    let screen = stdout.is_terminal();
    if screen {
        print!("\x1b[?1049h\x1b[?25l"); // alternate screen, hide cursor
        // SAFETY: `quit` only calls async-signal-safe functions
        unsafe {
            libc::signal(libc::SIGINT, quit as *const () as libc::sighandler_t);
            libc::signal(libc::SIGTERM, quit as *const () as libc::sighandler_t);
        }
    }
    let mut next = Instant::now();
    loop {
        let frame = match fetch_info(&options.url) {
            Ok(info) => {
                history.record(&info);
                render(&style, &options.url, &info, &history)
            }
            Err(message) => format!("Cannot reach {}: {message}", options.url),
        };
        let text = if screen { format!("\x1b[H\x1b[J{frame}\n\nCtrl+C to quit\n") } else { format!("\n{frame}\n") };
        let mut out = stdout.lock();
        if out.write_all(text.as_bytes()).and_then(|_| out.flush()).is_err() {
            exit(0); // the reader went away
        }
        drop(out);
        next += interval;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
    }
}

// For the HEALTHCHECK of an image that has no wget or curl: 0 when / answers with a 2xx
pub fn healthcheck() -> ! {
    match get(&format!("http://127.0.0.1:{}/", port()), Duration::from_secs(3)) {
        Ok((status, _)) if (200..300).contains(&status) => exit(0),
        Ok((status, _)) => eprintln!("unhealthy: HTTP {status}"),
        Err(message) => eprintln!("unhealthy: {message}"),
    }
    exit(1);
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
        render(&Style { color: false }, "http://x:3000", &info, &history)
    }

    #[test]
    fn the_view_has_every_figure() {
        let view = plain(INFO);
        assert!(view.starts_with("torchio 3.5.0  http://x:3000  up 1h2m5s\n\n"));
        assert!(view.contains("CPU workers   █████░░░░░░░░░░░░░░░  1 busy / 4"));
        assert!(view.contains("Memory held   ██████████░░░░░░░░░░  150 / 300 MB"));
        assert!(view.contains("Process RSS   ██████████████░░░░░░  "));
        assert!(view.contains("180 / 256 MB (container)"));
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
    fn bars_are_coloured_by_how_full_they_are() {
        let style = Style { color: true };
        assert_eq!(style.bar(50, 100, false), "\x1b[32m██████████\x1b[0m░░░░░░░░░░");
        assert!(style.bar(70, 100, false).starts_with("\x1b[33m"));
        assert!(style.bar(90, 100, false).starts_with("\x1b[31m"));
        assert!(style.bar(90, 100, true).starts_with("\x1b[34m"));
        assert_eq!(Style { color: false }.bar(0, 0, false), "░".repeat(20));
    }

    #[test]
    fn small_helpers() {
        assert_eq!(spark(&[0, 4, 8], 8), "▁▄█");
        assert_eq!(spark(&[3], 0), "█");
        assert_eq!((duration(5), duration(65), duration(3725)), ("5s".into(), "1m5s".into(), "1h2m5s".into()));
        assert_eq!(shorten("/short", 40), "/short");
        assert_eq!(shorten("/a/very/long/path/to/some/directory/that/goes/on", 20), "…ectory/that/goes/on");
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

    #[test]
    fn info_is_asked_to_the_server_whatever_the_path_of_the_url() {
        assert_eq!(info_url("http://127.0.0.1:3000").unwrap(), "http://127.0.0.1:3000/info");
        assert_eq!(info_url("http://host:3000/some/path?x=1").unwrap(), "http://host:3000/info");
        assert!(info_url("127.0.0.1:3000").is_err());
        assert!(info_url("http://").is_err());
    }

    #[test]
    fn options() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let options = parse_options(&args(&["--url", "http://a:1", "--interval=2", "--once"])).unwrap();
        assert_eq!((options.url.as_str(), options.interval.as_str(), options.once, options.help), ("http://a:1", "2", true, false));
        assert!(parse_options(&args(&["-h"])).unwrap().help);
        assert!(parse_options(&args(&["--nope"])).is_err());
        assert!(parse_options(&args(&["--url"])).is_err());
    }
}
