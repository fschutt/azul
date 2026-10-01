//! The width half of the automatic table layout: what each column asks for
//! (its min- and max-content, a percentage, a fixed width), how wide that
//! makes the table (its min- and max-content), and how a table width is
//! distributed over the columns.
//!
//! ONE implementation for the two places that need it: the table's
//! intrinsic sizes (`sizing::calculate_table_intrinsic_sizes`, which decide
//! the table's used width in its containing block) and the column widths of
//! its layout (`fc::calculate_column_widths_auto_with_width`). They used to
//! be two different sums, and a table could be sized for columns its layout
//! then did not give it.
//!
//! The rules are CSS Tables 3 (section 3.8 cell measures, 3.9.1 table
//! min/max, 3.9.3 width distribution) - what browsers implement - which is
//! CSS 2.1 17.5.2.2 with its open questions answered:
//!
//! - a column is *constrained* when a cell (or `<col>`) in it has a fixed `width`; its max-content
//!   is then that width (at least its min-content), not its content's max-content;
//! - a column's *percentage* is the largest percentage `width` of its cells; percentages that sum
//!   over 100% are cut back left to right;
//! - the min-content of a column is its cells' min-content - a `width` does not raise it (only
//!   `min-width` does, through the cell's own intrinsic sizes).

use azul_core::{dom::NodeId, styled_dom::StyledDom};
use azul_css::props::layout::{dimensions::LayoutWidth, LayoutBoxSizing};

use crate::solver3::getters::{
    get_css_box_sizing, get_css_width, get_element_font_size, get_root_font_size, MultiValue,
};

/// What one column asks of the table (border-box widths, px).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ColumnConstraint {
    /// The column's min-content width: the widest min-content of its cells.
    pub min: f32,
    /// The column's max-content width (for a constrained column: its fixed width, at least `min`).
    pub max: f32,
    /// The column's percentage width, 0..=100 (0: none).
    pub percent: f32,
    /// A cell or `<col>` in the column has a fixed (length) `width`.
    pub constrained: bool,
}

/// A cell's (or `<col>`'s) specified `width`, as the column algorithm reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpecifiedWidth {
    /// `auto` (or anything the algorithm cannot use: `min-content`, a `calc()`).
    Auto,
    /// A length, as a BORDER-box width in px.
    Fixed(f32),
    /// A percentage of the table, 0..=100.
    Percent(f32),
}

/// The specified `width` of the element `dom_id` (a cell or a `<col>`) as a
/// [`SpecifiedWidth`]. `h_extras` is its horizontal padding + border: a
/// `content-box` length gets them added, so every width here is a border box
/// like the columns' min- and max-content.
#[must_use]
pub fn specified_width(styled_dom: &StyledDom, dom_id: NodeId, h_extras: f32) -> SpecifiedWidth {
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let MultiValue::Exact(LayoutWidth::Px(px)) = get_css_width(styled_dom, dom_id, node_state)
    else {
        return SpecifiedWidth::Auto;
    };
    if let Some(p) = px.to_percent() {
        let percent = p.get() * 100.0;
        return if percent.is_finite() && percent > 0.0 {
            SpecifiedWidth::Percent(percent.min(100.0))
        } else {
            SpecifiedWidth::Auto
        };
    }
    let em = get_element_font_size(styled_dom, dom_id, node_state);
    let rem = get_root_font_size(styled_dom, node_state);
    let Some(w) = crate::solver3::calc::resolve_pixel_value_no_percent(&px, em, rem) else {
        return SpecifiedWidth::Auto;
    };
    if !w.is_finite() {
        return SpecifiedWidth::Auto;
    }
    let border_box = match get_css_box_sizing(styled_dom, dom_id, node_state) {
        MultiValue::Exact(LayoutBoxSizing::BorderBox) => w.max(h_extras),
        _ => w.max(0.0) + h_extras,
    };
    SpecifiedWidth::Fixed(border_box)
}

/// A column being built from its cells: the cells' widest min-content, the
/// widest content max-content, the widest fixed width, the largest
/// percentage.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ColumnAccumulator {
    min: f32,
    content_max: f32,
    fixed_max: Option<f32>,
    percent: f32,
}

