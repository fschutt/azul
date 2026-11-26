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
/// The top bar's height (the macOS traffic lights' bar) and the bottom bar's.
pub const TOP_H: f32 = 28.0;
pub const BOTTOM_H: f32 = 30.0;
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
        Self { tree_open: true, tree_w: TREE_W_DEFAULT, right_open: false, right_w: RIGHT_W_DEFAULT }
    }
}

/// The settings keys the panels are remembered under.
pub const KEY_TREE_OPEN: &str = "tree-open";
pub const KEY_TREE_W: &str = "tree-width";
pub const KEY_RIGHT_OPEN: &str = "right-open";
pub const KEY_RIGHT_W: &str = "right-width";

fn finite_or(v: f32, default: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        default
    }
}

impl Panels {
    /// The panels as the settings remember them, each value held to its limits, and a word for each one that had to
    /// be held or could not be read (the value is then the default).
    #[must_use]
    pub fn remembered(get: &dyn Fn(&str) -> Option<String>) -> (Self, Vec<String>) {
        let mut panels = Self::default();
        let mut notes = Vec::new();
        for (key, into) in [(KEY_TREE_OPEN, &mut panels.tree_open), (KEY_RIGHT_OPEN, &mut panels.right_open)] {
            if let Some(v) = get(key) {
                match v.trim() {
                    "1" => *into = true,
                    "0" => *into = false,
                    other => notes.push(format!("{key}: \"{other}\" is neither 1 nor 0, the default is used")),
                }
            }
        }
        for (key, into, min, max) in [
            (KEY_TREE_W, &mut panels.tree_w, TREE_W_MIN, TREE_W_MAX),
            (KEY_RIGHT_W, &mut panels.right_w, RIGHT_W_MIN, RIGHT_W_MAX),
        ] {
            if let Some(v) = get(key) {
                match v.trim().parse::<f32>() {
                    Ok(n) if n.is_finite() => {
                        let held = n.clamp(min, max);
                        if (held - n).abs() > f32::EPSILON {
                            notes.push(format!("{key}: {n} is outside {min} to {max}, held to {held}"));
                        }
                        *into = held;
                    }
                    _ => notes.push(format!("{key}: \"{}\" is no width, the default is used", v.trim())),
                }
            }
        }
        (panels, notes)
    }

    /// What is stored: the keys and their values, to be read back by [`Self::remembered`].
    #[must_use]
    pub fn to_values(&self) -> Vec<(String, String)> {
        vec![
            (KEY_TREE_OPEN.to_string(), u8::from(self.tree_open).to_string()),
            (KEY_TREE_W.to_string(), format!("{}", self.tree_w.round())),
            (KEY_RIGHT_OPEN.to_string(), u8::from(self.right_open).to_string()),
            (KEY_RIGHT_W.to_string(), format!("{}", self.right_w.round())),
        ]
    }
}

/// Whether the tree starts open: what was remembered, and where nothing was, open where it pushes and closed where it
/// would cover the pages (the page is what one sees first in a small window).
#[must_use]
pub fn initial_tree_open(remembered: Option<bool>, window_w: f32) -> bool {
    remembered.unwrap_or_else(|| !is_overlay(window_w))
}

/// Whether a window of this width has no room to push: the tree covers the pages.
#[must_use]
pub fn is_overlay(window_w: f32) -> bool {
    window_w < OVERLAY_BELOW_W
}

/// The widest the tree may be in a window: where it pushes, what leaves the pages their minimum beside the strip and
/// the right bar, and the right panel's minimum when that is open too (the panel gives way first, then the tree);
/// where it covers, all but a margin.
fn tree_w_max(window_w: f32, right_reserved: f32) -> f32 {
    let window_w = finite_or(window_w, 0.0).max(0.0);
    if is_overlay(window_w) {
        (window_w - OVERLAY_MARGIN).clamp(TREE_W_MIN, TREE_W_MAX)
    } else {
        (window_w - PAGES_MIN_W - STRIP_W - RIGHT_BAR_W - right_reserved).clamp(TREE_W_MIN, TREE_W_MAX)
    }
}

/// A tree width held between its minimum and what the window can spare with no right panel; no number is the default.
#[must_use]
pub fn clamp_tree_w(wanted: f32, window_w: f32) -> f32 {
    clamp_tree_w_beside(wanted, window_w, 0.0)
}

/// [`clamp_tree_w`] with `right_reserved` px kept for an open right panel.
#[must_use]
pub fn clamp_tree_w_beside(wanted: f32, window_w: f32, right_reserved: f32) -> f32 {
    let max = tree_w_max(window_w, right_reserved);
    if wanted.is_nan() {
        return TREE_W_DEFAULT.min(max);
    }
    wanted.clamp(TREE_W_MIN, max)
}

/// The widest the right panel may be beside a tree that is `tree_w` wide where it pushes.
fn right_w_max(window_w: f32, tree_w: f32) -> f32 {
    let window_w = finite_or(window_w, 0.0).max(0.0);
    let pushed = if is_overlay(window_w) { 0.0 } else { finite_or(tree_w, 0.0).max(0.0) };
    (window_w - pushed - STRIP_W - RIGHT_BAR_W - PAGES_MIN_W).clamp(RIGHT_W_MIN, RIGHT_W_MAX)
}

