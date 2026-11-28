//! What a vertical gesture on a tab panel does: scroll the content, pull towards the next or
//! the previous tab, or flip to it.
//!
//! The rules, which `TabContent::with_flip` runs and the guide states:
//!
//! 1. Content that can scroll scrolls FIRST. The finger that reaches its end has used up its
//!    gesture: lift it, put it down again at the end, and pull. A pull that starts with the panel
//!    at the edge it pulls past is the only one that flips (the web's pull to refresh asks the
//!    same, and it is why a long page never teleports the reader while they scroll it).
//! 2. Content that does not scroll flips on a vertical swipe, either way.
//! 3. A pull moves the content with RESISTANCE ([`rubber`]), shows what is next (the hint:
//!    "Next: Share", from [`HINT_MIN_PROGRESS`] of the threshold on) and flips the moment it
//!    crosses [`FLIP_THRESHOLD`] of pull. The finger that carries on pulls on: content that does not
//!    scroll flips again a threshold further, one tab per crossing (a page that scrolls flips once
//!    per gesture). Let go short of the threshold it springs back; let go FAST ([`FLING_VELOCITY`])
//!    past [`FLING_MIN_PULL`] it flips, and a faster fling carries on over more tabs, one flip
//!    after the other in a slowing rhythm ([`fling_plan`]): the tick of every crossing gives the
//!    fling its momentum.
//! 4. The first tab has no previous and the last no next: the pull is a stretch only, with more
//!    resistance, no hint, no flip (rubber band, no wrap).
//! 5. Never while the finger went down on something that claims the gesture (a nested
//!    scrollable with room in that direction, a slider, a map, a canvas), and never for a gesture
//!    that is more sideways than vertical. A system gesture that takes the finger away (the
//!    system sends CANCEL) ends the pull with a spring back: no flip, ever.
//! 6. A wheel or trackpad flips only for [`TabFlip::Always`], only against the edge, only after
//!    [`WHEEL_THRESHOLD`] of push, and not again within [`WHEEL_COOLDOWN_MS`]: one flick of a
//!    trackpad is one tab.
//!
//! All of it is plain numbers in logical px and milliseconds, so the table of gestures and
//! content states is tested without a window.

use alloc::string::String;

/// Whether a vertical gesture on a tab panel flips to the previous / next tab
/// (`TabContent::with_flip`). The pills stay the explicit way to choose a tab, and the one
/// assistive technology uses: a flip is a shortcut, never the only path.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum TabFlip {
    /// Never.
    Off,
    /// A finger that pulls past the end of the content (or swipes on content that does not scroll)
    /// flips. The wheel and the trackpad never do: a wheel at the end of a long page must not
    /// move the reader to another tab. The default.
    #[default]
    AtEdges,
    /// As `AtEdges`, and a wheel or trackpad that keeps pushing against the end of the content
    /// flips too, once per push (a panel made to be flipped: a kiosk, a ribbon band).
    Always,
}

/// Which way a flip goes.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FlipDirection {
    /// To the tab before this one: the content is pulled DOWN (a finger moving down).
    Previous,
    /// To the tab after this one: the content is pulled UP (a finger moving up).
    Next,
}

impl FlipDirection {
    /// The tab `direction` of `tab` among `count`, or `None` at the end (no wrap).
    #[must_use]
    pub const fn neighbour(self, tab: usize, count: usize) -> Option<usize> {
        match self {
            Self::Previous => {
                if tab > 0 && tab <= count {
                    Some(tab - 1)
                } else {
                    None
                }
            }
            Self::Next => {
                if tab + 1 < count {
                    Some(tab + 1)
                } else {
                    None
                }
            }
        }
    }
}

