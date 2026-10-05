//! The code view: columns, the window over a long text, a line's pieces, the
//! keys (edits, moves, multi-cursor), the pointer, the wheel, the build and
//! the looks.

use std::sync::{Arc, Mutex};

use azul_core::{dom::Dom, refany::RefAny, window::VirtualKeyCode as K};
use azul_css::AzString;

use super::fixtures::{over, sample, VecLines};
use super::*;
use crate::widgets::themes::{theme_blocks::checks, theme_checks as tc, UiTheme};

// ---- helpers ----

fn p(line: u32, column: u32) -> CodeViewPosition {
    CodeViewPosition::create(line, column)
}

fn at(cv: &mut CodeView, line: u32, column: u32) {
    cv.view.set_cursor(p(line, column));
}

fn cursors(cv: &mut CodeView, list: Vec<CodeViewCursor>) {
    cv.view.cursors = CodeViewCursorVec::from_vec(list);
}

fn heads(view: &CodeViewView) -> Vec<CodeViewPosition> {
    view.cursors.as_slice().iter().map(|c| c.head).collect()
}

const NONE: Mods = Mods {
    shift: false,
    primary: false,
    word: false,
    line: false,
    alt: false,
};

const SHIFT: Mods = Mods { shift: true, ..NONE };
const PRIMARY: Mods = Mods { primary: true, ..NONE };
const WORD: Mods = Mods { word: true, ..NONE };

/// `text` after an event's edits, applied in order - what the app does.
fn apply(text: &str, edits: &[CodeViewEdit]) -> String {
    let mut s = String::from(text);
    for e in edits {
        let offset = |s: &str, at: CodeViewPosition| -> usize {
            s.split('\n').take(at.line as usize).map(|l| l.len() + 1).sum::<usize>() + at.column as usize
        };
        let a = offset(&s, e.start);
        let b = offset(&s, e.end);
        s = format!("{}{}{}", &s[..a], e.text.as_str(), &s[b..]);
    }
    s
}

/// What `k` does to `cv` over `text`; the view must take the key.
fn key(cv: &CodeView, text: &str, k: K, mods: Mods) -> CodeViewEvent {
    key_event(cv, &VecLines::of(text), k, mods).expect("the view takes the key")
}

fn run(text: &str, kind: CodeTokenKind, selected: bool) -> Piece {
    Piece::Text {
        text: String::from(text),
        kind,
        selected,
    }
}

/// Every text under `node`, depth first, joined.
fn text_of(node: &Dom) -> String {
    tc::nodes(node)
        .into_iter()
        .filter_map(|(_, n)| n.root.get_node_type().get_text())
        .map(|s| String::from(s.as_str()))
        .collect()
}

/// `count` lines `"line <n>"`, generated on demand; every line asked for
/// is logged.
struct Generated {
    asked: Arc<Mutex<Vec<u32>>>,
}

extern "C" fn generated_line(mut data: RefAny, line: u32) -> CodeViewLine {
    if let Some(g) = data.downcast_ref::<Generated>() {
        g.asked.lock().expect("log").push(line);
    }
    CodeViewLine::create_plain(AzString::from(format!("line {line}")))
}

fn generated(count: u32, asked: &Arc<Mutex<Vec<u32>>>) -> CodeView {
    CodeView::create(count).with_viewport(600.0, 400.0).with_data_source(
        RefAny::new(Generated { asked: asked.clone() }),
        generated_line as CodeViewDataSourceCallbackType,
    )
}

fn hundred_lines() -> String {
    (0..100).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")
}

// ---- columns ----

#[test]
fn visual_columns_expand_tabs_to_the_next_tab_stop() {
    assert_eq!(visual_column("\tab", 1, 4), 4);
    assert_eq!(visual_column("a\tb", 2, 4), 4);
    assert_eq!(visual_column("ab\tc", 3, 4), 4);
    assert_eq!(visual_column("abcd\te", 5, 4), 8);
    assert_eq!(visual_column("\u{e4}b", 2, 4), 1, "a two-byte character is one column");
    assert_eq!(expand_tabs("a\tb", 0, 4), "a   b");
    assert_eq!(expand_tabs("\tx", 2, 4), "  x", "a tab fills to the stop after its column");
    assert_eq!(byte_at_visual("\tab", 1, 4), 0, "a column in a tab's first half: before it");
    assert_eq!(byte_at_visual("\tab", 3, 4), 1, "in its second half: after it");
    assert_eq!(byte_at_visual("\tab", 5, 4), 2);
    assert_eq!(byte_at_visual("ab", 9, 4), 2, "past the end is the end");
}

