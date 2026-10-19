use crate::app::AppState;
use crate::endpoints::specs;
use crate::params;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

// Keeps the CPU at a level instead of running a calculation: each worker is busy for `percent`
// of the time during `hold` seconds, which is what a CPU based autoscaler reacts to. With
// one worker for each core, `percent` is the load of the machine. A new request replaces
// the previous one, and percent=0 stops it.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Response {
    let params = match params::read(query.as_deref(), &specs(&state.config).cpu) {
        Ok(params) => params,
        Err(rejection) => return rejection.into_response(),
    };
    let (percent, hold) = (params.get("percent"), params.get("hold"));

    state.pool.burn(percent, hold);
    if percent == 0 || hold == 0 {
        return (StatusCode::OK, "Stopped burning CPU").into_response();
    }
    (StatusCode::OK, format!("Burning {percent}% of {} workers for {hold} s", state.pool.workers())).into_response()
}
