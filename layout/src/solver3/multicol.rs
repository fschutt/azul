//! CSS Multi-column Layout 1: the columns of a multi-column container.
//!
//! A box with a non-`auto` `column-count` or `column-width` is a
//! multi-column container. Its content is laid out at the column width and
//! fills the columns in order:
//!
//! - an inline formatting context ROOT with columns (text directly in the container) is split by
//!   text3 itself (`UnifiedConstraints::columns`), line by line;
//! - a block container (`layout_bfc`) lays its children out ONCE as one column of the column width,
//!   then [`plan_columns`] cuts that single column into the container's columns - between two
//!   siblings, or between two lines of an inline formatting context child - and the children move
//!   into place. It is azul's pagination model (one continuous layout, then the break analysis),
//!   with the column boxes for pages.
//!
//! [`column_style`] is THE reader of a box's column declarations and
//! [`ColumnStyle::geometry`] THE resolution of the column count and width
//! (§3.4), for both paths.

use azul_core::{
    dom::NodeId,
    geom::LogicalSize,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    compact_cache::{DOM_HAS_COLUMN_COUNT, DOM_HAS_COLUMN_GAP, DOM_HAS_COLUMN_WIDTH},
    props::{
        basic::{PhysicalSize, PixelValue, PropertyContext, ResolutionContext},
        layout::{ColumnCount, ColumnFill, ColumnWidth},
    },
};

use crate::solver3::getters::{get_element_font_size, get_parent_font_size, get_root_font_size};

/// The column declarations of a multi-column container, lengths resolved
/// to px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnStyle {
    /// `column-count`; `None` = `auto`.
    pub count: Option<u32>,
    /// `column-width`; `None` = `auto`.
    pub width: Option<f32>,
    /// `column-gap`; `normal` is 1em.
    pub gap: f32,
    /// `column-fill`.
    pub fill: ColumnFill,
}

/// The columns of a multi-column container for one available width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnGeometry {
    /// How many columns (`N`, at least 1).
    pub count: u32,
    /// The width of every column box (`W`).
    pub width: f32,
    /// The space between two columns.
    pub gap: f32,
}

impl ColumnStyle {
    /// CSS Multicol 1 §3.4, the pseudo-algorithm: the column count `N` and
    /// width `W` in an available width `U`.
    ///
    /// - `column-count` alone: `N` columns sharing `U` minus the gaps.
    /// - `column-width` alone: as many columns of at least that width as fit, `W` stretched to fill
    ///   `U`.
    /// - both: `column-count` is the maximum.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )] // a column count: small, positive, floored
    pub fn geometry(&self, available: f32) -> ColumnGeometry {
        let gap = self.gap.max(0.0);
        let available = available.max(0.0);
        let fitting = self.width.filter(|w| *w > 0.0).map(|w| {
            let n = ((available + gap) / (w + gap)).floor();
            if n.is_finite() && n >= 1.0 {
                n.min(u32::MAX as f32) as u32
            } else {
                1
            }
        });
        let count = match (self.count, fitting) {
            (Some(c), Some(f)) => c.max(1).min(f),
            (Some(c), None) => c.max(1),
            (None, Some(f)) => f,
            (None, None) => 1,
        };
        let n = count as f32;
        ColumnGeometry {
            count,
            width: ((available - gap * (n - 1.0)) / n).max(0.0),
            gap,
        }
    }
}

impl ColumnGeometry {
    /// The inline offset of column `k` from the container's content-box
    /// left edge, for a container `content_width` wide. Columns run in the
    /// inline base direction: left to right, or right to left under
    /// `direction: rtl`. Overflow columns (`k >= count`) continue the same
    /// way, past the container's edge.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // a column index
    pub fn column_x(&self, k: usize, content_width: f32, rtl: bool) -> f32 {
        let advance = (self.width + self.gap) * k as f32;
        if rtl {
            content_width - self.width - advance
        } else {
            advance
        }
    }

    /// The inline distance from one column to the next (negative under
    /// `direction: rtl`).
    #[must_use]
    pub fn advance(&self, rtl: bool) -> f32 {
        let a = self.width + self.gap;
        if rtl {
            -a
        } else {
            a
        }
    }
}

/// Whether ANY box of `styled_dom` declares `column-count` or
/// `column-width` - the compact cache's DOM-wide bits, so a document
/// without columns (nearly every one) never walks the cascade for them.
/// Without a compact cache the answer is a conservative `true`.
#[must_use]
pub fn dom_declares_columns(styled_dom: &StyledDom) -> bool {
    dom_declared_flags(styled_dom) & (DOM_HAS_COLUMN_COUNT | DOM_HAS_COLUMN_WIDTH) != 0
}

fn dom_declared_flags(styled_dom: &StyledDom) -> u32 {
    styled_dom
        .css_property_cache
        .ptr
        .compact_cache
        .as_ref()
        .map_or(!0u32, |cc| cc.dom_declared_flags)
}

/// The declared `column-count` and (unresolved) `column-width` of `dom_id`
/// when it is a multi-column container - at least one of them not `auto`.
fn declared_columns(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<(Option<u32>, Option<PixelValue>)> {
    let declared = dom_declared_flags(styled_dom);
    if declared & (DOM_HAS_COLUMN_COUNT | DOM_HAS_COLUMN_WIDTH) == 0 {
        return None;
    }
    let cache = &styled_dom.css_property_cache.ptr;
    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let count = if declared & DOM_HAS_COLUMN_COUNT == 0 {
        None
    } else {
        match cache
            .get_column_count(node_data, &dom_id, node_state)
            .and_then(|v| v.get_property())
        {
            Some(ColumnCount::Integer(n)) => Some(*n),
            Some(ColumnCount::Auto) | None => None,
        }
    };
    let width = if declared & DOM_HAS_COLUMN_WIDTH == 0 {
        None
    } else {
        match cache
            .get_column_width(node_data, &dom_id, node_state)
            .and_then(|v| v.get_property())
        {
            Some(ColumnWidth::Length(px)) => Some(*px),
            Some(ColumnWidth::Auto) | None => None,
        }
    };
    (count.is_some() || width.is_some()).then_some((count, width))
}

/// Whether `dom_id` is a multi-column container (a non-`auto`
/// `column-count` or `column-width`) - without resolving any length, for
/// the hot paths that only need the yes or no.
#[must_use]
pub fn is_multicol_container(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    node_state: &StyledNodeState,
) -> bool {
    declared_columns(styled_dom, dom_id, node_state).is_some()
}

/// [`is_multicol_container`] for a layout box: an anonymous box (`None`)
/// never is one.
#[must_use]
pub fn is_multicol_box(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> bool {
    dom_id.is_some_and(|id| {
        styled_dom
            .styled_nodes
            .as_container()
            .get(id)
            .is_some_and(|n| is_multicol_container(styled_dom, id, &n.styled_node_state))
    })
}

/// The column declarations of `dom_id` when it is a multi-column container
/// (a non-`auto` `column-count` or `column-width`), else `None`.
///
/// `column-count`, `column-width` and `column-gap` are not inherited: they
/// are read from `dom_id` alone. An anonymous box has none of its own -
/// its caller must not ask on its parent's behalf.
#[must_use]
pub fn column_style(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    node_state: &StyledNodeState,
    viewport_size: LogicalSize,
) -> Option<ColumnStyle> {
    let (count, declared_width) = declared_columns(styled_dom, dom_id, node_state)?;
    let declared = dom_declared_flags(styled_dom);
    let cache = &styled_dom.css_property_cache.ptr;
    let node_data = &styled_dom.node_data.as_container()[dom_id];

    let resolve_ctx = ResolutionContext {
        vertical_writing_mode: false,
        element_font_size: get_element_font_size(styled_dom, dom_id, node_state),
        parent_font_size: get_parent_font_size(styled_dom, dom_id, node_state),
        root_font_size: get_root_font_size(styled_dom, node_state),
        containing_block_size: PhysicalSize::new(0.0, 0.0),
        element_size: None,
        viewport_size: PhysicalSize::new(viewport_size.width, viewport_size.height),
    };
    let width =
        declared_width.map(|px| px.resolve_with_context(&resolve_ctx, PropertyContext::Other));
    // `column-gap: normal` is 1em in a multi-column container.
    let declared_gap = if declared & DOM_HAS_COLUMN_GAP == 0 {
        None
    } else {
        cache
            .get_column_gap(node_data, &dom_id, node_state)
            .and_then(|v| v.get_property())
            .map(|g| {
                g.inner
                    .resolve_with_context(&resolve_ctx, PropertyContext::Other)
            })
    };
    let gap = declared_gap.unwrap_or(resolve_ctx.element_font_size);
    let fill = cache
        .get_column_fill(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property())
        .copied()
        .unwrap_or_default();

    Some(ColumnStyle {
        count,
        width,
        gap,
        fill,
    })
}

// ---------------------------------------------------------------------------
// The column breaks of a multi-column block container
// ---------------------------------------------------------------------------

/// One in-flow child of a multi-column block container, as laid out in the
/// single column of the column width (block-axis offsets from the
/// container's content-box top).
#[derive(Debug, Clone, PartialEq)]
pub struct FlowBox {
    /// Its border-box top (its top margin above it).
    pub top: f32,
    /// Its border-box bottom.
    pub bottom: f32,
    /// The lines of a box that may continue in the next column between two
    /// of them (a plain inline formatting context), in order; empty for a
    /// box that moves to the next column whole.
    pub lines: Vec<FlowLine>,
    /// The heights of the footnotes anchored in the box other than in one
    /// of `lines` (`float: footnote`, paged media): they stand at the
    /// bottom of the page the box's top lands on, and take their room from
    /// its columns ([`plan_paged_columns`]). 0 without footnotes.
    pub notes: f32,
}

/// One line of a [`FlowBox`]: the extent of its content in the flow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowLine {
    /// The line's index in its inline formatting context.
    pub index: usize,
    /// The top of the line's content.
    pub top: f32,
    /// The bottom of the line's content.
    pub bottom: f32,
    /// The heights of the footnotes anchored in the line ([`FlowBox::notes`]).
    pub notes: f32,
}

/// Where one [`FlowBox`] goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoxPlacement {
    /// The column the box starts in.
    pub column: usize,
    /// The line indices at which the box continues at the top of the next
    /// columns, ascending; empty when it sits in one column.
    pub line_breaks: Vec<usize>,
    /// The column each of `line_breaks` continues in: the next one - or, on
    /// paged media, the first column of the next row when the row ended
    /// before its last column (before a piece whose footnote did not fit,
    /// [`plan_paged_columns`]).
    pub break_columns: Vec<usize>,
}

