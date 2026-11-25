//! `claude -p` as a backend: the command line it is given, the answer it is read from (the real
//! envelopes of claude 2.1.332 are the fixtures: two answers and the "not logged in" failure, with
//! the ids made up), the failures told apart by their words, and the whole path through a stand-in
//! program that records what it was given. No model, no network.

use std::path::PathBuf;

use super::*;
use crate::{
    agent_backend::{Request, Task},
    agent_review::{build_call, Context, ReviewRequest, Utterance},
};

const OK_EDITING: &str = include_str!("agent_fixtures/claude_ok_editing.json");
const OK_EXPLORATION: &str = include_str!("agent_fixtures/claude_ok_exploration.json");
const NOT_LOGGED_IN: &str = include_str!("agent_fixtures/claude_not_logged_in.json");

fn config() -> ClaudeConfig {
    ClaudeConfig {
        program: PathBuf::from("/usr/local/bin/claude"),
        model: "haiku".to_string(),
        timeout_secs: 90,
        budget_cents: 30,
        max_turns: 0,
        effort: String::new(),
        extra_args: Vec::new(),
        key: None,
        limits: Limits::default(),
    }
}

fn request(task: Task) -> Request {
    Request::Review(ReviewRequest {
        task,
        conversation: 1,
        language: "en".to_string(),
        utterances: vec![Utterance {
            text: "what does this do?".to_string(),
            start_ms: 0,
            end_ms: 900,
        }],
        strokes: Vec::new(),
        context: Context {
            file: "a.rs".to_string(),
            page: 0,
            first_line: 1,
            lines: vec!["fn a() {}".to_string()],
            total_lines: 1,
            focus: None,
            words_under: Vec::new(),
            grid: None,
        },
        turns: Vec::new(),
        workspace: Some(PathBuf::from("/work/space")),
    })
}

