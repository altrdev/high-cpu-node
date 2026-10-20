//! A list of steps that runs by itself, one after the other, so a test can be repeated without
//! calling several endpoints by hand. This is the list and where it is; the routes run it.
use crate::config::Config;
use crate::endpoints::specs;
use crate::http::params::Param;
use serde_json::{Map, Value, json};
use std::sync::Mutex;
use tokio::task::AbortHandle;
use tokio::time::Instant;

/// The most steps a scenario can have
pub const MAX_STEPS: usize = 100;

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

// The keys that name what a step does
pub const ACTIONS: [&str; 7] = ["cpu", "memory", "disk", "fd", "connections", "ready", "block"];

fn integer(step: usize, name: &str, value: &Value, param: &Param) -> Result<i64, String> {
    let number = value.as_i64().ok_or_else(|| format!("step {step}: {name} must be an integer"))?;
    param.check(name, number).map_err(|message| format!("step {step}: {message}"))
}

fn parse_step(number: usize, value: &Value, config: &Config) -> Result<Step, String> {
    let object = value.as_object().ok_or_else(|| format!("step {number}: must be an object, like {{\"cpu\": 80, \"seconds\": 60}}"))?;
    let actions: Vec<&str> = ACTIONS.iter().copied().filter(|name| object.contains_key(*name)).collect();
    if let Some(unknown) = object.keys().find(|key| !ACTIONS.contains(&key.as_str()) && !["seconds", "workers"].contains(&key.as_str())) {
        return Err(format!("step {number}: unknown key '{unknown}', a step takes one of {} and seconds", ACTIONS.join(", ")));
    }
    if actions.len() > 1 {
        return Err(format!("step {number}: more than one action ({}), a step does one thing", actions.join(" and ")));
    }
    if object.contains_key("workers") && actions != ["cpu"] {
        return Err(format!("step {number}: workers goes with cpu"));
    }

    let s = specs(config);
    let seconds_param = Param::new(1, 1, config.max_hold_seconds);
    let seconds = |required: bool| -> Result<Option<u64>, String> {
        match object.get("seconds") {
            Some(value) => integer(number, "seconds", value, &seconds_param).map(|n| Some(n as u64)),
            None if required => Err(format!("step {number}: seconds is needed")),
            None => Ok(None),
        }
    };

    let Some(action) = actions.first() else {
        let seconds = seconds(false)?.ok_or_else(|| format!("step {number}: nothing to do, name an action or give seconds for a pause"))?;
        return Ok(Step { action: Action::Pause, seconds });
    };
    let value = &object[*action];
    let action_of = match *action {
        "cpu" => Action::Cpu {
            percent: integer(number, "cpu", value, &s.cpu[0].1)?,
            workers: match object.get("workers") {
                Some(workers) => integer(number, "workers", workers, &s.cpu[2].1)?,
                None => config.workers,
            },
        },
        "memory" => Action::Memory { mb: integer(number, "memory", value, &s.memory[0].1)? },
        "disk" => Action::Disk { mb: integer(number, "disk", value, &s.disk[0].1)? },
        "fd" => Action::Fd { count: integer(number, "fd", value, &s.fd[0].1)? },
        "connections" => Action::Connections { count: integer(number, "connections", value, &s.connections[0].1)? },
        "ready" if value == "off" => Action::NotReady,
        "ready" => return Err(format!("step {number}: ready only takes \"off\", and /ready/on is what the end of the step does")),
        _ => Action::Block { ms: integer(number, "block", value, &s.block[0].1)? },
    };
    // a block lasts as long as it says, and the others as long as they are asked to
    let seconds = if let Action::Block { ms } = action_of {
        if object.contains_key("seconds") {
            return Err(format!("step {number}: a block lasts the ms it says, without seconds"));
        }
        ((ms as u64).div_ceil(1000)).max(1)
    } else {
        seconds(true)?.expect("seconds is required")
    };
    Ok(Step { action: action_of, seconds })
}