/// The columns a multi-column block container's flow is cut into.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnPlan {
    /// Where on the single-column flow each column starts (the first at 0):
    /// content at flow offset `y` in column `k` sits `y - starts[k]` below
    /// the column's top.
    pub starts: Vec<f32>,
    /// The tallest column's content: the container's content height.
    pub height: f32,
    /// Where each [`FlowBox`] goes, in the order given.
    pub boxes: Vec<BoxPlacement>,
    /// Paged media ([`plan_paged_columns`]): how many columns stand side by
    /// side before the next one starts a new ROW, a page further down. `None`
    /// = one row, the columns beyond the count running on to the side.
    pub lanes: Option<usize>,
    /// The top of every row, from the container's content-box top: `[0]`
    /// for the one row of [`plan_columns`].
    pub row_tops: Vec<f32>,
    /// The heights of the footnotes anchored in every row
    /// ([`plan_paged_columns`]); `[0]` for the one row of [`plan_columns`],
    /// which is not told about footnotes.
    pub row_notes: Vec<f32>,
}

impl ColumnPlan {
    /// The place across the container column `k` takes: its own index, or
    /// on paged media its place in its row.
    #[must_use]
    pub fn lane(&self, k: usize) -> usize {
        self.lanes.map_or(k, |lanes| k % lanes)
    }

    /// Where the row of column `k` starts on the block axis.
    #[must_use]
    pub fn row_top(&self, k: usize) -> f32 {
        let row = self.lanes.map_or(0, |lanes| k / lanes);
        self.row_tops.get(row).copied().unwrap_or(0.0)
    }

    /// The column holding flow offset `y`: the last one starting at or
    /// before it.
    #[must_use]
    pub fn column_at(&self, y: f32) -> usize {
        self.starts
            .iter()
            .rposition(|&start| start <= y + FIT_EPS)
            .unwrap_or(0)
    }
}

/// Sub-1/100-px overshoot is float noise, not a reason to break.
const FIT_EPS: f32 = 0.01;

/// The smallest piece of the flow the columns are cut between: a box that
/// moves whole, or the top of a splittable box through its first line, or
/// one further line of it.
#[derive(Debug, Clone, Copy)]
struct Atom {
    bottom: f32,
    /// Where a column starts when the break falls before this atom; `None`
    /// = no break here (the first atom, a line on the line before).
    break_at: Option<f32>,
    owner: usize,
    /// The line index the atom opens, for the lines after a box's first.
    line: Option<usize>,
    /// The heights of the footnotes anchored in the atom.
    notes: f32,
}

fn atoms_of(boxes: &[FlowBox]) -> Vec<Atom> {
    let mut atoms: Vec<Atom> = Vec::new();
    let mut last_break = f32::MIN;
    let mut push = |atoms: &mut Vec<Atom>,
                    at: f32,
                    bottom: f32,
                    owner: usize,
                    line: Option<usize>,
                    notes: f32| {
        // A break must move the flow forward; one that would not (a line
        // overlapping the one before it) is no break opportunity.
        let break_at = (at.is_finite() && at > last_break + FIT_EPS).then_some(at);
        if let Some(at) = break_at {
            last_break = at;
        }
        atoms.push(Atom {
            bottom,
            break_at,
            owner,
            line,
            notes: if notes.is_finite() { notes.max(0.0) } else { 0.0 },
        });
    };
    for (owner, flow_box) in boxes.iter().enumerate() {
        let lines: Vec<&FlowLine> = flow_box
            .lines
            .iter()
            .filter(|l| l.top.is_finite() && l.bottom.is_finite() && l.bottom > l.top)
            .collect();
        // Every note of the box is some atom's: those of the lines that
        // open no atom of their own go with the box's first.
        let line_notes: f32 = flow_box.lines.iter().map(|l| l.notes).sum();
        if lines.len() < 2 {
            push(
                &mut atoms,
                flow_box.top,
                flow_box.bottom.max(flow_box.top),
                owner,
                None,
                flow_box.notes + line_notes,
            );
            continue;
        }
        let own_atom_notes: f32 = lines[1..].iter().map(|l| l.notes).sum();
        // The box's top (border, padding) goes with its first line, its
        // bottom with its last; a column break between two lines falls
        // where one line box ends and the next begins.
        push(
            &mut atoms,
            flow_box.top,
            lines[0].bottom.max(flow_box.top),
            owner,
            None,
            flow_box.notes + line_notes - own_atom_notes,
        );
        for j in 1..lines.len() {
            let at = f32::midpoint(lines[j - 1].bottom, lines[j].top).max(flow_box.top);
            let bottom = if j + 1 == lines.len() {
                lines[j].bottom.max(flow_box.bottom)
            } else {
                lines[j].bottom
            };
            push(&mut atoms, at, bottom, owner, Some(lines[j].index), lines[j].notes);
        }
    }
    // Nothing comes before the first atom to break from.
    if let Some(first) = atoms.first_mut() {
        first.break_at = None;
    }
    atoms
}

/// Where the column starting with atom `a` starts on the flow.
fn column_start(atoms: &[Atom], a: usize) -> f32 {
    if a == 0 {
        0.0
    } else {
        atoms[a].break_at.unwrap_or(0.0)
    }
}

