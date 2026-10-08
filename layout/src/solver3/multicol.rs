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
}

/// Where one [`FlowBox`] goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoxPlacement {
    /// The column the box starts in.
    pub column: usize,
    /// The line indices at which the box continues at the top of the next
    /// columns, ascending; empty when it sits in one column.
    pub line_breaks: Vec<usize>,
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
}

impl ColumnPlan {
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
}

fn atoms_of(boxes: &[FlowBox]) -> Vec<Atom> {
    let mut atoms: Vec<Atom> = Vec::new();
    let mut last_break = f32::MIN;
    let mut push =
        |atoms: &mut Vec<Atom>, at: f32, bottom: f32, owner: usize, line: Option<usize>| {
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
            });
        };
    for (owner, flow_box) in boxes.iter().enumerate() {
        let lines: Vec<&FlowLine> = flow_box
            .lines
            .iter()
            .filter(|l| l.top.is_finite() && l.bottom.is_finite() && l.bottom > l.top)
            .collect();
        if lines.len() < 2 {
            push(
                &mut atoms,
                flow_box.top,
                flow_box.bottom.max(flow_box.top),
                owner,
                None,
            );
            continue;
        }
        // The box's top (border, padding) goes with its first line, its
        // bottom with its last; a column break between two lines falls
        // where one line box ends and the next begins.
        push(
            &mut atoms,
            flow_box.top,
            lines[0].bottom.max(flow_box.top),
            owner,
            None,
        );
        for j in 1..lines.len() {
            let at = f32::midpoint(lines[j - 1].bottom, lines[j].top).max(flow_box.top);
            let bottom = if j + 1 == lines.len() {
                lines[j].bottom.max(flow_box.bottom)
            } else {
                lines[j].bottom
            };
            push(&mut atoms, at, bottom, owner, Some(lines[j].index));
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

/// Fills columns `height` tall in order: each takes atoms until the next
/// would end below it, and breaks before that one at the last break
/// opportunity whose preceding content fits. A column that cannot fit even
/// its first piece takes it anyway and breaks at the first opportunity
/// after it (a monolith overflows, never loops). Returns the first atom of
/// every column.
fn fill_columns(atoms: &[Atom], height: f32) -> Vec<usize> {
    let mut firsts = vec![0usize];
    let mut a = 0usize;
    while a < atoms.len() {
        let limit = column_start(atoms, a) + height + FIT_EPS;
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
#[allow(clippy::cast_precision_loss)] // a column count
pub fn plan_columns(
    boxes: &[FlowBox],
    count: u32,
    height: Option<f32>,
    fill: ColumnFill,
) -> ColumnPlan {
    let atoms = atoms_of(boxes);
    if atoms.is_empty() {
        return ColumnPlan {
            starts: vec![0.0],
            height: 0.0,
            boxes: Vec::new(),
        };
    }
    let count = count.max(1) as usize;
    let cap = height.filter(|h| h.is_finite() && *h > 0.0);
    let total = atoms.iter().map(|a| a.bottom).fold(0.0_f32, f32::max);

    let column_height = if let (ColumnFill::Auto, Some(cap)) = (fill, cap) { cap } else {
        // The smallest height that needs at most `count` columns: a
        // greedy fill never needs MORE columns for a taller height, so
        // bisect between the even share and the whole flow.
        let needs = |h: f32| fill_columns(&atoms, h).len();
        let mut lo = total / count as f32;
        let mut hi = total;
        if needs(lo) <= count {
            hi = lo;
        } else {
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
        }
        cap.map_or(hi, |cap| hi.min(cap))
    };

    let firsts = fill_columns(&atoms, column_height);
    let starts: Vec<f32> = firsts.iter().map(|&a| column_start(&atoms, a)).collect();
    let mut tallest = 0.0_f32;
    for (k, &first) in firsts.iter().enumerate() {
        let end = firsts.get(k + 1).copied().unwrap_or(atoms.len());
        let bottom = atoms[first..end]
            .iter()
            .map(|a| a.bottom)
            .fold(starts[k], f32::max);
        tallest = tallest.max(bottom - starts[k]);
    }

    let mut placements: Vec<BoxPlacement> = boxes
        .iter()
        .map(|_| BoxPlacement {
            column: 0,
            line_breaks: Vec::new(),
        })
        .collect();
    let mut column = 0usize;
    for (i, atom) in atoms.iter().enumerate() {
        let opens_column = firsts.get(column + 1) == Some(&i);
        if opens_column {
            column += 1;
        }
        match atom.line {
            None => placements[atom.owner].column = column,
            Some(line) if opens_column => placements[atom.owner].line_breaks.push(line),
            Some(_) => {}
        }
    }

    ColumnPlan {
        starts,
        height: tallest,
        boxes: placements,
    }
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
                }
            })
            .collect();
        #[allow(clippy::cast_precision_loss)] // a line count
        let bottom = top + 20.0 * n as f32;
        FlowBox { top, bottom, lines }
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
}
