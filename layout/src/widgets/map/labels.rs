//! Where the map's labels go, frame after frame.
//!
//! Every tile hands the render its labels: city names, rivers, roads, shops.
//! The render lays them over the whole view so that none covers another and
//! the more important ones win. Solving that from scratch every frame makes
//! the names flicker: two labels of equal rank swap when the view moves a
//! pixel, because the winner was whichever tile happened to be read first.
//! So the placement remembers what it showed:
//!
//! - A label's identity does not depend on the tile it came from. It is its
//!   kind and text plus where it stands in the world, so the same city keeps
//!   its identity across tiles, pans and zoom levels.
//! - Labels are tried in a fixed order: rank first, then the labels that hold
//!   a slot from the last frame, then the identity. A label keeps its slot
//!   until a label of strictly higher rank needs the space, and nothing in the
//!   order depends on the tiles' order.
//! - A label that gains a slot fades in and one that loses it fades out, over
//!   [`LABEL_FADE_SECS`]. A fading label takes no space.
//!
//! Positions are the render's: the grid's pixels. World positions are Web
//! Mercator fractions, `0..1` from west to east and from north to south.

use alloc::{collections::BTreeMap, vec::Vec};

use super::TileLabel;

/// How long a label takes to appear or to go, in seconds.
pub const LABEL_FADE_SECS: f64 = 0.15;

/// The most labels one frame shows. Each is a text node the view lays out
/// and shapes on every render; 300 cost more than the tiles under them, and
/// a decluttered map window rarely holds more than this many readable names.
pub const MAX_LABELS_PER_FRAME: usize = 160;

/// A label is not repeated closer than this to itself (one road name per
/// tile reads as a repeat every 256 px otherwise).
pub const LABEL_REPEAT_PX: f32 = 180.0;

/// House numbers only from this zoom on (the tiles carry them from z14,
/// where they would bury the streets).
pub const HOUSENUMBER_MIN_ZOOM: f32 = 17.0;

/// How far a label may lie from where it was last frame and still be the
/// same label, in screen px. A place's anchor moves by a fraction of a pixel
/// between the tiles of two zoom levels; two anchors of one river name are
/// a whole tile apart.
const SAME_LABEL_PX: f64 = 48.0;

/// The most labels the placement remembers. A view shows a few hundred at
/// most; the rest are labels fading out or held back by a collision.
const MAX_REMEMBERED: usize = 2048;

/// One label a tile offers this frame.
#[derive(Debug, Clone, Copy)]
pub struct LabelCandidate<'a> {
    /// Where the anchor (the label's middle) is drawn, grid px.
    pub x: f32,
    pub y: f32,
    pub label: &'a TileLabel,
    /// Where the anchor is in the world (Web Mercator fractions).
    pub world: (f64, f64),
}

/// The view the labels are placed in.
#[derive(Debug, Clone, Copy)]
pub struct LabelView {
    /// The view's size, px.
    pub width: f32,
    pub height: f32,
    /// How far outside the view a label may stand and still be placed: a
    /// turned or tilted camera shows more of the grid than its box.
    pub margin: f32,
    /// The viewport's (fractional) zoom.
    pub zoom: f32,
    /// The world point at the view's centre (Web Mercator fractions).
    pub centre: (f64, f64),
}

impl LabelView {
    /// The world's width in px at this zoom.
    #[must_use]
    pub fn world_px(&self) -> f64 {
        256.0 * 2f64.powf(f64::from(self.zoom))
    }

    /// `candidate`'s world point in the copy of the world it is drawn in:
    /// its west-east fraction moved by the whole world widths between where
    /// the tile puts it and where the view draws it. A view wider than the
    /// world (a low zoom) shows the world more than once, and the same label
    /// stands in every copy at the same point of the world: told apart only
    /// by that point, the copies took each other's remembered slots frame
    /// after frame and the labels never stopped fading.
    fn drawn_copy(&self, candidate: &LabelCandidate<'_>) -> (f64, f64) {
        let seen = self.centre.0
            + (f64::from(candidate.x) - f64::from(self.width) * 0.5) / self.world_px();
        let copy = (seen - candidate.world.0).round();
        let copy = if copy.is_finite() { copy } else { 0.0 };
        (candidate.world.0 + copy, candidate.world.1)
    }

