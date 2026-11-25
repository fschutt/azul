//! The review tasks: the prompt a request becomes, the answer a reply is read as. Text in, text out:
//! no process, no socket, no window.

use super::*;
use crate::{agent_backend::Tools, agent_json, agent_stroke::Sample};

// ==== fixtures ====

fn code() -> Vec<String> {
    [
        "fn main() {",
        "    let x = foo(1);",
        "    let y = bar(x);",
        "    // stale comment",
        "    println!(\"{y}\");",
        "}",
        "",
    ]
    .iter()
    .map(|l| (*l).to_string())
    .collect()
}

fn context() -> Context {
    Context {
        file: "src/lib.rs".to_string(),
        page: 2,
        first_line: 38,
        lines: code(),
        total_lines: 120,
        focus: Some((39, 40)),
        words_under: vec![WordRef {
            stroke: 12,
            line: 39,
            col: 12,
            text: "foo".to_string(),
        }],
        grid: Some(PageGrid {
            first_line: 38,
            origin_x: 48.0,
            origin_y: 324.0,
            line_height: 16.0,
            char_width: 7.2,
        }),
    }
}

fn stroke(id: u64, pinned: bool, meaning: Option<Meaning>) -> StrokeInput {
    StrokeInput {
        id,
        page: 2,
        meaning,
        pinned,
        nib: "pen".to_string(),
        nib_width: 2.4,
        t0_ms: 4_000,
        samples: (0..12)
            .map(|i| Sample {
                x: 100.0 + i as f32 * 9.0,
                y: 345.0,
                pressure: 0.5,
                dt_ms: i * 120,
                tilt: None,
            })
            .collect(),
    }
}

fn review(task: Task) -> ReviewRequest {
    ReviewRequest {
        task,
        conversation: 7,
        language: "de".to_string(),
        utterances: vec![Utterance {
            text: "rename this one to count".to_string(),
            start_ms: 4_100,
            end_ms: 5_300,
        }],
        strokes: vec![
            stroke(12, true, Some(Meaning::Issue)),
            stroke(13, false, None),
        ],
        context: context(),
        turns: vec![
            Turn {
                from: Speaker::Reviewer,
                text: "what does foo return?".to_string(),
            },
            Turn {
                from: Speaker::Agent,
                text: "an i32".to_string(),
            },
        ],
        workspace: Some(PathBuf::from("/work/space")),
    }
}

fn built(
    review: ReviewRequest,
    limits: &Limits,
) -> (
    Result<(String, String, Option<PathBuf>, Tools, Vec<String>), AgentError>,
    Request,
) {
    let request = Request::Review(review.clone());
    let result = build_call(&request, &review, limits)
        .map(|c| (c.system, c.prompt, c.workdir, c.tools, c.diagnostics));
    (result, request)
}

fn prompt_of(review: ReviewRequest) -> String {
    built(review, &Limits::default()).0.expect("builds").1
}

// ==== the prompt ====

#[test]
fn an_editing_prompt_has_the_file_the_code_the_strokes_the_words_and_the_conversation_in_order() {
    let p = prompt_of(review(Task::Editing));
    let at = |needle: &str| {
        p.find(needle)
            .unwrap_or_else(|| panic!("{needle:?} missing in:\n{p}"))
    };
    let order = [
        at("Task: editing"),
        at("Language of the reviewer: de"),
        at("File: src/lib.rs (page 3, 120 lines)"),
        at("Code (line numbers as in the file"),
        at("Layout of the page"),
        at("Words under the strokes"),
        at("Strokes (SVG"),
        at("What the reviewer said"),
        at("Meaning of the strokes"),
        at("Conversation so far"),
        at("Reply with the JSON object."),
    ];
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}\n{p}");
}

#[test]
fn the_code_is_numbered_as_in_the_file_and_the_covered_lines_are_marked() {
    let p = prompt_of(review(Task::Editing));
    assert!(p.contains("\n  38| fn main() {\n"), "{p}");
    assert!(p.contains("\n> 39|     let x = foo(1);\n"), "{p}");
    assert!(p.contains("\n> 40|     let y = bar(x);\n"), "{p}");
    assert!(p.contains("\n  41|     // stale comment\n"), "{p}");
}

