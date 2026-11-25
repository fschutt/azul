//! The agents' settings: the defaults, the flat file they are kept in, the rows of the gear panel,
//! what is refused and what is held to a range, the key references (never a key), and the test
//! request. No window, no process, no environment: the environment is a map the test makes.

use std::sync::Arc;

use super::*;
use crate::agent_backend::{never_cancelled, run_task, Control, ManualClock};
use crate::agent_mock::{MockBackend, Step};
use crate::agent_review::Reply;

const EN: &str = include_str!("../resources/en.ftl");
const DE: &str = include_str!("../resources/de.ftl");

/// Keys as their issuers write them, made at run time: the repository holds no key-shaped text (the
/// public-tree check reads every file), and these are not keys of anybody.
fn anthropic_like() -> String {
    format!("sk-ant-api03-{}", "AbCdEf0123456789".repeat(3))
}

fn google_like() -> String {
    format!("AIza{}", "Sy-1234abcd".repeat(4))
}

fn github_like() -> String {
    format!("ghp_{}", "a1B2".repeat(9))
}

fn hex_like() -> String {
    "0123456789abcdef".repeat(2) + "01234567"
}

fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

// ==== defaults and the file ====

#[test]
fn every_task_starts_on_claude_print_with_the_agreed_models_and_two_agents_at_once() {
    let s = AgentSettings::default();
    for task in Task::all() {
        assert_eq!(s.task(task).backend, BackendKind::ClaudePrint, "{task:?}");
    }
    assert_eq!(s.task(Task::Editing).model, "haiku");
    assert_eq!(s.task(Task::Exploration).model, "sonnet");
    assert_eq!(s.task(Task::Export).model, "sonnet");
    assert_eq!(s.max_concurrent, 2);
    assert_eq!(s.limit_scale, 1);
    assert_eq!(s.task(Task::Editing).timeout_secs, 90);
    assert_eq!(s.task(Task::Editing).budget_cents, 30);
    assert!(s.validate().is_empty(), "{:?}", s.validate());
}

#[test]
fn the_file_has_every_row_of_every_task_and_the_two_global_settings() {
    let v = AgentSettings::default().to_values();
    for task in Task::all() {
        for spec in &FIELDS {
            let key = format!("agent-{}-{}", task.key(), spec.key);
            assert!(v.contains_key(&key), "{key}");
        }
    }
    assert_eq!(
        v.get("agent-editing-model").map(String::as_str),
        Some("haiku")
    );
    assert_eq!(v.get("agent-max-concurrent").map(String::as_str), Some("2"));
    assert_eq!(v.get("agent-limit-scale").map(String::as_str), Some("1"));
    assert_eq!(v.len(), Task::all().count() * FIELDS.len() + 2);
}

#[test]
fn settings_written_and_read_back_are_the_same_settings_without_a_complaint() {
    let mut s = AgentSettings::default();
    s.set(Task::Editing, "backend", "gemini-cli")
        .expect("backend");
    s.set(Task::Editing, "model", "gemini-2.5-pro")
        .expect("model");
    s.set(Task::Exploration, "timeout", "240").expect("timeout");
    s.set(Task::Exploration, "key", "env:ANTHROPIC_API_KEY")
        .expect("key");
    s.set(Task::Export, "args", "--effort low").expect("args");
    s.set(Task::Export, "program", "/opt/tools/claude")
        .expect("program");
    s.set_max_concurrent(3);
    s.set_limit_scale(2);
    let (again, problems) = AgentSettings::from_values(&s.to_values());
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(again, s);
}

#[test]
fn an_empty_file_is_the_defaults_and_other_settings_are_not_ours() {
    let (s, problems) = AgentSettings::from_values(&values(&[
        ("dictation-input", "pen"),
        ("last-folder", "/x"),
    ]));
    assert_eq!(s, AgentSettings::default());
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn a_setting_nobody_knows_is_a_note_and_the_rest_is_still_read() {
    let (s, problems) = AgentSettings::from_values(&values(&[
        ("agent-editing-flavour", "mint"),
        ("agent-weather-model", "x"),
        ("agent-editing-model", "opus"),
    ]));
    assert_eq!(s.task(Task::Editing).model, "opus");
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems.iter().all(|p| p.severity == Severity::Note));
}

