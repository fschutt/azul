//! `UITextInput` conformance — marked text, the edit menu and dictation.
//!
//! # Why this exists when `UIKeyInput` already worked
//!
//! `UIKeyInput` has three methods and no concept of a POSITION. It can accept
//! committed text and delete backwards, and that is all. Everything the system
//! does on top of a text field — an IME's marked text (the underlined preedit
//! the user is still choosing candidates for), the selection handles, the edit
//! menu, dictation, "Scan Text from Camera", the caret rectangle VoiceOver
//! reads — is routed through `UITextInput`, which is a 27-member protocol over
//! two object graphs.
//!
//! So on iOS a Japanese or Chinese user could type only what their IME had
//! already committed: the candidate bar arrived as finished text with no
//! preedit ever shown in the app. That is what "including IME" means here.
//!
//! # The document model
//!
//! UIKit addresses text by opaque `UITextPosition` objects. This maps them to
//! BYTE OFFSETS into one flat string:
//!
//! ```text
//!   committed[..caret] + preedit + committed[caret..]
//! ```
//!
//! The preedit is spliced in at the caret rather than kept beside the document,
//! because UIKit requires `textInRange:` to return marked text — it asks for
//! the range it was just given by `markedTextRange` and compares. A model that
//! hid the preedit would answer its own questions inconsistently, and UIKit
//! responds to that by asking again with a wider range until it gives up.
//!
//! # Why every method must be total
//!
//! UIKit probes for the protocol and then CALLS it. Returning nil where the
//! header says non-null crashes inside UIKit rather than degrading, which is
//! exactly why `10b` shipped `UIKeyInput` and left this for later. Every
//! position returned here is clamped into `0..=len`, every range is ordered,
//! and every accessor has a defined answer when nothing is focused (an empty
//! document at position 0) rather than a nil.

use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};

use objc::{Encode, Encoding};

use super::{azul_ios_window, ns_string_to_rust, settle, CGPoint, CGRect, CGSize, IOSWindow};
use crate::desktop::shell2::common::{debug_server::LogCategory, event::PlatformWindow};
use crate::log_debug;

/// `NSRange`. Not declared anywhere else in the iOS shell - `setMarkedText:
/// selectedRange:` is the first place it crosses the boundary.
///
/// Both fields are `NSUInteger`, which is 64-bit on every iOS device azul
/// targets. The encoding string must match exactly or the ObjC runtime
/// marshals the argument wrongly and the selected range comes through as
/// garbage.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NSRange {
    pub location: u64,
    pub length: u64,
}

unsafe impl Encode for NSRange {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{_NSRange=QQ}") }
    }
}

/// A zero rect, for the "nothing is focused" answers.
const ZERO_RECT: CGRect = CGRect {
    origin: CGPoint { x: 0.0, y: 0.0 },
    size: CGSize {
        width: 0.0,
        height: 0.0,
    },
};

/// `NSString` from a Rust string. The iOS shell has two private copies of this
/// already (accessibility, clipboard); this is a third rather than a refactor
/// that would touch two working modules for no behaviour change.
unsafe fn ns_string(s: &str) -> *mut Object {
    let bytes = s.as_bytes();
    let cls = class!(NSString);
    let obj: *mut Object = msg_send![
        cls,
        stringWithBytes: bytes.as_ptr()
                 length: bytes.len()
               encoding: 4usize // NSUTF8StringEncoding
    ];
    obj
}

const POSITION_IVAR: &str = "azOffset";
const RANGE_START_IVAR: &str = "azStart";
const RANGE_END_IVAR: &str = "azEnd";

/// `NSComparisonResult`.
const ORDERED_ASCENDING: i64 = -1;
const ORDERED_SAME: i64 = 0;
const ORDERED_DESCENDING: i64 = 1;

/// `UITextLayoutDirection` — the first four alias `UITextStorageDirection`.
const DIRECTION_RIGHT: i64 = 0;
const DIRECTION_LEFT: i64 = 1;
const DIRECTION_UP: i64 = 2;
const DIRECTION_DOWN: i64 = 3;

/// `NSWritingDirection.natural`.
const WRITING_DIRECTION_NATURAL: i64 = -1;

// ─── The document ─────────────────────────────────────────────────────

