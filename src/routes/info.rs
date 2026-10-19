use crate::app::{AppState, VERSION};
use crate::units::MB;
use axum::Json;
use axum::extract::State;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::SystemTime;

fn to_mb(bytes: u64) -> i64 {
    (bytes as f64 / MB as f64).round() as i64
}

/// What the server is doing right now, so you can see the remaining budgets
/// without having to hit a 429 or a 507
pub async fn handler(State(state): State<Arc<AppState>>) -> Json<Value> {
    let config = &state.config;
    let snapshot = state.snapshot();
    let uptime = SystemTime::now().duration_since(state.started_at).unwrap_or_default();

    Json(json!({
        "name": config.name,
        "version": VERSION,
        "uptimeSeconds": uptime.as_secs_f64().round() as i64,
        "auth": !config.token.is_empty(),
        "stress": snapshot.stress,
        "delay": {"inFlight": snapshot.delay_in_flight},
        "memory": {
            "heldMb": snapshot.memory.held_mb,
            "limitMb": snapshot.memory.limit_mb,
            "processRssMb": to_mb(snapshot.memory.rss_bytes),
            "containerLimitMb": snapshot.memory.container_limit_bytes.map(to_mb),
        },
        "disk": {
            "heldMb": snapshot.disk.held_mb,
            "limitMb": snapshot.disk.limit_mb,
            "dir": snapshot.disk.dir,
            "freeMb": snapshot.disk.free_bytes.map(to_mb),
        },
        "fd": {"held": snapshot.fd.held, "limit": snapshot.fd.limit},
        "connections": {"held": snapshot.connections.held, "limit": snapshot.connections.limit},
        "ready": snapshot.ready,
        "log": snapshot.log,
        "limits": {
            "maxNumber": config.max_number,
            "maxDelayMs": config.max_delay_ms,
            "maxHoldSeconds": config.max_hold_seconds,
            "maxLogLines": config.max_log_lines,
            "maxLogRate": config.max_log_rate,
        },
    }))
}
