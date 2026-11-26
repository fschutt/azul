//! The Gemini CLI as a backend: the command line it is given, the text that goes in on standard
//! input (which can never begin like the "yes" a signed-out CLI waits for), the answer it is read
//! from, and the failures. The fixtures: the CLI's own words for "no auth method" and for "sign in
//! again" are recorded from gemini 0.65.0; its success has the shape its source builds
//! (`JsonFormatter`), because the machine this was written on had no valid sign-in to record one.

use std::path::PathBuf;

use super::*;
use crate::{
    agent_backend::{Request, Task},
    agent_review::{build_call, Context, ReviewRequest, Utterance},
};

const OK_FROM_SOURCE: &str = include_str!("agent_fixtures/gemini_ok_from_source.json");
const NO_AUTH_STDERR: &str = include_str!("agent_fixtures/gemini_no_auth.stderr");
const EXPIRED_STDOUT: &str = include_str!("agent_fixtures/gemini_signin_expired.stdout");
const EXPIRED_STDERR: &str = include_str!("agent_fixtures/gemini_signin_expired.stderr");

fn config() -> GeminiConfig {
    GeminiConfig {
        program: PathBuf::from("/opt/homebrew/bin/gemini"),
        model: "gemini-2.5-flash".to_string(),
        timeout_secs: 90,
        extra_args: Vec::new(),
        key: None,
        scratch: Some(PathBuf::from("/tmp/agent-scratch")),
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
            words: Vec::new(),
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
    let review = req.as_review().expect("a review request");
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

// ==== the command line and what goes in ====

#[test]
fn the_command_line_is_headless_json_read_only_and_trusts_the_folder_so_plan_mode_holds() {
    with_call(Task::Editing, |call| {
        assert_eq!(
            args(&config(), call),
            vec![
                "-p",
                "Follow the instructions above and answer from the text given, without using any tool.",
                "-o",
                "json",
                "--approval-mode",
                "plan",
                "--skip-trust",
                "-m",
                "gemini-2.5-flash",
            ]
        );
    });
}

#[test]
fn no_model_is_the_clis_own_routing_and_extra_arguments_come_last() {
    with_call(Task::Exploration, |call| {
        let mut c = config();
        c.model = "  ".to_string();
        c.extra_args = vec!["--include-directories".to_string(), "/x".to_string()];
        let a = args(&c, call);
        assert!(!a.contains(&"-m".to_string()), "{a:?}");
        assert_eq!(&a[a.len() - 2..], ["--include-directories", "/x"]);
        assert!(a[1].contains("may read files"), "{}", a[1]);
    });
}

#[test]
fn what_goes_in_begins_with_a_line_that_is_no_answer_and_then_the_instructions_and_the_request() {
    with_call(Task::Editing, |call| {
        let text = input_text(call);
        assert!(text.starts_with("# AzReview agent request"), "{text}");
        let first = text.chars().next().expect("not empty");
        assert!(
            !first.eq_ignore_ascii_case(&'y') && !first.is_whitespace(),
            "a signed-out CLI would take it for an answer"
        );
        let system_at = text.find(&call.system).expect("the instructions");
        let prompt_at = text.find(&call.prompt).expect("the request");
        assert!(system_at < prompt_at);
    });
}

#[test]
fn the_folder_is_the_workspace_for_an_exploration_and_an_empty_one_for_the_rest() {
    with_call(Task::Exploration, |call| {
        assert_eq!(
            spec(&config(), call).cwd,
            Some(PathBuf::from("/work/space"))
        );
    });
    with_call(Task::Editing, |call| {
        let s = spec(&config(), call);
        assert_eq!(s.cwd, Some(PathBuf::from("/tmp/agent-scratch")));
        assert_eq!(s.stdin, input_text(call).into_bytes());
        assert_eq!(s.timeout_ms, 90_000);
        assert!(s.env_set.is_empty(), "no key: the program's own sign-in");
        let mut c = config();
        c.key = Some(Secret::new("a-key-of-the-user"));
        c.scratch = None;
        let s = spec(&c, call);
        assert_eq!(s.cwd, None);
        assert_eq!(
            s.env_set,
            vec![(
                "GEMINI_API_KEY".to_string(),
                "a-key-of-the-user".to_string()
            )]
        );
    });
}

// ==== the answer ====

#[test]
fn an_answer_is_read_with_its_tokens_and_the_model_that_gave_it() {
    let c = read_output(&output(Some(0), OK_FROM_SOURCE, ""), &config(), &[]).expect("an answer");
    assert_eq!(c.text, "{\"answer\": \"ok\", \"suggestions\": []}");
    assert_eq!(
        c.usage.input_tokens,
        Some(1_450),
        "all the prompt tokens, the cached ones included"
    );
    assert_eq!(c.usage.output_tokens, Some(14));
    assert_eq!(c.usage.cost_usd, None, "the CLI does not say what it cost");
    assert_eq!(c.model, "gemini-2.5-flash");
}

#[test]
fn an_answer_without_stats_is_still_an_answer() {
    let c = read_output(
        &output(Some(0), "{\"response\": \"plain\"}", ""),
        &config(),
        &[],
    )
    .expect("an answer");
    assert_eq!(
        (c.text.as_str(), c.usage.input_tokens, c.model.as_str()),
        ("plain", None, "")
    );
}

#[test]
fn the_object_is_found_after_a_line_the_cli_printed_first() {
    let text = format!("Loaded cached credentials.\n{OK_FROM_SOURCE}");
    assert!(read_output(&output(Some(0), &text, ""), &config(), &[]).is_ok());
}

#[test]
fn a_machine_with_no_auth_method_is_told_to_sign_in() {
    match read_output(&output(Some(41), "", NO_AUTH_STDERR), &config(), &[]).expect_err("no auth") {
        AgentError::NotLoggedIn { program, detail } => {
            assert_eq!(program, "gemini");
            assert!(detail.contains("Auth method"), "{detail}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_sign_in_that_has_expired_is_told_in_plain_words_and_nothing_opens_a_browser() {
    match read_output(
        &output(Some(130), EXPIRED_STDOUT, EXPIRED_STDERR),
        &config(),
        &[],
    )
    .expect_err("expired")
    {
        AgentError::NotLoggedIn { program, detail } => {
            assert_eq!(program, "gemini");
            assert_eq!(detail, "its sign-in has expired");
        }
        other => panic!("{other:?}"),
    }
}

fn error_json(message: &str, code: i64) -> String {
    format!(
        "{{\"session_id\": \"x\", \"error\": {{\"type\": \"ApiError\", \"message\": {}, \"code\": {code}}}}}",
        crate::agent_json::quote(message)
    )
}

#[test]
fn failures_are_told_apart_by_the_code_and_by_what_they_say() {
    let said = |message: &str, code: i64| {
        read_output(
            &output(Some(1), "", &error_json(message, code)),
            &config(),
            &[],
        )
        .expect_err(message)
    };
    assert!(matches!(
        said(
            "Quota exceeded for quota metric 'Gemini 2.5 Flash requests per minute'",
            429
        ),
        AgentError::RateLimited { .. }
    ));
    assert!(matches!(
        said("The model is overloaded. Please try again later.", 503),
        AgentError::RateLimited { .. }
    ));
    assert!(matches!(
        said(
            "request to https://example.test failed, reason: getaddrinfo ENOTFOUND",
            1
        ),
        AgentError::Network { .. }
    ));
    assert!(matches!(
        said("fetch failed", 1),
        AgentError::Network { .. }
    ));
    assert!(matches!(
        said("API key not valid. Please pass a valid API key.", 400),
        AgentError::NotLoggedIn { .. }
    ));
    assert!(matches!(said("anything", 53), AgentError::MaxTurns { .. }));
    match said(
        "models/gemini-9-ultra is not found for API version v1beta",
        404,
    ) {
        AgentError::Settings { why } => assert!(why.contains("gemini-2.5-flash"), "{why}"),
        other => panic!("{other:?}"),
    }
    match said("something nobody has seen", 42) {
        AgentError::Exit {
            program,
            code,
            detail,
        } => {
            assert_eq!((program.as_str(), code), ("gemini", Some(42)));
            assert!(detail.contains("nobody has seen"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_program_that_stopped_without_json_is_read_by_what_it_said() {
    match read_output(&output(Some(1), "", "Segmentation fault\n"), &config(), &[])
        .expect_err("no json")
    {
        AgentError::Exit { code, detail, .. } => {
            assert_eq!(code, Some(1));
            assert!(detail.contains("Segmentation fault"), "{detail}");
        }
        other => panic!("{other:?}"),
    }
    for stdout in ["", "words only"] {
        let e = read_output(&output(Some(0), stdout, ""), &config(), &[]).expect_err(stdout);
        assert!(
            matches!(e, AgentError::BadOutput { .. }),
            "{stdout:?}: {e:?}"
        );
    }
}

#[test]
fn a_key_that_leaks_into_what_the_program_says_is_scrubbed() {
    let stderr = error_json("request failed for ?key=the-users-own-key-123&alt=json", 1);
    match read_output(
        &output(Some(1), "", &stderr),
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
            "azreview-agent-gemini-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        dir
    }

    /// A program of the test's own (see the claude tests for why it is tried until it starts).
    fn stand_in(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("gemini");
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
        scratch: Option<PathBuf>,
    ) -> Result<crate::agent_backend::Response, AgentError> {
        let mut c = config();
        c.program = program;
        c.scratch = scratch;
        let mut backend = GeminiCli { config: c };
        let mut clock = ManualClock::with_real_pause(1);
        let mut ctl = Control::new(&mut clock, &never_cancelled);
        run_task(
            &mut backend,
            &request(Task::Editing),
            &Limits::default(),
            &mut ctl,
        )
    }

    #[test]
    fn the_whole_path_runs_the_program_with_the_request_on_stdin_and_reads_its_json() {
        let dir = temp("whole");
        std::fs::write(dir.join("ok.json"), OK_FROM_SOURCE).expect("fixture");
        let program = stand_in(
            &dir,
            &format!(
                "case \"$1\" in --probe) exit 0;; esac\nprintf '%s\\n' \"$@\" > '{d}/args'\npwd > '{d}/cwd'\ncat > '{d}/stdin'\ncat '{d}/ok.json'",
                d = dir.display()
            ),
        );
        let scratch = dir.join("scratch");
        std::fs::create_dir_all(&scratch).expect("a folder");
        let response = run_with(program, Some(scratch.clone())).expect("answers");
        let Body::Review(reply) = &response.body else {
            panic!("a review answer")
        };
        assert_eq!(reply.answer, "ok");
        assert_eq!(response.identity.kind, BackendKind::GeminiCli);
        assert_eq!(response.usage.output_tokens, Some(14));
        let args = std::fs::read_to_string(dir.join("args")).expect("it was run");
        assert!(
            args.contains("--skip-trust") && args.contains("--approval-mode\nplan"),
            "{args}"
        );
        let stdin = std::fs::read_to_string(dir.join("stdin")).expect("it read its input");
        assert!(stdin.starts_with("# AzReview agent request"), "{stdin}");
        assert!(stdin.contains("what does this do?"));
        let cwd = std::fs::read_to_string(dir.join("cwd")).expect("it said where it ran");
        assert_eq!(
            std::fs::canonicalize(cwd.trim()).expect("a folder"),
            std::fs::canonicalize(&scratch).expect("a folder")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_signed_out_program_is_a_not_signed_in_error_of_the_request() {
        let dir = temp("signedout");
        std::fs::write(dir.join("out"), EXPIRED_STDOUT).expect("fixture");
        std::fs::write(dir.join("err"), EXPIRED_STDERR).expect("fixture");
        let program = stand_in(&dir, &format!("case \"$1\" in --probe) exit 0;; esac\ncat > /dev/null\ncat '{d}/out'\ncat '{d}/err' >&2\nexit 130", d = dir.display()));
        let err = run_with(program, None).expect_err("signed out");
        assert!(
            matches!(err, AgentError::NotLoggedIn { ref program, .. } if program == "gemini"),
            "{err:?}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_program_that_is_not_there_is_not_installed() {
        let err = run_with(PathBuf::from("/nonexistent/folder/gemini"), None).expect_err("missing");
        assert!(
            matches!(err, AgentError::NotInstalled { ref program, .. } if program == "gemini"),
            "{err:?}"
        );
    }
}

#[test]
fn the_backend_names_itself_by_its_model_or_its_default() {
    let mut backend = GeminiCli { config: config() };
    assert_eq!(
        backend.identity(),
        Identity {
            kind: BackendKind::GeminiCli,
            model: "gemini-2.5-flash".to_string()
        }
    );
    backend.config.model = String::new();
    assert_eq!(backend.identity().label(), "gemini CLI, its default model");
}