/// The focused node's text with the preedit spliced in at the caret.
///
/// Returns `(text, preedit_range)`. `preedit_range` is `None` when no
/// composition is open.
fn document(window: &IOSWindow) -> (String, Option<(usize, usize)>) {
    let Some(lw) = window.common.layout_window.as_ref() else {
        return (String::new(), None);
    };
    let Some(focused) = lw.focus_manager.get_focused_node().copied() else {
        return (String::new(), None);
    };
    let Some(node_id) = focused.node.into_crate_internal() else {
        return (String::new(), None);
    };
    let content = lw.get_text_before_textinput(focused.dom, node_id);
    let committed = lw.extract_text_from_inline_content(&content);

    let Some(preedit) = lw.text_edit_manager.preedit_text.as_ref() else {
        return (committed, None);
    };
    // The preedit is appended, NOT spliced at the caret.
    //
    // azul addresses text by grapheme cluster (`TextCursor` is a
    // `(source_run, start_byte_in_run)` pair), while UIKit addresses it by a
    // flat offset, and bridging the two needs the node's shaped layout. The
    // macOS shell - where IME already works - does not bridge them either: its
    // `markedRange` reports `(0, preedit_len)` and its `selectedRange` is a
    // fixed `(0, 0)`, with a comment recording that a `NSNotFound` there stops
    // the IME talking at all. This follows that proven model rather than
    // inventing a bridge that cannot be tested here.
    //
    // The consequence is precision, not correctness: composing in the MIDDLE
    // of existing text reports the preedit at the wrong offset, so the
    // candidate window may be positioned by the node rect instead of the
    // caret. Logged as 10b-i-b.
    let caret = committed.len();
    let mut text = String::with_capacity(committed.len() + preedit.len());
    text.push_str(&committed);
    text.push_str(preedit);
    (text, Some((caret, caret + preedit.len())))
}

/// Snap an offset to a char boundary at or below it.
///
/// UIKit hands back offsets it computed from OUR numbers, so they are usually
/// aligned — but `positionFromPosition:offset:` is documented in CHARACTERS
/// while this model is in bytes, and an IME moving through a multi-byte
/// candidate can land mid-scalar. Slicing there panics, so every offset that
/// reaches a slice goes through here first.
fn clamp_to_char_boundary(s: &str, mut offset: usize) -> usize {
    if offset >= s.len() {
        return s.len();
    }
    while offset > 0 && !s.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn document_len(window: &IOSWindow) -> usize {
    document(window).0.len()
}

// ─── AzulTextPosition / AzulTextRange ─────────────────────────────────

fn position_class() -> &'static Class {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    static mut CLS: *const Class = core::ptr::null();
    ONCE.call_once(|| unsafe {
        let superclass = class!(UITextPosition);
        let mut decl = ClassDecl::new("AzulTextPosition", superclass)
            .expect("AzulTextPosition class name already registered");
        decl.add_ivar::<i64>(POSITION_IVAR);
        CLS = decl.register();
    });
    unsafe { &*CLS }
}

fn range_class() -> &'static Class {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    static mut CLS: *const Class = core::ptr::null();
    ONCE.call_once(|| unsafe {
        let superclass = class!(UITextRange);
        let mut decl = ClassDecl::new("AzulTextRange", superclass)
            .expect("AzulTextRange class name already registered");
        decl.add_ivar::<i64>(RANGE_START_IVAR);
        decl.add_ivar::<i64>(RANGE_END_IVAR);
        // `start`, `end` and `isEmpty` are the three readonly properties
        // UITextRange declares. UIKit reads them constantly.
        decl.add_method(
            sel!(start),
            range_start as extern "C" fn(&Object, Sel) -> *mut Object,
        );
        decl.add_method(
            sel!(end),
            range_end as extern "C" fn(&Object, Sel) -> *mut Object,
        );
        decl.add_method(sel!(isEmpty), range_is_empty as extern "C" fn(&Object, Sel) -> bool);
        CLS = decl.register();
    });
    unsafe { &*CLS }
}

/// Build an `AzulTextPosition`. Always non-nil: UIKit treats a nil where the
/// header promises an object as a programmer error and crashes.
fn make_position(offset: usize) -> *mut Object {
    unsafe {
        let cls = position_class();
        let obj: *mut Object = msg_send![cls, alloc];
        let obj: *mut Object = msg_send![obj, init];
        if obj.is_null() {
            return obj;
        }
        let ob: &mut Object = &mut *obj;
        ob.set_ivar::<i64>(POSITION_IVAR, i64::try_from(offset).unwrap_or(i64::MAX));
        obj
    }
}

