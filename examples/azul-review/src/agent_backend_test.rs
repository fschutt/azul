//! The boundary of the agents: the tasks table, the backend kinds, the clocks, the failures in words
//! and in the .ftl files, the scrubbing of secrets, and the three steps of `run_task` with a backend
//! that answers from a script. No process, no socket, no window.

use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

use super::*;
use crate::{
    agent_review::{Context, ReviewRequest, Speaker, Turn},
    agent_stroke::{Meaning, Sample, StrokeInput},
};

const EN: &str = include_str!("../resources/en.ftl");
const DE: &str = include_str!("../resources/de.ftl");

// ==== tasks and backends ====

#[test]
fn every_task_has_one_row_with_a_unique_key_and_the_keys_read_back() {
    let all: Vec<Task> = Task::all().collect();
    assert_eq!(all, vec![Task::Exploration, Task::Editing, Task::Export]);
    let mut keys: Vec<&str> = all.iter().map(|t| t.key()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), TASKS.len(), "unique keys");
    for task in all {
        assert_eq!(task.spec().task, task);
        assert_eq!(Task::from_key(task.key()), Some(task));
        assert!(task.spec().label.contains("-agent-task-"));
    }
    assert_eq!(Task::from_key(" Editing "), Some(Task::Editing));
    assert_eq!(Task::from_key("nonsense"), None);
}

#[test]
fn the_tasks_start_with_the_defaults_the_user_agreed_to() {
    assert_eq!(Task::Editing.spec().claude_model, "haiku");
    assert_eq!(Task::Exploration.spec().claude_model, "sonnet");
    assert_eq!(Task::Export.spec().claude_model, "sonnet");
    assert_eq!(Task::Exploration.spec().tools, Tools::ReadOnly);
    assert_eq!(Task::Editing.spec().tools, Tools::None);
    assert_eq!(Task::Export.spec().tools, Tools::None);
    for spec in &TASKS {
        assert!(
            spec.timeout_secs >= 30 && spec.timeout_secs <= 900,
            "{:?}",
            spec.task
        );
        assert!(spec.max_output_tokens >= 512);
    }
}

#[test]
fn the_backend_kinds_have_words_labels_and_what_they_need() {
    for kind in BackendKind::ALL {
        assert_eq!(BackendKind::from_key(kind.key()), Some(kind));
        assert!(kind.label().contains("-agent-backend-"));
        assert!(!kind.english().is_empty());
        assert_eq!(kind.is_process(), kind.program().is_some());
        assert_eq!(kind.is_http(), kind.default_base_url().is_some());
    }
    assert_eq!(
        BackendKind::from_key(" Claude-Print "),
        Some(BackendKind::ClaudePrint)
    );
    assert_eq!(BackendKind::from_key("claude"), None);
    assert_eq!(BackendKind::ClaudePrint.program(), Some("claude"));
    assert_eq!(BackendKind::GeminiCli.program(), Some("gemini"));
    assert_eq!(
        BackendKind::AnthropicApi.default_base_url(),
        Some("https://api.anthropic.com")
    );
    assert_eq!(
        BackendKind::GeminiApi.default_base_url(),
        Some("https://generativelanguage.googleapis.com")
    );
    assert!(!BackendKind::Mock.takes_key());
    assert!(BackendKind::AnthropicApi.takes_key() && BackendKind::ClaudePrint.takes_key());
}

#[test]
fn an_identity_names_the_backend_and_the_model_or_its_default() {
    let id = Identity {
        kind: BackendKind::ClaudePrint,
        model: "haiku".to_string(),
    };
    assert_eq!(id.label(), "claude -p, haiku");
    let id = Identity {
        kind: BackendKind::GeminiCli,
        model: "  ".to_string(),
    };
    assert_eq!(id.label(), "gemini CLI, its default model");
}

// ==== clocks and control ====

#[test]
fn a_manual_clock_moves_only_when_something_sleeps_on_it() {
    let mut clock = ManualClock::new();
    assert_eq!(clock.now_ms(), 0);
    clock.sleep_ms(1_500);
    clock.advance(500);
    assert_eq!(clock.now_ms(), 2_000);
}

