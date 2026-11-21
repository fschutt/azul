//! Which items of a list can reach a rect, without asking each of them.
//!
//! A walk steps over every item of a list and tests each against the clip it paints under
//! (`raster::outside_active_clip`). A view's list is a map of 17,000 items of which a damage
//! rect - a pan's strip, a label's box - reaches a few dozen, and a repaint in bands walks the
//! list once per band. The grid answers "which items can paint into this rect" from the items'
//! bounds, found once for the list.
//!
//! # Levels and children
//!
//! A list is a tree: a push opens a scope, its pop closes it. A LEVEL is the top of the list or
//! the inside of one scope; its CHILDREN are the items directly in it - a leaf (an item that is
//! no push and no pop), or a whole scope from its push to its pop. Every direct child of a
//! level is painted with the same walk state (the same offset, clip, text shadows, masks): the
//! state changes at pushes and is back at their pops. So one rect, found when the walk enters
//! the level, says for all of its children what the walk culls there
//! (`raster::level_cull_rect`).
//!
//! A child has a box in the coordinates the items are stored in:
//!
//! - a leaf: its visual bounds united with its bounds (the walk tests one or the other);
//! - a scope: the union of what is inside, widened by the reach of a text shadow it opens and
//!   united with the box of an opacity group or a mask;
//! - no box (it is ALWAYS visited): a leaf without bounds, a reference frame (the walk steps
//!   over those by their live matrix, `raster::frame_paints_nothing`), a scroll frame (its
//!   content is placed by a live offset), a filter group, and every scope that holds one.
//!
//! A level with more than [`MIN_GRID_CHILDREN`] children has a uniform grid over the boxes of
//! its children. A walk that enters it asks once for the children whose box meets its rect -
//! with those that are always visited, sorted by index, which is painter's order - and steps
//! from one to the next ([`Cursor`]). A child it steps over is a leaf the walk would have
//! culled, or a scope in which it would have culled every leaf: pushes and pops with nothing
//! painted between them leave every pixel as it was (a rounded clip and a mask blend their
//! snapshot back onto itself, an opacity group blends transparent pixels), so the picture is
//! the one the walk that tests every item paints, and the items handed to the painter are the
//! same ones in the same order.
//!
//! What the grid returns is a SUPERSET of what the walk accepts; the walk still makes its own
//! test on each child it visits. The rect is widened by the reach of the text shadows open at
//! the level and by a pixel and a rounding allowance: the walk compares `bounds - offset`
//! with the clip, the grid `bounds` with `clip + offset`.
//!
//! A list that is not well nested (a pop without its push, a push never popped) has no levels:
//! it is walked item by item, as before.

use std::{any::Any, collections::HashMap, sync::Arc};

use azul_core::geom::LogicalRect;

use crate::solver3::display_list::{DisplayList, DisplayListItem, FrameIndex};

/// The fewest children a level must have for a grid: below it the walk's own test per child is
/// as fast as the grid's bookkeeping.
pub(crate) const MIN_GRID_CHILDREN: usize = 256;

/// The most cells a child's box may cover and still be filed in cells; a larger one (a ground,
/// a panel's background) is kept in a list of its own and tested per query.
const MAX_CELLS_PER_CHILD: usize = 16;

/// The most cells along one side of a grid.
const MAX_SIDE: usize = 512;

/// `[x0, y0, x1, y1]`, `x0 <= x1`, `y0 <= y1`, in the coordinates the items are stored in.
pub(crate) type Box4 = [f32; 4];

/// Do the closed boxes `a` and `b` meet (a shared edge counts, as in the walk's own test,
/// `rects_overlap_or_adjacent`)?
fn meet(a: &Box4, b: &Box4) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// What a child can paint, as far as the list says.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Bound {
    /// Nothing (an empty scope).
    Empty,
    Box(Box4),
    /// No box bounds it: always visited.
    Unbounded,
}

impl Bound {
    fn of(r: Option<LogicalRect>) -> Self {
        let Some(r) = r else {
            return Self::Unbounded;
        };
        let (xa, xb) = (r.origin.x, r.origin.x + r.size.width);
        let (ya, yb) = (r.origin.y, r.origin.y + r.size.height);
        if ![xa, xb, ya, yb].iter().all(|v| v.is_finite()) {
            return Self::Unbounded;
        }
        Self::Box([xa.min(xb), ya.min(yb), xa.max(xb), ya.max(yb)])
    }