fn make_range(start: usize, end: usize) -> *mut Object {
    // Ordered on construction, so nothing downstream has to wonder. UIKit
    // does hand out reversed pairs while dragging a selection backwards.
    let (start, end) = if start <= end { (start, end) } else { (end, start) };
    unsafe {
        let cls = range_class();
        let obj: *mut Object = msg_send![cls, alloc];
        let obj: *mut Object = msg_send![obj, init];
        if obj.is_null() {
            return obj;
        }
        let ob: &mut Object = &mut *obj;
        ob.set_ivar::<i64>(RANGE_START_IVAR, i64::try_from(start).unwrap_or(i64::MAX));
        ob.set_ivar::<i64>(RANGE_END_IVAR, i64::try_from(end).unwrap_or(i64::MAX));
        obj
    }
}

/// Read a position's offset. `None` for nil or a foreign class — UIKit is
/// documented to pass only our own objects back, but a wrong assumption here
/// would be a wild read rather than a miss.
unsafe fn position_offset(pos: *mut Object) -> Option<usize> {
    if pos.is_null() {
        return None;
    }
    let is_ours: bool = msg_send![pos, isKindOfClass: position_class()];
    if !is_ours {
        return None;
    }
    let v: i64 = *(*pos).get_ivar::<i64>(POSITION_IVAR);
    usize::try_from(v).ok()
}

unsafe fn range_bounds(range: *mut Object) -> Option<(usize, usize)> {
    if range.is_null() {
        return None;
    }
    let is_ours: bool = msg_send![range, isKindOfClass: range_class()];
    if !is_ours {
        return None;
    }
    let s: i64 = *(*range).get_ivar::<i64>(RANGE_START_IVAR);
    let e: i64 = *(*range).get_ivar::<i64>(RANGE_END_IVAR);
    Some((usize::try_from(s).ok()?, usize::try_from(e).ok()?))
}

extern "C" fn range_start(this: &Object, _cmd: Sel) -> *mut Object {
    let v: i64 = unsafe { *this.get_ivar::<i64>(RANGE_START_IVAR) };
    make_position(usize::try_from(v).unwrap_or(0))
}

extern "C" fn range_end(this: &Object, _cmd: Sel) -> *mut Object {
    let v: i64 = unsafe { *this.get_ivar::<i64>(RANGE_END_IVAR) };
    make_position(usize::try_from(v).unwrap_or(0))
}

extern "C" fn range_is_empty(this: &Object, _cmd: Sel) -> bool {
    unsafe {
        let s: i64 = *this.get_ivar::<i64>(RANGE_START_IVAR);
        let e: i64 = *this.get_ivar::<i64>(RANGE_END_IVAR);
        s == e
    }
}

// ─── UITextInput: reading and writing text ────────────────────────────

extern "C" fn text_in_range(_this: &Object, _cmd: Sel, range: *mut Object) -> *mut Object {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return unsafe { ns_string("") };
    };
    let Some((start, end)) = (unsafe { range_bounds(range) }) else {
        return unsafe { ns_string("") };
    };
    let (text, _) = document(window);
    let start = clamp_to_char_boundary(&text, start.min(text.len()));
    let end = clamp_to_char_boundary(&text, end.min(text.len()));
    unsafe { ns_string(&text[start..end]) }
}

extern "C" fn replace_range_with_text(
    _this: &Object,
    _cmd: Sel,
    range: *mut Object,
    text: *mut Object,
) {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return;
    };
    let replacement = unsafe { ns_string_to_rust(text) };
    let Some((start, end)) = (unsafe { range_bounds(range) }) else {
        return;
    };
    let doc_len = document(window).0.len();
    if let Some(lw) = window.common.layout_window.as_mut() {
        if start == end {
            // A pure INSERT at a point. The engine inserts at its own caret,
            // which is the end of the document in this offset space, so this
            // is exact for the common case (typing, dictation appending).
            if !replacement.is_empty() {
                let _ = lw.record_text_input(&replacement);
            }
        } else if end >= doc_len && start <= doc_len {
            // A replace that reaches the END of the document - autocorrect
            // swapping the word being typed. Expressible as delete-back then
            // insert, which is what the engine models.
            if let Some(focused) = lw.focus_manager.get_focused_node().copied() {
                for _ in 0..(end - start) {
                    lw.delete_selection(focused, false);
                }
                if !replacement.is_empty() {
                    let _ = lw.record_text_input(&replacement);
                }
            }
        } else {
            // An interior replace. Deliberately DROPPED rather than applied
            // as an insert: falling through to an insert would append the
            // correction instead of replacing, silently duplicating text on
            // every autocorrect. Doing nothing loses the correction; doing
            // the wrong thing corrupts the document. Logged as 10b-i-b.
            log_debug!(
                LogCategory::Input,
                "[iOS] replaceRange {}..{} is interior; no engine seam for it (10b-i-b)",
                start,
                end
            );
        }
    }
    let result = window.process_window_events(0);
    settle(window, result);
}

