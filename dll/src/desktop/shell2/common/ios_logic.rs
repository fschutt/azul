//! The decisions of the iOS shell that need no UIKit, so a host test binary can run them
//! (`shell2/ios` itself only exists for an iOS target).
//!
//! UIKit hands the shell numbers in POINTS: the safe-area insets of the render view, the
//! keyboard's frame, the view's bounds. Everything here takes those numbers (never an object) and
//! answers with the engine's own types.

use azul_css::{
    props::basic::pixel::{OptionPixelValue, PixelValue},
    system::SafeAreaInsets,
};

/// An inset of this many points or less reads as "no inset". UIKit answers 0.0 on a screen without
/// a notch or a home indicator and sub-point fractions on a few scaled displays; neither is a
/// region content must keep out of.
pub const INSET_THRESHOLD_POINTS: f64 = 0.5;

/// The longest piece of one line handed to the system log. The unified log cuts a message at about
/// 1024 bytes ("<Mixed Content>" past it), and a panic's backtrace is far longer.
pub const SYSTEM_LOG_PIECE_BYTES: usize = 800;

/// One edge of a `UIEdgeInsets` as the engine's optional inset: `None` for no inset (and for a
/// number that is not one), the value in CSS pixels (= points on iOS) otherwise.
#[must_use]
pub fn edge_inset(points: f64) -> OptionPixelValue {
    if points.is_finite() && points > INSET_THRESHOLD_POINTS {
        OptionPixelValue::Some(PixelValue::px(points as f32))
    } else {
        OptionPixelValue::None
    }
}

/// The window's safe-area insets after a `layoutSubviews` reported the four edges of the render
/// view's `safeAreaInsets`.
///
/// The four edges are UIKit's; the KEYBOARD inset is not: it is written by the
/// `UIKeyboardWillChangeFrameNotification` handler and has to survive every layout pass. A pass
/// that rebuilt the whole struct (as the shell did) wiped the keyboard's inset to `None` the
/// next time UIKit laid the view out, so an app could never see the keyboard it had raised.
#[must_use]
pub fn safe_area_after_layout(
    previous: &SafeAreaInsets,
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
) -> SafeAreaInsets {
    SafeAreaInsets {
        top: edge_inset(top),
        bottom: edge_inset(bottom),
        left: edge_inset(left),
        right: edge_inset(right),
        keyboard: previous.keyboard,
    }
}

/// How many points of a view the keyboard covers, from the view's bottom edge.
///
/// `view_bottom` is the bottom of the view's bounds (`bounds.origin.y + bounds.size.height`),
/// `keyboard_top` the top of the keyboard's end frame in the SAME coordinate space,
/// `view_height` the height of the view. A keyboard below the view (UIKit reports a dismissed
/// keyboard as a frame under the screen, not as an empty one) covers nothing, and one taller than
/// the view cannot cover more than the view has. Never negative, never NaN.
#[must_use]
pub fn keyboard_cover(view_bottom: f64, keyboard_top: f64, view_height: f64) -> f64 {
    let overlap = view_bottom - keyboard_top;
    if !overlap.is_finite() || !view_height.is_finite() {
        return 0.0;
    }
    overlap.max(0.0).min(view_height.max(0.0))
}

/// The keyboard's inset for a cover: `None` when it covers (next to) nothing, which the field's
/// own docs distinguish from `Some(0)`: an app laying out around a zero inset would reserve a row
/// for a keyboard that is not there.
#[must_use]
pub fn keyboard_inset(cover: f64) -> OptionPixelValue {
    edge_inset(cover)
}

/// The name an Objective-C class is declared under on attempt `attempt`: the plain name first,
/// then `<name>_1`, `<name>_2`, ...
///
/// `objc_allocateClassPair` refuses a name that is taken, which is what a second copy of this
/// library in one process (a static libazul and a plug-in that links its own) runs into. The
/// shell declared its classes with `.unwrap()`, and a panic in a phone app is an abort at launch;
/// it now takes the next free name, each copy reaching only the classes it registered itself.
#[must_use]
pub fn class_name_attempt(name: &str, attempt: u32) -> String {
    if attempt == 0 {
        name.to_string()
    } else {
        format!("{name}_{attempt}")
    }
}

/// `text` cut into pieces of at most `max_bytes` bytes each, on line ends where it can and on
/// `char` boundaries always, so the system log (which cuts a message at about 1 KiB) keeps all of
/// a panic message and its backtrace. Empty lines are dropped; no piece is empty.
#[must_use]
pub fn log_pieces(text: &str, max_bytes: usize) -> Vec<&str> {
    let max_bytes = max_bytes.max(4);
    let mut pieces = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while !rest.is_empty() {
            if rest.len() <= max_bytes {
                pieces.push(rest);
                break;
            }
            let mut cut = max_bytes;
            while !rest.is_char_boundary(cut) {
                cut -= 1;
            }
            let (piece, tail) = rest.split_at(cut);
            pieces.push(piece);
            rest = tail;
        }
    }
    pieces
}

#[cfg(test)]
#[path = "ios_logic_test.rs"]
mod ios_logic_test;
