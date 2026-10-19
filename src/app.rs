//! What the routes share, and the router.
use crate::config::Config;
use crate::http::counters::{self, Counters};
use crate::http::{auth, trust::Trust};
use crate::logging;
use crate::resources::delay::Delay;
use crate::resources::disk::Disk;
use crate::resources::fd::Fd;
use crate::resources::flaky::Flaky;
use crate::resources::log::Log;
use crate::resources::memory::Memory;
use crate::resources::ready::Ready;
use crate::resources::workers::WorkerPool;
use crate::routes;
use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use std::sync::Arc;
use std::time::SystemTime;
use tower_http::cors::CorsLayer;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the routes share: the configuration and the state that lives for the whole run
pub struct AppState {
    pub config: Config,
    pub trust: Trust,
    pub started_at: SystemTime,
    pub counters: Counters,
    pub ready: Arc<Ready>,
    pub delay: Delay,
    /// Held by `/memory` and `/leak`, which share its budget
    pub memory: Memory,
    pub disk: Arc<Disk>,
    pub fd: Fd,
    pub flaky: Flaky,
    pub log: Arc<Log>,
    pub pool: WorkerPool,
}

impl AppState {
    /// Fails when `TRUST_PROXY` can't be understood
    pub fn new(config: Config) -> Result<AppState, String> {
        Ok(AppState {
            trust: Trust::from_setting(&config.trust_proxy)?,
            started_at: SystemTime::now(),
            counters: Counters::default(),
            ready: Arc::new(Ready::default()),
            delay: Delay::default(),
            memory: Memory::new(config.max_memory_mb),
            disk: Arc::new(Disk::new(&config.disk_dir, config.max_disk_mb, config.log_format)),
            fd: Fd::new(config.max_fd),
            flaky: Flaky::default(),
            log: Arc::new(Log::new(config.log_format)),
            pool: WorkerPool::new(config.workers as usize),
            config,
        })
    }

    /// Releases what the server keeps outside its own memory. Used on shutdown, which can't wait for timers.
    pub fn stop(&self) {
        self.disk.remove_files();
    }
}

/// The router. The endpoints that change the state of the instance go behind the token.
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
        .route("/log", get(routes::log::handler))
        .route("/crash", get(routes::crash::handler))
        .route_layer(from_fn_with_state(state.clone(), auth::guard));

    let mut open = Router::new()
        .route("/", get(|| async { "Status: OK" }))
        .route("/delay", get(routes::delay::handler))
        .route("/status/{code}", get(routes::status::handler))
        .route("/flaky", get(routes::flaky::handler))
        .route("/ready", get(routes::ready::handler))
        .route("/echo", get(routes::echo::handler))
        .route("/info", get(routes::info::handler))
        .route("/metrics", get(routes::metrics::handler))
        .route("/help", get(routes::help::handler))
        .route("/config", get(routes::config::handler));
    if state.config.ui {
        open = open.route("/ui", get(routes::ui::handler));
    }

    open.merge(guarded)
        .route_layer(axum::middleware::from_fn(counters::mark))
        .layer(from_fn_with_state(state.clone(), counters::count))
        .layer(from_fn_with_state(state.clone(), logging::access))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
