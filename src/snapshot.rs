//! What the server is doing right now, in raw units, shared by `/info` (MB) and `/metrics` (bytes).
use crate::app::AppState;
use crate::resources::log::LogState;
use crate::resources::ready::ReadyState;
use crate::resources::shutdown::ShutdownSnapshot;
use crate::resources::workers::StressSnapshot;
use crate::system;

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

pub struct Connections {
    pub held: i64,
    pub limit: i64,
}

pub struct Snapshot {
    pub stress: StressSnapshot,
    pub delay_in_flight: usize,
    pub memory: Memory,
    pub disk: Disk,
    pub fd: Fd,
    pub connections: Connections,
    pub ready: ReadyState,
    pub log: Option<LogState>,
    pub shutdown: ShutdownSnapshot,
}

impl AppState {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            stress: self.pool.snapshot(),
            delay_in_flight: self.delay.in_flight(),
            memory: Memory {
                held_mb: self.memory.budget().held(),
                limit_mb: self.memory.budget().limit(),
                rss_bytes: system::rss_bytes(),
                container_limit_bytes: system::container_limit_bytes(),
            },
            disk: Disk {
                held_mb: self.disk.budget().held(),
                limit_mb: self.disk.budget().limit(),
                dir: self.config.disk_dir.clone(),
                free_bytes: system::free_bytes(&self.config.disk_dir),
            },
            fd: Fd { held: self.fd.budget().held(), limit: self.fd.budget().limit() },
            connections: Connections { held: self.connections.budget().held(), limit: self.connections.budget().limit() },
            ready: self.ready.state(),
            log: self.log.state(),
            shutdown: self.shutdown.snapshot(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn state(disk_dir: &str) -> AppState {
        let get = |name: &str| match name {
            "DISK_DIR" => Some(disk_dir.to_string()),
            "WORKERS" => Some("2".into()),
            "MAX_MEMORY_MB" => Some("64".into()),
            "MAX_DISK_MB" => Some("32".into()),
            "MAX_FD" => Some("20".into()),
            _ => None,
        };
        AppState::new(Config::read(get, |_| {})).unwrap()
    }

    #[test]
    fn it_reports_the_limits_and_that_nothing_is_held() {
        let dir = std::env::temp_dir().join(format!("torchio-snapshot-{}", std::process::id()));
        let snapshot = state(dir.to_str().unwrap()).snapshot();
        assert_eq!((snapshot.memory.held_mb, snapshot.memory.limit_mb), (0, 64));
        assert_eq!((snapshot.disk.held_mb, snapshot.disk.limit_mb), (0, 32));
        assert_eq!((snapshot.fd.held, snapshot.fd.limit), (0, 20));
        assert_eq!(snapshot.stress.workers, 2);
        assert_eq!(snapshot.delay_in_flight, 0);
        assert!(snapshot.ready.ready);
        assert!(snapshot.log.is_none());
        assert!(snapshot.memory.rss_bytes > 0);
        assert!(snapshot.disk.free_bytes.is_some(), "the directory was created");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn it_follows_what_is_held() {
        let dir = std::env::temp_dir().join(format!("torchio-snapshot-held-{}", std::process::id()));
        let state = state(dir.to_str().unwrap());
        state.fd.hold(5, 30).unwrap();
        state.memory.hold(8, 30).await.unwrap();
        let snapshot = state.snapshot();
        assert_eq!(snapshot.fd.held, 5);
        assert_eq!(snapshot.memory.held_mb, 8);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_directory_that_cannot_be_used_has_no_free_space() {
        let snapshot = state("/dev/null/stress").snapshot();
        assert_eq!(snapshot.disk.free_bytes, None);
    }
}