    /// Where the view draws world point `world`, grid px. `world` names the
    /// copy of the world it stands in ([`Self::drawn_copy`]), so a label of
    /// the copy right of the centre is drawn there, not in the nearer one.
    #[allow(clippy::cast_possible_truncation)] // screen coordinates of a view
    fn screen_of(&self, world: (f64, f64)) -> (f32, f32) {
        let px = self.world_px();
        let dx = world.0 - self.centre.0;
        let dy = world.1 - self.centre.1;
        (
            (dx * px + f64::from(self.width) * 0.5) as f32,
            (dy * px + f64::from(self.height) * 0.5) as f32,
        )
    }
}

/// What a frame's labels come from: the candidate at that index, or a
/// remembered label that is fading out ([`LabelPlacement::remembered`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacedFrom {
    Candidate(usize),
    Remembered(usize),
}

/// One label the frame draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedLabel {
    /// The anchor, grid px.
    pub x: f32,
    pub y: f32,
    /// 0 (invisible) .. 1 (fully drawn).
    pub opacity: f32,
    /// The label's identity across frames: the key its node keeps, so the
    /// view's reconcile pairs it with itself however its tile changed.
    pub id: u64,
    pub from: PlacedFrom,
}

/// The labels one frame draws.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelFrame {
    /// Fading-out labels first (they are drawn under the others), then the
    /// placed ones, most important first.
    pub placed: Vec<PlacedLabel>,
    /// When the last fade in flight ends (clock seconds); the frame's own
    /// time when nothing fades. The view must be drawn again until then.
    pub settled_at: f64,
}

/// One label the placement remembers.
#[derive(Debug, Clone)]
struct Remembered {
    /// [`label_identity`] of its kind and text.
    identity: u64,
    /// Where it stood when a tile last offered it.
    world: (f64, f64),
    /// The label as last offered: what a fading-out label is drawn with.
    label: TileLabel,
    /// The key of its node ([`PlacedLabel::id`]), unique per placement.
    id: u64,
    /// Whether it holds a slot. Its opacity runs towards 1 while it does,
    /// towards 0 while it does not.
    shown: bool,
    /// The opacity when `shown` last changed, and when that was.
    from: f32,
    since: f64,
    /// The frame a candidate last matched it in.
    seen: u64,
}

impl Remembered {
    fn opacity(&self, now: f64) -> f32 {
        let t = super::fade_progress(now - self.since, LABEL_FADE_SECS);
        let to = if self.shown { 1.0 } else { 0.0 };
        (self.from + (to - self.from) * t).clamp(0.0, 1.0)
    }

    /// Whether a fade is still running at `now`.
    fn fading(&self, now: f64) -> bool {
        let o = self.opacity(now);
        if self.shown {
            o < 1.0
        } else {
            o > 0.0
        }
    }

    fn set_shown(&mut self, shown: bool, now: f64) {
        if self.shown != shown {
            self.from = self.opacity(now);
            self.since = now;
            self.shown = shown;
        }
    }
}

/// The labels a map showed, kept between its renders (in its tile cache).
#[derive(Debug, Clone, Default)]
pub struct LabelPlacement {
    /// Every label shown, fading or held back since a tile last offered it.
    entries: Vec<Remembered>,
    /// The next [`Remembered::id`].
    next_id: u64,
    /// Counts [`Self::place`] calls: what `Remembered::seen` is measured in.
    frame: u64,
}

