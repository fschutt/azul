//! What the suggestions of the agents do to the lines of a page: the words they would take greyed and struck, the words
//! the reviewer took in the colour of the page, the words that would take their place yellow and green, the lines that
//! would be inserted. Pure: lines and numbers.

use super::*;
use crate::{
    agent_review::{Pos, Suggestion},
    conversation::{Decision, Proposal},
    model::{Note, NoteState},
};

fn lines() -> Vec<String> {
    vec![
        "let stale = foo(a, b);".to_string(),
        "    let n = items.len();".to_string(),
        "// é and ü are two bytes each".to_string(),
        "fn tail() {}".to_string(),
    ]
}

fn note_with(proposals: Vec<(Suggestion, Decision)>) -> Note {
    let mut n = Note {
        id: 1,
        page: 0,
        x: 0.0,
        y: 0.0,
        text: String::new(),
        language: String::new(),
        state: NoteState::Done,
        talk: crate::conversation::Conversation::default(),
    };
    n.talk.proposals = proposals.into_iter().map(|(suggestion, decision)| Proposal { suggestion, decision }).collect();
    n
}

fn remove(line: usize, from: usize, to: usize, old: &str) -> Suggestion {
    Suggestion::RemoveRange { start: Pos { line, col: from }, end: Pos { line, col: to }, old: old.to_string() }
}

fn replace(line: usize, from: usize, to: usize, old: &str, text: &str) -> Suggestion {
    Suggestion::ReplaceRange { start: Pos { line, col: from }, end: Pos { line, col: to }, old: old.to_string(), text: text.to_string() }
}

#[test]
fn a_word_the_agent_would_take_is_struck_and_one_the_reviewer_took_is_in_the_colour_of_the_page() {
    let waiting = marks_of(&[note_with(vec![(remove(1, 4, 9, "stale"), Decision::Waiting)])], 1, &lines());
    assert_eq!(waiting[0].struck, vec![(4, 9)]);
    assert!(waiting[0].hidden.is_empty());
    let taken = marks_of(&[note_with(vec![(remove(1, 4, 9, "stale"), Decision::Accepted)])], 1, &lines());
    assert_eq!(taken[0].hidden, vec![(4, 9)]);
    assert!(taken[0].struck.is_empty());
    let refused = marks_of(&[note_with(vec![(remove(1, 4, 9, "stale"), Decision::Rejected)])], 1, &lines());
    assert_eq!(refused[0], LineMarks::default(), "a refused suggestion leaves the line as it was");
    assert_eq!(waiting[1], LineMarks::default(), "and the other lines are not touched");
}

#[test]
fn a_replacement_strikes_the_old_words_and_shows_the_new_ones_after_them_yellow_then_green() {
    let waiting = marks_of(&[note_with(vec![(replace(1, 12, 15, "foo", "bar"), Decision::Waiting)])], 1, &lines());
    assert_eq!(waiting[0].struck, vec![(12, 15)]);
    assert_eq!(waiting[0].added, vec![Added { at: 15, text: "bar".to_string(), accepted: false }]);
    let taken = marks_of(&[note_with(vec![(replace(1, 12, 15, "foo", "bar"), Decision::Accepted)])], 1, &lines());
    assert_eq!(taken[0].hidden, vec![(12, 15)]);
    assert_eq!(taken[0].added, vec![Added { at: 15, text: "bar".to_string(), accepted: true }]);
    let refused = marks_of(&[note_with(vec![(replace(1, 12, 15, "foo", "bar"), Decision::Rejected)])], 1, &lines());
    assert_eq!(refused[0], LineMarks::default());
}

#[test]
fn a_suggestion_about_text_that_is_not_there_any_more_marks_nothing() {
    // the file says "stale" at 4 to 9, not "fresh"
    let stale = marks_of(&[note_with(vec![(remove(1, 4, 9, "fresh"), Decision::Waiting)])], 1, &lines());
    assert!(stale.iter().all(|m| *m == LineMarks::default()));
    // a range past the end of the line, a range in the middle of a letter, a line that is not on the page
    let odd = marks_of(
        &[note_with(vec![
            (remove(1, 40, 50, "x"), Decision::Waiting),
            (remove(3, 3, 4, "é"), Decision::Waiting),
            (remove(99, 0, 1, "l"), Decision::Waiting),
            (remove(0, 0, 1, "l"), Decision::Waiting),
        ])],
        1,
        &lines(),
    );
    assert!(odd.iter().all(|m| *m == LineMarks::default()), "{odd:?}");
}