// ─── Selection and marked text ────────────────────────────────────────

extern "C" fn selected_text_range(_this: &Object, _cmd: Sel) -> *mut Object {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return core::ptr::null_mut();
    };
    let (text, _) = document(window);
    let Some(lw) = window.common.layout_window.as_ref() else {
        return core::ptr::null_mut();
    };
    // nil is LEGAL here (the header marks it nullable) and means "no
    // selection", which is the honest answer with nothing focused.
    if lw.focus_manager.get_focused_node().is_none() {
        return core::ptr::null_mut();
    }
    // A caret at the end of the document: a VALID, non-nil answer, which is
    // what keeps UIKit sending text. macOS pins this to `(0, 0)` for exactly
    // the same reason - the answer has to be a real insertion point even when
    // the true one cannot be expressed in this offset space.
    let _ = lw;
    let caret = text.len();
    make_range(caret, caret)
}

extern "C" fn set_selected_text_range(_this: &Object, _cmd: Sel, range: *mut Object) {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return;
    };
    // Accepted and ignored. There is no engine seam that sets a selection
    // from a BYTE RANGE - `TextEditManager` selects by grapheme cluster - and
    // fabricating one from an offset without the shaped layout would move the
    // caret to the wrong grapheme in any non-ASCII text.
    //
    // Ignoring is safe: the dragged selection handle springs back, which is a
    // visible limitation rather than a corruption. Logged as 10b-i-b.
    let _ = (window, unsafe { range_bounds(range) });
}

extern "C" fn marked_text_range(_this: &Object, _cmd: Sel) -> *mut Object {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return core::ptr::null_mut();
    };
    // nil = "no composition open", which is what UIKit expects and is why
    // this one is nullable in the header.
    match document(window).1 {
        Some((start, end)) => make_range(start, end),
        None => core::ptr::null_mut(),
    }
}

extern "C" fn marked_text_style(_this: &Object, _cmd: Sel) -> *mut Object {
    // nil = "style the marked text however you normally would". Returning an
    // empty dictionary instead would suppress the preedit underline, which is
    // the one piece of UI that tells the user a composition is open.
    core::ptr::null_mut()
}

extern "C" fn set_marked_text_style(_this: &Object, _cmd: Sel, _style: *mut Object) {}

/// `setMarkedText:selectedRange:` — THE method this whole conformance exists
/// for. UIKit calls it on every keystroke while an IME candidate is being
/// composed, and `UIKeyInput` has no equivalent at all.
extern "C" fn set_marked_text(
    _this: &Object,
    _cmd: Sel,
    marked_text: *mut Object,
    selected_range: NSRange,
) {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return;
    };
    let text = unsafe { ns_string_to_rust(marked_text) };
    if let Some(lw) = window.common.layout_window.as_mut() {
        if text.is_empty() {
            lw.text_edit_manager.clear_preedit();
        } else {
            // `selectedRange` is relative to the MARKED TEXT, not the
            // document - which is exactly what `set_preedit` wants, so the
            // two line up without translation.
            let begin = i32::try_from(selected_range.location).unwrap_or(0);
            let len = i32::try_from(selected_range.length).unwrap_or(0);
            lw.text_edit_manager.set_preedit(text, begin, begin + len);
        }
    }
    let result = window.process_window_events(0);
    settle(window, result);
}