/// Fills columns in order, from atom `from` on: the `k`-th column (from 0)
/// is `height_of(k)` tall. Each takes atoms until the next would end below
/// it, and breaks before that one at the last break opportunity whose
/// preceding content fits. A column that cannot fit even its first piece
/// takes it anyway and breaks at the first opportunity after it (a
/// monolith overflows, never loops). Returns the first atom of every column
/// - of no more than `limit` columns and the one after them: a caller that
/// only asks where the first few columns end need not fill the rest.
fn fill_columns(
    atoms: &[Atom],
    from: usize,
    height_of: impl Fn(usize) -> f32,
    limit: usize,
) -> Vec<usize> {
    let mut firsts = vec![from];
    let mut a = from;
    while a < atoms.len() && firsts.len() <= limit {
        let limit = column_start(atoms, a) + height_of(firsts.len() - 1) + FIT_EPS;
        let mut content_bottom = atoms[a].bottom;
        let mut fitting_break = None;
        let mut first_break = None;
        let mut overflowed = false;
        for (i, atom) in atoms.iter().enumerate().skip(a + 1) {
            if atom.break_at.is_some() {
                if first_break.is_none() {
                    first_break = Some(i);
                }
                if content_bottom <= limit {
                    fitting_break = Some(i);
                } else {
                    overflowed = true;
                    break;
                }
            }
            content_bottom = content_bottom.max(atom.bottom);
        }
        if !overflowed && content_bottom <= limit {
            break; // the rest fits: this is the last column
        }
        match fitting_break.or(first_break) {
            Some(next) => {
                firsts.push(next);
                a = next;
            }
            None => break, // nowhere to break: the rest stays here
        }
    }
    firsts
}

/// The shortest column height whose in-order fill, from atom `from`, needs
/// no more than `count` columns - capped by `cap`. A greedy fill never
/// needs MORE columns for a taller height, so it bisects between the even
/// share and the whole flow.
#[allow(clippy::cast_precision_loss)] // a column count
fn balanced_height(atoms: &[Atom], from: usize, count: usize, cap: Option<f32>) -> f32 {
    let start = column_start(atoms, from);
    let total = atoms[from..]
        .iter()
        .map(|a| a.bottom)
        .fold(start, f32::max)
        - start;
    let needs = |h: f32| fill_columns(atoms, from, |_| h, usize::MAX).len();
    let mut lo = total / count as f32;
    let hi = if needs(lo) <= count {
        lo
    } else {
        let mut hi = total;
        for _ in 0..48 {
            if hi - lo <= FIT_EPS {
                break;
            }
            let mid = f32::midpoint(lo, hi);
            if needs(mid) <= count {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        hi
    };
    cap.map_or(hi, |cap| hi.min(cap))
}

/// The heights of the footnotes anchored in `atoms`.
fn notes_of(atoms: &[Atom]) -> f32 {
    atoms.iter().map(|a| a.notes).sum()
}

/// The row of `count` columns that starts with atom `from` on a page that
/// leaves it `room`: the atom the NEXT row starts with (`atoms.len()` when
/// this row is the last) and how tall its columns may be.
///
/// The footnotes anchored in the row stand at the bottom of its page
/// (`float: footnote`; CSS Generated Content for Paged Media: the footnote
/// area is at the bottom of the page, as wide as the page area - so it
/// shortens EVERY column of the page, not only the one a note is anchored
/// in): a row of the atoms up to `end` has the room less their notes. The
/// row is the longest that fits what its own notes leave it. A piece of the
/// flow whose note does not fit starts the next row, with its note: piece
/// and note stay on one page (the behaviour of `footnote-policy: line`).
///
/// A note that fits no page (taller than `area_max`, the most a page gives
/// its footnote area) is split over the pages, as the break analysis does
/// it (`footnotes::Footnotes::room_for_page`): the row it starts in gives
/// it the whole footnote area, and what is left of it (`carry_in`) takes
/// its room from the rows after.
///
/// Without notes in reach the row is `room` tall and ends where its
/// `count`-th column does. A first piece whose own note leaves it no room
/// is taken anyway (a monolith overflows, never loops).
fn plan_row(
    atoms: &[Atom],
    from: usize,
    count: usize,
    room: f32,
    area_max: f32,
    area: crate::solver3::footnotes::FootnoteArea,
    carry_in: f32,
) -> RowPlan {
    // The atom after the `count` columns filled `height` tall from `from`,
    // looking no further than `end`.
    let reach = |end: usize, height: f32| {
        fill_columns(&atoms[..end], from, |_| height, count)
            .get(count)
            .copied()
            .unwrap_or(end)
    };
    // What the notes of the row up to `end` ask of its page, and what the
    // page gives them: all of it - or, when a note that fits no page is
    // among them (or goes on from the row before), the whole footnote area.
    let asked = |end: usize| carry_in + notes_of(&atoms[from..end]);
    let splits = |end: usize| {
        carry_in > 0.0 || atoms[from..end].iter().any(|a| a.notes > area_max + FIT_EPS)
    };
    let done = |end: usize, height: f32| {
        let asked = asked(end);
        let notes = if splits(end) { asked.min(area_max) } else { asked };
        RowPlan {
            next: end,
            height,
            notes: area.taken(notes),
            rest: (asked - notes).max(0.0),
        }
    };
    let mut end = reach(atoms.len(), room);
    // Every round takes a note-bearing atom out of the row.
    for _ in 0..=atoms.len() {
        let asked_here = asked(end);
        if asked_here <= 0.0 {
            return done(end, room);
        }
        let split = splits(end);
        let notes = if split { asked_here.min(area_max) } else { asked_here };
        let height = (room - area.taken(notes)).max(0.0);
        let fits = reach(end, height);
        // Notes that are not split fit the footnote area whole, or one of
        // their pieces starts the next row.
        let area_holds = split || asked_here <= area_max + FIT_EPS;
        if fits >= end && area_holds {
            return done(end, height);
        }
        // The last atom of the row with a note: the atoms after it change
        // nothing about the row's room. If the columns reach past it, they
        // end the row; else it starts the next row (at the last break
        // opportunity at or before it), and the row is tried without it.
        let last_note = (from..end).rev().find(|&i| atoms[i].notes > 0.0);
        match last_note {
            Some(last) if fits > last && area_holds => return done(fits, height),
            Some(last) => {
                let cut = (from + 1..=last).rev().find(|&i| atoms[i].break_at.is_some());
                match cut {
                    Some(cut) => end = cut,
                    None => return done(fits.max(from + 1), height),
                }
            }
            // Only what goes on from the row before: the row is as tall as
            // that leaves it.
            None => return done(fits.max(from + 1), height),
        }
    }
    done(end, room)
}

/// One row of a paged multi-column container ([`plan_row`]).
struct RowPlan {
    /// The atom the next row starts with (`atoms.len()`: this is the last).
    next: usize,
    /// How tall the row's columns may be.
    height: f32,
    /// The room the footnote area takes at the bottom of the row's page:
    /// the notes and the area's own margin, rule and padding.
    notes: f32,
    /// What is left of footnotes that fit no page for the next row's page.
    rest: f32,
}

/// The columns that start with the atoms `firsts`, on `lanes` / `row_tops`.
fn plan_from(
    boxes: &[FlowBox],
    atoms: &[Atom],
    firsts: &[usize],
    lanes: Option<usize>,
    row_tops: Vec<f32>,
    row_notes: Vec<f32>,
) -> ColumnPlan {
    let starts: Vec<f32> = firsts.iter().map(|&a| column_start(atoms, a)).collect();
    let mut placements: Vec<BoxPlacement> = boxes
        .iter()
        .map(|_| BoxPlacement {
            column: 0,
            line_breaks: Vec::new(),
            break_columns: Vec::new(),
        })
        .collect();
    let mut column = 0usize;
    for (i, atom) in atoms.iter().enumerate() {
        // The atom opens the next column - the LAST of those that start
        // with it: a row that ended early left its other columns empty.
        let mut opens_column = false;
        while firsts.get(column + 1) == Some(&i) {
            column += 1;
            opens_column = true;
        }
        match atom.line {
            None => placements[atom.owner].column = column,
            Some(line) if opens_column => {
                placements[atom.owner].line_breaks.push(line);
                placements[atom.owner].break_columns.push(column);
            }
            Some(_) => {}
        }
    }
    let mut plan = ColumnPlan {
        starts,
        height: 0.0,
        boxes: placements,
        lanes,
        row_tops,
        row_notes,
    };
    for (k, &first) in firsts.iter().enumerate() {
        let end = firsts.get(k + 1).copied().unwrap_or(atoms.len());
        let bottom = atoms[first..end]
            .iter()
            .map(|a| a.bottom)
            .fold(plan.starts[k], f32::max);
        plan.height = plan.height.max(plan.row_top(k) + bottom - plan.starts[k]);
    }
    plan
}

fn empty_plan() -> ColumnPlan {
    ColumnPlan {
        starts: vec![0.0],
        height: 0.0,
        boxes: Vec::new(),
        lanes: None,
        row_tops: vec![0.0],
        row_notes: vec![0.0],
    }
}

/// CSS Multicol 1 §7 (`column-fill`) and §8 (overflow): cuts a
/// multi-column container's single-column flow into `count` columns.
///
/// `balance` (the initial value, and always when the container's height is
/// not definite): the shortest column height whose in-order fill needs no
/// more than `count` columns - capped by a definite `height`. `auto` with a
/// definite height: columns of that height, filled in turn. Content a
/// capped column height cannot fit in `count` columns runs on in further
/// (overflow) columns in the inline direction.
#[must_use]
pub fn plan_columns(
    boxes: &[FlowBox],
    count: u32,
    height: Option<f32>,
    fill: ColumnFill,
) -> ColumnPlan {
    let atoms = atoms_of(boxes);
    if atoms.is_empty() {
        return empty_plan();
    }
    let count = count.max(1) as usize;
    let cap = height.filter(|h| h.is_finite() && *h > 0.0);
    let column_height = match (fill, cap) {
        (ColumnFill::Auto, Some(cap)) => cap,
        _ => balanced_height(&atoms, 0, count, cap),
    };
    let firsts = fill_columns(&atoms, 0, |_| column_height, usize::MAX);
    plan_from(boxes, &atoms, &firsts, None, vec![0.0], vec![0.0])
}

/// The room paged media leaves a multi-column container's columns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRoom {
    /// The block-axis room left on the page the container's content starts
    /// on, from where it starts to the page's bottom.
    pub first: f32,
    /// The room of every page after it: the page's height.
    pub rest: f32,
    /// The box of the footnote area (`@page { @footnote }`): what it takes
    /// of a page besides its notes, and the most its notes may take.
    pub area: crate::solver3::footnotes::FootnoteArea,
}

/// CSS Multicol 1 §7 and CSS Fragmentation 3 on paged media: the columns
/// are as tall as the page has room for. They are filled in order - the
/// first column down to the bottom of the page before the second starts -
/// and the content that does not fit goes on in the columns of the next
/// page, from its top: the columns wrap into ROWS, one per page
/// ([`ColumnPlan::lanes`], [`ColumnPlan::row_tops`]). Only the last row is
/// balanced. A container whose first piece does not fit the room of its
/// first page starts on the next one.
///
/// Footnotes (`float: footnote`, [`FlowBox::notes`]): the notes anchored in
/// a row stand at the bottom of the row's page, so the row's columns are
/// only as tall as the page's room less those notes
/// ([`row_column_height`]). A row that carries notes is that much shorter
/// on the canvas too: the next row starts where its page's flow ends, which
/// is where the break analysis ends the page (`page_breaks`: the page's
/// height less its notes). A note taller than its page leaves no room: its
/// row holds one piece a column and runs past the page (a limit of this
/// implementation, notes are not split).
#[must_use]
pub fn plan_paged_columns(boxes: &[FlowBox], count: u32, room: PageRoom) -> ColumnPlan {
    let atoms = atoms_of(boxes);
    if atoms.is_empty() {
        return empty_plan();
    }
    let count = count.max(1) as usize;
    // The most a page gives its footnote area (`footnotes`): a note taller
    // than that is split over the pages.
    let area_max = room.area.content_max(room.rest);
    let (first_room, lead) = if atoms[0].bottom + room.area.taken(atoms[0].notes.min(area_max))
        > room.first + FIT_EPS
        && room.first < room.rest
    {
        (room.rest, room.first)
    } else {
        (room.first, 0.0)
    };
    let mut firsts: Vec<usize> = Vec::new();
    let mut row_tops = vec![lead];
    let mut row_notes: Vec<f32> = Vec::new();
    let mut from = 0usize;
    // What is left of the notes of the rows so far for the next page.
    let mut carry = 0.0_f32;
    loop {
        let page = if row_notes.is_empty() {
            first_room
        } else {
            room.rest
        };
        let row = plan_row(&atoms, from, count, page, area_max, room.area, carry);
        let (next, height, notes) = (row.next, row.height, row.notes);
        carry = row.rest;
        row_notes.push(notes);
        if next >= atoms.len() {
            // The last row: balanced, under what its notes leave it.
            let balanced = balanced_height(&atoms, from, count, Some(height));
            firsts.extend(fill_columns(&atoms, from, |_| balanced, usize::MAX));
            break;
        }
        // A full row: the next one starts on the next page - where this
        // page's flow ends, its room less its notes (never above the
        // row's own content).
        let filled = fill_columns(&atoms[..next], from, |_| height, usize::MAX);
        let content = (0..filled.len())
            .map(|c| {
                let end = filled.get(c + 1).copied().unwrap_or(next);
                let start = column_start(&atoms, filled[c]);
                atoms[filled[c]..end]
                    .iter()
                    .map(|a| a.bottom)
                    .fold(start, f32::max)
                    - start
            })
            .fold(0.0_f32, f32::max);
        let top = row_tops.last().copied().unwrap_or(lead);
        row_tops.push(top + (page - notes).max(content));
        // A row always has `count` columns: the next row's first column is
        // the one after them (`ColumnPlan::lanes`), also when the row ends
        // early, before a piece whose note does not fit.
        firsts.extend(&filled);
        firsts.extend(core::iter::repeat_n(next, count.saturating_sub(filled.len())));
        from = next;
    }
    plan_from(boxes, &atoms, &firsts, Some(count), row_tops, row_notes)
}

// ---------------------------------------------------------------------------
// Where on its page a multi-column container starts
// ---------------------------------------------------------------------------

/// A multi-column block container's flow, kept from the layout pass that
/// made it: what [`plan_page_rooms`] needs to tell how tall the container
/// is wherever it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnFlowRecord {
    /// The in-flow children in the single column.
    pub boxes: Vec<FlowBox>,
    /// The column count.
    pub count: u32,
    /// The height the container's content got in that pass.
    pub height: f32,
    /// Where its rows start, from its content-box top ([`ColumnPlan::row_tops`]).
    pub row_tops: Vec<f32>,
}

