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
        basic::{PhysicalSize, PropertyContext, ResolutionContext},
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
    let declared = dom_declared_flags(styled_dom);
    if declared & (DOM_HAS_COLUMN_COUNT | DOM_HAS_COLUMN_WIDTH) == 0 {
        return None;
    }
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
            Some(ColumnWidth::Length(px)) => {
                Some(px.resolve_with_context(&resolve_ctx, PropertyContext::Other))
            }
            Some(ColumnWidth::Auto) | None => None,
        }
    };
    if count.is_none() && width.is_none() {
        return None;
    }
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
}
