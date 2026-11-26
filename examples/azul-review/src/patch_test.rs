//! A resolved conversation as one patch file: the unified diff of what the reviewer took against the lines of the file,
//! under a small header. Pure: lines and numbers, no disk, no git.

use super::*;
use crate::agent_review::{Pos, Suggestion};

fn lines() -> Vec<String> {
    (1..=12).map(|i| format!("line {i}")).collect()
}

fn insert(after: usize, rows: &[&str]) -> Suggestion {
    Suggestion::InsertLines { after_line: after, lines: rows.iter().map(|s| (*s).to_string()).collect() }
}

fn replace(line: usize, from: usize, to: usize, old: &str, text: &str) -> Suggestion {
    Suggestion::ReplaceRange { start: Pos { line, col: from }, end: Pos { line, col: to }, old: old.to_string(), text: text.to_string() }
}

fn remove(line: usize, from: usize, to: usize, old: &str) -> Suggestion {
    Suggestion::RemoveRange { start: Pos { line, col: from }, end: Pos { line, col: to }, old: old.to_string() }
}

fn diff(accepted: &[Suggestion]) -> Diff {
    let refs: Vec<&Suggestion> = accepted.iter().collect();
    diff_for("src/lib.rs", &lines(), &refs).expect("a diff")
}

#[test]
fn an_inserted_line_is_a_hunk_with_three_lines_of_context_on_each_side() {
    let d = diff(&[insert(6, &["inserted"])]);
    assert!(d.changed);
    let expect = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,6 +4,7 @@\n line 4\n line 5\n line 6\n+inserted\n line 7\n line 8\n line 9\n";
    assert_eq!(d.text, expect);
}

#[test]
fn a_word_replaced_in_a_line_is_the_line_removed_and_the_line_added() {
    let d = diff(&[replace(3, 5, 6, "3", "three")]);
    let expect = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,6 +1,6 @@\n line 1\n line 2\n-line 3\n+line three\n line 4\n line 5\n line 6\n";
    assert_eq!(d.text, expect);
}

#[test]
fn a_word_removed_leaves_the_rest_of_the_line() {
    let d = diff(&[remove(10, 0, 5, "line ")]);
    assert!(d.text.contains("-line 10\n+10\n"), "{}", d.text);
    assert!(d.text.starts_with("--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -7,6 +7,6 @@\n"), "{}", d.text);
}

#[test]
fn context_stops_at_the_ends_of_the_file() {
    let top = diff(&[insert(0, &["first"])]);
    assert!(top.text.contains("@@ -1,3 +1,4 @@\n+first\n line 1\n line 2\n line 3\n"), "{}", top.text);
    let foot = diff(&[insert(12, &["last"])]);
    assert!(foot.text.contains("@@ -10,3 +10,4 @@\n line 10\n line 11\n line 12\n+last\n"), "{}", foot.text);
}

#[test]
fn edits_near_each_other_share_a_hunk_and_far_apart_ones_have_their_own() {
    let near = diff(&[insert(2, &["a"]), insert(5, &["b"])]);
    assert_eq!(near.text.matches("@@ ").count(), 1, "{}", near.text);
    assert!(near.text.contains("+a\n") && near.text.contains("+b\n"));
    let far = diff(&[insert(1, &["a"]), insert(12, &["b"])]);
    assert_eq!(far.text.matches("@@ ").count(), 2, "{}", far.text);
}

#[test]
fn a_suggestion_about_text_that_is_not_there_is_left_out_with_a_word_and_the_others_are_kept() {
    let refs = [replace(3, 5, 6, "9", "nine"), insert(6, &["kept"])];
    let refs: Vec<&Suggestion> = refs.iter().collect();
    let d = diff_for("src/lib.rs", &lines(), &refs).expect("the rest is a diff");
    assert!(d.text.contains("+kept\n"));
    assert!(!d.text.contains("nine"));
    assert_eq!(d.left_out.len(), 1);
    assert!(d.left_out[0].contains("line 3"), "{:?}", d.left_out);
}

#[test]
fn two_edits_over_the_same_words_are_one_and_the_second_is_left_out() {
    let refs = [replace(3, 0, 4, "line", "LINE"), replace(3, 2, 6, "ne 3", "NE 3")];
    let refs: Vec<&Suggestion> = refs.iter().collect();
    let d = diff_for("src/lib.rs", &lines(), &refs).expect("a diff");
    assert!(d.text.contains("+LINE 3\n"), "{}", d.text);
    assert_eq!(d.left_out.len(), 1, "{:?}", d.left_out);
}