/// How far the finger moves before the gesture is decided, in logical px.
pub const FLIP_SLOP: f32 = 8.0;
/// How far a pull goes (beyond the slop) before a release flips, in logical px: more than the
/// 48 dp of a touch target, so a stray drag is not a flip.
pub const FLIP_THRESHOLD: f32 = 56.0;
/// How far the content can be pulled at most, in logical px (the rubber's limit).
pub const RUBBER_MAX: f32 = 96.0;
/// From this share of the threshold on, the hint ("Next: Share") shows.
pub const HINT_MIN_PROGRESS: f32 = 0.15;
/// A release at least this fast (px per second) flips with a shorter pull.
pub const FLING_VELOCITY: f32 = 800.0;
/// The shortest pull a fling flips with, in logical px.
pub const FLING_MIN_PULL: f32 = 24.0;
/// A finger that rests this long (milliseconds) before it lifts is not flinging.
pub const FLING_STALE_MS: u64 = 100;
/// Each this much speed beyond [`FLING_VELOCITY`] (px per second) carries a fling one tab further.
pub const FLING_STEP_VELOCITY: f32 = 800.0;
/// The most tabs a fling carries on over after the lift.
pub const FLING_MAX_EXTRA: usize = 4;
/// The time before the first flip a fling carries on to, in milliseconds.
pub const MOMENTUM_FIRST_DELAY_MS: u64 = 110;
/// Each flip of a fling comes this much later than the one before: the rhythm slows down.
pub const MOMENTUM_GROWTH: f32 = 1.55;
/// How much a wheel or trackpad pushes against the edge before it flips, in logical px.
pub const WHEEL_THRESHOLD: f32 = 120.0;
/// A push that stops for this long (milliseconds) is over: the next one starts from nothing.
pub const WHEEL_RESET_MS: u64 = 350;
/// The time after a flip (milliseconds) in which a wheel or trackpad cannot flip again.
pub const WHEEL_COOLDOWN_MS: u64 = 800;
/// A scroll offset within this many px of an end is AT that end.
pub const EDGE_EPSILON: f32 = 1.0;

/// The pull of a finger moved `distance` beyond the slop, as the distance the content follows
/// it: the same at first, then ever less, never more than [`RUBBER_MAX`]. `resistance` 1 is the
/// normal rubber, 2 the one at the first and the last tab (twice as stiff).
#[must_use]
pub fn rubber(distance: f32, resistance: f32) -> f32 {
    if !distance.is_finite() || distance <= 0.0 {
        return 0.0;
    }
    let max = RUBBER_MAX
        / if resistance.is_finite() && resistance >= 1.0 {
            resistance
        } else {
            1.0
        };
    max * distance / (distance + max)
}

/// The flips a fling carries on to after the lift: the delay in milliseconds before each, from the
/// lift (or from the one before), slowing down. `velocity` is the finger's speed along the pull, px
/// per second; `available` the tabs left in that direction. Nothing below
/// [`FLING_VELOCITY`] + [`FLING_STEP_VELOCITY`]: a plain fling flips the one tab it is let go on.
#[must_use]
pub fn fling_plan(velocity: f32, available: usize) -> Vec<u64> {
    if !velocity.is_finite() || velocity < FLING_VELOCITY + FLING_STEP_VELOCITY {
        return Vec::new();
    }
    let extra = (((velocity - FLING_VELOCITY) / FLING_STEP_VELOCITY) as usize)
        .min(FLING_MAX_EXTRA)
        .min(available);
    let mut delay = MOMENTUM_FIRST_DELAY_MS as f32;
    (0..extra)
        .map(|_| {
            let now = delay;
            delay *= MOMENTUM_GROWTH;
            now.round() as u64
        })
        .collect()
}

/// What a fling carries on to after the lift ([`FlipGesture::momentum`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Momentum {
    /// Which way.
    pub direction: FlipDirection,
    /// The tab the fling stands on at the lift; the first flip of the momentum goes one tab
    /// further in `direction`.
    pub from: usize,
    /// The delay in milliseconds before each flip, slowing down.
    pub delays: Vec<u64>,
}

/// What the panel is, when a gesture on it begins.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct FlipPanel {
    /// The tab bar's choice.
    pub flip: TabFlip,
    /// The tab shown.
    pub tab: usize,
    /// How many tabs there are.
    pub tab_count: usize,
    /// Whether the content is taller than the panel (it can scroll).
    pub overflows: bool,
    /// How far the content is scrolled: 0 at the top.
    pub scroll: f32,
    /// How far it can scroll at most.
    pub scroll_max: f32,
}

impl FlipPanel {
    /// The content is at its top end (or has no overflow).
    #[must_use]
    pub fn at_top(&self) -> bool {
        !self.overflows || self.scroll <= EDGE_EPSILON || self.scroll.is_nan()
    }

