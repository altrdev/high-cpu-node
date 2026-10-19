use crate::app::{AppState, VERSION};
use crate::units::MB;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

type Labels = Vec<(&'static str, String)>;

fn escape_label(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\n', "\\n").replace('"', "\\\"")
}

// Adds a metric to `lines`; one without samples is left out altogether
fn metric(lines: &mut Vec<String>, name: &str, kind: &str, help: &str, samples: Vec<(Labels, f64)>) {
    if samples.is_empty() {
        return;
    }
    lines.push(format!("# HELP {name} {help}"));
    lines.push(format!("# TYPE {name} {kind}"));
    for (labels, value) in samples {
        let labels = if labels.is_empty() {
            String::new()
        } else {
            let pairs: Vec<String> = labels.iter().map(|(key, value)| format!("{key}=\"{}\"", escape_label(value))).collect();
            format!("{{{}}}", pairs.join(","))
        };
        lines.push(format!("{name}{labels} {value}"));
    }
}

fn one(value: impl Into<f64>) -> Vec<(Labels, f64)> {
    vec![(Vec::new(), value.into())]
}

fn optional(value: Option<u64>) -> Vec<(Labels, f64)> {
    value.map_or_else(Vec::new, |bytes| one(bytes as f64))
}

// The figures of /info in the Prometheus text format
pub async fn handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let s = state.snapshot();
    let mb = MB as f64;
    let started = state.started_at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64();
    let mut lines = Vec::new();

    metric(&mut lines, "torchio_info", "gauge", "Version of the server.", vec![(vec![("version", VERSION.to_string())], 1.0)]);
    metric(&mut lines, "torchio_start_time_seconds", "gauge", "Start time of the server, in Unix time.", one(started));
    metric(&mut lines, "torchio_workers", "gauge", "Worker processes for /stress.", one(s.stress.workers as f64));
    metric(&mut lines, "torchio_stress_requests_in_flight", "gauge", "/stress requests being calculated.", one(s.stress.in_flight as f64));
    metric(
        &mut lines,
        "torchio_worker_requests_in_flight",
        "gauge",
        "/stress requests being calculated, by worker.",
        s.stress.per_worker.iter().enumerate().map(|(index, count)| (vec![("worker", index.to_string())], *count as f64)).collect(),
    );
    metric(
        &mut lines,
        "torchio_cpu_burn_percent",
        "gauge",
        "Share of the time each worker is kept busy by /cpu, 0 when it is not running.",
        one(s.stress.burn.as_ref().map_or(0.0, |burn| burn.percent as f64)),
    );
    metric(&mut lines, "torchio_delay_requests_in_flight", "gauge", "/delay requests still waiting.", one(s.delay_in_flight as f64));
    metric(&mut lines, "torchio_memory_held_bytes", "gauge", "Memory held by /memory.", one(s.memory.held_mb as f64 * mb));
    metric(&mut lines, "torchio_memory_limit_bytes", "gauge", "Total memory /memory may hold (MAX_MEMORY_MB).", one(s.memory.limit_mb as f64 * mb));
    metric(&mut lines, "process_resident_memory_bytes", "gauge", "Resident memory of the server process.", one(s.memory.rss_bytes as f64));
    metric(&mut lines, "torchio_container_memory_limit_bytes", "gauge", "Memory limit of the container, when there is one.", optional(s.memory.container_limit_bytes));
    metric(&mut lines, "torchio_disk_held_bytes", "gauge", "Disk space held by /disk.", one(s.disk.held_mb as f64 * mb));
    metric(&mut lines, "torchio_disk_limit_bytes", "gauge", "Total disk space /disk may hold (MAX_DISK_MB).", one(s.disk.limit_mb as f64 * mb));
    metric(&mut lines, "torchio_disk_free_bytes", "gauge", "Free space in DISK_DIR.", optional(s.disk.free_bytes));
    metric(&mut lines, "torchio_fd_held", "gauge", "File descriptors held by /fd.", one(s.fd.held as f64));
    metric(&mut lines, "torchio_fd_limit", "gauge", "Total file descriptors /fd may hold (MAX_FD).", one(s.fd.limit as f64));
    metric(&mut lines, "torchio_ready", "gauge", "1 when /ready answers 200, 0 while it answers 503.", one(if s.ready.ready { 1.0 } else { 0.0 }));
    metric(
        &mut lines,
        "torchio_http_requests_total",
        "counter",
        "Requests answered, by route and status code.",
        state.counters.entries().into_iter().map(|e| (vec![("route", e.route), ("code", e.code.to_string())], e.count as f64)).collect(),
    );

    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")], lines.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_escaped() {
        assert_eq!(escape_label("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }

    #[test]
    fn a_metric_without_samples_is_left_out() {
        let mut lines = Vec::new();
        metric(&mut lines, "x", "gauge", "nothing", Vec::new());
        assert!(lines.is_empty());
        metric(&mut lines, "x", "gauge", "one", vec![(vec![("a", "b".into())], 2.0), (Vec::new(), 0.5)]);
        assert_eq!(lines, ["# HELP x one", "# TYPE x gauge", "x{a=\"b\"} 2", "x 0.5"]);
    }
}