#[test]
fn an_explicit_model_wins_over_the_default_of_the_backend_chosen_in_the_same_file() {
    let (s, problems) = AgentSettings::from_values(&values(&[
        ("agent-editing-model", "my-own-model"),
        ("agent-editing-backend", "gemini-api"),
    ]));
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(s.task(Task::Editing).backend, BackendKind::GeminiApi);
    assert_eq!(s.task(Task::Editing).model, "my-own-model");
}

#[test]
fn an_empty_model_in_the_file_means_the_backends_own_default_and_stays_empty() {
    let (s, _) = AgentSettings::from_values(&values(&[("agent-editing-model", "")]));
    assert_eq!(s.task(Task::Editing).model, "");
    assert_eq!(
        s.to_values().get("agent-editing-model").map(String::as_str),
        Some("")
    );
}

#[test]
fn what_cannot_be_read_keeps_the_default_and_is_said() {
    let (s, problems) = AgentSettings::from_values(&values(&[
        ("agent-editing-backend", "carrier-pigeon"),
        ("agent-editing-timeout", "soon"),
        ("agent-max-concurrent", "many"),
    ]));
    assert_eq!(s.task(Task::Editing).backend, BackendKind::ClaudePrint);
    assert_eq!(s.task(Task::Editing).timeout_secs, 90);
    assert_eq!(s.max_concurrent, 2);
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems.iter().all(|p| p.severity == Severity::Error));
    assert!(problems.iter().any(|p| p.kind
        == ProblemKind::UnknownBackend {
            word: "carrier-pigeon".to_string()
        }));
}

#[test]
fn a_number_outside_what_a_person_can_mean_is_held_and_it_is_said() {
    let (s, problems) = AgentSettings::from_values(&values(&[
        ("agent-editing-timeout", "99999"),
        ("agent-exploration-timeout", "0"),
        ("agent-export-max-tokens", "1"),
        ("agent-max-concurrent", "500"),
        ("agent-limit-scale", "0"),
    ]));
    assert_eq!(s.task(Task::Editing).timeout_secs, 1_800);
    assert_eq!(s.task(Task::Exploration).timeout_secs, 5);
    assert_eq!(s.task(Task::Export).max_output_tokens, 128);
    assert_eq!(s.max_concurrent, 8);
    assert_eq!(s.limit_scale, 1);
    assert_eq!(problems.len(), 5, "{problems:?}");
    assert!(problems.iter().all(|p| p.severity == Severity::Note));
    let first = problems
        .iter()
        .find(|p| p.field == "timeout" && p.task == Some(Task::Editing))
        .expect("a note");
    assert_eq!(
        first.kind,
        ProblemKind::OutOfRange {
            value: 99_999,
            min: 5,
            max: 1_800,
            used: 1_800
        }
    );
}

#[test]
fn the_limits_follow_the_scale() {
    let mut s = AgentSettings::default();
    assert_eq!(s.limits(), Limits::default());
    s.set_limit_scale(4);
    assert_eq!(
        s.limits().max_prompt_bytes,
        Limits::default().max_prompt_bytes * 4
    );
}

// ==== rows ====

#[test]
fn a_row_is_read_and_written_by_its_name() {
    let mut s = AgentSettings::default();
    assert_eq!(
        s.get(Task::Editing, "backend").as_deref(),
        Some("claude-print")
    );
    assert_eq!(s.get(Task::Editing, "timeout").as_deref(), Some("90"));
    assert_eq!(s.get(Task::Editing, "nonsense"), None);
    s.set(Task::Editing, "timeout", " 120 ").expect("a number");
    assert_eq!(s.task(Task::Editing).timeout_secs, 120);
    let err = s
        .set(Task::Editing, "nonsense", "x")
        .expect_err("no such row");
    assert!(
        matches!(err.kind, ProblemKind::UnknownField { .. }),
        "{err:?}"
    );
}

