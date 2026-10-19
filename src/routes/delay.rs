use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use std::sync::Arc;

/// Answers after `ms` milliseconds without blocking the server
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<String, Rejection> {
    let ms = params::read(query.as_deref(), &specs(&state.config).delay)?.get("ms");

    state.delay.wait(ms as u64).await;
    Ok(format!("Answered after {ms} ms"))
}