#[test]
fn a_manual_clock_with_a_real_pause_lets_real_time_pass_too() {
    let mut clock = ManualClock::with_real_pause(15);
    let began = std::time::Instant::now();
    clock.sleep_ms(1_000);
    assert!(began.elapsed().as_millis() >= 15);
    assert_eq!(clock.now_ms(), 1_000);
}

#[test]
fn the_wall_clock_counts_up_and_sleeps() {
    let mut clock = WallClock::new();
    let a = clock.now_ms();
    clock.sleep_ms(5);
    assert!(clock.now_ms() >= a + 5);
}

#[test]
fn control_asks_the_window_whether_the_call_was_cancelled() {
    let flag = AtomicBool::new(false);
    let check = || flag.load(Ordering::Relaxed);
    let mut clock = ManualClock::new();
    let mut ctl = Control::new(&mut clock, &check);
    assert!(!ctl.cancelled());
    flag.store(true, Ordering::Relaxed);
    assert!(ctl.cancelled());
    ctl.sleep_ms(10);
    assert_eq!(ctl.now_ms(), 10);
    assert!(!never_cancelled());
}

// ==== failures in words ====

fn samples() -> Vec<AgentError> {
    vec![
        AgentError::NotInstalled {
            program: "claude".to_string(),
            searched: vec!["/usr/bin".to_string(), "/Users/x/.local/bin".to_string()],
        },
        AgentError::NotLoggedIn {
            program: "claude".to_string(),
            detail: "Not logged in".to_string(),
        },
        AgentError::KeyMissing {
            reference: "env:ANTHROPIC_API_KEY".to_string(),
        },
        AgentError::KeyDenied {
            reference: "keychain:azreview".to_string(),
            why: "access refused".to_string(),
        },
        AgentError::Timeout { secs: 90 },
        AgentError::Cancelled,
        AgentError::Busy { running: 2, cap: 2 },
        AgentError::Spawn {
            program: "gemini".to_string(),
            why: "permission denied".to_string(),
        },
        AgentError::Exit {
            program: "claude".to_string(),
            code: Some(2),
            detail: "bad flag".to_string(),
        },
        AgentError::Http {
            status: 401,
            kind: "authentication_error".to_string(),
            detail: "invalid x-api-key".to_string(),
        },
        AgentError::RateLimited {
            retry_after_secs: Some(20),
            detail: "429 too many requests".to_string(),
        },
        AgentError::Network {
            why: "connection refused".to_string(),
        },
        AgentError::Budget { limit_usd: 0.3 },
        AgentError::MaxTurns { turns: 12 },
        AgentError::Refused {
            detail: "safety".to_string(),
        },
        AgentError::BadOutput {
            why: "no JSON".to_string(),
            excerpt: "I cannot".to_string(),
        },
        AgentError::TooLarge {
            what: "the prompt".to_string(),
            size: 90_000,
            limit: 65_536,
        },
        AgentError::Settings {
            why: "no model".to_string(),
        },
        AgentError::Unsupported {
            why: "no keychain here".to_string(),
        },
    ]
}

#[test]
fn every_failure_says_what_happened_in_words_the_user_can_act_on() {
    let said: Vec<String> = samples().iter().map(AgentError::message).collect();
    assert_eq!(
        said[0],
        "claude is not installed (looked in /usr/bin, /Users/x/.local/bin)"
    );
    assert_eq!(said[4], "Timeout after 90 s");
    assert_eq!(said[5], "Cancelled");
    assert_eq!(
        said[6],
        "2 agents are working already (at most 2 at once): wait for one to finish"
    );
    assert_eq!(said[8], "claude stopped with exit code 2: bad flag");
    assert_eq!(
        said[12],
        "The call would cost more than the limit of $0.30: raise it in the settings"
    );
    assert!(
        said[1].contains("not signed in") && said[1].contains("run claude"),
        "{}",
        said[1]
    );
    assert!(said[2].contains("never into the settings"), "{}", said[2]);
    for s in &said {
        assert!(!s.contains('{') && !s.contains('}'), "{s}");
        assert!(!s.is_empty());
    }
}

