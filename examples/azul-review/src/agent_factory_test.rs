//! Making a backend from settings: the program is looked for, the key is resolved, a refused
//! argument stays refused, and the factory is something a worker can run. No process, no network.

use std::sync::Arc;

use super::*;
use crate::{
    agent_api::Transport,
    agent_backend::{never_cancelled, run_task, Control, Identity, ManualClock, Request},
    agent_review::{Context, ReviewRequest, Utterance},
    agent_settings::{Env, Keychain},
};

fn has_claude(path: &Path) -> bool {
    path == Path::new("/Users/ann/.local/bin/claude") || path == Path::new("/opt/tools/claude")
}

fn has_nothing(_: &Path) -> bool {
    false
}

fn wiring_with(env: &[(&str, &str)], exists: fn(&Path) -> bool) -> Wiring {
    let keys = Keys {
        env: Env::from_pairs(
            env.iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
        ),
        keychain: None,
    };
    let mut w = Wiring::new(
        keys,
        Host::new(Some("/usr/bin:/bin"), Some("/Users/ann")),
        Limits::default(),
    );
    w.exists = exists;
    w
}

fn claude_settings() -> TaskSettings {
    TaskSettings::defaults(Task::Editing, BackendKind::ClaudePrint)
}

#[test]
fn claude_is_found_in_the_usual_places_and_gets_the_settings_as_they_are() {
    let w = wiring_with(&[], has_claude);
    let mut s = claude_settings();
    s.max_turns = 5;
    s.extra_args = "--effort low".to_string();
    let config = claude_config(&s, &w).expect("found");
    assert_eq!(
        config.program,
        PathBuf::from("/Users/ann/.local/bin/claude")
    );
    assert_eq!(
        (
            config.model.as_str(),
            config.timeout_secs,
            config.budget_cents,
            config.max_turns
        ),
        ("haiku", 90, 30, 5)
    );
    assert_eq!(config.extra_args, vec!["--effort", "low"]);
    assert_eq!(config.key, None, "no key named: the program's own sign-in");
    assert_eq!(config.limits, Limits::default());
}

#[test]
fn a_program_that_is_not_installed_says_where_it_was_looked_for() {
    let w = wiring_with(&[], has_nothing);
    match build_backend(Task::Editing, &claude_settings(), &w) {
        Err(AgentError::NotInstalled { program, searched }) => {
            assert_eq!(program, "claude");
            assert_eq!(searched.first().map(String::as_str), Some("/usr/bin"));
            assert!(
                searched.contains(&"/Users/ann/.local/bin".to_string()),
                "{searched:?}"
            );
        }
        other => panic!("{:?}", other.map(|b| b.identity())),
    }
}

