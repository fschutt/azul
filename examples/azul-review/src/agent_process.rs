//! Running a program for an agent: `claude -p` and `gemini` are programs.
//!
//! [`run`] starts a program with the prompt on its standard input (not on its command line: no
//! limit on the size, nothing for `ps` to show), reads its standard output and standard error into
//! buffers that are capped (a program that writes a gigabyte keeps the first megabytes and says so),
//! and waits for it on the clock it is given: the deadline ends it, the window's cancel ends it, and
//! "ends" means killed - the program and the group of programs it started, so no helper is left
//! behind. [`find_program`] looks for the program where a program started from the Finder (a thin
//! PATH) would not find it: on the PATH, then in the places such tools are installed.
//!
//! Nothing here reads the environment: the PATH and the home folder come in as [`Host`], the
//! environment the program gets is the app's own plus what the call adds.

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{sync_channel, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use crate::agent_backend::{AgentError, Control};

/// How often a running program is looked at, ms.
const POLL_MS: u64 = 15;

/// How long, in real time, what a finished program still wrote is waited for (a helper that
/// outlives it may hold the pipe open).
const TAIL: Duration = Duration::from_millis(1_500);

/// Chunks the readers may be ahead of the collector by (a program that writes faster waits).
const BACKLOG: usize = 64;

/// What to run.
#[derive(Debug, Clone)]
pub struct Spec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Set in the environment the program gets (it inherits the app's).
    pub env_set: Vec<(String, String)>,
    /// Taken out of it.
    pub env_remove: Vec<String>,
    /// Written to its standard input, which is then closed.
    pub stdin: Vec<u8>,
    pub timeout_ms: u64,
    pub max_stdout: usize,
    pub max_stderr: usize,
}

/// What a program that ran to its end left.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    /// The exit code (`None`: ended by a signal).
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// More was written than the caps keep.
    pub stdout_cut: bool,
    pub stderr_cut: bool,
    /// How long it ran, ms of the clock.
    pub elapsed_ms: u64,
}

impl Output {
    #[must_use]
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    #[must_use]
    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Why a program did not run to its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    NotFound,
    PermissionDenied,
    Spawn(String),
    /// Killed at the deadline.
    TimedOut,
    /// Killed because the window cancelled it.
    Cancelled,
}

impl Failure {
    /// The failure in the words of an [`AgentError`]; `timeout_ms` is the deadline that was set.
    #[must_use]
    pub fn into_error(self, program: &str, searched: Vec<String>, timeout_ms: u64) -> AgentError {
        match self {
            Self::NotFound => AgentError::NotInstalled {
                program: program.to_string(),
                searched,
            },
            Self::PermissionDenied => AgentError::Spawn {
                program: program.to_string(),
                why: "permission denied (is the file executable?)".to_string(),
            },
            Self::Spawn(why) => AgentError::Spawn {
                program: program.to_string(),
                why,
            },
            Self::TimedOut => AgentError::Timeout {
                secs: u32::try_from(timeout_ms.div_ceil(1_000)).unwrap_or(u32::MAX),
            },
            Self::Cancelled => AgentError::Cancelled,
        }
    }
}