extern "C" fn unmark_text(_this: &Object, _cmd: Sel) {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return;
    };
    if let Some(lw) = window.common.layout_window.as_mut() {
        // COMMIT, not discard. `unmarkText` means "the composition is
        // finished, accept it" - throwing the preedit away here would delete
        // whatever the user had just chosen.
        let preedit = lw.text_edit_manager.preedit_text.clone().unwrap_or_default();
        if !preedit.is_empty() {
            lw.text_edit_manager.commit_composition(preedit.clone());
            let _ = lw.record_text_input(&preedit);
        } else {
            lw.text_edit_manager.clear_preedit();
        }
    }
    let result = window.process_window_events(0);
    settle(window, result);
}

// ─── Positions and ranges ─────────────────────────────────────────────

extern "C" fn beginning_of_document(_this: &Object, _cmd: Sel) -> *mut Object {
    make_position(0)
}

extern "C" fn end_of_document(_this: &Object, _cmd: Sel) -> *mut Object {
    let len = unsafe { azul_ios_window() }.map_or(0, |w| document_len(w));
    make_position(len)
}

extern "C" fn text_range_from_position_to_position(
    _this: &Object,
    _cmd: Sel,
    from: *mut Object,
    to: *mut Object,
) -> *mut Object {
    unsafe {
        match (position_offset(from), position_offset(to)) {
            (Some(a), Some(b)) => make_range(a, b),
            _ => core::ptr::null_mut(),
        }
    }
}

extern "C" fn position_from_position_offset(
    _this: &Object,
    _cmd: Sel,
    position: *mut Object,
    offset: i64,
) -> *mut Object {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return core::ptr::null_mut();
    };
    let Some(base) = (unsafe { position_offset(position) }) else {
        return core::ptr::null_mut();
    };
    let (text, _) = document(window);
    // Out of bounds returns nil, which the header allows and UIKit relies on
    // to discover where the document ends. Saturating to the end instead
    // would make it walk forever.
    let Some(target) = base.checked_add_signed(offset as isize) else {
        return core::ptr::null_mut();
    };
    if target > text.len() {
        return core::ptr::null_mut();
    }
    make_position(clamp_to_char_boundary(&text, target))
}

extern "C" fn position_from_position_in_direction_offset(
    this: &Object,
    cmd: Sel,
    position: *mut Object,
    direction: i64,
    offset: i64,
) -> *mut Object {
    // Left/right are the storage directions; up/down are line motion, which
    // this flat model cannot express, so they are treated as line-sized
    // horizontal motion rather than answered wrongly with nil (nil here makes
    // UIKit think the document ends at the caret).
    let signed = match direction {
        DIRECTION_LEFT | DIRECTION_UP => -offset,
        DIRECTION_RIGHT | DIRECTION_DOWN => offset,
        _ => offset,
    };
    position_from_position_offset(this, cmd, position, signed)
}

extern "C" fn compare_position_to_position(
    _this: &Object,
    _cmd: Sel,
    a: *mut Object,
    b: *mut Object,
) -> i64 {
    unsafe {
        match (position_offset(a), position_offset(b)) {
            (Some(x), Some(y)) if x < y => ORDERED_ASCENDING,
            (Some(x), Some(y)) if x > y => ORDERED_DESCENDING,
            _ => ORDERED_SAME,
        }
    }
}

extern "C" fn offset_from_position_to_position(
    _this: &Object,
    _cmd: Sel,
    from: *mut Object,
    to: *mut Object,
) -> i64 {
    unsafe {
        match (position_offset(from), position_offset(to)) {
            (Some(a), Some(b)) => i64::try_from(b).unwrap_or(0) - i64::try_from(a).unwrap_or(0),
            _ => 0,
        }
    }
}

extern "C" fn position_within_range_farthest_in_direction(
    _this: &Object,
    _cmd: Sel,
    range: *mut Object,
    direction: i64,
) -> *mut Object {
    let Some((start, end)) = (unsafe { range_bounds(range) }) else {
        return core::ptr::null_mut();
    };
    match direction {
        DIRECTION_LEFT | DIRECTION_UP => make_position(start),
        _ => make_position(end),
    }
}

extern "C" fn character_range_by_extending_position_in_direction(
    _this: &Object,
    _cmd: Sel,
    position: *mut Object,
    direction: i64,
) -> *mut Object {
    let Some(window) = (unsafe { azul_ios_window() }) else {
        return core::ptr::null_mut();
    };
    let Some(base) = (unsafe { position_offset(position) }) else {
        return core::ptr::null_mut();
    };
    let (text, _) = document(window);
    // One CHARACTER, not one byte: extending by a byte would split a
    // multi-byte scalar and every subsequent slice would panic.
    match direction {
        DIRECTION_LEFT | DIRECTION_UP => {
            let start = text[..base.min(text.len())]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
            make_range(start, base)
        }
        _ => {
            let end = text[base.min(text.len())..]
                .chars()
                .next()
                .map_or(base, |c| base + c.len_utf8());
            make_range(base, end.min(text.len()))
        }
    }
}