#[test]
fn the_line_numbers_are_padded_to_the_widest() {
    let mut r = review(Task::Editing);
    r.context.first_line = 98;
    r.context.focus = None;
    r.context.grid = None;
    let p = prompt_of(r);
    assert!(p.contains("\n   98| fn main() {\n"), "{p}");
    assert!(p.contains("\n  104| \n") || p.contains("\n  104|"), "{p}");
}

#[test]
fn the_layout_line_lets_the_agent_read_the_coordinates_against_the_code() {
    let p = prompt_of(review(Task::Editing));
    assert!(
        p.contains("Layout of the page: line 38 starts at y=324 and every line is 16 px high; column 0 starts at x=48 and every column is 7.2 px wide."),
        "{p}"
    );
    let mut r = review(Task::Editing);
    r.context.grid = None;
    assert!(!prompt_of(r).contains("Layout of the page"));
}

#[test]
fn the_words_under_the_strokes_name_their_line_and_columns() {
    let p = prompt_of(review(Task::Editing));
    assert!(
        p.contains("- stroke 12: \"foo\" at line 39, columns 12 to 15"),
        "{p}"
    );
}

#[test]
fn the_strokes_are_an_svg_with_the_pens_widths_and_times() {
    let p = prompt_of(review(Task::Editing));
    assert!(
        p.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""),
        "{p}"
    );
    assert!(p.contains("id=\"s12\"") && p.contains("id=\"s13\""), "{p}");
    assert!(
        p.contains("data-t=\"0 1320\""),
        "a straight stroke keeps its two ends and their times: {p}"
    );
    assert!(p.contains("data-w=\"1.6 1.6\""), "{p}");
}

#[test]
fn the_words_are_tied_to_the_strokes_drawn_while_they_were_said() {
    let p = prompt_of(review(Task::Editing));
    assert!(
        p.contains("- 4.1 to 5.3 s: \"rename this one to count\" [during strokes 12, 13]"),
        "{p}"
    );
    let mut r = review(Task::Editing);
    r.utterances.push(Utterance {
        text: "and later".to_string(),
        start_ms: 60_000,
        end_ms: 61_000,
    });
    let p = prompt_of(r);
    assert!(
        p.contains("- 60.0 to 61.0 s: \"and later\"\n")
            || p.contains("- 60.0 to 61.0 s: \"and later\""),
        "{p}"
    );
    assert!(
        !p.contains("\"and later\" [during"),
        "no stroke was drawn then: {p}"
    );
}

#[test]
fn a_chosen_meaning_is_final_and_a_missing_one_is_for_the_agent_to_infer() {
    let p = prompt_of(review(Task::Editing));
    assert!(
        p.contains("- stroke 12: issue (chosen by the reviewer, final:"),
        "{p}"
    );
    assert!(
        p.contains("- stroke 13: no meaning chosen: infer it from the shape and the words"),
        "{p}"
    );
    let mut r = review(Task::Editing);
    r.strokes[0].pinned = false;
    r.strokes[0].meaning = Some(Meaning::Scope);
    let p = prompt_of(r);
    assert!(
        p.contains("- stroke 12: scope (the pen's own, not chosen on purpose:"),
        "{p}"
    );
}

#[test]
fn the_conversation_so_far_is_listed_by_speaker() {
    let p = prompt_of(review(Task::Editing));
    assert!(
        p.contains("Reviewer: what does foo return?\nAgent: an i32\n"),
        "{p}"
    );
}