#[test]
fn nothing_taken_is_no_diff() {
    let refs: Vec<&Suggestion> = Vec::new();
    let d = diff_for("src/lib.rs", &lines(), &refs).expect("an empty diff");
    assert!(!d.changed);
    assert!(d.text.is_empty());
    // everything left out is no diff either
    let stale = [replace(3, 5, 6, "9", "nine")];
    let refs: Vec<&Suggestion> = stale.iter().collect();
    assert!(!diff_for("src/lib.rs", &lines(), &refs).expect("empty").changed);
}

#[test]
fn the_columns_of_a_line_that_has_tabs_are_the_columns_of_what_the_page_showed() {
    // the page shows a tab as four spaces: the agent's columns count them, the file has the tab
    let raw = vec!["fn main() {".to_string(), "\tlet x = 1;".to_string(), "}".to_string()];
    // "    let x = 1;" on the page: "x" is at column 8
    let edits = [replace(2, 8, 9, "x", "y")];
    let refs: Vec<&Suggestion> = edits.iter().collect();
    let d = diff_for("a.rs", &raw, &refs).expect("a diff");
    assert!(d.text.contains("-\tlet x = 1;\n+\tlet y = 1;\n"), "the tab is kept: {}", d.text);
    // a range that starts in the middle of the four spaces of a tab is not a place in the file
    let odd = [replace(2, 2, 8, "  let ", "")];
    let refs: Vec<&Suggestion> = odd.iter().collect();
    let d = diff_for("a.rs", &raw, &refs).expect("empty");
    assert!(!d.changed && d.left_out.len() == 1, "{:?}", d.left_out);
}

#[test]
fn two_edits_in_one_line_that_do_not_touch_are_one_changed_line() {
    // "line 3": "li" -> "LI" and "3" -> "three" (the one taken first is the one that is kept where they are over each other)
    let edits = [replace(3, 5, 6, "3", "three"), replace(3, 0, 2, "li", "LI")];
    let refs: Vec<&Suggestion> = edits.iter().collect();
    let d = diff_for("src/lib.rs", &lines(), &refs).expect("a diff");
    assert!(d.left_out.is_empty(), "{:?}", d.left_out);
    assert_eq!(d.text.matches("\n-line 3\n").count(), 1, "{}", d.text);
    assert!(d.text.contains("\n+LIne three\n"), "{}", d.text);
}

#[test]
fn a_range_over_two_lines_joins_them() {
    // from the end of "line 3" through the start of "line 4": "\nli" goes
    let edits = [Suggestion::RemoveRange { start: Pos { line: 3, col: 6 }, end: Pos { line: 4, col: 2 }, old: "\nli".to_string() }];
    let refs: Vec<&Suggestion> = edits.iter().collect();
    let d = diff_for("src/lib.rs", &lines(), &refs).expect("a diff");
    assert!(d.text.contains("\n-line 3\n-line 4\n+line 3ne 4\n"), "{}", d.text);
    assert!(d.text.contains("@@ -1,7 +1,6 @@\n"), "one line less: {}", d.text);
}

#[test]
fn a_second_hunk_counts_its_lines_from_where_the_first_left_them() {
    // two lines are put after line 1, so the second hunk's lines are two further on in the new file
    let edits = [insert(1, &["a", "b"]), replace(12, 5, 7, "12", "twelve")];
    let refs: Vec<&Suggestion> = edits.iter().collect();
    let d = diff_for("src/lib.rs", &lines(), &refs).expect("a diff");
    assert_eq!(d.text.matches("@@ ").count(), 2, "{}", d.text);
    assert!(d.text.contains("@@ -1,4 +1,6 @@\n"), "{}", d.text);
    assert!(d.text.contains("@@ -9,4 +11,4 @@\n"), "{}", d.text);
}

#[test]
fn a_replacement_that_changes_nothing_is_no_diff() {
    let edits = [replace(3, 5, 6, "3", "3")];
    let refs: Vec<&Suggestion> = edits.iter().collect();
    let d = diff_for("src/lib.rs", &lines(), &refs).expect("empty");
    assert!(!d.changed && d.text.is_empty(), "{}", d.text);
}

#[test]
fn lines_put_after_a_line_the_file_does_not_have_are_left_out() {
    let edits = [insert(40, &["far"]), insert(2, &[])];
    let refs: Vec<&Suggestion> = edits.iter().collect();
    let d = diff_for("src/lib.rs", &lines(), &refs).expect("empty");
    assert!(!d.changed);
    assert_eq!(d.left_out.len(), 2, "{:?}", d.left_out);
}