impl LabelPlacement {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 0,
            frame: 0,
        }
    }

    /// The label a [`PlacedFrom::Remembered`] index of the last frame names.
    #[must_use]
    pub fn remembered(&self, index: usize) -> Option<&TileLabel> {
        self.entries.get(index).map(|e| &e.label)
    }

    /// Places this frame's `candidates` in `view` at clock time `now`
    /// (seconds) and returns what to draw. See the module intro for the
    /// rules.
    #[allow(clippy::too_many_lines)] // one pass: match, order, place, remember, draw
    pub fn place(
        &mut self,
        candidates: &[LabelCandidate<'_>],
        view: &LabelView,
        now: f64,
    ) -> LabelFrame {
        self.frame = self.frame.wrapping_add(1);
        let frame = self.frame;
        self.forget(now);
        let world_px = view.world_px();

        // Who is who: each candidate takes the nearest remembered label of
        // its kind and text, in a fixed order (by identity, then place), so
        // the outcome does not depend on the order the tiles came in. The
        // place is the one in the copy of the world the label is drawn in.
        let identities: Vec<u64> = candidates.iter().map(|c| label_identity(c.label)).collect();
        let worlds: Vec<(f64, f64)> = candidates.iter().map(|c| view.drawn_copy(c)).collect();
        let mut by_identity: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
        for (index, entry) in self.entries.iter().enumerate() {
            by_identity.entry(entry.identity).or_default().push(index);
        }
        let mut claimed = alloc::vec![false; self.entries.len()];
        let mut matched: Vec<Option<usize>> = alloc::vec![None; candidates.len()];
        let mut by_place: Vec<usize> = (0..candidates.len()).collect();
        by_place.sort_by(|&a, &b| {
            identities[a]
                .cmp(&identities[b])
                .then_with(|| world_key(worlds[a]).cmp(&world_key(worlds[b])))
        });
        for &c in &by_place {
            let Some(list) = by_identity.get(&identities[c]) else {
                continue;
            };
            let mut best: Option<(f64, usize)> = None;
            for &e in list {
                if claimed[e] {
                    continue;
                }
                let d = world_distance(self.entries[e].world, worlds[c]) * world_px;
                if d <= SAME_LABEL_PX && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, e));
                }
            }
            if let Some((_, e)) = best {
                claimed[e] = true;
                matched[c] = Some(e);
            }
        }

        // The order labels claim space in: rank, then the ones holding a
        // slot, then identity and place - never the order of the tiles.
        let holds = |c: usize| matched[c].is_some_and(|e| self.entries[e].shown);
        let mut order = by_place;
        order.sort_by(|&a, &b| {
            candidates[a]
                .label
                .priority
                .cmp(&candidates[b].label.priority)
                .then_with(|| holds(b).cmp(&holds(a)))
                .then_with(|| identities[a].cmp(&identities[b]))
                .then_with(|| world_key(worlds[a]).cmp(&world_key(worlds[b])))
        });
        let placed = choose(candidates, &order, view);

        // Remember the outcome: a label that gained a slot starts fading in,
        // one that lost it starts fading out.
        let mut entry_of: Vec<Option<usize>> = alloc::vec![None; candidates.len()];
        for c in 0..candidates.len() {
            let index = match matched[c] {
                Some(e) => e,
                // Held back and never shown: nothing to remember.
                None if !placed[c] => continue,
                None => {
                    self.entries.push(Remembered {
                        identity: identities[c],
                        world: worlds[c],
                        label: candidates[c].label.clone(),
                        id: self.next_id,
                        shown: false,
                        from: 0.0,
                        since: now,
                        seen: frame,
                    });
                    self.next_id = self.next_id.wrapping_add(1);
                    self.entries.len() - 1
                }
            };
            let entry = &mut self.entries[index];
            entry.world = worlds[c];
            entry.seen = frame;
            if entry.label != *candidates[c].label {
                entry.label = candidates[c].label.clone();
            }
            entry.set_shown(placed[c], now);
            entry_of[c] = Some(index);
        }
        // A label no tile offers any more (its tile left the view, or was
        // replaced by one that does not carry it) goes too.
        for entry in &mut self.entries {
            if entry.seen != frame {
                entry.set_shown(false, now);
            }
        }

        // What to draw: the fading-out labels under the placed ones.
        let mut out: Vec<PlacedLabel> = Vec::new();
        let mut where_now: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
        for (c, entry) in entry_of.iter().enumerate() {
            if let Some(e) = entry {
                where_now.insert(*e, (candidates[c].x, candidates[c].y));
            }
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.shown {
                continue;
            }
            let opacity = entry.opacity(now);
            if opacity <= 0.0 {
                continue;
            }
            let (x, y) = where_now
                .get(&index)
                .copied()
                .unwrap_or_else(|| view.screen_of(entry.world));
            out.push(PlacedLabel {
                x,
                y,
                opacity,
                id: entry.id,
                from: PlacedFrom::Remembered(index),
            });
        }
        for &c in &order {
            if !placed[c] {
                continue;
            }
            let Some(e) = entry_of[c] else {
                continue;
            };
            let entry = &self.entries[e];
            out.push(PlacedLabel {
                x: candidates[c].x,
                y: candidates[c].y,
                opacity: entry.opacity(now),
                id: entry.id,
                from: PlacedFrom::Candidate(c),
            });
        }
        let settled_at = self
            .entries
            .iter()
            .filter(|e| e.fading(now))
            .map(|e| e.since + LABEL_FADE_SECS)
            .fold(now, f64::max);
        LabelFrame {
            placed: out,
            settled_at,
        }
    }

    /// Drops the labels that are invisible and hold no slot, then the
    /// longest-unseen ones while there are more than [`MAX_REMEMBERED`].
    fn forget(&mut self, now: f64) {
        self.entries.retain(|e| e.shown || e.opacity(now) > 0.0);
        if self.entries.len() > MAX_REMEMBERED {
            self.entries
                .sort_by(|a, b| b.shown.cmp(&a.shown).then_with(|| b.seen.cmp(&a.seen)));
            self.entries.truncate(MAX_REMEMBERED);
        }
    }
}