#[test]
fn columns_and_word_jumps_never_split_a_character() {
    // l e t _ g r ö ö ß ß e _ = _ 1 ;  (ö and ß are two bytes each)
    let text = "let gr\u{f6}\u{df}e = 1;";
    assert_eq!(clamp_to_char(text, 7), 6);
    assert_eq!(clamp_to_char(text, 99), 16);
    assert_eq!(next_char(text, 6), 8);
    assert_eq!(prev_char(text, 8), 6);
    assert_eq!(prev_char(text, 0), 0);
    assert_eq!(next_char(text, 16), 16);
    assert_eq!(word_right(text, 0), 3);
    assert_eq!(word_right(text, 3), 11, "over the blank, then the whole word");
    assert_eq!(word_right(text, 11), 13, "punctuation is a run of its own");
    assert_eq!(word_left(text, 11), 4);
    assert_eq!(word_left(text, 4), 0);
    assert_eq!(word_at(text, 6), (4, 11));
    assert_eq!(word_at(text, 11), (4, 11), "the word just left of the caret");
    assert_eq!(word_at("a  b", 2), (2, 2), "between blanks there is no word");
    assert_eq!(first_non_blank("    let x"), 4);
    assert_eq!(first_non_blank("   "), 3);
}

// ---- the window ----

#[test]
fn the_window_builds_only_the_lines_in_view_of_a_million_line_text() {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let mut cv = generated(1_000_000, &asked);
    cv.view.top_line = 500_000;
    let geo = geometry(&cv);
    assert_eq!(geo.top, 500_000);
    assert_eq!(geo.rows, 22, "400 px of 19 px lines: 21 whole lines and a part");
    assert_eq!(geo.fit_lines, 21);
    let resolved = resolve(cv);
    let shown: Vec<u32> = resolved.lines.iter().map(|l| l.index).collect();
    assert_eq!(shown, (500_000..500_022).collect::<Vec<u32>>());
    assert_eq!(
        *asked.lock().expect("log"),
        shown,
        "only the lines in view are asked for, each once"
    );
    assert_eq!(resolved.lines[0].line.text.as_str(), "line 500000");

    // At the end of the text the window is shorter.
    let asked = Arc::new(Mutex::new(Vec::new()));
    let mut cv = generated(1_000_000, &asked);
    cv.view.top_line = 999_990;
    let geo = geometry(&cv);
    assert_eq!((geo.top, geo.rows), (999_990, 10));
}

#[test]
fn the_top_line_stays_in_range_when_the_text_shrinks() {
    let mut cv = over("a\nb\nc");
    cv.view.top_line = 40;
    let geo = geometry(&cv);
    assert_eq!((geo.top, geo.rows), (2, 1), "at most the last line is the top one");
    let mut v = cv.view.clone();
    clamp_view(&mut v, 3);
    assert_eq!(v.top_line, 2);
    v.set_cursor(p(9, 9));
    clamp_view(&mut v, 3);
    assert_eq!(v.primary().head.line, 2, "a caret past the end moves onto the last line");
}

#[test]
fn the_gutter_is_as_wide_as_the_longest_line_number() {
    let mut cv = over("a\nb");
    cv.view.char_width = 8.0;
    let geo = geometry(&cv);
    assert_eq!(geo.gutter_width, 3.0 * 8.0 + 2.0 * GUTTER_PAD, "three digits at least");
    assert_eq!(geo.text_left, geo.gutter_width + TEXT_PAD);
    let asked = Arc::new(Mutex::new(Vec::new()));
    let mut big = generated(1_000_000, &asked);
    big.view.char_width = 8.0;
    assert_eq!(geometry(&big).gutter_width, 7.0 * 8.0 + 2.0 * GUTTER_PAD);
    let bare = over("a").with_show_line_numbers(false);
    assert_eq!(geometry(&bare).gutter_width, 0.0);
}

// ---- a line's pieces ----

