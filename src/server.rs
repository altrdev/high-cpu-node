//! Starts the server, and ends the process.
use crate::app::{AppState, router};
use crate::config::Config;
use crate::http::open::Listener;
use crate::logging::LogFormat;
use axum::serve::ListenerExt;
use std::net::SocketAddr;
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

/// Runs the server until the process ends
pub fn run() -> ! {
    tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("a runtime").block_on(serve());
    exit(0)
}

async fn serve() {
    let config = Config::from_env();
    let format = config.log_format;
    let state = Arc::new(AppState::new(config.clone()).unwrap_or_else(|e| fail(format, format!("TypeError: {e}"))));
    let app = router(state.clone());

    tokio::spawn(shutdown(state.clone()));

    // START_DELAY_SECONDS: the port stays closed for a while, like an application that is slow to start
    if config.start_delay_seconds > 0 {
        config.log_format.info(&format!("{} will start listening in {} s", config.name, config.start_delay_seconds));
        tokio::time::sleep(Duration::from_secs(config.start_delay_seconds as u64)).await;
    }

    let port = u16::try_from(config.port).unwrap_or_else(|_| fail(format, format!("port {} is out of range", config.port)));
    let listener = TcpListener::bind((config.host, port)).await.unwrap_or_else(|e| fail(format, e));
    config.log_format.info(&format!("{} running on {}:{} with {} workers", config.name, config.host, config.port, config.workers));
    // the listener counts the connections and stops accepting when the draining begins; the no-op closure
    // is what gives the routes the address of the client
    let listener = Listener::new(listener, state.shutdown.clone()).tap_io(|_| {});
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap_or_else(|e| fail(format, e));
}

fn fail(format: LogFormat, error: impl std::fmt::Display) -> ! {
    format.error(&format!("Internal Server Error: {error}"));
    exit(1);
}

/// SIGTERM and SIGINT end the process. With no `SHUTDOWN_GRACE_SECONDS` that is right away, without
/// waiting for the requests in progress. With it the server drains first: it refuses new connections,
/// serves the ones open, and ends when nothing is open or the grace period is over, whichever comes
/// first. Another signal ends it at once.
async fn shutdown(state: Arc<AppState>) {
    let format = state.config.log_format;
    let grace = state.config.shutdown_grace_seconds;
    let mut term = signal(SignalKind::terminate()).expect("a SIGTERM handler");
    let mut int = signal(SignalKind::interrupt()).expect("a SIGINT handler");
    let name = tokio::select! {
        _ = term.recv() => "SIGTERM",
        _ = int.recv() => "SIGINT",
    };
    if grace == 0 {
        format.info(&format!("{name} received, exiting"));
        state.stop();
        exit(0);
    }

    state.shutdown.begin();
    format.info(&format!("{name} received, draining for up to {grace} s"));
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = term.recv() => { format.info("SIGTERM received again, exiting now"); break; }
            _ = int.recv() => { format.info("SIGINT received again, exiting now"); break; }
        }
        let s = state.shutdown.snapshot();
        let seconds = s.drain_seconds.unwrap_or(0.0);
        if s.open_requests == 0 && s.open_connections == 0 {
            format.info(&format!("Drained after {seconds:.1} s: nothing is open, exiting"));
            break;
        }
        if seconds >= grace as f64 {
            format.info(&format!(
                "The grace period of {grace} s is over with {} requests and {} connections still open, exiting",
                s.open_requests, s.open_connections
            ));
            break;
        }
        format.info(&format!("Draining, {seconds:.0} s: {} requests and {} connections open", s.open_requests, s.open_connections));
    }
    state.stop();
    exit(0);
}
