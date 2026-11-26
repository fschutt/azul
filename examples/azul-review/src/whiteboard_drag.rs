//! From a loop on a page to a cutout on the whiteboard: the region a loop arms, and the drag that
//! takes it to the right bar. A pure state machine on an injected clock (the milliseconds the caller
//! passes in) and on the pointer's samples; no window, no timer, no azul.
//!
//! - A recognised loop ([`crate::whiteboard_lasso`]) ARMS its region for [`ARM_MS`] (a setting: [`ARM_MIN_MS`] to
//!   [`ARM_MAX_MS`], [`ArmedRegions::set_lifetime_ms`]). Armed, the region has a visible grip on its outline (top right) and
//!   answers [`ArmedRegions::armed_region_at`] (the lines it covers and its outline as an SVG shape: what a pen resting over
//!   it is about). How much of its time is left is [`ArmedRegion::fraction_left`]: the ring the page draws round the grip.
//! - A plain press inside the region still inks: striking a word, underlining it, a quick slash are what
//!   a person does inside a loop they just drew, and a drag that began like any other press would steal
//!   exactly those strokes. The region is LIFTED by pressing its grip (at once), or by pressing and
//!   holding inside it ([`HOLD_MS`] without moving more than [`HOLD_JITTER_PX`]); a press that moves
//!   sooner is a stroke and the machine lets go of it.
//! - A lifted region follows the pointer after [`DRAG_START_PX`] of movement. Near the window's right
//!   edge the closed bar is asked to open (once); let go over the open bar and the region is DROPPED
//!   there, in the bar's coordinates; anywhere else, or Escape, it is put down and stays armed.

use crate::whiteboard_geom::{point_in_polygon, svg_path, Pt, Rect, Size};

/// How long a recognised loop stays armed unless the setting says otherwise, ms.
pub const ARM_MS: u64 = 8_000;
/// The shortest and the longest a loop may stay armed (the setting is held to these): less is no time to reach for the
/// grip, more leaves the page covered with loops nobody means any more.
pub const ARM_MIN_MS: u64 = 2_000;
pub const ARM_MAX_MS: u64 = 60_000;
/// The side of the grip on the region's outline, px.
pub const GRIP_PX: f32 = 22.0;
/// How long a press inside the region is held before it lifts the region, ms.
pub const HOLD_MS: u64 = 300;
/// How far the pointer may wander during that hold, px (a hand is not still).
pub const HOLD_JITTER_PX: f32 = 6.0;
/// How far a lifted region is moved before it is a drag, px.
pub const DRAG_START_PX: f32 = 6.0;
/// How near the window's right edge a drag asks a closed bar to open, px.
pub const EDGE_OPEN_PX: f32 = 28.0;
/// How many regions are armed at once (the oldest goes first).
pub const MAX_ARMED: usize = 8;

/// What names an armed region.
pub type RegionId = u64;

/// What a loop gives to be armed.
#[derive(Debug, Clone, PartialEq)]
pub struct Armed {
    pub page: usize,
    /// The loop's outline, in the page's px.
    pub polygon: Vec<Pt>,
    /// The lines of the file it covers (from 1).
    pub first_line: u32,
    pub last_line: u32,
    /// The stroke the loop was drawn with.
    pub stroke_id: u64,
}

/// A loop waiting to be lifted.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmedRegion {
    pub id: RegionId,
    pub page: usize,
    pub polygon: Vec<Pt>,
    pub bounds: Rect,
    pub first_line: u32,
    pub last_line: u32,
    pub stroke_id: u64,
    pub armed_at_ms: u64,
    /// How long it stays armed from `armed_at_ms`, ms (what the setting said when it was armed).
    pub lifetime_ms: u64,
    /// The grip: on the outline at the top right, kept on the page.
    pub grip: Rect,
}

impl ArmedRegion {
    /// The outline as the `d` of an SVG path (what an agent is told the region's shape is).
    #[must_use]
    pub fn shape_svg(&self) -> String {
        svg_path(&self.polygon)
    }

    /// When the region stops being armed.
    #[must_use]
    pub const fn expires_at_ms(&self) -> u64 {
        self.armed_at_ms.saturating_add(self.lifetime_ms)
    }