#[test]
fn a_range_over_several_lines_marks_the_part_of_each() {
    let multi = Suggestion::RemoveRange {
        start: Pos { line: 1, col: 17 },
        end: Pos { line: 2, col: 7 },
        old: "b);\n    let".to_string(),
    };
    let lines = lines();
    // line 1 is "let stale = foo(a, b);": from byte 19 ("b);" starts at 19)
    let multi = match multi {
        Suggestion::RemoveRange { end, old, .. } => Suggestion::RemoveRange { start: Pos { line: 1, col: 19 }, end, old },
        other => other,
    };
    let marks = marks_of(&[note_with(vec![(multi, Decision::Waiting)])], 1, &lines);
    assert_eq!(marks[0].struck, vec![(19, 22)], "to the end of the first line");
    assert_eq!(marks[1].struck, vec![(0, 7)], "from the start of the last");
}

#[test]
fn marks_only_come_from_the_lines_of_the_page_whose_first_line_is_given() {
    // the page starts at line 47: line 1 of the file is not on it, line 47 is its first line
    let marks = marks_of(&[note_with(vec![(remove(47, 4, 9, "stale"), Decision::Waiting), (remove(1, 4, 9, "stale"), Decision::Waiting)])], 47, &lines());
    assert_eq!(marks[0].struck, vec![(4, 9)]);
    assert_eq!(marks[1], LineMarks::default());
    assert!(marks[2..].iter().all(|m| *m == LineMarks::default()));
}

#[test]
fn the_suggestions_of_several_conversations_come_together_and_overlapping_ranges_join() {
    let a = note_with(vec![(remove(1, 4, 9, "stale"), Decision::Waiting)]);
    let mut b = note_with(vec![(remove(1, 8, 11, "e =", ), Decision::Waiting)]);
    b.id = 2;
    let marks = marks_of(&[a, b], 1, &lines());
    assert_eq!(marks[0].struck, vec![(4, 11)], "the two ranges touch and are one");
}

#[test]
fn lines_the_agent_would_insert_are_listed_with_where_they_go_and_whether_they_were_taken() {
    let insert = |after, rows: &[&str]| Suggestion::InsertLines { after_line: after, lines: rows.iter().map(|s| (*s).to_string()).collect() };
    let mut a = note_with(vec![(insert(2, &["let m = 1;", "let k = 2;"]), Decision::Waiting), (insert(4, &["// end"]), Decision::Accepted), (insert(9, &["far"]), Decision::Waiting)]);
    a.id = 7;
    let rows = inserts_of(&[a], 1, 4);
    assert_eq!(rows.len(), 2, "the one after line 9 is not on this page");
    assert_eq!((rows[0].after_line, rows[0].rows.len(), rows[0].accepted, rows[0].conversation), (2, 2, false, 7));
    assert_eq!((rows[1].after_line, rows[1].accepted), (4, true));
    let rejected = note_with(vec![(insert(2, &["x"]), Decision::Rejected)]);
    assert!(inserts_of(&[rejected], 1, 4).is_empty(), "a refused insertion is none");
    // lines inserted after the last line of a page belong to that page (it grows), not to the next one
    let at_foot = note_with(vec![(insert(46, &["foot"]), Decision::Waiting)]);
    assert!(inserts_of(&[at_foot.clone()], 47, 3).is_empty(), "not the next page's");
    assert_eq!(inserts_of(&[at_foot], 1, 46).len(), 1, "the page that has the line they follow");
    assert_eq!(inserts_of(&[note_with(vec![(insert(0, &["very first"]), Decision::Waiting)])], 1, 3).len(), 1, "before the first line of the file");
}

#[test]
fn a_line_is_drawn_in_pieces_each_with_its_look() {
    let line = "let stale = foo(a, b);";
    let marks = LineMarks {
        struck: vec![(12, 15)],
        hidden: vec![],
        added: vec![Added { at: 15, text: "bar".to_string(), accepted: false }],
    };
    let p = pieces(line, &[], &marks);
    let shown: Vec<(&str, Look)> = p.iter().map(|x| (x.text.as_str(), x.look)).collect();
    assert_eq!(
        shown,
        vec![("let stale = ", Look::Plain), ("foo", Look::Struck), ("bar", Look::Yellow), ("(a, b);", Look::Plain)]
    );
    assert_eq!(p[2].from, None, "added text is not in the file");
    assert_eq!(p[1].from, Some(12));
    // the pieces that are in the file say up to what they were: the line without the added text is the line
    let kept: String = p.iter().filter(|x| x.from.is_some()).map(|x| x.text.as_str()).collect();
    assert_eq!(kept, line);
}

