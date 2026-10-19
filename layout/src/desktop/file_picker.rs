//! The file picker a [`FileDialog`](super::dialogs::FileDialog) request
//! prefers: a picker another program registered, else the system's dialog.
//!
//! AzDrive registers itself on every start in
//! `<config dir>/azul-storage/file-picker.json`. Its picker knows the user's
//! cloud drives, pins and recent places, which the system's dialog does not.
//! The provider is a program started once per request with the arguments of
//! its registration: it reads one JSON line (the request) on stdin and
//! answers with exactly one line starting `AZPICK ` on stdout, among any
//! other lines it prints, then exits (protocol 1). It runs on a worker
//! thread, so the UI thread never waits for it, and the request resumes when
//! the answer is there. A provider that fails - it does not start, exits
//! without an answer, or answers in another protocol - hands the same request
//! to the system's dialog. A cancel is final.
//!
//! Which picker answers: `AZUL_FILE_PICKER=native` never asks a provider;
//! `AZUL_FILE_PICKER=<registration file>` asks that one, also in an e2e run
//! (a provider under test); otherwise the registration in the config folder,
//! except in an e2e run, which never starts a real picker of either kind.

use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
};

/// The request and answer format this engine speaks.
pub const PROTOCOL: u64 = 1;

/// What starts the answer line on the provider's stdout.
pub const ANSWER_PREFIX: &str = "AZPICK ";

/// The environment variable that picks the picker: `native`, or the path of
/// a registration file.
pub const PICKER_VAR: &str = "AZUL_FILE_PICKER";

/// What a request asks the picker for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickOp {
    /// One file to read.
    Open,
    /// One or more files to read.
    OpenMultiple,
    /// A folder.
    OpenDirectory,
    /// Where to write a file.
    Save,
}

impl PickOp {
    /// The name of this request in a registration's `modes`.
    #[must_use]
    pub const fn mode(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::OpenMultiple => "open-multiple",
            Self::OpenDirectory => "open-directory",
            Self::Save => "save",
        }
    }

    /// The request line's `op` and `multiple`.
    const fn wire(self) -> (&'static str, bool) {
        match self {
            Self::Open => ("open", false),
            Self::OpenMultiple => ("open", true),
            Self::OpenDirectory => ("open-directory", false),
            Self::Save => ("save", false),
        }
    }
}

/// One request, as the provider is asked it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickRequest {
    pub op: PickOp,
    /// The window's title ("Insert picture").
    pub title: String,
    /// The folder (or, for Save, the file name) the request starts at.
    pub default_location: Option<String>,
    /// The type filter: its name and patterns (`*.png`); empty for none.
    pub filters: Vec<(String, Vec<String>)>,
}

impl PickRequest {
    /// The JSON line the provider reads on stdin.
    #[must_use]
    pub fn line(&self) -> String {
        let (op, multiple) = self.op.wire();
        let filters: Vec<serde_json::Value> = self
            .filters
            .iter()
            .map(|(name, patterns)| serde_json::json!({ "name": name, "patterns": patterns }))
            .collect();
        let mut request = serde_json::json!({
            "v": PROTOCOL,
            "op": op,
            "multiple": multiple,
            "title": self.title,
            "filters": filters,
            "app": app_name(),
        });
        if let Some(location) = &self.default_location {
            request["default_location"] = serde_json::Value::from(location.as_str());
        }
        request.to_string()
    }
}

/// The asking program's name, for the provider's log.
fn app_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

/// How a request ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickAnswer {
    /// The user picked these paths (one for Open, OpenDirectory and Save).
    Selected(Vec<String>),
    /// The user cancelled: final, no second dialog.
    Cancelled,
    /// The provider could not serve the request (its message). The
    /// path-only requests have no way to carry it: the system's dialog is
    /// asked instead.
    Error(String),
    /// No usable answer (why): the provider did not start, ended without an
    /// `AZPICK` line, or answered in another protocol. The system's dialog
    /// is asked instead.
    Unusable(String),
}

impl PickAnswer {
    /// Whether the system's dialog has to answer this request after all.
    #[must_use]
    pub const fn falls_back(&self) -> bool {
        matches!(self, Self::Error(_) | Self::Unusable(_))
    }
}

