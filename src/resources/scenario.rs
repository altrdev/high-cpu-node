//! A list of steps that runs by itself, one after the other, so a test can be repeated without
//! calling several endpoints by hand. This is the list and where it is; the routes run it.
use serde_json::{Map, Value, json};
use std::sync::Mutex;
use tokio::task::AbortHandle;
use tokio::time::Instant;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Cpu { percent: i64, workers: i64 },
    Memory { mb: i64 },
    Disk { mb: i64 },
    Fd { count: i64 },
    Connections { count: i64 },
    NotReady,
    Block { ms: i64 },
    Pause,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub action: Action,
    pub seconds: u64,
}

impl Action {
    /// The key that names it in a scenario
    pub fn name(&self) -> &'static str {
        match self {
            Action::Cpu { .. } => "cpu",
            Action::Memory { .. } => "memory",
            Action::Disk { .. } => "disk",
            Action::Fd { .. } => "fd",
            Action::Connections { .. } => "connections",
            Action::NotReady => "ready",
            Action::Block { .. } => "block",
            Action::Pause => "pause",
        }
    }
}

impl Step {
    /// The step as it is written in a scenario
    pub fn to_json(&self) -> Value {
        let mut map = Map::new();
        match self.action {
            Action::Cpu { percent, workers } => {
                map.insert("cpu".into(), json!(percent));
                map.insert("workers".into(), json!(workers));
            }
            Action::Memory { mb } => {
                map.insert("memory".into(), json!(mb));
            }
            Action::Disk { mb } => {
                map.insert("disk".into(), json!(mb));
            }
            Action::Fd { count } => {
                map.insert("fd".into(), json!(count));
            }
            Action::Connections { count } => {
                map.insert("connections".into(), json!(count));
            }
            Action::NotReady => {
                map.insert("ready".into(), json!("off"));
            }
            Action::Block { ms } => {
                map.insert("block".into(), json!(ms));
            }
            Action::Pause => {}
        }
        map.insert("seconds".into(), json!(self.seconds));
        Value::Object(map)
    }

    /// What the step does, for the log
    pub fn describe(&self) -> String {
        let what = match self.action {
            Action::Cpu { percent, workers } => format!("cpu {percent}% of {workers} workers"),
            Action::Memory { mb } => format!("memory {mb} MB"),
            Action::Disk { mb } => format!("disk {mb} MB"),
            Action::Fd { count } => format!("{count} descriptors"),
            Action::Connections { count } => format!("{count} connections"),
            Action::NotReady => "not ready".to_string(),
            Action::Block { ms } => format!("block {ms} ms"),
            Action::Pause => "pause".to_string(),
        };
        format!("{what} for {} s", self.seconds)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Phase {
    #[default]
    Idle,
    Running,
    Finished,
    Stopped,
    Failed,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Running => "running",
            Phase::Finished => "finished",
            Phase::Stopped => "stopped",
            Phase::Failed => "failed",
        }
    }
}

#[derive(Default)]
struct Inner {
    phase: Phase,
    steps: Vec<Step>,
    current: usize,
    step_started: Option<Instant>,
    error: Option<String>,
    task: Option<AbortHandle>,
    // changes with every scenario, so what an old one reports does not touch the new one
    run: u64,
}

#[derive(Default)]
pub struct Scenario {
    inner: Mutex<Inner>,
}

/// There is a scenario running already
#[derive(Debug, PartialEq)]
pub struct Busy {
    pub step: usize,
    pub steps: usize,
}

impl Scenario {
    /// Takes `steps` as the scenario that is running, and gives the number to report its progress with
    pub fn start(&self, steps: Vec<Step>) -> Result<u64, Busy> {
        let mut inner = self.inner.lock().unwrap();
        if inner.phase == Phase::Running {
            return Err(Busy { step: inner.current + 1, steps: inner.steps.len() });
        }
        inner.run += 1;
        *inner = Inner { phase: Phase::Running, steps, current: 0, step_started: None, error: None, task: None, run: inner.run };
        Ok(inner.run)
    }

    /// The task that runs it, to stop it. A scenario that is over already has no use for it.
    pub fn attach(&self, run: u64, task: AbortHandle) {
        let mut inner = self.inner.lock().unwrap();
        if inner.run == run && inner.phase == Phase::Running {
            inner.task = Some(task);
        }
    }

    pub fn begin_step(&self, run: u64, index: usize) {
        let mut inner = self.inner.lock().unwrap();
        if inner.run == run && inner.phase == Phase::Running {
            inner.current = index;
            inner.step_started = Some(Instant::now());
        }
    }

    pub fn finish(&self, run: u64, outcome: Result<(), String>) {
        let mut inner = self.inner.lock().unwrap();
        if inner.run == run && inner.phase == Phase::Running {
            inner.task = None;
            match outcome {
                Ok(()) => inner.phase = Phase::Finished,
                Err(error) => {
                    inner.phase = Phase::Failed;
                    inner.error = Some(error);
                }
            }
        }
    }

    /// Ends the scenario that is running and gives back the step it was in, so its effect can be undone
    pub fn stop(&self) -> Option<Step> {
        let mut inner = self.inner.lock().unwrap();
        if inner.phase != Phase::Running {
            return None;
        }
        if let Some(task) = inner.task.take() {
            task.abort();
        }
        inner.phase = Phase::Stopped;
        inner.steps.get(inner.current).cloned()
    }

