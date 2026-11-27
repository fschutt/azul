//! The decisions of the iOS shell that need no UIKit, so a host test binary can run them
//! (`shell2/ios` itself only exists for an iOS target).
//!
//! UIKit hands the shell numbers in POINTS: the safe-area insets of the render view, the
//! keyboard's frame, the view's bounds. Everything here takes those numbers (never an object) and
//! answers with the engine's own types.

use azul_core::events::ProcessEventResult;
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

/// Whether a `UIKeyboardWillChangeFrameNotification` is about THIS app's keyboard.
///
/// `is_local` is the notification's `UIKeyboardIsLocalUserInfoKey`: "With Multitasking in iPadOS,
/// the system notifies all visible apps when the keyboard appears and disappears. The value is
/// true for the app that caused the keyboard to appear and false for the other apps." A keyboard
/// another app raised must not inset this app's layout. A notification without the key (older
/// systems) counts as ours.
#[must_use]
pub const fn keyboard_event_is_ours(is_local: Option<bool>) -> bool {
    !matches!(is_local, Some(false))
}

/// A text's length in UTF-16 code units (`NSString.length`).
#[must_use]
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The UTF-16 offset of the character boundary a `utf16_offset` leads to: itself when it already
/// is one, else (it points INSIDE a surrogate pair) the end of that character when `forward`, its
/// start when not. Past the end is the end.
#[must_use]
pub fn snap_utf16_offset(text: &str, utf16_offset: usize, forward: bool) -> usize {
    let mut units = 0usize;
    for ch in text.chars() {
        let next = units + ch.len_utf16();
        if utf16_offset < next {
            return if utf16_offset == units || !forward { units } else { next };
        }
        units = next;
    }
    units
}

/// `-positionFromPosition:offset:`, in UTF-16 units.
///
/// UIKit measures every offset of `UITextInput` in UTF-16 code units (`offset(from:to:)`:
/// "the number of UTF-16 characters between one text position and another"), the engine in
/// BYTES; the shell stored bytes, so the first non-ASCII character answered every question
/// wrongly (an umlaut is 2 bytes for 1 unit, a kana 3, an emoji 4 for 2) and a step of one
/// character over it landed inside it and snapped back to where it started: the tokenizer, the
/// loupe and the selection handles could never advance.
///
/// `None` ("nil") when the result lies before the start or past the end of the text: "Return nil
/// if the computed text position is less than 0 or greater than the length of the backing
/// string" - UIKit relies on it to find where the document ends. A result inside a surrogate pair
/// moves on to the next character boundary in the direction of travel, so every step makes
/// progress and visits every boundary once.
#[must_use]
pub fn position_from_offset(text: &str, base: usize, offset: i64) -> Option<usize> {
    let base = i64::try_from(base).ok()?;
    let target = usize::try_from(base.checked_add(offset)?).ok()?;
    if target > utf16_len(text) {
        return None;
    }
    Some(snap_utf16_offset(text, target, offset >= 0))
}