impl ColumnAccumulator {
    /// Add one single-column cell (its border-box min- and max-content and
    /// its specified width).
    pub fn add_cell(&mut self, min: f32, max: f32, width: SpecifiedWidth) {
        self.min = self.min.max(min);
        self.content_max = self.content_max.max(max).max(min);
        self.add_width(width);
    }

    /// Add a specified width that comes without content (a `<col>`, or a
    /// cell whose content is measured elsewhere).
    pub fn add_width(&mut self, width: SpecifiedWidth) {
        match width {
            SpecifiedWidth::Auto => {}
            SpecifiedWidth::Fixed(w) => {
                self.fixed_max = Some(self.fixed_max.map_or(w, |f| f.max(w)));
            }
            SpecifiedWidth::Percent(p) => self.percent = self.percent.max(p),
        }
    }

    /// Raise the column's min-/max-content to at least `min` / `max` (a
    /// spanning cell's share, or a measurement made elsewhere).
    pub fn raise(&mut self, min: f32, max: f32) {
        self.min = self.min.max(min);
        self.content_max = self.content_max.max(max);
    }

    /// The column's constraint (CSS Tables 3 3.8: a constrained column's
    /// max-content is its fixed width, never below its min-content).
    #[must_use]
    pub fn finish(self) -> ColumnConstraint {
        let min = self.min.max(0.0);
        match self.fixed_max {
            Some(fixed) => ColumnConstraint {
                min,
                max: fixed.max(min),
                percent: self.percent,
                constrained: true,
            },
            None => ColumnConstraint {
                min,
                max: self.content_max.max(min),
                percent: self.percent,
                constrained: false,
            },
        }
    }
}

/// Cut the columns' percentages back so they sum to at most 100%, left to
/// right: a column whose percentage would pass 100% keeps what is left.
pub fn clamp_percentages(columns: &mut [ColumnConstraint]) {
    let mut used = 0.0f32;
    for c in columns.iter_mut() {
        if c.percent > 0.0 {
            c.percent = c.percent.min((100.0 - used).max(0.0));
            used += c.percent;
        }
    }
}

/// The table's min- and max-content width from its columns, WITHOUT cell
/// spacing and the table's own padding and border (CSS Tables 3 3.9.1).
///
/// The max-content is the sum of the columns' max-content - widened so a
/// percentage column can be its percentage of the table: a 50% column of
/// 100px makes the table 200px, and auto columns of 60px beside 40% of
/// percentage columns make it 100px. (With 100% or more of percentages
/// beside other content no width satisfies them; the sum is used then.)
#[must_use]
pub fn table_min_max(columns: &[ColumnConstraint]) -> (f32, f32) {
    let min: f32 = columns.iter().map(|c| c.min).sum();
    let mut max: f32 = columns.iter().map(|c| c.max.max(c.min)).sum();
    let total_percent: f32 = columns.iter().map(|c| c.percent).sum();
    if total_percent > 0.0 {
        let mut estimate = 0.0f32;
        let mut non_percent_max = 0.0f32;
        for c in columns {
            if c.percent > 0.0 {
                estimate = estimate.max(c.max.max(c.min) * 100.0 / c.percent);
            } else {
                non_percent_max += c.max.max(c.min);
            }
        }
        if total_percent < 100.0 && non_percent_max > 0.0 {
            estimate = estimate.max(non_percent_max * 100.0 / (100.0 - total_percent));
        }
        if estimate.is_finite() {
            max = max.max(estimate);
        }
    }
    (min, max.max(min))
}

