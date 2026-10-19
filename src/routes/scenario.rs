use crate::app::AppState;
use crate::resources::disk::WriteError;
use crate::resources::memory::HoldError;
use crate::resources::scenario::{self, Action, Step};
use crate::system::errno_name;
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use std::time::Duration;

/// A moment between two steps, so what the last one held is given back before the next asks for it
const SETTLE: Duration = Duration::from_millis(100);

/// Where the scenario is, with every step and its state
pub async fn status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(state.scenario.snapshot(true))
}

/// Starts the scenario in the body: every step is checked against the limits first, and a wrong
/// one is a `400` that says which. The steps then run one after the other, by themselves.
pub async fn start(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    let steps = match scenario::parse(&body, &state.config) {
        Ok(steps) => steps,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    let run = match state.scenario.start(steps.clone()) {
        Ok(run) => run,
        Err(busy) => {
            let message = format!("A scenario is already running, at step {} of {}: stop it with DELETE /scenario", busy.step, busy.steps);
            return (StatusCode::CONFLICT, message).into_response();
        }
    };
    state.config.log_format.info(&format!("Scenario started: {} steps", steps.len()));
    let task = tokio::spawn(run_steps(Arc::clone(&state), run, steps));
    state.scenario.attach(run, task.abort_handle());
    (StatusCode::ACCEPTED, Json(state.scenario.snapshot(true))).into_response()
}

/// Ends the scenario that is running. What the current step holds for a time (memory, disk,
/// descriptors, connections) is given back when its seconds are over; the CPU load and the readiness
/// switch are undone at once.
pub async fn stop(State(state): State<Arc<AppState>>) -> Response {
    let Some(step) = state.scenario.stop() else {
        return (StatusCode::NOT_FOUND, "No scenario is running").into_response();
    };
    match step.action {
        Action::Cpu { .. } => state.pool.burn(0, 0, 0),
        Action::NotReady => state.ready.on(),
        _ => {}
    }
    state.config.log_format.info("Scenario stopped");
    Json(state.scenario.snapshot(true)).into_response()
}

async fn run_steps(state: Arc<AppState>, run: u64, steps: Vec<Step>) {
    let format = state.config.log_format;
    let total = steps.len();
    for (index, step) in steps.iter().enumerate() {
        state.scenario.begin_step(run, index);
        format.info(&format!("Scenario step {} of {total}: {}", index + 1, step.describe()));
        if let Err(message) = perform(&state, step).await {
            let error = format!("step {}: {message}", index + 1);
            format.warn(&format!("Scenario failed at {error}"));
            state.scenario.finish(run, Err(error));
            return;
        }
        tokio::time::sleep(Duration::from_secs(step.seconds)).await;
        if index + 1 < total {
            tokio::time::sleep(SETTLE).await;
        }
    }
    format.info("Scenario finished");
    state.scenario.finish(run, Ok(()));
}

/// Does what a step says, with the budgets the endpoints use. The error is why it could not.
async fn perform(state: &AppState, step: &Step) -> Result<(), String> {
    let seconds = step.seconds;
    match step.action {
        Action::Cpu { percent, workers } => state.pool.burn(percent, seconds as i64, workers as usize),
        Action::Memory { mb } => match state.memory.hold(mb, seconds).await {
            Ok(()) => {}
            Err(HoldError::OverBudget(over)) => return Err(format!("{} MB of memory already held, limit is {} MB", over.held, over.limit)),
            Err(HoldError::Allocation(e)) => return Err(format!("could not allocate {mb} MB: {e}")),
        },
        Action::Disk { mb } => match state.disk.write(mb, seconds).await {
            Ok(()) => {}
            Err(WriteError::OverBudget(over)) => return Err(format!("{} MB of disk already held, limit is {} MB", over.held, over.limit)),
            Err(WriteError::Write(e)) => return Err(format!("could not write {mb} MB: {}", errno_name(&e))),
        },
        Action::Fd { count } => match state.fd.hold(count, seconds) {
            // opening fewer than asked is the situation being tested: the step goes on
            Ok(opened) => warn_if_short(state, "descriptors", count, opened.opened, opened.failure),
            Err(over) => return Err(format!("{} descriptors already held, limit is {}", over.held, over.limit)),
        },
        Action::Connections { count } => match state.connections.hold(count, seconds).await {
            Ok(opened) => warn_if_short(state, "connections", count, opened.opened, opened.failure),
            Err(over) => return Err(format!("{} connections already held, limit is {}", over.held, over.limit)),
        },
        Action::NotReady => state.ready.off(seconds),
        Action::Block { ms } => {
            if let Err(busy) = state.block.start(ms as u64, state.config.log_format) {
                return Err(format!("already blocked for {} more ms", busy.ms_left));
            }
        }
        Action::Pause => {}
    }
    Ok(())
}

fn warn_if_short(state: &AppState, what: &str, asked: i64, opened: usize, failure: Option<String>) {
    if let Some(code) = failure {
        state.config.log_format.warn(&format!("Scenario opened {opened} of {asked} {what}, then {code}"));
    }
}
