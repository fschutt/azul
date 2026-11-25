//! The window's edges as numbers: where the file tree, the page strip, the pages, the right whiteboard bar and its
//! panel sit; how wide the tree and the panel are while they slide; what a drag of an edge makes of a width; what
//! may be remembered of them. Pure, so the arithmetic is tested without a window (`layout_test.rs`) and the DOM
//! (`ui.rs`) only reads the answers.
//!
//! The tree is the first column of the window, as tall as the window, and PUSHES the rest of the layout to the right
//! (`tree_pushes_toward`); below `OVERLAY_BELOW_W` there is no room to push and it covers the pages instead.

/// Which side of the window the file tree is on, and so which way it pushes the rest: one place to turn it round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// The side the tree pushes the pages toward (the tree sits on the opposite side).
#[must_use]
pub const fn tree_pushes_toward() -> Side {
    Side::Right
}

/// A window narrower than this (logical px) has no room to push: the tree covers the pages instead.
pub const OVERLAY_BELOW_W: f32 = 700.0;
/// The tree's width when nothing was chosen, and its limits.
pub const TREE_W_DEFAULT: f32 = 260.0;
pub const TREE_W_MIN: f32 = 160.0;
pub const TREE_W_MAX: f32 = 640.0;
/// The right panel's width when nothing was chosen, and its limits.
pub const RIGHT_W_DEFAULT: f32 = 360.0;
pub const RIGHT_W_MIN: f32 = 220.0;
pub const RIGHT_W_MAX: f32 = 900.0;
/// The vertical page strip's width and the right bar's.
pub const STRIP_W: f32 = 44.0;
pub const RIGHT_BAR_W: f32 = 22.0;
/// What the pages keep while a panel can still give way.
pub const PAGES_MIN_W: f32 = 360.0;
/// The grip on a panel's inner edge that is dragged to resize it.
pub const GRIP_W: f32 = 6.0;
/// How long a panel takes to slide in or out, ms.
pub const TREE_SLIDE_MS: f32 = 180.0;
/// What a covering tree leaves uncovered at the other edge of a narrow window.
const OVERLAY_MARGIN: f32 = 40.0;

/// What is open and how wide: the part of the layout a person chooses, and the part that is remembered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panels {
    pub tree_open: bool,
    pub tree_w: f32,
    pub right_open: bool,
    pub right_w: f32,
}

impl Default for Panels {
    fn default() -> Self {
        unimplemented!("the layout maths are not written yet")
    }
}

/// The settings keys the panels are remembered under.
pub const KEY_TREE_OPEN: &str = "tree-open";
pub const KEY_TREE_W: &str = "tree-width";
pub const KEY_RIGHT_OPEN: &str = "right-open";
pub const KEY_RIGHT_W: &str = "right-width";

fn finite_or(v: f32, default: f32) -> f32 {
    unimplemented!("the layout maths are not written yet")
}

impl Panels {
    /// The panels as the settings remember them, each value held to its limits, and a word for each one that had to
    /// be held or could not be read (the value is then the default).
    #[must_use]
    pub fn remembered(get: &dyn Fn(&str) -> Option<String>) -> (Self, Vec<String>) {
        unimplemented!("the layout maths are not written yet")
    }

    /// What is stored: the keys and their values, to be read back by [`Self::remembered`].
    #[must_use]
    pub fn to_values(&self) -> Vec<(String, String)> {
        unimplemented!("the layout maths are not written yet")
    }
}

/// Whether the tree starts open: what was remembered, and where nothing was, open where it pushes and closed where it
/// would cover the pages (the page is what one sees first in a small window).
#[must_use]
pub fn initial_tree_open(_remembered: Option<bool>, _window_w: f32) -> bool {
    unimplemented!("the layout maths are not written yet")
}

/// Whether a window of this width has no room to push: the tree covers the pages.
#[must_use]
pub fn is_overlay(window_w: f32) -> bool {
    unimplemented!("the layout maths are not written yet")
}

