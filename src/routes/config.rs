use crate::app::AppState;
use axum::Json;
use axum::extract::State;
use serde_json::{Value, json};
use std::sync::Arc;

// The effective configuration, as the server reads it from the environment. The token is never
// shown, only whether there is one.
pub async fn handler(State(state): State<Arc<AppState>>) -> Json<Value> {
    let mut config = serde_json::to_value(&state.config).expect("the configuration is serializable");
    config["auth"] = json!(!state.config.token.is_empty());
    Json(config)
}