#[test]
fn a_configured_path_is_used_and_not_searched_for_elsewhere() {
    let w = wiring_with(&[], has_claude);
    let mut s = claude_settings();
    s.program = "/opt/tools/claude".to_string();
    assert_eq!(
        claude_config(&s, &w).expect("found").program,
        PathBuf::from("/opt/tools/claude")
    );
    s.program = "/opt/other/claude".to_string();
    match claude_config(&s, &w) {
        Err(AgentError::NotInstalled { searched, .. }) => {
            assert_eq!(searched, vec!["/opt/other/claude"])
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_key_that_is_named_is_resolved_and_one_that_is_missing_is_a_failure_not_a_fall_back() {
    let mut s = claude_settings();
    s.key = "env:MY_KEY".to_string();
    let w = wiring_with(&[("MY_KEY", " the-key ")], has_claude);
    assert_eq!(
        claude_config(&s, &w).expect("resolved").key,
        Some(Secret::new("the-key"))
    );
    let w = wiring_with(&[], has_claude);
    assert_eq!(
        claude_config(&s, &w).expect_err("missing"),
        AgentError::KeyMissing {
            reference: "env:MY_KEY".to_string()
        }
    );
}

#[test]
fn a_keychain_reference_without_a_keychain_is_unsupported_and_with_one_is_read() {
    struct One;
    impl Keychain for One {
        fn read(&self, item: &str) -> Result<Option<String>, String> {
            Ok((item == "azreview").then(|| "from-the-keychain".to_string()))
        }
    }
    let mut s = claude_settings();
    s.key = "keychain:azreview".to_string();
    let w = wiring_with(&[], has_claude);
    assert!(matches!(
        claude_config(&s, &w),
        Err(AgentError::Unsupported { .. })
    ));
    let mut w = wiring_with(&[], has_claude);
    w.keys.keychain = Some(Arc::new(One));
    assert_eq!(
        claude_config(&s, &w).expect("read").key,
        Some(Secret::new("from-the-keychain"))
    );
}

#[test]
fn a_setting_a_hand_edited_file_got_past_the_panel_with_is_refused_here() {
    let w = wiring_with(&[], has_claude);
    let mut s = claude_settings();
    s.extra_args = "--dangerously-skip-permissions".to_string();
    match claude_config(&s, &w) {
        Err(AgentError::Settings { why }) => {
            assert!(why.contains("--dangerously-skip-permissions"), "{why}")
        }
        other => panic!("{other:?}"),
    }
    let mut s = claude_settings();
    s.key = "not a reference".to_string();
    assert!(matches!(
        claude_config(&s, &w),
        Err(AgentError::Settings { .. })
    ));
}

#[test]
fn gemini_gets_its_scratch_folder_made_and_its_model() {
    fn has_gemini(path: &Path) -> bool {
        path == Path::new("/opt/homebrew/bin/gemini")
    }
    let dir = std::env::temp_dir().join(format!("azreview-agent-factory-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut w = wiring_with(&[], has_gemini);
    w.scratch = Some(dir.join("scratch"));
    let s = TaskSettings::defaults(Task::Editing, BackendKind::GeminiCli);
    let config = gemini_config(&s, &w).expect("found");
    assert_eq!(config.program, PathBuf::from("/opt/homebrew/bin/gemini"));
    assert_eq!(config.model, "");
    assert_eq!(config.scratch, Some(dir.join("scratch")));
    assert!(dir.join("scratch").is_dir(), "the empty folder was made");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_mock_is_made_with_the_wirings_delay() {
    let mut w = wiring_with(&[], has_nothing);
    w.mock_delay_ms = 1_500;
    let s = TaskSettings::defaults(Task::Editing, BackendKind::Mock);
    let backend = build_backend(Task::Editing, &s, &w).expect("a mock");
    assert_eq!(backend.identity().kind, BackendKind::Mock);
}

/// A transport that is never used: the backends are only made here.
struct Unused;

impl Transport for Unused {
    fn send(
        &self,
        _: &crate::agent_api::HttpRequest,
    ) -> Result<crate::agent_api::HttpReply, crate::agent_api::TransportError> {
        Err(crate::agent_api::TransportError::Failed(
            "unused".to_string(),
        ))
    }
}

#[test]
fn an_api_backend_needs_a_transport_a_model_and_a_key_and_names_what_is_missing() {
    let s = TaskSettings::defaults(Task::Editing, BackendKind::AnthropicApi);
    let mut w = wiring_with(&[], has_nothing);
    assert!(
        matches!(
            build_backend(Task::Editing, &s, &w),
            Err(AgentError::Unsupported { .. })
        ),
        "no transport"
    );
    w.transport = Some(Arc::new(Unused));
    assert_eq!(
        build_backend(Task::Editing, &s, &w).err(),
        Some(AgentError::KeyMissing {
            reference: "env:ANTHROPIC_API_KEY".to_string()
        }),
        "no key: the conventional variable is named"
    );
    let mut w2 = wiring_with(&[("ANTHROPIC_API_KEY", "the-key")], has_nothing);
    w2.transport = Some(Arc::new(Unused));
    let backend = build_backend(Task::Editing, &s, &w2).expect("made");
    assert_eq!(
        backend.identity(),
        Identity {
            kind: BackendKind::AnthropicApi,
            model: "claude-haiku-5-5".to_string()
        }
    );
    let mut no_model = s.clone();
    no_model.model = String::new();
    assert!(matches!(
        build_backend(Task::Editing, &no_model, &w2),
        Err(AgentError::Settings { .. })
    ));
    let gemini = TaskSettings::defaults(Task::Editing, BackendKind::GeminiApi);
    assert_eq!(
        build_backend(Task::Editing, &gemini, &w2).err(),
        Some(AgentError::KeyMissing {
            reference: "env:GEMINI_API_KEY".to_string()
        })
    );
}

#[test]
fn the_apis_get_the_settings_as_they_are_and_a_named_key_wins_over_the_conventional_one() {
    let mut w = wiring_with(
        &[("ANTHROPIC_API_KEY", "conventional"), ("MINE", "named")],
        has_nothing,
    );
    w.transport = Some(Arc::new(Unused));
    let mut s = TaskSettings::defaults(Task::Exploration, BackendKind::AnthropicApi);
    s.effort = "medium".to_string();
    s.base_url = "http://127.0.0.1:9".to_string();
    s.max_output_tokens = 777;
    let config = anthropic_config(&s, &w).expect("made");
    assert_eq!(config.key, Secret::new("conventional"));
    assert_eq!(
        (
            config.model.as_str(),
            config.effort.as_str(),
            config.max_tokens
        ),
        ("claude-sonnet-5-5", "medium", 777)
    );
    assert_eq!(config.base_url, "http://127.0.0.1:9");
    s.key = "env:MINE".to_string();
    assert_eq!(
        anthropic_config(&s, &w).expect("made").key,
        Secret::new("named")
    );
    let g = TaskSettings::defaults(Task::Editing, BackendKind::GeminiApi);
    w.keys.env = Env::from_pairs([("GEMINI_API_KEY".to_string(), "g-key".to_string())]);
    let config = gemini_api_config(&g, &w).expect("made");
    assert_eq!(
        (config.model.as_str(), config.key),
        ("gemini-2.5-flash", Secret::new("g-key"))
    );
}

#[test]
fn the_backend_made_names_who_it_is() {
    let w = wiring_with(&[], has_claude);
    let backend = build_backend(Task::Editing, &claude_settings(), &w).expect("made");
    assert_eq!(
        backend.identity(),
        Identity {
            kind: BackendKind::ClaudePrint,
            model: "haiku".to_string()
        }
    );
}

#[test]
fn a_factory_makes_the_backend_when_a_worker_calls_it_and_the_job_runs() {
    let mut w = wiring_with(&[], has_nothing);
    w.mock_delay_ms = 0;
    let make = factory(
        Task::Editing,
        TaskSettings::defaults(Task::Editing, BackendKind::Mock),
        Arc::new(w),
    );
    let mut backend = make().expect("made on the worker");
    let request = Request::Review(ReviewRequest {
        task: Task::Editing,
        conversation: 1,
        language: "en".to_string(),
        utterances: vec![Utterance {
            text: "add a note".to_string(),
            start_ms: 0,
            end_ms: 500,
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
        workspace: None,
    });
    let mut clock = ManualClock::new();
    let mut ctl = Control::new(&mut clock, &never_cancelled);
    assert!(run_task(backend.as_mut(), &request, &Limits::default(), &mut ctl).is_ok());
}