#[test]
fn a_program_stopped_by_a_signal_has_no_exit_code_and_says_a_dash() {
    let e = AgentError::Exit {
        program: "gemini".to_string(),
        code: None,
        detail: String::new(),
    };
    assert!(e.message().contains("exit code -"), "{}", e.message());
}

#[test]
fn a_missing_program_with_no_places_listed_names_the_path() {
    let e = AgentError::NotInstalled {
        program: "gemini".to_string(),
        searched: Vec::new(),
    };
    assert_eq!(e.message(), "gemini is not installed (looked in the PATH)");
}

#[test]
fn the_kind_names_and_keys_are_unique_and_stable() {
    let errors = samples();
    let mut keys: Vec<&str> = errors.iter().map(AgentError::l10n_key).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), errors.len());
    assert!(errors
        .iter()
        .all(|e| e.l10n_key().contains("-agent-error-")));
    assert_eq!(errors[4].kind_name(), "timeout");
    assert_eq!(errors[0].kind_name(), "not-installed");
    assert_eq!(errors[15].kind_name(), "bad-output");
}

#[test]
fn only_what_may_work_the_next_time_is_retryable() {
    let errors = samples();
    let retryable: Vec<&str> = errors
        .iter()
        .filter(|e| e.is_retryable())
        .map(AgentError::kind_name)
        .collect();
    assert_eq!(
        retryable,
        vec!["timeout", "busy", "rate-limited", "network"]
    );
    let server = AgentError::Http {
        status: 529,
        kind: "overloaded_error".to_string(),
        detail: String::new(),
    };
    assert!(server.is_retryable());
    assert_eq!(errors[10].retry_after_secs(), Some(20));
    assert_eq!(errors[4].retry_after_secs(), None);
}

#[test]
fn a_failure_displays_as_its_message() {
    for e in samples() {
        assert_eq!(e.to_string(), e.message());
    }
}

