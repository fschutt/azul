//! What the keys of the row do to a file: the text typed in, the pairs, Tab and Shift+Tab over one line
//! and over several, the arrows over lines with wide characters; and the table of the keys.

use super::*;
use crate::buffer::TextBuffer;

const TEXT: &str = "fn main() {\n    let x = 1;\n\tlet y = 2;\n\n    ü€x\n}\n";

fn buffer(text: &str) -> TextBuffer {
    TextBuffer::from_text(text)
}

/// `action` pressed with the selection `anchor`..`head` in `text`; the outcome, and `text` after the edits.
fn run(text: &str, action: KeyAction, anchor: Pos, head: Pos) -> Option<(Outcome, String)> {
    let b = buffer(text);
    let count = b.line_count();
    let outcome = press(action, &|i| b.line(i), count, anchor, head, TAB_WIDTH)?;
    let mut after = b.clone();
    after.apply(&outcome.edits);
    Some((outcome, after.to_file_text()))
}

fn caret(line: usize, column: usize) -> Pos {
    Pos::new(line, column)
}

#[test]
fn a_character_typed_in_goes_to_the_caret_and_the_caret_after_it() {
    let (o, text) = run("ab\n", KeyAction::Insert(";"), caret(0, 1), caret(0, 1)).expect("an edit");
    assert_eq!(text, "a;b\n");
    assert_eq!((o.anchor, o.head), (caret(0, 2), caret(0, 2)));
}

#[test]
fn a_character_typed_over_a_selection_replaces_it() {
    let (o, text) = run("abcdef\n", KeyAction::Insert("="), caret(0, 1), caret(0, 4)).expect("an edit");
    assert_eq!(text, "a=ef\n");
    assert_eq!(o.head, caret(0, 2));
    // the selection made from the right to the left is the same selection
    let (_, backwards) = run("abcdef\n", KeyAction::Insert("="), caret(0, 4), caret(0, 1)).expect("an edit");
    assert_eq!(backwards, "a=ef\n");
}

#[test]
fn a_pair_is_typed_with_the_caret_between() {
    let (o, text) = run("f\n", KeyAction::Pair("{", "}"), caret(0, 1), caret(0, 1)).expect("an edit");
    assert_eq!(text, "f{}\n");
    assert_eq!((o.anchor, o.head), (caret(0, 2), caret(0, 2)));
    let (o, text) = run("\n", KeyAction::Pair("\"", "\""), caret(0, 0), caret(0, 0)).expect("an edit");
    assert_eq!(text, "\"\"\n");
    assert_eq!(o.head, caret(0, 1));
}

#[test]
fn a_pair_around_a_selection_wraps_it_and_keeps_it_selected() {
    let (o, text) = run("a bc d\n", KeyAction::Pair("(", ")"), caret(0, 2), caret(0, 4)).expect("an edit");
    assert_eq!(text, "a (bc) d\n");
    assert_eq!((o.anchor, o.head), (caret(0, 3), caret(0, 5)), "the selection is on bc");
    // over two lines: the end is on another line, its column does not move
    let (o, text) = run("a\nb\nc\n", KeyAction::Pair("[", "]"), caret(0, 0), caret(1, 1)).expect("an edit");
    assert_eq!(text, "[a\nb]\nc\n");
    assert_eq!((o.anchor, o.head), (caret(0, 1), caret(1, 1)));
}

#[test]
fn tab_in_a_line_goes_to_the_next_tab_stop() {
    let (o, text) = run("ab\n", KeyAction::Tab, caret(0, 2), caret(0, 2)).expect("an edit");
    assert_eq!(text, "ab  \n", "two columns to the stop at four");
    assert_eq!(o.head, caret(0, 4));
    let (_, text) = run("\n", KeyAction::Tab, caret(0, 0), caret(0, 0)).expect("an edit");
    assert_eq!(text, "    \n", "a whole level at a stop");
    let (_, text) = run("abcd\n", KeyAction::Tab, caret(0, 4), caret(0, 4)).expect("an edit");
    assert_eq!(text, "abcd    \n");
}

#[test]
fn the_tab_stops_count_characters_and_tabs_not_bytes() {
    // two characters of 2 and 3 bytes: the caret is at byte 5, visual column 2
    let (_, text) = run("ü€x\n", KeyAction::Tab, caret(0, 5), caret(0, 5)).expect("an edit");
    assert_eq!(text, "ü€  x\n");
    // a tab char counts to its stop: "\ta" has the caret at column 5 after the a
    let (_, text) = run("\ta\n", KeyAction::Tab, caret(0, 2), caret(0, 2)).expect("an edit");
    assert_eq!(text, "\ta   \n");
}

#[test]
fn tab_over_several_lines_indents_each_non_empty_one_in_its_own_style() {
    let (o, text) = run(TEXT, KeyAction::Tab, caret(1, 6), caret(3, 0)).expect("edits");
    assert_eq!(
        text,
        "fn main() {\n        let x = 1;\n\t\tlet y = 2;\n\n    ü€x\n}\n",
        "lines 1 and 2 indented (spaces, then a tab: each in its own style); line 3 is where the selection ends at column 0 and is left alone"
    );
    assert_eq!(o.anchor, caret(1, 10), "the start moved by the four spaces of its line");
    assert_eq!(o.head, caret(3, 0), "the end at column 0 of an untouched line stays");
}

