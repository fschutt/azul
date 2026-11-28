//! The key row above a phone's soft keyboard: Tab, the braces and brackets, the quotes, the
//! arrows - what code needs and a keyboard for words has no key for. Plain Rust over the text:
//! what a key does to the lines of the file in front, as the edits the buffer takes
//! ([`crate::buffer::Edit`], positions before them, the last in the text first) and where the
//! caret and the selection go afterwards. The window's side of it is `phone.rs` (the row) and
//! [`crate::app::Doc::press_key`] (the edits applied to the file and its view).
//!
//! Columns are BYTE offsets into a line, as everywhere in AzCode; the arrows and the tab stops
//! count characters, so a caret never lands inside one.

use crate::buffer::{Edit, Pos};

/// The columns between two tab stops when the file does not say.
pub const TAB_WIDTH: usize = 4;

/// What a key of the row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Tab: spaces to the next tab stop, or the selected lines one level in.
    Tab,
    /// Shift+Tab: the selected lines (or the caret's) one level out.
    BackTab,
    /// A character typed in, replacing the selection.
    Insert(&'static str),
    /// A pair typed in with the caret between (`{` and `}`); around a selection, wrapping it.
    Pair(&'static str, &'static str),
    /// One character left.
    Left,
    /// One character right.
    Right,
    /// One line up.
    Up,
    /// One line down.
    Down,
}

/// One key of the row: what it shows, what a screen reader says, what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// The label, for a key that shows text; empty for a key that shows its icon.
    pub label: &'static str,
    /// The icon (a Material name), for a key that shows no text; empty otherwise.
    pub icon: &'static str,
    /// The key's accessible name, a key of the resources (`azcode-key-tab`).
    pub name: &'static str,
    /// What it does.
    pub action: KeyAction,
}

/// The keys, left to right: the ones code needs most, then the arrows nearest the thumb.
pub static KEYS: [Key; 13] = [
    Key { label: "Tab", icon: "", name: "azcode-key-tab", action: KeyAction::Tab },
    Key { label: "", icon: "format_indent_decrease", name: "azcode-key-back-tab", action: KeyAction::BackTab },
    Key { label: "{ }", icon: "", name: "azcode-key-braces", action: KeyAction::Pair("{", "}") },
    Key { label: "( )", icon: "", name: "azcode-key-parentheses", action: KeyAction::Pair("(", ")") },
    Key { label: "[ ]", icon: "", name: "azcode-key-brackets", action: KeyAction::Pair("[", "]") },
    Key { label: "\" \"", icon: "", name: "azcode-key-quotes", action: KeyAction::Pair("\"", "\"") },
    Key { label: ";", icon: "", name: "azcode-key-semicolon", action: KeyAction::Insert(";") },
    Key { label: ":", icon: "", name: "azcode-key-colon", action: KeyAction::Insert(":") },
    Key { label: "=", icon: "", name: "azcode-key-equals", action: KeyAction::Insert("=") },
    Key { label: "", icon: "arrow_back", name: "azcode-key-left", action: KeyAction::Left },
    Key { label: "", icon: "arrow_forward", name: "azcode-key-right", action: KeyAction::Right },
    Key { label: "", icon: "arrow_upward", name: "azcode-key-up", action: KeyAction::Up },
    Key { label: "", icon: "arrow_downward", name: "azcode-key-down", action: KeyAction::Down },
];

/// What a key did: the edits (positions in the text before them, the last in the text first, as the
/// buffer takes them; none for an arrow), and where the selection is afterwards (`anchor` and
/// `head` are one place when there is only a caret).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The edits to apply as one step.
    pub edits: Vec<Edit>,
    /// Where the selection starts.
    pub anchor: Pos,
    /// Where the caret is.
    pub head: Pos,
}

/// The number of characters of `line` before byte `column` (a column inside a character counts it).
fn chars_before(line: &str, column: usize) -> usize {
    line.char_indices().take_while(|(i, _)| *i < column).count()
}

/// The byte offset of character `chars` of `line` (the line's end past its last character).
fn byte_of(line: &str, chars: usize) -> usize {
    line.char_indices().nth(chars).map_or(line.len(), |(i, _)| i)
}

/// `column` moved back onto a character boundary of `line`, and no further than its end.
fn on_boundary(line: &str, column: usize) -> usize {
    let mut c = column.min(line.len());
    while c > 0 && !line.is_char_boundary(c) {
        c -= 1;
    }
    c
}

/// The visual column of byte `column` of `line`: tabs go to the next tab stop of `tab` columns.
fn visual_column(line: &str, column: usize, tab: usize) -> usize {
    let tab = tab.max(1);
    let mut col = 0;
    for (i, ch) in line.char_indices() {
        if i >= column {
            break;
        }
        col = if ch == '\t' { (col / tab + 1) * tab } else { col + 1 };
    }
    col
}

/// One level of indentation in the style of `line`: a tab where it is indented with tabs, else `tab`
/// spaces.
fn indent_unit(line: &str, tab: usize) -> String {
    if line.starts_with('\t') {
        "\t".to_string()
    } else {
        " ".repeat(tab.max(1))
    }
}

/// How many bytes of indentation one Shift+Tab takes off `line`: a tab, or up to `tab` spaces.
fn outdent_bytes(line: &str, tab: usize) -> usize {
    if line.starts_with('\t') {
        return 1;
    }
    line.bytes().take(tab.max(1)).take_while(|b| *b == b' ').count()
}

