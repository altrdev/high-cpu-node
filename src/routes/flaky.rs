use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// Fails `rate` percent of the requests with `code` and answers `200` to the rest, to test retries,
/// circuit breakers and outlier detection. A failing answer waits `ms` first, like `/delay`.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Response, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).flaky)?;
    let (rate, code, ms) = (params.get("rate"), params.get("code"), params.get("ms"));

    if !state.flaky.fails(rate) {
        return Ok("Status: OK".into_response());
    }
    if ms > 0 {
        state.delay.wait(ms as u64).await;
    }
    let status = StatusCode::from_u16(code as u16).expect("a code between 400 and 599");
    Ok((status, format!("Status: {code}")).into_response())
}
