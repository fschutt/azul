//! The table of what the agents must do: through the mock (the reference of what a program can know), the
//! judge that reads a reply against its case, the table of pass rates, and - run by hand, each case
//! several times because a model does not answer twice alike - the table through the real `claude -p`.

use std::path::PathBuf;

use super::*;
use crate::{
    agent_backend::{never_cancelled, run_task, Control, ManualClock, Request, WallClock},
    agent_claude::{read_envelope, spec, ClaudeConfig},
    agent_export,
    agent_limits::{Limits, Notes},
    agent_mock::MockBackend,
    agent_process::{self, Host},
    agent_review::{build_call, parse_body},
};

fn through_the_mock(case: &Case) -> Result<Reply, AgentError> {
    let request = Request::Review(request_of(case));
    let mut mock = MockBackend::auto(0);
    let mut clock = ManualClock::new();
    let mut ctl = Control::new(&mut clock, &never_cancelled);
    run_task(&mut mock, &request, &Limits::default(), &mut ctl)
        .map(|response| response.reply().cloned().expect("a review answer"))
}

#[test]
fn the_table_has_the_cases_that_were_asked_for_and_each_once() {
    let mut names: Vec<&str> = CASES.iter().map(|c| c.name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before, "a name twice");
    for wanted in [
        "nonsense-unrelated",
        "gibberish",
        "test-phrase-hello-world",
        "test-phrase-testing-one-two",
        "empty",
        "low-probability-guesses",
        "ambiguous-make-it-better",
        "clear-remove",
        "clear-rename",
        "misheard-identifier",
        "biased-low-probability-identifier",
        "unlisted-low-probability-word",
    ] {
        assert!(names.contains(&wanted), "{wanted} is not in the table");
    }
    assert!(CASES.iter().filter(|c| c.important).count() >= 8);
    assert!(CASES.iter().any(|c| matches!(c.expect, Expect::NoAction)));
    assert!(CASES
        .iter()
        .any(|c| matches!(c.expect, Expect::Suggests { .. })));
    assert!(CASES.iter().any(|c| matches!(c.expect, Expect::Answers)));
    assert!(CASES
        .iter()
        .any(|c| matches!(c.expect, Expect::Corrects { .. })));
}

#[test]
fn the_mock_gets_every_case_it_is_the_reference_of_right() {
    let mut checked = 0;
    for case in CASES.iter().filter(|c| c.reference) {
        let verdict = judge(case, &through_the_mock(case));
        assert!(verdict.pass, "{}: {}", case.name, verdict.why);
        checked += 1;
    }
    assert!(
        checked >= 15,
        "the mock is the reference of most of the table: {checked}"
    );
}

#[test]
fn what_takes_a_models_judgement_is_not_something_the_mock_pretends_to_know() {
    // the mock does not tell nonsense from a remark, nor answer a question in words: those are for the
    // real runs, and a case that the mock happened to pass would not need to be marked as one it cannot
    for case in CASES.iter().filter(|c| !c.reference) {
        let verdict = judge(case, &through_the_mock(case));
        assert!(
            !verdict.pass,
            "{} is marked as beyond the mock and the mock passes it",
            case.name
        );
    }
}

#[test]
fn every_request_builds_a_prompt_that_carries_what_the_case_says() {
    for case in &CASES {
        let request = Request::Review(request_of(case));
        let Request::Review(review) = &request else {
            panic!("a review request")
        };
        let call = build_call(&request, review, &Limits::default()).expect("builds");
        if !case.words.is_empty() {
            assert!(
                call.prompt.contains("word probabilities: "),
                "{}: {}",
                case.name,
                call.prompt
            );
        }
        if case.words.iter().any(|(_, _, biased)| *biased) {
            assert!(call.prompt.contains("(biased)"), "{}", case.name);
        }
        if !case.said.is_empty() {
            assert!(call.prompt.contains(case.said), "{}", case.name);
        }
        assert!(
            call.prompt.contains("Vocabulary near the strokes"),
            "{}",
            case.name
        );
    }
}

