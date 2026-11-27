//! What the first frame of a window owes layout.
//!
//! A window is born owing one layout (`RegenerationState::pending_initial`): the constructor (or
//! the first turn of the loop) runs it, and it retires the requests that stood when it began. The
//! first FRAME then builds its display list from that layout. Asking for a second layout there
//! rebuilds the DOM again for nothing: the user's `layout()` callback, the styled DOM and the
//! equivalence check run, find the DOM unchanged, and the display list is built from the first
//! layout anyway. The startup timeline (`AZ_STARTUP=1`, 2026-11-26) counted exactly that on macOS:
//! two layouts before the first present for an app that had changed nothing.

/// Whether the first frame of a window must run a layout of its own: only when no layout has run
/// (the constructor's failed, or the window was made without one). A window whose display list is
/// already initialised is past its first frame; its layouts come from the requests that raise them
/// (a resize, a callback's refresh), never from the frame itself.
#[must_use]
pub const fn first_frame_owes_layout(display_list_initialized: bool, layout_exists: bool) -> bool {
    !display_list_initialized && !layout_exists
}

#[cfg(test)]
#[path = "first_frame_test.rs"]
mod first_frame_test;