/// Reads the answer line's JSON (after [`ANSWER_PREFIX`]).
#[must_use]
pub fn parse_answer(json: &str) -> PickAnswer {
    let Ok(answer) = serde_json::from_str::<serde_json::Value>(json.trim()) else {
        return PickAnswer::Unusable(format!("the answer is not JSON: {json}"));
    };
    if answer.get("v").and_then(serde_json::Value::as_u64) != Some(PROTOCOL) {
        return PickAnswer::Unusable(format!("the answer is not protocol {PROTOCOL}: {json}"));
    }
    match answer.get("status").and_then(serde_json::Value::as_str) {
        Some("selected") => {
            let paths: Vec<String> = answer
                .get("items")
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("path").and_then(serde_json::Value::as_str))
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            if paths.is_empty() {
                // A cloud handle without a copy has no path: nothing the
                // path-only requests can hand the app.
                PickAnswer::Unusable(format!("the answer names no path: {json}"))
            } else {
                PickAnswer::Selected(paths)
            }
        }
        Some("cancelled") => PickAnswer::Cancelled,
        Some("error") => PickAnswer::Error(
            answer
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("the picker failed")
                .to_string(),
        ),
        _ => PickAnswer::Unusable(format!("the answer has no known status: {json}")),
    }
}

/// A provider's registration (`file-picker.json`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    /// Who answers ("AzDrive"), for the log.
    pub provider: String,
    /// The program, an absolute path.
    pub exec: PathBuf,
    /// Its arguments (`--pick`).
    pub args: Vec<String>,
    /// The requests it answers ([`PickOp::mode`] names).
    pub modes: Vec<String>,
}

impl Registration {
    /// Reads a registration; `Err` says why it is not one this engine uses.
    ///
    /// # Errors
    /// Not JSON, another format, another protocol, or no `exec`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        if value.get("format").and_then(serde_json::Value::as_str)
            != Some("azul-storage.file-picker")
        {
            return Err("not an azul-storage.file-picker registration".to_string());
        }
        let protocol = value.get("protocol").and_then(serde_json::Value::as_u64);
        if protocol != Some(PROTOCOL) {
            return Err(format!(
                "protocol {} is not spoken here (this engine speaks {PROTOCOL})",
                protocol.map_or_else(|| "(none)".to_string(), |p| p.to_string())
            ));
        }
        let strings = |key: &str| -> Vec<String> {
            value
                .get(key)
                .and_then(serde_json::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default()
        };
        let exec = value
            .get("exec")
            .and_then(serde_json::Value::as_str)
            .filter(|e| !e.is_empty())
            .ok_or("no exec")?;
        Ok(Self {
            provider: value
                .get("provider")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("the registered picker")
                .to_string(),
            exec: PathBuf::from(exec),
            args: strings("args"),
            modes: strings("modes"),
        })
    }

    /// Reads and checks the registration at `path` for `op`.
    ///
    /// # Errors
    /// Why it cannot be used: unreadable, not a registration, not for `op`,
    /// or a program that is not safe to start ([`check_exec`]).
    pub fn load(path: &Path, op: PickOp) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("{} cannot be read: {e}", path.display()))?;
        let registration = Self::parse(&text)?;
        if !registration.modes.iter().any(|m| m == op.mode()) {
            return Err(format!(
                "{} does not answer {} requests",
                registration.provider,
                op.mode()
            ));
        }
        check_exec(&registration.exec)?;
        Ok(registration)
    }
}

/// Whether `exec` may be started on the strength of a file in the user's
/// config folder: an absolute path to an existing regular file that only
/// its owner can change, and on Unix one owned by the user or by root.
/// Anything else could let another account or a careless install point
/// this user's apps at a program of its choosing.
///
/// # Errors
/// Why the program is refused.
pub fn check_exec(exec: &Path) -> Result<(), String> {
    if !exec.is_absolute() {
        return Err(format!("{} is not an absolute path", exec.display()));
    }
    let meta = std::fs::metadata(exec).map_err(|e| format!("{}: {e}", exec.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a file", exec.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.mode() & 0o022 != 0 {
            return Err(format!(
                "{} can be changed by others than its owner",
                exec.display()
            ));
        }
        // SAFETY: getuid has no preconditions and cannot fail.
        let me = unsafe { libc::getuid() };
        if meta.uid() != me && meta.uid() != 0 {
            return Err(format!(
                "{} belongs to another user",
                exec.display()
            ));
        }
    }
    Ok(())
}

/// Which picker answers a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picker {
    /// The system's dialog (or, in an e2e run, the mock store).
    Native,
    /// The provider registered in this file.
    Provider(PathBuf),
}