#[test]
fn a_file_with_windows_line_ends_keeps_them_in_what_is_added() {
    let raw: Vec<String> = (1..=3).map(|i| format!("line {i}\r")).collect();
    let edits = [insert(1, &["added"])];
    let refs: Vec<&Suggestion> = edits.iter().collect();
    let d = diff_for("a.rs", &raw, &refs).expect("a diff");
    assert!(d.text.contains("+added\r\n"), "{:?}", d.text);
    assert!(d.text.contains(" line 1\r\n"), "{:?}", d.text);
}

#[test]
fn the_patch_has_a_header_before_its_diff_that_says_who_asked_what_over_which_revision() {
    let left_out = ["line 9: the words \"x\" are not what the file says there any more".to_string()];
    let header = Header {
        conversation: 7,
        outcome: Outcome::Accepted,
        base: Some("3833f10608241be94251152cd9c4da3de1b6a283"),
        time_unix_ms: 1_764_135_612_000,
        said: "rename step three to first step",
        understood: Some("rename step_3 to first_step"),
        rationale: "step_3 is renamed on its one line.\nNothing else calls it.",
        left_out: &left_out,
    };
    let text = render(&header, &[("src/lib.rs".to_string(), "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-a\n+b\n".to_string())]);
    let head: Vec<&str> = text.lines().take(11).collect();
    assert_eq!(head[0], "AzReview-Patch: 1");
    assert_eq!(head[1], "Conversation: 7");
    assert_eq!(head[2], "Outcome: accepted");
    assert_eq!(head[3], "Base: 3833f10608241be94251152cd9c4da3de1b6a283");
    assert_eq!(head[4], "Time: 2025-11-26T05:40:12Z");
    assert_eq!(head[5], "Said: rename step three to first step");
    assert_eq!(head[6], "Understood: rename step_3 to first_step");
    assert_eq!(head[7], "Rationale: step_3 is renamed on its one line. Nothing else calls it.");
    assert_eq!(head[8], "Files: src/lib.rs");
    assert_eq!(head[9], "Left-out: line 9: the words \"x\" are not what the file says there any more");
    assert_eq!(head[10], "");
    assert!(text.contains("diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n"), "{text}");
    assert!(text.ends_with("+b\n"));
}

#[test]
fn a_header_with_no_revision_and_no_understanding_says_so_and_never_breaks_a_line() {
    let header = Header { conversation: 1, outcome: Outcome::Closed, base: None, time_unix_ms: 0, said: "two\nlines", understood: None, rationale: "", left_out: &[] };
    let text = render(&header, &[]);
    assert!(text.contains("Base: none\n"));
    assert!(text.contains("Said: two lines\n"), "a header line is one line: {text}");
    assert!(!text.contains("Understood:"), "{text}");
    assert!(!text.contains("Left-out:"), "{text}");
    assert!(text.contains("Time: 1970-01-01T00:00:00Z\n"));
}

#[test]
fn a_conversation_that_changed_nothing_is_a_header_alone_that_says_how_it_ended() {
    for (outcome, word) in [(Outcome::Accepted, "accepted"), (Outcome::Rejected, "rejected"), (Outcome::Answered, "answered"), (Outcome::Closed, "closed")] {
        assert_eq!(outcome.name(), word);
        let header = Header { conversation: 3, outcome, base: None, time_unix_ms: 1_764_135_612_000, said: "why is this here", understood: None, rationale: "It is read twice.", left_out: &[] };
        let text = render(&header, &[]);
        assert!(text.contains(&format!("Outcome: {word}\n")), "{text}");
        assert!(text.contains("Files: none\n"), "{text}");
        assert!(!text.contains("diff --git"), "no diff where nothing was taken: {text}");
        assert!(text.ends_with("\n\n"), "the header ends with an empty line: {text:?}");
    }
}

#[test]
fn a_patch_has_a_name_that_sorts_by_time_and_says_which_conversation() {
    assert_eq!(file_name(7, 1_764_135_612_000), "20251126-054012-c7.patch");
    assert_eq!(file_name(12, 0), "19700101-000000-c12.patch");
    // the time is the calendar's, over a leap day and a year's end
    assert_eq!(time_text(951_782_400_000), "2000-02-29T00:00:00Z");
    assert_eq!(time_text(1_767_225_599_000), "2025-12-31T23:59:59Z");
}

#[test]
fn only_what_the_app_names_a_patch_is_a_patch_name() {
    assert!(is_name(&file_name(7, 1_764_135_612_000)));
    assert!(is_name(&file_name(u64::MAX, u64::MAX / 1000)), "the longest name is within the bound");
    for bad in ["", ".patch", "a.txt", "../x.patch", "a/b.patch", "a\\b.patch", "..patch", ".hidden.patch", "a b.patch", "é.patch", "a..b.patch"] {
        assert!(!is_name(bad), "{bad:?}");
    }
    assert!(!is_name(&format!("{}.patch", "a".repeat(80))), "too long");
}