// ─── Writing direction ────────────────────────────────────────────────

extern "C" fn base_writing_direction_for_position(
    _this: &Object,
    _cmd: Sel,
    _position: *mut Object,
    _direction: i64,
) -> i64 {
    // `natural` defers to the paragraph's own resolved direction, which the
    // engine already computes during shaping. Reporting a hard LTR here would
    // put the caret on the wrong side of Arabic and Hebrew text.
    WRITING_DIRECTION_NATURAL
}

extern "C" fn set_base_writing_direction_for_range(
    _this: &Object,
    _cmd: Sel,
    _direction: i64,
    _range: *mut Object,
) {
    // The engine has no per-range direction override; ignoring the request is
    // the documented behaviour for a view that does not support one.
}

// ─── Geometry ─────────────────────────────────────────────────────────
//
// UIKit uses these to place the caret, the selection handles, the magnifier
// and the candidate window. They are answered from the FOCUSED NODE's rect
// rather than per-glyph: the engine's per-character rects are not reachable
// from here without a layout query the shell has no seam for, and a plausible
// box puts the IME candidate window in the right region instead of at the
// screen origin. Refining them to real glyph rects is 10b-i-a.

extern "C" fn first_rect_for_range(_this: &Object, _cmd: Sel, range: *mut Object) -> CGRect {
    // PER-GLYPH now (10b-i-a). `firstRectForRange:` places the IME candidate
    // window, so the node box put it beside the field rather than under the
    // text being composed.
    let bounds = unsafe { range_bounds(range) };
    let rect = bounds.and_then(|(start, end)| {
        let window = unsafe { azul_ios_window() }?;
        let lw = window.common.layout_window.as_ref()?;
        lw.focused_rect_for_byte_range(start, end)
    });
    match rect {
        Some(r) => to_cgrect(r),
        // The node box is still the FALLBACK, and a deliberate one: a field
        // with no live text layout (empty, or not yet laid out) has no glyph
        // to point at, and answering zero would put the candidate window in
        // the screen corner.
        None => focused_node_rect().unwrap_or(ZERO_RECT),
    }
}

extern "C" fn caret_rect_for_position(_this: &Object, _cmd: Sel, position: *mut Object) -> CGRect {
    let offset = unsafe { position_offset(position) };
    let rect = offset.and_then(|offset| {
        let window = unsafe { azul_ios_window() }?;
        let lw = window.common.layout_window.as_ref()?;
        lw.focused_rect_for_byte_offset(offset)
    });
    let mut r = match rect {
        Some(r) => to_cgrect(r),
        None => focused_node_rect().unwrap_or(ZERO_RECT),
    };
    // Zero WIDTH, never zero height: the engine's caret rect is already a thin
    // bar, but a fallback node box is not - and a zero-HEIGHT rect makes UIKit
    // place the magnifier off-screen.
    r.size.width = 2.0;
    if r.size.height <= 0.0 {
        r.size.height = 1.0;
    }
    r
}

extern "C" fn selection_rects_for_range(_this: &Object, _cmd: Sel, _range: *mut Object) -> *mut Object {
    // An EMPTY array, not nil: UIKit enumerates the result unconditionally.
    unsafe { msg_send![class!(NSArray), array] }
}

extern "C" fn closest_position_to_point(_this: &Object, _cmd: Sel, point: CGPoint) -> *mut Object {
    // A REAL HIT TEST now (10b-i-a). This drives the loupe and tap-to-place:
    // answering "the end of the document" for every point, as this used to,
    // meant a tap anywhere put the caret at the end.
    let hit = (|| {
        let window = unsafe { azul_ios_window() }?;
        let lw = window.common.layout_window.as_ref()?;
        lw.focused_byte_offset_for_point(azul_core::geom::LogicalPosition::new(
            point.x as f32,
            point.y as f32,
        ))
    })();
    match hit {
        Some(offset) => make_position(offset),
        // The end of the document remains the fallback for a field with no
        // text layout. Nil would break the loupe entirely.
        None => {
            let len = unsafe { azul_ios_window() }.map_or(0, |w| document_len(w));
            make_position(len)
        }
    }
}

