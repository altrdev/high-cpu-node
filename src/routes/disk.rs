use crate::app::AppState;
use crate::budget::Budget;
use crate::endpoints::specs;
use crate::units::MB;
use crate::{errno, params};
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;

const PREFIX: &str = "torchio-disk-";

// The files /disk keeps, against the budget MAX_DISK_MB
pub struct Disk {
    pub budget: Arc<Budget>,
    dir: PathBuf,
    files: Mutex<HashSet<PathBuf>>,
    next_id: AtomicU64,
}

impl Disk {
    pub fn new(dir: &str, limit: i64) -> Disk {
        if let Err(e) = Disk::prepare(Path::new(dir)) {
            eprintln!("DISK_DIR {dir} is not usable, /disk will answer 507: {}", errno::name(&e));
        }
        Disk { budget: Budget::new(limit), dir: PathBuf::from(dir), files: Mutex::default(), next_id: AtomicU64::new(0) }
    }

    // Creates the directory and removes files left behind by a previous run that didn't shut down cleanly
    fn prepare(dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(PREFIX) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }

    // Used on shutdown, which can't wait for the timers
    pub fn remove_files(&self) {
        for file in self.files.lock().unwrap().drain() {
            let _ = std::fs::remove_file(file);
        }
    }

    fn forget(&self, file: &Path) {
        self.files.lock().unwrap().remove(file);
    }
}

async fn write_file(file: &Path, mb: i64) -> std::io::Result<()> {
    let mut handle = tokio::fs::File::create(file).await?;
    let chunk = vec![1u8; MB];
    for _ in 0..mb {
        handle.write_all(&chunk).await?;
    }
    handle.sync_all().await
}

// Writes `mb` megabytes to a file in DISK_DIR and keeps it for `hold` seconds. As for
// /memory, MAX_DISK_MB is the total that can be held at the same time.
pub async fn handler(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Response {
    let params = match params::read(query.as_deref(), &specs(&state.config).disk) {
        Ok(params) => params,
        Err(rejection) => return rejection.into_response(),
    };
    let (mb, hold) = (params.get("mb"), params.get("hold"));
    let disk = &state.disk;

    let Some(reservation) = disk.budget.reserve(mb) else {
        let message = format!("{} MB already held, limit is {} MB", disk.budget.held(), disk.budget.limit());
        return (StatusCode::TOO_MANY_REQUESTS, message).into_response();
    };

    let file = disk.dir.join(format!("{PREFIX}{}", disk.next_id.fetch_add(1, Ordering::Relaxed)));
    disk.files.lock().unwrap().insert(file.clone());

    // the file is written and removed even if the client hangs up in the meantime
    let (done, answer) = oneshot::channel();
    let task = state.clone();
    tokio::spawn(async move {
        let result = write_file(&file, mb).await;
        let failed = result.is_err();
        let _ = done.send(result);
        if !failed {
            tokio::time::sleep(Duration::from_secs(hold as u64)).await;
        }
        task.disk.forget(&file);
        let _ = tokio::fs::remove_file(&file).await;
        drop(reservation);
    });

    let dir = disk.dir.display();
    match answer.await {
        Ok(Ok(())) => (StatusCode::OK, format!("Wrote {mb} MB to {dir}, held for {hold} s")).into_response(),
        Ok(Err(e)) => (StatusCode::INSUFFICIENT_STORAGE, format!("Could not write {mb} MB to {dir}: {}", errno::name(&e))).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