#[test]
fn a_line_splits_at_its_colours_its_selection_and_its_caret() {
    let spans = [
        CodeViewSpan::create(0, 2, CodeTokenKind::Keyword),
        CodeViewSpan::create(3, 7, CodeTokenKind::Function),
    ];
    let got = line_pieces("fn main() {", &spans, &[(3, 7)], &[7], 4, 0, 80);
    assert_eq!(
        got.pieces,
        vec![
            run("fn", CodeTokenKind::Keyword, false),
            run(" ", CodeTokenKind::Plain, false),
            run("main", CodeTokenKind::Function, true),
            Piece::Caret,
            run("() {", CodeTokenKind::Plain, false),
        ]
    );
    assert!(!got.eol_selected);
    // A caret at the end of the line comes after the text.
    let got = line_pieces("ab", &[], &[], &[2], 4, 0, 80);
    assert_eq!(got.pieces, vec![run("ab", CodeTokenKind::Plain, false), Piece::Caret]);
    // An empty line with a caret is the caret alone.
    assert_eq!(line_pieces("", &[], &[], &[0], 4, 0, 80).pieces, vec![Piece::Caret]);
}

#[test]
fn a_line_scrolled_right_shows_only_its_visible_columns() {
    let spans = [CodeViewSpan::create(1, 4, CodeTokenKind::Keyword)];
    // "\tlet x = 1;" reads "    let x = 1;": columns 2..8 are "  let ".
    let got = line_pieces("\tlet x = 1;", &spans, &[], &[], 4, 2, 6);
    assert_eq!(
        got.pieces,
        vec![
            run("  ", CodeTokenKind::Plain, false),
            run("let", CodeTokenKind::Keyword, false),
            run(" ", CodeTokenKind::Plain, false),
        ]
    );
    // A caret left of the window is not built.
    assert!(!line_pieces("abcdef", &[], &[], &[1], 4, 3, 3).pieces.contains(&Piece::Caret));
}

#[test]
fn a_selection_across_the_line_break_marks_the_end_of_the_line() {
    let mut v = CodeViewView::create();
    v.select(p(0, 3), p(2, 1));
    let (sel0, car0) = line_marks(&v, 0);
    assert_eq!(sel0, vec![(3, u32::MAX)]);
    assert!(car0.is_empty());
    assert_eq!(line_marks(&v, 1).0, vec![(0, u32::MAX)]);
    assert_eq!(line_marks(&v, 2), (vec![(0, 1)], vec![1]));
    assert_eq!(line_marks(&v, 3), (Vec::new(), Vec::new()));

    let got = line_pieces("abcdef", &[], &[(3, u32::MAX)], &[], 4, 0, 80);
    assert!(got.eol_selected);
    assert_eq!(
        got.pieces,
        vec![
            run("abc", CodeTokenKind::Plain, false),
            run("def", CodeTokenKind::Plain, true),
        ]
    );
    let blank = line_pieces("", &[], &[(0, u32::MAX)], &[], 4, 0, 80);
    assert!(blank.eol_selected, "an empty line inside a selection shows its break selected");
    assert!(blank.pieces.is_empty());
}

// ---- typing and deleting ----

#[test]
fn typing_replaces_every_selection_and_moves_every_caret() {
    let text = "let a = 1;\nlet b = 2;";
    let mut cv = over(text);
    cursors(
        &mut cv,
        vec![
            CodeViewCursor::create_selection(p(0, 4), p(0, 5)),
            CodeViewCursor::create_selection(p(1, 4), p(1, 5)),
        ],
    );
    let e = typed_event(&cv, &VecLines::of(text), "value").expect("an edit");
    assert_eq!(e.kind, CodeViewEventKind::Edit);
    assert_eq!(apply(text, e.edits.as_slice()), "let value = 1;\nlet value = 2;");
    assert_eq!(heads(&e.view), vec![p(0, 9), p(1, 9)]);
    assert!(e.view.cursors.as_slice().iter().all(CodeViewCursor::is_empty));
}

#[test]
fn edits_come_last_in_the_text_first() {
    let text = "ab";
    let mut cv = over(text);
    cursors(
        &mut cv,
        vec![
            CodeViewCursor::create(p(0, 0)),
            CodeViewCursor::create(p(0, 1)),
            CodeViewCursor::create(p(0, 2)),
        ],
    );
    let e = typed_event(&cv, &VecLines::of(text), "x").expect("an edit");
    let starts: Vec<CodeViewPosition> = e.edits.as_slice().iter().map(|x| x.start).collect();
    assert_eq!(starts, vec![p(0, 2), p(0, 1), p(0, 0)]);
    assert_eq!(apply(text, e.edits.as_slice()), "xaxbx");
    assert_eq!(heads(&e.view), vec![p(0, 1), p(0, 3), p(0, 5)]);
}