/// What the multi-column layout of one pass leaves for the next.
///
/// A container on paged media is as tall as its columns - and how tall that
/// is depends on how much of its first page is left where it starts, which
/// no box knows while it is laid out: the pass lays every container out
/// unpaged (`rooms` empty), then [`plan_page_rooms`] tells from the
/// positions it ended with, and the pass is repeated with the rooms.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColumnPages {
    /// The room of the first page of every container that does not fit
    /// the one it starts on: those lay their columns out page by page.
    pub rooms: std::collections::BTreeMap<NodeId, f32>,
    /// The flow of every multi-column block container of the last pass.
    pub flows: std::collections::BTreeMap<NodeId, ColumnFlowRecord>,
    /// The heights of the pages the layout is cut into; `None` = continuous
    /// media (or a caller that does not know them): the columns stay one row.
    pub pages: Option<crate::solver3::page_breaks::PageConstraints>,
    /// How much earlier than [`plan_page_rooms`] counts on the break analysis
    /// ends the first page of a container: a break it pushed up (widows,
    /// orphans, atomic lines) moved the page's end - and the pages after it.
    pub slack: std::collections::BTreeMap<NodeId, f32>,
    /// How often the pages were laid out again for what the break analysis
    /// made of them.
    pub retries: usize,
    /// The margin-box height of every footnote (`float: footnote`) of the
    /// last pass, by its element: the room it takes at the bottom of its
    /// page. A footnote is sized after the flow it was taken out of, so the
    /// columns of a pass are filled with the heights of the pass before
    /// ([`FlowBox::notes`]); a footnote is as wide as the page, its height
    /// does not depend on the columns.
    pub note_heights: std::collections::BTreeMap<NodeId, f32>,
    /// The box of the footnote area of the document (`@page { @footnote }`).
    pub note_area: crate::solver3::footnotes::FootnoteArea,
}

