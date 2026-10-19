//! The lines that `/log` writes to stdout at a given rate.
use crate::logging::{Level, LogFormat};
use serde::Serialize;
use std::io::Write;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

/// `2026-10-19T12:34:56.789Z`: ISO 8601, in UTC, with milliseconds
pub fn iso(time: SystemTime) -> String {
    let since_epoch = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let days = (since_epoch.as_secs() / 86400) as i64;
    let seconds = since_epoch.as_secs() % 86400;
    // days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let day_of_era = z.rem_euclid(146097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60,
        since_epoch.subsec_millis()
    )
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

    #[test]
    fn dates_are_iso_8601_in_utc_with_milliseconds() {
        let at = |ms: u64| iso(UNIX_EPOCH + Duration::from_millis(ms));
        assert_eq!(at(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(at(1_700_000_000_123), "2023-11-14T22:13:20.123Z");
        assert_eq!(at(951_782_400_000), "2000-02-29T00:00:00.000Z", "a leap day");
        assert_eq!(at(951_868_800_000), "2000-03-01T00:00:00.000Z");
        assert_eq!(at(4_102_444_799_999), "2099-12-31T23:59:59.999Z");
    }

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