#[test]
fn a_number_that_is_held_comes_back_as_a_note_and_one_that_is_no_number_is_an_error() {
    let mut s = AgentSettings::default();
    let note = s
        .set(Task::Editing, "max-turns", "500")
        .expect("held")
        .expect("a note");
    assert_eq!(s.task(Task::Editing).max_turns, 100);
    assert_eq!(note.severity, Severity::Note);
    let err = s
        .set(Task::Editing, "max-turns", "ten")
        .expect_err("no number");
    assert_eq!(err.severity, Severity::Error);
    assert_eq!(
        s.task(Task::Editing).max_turns,
        100,
        "an error writes nothing"
    );
    assert!(
        s.set(Task::Editing, "max-turns", "-3").is_err(),
        "negative is no number here"
    );
}

#[test]
fn changing_the_backend_changes_a_model_that_was_the_old_defaults_and_keeps_one_the_user_chose() {
    let mut s = AgentSettings::default();
    s.set(Task::Editing, "backend", "gemini-api")
        .expect("backend");
    assert_eq!(s.task(Task::Editing).model, "gemini-2.5-flash");
    assert_eq!(
        s.task(Task::Editing).budget_cents,
        0,
        "a cost limit is a claude -p setting"
    );
    s.set(Task::Editing, "backend", "claude-print")
        .expect("backend");
    assert_eq!(
        s.task(Task::Editing).model,
        "haiku",
        "the old backend's default gives way to the new one's"
    );
    assert_eq!(
        s.task(Task::Editing).budget_cents,
        30,
        "and claude -p has its cost limit again"
    );
    let mut t = AgentSettings::default();
    t.set(Task::Editing, "model", "my-fine-tune")
        .expect("model");
    t.set(Task::Editing, "backend", "gemini-cli")
        .expect("backend");
    assert_eq!(t.task(Task::Editing).model, "my-fine-tune");
    let err = t
        .set(Task::Editing, "backend", "carrier-pigeon")
        .expect_err("unknown");
    assert!(matches!(err.kind, ProblemKind::UnknownBackend { .. }));
    assert_eq!(
        t.task(Task::Editing).backend,
        BackendKind::GeminiCli,
        "an error writes nothing"
    );
}

#[test]
fn a_model_name_is_trimmed_and_held_to_one_line_of_a_sane_length() {
    let mut s = AgentSettings::default();
    s.set(Task::Editing, "model", "  sonnet \n").expect("model");
    assert_eq!(s.task(Task::Editing).model, "sonnet");
    s.set(Task::Editing, "model", &"m".repeat(500))
        .expect("model");
    assert_eq!(s.task(Task::Editing).model.chars().count(), 120);
}

#[test]
fn an_address_is_an_http_address_without_credentials_or_a_query() {
    let mut s = AgentSettings::default();
    s.set(Task::Editing, "base-url", "https://proxy.example.com/")
        .expect("fine");
    assert_eq!(s.task(Task::Editing).base_url, "https://proxy.example.com");
    s.set(Task::Editing, "base-url", "http://127.0.0.1:8080")
        .expect("local mock");
    s.set(Task::Editing, "base-url", "")
        .expect("empty: the usual one");
    assert_eq!(s.task(Task::Editing).base_url, "");
    for bad in [
        "ftp://x.example",
        "example.com",
        "https://",
        "https://user:pw@host.example",
        "https://h.example/?key=abc",
        "https://a b.example",
    ] {
        let err = s.set(Task::Editing, "base-url", bad).expect_err(bad);
        assert!(
            matches!(err.kind, ProblemKind::UrlInvalid { .. }),
            "{bad}: {err:?}"
        );
    }
    assert_eq!(
        s.task(Task::Editing).base_url,
        "",
        "an error writes nothing"
    );
}

#[test]
fn extra_arguments_may_not_widen_what_the_agent_may_do() {
    let mut s = AgentSettings::default();
    s.set(
        Task::Editing,
        "args",
        "--effort low --append-system-prompt \"be brief\"",
    )
    .expect("fine");
    assert_eq!(
        s.task(Task::Editing).args(),
        vec!["--effort", "low", "--append-system-prompt", "be brief"]
    );
    for bad in [
        "--dangerously-skip-permissions",
        "--permission-mode=bypassPermissions",
        "--permission-mode plan",
        "--allowedTools Bash",
        "--tools Bash,Edit",
        "-y",
        "--yolo",
        "--approval-mode yolo",
        "--settings /tmp/x.json",
        "--effort low --ALLOWED-TOOLS Edit",
    ] {
        let err = s.set(Task::Editing, "args", bad).expect_err(bad);
        assert!(
            matches!(err.kind, ProblemKind::ArgForbidden { .. }),
            "{bad}: {err:?}"
        );
    }
    assert_eq!(
        s.task(Task::Editing).extra_args,
        "--effort low --append-system-prompt \"be brief\""
    );
}

