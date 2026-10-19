//! The files that `/disk` writes and keeps, against the budget `MAX_DISK_MB`.
use crate::logging::LogFormat;
use crate::resources::budget::{Budget, OverBudget};
use crate::system::errno_name;
use crate::units::MB;
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;

/// The files of this server start with this, so the leftovers of a run that did not end cleanly can be told
const PREFIX: &str = "torchio-disk-";

pub struct Disk {
    budget: Arc<Budget>,
    dir: PathBuf,
    files: Mutex<HashSet<PathBuf>>,
    next_id: AtomicU64,
}

#[derive(Debug)]
pub enum WriteError {
    OverBudget(OverBudget),
    Write(io::Error),
}

impl Disk {
    /// Prepares `dir`: creates it and removes the leftovers of a previous run. A directory that
    /// can't be used is reported, and every write will fail.
    pub fn new(dir: &str, limit_mb: i64, format: LogFormat) -> Disk {
        if let Err(e) = Disk::prepare(Path::new(dir)) {
            format.warn(&format!("DISK_DIR {dir} is not usable, /disk will answer 507: {}", errno_name(&e)));
        }
        Disk { budget: Budget::new(limit_mb), dir: PathBuf::from(dir), files: Mutex::default(), next_id: AtomicU64::new(0) }
    }

    fn prepare(dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(PREFIX) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }

    pub fn budget(&self) -> &Arc<Budget> {
        &self.budget
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes `mb` megabytes to a new file and answers once the data is on disk, then keeps the
    /// file for `seconds` and removes it. That goes on even if nobody waits for the answer any more.
    pub async fn write(self: &Arc<Self>, mb: i64, seconds: u64) -> Result<(), WriteError> {
        let reservation = self.budget.reserve(mb).map_err(WriteError::OverBudget)?;
        let file = self.dir.join(format!("{PREFIX}{}", self.next_id.fetch_add(1, Ordering::Relaxed)));
        self.files.lock().unwrap().insert(file.clone());

        let (written, answer) = oneshot::channel();
        let disk = Arc::clone(self);
        tokio::spawn(async move {
            let result = write_file(&file, mb).await;
            let failed = result.is_err();
            let _ = written.send(result);
            if !failed {
                tokio::time::sleep(Duration::from_secs(seconds)).await;
            }
            disk.files.lock().unwrap().remove(&file);
            let _ = tokio::fs::remove_file(&file).await;
            drop(reservation);
        });
        match answer.await {
            Ok(result) => result.map_err(WriteError::Write),
            Err(_) => Err(WriteError::Write(io::Error::other("the write was cancelled"))),
        }
    }

    /// Removes the files still held. Used on shutdown, which can't wait for the timers.
    pub fn remove_files(&self) {
        for file in self.files.lock().unwrap().drain() {
            let _ = std::fs::remove_file(file);
        }
    }
}

async fn write_file(file: &Path, mb: i64) -> io::Result<()> {
    let mut handle = tokio::fs::File::create(file).await?;
    let chunk = vec![1u8; MB];
    for _ in 0..mb {
        handle.write_all(&chunk).await?;
    }
    handle.sync_all().await
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Dir {
            let path = std::env::temp_dir().join(format!("torchio-unit-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            Dir(path)
        }

        fn names(&self) -> Vec<String> {
            let mut names: Vec<String> =
                std::fs::read_dir(&self.0).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
            names.sort();
            names
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn it_creates_the_directory_and_removes_only_its_own_leftovers() {
        let dir = Dir::new("prepare");
        std::fs::create_dir_all(&dir.0).unwrap();
        std::fs::write(dir.0.join("torchio-disk-7"), "old").unwrap();
        std::fs::write(dir.0.join("mine.txt"), "keep").unwrap();

        let _disk = Disk::new(dir.0.to_str().unwrap(), 10, LogFormat::Text);
        assert_eq!(dir.names(), ["mine.txt"]);

        let nested = dir.0.join("a/b");
        let _disk = Disk::new(nested.to_str().unwrap(), 10, LogFormat::Text);
        assert!(nested.is_dir());
    }

    #[tokio::test]
    async fn it_writes_a_file_of_the_size_asked_and_removes_it() {
        let dir = Dir::new("write");
        let disk = Arc::new(Disk::new(dir.0.to_str().unwrap(), 10, LogFormat::Text));
        disk.write(2, 0).await.unwrap();
        // the file may already be on its way out; while it exists it has the size asked
        for name in dir.names() {
            if let Ok(meta) = std::fs::metadata(dir.0.join(name)) {
                assert_eq!(meta.len(), 2 * MB as u64);
            }
        }
        for _ in 0..200 {
            if dir.names().is_empty() && disk.budget().held() == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the file was not removed");
    }

    #[tokio::test]
    async fn it_refuses_what_does_not_fit() {
        let dir = Dir::new("budget");
        let disk = Arc::new(Disk::new(dir.0.to_str().unwrap(), 10, LogFormat::Text));
        disk.write(8, 30).await.unwrap();
        match disk.write(5, 0).await {
            Err(WriteError::OverBudget(over)) => assert_eq!(over, OverBudget { held: 8, limit: 10 }),
            other => panic!("{other:?}"),
        }
        disk.remove_files();
        assert!(dir.names().is_empty());
    }

    #[tokio::test]
    async fn a_directory_that_cannot_be_used_makes_every_write_fail_and_gives_the_budget_back() {
        let disk = Arc::new(Disk::new("/dev/null/stress", 10, LogFormat::Text));
        assert!(matches!(disk.write(1, 0).await, Err(WriteError::Write(_))));
        for _ in 0..100 {
            if disk.budget().held() == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the budget was not given back");
    }
}