/// Distribute `target` (the table's width for its columns: its content
/// width minus the cell spacing) over `columns` (CSS Tables 3 3.9.3).
///
/// Four "sizing guesses" are summed: every column at its min-content; then
/// percentage columns at their percentage of `target`; then constrained
/// columns at their max-content; then auto columns at their max-content.
/// The target falls between two consecutive guesses, and each column takes
/// its value in the lower guess plus a share of the difference to the higher
/// one in proportion to its own difference. Below the first guess every
/// column keeps its min-content (the table overflows). Above the last, the
/// excess goes to the auto columns in proportion to their max-content
/// (equally if all of them are empty), else to the constrained columns,
/// else to the percentage columns in proportion to their percentages, else
/// equally to all.
#[must_use]
pub fn distribute_to_columns(columns: &[ColumnConstraint], target: f32) -> Vec<f32> {
    let n = columns.len();
    if n == 0 {
        return Vec::new();
    }
    if !target.is_finite() {
        return columns.iter().map(|c| c.max.max(c.min)).collect();
    }
    let percent_width = |c: &ColumnConstraint| c.min.max(c.percent * target / 100.0);
    let is_percent = |c: &ColumnConstraint| c.percent > 0.0;
    let is_fixed = |c: &ColumnConstraint| !is_percent(c) && c.constrained;
    let is_auto = |c: &ColumnConstraint| !is_percent(c) && !c.constrained;

    // The guesses, per column: [min, min-percentage, min-specified, max].
    let guess = |c: &ColumnConstraint, stage: usize| -> f32 {
        let max = c.max.max(c.min);
        if is_percent(c) {
            if stage == 0 {
                c.min
            } else {
                percent_width(c)
            }
        } else if is_fixed(c) {
            if stage >= 2 {
                max
            } else {
                c.min
            }
        } else if stage >= 3 {
            max
        } else {
            c.min
        }
    };
    let sums: Vec<f32> = (0..4)
        .map(|stage| columns.iter().map(|c| guess(c, stage)).sum::<f32>())
        .collect();

    if target <= sums[0] {
        return columns.iter().map(|c| c.min).collect();
    }
    for stage in 1..4 {
        if target <= sums[stage] {
            let lower = sums[stage - 1];
            let span = sums[stage] - lower;
            let t = if span > 0.0 {
                (target - lower) / span
            } else {
                0.0
            };
            return columns
                .iter()
                .map(|c| {
                    let a = guess(c, stage - 1);
                    let b = guess(c, stage);
                    a + (b - a) * t
                })
                .collect();
        }
    }

    // Every column at its max-content guess, and width left over.
    let mut widths: Vec<f32> = columns.iter().map(|c| guess(c, 3)).collect();
    let excess = target - sums[3];
    let share = |widths: &mut Vec<f32>,
                 pick: &dyn Fn(&ColumnConstraint) -> bool,
                 weight: &dyn Fn(&ColumnConstraint) -> f32|
     -> bool {
        let total: f32 = columns.iter().filter(|c| pick(c)).map(weight).sum();
        let count = columns.iter().filter(|c| pick(c)).count();
        if count == 0 {
            return false;
        }
        for (w, c) in widths.iter_mut().zip(columns) {
            if pick(c) {
                *w += if total > 0.0 {
                    excess * weight(c) / total
                } else {
                    excess / count as f32
                };
            }
        }
        true
    };
    let max_of = |c: &ColumnConstraint| c.max.max(c.min);
    let one = |_: &ColumnConstraint| 1.0f32;
    let distributed = share(
        &mut widths,
        &|c: &ColumnConstraint| is_auto(c) && max_of(c) > 0.0,
        &max_of,
    ) || share(&mut widths, &is_auto, &one)
        || share(
            &mut widths,
            &|c: &ColumnConstraint| is_fixed(c) && max_of(c) > 0.0,
            &max_of,
        )
        || share(&mut widths, &is_percent, &|c: &ColumnConstraint| c.percent)
        || share(&mut widths, &|_: &ColumnConstraint| true, &one);
    debug_assert!(distributed, "a non-empty table distributes its excess");
    widths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto(min: f32, max: f32) -> ColumnConstraint {
        ColumnConstraint {
            min,
            max,
            percent: 0.0,
            constrained: false,
        }
    }
    fn fixed(min: f32, width: f32) -> ColumnConstraint {
        ColumnConstraint {
            min,
            max: width.max(min),
            percent: 0.0,
            constrained: true,
        }
    }
    fn percent(min: f32, p: f32) -> ColumnConstraint {
        ColumnConstraint {
            min,
            max: min,
            percent: p,
            constrained: false,
        }
    }
    fn close(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
    }

    #[test]
    fn excess_goes_to_auto_columns_in_proportion_to_their_max_content() {
        let w = distribute_to_columns(&[auto(10.0, 50.0), auto(10.0, 150.0)], 400.0);
        assert!(close(&w, &[100.0, 300.0]), "{w:?}");
    }

    #[test]
    fn a_fixed_column_keeps_its_width_when_an_auto_column_can_take_the_rest() {
        let w = distribute_to_columns(&[fixed(10.0, 100.0), auto(10.0, 10.0)], 400.0);
        assert!(close(&w, &[100.0, 300.0]), "{w:?}");
    }

    #[test]
    fn percent_fixed_and_auto() {
        let w = distribute_to_columns(
            &[fixed(10.0, 100.0), percent(10.0, 50.0), auto(10.0, 10.0)],
            400.0,
        );
        assert!(close(&w, &[100.0, 200.0, 100.0]), "{w:?}");
    }

    #[test]
    fn over_a_hundred_percent_is_cut_back_left_to_right() {
        let mut c = [percent(10.0, 80.0), percent(10.0, 80.0)];
        clamp_percentages(&mut c);
        assert!((c[1].percent - 20.0).abs() < 0.01, "{c:?}");
        let w = distribute_to_columns(&c, 400.0);
        assert!(close(&w, &[320.0, 80.0]), "{w:?}");
    }

    #[test]
    fn a_narrow_target_interpolates_the_fixed_columns() {
        let c = [
            fixed(100.0, 200.0),
            fixed(100.0, 200.0),
            fixed(100.0, 200.0),
        ];
        let w = distribute_to_columns(&c, 400.0);
        let third = 400.0 / 3.0;
        assert!(close(&w, &[third, third, third]), "{w:?}");
    }

    #[test]
    fn below_the_min_content_every_column_keeps_its_min() {
        let w = distribute_to_columns(&[auto(100.0, 200.0), auto(50.0, 60.0)], 10.0);
        assert!(close(&w, &[100.0, 50.0]), "{w:?}");
    }

    #[test]
    fn between_min_and_max_auto_columns_grow_by_their_difference() {
        // min 100 + 50, max 300 + 50: target 250 is half way for the first.
        let w = distribute_to_columns(&[auto(100.0, 300.0), auto(50.0, 50.0)], 250.0);
        assert!(close(&w, &[200.0, 50.0]), "{w:?}");
    }

    #[test]
    fn empty_auto_columns_share_the_excess_equally() {
        let w = distribute_to_columns(&[auto(0.0, 0.0), auto(0.0, 0.0)], 100.0);
        assert!(close(&w, &[50.0, 50.0]), "{w:?}");
    }

    #[test]
    fn the_table_is_wide_enough_for_its_percentages() {
        assert_eq!(
            table_min_max(&[percent(100.0, 50.0), auto(20.0, 20.0)]),
            (120.0, 200.0)
        );
        assert_eq!(
            table_min_max(&[auto(30.0, 60.0), percent(0.0, 40.0)]),
            (30.0, 100.0)
        );
        assert_eq!(
            table_min_max(&[auto(30.0, 50.0), auto(20.0, 70.0)]),
            (50.0, 120.0)
        );
    }

    #[test]
    fn a_constrained_column_asks_for_its_width_not_its_content() {
        let mut acc = ColumnAccumulator::default();
        acc.add_cell(10.0, 500.0, SpecifiedWidth::Fixed(100.0));
        let c = acc.finish();
        assert_eq!(c.min, 10.0);
        assert_eq!(c.max, 100.0);
        assert!(c.constrained);
        let mut acc = ColumnAccumulator::default();
        acc.add_cell(150.0, 500.0, SpecifiedWidth::Fixed(100.0));
        assert_eq!(acc.finish().max, 150.0, "never below the min-content");
    }

    #[test]
    fn non_finite_targets_give_every_column_its_max() {
        let w = distribute_to_columns(&[auto(1.0, 5.0)], f32::INFINITY);
        assert!(close(&w, &[5.0]), "{w:?}");
        assert!(distribute_to_columns(&[], 100.0).is_empty());
    }
}