#[test]
fn enter_keeps_the_indentation_of_the_line() {
    let text = "fn main() {\n    let x = 1;\n}";
    let mut cv = over(text);
    at(&mut cv, 1, 14);
    let e = key(&cv, text, K::Return, NONE);
    assert_eq!(apply(text, e.edits.as_slice()), "fn main() {\n    let x = 1;\n    \n}");
    assert_eq!(heads(&e.view), vec![p(2, 4)]);
    // After an opening bracket the new line goes one level deeper.
    at(&mut cv, 0, 11);
    let e = key(&cv, text, K::Return, NONE);
    assert_eq!(apply(text, e.edits.as_slice()), "fn main() {\n    \n    let x = 1;\n}");
    assert_eq!(heads(&e.view), vec![p(1, 4)]);
}

#[test]
fn backspace_at_a_line_start_joins_it_to_the_line_above() {
    let text = "abc\ndef";
    let mut cv = over(text);
    at(&mut cv, 1, 0);
    let e = key(&cv, text, K::Back, NONE);
    assert_eq!(
        e.edits.as_slice(),
        &[CodeViewEdit::create(p(0, 3), p(1, 0), AzString::from(""))]
    );
    assert_eq!(apply(text, e.edits.as_slice()), "abcdef");
    assert_eq!(heads(&e.view), vec![p(0, 3)]);
    at(&mut cv, 0, 0);
    assert!(
        key_event(&cv, &VecLines::of(text), K::Back, NONE).is_none(),
        "nothing to delete at the start of the text"
    );

    let text = "let gr\u{f6}\u{df}e";
    let mut cv = over(text);
    at(&mut cv, 0, 10);
    let e = key(&cv, text, K::Back, NONE);
    assert_eq!(apply(text, e.edits.as_slice()), "let gr\u{f6}e", "one character, two bytes");
    at(&mut cv, 0, 11);
    let e = key(&cv, text, K::Back, WORD);
    assert_eq!(apply(text, e.edits.as_slice()), "let ", "the word modifier takes the word");
}

#[test]
fn delete_at_a_line_end_joins_the_next_line() {
    let text = "abc\ndef";
    let mut cv = over(text);
    at(&mut cv, 0, 3);
    let e = key(&cv, text, K::Delete, NONE);
    assert_eq!(apply(text, e.edits.as_slice()), "abcdef");
    assert_eq!(heads(&e.view), vec![p(0, 3)]);
    at(&mut cv, 1, 3);
    assert!(key_event(&cv, &VecLines::of(text), K::Delete, NONE).is_none());
}

#[test]
fn tab_indents_every_selected_line_and_shift_tab_outdents_them() {
    let text = "a\nb\nc";
    let mut cv = over(text);
    cv.view.select(p(0, 0), p(1, 1));
    let e = key(&cv, text, K::Tab, NONE);
    let indented = apply(text, e.edits.as_slice());
    assert_eq!(indented, "    a\n    b\nc");
    assert_eq!(
        e.view.primary(),
        CodeViewCursor::create_selection(p(0, 4), p(1, 5)),
        "the lines stay selected"
    );
    let mut cv = over(&indented);
    cv.view = e.view;
    let e = key(&cv, &indented, K::Tab, SHIFT);
    assert_eq!(apply(&indented, e.edits.as_slice()), "a\nb\nc");
    assert_eq!(e.view.primary(), CodeViewCursor::create_selection(p(0, 0), p(1, 1)));

    // A caret alone: spaces to the next tab stop.
    let mut cv = over("ab");
    at(&mut cv, 0, 2);
    let e = key(&cv, "ab", K::Tab, NONE);
    assert_eq!(apply("ab", e.edits.as_slice()), "ab  ");
    assert_eq!(heads(&e.view), vec![p(0, 4)]);
}

// ---- moving ----