#[test]
fn a_request_with_nothing_to_say_has_no_empty_sections() {
    let r = ReviewRequest {
        task: Task::Editing,
        conversation: 1,
        language: String::new(),
        utterances: Vec::new(),
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
    };
    let p = prompt_of(r);
    for absent in [
        "Language of the reviewer",
        "Layout of the page",
        "Words under",
        "Strokes (SVG",
        "What the reviewer said",
        "Meaning of the strokes",
        "Conversation so far",
    ] {
        assert!(!p.contains(absent), "{absent:?} in:\n{p}");
    }
    assert!(
        p.starts_with("Task: editing\nFile: a.rs (page 1, 1 lines)\n"),
        "{p}"
    );
    assert!(p.contains("  1| fn a() {}"));
    assert!(p.trim_end().ends_with("Reply with the JSON object."), "{p}");
}

#[test]
fn the_same_request_makes_the_same_prompt() {
    assert_eq!(
        prompt_of(review(Task::Editing)),
        prompt_of(review(Task::Editing))
    );
}

// ==== the system prompt, the tools, the folder ====

#[test]
fn editing_has_no_tools_no_folder_and_asks_for_the_json_object() {
    let (result, _) = built(review(Task::Editing), &Limits::default());
    let (system, _, workdir, tools, _) = result.expect("builds");
    assert_eq!(tools, Tools::None);
    assert_eq!(workdir, None, "an agent without tools reads no folder");
    for needle in [
        "exactly one JSON object",
        "insert_lines",
        "\"replace\"",
        "\"remove\"",
        "after_line",
        "never write files",
    ] {
        assert!(system.contains(needle), "{needle:?} missing in:\n{system}");
    }
}

#[test]
fn exploration_reads_the_workspace_read_only_and_answers_in_plain_text() {
    let (result, _) = built(review(Task::Exploration), &Limits::default());
    let (system, prompt, workdir, tools, _) = result.expect("builds");
    assert_eq!(tools, Tools::ReadOnly);
    assert_eq!(workdir, Some(PathBuf::from("/work/space")));
    assert!(
        system.contains("read-only") && system.contains("plain text"),
        "{system}"
    );
    assert!(prompt.contains("Task: exploration"), "{prompt}");
    assert!(
        prompt.trim_end().ends_with("Answer the reviewer."),
        "{prompt}"
    );
    assert!(
        prompt.contains("paths are relative to the workspace"),
        "{prompt}"
    );
}

#[test]
fn the_intent_is_asked_for_only_when_a_stroke_has_no_chosen_meaning() {
    let (with, _) = built(review(Task::Editing), &Limits::default());
    let system_with = with.expect("builds").0;
    assert!(
        system_with.contains("\"intent\"") && system_with.contains("never change them"),
        "{system_with}"
    );
    let mut r = review(Task::Editing);
    r.strokes.retain(|s| s.pinned);
    let system_without = built(r, &Limits::default()).0.expect("builds").0;
    assert!(!system_without.contains("\"intent\""), "{system_without}");
}

#[test]
fn a_task_that_is_not_a_review_task_is_refused() {
    let (result, _) = built(review(Task::Export), &Limits::default());
    assert!(
        matches!(result, Err(AgentError::Settings { .. })),
        "{result:?}"
    );
}

// ==== the caps ====

#[test]
fn more_strokes_than_the_cap_keep_the_newest_and_say_so() {
    let mut r = review(Task::Editing);
    r.strokes = (1..=11)
        .map(|i| stroke(i, true, Some(Meaning::Scope)))
        .collect();
    let (result, _) = built(r, &Limits::default());
    let (_, prompt, _, _, notes) = result.expect("builds");
    assert!(!prompt.contains("id=\"s1\""), "{prompt}");
    assert!(
        prompt.contains("id=\"s11\"") && prompt.contains("id=\"s4\""),
        "{prompt}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("11 strokes") && n.contains("8")),
        "{notes:?}"
    );
}

