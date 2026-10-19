use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// The readiness check: `200`, or `503` while it is switched off
pub async fn handler(State(state): State<Arc<AppState>>) -> Response {
    let ready = state.ready.state();
    if ready.ready {
        return (StatusCode::OK, "Ready").into_response();
    }
    let body = match ready.seconds_left {
        None => "Not ready".to_string(),
        Some(seconds) => format!("Not ready, ready again in {seconds} s"),
    };
    (StatusCode::SERVICE_UNAVAILABLE, body).into_response()
}

/// Makes `/ready` answer `503` for `seconds`, or until `/ready/on` when it is 0
pub async fn off(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<String, Rejection> {
    let seconds = params::read(query.as_deref(), &specs(&state.config).ready_off)?.get("seconds");

    state.ready.off(seconds as u64);
    Ok(if seconds > 0 { format!("Not ready for {seconds} s") } else { "Not ready until /ready/on".to_string() })
}

pub async fn on(State(state): State<Arc<AppState>>) -> &'static str {
    state.ready.on();
    "Ready"
}
