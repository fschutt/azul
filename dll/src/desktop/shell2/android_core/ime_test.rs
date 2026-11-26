//! The IME's view of the text, in UTF-16 units. No device.

use super::*;

fn mirror(text: &str, caret: usize) -> ImeMirror {
    ImeMirror {
        active: true,
        text: text.to_string(),
        caret,
        ..ImeMirror::default()
    }
}

/// `n` is a count of Java chars (UTF-16 units), not of bytes: ten characters of Japanese are thirty
/// bytes and ten units.
///
/// Mutation: a slice of `n` bytes (what the shell answered: three characters for ten asked).
#[test]
fn the_text_before_the_caret_counts_utf16_units_not_bytes() {
    let text = "こんにちは世界ですね";
    assert_eq!(text.len(), 30);
    assert_eq!(mirror(text, text.len()).text_before_cursor(10), text);
    assert_eq!(mirror(text, text.len()).text_before_cursor(3), "ですね");
    assert_eq!(mirror("abcdef", 6).text_before_cursor(2), "ef");
    assert_eq!(mirror("abcdef", 3).text_before_cursor(100), "abc");
    assert_eq!(mirror("abcdef", 0).text_before_cursor(5), "");
}

#[test]
fn the_text_after_the_caret_counts_utf16_units_not_bytes() {
    let text = "こんにちは世界";
    assert_eq!(mirror(text, 0).text_after_cursor(3), "こんに");
    assert_eq!(mirror("abcdef", 2).text_after_cursor(2), "cd");
    assert_eq!(mirror("abcdef", 6).text_after_cursor(4), "");
    assert_eq!(mirror("abcdef", 2).text_after_cursor(99), "cdef");
}

/// An emoji is a surrogate pair, two units: a request that would cut it in half gets the text without
/// it, never half of it (a lone surrogate is not valid text).
///
/// Mutation: a suffix cut at the unit; a count of one unit per char.
#[test]
fn a_surrogate_pair_is_never_cut_in_half() {
    let text = "a😀b";
    assert_eq!(utf16_len(text), 4);
    assert_eq!(mirror(text, text.len()).text_before_cursor(1), "b");
    assert_eq!(mirror(text, text.len()).text_before_cursor(2), "b", "one unit of the emoji is none of it");
    assert_eq!(mirror(text, text.len()).text_before_cursor(3), "😀b");
    assert_eq!(mirror(text, 1).text_after_cursor(1), "", "half of the emoji is none of it");
    assert_eq!(mirror(text, 1).text_after_cursor(2), "😀");
    assert_eq!(mirror(text, 1).text_after_cursor(3), "😀b");
}

#[test]
fn a_negative_or_zero_count_is_an_empty_answer() {
    assert_eq!(mirror("abc", 3).text_before_cursor(-4), "");
    assert_eq!(mirror("abc", 1).text_after_cursor(0), "");
}

/// A caret the mirror holds past the text or inside a char answers nothing rather than panicking in
/// the UI thread (the slice of a string at a byte that is not a boundary panics).
#[test]
fn a_caret_off_the_text_answers_nothing_and_does_not_panic() {
    assert_eq!(mirror("héllo", 2).text_before_cursor(5), "", "byte 2 is inside the é");
    assert_eq!(mirror("héllo", 99).text_before_cursor(2), "lo", "past the end is the end");
    assert_eq!(mirror("héllo", 2).text_after_cursor(5), "");
}

/// `deleteSurroundingText(before, after)` counts units; the engine deletes chars.
///
/// Mutation: one char per unit (an emoji took the character before it along).
#[test]
fn deleting_around_the_caret_deletes_whole_chars_for_the_units_asked() {
    assert_eq!(chars_covering_units_at_end("abc", 1), 1);
    assert_eq!(chars_covering_units_at_end("abc", 2), 2);
    assert_eq!(chars_covering_units_at_end("abc", 9), 3, "no more than there is");
    assert_eq!(chars_covering_units_at_end("a😀", 2), 1, "the emoji is two units and one char");
    assert_eq!(chars_covering_units_at_end("a😀", 1), 1, "a request that ends inside it takes it whole");
    assert_eq!(chars_covering_units_at_end("a😀", 3), 2);
    assert_eq!(chars_covering_units_at_end("abc", 0), 0);
    assert_eq!(chars_covering_units_at_start("😀bc", 2), 1);
    assert_eq!(chars_covering_units_at_start("😀bc", 3), 2);
    assert_eq!(chars_covering_units_at_start("", 3), 0);
}

#[test]
fn no_selection_is_null_and_an_empty_one_is_no_selection() {
    let mut m = mirror("abc", 1);
    assert_eq!(m.selected_text(), None);
    m.selected = Some(String::new());
    assert_eq!(m.selected_text(), None, "an empty string would read as a selection that is empty");
    m.selected = Some("b".to_string());
    assert_eq!(m.selected_text().as_deref(), Some("b"));
}

/// The rect the floating toolbar is placed by: four 16-bit fields, and 0 for none.
#[test]
fn the_selection_rect_is_packed_into_four_16_bit_fields() {
    assert_eq!(pack_rect(None), 0);
    let packed = pack_rect(Some((10, 20, 30, 40)));
    assert_eq!((packed >> 48) & 0xFFFF, 10);
    assert_eq!((packed >> 32) & 0xFFFF, 20);
    assert_eq!((packed >> 16) & 0xFFFF, 30);
    assert_eq!(packed & 0xFFFF, 40);
    // a rect of no size is one pixel (so it is not the "no selection" zero), a negative origin is 0,
    // a field past 16 bits is clamped, never wrapped into its neighbour
    let packed = pack_rect(Some((-5, 70_000, 0, 0)));
    assert_eq!((packed >> 48) & 0xFFFF, 0);
    assert_eq!((packed >> 32) & 0xFFFF, 65_535);
    assert_eq!((packed >> 16) & 0xFFFF, 1);
    assert_eq!(packed & 0xFFFF, 1);
    assert_ne!(pack_rect(Some((0, 0, 0, 0))), 0);
}

/// "Delete before the caret" is BACKSPACE: the engine's flag is `forward`, and Backspace is `false`
/// everywhere (`CallbackChange::DeleteBackward`, every desktop shell). The shell had it the other way
/// round, so an IME's delete-before ate the character after the caret.
///
/// Mutation: the two constants swapped; the counts swapped.
#[test]
fn deleting_before_the_caret_is_backspace_and_after_is_the_delete_key() {
    assert!(!DELETE_BACKWARD, "Backspace is `delete_selection(node, false)`");
    assert!(DELETE_FORWARD, "the Delete key is `delete_selection(node, true)`");
    assert_eq!(surrounding_delete_plan("abc", "def", 1, 0), (1, 0), "one char before, none after");
    assert_eq!(surrounding_delete_plan("abc", "def", 0, 2), (0, 2));
    assert_eq!(surrounding_delete_plan("abc", "def", 2, 1), (2, 1));
    // units, not chars, on each side
    assert_eq!(surrounding_delete_plan("a😀", "😀b", 2, 2), (1, 1));
    assert_eq!(surrounding_delete_plan("", "", 5, 5), (0, 0));
    assert_eq!(surrounding_delete_plan("abc", "def", -1, -1), (0, 0));
}
