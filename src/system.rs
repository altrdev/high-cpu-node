//! What the operating system says about the process and the machine. Linux is what runs in
//! containers; the other platforms answer as much as is useful during development.
use std::ffi::CString;
use std::io;

/// The name of an error as the tools of the system print it (`EMFILE`, `ENOTDIR`), for the codes
/// that matter here. Any other error is described in words.
pub fn errno_name(error: &io::Error) -> String {
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

#[cfg(target_os = "linux")]
fn page_size() -> u64 {
    // SAFETY: sysconf has no preconditions
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(1) as u64
}

/// Resident memory of this process, in bytes
#[cfg(target_os = "linux")]
pub fn rss_bytes() -> u64 {
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    statm.split_whitespace().nth(1).and_then(|pages| pages.parse::<u64>().ok()).unwrap_or(0) * page_size()
}

#[cfg(target_os = "macos")]
pub fn rss_bytes() -> u64 {
    let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
    // SAFETY: proc_taskinfo is plain data, and proc_pidinfo writes at most `size` bytes into it
    unsafe {
        let mut info: libc::proc_taskinfo = std::mem::zeroed();
        let written = libc::proc_pidinfo(libc::getpid(), libc::PROC_PIDTASKINFO, 0, (&mut info as *mut libc::proc_taskinfo).cast(), size);
        if written == size { info.pti_resident_size } else { 0 }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn rss_bytes() -> u64 {
    0
}

// A cgroup limit file: a number, or "max" (v2) when there is none
#[cfg(target_os = "linux")]
fn parse_limit(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

#[cfg(target_os = "linux")]
fn cgroup_limit() -> Option<u64> {
    let cgroups = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    let read = |path: String| std::fs::read_to_string(path).ok();
    // cgroup v2: the line "0::/path"
    let v2_path = cgroups.lines().find_map(|line| line.strip_prefix("0::")).unwrap_or("/");
    let v2 =
        read(format!("/sys/fs/cgroup{}/memory.max", v2_path.trim_end_matches('/'))).or_else(|| read("/sys/fs/cgroup/memory.max".into()));
    if let Some(text) = v2 {
        return parse_limit(&text);
    }
    // cgroup v1: the line "N:memory:/path"
    let v1_path = cgroups
        .lines()
        .find_map(|line| line.split(':').nth(1).filter(|c| c.split(',').any(|c| c == "memory")).and(line.splitn(3, ':').nth(2)))
        .unwrap_or("/");
    let v1 = read(format!("/sys/fs/cgroup/memory{}/memory.limit_in_bytes", v1_path.trim_end_matches('/')))
        .or_else(|| read("/sys/fs/cgroup/memory/memory.limit_in_bytes".into()));
    v1.and_then(|text| parse_limit(&text))
}

/// The memory limit of the container, when there is one lower than the memory of the machine
#[cfg(target_os = "linux")]
pub fn container_limit_bytes() -> Option<u64> {
    // SAFETY: sysconf has no preconditions
    let total = unsafe { libc::sysconf(libc::_SC_PHYS_PAGES) }.max(0) as u64 * page_size();
    cgroup_limit().filter(|limit| *limit > 0 && *limit < total)
}

#[cfg(not(target_os = "linux"))]
pub fn container_limit_bytes() -> Option<u64> {
    None
}

/// Free space for an unprivileged process in the file system of `dir`, `None` when it can't be read
pub fn free_bytes(dir: &str) -> Option<u64> {
    let path = CString::new(dir).ok()?;
    // SAFETY: `path` is a valid C string and `stats` is plain data that statfs fills in
    unsafe {
        let mut stats: libc::statfs = std::mem::zeroed();
        (libc::statfs(path.as_ptr(), &mut stats) == 0).then(|| stats.f_bavail as u64 * stats.f_bsize as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn limits() {
        assert_eq!(parse_limit("268435456\n"), Some(268435456));
        assert_eq!(parse_limit("max\n"), None);
    }

    #[test]
    fn errors_are_named_when_it_matters() {
        assert_eq!(errno_name(&io::Error::from_raw_os_error(24)), "EMFILE");
        assert_eq!(errno_name(&io::Error::from_raw_os_error(20)), "ENOTDIR");
        assert!(errno_name(&io::Error::other("something else")).contains("something else"));
    }

    #[test]
    fn this_process_has_memory_and_the_temp_dir_has_space() {
        assert!(rss_bytes() > 0);
        assert!(free_bytes(&std::env::temp_dir().to_string_lossy()).unwrap() > 0);
        assert_eq!(free_bytes("/dev/null/stress"), None);
    }
}