    /// The content is at its bottom end (or has no overflow).
    #[must_use]
    pub fn at_bottom(&self) -> bool {
        !self.overflows || self.scroll >= self.scroll_max - EDGE_EPSILON || self.scroll.is_nan()
    }

    /// Whether a pull towards `direction` may begin with the panel as it is: the content is at
    /// the end that pull goes past.
    #[must_use]
    pub fn may_pull(&self, direction: FlipDirection) -> bool {
        if self.flip == TabFlip::Off || self.tab_count < 2 {
            return false;
        }
        match direction {
            FlipDirection::Previous => self.at_top(),
            FlipDirection::Next => self.at_bottom(),
        }
    }
}

/// What a gesture asks the panel to show or do.
#[derive(Debug, Clone, PartialEq)]
pub enum FlipOutcome {
    /// Not ours (or nothing yet): the content scrolls, the panel does nothing.
    Idle,
    /// A pull is on: shift the content by `offset` px (towards the finger: up for `Next`, down for
    /// `Previous`), show the hint when `hint` is `Some` (`progress` 1 = it would flip now).
    Pulling {
        /// Which way.
        direction: FlipDirection,
        /// How far the content follows the finger, px.
        offset: f32,
        /// 0 to 1 (and over): the share of the threshold.
        progress: f32,
        /// The tab this goes to, `None` at the end of the row (a stretch, no hint).
        target: Option<usize>,
        /// The hint to show, or `None`: not yet far enough, or nothing to flip to.
        hint: Option<String>,
    },
    /// Flip to tab `to`.
    Flip {
        /// Which way.
        direction: FlipDirection,
        /// The new tab.
        to: usize,
    },
    /// The gesture ended without a flip: the content springs back.
    Release,
}

/// What a finger is doing to the panel: down, moves, up (or taken away).
// Not `Copy`: a gesture is state, and a copy of it would be a second finger.
#[allow(missing_copy_implementations)]
#[derive(Debug, Clone)]
pub struct FlipGesture {
    panel: FlipPanel,
    start: (f32, f32),
    last: (f32, f32, u64),
    previous: (f32, f32, u64),
    state: GestureState,
    /// How many times the finger has flipped this gesture.
    flips: u32,
    /// What a fling carries on to after the lift, set by [`Self::lifted`].
    momentum: Option<Momentum>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum GestureState {
    /// Down, not decided yet.
    Undecided,
    /// A pull towards this direction.
    Pulling(FlipDirection),
    /// Not ours: the content (or whoever claimed it) has this finger.
    NotOurs,
    /// Over.
    Done,
}

impl FlipGesture {
    /// A finger went down at `(x, y)` at time `t_ms` on a panel as `panel` says. `claimed`: the
    /// finger went down on something that claims the gesture (a nested scrollable with room in
    /// that direction, a slider, a map, a canvas): the gesture is not the panel's.
    #[must_use]
    pub fn begin(panel: FlipPanel, x: f32, y: f32, t_ms: u64, claimed: bool) -> Self {
        let state = if claimed || panel.flip == TabFlip::Off || panel.tab_count < 2 {
            GestureState::NotOurs
        } else {
            GestureState::Undecided
        };
        Self {
            panel,
            start: (x, y),
            last: (x, y, t_ms),
            previous: (x, y, t_ms),
            state,
            flips: 0,
            momentum: None,
        }
    }

    /// The finger moved to `(x, y)` at time `t_ms`. `names` gives the label of tab `i` for the hint.
    pub fn moved(
        &mut self,
        x: f32,
        y: f32,
        t_ms: u64,
        names: &dyn Fn(usize) -> String,
    ) -> FlipOutcome {
        if !x.is_finite() || !y.is_finite() {
            return self.cancel();
        }
        self.previous = self.last;
        self.last = (x, y, t_ms);
        if self.state == GestureState::Undecided {
            self.decide();
        }
        match self.state {
            GestureState::Pulling(direction) => {
                if self.pull_distance(direction) >= FLIP_THRESHOLD {
                    if let Some(to) = direction.neighbour(self.panel.tab, self.panel.tab_count) {
                        return self.crossed(direction, to);
                    }
                }
                self.pulling(direction, names)
            }
            _ => FlipOutcome::Idle,
        }
    }

