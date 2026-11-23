//! The real process environment reaches the settings of an app, once, through the one reader.
//!
//! Everything the engine asks of the environment is a plain value built at the start of an app
//! by `AppSettings::from_lookup(&ProcessEnv::new())`; nothing downstream asks the process again.
//! The unit tests prove each switch with a `MapEnv`; this proves the seam itself with REAL
//! variables of a REAL process: the test binary runs itself as a child with the variables set (or
//! with none), the child builds the settings the way `App::create` does and prints them, and the
//! parent reads what the child saw.
//!
//! The parent never writes its own environment (a test that did would race every other test of
//! the binary); the child's environment is made with `Command::env`.

use std::process::Command;

use azul::{desktop::app_settings::AppSettings, env::ProcessEnv};

/// Marks the child run of this binary.
const CHILD: &str = "AZ_ENV_PROCESS_PROBE_CHILD";

/// What the child prints: the settings it built from its process environment, one `key=value`
/// per switch, in a line the parent can find.
fn describe(settings: &AppSettings) -> String {
    format!(
        "PROBE backend={:?} headless_snapshot={:?} hit_debug={} exit_after_first_frame={} \
         patching={} dense_text={:?} hinting={} atlas={}",
        settings.shell.backend,
        settings.shell.headless_snapshot_path,
        settings.shell.hit_debug,
        settings.shell.exit_after_first_frame,
        settings.layout.solver.patching,
        settings.layout.solver.dense_text,
        settings.layout.text.hinting,
        settings.webrender.atlas_size,
    )
}

/// Runs this very test as a child with the environment `vars` (every `AZ_*` variable of the
/// parent removed first, so what the child sees is exactly this) and returns its probe line.
fn probe(vars: &[(&str, &str)]) -> String {
    let mut child = Command::new(std::env::current_exe().expect("the test binary"));
    child.args([
        "--exact",
        "the_real_environment_reaches_the_settings_once",
        "--nocapture",
    ]);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("AZ_") {
            child.env_remove(name);
        }
    }
    child.env(CHILD, "1");
    for (name, value) in vars {
        child.env(name, value);
    }
    let output = child.output().expect("the child runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "the child failed: {text}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    text.lines()
        .find(|line| line.starts_with("PROBE "))
        .unwrap_or_else(|| panic!("the child printed no probe line: {text}"))
        .to_string()
}

#[test]
fn the_real_environment_reaches_the_settings_once() {
    // The child half: build the settings of the process the way `App::create` does and say what
    // they are.
    if std::env::var_os(CHILD).is_some() {
        let settings = AppSettings::from_lookup(&ProcessEnv::new());
        // (a line of its own: a harness that runs one test at a time has written `test name ... `
        // to this line already)
        println!("\n{}", describe(&settings));
        return;
    }

    // An environment that asked for things: each variable lands in its field.
    let asked = probe(&[
        ("AZ_BACKEND", "headless"),
        ("AZ_HEADLESS_SNAPSHOT_PATH", "/tmp/azul-probe.png"),
        ("AZ_HIT_DEBUG", "1"),
        ("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER", "1"),
        ("AZ_NO_DL_PATCH", "1"),
        ("AZ_DENSE_TEXT", "verify"),
        ("AZ_TEXT_HINTING", "0"),
        ("AZ_WR_ATLAS_SIZE", "2048"),
    ]);
    assert!(asked.contains("backend=Some(\"headless\")"), "{asked}");
    assert!(
        asked.contains("headless_snapshot=Some(\"/tmp/azul-probe.png\")"),
        "{asked}"
    );
    assert!(asked.contains("hit_debug=true"), "{asked}");
    assert!(asked.contains("exit_after_first_frame=true"), "{asked}");
    assert!(asked.contains("patching=false"), "AZ_NO_DL_PATCH: {asked}");
    assert!(
        asked.contains("dense_text=Verify"),
        "AZ_DENSE_TEXT: {asked}"
    );
    assert!(asked.contains("hinting=false"), "AZ_TEXT_HINTING: {asked}");
    assert!(asked.contains("atlas=2048"), "AZ_WR_ATLAS_SIZE: {asked}");

    // An empty environment is the default of everything: the shipped behaviour.
    let none = probe(&[]);
    assert!(none.contains("backend=None"), "{none}");
    assert!(none.contains("headless_snapshot=None"), "{none}");
    assert!(none.contains("hit_debug=false"), "{none}");
    assert!(none.contains("exit_after_first_frame=false"), "{none}");
    assert!(none.contains("patching=true"), "{none}");
    assert!(none.contains("dense_text=On"), "{none}");
    assert!(none.contains("atlas=1024"), "{none}");
    // and a value that is not valid text is no value: `std::env::var(..).ok()` was `None` for it
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut child = Command::new(std::env::current_exe().expect("the test binary"));
        child.args([
            "--exact",
            "the_real_environment_reaches_the_settings_once",
            "--nocapture",
        ]);
        child.env(CHILD, "1");
        child.env(
            "AZ_BACKEND",
            std::ffi::OsStr::from_bytes(&[0x68, 0xff, 0x69]),
        );
        let output = child.output().expect("the child runs");
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(
            text.contains("backend=None"),
            "a value that is not UTF-8 is unset: {text}"
        );
    }
}

/// The reader of a process with no Android properties is the process environment and nothing
/// else.
#[test]
fn the_reader_of_a_desktop_is_the_process_environment() {
    use azul_css::env::EnvSource;

    if std::env::var_os(CHILD).is_some() {
        return;
    }
    let env = ProcessEnv::new();
    assert_eq!(env.var("PATH"), std::env::var("PATH").ok());
    assert_eq!(env.var_os("PATH"), std::env::var_os("PATH"));
}

/// What `PlatformCapability::notifications()` answers in the child: its `backend` word.
fn probe_backend_word() -> String {
    azul::desktop::notifications::probe()
        .backend
        .as_str()
        .to_string()
}

/// An app that asks whether it can notify while it builds its state, BEFORE `App::create` has
/// read the environment (AzClock's `ClockApp::new`), is told that a headless run records: the
/// process itself is asked, through the one reader.
#[test]
fn a_probe_before_the_app_exists_sees_the_headless_backend() {
    const PROBE_CHILD: &str = "AZ_ENV_PROCESS_NOTIFICATION_CHILD";
    if std::env::var_os(PROBE_CHILD).is_some() {
        println!("\nNOTIFICATIONS {}", probe_backend_word());
        return;
    }
    let run = |headless: bool| -> String {
        let mut child = Command::new(std::env::current_exe().expect("the test binary"));
        child.args([
            "--exact",
            "a_probe_before_the_app_exists_sees_the_headless_backend",
            "--nocapture",
        ]);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("AZ_") {
                child.env_remove(name);
            }
        }
        child.env(PROBE_CHILD, "1");
        if headless {
            child.env("AZ_BACKEND", "headless");
        }
        let output = child.output().expect("the child runs");
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(
            output.status.success(),
            "the child failed: {text}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        text.lines()
            .find_map(|line| line.strip_prefix("NOTIFICATIONS "))
            .unwrap_or_else(|| panic!("the child printed no probe line: {text}"))
            .to_string()
    };
    assert_eq!(run(true), "headless (recorded)", "AZ_BACKEND=headless");
    assert_ne!(run(false), "headless (recorded)", "no AZ_BACKEND");
}