#[test]
fn up_and_down_keep_the_visual_column_through_shorter_lines() {
    let text = "abcdefgh\nab\nabcdefgh";
    let mut cv = over(text);
    at(&mut cv, 0, 6);
    let e = key(&cv, text, K::Down, NONE);
    assert_eq!(e.kind, CodeViewEventKind::Move);
    assert_eq!(heads(&e.view), vec![p(1, 2)]);
    assert_eq!(e.view.primary().goal, 6);
    cv.view = e.view;
    let e = key(&cv, text, K::Down, NONE);
    assert_eq!(heads(&e.view), vec![p(2, 6)], "the column comes back on a long line");

    at(&mut cv, 0, 3);
    assert_eq!(heads(&key(&cv, text, K::Up, NONE).view), vec![p(0, 0)]);
    at(&mut cv, 2, 3);
    assert_eq!(heads(&key(&cv, text, K::Down, NONE).view), vec![p(2, 8)]);

    let text = "\tx\nabcdefgh";
    let mut cv = over(text);
    at(&mut cv, 0, 1);
    assert_eq!(
        heads(&key(&cv, text, K::Down, NONE).view),
        vec![p(1, 4)],
        "a tab counts as the columns it fills"
    );
}

#[test]
fn left_and_right_step_characters_and_shift_extends() {
    let text = "a\u{df}c\nd";
    let mut cv = over(text);
    at(&mut cv, 0, 1);
    assert_eq!(heads(&key(&cv, text, K::Right, NONE).view), vec![p(0, 3)]);
    at(&mut cv, 0, 4);
    assert_eq!(heads(&key(&cv, text, K::Right, NONE).view), vec![p(1, 0)], "on to the next line");
    at(&mut cv, 1, 0);
    assert_eq!(heads(&key(&cv, text, K::Left, NONE).view), vec![p(0, 4)]);
    at(&mut cv, 0, 1);
    let e = key(&cv, text, K::Right, SHIFT);
    assert_eq!(e.view.primary(), CodeViewCursor::create_selection(p(0, 1), p(0, 3)));
    // Left on a selection without Shift collapses it to its start.
    cv.view = e.view;
    assert_eq!(heads(&key(&cv, text, K::Left, NONE).view), vec![p(0, 1)]);
}

#[test]
fn home_goes_to_the_first_non_blank_then_to_the_line_start() {
    let text = "    let x;";
    let mut cv = over(text);
    at(&mut cv, 0, 8);
    let e = key(&cv, text, K::Home, NONE);
    assert_eq!(heads(&e.view), vec![p(0, 4)]);
    cv.view = e.view;
    let e = key(&cv, text, K::Home, NONE);
    assert_eq!(heads(&e.view), vec![p(0, 0)]);
    cv.view = e.view;
    assert_eq!(heads(&key(&cv, text, K::Home, NONE).view), vec![p(0, 4)]);
    assert_eq!(heads(&key(&cv, text, K::End, NONE).view), vec![p(0, 10)]);
    at(&mut cv, 0, 8);
    let e = key(&cv, text, K::Home, SHIFT);
    assert_eq!(e.view.primary(), CodeViewCursor::create_selection(p(0, 8), p(0, 4)));
}

#[test]
fn moving_the_caret_below_the_view_scrolls_it_into_sight() {
    let text = hundred_lines();
    let mut cv = over(&text);
    cv.view.visible_lines = 10;
    at(&mut cv, 9, 0);
    let e = key(&cv, &text, K::Down, NONE);
    assert_eq!(heads(&e.view), vec![p(10, 0)]);
    assert_eq!(e.view.top_line, 1);
    let e = key(&cv, &text, K::End, PRIMARY);
    assert_eq!(heads(&e.view), vec![p(99, 7)]);
    assert_eq!(e.view.top_line, 90, "the last line at the bottom of the view");
    cv.view = e.view;
    let e = key(&cv, &text, K::Home, PRIMARY);
    assert_eq!(heads(&e.view), vec![p(0, 0)]);
    assert_eq!(e.view.top_line, 0);
}

#[test]
fn page_down_moves_the_caret_and_the_view_by_a_screen() {
    let text = hundred_lines();
    let mut cv = over(&text);
    cv.view.visible_lines = 10;
    at(&mut cv, 2, 0);
    let e = key(&cv, &text, K::PageDown, NONE);
    assert_eq!(heads(&e.view), vec![p(11, 0)]);
    assert_eq!(e.view.top_line, 9);
    cv.view = e.view;
    let e = key(&cv, &text, K::PageUp, NONE);
    assert_eq!(heads(&e.view), vec![p(2, 0)]);
    assert_eq!(e.view.top_line, 0);
}

// ---- the clipboard, undo, select ----