/// The decision of the module intro, from its inputs: `env` is
/// [`PICKER_VAR`], `e2e` whether this is an e2e run (the mock store is
/// armed), `registered` the registration file in the config folder.
#[must_use]
pub fn choose(env: Option<&str>, e2e: bool, registered: Option<PathBuf>) -> Picker {
    match env.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) if v.eq_ignore_ascii_case("native") => Picker::Native,
        Some(file) => Picker::Provider(PathBuf::from(file)),
        None if e2e => Picker::Native,
        None => registered.map_or(Picker::Native, Picker::Provider),
    }
}

/// The registration AzDrive writes: `<config dir>/azul-storage/file-picker.json`.
#[must_use]
pub fn registered_file() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("azul-storage").join("file-picker.json"))
}

/// A request a provider is answering: polled once per frame by the request
/// queue. Dropping it (the request is gone) ends the provider.
pub struct PendingPick {
    answer: Mutex<mpsc::Receiver<PickAnswer>>,
    child: Arc<Mutex<Option<Child>>>,
}

impl PendingPick {
    /// The answer once the provider has given it (or failed).
    #[must_use]
    pub fn poll(&self) -> Option<PickAnswer> {
        let rx = self
            .answer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match rx.try_recv() {
            Ok(answer) => Some(answer),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(PickAnswer::Unusable(
                "the picker's worker ended without an answer".to_string(),
            )),
        }
    }
}

