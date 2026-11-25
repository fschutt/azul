//! Running a program for an agent: the prompt on standard input, capped output, the deadline and
//! the cancel that kill the program and its helpers, and the search for the program. The programs
//! are `/bin/sh -c` one-liners (no script files: a script written and run at once races a parallel
//! test's `fork` for "text file busy").

use std::path::{Path, PathBuf};

use super::*;
use crate::agent_backend::{never_cancelled, Clock, ManualClock};

// ==== finding a program (pure) ====

fn host() -> Host {
    Host::new(Some("/usr/bin:/bin"), Some("/Users/ann"))
}

#[test]
fn the_path_is_searched_first_and_the_usual_places_after_it() {
    let folders: Vec<String> = host()
        .folders()
        .iter()
        .map(|f| f.display().to_string())
        .collect();
    assert_eq!(
        folders,
        vec![
            "/usr/bin",
            "/bin",
            "/Users/ann/.local/bin",
            "/Users/ann/.claude/local",
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/Users/ann/.npm-global/bin",
            "/Users/ann/.bun/bin",
        ]
    );
}

#[test]
fn without_a_home_folder_the_places_under_it_are_left_out() {
    let folders: Vec<String> = Host::new(Some("/usr/bin"), None)
        .folders()
        .iter()
        .map(|f| f.display().to_string())
        .collect();
    assert_eq!(
        folders,
        vec!["/usr/bin", "/opt/homebrew/bin", "/usr/local/bin"]
    );
    assert_eq!(Host::new(None, Some("  ")), Host::default());
}

#[test]
fn a_folder_of_the_path_that_is_also_a_usual_place_is_looked_in_once() {
    let h = Host::new(Some("/opt/homebrew/bin:/usr/bin"), None);
    let folders: Vec<String> = h
        .folders()
        .iter()
        .map(|f| f.display().to_string())
        .collect();
    assert_eq!(
        folders,
        vec!["/opt/homebrew/bin", "/usr/bin", "/usr/local/bin"]
    );
}

#[test]
fn the_first_folder_that_has_the_program_wins() {
    let has = |p: &Path| {
        p == Path::new("/Users/ann/.local/bin/claude") || p == Path::new("/usr/local/bin/claude")
    };
    assert_eq!(
        find_program("", "claude", &host(), &has),
        Ok(PathBuf::from("/Users/ann/.local/bin/claude"))
    );
    let on_path =
        |p: &Path| p == Path::new("/bin/claude") || p == Path::new("/opt/homebrew/bin/claude");
    assert_eq!(
        find_program("", "claude", &host(), &on_path),
        Ok(PathBuf::from("/bin/claude"))
    );
}

#[test]
fn a_program_that_is_not_anywhere_lists_every_place_that_was_looked_in() {
    let nowhere = |_: &Path| false;
    let searched = find_program("", "gemini", &host(), &nowhere).expect_err("not there");
    assert_eq!(searched.len(), 8);
    assert_eq!(searched[0], "/usr/bin");
    assert_eq!(searched[7], "/Users/ann/.bun/bin");
}

#[test]
fn a_configured_path_is_used_as_it_is_and_a_configured_name_is_searched_for() {
    let has =
        |p: &Path| p == Path::new("/opt/tools/claude") || p == Path::new("/usr/bin/claude-next");
    assert_eq!(
        find_program("/opt/tools/claude", "claude", &host(), &has),
        Ok(PathBuf::from("/opt/tools/claude"))
    );
    assert_eq!(
        find_program("/opt/other/claude", "claude", &host(), &has),
        Err(vec!["/opt/other/claude".to_string()]),
        "a path that is not there is not searched for elsewhere"
    );
    assert_eq!(
        find_program(" claude-next ", "claude", &host(), &has),
        Ok(PathBuf::from("/usr/bin/claude-next"))
    );
}

#[test]
fn the_names_a_program_has_on_windows_are_tried_too() {
    let has = |p: &Path| p == Path::new("/bin/claude.cmd");
    assert_eq!(
        find_program("", "claude", &host(), &has),
        Ok(PathBuf::from("/bin/claude.cmd"))
    );
}

// ==== running a program ====

#[cfg(unix)]
mod unix {
    use std::{cell::Cell, rc::Rc, time::Instant};

    use super::*;