/// One thing on the canvas that decides where the pages break, in the
/// order of the canvas.
#[derive(Debug, Clone, Copy)]
pub enum PageEvent {
    /// A forced break (`break-before: page`, ...) at `y`.
    ForcedBreak { y: f32 },
    /// A multi-column container whose content starts at `y`.
    Container { node: NodeId, y: f32 },
    /// A footnote (`float: footnote`) anchored OUTSIDE every multi-column
    /// container, in a line (or at a place in the block flow) from `y` to
    /// `bottom`, `height` tall: it stands at the bottom of its page and
    /// takes that much from the page's flow - and so from the room of a
    /// container on the same page. (The notes anchored inside a container
    /// are in its flow record, [`FlowBox::notes`].)
    Note { y: f32, bottom: f32, height: f32 },
}

impl PageEvent {
    const fn y(&self) -> f32 {
        match *self {
            Self::ForcedBreak { y } | Self::Container { y, .. } | Self::Note { y, .. } => y,
        }
    }
}

/// The room each multi-column container that does not fit its first page
/// has on it, given the canvas of the last pass: `events` as that pass
/// positioned them, `flows` as it kept them, the first page `first` tall and
/// every other `rest` (a footer takes room from all but the first), pages
/// breaking at their heights and at each forced break, the first page of a
/// container `slack` shorter where the break analysis pushed its end up.
///
/// A container that is laid out over several pages is taller or shorter
/// than it was, and everything below it moves; the events below it are
/// seen where they will be. The pages are the same: a forced break below
/// restarts them.
///
/// Footnotes: a page's notes stand at its bottom, and its flow ends where
/// they begin - the pages are cut from the canvas there
/// (`footnotes::Footnotes::room_for_page`, which this follows without the
/// break analysis's own adjustments, like the rest of this function). So a
/// page of the canvas is its height LESS the notes anchored on it, a note
/// whose line no longer fits above it starts the next page with its line,
/// and a container has the room its page's notes leave.
#[must_use]
pub fn plan_page_rooms(
    first: f32,
    rest: f32,
    events: &[PageEvent],
    flows: &std::collections::BTreeMap<NodeId, ColumnFlowRecord>,
    slack: &std::collections::BTreeMap<NodeId, f32>,
    area: crate::solver3::footnotes::FootnoteArea,
) -> std::collections::BTreeMap<NodeId, f32> {
    // The page being filled: where it starts on the canvas, how tall it is
    // and how much of that its footnotes take so far.
    struct Page {
        top: f32,
        height: f32,
        notes: f32,
        rest: f32,
        /// What is left of footnotes that fit no page, for the pages after
        /// this one (`footnotes::Footnotes::room_for_page`).
        carry: f32,
        area: crate::solver3::footnotes::FootnoteArea,
    }
    impl Page {
        /// Where the page's flow ends on the canvas.
        fn end(&self) -> f32 {
            self.top + self.height - self.area.taken(self.notes)
        }
        /// The most the page gives its footnote area.
        fn area_max(&self) -> f32 {
            self.area.content_max(self.height)
        }
        fn start_at(&mut self, top: f32) {
            self.top = top;
            self.height = self.rest;
            // What goes on from the page before comes first.
            self.notes = self.carry.min(self.area_max());
            self.carry -= self.notes;
        }
        /// Moves to the page `y` is on; an end (`starts_here` false) on a
        /// boundary is still on the page before it.
        fn advance(&mut self, y: f32, starts_here: bool) {
            // A page its notes fill cannot be moved past: stop there.
            while self.end() > self.top
                && ((starts_here && self.end() <= y + FIT_EPS)
                    || (!starts_here && self.end() < y - FIT_EPS))
            {
                let end = self.end();
                self.start_at(end);
            }
        }
    }

    let mut rooms = std::collections::BTreeMap::new();
    if !(first.is_finite() && first > 0.0 && rest.is_finite() && rest > 0.0) {
        return rooms;
    }
    let mut events: Vec<&PageEvent> = events.iter().collect();
    events.sort_by(|a, b| a.y().total_cmp(&b.y()));

    let mut shift = 0.0_f32;
    let mut page = Page {
        top: 0.0,
        height: first,
        notes: 0.0,
        rest,
        carry: 0.0,
        area,
    };
    for event in events {
        match *event {
            PageEvent::ForcedBreak { y } => {
                let y = y + shift;
                page.advance(y, true);
                if y > page.top + FIT_EPS {
                    page.start_at(y);
                }
            }
            PageEvent::Note { y, bottom, height } => {
                let (y, bottom) = (y + shift, bottom + shift);
                page.advance(y, true);
                // A line (or place) at the page's top stays with its note
                // whatever its height (`Footnotes::room_for_page`).
                let at_top = y <= page.top + 1.0;
                if height > page.area_max() + FIT_EPS {
                    // It fits no page: it is split. It starts where the
                    // whole footnote area is free under its line (else its
                    // line starts the next page), and goes on on the pages
                    // after.
                    if !at_top
                        && bottom
                            > page.top + page.height - area.chrome() - page.area_max() + FIT_EPS
                    {
                        page.start_at(y);
                    }
                    let part = (page.area_max() - page.notes).max(0.0).min(height);
                    page.notes += part;
                    page.carry += height - part;
                } else if bottom
                    <= page.top + page.height - area.taken(page.notes + height) + FIT_EPS
                    || at_top
                {
                    page.notes += height;
                } else {
                    // The line goes to the next page with its note.
                    page.start_at(y);
                    page.notes += height;
                }
            }
            PageEvent::Container { node, y } => {
                let Some(flow) = flows.get(&node) else {
                    continue;
                };
                let y = y + shift;
                page.advance(y, true);
                let room = (page.end() - y - slack.get(&node).copied().unwrap_or(0.0)).max(0.0);
                let unpaged = plan_columns(&flow.boxes, flow.count, None, ColumnFill::Balance);
                let notes: f32 = flow
                    .boxes
                    .iter()
                    .map(|b| b.notes + b.lines.iter().map(|l| l.notes).sum::<f32>())
                    .sum();
                let fits = y + unpaged.height
                    <= page.top + page.height
                        - area.taken(page.notes + notes)
                        - slack.get(&node).copied().unwrap_or(0.0)
                        + FIT_EPS;
                let height = if fits {
                    // It fits its page in one row, above its notes.
                    page.notes += notes;
                    unpaged.height
                } else {
                    rooms.insert(node, room);
                    let plan = plan_paged_columns(
                        &flow.boxes,
                        flow.count,
                        PageRoom {
                            first: room,
                            rest,
                            area,
                        },
                    );
                    // Its rows start pages (forced breaks): the pages after it go on from
                    // its last row, not from the grid it started on - with the notes of
                    // that row at the page's bottom.
                    if let Some(&last_row) = plan.row_tops.last() {
                        page.start_at(y + last_row);
                        // (`row_notes` is what the area takes, its own
                        // margin, rule and padding included.)
                        let last_notes = plan.row_notes.last().copied().unwrap_or(0.0);
                        page.notes = (last_notes - area.chrome()).max(0.0);
                    }
                    plan.height
                };
                shift += height - flow.height;
                page.advance(y + height, false);
            }
        }
    }
    rooms
}

