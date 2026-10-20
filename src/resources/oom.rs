//! Allocates memory without a budget until the kernel kills the process, to test what a container
//! does when it is killed for memory: `OOMKilled`, exit code 137, restart policies. Only with `ALLOW_OOM`.
use crate::units::MB;
use std::time::Duration;

const CHUNK: usize = 64 * MB as usize;

/// Fills memory in a thread of its own, a chunk at a time, writing every byte so the kernel has to
/// back it with real pages. It never returns: the process ends when the kernel says so.
pub fn start() {
    std::thread::spawn(|| {
        // a moment to let the answer leave
        std::thread::sleep(Duration::from_millis(50));
        let mut held: Vec<Vec<u8>> = Vec::new();
        loop {
            held.push(vec![0xA5; CHUNK]);
            std::thread::sleep(Duration::from_millis(5));
        }
    });
}