/// A reader of one of the program's pipes: what it reads arrives in chunks.
fn reader(mut pipe: impl Read + Send + 'static) -> Receiver<Vec<u8>> {
    let (send, receive) = sync_channel(BACKLOG);
    thread::spawn(move || {
        let mut buffer = [0_u8; 8 * 1024];
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if send.send(buffer[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    receive
}

/// A capped buffer.
#[derive(Default)]
struct Capture {
    bytes: Vec<u8>,
    cap: usize,
    cut: bool,
}

impl Capture {
    fn add(&mut self, chunk: &[u8]) {
        let room = self.cap.saturating_sub(self.bytes.len());
        if chunk.len() > room {
            self.cut = true;
        }
        self.bytes
            .extend_from_slice(&chunk[..chunk.len().min(room)]);
    }

    /// Takes what has arrived; whether the pipe is closed.
    fn drain(&mut self, receive: &Receiver<Vec<u8>>) -> bool {
        loop {
            match receive.try_recv() {
                Ok(chunk) => self.add(&chunk),
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => return true,
            }
        }
    }
}

/// Kills the program and the group of programs it started.
fn kill(child: &mut Child) {
    #[cfg(unix)]
    {
        // the program leads a group of its own (see `run`): everything it started is in it
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Runs `spec`, waiting on `ctl`'s clock.
///
/// # Errors
/// [`Failure`]: not found, not allowed, not startable, past its deadline, cancelled.
pub fn run(spec: &Spec, ctl: &mut Control<'_>) -> Result<Output, Failure> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = &spec.cwd {
        command.current_dir(dir);
    }
    for (name, value) in &spec.env_set {
        command.env(name, value);
    }
    for name in &spec.env_remove {
        command.env_remove(name);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Failure::NotFound,
        std::io::ErrorKind::PermissionDenied => Failure::PermissionDenied,
        _ => Failure::Spawn(e.to_string()),
    })?;
    let started = ctl.now_ms();
    let deadline = started.saturating_add(spec.timeout_ms);

    // the prompt goes in on a thread of its own: a program that writes before it has read
    // everything would otherwise wait on us while we wait on it
    if let Some(mut stdin) = child.stdin.take() {
        let bytes = spec.stdin.clone();
        thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
    }
    let out_pipe = child.stdout.take().map(reader);
    let err_pipe = child.stderr.take().map(reader);
    let mut stdout = Capture {
        cap: spec.max_stdout,
        ..Capture::default()
    };
    let mut stderr = Capture {
        cap: spec.max_stderr,
        ..Capture::default()
    };

    let status = loop {
        if let Some(pipe) = &out_pipe {
            stdout.drain(pipe);
        }
        if let Some(pipe) = &err_pipe {
            stderr.drain(pipe);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                kill(&mut child);
                return Err(Failure::Spawn(e.to_string()));
            }
        }
        if ctl.cancelled() {
            kill(&mut child);
            return Err(Failure::Cancelled);
        }
        if ctl.now_ms() >= deadline {
            kill(&mut child);
            return Err(Failure::TimedOut);
        }
        ctl.sleep_ms(POLL_MS);
    };

    // what it wrote last, until the pipes close (or a helper that outlived it holds them)
    let tail_until = Instant::now() + TAIL;
    let (mut out_open, mut err_open) = (out_pipe.is_some(), err_pipe.is_some());
    while (out_open || err_open) && Instant::now() < tail_until {
        if let Some(pipe) = &out_pipe {
            out_open = !stdout.drain(pipe);
        }
        if let Some(pipe) = &err_pipe {
            err_open = !stderr.drain(pipe);
        }
        if out_open || err_open {
            thread::sleep(Duration::from_millis(2));
        }
    }
    Ok(Output {
        code: status.code(),
        stdout: stdout.bytes,
        stderr: stderr.bytes,
        stdout_cut: stdout.cut,
        stderr_cut: stderr.cut,
        elapsed_ms: ctl.now_ms().saturating_sub(started),
    })
}

// ==== Finding a program ====

/// Where programs are looked for besides the PATH: the places such tools install themselves. `~`
/// is the home folder.
pub static USUAL_PLACES: [&str; 6] = [
    "~/.local/bin",
    "~/.claude/local",
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "~/.npm-global/bin",
    "~/.bun/bin",
];

/// What the search needs to know about the machine, taken once at the start of the app.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Host {
    /// The folders of the PATH, in order.
    pub path: Vec<PathBuf>,
    pub home: Option<PathBuf>,
}

impl Host {
    /// From the text of the PATH variable and the home folder.
    #[must_use]
    pub fn new(path_var: Option<&str>, home: Option<&str>) -> Self {
        Self {
            path: path_var
                .map(|p| {
                    std::env::split_paths(p)
                        .filter(|d| !d.as_os_str().is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            home: home.filter(|h| !h.trim().is_empty()).map(PathBuf::from),
        }
    }

    /// Every folder a program is looked for in: the PATH, then the usual places.
    #[must_use]
    pub fn folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = self.path.clone();
        for place in &USUAL_PLACES {
            let folder = match place.strip_prefix("~/") {
                Some(rest) => match &self.home {
                    Some(home) => home.join(rest),
                    None => continue,
                },
                None => PathBuf::from(place),
            };
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
        folders
    }
}

/// Whether `path` is a file a program can be started from.
#[must_use]
pub fn is_program(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// The program to run: `configured` (a path, or a name to look for; empty: `default_name`), looked
/// for in `host`'s folders with `exists` (the real one is [`is_program`]).
///
/// # Errors
/// The places that were looked in, for the words "X is not installed (looked in ...)".
pub fn find_program(
    configured: &str,
    default_name: &str,
    host: &Host,
    exists: &dyn Fn(&Path) -> bool,
) -> Result<PathBuf, Vec<String>> {
    let configured = configured.trim();
    let name = if configured.is_empty() {
        default_name
    } else {
        configured
    };
    if name.contains('/') || name.contains('\\') {
        let path = PathBuf::from(name);
        return if exists(&path) {
            Ok(path)
        } else {
            Err(vec![path.display().to_string()])
        };
    }
    let folders = host.folders();
    for folder in &folders {
        for candidate in [
            name.to_string(),
            format!("{name}.exe"),
            format!("{name}.cmd"),
        ] {
            let path = folder.join(candidate);
            if exists(&path) {
                return Ok(path);
            }
        }
    }
    Err(folders.iter().map(|f| f.display().to_string()).collect())
}

#[cfg(test)]
#[path = "agent_process_test.rs"]
mod agent_process_test;
