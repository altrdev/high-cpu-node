use crate::app::{AppState, VERSION};
use crate::endpoints::{catalog, to_json, to_text};
use crate::http::accept::prefers_html;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::sync::Arc;

/// Lists every endpoint with its parameters, defaults and limits: JSON, or plain text for a browser
pub async fn handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let endpoints = catalog(&state.config);
    let accept = headers.get(header::ACCEPT).and_then(|value| value.to_str().ok());
    if prefers_html(accept) {
        return ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], to_text(VERSION, &endpoints)).into_response();
    }
    Json(json!({"name": state.config.name, "version": VERSION, "endpoints": to_json(&endpoints)})).into_response()
}
