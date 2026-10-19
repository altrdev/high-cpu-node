use crate::app::AppState;
use crate::budget::Budget;
use crate::endpoints::specs;
use crate::{errno, params};
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::fs::File;
use std::sync::Arc;
use std::time::Duration;

// The descriptors /fd holds, against the budget MAX_FD
pub struct Fd {
    pub budget: Arc<Budget>,
}

impl Fd {
    pub fn new(limit: i64) -> Fd {
        Fd { budget: Budget::new(limit) }
    }
}

// Opens `count` file descriptors and holds them for `hold` seconds, to test what happens when a
// process runs out of them (EMFILE). MAX_FD is the total that can be held at the same time.
// When the limit of the process is lower, the ones that could be opened stay open until `hold`
// ends: the server then can't accept connections either, which is the case being tested.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Response {
    let params = match params::read(query.as_deref(), &specs(&state.config).fd) {
        Ok(params) => params,
        Err(rejection) => return rejection,
    };
    let (count, hold) = (params.get("count"), params.get("hold"));
    let budget = &state.fd.budget;

    let Some(mut reservation) = budget.reserve(count) else {
        let message = format!("{} descriptors already held, limit is {}", budget.held(), budget.limit());
        return (StatusCode::TOO_MANY_REQUESTS, message).into_response();
    };

    let mut files = Vec::with_capacity(count as usize);
    let mut failure = None;
    for _ in 0..count {
        match File::open("/dev/null") {
            Ok(file) => files.push(file),
            Err(e) => {
                failure = Some(errno::name(&e));
                break;
            }
        }
    }
    // what was reserved but not opened goes back
    reservation.shrink_to(files.len() as i64);
    let opened = files.len();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(hold as u64)).await;
        drop(files);
        drop(reservation);
    });

    match failure {
        Some(code) => (StatusCode::SERVICE_UNAVAILABLE, format!("Opened {opened} of {count} descriptors, then {code}. Held for {hold} s")).into_response(),
        None => (StatusCode::OK, format!("Opened {count} descriptors, held for {hold} s")).into_response(),
    }
}