#[test]
fn what_the_reviewer_slashed_and_what_an_agent_would_take_are_drawn_together() {
    let line = "let stale = foo(a, b);";
    let marks = LineMarks { struck: vec![(12, 15)], ..LineMarks::default() };
    let p = pieces(line, &[(4, 9)], &marks);
    let shown: Vec<(&str, Look)> = p.iter().map(|x| (x.text.as_str(), x.look)).collect();
    assert_eq!(shown, vec![("let ", Look::Plain), ("stale", Look::Hidden), (" = ", Look::Plain), ("foo", Look::Struck), ("(a, b);", Look::Plain)]);
    // hidden wins over struck where they cover the same bytes
    let both = pieces(line, &[(12, 15)], &marks);
    assert!(both.iter().any(|x| x.text == "foo" && x.look == Look::Hidden), "{both:?}");
    assert!(both.iter().all(|x| x.look != Look::Struck));
}

#[test]
fn text_added_past_the_end_of_a_line_or_in_the_middle_of_a_letter_is_never_a_panic() {
    let line = "é = 1";
    let marks = LineMarks {
        struck: vec![(1, 2), (40, 50)],
        hidden: vec![(1, 3)],
        added: vec![Added { at: 1, text: "mid".to_string(), accepted: true }, Added { at: 99, text: "tail".to_string(), accepted: false }],
    };
    let p = pieces(line, &[], &marks);
    let kept: String = p.iter().filter(|x| x.from.is_some()).map(|x| x.text.as_str()).collect();
    assert_eq!(kept, line, "the line is whole");
    assert!(p.iter().any(|x| x.text == "tail" && x.look == Look::Yellow), "past the end: after the line");
    assert!(p.iter().any(|x| x.text == "mid"), "inside a letter: after the letter it is in");
}

#[test]
fn a_line_with_no_marks_is_one_piece() {
    let p = pieces("fn tail() {}", &[], &LineMarks::default());
    assert_eq!(p.len(), 1);
    assert_eq!((p[0].text.as_str(), p[0].look, p[0].from), ("fn tail() {}", Look::Plain, Some(0)));
    assert!(pieces("", &[], &LineMarks::default()).is_empty() || pieces("", &[], &LineMarks::default())[0].text.is_empty());
}

#[test]
fn how_many_rows_each_page_grows_by_is_known_for_the_pages_that_have_lines_put_in() {
    let insert = |after, rows: &[&str]| Suggestion::InsertLines { after_line: after, lines: rows.iter().map(|s| (*s).to_string()).collect() };
    let a = note_with(vec![
        (insert(4, &["a", "b"]), Decision::Waiting),
        (insert(46, &["last of page 1"]), Decision::Accepted),
        (insert(47, &["first of page 2"]), Decision::Waiting),
        (insert(5, &["refused"]), Decision::Rejected),
        (remove(1, 4, 9, "stale"), Decision::Waiting),
    ]);
    let b = note_with(vec![(insert(0, &["before everything"]), Decision::Waiting), (insert(100, &["third page"]), Decision::Waiting), (insert(300, &["no such page"]), Decision::Waiting)]);
    let grown = extra_rows_by_page(&[a, b], |page| if page < 3 { 46 } else { 0 });
    // page 0: two after line 4, one after line 46 (it grows the page that has the line), one before the first line
    assert_eq!(grown.get(&0), Some(&4));
    assert_eq!(grown.get(&1), Some(&1), "after line 47: the second page");
    assert_eq!(grown.get(&2), Some(&1), "line 100 is on the third page");
    assert_eq!(grown.len(), 3, "line 300 is on a page the file does not have");
    assert_eq!(extra_rows_by_page(&[], |_| 46), std::collections::BTreeMap::new());
}

#[test]
fn lines_put_after_a_line_the_file_does_not_have_grow_no_page() {
    let far = Suggestion::InsertLines { after_line: 500, lines: vec!["x".to_string()] };
    // the file has 100 lines: 46, 46 and 8 on its pages
    let grown = extra_rows_by_page(&[note_with(vec![(far, Decision::Waiting)])], |page| [46, 46, 8].get(page).copied().unwrap_or(0));
    assert!(grown.is_empty(), "{grown:?}");
}
