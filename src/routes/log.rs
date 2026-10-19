use crate::app::AppState;
use crate::endpoints::specs;
use crate::params;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use std::io::Write;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::task::JoinHandle;

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

// What is still being written, None when nothing is
#[derive(Debug, PartialEq, Serialize)]
pub struct LogState {
    pub sent: i64,
    pub lines: i64,
    pub rate: i64,
}

// Writes `lines` lines to stdout at `rate` lines each second and stops, to test log collection,
// rotation and what the logs of a container do to a disk. A new request replaces the one still
// running, and the lines already written stay where they are.
#[derive(Default)]
pub struct Log {
    inner: Mutex<Inner>,
}

impl Log {
    pub fn state(&self) -> Option<LogState> {
        let inner = self.inner.lock().unwrap();
        inner.job.as_ref().map(|job| LogState { sent: job.sent.load(Ordering::Relaxed), lines: job.lines, rate: job.rate })
    }
}

// 2026-10-19T12:34:56.789Z, the way JavaScript prints a date
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

async fn write_lines(lines: i64, rate: i64, sent: Arc<AtomicI64>) {
    let started = Instant::now();
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let due = lines.min((started.elapsed().as_secs_f64() * rate as f64).floor() as i64);
        let done = sent.load(Ordering::Relaxed);
        if due > done {
            let now = iso(SystemTime::now());
            let chunk: String = (done + 1..=due).map(|n| format!("{now} torchio-log line {n} of {lines}\n")).collect();
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(chunk.as_bytes());
            sent.store(due, Ordering::Relaxed);
        }
        if due >= lines {
            return;
        }
    }
}

pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Response {
    let params = match params::read(query.as_deref(), &specs(&state.config).log) {
        Ok(params) => params,
        Err(rejection) => return rejection,
    };
    let (lines, rate) = (params.get("lines"), params.get("rate"));

    let mut inner = state.log.inner.lock().unwrap();
    if let Some(running) = inner.job.take() {
        running.task.abort();
    }
    let id = inner.next_id;
    inner.next_id += 1;
    let sent = Arc::new(AtomicI64::new(0));
    let task = tokio::spawn({
        let (state, sent) = (state.clone(), sent.clone());
        async move {
            write_lines(lines, rate, sent).await;
            // a request that replaced this one has its own job
            let mut inner = state.log.inner.lock().unwrap();
            if inner.job.as_ref().is_some_and(|job| job.id == id) {
                inner.job = None;
            }
        }
    });
    inner.job = Some(Job { id, lines, rate, sent, task });
    drop(inner);

    let seconds = (lines + rate - 1) / rate;
    (StatusCode::OK, format!("Writing {lines} lines at {rate} lines/s ({seconds} s)")).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_printed_like_javascript_does() {
        let at = |ms: u64| iso(UNIX_EPOCH + Duration::from_millis(ms));
        assert_eq!(at(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(at(1_700_000_000_123), "2023-11-14T22:13:20.123Z");
        assert_eq!(at(951_782_400_000), "2000-02-29T00:00:00.000Z", "a leap day");
        assert_eq!(at(4_102_444_799_999), "2099-12-31T23:59:59.999Z");
    }
}
