//! What the IME is told about the text it edits, in the units Android speaks.
//!
//! `InputConnection` is a Java interface the IME calls on the UI thread, in UTF-16 code units
//! (`getTextBeforeCursor(n)`: "n characters" of a Java `CharSequence`; `deleteSurroundingText(before,
//! after)`: the same). The engine's text is UTF-8 and its caret a byte offset. The shell used to answer
//! with `n` BYTES (an IME that asked for 10 characters of Japanese got 3, and the context its
//! prediction works from shrank with every non-ASCII character), and to delete `before` GRAPHEMES for
//! `before` UTF-16 units (an emoji is two units: one emoji and the character before it went).
//!
//! The answers come from a mirror the loop thread publishes ([`ImeMirror`]), never from the layout
//! window itself: the UI thread asking the layout window while the loop thread rebuilds it was a data
//! race.

/// The focused editable's state, as the IME sees it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImeMirror {
    /// An editable has the focus (when not, the IME is shown an empty, plain text field).
    pub active: bool,
    /// The editable's whole text.
    pub text: String,
    /// The caret: a byte offset into `text`, on a char boundary.
    pub caret: usize,
    /// The selected text, when the selection is not empty.
    pub selected: Option<String>,
    /// The selection's bounds in PHYSICAL pixels `(left, top, width, height)`, for the floating toolbar.
    pub selection_rect: Option<(i32, i32, i32, i32)>,
    /// `purpose | (multiline << 8)` of the focused control (`NativeTextBridge.nativeGetInputHints`).
    pub hints: i32,
    /// A composition is open.
    pub composing: bool,
}

/// UTF-16 code units of `s`.
#[must_use]
pub fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The longest suffix of `s` of at most `n` UTF-16 units that starts on a char (a surrogate pair is
/// never cut in half).
#[must_use]
pub fn last_utf16_units(s: &str, n: usize) -> &str {
    let mut units = 0;
    let mut start = s.len();
    for (index, c) in s.char_indices().rev() {
        units += c.len_utf16();
        if units > n {
            break;
        }
        start = index;
    }
    &s[start..]
}

/// The longest prefix of `s` of at most `n` UTF-16 units that ends on a char.
#[must_use]
pub fn first_utf16_units(s: &str, n: usize) -> &str {
    let mut units = 0;
    let mut end = 0;
    for (index, c) in s.char_indices() {
        units += c.len_utf16();
        if units > n {
            break;
        }
        end = index + c.len_utf8();
    }
    &s[..end]
}

/// The `forward` argument of the engine's `LayoutWindow::delete_selection(node, forward)` for Backspace:
/// the text BEFORE the caret goes (`CallbackChange::DeleteBackward` passes `false`, as every desktop shell
/// does for Backspace). The shell passed `true` for `deleteSurroundingText`'s "before" and `false` for its
/// "after": an IME's delete-before ate the character AFTER the caret.
pub const DELETE_BACKWARD: bool = false;
/// See [`DELETE_BACKWARD`]: the Delete key's direction, the text AFTER the caret.
pub const DELETE_FORWARD: bool = true;

/// What `InputConnection.deleteSurroundingText(before, after)` asks of the engine, in the engine's terms:
/// `(backward_chars, forward_chars)`, the whole chars that cover the UTF-16 units asked for on each side
/// of the caret. Each backward char is one `delete_selection(node, DELETE_BACKWARD)`, each forward char
/// one `delete_selection(node, DELETE_FORWARD)`.
#[must_use]
pub fn surrounding_delete_plan(
    text_before_caret: &str,
    text_after_caret: &str,
    before_units: i32,
    after_units: i32,
) -> (usize, usize) {
    let units = |n: i32| usize::try_from(n).unwrap_or(0);
    (
        chars_covering_units_at_end(text_before_caret, units(before_units)),
        chars_covering_units_at_start(text_after_caret, units(after_units)),
    )
}

/// How many whole chars cover `units` UTF-16 units at the end of `s` (what `deleteSurroundingText`
/// asks to delete before the caret): a surrogate pair is one char of two units, and a request that
/// ends inside one deletes the whole char.
#[must_use]
pub fn chars_covering_units_at_end(s: &str, units: usize) -> usize {
    let mut covered = 0;
    let mut chars = 0;
    for c in s.chars().rev() {
        if covered >= units {
            break;
        }
        covered += c.len_utf16();
        chars += 1;
    }
    chars
}

/// How many whole chars cover `units` UTF-16 units at the start of `s` (the delete after the caret).
#[must_use]
pub fn chars_covering_units_at_start(s: &str, units: usize) -> usize {
    let mut covered = 0;
    let mut chars = 0;
    for c in s.chars() {
        if covered >= units {
            break;
        }
        covered += c.len_utf16();
        chars += 1;
    }
    chars
}

impl ImeMirror {
    /// `getTextBeforeCursor(n)`: up to `n` UTF-16 units before the caret.
    #[must_use]
    pub fn text_before_cursor(&self, n: i32) -> String {
        let caret = self.caret.min(self.text.len());
        if !self.text.is_char_boundary(caret) {
            return String::new();
        }
        last_utf16_units(&self.text[..caret], usize::try_from(n).unwrap_or(0)).to_string()
    }

    /// `getTextAfterCursor(n)`: up to `n` UTF-16 units after the caret.
    #[must_use]
    pub fn text_after_cursor(&self, n: i32) -> String {
        let caret = self.caret.min(self.text.len());
        if !self.text.is_char_boundary(caret) {
            return String::new();
        }
        first_utf16_units(&self.text[caret..], usize::try_from(n).unwrap_or(0)).to_string()
    }

    /// `getSelectedText`: `None` when nothing is selected (an IME reads an empty string as "a
    /// selection that exists and is empty").
    #[must_use]
    pub fn selected_text(&self) -> Option<String> {
        self.selected.clone().filter(|s| !s.is_empty())
    }
}

/// The selection bounds as the one `jlong` `NativeTextBridge.nativeGetSelectionRect` returns: four
/// 16-bit fields `x | y | w | h`, each clamped to `0..=65535`, width and height at least 1; `0` means
/// no selection. Java reads them back with `(packed >> 48) & 0xFFFF` and so on.
#[must_use]
pub fn pack_rect(rect: Option<(i32, i32, i32, i32)>) -> i64 {
    let Some((x, y, w, h)) = rect else {
        return 0;
    };
    let clamp = |v: i32| i64::from(v.clamp(0, 65_535));
    (clamp(x) << 48) | (clamp(y) << 32) | (clamp(w).max(1) << 16) | clamp(h).max(1)
}

#[cfg(test)]
#[path = "ime_test.rs"]
mod tests;
