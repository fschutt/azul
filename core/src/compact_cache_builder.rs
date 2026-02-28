//! Builder function to convert CssPropertyCache → CompactLayoutCache.
//!
//! Called once after restyle + apply_ua_css + compute_inherited_values.
//! Uses typed getters on CssPropertyCache (which cascade through all sources)
//! to resolve each property for the "normal" state (all pseudo-states = false).

use crate::dom::{NodeData, NodeId};
use crate::prop_cache::CssPropertyCache;
use crate::styled_dom::StyledNodeState;
use azul_css::compact_cache::*;
use azul_css::css::CssPropertyValue;
use azul_css::props::basic::length::SizeMetric;
use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
use azul_css::props::layout::flex::LayoutFlexBasis;
use azul_css::props::layout::position::LayoutZIndex;
use core::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

impl CssPropertyCache {
    /// Build a CompactLayoutCache from the current property cache state.
    ///
    /// Must be called after `restyle()`, `apply_ua_css()`, and `compute_inherited_values()`.
    /// Resolves all layout-relevant properties for every node in the "normal" state
    /// (no hover/active/focus) and encodes them into compact tier1/2/2b arrays.
    ///
    /// Fix 3: `tier3_overflow` is no longer populated. `get_property()` falls through
    /// to `get_property_slow()` (cascade binary search) for non-compact properties.
    /// This eliminates the `build_resolved_cache()` startup cost and the per-node
    /// `Vec<(CssPropertyType, CssProperty)>` clone (~5 MB for 500 nodes).
    pub fn build_compact_cache(
        &self,
        node_data: &[NodeData],
    ) -> CompactLayoutCache {
        let node_count = self.node_count;
        let default_state = StyledNodeState::default();
        let mut result = CompactLayoutCache::with_capacity(node_count);

        for i in 0..node_count {
            let node_id = NodeId::new(i);
            let nd = &node_data[i];

            // =====================================================================
            // Tier 1: Encode all 20 enum properties into u64
            // =====================================================================

            let display = self
                .get_display(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let position = self
                .get_position(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let float = self
                .get_float(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let overflow_x = self
                .get_overflow_x(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let overflow_y = self
                .get_overflow_y(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let box_sizing = self
                .get_box_sizing(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let flex_direction = self
                .get_flex_direction(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let flex_wrap = self
                .get_flex_wrap(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let justify_content = self
                .get_justify_content(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let align_items = self
                .get_align_items(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let align_content = self
                .get_align_content(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let writing_mode = self
                .get_writing_mode(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let clear = self
                .get_clear(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let font_weight = self
                .get_font_weight(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let font_style = self
                .get_font_style(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let text_align = self
                .get_text_align(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let visibility = self
                .get_visibility(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let white_space = self
                .get_white_space(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let direction = self
                .get_direction(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();
            let vertical_align = self
                .get_vertical_align(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();

            let border_collapse = self
                .get_border_collapse(nd, &node_id, &default_state)
                .and_then(|v| v.get_property().copied())
                .unwrap_or_default();

            result.tier1_enums[i] = encode_tier1(
                display,
                position,
                float,
                overflow_x,
                overflow_y,
                box_sizing,
                flex_direction,
                flex_wrap,
                justify_content,
                align_items,
                align_content,
                writing_mode,
                clear,
                font_weight,
                font_style,
                text_align,
                visibility,
                white_space,
                direction,
                vertical_align,
                border_collapse,
            );

            // =====================================================================
            // Tier 2: Encode numeric dimension properties
            // =====================================================================

            // Width/Height are enums: Auto | Px(PixelValue) | MinContent | MaxContent | Calc
            if let Some(val) = self.get_width(nd, &node_id, &default_state) {
                result.tier2_dims[i].width = encode_layout_width(val);
            }
            if let Some(val) = self.get_height(nd, &node_id, &default_state) {
                result.tier2_dims[i].height = encode_layout_height(val);
            }

            // Min/Max Width/Height are simple PixelValue wrappers
            if let Some(val) = self.get_min_width(nd, &node_id, &default_state) {
                result.tier2_dims[i].min_width = encode_pixel_prop(val);
            }
            if let Some(val) = self.get_max_width(nd, &node_id, &default_state) {
                result.tier2_dims[i].max_width = encode_pixel_prop(val);
            }
            if let Some(val) = self.get_min_height(nd, &node_id, &default_state) {
                result.tier2_dims[i].min_height = encode_pixel_prop(val);
            }
            if let Some(val) = self.get_max_height(nd, &node_id, &default_state) {
                result.tier2_dims[i].max_height = encode_pixel_prop(val);
            }

            // Flex basis (enum: Auto | Exact(PixelValue))
            if let Some(val) = self.get_flex_basis(nd, &node_id, &default_state) {
                result.tier2_dims[i].flex_basis = encode_flex_basis(val);
            }

            // Font size
            if let Some(val) = self.get_font_size(nd, &node_id, &default_state) {
                result.tier2_dims[i].font_size = encode_pixel_prop(val);
            }

            // Padding (i16 × 10 resolved px)
            if let Some(val) = self.get_padding_top(nd, &node_id, &default_state) {
                result.tier2_dims[i].padding_top = encode_css_pixel_as_i16(val);
            }
            if let Some(val) = self.get_padding_right(nd, &node_id, &default_state) {
                result.tier2_dims[i].padding_right = encode_css_pixel_as_i16(val);
            }
            if let Some(val) = self.get_padding_bottom(nd, &node_id, &default_state) {
                result.tier2_dims[i].padding_bottom = encode_css_pixel_as_i16(val);
            }
            if let Some(val) = self.get_padding_left(nd, &node_id, &default_state) {
                result.tier2_dims[i].padding_left = encode_css_pixel_as_i16(val);
            }

            // Margin (i16, auto is special)
            if let Some(val) = self.get_margin_top(nd, &node_id, &default_state) {
                result.tier2_dims[i].margin_top = encode_margin_i16(val);
            }
            if let Some(val) = self.get_margin_right(nd, &node_id, &default_state) {
                result.tier2_dims[i].margin_right = encode_margin_i16(val);
            }
            if let Some(val) = self.get_margin_bottom(nd, &node_id, &default_state) {
                result.tier2_dims[i].margin_bottom = encode_margin_i16(val);
            }
            if let Some(val) = self.get_margin_left(nd, &node_id, &default_state) {
                result.tier2_dims[i].margin_left = encode_margin_i16(val);
            }

            // Border widths (i16 × 10 resolved px)
            if let Some(val) = self.get_border_top_width(nd, &node_id, &default_state) {
                result.tier2_dims[i].border_top_width = encode_css_pixel_as_i16(val);
            }
            if let Some(val) = self.get_border_right_width(nd, &node_id, &default_state) {
                result.tier2_dims[i].border_right_width = encode_css_pixel_as_i16(val);
            }
            if let Some(val) = self.get_border_bottom_width(nd, &node_id, &default_state) {
                result.tier2_dims[i].border_bottom_width = encode_css_pixel_as_i16(val);
            }
            if let Some(val) = self.get_border_left_width(nd, &node_id, &default_state) {
                result.tier2_dims[i].border_left_width = encode_css_pixel_as_i16(val);
            }

            // Position offsets (top/right/bottom/left)
            if let Some(val) = self.get_top(nd, &node_id, &default_state) {
                result.tier2_dims[i].top = encode_css_pixel_as_i16(val);
            }
            if let Some(val) = self.get_right(nd, &node_id, &default_state) {
                result.tier2_dims[i].right = encode_css_pixel_as_i16(val);
            }
            if let Some(val) = self.get_bottom(nd, &node_id, &default_state) {
                result.tier2_dims[i].bottom = encode_css_pixel_as_i16(val);
            }
            if let Some(val) = self.get_left(nd, &node_id, &default_state) {
                result.tier2_dims[i].left = encode_css_pixel_as_i16(val);
            }

            // Flex grow/shrink (u16 × 100)
            if let Some(val) = self.get_flex_grow(nd, &node_id, &default_state) {
                if let Some(exact) = val.get_property() {
                    result.tier2_dims[i].flex_grow = encode_flex_u16(exact.inner.get());
                }
            }
            if let Some(val) = self.get_flex_shrink(nd, &node_id, &default_state) {
                if let Some(exact) = val.get_property() {
                    result.tier2_dims[i].flex_shrink = encode_flex_u16(exact.inner.get());
                }
            }

            // Z-index
            if let Some(val) = self.get_z_index(nd, &node_id, &default_state) {
                if let Some(exact) = val.get_property() {
                    match exact {
                        LayoutZIndex::Auto => result.tier2_dims[i].z_index = I16_AUTO,
                        LayoutZIndex::Integer(z) => {
                            if *z >= I16_SENTINEL_THRESHOLD as i32 {
                                result.tier2_dims[i].z_index = I16_SENTINEL;
                            } else {
                                result.tier2_dims[i].z_index = *z as i16;
                            }
                        }
                    }
                }
            }

            // Border styles (packed into u16)
            {
                let bts = self.get_border_top_style(nd, &node_id, &default_state)
                    .and_then(|v| v.get_property().copied())
                    .map(|v| v.inner)
                    .unwrap_or_default();
                let brs = self.get_border_right_style(nd, &node_id, &default_state)
                    .and_then(|v| v.get_property().copied())
                    .map(|v| v.inner)
                    .unwrap_or_default();
                let bbs = self.get_border_bottom_style(nd, &node_id, &default_state)
                    .and_then(|v| v.get_property().copied())
                    .map(|v| v.inner)
                    .unwrap_or_default();
                let bls = self.get_border_left_style(nd, &node_id, &default_state)
                    .and_then(|v| v.get_property().copied())
                    .map(|v| v.inner)
                    .unwrap_or_default();
                result.tier2_dims[i].border_styles_packed =
                    encode_border_styles_packed(bts, brs, bbs, bls);
            }

            // Border colors (ColorU → u32 as 0xRRGGBBAA)
            if let Some(val) = self.get_border_top_color(nd, &node_id, &default_state) {
                if let Some(color) = val.get_property() {
                    result.tier2_dims[i].border_top_color = encode_color_u32(&color.inner);
                }
            }
            if let Some(val) = self.get_border_right_color(nd, &node_id, &default_state) {
                if let Some(color) = val.get_property() {
                    result.tier2_dims[i].border_right_color = encode_color_u32(&color.inner);
                }
            }
            if let Some(val) = self.get_border_bottom_color(nd, &node_id, &default_state) {
                if let Some(color) = val.get_property() {
                    result.tier2_dims[i].border_bottom_color = encode_color_u32(&color.inner);
                }
            }
            if let Some(val) = self.get_border_left_color(nd, &node_id, &default_state) {
                if let Some(color) = val.get_property() {
                    result.tier2_dims[i].border_left_color = encode_color_u32(&color.inner);
                }
            }

            // Border spacing (two PixelValue → i16 × 10 resolved px)
            if let Some(val) = self.get_border_spacing(nd, &node_id, &default_state) {
                if let Some(spacing) = val.get_property() {
                    if spacing.horizontal.metric == SizeMetric::Px {
                        result.tier2_dims[i].border_spacing_h = encode_resolved_px_i16(spacing.horizontal.number.get());
                    }
                    if spacing.vertical.metric == SizeMetric::Px {
                        result.tier2_dims[i].border_spacing_v = encode_resolved_px_i16(spacing.vertical.number.get());
                    }
                }
            }

            // Tab size (PixelValue → i16 × 10 resolved px)
            if let Some(val) = self.get_tab_size(nd, &node_id, &default_state) {
                result.tier2_dims[i].tab_size = encode_css_pixel_as_i16(val);
            }

            // =====================================================================
            // Tier 2b: Text properties
            // =====================================================================

            // Text color (ColorU → u32 as 0xRRGGBBAA)
            if let Some(val) = self.get_text_color(nd, &node_id, &default_state) {
                if let Some(color) = val.get_property() {
                    let c = &color.inner;
                    result.tier2b_text[i].text_color =
                        ((c.r as u32) << 24) | ((c.g as u32) << 16) | ((c.b as u32) << 8) | (c.a as u32);
                }
            }

            // Font-family (hash the whole StyleFontFamilyVec for fast comparison)
            if let Some(val) = self.get_font_family(nd, &node_id, &default_state) {
                if let Some(families) = val.get_property() {
                    let mut hasher = DefaultHasher::new();
                    families.hash(&mut hasher);
                    let h = hasher.finish();
                    // 0 is reserved as "unset" sentinel, avoid collision
                    result.tier2b_text[i].font_family_hash = if h == 0 { 1 } else { h };
                }
            }

            // Line-height (PercentageValue: internal number is value × 1000, we store % × 10)
            if let Some(val) = self.get_line_height(nd, &node_id, &default_state) {
                if let Some(lh) = val.get_property() {
                    // lh.inner is PercentageValue, normalized() = value/100.
                    // Internal number = percentage × 1000 (e.g. 120% → 120_000).
                    // We store percentage × 10 as i16 (e.g. 120% → 1200).
                    let pct_x10 = (lh.inner.normalized() * 1000.0).round() as i32;
                    if pct_x10 >= -32768 && pct_x10 < I16_SENTINEL_THRESHOLD as i32 {
                        result.tier2b_text[i].line_height = pct_x10 as i16;
                    } else {
                        result.tier2b_text[i].line_height = I16_SENTINEL;
                    }
                }
            }

            // Letter-spacing (PixelValue wrapper → i16 × 10 resolved px)
            if let Some(val) = self.get_letter_spacing(nd, &node_id, &default_state) {
                result.tier2b_text[i].letter_spacing = encode_css_pixel_as_i16(val);
            }

            // Word-spacing (PixelValue wrapper → i16 × 10 resolved px)
            if let Some(val) = self.get_word_spacing(nd, &node_id, &default_state) {
                result.tier2b_text[i].word_spacing = encode_css_pixel_as_i16(val);
            }

            // Text-indent (PixelValue wrapper → i16 × 10 resolved px)
            if let Some(val) = self.get_text_indent(nd, &node_id, &default_state) {
                result.tier2b_text[i].text_indent = encode_css_pixel_as_i16(val);
            }
        }

        // Fix 3: tier3_overflow is no longer populated here.
        // get_property() calls get_property_slow() directly, which walks the
        // cascade layers (already sorted Vecs + compact inline table).

        result
    }
}

// =============================================================================
// Helper encoders for dimension properties
// =============================================================================

/// Encode a CssPropertyValue<LayoutWidth> into u32 compact form.
fn encode_layout_width<T: LayoutWidthLike>(val: &CssPropertyValue<T>) -> u32 {
    match val {
        CssPropertyValue::Exact(w) => w.encode_compact_u32(),
        CssPropertyValue::Auto => U32_AUTO,
        CssPropertyValue::Initial => U32_INITIAL,
        CssPropertyValue::Inherit => U32_INHERIT,
        CssPropertyValue::None => U32_NONE,
        _ => U32_SENTINEL,
    }
}

/// Encode a CssPropertyValue<LayoutHeight> into u32 compact form.
fn encode_layout_height<T: LayoutWidthLike>(val: &CssPropertyValue<T>) -> u32 {
    encode_layout_width(val)
}

/// Trait for types that can be encoded as compact u32 dimension values.
/// Implemented for LayoutWidth, LayoutHeight (which are Auto|Px|MinContent|MaxContent|Calc enums).
trait LayoutWidthLike {
    fn encode_compact_u32(&self) -> u32;
}

impl LayoutWidthLike for LayoutWidth {
    fn encode_compact_u32(&self) -> u32 {
        match self {
            LayoutWidth::Auto => U32_AUTO,
            LayoutWidth::Px(pv) => encode_pixel_value_u32(pv),
            LayoutWidth::MinContent => U32_MIN_CONTENT,
            LayoutWidth::MaxContent => U32_MAX_CONTENT,
            LayoutWidth::Calc(_) => U32_SENTINEL, // Calc → overflow to tier 3
        }
    }
}

impl LayoutWidthLike for LayoutHeight {
    fn encode_compact_u32(&self) -> u32 {
        match self {
            LayoutHeight::Auto => U32_AUTO,
            LayoutHeight::Px(pv) => encode_pixel_value_u32(pv),
            LayoutHeight::MinContent => U32_MIN_CONTENT,
            LayoutHeight::MaxContent => U32_MAX_CONTENT,
            LayoutHeight::Calc(_) => U32_SENTINEL,
        }
    }
}

/// Encode a CssPropertyValue wrapping a simple PixelValue struct (LayoutMinWidth, etc.)
fn encode_pixel_prop<T: HasInnerPixelValue>(val: &CssPropertyValue<T>) -> u32 {
    match val {
        CssPropertyValue::Exact(inner) => encode_pixel_value_u32(&inner.get_inner_pixel()),
        CssPropertyValue::Auto => U32_AUTO,
        CssPropertyValue::Initial => U32_INITIAL,
        CssPropertyValue::Inherit => U32_INHERIT,
        CssPropertyValue::None => U32_NONE,
        _ => U32_SENTINEL,
    }
}

/// Trait for dimension structs wrapping `inner: PixelValue`.
trait HasInnerPixelValue {
    fn get_inner_pixel(&self) -> azul_css::props::basic::pixel::PixelValue;
}

macro_rules! impl_has_inner_pixel {
    ($($ty:ty),*) => {
        $(
            impl HasInnerPixelValue for $ty {
                fn get_inner_pixel(&self) -> azul_css::props::basic::pixel::PixelValue {
                    self.inner
                }
            }
        )*
    };
}

impl_has_inner_pixel!(
    azul_css::props::layout::dimensions::LayoutMinWidth,
    azul_css::props::layout::dimensions::LayoutMaxWidth,
    azul_css::props::layout::dimensions::LayoutMinHeight,
    azul_css::props::layout::dimensions::LayoutMaxHeight,
    azul_css::props::basic::font::StyleFontSize,
    azul_css::props::layout::spacing::LayoutPaddingTop,
    azul_css::props::layout::spacing::LayoutPaddingRight,
    azul_css::props::layout::spacing::LayoutPaddingBottom,
    azul_css::props::layout::spacing::LayoutPaddingLeft,
    azul_css::props::layout::spacing::LayoutMarginTop,
    azul_css::props::layout::spacing::LayoutMarginRight,
    azul_css::props::layout::spacing::LayoutMarginBottom,
    azul_css::props::layout::spacing::LayoutMarginLeft,
    azul_css::props::style::border::LayoutBorderTopWidth,
    azul_css::props::style::border::LayoutBorderRightWidth,
    azul_css::props::style::border::LayoutBorderBottomWidth,
    azul_css::props::style::border::LayoutBorderLeftWidth,
    azul_css::props::layout::position::LayoutTop,
    azul_css::props::layout::position::LayoutRight,
    azul_css::props::layout::position::LayoutInsetBottom,
    azul_css::props::layout::position::LayoutLeft,
    azul_css::props::style::text::StyleLetterSpacing,
    azul_css::props::style::text::StyleWordSpacing,
    azul_css::props::style::text::StyleTextIndent,
    azul_css::props::style::text::StyleTabSize
);

/// Encode a CssPropertyValue<T> where T wraps a PixelValue, as i16 (×10 resolved px).
/// Only encodes absolute `px` values; everything else → sentinel.
fn encode_css_pixel_as_i16<T: HasInnerPixelValue>(val: &CssPropertyValue<T>) -> i16 {
    match val {
        CssPropertyValue::Exact(inner) => {
            let pv = inner.get_inner_pixel();
            if pv.metric == SizeMetric::Px {
                encode_resolved_px_i16(pv.number.get())
            } else {
                I16_SENTINEL // non-px units need resolution context → slow path
            }
        }
        CssPropertyValue::Auto => I16_AUTO,
        CssPropertyValue::Initial => I16_INITIAL,
        CssPropertyValue::Inherit => I16_INHERIT,
        _ => I16_SENTINEL,
    }
}

/// Encode margin: same as encode_css_pixel_as_i16 but Auto is a distinct value.
fn encode_margin_i16<T: HasInnerPixelValue>(val: &CssPropertyValue<T>) -> i16 {
    encode_css_pixel_as_i16(val)
}

/// Encode CssPropertyValue<LayoutFlexBasis> — LayoutFlexBasis is Auto | Exact(PixelValue).
fn encode_flex_basis(val: &CssPropertyValue<LayoutFlexBasis>) -> u32 {
    match val {
        CssPropertyValue::Exact(fb) => match fb {
            LayoutFlexBasis::Auto => U32_AUTO,
            LayoutFlexBasis::Exact(pv) => encode_pixel_value_u32(pv),
        },
        CssPropertyValue::Auto => U32_AUTO,
        CssPropertyValue::Initial => U32_INITIAL,
        CssPropertyValue::Inherit => U32_INHERIT,
        CssPropertyValue::None => U32_NONE,
        _ => U32_SENTINEL,
    }
}