#[test]
fn a_long_line_is_cut_and_a_big_window_is_kept_around_the_strokes() {
    let mut r = review(Task::Editing);
    r.context.first_line = 1;
    r.context.lines = (1..=1_000)
        .map(|i| format!("line {i} {}", "x".repeat(if i == 500 { 900 } else { 10 })))
        .collect();
    r.context.total_lines = 1_000;
    r.context.focus = Some((500, 501));
    r.context.words_under.clear();
    r.context.grid = None;
    let limits = Limits::default();
    let (result, _) = built(r, &limits);
    let (_, prompt, _, _, notes) = result.expect("builds");
    assert!(
        prompt.contains("| line 500 "),
        "the focus is inside the window: {prompt}"
    );
    assert!(!prompt.contains("| line 1 "), "the far lines are out");
    assert!(
        !prompt.contains(&"x".repeat(limits.max_line_chars + 5)),
        "the long line is cut"
    );
    assert!(prompt.contains('\u{2026}'));
    assert!(
        notes.iter().any(|n| n.contains("lines of code")),
        "{notes:?}"
    );
    assert!(notes.iter().any(|n| n.contains("cut")), "{notes:?}");
}

#[test]
fn the_oldest_turns_go_first_when_the_conversation_is_long() {
    let mut r = review(Task::Editing);
    r.turns = (0..40)
        .map(|i| Turn {
            from: Speaker::Reviewer,
            text: format!("turn number {i}"),
        })
        .collect();
    let (result, _) = built(r, &Limits::default());
    let (_, prompt, _, _, notes) = result.expect("builds");
    assert!(
        prompt.contains("turn number 39") && prompt.contains("turn number 20"),
        "{prompt}"
    );
    assert!(!prompt.contains("turn number 19\n"), "{prompt}");
    assert!(notes.iter().any(|n| n.contains("turns")), "{notes:?}");
}

#[test]
fn a_prompt_that_does_not_fit_is_trimmed_before_it_is_refused() {
    let mut r = review(Task::Editing);
    r.turns = (0..20)
        .map(|i| Turn {
            from: Speaker::Reviewer,
            text: format!("{i} {}", "word ".repeat(380)),
        })
        .collect();
    r.context.lines = (0..160)
        .map(|i| format!("{i:03} {}", "y".repeat(290)))
        .collect();
    r.context.first_line = 1;
    r.context.total_lines = 160;
    r.context.focus = Some((80, 81));
    let limits = Limits::default();
    let (result, _) = built(r, &limits);
    let (system, prompt, _, _, notes) = result.expect("trimmed to fit");
    assert!(
        system.len() + prompt.len() <= limits.max_prompt_bytes,
        "{}",
        system.len() + prompt.len()
    );
    assert!(notes.iter().any(|n| n.contains("trimmed")), "{notes:?}");
}