    fn live(&self, now_ms: u64) -> bool {
        now_ms < self.expires_at_ms()
    }

    /// How much of its time is left at `now_ms`: 1 when it was armed, 0 when it is up (what a ring round the grip shows,
    /// sweeping from a full circle to nothing).
    #[must_use]
    pub fn fraction_left(&self, now_ms: u64) -> f32 {
        if self.lifetime_ms == 0 {
            return 0.0;
        }
        let spent = now_ms.saturating_sub(self.armed_at_ms);
        (1.0 - spent as f64 / self.lifetime_ms as f64).clamp(0.0, 1.0) as f32
    }

    /// The seconds left at `now_ms`, rounded up (what a screen reader says: "5 seconds left"); 0 when it is up.
    #[must_use]
    pub const fn seconds_left(&self, now_ms: u64) -> u64 {
        self.expires_at_ms().saturating_sub(now_ms).div_ceil(1_000)
    }
}

/// The regions armed now.
#[derive(Debug, Clone)]
pub struct ArmedRegions {
    list: Vec<ArmedRegion>,
    next_id: RegionId,
    /// How long a region armed from now on stays armed, ms.
    lifetime_ms: u64,
}

impl Default for ArmedRegions {
    fn default() -> Self {
        Self { list: Vec::new(), next_id: 0, lifetime_ms: ARM_MS }
    }
}

impl ArmedRegions {
    /// How long a region armed now stays armed, ms.
    #[must_use]
    pub const fn lifetime_ms(&self) -> u64 {
        self.lifetime_ms
    }

    /// How long the regions armed from now on stay armed (held to [`ARM_MIN_MS`]..=[`ARM_MAX_MS`]); what it is now. The
    /// regions that are armed keep the time they were given.
    pub fn set_lifetime_ms(&mut self, ms: u64) -> u64 {
        self.lifetime_ms = ms.clamp(ARM_MIN_MS, ARM_MAX_MS);
        self.lifetime_ms
    }

    /// Arms a loop at `now_ms`; the newest region answers first, and past [`MAX_ARMED`] the oldest goes.
    pub fn arm(&mut self, armed: Armed, page: Size, now_ms: u64) -> RegionId {
        self.next_id += 1;
        let id = self.next_id;
        let bounds = Rect::from_points(&armed.polygon).unwrap_or_default();
        // The point of the outline that is furthest up and to the right.
        let corner = armed
            .polygon
            .iter()
            .copied()
            .max_by(|a, b| (a.x - a.y).total_cmp(&(b.x - b.y)))
            .unwrap_or_else(|| Pt::new(bounds.right(), bounds.y));
        let grip = Rect::new(
            (corner.x - GRIP_PX * 0.5).clamp(0.0, (page.w - GRIP_PX).max(0.0)),
            (corner.y - GRIP_PX * 0.5).clamp(0.0, (page.h - GRIP_PX).max(0.0)),
            GRIP_PX,
            GRIP_PX,
        );
        self.list.push(ArmedRegion {
            id,
            page: armed.page,
            polygon: armed.polygon,
            bounds,
            first_line: armed.first_line,
            last_line: armed.last_line,
            stroke_id: armed.stroke_id,
            armed_at_ms: now_ms,
            lifetime_ms: self.lifetime_ms,
            grip,
        });
        while self.list.len() > MAX_ARMED {
            self.list.remove(0);
        }
        id
    }

    /// Lets go of the regions whose time is up; whether any went.
    pub fn expire(&mut self, now_ms: u64) -> bool {
        self.expire_except(now_ms, None)
    }

    /// The same, but the region `keep` stays whatever its time (the one the pointer holds or carries: it is not let go of
    /// under the hand; its time is up as soon as the pointer lets it go).
    pub fn expire_except(&mut self, now_ms: u64, keep: Option<RegionId>) -> bool {
        let before = self.list.len();
        self.list.retain(|r| r.live(now_ms) || Some(r.id) == keep);
        self.list.len() != before
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: RegionId) -> Option<&ArmedRegion> {
        self.list.iter().find(|r| r.id == id)
    }

    /// The regions, the oldest first.
    #[must_use]
    pub fn all(&self) -> &[ArmedRegion] {
        &self.list
    }