fn with_call<T>(task: Task, f: impl FnOnce(&Call<'_>) -> T) -> T {
    let req = request(task);
    let Request::Review(review) = &req;
    let call = build_call(&req, review, &Limits::default()).expect("builds");
    f(&call)
}

fn output(code: Option<i32>, stdout: &str, stderr: &str) -> Output {
    Output {
        code,
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        ..Output::default()
    }
}

// ==== the command line ====

#[test]
fn the_editing_has_no_tools_and_its_own_instructions_and_the_prompt_is_not_on_the_line() {
    with_call(Task::Editing, |call| {
        let a = args(&config(), call);
        assert_eq!(
            a,
            vec![
                "-p",
                "--no-session-persistence",
                "--safe-mode",
                "--tools",
                "",
                "--system-prompt",
                call.system.as_str(),
                "--model",
                "haiku",
                "--max-budget-usd",
                "0.30",
                "--output-format",
                "json",
            ]
        );
        assert!(
            !a.iter().any(|x| x == &call.prompt),
            "the prompt goes in on standard input"
        );
    });
}

#[test]
fn the_exploration_may_read_and_search_and_nothing_else_and_asks_nobody() {
    with_call(Task::Exploration, |call| {
        let a = args(&config(), call);
        assert_eq!(
            a,
            vec![
                "-p",
                "--no-session-persistence",
                "--safe-mode",
                "--tools",
                "Read,Grep,Glob",
                "--permission-mode",
                "dontAsk",
                "--allowedTools",
                "Read,Grep,Glob",
                "--append-system-prompt",
                call.system.as_str(),
                "--model",
                "haiku",
                "--max-budget-usd",
                "0.30",
                "--output-format",
                "json",
            ]
        );
    });
}

#[test]
fn a_model_a_cost_limit_and_a_turn_limit_are_only_there_when_they_are_set() {
    with_call(Task::Editing, |call| {
        let mut c = config();
        c.model = "  ".to_string();
        c.budget_cents = 0;
        let a = args(&c, call);
        assert!(
            !a.contains(&"--model".to_string()) && !a.contains(&"--max-budget-usd".to_string()),
            "{a:?}"
        );
        assert!(!a.contains(&"--max-turns".to_string()));
        c.budget_cents = 125;
        c.max_turns = 6;
        c.model = " opus ".to_string();
        let a = args(&c, call);
        let at = |flag: &str| a.iter().position(|x| x == flag).map(|i| a[i + 1].as_str());
        assert_eq!(at("--max-budget-usd"), Some("1.25"));
        assert_eq!(at("--max-turns"), Some("6"));
        assert_eq!(at("--model"), Some("opus"));
        c.budget_cents = 5;
        let a = args(&c, call);
        let at = |flag: &str| a.iter().position(|x| x == flag).map(|i| a[i + 1].as_str());
        assert_eq!(at("--max-budget-usd"), Some("0.05"));
    });
}

#[test]
fn an_effort_is_only_there_when_it_is_set() {
    with_call(Task::Editing, |call| {
        let mut c = config();
        assert!(!args(&c, call).contains(&"--effort".to_string()));
        c.effort = " low ".to_string();
        let a = args(&c, call);
        let at = a.iter().position(|x| x == "--effort").expect("set");
        assert_eq!(a[at + 1], "low");
        assert!(at < a.iter().position(|x| x == "--output-format").expect("last"));
    });
}

#[test]
fn extra_arguments_come_last_after_an_option_that_takes_one_value() {
    with_call(Task::Exploration, |call| {
        let mut c = config();
        c.extra_args = vec!["--effort".to_string(), "low".to_string()];
        let a = args(&c, call);
        let n = a.len();
        assert_eq!(&a[n - 4..], ["--output-format", "json", "--effort", "low"]);
        // `--tools` and `--allowedTools` take any number of values: neither is last
        assert_ne!(a[n - 1], "Read,Grep,Glob");
    });
}

#[test]
fn what_to_run_has_the_prompt_the_folder_the_environment_and_the_caps() {
    with_call(Task::Exploration, |call| {
        let mut c = config();
        c.key = Some(Secret::new("a-key-of-the-user"));
        let spec = spec(&c, call);
        assert_eq!(spec.program, PathBuf::from("/usr/local/bin/claude"));
        assert_eq!(spec.stdin, call.prompt.as_bytes());
        assert_eq!(spec.cwd, Some(PathBuf::from("/work/space")));
        assert_eq!(spec.timeout_ms, 90_000);
        assert_eq!(spec.max_stdout, Limits::default().max_stdout_bytes);
        assert!(spec
            .env_set
            .contains(&("DISABLE_AUTOUPDATER".to_string(), "1".to_string())));
        assert!(spec.env_set.contains(&(
            "ANTHROPIC_API_KEY".to_string(),
            "a-key-of-the-user".to_string()
        )));
    });
    with_call(Task::Editing, |call| {
        let spec = spec(&config(), call);
        assert_eq!(spec.cwd, None, "an agent without tools reads no folder");
        assert!(
            !spec.env_set.iter().any(|(k, _)| k == "ANTHROPIC_API_KEY"),
            "no key: the program's own login"
        );
    });
}

// ==== the answer ====

#[test]
fn a_real_answer_is_read_with_what_it_cost_and_who_gave_it() {
    let c = read_envelope(&output(Some(0), OK_EDITING, ""), &config(), &[]).expect("an answer");
    assert_eq!(
        c.text,
        "{\"answer\": \"Connection test received: the reviewer asked for the answer ok with no suggestions.\", \"suggestions\": []}"
    );
    assert_eq!(
        c.usage.input_tokens,
        Some(1_265),
        "2 new and 1263 written to the cache"
    );
    assert_eq!(c.usage.output_tokens, Some(43));
    assert_eq!(c.usage.cost_usd, Some(0.000_274_3));
    assert_eq!(c.usage.turns, Some(1));
    assert_eq!(c.model, "claude-haiku-5-5");
}

#[test]
fn a_real_exploration_answer_took_two_turns_and_is_plain_text() {
    let c = read_envelope(&output(Some(0), OK_EXPLORATION, ""), &config(), &[]).expect("an answer");
    assert_eq!(c.text, "pelican");
    assert_eq!(c.usage.turns, Some(2));
    assert_eq!(c.usage.input_tokens, Some(12_107));
}

#[test]
fn a_failure_that_says_success_is_still_a_failure_and_is_told_to_sign_in() {
    // the real envelope of a machine that is not signed in: exit code 1, `is_error`, and a subtype
    // that says "success"
    match read_envelope(&output(Some(1), NOT_LOGGED_IN, ""), &config(), &[]) {
        Err(AgentError::NotLoggedIn { program, detail }) => {
            assert_eq!(program, "claude");
            assert!(detail.contains("Not logged in"), "{detail}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_envelope_is_found_after_a_line_of_warning() {
    let text = format!("warning: something about the terminal\n{OK_EDITING}");
    assert!(read_envelope(&output(Some(0), &text, ""), &config(), &[]).is_ok());
}

fn failing(subtype: &str, result: &str, turns: u32) -> String {
    format!("{{\"type\":\"result\",\"subtype\":\"{subtype}\",\"is_error\":true,\"num_turns\":{turns},\"result\":{}}}", crate::agent_json::quote(result))
}

#[test]
fn the_agent_that_used_its_turns_or_its_budget_says_so() {
    let mut c = config();
    c.max_turns = 8;
    assert_eq!(
        read_envelope(
            &output(Some(1), &failing("error_max_turns", "", 9), ""),
            &c,
            &[]
        )
        .expect_err("turns"),
        AgentError::MaxTurns { turns: 9 }
    );
    assert_eq!(
        read_envelope(
            &output(Some(1), &failing("error_max_budget_usd", "", 2), ""),
            &c,
            &[]
        )
        .expect_err("budget"),
        AgentError::Budget { limit_usd: 0.30 }
    );
}

#[test]
fn failures_that_come_as_words_are_told_apart_by_what_they_say() {
    let said = |text: &str| {
        read_envelope(
            &output(Some(1), &failing("success", text, 1), ""),
            &config(),
            &[],
        )
        .expect_err(text)
    };
    assert!(matches!(
        said("Invalid API key · Fix external API key"),
        AgentError::NotLoggedIn { .. }
    ));
    assert!(matches!(
        said("OAuth token has expired. Please run /login"),
        AgentError::NotLoggedIn { .. }
    ));
    assert!(matches!(
        said("API Error: 429 Too Many Requests"),
        AgentError::RateLimited { .. }
    ));
    assert!(matches!(
        said("Claude usage limit reached. Your limit will reset at 3pm"),
        AgentError::RateLimited { .. }
    ));
    assert!(matches!(
        said("API Error: 529 overloaded_error"),
        AgentError::RateLimited { .. }
    ));
    assert!(matches!(
        said("API Error: Unable to connect to API (ConnectionRefused)"),
        AgentError::Network { .. }
    ));
    match said("There's an issue with the selected model (nonesuch). It may not exist or you may not have access to it.") {
        AgentError::Settings { why } => assert!(why.contains("haiku"), "{why}"),
        other => panic!("{other:?}"),
    }
    match said("something nobody has seen before") {
        AgentError::Exit {
            program,
            code,
            detail,
        } => {
            assert_eq!((program.as_str(), code), ("claude", Some(1)));
            assert!(detail.contains("nobody has seen"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_program_that_stopped_without_an_envelope_is_read_by_what_it_said_on_standard_error() {
    let usage = "\"x\" was read as a value of --tools, not as a prompt\nError: Input must be provided either through stdin or as a prompt argument when using --print";
    match read_envelope(&output(Some(1), "", usage), &config(), &[]).expect_err("no envelope") {
        AgentError::Exit { detail, .. } => {
            assert!(detail.contains("Input must be provided"), "{detail}");
            assert!(detail.contains(" | "), "the last lines, one line: {detail}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_exit_of_zero_without_an_envelope_is_a_bad_output() {
    for stdout in ["", "just words", "{\"unrelated\": 1}"] {
        let e = read_envelope(&output(Some(0), stdout, ""), &config(), &[]).expect_err(stdout);
        assert!(
            matches!(e, AgentError::BadOutput { .. }),
            "{stdout:?}: {e:?}"
        );
    }
}

#[test]
fn a_key_that_leaks_into_what_the_program_says_is_scrubbed() {
    let stderr = "request failed with x-api-key: the-users-own-key-123 on the wire";
    match read_envelope(
        &output(Some(1), "", stderr),
        &config(),
        &["the-users-own-key-123"],
    )
    .expect_err("failed")
    {
        AgentError::Exit { detail, .. } => {
            assert!(!detail.contains("the-users-own-key-123"), "{detail}")
        }
        other => panic!("{other:?}"),
    }
}

// ==== through a stand-in program ====

#[cfg(unix)]
mod through_a_program {
    use std::{
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
    };

    use super::*;
    use crate::agent_backend::{never_cancelled, run_task, Body, ManualClock};

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "azreview-agent-claude-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        dir
    }

    /// A program of the test's own that records what it is given and answers with `body`. A script
    /// written and started at once can meet "text file busy" when another test forks in between, so it
    /// is tried until it starts.
    fn stand_in(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("claude");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("a script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
        for _ in 0..200 {
            match std::process::Command::new(&path)
                .arg("--probe")
                .stdin(std::process::Stdio::null())
                .output()
            {
                Err(e) if e.raw_os_error() == Some(26) => {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                _ => break,
            }
        }
        path
    }

    fn run_with(
        program: PathBuf,
        task: Task,
        timeout_secs: u32,
    ) -> Result<crate::agent_backend::Response, AgentError> {
        let mut c = config();
        c.program = program;
        c.timeout_secs = timeout_secs;
        let mut backend = ClaudePrint { config: c };
        let mut clock = ManualClock::with_real_pause(1);
        let mut ctl = Control::new(&mut clock, &never_cancelled);
        run_task(&mut backend, &request(task), &Limits::default(), &mut ctl)
    }

    #[test]
    fn the_whole_path_runs_the_program_with_the_prompt_on_stdin_and_reads_its_envelope() {
        let dir = temp("whole");
        std::fs::write(dir.join("envelope.json"), OK_EDITING).expect("fixture");
        let program = stand_in(
            &dir,
            &format!(
                "case \"$1\" in --probe) exit 0;; esac\nprintf '%s\\n' \"$@\" > '{d}/args'\ncat > '{d}/stdin'\ncat '{d}/envelope.json'",
                d = dir.display()
            ),
        );
        let response = run_with(program, Task::Editing, 30).expect("answers");
        let Body::Review(reply) = &response.body;
        assert!(
            reply.answer.contains("Connection test received"),
            "{}",
            reply.answer
        );
        assert_eq!(response.identity.kind, BackendKind::ClaudePrint);
        assert_eq!(response.usage.cost_usd, Some(0.000_274_3));
        let args = std::fs::read_to_string(dir.join("args")).expect("it was run");
        assert!(
            args.starts_with(
                "-p\n--no-session-persistence\n--safe-mode\n--tools\n\n--system-prompt\n"
            ),
            "{args}"
        );
        let stdin = std::fs::read_to_string(dir.join("stdin")).expect("it read its input");
        assert!(stdin.starts_with("Task: editing\n"), "{stdin}");
        assert!(stdin.contains("what does this do?"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_program_that_never_answers_is_stopped_at_the_timeout_and_the_error_says_how_long() {
        let dir = temp("slow");
        let program = stand_in(&dir, "case \"$1\" in --probe) exit 0;; esac\nsleep 30");
        let err = run_with(program, Task::Editing, 1).expect_err("timeout");
        assert_eq!(err, AgentError::Timeout { secs: 1 });
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_program_that_is_not_there_is_not_installed() {
        let err = run_with(
            PathBuf::from("/nonexistent/folder/claude"),
            Task::Editing,
            5,
        )
        .expect_err("missing");
        assert!(
            matches!(err, AgentError::NotInstalled { ref program, .. } if program == "claude"),
            "{err:?}"
        );
    }

    #[test]
    fn the_backend_names_itself_by_its_model() {
        let backend = ClaudePrint { config: config() };
        assert_eq!(
            backend.identity(),
            Identity {
                kind: BackendKind::ClaudePrint,
                model: "haiku".to_string()
            }
        );
    }
}
