use crate::app::AppState;
use crate::endpoints::specs;
use crate::params;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use tokio::sync::oneshot;

// Runs a Fibonacci calculation in a worker, to keep a core busy
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Response {
    let params = match params::read(query.as_deref(), &specs(&state.config).stress) {
        Ok(params) => params,
        Err(rejection) => return rejection.into_response(),
    };

    let (reply, answer) = oneshot::channel();
    let _waiting = state.pool.run(params.get("number"), reply);
    match answer.await {
        Ok(value) => (StatusCode::OK, format!("I'm still alive after fibonacci calculation with result: {value}")).into_response(),
        // the worker went away before it answered
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Worker crashed").into_response(),
    }
}
