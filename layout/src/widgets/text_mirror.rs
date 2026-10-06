//! The engine-mirror helpers `TextInput` and `TextArea` share.
//!
//! The engine owns a field's buffer; a widget keeps a MIRROR of it for its
//! hooks (`on_text_input` sees the would-be text before the engine commits
//! it). These read the engine's answer and compute the mirror's next text the
//! way the engine applies an insertion. One copy: the two widgets each had
//! their own, and a fix to one (typing over a selection replaces it, a
//! trailing caret stands after its cluster) left the other wrong (E2E-A,
//! 2026-10-06).

use alloc::string::String;

use azul_core::{dom::DomNodeId, selection::TextCursor};
use unicode_segmentation::UnicodeSegmentation;

use crate::callbacks::CallbackInfo;

/// The engine's text for `node` - an EMPTY answer too: it is the field the
/// user just cleared. `None` for a node the engine cannot read.
pub(crate) fn engine_text(info: &CallbackInfo, node: DomNodeId) -> Option<String> {
    info.get_node_text_content(node)
}

/// The byte offset in `text` a caret stands at: a LEADING caret before its
/// grapheme cluster, a TRAILING one after it.
pub(crate) fn caret_byte(cursor: &TextCursor, text: &str) -> usize {
    let start = (cursor.cluster_id.start_byte_in_run as usize).min(text.len());
    match cursor.affinity {
        azul_core::selection::CursorAffinity::Leading => start,
        azul_core::selection::CursorAffinity::Trailing => text
            .get(start..)
            .and_then(|rest| rest.graphemes(true).next())
            .map_or(start, |cluster| start + cluster.len()),
    }
}

/// The engine's caret in `node`, as a byte offset into the engine's buffer.
pub(crate) fn engine_caret(info: &CallbackInfo, node: DomNodeId) -> Option<usize> {
    let cursor = info.get_node_cursor_position(node)?;
    // An empty buffer has no cluster to measure: the cluster start then.
    Some(
        match info.get_node_text_content(node).filter(|t| !t.is_empty()) {
            Some(text) => caret_byte(&cursor, &text),
            None => cursor.cluster_id.start_byte_in_run as usize,
        },
    )
}

/// The engine's live selection in `node` as the byte range `[from, to)` of
/// `text` (the value before the edit), each end through its affinity. `None`
/// without a selection, for a collapsed one, or off `text`'s character
/// boundaries.
pub(crate) fn engine_selected_bytes(
    info: &CallbackInfo,
    node: DomNodeId,
    text: &str,
) -> Option<(usize, usize)> {
    let ranges = info.get_node_selection_ranges(node);
    let range = *ranges.as_ref().first()?;
    let a = caret_byte(&range.start, text);
    let b = caret_byte(&range.end, text);
    let (from, to) = (a.min(b), a.max(b));
    (from < to && text.is_char_boundary(from) && text.is_char_boundary(to)).then_some((from, to))
}

/// The text after the engine applies `inserted` to `text`, and the caret
/// after it (a byte offset): a live selection (`selected`) is replaced, else
/// the text goes in at the caret when it is readable and on a character
/// boundary, else at the end (where the caret sits for every append-only
/// path).
pub(crate) fn insertion(
    text: &str,
    inserted: &str,
    caret: Option<usize>,
    selected: Option<(usize, usize)>,
) -> (String, usize) {
    let (from, to) = match selected.filter(|&(a, b)| {
        a < b && b <= text.len() && text.is_char_boundary(a) && text.is_char_boundary(b)
    }) {
        Some(range) => range,
        None => {
            let at = caret
                .filter(|at| *at <= text.len() && text.is_char_boundary(*at))
                .unwrap_or(text.len());
            (at, at)
        }
    };
    let mut next = String::with_capacity(text.len() + inserted.len());
    next.push_str(&text[..from]);
    next.push_str(inserted);
    next.push_str(&text[to..]);
    (next, from.saturating_add(inserted.len()))
}

#[cfg(test)]
mod tests {
    use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};

    use super::*;

    fn at(byte: u32, affinity: CursorAffinity) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: byte,
            },
            affinity,
        }
    }

    #[test]
    fn an_insertion_replaces_a_selection_and_goes_in_at_the_caret_otherwise() {
        assert_eq!(insertion("krug", "e", Some(4), Some((0, 4))), ("e".into(), 1));
        assert_eq!(insertion("krug", "X", Some(2), None), ("krXug".into(), 3));
        assert_eq!(insertion("krug", "X", None, None), ("krugX".into(), 5));
        assert_eq!(insertion("ä", "X", Some(1), None), ("äX".into(), 3), "off a boundary: appended");
    }

    #[test]
    fn a_trailing_caret_stands_after_its_cluster() {
        assert_eq!(caret_byte(&at(3, CursorAffinity::Trailing), "krug"), 4);
        assert_eq!(caret_byte(&at(3, CursorAffinity::Leading), "krug"), 3);
        assert_eq!(caret_byte(&at(0, CursorAffinity::Trailing), "äb"), 2);
    }
}