#[test]
fn a_prompt_that_cannot_be_trimmed_to_fit_is_a_too_large_error() {
    let limits = Limits {
        max_prompt_bytes: 400,
        ..Limits::default()
    };
    let (result, _) = built(review(Task::Editing), &limits);
    match result {
        Err(AgentError::TooLarge { what, limit, .. }) => {
            assert_eq!(what, "the prompt");
            assert_eq!(limit, 400);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_silly_language_tag_and_file_name_are_cut_not_refused() {
    let mut r = review(Task::Editing);
    r.language = "x".repeat(200_000);
    r.context.file = "f".repeat(5_000);
    let (result, _) = built(r, &Limits::default());
    let (_, prompt, _, _, _) = result.expect("builds");
    assert!(prompt.len() < 10_000, "{}", prompt.len());
    assert!(prompt.contains("Language of the reviewer: xxxx"));
}

#[test]
fn the_words_under_the_strokes_are_capped() {
    let mut r = review(Task::Editing);
    r.context.words_under = (0..200)
        .map(|i| WordRef {
            stroke: 12,
            line: 39,
            col: i,
            text: format!("w{i}"),
        })
        .collect();
    let (result, _) = built(r, &Limits::default());
    let (_, prompt, _, _, notes) = result.expect("builds");
    assert_eq!(
        prompt.matches("- stroke 12: \"w").count(),
        Limits::default().max_words_under
    );
    assert!(notes.iter().any(|n| n.contains("words")), "{notes:?}");
}

// ==== reading an answer ====

fn editing() -> ReviewRequest {
    review(Task::Editing)
}

fn parse(text: &str) -> (Result<Reply, AgentError>, Vec<String>) {
    parse_for(&editing(), text)
}

fn parse_for(review: &ReviewRequest, text: &str) -> (Result<Reply, AgentError>, Vec<String>) {
    let mut notes = Notes::new();
    let result = parse_body(review, text, &Limits::default(), &mut notes);
    (result, notes.finish())
}

const FULL: &str = r##"{
  "answer": "I renamed foo to count and removed the stale comment.",
  "suggestions": [
    {"kind": "insert_lines", "after_line": 40, "lines": ["    let n = count;", "    // checked"]},
    {"kind": "replace", "start": {"line": 39, "col": 12}, "end": {"line": 39, "col": 15}, "old": "foo", "text": "count"},
    {"kind": "remove", "start": {"line": 41, "col": 4}, "end": {"line": 41, "col": 20}, "old": "// stale comment"}
  ],
  "intent": {"meaning": "issue", "confidence": 0.8, "rationale": "the reviewer wants a rename"}
}"##;

#[test]
fn a_full_reply_is_read_into_its_answer_its_three_kinds_of_suggestion_and_its_intent() {
    let (result, notes) = parse(FULL);
    let reply = result.expect("reads");
    assert_eq!(
        reply.answer,
        "I renamed foo to count and removed the stale comment."
    );
    assert_eq!(
        reply.suggestions,
        vec![
            Suggestion::InsertLines {
                after_line: 40,
                lines: vec![
                    "    let n = count;".to_string(),
                    "    // checked".to_string()
                ]
            },
            Suggestion::ReplaceRange {
                start: Pos { line: 39, col: 12 },
                end: Pos { line: 39, col: 15 },
                old: "foo".to_string(),
                text: "count".to_string()
            },
            Suggestion::RemoveRange {
                start: Pos { line: 41, col: 4 },
                end: Pos { line: 41, col: 20 },
                old: "// stale comment".to_string()
            },
        ]
    );
    let intent = reply.intent.expect("intent");
    assert_eq!(intent.meaning, Some(Meaning::Issue));
    assert!((intent.confidence - 0.8).abs() < 1e-6);
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn a_reply_between_prose_and_a_code_fence_is_found() {
    let text =
        format!("Sure, here is the change:\n```json\n{FULL}\n```\nLet me know if you want more.");
    let (result, notes) = parse(&text);
    assert_eq!(result.expect("reads").suggestions.len(), 3);
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn the_names_models_use_for_the_same_things_are_understood() {
    let text = r##"{"reply": "ok", "edits": [
        {"type": "insert", "after": 39, "text": "// a\n// b"},
        {"action": "rewrite", "start": [39, 12], "end": [39, 15], "original": "foo", "replacement": "count"},
        {"op": "delete", "line": 41, "start_col": 4, "end_col": 20, "old": "// stale comment"}
    ]}"##;
    let reply = parse(text).0.expect("reads");
    assert_eq!(reply.answer, "ok");
    assert_eq!(reply.suggestions.len(), 3, "{reply:?}");
    assert_eq!(
        reply.suggestions[0],
        Suggestion::InsertLines {
            after_line: 39,
            lines: vec!["// a".to_string(), "// b".to_string()]
        }
    );
}

#[test]
fn a_raw_line_break_in_a_string_is_repaired_and_said() {
    let text = "{\"answer\": \"two\nlines\", \"suggestions\": []}";
    let (result, notes) = parse_for(&editing(), text);
    assert_eq!(result.expect("reads").answer, "two\nlines");
    assert!(notes.iter().any(|n| n.contains("repair")), "{notes:?}");
}

#[test]
fn a_column_that_is_off_is_moved_to_where_the_old_text_is_and_it_is_said() {
    // the agent counted the column of `foo` as 10, the code has it at 12
    let text = r##"{"answer": "x", "suggestions": [
        {"kind": "replace", "start": {"line": 39, "col": 10}, "end": {"line": 39, "col": 13}, "old": "foo", "text": "count"}]}"##;
    let (result, notes) = parse(text);
    let reply = result.expect("reads");
    assert_eq!(
        reply.suggestions,
        vec![Suggestion::ReplaceRange {
            start: Pos { line: 39, col: 12 },
            end: Pos { line: 39, col: 15 },
            old: "foo".to_string(),
            text: "count".to_string()
        }]
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("moved") && n.contains("39")),
        "{notes:?}"
    );
}

