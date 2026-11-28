//! Detents: the pure part of a panel that rests at a few heights - collapsed, peek, half, full - and is dragged between them.
//! No DOM, no callbacks: numbers in, numbers out, so every widget that has such a pane (the [`BottomSheet`], an overlay over a map;
//! the Preview's bottom pane, a split under a list) decides the same way, and one test pins what they decide.
//!
//! ```text
//!   container ┌──────────────┐ ─ 0
//!             │              │
//!   Full      ├──────────────┤ ─ 15 % of the container from the top (full = 85 %)
//!   Half      ├──────────────┤ ─ 50 %
//!   Peek      ├──────────────┤ ─ 130 px
//!   Collapsed ├──────────────┤ ─ 30 px: the grip and one line
//!             └──────────────┘
//! ```
//!
//! The numbers are the wireframe's (row 11): collapsed 30 px, peek 130 px, half 50 %, full 85 %; an app names its own
//! (`peek`, `collapsed`, `top_margin`). They are always held in order, whatever they are: a number that is not a positive finite one
//! is its default, never an error.
//!
//! What is here:
//!
//! * [`SheetDetent`] and the order of the four;
//! * [`Heights`]: the px of each detent for a container, sanitised;
//! * [`Heights::nearest`], [`Heights::settle`] (where a drag that ended at a height, moving at a speed, comes to rest: the nearest
//!   detent, after a fling has carried it on) and [`Heights::settle_free`] (the same for a pane that stays where it was left
//!   between the detents and snaps only within a few px of one);
//! * [`Heights::dragged`]: the height the pointer makes of the pane;
//! * [`detent_after_key`] and [`step_round`]: what the keys on a grip and a tap on it do.
//!
//! Key types: [`SheetDetent`], [`Heights`].

use azul_core::window::VirtualKeyCode;

/// The peek detent's height when the app names none, px: a name, a line and the keys under it.
pub const PEEK_HEIGHT_DEFAULT: f32 = 130.0;
/// The collapsed detent's height when the app names none, px: the grip and one line.
pub const COLLAPSED_HEIGHT_DEFAULT: f32 = 30.0;
/// The half detent's share of the container.
pub const HALF_SHARE: f32 = 0.5;
/// The full detent's share of the container when the app names no top margin.
pub const FULL_SHARE: f32 = 0.85;
/// What a container is taken to be when the app names none (or a number that is not one), px.
pub const CONTAINER_HEIGHT_DEFAULT: f32 = 800.0;
/// A last step of this many px (per event, about a frame) or more is a fling.
pub const FLING_STEP: f32 = 6.0;
/// How many events of that step a fling carries on: where it would end, px = step x this.
pub const FLING_EVENTS: f32 = 12.0;

/// Where a pane rests.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SheetDetent {
    /// Only the grip and one line (a tab strip, a title) show.
    Collapsed,
    /// A summary: a place's name and its actions.
    #[default]
    Peek,
    /// Half of the container.
    Half,
    /// 85 % of the container, or the container less the top margin the app names.
    Full,
}

impl SheetDetent {
    /// The detent above this one (the highest stays).
    #[must_use]
    pub const fn higher(self) -> Self {
        match self {
            Self::Collapsed => Self::Peek,
            Self::Peek => Self::Half,
            Self::Half | Self::Full => Self::Full,
        }
    }

    /// The detent below this one (the lowest stays).
    #[must_use]
    pub const fn lower(self) -> Self {
        match self {
            Self::Collapsed | Self::Peek => Self::Collapsed,
            Self::Half => Self::Peek,
            Self::Full => Self::Half,
        }
    }
}

/// A number that must be a finite, positive length, else `default`.
#[must_use]
pub fn length(value: f32, default: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        default
    }
}

/// The four heights of a pane, px, in order: collapsed <= peek <= half <= full <= the container.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Heights {
    /// The collapsed detent.
    pub collapsed: f32,
    /// The peek detent.
    pub peek: f32,
    /// The half detent.
    pub half: f32,
    /// The full detent.
    pub full: f32,
}

impl Heights {
    /// The heights for a container `container` tall: a number that is not a positive finite one is its default (a `top_margin`
    /// that names none leaves 15 % of the container above the full detent), and the four are held in order (a container shorter
    /// than the peek makes every detent the container).
    #[must_use]
    pub fn new(container: f32, collapsed: f32, peek: f32, top_margin: f32) -> Self {
        let container = length(container, CONTAINER_HEIGHT_DEFAULT);
        let peek = length(peek, PEEK_HEIGHT_DEFAULT).min(container);
        let collapsed = length(collapsed, COLLAPSED_HEIGHT_DEFAULT).min(peek);
        let full_wanted = if top_margin.is_finite() && top_margin > 0.0 {
            container - top_margin
        } else {
            container * FULL_SHARE
        };
        let half = (container * HALF_SHARE).max(peek);
        let full = full_wanted.max(half);
        Self {
            collapsed,
            peek,
            half,
            full,
        }
    }