    /// The pull crossed the threshold: flip now. Content that does not scroll lets the finger carry
    /// on (the new tab is the panel, and the next flip is a threshold further from where the finger
    /// is); a page that scrolls flips once per gesture.
    const fn crossed(&mut self, direction: FlipDirection, to: usize) -> FlipOutcome {
        self.flips += 1;
        if self.panel.overflows {
            self.state = GestureState::Done;
        } else {
            self.panel.tab = to;
            self.start = (self.last.0, self.last.1);
        }
        FlipOutcome::Flip { direction, to }
    }

    /// The finger went up at time `t_ms`: where it is is where the last move left it. A pull that
    /// is let go fast flips the tab it is on (past [`FLING_MIN_PULL`]); a faster one is a fling
    /// that carries on ([`Self::momentum`]).
    pub fn lifted(&mut self, t_ms: u64) -> FlipOutcome {
        if let GestureState::Pulling(direction) = self.state {
            self.state = GestureState::Done;
            let pull = self.pull_distance(direction);
            // A finger that paused before it lifted has no speed to fling with.
            let stale = t_ms.saturating_sub(self.last.2) > FLING_STALE_MS;
            let speed = if stale { 0.0 } else { self.velocity(direction) };
            let fast = speed >= FLING_VELOCITY && (pull >= FLING_MIN_PULL || self.flips > 0);
            let target = direction.neighbour(self.panel.tab, self.panel.tab_count);
            let immediate = match target {
                Some(to) if pull >= FLIP_THRESHOLD || (fast && pull >= FLING_MIN_PULL) => Some(to),
                _ => None,
            };
            let from = immediate.unwrap_or(self.panel.tab);
            if fast && !self.panel.overflows {
                let available = match direction {
                    FlipDirection::Next => self.panel.tab_count.saturating_sub(from + 1),
                    FlipDirection::Previous => from,
                };
                let delays = fling_plan(speed, available);
                if !delays.is_empty() {
                    self.momentum = Some(Momentum {
                        direction,
                        from,
                        delays,
                    });
                }
            }
            return match immediate {
                Some(to) => {
                    self.flips += 1;
                    FlipOutcome::Flip { direction, to }
                }
                None => FlipOutcome::Release,
            };
        }
        self.state = GestureState::Done;
        FlipOutcome::Idle
    }

    /// What a fling carries on to after [`Self::lifted`]: `None` for a lift that was not a fast one,
    /// or whose content scrolls.
    #[must_use]
    pub fn momentum(&self) -> Option<Momentum> {
        self.momentum.clone()
    }

    /// How many times the finger flipped in this gesture.
    #[must_use]
    pub const fn flips(&self) -> u32 {
        self.flips
    }

    /// The finger was taken away: the system took the gesture (a swipe from an edge, the back or
    /// the home gesture), a call came in, the window lost the touch. Never a flip.
    pub const fn cancel(&mut self) -> FlipOutcome {
        let pulling = matches!(self.state, GestureState::Pulling(_));
        self.state = GestureState::Done;
        if pulling {
            FlipOutcome::Release
        } else {
            FlipOutcome::Idle
        }
    }

    /// Whether the gesture is a pull of the panel's.
    #[must_use]
    pub const fn is_pulling(&self) -> bool {
        matches!(self.state, GestureState::Pulling(_))
    }

    fn decide(&mut self) {
        let dx = self.last.0 - self.start.0;
        let dy = self.last.1 - self.start.1;
        let travelled = dy.abs().max(dx.abs());
        if travelled < FLIP_SLOP {
            return;
        }
        // More sideways than vertical: the strip's, a carousel's, not a flip.
        if dx.abs() > dy.abs() {
            self.state = GestureState::NotOurs;
            return;
        }
        // A finger moving UP pulls the content up, to the end below it: the next tab.
        let direction = if dy < 0.0 {
            FlipDirection::Next
        } else {
            FlipDirection::Previous
        };
        self.state = if self.panel.may_pull(direction) {
            GestureState::Pulling(direction)
        } else {
            GestureState::NotOurs
        };
    }