#[test]
fn an_old_text_on_another_line_of_the_window_is_found_when_it_is_the_only_one() {
    let text = r##"{"answer": "x", "suggestions": [
        {"kind": "remove", "start": {"line": 39, "col": 4}, "end": {"line": 39, "col": 20}, "old": "// stale comment"}]}"##;
    let reply = parse(text).0.expect("reads");
    assert_eq!(
        reply.suggestions,
        vec![Suggestion::RemoveRange {
            start: Pos { line: 41, col: 4 },
            end: Pos { line: 41, col: 20 },
            old: "// stale comment".to_string()
        }]
    );
}

#[test]
fn an_old_text_that_is_nowhere_in_the_window_drops_the_suggestion_and_says_why() {
    let text = r##"{"answer": "kept", "suggestions": [
        {"kind": "replace", "start": {"line": 39, "col": 12}, "end": {"line": 39, "col": 15}, "old": "nowhere", "text": "count"}]}"##;
    let (result, notes) = parse(text);
    let reply = result.expect("the answer is still there");
    assert!(reply.suggestions.is_empty());
    assert!(
        notes
            .iter()
            .any(|n| n.contains("dropped") && n.contains("nowhere")),
        "{notes:?}"
    );
}

#[test]
fn an_old_text_that_is_twice_in_the_line_is_not_guessed() {
    let mut r = editing();
    r.context.lines[1] = "    let x = foo(foo(1));".to_string();
    let text = r##"{"answer": "k", "suggestions": [
        {"kind": "replace", "start": {"line": 39, "col": 0}, "end": {"line": 39, "col": 3}, "old": "foo", "text": "count"}]}"##;
    let (result, notes) = parse_for(&r, text);
    assert!(result.expect("reads").suggestions.is_empty());
    assert!(
        notes.iter().any(|n| n.contains("more than once")),
        "{notes:?}"
    );
}

#[test]
fn a_missing_old_text_inside_the_window_is_filled_in_from_the_code() {
    let text = r##"{"answer": "x", "suggestions": [
        {"kind": "remove", "start": {"line": 41, "col": 4}, "end": {"line": 41, "col": 20}}]}"##;
    let reply = parse(text).0.expect("reads");
    assert_eq!(
        reply.suggestions,
        vec![Suggestion::RemoveRange {
            start: Pos { line: 41, col: 4 },
            end: Pos { line: 41, col: 20 },
            old: "// stale comment".to_string()
        }]
    );
}

#[test]
fn a_range_outside_the_window_is_kept_but_called_unverified() {
    let text = r##"{"answer": "x", "suggestions": [
        {"kind": "replace", "start": {"line": 80, "col": 0}, "end": {"line": 80, "col": 3}, "old": "abc", "text": "xyz"}]}"##;
    let (result, notes) = parse(text);
    assert_eq!(result.expect("reads").suggestions.len(), 1);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("unverified") && n.contains("80")),
        "{notes:?}"
    );
}

#[test]
fn a_range_past_the_end_of_the_file_or_backwards_is_dropped() {
    let text = r##"{"answer": "x", "suggestions": [
        {"kind": "remove", "start": {"line": 500, "col": 0}, "end": {"line": 500, "col": 3}, "old": "abc"},
        {"kind": "remove", "start": {"line": 40, "col": 9}, "end": {"line": 40, "col": 4}, "old": "bar(x)"}]}"##;
    let (result, notes) = parse(text);
    assert!(result.expect("reads").suggestions.is_empty());
    assert!(notes.len() >= 2, "{notes:?}");
}