/// The first container, in the order of the canvas, whose first page the
/// break analysis cut earlier than its layout counted on - its columns end
/// at `counted_on`, and a break fell before - and by how much. `breaks` are
/// the pages' ends as the analysis made them for THIS layout.
#[must_use]
pub fn early_page_end(
    events: &[PageEvent],
    flows: &std::collections::BTreeMap<NodeId, ColumnFlowRecord>,
    breaks: &[f32],
) -> Option<(NodeId, f32)> {
    let mut containers: Vec<(NodeId, f32)> = events
        .iter()
        .filter_map(|event| match *event {
            PageEvent::Container { node, y } => Some((node, y)),
            PageEvent::ForcedBreak { .. } | PageEvent::Note { .. } => None,
        })
        .collect();
    containers.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (node, y) in containers {
        let Some(flow) = flows.get(&node) else {
            continue;
        };
        // Where its first page ends: the second row's top (or the start of
        // a container that moved to the next page), else the container's end.
        let counted_on = y + flow
            .row_tops
            .iter()
            .copied()
            .find(|top| *top > 0.0)
            .unwrap_or(flow.height);
        let first_break = breaks
            .iter()
            .copied()
            .filter(|b| *b > y + 1.0)
            .fold(f32::MAX, f32::min);
        if first_break < counted_on - 1.0 {
            return Some((node, counted_on - first_break));
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn style(count: Option<u32>, width: Option<f32>, gap: f32) -> ColumnStyle {
        ColumnStyle {
            count,
            width,
            gap,
            fill: ColumnFill::Balance,
        }
    }

    #[test]
    fn a_column_count_shares_the_width_minus_the_gaps() {
        let g = style(Some(2), None, 20.0).geometry(420.0);
        assert_eq!((g.count, g.width, g.gap), (2, 200.0, 20.0));
    }

    #[test]
    fn a_column_width_fits_as_many_columns_as_it_can_and_stretches_them() {
        // floor((430 + 10) / (100 + 10)) = 4; (440 / 4) - 10 = 100.
        let g = style(None, Some(100.0), 10.0).geometry(430.0);
        assert_eq!((g.count, g.width), (4, 100.0));
        // floor((450 + 10) / 110) = 4; (460 / 4) - 10 = 105.
        let g = style(None, Some(100.0), 10.0).geometry(450.0);
        assert_eq!((g.count, g.width), (4, 105.0));
        // Narrower than one column: one column, the whole width.
        let g = style(None, Some(100.0), 10.0).geometry(60.0);
        assert_eq!((g.count, g.width), (1, 60.0));
    }

    #[test]
    fn with_both_the_column_count_is_the_maximum() {
        assert_eq!(style(Some(2), Some(100.0), 10.0).geometry(430.0).count, 2);
        assert_eq!(style(Some(9), Some(100.0), 10.0).geometry(430.0).count, 4);
    }

    #[test]
    fn degenerate_inputs_still_make_one_column_or_more() {
        assert_eq!(style(Some(0), None, 10.0).geometry(100.0).count, 1);
        let g = style(Some(3), None, 50.0).geometry(40.0);
        assert_eq!((g.count, g.width), (3, 0.0));
        assert_eq!(style(None, Some(f32::NAN), 10.0).geometry(100.0).count, 1);
        assert_eq!(style(None, Some(100.0), 10.0).geometry(f32::NAN).count, 1);
    }

    #[test]
    fn columns_run_in_the_inline_base_direction() {
        let g = style(Some(2), None, 20.0).geometry(420.0);
        assert_eq!(g.column_x(0, 420.0, false), 0.0);
        assert_eq!(g.column_x(1, 420.0, false), 220.0);
        assert_eq!(g.column_x(2, 420.0, false), 440.0);
        assert_eq!(g.column_x(0, 420.0, true), 220.0);
        assert_eq!(g.column_x(1, 420.0, true), 0.0);
        assert_eq!(g.advance(false), 220.0);
        assert_eq!(g.advance(true), -220.0);
    }

    // ---- plan_columns ----

    /// A box that moves whole, `top..bottom`.
    fn block(top: f32, bottom: f32) -> FlowBox {
        FlowBox {
            top,
            bottom,
            lines: Vec::new(),
            notes: 0.0,
        }
    }

    /// A paragraph at `top` of `n` 20px line boxes, each line's content
    /// 19px tall in the middle of its line box.
    fn paragraph(top: f32, n: usize) -> FlowBox {
        #[allow(clippy::cast_precision_loss)] // a line index
        let lines = (0..n)
            .map(|index| {
                let line_top = top + 20.0 * index as f32;
                FlowLine {
                    index,
                    top: line_top + 0.5,
                    bottom: line_top + 19.5,
                    notes: 0.0,
                }
            })
            .collect();
        #[allow(clippy::cast_precision_loss)] // a line count
        let bottom = top + 20.0 * n as f32;
        FlowBox {
            top,
            bottom,
            lines,
            notes: 0.0,
        }
    }

    fn columns_of(plan: &ColumnPlan) -> Vec<usize> {
        plan.boxes.iter().map(|b| b.column).collect()
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.05
    }

    #[test]
    fn four_boxes_balance_two_per_column() {
        let boxes = [
            block(0.0, 40.0),
            block(40.0, 80.0),
            block(80.0, 120.0),
            block(120.0, 160.0),
        ];
        let plan = plan_columns(&boxes, 2, None, ColumnFill::Balance);
        assert_eq!(columns_of(&plan), [0, 0, 1, 1]);
        assert_eq!(plan.starts.len(), 2);
        assert!(near(plan.starts[1], 80.0), "{:?}", plan.starts);
        assert!(near(plan.height, 80.0), "{}", plan.height);
        assert!(plan.boxes.iter().all(|b| b.line_breaks.is_empty()));
    }

    #[test]
    fn as_many_columns_as_boxes_put_one_box_in_each() {
        let boxes = [
            block(0.0, 40.0),
            block(40.0, 80.0),
            block(80.0, 120.0),
            block(120.0, 160.0),
        ];
        let plan = plan_columns(&boxes, 4, None, ColumnFill::Balance);
        assert_eq!(columns_of(&plan), [0, 1, 2, 3]);
        assert!(near(plan.height, 40.0), "{}", plan.height);
    }

    #[test]
    fn a_paragraph_breaks_between_two_lines() {
        // 40px block + six 20px lines: balanced at 80px, the paragraph's
        // third line (index 2) opens the second column.
        let boxes = [block(0.0, 40.0), paragraph(40.0, 6)];
        let plan = plan_columns(&boxes, 2, None, ColumnFill::Balance);
        assert_eq!(columns_of(&plan), [0, 0]);
        assert_eq!(plan.boxes[1].line_breaks, [2]);
        assert!(near(plan.starts[1], 80.0), "{:?}", plan.starts);
        assert!(near(plan.height, 80.0), "{}", plan.height);
    }

    #[test]
    fn a_long_paragraph_alone_runs_through_every_column() {
        let plan = plan_columns(&[paragraph(0.0, 9)], 3, None, ColumnFill::Balance);
        assert_eq!(plan.boxes[0].column, 0);
        assert_eq!(plan.boxes[0].line_breaks, [3, 6]);
        assert!(near(plan.height, 60.0), "{}", plan.height);
    }

    #[test]
    fn a_box_too_tall_for_what_is_left_moves_whole() {
        // 50 + 50 + 60: at 80px the 60px box cannot follow the second.
        let boxes = [block(0.0, 50.0), block(50.0, 100.0), block(100.0, 160.0)];
        let plan = plan_columns(&boxes, 2, None, ColumnFill::Balance);
        assert_eq!(columns_of(&plan), [0, 0, 1]);
        assert!(near(plan.height, 100.0), "{}", plan.height);
    }

    #[test]
    fn a_fixed_height_runs_on_into_overflow_columns() {
        let boxes = [
            block(0.0, 40.0),
            block(40.0, 80.0),
            block(80.0, 120.0),
            block(120.0, 160.0),
        ];
        let plan = plan_columns(&boxes, 2, Some(60.0), ColumnFill::Balance);
        assert_eq!(columns_of(&plan), [0, 1, 2, 3]);
        // A fixed height under `column-fill: auto` fills each column first.
        let plan = plan_columns(&boxes, 2, Some(100.0), ColumnFill::Auto);
        assert_eq!(columns_of(&plan), [0, 0, 1, 1]);
    }

    #[test]
    fn a_monolith_taller_than_any_column_stays_whole_and_ends() {
        let plan = plan_columns(&[block(0.0, 200.0)], 2, None, ColumnFill::Balance);
        assert_eq!(columns_of(&plan), [0]);
        assert_eq!(plan.starts.len(), 1);
        assert!(near(plan.height, 200.0), "{}", plan.height);
        let plan = plan_columns(
            &[block(0.0, 200.0), block(200.0, 210.0)],
            2,
            Some(50.0),
            ColumnFill::Balance,
        );
        assert_eq!(columns_of(&plan), [0, 1]);
    }

    #[test]
    fn an_empty_flow_is_one_empty_column() {
        let plan = plan_columns(&[], 3, None, ColumnFill::Balance);
        assert_eq!(plan.starts, [0.0]);
        assert_eq!(plan.height, 0.0);
        assert!(plan.boxes.is_empty());
        assert_eq!(plan.column_at(123.0), 0);
    }

    #[test]
    fn column_at_finds_the_column_holding_an_offset() {
        let boxes = [
            block(0.0, 40.0),
            block(40.0, 80.0),
            block(80.0, 120.0),
            block(120.0, 160.0),
        ];
        let plan = plan_columns(&boxes, 2, None, ColumnFill::Balance);
        assert_eq!(plan.column_at(0.0), 0);
        assert_eq!(plan.column_at(79.0), 0);
        assert_eq!(plan.column_at(80.0), 1);
        assert_eq!(plan.column_at(150.0), 1);
    }
    // ---- paged media ----

    /// `n` 40px boxes in a row.
    fn boxes_of(n: usize) -> Vec<FlowBox> {
        #[allow(clippy::cast_precision_loss)] // a box index
        (0..n)
            .map(|i| block(40.0 * i as f32, 40.0 * (i + 1) as f32))
            .collect()
    }

    #[test]
    fn paged_columns_fill_a_page_before_the_next_row_starts() {
        // 12 boxes, 140px left on the first page, 200px pages: three boxes a
        // column on the first page, the rest balanced on the second.
        let plan = plan_paged_columns(
            &boxes_of(12),
            2,
            PageRoom {
                first: 140.0,
                rest: 200.0,
                area: Default::default(),
            },
        );
        assert_eq!(columns_of(&plan), [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3]);
        assert_eq!(plan.row_tops, [0.0, 140.0]);
        assert_eq!((plan.lane(1), plan.lane(2), plan.lane(3)), (1, 0, 1));
        assert_eq!((plan.row_top(1), plan.row_top(2)), (0.0, 140.0));
        // The second row starts a page down and holds three boxes.
        assert!(near(plan.height, 140.0 + 120.0), "{}", plan.height);
    }

    #[test]
    fn paged_columns_that_fit_the_first_page_are_balanced_on_it() {
        let plan = plan_paged_columns(
            &boxes_of(4),
            2,
            PageRoom {
                first: 140.0,
                rest: 200.0,
                area: Default::default(),
            },
        );
        assert_eq!(columns_of(&plan), [0, 0, 1, 1]);
        assert_eq!(plan.row_tops, [0.0]);
        assert!(near(plan.height, 80.0));
    }

    #[test]
    fn a_container_whose_first_piece_does_not_fit_starts_on_the_next_page() {
        let plan = plan_paged_columns(
            &boxes_of(4),
            2,
            PageRoom {
                first: 30.0,
                rest: 200.0,
                area: Default::default(),
            },
        );
        assert_eq!(plan.row_tops, [30.0]);
        assert!(near(plan.height, 30.0 + 80.0), "{}", plan.height);
    }

    #[test]
    fn page_rooms_follow_the_containers_above_them() {
        use std::collections::BTreeMap;
        let flows: BTreeMap<NodeId, ColumnFlowRecord> = [1, 2]
            .iter()
            .map(|&n| {
                (
                    NodeId::new(n),
                    ColumnFlowRecord {
                        boxes: boxes_of(12),
                        count: 2,
                        height: 240.0, // balanced: six boxes a column
                        row_tops: vec![0.0],
                    },
                )
            })
            .collect();
        // 200px pages. The first container starts at 60 and does not fit the
        // 140px there; laid out page by page it ends at 200 + 120 = 320,
        // where it stood to end at 300. The second, that stood at 300, now
        // starts at 320 with 80px left on its page.
        let events = [
            PageEvent::Container {
                node: NodeId::new(1),
                y: 60.0,
            },
            PageEvent::Container {
                node: NodeId::new(2),
                y: 300.0,
            },
        ];
        let rooms = plan_page_rooms(200.0, 200.0, &events, &flows, &Default::default(), Default::default());
        assert_eq!(rooms.len(), 2, "{rooms:?}");
        assert!(near(rooms[&NodeId::new(1)], 140.0), "{rooms:?}");
        assert!(near(rooms[&NodeId::new(2)], 80.0), "{rooms:?}");
    }

    #[test]
    fn a_forced_break_restarts_the_pages_for_the_room() {
        use std::collections::BTreeMap;
        let flows: BTreeMap<NodeId, ColumnFlowRecord> = [(
            NodeId::new(1),
            ColumnFlowRecord {
                boxes: boxes_of(12),
                count: 2,
                height: 240.0,
                row_tops: vec![0.0],
            },
        )]
        .into_iter()
        .collect();
        // A page break at 50 makes 250 the next boundary: a container at 70
        // has 180px, not 130.
        let events = [
            PageEvent::ForcedBreak { y: 50.0 },
            PageEvent::Container {
                node: NodeId::new(1),
                y: 70.0,
            },
        ];
        let rooms = plan_page_rooms(200.0, 200.0, &events, &flows, &Default::default(), Default::default());
        assert!(near(rooms[&NodeId::new(1)], 180.0), "{rooms:?}");
    }

    #[test]
    fn a_container_that_fits_its_page_needs_no_room() {
        use std::collections::BTreeMap;
        let flows: BTreeMap<NodeId, ColumnFlowRecord> = [(
            NodeId::new(1),
            ColumnFlowRecord {
                boxes: boxes_of(4),
                count: 2,
                height: 80.0,
                row_tops: vec![0.0],
            },
        )]
        .into_iter()
        .collect();
        let events = [PageEvent::Container {
            node: NodeId::new(1),
            y: 100.0,
        }];
        assert!(plan_page_rooms(200.0, 200.0, &events, &flows, &Default::default(), Default::default()).is_empty());
    }
    #[test]
    fn a_break_pushed_up_before_the_end_of_the_first_page_is_its_slack() {
        use std::collections::BTreeMap;
        let flows: BTreeMap<NodeId, ColumnFlowRecord> = [(
            NodeId::new(1),
            ColumnFlowRecord {
                boxes: boxes_of(12),
                count: 2,
                height: 260.0,
                row_tops: vec![0.0, 140.0],
            },
        )]
        .into_iter()
        .collect();
        let events = [PageEvent::Container {
            node: NodeId::new(1),
            y: 60.0,
        }];
        // the rows end at 200; the analysis cut the page at 188 instead
        let early = early_page_end(&events, &flows, &[188.0, 388.0]);
        assert_eq!(early.map(|(n, e)| (n, e.round())), Some((NodeId::new(1), 12.0)));
        // a break where the rows end, or none before it, is no slack
        assert!(early_page_end(&events, &flows, &[200.0, 400.0]).is_none());
        // and the room takes the slack off
        let slack: BTreeMap<NodeId, f32> = [(NodeId::new(1), 12.0)].into_iter().collect();
        let boxes12 = ColumnFlowRecord {
            boxes: boxes_of(12),
            count: 2,
            height: 240.0,
            row_tops: vec![0.0],
        };
        let flows12: BTreeMap<NodeId, ColumnFlowRecord> = [(NodeId::new(1), boxes12)].into_iter().collect();
        let rooms = plan_page_rooms(200.0, 200.0, &events, &flows12, &slack, Default::default());
        assert!(near(rooms[&NodeId::new(1)], 128.0), "{rooms:?}");
    }
    #[test]
    fn the_pages_after_a_paged_container_go_on_from_its_last_row() {
        use std::collections::BTreeMap;
        let record = |height: f32| ColumnFlowRecord {
            boxes: boxes_of(12),
            count: 2,
            height,
            row_tops: vec![0.0],
        };
        let flows: BTreeMap<NodeId, ColumnFlowRecord> =
            [(NodeId::new(1), record(240.0)), (NodeId::new(2), record(240.0))].into_iter().collect();
        // 200px pages; the break analysis cut the first container's first page 30px early,
        // so its rows start pages at 60 + 110 and 60 + 310. The second container, below it,
        // stands where the first container's new height puts it, on the page that begins
        // with the first one's last row.
        let events = [
            PageEvent::Container { node: NodeId::new(1), y: 60.0 },
            PageEvent::Container { node: NodeId::new(2), y: 300.0 },
        ];
        let slack: BTreeMap<NodeId, f32> = [(NodeId::new(1), 30.0)].into_iter().collect();
        let rooms = plan_page_rooms(200.0, 200.0, &events, &flows, &slack, Default::default());
        let first = rooms[&NodeId::new(1)];
        assert!(near(first, 110.0), "{rooms:?}");
        // the first container's rows: [0, 110, 310]: the last row starts at 60 + 310 = 370;
        // the second container, 40px taller below it than it stood (shift), is on that page
        let plan = plan_paged_columns(&boxes_of(12), 2, PageRoom { first, rest: 200.0, area: Default::default() });
        let last_row_top = 60.0 + plan.row_tops.last().copied().unwrap();
        let second_y = 300.0 + (plan.height - 240.0);
        let expected = last_row_top + 200.0 - second_y;
        assert!(near(rooms[&NodeId::new(2)], expected), "{rooms:?} expected {expected}");
    }

    // ---- footnotes ----

    /// `n` 40px boxes, the box `with_note` carrying a note `height` tall.
    fn boxes_with_note(n: usize, with_note: usize, height: f32) -> Vec<FlowBox> {
        #[allow(clippy::cast_precision_loss)] // a box index
        (0..n)
            .map(|i| FlowBox {
                notes: if i == with_note { height } else { 0.0 },
                ..block(40.0 * i as f32, 40.0 * (i + 1) as f32)
            })
            .collect()
    }

    #[test]
    fn a_note_in_a_row_shortens_every_column_of_the_row() {
        // 140px for the first row: three boxes a column - but with the 40px
        // note of the fourth box (in the right column) only 100px, two.
        let plan = plan_paged_columns(
            &boxes_with_note(12, 3, 40.0),
            2,
            PageRoom {
                first: 140.0,
                rest: 200.0,
                area: Default::default(),
            },
        );
        let columns: Vec<usize> = plan.boxes.iter().map(|b| b.column).collect();
        assert_eq!(columns, vec![0, 0, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3]);
        // The next row starts where the first page's flow ends.
        assert_eq!(plan.row_tops, vec![0.0, 100.0]);
        assert_eq!(plan.row_notes, vec![40.0, 0.0]);
        assert!(near(plan.height, 260.0), "{}", plan.height);
    }

    #[test]
    fn a_box_goes_to_the_next_page_with_a_note_that_does_not_fit_under_it() {
        // 140px: three boxes a column would fit, the sixth with its 60px
        // note does not (120 + 60 > 140). With the note the row may be 80px
        // tall - but then the sixth box is not in it, and neither is its
        // note: the row is as tall as it gets without that box, five boxes.
        let plan = plan_paged_columns(
            &boxes_with_note(12, 5, 60.0),
            2,
            PageRoom {
                first: 140.0,
                rest: 200.0,
                area: Default::default(),
            },
        );
        let columns: Vec<usize> = plan.boxes.iter().map(|b| b.column).collect();
        assert_eq!(columns[..6], [0, 0, 0, 1, 1, 2], "{columns:?}");
        assert_eq!(plan.row_notes[0], 0.0);
        assert_eq!(plan.row_tops[1], 140.0);
        assert_eq!(plan.row_notes[1], 60.0);
    }

    #[test]
    fn the_last_row_is_balanced_under_its_notes() {
        // Eight boxes on a 200px page with an 80px note: 120px are left,
        // which three boxes a column do not fill evenly - four and four
        // would, but they are 160px. The row is not the last: three a
        // column stay, the other two are balanced on the next page.
        let plan = plan_paged_columns(
            &boxes_with_note(8, 0, 80.0),
            2,
            PageRoom {
                first: 200.0,
                rest: 200.0,
                area: Default::default(),
            },
        );
        let columns: Vec<usize> = plan.boxes.iter().map(|b| b.column).collect();
        assert_eq!(columns, vec![0, 0, 0, 1, 1, 1, 2, 3]);
        assert_eq!(plan.row_tops, vec![0.0, 120.0]);
    }

    #[test]
    fn a_plan_without_notes_is_the_plan_it_was() {
        // Rows a page apart, the last one balanced (as before footnotes).
        let plan = plan_paged_columns(
            &boxes_of(12),
            2,
            PageRoom {
                first: 140.0,
                rest: 200.0,
                area: Default::default(),
            },
        );
        let columns: Vec<usize> = plan.boxes.iter().map(|b| b.column).collect();
        assert_eq!(columns, vec![0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3]);
        assert_eq!(plan.row_tops, vec![0.0, 140.0]);
        assert_eq!(plan.row_notes, vec![0.0, 0.0]);
    }

    #[test]
    fn a_note_above_a_container_on_its_page_takes_room_from_it() {
        use std::collections::BTreeMap;
        let flows: BTreeMap<NodeId, ColumnFlowRecord> = [(
            NodeId::new(1),
            ColumnFlowRecord {
                boxes: boxes_of(12),
                count: 2,
                height: 240.0,
                row_tops: vec![0.0],
            },
        )]
        .into_iter()
        .collect();
        let events = [
            PageEvent::Note {
                y: 0.0,
                bottom: 20.0,
                height: 40.0,
            },
            PageEvent::Container {
                node: NodeId::new(1),
                y: 60.0,
            },
        ];
        let rooms = plan_page_rooms(200.0, 200.0, &events, &flows, &Default::default(), Default::default());
        assert!(near(rooms[&NodeId::new(1)], 100.0), "{rooms:?}");
    }

    #[test]
    fn a_container_fits_its_page_only_above_its_notes() {
        use std::collections::BTreeMap;
        // Six boxes, three a column: 120px at 60, under the page's 200 -
        // but not above its own 40px note.
        let record = |with_note: bool| ColumnFlowRecord {
            boxes: if with_note {
                boxes_with_note(6, 1, 40.0)
            } else {
                boxes_of(6)
            },
            count: 2,
            height: 120.0,
            row_tops: vec![0.0],
        };
        let events = [PageEvent::Container {
            node: NodeId::new(1),
            y: 60.0,
        }];
        let rooms_of = |with_note: bool| {
            let flows: BTreeMap<NodeId, ColumnFlowRecord> =
                [(NodeId::new(1), record(with_note))].into_iter().collect();
            plan_page_rooms(200.0, 200.0, &events, &flows, &Default::default(), Default::default())
        };
        assert!(rooms_of(false).is_empty());
        assert!(near(rooms_of(true)[&NodeId::new(1)], 140.0));
    }

    #[test]
    fn a_note_that_fits_no_page_takes_the_footnote_area_of_the_rows_it_crosses() {
        // Twenty-four boxes on 200px pages, a 300px note in the second. The
        // footnote area holds 160px: the first row has 40px, one box a
        // column, and the note's first 160px; the second row the other
        // 140px and 60px, one box a column again; the rest is balanced.
        let plan = plan_paged_columns(
            &boxes_with_note(8, 1, 300.0),
            2,
            PageRoom {
                first: 200.0,
                rest: 200.0,
                area: Default::default(),
            },
        );
        let columns: Vec<usize> = plan.boxes.iter().map(|b| b.column).collect();
        assert_eq!(columns, vec![0, 1, 2, 3, 4, 4, 5, 5]);
        assert_eq!(plan.row_notes, vec![160.0, 140.0, 0.0]);
        assert_eq!(plan.row_tops, vec![0.0, 40.0, 100.0]);
    }
}