extern "C" fn closest_position_to_point_within_range(
    _this: &Object,
    _cmd: Sel,
    point: CGPoint,
    range: *mut Object,
) -> *mut Object {
    let Some((start, end)) = (unsafe { range_bounds(range) }) else {
        return core::ptr::null_mut();
    };
    // The real hit test, then CLAMPED to the range - which is the whole
    // difference from `closestPositionToPoint:`. Returning the range's end
    // unconditionally, as this used to, made a drag inside a selection jump to
    // its far edge.
    let hit = (|| {
        let window = unsafe { azul_ios_window() }?;
        let lw = window.common.layout_window.as_ref()?;
        lw.focused_byte_offset_for_point(azul_core::geom::LogicalPosition::new(
            point.x as f32,
            point.y as f32,
        ))
    })();
    let (lo, hi) = if start <= end { (start, end) } else { (end, start) };
    make_position(hit.map_or(hi, |o| o.clamp(lo, hi)))
}

extern "C" fn character_range_at_point(_this: &Object, _cmd: Sel, _point: CGPoint) -> *mut Object {
    core::ptr::null_mut()
}

/// An engine rect as a `CGRect`.
///
/// Both are already in LOGICAL points and in view coordinates, so this is a
/// widening cast and nothing else - a scale factor here would be the classic
/// double-application bug, since UIKit works in points too.
fn to_cgrect(r: azul_core::geom::LogicalRect) -> CGRect {
    CGRect {
        origin: CGPoint {
            x: f64::from(r.origin.x),
            y: f64::from(r.origin.y),
        },
        size: CGSize {
            width: f64::from(r.size.width),
            height: f64::from(r.size.height),
        },
    }
}

/// The focused node's rect in view coordinates, if there is one.
fn focused_node_rect() -> Option<CGRect> {
    let window = unsafe { azul_ios_window() }?;
    let lw = window.common.layout_window.as_ref()?;
    let focused = lw.focus_manager.get_focused_node().copied()?;
    let node_id = focused.node.into_crate_internal()?;
    let _ = node_id;
    let position = lw.get_node_position(focused)?;
    let size = lw.get_node_size(focused)?;
    Some(CGRect {
        origin: CGPoint {
            x: f64::from(position.x),
            y: f64::from(position.y),
        },
        size: CGSize {
            width: f64::from(size.width),
            height: f64::from(size.height),
        },
    })
}

// ─── Delegate + tokenizer ─────────────────────────────────────────────

extern "C" fn input_delegate(_this: &Object, _cmd: Sel) -> *mut Object {
    unsafe { INPUT_DELEGATE }
}

extern "C" fn set_input_delegate(_this: &Object, _cmd: Sel, delegate: *mut Object) {
    // Stored WEAK per the header: UIKit owns the delegate and retaining it
    // here would be a cycle that outlives the keyboard session.
    unsafe { INPUT_DELEGATE = delegate };
}

static mut INPUT_DELEGATE: *mut Object = core::ptr::null_mut();
static mut TOKENIZER: *mut Object = core::ptr::null_mut();

extern "C" fn tokenizer(this: &Object, _cmd: Sel) -> *mut Object {
    unsafe {
        if TOKENIZER.is_null() {
            // `UITextInputStringTokenizer` is Apple's own concrete
            // implementation of the 4-method tokenizer protocol. Using it is
            // what the documentation recommends and saves reimplementing word
            // and paragraph boundaries - which would be WRONG for most
            // languages if hand-rolled here.
            let Some(cls) = Class::get("UITextInputStringTokenizer") else {
                return core::ptr::null_mut();
            };
            let t: *mut Object = msg_send![cls, alloc];
            let this_ptr = this as *const Object as *mut Object;
            TOKENIZER = msg_send![t, initWithTextInput: this_ptr];
        }
        TOKENIZER
    }
}

// ─── Registration ─────────────────────────────────────────────────────