/// The one character before `base` (`toward_start`) or after it in `text`, as a `(start, end)`
/// byte range, for `characterRangeByExtendingPosition:inDirection:`.
///
/// `base` arrives from UIKit, computed from positions this shell handed out earlier: after the
/// text changed it can be past the end or inside a multi-byte character, and slicing there is a
/// panic - inside an `extern "C"` callback, an abort. It is clamped to the text and snapped down
/// to a character boundary first. At either end of the text the range is empty (`start == end`),
/// never inverted.
#[must_use]
pub fn one_character_around(text: &str, base: usize, toward_start: bool) -> (usize, usize) {
    let mut base = base.min(text.len());
    while base > 0 && !text.is_char_boundary(base) {
        base -= 1;
    }
    if toward_start {
        let start = text[..base]
            .char_indices()
            .next_back()
            .map_or(base, |(index, _)| index);
        (start, base)
    } else {
        let end = text[base..]
            .chars()
            .next()
            .map_or(base, |c| base + c.len_utf8());
        (base, end)
    }
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

/// How to ask `UTType` for the type a file-picker filter descriptor names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UtiQuery {
    /// `+[UTType typeWithIdentifier:]`, with this uniform type identifier.
    Identifier(&'static str),
    /// `+[UTType typeWithFilenameExtension:]`, with this extension (lower case, no dot).
    Extension(String),
    /// `+[UTType typeWithMIMEType:]`, with this MIME type.
    MimeType(String),
}

/// `public.data`: what a picker with no usable filter offers (`UTTypeData`).
pub const UTI_DATA: &str = "public.data";
/// `public.folder`: what the directory picker opens (`UTTypeFolder`).
pub const UTI_FOLDER: &str = "public.folder";

/// The query a descriptor (`"*.png"`, `".png"`, `"png"`, `"image/png"`) stands for, or `None` for
/// one that names nothing.
///
/// `UTType`'s well-known types (`png`, `pdf`, `data`, `folder`, ...) are Objective-C GLOBAL
/// CONSTANTS (`UTTypePNG`, `UTTypeData`; `UTType.png` is only Swift's spelling), not class
/// methods: `+[UTType png]` and `+[UTType data]` do not exist (the runtime answers "unrecognized
/// selector", an Objective-C exception that aborts the process through Rust frames), and the
/// picker called them with `performSelector:` through a selector built with a trailing NUL, which
/// panics before the message is even sent. Every use of the picker died; the identifiers below
/// are the documented values of those constants, asked for with `typeWithIdentifier:`.
#[must_use]
pub fn uti_query_for_descriptor(descriptor: &str) -> Option<UtiQuery> {
    let trimmed = descriptor
        .trim()
        .trim_start_matches('*')
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('/') {
        return Some(UtiQuery::MimeType(trimmed));
    }
    let identifier = match trimmed.as_str() {
        "png" => "public.png",
        "jpg" | "jpeg" => "public.jpeg",
        "pdf" => "com.adobe.pdf",
        "gif" => "com.compuserve.gif",
        "txt" | "text" => "public.plain-text",
        "json" => "public.json",
        "html" | "htm" => "public.html",
        "xml" => "public.xml",
        "zip" => "public.zip-archive",
        "mov" => "public.movie",
        "mp4" => "public.mpeg-4",
        "mp3" => "public.mp3",
        "wav" => "com.microsoft.waveform-audio",
        "svg" => "public.svg-image",
        _ => return Some(UtiQuery::Extension(trimmed)),
    };
    Some(UtiQuery::Identifier(identifier))
}

/// Which finger the emulated mouse follows after a `touchesBegan:`.
///
/// The W3C pointer rule the shell documents (U2-a-iii): the FIRST finger of a group is primary, no
/// later finger inherits that, and the next primary is born only once EVERY finger has lifted.
/// `current` is the primary now (if any), `fingers_before` how many fingers were already down
/// before this event, `began` the identities that just landed.
///
/// The shell made the first landed finger primary whenever there was no primary - which is also
/// the case after the primary lifted while another finger stayed down: a third finger landing then
/// took the mouse and pressed the button (a phantom drag in the middle of a two-finger gesture).
#[must_use]
pub fn primary_after_began(current: Option<u64>, fingers_before: usize, began: &[u64]) -> Option<u64> {
    match current {
        Some(_) => current,
        None if fingers_before == 0 => began.first().copied(),
        None => None,
    }
}

/// What the frame path owes after a pass of `process_window_events`, at the weight the pass
/// classified it (`ProcessEventResult`'s own ladder, lightest first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameStep {
    /// Nothing changed.
    Nothing,
    /// The pixels changed (a scroll offset, a hover restyle, a new display list), the layout did
    /// not: re-raster the frame. No app callback, no layout pass.
    Repaint,
    /// A restyle or a runtime edit changed what layout sees: re-run layout on the EXISTING
    /// `StyledDom`, then repaint. NOT a DOM rebuild - the app's `layout()` must not run, or it
    /// rebuilds the DOM from a model that has not caught up with the edit (a committed IME string
    /// was laid out again from the app's unchanged state and vanished).
    IncrementalRelayout,
    /// A callback asked for a new DOM: run the app's `layout()` again.
    Regenerate,
}