    fn sh(script: &str) -> Spec {
        Spec {
            program: PathBuf::from("/bin/sh"),
            args: vec!["-c".to_string(), script.to_string()],
            cwd: None,
            env_set: Vec::new(),
            env_remove: Vec::new(),
            stdin: Vec::new(),
            // ms of the test's clock, which a poll of 15 ms moves by 15 ms while it waits 1 ms of
            // real time: a program that is slow on a busy machine is not the thing under test (the
            // tests of the deadline set their own)
            timeout_ms: 600_000,
            max_stdout: 64 * 1024,
            max_stderr: 64 * 1024,
        }
    }

    fn run_on(spec: &Spec, clock: &mut ManualClock) -> Result<Output, Failure> {
        let mut ctl = Control::new(clock, &never_cancelled);
        run(spec, &mut ctl)
    }

    fn quick(spec: &Spec) -> Result<Output, Failure> {
        run_on(spec, &mut ManualClock::with_real_pause(1))
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "azreview-agent-process-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        dir
    }

    #[test]
    fn the_prompt_goes_in_on_standard_input_and_what_comes_out_is_read() {
        let mut spec = sh("cat");
        spec.stdin = b"hello\nworld\n".to_vec();
        let out = quick(&spec).expect("runs");
        assert_eq!(out.code, Some(0));
        assert_eq!(out.stdout_text(), "hello\nworld\n");
        assert!(!out.stdout_cut && !out.stderr_cut);
        assert_eq!(out.stderr_text(), "");
    }

    #[test]
    fn an_exit_code_and_standard_error_come_back_and_a_failing_program_is_no_failure_of_ours() {
        let out = quick(&sh("echo oops >&2; exit 3")).expect("runs");
        assert_eq!(out.code, Some(3));
        assert_eq!(out.stderr_text(), "oops\n");
    }

    #[test]
    fn a_program_ended_by_a_signal_has_no_code() {
        let out = quick(&sh("kill -9 $$")).expect("runs");
        assert_eq!(out.code, None);
    }