#[test]
fn copy_without_a_selection_copies_the_whole_line() {
    let text = "one\ntwo\nthree";
    let lines = VecLines::of(text);
    let mut cv = over(text);
    at(&mut cv, 1, 1);
    assert_eq!(copy_text(&cv.view, &lines), "two\n");
    cv.view.select(p(0, 1), p(2, 2));
    assert_eq!(copy_text(&cv.view, &lines), "ne\ntwo\nth");
    cursors(
        &mut cv,
        vec![
            CodeViewCursor::create_selection(p(0, 0), p(0, 3)),
            CodeViewCursor::create_selection(p(1, 0), p(1, 3)),
        ],
    );
    assert_eq!(copy_text(&cv.view, &lines), "one\ntwo");

    // Cut without a selection takes the line and its break.
    at(&mut cv, 1, 1);
    let e = key(&cv, text, K::X, PRIMARY);
    assert_eq!(e.kind, CodeViewEventKind::Edit);
    assert_eq!(e.text.as_str(), "two\n");
    assert_eq!(apply(text, e.edits.as_slice()), "one\nthree");
    assert_eq!(heads(&e.view), vec![p(1, 0)]);
    let e = key(&cv, text, K::C, PRIMARY);
    assert_eq!((e.kind, e.text.as_str()), (CodeViewEventKind::Copy, "two\n"));
    assert!(e.edits.is_empty());
}

#[test]
fn paste_with_as_many_lines_as_cursors_gives_each_cursor_one_line() {
    let text = "a\nb";
    let lines = VecLines::of(text);
    let mut cv = over(text);
    cursors(
        &mut cv,
        vec![CodeViewCursor::create(p(0, 1)), CodeViewCursor::create(p(1, 1))],
    );
    let e = paste_event(&cv, &lines, "X\nY").expect("an edit");
    assert_eq!(apply(text, e.edits.as_slice()), "aX\nbY");
    assert_eq!(heads(&e.view), vec![p(0, 2), p(1, 2)]);
    at(&mut cv, 0, 1);
    let e = paste_event(&cv, &lines, "X\nY").expect("an edit");
    assert_eq!(apply(text, e.edits.as_slice()), "aX\nY\nb");
    assert_eq!(heads(&e.view), vec![p(1, 1)]);
    let e = paste_event(&cv, &lines, "X\r\nY").expect("an edit");
    assert_eq!(apply(text, e.edits.as_slice()), "aX\nY\nb", "CRLF arrives as LF");
}

#[test]
fn undo_and_redo_are_the_apps_history() {
    let text = "abc";
    let cv = over(text);
    let e = key(&cv, text, K::Z, PRIMARY);
    assert_eq!(e.kind, CodeViewEventKind::Undo);
    assert!(e.edits.is_empty());
    assert_eq!(key(&cv, text, K::Z, Mods { shift: true, ..PRIMARY }).kind, CodeViewEventKind::Redo);
    assert_eq!(key(&cv, text, K::Y, PRIMARY).kind, CodeViewEventKind::Redo);
}

#[test]
fn select_all_selects_the_whole_text() {
    let text = "one\ntwo";
    let cv = over(text);
    let e = key(&cv, text, K::A, PRIMARY);
    assert_eq!(e.view.cursors.as_slice(), &[CodeViewCursor::create_selection(p(0, 0), p(1, 3))]);
}

#[test]
fn the_word_shortcut_selects_the_word_then_adds_its_next_occurrence() {
    let text = "let a = b;\nlet c = a + a;";
    let mut cv = over(text);
    at(&mut cv, 0, 4);
    let e = key(&cv, text, K::D, PRIMARY);
    assert_eq!(e.view.cursors.as_slice(), &[CodeViewCursor::create_selection(p(0, 4), p(0, 5))]);
    cv.view = e.view;
    let e = key(&cv, text, K::D, PRIMARY);
    assert_eq!(
        e.view.cursors.as_slice(),
        &[
            CodeViewCursor::create_selection(p(0, 4), p(0, 5)),
            CodeViewCursor::create_selection(p(1, 8), p(1, 9)),
        ]
    );
    cv.view = e.view;
    let e = key(&cv, text, K::D, PRIMARY);
    assert_eq!(e.view.cursor_count(), 3);
    assert_eq!(e.view.primary(), CodeViewCursor::create_selection(p(1, 12), p(1, 13)));
    cv.view = e.view;
    assert!(
        key_event(&cv, &VecLines::of(text), K::D, PRIMARY).is_none(),
        "every occurrence has a cursor"
    );
}