/// A right panel width held between its minimum and what the pages leave it beside a tree `tree_w` wide; no number is
/// the default.
#[must_use]
pub fn clamp_right_w(wanted: f32, window_w: f32, tree_w: f32) -> f32 {
    let max = right_w_max(window_w, tree_w);
    if wanted.is_nan() {
        return RIGHT_W_DEFAULT.min(max);
    }
    wanted.clamp(RIGHT_W_MIN, max)
}

/// The tree's width while its edge is dragged: the width it had at the grab plus what the pointer has moved since.
#[must_use]
pub fn dragged_tree_w(start_w: f32, grab_x: f32, now_x: f32, window_w: f32) -> f32 {
    clamp_tree_w(start_w + (now_x - grab_x), window_w)
}

/// The right panel's width while its inner (left) edge is dragged: the pointer moving left makes it wider.
#[must_use]
pub fn dragged_right_w(start_w: f32, grab_x: f32, now_x: f32, window_w: f32, tree_w: f32) -> f32 {
    clamp_right_w(start_w + (grab_x - now_x), window_w, tree_w)
}

/// The panels that slide and are resized by their inner edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Tree,
    Right,
}

/// A drag of a panel's edge: the width the panel had at the grab and where the pointer was.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grip {
    pub panel: Panel,
    pub start_w: f32,
    pub grab_x: f32,
}

impl Grip {
    /// The panel's width with the pointer at `now_x` in a window of `window_w`; `tree_w` is the tree's width beside
    /// the right panel.
    #[must_use]
    pub fn width_at(&self, now_x: f32, window_w: f32, tree_w: f32) -> f32 {
        match self.panel {
            Panel::Tree => dragged_tree_w(self.start_w, self.grab_x, now_x, window_w),
            Panel::Right => dragged_right_w(self.start_w, self.grab_x, now_x, window_w, tree_w),
        }
    }
}

/// A slide's progress (0 to 1, in time) as the distance it has covered: fast at first, slow at its end.
#[must_use]
pub fn ease_out(t: f32) -> f32 {
    if t.is_nan() {
        return 0.0;
    }
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t)
}

/// How far each panel has slid: 0 closed, 1 open, in time (the width is its `ease_out`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slide {
    pub tree: f32,
    pub right: f32,
}

fn toward(progress: f32, open: bool, dt_ms: f32) -> f32 {
    let target = if open { 1.0 } else { 0.0 };
    if dt_ms.is_nan() || dt_ms <= 0.0 {
        return progress;
    }
    let by = dt_ms / TREE_SLIDE_MS;
    if open {
        (progress + by).min(target)
    } else {
        (progress - by).max(target)
    }
}

impl Slide {
    /// Where each panel is when nothing moves: at its end.
    #[must_use]
    pub fn settled(panels: &Panels) -> Self {
        Self { tree: f32::from(u8::from(panels.tree_open)), right: f32::from(u8::from(panels.right_open)) }
    }

    /// `dt_ms` later, each panel closer to its end; a step that is no time (zero, negative, no number) changes nothing.
    #[must_use]
    pub fn step(self, panels: &Panels, dt_ms: f32) -> Self {
        Self { tree: toward(self.tree, panels.tree_open, dt_ms), right: toward(self.right, panels.right_open, dt_ms) }
    }

    /// Whether a panel is still on its way.
    #[must_use]
    pub fn moving(self, panels: &Panels) -> bool {
        self != Self::settled(panels)
    }
}

/// How far the top bar keeps from the window's left edge: clear of the traffic lights and the tree's switch
/// (`reserved`), until the tree, which sits under them, is wider than that.
#[must_use]
pub fn top_bar_left_pad(tree_now_w: f32, reserved: f32) -> f32 {
    (reserved - tree_now_w).max(0.0)
}

/// The widths of the moment: what the DOM is built from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solved {
    /// The tree covers the pages (no room to push).
    pub overlay: bool,
    /// How wide the tree is now (it slides).
    pub tree_w: f32,
    /// How wide the tree is when it has slid out: what its rows are laid out at while it moves.
    pub tree_full_w: f32,
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
    let overlay = is_overlay(window_w);
    // The widths a panel settles at depend on which panels are open, never on how far one has slid: the other
    // panel does not breathe while one moves.
    let right_reserved = if panels.right_open { RIGHT_W_MIN } else { 0.0 };
    let tree_settled = clamp_tree_w_beside(panels.tree_w, window_w, right_reserved);
    let tree_w = tree_settled * ease_out(slide.tree);
    let right_settled = clamp_right_w(panels.right_w, window_w, if panels.tree_open { tree_settled } else { 0.0 });
    let right_w = right_settled * ease_out(slide.right);
    let pushed = if overlay { 0.0 } else { tree_w };
    let window = finite_or(window_w, 0.0).max(0.0);
    Solved {
        overlay,
        tree_w,
        tree_full_w: tree_settled,
        right_w,
        pages_w: (window - pushed - STRIP_W - RIGHT_BAR_W - right_w).max(0.0),
        top_pad_left: top_bar_left_pad(pushed, reserved_left),
    }
}

#[cfg(test)]
#[path = "layout_test.rs"]
mod layout_test;
