//! The lines that `/log` writes to stdout at a given rate.
use crate::logging::{Level, LogFormat, iso};
use serde::Serialize;
use std::io::Write;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::task::JoinHandle;
use tokio::time::Instant;

const TICK: Duration = Duration::from_millis(20);

struct Job {
    id: u64,
    lines: i64,
    rate: i64,
    sent: Arc<AtomicI64>,
    task: JoinHandle<()>,
}

#[derive(Default)]
struct Inner {
    job: Option<Job>,
    next_id: u64,
}

/// What is still being written
#[derive(Debug, PartialEq, Serialize)]
pub struct LogState {
    pub sent: i64,
    pub lines: i64,
    pub rate: i64,
}

/// One job at a time: a new one replaces the one still running, and the lines already written stay.
#[derive(Default)]
pub struct Log {
    inner: Mutex<Inner>,
    format: LogFormat,
}

impl Log {
    pub fn new(format: LogFormat) -> Log {
        Log { inner: Mutex::default(), format }
    }

    /// What is being written, `None` when nothing is
    pub fn state(&self) -> Option<LogState> {
        let inner = self.inner.lock().unwrap();
        inner.job.as_ref().map(|job| LogState { sent: job.sent.load(Ordering::Relaxed), lines: job.lines, rate: job.rate })
    }

    /// Starts writing `lines` lines at `rate` lines each second
    pub fn start(self: &Arc<Self>, lines: i64, rate: i64) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(running) = inner.job.take() {
            running.task.abort();
        }
        let id = inner.next_id;
        inner.next_id += 1;
        let sent = Arc::new(AtomicI64::new(0));
        let task = tokio::spawn({
            let (log, sent, format) = (Arc::clone(self), Arc::clone(&sent), self.format);
            async move {
                write_lines(lines, rate, format, &sent, &mut std::io::stdout()).await;
                // a job that replaced this one has its own entry
                let mut inner = log.inner.lock().unwrap();
                if inner.job.as_ref().is_some_and(|job| job.id == id) {
                    inner.job = None;
                }
            }
        });
        inner.job = Some(Job { id, lines, rate, sent, task });
    }
}

/// Line `n` of `lines`: the time, then `torchio-log line 7 of 1000`; or a JSON object with the same message
fn line(format: LogFormat, at: SystemTime, n: i64, lines: i64) -> String {
    let message = format!("torchio-log line {n} of {lines}");
    match format {
        LogFormat::Text => format!("{} {message}", iso(at)),
        LogFormat::Json => format.render(Level::Info, &message, &[], at),
    }
}

/// Writes the lines that are due every tick until all of them are written, counting them in `sent`
async fn write_lines(lines: i64, rate: i64, format: LogFormat, sent: &AtomicI64, out: &mut impl Write) {
    let started = Instant::now();
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let due = lines.min((started.elapsed().as_secs_f64() * rate as f64).floor() as i64);
        let done = sent.load(Ordering::Relaxed);
        if due > done {
            let now = SystemTime::now();
            let chunk: String = (done + 1..=due).map(|n| line(format, now, n, lines) + "\n").collect();
            let _ = out.write_all(chunk.as_bytes());
            sent.store(due, Ordering::Relaxed);
        }
        if due >= lines {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn it_writes_the_lines_at_the_rate_and_stops() {
        let sent = AtomicI64::new(0);
        let mut out = Vec::new();
        write_lines(30, 10, LogFormat::Text, &sent, &mut out).await;

        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 30);
        assert!(lines[0].ends_with(" torchio-log line 1 of 30"), "{}", lines[0]);
        assert!(lines[29].ends_with(" torchio-log line 30 of 30"));
        assert_eq!(sent.load(Ordering::Relaxed), 30);
    }

    #[tokio::test(start_paused = true)]
    async fn json_lines_have_the_same_message() {
        let sent = AtomicI64::new(0);
        let mut out = Vec::new();
        write_lines(3, 100, LogFormat::Json, &sent, &mut out).await;

        let text = String::from_utf8(out).unwrap();
        let lines: Vec<serde_json::Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[2]["message"], "torchio-log line 3 of 3");
        assert_eq!(lines[2]["level"], "info");
    }

    #[tokio::test(start_paused = true)]
    async fn it_does_not_write_faster_than_the_rate() {
        let sent = Arc::new(AtomicI64::new(0));
        let task = tokio::spawn({
            let sent = Arc::clone(&sent);
            async move { write_lines(1000, 10, LogFormat::Text, &sent, &mut std::io::sink()).await }
        });
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let so_far = sent.load(Ordering::Relaxed);
        assert!((24..=26).contains(&so_far), "{so_far} lines in 2.5 s at 10 lines/s");
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_job_replaces_the_one_running() {
        let log = Arc::new(Log::default());
        assert_eq!(log.state(), None);
        log.start(1000, 10);
        assert_eq!(log.state().map(|s| (s.lines, s.rate)), Some((1000, 10)));
        log.start(5, 1000);
        assert_eq!(log.state().map(|s| (s.lines, s.rate)), Some((5, 1000)));

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(log.state(), None, "a finished job is forgotten");
    }
}
