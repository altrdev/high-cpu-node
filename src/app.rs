//! What the routes share, and the router.
use crate::config::Config;
use crate::endpoints::{Specs, specs};
use crate::http::counters::{self, Counters};
use crate::http::{auth, open, trust::Trust};
use crate::logging;
use crate::resources::block::Block;
use crate::resources::connections::Connections;
use crate::resources::delay::Delay;
use crate::resources::disk::Disk;
use crate::resources::fd::Fd;
use crate::resources::flaky::Flaky;
use crate::resources::log::Log;
use crate::resources::memory::Memory;
use crate::resources::ready::Ready;
use crate::resources::scenario::Scenario;
use crate::resources::shutdown::Shutdown;
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
    /// The parameters of the endpoints with their defaults and limits, from the configuration
    pub specs: Specs,
    pub trust: Trust,
    pub started_at: SystemTime,
    pub counters: Counters,
    pub ready: Arc<Ready>,
    pub delay: Delay,
    /// Held by `/memory` and `/leak`, which share its budget
    pub memory: Memory,
    pub disk: Arc<Disk>,
    pub fd: Fd,
    pub connections: Connections,
    pub flaky: Flaky,
    pub block: Block,
    pub scenario: Scenario,
    pub log: Arc<Log>,
    pub pool: WorkerPool,
    pub shutdown: Arc<Shutdown>,
}

impl AppState {
    /// Fails when `TRUST_PROXY` can't be understood
    pub fn new(config: Config) -> Result<AppState, String> {
        Ok(AppState {
            specs: specs(&config),
            trust: Trust::from_setting(&config.trust_proxy)?,
            started_at: SystemTime::now(),
            counters: Counters::default(),
            ready: Arc::new(Ready::default()),
            delay: Delay::default(),
            memory: Memory::new(config.max_memory_mb),
            disk: Arc::new(Disk::new(&config.disk_dir, config.max_disk_mb, config.log_format)),
            fd: Fd::new(config.max_fd),
            // port is checked when the server starts, and a value out of range connects nowhere
            connections: Connections::new(config.max_connections, u16::try_from(config.port).unwrap_or(0)),
            flaky: Flaky::default(),
            block: Block::default(),
            scenario: Scenario::default(),
            log: Arc::new(Log::new(config.log_format)),
            pool: WorkerPool::new(config.workers as usize),
            shutdown: Arc::new(Shutdown::new(config.shutdown_grace_seconds)),
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
        .route("/connections", get(routes::connections::handler))
        .route("/log", get(routes::log::handler))
        .route("/crash", get(routes::crash::handler))
        .route("/block", get(routes::block::handler))
        .route("/oom", get(routes::oom::handler))
        .route("/payload", get(routes::payload::download).post(routes::payload::upload))
        .route("/scenario", get(routes::scenario::status).post(routes::scenario::start).delete(routes::scenario::stop))
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
        .layer(from_fn_with_state(state.clone(), open::track))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