    /// Takes region `id` out (its cutout is being made).
    pub fn take(&mut self, id: RegionId) -> Option<ArmedRegion> {
        let at = self.list.iter().position(|r| r.id == id)?;
        Some(self.list.remove(at))
    }

    /// Lets go of region `id`.
    pub fn disarm(&mut self, id: RegionId) {
        self.list.retain(|r| r.id != id);
    }

    /// Lets go of every region.
    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// Lets go of the regions of `page` (it was edited, or left).
    pub fn clear_page(&mut self, page: usize) {
        self.list.retain(|r| r.page != page);
    }

    /// The armed region (the newest) that has the point `p` of `page` inside its outline, at `now_ms`.
    /// What a pen resting there is about: the region carries the lines it covers and its shape.
    #[must_use]
    pub fn armed_region_at(&self, page: usize, p: Pt, now_ms: u64) -> Option<&ArmedRegion> {
        self.list
            .iter()
            .rev()
            .find(|r| r.page == page && r.live(now_ms) && (r.bounds.contains(p) && point_in_polygon(&r.polygon, p)))
    }

    /// The region (the newest) whose grip is under the point `p` of `page`.
    #[must_use]
    pub fn grip_at(&self, page: usize, p: Pt, now_ms: u64) -> Option<RegionId> {
        self.list.iter().rev().find(|r| r.page == page && r.live(now_ms) && r.grip.contains(p)).map(|r| r.id)
    }
}

/// Where the pointer is: on which page, where on it, and where in the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointer {
    pub page: usize,
    pub in_page: Pt,
    pub in_window: Pt,
}

/// The whiteboard's bar as the window has it now: where a drop lands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    /// Its rectangle in the window (the thin edge while it is closed).
    pub rect: Rect,
    pub open: bool,
    /// The window's width: the right edge is where a closed bar is asked to open.
    pub window_w: f32,
}

/// What a press is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// The grip of a region: it lifts as the pointer moves.
    Grip(RegionId),
    /// Inside a region: it inks, and lifts when it is held.
    Inside(RegionId),
    /// Nothing armed: it inks.
    Elsewhere,
}

/// What a motion of the pointer meant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    Nothing,
    /// The press moved before the hold time: it is a stroke, the machine has let go of it.
    Inking,
    /// The press was held (told by a motion that came after the hold time: the tick may not have).
    Held(RegionId),
    /// The region is lifted: from now on it follows the pointer.
    Lifted(RegionId),
    /// The lifted region is on its way. `open_bar`: the pointer reached the edge of a closed bar (told once).
    Dragging { region: RegionId, over_bar: bool, open_bar: bool },
}

/// What the passing of time meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    /// The press inside the region has been held: the stroke it began is dropped, the region lifts.
    Held(RegionId),
}

/// What letting go meant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Release {
    Nothing,
    /// The region is dropped on the open bar, `at_bar` from the bar's top-left corner.
    Dropped { region: RegionId, at_bar: Pt },
    /// The region was lifted and let go of elsewhere: it is put down and stays armed.
    Cancelled,
}

/// The lifted region and where the pointer carries it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ghost {
    pub region: RegionId,
    /// In the window.
    pub at: Pt,
    /// Where the press that lifted it was, in the window.
    pub from: Pt,
}

impl Ghost {
    /// How far the pointer has carried the region from where it lay: draw its outline moved by this.
    #[must_use]
    pub fn offset(&self) -> Pt {
        Pt::new(self.at.x - self.from.x, self.at.y - self.from.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    /// A press on a region, not yet a drag.
    Pressed { region: RegionId, origin: Pt, since_ms: u64, via_grip: bool, held: bool },
    Dragging { region: RegionId, at: Pt, from: Pt, bar_asked: bool },
}

/// The drag of an armed region to the bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drag {
    phase: Phase,
}

impl Default for Drag {
    fn default() -> Self {
        Self { phase: Phase::Idle }
    }
}

impl Drag {
    /// A press of the pointer. What it is over; a press over a region is followed from here on.
    pub fn press(&mut self, regions: &ArmedRegions, at: Pointer, now_ms: u64) -> Press {
        self.phase = Phase::Idle;
        if let Some(id) = regions.grip_at(at.page, at.in_page, now_ms) {
            self.phase = Phase::Pressed { region: id, origin: at.in_window, since_ms: now_ms, via_grip: true, held: true };
            return Press::Grip(id);
        }
        if let Some(r) = regions.armed_region_at(at.page, at.in_page, now_ms) {
            self.phase = Phase::Pressed { region: r.id, origin: at.in_window, since_ms: now_ms, via_grip: false, held: false };
            return Press::Inside(r.id);
        }
        Press::Elsewhere
    }