impl Drop for PendingPick {
    /// The request is gone (answered, or dropped with its window). A provider
    /// that has not answered yet is ended; one that has answered is the
    /// worker's to wait for, so it can close on its own.
    fn drop(&mut self) {
        let child = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Asks the provider registered in `registration` for `request`, on a
/// worker thread.
#[must_use]
pub fn ask(registration: &Registration, request: &PickRequest) -> PendingPick {
    let (tx, rx) = mpsc::channel();
    let child_slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let started = Command::new(&registration.exec)
        .args(&registration.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
    match started {
        Err(e) => {
            let _ = tx.send(PickAnswer::Unusable(format!(
                "{} did not start: {e}",
                registration.exec.display()
            )));
        }
        Ok(mut child) => {
            let stdin = child.stdin.take();
            let stdout = child.stdout.take();
            *child_slot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(child);
            let line = request.line();
            let slot = Arc::clone(&child_slot);
            let provider = registration.provider.clone();
            let spawned = std::thread::Builder::new()
                .name("azul-file-picker".to_string())
                .spawn(move || {
                    let answer = converse(stdin, stdout, &line, &provider);
                    // Answered (or ended): the provider is this thread's now.
                    // It closes its window and exits on its own; waiting for
                    // it here leaves no zombie and never holds the UI thread.
                    let child = slot
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take();
                    let _ = tx.send(answer);
                    if let Some(mut child) = child {
                        let _ = child.wait();
                    }
                });
            if let Err(e) = spawned {
                return PendingPick {
                    answer: Mutex::new(answered(PickAnswer::Unusable(format!(
                        "no worker thread for the picker: {e}"
                    )))),
                    child: child_slot,
                };
            }
        }
    }
    PendingPick {
        answer: Mutex::new(rx),
        child: child_slot,
    }
}

/// A receiver that already holds `answer`.
fn answered(answer: PickAnswer) -> mpsc::Receiver<PickAnswer> {
    let (tx, rx) = mpsc::channel();
    let _ = tx.send(answer);
    rx
}

/// Writes the request line and reads stdout until the answer line (or the
/// end of the provider's output).
fn converse(
    stdin: Option<std::process::ChildStdin>,
    stdout: Option<std::process::ChildStdout>,
    line: &str,
    provider: &str,
) -> PickAnswer {
    if let Some(mut stdin) = stdin {
        // A provider that does not read its stdin still answers; a broken
        // pipe here is not a failure of the request.
        let _ = writeln!(stdin, "{line}");
        // Dropped here: the provider sees the end of its input.
    }
    let Some(stdout) = stdout else {
        return PickAnswer::Unusable(format!("{provider}: no stdout"));
    };
    for read in BufReader::new(stdout).lines() {
        let Ok(text) = read else {
            break;
        };
        if let Some(json) = text.strip_prefix(ANSWER_PREFIX) {
            return parse_answer(json);
        }
    }
    PickAnswer::Unusable(format!("{provider} ended without an answer"))
}

/// Asks the preferred picker for `request`: `None` when the system's dialog
/// answers it (the decision of the module intro, or a registration that
/// cannot be used - logged once, with the reason).
#[must_use]
pub fn ask_preferred(request: &PickRequest) -> Option<PendingPick> {
    let env = std::env::var(PICKER_VAR).ok();
    // A test names its provider for its own thread, as `AZUL_FILE_PICKER`
    // names one for a process: the environment is shared by every test.
    #[cfg(test)]
    let env = tests::provider_for_this_thread().or(env);
    let e2e = crate::request::mock::is_armed();
    let Picker::Provider(file) = choose(env.as_deref(), e2e, registered_file().filter(|f| f.exists()))
    else {
        return None;
    };
    match Registration::load(&file, request.op) {
        Ok(registration) => Some(ask(&registration, request)),
        Err(why) => {
            log_once(&format!(
                "[azul][file-picker] the system's dialog answers: {why}"
            ));
            None
        }
    }
}

/// Prints `message` on stderr the first time it is seen.
fn log_once(message: &str) {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut seen = SEEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !seen.iter().any(|m| m == message) {
        eprintln!("{message}");
        seen.push(message.to_string());
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{cell::RefCell, path::PathBuf};

    use super::*;

    std::thread_local! {
        static PROVIDER: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    /// The registration file a test named for its thread.
    pub(crate) fn provider_for_this_thread() -> Option<String> {
        PROVIDER.with(|p| p.borrow().clone())
    }

    /// Runs `f` with `registration` as this thread's picker.
    pub(crate) fn with_provider<R>(registration: &Path, f: impl FnOnce() -> R) -> R {
        PROVIDER.with(|p| *p.borrow_mut() = Some(registration.to_string_lossy().into_owned()));
        let out = f();
        PROVIDER.with(|p| *p.borrow_mut() = None);
        out
    }

    /// A folder of its own for one test, under the system's temp folder.
    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "azul-file-picker-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        dir
    }

    /// A provider: a shell script with `body`, mode 0755, and its
    /// registration beside it. Returns the registration's path.
    #[cfg(unix)]
    pub(crate) fn fake_provider(dir: &Path, body: &str, protocol: u64) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let exec = dir.join("picker.sh");
        std::fs::write(&exec, format!("#!/bin/sh\n{body}\n")).expect("the script");
        std::fs::set_permissions(&exec, std::fs::Permissions::from_mode(0o755))
            .expect("its mode");
        let registration = dir.join("file-picker.json");
        std::fs::write(
            &registration,
            serde_json::json!({
                "format": "azul-storage.file-picker",
                "version": 1,
                "provider": "FakePicker",
                "exec": exec.to_string_lossy(),
                "args": ["--pick"],
                "protocol": protocol,
                "modes": ["open", "open-multiple", "open-directory", "save"],
                "cloud": true,
            })
            .to_string(),
        )
        .expect("the registration");
        registration
    }

    /// Polls `pending` until it answers (a provider is a real process).
    pub(crate) fn wait(pending: &PendingPick) -> PickAnswer {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            if let Some(answer) = pending.poll() {
                return answer;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the provider did not answer in 20 s"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn open_request() -> PickRequest {
        PickRequest {
            op: PickOp::Open,
            title: "Insert picture".to_string(),
            default_location: Some("/tmp".to_string()),
            filters: vec![("Pictures".to_string(), vec!["*.png".to_string()])],
        }
    }

    #[test]
    fn native_in_the_environment_never_asks_a_provider() {
        let registered = Some(PathBuf::from("/c/azul-storage/file-picker.json"));
        assert_eq!(choose(Some("native"), false, registered.clone()), Picker::Native);
        assert_eq!(choose(Some("NATIVE"), true, registered), Picker::Native);
    }

    #[test]
    fn an_e2e_run_asks_only_the_provider_the_environment_names() {
        let registered = Some(PathBuf::from("/c/azul-storage/file-picker.json"));
        assert_eq!(choose(None, true, registered.clone()), Picker::Native);
        assert_eq!(
            choose(Some("/t/fake.json"), true, registered.clone()),
            Picker::Provider(PathBuf::from("/t/fake.json"))
        );
        assert_eq!(
            choose(None, false, registered.clone()),
            Picker::Provider(PathBuf::from("/c/azul-storage/file-picker.json"))
        );
        assert_eq!(choose(Some("  "), false, None), Picker::Native);
    }

    #[test]
    fn a_registration_of_another_protocol_or_format_is_not_used() {
        let ok = r#"{"format":"azul-storage.file-picker","protocol":1,"exec":"/x","modes":["open"]}"#;
        assert!(Registration::parse(ok).is_ok());
        let two = r#"{"format":"azul-storage.file-picker","protocol":2,"exec":"/x","modes":["open"]}"#;
        assert!(Registration::parse(two).unwrap_err().contains("protocol 2"));
        let other = r#"{"format":"something-else","protocol":1,"exec":"/x"}"#;
        assert!(Registration::parse(other).is_err());
        let no_exec = r#"{"format":"azul-storage.file-picker","protocol":1}"#;
        assert!(Registration::parse(no_exec).is_err());
        assert!(Registration::parse("not json").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_registration_whose_program_is_missing_relative_or_group_writable_is_not_used() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("exec-checks");
        let registration = fake_provider(&dir, "exit 0", 1);
        assert!(Registration::load(&registration, PickOp::Open).is_ok());

        let exec = dir.join("picker.sh");
        std::fs::set_permissions(&exec, std::fs::Permissions::from_mode(0o775)).expect("mode");
        let why = Registration::load(&registration, PickOp::Open).unwrap_err();
        assert!(why.contains("others than its owner"), "{why}");

        std::fs::remove_file(&exec).expect("remove");
        assert!(Registration::load(&registration, PickOp::Open).is_err());
        assert!(check_exec(Path::new("picker.sh")).unwrap_err().contains("absolute"));
        assert!(check_exec(&dir).unwrap_err().contains("not a file"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_registration_answers_only_the_requests_it_lists() {
        let dir = scratch("modes");
        let registration = fake_provider(&dir, "exit 0", 1);
        let text = std::fs::read_to_string(&registration)
            .expect("read")
            .replace(r#""open-directory","save""#, "");
        std::fs::write(&registration, text.replace(",]", "]")).expect("write");
        assert!(Registration::load(&registration, PickOp::Open).is_ok());
        assert!(Registration::load(&registration, PickOp::Save)
            .unwrap_err()
            .contains("save"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_provider_reads_the_request_and_its_answer_line_is_found_among_its_other_lines() {
        let dir = scratch("answer");
        let got = dir.join("request.json");
        let registration = fake_provider(
            &dir,
            &format!(
                "read line; printf '%s' \"$line\" > '{}'; echo AZDRIVE_READY; echo 'AZPICK \
                 {{\"v\":1,\"status\":\"selected\",\"items\":[{{\"path\":\"/p/a.png\"}}]}}'",
                got.display()
            ),
            1,
        );
        let provider = Registration::load(&registration, PickOp::Open).expect("usable");
        let answer = wait(&ask(&provider, &open_request()));
        assert_eq!(answer, PickAnswer::Selected(vec!["/p/a.png".to_string()]));
        let request: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&got).expect("the request")).expect("json");
        assert_eq!(request["v"], 1);
        assert_eq!(request["op"], "open");
        assert_eq!(request["multiple"], false);
        assert_eq!(request["title"], "Insert picture");
        assert_eq!(request["default_location"], "/tmp");
        assert_eq!(request["filters"][0]["patterns"][0], "*.png");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_provider_that_exits_without_an_answer_hands_the_request_to_the_system_dialog() {
        let dir = scratch("no-answer");
        let registration = fake_provider(&dir, "echo starting; exit 1", 1);
        let provider = Registration::load(&registration, PickOp::Open).expect("usable");
        let answer = wait(&ask(&provider, &open_request()));
        assert!(
            matches!(answer, PickAnswer::Unusable(_)) && answer.falls_back(),
            "{answer:?}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn answers_in_another_protocol_or_without_a_path_fall_back_and_a_cancel_is_final() {
        assert_eq!(
            parse_answer(r#"{"v":1,"status":"cancelled"}"#),
            PickAnswer::Cancelled
        );
        assert!(!PickAnswer::Cancelled.falls_back());
        assert!(parse_answer(r#"{"v":2,"status":"selected","items":[{"path":"/a"}]}"#).falls_back());
        assert!(parse_answer(r#"{"v":1,"status":"selected","items":[{"cloud":{}}]}"#).falls_back());
        assert_eq!(
            parse_answer(r#"{"v":1,"status":"error","message":"no keys"}"#),
            PickAnswer::Error("no keys".to_string())
        );
        assert!(parse_answer("garbage").falls_back());
    }
}
