//! What the integration tests share: a real `torchio` started on a free port with small limits and
//! its own `DISK_DIR`, and a small HTTP client to talk to it.
#![allow(dead_code, unused_imports)]

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub use torchio::units::MB;
pub const BIN: &str = env!("CARGO_BIN_EXE_torchio");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn sleep(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

// Polls `check` until it is true, and fails the test when it takes longer than `timeout_ms`
pub fn wait_for(what: &str, timeout_ms: u64, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < deadline {
        if check() {
            return;
        }
        sleep(50);
    }
    panic!("timed out waiting for {what}");
}

// A directory that is removed with the value
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!("torchio-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub struct Response {
    pub status: u16,
    headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.body))
    }
}

fn client() -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(10))).build().into()
}

// GET `url` with extra headers; an Err is a connection that failed
pub fn try_get(url: &str, headers: &[(&str, &str)]) -> Result<Response, String> {
    let mut request = client().get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request.call().map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let headers = response.headers().iter().map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect();
    let body = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    Ok(Response { status, headers, body })
}

// A request written by hand, for what an HTTP client won't send, and the whole answer
pub fn raw(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    answer
}

// What to do with DISK_DIR before the server starts
type Prepare = Box<dyn Fn(&Path)>;

// What a server is started with: the environment, and what to put in DISK_DIR first
#[derive(Default)]
pub struct Options {
    env: Vec<(String, String)>,
    prepare: Option<Prepare>,
}

impl Options {
    pub fn env(mut self, name: &str, value: &str) -> Options {
        self.env.push((name.to_string(), value.to_string()));
        self
    }

    pub fn prepare(mut self, prepare: impl Fn(&Path) + 'static) -> Options {
        self.prepare = Some(Box::new(prepare));
        self
    }

    pub fn start(self) -> Server {
        Server::spawn(self)
    }
}

pub struct Server {
    child: Child,
    pub port: u16,
    pub disk_dir: TempDir,
    output: Arc<Mutex<String>>,
    readers: Vec<thread::JoinHandle<()>>,
}

impl Server {
    pub fn start() -> Server {
        Options::default().start()
    }

    pub fn with(env: &[(&str, &str)]) -> Server {
        env.iter().fold(Options::default(), |options, (name, value)| options.env(name, value)).start()
    }

    // The port is free when it is picked, but another test can take it before the server binds it, so a server that
    // finds it taken is started again on another one
    fn spawn(options: Options) -> Server {
        for _ in 0..4 {
            if let Some(server) = Server::try_spawn(&options) {
                return server;
            }
        }
        Server::try_spawn(&options).expect("a free port to start the server on")
    }