#[test]
fn escape_drops_the_extra_cursors() {
    let text = "ab\ncd";
    let mut cv = over(text);
    cursors(
        &mut cv,
        vec![CodeViewCursor::create(p(0, 1)), CodeViewCursor::create(p(1, 1))],
    );
    let e = key(&cv, text, K::Escape, NONE);
    assert_eq!(e.view.cursors.as_slice(), &[CodeViewCursor::create(p(1, 1))]);
    cv.view.select(p(0, 0), p(0, 2));
    assert_eq!(heads(&key(&cv, text, K::Escape, NONE).view), vec![p(0, 2)]);
    at(&mut cv, 0, 1);
    assert!(
        key_event(&cv, &VecLines::of(text), K::Escape, NONE).is_none(),
        "a lone caret leaves Escape to the app"
    );
}

#[test]
fn a_read_only_view_moves_and_copies_but_never_edits() {
    let text = "abc";
    let lines = VecLines::of(text);
    let mut cv = over(text).with_read_only(true);
    at(&mut cv, 0, 1);
    assert!(typed_event(&cv, &lines, "x").is_none());
    assert!(paste_event(&cv, &lines, "x").is_none());
    assert!(key_event(&cv, &lines, K::Back, NONE).is_none());
    assert!(key_event(&cv, &lines, K::Return, NONE).is_none());
    assert!(key_event(&cv, &lines, K::Z, PRIMARY).is_none());
    assert_eq!(key(&cv, text, K::Right, NONE).kind, CodeViewEventKind::Move);
    let cut = key(&cv, text, K::X, PRIMARY);
    assert_eq!(cut.kind, CodeViewEventKind::Copy, "a cut in a read-only view only copies");
    assert!(cut.edits.is_empty());
}

// ---- the pointer and the wheel ----

#[test]
fn a_press_places_the_caret_under_the_pointer_and_a_press_on_a_line_number_selects_the_line() {
    let text = "fn main() {\n\tlet x = 1;\n}";
    let lines = VecLines::of(text);
    let mut cv = over(text);
    cv.view.char_width = 8.0;
    let geo = geometry(&cv);
    assert_eq!(geo.text_left, 50.0);
    let hit = hit_test(&cv, &geo, &lines, 50.0 + 6.0 * 8.0 + 1.0, 19.0 * 1.5);
    assert_eq!(hit, Hit::Text(p(1, 3)), "visual column 6 is past the tab, on the t");
    let e = press_event(&cv, &geo, &lines, hit, NONE, 0.0).expect("a press");
    assert_eq!(e.view.cursors.as_slice(), &[CodeViewCursor::create(p(1, 3))]);
    assert_eq!(e.view.drag, CodeViewDragKind::Select);
    assert_eq!(
        hit_test(&cv, &geo, &lines, 50.0 + 40.0 * 8.0, 19.0 * 0.5),
        Hit::Text(p(0, 11)),
        "past the line's end: its end"
    );
    assert_eq!(
        hit_test(&cv, &geo, &lines, 60.0, 19.0 * 7.0),
        Hit::Text(p(2, 1)),
        "below the last line: the last line"
    );

    let hit = hit_test(&cv, &geo, &lines, 10.0, 19.0 * 1.2);
    assert_eq!(hit, Hit::Gutter(1));
    let e = press_event(&cv, &geo, &lines, hit, NONE, 0.0).expect("a press");
    assert_eq!(e.view.cursors.as_slice(), &[CodeViewCursor::create_selection(p(1, 0), p(2, 0))]);

    at(&mut cv, 0, 2);
    let e = press_event(&cv, &geo, &lines, Hit::Text(p(1, 3)), SHIFT, 0.0).expect("a press");
    assert_eq!(e.view.primary(), CodeViewCursor::create_selection(p(0, 2), p(1, 3)));
    let alt = Mods { alt: true, ..NONE };
    let e = press_event(&cv, &geo, &lines, Hit::Text(p(1, 3)), alt, 0.0).expect("a press");
    assert_eq!(heads(&e.view), vec![p(0, 2), p(1, 3)], "Alt adds a cursor");
}