/// The widest the tree may be in a window: where it pushes, what leaves the pages their minimum beside the strip and
/// the right bar, and the right panel's minimum when that is open too (the panel gives way first, then the tree);
/// where it covers, all but a margin.
fn tree_w_max(window_w: f32, right_reserved: f32) -> f32 {
    unimplemented!("the layout maths are not written yet")
}

/// A tree width held between its minimum and what the window can spare with no right panel; no number is the default.
#[must_use]
pub fn clamp_tree_w(wanted: f32, window_w: f32) -> f32 {
    unimplemented!("the layout maths are not written yet")
}

/// [`clamp_tree_w`] with `right_reserved` px kept for an open right panel.
#[must_use]
pub fn clamp_tree_w_beside(wanted: f32, window_w: f32, right_reserved: f32) -> f32 {
    unimplemented!("the layout maths are not written yet")
}

/// The widest the right panel may be beside a tree that is `tree_w` wide where it pushes.
fn right_w_max(window_w: f32, tree_w: f32) -> f32 {
    unimplemented!("the layout maths are not written yet")
}

/// A right panel width held between its minimum and what the pages leave it beside a tree `tree_w` wide; no number is
/// the default.
#[must_use]
pub fn clamp_right_w(wanted: f32, window_w: f32, tree_w: f32) -> f32 {
    unimplemented!("the layout maths are not written yet")
}

/// The tree's width while its edge is dragged: the width it had at the grab plus what the pointer has moved since.
#[must_use]
pub fn dragged_tree_w(start_w: f32, grab_x: f32, now_x: f32, window_w: f32) -> f32 {
    unimplemented!("the layout maths are not written yet")
}

/// The right panel's width while its inner (left) edge is dragged: the pointer moving left makes it wider.
#[must_use]
pub fn dragged_right_w(start_w: f32, grab_x: f32, now_x: f32, window_w: f32, tree_w: f32) -> f32 {
    unimplemented!("the layout maths are not written yet")
}

/// A slide's progress (0 to 1, in time) as the distance it has covered: fast at first, slow at its end.
#[must_use]
pub fn ease_out(t: f32) -> f32 {
    unimplemented!("the layout maths are not written yet")
}

/// How far each panel has slid: 0 closed, 1 open, in time (the width is its `ease_out`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slide {
    pub tree: f32,
    pub right: f32,
}

fn toward(progress: f32, open: bool, dt_ms: f32) -> f32 {
    unimplemented!("the layout maths are not written yet")
}

impl Slide {
    /// Where each panel is when nothing moves: at its end.
    #[must_use]
    pub fn settled(panels: &Panels) -> Self {
        unimplemented!("the layout maths are not written yet")
    }

    /// `dt_ms` later, each panel closer to its end; a step that is no time (zero, negative, no number) changes nothing.
    #[must_use]
    pub fn step(self, panels: &Panels, dt_ms: f32) -> Self {
        unimplemented!("the layout maths are not written yet")
    }

    /// Whether a panel is still on its way.
    #[must_use]
    pub fn moving(self, panels: &Panels) -> bool {
        unimplemented!("the layout maths are not written yet")
    }
}

/// How far the top bar keeps from the window's left edge: clear of the traffic lights and the tree's switch
/// (`reserved`), until the tree, which sits under them, is wider than that.
#[must_use]
pub fn top_bar_left_pad(tree_now_w: f32, reserved: f32) -> f32 {
    unimplemented!("the layout maths are not written yet")
}

/// The widths of the moment: what the DOM is built from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solved {
    /// The tree covers the pages (no room to push).
    pub overlay: bool,
    /// How wide the tree is now (it slides).
    pub tree_w: f32,
    /// How wide the right panel is now (it slides).
    pub right_w: f32,
    /// What the pages have.
    pub pages_w: f32,
    /// The top bar's left padding.
    pub top_pad_left: f32,
}

/// The widths of a window of `window_w` with these panels this far slid. `reserved_left` is what the traffic lights
/// and the tree's switch take of the top bar.
#[must_use]
pub fn solve(window_w: f32, panels: &Panels, slide: &Slide, reserved_left: f32) -> Solved {
    unimplemented!("the layout maths are not written yet")
}

#[cfg(test)]
#[path = "layout_test.rs"]
mod layout_test;