    // None when the port was taken
    fn try_spawn(options: &Options) -> Option<Server> {
        let port = free_port();
        let disk_dir = TempDir::new();
        if let Some(prepare) = &options.prepare {
            prepare(disk_dir.path());
        }

        let mut command = Command::new(BIN);
        command.env_clear().env("PATH", std::env::var_os("PATH").unwrap_or_default());
        let defaults = [
            ("PORT", port.to_string()),
            ("WORKERS", "2".into()),
            ("MAX_MEMORY_MB", "300".into()),
            ("MAX_DISK_MB", "300".into()),
            ("MAX_HOLD_SECONDS", "30".into()),
            ("MAX_DELAY_MS", "5000".into()),
            ("DISK_DIR", disk_dir.path().to_string_lossy().into_owned()),
        ];
        for (name, value) in defaults {
            command.env(name, value);
        }
        for (name, value) in &options.env {
            command.env(name, value);
        }
        let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("the binary starts");

        // everything the server writes, to look at in the test
        let output = Arc::new(Mutex::new(String::new()));
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut readers = Vec::new();
        for stream in [Box::new(stdout) as Box<dyn Read + Send>, Box::new(stderr)] {
            let output = Arc::clone(&output);
            readers.push(thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let mut output = output.lock().unwrap();
                    output.push_str(&line);
                    output.push('\n');
                }
            }));
        }

        let mut server = Server { child, port, disk_dir, output, readers };
        for _ in 0..100 {
            if let Some(status) = server.child.try_wait().unwrap() {
                // the reader threads may not have the last line yet
                for reader in server.readers.drain(..) {
                    let _ = reader.join();
                }
                if server.output().contains("Address already in use") {
                    return None;
                }
                panic!("the server exited while starting ({status}): {}", server.output());
            }
            if try_get(&format!("{}/", server.url()), &[]).is_ok_and(|r| r.status == 200) {
                return Some(server);
            }
            sleep(100);
        }
        panic!("the server did not start: {}", server.output());
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn get(&self, route: &str) -> Response {
        self.get_with(route, &[])
    }

    pub fn get_with(&self, route: &str, headers: &[(&str, &str)]) -> Response {
        try_get(&format!("{}{route}", self.url()), headers).unwrap_or_else(|e| panic!("GET {route} failed: {e}"))
    }

    pub fn status(&self, route: &str) -> u16 {
        self.get(route).status
    }

    pub fn text(&self, route: &str) -> String {
        self.get(route).body
    }

    pub fn json(&self, route: &str) -> Value {
        self.get(route).json()
    }

    pub fn info(&self) -> Value {
        self.json("/info")
    }

    // What the server has written to stdout and stderr so far
    pub fn output(&self) -> String {
        self.output.lock().unwrap().clone()
    }

    // The files /disk keeps in DISK_DIR
    pub fn disk_files(&self) -> Vec<String> {
        let mut files: Vec<String> = std::fs::read_dir(self.disk_dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| name.starts_with("torchio-disk-"))
            .collect();
        files.sort();
        files
    }

    pub fn exited(&mut self) -> Option<ExitStatus> {
        let status = self.child.try_wait().unwrap()?;
        // the last lines are still in the pipes: wait for the readers to take them, so `output()` is complete
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
        Some(status)
    }

    // Waits for the process to end by itself
    pub fn wait_exit(&mut self, timeout_ms: u64) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if let Some(status) = self.exited() {
                return status;
            }
            assert!(Instant::now() < deadline, "the server did not exit in {timeout_ms} ms");
            sleep(20);
        }
    }

    pub fn signal(&self, signal: libc::c_int) {
        // SAFETY: kill has no preconditions; the pid is that of our own child
        unsafe { libc::kill(self.child.id() as libc::pid_t, signal) };
    }

    // Sends SIGTERM and waits for the process to end
    pub fn terminate(&mut self) -> ExitStatus {
        self.signal(libc::SIGTERM);
        self.wait_exit(5000)
    }

    // Resident memory of the process in kB, as `ps` sees it
    pub fn rss_kb(&self) -> u64 {
        ps(self.pid(), "rss").trim().parse().expect("a number of kB")
    }

    // CPU time the process has used, in seconds
    #[cfg(target_os = "linux")]
    pub fn cpu_seconds(&self) -> f64 {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", self.pid())).unwrap();
        // utime and stime are the 14th and 15th fields, counted after the name of the process
        let fields: Vec<&str> = stat[stat.rfind(')').unwrap() + 2..].split(' ').collect();
        // SAFETY: sysconf has no preconditions
        let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
        (fields[11].parse::<f64>().unwrap() + fields[12].parse::<f64>().unwrap()) / ticks
    }

    #[cfg(not(target_os = "linux"))]
    pub fn cpu_seconds(&self) -> f64 {
        ps(self.pid(), "time").trim().split(':').fold(0.0, |total, part| total * 60.0 + part.parse::<f64>().unwrap_or(0.0))
    }
}

fn ps(pid: u32, column: &str) -> String {
    let output = Command::new("ps").args(["-o", &format!("{column}="), "-p", &pid.to_string()]).output().expect("ps runs");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

// The value of a series in the text of /metrics, e.g. `torchio_ready` or `torchio_http_requests_total{route="/",code="200"}`
pub fn metric(text: &str, series: &str) -> Option<f64> {
    text.lines().find_map(|line| line.strip_prefix(series)?.strip_prefix(' ')?.parse().ok())
}

// The lines /log wrote
pub fn logged(server: &Server) -> Vec<String> {
    server.output().lines().filter(|line| line.contains("torchio-log line")).map(String::from).collect()
}

// Runs `torchio <args>` to the end, with the given environment on top of an empty one
pub fn run(args: &[&str], env: &[(&str, &str)]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .envs(env.iter().copied())
        .output()
        .expect("the binary runs")
}