    /// Where it is. `plan` adds every step with its state, which `/info` leaves out.
    pub fn snapshot(&self, plan: bool) -> Value {
        let inner = self.inner.lock().unwrap();
        if inner.phase == Phase::Idle {
            return json!({"state": "idle"});
        }
        let running = inner.phase == Phase::Running;
        let left = |step: &Step| match inner.step_started {
            Some(started) if running => step.seconds.saturating_sub(started.elapsed().as_secs_f64() as u64),
            _ => step.seconds,
        };
        let mut map = Map::new();
        map.insert("state".into(), json!(inner.phase.name()));
        map.insert("step".into(), json!(inner.current + 1));
        map.insert("steps".into(), json!(inner.steps.len()));
        if running {
            let current = left(&inner.steps[inner.current]);
            let after: u64 = inner.steps[inner.current + 1..].iter().map(|step| step.seconds).sum();
            map.insert("secondsLeft".into(), json!(current));
            map.insert("remainingSeconds".into(), json!(current + after));
            map.insert("now".into(), inner.steps[inner.current].to_json());
        }
        if let Some(error) = &inner.error {
            map.insert("error".into(), json!(error));
        }
        if plan {
            let steps: Vec<Value> = inner
                .steps
                .iter()
                .enumerate()
                .map(|(index, step)| {
                    let state = match (index.cmp(&inner.current), inner.phase) {
                        (std::cmp::Ordering::Less, _) | (_, Phase::Finished) => "done",
                        (std::cmp::Ordering::Equal, Phase::Running) => "running",
                        (std::cmp::Ordering::Equal, Phase::Failed) => "failed",
                        (std::cmp::Ordering::Equal, _) => "stopped",
                        (std::cmp::Ordering::Greater, _) => "pending",
                    };
                    let mut value = step.to_json();
                    value["state"] = json!(state);
                    value
                })
                .collect();
            map.insert("plan".into(), Value::Array(steps));
        }
        Value::Object(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(action: Action, seconds: u64) -> Step {
        Step { action, seconds }
    }

    fn pauses(seconds: &[u64]) -> Vec<Step> {
        seconds.iter().map(|seconds| step(Action::Pause, *seconds)).collect()
    }

    #[test]
    fn a_step_is_written_back_as_it_was_given() {
        let steps = [step(Action::Cpu { percent: 80, workers: 2 }, 60), step(Action::NotReady, 3), step(Action::Pause, 4)];
        assert_eq!(steps[0].to_json(), json!({"cpu": 80, "workers": 2, "seconds": 60}));
        assert_eq!(steps[1].to_json(), json!({"ready": "off", "seconds": 3}));
        assert_eq!(steps[2].to_json(), json!({"seconds": 4}));
    }

    #[tokio::test(start_paused = true)]
    async fn it_tells_where_it_is_and_what_is_left() {
        let scenario = Scenario::default();
        assert_eq!(scenario.snapshot(true), json!({"state": "idle"}));

        let run = scenario
            .start(vec![step(Action::Memory { mb: 10 }, 30), step(Action::Pause, 20), step(Action::Cpu { percent: 10, workers: 1 }, 10)])
            .unwrap();
        scenario.begin_step(run, 0);
        tokio::time::advance(std::time::Duration::from_secs(10)).await;
        let now = scenario.snapshot(true);
        assert_eq!((now["state"].as_str(), now["step"].as_u64(), now["steps"].as_u64()), (Some("running"), Some(1), Some(3)));
        assert_eq!((now["secondsLeft"].as_u64(), now["remainingSeconds"].as_u64()), (Some(20), Some(50)));
        let states: Vec<&str> = now["plan"].as_array().unwrap().iter().map(|step| step["state"].as_str().unwrap()).collect();
        assert_eq!(states, ["running", "pending", "pending"]);
        assert!(scenario.snapshot(false).get("plan").is_none());

        scenario.begin_step(run, 1);
        scenario.finish(run, Ok(()));
        let done = scenario.snapshot(true);
        assert_eq!(done["state"], "finished");
        assert!(done.get("secondsLeft").is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn only_one_runs_at_a_time_and_a_new_one_can_follow() {
        let scenario = Scenario::default();
        let first = scenario.start(pauses(&[5, 5])).unwrap();
        scenario.begin_step(first, 1);
        assert_eq!(scenario.start(pauses(&[1])).unwrap_err(), Busy { step: 2, steps: 2 });

        assert_eq!(scenario.stop().map(|step| step.seconds), Some(5));
        assert_eq!(scenario.snapshot(false)["state"], "stopped");
        assert_eq!(scenario.stop(), None, "there is nothing left to stop");

        let second = scenario.start(pauses(&[1])).unwrap();
        scenario.finish(first, Err("late".into()));
        assert_eq!(scenario.snapshot(false)["state"], "running", "an old scenario can't change the new one");
        scenario.finish(second, Err("memory: nothing left".into()));
        let failed = scenario.snapshot(true);
        assert_eq!((failed["state"].as_str(), failed["error"].as_str()), (Some("failed"), Some("memory: nothing left")));
        assert_eq!(failed["plan"][0]["state"], "failed");
    }
}