#[test]
fn arguments_are_split_at_spaces_and_quotes_hold_spaces_together() {
    assert_eq!(split_args("a b  c"), vec!["a", "b", "c"]);
    assert_eq!(
        split_args("--x 'two words' \"and more\""),
        vec!["--x", "two words", "and more"]
    );
    assert_eq!(split_args("  "), Vec::<String>::new());
    assert_eq!(
        split_args("a \"\" b"),
        vec!["a", "", "b"],
        "an empty pair of quotes is an empty argument"
    );
    assert_eq!(
        split_args("it's"),
        vec!["its"],
        "a quote that is never closed runs to the end"
    );
}

#[test]
fn the_rows_of_a_backend_are_the_ones_that_matter_to_it() {
    let keys = |kind: BackendKind| fields_for(kind).iter().map(|f| f.key).collect::<Vec<_>>();
    assert_eq!(
        keys(BackendKind::ClaudePrint),
        vec![
            "backend",
            "model",
            "timeout",
            "budget",
            "max-turns",
            "effort",
            "key",
            "program",
            "args"
        ]
    );
    assert_eq!(
        keys(BackendKind::AnthropicApi),
        vec![
            "backend",
            "model",
            "timeout",
            "max-tokens",
            "effort",
            "key",
            "base-url"
        ]
    );
    assert_eq!(
        keys(BackendKind::GeminiCli),
        vec!["backend", "model", "timeout", "key", "program", "args"]
    );
    assert_eq!(keys(BackendKind::Mock), vec!["backend"]);
}

#[test]
fn every_row_has_a_label_and_a_hint_in_both_languages() {
    for spec in &FIELDS {
        assert!(spec.label.contains("-agent-field-") && spec.hint.contains("-agent-hint-"));
        for source in [EN, DE] {
            for key in [spec.label, spec.hint] {
                assert!(source.contains(&format!("{key} = ")), "{key}");
            }
        }
    }
    for global in &GLOBALS {
        for source in [EN, DE] {
            for key in [global.label, global.hint] {
                assert!(source.contains(&format!("{key} = ")), "{key}");
            }
        }
        assert!(global.min >= 1 && global.max <= 8 && global.min <= global.max);
    }
    assert_eq!(
        GLOBALS.iter().map(|g| g.key).collect::<Vec<_>>(),
        vec![KEY_MAX_CONCURRENT, KEY_LIMIT_SCALE]
    );
}

#[test]
fn the_models_suggested_are_aliases_for_the_claude_cli_and_data() {
    assert_eq!(
        presets(BackendKind::ClaudePrint),
        ["haiku", "sonnet", "opus", "fable"]
    );
    assert!(presets(BackendKind::GeminiCli).contains(&"gemini-2.5-pro"));
    for kind in BackendKind::ALL {
        let _ = presets(kind);
    }
    assert_eq!(
        default_model(Task::Editing, BackendKind::GeminiCli),
        "",
        "the CLI's own routing"
    );
}

#[test]
fn what_cannot_work_as_it_stands_is_found_by_validate() {
    let mut s = AgentSettings::default();
    s.set(Task::Editing, "backend", "anthropic-api")
        .expect("backend");
    assert_eq!(
        s.task(Task::Editing).model,
        "claude-haiku-5-5",
        "the API starts with the id of the same family"
    );
    assert!(s.validate().is_empty());
    s.set(Task::Editing, "model", "").expect("empty on purpose");
    let problems = s.validate();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].kind, ProblemKind::ModelRequired);
    assert_eq!(problems[0].path(), "editing.model");
    s.set(Task::Editing, "model", "some-model").expect("model");
    assert!(s.validate().is_empty());
    // a hand-made setting that bypassed `set`
    s.tasks.get_mut(&Task::Editing).expect("task").key = anthropic_like();
    assert_eq!(s.validate()[0].kind, ProblemKind::KeyLooksLikeSecret);
}

// ==== key references ====