fn reply_with(f: impl FnOnce(&mut Reply)) -> Result<Reply, AgentError> {
    let mut reply = Reply::default();
    f(&mut reply);
    Ok(reply)
}

fn named(name: &str) -> &'static Case {
    CASES.iter().find(|c| c.name == name).expect("a case")
}

#[test]
fn a_reply_is_judged_against_what_its_case_wants() {
    // nothing to do
    let nothing = named("test-phrase-hello-world");
    assert!(judge(nothing, &Ok(Reply::nothing("a test phrase"))).pass);
    let acted = reply_with(|r| {
        r.answer = "Sure!".to_string();
        r.suggestions = vec![Suggestion::InsertLines {
            after_line: 1,
            lines: vec!["// x".to_string()],
        }];
    });
    let verdict = judge(nothing, &acted);
    assert!(
        !verdict.pass && verdict.why.starts_with("it acted: 1 suggestion(s)"),
        "{verdict:?}"
    );
    assert!(!judge(nothing, &Err(AgentError::Cancelled)).pass);
    // a clear instruction
    let remove = named("clear-remove");
    let pos = crate::agent_review::Pos { line: 12, col: 8 };
    let removed = reply_with(|r| {
        r.suggestions = vec![Suggestion::RemoveRange {
            start: pos,
            end: pos,
            old: "stale".to_string(),
        }];
    });
    assert!(judge(remove, &removed).pass);
    let wrong_word = reply_with(|r| {
        r.suggestions = vec![Suggestion::RemoveRange {
            start: pos,
            end: pos,
            old: "foo".to_string(),
        }];
    });
    assert!(
        judge(remove, &wrong_word).why.contains("not \"stale\""),
        "{:?}",
        judge(remove, &wrong_word)
    );
    let wrong_kind = reply_with(|r| {
        r.suggestions = vec![Suggestion::InsertLines {
            after_line: 1,
            lines: vec![],
        }];
    });
    assert!(judge(remove, &wrong_kind).why.contains("not a Remove"));
    assert!(judge(remove, &Ok(Reply::nothing("unsure")))
        .why
        .starts_with("it did nothing"));
    assert!(judge(remove, &reply_with(|_| {}))
        .why
        .contains("no suggestion"));
    // a question
    let question = named("question-is-answered");
    assert!(
        judge(
            question,
            &reply_with(|r| r.answer = "Because of the cache.".to_string())
        )
        .pass
    );
    assert!(!judge(question, &removed).pass);
    assert!(!judge(question, &Ok(Reply::nothing("x"))).pass);
    assert!(!judge(question, &reply_with(|_| {})).pass);
    // a name put right
    let corrects = named("misheard-identifier");
    let replaced = |heard: Option<&str>| {
        reply_with(|r| {
            r.suggestions = vec![Suggestion::ReplaceRange {
                start: pos,
                end: pos,
                old: "foo".to_string(),
                text: "count".to_string(),
            }];
            r.heard_as = heard.map(str::to_string);
        })
    };
    assert!(
        judge(corrects, &replaced(Some("Rename FOO to count"))).pass,
        "case does not matter"
    );
    assert!(judge(corrects, &replaced(Some("rename food to count")))
        .why
        .contains("heard_as does not say"));
    assert!(judge(corrects, &replaced(None))
        .why
        .contains("heard_as does not say"));
    // a compound name may be said as itself or as it is spoken
    let compound = named("misheard-spoken-compound");
    let replaced_compound = |heard: &str| {
        reply_with(|r| {
            r.suggestions = vec![Suggestion::ReplaceRange {
                start: pos,
                end: pos,
                old: "compute_stale_flag".to_string(),
                text: "check_freshness".to_string(),
            }];
            r.heard_as = Some(heard.to_string());
        })
    };
    assert!(
        judge(
            compound,
            &replaced_compound("rename compute_stale_flag to check freshness")
        )
        .pass
    );
    assert!(
        judge(
            compound,
            &replaced_compound("Rename compute stale flag to check freshness")
        )
        .pass
    );
    assert!(
        !judge(
            compound,
            &replaced_compound("rename compute to check freshness")
        )
        .pass
    );
}