#[test]
fn a_drag_extends_the_selection_from_the_press() {
    let text = "abcdef\nghijkl";
    let lines = VecLines::of(text);
    let mut cv = over(text);
    cv.view.char_width = 8.0;
    let geo = geometry(&cv);
    let e = press_event(&cv, &geo, &lines, Hit::Text(p(0, 1)), NONE, 0.0).expect("a press");
    cv.view = e.view;
    let e = drag_event(&cv, &geo, &lines, geo.text_left + 3.0 * 8.0, 19.0 * 1.5).expect("a drag");
    assert_eq!(e.view.primary(), CodeViewCursor::create_selection(p(0, 1), p(1, 3)));
    assert_eq!(e.view.drag, CodeViewDragKind::Select);
}

#[test]
fn a_double_click_selects_the_word() {
    let text = "let answer = 42;";
    let cv = over(text);
    let e = double_click_event(&cv, &VecLines::of(text), Hit::Text(p(0, 6))).expect("a word");
    assert_eq!(e.view.primary(), CodeViewCursor::create_selection(p(0, 4), p(0, 10)));
}

#[test]
fn the_wheel_scrolls_whole_lines_and_never_past_the_last_line() {
    let text = hundred_lines();
    let mut cv = over(&text);
    let e = scroll_event(&cv, 3, 0).expect("a scroll");
    assert_eq!((e.kind, e.view.top_line), (CodeViewEventKind::Scroll, 3));
    cv.view.top_line = 98;
    assert_eq!(scroll_event(&cv, 5, 0).map(|e| e.view.top_line), Some(99));
    cv.view.top_line = 99;
    assert!(scroll_event(&cv, 1, 0).is_none(), "already at the last line");
    cv.view.top_line = 5;
    assert_eq!(scroll_event(&cv, -200, 0).map(|e| e.view.top_line), Some(0));
    assert_eq!(scroll_event(&cv, 0, 4).map(|e| e.view.left_column), Some(4));
    assert!(scroll_event(&cv, 0, -1).is_none());
}

#[test]
fn the_scroll_bar_thumb_drags_through_every_line() {
    let text = hundred_lines();
    let lines = VecLines::of(&text);
    let mut cv = over(&text);
    cv.view.char_width = 8.0;
    let geo = geometry(&cv);
    let bar = geo.vbar.expect("100 lines in a 10-line view have a scroll bar");
    let (x, _, w, h) = bar.track;
    let thumb_y = bar.thumb_start + 1.0;
    assert_eq!(hit_test(&cv, &geo, &lines, x + w / 2.0, thumb_y), Hit::Thumb);
    assert_eq!(hit_test(&cv, &geo, &lines, x + w / 2.0, h - 1.0), Hit::TrackBelow);
    let e = press_event(&cv, &geo, &lines, Hit::Thumb, NONE, thumb_y).expect("a press");
    assert_eq!(e.view.drag, CodeViewDragKind::ScrollBar);
    cv.view = e.view;
    let e = drag_event(&cv, &geo, &lines, x, thumb_y + h).expect("a drag");
    assert_eq!(e.view.top_line, 99, "dragged to the end of the track: the last line");
    let e = press_event(&cv, &geo, &lines, Hit::TrackBelow, NONE, h - 1.0).expect("a page");
    assert_eq!(e.view.top_line, geo.fit_lines.saturating_sub(1));
}

// ---- the build and the looks ----

#[test]
fn the_view_builds_its_lines_with_the_kind_classes_and_the_caret() {
    let dom = sample().with_theme(UiTheme::Flat).dom();
    let lines = tc::find_all(&dom, LINE_CLASS_NAME);
    assert_eq!(lines.len(), 4, "the sample's four lines");
    let keyword = tc::find(&dom, CodeTokenKind::Keyword.class_name()).expect("the fn keyword");
    assert_eq!(text_of(keyword), "fn");
    assert_eq!(text_of(lines[1]), "2    let answer = 42;", "the number, then the tab as spaces");
    assert_eq!(tc::find_all(&dom, CARET_CLASS_NAME).len(), 1, "one caret");
    assert!(tc::find(lines[1], CURRENT_LINE_CLASS_NAME).is_some() || tc::has_class(lines[1], CURRENT_LINE_CLASS_NAME));
    assert_eq!(dom.root.get_tab_index(), Some(azul_core::dom::TabIndex::Auto), "one focus stop");
}

#[test]
fn the_view_follows_the_app_theme() {
    checks::assert_follows_the_app_theme(
        "code_view",
        || sample().dom(),
        |t: UiTheme| sample().with_theme(t).dom(),
    );
}