#[test]
fn a_key_is_named_by_a_variable_or_a_keychain_item() {
    assert_eq!(
        KeyRef::parse("env:ANTHROPIC_API_KEY"),
        Ok(Some(KeyRef::Env("ANTHROPIC_API_KEY".to_string())))
    );
    assert_eq!(
        KeyRef::parse("ANTHROPIC_API_KEY"),
        Ok(Some(KeyRef::Env("ANTHROPIC_API_KEY".to_string())))
    );
    assert_eq!(
        KeyRef::parse(" keychain:azreview anthropic "),
        Ok(Some(KeyRef::Keychain("azreview anthropic".to_string())))
    );
    assert_eq!(KeyRef::parse(""), Ok(None));
    assert_eq!(KeyRef::parse("   "), Ok(None));
    assert_eq!(KeyRef::Env("X".to_string()).text(), "env:X");
    assert_eq!(KeyRef::Keychain("a b".to_string()).text(), "keychain:a b");
}

#[test]
fn what_is_neither_is_invalid() {
    for bad in [
        "env:bad name",
        "env:",
        "env:9LIVES",
        "keychain:",
        "keychain:a/b",
        "mykey",
        "some words",
        "file:/etc/key",
    ] {
        assert_eq!(KeyRef::parse(bad), Err(ProblemKind::KeyRefInvalid), "{bad}");
    }
}

#[test]
fn a_key_itself_is_refused_in_every_spelling() {
    for key in [
        anthropic_like(),
        google_like(),
        github_like(),
        hex_like(),
        format!("env:{}", anthropic_like()),
    ] {
        assert_eq!(
            KeyRef::parse(&key),
            Err(ProblemKind::KeyLooksLikeSecret),
            "{key}"
        );
        assert!(looks_like_secret(key.strip_prefix("env:").unwrap_or(&key)));
    }
}

#[test]
fn names_of_variables_and_items_are_not_taken_for_keys() {
    for name in [
        "ANTHROPIC_API_KEY",
        "env:MY_VERY_LONG_COMPANY_ANTHROPIC_API_KEY_FOR_PRODUCTION_USE_2026",
        "keychain:azreview anthropic key of the production account",
        "GEMINI_API_KEY",
    ] {
        assert!(KeyRef::parse(name).is_ok(), "{name}");
    }
    assert!(!looks_like_secret("claude-sonnet"));
    assert!(!looks_like_secret("short"));
}

#[test]
fn a_key_typed_into_the_row_is_not_kept_and_not_repeated() {
    let key = anthropic_like();
    let mut s = AgentSettings::default();
    let err = s.set(Task::Editing, "key", &key).expect_err("refused");
    assert_eq!(err.kind, ProblemKind::KeyLooksLikeSecret);
    assert!(!err.message().contains("sk-ant"), "{}", err.message());
    assert!(!format!("{err:?}").contains("sk-ant"), "{err:?}");
    assert_eq!(s.task(Task::Editing).key, "");
    let (read, problems) = AgentSettings::from_values(&values(&[("agent-editing-key", &key)]));
    assert_eq!(
        read.task(Task::Editing).key,
        "",
        "a key found in a file is dropped"
    );
    assert_eq!(problems.len(), 1);
    assert!(!problems[0].message().contains("sk-ant"));
}

#[test]
fn a_reference_is_kept_in_its_written_form() {
    let mut s = AgentSettings::default();
    s.set(Task::Editing, "key", "  GEMINI_API_KEY ")
        .expect("a bare name");
    assert_eq!(s.task(Task::Editing).key, "env:GEMINI_API_KEY");
    s.set(Task::Editing, "key", "").expect("no key");
    assert_eq!(s.task(Task::Editing).key, "");
    assert_eq!(s.task(Task::Editing).key_ref(), Ok(None));
}

#[test]
fn the_key_is_read_from_the_environment_the_app_took_at_its_start() {
    let keys = Keys {
        env: Env::from_pairs([
            ("A_KEY".to_string(), " secret-value\n".to_string()),
            ("EMPTY".to_string(), "  ".to_string()),
        ]),
        keychain: None,
    };
    let key = keys
        .resolve(&KeyRef::Env("A_KEY".to_string()))
        .expect("set");
    assert_eq!(key.expose(), "secret-value", "trimmed");
    assert_eq!(
        keys.resolve(&KeyRef::Env("NOPE".to_string()))
            .expect_err("missing"),
        AgentError::KeyMissing {
            reference: "env:NOPE".to_string()
        }
    );
    assert!(
        matches!(
            keys.resolve(&KeyRef::Env("EMPTY".to_string())),
            Err(AgentError::KeyMissing { .. })
        ),
        "blank is missing"
    );
}