#[test]
fn the_table_of_pass_rates_says_how_often_each_case_passed_and_why_not() {
    let fail = |why: &str| Verdict {
        pass: false,
        why: why.to_string(),
    };
    let pass = Verdict {
        pass: true,
        why: String::new(),
    };
    let runs = vec![
        Run {
            case: "a",
            model: "haiku".to_string(),
            verdict: pass.clone(),
        },
        Run {
            case: "a",
            model: "haiku".to_string(),
            verdict: fail("it acted | a lot"),
        },
        Run {
            case: "a",
            model: "haiku".to_string(),
            verdict: fail("it acted | a lot"),
        },
        Run {
            case: "a",
            model: "sonnet".to_string(),
            verdict: pass.clone(),
        },
        Run {
            case: "b",
            model: "haiku".to_string(),
            verdict: pass,
        },
    ];
    let table = report(&runs);
    assert!(
        table.contains("| a | haiku | 1/3 | it acted / a lot |"),
        "{table}"
    );
    assert!(table.contains("| a | sonnet | 1/1 |  |"), "{table}");
    assert!(table.contains("| b | haiku | 1/1 |  |"), "{table}");
    assert!(table.ends_with("total: 3/5\n"), "{table}");
}

#[test]
fn an_export_is_judged_by_the_recorders_own_check() {
    let (request, manifest) = export_fixture();
    let mut notes = Notes::new();
    let good = agent_export::parse_body(
        &request,
        r#"{"title": "A leak", "segments": [{"in_s": 9, "out_s": 14, "camera": true}, {"in_s": 199, "out_s": 203}]}"#,
        &Limits::default(),
        &mut notes,
    );
    assert!(
        judge_export(&good, &manifest, 200).pass,
        "{:?}",
        judge_export(&good, &manifest, 200)
    );
    assert!(!judge_export(&good, &manifest, 1).pass, "over the cap");
    assert!(judge_export(&Err(AgentError::Cancelled), &manifest, 200)
        .why
        .starts_with("an error: "));
    let empty = Ok(agent_export::CutList {
        title: String::new(),
        segments: Vec::new(),
        chapters: Vec::new(),
    });
    assert_eq!(judge_export(&empty, &manifest, 200).why, "no segment");
}

// ==== the real claude, run by hand ====

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn real_config(model: &str) -> ClaudeConfig {
    let host = Host::new(
        std::env::var("PATH").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    );
    let program = agent_process::find_program("", "claude", &host, &agent_process::is_program)
        .expect("claude is installed");
    ClaudeConfig {
        program,
        model: model.to_string(),
        timeout_secs: 180,
        budget_cents: 30,
        max_turns: 0,
        effort: String::new(),
        extra_args: Vec::new(),
        key: None,
        limits: Limits::default(),
    }
}

/// What one call of the real program made of a call: the text it answered and its envelope.
fn real_text(
    config: &ClaudeConfig,
    call: &crate::agent_backend::Call<'_>,
) -> (Result<String, AgentError>, String) {
    let spec = spec(config, call);
    let mut clock = WallClock::new();
    let cancel = || false;
    let mut ctl = Control::new(&mut clock, &cancel);
    match agent_process::run(&spec, &mut ctl) {
        Ok(out) => {
            let envelope = out.stdout_text();
            (read_envelope(&out, config, &[]).map(|c| c.text), envelope)
        }
        Err(failure) => (
            Err(failure.into_error("claude", Vec::new(), spec.timeout_ms)),
            String::new(),
        ),
    }
}

/// The cases a run asks for: `AZREVIEW_EVAL_ONLY` is `important`, or names or the beginnings of names
/// separated by commas (default: all).
fn wanted(case: &Case) -> bool {
    match env_or("AZREVIEW_EVAL_ONLY", "").as_str() {
        "" => true,
        "important" => case.important,
        list => list.split(',').any(|p| case.name.starts_with(p.trim())),
    }
}

fn save(dir: &PathBuf, name: &str, text: &str) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join(name), text);
}