#[test]
fn an_insertion_after_line_zero_is_fine_and_one_past_the_end_of_the_file_is_not() {
    let ok = r##"{"answer": "x", "suggestions": [{"kind": "insert_lines", "after_line": 0, "lines": ["// header"]}]}"##;
    assert_eq!(parse(ok).0.expect("reads").suggestions.len(), 1);
    let past = r##"{"answer": "x", "suggestions": [{"kind": "insert_lines", "after_line": 121, "lines": ["// header"]}]}"##;
    let (result, notes) = parse(past);
    assert!(result.expect("reads").suggestions.is_empty());
    assert!(
        notes.iter().any(|n| n.contains("after line 121")),
        "{notes:?}"
    );
}

#[test]
fn an_unknown_kind_is_dropped_with_a_note_and_the_rest_stays() {
    let text = r##"{"answer": "x", "suggestions": [{"kind": "teleport", "line": 3}, {"kind": "insert_lines", "after_line": 1, "lines": ["a"]}]}"##;
    let (result, notes) = parse(text);
    assert_eq!(result.expect("reads").suggestions.len(), 1);
    assert!(notes.iter().any(|n| n.contains("teleport")), "{notes:?}");
}

#[test]
fn identical_suggestions_are_kept_once() {
    let one = r##"{"kind": "insert_lines", "after_line": 1, "lines": ["a"]}"##;
    let text = format!("{{\"answer\": \"x\", \"suggestions\": [{one}, {one}]}}");
    assert_eq!(parse(&text).0.expect("reads").suggestions.len(), 1);
}