struct FakeKeychain(Result<Option<String>, String>);

impl Keychain for FakeKeychain {
    fn read(&self, _item: &str) -> Result<Option<String>, String> {
        self.0.clone()
    }
}

#[test]
fn the_key_is_read_from_the_keychain_when_it_is_there_and_the_failures_are_told_apart() {
    let with = |answer: Result<Option<String>, String>| Keys {
        env: Env::default(),
        keychain: Some(Arc::new(FakeKeychain(answer))),
    };
    let item = KeyRef::Keychain("azreview".to_string());
    assert_eq!(
        with(Ok(Some("k-123456".to_string())))
            .resolve(&item)
            .expect("there")
            .expose(),
        "k-123456"
    );
    assert!(matches!(
        with(Ok(None)).resolve(&item),
        Err(AgentError::KeyMissing { .. })
    ));
    match with(Err("user refused access key=abcdef".to_string())).resolve(&item) {
        Err(AgentError::KeyDenied { reference, why }) => {
            assert_eq!(reference, "keychain:azreview");
            assert!(!why.contains("abcdef"), "scrubbed: {why}");
        }
        other => panic!("{other:?}"),
    }
    assert!(
        matches!(
            Keys::default().resolve(&item),
            Err(AgentError::Unsupported { .. })
        ),
        "no keychain here"
    );
}

#[test]
fn a_secret_never_shows_in_its_debug_text() {
    let secret = Secret::new("hunter2-hunter2");
    assert_eq!(format!("{secret:?}"), "Secret(***)");
    let keys = Keys {
        env: Env::from_pairs([("K".to_string(), "hunter2-hunter2".to_string())]),
        keychain: None,
    };
    assert!(!format!("{keys:?}").contains("hunter2"));
    assert_eq!(keys.env.iter().count(), 1);
}

// ==== problems in words ====