/// The lines a selection covers for indenting: its first to its last, the last left out when the
/// selection ends at its very start (a line only touched by the end of the selection).
fn covered_lines(start: Pos, end: Pos) -> (usize, usize) {
    if end.line > start.line && end.column == 0 {
        (start.line, end.line - 1)
    } else {
        (start.line, end.line)
    }
}

/// `at` after `text` (no line break in it) typed there.
const fn after(at: Pos, text_len: usize) -> Pos {
    Pos::new(at.line, at.column + text_len)
}

/// What key `action` does with `anchor` and `head` the selection in a file of `count` lines whose
/// line `i` is `lines(i)` (without its break); `tab` columns between two tab stops. `None` when it
/// does nothing (an arrow at the end of the file).
#[must_use]
pub fn press(
    action: KeyAction,
    lines: &dyn Fn(usize) -> String,
    count: usize,
    anchor: Pos,
    head: Pos,
    tab: usize,
) -> Option<Outcome> {
    let (start, end) = if anchor <= head { (anchor, head) } else { (head, anchor) };
    let caret = |at: Pos| Outcome { edits: Vec::new(), anchor: at, head: at };
    match action {
        KeyAction::Insert(text) => {
            let at = after(start, text.len());
            Some(Outcome { edits: vec![Edit::new(start, end, text)], anchor: at, head: at })
        }
        KeyAction::Pair(open, close) => {
            if start == end {
                let both = format!("{open}{close}");
                let at = after(start, open.len());
                return Some(Outcome { edits: vec![Edit::new(start, end, &both)], anchor: at, head: at });
            }
            // Around a selection: the closing one first (the last in the text), the selection stays
            // on what was in it.
            let inner_start = after(start, open.len());
            let inner_end = if end.line == start.line { after(end, open.len()) } else { end };
            let (new_anchor, new_head) = if anchor <= head { (inner_start, inner_end) } else { (inner_end, inner_start) };
            Some(Outcome {
                edits: vec![Edit::new(end, end, close), Edit::new(start, start, open)],
                anchor: new_anchor,
                head: new_head,
            })
        }
        KeyAction::Tab => {
            if start.line == end.line {
                // Spaces to the next tab stop, replacing what is selected on the line.
                let line = lines(start.line);
                let start = Pos::new(start.line, on_boundary(&line, start.column));
                let end = Pos::new(end.line, on_boundary(&line, end.column));
                let tab = tab.max(1);
                let spaces = tab - visual_column(&line, start.column, tab) % tab;
                let text = " ".repeat(spaces);
                let at = after(start, spaces);
                return Some(Outcome { edits: vec![Edit::new(start, end, &text)], anchor: at, head: at });
            }
            let (first, last) = covered_lines(start, end);
            let mut edits = Vec::new();
            let mut unit_of: Vec<(usize, usize)> = Vec::new();
            for l in first..=last.min(count.saturating_sub(1)) {
                let line = lines(l);
                if line.is_empty() {
                    continue;
                }
                let unit = indent_unit(&line, tab);
                unit_of.push((l, unit.len()));
                edits.push(Edit::new(Pos::new(l, 0), Pos::new(l, 0), &unit));
            }
            edits.reverse();
            let shifted = |p: Pos| {
                let by = unit_of.iter().find(|(l, _)| *l == p.line).map_or(0, |(_, n)| *n);
                Pos::new(p.line, p.column + by)
            };
            Some(Outcome { edits, anchor: shifted(anchor), head: shifted(head) })
        }
        KeyAction::BackTab => {
            let (first, last) = covered_lines(start, end);
            let mut edits = Vec::new();
            let mut taken: Vec<(usize, usize)> = Vec::new();
            for l in first..=last.min(count.saturating_sub(1)) {
                let line = lines(l);
                let n = outdent_bytes(&line, tab);
                if n == 0 {
                    continue;
                }
                taken.push((l, n));
                edits.push(Edit::new(Pos::new(l, 0), Pos::new(l, n), ""));
            }
            if edits.is_empty() {
                return None;
            }
            edits.reverse();
            let shifted = |p: Pos| {
                let by = taken.iter().find(|(l, _)| *l == p.line).map_or(0, |(_, n)| *n);
                Pos::new(p.line, p.column.saturating_sub(by))
            };
            Some(Outcome { edits, anchor: shifted(anchor), head: shifted(head) })
        }
        KeyAction::Left => {
            let line = lines(head.line);
            let chars = chars_before(&line, on_boundary(&line, head.column));
            if chars > 0 {
                return Some(caret(Pos::new(head.line, byte_of(&line, chars - 1))));
            }
            if head.line == 0 {
                return None;
            }
            let up = lines(head.line - 1);
            Some(caret(Pos::new(head.line - 1, up.len())))
        }
        KeyAction::Right => {
            let line = lines(head.line);
            let column = on_boundary(&line, head.column);
            let chars = chars_before(&line, column);
            if column < line.len() {
                return Some(caret(Pos::new(head.line, byte_of(&line, chars + 1))));
            }
            if head.line + 1 >= count {
                return None;
            }
            Some(caret(Pos::new(head.line + 1, 0)))
        }
        KeyAction::Up | KeyAction::Down => {
            let line = lines(head.line);
            let chars = chars_before(&line, on_boundary(&line, head.column));
            let target = if action == KeyAction::Up {
                head.line.checked_sub(1)?
            } else if head.line + 1 < count {
                head.line + 1
            } else {
                return None;
            };
            Some(caret(Pos::new(target, byte_of(&lines(target), chars))))
        }
    }
}

#[cfg(test)]
#[path = "keyrow_test.rs"]
mod tests;
