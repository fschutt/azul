//! The touch hit-slop: where a finger that lands NEXT TO a small control still presses it.
//!
//! Material asks a touch target of 48 dp and Apple one of 44 pt, whatever the control looks like:
//! a switch is a 36 x 20 track, a check box a 14 px square, a slider a 16 px bar, an icon button of
//! an app's own what its author drew. The standard answer (Android's `TouchDelegate`, the 48 dp
//! target round a 32 dp `Switch`, iOS's 44 pt hit area on a 31 pt `UISwitch`) is to keep the
//! control's look and widen where it listens. This module is the rule, with no tree in it: the
//! hit tester (`headless::CpuHitTester`) hands it the interactive boxes of a DOM and a touch point,
//! and it says which box the touch is for.
//!
//! - The **effective rect** of an interactive box is its visual rect grown to the floor on each
//!   axis where it is smaller, centred; a box at or above the floor is unchanged
//!   ([`effective_rect`]).
//! - A touch inside the REAL visual rect of ANY interactive box belongs to the normal hit test:
//!   the visual always beats another box's slop ([`pick`] answers `None`).
//! - Only a touch outside every visual rect uses the slop: the box whose effective rect holds the
//!   point and whose centre is nearest wins, a tie goes to the box painted in front.
//! - The slop never goes where the box could not be hit anyway: a clip (a scroller's viewport, an
//!   `overflow: hidden` ancestor) cuts the effective rect as it cuts the visual one.
//!
//! A mouse never uses it (a pointer is as precise as a pixel), and a desktop window has no floor
//! ([`touch_floor`]).

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

/// The least a touch target is on Android, in logical px (Material: 48 dp).
pub const ANDROID_TOUCH_TARGET_PX: f32 = 48.0;
/// The least a touch target is on iOS, in logical px (Apple: 44 pt).
pub const IOS_TOUCH_TARGET_PX: f32 = 44.0;

/// The floor a touch hit-slop grows boxes to on the platform `os` (`"android"`, `"ios"`, as
/// `std::env::consts::OS` and the system style name them); `None` where a finger has no slop: a
/// desktop.
#[must_use]
pub fn touch_floor(os: &str) -> Option<f32> {
    match os {
        "android" => Some(ANDROID_TOUCH_TARGET_PX),
        "ios" => Some(IOS_TOUCH_TARGET_PX),
        _ => None,
    }
}

/// An interactive box the touch can be for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlopCandidate<Id: Copy> {
    /// What the caller calls the box (a node, an index).
    pub id: Id,
    /// Where the box is painted, in the touch's space.
    pub visual: LogicalRect,
    /// What cuts the box (the intersection of its clipping ancestors), when something does.
    pub clip: Option<LogicalRect>,
    /// Its place in paint order: the larger, the further in front.
    pub order: usize,
}

/// `rect` grown to `floor` on each axis where it is smaller, the same distance on both sides. A
/// box at or above the floor, and a rect that is no finite size, come back as they are.
#[must_use]
pub fn effective_rect(rect: LogicalRect, floor: f32) -> LogicalRect {
    let (x, y) = (rect.origin.x, rect.origin.y);
    let (w, h) = (rect.size.width, rect.size.height);
    if ![x, y, w, h, floor].iter().all(|v| v.is_finite()) || w < 0.0 || h < 0.0 {
        return rect;
    }
    let grown = |origin: f32, length: f32| -> (f32, f32) {
        if length >= floor {
            (origin, length)
        } else {
            (origin - (floor - length) / 2.0, floor)
        }
    };
    let (gx, gw) = grown(x, w);
    let (gy, gh) = grown(y, h);
    LogicalRect::new(LogicalPosition::new(gx, gy), LogicalSize::new(gw, gh))
}

/// The part of `rect` inside `clip` (`None`: nothing is left).
fn clipped(rect: LogicalRect, clip: Option<LogicalRect>) -> Option<LogicalRect> {
    let Some(clip) = clip else { return Some(rect) };
    let left = rect.min_x().max(clip.min_x());
    let top = rect.min_y().max(clip.min_y());
    let right = rect.max_x().min(clip.max_x());
    let bottom = rect.max_y().min(clip.max_y());
    if right < left || bottom < top {
        return None;
    }
    Some(LogicalRect::new(
        LogicalPosition::new(left, top),
        LogicalSize::new(right - left, bottom - top),
    ))
}

/// Whether `point` lies in `rect` (the edges count: a finger on the border is on the box).
fn holds(rect: LogicalRect, point: LogicalPosition) -> bool {
    point.x >= rect.min_x()
        && point.x <= rect.max_x()
        && point.y >= rect.min_y()
        && point.y <= rect.max_y()
}

/// The centre of `rect`.
fn centre(rect: LogicalRect) -> (f32, f32) {
    (
        rect.origin.x + rect.size.width / 2.0,
        rect.origin.y + rect.size.height / 2.0,
    )
}

/// Which box a touch at `point` is for, when the normal hit test is not enough: `None` when the
/// point is inside the visual rect of ANY candidate (the normal hit test answers, the visual
/// beats every slop) or inside no candidate's effective rect; else the candidate whose effective
/// rect holds the point and whose centre is nearest, a tie to the one painted in front.
#[must_use]
pub fn pick<Id: Copy>(
    point: LogicalPosition,
    candidates: &[SlopCandidate<Id>],
    floor: f32,
) -> Option<Id> {
    if !point.x.is_finite() || !point.y.is_finite() || !floor.is_finite() || floor <= 0.0 {
        return None;
    }
    // A touch on any box's own pixels is the normal hit test's.
    if candidates
        .iter()
        .any(|c| clipped(c.visual, c.clip).is_some_and(|visual| holds(visual, point)))
    {
        return None;
    }
    let mut best: Option<(f32, usize, Id)> = None;
    for c in candidates {
        let Some(effective) = clipped(effective_rect(c.visual, floor), c.clip) else {
            continue;
        };
        if !holds(effective, point) {
            continue;
        }
        // The distance to the centre of the box as it LOOKS: the nearer control is the one meant.
        let (cx, cy) = centre(c.visual);
        let distance = (point.x - cx).hypot(point.y - cy);
        let better = match best {
            None => true,
            Some((d, order, _)) => distance < d || (distance == d && c.order > order),
        };
        if better {
            best = Some((distance, c.order, c.id));
        }
    }
    best.map(|(_, _, id)| id)
}

#[cfg(test)]
#[path = "hit_slop_test.rs"]
mod hit_slop_test;