/// Add every `UITextInput` member to the view class.
///
/// Called from `get_or_create_view_class` BEFORE `decl.register()`. Protocol
/// conformance is declared separately and last, so that a class missing a
/// method can never be advertised as conforming.
pub(super) fn register(decl: &mut ClassDecl) {
    unsafe {
        decl.add_method(
            sel!(textInRange:),
            text_in_range as extern "C" fn(&Object, Sel, *mut Object) -> *mut Object,
        );
        decl.add_method(
            sel!(replaceRange:withText:),
            replace_range_with_text as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(selectedTextRange),
            selected_text_range as extern "C" fn(&Object, Sel) -> *mut Object,
        );
        decl.add_method(
            sel!(setSelectedTextRange:),
            set_selected_text_range as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(markedTextRange),
            marked_text_range as extern "C" fn(&Object, Sel) -> *mut Object,
        );
        decl.add_method(
            sel!(markedTextStyle),
            marked_text_style as extern "C" fn(&Object, Sel) -> *mut Object,
        );
        decl.add_method(
            sel!(setMarkedTextStyle:),
            set_marked_text_style as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(setMarkedText:selectedRange:),
            set_marked_text as extern "C" fn(&Object, Sel, *mut Object, NSRange),
        );
        decl.add_method(sel!(unmarkText), unmark_text as extern "C" fn(&Object, Sel));
        decl.add_method(
            sel!(beginningOfDocument),
            beginning_of_document as extern "C" fn(&Object, Sel) -> *mut Object,
        );
        decl.add_method(
            sel!(endOfDocument),
            end_of_document as extern "C" fn(&Object, Sel) -> *mut Object,
        );
        decl.add_method(
            sel!(textRangeFromPosition:toPosition:),
            text_range_from_position_to_position
                as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> *mut Object,
        );
        decl.add_method(
            sel!(positionFromPosition:offset:),
            position_from_position_offset
                as extern "C" fn(&Object, Sel, *mut Object, i64) -> *mut Object,
        );
        decl.add_method(
            sel!(positionFromPosition:inDirection:offset:),
            position_from_position_in_direction_offset
                as extern "C" fn(&Object, Sel, *mut Object, i64, i64) -> *mut Object,
        );
        decl.add_method(
            sel!(comparePosition:toPosition:),
            compare_position_to_position
                as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(offsetFromPosition:toPosition:),
            offset_from_position_to_position
                as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(inputDelegate),
            input_delegate as extern "C" fn(&Object, Sel) -> *mut Object,
        );
        decl.add_method(
            sel!(setInputDelegate:),
            set_input_delegate as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(tokenizer),
            tokenizer as extern "C" fn(&Object, Sel) -> *mut Object,
        );
        decl.add_method(
            sel!(positionWithinRange:farthestInDirection:),
            position_within_range_farthest_in_direction
                as extern "C" fn(&Object, Sel, *mut Object, i64) -> *mut Object,
        );
        decl.add_method(
            sel!(characterRangeByExtendingPosition:inDirection:),
            character_range_by_extending_position_in_direction
                as extern "C" fn(&Object, Sel, *mut Object, i64) -> *mut Object,
        );
        decl.add_method(
            sel!(baseWritingDirectionForPosition:inDirection:),
            base_writing_direction_for_position
                as extern "C" fn(&Object, Sel, *mut Object, i64) -> i64,
        );
        decl.add_method(
            sel!(setBaseWritingDirection:forRange:),
            set_base_writing_direction_for_range
                as extern "C" fn(&Object, Sel, i64, *mut Object),
        );
        decl.add_method(
            sel!(firstRectForRange:),
            first_rect_for_range as extern "C" fn(&Object, Sel, *mut Object) -> CGRect,
        );
        decl.add_method(
            sel!(caretRectForPosition:),
            caret_rect_for_position as extern "C" fn(&Object, Sel, *mut Object) -> CGRect,
        );
        decl.add_method(
            sel!(selectionRectsForRange:),
            selection_rects_for_range as extern "C" fn(&Object, Sel, *mut Object) -> *mut Object,
        );
        decl.add_method(
            sel!(closestPositionToPoint:),
            closest_position_to_point as extern "C" fn(&Object, Sel, CGPoint) -> *mut Object,
        );
        decl.add_method(
            sel!(closestPositionToPoint:withinRange:),
            closest_position_to_point_within_range
                as extern "C" fn(&Object, Sel, CGPoint, *mut Object) -> *mut Object,
        );
        decl.add_method(
            sel!(characterRangeAtPoint:),
            character_range_at_point as extern "C" fn(&Object, Sel, CGPoint) -> *mut Object,
        );
    }
}
