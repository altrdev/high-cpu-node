//! What the server holds or does on purpose, apart from answering requests: budgets, memory,
//! files, descriptors, worker threads, the readiness switch and the lines of `/log`. None of it
//! knows about HTTP; the routes use it.
pub mod block;
pub mod budget;
pub mod connections;
pub mod delay;
pub mod disk;
pub mod fd;
pub mod flaky;
pub mod log;
pub mod memory;
pub mod oom;
pub mod ready;
pub mod shutdown;
pub mod workers;