    #[test]
    fn the_working_folder_and_the_environment_are_the_calls() {
        let dir = temp("cwd");
        let mut spec = sh("pwd; echo \"$AGB_ONE-${AGB_TWO:-unset}\"");
        spec.cwd = Some(dir.clone());
        spec.env_set = vec![
            ("AGB_ONE".to_string(), "1".to_string()),
            ("AGB_TWO".to_string(), "2".to_string()),
        ];
        spec.env_remove = vec!["AGB_TWO".to_string()];
        let out = quick(&spec).expect("runs");
        let lines: Vec<String> = out.stdout_text().lines().map(str::to_string).collect();
        assert_eq!(
            std::fs::canonicalize(&lines[0]).expect("a real folder"),
            std::fs::canonicalize(&dir).expect("a real folder")
        );
        assert_eq!(lines[1], "1-unset");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_program_that_never_ends_is_killed_at_the_deadline() {
        let mut spec = sh("sleep 30");
        spec.timeout_ms = 300;
        let began = Instant::now();
        let mut clock = ManualClock::with_real_pause(1);
        assert_eq!(run_on(&spec, &mut clock), Err(Failure::TimedOut));
        assert!(
            clock.now_ms() >= 300 && clock.now_ms() < 400,
            "{}",
            clock.now_ms()
        );
        assert!(
            began.elapsed().as_secs() < 10,
            "it did not wait for the sleep"
        );
    }

    #[test]
    fn a_helper_the_program_started_is_killed_with_it() {
        let dir = temp("group");
        let pid_file = dir.join("pid");
        let mut spec = sh(&format!(
            "sleep 31 & echo $! > '{}'; wait",
            pid_file.display()
        ));
        spec.timeout_ms = 400;
        assert_eq!(quick(&spec), Err(Failure::TimedOut));
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .expect("the helper wrote its pid")
            .trim()
            .parse()
            .expect("a pid");
        let gone = (0..100).any(|_| {
            let alive = Command::new("/bin/kill")
                .args(["-0", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if alive {
                std::thread::sleep(Duration::from_millis(30));
            }
            !alive
        });
        assert!(gone, "the helper {pid} is still running");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_window_cancelling_kills_the_program_at_once() {
        struct Shared(Rc<Cell<u64>>);
        impl Clock for Shared {
            fn now_ms(&mut self) -> u64 {
                self.0.get()
            }
            fn sleep_ms(&mut self, ms: u64) {
                self.0.set(self.0.get() + ms);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        let time = Rc::new(Cell::new(0));
        let mut clock = Shared(time.clone());
        let seen = time.clone();
        let cancel = move || seen.get() >= 150;
        let mut ctl = Control::new(&mut clock, &cancel);
        let began = Instant::now();
        assert_eq!(run(&sh("sleep 30"), &mut ctl), Err(Failure::Cancelled));
        assert!(time.get() >= 150 && time.get() < 200, "{}", time.get());
        assert!(began.elapsed().as_secs() < 10);
    }

    #[test]
    fn what_a_program_writes_is_capped_and_it_is_still_waited_for_to_its_end() {
        let mut spec = sh("head -c 3000000 /dev/zero | tr '\\0' x; echo done >&2");
        spec.max_stdout = 1_000;
        let out = quick(&spec).expect("runs");
        assert_eq!(out.stdout.len(), 1_000);
        assert!(out.stdout_cut);
        assert_eq!(out.stderr_text(), "done\n");
        assert_eq!(out.code, Some(0));
    }

    #[test]
    fn standard_error_is_capped_on_its_own() {
        let mut spec = sh("head -c 200000 /dev/zero | tr '\\0' e >&2");
        spec.max_stderr = 100;
        let out = quick(&spec).expect("runs");
        assert_eq!(
            (out.stderr.len(), out.stderr_cut, out.stdout_cut),
            (100, true, false)
        );
    }

    #[test]
    fn a_big_prompt_is_written_without_a_deadlock_whether_the_program_reads_it_or_not() {
        let big = vec![b'p'; 300_000];
        let mut echo = sh("cat");
        echo.stdin = big.clone();
        echo.max_stdout = 1 << 20;
        let out = quick(&echo).expect("runs");
        assert_eq!(out.stdout.len(), big.len());
        let mut deaf = sh("true");
        deaf.stdin = big;
        assert_eq!(quick(&deaf).expect("runs").code, Some(0));
    }

    #[test]
    fn a_helper_that_outlives_the_program_does_not_hold_the_answer_back() {
        let began = Instant::now();
        let out = quick(&sh("sleep 6 & echo done")).expect("runs");
        assert_eq!(out.stdout_text(), "done\n");
        assert!(began.elapsed().as_secs() < 5, "{:?}", began.elapsed());
    }

    #[test]
    fn a_program_that_is_not_there_is_not_found_and_one_that_may_not_run_is_refused() {
        let mut spec = sh("true");
        spec.program = PathBuf::from("/nonexistent/folder/program");
        assert_eq!(quick(&spec), Err(Failure::NotFound));
        let dir = temp("denied");
        let file = dir.join("not-executable");
        std::fs::write(&file, "#!/bin/sh\ntrue\n").expect("a file");
        spec.program = file.clone();
        assert_eq!(quick(&spec), Err(Failure::PermissionDenied));
        assert!(!is_program(&file));
        assert!(is_program(Path::new("/bin/sh")));
        assert!(!is_program(&dir), "a folder is no program");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_failures_are_said_in_the_words_of_an_agent_error() {
        let places = vec!["/usr/bin".to_string()];
        assert_eq!(
            Failure::NotFound.into_error("claude", places.clone(), 90_000),
            AgentError::NotInstalled {
                program: "claude".to_string(),
                searched: places
            }
        );
        assert_eq!(
            Failure::TimedOut.into_error("claude", Vec::new(), 90_000),
            AgentError::Timeout { secs: 90 }
        );
        assert_eq!(
            Failure::TimedOut.into_error("claude", Vec::new(), 1_200),
            AgentError::Timeout { secs: 2 }
        );
        assert_eq!(
            Failure::Cancelled.into_error("claude", Vec::new(), 1),
            AgentError::Cancelled
        );
        assert!(matches!(
            Failure::PermissionDenied.into_error("claude", Vec::new(), 1),
            AgentError::Spawn { .. }
        ));
        assert!(
            matches!(Failure::Spawn("boom".to_string()).into_error("claude", Vec::new(), 1), AgentError::Spawn { why, .. } if why == "boom")
        );
    }
}
