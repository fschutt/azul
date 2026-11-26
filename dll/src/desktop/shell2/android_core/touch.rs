//! What a phase of a touch gesture does to the mouse the engine mirrors it with.
//!
//! The shell turns the primary finger into a mouse (position, left button) and runs the engine's
//! ordinary event pass over it. Most phases map one to one; `ACTION_CANCEL` is the exception.
//! The `MotionEvent` documentation: "The current gesture has been aborted. You will not receive any
//! more points in it. You should treat this as an up event, but not perform any action that you
//! normally would." The system sends it when it takes the gesture over (the notification shade is
//! pulled down, the back gesture starts at the edge, a parent view intercepts, the activity loses
//! the focus). The shell released the left button over the node under the finger, which is a CLICK:
//! a finger that slid off a Delete button towards the shade deleted whatever the button deleted.

/// A phase of a `MotionEvent`, as the shell reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The first finger landed (`ACTION_DOWN`).
    Down,
    /// A further finger landed (`ACTION_POINTER_DOWN`).
    PointerDown,
    /// A finger moved (`ACTION_MOVE`).
    Move,
    /// A mouse or a stylus hovers (`ACTION_HOVER_MOVE`).
    HoverMove,
    /// A finger other than the last lifted (`ACTION_POINTER_UP`).
    PointerUp,
    /// The last finger lifted (`ACTION_UP`).
    Up,
    /// The system aborted the gesture (`ACTION_CANCEL`).
    Cancel,
    /// Anything else (scroll, button press, ...).
    Other,
}

/// Where the engine's hit test is taken for the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTest {
    /// At the mouse: the node under the finger gets the event.
    AtCursor,
    /// Nowhere: the pointer left, no node is under it and the hover chain is cleared (what a mouse
    /// leaving the window does on every desktop shell).
    Nowhere,
}

/// What a phase does to the mirrored mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEffect {
    /// The left button: `Some(down)` to set it, `None` to leave it.
    pub left_down: Option<bool>,
    /// The pointer leaves the window (the cursor is out of the window at its last position).
    pub pointer_leaves: bool,
    /// Where the hit test is taken.
    pub hit_test: HitTest,
    /// The event may start or continue a scroll (a pan).
    pub may_scroll: bool,
}

/// The effect of `phase`. `primary_lifting` is true when the finger that lifts is the primary one (the
/// one the mouse mirrors): the primary lifting while others stay down is a mouse release.
#[must_use]
pub const fn mouse_effect(phase: Phase, primary_lifting: bool) -> MouseEffect {
    match phase {
        Phase::Down => MouseEffect {
            left_down: Some(true),
            pointer_leaves: false,
            hit_test: HitTest::AtCursor,
            may_scroll: true,
        },
        Phase::Up => MouseEffect {
            left_down: Some(false),
            pointer_leaves: false,
            hit_test: HitTest::AtCursor,
            may_scroll: true,
        },
        // The gesture was aborted: the button comes up, but over NOTHING, so no node is clicked and
        // the hover chain is cleared. The scroll physics still get their end phase (a pan must end).
        Phase::Cancel => MouseEffect {
            left_down: Some(false),
            pointer_leaves: true,
            hit_test: HitTest::Nowhere,
            may_scroll: true,
        },
        Phase::PointerUp => MouseEffect {
            left_down: if primary_lifting { Some(false) } else { None },
            pointer_leaves: false,
            hit_test: HitTest::AtCursor,
            may_scroll: true,
        },
        Phase::PointerDown | Phase::Move | Phase::HoverMove | Phase::Other => MouseEffect {
            left_down: None,
            pointer_leaves: false,
            hit_test: HitTest::AtCursor,
            may_scroll: true,
        },
    }
}

#[cfg(test)]
#[path = "touch_test.rs"]
mod tests;