    const fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unbounded, _) | (_, Self::Unbounded) => Self::Unbounded,
            (Self::Empty, b) | (b, Self::Empty) => b,
            (Self::Box(a), Self::Box(b)) => {
                Self::Box([a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
            }
        }
    }

    fn widened(self, by: f32) -> Self {
        match self {
            Self::Box(b) if by > 0.0 => Self::Box([b[0] - by, b[1] - by, b[2] + by, b[3] + by]),
            // (A reach that is not a number bounds nothing.)
            Self::Box(_) if by.is_nan() => Self::Unbounded,
            other => other,
        }
    }
}

/// The scopes a list opens and closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Clip,
    ImageMask,
    ScrollFrame,
    StackingContext,
    Frame,
    Filter,
    BackdropFilter,
    Opacity,
    TextShadow,
}

/// `Some((scope, true))` for a push, `Some((scope, false))` for a pop, `None` for a leaf: the
/// items of `DisplayListItem::is_state_management`, and no others.
const fn scope_of(item: &DisplayListItem) -> Option<(Scope, bool)> {
    Some(match item {
        DisplayListItem::PushClip { .. } => (Scope::Clip, true),
        DisplayListItem::PopClip => (Scope::Clip, false),
        DisplayListItem::PushImageMaskClip { .. } => (Scope::ImageMask, true),
        DisplayListItem::PopImageMaskClip => (Scope::ImageMask, false),
        DisplayListItem::PushScrollFrame { .. } => (Scope::ScrollFrame, true),
        DisplayListItem::PopScrollFrame => (Scope::ScrollFrame, false),
        DisplayListItem::PushStackingContext { .. } => (Scope::StackingContext, true),
        DisplayListItem::PopStackingContext => (Scope::StackingContext, false),
        DisplayListItem::PushReferenceFrame { .. } => (Scope::Frame, true),
        DisplayListItem::PopReferenceFrame => (Scope::Frame, false),
        DisplayListItem::PushFilter { .. } => (Scope::Filter, true),
        DisplayListItem::PopFilter => (Scope::Filter, false),
        DisplayListItem::PushBackdropFilter { .. } => (Scope::BackdropFilter, true),
        DisplayListItem::PopBackdropFilter => (Scope::BackdropFilter, false),
        DisplayListItem::PushOpacity { .. } => (Scope::Opacity, true),
        DisplayListItem::PopOpacity => (Scope::Opacity, false),
        DisplayListItem::PushTextShadow { .. } => (Scope::TextShadow, true),
        DisplayListItem::PopTextShadow => (Scope::TextShadow, false),
        _ => return None,
    })
}

/// One child of a level while the list is read: where it starts, its box, and whether an image
/// mask is opened in it (a walk that culls by its clip culls nothing under an open mask).
#[derive(Debug, Clone, Copy)]
struct Child {
    start: u32,
    bound: Bound,
    masked: bool,
}

/// One level with a grid.
#[derive(Debug)]
struct Level {
    /// One past the level's last item: the index of the pop that closes it, or the list's
    /// length for the top of the list.
    end: usize,
    /// The children without a box, by start index.
    always: Vec<u32>,
    /// The children with a box in which a mask is opened, by start index: always visited by a
    /// walk that culls by its clip.
    masked: Vec<u32>,
    /// The children whose box covers more than [`MAX_CELLS_PER_CHILD`] cells.
    large: Vec<(u32, Box4)>,
    /// The others; `entries` holds positions in here.
    small: Vec<(u32, Box4)>,
    origin: (f32, f32),
    /// The size of a cell, positive.
    cell: (f32, f32),
    cols: usize,
    rows: usize,
    /// Cell `c` (row-major) holds `entries[cells[c] .. cells[c + 1]]`.
    cells: Vec<u32>,
    entries: Vec<u32>,
}