    /// How far the finger is past the slop, along the pull, px.
    fn pull_distance(&self, direction: FlipDirection) -> f32 {
        let dy = self.last.1 - self.start.1;
        let along = match direction {
            FlipDirection::Next => -dy,
            FlipDirection::Previous => dy,
        };
        (along - FLIP_SLOP).max(0.0)
    }

    /// The speed of the finger along the pull over the last two samples, px per second; 0 when
    /// the samples are too close in time to tell.
    fn velocity(&self, direction: FlipDirection) -> f32 {
        let (_, y0, t0) = self.previous;
        let (_, y1, t1) = self.last;
        if t1 <= t0 {
            return 0.0;
        }
        let dy = y1 - y0;
        let along = match direction {
            FlipDirection::Next => -dy,
            FlipDirection::Previous => dy,
        };
        along / ((t1 - t0) as f32 / 1000.0)
    }

    fn pulling(&self, direction: FlipDirection, names: &dyn Fn(usize) -> String) -> FlipOutcome {
        let pull = self.pull_distance(direction);
        let target = direction.neighbour(self.panel.tab, self.panel.tab_count);
        let resistance = if target.is_some() { 1.0 } else { 2.0 };
        let progress = if target.is_some() {
            pull / FLIP_THRESHOLD
        } else {
            0.0
        };
        let hint = match target {
            Some(to) if progress >= HINT_MIN_PROGRESS => Some(hint_text(direction, &names(to))),
            _ => None,
        };
        FlipOutcome::Pulling {
            direction,
            offset: rubber(pull, resistance),
            progress,
            target,
            hint,
        }
    }
}

/// The text of the hint chip: "Next: Share" / "Previous: Home".
#[must_use]
pub fn hint_text(direction: FlipDirection, name: &str) -> String {
    let mut text = String::from(match direction {
        FlipDirection::Next => "Next: ",
        FlipDirection::Previous => "Previous: ",
    });
    text.push_str(name);
    text
}

/// A wheel or trackpad pushing against the edge of the panel ([`TabFlip::Always`] only).
// Not `Copy`: the push is state, and a copy of it would count the same wheel twice.
#[allow(missing_copy_implementations)]
#[derive(Debug, Clone, Default)]
pub struct WheelPush {
    /// The push so far, px, signed (positive: the content scrolls down).
    pushed: f32,
    /// When the last step came, milliseconds.
    last_step: Option<u64>,
    /// When the last flip happened, milliseconds.
    last_flip: Option<u64>,
}

impl WheelPush {
    /// A wheel step of `delta` px (positive: the content scrolls down) at `t_ms` on `panel`.
    pub fn step(&mut self, panel: &FlipPanel, delta: f32, t_ms: u64) -> FlipOutcome {
        if panel.flip != TabFlip::Always || !delta.is_finite() || delta == 0.0 {
            return FlipOutcome::Idle;
        }
        let direction = if delta > 0.0 {
            FlipDirection::Next
        } else {
            FlipDirection::Previous
        };
        // Silence ends a push; so does turning round.
        let silent = self
            .last_step
            .is_none_or(|t| t_ms.saturating_sub(t) > WHEEL_RESET_MS);
        if silent || (self.pushed > 0.0) != (delta > 0.0) {
            self.pushed = 0.0;
        }
        self.last_step = Some(t_ms);
        // Only against the edge: a wheel that still scrolls the content is the content's.
        if !panel.may_pull(direction) {
            self.pushed = 0.0;
            return FlipOutcome::Idle;
        }
        let cooling = self
            .last_flip
            .is_some_and(|t| t_ms.saturating_sub(t) < WHEEL_COOLDOWN_MS);
        if cooling {
            return FlipOutcome::Idle;
        }
        self.pushed += delta;
        if self.pushed.abs() >= WHEEL_THRESHOLD {
            self.pushed = 0.0;
            return match direction.neighbour(panel.tab, panel.tab_count) {
                Some(to) => {
                    self.last_flip = Some(t_ms);
                    FlipOutcome::Flip { direction, to }
                }
                None => FlipOutcome::Release,
            };
        }
        FlipOutcome::Idle
    }
}

#[cfg(test)]
#[path = "tab_flip_test.rs"]
mod tab_flip_test;
