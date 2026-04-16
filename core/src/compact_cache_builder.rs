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
use azul_css::props::property::CssProperty;
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
    /// (no hover/active/focus) and encodes them into compact arrays.
    ///
    /// Tier 1/2/2b provide fast-path access for layout-hot properties.
    /// Non-compact properties (background, transform, box-shadow, etc.) are
    /// resolved via the slow cascade path in `get_property_slow()`.
    ///
    /// `prev_font_hashes` is the per-node font hash array from the previous frame.
    /// When non-empty, each node's new `font_family_hash` is compared against the
    /// previous value, and differing nodes are recorded in `font_dirty_nodes`.
    /// On the first build (empty slice), ALL text nodes are marked dirty.
    pub fn build_compact_cache(
        &self,
        node_data: &[NodeData],
        prev_font_hashes: &[u64],
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

            // =====================================================================
            // Tier 2 cold: Paint-only properties
            // =====================================================================

            // Z-index
            if let Some(val) = self.get_z_index(nd, &node_id, &default_state) {
                if let Some(exact) = val.get_property() {
                    match exact {
                        LayoutZIndex::Auto => result.tier2_cold[i].z_index = I16_AUTO,
                        LayoutZIndex::Integer(z) => {
                            if *z >= I16_SENTINEL_THRESHOLD as i32 {
                                result.tier2_cold[i].z_index = I16_SENTINEL;
                            } else {
                                result.tier2_cold[i].z_index = *z as i16;
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
                result.tier2_cold[i].border_styles_packed =
                    encode_border_styles_packed(bts, brs, bbs, bls);
            }

            // Border colors (ColorU → u32 as 0xRRGGBBAA)
            if let Some(val) = self.get_border_top_color(nd, &node_id, &default_state) {
                if let Some(color) = val.get_property() {
                    result.tier2_cold[i].border_top_color = encode_color_u32(&color.inner);
                }
            }
            if let Some(val) = self.get_border_right_color(nd, &node_id, &default_state) {
                if let Some(color) = val.get_property() {
                    result.tier2_cold[i].border_right_color = encode_color_u32(&color.inner);
                }
            }
            if let Some(val) = self.get_border_bottom_color(nd, &node_id, &default_state) {
                if let Some(color) = val.get_property() {
                    result.tier2_cold[i].border_bottom_color = encode_color_u32(&color.inner);
                }
            }
            if let Some(val) = self.get_border_left_color(nd, &node_id, &default_state) {
                if let Some(color) = val.get_property() {
                    result.tier2_cold[i].border_left_color = encode_color_u32(&color.inner);
                }
            }

            // Border spacing (two PixelValue → i16 × 10 resolved px)
            if let Some(val) = self.get_border_spacing(nd, &node_id, &default_state) {
                if let Some(spacing) = val.get_property() {
                    if spacing.horizontal.metric == SizeMetric::Px {
                        result.tier2_cold[i].border_spacing_h = encode_resolved_px_i16(spacing.horizontal.number.get());
                    }
                    if spacing.vertical.metric == SizeMetric::Px {
                        result.tier2_cold[i].border_spacing_v = encode_resolved_px_i16(spacing.vertical.number.get());
                    }
                }
            }

            // Tab size (PixelValue → i16 × 10 resolved px)
            if let Some(val) = self.get_tab_size(nd, &node_id, &default_state) {
                result.tier2_cold[i].tab_size = encode_css_pixel_as_i16(val);
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
                    let h = if h == 0 { 1 } else { h };
                    result.tier2b_text[i].font_family_hash = h;
                    result.font_hash_to_families.insert(h, families.clone());
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

        // =====================================================================
        // Per-node font dirty tracking (P4)
        // Compare each node's font_family_hash against the previous frame's hash.
        // Nodes whose hash changed are recorded in font_dirty_nodes for
        // incremental font chain re-resolution instead of all-or-nothing.
        // =====================================================================
        result.font_dirty_nodes.clear();
        for i in 0..node_count {
            let new_hash = result.tier2b_text[i].font_family_hash;
            let old_hash = prev_font_hashes.get(i).copied().unwrap_or(0);
            if new_hash != old_hash {
                result.font_dirty_nodes.push(i);
            }
        }
        // Save current hashes as prev_font_hashes for next frame comparison
        result.prev_font_hashes = result.tier2b_text.iter().map(|t| t.font_family_hash).collect();

        result
    }

    /// Build compact cache with inheritance in a single pass.
    ///
    /// Replaces the separate `compute_inherited_values()` + `build_compact_cache()` calls.
    /// For each node (in DOM index order, which is pre-order = parents before children):
    ///   1. Copy parent's compact values for INHERITABLE properties
    ///   2. Apply this node's CSS properties on top (from css_props + inline + UA)
    ///   3. Write directly to compact arrays
    ///
    /// This eliminates 50K Vec clones from compute_inherited_values and
    /// avoids re-reading properties from 5 separate data structures.
    pub fn build_compact_cache_with_inheritance(
        &self,
        node_data: &[NodeData],
        node_hierarchy: &[crate::styled_dom::NodeHierarchyItem],
        prev_font_hashes: &[u64],
    ) -> CompactLayoutCache {
        self.build_compact_cache_with_inheritance_debug(node_data, node_hierarchy, prev_font_hashes, &mut None)
    }

    /// Same as `build_compact_cache_with_inheritance` but with optional debug logging.
    pub fn build_compact_cache_with_inheritance_debug(
        &self,
        node_data: &[NodeData],
        node_hierarchy: &[crate::styled_dom::NodeHierarchyItem],
        prev_font_hashes: &[u64],
        debug_messages: &mut Option<Vec<azul_css::LayoutDebugMessage>>,
    ) -> CompactLayoutCache {
        let node_count = self.node_count;
        let default_state = StyledNodeState::default();
        let mut result = CompactLayoutCache::with_capacity(node_count);

        // Pre-encode global CSS properties (from `*` rules) into compact form.
        // These are applied as baseline for every node before inheritance.
        let mut global_tier1: u64 = 0;
        let mut global_dims = CompactNodeProps::default();
        let mut global_cold = CompactNodePropsCold::default();
        let mut global_text = CompactTextProps::default();
        let has_global = !self.global_css_props.is_empty();

        if has_global {
            use azul_css::props::property::CssProperty;

            for prop in &self.global_css_props {
                // Apply each global property to the pre-encoded compact values
                macro_rules! global_tier1_enum {
                    ($variant:ident, $shift:ident, $mask:ident, $encoder:ident) => {
                        if let CssProperty::$variant(v) = prop {
                            if let Some(exact) = v.get_property() {
                                let encoded = $encoder(*exact) as u64;
                                let shifted_mask = $mask << $shift;
                                global_tier1 = (global_tier1 & !shifted_mask) | ((encoded & $mask) << $shift);
                            }
                        }
                    };
                }

                global_tier1_enum!(Display, DISPLAY_SHIFT, DISPLAY_MASK, layout_display_to_u8);
                global_tier1_enum!(Position, POSITION_SHIFT, POSITION_MASK, layout_position_to_u8);
                global_tier1_enum!(Float, FLOAT_SHIFT, FLOAT_MASK, layout_float_to_u8);
                global_tier1_enum!(OverflowX, OVERFLOW_X_SHIFT, OVERFLOW_MASK, layout_overflow_to_u8);
                global_tier1_enum!(OverflowY, OVERFLOW_Y_SHIFT, OVERFLOW_MASK, layout_overflow_to_u8);
                global_tier1_enum!(BoxSizing, BOX_SIZING_SHIFT, BOX_SIZING_MASK, layout_box_sizing_to_u8);
                global_tier1_enum!(FlexDirection, FLEX_DIRECTION_SHIFT, FLEX_DIR_MASK, layout_flex_direction_to_u8);
                global_tier1_enum!(FlexWrap, FLEX_WRAP_SHIFT, FLEX_WRAP_MASK, layout_flex_wrap_to_u8);
                global_tier1_enum!(JustifyContent, JUSTIFY_CONTENT_SHIFT, JUSTIFY_MASK, layout_justify_content_to_u8);
                global_tier1_enum!(AlignItems, ALIGN_ITEMS_SHIFT, ALIGN_MASK, layout_align_items_to_u8);
                global_tier1_enum!(AlignContent, ALIGN_CONTENT_SHIFT, ALIGN_MASK, layout_align_content_to_u8);
                global_tier1_enum!(Clear, CLEAR_SHIFT, CLEAR_MASK, layout_clear_to_u8);
                global_tier1_enum!(Visibility, VISIBILITY_SHIFT, VISIBILITY_MASK, style_visibility_to_u8);
                global_tier1_enum!(WritingMode, WRITING_MODE_SHIFT, WRITING_MODE_MASK, layout_writing_mode_to_u8);
                global_tier1_enum!(FontWeight, FONT_WEIGHT_SHIFT, FONT_WEIGHT_MASK, style_font_weight_to_u8);
                global_tier1_enum!(FontStyle, FONT_STYLE_SHIFT, FONT_STYLE_MASK, style_font_style_to_u8);
                global_tier1_enum!(TextAlign, TEXT_ALIGN_SHIFT, TEXT_ALIGN_MASK, style_text_align_to_u8);
                global_tier1_enum!(WhiteSpace, WHITE_SPACE_SHIFT, WHITE_SPACE_MASK, style_white_space_to_u8);
                global_tier1_enum!(Direction, DIRECTION_SHIFT, DIRECTION_MASK, style_direction_to_u8);
                global_tier1_enum!(VerticalAlign, VERTICAL_ALIGN_SHIFT, VERTICAL_ALIGN_MASK, style_vertical_align_to_u8);
                global_tier1_enum!(BorderCollapse, BORDER_COLLAPSE_SHIFT, BORDER_COLLAPSE_MASK, border_collapse_to_u8);

                // Tier 2 dims
                match prop {
                    CssProperty::PaddingTop(v) => { global_dims.padding_top = encode_css_pixel_as_i16(v); }
                    CssProperty::PaddingRight(v) => { global_dims.padding_right = encode_css_pixel_as_i16(v); }
                    CssProperty::PaddingBottom(v) => { global_dims.padding_bottom = encode_css_pixel_as_i16(v); }
                    CssProperty::PaddingLeft(v) => { global_dims.padding_left = encode_css_pixel_as_i16(v); }
                    CssProperty::MarginTop(v) => { global_dims.margin_top = encode_margin_i16(v); }
                    CssProperty::MarginRight(v) => { global_dims.margin_right = encode_margin_i16(v); }
                    CssProperty::MarginBottom(v) => { global_dims.margin_bottom = encode_margin_i16(v); }
                    CssProperty::MarginLeft(v) => { global_dims.margin_left = encode_margin_i16(v); }
                    CssProperty::Width(v) => { global_dims.width = encode_layout_width(v); }
                    CssProperty::Height(v) => { global_dims.height = encode_layout_height(v); }
                    CssProperty::FontSize(v) => { global_dims.font_size = encode_pixel_prop(v); }
                    CssProperty::BorderTopWidth(v) => { global_dims.border_top_width = encode_css_pixel_as_i16(v); }
                    CssProperty::BorderRightWidth(v) => { global_dims.border_right_width = encode_css_pixel_as_i16(v); }
                    CssProperty::BorderBottomWidth(v) => { global_dims.border_bottom_width = encode_css_pixel_as_i16(v); }
                    CssProperty::BorderLeftWidth(v) => { global_dims.border_left_width = encode_css_pixel_as_i16(v); }
                    _ => {}
                }
            }

            if global_tier1 != 0 {
                global_tier1 |= TIER1_POPULATED_BIT;
            }
        }

        // Helper: push debug message if debug_messages is Some
        macro_rules! cascade_debug {
            ($($arg:tt)*) => {
                if let Some(ref mut msgs) = debug_messages {
                    msgs.push(azul_css::LayoutDebugMessage::css_getter(format!($($arg)*)));
                }
            };
        }

        for i in 0..node_count {
            let node_id = NodeId::new(i);
            let nd = &node_data[i];

            // Step 0: Apply UA CSS defaults first (lowest priority).
            // Then global `*` rules override UA (higher priority).
            // Then per-node CSS (Step 3) overrides both.
            //
            // CSS cascade priority: UA < author `*` < author specific < inline

            // Step 1: Inherit from parent's COMPACT values (not computed_values)
            // Parent index is always < i in pre-order arena, so already computed.
            //
            // Step 1: Inherit ONLY inheritable CSS properties from parent.
            // Non-inheritable fields (display, position, float, overflow, box-sizing,
            // flex-*, clear, vertical-align, writing-mode) stay at 0 (CSS initial value).
            // They get set by UA CSS (Step 2) and author CSS (Step 3).
            const INHERITABLE_TIER1_MASK: u64 =
                (FONT_WEIGHT_MASK << FONT_WEIGHT_SHIFT)
                | (FONT_STYLE_MASK << FONT_STYLE_SHIFT)
                | (TEXT_ALIGN_MASK << TEXT_ALIGN_SHIFT)
                | (VISIBILITY_MASK << VISIBILITY_SHIFT)
                | (WHITE_SPACE_MASK << WHITE_SPACE_SHIFT)
                | (DIRECTION_MASK << DIRECTION_SHIFT)
                | (BORDER_COLLAPSE_MASK << BORDER_COLLAPSE_SHIFT);

            let parent_id = node_hierarchy[i].parent_id();
            if let Some(pid) = parent_id {
                let pi = pid.index();

                // Copy only inheritable tier1 fields from parent
                result.tier1_enums[i] = result.tier1_enums[pi] & INHERITABLE_TIER1_MASK;

                // Inheritable tier2: font_size
                result.tier2_dims[i].font_size = result.tier2_dims[pi].font_size;

                // Inheritable tier2_cold: border_spacing, tab_size
                result.tier2_cold[i].border_spacing_h = result.tier2_cold[pi].border_spacing_h;
                result.tier2_cold[i].border_spacing_v = result.tier2_cold[pi].border_spacing_v;
                result.tier2_cold[i].tab_size = result.tier2_cold[pi].tab_size;

                // Inheritable tier2b: all text properties
                result.tier2b_text[i] = result.tier2b_text[pi];
            }

            {
                let d = &result.tier2_dims[i];
                cascade_debug!("node[{}] {:?} after-inherit: pt={} pb={} pl={} pr={} mt={} mb={} ml={} mr={} w={} h={}",
                    i, nd.node_type, d.padding_top, d.padding_bottom, d.padding_left, d.padding_right,
                    d.margin_top, d.margin_bottom, d.margin_left, d.margin_right, d.width, d.height);
            }

            // Step 2: Apply UA CSS defaults for this node type directly to compact values.
            // UA defaults have lowest cascade priority — overridden by author CSS below.
            // This replaces the separate apply_ua_css() pass + cascaded_props + sort.
            {
                // Apply ALL UA CSS defaults for this node type.
                // Use apply_css_property_to_compact for consistent encoding.
                // This replaces the incomplete property list that missed overflow, position, etc.
                use azul_css::props::property::CssPropertyType as PT2;
                const UA_PROPERTY_TYPES: &[PT2] = &[
                    // Tier1 enum properties
                    PT2::Display, PT2::Position, PT2::Float, PT2::Clear,
                    PT2::OverflowX, PT2::OverflowY, PT2::BoxSizing,
                    PT2::FlexDirection, PT2::FlexWrap, PT2::JustifyContent,
                    PT2::AlignItems, PT2::AlignContent, PT2::WritingMode,
                    PT2::FontWeight, PT2::FontStyle, PT2::TextAlign,
                    PT2::Visibility, PT2::WhiteSpace, PT2::Direction,
                    PT2::VerticalAlign, PT2::BorderCollapse,
                    // Tier2 dimension properties
                    PT2::Width, PT2::Height, PT2::FontSize,
                    PT2::MarginTop, PT2::MarginBottom, PT2::MarginLeft, PT2::MarginRight,
                    PT2::PaddingTop, PT2::PaddingBottom, PT2::PaddingLeft, PT2::PaddingRight,
                    PT2::BorderTopWidth, PT2::BorderTopStyle, PT2::BorderTopColor,
                    PT2::BorderRightWidth, PT2::BorderRightStyle, PT2::BorderRightColor,
                    PT2::BorderBottomWidth, PT2::BorderBottomStyle, PT2::BorderBottomColor,
                    PT2::BorderLeftWidth, PT2::BorderLeftStyle, PT2::BorderLeftColor,
                    // Text properties
                    PT2::TextColor, PT2::LineHeight, PT2::LetterSpacing, PT2::WordSpacing,
                    PT2::TextDecoration, PT2::Cursor, PT2::ListStyleType,
                ];
                for pt in UA_PROPERTY_TYPES {
                    if let Some(ua_prop) = crate::ua_css::get_ua_property(&nd.node_type, *pt) {
                        apply_css_property_to_compact(
                            ua_prop,
                            &mut result.tier1_enums[i],
                            &mut result.tier2_dims[i],
                            &mut result.tier2_cold[i],
                            &mut result.tier2b_text[i],
                            &mut result.font_hash_to_families,
                        );
                    }
                }
            }

            {
                let d = &result.tier2_dims[i];
                cascade_debug!("node[{}] {:?} after-UA: pt={} pb={} pl={} pr={} mt={} mb={} ml={} mr={}",
                    i, nd.node_type, d.padding_top, d.padding_bottom, d.padding_left, d.padding_right,
                    d.margin_top, d.margin_bottom, d.margin_left, d.margin_right);
            }

            // Step 2.5: Apply global `*` author CSS (overrides UA, overridden by specific rules)
            // Apply each `*` rule property individually (not bulk-assign) so we only
            // override properties the `*` rule actually set, preserving UA CSS for others.
            //
            // Per CSS spec, `*` matches all ELEMENTS. Text nodes are not elements —
            // they must only inherit from their parent. Without this check, `* { color: #666 }`
            // would overwrite the inherited `color: red` on a Text child of `<p>`,
            // even though `<p>` correctly got red from `p { color: red }`.
            if !nd.is_text_node() {
                for prop in self.global_css_props.iter() {
                    apply_css_property_to_compact(
                        prop,
                        &mut result.tier1_enums[i],
                        &mut result.tier2_dims[i],
                        &mut result.tier2_cold[i],
                        &mut result.tier2b_text[i],
                        &mut result.font_hash_to_families,
                    );
                }
            }

            {
                let d = &result.tier2_dims[i];
                cascade_debug!("node[{}] {:?} after-global-star: pt={} pb={} pl={} pr={} mt={} mb={} ml={} mr={}",
                    i, nd.node_type, d.padding_top, d.padding_bottom, d.padding_left, d.padding_right,
                    d.margin_top, d.margin_bottom, d.margin_left, d.margin_right);
                let n_props = self.css_props.get_slice(i).len();
                let n_inline = nd.css_props.as_ref().len();
                cascade_debug!("node[{}] css_props={} entries, inline={} entries", i, n_props, n_inline);
                for prop in self.css_props.get_slice(i) {
                    cascade_debug!("node[{}]   css_prop: state={:?} type={:?}", i, prop.state, prop.prop_type);
                }
            }

            // Step 3: Apply this node's CSS properties directly to compact values.
            // Per-node author CSS has higher specificity than global `*`.

            // Scan css_props (stylesheet rules, sorted by (state, prop_type))
            // Typically 5-15 entries per node. Only Normal state matters for layout.
            for prop in self.css_props.get_slice(i) {
                if prop.state != azul_css::dynamic_selector::PseudoStateType::Normal { continue; }
                apply_css_property_to_compact(
                    &prop.property,
                    &mut result.tier1_enums[i],
                    &mut result.tier2_dims[i],
                    &mut result.tier2_cold[i],
                    &mut result.tier2b_text[i],
                    &mut result.font_hash_to_families,
                );
            }

            {
                let d = &result.tier2_dims[i];
                cascade_debug!("node[{}] {:?} after-css-props: pt={} pb={} pl={} pr={} mt={} mb={} ml={} mr={}",
                    i, nd.node_type, d.padding_top, d.padding_bottom, d.padding_left, d.padding_right,
                    d.margin_top, d.margin_bottom, d.margin_left, d.margin_right);
            }

            // Scan inline CSS (node_data.css_props, typically 0-3 entries)
            // Inline CSS has highest specificity — applied last to override stylesheet.
            for inline in nd.css_props.as_ref().iter() {
                // Only apply Normal state (no pseudo-selectors like :hover)
                let is_normal = inline.apply_if.as_slice().is_empty()
                    || inline.apply_if.as_slice().iter().all(|c|
                        matches!(c, azul_css::dynamic_selector::DynamicSelector::PseudoState(
                            azul_css::dynamic_selector::PseudoStateType::Normal
                        ))
                    );
                if !is_normal { continue; }
                apply_css_property_to_compact(
                    &inline.property,
                    &mut result.tier1_enums[i],
                    &mut result.tier2_dims[i],
                    &mut result.tier2_cold[i],
                    &mut result.tier2b_text[i],
                    &mut result.font_hash_to_families,
                );
            }

            // Resolve font-size from em/percent/pt/etc. to px.
            // CSS 2.1: inherited font-size is the COMPUTED (px) value, not the specified value.
            // Pre-order traversal guarantees parent's font_size is already resolved.
            {
                let raw_fs = result.tier2_dims[i].font_size;
                if raw_fs != U32_SENTINEL && raw_fs < U32_SENTINEL_THRESHOLD {
                    if let Some(pv) = decode_pixel_value_u32(raw_fs) {
                        if pv.metric != SizeMetric::Px {
                            let parent_font_size_px = parent_id
                                .map(|pid| {
                                    decode_pixel_value_u32(result.tier2_dims[pid.index()].font_size)
                                        .map(|ppv| ppv.number.get())
                                        .unwrap_or(16.0)
                                })
                                .unwrap_or(16.0);

                            let resolved_px = match pv.metric {
                                SizeMetric::Em => pv.number.get() * parent_font_size_px,
                                SizeMetric::Percent => pv.number.get() / 100.0 * parent_font_size_px,
                                SizeMetric::Rem => {
                                    decode_pixel_value_u32(result.tier2_dims[0].font_size)
                                        .map(|rpv| rpv.number.get())
                                        .unwrap_or(16.0)
                                        * pv.number.get()
                                }
                                SizeMetric::Pt => pv.number.get() * 96.0 / 72.0,
                                _ => pv.number.get(),
                            };
                            result.tier2_dims[i].font_size =
                                encode_pixel_value_u32(&azul_css::props::basic::pixel::PixelValue::px(resolved_px));
                        }
                    }
                }
            }

            // Set populated bit
            if result.tier1_enums[i] != 0 {
                result.tier1_enums[i] |= TIER1_POPULATED_BIT;
            }
        }

        // Font dirty tracking.
        // When prev_font_hashes is empty (first build for this DOM), mark ALL
        // text nodes dirty to force font resolution. Without this, a DOM with
        // no explicit font-family (all hashes 0) would compare 0==0 and skip
        // resolution, even though font-weight/font-style may differ from the
        // cached chains of a previous DOM.
        result.font_dirty_nodes.clear();
        let first_build = prev_font_hashes.is_empty();
        for i in 0..node_count {
            let new_hash = result.tier2b_text[i].font_family_hash;
            let old_hash = prev_font_hashes.get(i).copied().unwrap_or(0);
            if first_build || new_hash != old_hash {
                result.font_dirty_nodes.push(i);
            }
        }
        result.prev_font_hashes = result.tier2b_text.iter().map(|t| t.font_family_hash).collect();

        result
    }
}

// =============================================================================
// Direct CssProperty → compact field writer
// =============================================================================

/// Apply a single CssProperty directly to the compact representation.
/// Called once per property per node — replaces the old 56+ getter approach.
#[inline]
fn apply_css_property_to_compact(
    prop: &CssProperty,
    tier1: &mut u64,
    dims: &mut CompactNodeProps,
    cold: &mut CompactNodePropsCold,
    text: &mut CompactTextProps,
    font_hash_map: &mut alloc::collections::BTreeMap<u64, azul_css::props::basic::font::StyleFontFamilyVec>,
) {
    macro_rules! set_tier1 {
        ($v:expr, $shift:expr, $mask:expr, $encoder:ident) => {
            if let Some(exact) = $v.get_property() {
                let encoded = $encoder(*exact) as u64;
                let shifted_mask = $mask << $shift;
                *tier1 = (*tier1 & !shifted_mask) | ((encoded & $mask) << $shift);
            }
        };
    }

    match prop {
        // Tier 1 enums
        CssProperty::Display(v) => set_tier1!(v, DISPLAY_SHIFT, DISPLAY_MASK, layout_display_to_u8),
        CssProperty::Position(v) => set_tier1!(v, POSITION_SHIFT, POSITION_MASK, layout_position_to_u8),
        CssProperty::Float(v) => set_tier1!(v, FLOAT_SHIFT, FLOAT_MASK, layout_float_to_u8),
        CssProperty::OverflowX(v) => set_tier1!(v, OVERFLOW_X_SHIFT, OVERFLOW_MASK, layout_overflow_to_u8),
        CssProperty::OverflowY(v) => set_tier1!(v, OVERFLOW_Y_SHIFT, OVERFLOW_MASK, layout_overflow_to_u8),
        CssProperty::BoxSizing(v) => set_tier1!(v, BOX_SIZING_SHIFT, BOX_SIZING_MASK, layout_box_sizing_to_u8),
        CssProperty::FlexDirection(v) => set_tier1!(v, FLEX_DIRECTION_SHIFT, FLEX_DIR_MASK, layout_flex_direction_to_u8),
        CssProperty::FlexWrap(v) => set_tier1!(v, FLEX_WRAP_SHIFT, FLEX_WRAP_MASK, layout_flex_wrap_to_u8),
        CssProperty::JustifyContent(v) => set_tier1!(v, JUSTIFY_CONTENT_SHIFT, JUSTIFY_MASK, layout_justify_content_to_u8),
        CssProperty::AlignItems(v) => set_tier1!(v, ALIGN_ITEMS_SHIFT, ALIGN_MASK, layout_align_items_to_u8),
        CssProperty::AlignContent(v) => set_tier1!(v, ALIGN_CONTENT_SHIFT, ALIGN_MASK, layout_align_content_to_u8),
        CssProperty::WritingMode(v) => set_tier1!(v, WRITING_MODE_SHIFT, WRITING_MODE_MASK, layout_writing_mode_to_u8),
        CssProperty::Clear(v) => set_tier1!(v, CLEAR_SHIFT, CLEAR_MASK, layout_clear_to_u8),
        CssProperty::FontWeight(v) => set_tier1!(v, FONT_WEIGHT_SHIFT, FONT_WEIGHT_MASK, style_font_weight_to_u8),
        CssProperty::FontStyle(v) => set_tier1!(v, FONT_STYLE_SHIFT, FONT_STYLE_MASK, style_font_style_to_u8),
        CssProperty::TextAlign(v) => set_tier1!(v, TEXT_ALIGN_SHIFT, TEXT_ALIGN_MASK, style_text_align_to_u8),
        CssProperty::Visibility(v) => set_tier1!(v, VISIBILITY_SHIFT, VISIBILITY_MASK, style_visibility_to_u8),
        CssProperty::WhiteSpace(v) => set_tier1!(v, WHITE_SPACE_SHIFT, WHITE_SPACE_MASK, style_white_space_to_u8),
        CssProperty::Direction(v) => set_tier1!(v, DIRECTION_SHIFT, DIRECTION_MASK, style_direction_to_u8),
        CssProperty::VerticalAlign(v) => set_tier1!(v, VERTICAL_ALIGN_SHIFT, VERTICAL_ALIGN_MASK, style_vertical_align_to_u8),
        CssProperty::BorderCollapse(v) => set_tier1!(v, BORDER_COLLAPSE_SHIFT, BORDER_COLLAPSE_MASK, border_collapse_to_u8),
        CssProperty::AlignSelf(v) => set_tier1!(v, ALIGN_SELF_SHIFT, ALIGN_SELF_MASK, layout_align_self_to_u8),
        CssProperty::JustifySelf(v) => set_tier1!(v, JUSTIFY_SELF_SHIFT, JUSTIFY_SELF_MASK, layout_justify_self_to_u8),
        CssProperty::GridAutoFlow(v) => set_tier1!(v, GRID_AUTO_FLOW_SHIFT, GRID_AUTO_FLOW_MASK, layout_grid_auto_flow_to_u8),
        CssProperty::JustifyItems(v) => set_tier1!(v, JUSTIFY_ITEMS_SHIFT, JUSTIFY_ITEMS_MASK, layout_justify_items_to_u8),

        // Tier 2 dimensions
        CssProperty::Width(v) => { dims.width = encode_layout_width(v); }
        CssProperty::Height(v) => { dims.height = encode_layout_height(v); }
        CssProperty::MinWidth(v) => { dims.min_width = encode_pixel_prop(v); }
        CssProperty::MaxWidth(v) => { dims.max_width = encode_pixel_prop(v); }
        CssProperty::MinHeight(v) => { dims.min_height = encode_pixel_prop(v); }
        CssProperty::MaxHeight(v) => { dims.max_height = encode_pixel_prop(v); }
        CssProperty::FlexBasis(v) => { dims.flex_basis = encode_flex_basis(v); }
        CssProperty::FontSize(v) => { dims.font_size = encode_pixel_prop(v); }
        CssProperty::PaddingTop(v) => { dims.padding_top = encode_css_pixel_as_i16(v); }
        CssProperty::PaddingRight(v) => { dims.padding_right = encode_css_pixel_as_i16(v); }
        CssProperty::PaddingBottom(v) => { dims.padding_bottom = encode_css_pixel_as_i16(v); }
        CssProperty::PaddingLeft(v) => { dims.padding_left = encode_css_pixel_as_i16(v); }
        CssProperty::MarginTop(v) => { dims.margin_top = encode_margin_i16(v); }
        CssProperty::MarginRight(v) => { dims.margin_right = encode_margin_i16(v); }
        CssProperty::MarginBottom(v) => { dims.margin_bottom = encode_margin_i16(v); }
        CssProperty::MarginLeft(v) => { dims.margin_left = encode_margin_i16(v); }
        CssProperty::BorderTopWidth(v) => { dims.border_top_width = encode_css_pixel_as_i16(v); }
        CssProperty::BorderRightWidth(v) => { dims.border_right_width = encode_css_pixel_as_i16(v); }
        CssProperty::BorderBottomWidth(v) => { dims.border_bottom_width = encode_css_pixel_as_i16(v); }
        CssProperty::BorderLeftWidth(v) => { dims.border_left_width = encode_css_pixel_as_i16(v); }
        CssProperty::Top(v) => { dims.top = encode_css_pixel_as_i16(v); }
        CssProperty::Right(v) => { dims.right = encode_css_pixel_as_i16(v); }
        CssProperty::Bottom(v) => { dims.bottom = encode_css_pixel_as_i16(v); }
        CssProperty::Left(v) => { dims.left = encode_css_pixel_as_i16(v); }
        CssProperty::FlexGrow(v) => {
            if let Some(exact) = v.get_property() {
                dims.flex_grow = encode_flex_u16(exact.inner.get());
            }
        }
        CssProperty::FlexShrink(v) => {
            if let Some(exact) = v.get_property() {
                dims.flex_shrink = encode_flex_u16(exact.inner.get());
            }
        }

        CssProperty::RowGap(v) => {
            if let Some(g) = v.get_property() {
                if g.inner.metric == SizeMetric::Px {
                    dims.row_gap = encode_resolved_px_i16(g.inner.number.get());
                }
            }
        }
        CssProperty::ColumnGap(v) => {
            if let Some(g) = v.get_property() {
                if g.inner.metric == SizeMetric::Px {
                    dims.column_gap = encode_resolved_px_i16(g.inner.number.get());
                }
            }
        }
        CssProperty::Gap(v) => {
            if let Some(g) = v.get_property() {
                if g.inner.metric == SizeMetric::Px {
                    let enc = encode_resolved_px_i16(g.inner.number.get());
                    dims.row_gap = enc;
                    dims.column_gap = enc;
                }
            }
        }

        // Tier 2 cold
        CssProperty::ZIndex(v) => {
            if let Some(exact) = v.get_property() {
                match exact {
                    LayoutZIndex::Auto => cold.z_index = I16_AUTO,
                    LayoutZIndex::Integer(z) => {
                        cold.z_index = if *z >= I16_SENTINEL_THRESHOLD as i32 { I16_SENTINEL } else { *z as i16 };
                    }
                }
            }
        }
        CssProperty::BorderTopStyle(v) => {
            if let Some(exact) = v.get_property() {
                let bs = border_style_to_u8(exact.inner) as u16;
                cold.border_styles_packed = (cold.border_styles_packed & !0x000F) | bs;
            }
        }
        CssProperty::BorderRightStyle(v) => {
            if let Some(exact) = v.get_property() {
                let bs = border_style_to_u8(exact.inner) as u16;
                cold.border_styles_packed = (cold.border_styles_packed & !0x00F0) | (bs << 4);
            }
        }
        CssProperty::BorderBottomStyle(v) => {
            if let Some(exact) = v.get_property() {
                let bs = border_style_to_u8(exact.inner) as u16;
                cold.border_styles_packed = (cold.border_styles_packed & !0x0F00) | (bs << 8);
            }
        }
        CssProperty::BorderLeftStyle(v) => {
            if let Some(exact) = v.get_property() {
                let bs = border_style_to_u8(exact.inner) as u16;
                cold.border_styles_packed = (cold.border_styles_packed & !0xF000) | (bs << 12);
            }
        }
        CssProperty::BorderTopColor(v) => {
            if let Some(c) = v.get_property() { cold.border_top_color = encode_color_u32(&c.inner); }
        }
        CssProperty::BorderRightColor(v) => {
            if let Some(c) = v.get_property() { cold.border_right_color = encode_color_u32(&c.inner); }
        }
        CssProperty::BorderBottomColor(v) => {
            if let Some(c) = v.get_property() { cold.border_bottom_color = encode_color_u32(&c.inner); }
        }
        CssProperty::BorderLeftColor(v) => {
            if let Some(c) = v.get_property() { cold.border_left_color = encode_color_u32(&c.inner); }
        }
        CssProperty::BorderSpacing(v) => {
            if let Some(spacing) = v.get_property() {
                if spacing.horizontal.metric == SizeMetric::Px {
                    cold.border_spacing_h = encode_resolved_px_i16(spacing.horizontal.number.get());
                }
                if spacing.vertical.metric == SizeMetric::Px {
                    cold.border_spacing_v = encode_resolved_px_i16(spacing.vertical.number.get());
                }
            }
        }
        CssProperty::TabSize(v) => { cold.tab_size = encode_css_pixel_as_i16(v); }

        // Tier 2b text
        CssProperty::TextColor(v) => {
            if let Some(color) = v.get_property() {
                let c = &color.inner;
                text.text_color = ((c.r as u32) << 24) | ((c.g as u32) << 16) | ((c.b as u32) << 8) | (c.a as u32);
            }
        }
        CssProperty::FontFamily(v) => {
            if let Some(families) = v.get_property() {
                let mut hasher = DefaultHasher::new();
                families.hash(&mut hasher);
                let h = hasher.finish();
                let h = if h == 0 { 1 } else { h };
                text.font_family_hash = h;
                font_hash_map.insert(h, families.clone());
            }
        }
        CssProperty::LineHeight(v) => {
            if let Some(lh) = v.get_property() {
                let pct_x10 = (lh.inner.normalized() * 1000.0).round() as i32;
                if pct_x10 >= -32768 && pct_x10 < I16_SENTINEL_THRESHOLD as i32 {
                    text.line_height = pct_x10 as i16;
                } else {
                    text.line_height = I16_SENTINEL;
                }
            }
        }
        CssProperty::LetterSpacing(v) => { text.letter_spacing = encode_css_pixel_as_i16(v); }
        CssProperty::WordSpacing(v) => { text.word_spacing = encode_css_pixel_as_i16(v); }
        CssProperty::TextIndent(v) => { text.text_indent = encode_css_pixel_as_i16(v); }

        // Non-compact properties (background, transform, box-shadow, etc.)
        // — handled by get_property_slow fallback at paint time
        _ => {}
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
            LayoutWidth::FitContent(_) => U32_SENTINEL,
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
            LayoutHeight::FitContent(_) => U32_SENTINEL,
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
