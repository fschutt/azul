//! The problems and the output lists: what a report does to both, the same problem twice, the caps, the cut text.

use super::*;

#[test]
fn a_reported_problem_is_listed_and_said_in_the_output() {
    let mut d = Diagnostics::default();
    d.error("src/main.rs could not be read: gone", Some("src/main.rs"));
    assert_eq!(d.problems().len(), 1);
    assert_eq!(d.problems()[0].file.as_deref(), Some("src/main.rs"));
    assert_eq!(d.problems()[0].severity, Severity::Error);
    assert_eq!(d.output().collect::<Vec<_>>(), ["src/main.rs could not be read: gone"]);
    assert_eq!((d.count(Severity::Error), d.count(Severity::Warning), d.count(Severity::Info)), (1, 0, 0));
}

#[test]
fn the_same_problem_twice_stands_once_at_the_end() {
    let mut d = Diagnostics::default();
    d.error("a failed", Some("a"));
    d.error("b failed", Some("b"));
    d.error("a failed", Some("a"));
    let messages: Vec<&str> = d.problems().iter().map(|p| p.message.as_str()).collect();
    assert_eq!(messages, ["b failed", "a failed"]);
    // another file with the same words is another problem
    d.error("a failed", Some("c"));
    assert_eq!(d.problems().len(), 3);
    // the output keeps every line it was told
    assert_eq!(d.output_len(), 4);
}

#[test]
fn the_lists_keep_the_newest_and_never_grow_past_their_caps() {
    let mut d = Diagnostics::default();
    for i in 0..(MAX_PROBLEMS + 50) {
        d.error(&format!("problem {i}"), None);
    }
    assert_eq!(d.problems().len(), MAX_PROBLEMS);
    assert_eq!(d.problems()[0].message, "problem 50", "the oldest went");
    assert_eq!(d.output_len(), MAX_OUTPUT.min(MAX_PROBLEMS + 50));
    for i in 0..(MAX_OUTPUT + 10) {
        d.say(&format!("line {i}"));
    }
    assert_eq!(d.output_len(), MAX_OUTPUT);
    assert_eq!(d.output().last(), Some(&*format!("line {}", MAX_OUTPUT + 9)));
}

#[test]
fn a_long_message_is_cut_with_an_ellipsis_and_a_line_break_is_a_space() {
    let mut d = Diagnostics::default();
    d.say(&"x".repeat(MAX_CHARS + 100));
    let line = d.output().next().expect("a line");
    assert_eq!(line.chars().count(), MAX_CHARS);
    assert!(line.ends_with('\u{2026}'));
    d.say("first\nsecond\tthird\r\n");
    assert_eq!(d.output().nth(1), Some("first second third"));
    d.say("  padded  ");
    assert_eq!(d.output().nth(2), Some("padded"));
}

#[test]
fn a_cut_never_splits_a_character() {
    let mut d = Diagnostics::default();
    d.say(&"\u{20ac}".repeat(MAX_CHARS + 5));
    assert_eq!(d.output().next().map(|l| l.chars().count()), Some(MAX_CHARS));
}

#[test]
fn clearing_each_list_leaves_the_other() {
    let mut d = Diagnostics::default();
    d.error("e", None);
    d.say("o");
    d.clear_problems();
    assert!(d.problems().is_empty());
    assert_eq!(d.output_len(), 2);
    d.clear_output();
    assert_eq!(d.output_len(), 0);
}

#[test]
fn a_warning_and_an_info_are_counted_apart_from_the_errors() {
    let mut d = Diagnostics::default();
    d.report(Severity::Warning, "w", None, None);
    d.report(Severity::Info, "i", Some("f"), Some(3));
    d.error("e", None);
    assert_eq!((d.count(Severity::Error), d.count(Severity::Warning), d.count(Severity::Info)), (1, 1, 1));
    assert_eq!(d.problems()[1].line, Some(3));
}

#[test]
fn every_severity_has_an_icon_and_a_name_of_its_own() {
    let all = [Severity::Error, Severity::Warning, Severity::Info];
    let mut icons: Vec<&str> = all.iter().map(|s| s.icon()).collect();
    icons.sort_unstable();
    icons.dedup();
    assert_eq!(icons.len(), 3);
    let mut names: Vec<&str> = all.iter().map(|s| s.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 3);
}