    /// The height of `detent`.
    #[must_use]
    pub const fn of(&self, detent: SheetDetent) -> f32 {
        match detent {
            SheetDetent::Collapsed => self.collapsed,
            SheetDetent::Peek => self.peek,
            SheetDetent::Half => self.half,
            SheetDetent::Full => self.full,
        }
    }

    /// The detent, between `lowest` and `highest`, whose height is nearest to `height` (the higher one when two are as near).
    #[must_use]
    pub fn nearest(&self, height: f32, lowest: SheetDetent, highest: SheetDetent) -> SheetDetent {
        let (lowest, highest) = (lowest.min(highest), lowest.max(highest));
        let mut best = highest;
        let mut at = highest;
        let mut distance = (self.of(highest) - height).abs();
        while at > lowest {
            at = at.lower();
            let d = (self.of(at) - height).abs();
            if d < distance {
                best = at;
                distance = d;
            }
        }
        best
    }

    /// Where a drag that ended at `height`, last moving `step` px per event (up is positive), settles: a fling (`step` of
    /// [`FLING_STEP`] or more) carries on for [`FLING_EVENTS`] events, then the nearest detent in range wins.
    #[must_use]
    pub fn settle(&self, height: f32, step: f32, lowest: SheetDetent, highest: SheetDetent) -> SheetDetent {
        self.nearest(height + carried(step), lowest, highest)
    }

    /// The same for a pane that stays where it was left: the height it rests at - a detent's height when a fling carried it
    /// to within `snap` px of one, else the height itself, held between the lowest and the highest detent's.
    #[must_use]
    pub fn settle_free(&self, height: f32, step: f32, snap: f32, lowest: SheetDetent, highest: SheetDetent) -> f32 {
        let (lowest, highest) = (lowest.min(highest), lowest.max(highest));
        let (low, high) = (self.of(lowest), self.of(highest));
        let height = if height.is_nan() { low } else { height.max(low).min(high) };
        let landing = (height + carried(step)).max(low).min(high);
        let near = self.nearest(landing, lowest, highest);
        let snap = if snap.is_finite() && snap > 0.0 { snap } else { 0.0 };
        if (self.of(near) - landing).abs() <= snap {
            self.of(near)
        } else {
            height
        }
    }

    /// The height a drag that began at `start_height` with the pointer at `start_y` makes of the pane with the pointer at
    /// `now_y` (window coordinates: y grows downwards, so a pointer going up grows the pane), held between the lowest and the
    /// highest detent's heights.
    #[must_use]
    pub fn dragged(&self, start_height: f32, start_y: f32, now_y: f32, lowest: SheetDetent, highest: SheetDetent) -> f32 {
        let wanted = start_height + (start_y - now_y);
        let (a, b) = (self.of(lowest), self.of(highest));
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        let wanted = if wanted.is_nan() { start_height } else { wanted };
        wanted.max(low).min(high)
    }
}

/// What a fling carries a pane on by, px: nothing for a step under [`FLING_STEP`] or one that is no number.
fn carried(step: f32) -> f32 {
    if step.is_finite() && step.abs() >= FLING_STEP {
        step * FLING_EVENTS
    } else {
        0.0
    }
}

/// What a key on the focused grip does to the detent: `None` for a key the grip ignores.
#[must_use]
pub fn detent_after_key(key: VirtualKeyCode, detent: SheetDetent, lowest: SheetDetent, highest: SheetDetent) -> Option<SheetDetent> {
    let (lowest, highest) = (lowest.min(highest), lowest.max(highest));
    let wanted = match key {
        VirtualKeyCode::Up | VirtualKeyCode::PageUp => detent.higher(),
        VirtualKeyCode::Down | VirtualKeyCode::PageDown => detent.lower(),
        VirtualKeyCode::Home => highest,
        VirtualKeyCode::End => lowest,
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter | VirtualKeyCode::Space => {
            return Some(step_round(detent, lowest, highest))
        }
        _ => return None,
    };
    Some(wanted.clamp(lowest, highest))
}

/// The tap's step: the next detent up, from the highest back to the lowest.
#[must_use]
pub fn step_round(detent: SheetDetent, lowest: SheetDetent, highest: SheetDetent) -> SheetDetent {
    let (lowest, highest) = (lowest.min(highest), lowest.max(highest));
    if detent >= highest {
        lowest
    } else {
        detent.higher().clamp(lowest, highest)
    }
}

#[cfg(test)]
#[path = "detents_test.rs"]
mod detents_test;