/// The steps in a request body, or what is wrong with it and in which step
pub fn parse(body: &[u8], config: &Config) -> Result<Vec<Step>, String> {
    let value: Value = serde_json::from_slice(body).map_err(|e| format!("The body is not JSON: {e}"))?;
    let list = value.as_array().ok_or("The body must be a list of steps, like [{\"cpu\": 80, \"seconds\": 60}]")?;
    if list.is_empty() {
        return Err("The scenario has no steps".to_string());
    }
    if list.len() > MAX_STEPS {
        return Err(format!("A scenario has at most {MAX_STEPS} steps, this one has {}", list.len()));
    }
    list.iter().enumerate().map(|(index, step)| parse_step(index + 1, step, config)).collect()
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

    fn config() -> Config {
        let get = |name: &str| match name {
            "WORKERS" => Some("4".to_string()),
            "MAX_MEMORY_MB" => Some("512".to_string()),
            "MAX_HOLD_SECONDS" => Some("300".to_string()),
            _ => None,
        };
        Config::read(get, |_| {})
    }

    fn parsed(body: &str) -> Result<Vec<Step>, String> {
        parse(body.as_bytes(), &config())
    }

    #[test]
    fn a_scenario_is_a_list_of_steps() {
        let steps = parsed(
            r#"[{"cpu":80,"seconds":60},{"memory":500,"seconds":30},{"ready":"off","seconds":20},{"seconds":5},{"cpu":50,"workers":2,"seconds":1}]"#,
        )
        .unwrap();
        assert_eq!(steps[0], Step { action: Action::Cpu { percent: 80, workers: 4 }, seconds: 60 });
        assert_eq!(steps[1], Step { action: Action::Memory { mb: 500 }, seconds: 30 });
        assert_eq!(steps[2], Step { action: Action::NotReady, seconds: 20 });
        assert_eq!(steps[3], Step { action: Action::Pause, seconds: 5 });
        assert_eq!(steps[4].action, Action::Cpu { percent: 50, workers: 2 });
    }

    #[test]
    fn a_block_lasts_its_ms_rounded_up_to_the_second() {
        assert_eq!(parsed(r#"[{"block":2500}]"#).unwrap()[0], Step { action: Action::Block { ms: 2500 }, seconds: 3 });
        assert_eq!(parsed(r#"[{"block":1}]"#).unwrap()[0].seconds, 1);
        assert!(parsed(r#"[{"block":1000,"seconds":5}]"#).unwrap_err().contains("without seconds"));
    }

    #[test]
    fn what_is_wrong_says_which_step() {
        for (body, message) in [
            ("nope", "not JSON"),
            ("{}", "must be a list"),
            ("[]", "no steps"),
            (r#"[1]"#, "step 1: must be an object"),
            (r#"[{"seconds":5},{"memory":513,"seconds":5}]"#, "step 2: memory must be between 1 and 512"),
            (r#"[{"cpu":101,"seconds":5}]"#, "step 1: cpu must be between 0 and 100"),
            (r#"[{"cpu":50,"workers":5,"seconds":5}]"#, "step 1: workers must be between 1 and 4"),
            (r#"[{"cpu":"high","seconds":5}]"#, "step 1: cpu must be an integer"),
            (r#"[{"cpu":50}]"#, "step 1: seconds is needed"),
            (r#"[{"cpu":50,"seconds":0}]"#, "step 1: seconds must be between 1 and 300"),
            (r#"[{"cpu":50,"seconds":301}]"#, "step 1: seconds must be between 1 and 300"),
            (r#"[{"cpu":50,"memory":10,"seconds":5}]"#, "step 1: more than one action (cpu and memory)"),
            (r#"[{"crash":1,"seconds":5}]"#, "step 1: unknown key 'crash'"),
            (r#"[{"oom":1,"seconds":5}]"#, "step 1: unknown key 'oom'"),
            (r#"[{"memory":10,"workers":2,"seconds":5}]"#, "step 1: workers goes with cpu"),
            (r#"[{"ready":"on","seconds":5}]"#, "step 1: ready only takes \"off\""),
            (r#"[{}]"#, "step 1: nothing to do"),
        ] {
            let error = parsed(body).unwrap_err();
            assert!(error.contains(message), "{body}: {error}");
        }
    }

    #[test]
    fn the_documentation_has_a_row_for_every_action() {
        let page = include_str!("../../book/src/scenarios.md");
        for action in ACTIONS {
            assert!(page.contains(&format!("| `{action}` ")), "the page Scenarios has no row for '{action}'");
        }
        assert!(page.contains(&format!("at most **{MAX_STEPS} steps**")), "the page does not say how many steps there can be");
    }

    #[test]
    fn a_scenario_has_a_limit_of_steps() {
        let steps = vec![r#"{"seconds":1}"#; MAX_STEPS + 1].join(",");
        assert!(parsed(&format!("[{steps}]")).unwrap_err().contains("at most 100 steps"));
        let steps = vec![r#"{"seconds":1}"#; MAX_STEPS].join(",");
        assert_eq!(parsed(&format!("[{steps}]")).unwrap().len(), MAX_STEPS);
    }

    #[test]
    fn a_step_is_written_back_as_it_was_given() {
        let steps = parsed(r#"[{"cpu":80,"workers":2,"seconds":60},{"ready":"off","seconds":3},{"seconds":4}]"#).unwrap();
        assert_eq!(steps[0].to_json(), json!({"cpu": 80, "workers": 2, "seconds": 60}));
        assert_eq!(steps[1].to_json(), json!({"ready": "off", "seconds": 3}));
        assert_eq!(steps[2].to_json(), json!({"seconds": 4}));
    }

    #[tokio::test(start_paused = true)]
    async fn it_tells_where_it_is_and_what_is_left() {
        let scenario = Scenario::default();
        assert_eq!(scenario.snapshot(true), json!({"state": "idle"}));

        let run = scenario.start(parsed(r#"[{"memory":10,"seconds":30},{"seconds":20},{"cpu":10,"seconds":10}]"#).unwrap()).unwrap();
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
        let first = scenario.start(parsed(r#"[{"seconds":5},{"seconds":5}]"#).unwrap()).unwrap();
        scenario.begin_step(first, 1);
        assert_eq!(scenario.start(parsed(r#"[{"seconds":1}]"#).unwrap()).unwrap_err(), Busy { step: 2, steps: 2 });

        assert_eq!(scenario.stop().map(|step| step.seconds), Some(5));
        assert_eq!(scenario.snapshot(false)["state"], "stopped");
        assert_eq!(scenario.stop(), None, "there is nothing left to stop");

        let second = scenario.start(parsed(r#"[{"seconds":1}]"#).unwrap()).unwrap();
        scenario.finish(first, Err("late".into()));
        assert_eq!(scenario.snapshot(false)["state"], "running", "an old scenario can't change the new one");
        scenario.finish(second, Err("memory: nothing left".into()));
        let failed = scenario.snapshot(true);
        assert_eq!((failed["state"].as_str(), failed["error"].as_str()), (Some("failed"), Some("memory: nothing left")));
        assert_eq!(failed["plan"][0]["state"], "failed");
    }
}