/// The message `key` of a Fluent source with its placeables filled in (single-line messages).
fn ftl(source: &str, key: &str, args: &[(&'static str, String)]) -> Option<String> {
    let line = source
        .lines()
        .find(|l| l.starts_with(&format!("{key} = ")))?;
    let mut text = line[key.len() + 3..].to_string();
    for (name, value) in args {
        text = text.replace(&format!("{{ ${name} }}"), value);
    }
    Some(text)
}

#[test]
fn the_english_resource_says_exactly_what_the_message_says() {
    for e in samples() {
        let said = ftl(EN, e.l10n_key(), &e.l10n_args())
            .unwrap_or_else(|| panic!("{} missing in en.ftl", e.l10n_key()));
        assert_eq!(said, e.message(), "{}", e.l10n_key());
    }
}

#[test]
fn the_german_resource_has_every_failure_with_the_same_placeholders() {
    for e in samples() {
        let key = e.l10n_key();
        let de = DE
            .lines()
            .find(|l| l.starts_with(&format!("{key} = ")))
            .unwrap_or_else(|| panic!("{key} missing in de.ftl"));
        for (name, _) in e.l10n_args() {
            assert!(
                de.contains(&format!("{{ ${name} }}")),
                "{key} lacks {name} in German: {de}"
            );
        }
        assert!(!de.contains(" is "), "{key} looks untranslated: {de}");
    }
}

#[test]
fn the_names_of_tasks_and_backends_are_in_both_resources() {
    for spec in &TASKS {
        for source in [EN, DE] {
            assert!(
                source.contains(&format!("{} = ", spec.label)),
                "{}",
                spec.label
            );
        }
    }
    for kind in BackendKind::ALL {
        for source in [EN, DE] {
            assert!(
                source.contains(&format!("{} = ", kind.label())),
                "{}",
                kind.label()
            );
        }
    }
}

// ==== secrets in text ====

#[test]
fn a_key_the_caller_holds_is_taken_out_wherever_it_stands() {
    let key = "sk-live-0123456789abcdef";
    let said = scrub(&format!("error: {key} was refused; again {key}"), &[key]);
    assert_eq!(said, "error: *** was refused; again ***");
    assert_eq!(
        scrub("short", &["abc"]),
        "short",
        "a secret under six characters is not looked for"
    );
}

#[test]
fn what_looks_like_a_services_token_is_taken_out() {
    let said = scrub(
        "key sk-ant-api03-AbCdEfGhIjKlMnOp_qrst and AIzaSyA1234567890abcdefghij ok",
        &[],
    );
    assert_eq!(said, "key *** and *** ok");
    assert_eq!(scrub("a short sk-abc token", &[]), "a short sk-abc token");
    assert_eq!(scrub("ghp_0123456789abcdefghij", &[]), "***");
}

#[test]
fn the_value_after_bearer_a_key_name_or_a_header_is_taken_out() {
    assert_eq!(
        scrub("Authorization: Bearer abcDEF123456", &[]),
        "Authorization: ***"
    );
    assert_eq!(
        scrub("sent Bearer xyz9876543210 there", &[]),
        "sent Bearer *** there"
    );
    assert_eq!(
        scrub("GET /v1/models?key=SECRET123&alt=json HTTP/1.1", &[]),
        "GET /v1/models?key=***&alt=json HTTP/1.1"
    );
    assert_eq!(
        scrub("x-api-key: abc123456\nnext line", &[]),
        "x-api-key: ***\nnext line"
    );
    assert_eq!(scrub("x-goog-api-key:abc123456", &[]), "x-goog-api-key:***");
    assert_eq!(
        scrub("password=hunter2 user=ann", &[]),
        "password=*** user=ann"
    );
}

#[test]
fn ordinary_messages_and_other_scripts_pass_through_untouched() {
    for text in [
        "exit code 2: no such file src/lib.rs",
        "Connection refused (os error 61)",
        "ünïcödé text with emoji 😀 and keys of a map",
        "the monkey=banana",
        "",
    ] {
        assert_eq!(scrub(text, &[]), text);
    }
}

// ==== run_task ====

fn request(task: Task) -> Request {
    Request::Review(ReviewRequest { symbols: String::new(),
        task,
        conversation: 9,
        language: "en".to_string(),
        utterances: Vec::new(),
        strokes: vec![StrokeInput {
            id: 1,
            page: 0,
            meaning: Some(Meaning::Issue),
            pinned: true,
            nib: "pen".to_string(),
            nib_width: 2.0,
            t0_ms: 0,
            samples: vec![
                Sample {
                    x: 0.0,
                    y: 0.0,
                    pressure: 0.5,
                    dt_ms: 0,
                    tilt: None,
                },
                Sample {
                    x: 40.0,
                    y: 0.0,
                    pressure: 0.5,
                    dt_ms: 40,
                    tilt: None,
                },
            ],
        }],
        context: Context {
            file: "a.rs".to_string(),
            page: 0,
            first_line: 1,
            lines: vec!["fn a() {}".to_string(), "fn b() {}".to_string()],
            total_lines: 2,
            focus: Some((1, 1)),
            words_under: Vec::new(),
            grid: None,
        },
        turns: vec![Turn {
            from: Speaker::Reviewer,
            text: "hi".to_string(),
        }],
        workspace: None,
    })
}

/// A backend that answers from a script and takes `takes_ms` of its clock.
struct Scripted {
    answer: Result<String, AgentError>,
    takes_ms: u64,
    prompts: Arc<AtomicUsize>,
}

impl Backend for Scripted {
    fn identity(&self) -> Identity {
        Identity {
            kind: BackendKind::Mock,
            model: "script".to_string(),
        }
    }

    fn complete(
        &mut self,
        _call: &Call<'_>,
        ctl: &mut Control<'_>,
    ) -> Result<Completion, AgentError> {
        self.prompts.fetch_add(1, Ordering::Relaxed);
        ctl.sleep_ms(self.takes_ms);
        self.answer.clone().map(|text| Completion {
            text,
            usage: Usage {
                input_tokens: Some(100),
                output_tokens: Some(20),
                ..Usage::default()
            },
            model: "script-1".to_string(),
            notes: Vec::new(),
        })
    }
}

fn run(
    answer: Result<String, AgentError>,
    takes_ms: u64,
    cancelled: bool,
) -> (Result<Response, AgentError>, usize) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut backend = Scripted {
        answer,
        takes_ms,
        prompts: calls.clone(),
    };
    let mut clock = ManualClock::new();
    let cancel = move || cancelled;
    let mut ctl = Control::new(&mut clock, &cancel);
    let result = run_task(
        &mut backend,
        &request(Task::Editing),
        &Limits::default(),
        &mut ctl,
    );
    (result, calls.load(Ordering::Relaxed))
}

#[test]
fn the_three_steps_build_send_and_read_and_the_response_says_who_and_how_long() {
    let (result, calls) = run(
        Ok(r#"{"answer": "fine", "suggestions": []}"#.to_string()),
        2_500,
        false,
    );
    let response = result.expect("answered");
    assert_eq!(calls, 1);
    assert_eq!(response.reply().map(|r| r.answer.as_str()), Some("fine"));
    assert_eq!(response.identity.kind, BackendKind::Mock);
    assert_eq!(response.usage.elapsed_ms, 2_500);
    assert_eq!(response.usage.input_tokens, Some(100));
    assert_eq!(response.first_words(40), "fine");
}

#[test]
fn a_call_cancelled_before_it_starts_never_reaches_the_backend() {
    let (result, calls) = run(Ok("{}".to_string()), 10, true);
    assert_eq!(result.expect_err("cancelled"), AgentError::Cancelled);
    assert_eq!(calls, 0);
}

#[test]
fn a_backend_failure_is_the_failure_of_the_request() {
    let (result, _) = run(Err(AgentError::Timeout { secs: 5 }), 5_000, false);
    assert_eq!(result.expect_err("failed"), AgentError::Timeout { secs: 5 });
}

#[test]
fn an_answer_that_cannot_be_read_is_a_bad_output_of_the_request() {
    let (result, _) = run(Ok("{\"answer\": \"cut".to_string()), 10, false);
    assert!(
        matches!(result, Err(AgentError::BadOutput { .. })),
        "{result:?}"
    );
}

#[test]
fn the_notes_of_the_build_and_of_the_reading_come_back_together() {
    let reply = r#"{"answer": "x", "suggestions": [{"kind": "teleport"}, {"kind": "insert_lines", "after_line": 1, "lines": ["// a"]}]}"#;
    let (result, _) = run(Ok(reply.to_string()), 10, false);
    let response = result.expect("answered");
    assert!(
        response.diagnostics.iter().any(|n| n.contains("teleport")),
        "{:?}",
        response.diagnostics
    );
}

#[test]
fn the_first_words_of_an_answer_without_text_count_its_suggestions() {
    let reply = r#"{"answer": "", "suggestions": [{"kind": "insert_lines", "after_line": 1, "lines": ["// a"]}]}"#;
    let response = run(Ok(reply.to_string()), 1, false).0.expect("answered");
    assert_eq!(response.first_words(40), "1 suggestion(s)");
}

#[test]
fn the_first_words_of_an_answer_that_does_nothing_say_so_and_why() {
    let reply = r#"{"action": "none", "reason": "a test phrase"}"#;
    let response = run(Ok(reply.to_string()), 1, false).0.expect("answered");
    assert_eq!(response.first_words(40), "no action: a test phrase");
    assert!(response.reply().is_some_and(|r| r.is_no_action()));
}

#[test]
fn a_request_knows_its_task_and_its_conversation() {
    let r = request(Task::Exploration);
    assert_eq!(r.task(), Task::Exploration);
    assert_eq!(r.conversation(), 9);
}