/// Which candidates get a slot, tried in `order`: none on top of another,
/// none repeated within [`LABEL_REPEAT_PX`], none outside the view (with the
/// view's margin to spare), at most [`MAX_LABELS_PER_FRAME`].
#[allow(clippy::cast_precision_loss)] // label counts and character counts are small
fn choose(candidates: &[LabelCandidate<'_>], order: &[usize], view: &LabelView) -> Vec<bool> {
    let mut placed = alloc::vec![false; candidates.len()];
    let mut taken: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut shown: Vec<(&str, f32, f32)> = Vec::new();
    let mut count = 0usize;
    let margin = view.margin;
    for &index in order {
        if count >= MAX_LABELS_PER_FRAME {
            break;
        }
        let LabelCandidate { x, y, label, .. } = candidates[index];
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        if label.kind == "housenumber" && view.zoom < HOUSENUMBER_MIN_ZOOM {
            continue;
        }
        let (w, h) = label_box(label);
        let (sin, cos) = label.angle.to_radians().sin_cos();
        let (bw, bh) = (
            w * cos.abs() + h * sin.abs(),
            w * sin.abs() + h * cos.abs(),
        );
        let rect = (
            x - bw * 0.5 - 2.0,
            y - bh * 0.5 - 2.0,
            x + bw * 0.5 + 2.0,
            y + bh * 0.5 + 2.0,
        );
        if rect.2 < -margin
            || rect.0 > view.width + margin
            || rect.3 < -margin
            || rect.1 > view.height + margin
        {
            continue;
        }
        if taken
            .iter()
            .any(|o| rect.0 < o.2 && o.0 < rect.2 && rect.1 < o.3 && o.1 < rect.3)
        {
            continue;
        }
        if shown
            .iter()
            .any(|(text, sx, sy)| *text == label.text && (sx - x).hypot(sy - y) < LABEL_REPEAT_PX)
        {
            continue;
        }
        taken.push(rect);
        shown.push((label.text.as_str(), x, y));
        placed[index] = true;
        count += 1;
    }
    placed
}

/// The box a label's text takes: an estimate (no font is measured on the
/// render path), generous so a centred line never overflows to one side.
#[allow(clippy::cast_precision_loss)] // character counts are small
#[must_use]
pub fn label_box(label: &TileLabel) -> (f32, f32) {
    let chars = label.text.chars().count() as f32;
    (chars * label.size * 0.62 + 6.0, label.size * 1.35)
}

/// A label's kind and text as one number (FNV-1a): equal for the same name
/// of the same kind in every tile and on every run.
#[must_use]
pub fn label_identity(label: &TileLabel) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in label
        .kind
        .bytes()
        .chain(core::iter::once(0xff))
        .chain(label.text.bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// A world point as integers, for a total and stable order (north first,
/// then west).
#[allow(clippy::cast_possible_truncation)] // fractions of the world, far inside i64
fn world_key(world: (f64, f64)) -> (i64, i64) {
    const SCALE: f64 = 4_294_967_296.0;
    let q = |v: f64| if v.is_finite() { (v * SCALE).round() as i64 } else { 0 };
    (q(world.1), q(world.0))
}

/// The distance between two world points, in world widths. Both name the
/// copy of the world they are drawn in ([`LabelView::drawn_copy`]), so the
/// same label in two copies is a world width away from itself.
fn world_distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}