#[test]
#[ignore = "X-EDGE-C-2: real claude -p evaluation of the table, run by hand: AZREVIEW_EVAL_MODEL=haiku AZREVIEW_EVAL_K=5 AZREVIEW_EVAL_ONLY=important AZREVIEW_EVAL_OUT=dir cargo test --lib real_claude_table -- --ignored --nocapture"]
fn real_claude_table() {
    let model = env_or("AZREVIEW_EVAL_MODEL", "haiku");
    let k: usize = env_or("AZREVIEW_EVAL_K", "5").parse().unwrap_or(5);
    let out = PathBuf::from(env_or(
        "AZREVIEW_EVAL_OUT",
        &std::env::temp_dir().join("azreview-eval").to_string_lossy(),
    ))
    .join(&model);
    let config = real_config(&model);
    let mut runs: Vec<Run> = Vec::new();
    for case in CASES.iter().filter(|c| wanted(c)) {
        for i in 0..k {
            let request = Request::Review(request_of(case));
            let Request::Review(review) = &request else {
                panic!("a review request")
            };
            let call = build_call(&request, review, &config.limits).expect("builds");
            let began = std::time::Instant::now();
            let (text, envelope) = real_text(&config, &call);
            let result = text.and_then(|text| {
                save(&out, &format!("{}-{i}.txt", case.name), &text);
                let mut notes = Notes::new();
                parse_body(review, &text, &config.limits, &mut notes)
            });
            if i == 0 {
                save(&out, &format!("{}.envelope.json", case.name), &envelope);
            }
            let verdict = judge(case, &result);
            println!(
                "{} {} #{i} {} ({:.1} s){}",
                model,
                case.name,
                if verdict.pass { "pass" } else { "FAIL" },
                began.elapsed().as_secs_f64(),
                if verdict.pass {
                    String::new()
                } else {
                    format!(": {}", verdict.why)
                }
            );
            runs.push(Run {
                case: case.name,
                model: model.clone(),
                verdict,
            });
        }
    }
    let table = report(&runs);
    println!("\n{table}");
    save(&out, "report.md", &table);
}

#[test]
#[ignore = "X-EDGE-C-2: real claude -p evaluation of the export task, run by hand: AZREVIEW_EVAL_MODEL=sonnet AZREVIEW_EVAL_K=3 cargo test --lib real_claude_export -- --ignored --nocapture"]
fn real_claude_export() {
    let model = env_or("AZREVIEW_EVAL_MODEL", "sonnet");
    let k: usize = env_or("AZREVIEW_EVAL_K", "3").parse().unwrap_or(3);
    let out = PathBuf::from(env_or(
        "AZREVIEW_EVAL_OUT",
        &std::env::temp_dir().join("azreview-eval").to_string_lossy(),
    ))
    .join(format!("{model}-export"));
    let config = real_config(&model);
    let (export, manifest) = export_fixture();
    let mut runs: Vec<Run> = Vec::new();
    for i in 0..k {
        let request = Request::Export(export.clone());
        let call = agent_export::build_call(&request, &export, &config.limits).expect("builds");
        let began = std::time::Instant::now();
        let (text, envelope) = real_text(&config, &call);
        let mut notes = Notes::new();
        let result = text.and_then(|text| {
            save(&out, &format!("export-{i}.txt"), &text);
            agent_export::parse_body(&export, &text, &config.limits, &mut notes)
        });
        if i == 0 {
            save(&out, "export.envelope.json", &envelope);
        }
        let verdict = judge_export(&result, &manifest, config.limits.max_cut_segments);
        println!(
            "{model} export #{i} {} ({:.1} s, {} segments){}{}",
            if verdict.pass { "pass" } else { "FAIL" },
            began.elapsed().as_secs_f64(),
            result.as_ref().map_or(0, |l| l.segments.len()),
            if verdict.pass {
                String::new()
            } else {
                format!(": {}", verdict.why)
            },
            if notes.finish().is_empty() {
                String::new()
            } else {
                " (notes)".to_string()
            }
        );
        runs.push(Run {
            case: "export",
            model: model.clone(),
            verdict,
        });
    }
    let table = report(&runs);
    println!("\n{table}");
    save(&out, "report.md", &table);
}
