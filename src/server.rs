//! Starts the server, and ends the process.
use crate::app::{AppState, router};
use crate::config::Config;
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
    let state = Arc::new(AppState::new(config.clone()).unwrap_or_else(|e| fail(format!("TypeError: {e}"))));
    let app = router(state.clone());

    tokio::spawn(shutdown(state));

    // START_DELAY_SECONDS: the port stays closed for a while, like an application that is slow to start
    if config.start_delay_seconds > 0 {
        println!("{} will start listening in {} s", config.name, config.start_delay_seconds);
        tokio::time::sleep(Duration::from_secs(config.start_delay_seconds as u64)).await;
    }

    let port = u16::try_from(config.port).unwrap_or_else(|_| fail(format!("port {} is out of range", config.port)));
    let listener = TcpListener::bind((config.host, port)).await.unwrap_or_else(|e| fail(e));
    println!("{} running on {}:{} with {} workers", config.name, config.host, config.port, config.workers);
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap_or_else(|e| fail(e));
}

fn fail(error: impl std::fmt::Display) -> ! {
    eprintln!("Internal Server Error: {error}");
    exit(1);
}

/// SIGTERM and SIGINT end the process right away, without waiting for the requests in progress
async fn shutdown(state: Arc<AppState>) {
    let mut term = signal(SignalKind::terminate()).expect("a SIGTERM handler");
    let mut int = signal(SignalKind::interrupt()).expect("a SIGINT handler");
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
    state.stop();
    exit(0);
}