    /// The pointer moved (to `at`, in the window) with the button down.
    pub fn motion(&mut self, at: Pt, now_ms: u64, bar: &Bar) -> Motion {
        match self.phase {
            Phase::Idle => Motion::Nothing,
            Phase::Pressed { region, origin, since_ms, via_grip, held } => {
                let moved = origin.dist(at);
                if !held {
                    if moved > HOLD_JITTER_PX && now_ms.saturating_sub(since_ms) < HOLD_MS {
                        self.phase = Phase::Idle;
                        return Motion::Inking;
                    }
                    if now_ms.saturating_sub(since_ms) >= HOLD_MS && moved <= HOLD_JITTER_PX {
                        self.phase = Phase::Pressed { region, origin, since_ms, via_grip, held: true };
                        return Motion::Held(region);
                    }
                    if moved > HOLD_JITTER_PX {
                        // Held long enough, and now it moves: a hold that no tick told yet, then a lift.
                        self.phase = Phase::Dragging { region, at, from: origin, bar_asked: false };
                        return Motion::Lifted(region);
                    }
                    return Motion::Nothing;
                }
                if moved > DRAG_START_PX {
                    self.phase = Phase::Dragging { region, at, from: origin, bar_asked: false };
                    return Motion::Lifted(region);
                }
                Motion::Nothing
            }
            Phase::Dragging { region, from, bar_asked, .. } => {
                let over_bar = bar.rect.contains(at);
                let near_edge = bar.window_w > 0.0 && at.x >= bar.window_w - EDGE_OPEN_PX;
                let open_bar = !bar.open && !bar_asked && (near_edge || over_bar);
                self.phase = Phase::Dragging { region, at, from, bar_asked: bar_asked || open_bar };
                Motion::Dragging { region, over_bar, open_bar }
            }
        }
    }

    /// Time passes: a press that has been held long enough lifts its region.
    pub fn tick(&mut self, now_ms: u64) -> Option<Tick> {
        if let Phase::Pressed { region, origin, since_ms, via_grip, held: false } = self.phase {
            if now_ms.saturating_sub(since_ms) >= HOLD_MS {
                self.phase = Phase::Pressed { region, origin, since_ms, via_grip, held: true };
                return Some(Tick::Held(region));
            }
        }
        None
    }

    /// The button went up at `at` (in the window).
    pub fn release(&mut self, at: Pt, bar: &Bar) -> Release {
        let phase = std::mem::replace(&mut self.phase, Phase::Idle);
        match phase {
            Phase::Dragging { region, .. } => {
                if bar.open && bar.rect.contains(at) {
                    Release::Dropped { region, at_bar: Pt::new(at.x - bar.rect.x, at.y - bar.rect.y) }
                } else {
                    Release::Cancelled
                }
            }
            Phase::Idle | Phase::Pressed { .. } => Release::Nothing,
        }
    }

    /// Escape: whatever was going on is dropped; whether anything was.
    pub fn cancel(&mut self) -> bool {
        let was = self.is_active();
        self.phase = Phase::Idle;
        was
    }

    /// Whether a press on a region is being followed.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    /// The region the press that is followed is on (held, or carried).
    #[must_use]
    pub const fn region(&self) -> Option<RegionId> {
        match self.phase {
            Phase::Idle => None,
            Phase::Pressed { region, .. } | Phase::Dragging { region, .. } => Some(region),
        }
    }

    /// The region the pointer carries, and where.
    #[must_use]
    pub const fn ghost(&self) -> Option<Ghost> {
        match self.phase {
            Phase::Dragging { region, at, from, .. } => Some(Ghost { region, at, from }),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "whiteboard_drag_test.rs"]
mod whiteboard_drag_test;