/// The step a pass's result calls for. The same ladder the macOS, Windows, X11, Wayland and
/// Android shells follow: heavy reasons regenerate, light ones only repaint. The iOS shell turned
/// EVERY result but `DoNothing` into a DOM rebuild, so a scrolling finger re-ran the app's
/// `layout()` at the touch rate (Android measured 121-443 ms per escalated scroll frame against
/// ~6 ms honoured), and an IME commit was overwritten by the app's unchanged model.
#[must_use]
pub const fn frame_step(result: ProcessEventResult) -> FrameStep {
    match result {
        ProcessEventResult::DoNothing => FrameStep::Nothing,
        ProcessEventResult::ShouldReRenderCurrentWindow
        | ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
        | ProcessEventResult::UpdateHitTesterAndProcessAgain => FrameStep::Repaint,
        ProcessEventResult::ShouldIncrementalRelayout => FrameStep::IncrementalRelayout,
        ProcessEventResult::ShouldRegenerateDomCurrentWindow
        | ProcessEventResult::ShouldRegenerateDomAllWindows => FrameStep::Regenerate,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// What a touch phase does to the mouse the engine mirrors it with
// ---------------------------------------------------------------------------------------------------------------

use crate::desktop::shell2::android_core::slop::{Lift, Move};

/// The distance a finger may wander before it is a pan, in points. The engine's logical pixel IS the point on iOS
/// (the frame factor `dpi / 96` is the screen scale), so positions are compared with it as they are. UIKit's own pan
/// recognizer starts after about this much.
pub const TOUCH_SLOP_POINTS: f32 = 10.0;

/// The phase of a `UITouch` event, as `handle_touch` reads it (`touchesBegan:` ... `touchesCancelled:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    /// `touchesBegan:withEvent:`
    Began,
    /// `touchesMoved:withEvent:`
    Moved,
    /// `touchesEnded:withEvent:`
    Ended,
    /// `touchesCancelled:withEvent:`: the system took the gesture over (a system alert, the notification
    /// centre, an incoming call). UIResponder: "clean up any state"; it is no tap.
    Cancelled,
}

/// Where the mirrored mouse's cursor goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorPlan {
    /// To the primary finger's position in this event.
    Follow,
    /// Where it is: a finger that is not the primary moved, or the primary is still inside the touch slop.
    Stay,
    /// To this point: where a tap LANDED, whatever the jitter of the finger before it lifted.
    At((f32, f32)),
    /// Out of the window at its last position: the gesture ended without a click.
    Leave,
}

/// Where the engine's hit test is taken for the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitPlan {
    /// At the cursor as this plan leaves it.
    AtCursor,
    /// Nowhere: the pointer left, no node is under it and the hover chain is cleared (what a mouse leaving the
    /// window does on every desktop shell).
    Nowhere,
    /// The previous hit test stands (the pointer did not move).
    Keep,
}

/// What a phase does to the mirrored mouse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MousePlan {
    /// The left button: `Some(down)` to set it, `None` to leave it.
    pub left_down: Option<bool>,
    /// The cursor.
    pub cursor: CursorPlan,
    /// The hit test.
    pub hit: HitPlan,
}

/// The mouse's part of one touch phase. `primary_in_set`: this event carries the primary finger (the one the
/// mouse mirrors); a second finger's phases are nothing to the mouse. `slop_move` is the answer of
/// [`TapSlop::moved`](crate::desktop::shell2::android_core::slop::TapSlop::moved) for a move of the primary, `lift`
/// that of [`TapSlop::up`](crate::desktop::shell2::android_core::slop::TapSlop::up) for its lift.
///
/// The rule is the one every touch toolkit has and the Android shell follows (`android_core::slop`,
/// `android_core::touch`): a finger that went down is a tap candidate until it has moved more than the slop from where
/// it landed, a lift inside the slop is a click at the landing point, the lift of a pan clicks nothing, and a
/// cancelled gesture releases the button over nothing. This shell moved the pointer and scrolled by any move above a
/// hundredth of a point (a real finger jitters by a few points: a plain tap scrolled the page, and the click was
/// lost whenever the node under the finger changed) and treated a cancelled touch as an ended one (a click).
#[must_use]
pub fn mouse_plan(
    phase: TouchPhase,
    primary_in_set: bool,
    slop_move: Option<Move>,
    lift: Option<Lift>,
) -> MousePlan {
    const NOTHING: MousePlan = MousePlan {
        left_down: None,
        cursor: CursorPlan::Stay,
        hit: HitPlan::Keep,
    };
    if !primary_in_set {
        return NOTHING;
    }
    match phase {
        TouchPhase::Began => MousePlan {
            left_down: Some(true),
            cursor: CursorPlan::Follow,
            hit: HitPlan::AtCursor,
        },
        TouchPhase::Moved => match slop_move {
            Some(Move::Hold) => NOTHING,
            _ => MousePlan {
                left_down: None,
                cursor: CursorPlan::Follow,
                hit: HitPlan::AtCursor,
            },
        },
        TouchPhase::Ended => match lift {
            Some(Lift::Pan) => MousePlan {
                left_down: Some(false),
                cursor: CursorPlan::Leave,
                hit: HitPlan::Nowhere,
            },
            Some(Lift::Click { at: Some(landed) }) => MousePlan {
                left_down: Some(false),
                cursor: CursorPlan::At(landed),
                hit: HitPlan::AtCursor,
            },
            Some(Lift::Click { at: None }) | None => MousePlan {
                left_down: Some(false),
                cursor: CursorPlan::Follow,
                hit: HitPlan::AtCursor,
            },
        },
        TouchPhase::Cancelled => MousePlan {
            left_down: Some(false),
            cursor: CursorPlan::Leave,
            hit: HitPlan::Nowhere,
        },
    }
}

#[cfg(test)]
#[path = "ios_logic_test.rs"]
mod ios_logic_test;
