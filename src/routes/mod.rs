//! One file for each endpoint: it reads the parameters, asks `resources` to do the work and writes the answer.
pub mod config;
pub mod cpu;
pub mod crash;
pub mod delay;
pub mod disk;
pub mod echo;
pub mod fd;
pub mod flaky;
pub mod help;
pub mod info;
pub mod leak;
pub mod log;
pub mod memory;
pub mod metrics;
pub mod ready;
pub mod status;
pub mod stress;
pub mod ui;

use crate::resources::budget::OverBudget;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// The `429` of the endpoints that count in MB
pub fn over_budget_mb(over: &OverBudget) -> Response {
    (StatusCode::TOO_MANY_REQUESTS, format!("{} MB already held, limit is {} MB", over.held, over.limit)).into_response()
}
