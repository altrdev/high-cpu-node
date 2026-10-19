use crate::auth;
use crate::config::Config;
use crate::ready::Ready;
use crate::routes;
use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use std::sync::Arc;
use std::time::SystemTime;
use tower_http::cors::CorsLayer;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// What the routes share: the configuration and the state that lives for the whole run
pub struct AppState {
    pub config: Config,
    pub started_at: SystemTime,
    pub ready: Arc<Ready>,
}

impl AppState {
    pub fn new(config: Config) -> AppState {
        AppState { config, started_at: SystemTime::now(), ready: Arc::new(Ready::default()) }
    }
}

// The endpoints that change the state of the instance go behind `auth::guard`
pub fn router(state: Arc<AppState>) -> Router {
    let guarded = Router::new()
        .route("/ready/off", get(routes::ready::off))
        .route("/ready/on", get(routes::ready::on))
        .route_layer(from_fn_with_state(state.clone(), auth::guard));

    Router::new()
        .route("/", get(|| async { "Status: OK" }))
        .route("/status/{code}", get(routes::status::handler))
        .route("/ready", get(routes::ready::handler))
        .route("/help", get(routes::help::handler))
        .route("/config", get(routes::config::handler))
        .merge(guarded)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
