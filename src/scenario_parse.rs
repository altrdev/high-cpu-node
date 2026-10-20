//! Reads a scenario from the body of a request: a list of steps, each checked against the limits
//! of the endpoint it stands for, so a wrong one is reported before anything starts. What a step is,
//! and where a running scenario is, belongs to [`resources::scenario`](crate::resources::scenario).
use crate::config::Config;
use crate::endpoints::specs;
use crate::http::params::Param;
use crate::resources::scenario::{Action, Step};
use serde_json::Value;

/// The most steps a scenario can have
pub const MAX_STEPS: usize = 100;

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
        let page = include_str!("../book/src/scenarios.md");
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
}
