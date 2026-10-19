use crate::auth;
use crate::budget::Budget;
use crate::config::Config;
use crate::ready::Ready;
use crate::routes;
use crate::routes::delay::Delay;
use crate::routes::disk::Disk;
use crate::routes::fd::Fd;
use crate::workers::WorkerPool;
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
    pub delay: Delay,
    // the budget of /memory, which /leak uses too
    pub memory: Arc<Budget>,
    pub disk: Disk,
    pub fd: Fd,
    pub pool: WorkerPool,
}

impl AppState {
    pub fn new(config: Config) -> AppState {
        AppState {
            started_at: SystemTime::now(),
            ready: Arc::new(Ready::default()),
            delay: Delay::default(),
            memory: Budget::new(config.max_memory_mb),
            disk: Disk::new(&config.disk_dir, config.max_disk_mb),
            fd: Fd::new(config.max_fd),
            pool: WorkerPool::new(config.workers as usize),
            config,
        }
    }

    // Releases what the server keeps outside its own memory. Used on shutdown, which can't wait for timers.
    pub fn stop(&self) {
        self.disk.remove_files();
    }
}

// The endpoints that change the state of the instance go behind `auth::guard`
pub fn router(state: Arc<AppState>) -> Router {
    let guarded = Router::new()
        .route("/ready/off", get(routes::ready::off))
        .route("/ready/on", get(routes::ready::on))
        .route("/stress", get(routes::stress::handler))
        .route("/cpu", get(routes::cpu::handler))
        .route("/memory", get(routes::memory::handler))
        .route("/leak", get(routes::leak::handler))
        .route("/disk", get(routes::disk::handler))
        .route("/fd", get(routes::fd::handler))
        .route_layer(from_fn_with_state(state.clone(), auth::guard));

    Router::new()
        .route("/", get(|| async { "Status: OK" }))
        .route("/delay", get(routes::delay::handler))
        .route("/status/{code}", get(routes::status::handler))
        .route("/ready", get(routes::ready::handler))
        .route("/help", get(routes::help::handler))
        .route("/config", get(routes::config::handler))
        .merge(guarded)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
