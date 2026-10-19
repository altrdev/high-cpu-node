use crate::app::AppState;
use crate::ready::ReadyState;
use crate::routes::log::LogState;
use crate::sys;
use crate::workers::StressSnapshot;

pub struct Memory {
    pub held_mb: i64,
    pub limit_mb: i64,
    pub rss_bytes: u64,
    pub container_limit_bytes: Option<u64>,
}

pub struct Disk {
    pub held_mb: i64,
    pub limit_mb: i64,
    pub dir: String,
    pub free_bytes: Option<u64>,
}

pub struct Fd {
    pub held: i64,
    pub limit: i64,
}

// What the server is doing right now, in raw units, shared by /info (MB) and /metrics (bytes)
pub struct Snapshot {
    pub stress: StressSnapshot,
    pub delay_in_flight: usize,
    pub memory: Memory,
    pub disk: Disk,
    pub fd: Fd,
    pub ready: ReadyState,
    pub log: Option<LogState>,
}

impl AppState {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            stress: self.pool.snapshot(),
            delay_in_flight: self.delay.in_flight(),
            memory: Memory {
                held_mb: self.memory.held(),
                limit_mb: self.memory.limit(),
                rss_bytes: sys::rss_bytes(),
                container_limit_bytes: sys::container_limit_bytes(),
            },
            disk: Disk {
                held_mb: self.disk.budget.held(),
                limit_mb: self.disk.budget.limit(),
                dir: self.config.disk_dir.clone(),
                free_bytes: sys::free_bytes(&self.config.disk_dir),
            },
            fd: Fd { held: self.fd.budget.held(), limit: self.fd.budget.limit() },
            ready: self.ready.state(),
            log: self.log.state(),
        }
    }
}
