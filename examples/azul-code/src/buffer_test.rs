use super::*;

fn lines(b: &TextBuffer) -> Vec<String> {
    (0..b.line_count()).map(|i| b.line(i)).collect()
}

/// The model the buffer must agree with: a plain string.
fn replace_in(model: &mut String, e: &Edit) {
    let offset = |s: &str, at: Pos| -> usize {
        s.split('\n').take(at.line).map(|l| l.len() + 1).sum::<usize>() + at.column
    };
    let a = offset(model, e.start);
    let b = offset(model, e.end);
    model.replace_range(a..b, &e.text);
}

/// A small deterministic generator (no rand crate).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % below.max(1)
    }
}

/// A random position on a char boundary of `model`.
fn random_pos(model: &str, rng: &mut Lcg) -> Pos {
    let all: Vec<&str> = model.split('\n').collect();
    let line = rng.next(all.len());
    let text = all[line];
    let mut column = rng.next(text.len() + 1);
    while !text.is_char_boundary(column) {
        column -= 1;
    }
    Pos::new(line, column)
}

#[test]
fn a_file_reads_back_line_by_line_and_keeps_its_line_endings() {
    let b = TextBuffer::from_text("fn main() {\r\n    go();\r\n}\r\n");
    assert_eq!(b.line_ending(), LineEnding::CrLf);
    assert_eq!(lines(&b), vec!["fn main() {", "    go();", "}", ""]);
    assert_eq!(b.text(), "fn main() {\n    go();\n}\n");
    assert_eq!(b.to_file_text(), "fn main() {\r\n    go();\r\n}\r\n");
    let empty = TextBuffer::from_text("");
    assert_eq!((empty.line_count(), empty.line(0)), (1, String::new()));
    let bom = TextBuffer::from_text("\u{feff}a\nb");
    assert_eq!(lines(&bom), vec!["a", "b"]);
    assert_eq!(bom.to_file_text(), "\u{feff}a\nb", "the byte-order mark is written back");
}

#[test]
fn an_insert_in_the_middle_of_a_line_splits_it() {
    let mut b = TextBuffer::from_text("abc\ndef");
    let changes = b.apply(&[Edit::new(Pos::new(0, 1), Pos::new(0, 1), "XY\nZ")]);
    assert_eq!(lines(&b), vec!["aXY", "Zbc", "def"]);
    assert_eq!(changes, vec![LineChange { first: 0, removed: 0, added: 1 }]);
    assert_eq!(b.offset_of(Pos::new(1, 1)), 5);
    assert_eq!(b.pos_of(5), Pos::new(1, 1));
}

#[test]
fn a_delete_across_lines_joins_them() {
    let mut b = TextBuffer::from_text("abc\ndef\nghi");
    let changes = b.apply(&[Edit::new(Pos::new(0, 1), Pos::new(2, 1), "")]);
    assert_eq!(lines(&b), vec!["ahi"]);
    assert_eq!(changes, vec![LineChange { first: 0, removed: 2, added: 0 }]);
}

#[test]
fn typing_at_the_end_of_an_insertion_extends_its_piece() {
    let mut b = TextBuffer::from_text("let x = ;");
    for (i, ch) in "4242".chars().enumerate() {
        let at = Pos::new(0, 8 + i);
        b.apply(&[Edit::new(at, at, &ch.to_string())]);
    }
    assert_eq!(b.line(0), "let x = 4242;");
    assert_eq!(b.piece_count(), 3, "before, the typed run, after - not one piece per key");
}

#[test]
fn many_edits_agree_with_a_plain_string() {
    let mut model: String = (0..200).map(|i| format!("line {i} \u{e4}\u{f6}\n")).collect();
    let mut b = TextBuffer::from_text(&model);
    let mut rng = Lcg(7);
    for round in 0..500 {
        let p = random_pos(&model, &mut rng);
        let q = random_pos(&model, &mut rng);
        let (start, end) = if p <= q { (p, q) } else { (q, p) };
        let text = match rng.next(4) {
            0 => String::new(),
            1 => "x".to_string(),
            2 => "new\nline".to_string(),
            _ => format!("{round}\n\n"),
        };
        let e = Edit { start, end, text };
        replace_in(&mut model, &e);
        b.apply(&[e]);
        assert_eq!(b.text(), model, "after edit {round}");
    }
    assert_eq!(b.line_count(), model.split('\n').count());
}

#[test]
fn a_line_of_a_million_line_file_is_found_after_edits() {
    let text: String = (0..1_000_000).map(|i| format!("line {i}\n")).collect();
    let mut b = TextBuffer::from_text(&text);
    assert_eq!(b.line_count(), 1_000_001);
    assert_eq!(b.line(999_999), "line 999999");
    assert_eq!(b.line(500_000), "line 500000");
    for k in 0..200 {
        let line = (k * 4_999) % 1_000_000;
        b.apply(&[Edit::new(Pos::new(line, 0), Pos::new(line, 0), "// ")]);
    }
    assert_eq!(b.line(4_999), "// line 4999");
    assert_eq!(b.line(5_000), "line 5000");
    assert_eq!(b.line(999_999), "line 999999");
    assert_eq!(b.line_count(), 1_000_001);
}

#[test]
fn an_events_edits_are_one_undo_step() {
    let mut b = TextBuffer::from_text("a = 1\nb = 2");
    // Two cursors, last in the text first.
    b.apply(&[
        Edit::new(Pos::new(1, 0), Pos::new(1, 1), "y"),
        Edit::new(Pos::new(0, 0), Pos::new(0, 1), "x"),
    ]);
    assert_eq!(b.text(), "x = 1\ny = 2");
    assert!(b.is_dirty());
    let undone = b.undo().expect("a step");
    assert_eq!(b.text(), "a = 1\nb = 2");
    assert_eq!(undone.caret, Pos::new(0, 1));
    assert!(!b.is_dirty(), "back at the saved text");
    b.redo().expect("the step again");
    assert_eq!(b.text(), "x = 1\ny = 2");
    assert!(b.is_dirty());
    assert!(b.redo().is_none());
}

#[test]
fn consecutive_typing_is_one_undo_step_and_saving_marks_it_clean() {
    let mut b = TextBuffer::from_text("");
    for (i, ch) in "hello".chars().enumerate() {
        let at = Pos::new(0, i);
        b.apply(&[Edit::new(at, at, &ch.to_string())]);
    }
    b.apply(&[Edit::new(Pos::new(0, 5), Pos::new(0, 5), "\n")]);
    b.mark_saved();
    assert!(!b.is_dirty());
    b.undo();
    assert_eq!(b.text(), "hello");
    assert!(b.is_dirty());
    b.undo();
    assert_eq!(b.text(), "", "the five keys are one step");
    assert!(b.undo().is_none());
    // A new edit after an undo drops the redo branch; the saved text
    // can no longer be reached by undo / redo.
    b.apply(&[Edit::new(Pos::new(0, 0), Pos::new(0, 0), "x")]);
    assert!(b.redo().is_none());
    assert!(b.is_dirty());
}
