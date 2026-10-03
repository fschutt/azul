//! The text of an open file: a PIECE TABLE.
//!
//! The file as it was read is kept once (`original`, never copied or
//! moved); everything typed or pasted is appended to `added`. The text is
//! the list of pieces - runs of either buffer - in order. Both buffers know
//! where their line breaks are, and every piece knows how many it holds,
//! so the start of line N is found by a binary search over the pieces and
//! one over the buffer's breaks: a million-line file costs its bytes plus
//! a `usize` per line, and an edit costs O(pieces), not O(text).
//!
//! Edits are CodeView's: "replace `start..end` with `text`" in (line, byte)
//! positions, several at once (one per cursor, last in the text first).
//! One call is ONE undo step; consecutive typing (single characters, each
//! after the last) merges into one step. Line endings are read as they
//! are (LF or CRLF, the first break decides), kept as LF inside, written
//! back as found; a UTF-8 byte-order mark is kept too.

/// How the file breaks its lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

/// A place in the text: a line and a byte offset into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pos {
    pub line: usize,
    pub column: usize,
}

impl Pos {
    #[must_use]
    pub const fn new(line: usize, column: usize) -> Pos {
        Pos { line, column }
    }
}

/// One replacement: `start..end` (positions before the edit) becomes `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub start: Pos,
    pub end: Pos,
    pub text: String,
}

impl Edit {
    #[must_use]
    pub fn new(start: Pos, end: Pos, text: &str) -> Edit {
        Edit {
            start,
            end,
            text: text.to_string(),
        }
    }
}

/// What an edit did to the lines (what a highlighter invalidates): line
/// `first` and the `removed` lines after it became `first` and `added`
/// lines after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineChange {
    pub first: usize,
    pub removed: usize,
    pub added: usize,
}

/// What an undo or a redo did: where the caret goes, and the lines it
/// changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undone {
    pub caret: Pos,
    pub changes: Vec<LineChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Original,
    Added,
}

/// A run of one buffer: `len` bytes from `start`, holding `breaks` line
/// breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Piece {
    source: Source,
    start: usize,
    len: usize,
    breaks: usize,
}

/// One replacement as the history keeps it: at byte `offset`, `removed`
/// was replaced by `inserted`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    offset: usize,
    removed: String,
    inserted: String,
}

/// The text. See the module documentation.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    original: String,
    added: String,
    /// Byte offsets of the `\n`s of `original` / `added`.
    original_breaks: Vec<usize>,
    added_breaks: Vec<usize>,
    pieces: Vec<Piece>,
    /// Bytes and breaks before piece `i` (rebuilt after every edit).
    bytes_before: Vec<usize>,
    breaks_before: Vec<usize>,
    len: usize,
    breaks: usize,
    ending: LineEnding,
    bom: bool,
    undo: Vec<Vec<Step>>,
    redo: Vec<Vec<Step>>,
    /// The last undo step may take more typing.
    typing: bool,
    /// `undo.len()` when the text was saved (`usize::MAX`: unreachable).
    saved_depth: usize,
}

impl TextBuffer {
    /// A buffer holding `text` as a file has it.
    #[must_use]
    pub fn from_text(text: &str) -> TextBuffer {
        todo!("GREEN: from_text {}", text.len())
    }

    /// How many lines there are (an empty text has one).
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.breaks + 1
    }

    /// The text's length in bytes (LF breaks).
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// How the file broke its lines.
    #[must_use]
    pub fn line_ending(&self) -> LineEnding {
        self.ending
    }

    /// How many pieces the text is made of.
    #[must_use]
    pub fn piece_count(&self) -> usize {
        self.pieces.len()
    }

    /// Line `line`'s text without its break ("" past the end).
    #[must_use]
    pub fn line(&self, line: usize) -> String {
        todo!("GREEN: line {line}")
    }

    /// The whole text, LF breaks.
    #[must_use]
    pub fn text(&self) -> String {
        self.slice(0, self.len)
    }

    /// The whole text as the file has it: its line endings, its BOM.
    #[must_use]
    pub fn to_file_text(&self) -> String {
        todo!("GREEN: to_file_text")
    }

    /// Bytes `from..to` of the text.
    #[must_use]
    pub fn slice(&self, from: usize, to: usize) -> String {
        todo!("GREEN: slice {from} {to}")
    }

    /// The byte offset of `at` (clamped into the text).
    #[must_use]
    pub fn offset_of(&self, at: Pos) -> usize {
        todo!("GREEN: offset_of {at:?}")
    }

    /// The position of byte `offset`.
    #[must_use]
    pub fn pos_of(&self, offset: usize) -> Pos {
        todo!("GREEN: pos_of {offset}")
    }

    /// Applies `edits` in order (CodeView's order: last in the text first)
    /// as ONE undo step; returns what each did to the lines.
    pub fn apply(&mut self, edits: &[Edit]) -> Vec<LineChange> {
        todo!("GREEN: apply {}", edits.len())
    }

    /// Takes back the last step; `None` when there is none.
    pub fn undo(&mut self) -> Option<Undone> {
        todo!("GREEN: undo")
    }

    /// Does again the last step taken back.
    pub fn redo(&mut self) -> Option<Undone> {
        todo!("GREEN: redo")
    }

    /// The text differs from what was last saved (or read).
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.undo.len() != self.saved_depth
    }

    /// The text as it is now was saved.
    pub fn mark_saved(&mut self) {
        self.saved_depth = self.undo.len();
        self.typing = false;
    }
}

#[cfg(test)]
mod tests {
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
}
