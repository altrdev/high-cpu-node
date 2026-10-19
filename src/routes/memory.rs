use crate::app::AppState;
use crate::endpoints::specs;
use crate::http::params::{self, Rejection};
use crate::resources::memory::HoldError;
use crate::routes::over_budget_mb;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// Allocates `mb` megabytes and holds them for `hold` seconds. `MAX_MEMORY_MB` is the
/// total that can be held at the same time.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Result<Response, Rejection> {
    let params = params::read(query.as_deref(), &specs(&state.config).memory)?;
    let (mb, hold) = (params.get("mb"), params.get("hold"));

    Ok(match state.memory.hold(mb, hold as u64).await {
        Ok(()) => (StatusCode::OK, format!("Allocated {mb} MB, held for {hold} s")).into_response(),
        Err(HoldError::OverBudget(over)) => over_budget_mb(&over),
        Err(HoldError::Allocation(e)) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Could not allocate {mb} MB: {e}")).into_response(),
    })
}