impl Level {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)] // cell counts
    fn new(end: usize, children: &[Child]) -> Self {
        let mut always = Vec::new();
        let mut masked = Vec::new();
        let mut boxes: Vec<(u32, Box4)> = Vec::new();
        let mut all = Bound::Empty;
        for child in children {
            match child.bound {
                Bound::Empty => {}
                Bound::Unbounded => always.push(child.start),
                Bound::Box(b) => {
                    boxes.push((child.start, b));
                    all = all.union(child.bound);
                    if child.masked {
                        masked.push(child.start);
                    }
                }
            }
        }
        let [x0, y0, x1, y1] = match all {
            Bound::Box(b) => b,
            Bound::Empty | Bound::Unbounded => [0.0, 0.0, 0.0, 0.0],
        };
        let (w, h) = (x1 - x0, y1 - y0);
        // About one child a cell, the cells about square.
        let n = boxes.len().max(1) as f32;
        let cols = if w > 0.0 && h > 0.0 {
            (n * w / h).sqrt().round()
        } else if w > 0.0 {
            n
        } else {
            1.0
        };
        let cols = if cols.is_finite() { (cols as usize).clamp(1, MAX_SIDE) } else { 1 };
        let rows = if h > 0.0 { boxes.len().div_ceil(cols).clamp(1, MAX_SIDE) } else { 1 };
        let side = |extent: f32, count: usize| {
            let s = extent / count as f32;
            if s.is_finite() && s > 0.0 {
                s
            } else {
                1.0
            }
        };
        let mut level = Self {
            end,
            always,
            masked,
            large: Vec::new(),
            small: Vec::new(),
            origin: (x0, y0),
            cell: (side(w, cols), side(h, rows)),
            cols,
            rows,
            cells: vec![0; cols * rows + 1],
            entries: Vec::new(),
        };
        // Two passes: how many children each cell holds, then the children.
        let mut ranges: Vec<[usize; 4]> = Vec::new();
        for (start, b) in boxes {
            let r = level.cell_range(&b);
            let covered = (r[2] - r[0] + 1) * (r[3] - r[1] + 1);
            if covered > MAX_CELLS_PER_CHILD {
                level.large.push((start, b));
                continue;
            }
            level.small.push((start, b));
            ranges.push(r);
            for cy in r[1]..=r[3] {
                for cx in r[0]..=r[2] {
                    level.cells[cy * cols + cx + 1] += 1;
                }
            }
        }
        for c in 1..level.cells.len() {
            level.cells[c] += level.cells[c - 1];
        }
        level.entries = vec![0; level.cells[cols * rows] as usize];
        let mut filled: Vec<u32> = level.cells[..cols * rows].to_vec();
        for (at, r) in ranges.iter().enumerate() {
            for cy in r[1]..=r[3] {
                for cx in r[0]..=r[2] {
                    let slot = &mut filled[cy * cols + cx];
                    level.entries[*slot as usize] = at as u32;
                    *slot += 1;
                }
            }
        }
        level
    }

    /// The cells `[cx0, cy0, cx1, cy1]` (inclusive) the box `b` covers, clamped to the grid.
    /// Monotone in every edge of `b`: two boxes that meet cover a cell in common.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)] // clamped below
    fn cell_range(&self, b: &Box4) -> [usize; 4] {
        let along = |v: f32, origin: f32, cell: f32, count: usize| -> usize {
            let at = ((v - origin) / cell).floor();
            if at.is_nan() || at <= 0.0 {
                0
            } else if at >= (count - 1) as f32 {
                count - 1
            } else {
                at as usize
            }
        };
        [
            along(b[0], self.origin.0, self.cell.0, self.cols),
            along(b[1], self.origin.1, self.cell.1, self.rows),
            along(b[2], self.origin.0, self.cell.0, self.cols),
            along(b[3], self.origin.1, self.cell.1, self.rows),
        ]
    }

    /// The start indices of the children that can paint into the box `q`, ascending: those
    /// whose box meets it, and those without a box (with `by_clip`, also those in which a mask
    /// is opened). `None` when `q` covers at least half of the grid's cells: the walk's own
    /// test of every child costs no more than the answer.
    fn children_meeting(&self, q: &Box4, by_clip: bool) -> Option<Vec<u32>> {
        let r = self.cell_range(q);
        let covered = (r[2] - r[0] + 1) * (r[3] - r[1] + 1);
        if covered * 2 >= self.cols * self.rows && self.cols * self.rows > 1 {
            return None;
        }
        let mut out = self.always.clone();
        if by_clip {
            out.extend_from_slice(&self.masked);
        }
        out.extend(self.large.iter().filter(|(_, b)| meet(b, q)).map(|(start, _)| *start));
        for cy in r[1]..=r[3] {
            for cx in r[0]..=r[2] {
                let c = cy * self.cols + cx;
                for at in &self.entries[self.cells[c] as usize..self.cells[c + 1] as usize] {
                    let (start, b) = &self.small[*at as usize];
                    if meet(b, q) {
                        out.push(*start);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        Some(out)
    }
}

/// The levels of one list that have a grid. Made once for a list (kept in the list's
/// `FrameIndex`, so it lives exactly as long as the items it describes) and read by every
/// pass, on every band thread.
#[derive(Debug, Default)]
pub(crate) struct ItemGrid {
    /// Per item, the index of the last item of the child that starts there: the item itself,
    /// or its pop for a push. Empty for a list that is not well nested.
    end_of: Vec<u32>,
    levels: Vec<Level>,
    /// The level inside the scope pushed at `key - 1`; `0` is the top of the list.
    by_scope: HashMap<usize, usize>,
}

/// A scope that is open while the list is read.
struct Open {
    push: usize,
    scope: Scope,
    children: Vec<Child>,
    content: Bound,
    masked: bool,
}

impl ItemGrid {
    /// [`Self::of_with`] with every item's own visual bounds (a text run's estimate).
    pub(crate) fn of(items: &[DisplayListItem]) -> Self {
        Self::of_with(items, |index| items[index].visual_bounds())
    }

    /// Read `items` once: the nesting, the box of every child, a grid for every level that is
    /// worth one. `visual(i)` is the visual bounds of item `i` as the walk will ask for them
    /// (`DisplayList::visual_bounds_of`: a text run's ink where the list knows it).
    #[allow(clippy::cast_possible_truncation)] // a list has far fewer than 2^32 items
    pub(crate) fn of_with(
        items: &[DisplayListItem],
        visual: impl Fn(usize) -> Option<LogicalRect>,
    ) -> Self {
        if items.len() >= u32::MAX as usize {
            return Self::default();
        }
        let mut grid = Self { end_of: (0..items.len() as u32).collect(), ..Self::default() };
        let mut open: Vec<Open> = vec![Open {
            push: usize::MAX,
            scope: Scope::StackingContext,
            children: Vec::new(),
            content: Bound::Empty,
            masked: false,
        }];
        for (i, item) in items.iter().enumerate() {
            match scope_of(item) {
                None => {
                    // The walk tests an item's visual bounds, or its bounds where it has none
                    // or a text shadow is open: both.
                    let bound = match (visual(i), item.bounds()) {
                        (None, None) => Bound::Unbounded,
                        (Some(a), None) | (None, Some(a)) => Bound::of(Some(a)),
                        (Some(a), Some(b)) => Bound::of(Some(a)).union(Bound::of(Some(b))),
                    };
                    if let Some(level) = open.last_mut() {
                        level.children.push(Child { start: i as u32, bound, masked: false });
                        level.content = level.content.union(bound);
                    }
                }
                Some((scope, true)) => open.push(Open {
                    push: i,
                    scope,
                    children: Vec::new(),
                    content: Bound::Empty,
                    masked: matches!(scope, Scope::ImageMask),
                }),
                Some((scope, false)) => {
                    // The innermost open scope, of this kind: else the list is not a tree.
                    if open.len() < 2 || open.last().is_none_or(|o| o.scope != scope) {
                        return Self::default();
                    }
                    let Some(done) = open.pop() else {
                        return Self::default();
                    };
                    grid.end_of[done.push] = i as u32;
                    let own = Bound::of(items[done.push].bounds());
                    let bound = match (scope, &items[done.push]) {
                        (Scope::Clip | Scope::StackingContext, _) => done.content,
                        (Scope::TextShadow, DisplayListItem::PushTextShadow { shadow }) => done
                            .content
                            .widened(crate::solver3::display_list::text_shadow_reach(shadow)),
                        (Scope::Opacity | Scope::ImageMask, _) => done.content.union(own),
                        _ => Bound::Unbounded,
                    };
                    if done.children.len() > MIN_GRID_CHILDREN {
                        grid.by_scope.insert(done.push + 1, grid.levels.len());
                        grid.levels.push(Level::new(i, &done.children));
                    }
                    if let Some(level) = open.last_mut() {
                        level.children.push(Child { start: done.push as u32, bound, masked: done.masked });
                        level.content = level.content.union(bound);
                        level.masked |= done.masked;
                    }
                }
            }
        }
        // Only the top of the list is left open in a list that is a tree.
        let (Some(top), true) = (open.pop(), open.is_empty()) else {
            return Self::default();
        };
        if top.children.len() > MIN_GRID_CHILDREN {
            grid.by_scope.insert(0, grid.levels.len());
            grid.levels.push(Level::new(items.len(), &top.children));
        }
        grid
    }

    /// The grid of `list`, made on the first ask and kept in the list's `index`.
    pub(crate) fn of_list(list: &DisplayList, index: &FrameIndex) -> Option<Arc<Self>> {
        let kept = index.derived.get_or_init(|| {
            let grid: Arc<dyn Any + Send + Sync> =
                Arc::new(Self::of_with(&list.items, |index| list.visual_bounds_of(index)));
            grid
        });
        Arc::clone(kept).downcast::<Self>().ok()
    }

    /// Whether any level of the list has a grid.
    pub(crate) const fn has_levels(&self) -> bool {
        !self.levels.is_empty()
    }

    /// Whether the inside of the scope pushed at `push` is a level with a grid.
    pub(crate) fn has_level_inside(&self, push: usize) -> bool {
        self.by_scope.contains_key(&(push + 1))
    }

    /// A cursor over the level a walk of the items `start .. end` begins in - the top of the
    /// list for a walk of all of it, the inside of the scope pushed at `start - 1` for a walk
    /// of exactly that - or `None` when that level has no grid, or the walk culls nothing
    /// there (`rect`: `raster::level_cull_rect`).
    pub(crate) fn cursor_for_walk(
        &self,
        start: usize,
        end: usize,
        rect: Option<(Box4, bool)>,
    ) -> Option<Cursor> {
        let level = &self.levels[*self.by_scope.get(&start)?];
        if level.end != end {
            return None;
        }
        self.cursor(level, start, rect)
    }

    /// A cursor over the inside of the scope whose push at `push` the walk just made, as
    /// [`Self::cursor_for_walk`].
    pub(crate) fn cursor_inside(&self, push: usize, rect: Option<(Box4, bool)>) -> Option<Cursor> {
        let level = &self.levels[*self.by_scope.get(&(push + 1))?];
        self.cursor(level, push + 1, rect)
    }

    // a method like its siblings: the cursor belongs to the grid
    #[allow(clippy::unused_self)]
    fn cursor(&self, level: &Level, first: usize, rect: Option<(Box4, bool)>) -> Option<Cursor> {
        let (q, by_clip) = rect?;
        Some(Cursor {
            end: level.end,
            starts: level.children_meeting(&q, by_clip)?,
            at: 0,
            resume_at: first,
        })
    }

    /// The index of the last item of the child that starts at `at`.
    fn end_of(&self, at: usize) -> usize {
        self.end_of.get(at).map_or(at, |end| *end as usize)
    }
}

/// Where a walk is in one level with a grid: the children that can paint under its cull there
/// and the next of them.
#[derive(Debug)]
pub(crate) struct Cursor {
    /// One past the level's last item.
    pub(crate) end: usize,
    /// The children the walk visits, by start index, ascending.
    starts: Vec<u32>,
    at: usize,
    /// The first index at which the walk is between children of the level again: while it is
    /// inside a child (walking a scope it entered), the cursor has nothing to say.
    resume_at: usize,
}

impl Cursor {
    /// Where the walk goes on when it is about to take the item `next` (inside the level,
    /// `next < self.end`): `next` itself while the walk is inside a child, or at the start of
    /// a child it visits; otherwise the start of the next child it visits, or the level's end.
    /// Never less than `next`.
    pub(crate) fn step(&mut self, next: usize, grid: &ItemGrid) -> usize {
        if next < self.resume_at {
            return next;
        }
        while self.starts.get(self.at).is_some_and(|start| (*start as usize) < next) {
            self.at += 1;
        }
        match self.starts.get(self.at) {
            Some(start) if *start as usize == next => {
                self.resume_at = grid.end_of(next) + 1;
                next
            }
            Some(start) => *start as usize,
            None => self.end,
        }
    }
}

/// Is the grid used (the default), or does every walk test every item (`AZ_CPU_GRID=off`: the
/// walk before, for comparing the two)?
pub(crate) fn grid_is_used() -> bool {
    static USED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *USED.get_or_init(|| std::env::var("AZ_CPU_GRID").map_or(true, |v| v != "off" && v != "0"))
}

#[cfg(test)]
mod tests {
    use azul_core::geom::{LogicalPosition, LogicalSize};
    use azul_css::props::basic::ColorU;

    use super::*;
    use crate::solver3::display_list::BorderRadius;

    fn fill(x: f32, y: f32, w: f32, h: f32) -> DisplayListItem {
        DisplayListItem::Rect {
            bounds: LogicalRect {
                origin: LogicalPosition { x, y },
                size: LogicalSize { width: w, height: h },
            }
            .into(),
            color: ColorU { r: 1, g: 2, b: 3, a: 255 },
            border_radius: BorderRadius::default(),
        }
    }

    fn clip(x: f32, y: f32, w: f32, h: f32) -> DisplayListItem {
        DisplayListItem::PushClip {
            bounds: LogicalRect {
                origin: LogicalPosition { x, y },
                size: LogicalSize { width: w, height: h },
            }
            .into(),
            border_radius: BorderRadius::default(),
        }
    }

    /// A small generator of numbers (a fixed sequence per seed).
    struct Numbers(u64);

    impl Numbers {
        fn next(&mut self, below: u32) -> f32 {
            self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 33) % u64::from(below)) as f32
        }
    }

    /// `n` rows of a document, 20 px apart: a row is a fill 800 px wide, or (every fifth) a
    /// clip around three fills that end at x = 770. With the list: where each child of its top
    /// level starts, and the child's box.
    fn document(n: usize) -> (Vec<DisplayListItem>, Vec<(usize, Box4)>) {
        let mut items = Vec::new();
        let mut children = Vec::new();
        for row in 0..n {
            let y = row as f32 * 20.0;
            let right = if row % 5 == 4 { 2.0 * 260.0 + 250.0 } else { 800.0 };
            children.push((items.len(), [0.0, y, right, y + 18.0]));
            if row % 5 == 4 {
                items.push(clip(0.0, y, 800.0, 18.0));
                for part in 0..3 {
                    items.push(fill(part as f32 * 260.0, y, 250.0, 18.0));
                }
                items.push(DisplayListItem::PopClip);
            } else {
                items.push(fill(0.0, y, 800.0, 18.0));
            }
        }
        (items, children)
    }

    /// A1-07b: what the grid of a level answers is, for every rect, at least the children
    /// whose box meets the rect, in ascending order and none twice; and for a small rect, few
    /// more. A document of 2,000 rows (2,000 children at the top, 400 of them clips around
    /// three fills) and 200 rects from a fixed sequence: every child a search of all of them
    /// finds is among the grid's, and the grid's answer for a rect of 100 x 40 px is at most
    /// 16 children whatever the document's length.
    /// Mutations: a cell range that is not clamped at the far edge loses the last row; a
    /// query that reads the cells of `q`'s near corner only loses every child the rect
    /// reaches further on.
    #[test]
    fn the_grid_of_a_level_finds_every_child_that_meets_a_rect() {
        let (items, children) = document(2000);
        let grid = ItemGrid::of(&items);
        assert!(grid.has_levels());
        assert_eq!(grid.levels.len(), 1, "the clips' insides are three fills: no grid of their own");
        let level = &grid.levels[0];
        assert_eq!(level.end, items.len());
        assert!(level.always.is_empty() && level.masked.is_empty());
        assert_eq!(level.small.len() + level.large.len(), 2000);
        let mut numbers = Numbers(7);
        let mut answered = 0;
        for _ in 0..200 {
            let (x, y) = (numbers.next(900) - 50.0, numbers.next(41_000) - 500.0);
            let (w, h) = (numbers.next(300) + 1.0, numbers.next(200) + 1.0);
            let q = [x, y, x + w, y + h];
            let Some(found) = level.children_meeting(&q, false) else {
                continue;
            };
            answered += 1;
            assert!(found.windows(2).all(|pair| pair[0] < pair[1]), "ascending, none twice");
            let wanted: Vec<u32> =
                children.iter().filter(|(_, b)| meet(b, &q)).map(|(start, _)| *start as u32).collect();
            assert!(wanted.iter().all(|start| found.contains(start)), "{q:?}: {wanted:?} within {found:?}");
            assert!(found.len() <= wanted.len() + 16, "{q:?}: {} for {}", found.len(), wanted.len());
        }
        assert!(answered > 150, "most rects are small against the document ({answered})");
        // The whole document: not worth an answer.
        assert!(level.children_meeting(&[0.0, 0.0, 800.0, 40_000.0], false).is_none());
        // A fixed small rect, whatever the length.
        for n in [500usize, 2000, 8000] {
            let grid = ItemGrid::of(&document(n).0);
            let found = grid.levels[0].children_meeting(&[0.0, 100.0, 100.0, 140.0], false).expect("a small rect");
            assert!(found.len() >= 3 && found.len() <= 16, "n = {n}: {}", found.len());
        }
    }

    /// A1-07b: the nesting. In a list of 300 clips, each around a fill: the child that starts
    /// at a push ends at its pop, a leaf at itself; a walk between children is sent to the
    /// next child that meets the rect or to the level's end, a walk inside a child is left
    /// alone, and a child the cursor hands out is not handed out again. A list with a pop too
    /// many, or a push never popped, has no levels.
    /// Mutations: `resume_at` left at the child's start sends a walk that is inside a clip
    /// (at its fill) on to the next child, past the clip's pop; `end_of` of a push left at
    /// the push does the same.
    #[test]
    fn a_cursor_steps_between_children_and_never_out_of_one() {
        let mut items = Vec::new();
        for row in 0..300 {
            let y = row as f32 * 20.0;
            items.push(clip(0.0, y, 100.0, 18.0));
            items.push(fill(0.0, y, 100.0, 18.0));
            items.push(DisplayListItem::PopClip);
        }
        let grid = ItemGrid::of(&items);
        assert_eq!((grid.end_of(0), grid.end_of(1), grid.end_of(2), grid.end_of(3)), (2, 1, 2, 5));
        // Rows 10 and 11 (y 200 .. 238) meet the rect y 205 .. 225; with the cells' reach a
        // few more children may be handed out, but row 10 is the first of them all.
        let rect = Some(([0.0, 205.0, 50.0, 225.0], false));
        let mut cursor = grid.cursor_for_walk(0, items.len(), rect).expect("the top of the list has a grid");
        assert_eq!(cursor.end, items.len());
        assert_eq!(cursor.step(0, &grid), 30, "the walk at the list's start is sent to row 10");
        assert_eq!(cursor.step(30, &grid), 30, "which it visits");
        assert_eq!((cursor.step(31, &grid), cursor.step(32, &grid)), (31, 32), "inside the clip it is left alone");
        assert_eq!(cursor.step(33, &grid), 33, "row 11 meets the rect too");
        assert_eq!((cursor.step(34, &grid), cursor.step(35, &grid)), (34, 35));
        assert_eq!(cursor.step(36, &grid), items.len(), "nothing further on: the level's end");
        // No rect (the walk culls nothing), a walk of a part of the list: no cursor.
        assert!(grid.cursor_for_walk(0, items.len(), None).is_none());
        assert!(grid.cursor_for_walk(0, 30, rect).is_none());
        assert!(grid.cursor_inside(0, rect).is_none(), "one fill in the clip: no grid");
        // Not a tree.
        let mut one_pop_more = items.clone();
        one_pop_more.push(DisplayListItem::PopClip);
        assert!(!ItemGrid::of(&one_pop_more).has_levels());
        let mut never_popped = items.clone();
        never_popped.insert(0, clip(0.0, 0.0, 10.0, 10.0));
        assert!(!ItemGrid::of(&never_popped).has_levels());
        let mut crossed = items;
        crossed.push(clip(0.0, 0.0, 10.0, 10.0));
        crossed.push(DisplayListItem::PushStackingContext {
            z_index: 0,
            bounds: LogicalRect {
                origin: LogicalPosition { x: 0.0, y: 0.0 },
                size: LogicalSize { width: 1.0, height: 1.0 },
            }
            .into(),
        });
        crossed.push(DisplayListItem::PopClip);
        crossed.push(DisplayListItem::PopStackingContext);
        assert!(!ItemGrid::of(&crossed).has_levels(), "a pop that closes another scope than the innermost");
    }

    /// A1-07b: which children have a box. In a level of 300 fills: a reference frame and a
    /// scroll frame are always visited (their content is placed by live values), so is a clip
    /// that holds one; an opacity group's box is its content united with its own box; a text
    /// shadow widens its scope by its reach; a scope with nothing in it is never visited; a
    /// mask's scope is visited by a walk that culls by its clip, whatever the rect.
    /// Mutation: a scroll frame given the box of its content is stepped over at an offset
    /// that scrolls the content into the rect.
    #[test]
    fn a_child_without_a_box_is_always_visited() {
        use azul_css::props::basic::pixel::{PixelValue, PixelValueNoPercent};
        let wrect = |x: f32, y: f32, w: f32, h: f32| -> crate::solver3::display_list::WindowLogicalRect {
            LogicalRect { origin: LogicalPosition { x, y }, size: LogicalSize { width: w, height: h } }.into()
        };
        let mut items: Vec<DisplayListItem> = (0..300).map(|row| fill(0.0, row as f32 * 20.0, 100.0, 18.0)).collect();
        let far = 4000.0;
        // 300: a frame, 303: a scroll frame, 306: a clip around a scroll frame.
        items.push(DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: 1 },
            initial_transform: azul_core::transform::ComputedTransform3D::IDENTITY,
            bounds: wrect(0.0, far, 10.0, 10.0),
        });
        items.push(fill(0.0, far, 10.0, 10.0));
        items.push(DisplayListItem::PopReferenceFrame);
        items.push(DisplayListItem::PushScrollFrame {
            clip_bounds: wrect(0.0, far, 10.0, 10.0),
            content_size: LogicalSize { width: 10.0, height: 10.0 },
            scroll_id: 1,
        });
        items.push(fill(0.0, far, 10.0, 10.0));
        items.push(DisplayListItem::PopScrollFrame);
        items.push(clip(0.0, far, 10.0, 10.0));
        items.push(DisplayListItem::PushScrollFrame {
            clip_bounds: wrect(0.0, far, 10.0, 10.0),
            content_size: LogicalSize { width: 10.0, height: 10.0 },
            scroll_id: 2,
        });
        items.push(DisplayListItem::PopScrollFrame);
        items.push(DisplayListItem::PopClip);
        // 310: an opacity group at (0, 5000) 10 x 10 around a fill at (50, 5000).
        let group = items.len();
        items.push(DisplayListItem::PushOpacity {
            bounds: wrect(0.0, 5000.0, 10.0, 10.0),
            opacity: 0.5,
            opacity_key: None,
            mix_blend_mode: Default::default(),
        });
        items.push(fill(50.0, 5000.0, 10.0, 10.0));
        items.push(DisplayListItem::PopOpacity);
        // A text shadow of reach 2 + 3 = 5 around a fill at (0, 6000) 10 x 10.
        let shadowed = items.len();
        let length = |v: f32| PixelValueNoPercent { inner: PixelValue::px(v) };
        items.push(DisplayListItem::PushTextShadow {
            shadow: azul_css::props::style::box_shadow::StyleBoxShadow {
                offset_x: length(0.0),
                offset_y: length(0.0),
                blur_radius: length(2.0),
                spread_radius: length(3.0),
                color: ColorU { r: 0, g: 0, b: 0, a: 255 },
                clip_mode: azul_css::props::style::box_shadow::BoxShadowClipMode::Outset,
            },
        });
        items.push(fill(0.0, 6000.0, 10.0, 10.0));
        items.push(DisplayListItem::PopTextShadow);
        // An empty clip, and a mask around a fill at (0, 7000).
        let empty = items.len();
        items.push(clip(0.0, 0.0, 100.0, 100.0));
        items.push(DisplayListItem::PopClip);
        let mask = items.len();
        items.push(DisplayListItem::PushImageMaskClip {
            bounds: wrect(0.0, 7000.0, 10.0, 10.0),
            mask_image: None,
            mask_rect: wrect(0.0, 7000.0, 10.0, 10.0),
            geometry: None,
        });
        items.push(fill(0.0, 7000.0, 10.0, 10.0));
        items.push(DisplayListItem::PopImageMaskClip);
        let grid = ItemGrid::of(&items);
        let level = &grid.levels[0];
        assert_eq!(level.always, vec![300, 303, 306]);
        assert_eq!(level.masked, vec![mask as u32]);
        let at = |q: Box4, by_clip: bool| level.children_meeting(&q, by_clip).expect("a small rect");
        // Far from everything: only the children without a box (and, by clip, the mask).
        assert_eq!(at([900.0, 900.0, 910.0, 910.0], false), vec![300, 303, 306]);
        assert_eq!(at([900.0, 900.0, 910.0, 910.0], true), vec![300, 303, 306, mask as u32]);
        // The group: its own box, and its content outside that box.
        assert!(at([0.0, 5000.0, 5.0, 5005.0], false).contains(&(group as u32)));
        assert!(at([55.0, 5000.0, 58.0, 5005.0], false).contains(&(group as u32)));
        assert!(!at([70.0, 5000.0, 75.0, 5005.0], false).contains(&(group as u32)));
        // The shadow's scope reaches 5 px past its fill (x up to 15), not 6.
        assert!(at([14.5, 6000.0, 20.0, 6005.0], false).contains(&(shadowed as u32)));
        assert!(!at([15.5, 6000.0, 20.0, 6005.0], false).contains(&(shadowed as u32)));
        // The empty clip is never visited, although the rect lies in its bounds.
        assert!(!at([0.0, 0.0, 5.0, 5.0], false).contains(&(empty as u32)));
        assert!(at([0.0, 0.0, 5.0, 5.0], false).contains(&0));
    }
}
