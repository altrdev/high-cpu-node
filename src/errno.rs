use std::io;

// The name of an error the way Node prints it (EMFILE, ENOTDIR), for the codes that matter here
pub fn name(error: &io::Error) -> String {
    match error.raw_os_error() {
        Some(2) => "ENOENT".into(),
        Some(12) => "ENOMEM".into(),
        Some(13) => "EACCES".into(),
        Some(20) => "ENOTDIR".into(),
        Some(21) => "EISDIR".into(),
        Some(23) => "ENFILE".into(),
        Some(24) => "EMFILE".into(),
        Some(28) => "ENOSPC".into(),
        Some(30) => "EROFS".into(),
        _ => error.to_string(),
    }
}