#[test]
fn too_many_suggestions_and_too_many_lines_and_a_huge_text_are_capped_and_said() {
    let limits = Limits::default();
    let many: Vec<String> = (0..30)
        .map(|i| {
            format!(
                "{{\"kind\": \"insert_lines\", \"after_line\": {}, \"lines\": [\"l{i}\"]}}",
                i + 1
            )
        })
        .collect();
    let text = format!(
        "{{\"answer\": \"x\", \"suggestions\": [{}]}}",
        many.join(",")
    );
    let (result, notes) = parse(&text);
    assert_eq!(
        result.expect("reads").suggestions.len(),
        limits.max_suggestions
    );
    assert!(
        notes.iter().any(|n| n.contains("30 suggestions")),
        "{notes:?}"
    );

    let lines: Vec<String> = (0..300).map(|i| format!("\"line {i}\"")).collect();
    let text = format!("{{\"answer\": \"x\", \"suggestions\": [{{\"kind\": \"insert_lines\", \"after_line\": 1, \"lines\": [{}]}}]}}", lines.join(","));
    let reply = parse(&text).0.expect("reads");
    match &reply.suggestions[0] {
        Suggestion::InsertLines { lines, .. } => assert_eq!(lines.len(), limits.max_insert_lines),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_long_answer_is_cut_to_the_limit() {
    let text = format!(
        "{{\"answer\": \"{}\", \"suggestions\": []}}",
        "a".repeat(40_000)
    );
    let (result, notes) = parse(&text);
    let reply = result.expect("reads");
    assert_eq!(
        reply.answer.chars().count(),
        Limits::default().max_answer_chars
    );
    assert!(notes.iter().any(|n| n.contains("answer")), "{notes:?}");
}

#[test]
fn an_editing_reply_in_plain_prose_is_the_answer_without_suggestions() {
    let (result, notes) = parse("I could not find a variable called count in these lines.");
    let reply = result.expect("prose is an answer");
    assert_eq!(
        reply.answer,
        "I could not find a variable called count in these lines."
    );
    assert!(reply.suggestions.is_empty());
    assert!(notes.iter().any(|n| n.contains("plain text")), "{notes:?}");
}

#[test]
fn an_exploration_reply_in_prose_is_the_answer_and_a_json_example_in_it_is_not_a_reply() {
    let r = review(Task::Exploration);
    let text = "foo returns the config, like this:\n```json\n{\"name\": \"x\", \"size\": 3}\n```\nSo it is an object.";
    let (result, notes) = parse_for(&r, text);
    let reply = result.expect("prose is an answer");
    assert_eq!(reply.answer, text);
    assert!(reply.suggestions.is_empty());
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn an_exploration_may_end_with_a_suggestions_block_and_the_prose_before_it_is_the_answer() {
    let r = review(Task::Exploration);
    let text = "Rename it.\n```json\n{\"suggestions\": [{\"kind\": \"insert_lines\", \"after_line\": 1, \"lines\": [\"// x\"]}]}\n```";
    let reply = parse_for(&r, text).0.expect("reads");
    assert_eq!(reply.answer, "Rename it.");
    assert_eq!(reply.suggestions.len(), 1);
}

#[test]
fn a_reply_that_starts_like_json_but_is_broken_is_a_bad_output_with_an_excerpt() {
    let (result, _) = parse("{\"answer\": \"ok\", \"suggestions\": [{\"kind\"");
    match result {
        Err(AgentError::BadOutput { why, excerpt }) => {
            assert!(why.contains("JSON"), "{why}");
            assert!(excerpt.starts_with("{\"answer\""), "{excerpt}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_reply_with_neither_an_answer_nor_a_usable_suggestion_is_a_bad_output() {
    let (result, _) = parse(r##"{"answer": "", "suggestions": [{"kind": "teleport"}]}"##);
    assert!(
        matches!(result, Err(AgentError::BadOutput { .. })),
        "{result:?}"
    );
    let (result, _) = parse("");
    assert!(
        matches!(result, Err(AgentError::BadOutput { .. })),
        "{result:?}"
    );
}

#[test]
fn megabytes_of_garbage_are_a_quick_bad_output_not_a_stall() {
    let junk = "{\"a\": ".repeat(900_000);
    let started = std::time::Instant::now();
    let (result, notes) = parse(&junk);
    assert!(
        matches!(result, Err(AgentError::BadOutput { .. })),
        "{result:?}"
    );
    assert!(started.elapsed().as_secs() < 5);
    assert!(notes.iter().any(|n| n.contains("cut")), "{notes:?}");
}

#[test]
fn the_intent_is_held_to_its_ranges_and_a_broken_one_is_dropped_with_a_note() {
    let text = r##"{"answer": "x", "intent": {"meaning": "praise", "confidence": 7, "rationale": "REASON"}}"##
        .replace("REASON", &"r".repeat(1_000));
    let reply = parse(&text).0.expect("reads");
    let intent = reply.intent.expect("intent");
    assert_eq!(intent.meaning, Some(Meaning::Praise));
    assert!((intent.confidence - 1.0).abs() < 1e-6, "held to 1");
    assert_eq!(
        intent.rationale.chars().count(),
        Limits::default().max_rationale_chars
    );

    let other = parse(r##"{"answer": "x", "intent": {"meaning": "weird", "confidence": -2}}"##)
        .0
        .expect("reads");
    let intent = other.intent.expect("intent");
    assert_eq!(intent.meaning, None, "an unknown meaning is other");
    assert!(intent.confidence.abs() < 1e-6, "held to 0");

    let (broken, notes) = parse(r##"{"answer": "x", "intent": "issue"}"##);
    assert!(broken.expect("reads").intent.is_none());
    assert!(notes.iter().any(|n| n.contains("intent")), "{notes:?}");
}

#[test]
fn a_reply_written_by_this_module_reads_back_as_itself() {
    let original = parse(FULL).0.expect("reads");
    let text = reply_text(&original);
    let again = parse(&text).0.expect("reads back");
    assert_eq!(again, original);
    let json = agent_json::parse(&text, 1 << 20).expect("valid json");
    assert!(agent_json::array_of(&json, "suggestions").is_some());
}

#[test]
fn only_the_exploration_reads_the_workspace() {
    assert!(reads_workspace(Tools::ReadOnly));
    assert!(!reads_workspace(Tools::None));
}
