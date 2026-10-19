use crate::app::AppState;
use crate::endpoints::specs;
use crate::params;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

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

pub async fn off(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Response {
    let params = match params::read(query.as_deref(), &specs(&state.config).ready_off) {
        Ok(params) => params,
        Err(rejection) => return rejection,
    };
    let seconds = params.get("seconds");

    state.ready.off(seconds as u64);
    let body = if seconds > 0 { format!("Not ready for {seconds} s") } else { "Not ready until /ready/on".to_string() };
    (StatusCode::OK, body).into_response()
}

pub async fn on(State(state): State<Arc<AppState>>) -> Response {
    state.ready.on();
    (StatusCode::OK, "Ready").into_response()
}