fn problem_samples() -> Vec<Problem> {
    let p = |kind| Problem {
        task: Some(Task::Editing),
        field: "model",
        severity: Severity::Error,
        kind,
    };
    vec![
        p(ProblemKind::UnknownBackend {
            word: "pigeon".to_string(),
        }),
        p(ProblemKind::NotANumber {
            word: "soon".to_string(),
        }),
        p(ProblemKind::NotAChoice {
            word: "turbo".to_string(),
            choices: "low, medium, high, xhigh, max".to_string(),
        }),
        p(ProblemKind::OutOfRange {
            value: 99_999,
            min: 5,
            max: 1_800,
            used: 1_800,
        }),
        p(ProblemKind::KeyRefInvalid),
        p(ProblemKind::KeyLooksLikeSecret),
        p(ProblemKind::ArgForbidden {
            arg: "--yolo".to_string(),
        }),
        p(ProblemKind::ModelRequired),
        p(ProblemKind::UrlInvalid {
            text: "ftp://x".to_string(),
        }),
        Problem {
            task: None,
            field: "field",
            severity: Severity::Note,
            kind: ProblemKind::UnknownField {
                word: "agent-x".to_string(),
            },
        },
    ]
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
fn the_english_resource_says_exactly_what_a_problem_says() {
    for p in problem_samples() {
        let said = ftl(EN, p.l10n_key(), &p.l10n_args())
            .unwrap_or_else(|| panic!("{} missing in en.ftl", p.l10n_key()));
        assert_eq!(said, p.message(), "{}", p.l10n_key());
    }
}

#[test]
fn the_german_resource_has_every_problem_with_the_same_placeholders() {
    for p in problem_samples() {
        let key = p.l10n_key();
        let de = DE
            .lines()
            .find(|l| l.starts_with(&format!("{key} = ")))
            .unwrap_or_else(|| panic!("{key} missing in de.ftl"));
        for (name, _) in p.l10n_args() {
            assert!(
                de.contains(&format!("{{ ${name} }}")),
                "{key} lacks {name} in German: {de}"
            );
        }
    }
}

#[test]
fn a_problem_is_named_by_its_task_and_row() {
    let p = &problem_samples()[0];
    assert_eq!(p.path(), "editing.model");
    assert_eq!(problem_samples().last().expect("a sample").path(), "field");
}

// ==== test this backend ====

#[test]
fn the_test_request_is_a_tiny_editing_request_that_builds() {
    let request = test_request("de");
    assert_eq!(request.task(), Task::Editing);
    let Request::Review(review) = &request;
    let call =
        crate::agent_review::build_call(&request, review, &Limits::default()).expect("builds");
    assert!(call.prompt.len() < 800, "{}", call.prompt.len());
    assert!(call.prompt.contains("connection test"));
}

#[test]
fn a_test_that_worked_reports_the_latency_and_the_first_words() {
    let mut mock = MockBackend::scripted(vec![Step::reply(
        420,
        &Reply {
            answer: "ok".to_string(),
            ..Reply::default()
        },
    )]);
    let mut clock = ManualClock::new();
    let mut ctl = Control::new(&mut clock, &never_cancelled);
    let result = run_task(&mut mock, &test_request("en"), &Limits::default(), &mut ctl);
    let report = test_report(
        &result,
        &TaskSettings::defaults(Task::Editing, BackendKind::Mock).identity(),
    );
    assert!(report.ok);
    assert_eq!(report.latency_ms, 420);
    assert_eq!(report.first_words, "ok");
    assert_eq!(report.identity.kind, BackendKind::Mock);
    assert_eq!(report.error, None);
}

#[test]
fn a_test_that_failed_says_who_was_asked_and_why_it_failed() {
    let asked = TaskSettings::defaults(Task::Editing, BackendKind::ClaudePrint).identity();
    let result = Err(AgentError::NotInstalled {
        program: "claude".to_string(),
        searched: vec!["/usr/bin".to_string()],
    });
    let report = test_report(&result, &asked);
    assert!(!report.ok);
    assert_eq!(report.identity, asked);
    assert!(matches!(
        report.error,
        Some(AgentError::NotInstalled { .. })
    ));
    assert_eq!(report.first_words, "");
}

#[test]
fn a_task_settings_identity_is_its_backend_and_trimmed_model() {
    let mut t = TaskSettings::defaults(Task::Editing, BackendKind::ClaudePrint);
    t.model = " haiku ".to_string();
    assert_eq!(
        t.identity(),
        Identity {
            kind: BackendKind::ClaudePrint,
            model: "haiku".to_string()
        }
    );
}

#[test]
fn the_effort_is_one_of_the_levels_or_empty_and_its_case_does_not_matter() {
    let mut s = AgentSettings::default();
    assert_eq!(
        s.get(Task::Editing, "effort").as_deref(),
        Some(""),
        "the model's own default"
    );
    s.set(Task::Editing, "effort", " Low ").expect("a level");
    assert_eq!(s.task(Task::Editing).effort, "low");
    s.set(Task::Editing, "effort", "").expect("none");
    assert_eq!(s.task(Task::Editing).effort, "");
    let err = s
        .set(Task::Editing, "effort", "turbo")
        .expect_err("no such level");
    assert_eq!(
        err.kind,
        ProblemKind::NotAChoice {
            word: "turbo".to_string(),
            choices: "low, medium, high, xhigh, max".to_string()
        }
    );
    assert_eq!(s.task(Task::Editing).effort, "", "an error writes nothing");
    s.set(Task::Exploration, "effort", "xhigh")
        .expect("a level");
    let (again, problems) = AgentSettings::from_values(&s.to_values());
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(again.task(Task::Exploration).effort, "xhigh");
}

#[test]
fn the_apis_ids_follow_the_aliases_of_the_claude_cli() {
    assert_eq!(api_model("haiku"), "claude-haiku-5-5");
    assert_eq!(api_model("sonnet"), "claude-sonnet-5-5");
    assert_eq!(
        api_model("something-else"),
        "something-else",
        "an id given as it is stays"
    );
    for task in Task::all() {
        let id = default_model(task, BackendKind::AnthropicApi);
        assert!(id.starts_with("claude-"), "{id}");
        assert!(presets(BackendKind::AnthropicApi).contains(&id), "{id}");
    }
}
