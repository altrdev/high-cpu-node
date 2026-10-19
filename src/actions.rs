//! What each endpoint that changes the state of the server does once its values are known, and what
//! it says about it. The routes read their parameters and answer with the outcome, and a
//! [scenario](crate::resources::scenario) calls the same functions for its steps, so a refusal reads
//! the same wherever it comes from and the budgets are the ones of the endpoints.
use crate::app::AppState;
use crate::resources::budget::OverBudget;
use crate::resources::disk::WriteError;
use crate::resources::memory::HoldError;
use crate::system::errno_name;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// How an action ended
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Did what was asked
    Done(String),
    /// Did part of it: `/fd` and `/connections` open fewer than asked when the process reaches its own limit, which is the case being tested
    Partial(String),
    /// Did nothing, and the status that says why
    Refused(StatusCode, String),
}

impl Outcome {
    pub fn message(&self) -> &str {
        match self {
            Outcome::Done(message) | Outcome::Partial(message) | Outcome::Refused(_, message) => message,
        }
    }

    /// The status of the answer
    pub fn status(&self) -> StatusCode {
        match self {
            Outcome::Done(_) => StatusCode::OK,
            Outcome::Partial(_) => StatusCode::SERVICE_UNAVAILABLE,
            Outcome::Refused(status, _) => *status,
        }
    }
}

impl IntoResponse for Outcome {
    fn into_response(self) -> Response {
        (self.status(), self.message().to_string()).into_response()
    }
}

/// The `429` of what is counted in MB
pub fn over_budget_mb(over: &OverBudget) -> Outcome {
    Outcome::Refused(StatusCode::TOO_MANY_REQUESTS, format!("{} MB already held, limit is {} MB", over.held, over.limit))
}

pub fn cpu(state: &AppState, percent: i64, hold: i64, workers: usize) -> Outcome {
    state.pool.burn(percent, hold, workers);
    if percent == 0 || hold == 0 {
        return Outcome::Done("Stopped burning CPU".into());
    }
    Outcome::Done(format!("Burning {percent}% of {workers} workers for {hold} s"))
}

pub async fn memory(state: &AppState, mb: i64, hold: i64) -> Outcome {
    match state.memory.hold(mb, hold as u64).await {
        Ok(()) => Outcome::Done(format!("Allocated {mb} MB, held for {hold} s")),
        Err(HoldError::OverBudget(over)) => over_budget_mb(&over),
        Err(HoldError::Allocation(e)) => Outcome::Refused(StatusCode::INTERNAL_SERVER_ERROR, format!("Could not allocate {mb} MB: {e}")),
    }
}

pub async fn leak(state: &AppState, rate: i64, mb: i64, hold: i64) -> Outcome {
    match state.memory.leak(rate, mb, hold as u64).await {
        Ok(()) => {
            let seconds = (mb + rate - 1) / rate;
            Outcome::Done(format!("Leaking {rate} MB/s up to {mb} MB ({seconds} s), then held for {hold} s"))
        }
        Err(over) => over_budget_mb(&over),
    }
}

pub async fn disk(state: &AppState, mb: i64, hold: i64) -> Outcome {
    let dir = state.disk.dir().display();
    match state.disk.write(mb, hold as u64).await {
        Ok(()) => Outcome::Done(format!("Wrote {mb} MB to {dir}, held for {hold} s")),
        Err(WriteError::OverBudget(over)) => over_budget_mb(&over),
        Err(WriteError::Write(e)) => {
            Outcome::Refused(StatusCode::INSUFFICIENT_STORAGE, format!("Could not write {mb} MB to {dir}: {}", errno_name(&e)))
        }
    }
}

pub fn fd(state: &AppState, count: i64, hold: i64) -> Outcome {
    match state.fd.hold(count, hold as u64) {
        Ok(opened) => match opened.failure {
            None => Outcome::Done(format!("Opened {count} descriptors, held for {hold} s")),
            Some(code) => Outcome::Partial(format!("Opened {} of {count} descriptors, then {code}. Held for {hold} s", opened.opened)),
        },
        Err(over) => {
            Outcome::Refused(StatusCode::TOO_MANY_REQUESTS, format!("{} descriptors already held, limit is {}", over.held, over.limit))
        }
    }
}

pub async fn connections(state: &AppState, count: i64, hold: i64) -> Outcome {
    match state.connections.hold(count, hold as u64).await {
        Ok(opened) => match opened.failure {
            None => Outcome::Done(format!("Opened {count} connections, held for {hold} s")),
            Some(code) => Outcome::Partial(format!("Opened {} of {count} connections, then {code}. Held for {hold} s", opened.opened)),
        },
        Err(over) => {
            Outcome::Refused(StatusCode::TOO_MANY_REQUESTS, format!("{} connections already held, limit is {}", over.held, over.limit))
        }
    }
}

pub fn ready_off(state: &AppState, seconds: i64) -> Outcome {
    state.ready.off(seconds as u64);
    Outcome::Done(if seconds > 0 { format!("Not ready for {seconds} s") } else { "Not ready until /ready/on".to_string() })
}

pub fn block(state: &AppState, ms: i64) -> Outcome {
    match state.block.start(ms as u64, state.config.log_format) {
        Ok(()) => Outcome::Done(format!("Blocking for {ms} ms")),
        Err(busy) => Outcome::Refused(StatusCode::CONFLICT, format!("Already blocked for {} more ms", busy.ms_left)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn state() -> AppState {
        let get = |name: &str| match name {
            "WORKERS" => Some("1".to_string()),
            "MAX_FD" => Some("10".to_string()),
            "MAX_MEMORY_MB" => Some("20".to_string()),
            _ => None,
        };
        AppState::new(Config::read(get, |_| {})).unwrap()
    }

    #[tokio::test]
    async fn a_refusal_has_a_status_and_the_words_of_the_endpoint() {
        let state = state();
        assert_eq!(memory(&state, 15, 5).await, Outcome::Done("Allocated 15 MB, held for 5 s".into()));
        let refused = memory(&state, 10, 5).await;
        assert_eq!(refused, Outcome::Refused(StatusCode::TOO_MANY_REQUESTS, "15 MB already held, limit is 20 MB".into()));
        assert_eq!((refused.status(), refused.message()), (StatusCode::TOO_MANY_REQUESTS, "15 MB already held, limit is 20 MB"));

        assert!(matches!(fd(&state, 6, 5), Outcome::Done(_)));
        assert_eq!(fd(&state, 6, 5), Outcome::Refused(StatusCode::TOO_MANY_REQUESTS, "6 descriptors already held, limit is 10".into()));
    }

    #[tokio::test]
    async fn the_others_say_what_they_did() {
        let state = state();
        assert_eq!(cpu(&state, 0, 10, 1), Outcome::Done("Stopped burning CPU".into()));
        assert_eq!(cpu(&state, 50, 10, 1), Outcome::Done("Burning 50% of 1 workers for 10 s".into()));
        assert_eq!(ready_off(&state, 5), Outcome::Done("Not ready for 5 s".into()));
        assert_eq!(ready_off(&state, 0), Outcome::Done("Not ready until /ready/on".into()));
        assert_eq!(block(&state, 100), Outcome::Done("Blocking for 100 ms".into()));
        assert_eq!(block(&state, 100).status(), StatusCode::CONFLICT, "a block is going on");
    }

    #[test]
    fn a_partial_outcome_is_a_503_that_a_scenario_goes_on_from() {
        let partial = Outcome::Partial("Opened 3 of 5 descriptors, then EMFILE. Held for 1 s".into());
        assert_eq!(partial.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