#[test]
fn tab_does_not_indent_empty_lines() {
    let (_, text) = run("a\n\nb\n", KeyAction::Tab, caret(0, 0), caret(2, 1)).expect("edits");
    assert_eq!(text, "    a\n\n    b\n");
}

#[test]
fn shift_tab_takes_a_level_off_each_line_it_covers() {
    let (o, text) = run(TEXT, KeyAction::BackTab, caret(1, 8), caret(2, 5)).expect("edits");
    assert_eq!(text, "fn main() {\nlet x = 1;\nlet y = 2;\n\n    ü€x\n}\n");
    assert_eq!(o.anchor, caret(1, 4), "the caret moves back with its line: 8 to 4");
    assert_eq!(o.head, caret(2, 4), "a tab is one byte: 5 to 4");
    // no indentation at the caret's line: nothing to do
    assert!(run("x\n", KeyAction::BackTab, caret(0, 1), caret(0, 1)).is_none());
}

#[test]
fn shift_tab_takes_only_the_spaces_there_are() {
    let (_, text) = run("  a\n", KeyAction::BackTab, caret(0, 3), caret(0, 3)).expect("an edit");
    assert_eq!(text, "a\n", "two spaces, a level is four at most");
}

#[test]
fn left_and_right_move_by_characters_across_the_line_ends() {
    // line 4 is "    ü€x": bytes 0-3 spaces, ü 4-5, € 6-8, x 9
    let o = run(TEXT, KeyAction::Right, caret(4, 4), caret(4, 4)).expect("a move").0;
    assert_eq!(o.head, caret(4, 6), "past the 2-byte ü");
    let o = run(TEXT, KeyAction::Right, caret(4, 6), caret(4, 6)).expect("a move").0;
    assert_eq!(o.head, caret(4, 9), "past the 3-byte €");
    let o = run(TEXT, KeyAction::Left, caret(4, 9), caret(4, 9)).expect("a move").0;
    assert_eq!(o.head, caret(4, 6));
    let o = run(TEXT, KeyAction::Left, caret(4, 0), caret(4, 0)).expect("a move").0;
    assert_eq!(o.head, caret(3, 0), "to the end of the line before (an empty line)");
    let o = run(TEXT, KeyAction::Right, caret(4, 10), caret(4, 10)).expect("a move").0;
    assert_eq!(o.head, caret(5, 0), "to the start of the next line");
    assert!(run(TEXT, KeyAction::Left, caret(0, 0), caret(0, 0)).is_none(), "nothing before the file");
    assert!(run("a", KeyAction::Right, caret(0, 1), caret(0, 1)).is_none(), "nothing after the last line's end");
}

#[test]
fn an_arrow_collapses_the_selection_to_where_the_caret_goes() {
    let o = run(TEXT, KeyAction::Right, caret(1, 4), caret(1, 8)).expect("a move").0;
    assert_eq!((o.anchor, o.head), (caret(1, 9), caret(1, 9)));
    assert!(o.edits.is_empty(), "an arrow edits nothing");
}

#[test]
fn up_and_down_keep_the_character_column_on_a_line_of_wide_characters() {
    // from "    ü€x" after the € (5 characters in) to the line above ("\tlet y = 2;"): character 5 is the e of let? chars: \t l e t _ y
    let o = run(TEXT, KeyAction::Up, caret(4, 9), caret(4, 9)).expect("a move").0;
    assert_eq!(o.head.line, 3, "the empty line above: the column is clamped to its end");
    assert_eq!(o.head.column, 0);
    let o = run(TEXT, KeyAction::Up, caret(2, 3), caret(2, 3)).expect("a move").0;
    assert_eq!(o.head, caret(1, 3));
    let o = run(TEXT, KeyAction::Down, caret(1, 9), caret(1, 9)).expect("a move").0;
    assert_eq!(o.head, caret(2, 9.min("\tlet y = 2;".len())));
    assert!(run(TEXT, KeyAction::Up, caret(0, 3), caret(0, 3)).is_none(), "nothing above the first line");
    assert!(run("a\nb", KeyAction::Down, caret(1, 0), caret(1, 0)).is_none(), "nothing under the last line");
}

#[test]
fn a_caret_inside_a_character_is_taken_back_to_its_start() {
    // byte 3 of "ü€" is inside the € (bytes 2-4): Left goes to before the ü, Right to after the €
    let o = run("ü€\n", KeyAction::Left, caret(0, 3), caret(0, 3)).expect("a move").0;
    assert_eq!(o.head, caret(0, 0), "never a column in the middle of a character");
    let o = run("ü€\n", KeyAction::Right, caret(0, 3), caret(0, 3)).expect("a move").0;
    assert_eq!(o.head, caret(0, 5));
}

#[test]
fn the_keys_are_thirteen_each_with_a_name_a_label_or_an_icon_and_an_action_of_its_own() {
    assert_eq!(KEYS.len(), 13);
    let mut names: Vec<&str> = KEYS.iter().map(|k| k.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), KEYS.len(), "no two keys share a name");
    for k in &KEYS {
        assert!(!k.name.is_empty(), "{k:?}");
        // a control shows either text or an icon
        assert!(k.label.is_empty() != k.icon.is_empty(), "{k:?}: text or an icon, never both, never neither");
    }
    assert_eq!(KEYS[0].action, KeyAction::Tab);
    assert!(KEYS.iter().any(|k| k.action == KeyAction::Pair("{", "}")));
    assert!(KEYS.iter().any(|k| k.action == KeyAction::Down));
}
