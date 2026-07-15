//! Builder function to convert CssPropertyCache → CompactLayoutCache.
//!
//! Called once after restyle + apply_ua_css + compute_inherited_values.
//! Uses typed getters on CssPropertyCache (which cascade through all sources)
//! to resolve each property for the "normal" state (all pseudo-states = false).

use crate::dom::{NodeData, NodeId};
use crate::prop_cache::CssPropertyCache;

use crate::styled_dom::StyledNodeState;
// wildcard import: this module is the consumer of the whole compact_cache codec
// (encode/decode helpers + sentinel consts); enumerating them is unmaintainable.
#[allow(clippy::wildcard_imports)]
use azul_css::compact_cache::*;
use azul_css::css::CssPropertyValue;
use azul_css::props::property::CssProperty;
use azul_css::props::basic::length::SizeMetric;
use azul_css::props::layout::dimensions::{LayoutHeight, LayoutWidth};
use azul_css::props::layout::flex::LayoutFlexBasis;
use azul_css::props::layout::position::LayoutZIndex;
use core::hash::{Hash, Hasher};
use alloc::vec::Vec;
use crate::hash::DefaultHasher;

impl CssPropertyCache {
    /// Build a `CompactLayoutCache` from the current property cache state.
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
    // fixed-point encoders: z-index and line-height (%×10) are range-checked
    // against the i16 sentinel threshold before the deliberate narrowing cast.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::similar_names)] // domain-standard coordinate/control-point names
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
    pub fn build_compact_cache(
        &self,
        node_data: &[NodeData],
        prev_font_hashes: &[u64],
    ) -> CompactLayoutCache {
        let node_count = self.node_count;
        let default_state = StyledNodeState::default();
        let mut result = CompactLayoutCache::with_capacity(node_count);

        for (i, nd) in node_data.iter().enumerate().take(node_count) {
            let node_id = NodeId::new(i);

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
                            // Two-sided, like the line-height encoder: a large NEGATIVE z
                            // used to fall through to `*z as i16` and WRAP positive
                            // (-40000 -> +25536). Escape both out-of-range ends to the
                            // sentinel (tier 3) so the real value is preserved.
                            result.tier2_cold[i].z_index =
                                if *z >= -32768 && *z < i32::from(I16_SENTINEL_THRESHOLD) {
                                    *z as i16
                                } else {
                                    I16_SENTINEL
                                };
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
                        (u32::from(c.r) << 24) | (u32::from(c.g) << 16) | (u32::from(c.b) << 8) | u32::from(c.a);
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
                    if pct_x10 >= -32768 && pct_x10 < i32::from(I16_SENTINEL_THRESHOLD) {
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
    ///   2. Apply this node's CSS properties on top (from `css_props` + inline + UA)
    ///   3. Write directly to compact arrays
    ///
    /// This eliminates 50K Vec clones from `compute_inherited_values` and
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
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
    pub fn build_compact_cache_with_inheritance_debug(
        &self,
        node_data: &[NodeData],
        node_hierarchy: &[crate::styled_dom::NodeHierarchyItem],
        prev_font_hashes: &[u64],
        debug_messages: &mut Option<Vec<azul_css::LayoutDebugMessage>>,
    ) -> CompactLayoutCache {
        // Inheritable tier1 CSS fields (font-weight/style, text-align, visibility,
        // white-space, direction, border-collapse). Copied from parent in Step 1.
        const INHERITABLE_TIER1_MASK: u64 =
            (FONT_WEIGHT_MASK << FONT_WEIGHT_SHIFT)
            | (FONT_STYLE_MASK << FONT_STYLE_SHIFT)
            | (TEXT_ALIGN_MASK << TEXT_ALIGN_SHIFT)
            | (VISIBILITY_MASK << VISIBILITY_SHIFT)
            | (WHITE_SPACE_MASK << WHITE_SPACE_SHIFT)
            | (DIRECTION_MASK << DIRECTION_SHIFT)
            | (BORDER_COLLAPSE_MASK << BORDER_COLLAPSE_SHIFT);

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
                                let encoded = u64::from($encoder(*exact));
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
            let parent_id = node_hierarchy[i].parent_id();
            if let Some(pid) = parent_id {
                let pi = pid.index();

                // AUDIT: inheritance assumes a PRE-ORDER arena, i.e. a node's
                // parent is always stored at a lower index (`pi < i`) and has
                // therefore already been fully cascaded. A forward reference
                // (`pi >= i`) would silently inherit that parent's still-default
                // (all-zero) values, and an out-of-bounds `pi >= node_count`
                // would panic. Guard against both: assert the pre-order
                // invariant in debug builds, and skip inheritance (treat the
                // node as a root) for any malformed reference in release builds.
                debug_assert!(
                    pi < i,
                    "compact cascade: non-pre-order arena — node {i}'s parent {pi} \
                     is not stored before it; inheritance would read default values",
                );
                if pi < i {
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
            }

            {
                let d = &result.tier2_dims[i];
                cascade_debug!("node[{}] {:?} after-inherit: pt={} pb={} pl={} pr={} mt={} mb={} ml={} mr={} w={} h={}",
                    i, nd.node_type, d.padding_top, d.padding_bottom, d.padding_left, d.padding_right,
                    d.margin_top, d.margin_bottom, d.margin_left, d.margin_right, d.width, d.height);
            }

            // Step 2: Apply UA CSS defaults for this node type directly to compact values.
            // UA defaults have lowest cascade priority — overridden by author CSS below.
            apply_ua_css_to_compact(
                &nd.node_type,
                &mut result.tier1_enums[i],
                &mut result.tier2_dims[i],
                &mut result.tier2_cold[i],
                &mut result.tier2b_text[i],
                &mut result.font_hash_to_families,
            );

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
                for prop in &self.global_css_props {
                    apply_css_property_to_compact(
                        prop,
                        &mut result.tier1_enums[i],
                        &mut result.tier2_dims[i],
                        &mut result.tier2_cold[i],
                        &mut result.tier2b_text[i],
                        &mut result.font_hash_to_families,
                    );
                    update_dom_declared_flags(prop, &mut result.dom_declared_flags);
                }
            }

            {
                let d = &result.tier2_dims[i];
                cascade_debug!("node[{}] {:?} after-global-star: pt={} pb={} pl={} pr={} mt={} mb={} ml={} mr={}",
                    i, nd.node_type, d.padding_top, d.padding_bottom, d.padding_left, d.padding_right,
                    d.margin_top, d.margin_bottom, d.margin_left, d.margin_right);
                let n_props = self.css_props.get_slice(i).len();
                let n_inline = nd.style.iter_inline_properties().count();
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
                update_dom_declared_flags(&prop.property, &mut result.dom_declared_flags);
            }

            {
                let d = &result.tier2_dims[i];
                cascade_debug!("node[{}] {:?} after-css-props: pt={} pb={} pl={} pr={} mt={} mb={} ml={} mr={}",
                    i, nd.node_type, d.padding_top, d.padding_bottom, d.padding_left, d.padding_right,
                    d.margin_top, d.margin_bottom, d.margin_left, d.margin_right);
            }

            // Scan inline CSS (node_data.style — typically 0-3 properties).
            // Inline CSS has highest specificity — applied last to override stylesheet.
            for (prop, conds) in nd.style.iter_inline_properties() {
                // Only apply Normal state (no pseudo-selectors like :hover)
                let is_normal = conds.as_slice().is_empty()
                    || conds.as_slice().iter().all(|c|
                        matches!(c, azul_css::dynamic_selector::DynamicSelector::PseudoState(
                            azul_css::dynamic_selector::PseudoStateType::Normal
                        ))
                    );
                if !is_normal { continue; }
                // Layout-critical props dispatched via single-variant `if let` (direct discriminant
                // COMPARES, no indirect jump). apply_css_property_to_compact's ~100-arm `match` lowers
                // to a jump table that remill mis-lifts (never reaches the right arm) — same class as the
                // CssProperty::clone bug. With the conversion-clone fix the prop discriminant is now
                // correct, so these compares match and apply the value; everything else falls back.
                // (CssProperty is imported at module top.)
                if let CssProperty::Width(v) = prop {
                    result.tier2_dims[i].width = encode_layout_width(v);
                } else if let CssProperty::Height(v) = prop {
                    result.tier2_dims[i].height = encode_layout_height(v);
                } else if let CssProperty::FlexGrow(v) = prop {
                    if let Some(e) = v.get_property() {
                        result.tier2_dims[i].flex_grow = encode_flex_u16(e.inner.get());
                    }
                } else if let CssProperty::Display(v) = prop {
                    if let Some(e) = v.get_property() {
                        let enc = u64::from(layout_display_to_u8(*e));
                        let m = DISPLAY_MASK;
                        let s = DISPLAY_SHIFT;
                        result.tier1_enums[i] = (result.tier1_enums[i] & !(m << s)) | ((enc & m) << s);
                    }
                } else {
                    apply_css_property_to_compact(
                        prop,
                        &mut result.tier1_enums[i],
                        &mut result.tier2_dims[i],
                        &mut result.tier2_cold[i],
                        &mut result.tier2b_text[i],
                        &mut result.font_hash_to_families,
                    );
                }
                update_dom_declared_flags(prop, &mut result.dom_declared_flags);
            }

            // Resolve font-size from em/percent/pt/etc. to px.
            // CSS 2.1: inherited font-size is the COMPUTED (px) value, not the specified value.
            // Pre-order traversal guarantees parent's font_size is already resolved.
            resolve_font_size_to_px(
                &mut result.tier2_dims,
                i,
                parent_id,
            );

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
// Helpers extracted from build_compact_cache_with_inheritance_debug
// =============================================================================

/// Apply UA CSS defaults for a node type directly to compact values.
/// UA defaults have lowest cascade priority — overridden by author CSS.
fn apply_ua_css_to_compact(
    node_type: &crate::dom::NodeType,
    tier1: &mut u64,
    dims: &mut CompactNodeProps,
    cold: &mut CompactNodePropsCold,
    text: &mut CompactTextProps,
    font_hash_map: &mut alloc::collections::BTreeMap<u64, azul_css::props::basic::font::StyleFontFamilyVec>,
) {
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
        if let Some(ua_prop) = crate::ua_css::get_ua_property(node_type, *pt) {
            apply_css_property_to_compact(ua_prop, tier1, dims, cold, text, font_hash_map);
        }
    }
}

/// Resolve a node's font-size from relative units (em, %, rem, pt) to absolute px.
/// CSS 2.1: inherited font-size is the COMPUTED (px) value, not the specified value.
/// Pre-order traversal guarantees parent's `font_size` is already resolved.
fn resolve_font_size_to_px(
    tier2_dims: &mut [CompactNodeProps],
    node_idx: usize,
    parent_id: Option<NodeId>,
) {
    let raw_fs = tier2_dims[node_idx].font_size;
    if raw_fs == U32_SENTINEL || raw_fs >= U32_SENTINEL_THRESHOLD {
        return;
    }
    let pv = match decode_pixel_value_u32(raw_fs) {
        Some(pv) if pv.metric != SizeMetric::Px => pv,
        _ => return,
    };

    // AUDIT: pre-order arena assumed — the parent's font-size is already
    // resolved to px only when `pid < node_idx`. Use checked `get` so an
    // out-of-bounds parent ref cannot panic, and require `pid < node_idx` so a
    // forward reference falls back to the 16px CSS initial value instead of
    // reading an unresolved (still em/%) parent value.
    let parent_font_size_px = parent_id
        .map_or(16.0, |pid| {
            let pi = pid.index();
            debug_assert!(
                pi < node_idx,
                "compact font-size resolve: non-pre-order arena — node {node_idx}'s \
                 parent {pi} font-size is not yet resolved",
            );
            if pi < node_idx {
                tier2_dims
                    .get(pi)
                    .and_then(|p| decode_pixel_value_u32(p.font_size))
                    .map_or(16.0, |ppv| ppv.number.get())
            } else {
                16.0
            }
        });

    let resolved_px = match pv.metric {
        SizeMetric::Em => pv.number.get() * parent_font_size_px,
        SizeMetric::Percent => pv.number.get() / 100.0 * parent_font_size_px,
        SizeMetric::Rem => {
            // rem = the ROOT element's font size. For the root itself that is circular,
            // so CSS resolves root rem against the 16px INITIAL value (Selectors/Values:
            // "when specified on the root element, rem refers to the initial value").
            // tier2_dims[0] IS the root's slot, but while resolving the root it still
            // holds the root's own unresolved raw rem — so `html { font-size: 2rem }`
            // computed 2*2 = 4px instead of 2*16 = 32px.
            let rem_base = if parent_id.is_none() {
                16.0
            } else {
                tier2_dims
                    .first()
                    .and_then(|r| decode_pixel_value_u32(r.font_size))
                    .map_or(16.0, |rpv| rpv.number.get())
            };
            rem_base * pv.number.get()
        }
        SizeMetric::Pt => pv.number.get() * 96.0 / 72.0,
        _ => pv.number.get(),
    };
    tier2_dims[node_idx].font_size =
        encode_pixel_value_u32(&azul_css::props::basic::pixel::PixelValue::px(resolved_px));
}

// =============================================================================
// Direct CssProperty → compact field writer
// =============================================================================

/// Apply a single `CssProperty` directly to the compact representation.
/// Called once per property per node — replaces the old 56+ getter approach.
#[inline]
// The scrollbar-* and counter-* arms have identical bodies
// (`if v.get_property().is_some() { flags |= … }`) but each variant wraps a
// DIFFERENT value type (StyleBackgroundContentValue, LayoutScrollbarWidthValue,
// StyleScrollbarColorValue, CounterResetValue, CounterIncrementValue, …), so an
// or-pattern binding `v` cannot be expressed across them.
#[allow(clippy::match_same_arms)]
// fixed-point encoders: z-index / line-height are range-checked before the
// narrowing cast, and opacity is clamped to [0,1] then scaled to [0,254] (u8).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
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
                let encoded = u64::from($encoder(*exact));
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

        // Grid placement (compact encoding for common Auto/Line cases)
        CssProperty::GridColumn(v) => {
            if let Some(gp) = v.get_property() {
                cold.grid_col_start = encode_grid_line(&gp.grid_start);
                cold.grid_col_end = encode_grid_line(&gp.grid_end);
            }
        }
        CssProperty::GridRow(v) => {
            if let Some(gp) = v.get_property() {
                cold.grid_row_start = encode_grid_line(&gp.grid_start);
                cold.grid_row_end = encode_grid_line(&gp.grid_end);
            }
        }

        // Tier 2 cold
        CssProperty::ZIndex(v) => {
            if let Some(exact) = v.get_property() {
                match exact {
                    LayoutZIndex::Auto => cold.z_index = I16_AUTO,
                    LayoutZIndex::Integer(z) => {
                        // Two-sided (see the tier2_cold path above): a large negative z
                        // used to wrap positive via `*z as i16`. Escape both ends.
                        cold.z_index = if *z >= -32768 && *z < i32::from(I16_SENTINEL_THRESHOLD) {
                            *z as i16
                        } else {
                            I16_SENTINEL
                        };
                    }
                }
            }
        }
        CssProperty::BorderTopStyle(v) => {
            if let Some(exact) = v.get_property() {
                let bs = u16::from(border_style_to_u8(exact.inner));
                cold.border_styles_packed = (cold.border_styles_packed & !0x000F) | bs;
            }
        }
        CssProperty::BorderRightStyle(v) => {
            if let Some(exact) = v.get_property() {
                let bs = u16::from(border_style_to_u8(exact.inner));
                cold.border_styles_packed = (cold.border_styles_packed & !0x00F0) | (bs << 4);
            }
        }
        CssProperty::BorderBottomStyle(v) => {
            if let Some(exact) = v.get_property() {
                let bs = u16::from(border_style_to_u8(exact.inner));
                cold.border_styles_packed = (cold.border_styles_packed & !0x0F00) | (bs << 8);
            }
        }
        CssProperty::BorderLeftStyle(v) => {
            if let Some(exact) = v.get_property() {
                let bs = u16::from(border_style_to_u8(exact.inner));
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
                text.text_color = (u32::from(c.r) << 24) | (u32::from(c.g) << 16) | (u32::from(c.b) << 8) | u32::from(c.a);
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
                if pct_x10 >= -32768 && pct_x10 < i32::from(I16_SENTINEL_THRESHOLD) {
                    text.line_height = pct_x10 as i16;
                } else {
                    text.line_height = I16_SENTINEL;
                }
            }
        }
        CssProperty::LetterSpacing(v) => { text.letter_spacing = encode_css_pixel_as_i16(v); }
        CssProperty::WordSpacing(v) => { text.word_spacing = encode_css_pixel_as_i16(v); }
        CssProperty::TextIndent(v) => { text.text_indent = encode_css_pixel_as_i16(v); }

        // Border radii (cold): encode px × 10 into i16; sentinel stays = unset/0
        CssProperty::BorderTopLeftRadius(v) => {
            if let Some(exact) = v.get_property() {
                if exact.inner.metric == SizeMetric::Px {
                    cold.border_top_left_radius = encode_resolved_px_i16(exact.inner.number.get());
                }
            }
        }
        CssProperty::BorderTopRightRadius(v) => {
            if let Some(exact) = v.get_property() {
                if exact.inner.metric == SizeMetric::Px {
                    cold.border_top_right_radius = encode_resolved_px_i16(exact.inner.number.get());
                }
            }
        }
        CssProperty::BorderBottomLeftRadius(v) => {
            if let Some(exact) = v.get_property() {
                if exact.inner.metric == SizeMetric::Px {
                    cold.border_bottom_left_radius = encode_resolved_px_i16(exact.inner.number.get());
                }
            }
        }
        CssProperty::BorderBottomRightRadius(v) => {
            if let Some(exact) = v.get_property() {
                if exact.inner.metric == SizeMetric::Px {
                    cold.border_bottom_right_radius = encode_resolved_px_i16(exact.inner.number.get());
                }
            }
        }

        // Opacity: encode as 0-254, 255 = sentinel (unset/default = 1.0)
        CssProperty::Opacity(v) => {
            if let Some(exact) = v.get_property() {
                let o = exact.inner.normalized().clamp(0.0, 1.0);
                let byte = (o * 254.0).round() as u8;
                // byte is in [0, 254], never collides with OPACITY_SENTINEL=255
                cold.opacity = byte;
            }
        }

        // has-flags: set bit whenever property is set (regardless of value).
        // Getter uses this as a fast "is the default" bail-out.
        CssProperty::Transform(v) => {
            if v.get_property().is_some() { cold.hot_flags |= HOT_FLAG_HAS_TRANSFORM; }
        }
        CssProperty::TransformOrigin(v) => {
            if v.get_property().is_some() { cold.hot_flags |= HOT_FLAG_HAS_TRANSFORM_ORIGIN; }
        }
        // All four shadow sides wrap the same StyleBoxShadowValue and set the
        // single has-box-shadow bit.
        CssProperty::BoxShadowTop(v)
        | CssProperty::BoxShadowBottom(v)
        | CssProperty::BoxShadowLeft(v)
        | CssProperty::BoxShadowRight(v) => {
            if v.get_property().is_some() { cold.hot_flags |= HOT_FLAG_HAS_BOX_SHADOW; }
        }
        CssProperty::TextDecoration(v) => {
            if v.get_property().is_some() { cold.hot_flags |= HOT_FLAG_HAS_TEXT_DECORATION; }
        }
        CssProperty::ScrollbarGutter(v) => {
            if let Some(exact) = v.get_property() {
                use azul_css::props::layout::overflow::StyleScrollbarGutter;
                let bits: u8 = match exact {
                    StyleScrollbarGutter::Auto => SCROLLBAR_GUTTER_AUTO,
                    StyleScrollbarGutter::Stable => SCROLLBAR_GUTTER_STABLE,
                    StyleScrollbarGutter::StableBothEdges => SCROLLBAR_GUTTER_BOTH_EDGES,
                };
                cold.hot_flags = (cold.hot_flags & !HOT_FLAG_SCROLLBAR_GUTTER_MASK)
                    | ((bits << HOT_FLAG_SCROLLBAR_GUTTER_SHIFT) & HOT_FLAG_SCROLLBAR_GUTTER_MASK);
            }
        }
        CssProperty::BackgroundContent(v) => {
            if v.get_property().is_some() { cold.hot_flags |= HOT_FLAG_HAS_BACKGROUND; }
        }
        CssProperty::ClipPath(v) => {
            if v.get_property().is_some() { cold.hot_flags |= HOT_FLAG_HAS_CLIP_PATH; }
        }

        // Any scrollbar customisation sets the single `has_any_scrollbar_css`
        // bit. When unset, get_scrollbar_style can bail to UA defaults without
        // doing 8 cascade walks.
        CssProperty::ScrollbarTrack(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_SCROLLBAR_CSS; }
        }
        CssProperty::ScrollbarThumb(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_SCROLLBAR_CSS; }
        }
        CssProperty::ScrollbarButton(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_SCROLLBAR_CSS; }
        }
        CssProperty::ScrollbarCorner(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_SCROLLBAR_CSS; }
        }
        CssProperty::ScrollbarWidth(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_SCROLLBAR_CSS; }
        }
        CssProperty::ScrollbarColor(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_SCROLLBAR_CSS; }
        }
        CssProperty::ScrollbarVisibility(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_SCROLLBAR_CSS; }
        }
        CssProperty::ScrollbarFadeDelay(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_SCROLLBAR_CSS; }
        }
        CssProperty::ScrollbarFadeDuration(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_SCROLLBAR_CSS; }
        }

        // Rare paint/layout props with dedicated fast-path bits.
        CssProperty::CounterReset(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_COUNTER; }
        }
        CssProperty::CounterIncrement(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_COUNTER; }
        }
        // Both break-before/after wrap PageBreakValue and set the has-break bit.
        CssProperty::BreakBefore(v) | CssProperty::BreakAfter(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_BREAK; }
        }
        CssProperty::TextOrientation(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_TEXT_ORIENTATION; }
        }
        CssProperty::TextShadow(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_TEXT_SHADOW; }
        }
        CssProperty::BackdropFilter(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_BACKDROP_FILTER; }
        }
        CssProperty::Filter(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_FILTER; }
        }
        CssProperty::MixBlendMode(v) => {
            if v.get_property().is_some() { cold.extra_flags |= EXTRA_FLAG_HAS_MIX_BLEND_MODE; }
        }

        // Non-compact properties (background, etc.) — handled by get_property_slow fallback
        _ => {}
    }
}

/// OR the DOM-level declared-flag for rarely-set text properties. Called once
/// per property per node so that when a flag bit is clear, callers
/// (e.g. `translate_to_text3_constraints`) can skip the cascade walk and use
/// the default value — the slow walk would never find a declaration anyway.
const fn update_dom_declared_flags(prop: &CssProperty, flags: &mut u32) {
    // Only mark if the property value is actually "set" (not Auto/Initial/etc.).
    // Using `get_property().is_some()` mirrors the pattern used elsewhere in
    // this builder for has-X bits.
    match prop {
        CssProperty::ShapeInside(v) => if v.get_property().is_some() { *flags |= DOM_HAS_SHAPE_INSIDE; }
        CssProperty::ShapeOutside(v) => if v.get_property().is_some() { *flags |= DOM_HAS_SHAPE_OUTSIDE; }
        CssProperty::TextJustify(v) => if v.get_property().is_some() { *flags |= DOM_HAS_TEXT_JUSTIFY; }
        CssProperty::TextIndent(v) => if v.get_property().is_some() { *flags |= DOM_HAS_TEXT_INDENT; }
        CssProperty::ColumnCount(v) => if v.get_property().is_some() { *flags |= DOM_HAS_COLUMN_COUNT; }
        CssProperty::ColumnGap(v) => if v.get_property().is_some() { *flags |= DOM_HAS_COLUMN_GAP; }
        CssProperty::ColumnWidth(v) => if v.get_property().is_some() { *flags |= DOM_HAS_COLUMN_WIDTH; }
        CssProperty::InitialLetter(v) => if v.get_property().is_some() { *flags |= DOM_HAS_INITIAL_LETTER; }
        CssProperty::InitialLetterAlign(v) => if v.get_property().is_some() { *flags |= DOM_HAS_INITIAL_LETTER_ALIGN; }
        CssProperty::LineClamp(v) => if v.get_property().is_some() { *flags |= DOM_HAS_LINE_CLAMP; }
        CssProperty::HangingPunctuation(v) => if v.get_property().is_some() { *flags |= DOM_HAS_HANGING_PUNCTUATION; }
        CssProperty::TextCombineUpright(v) => if v.get_property().is_some() { *flags |= DOM_HAS_TEXT_COMBINE_UPRIGHT; }
        CssProperty::ExclusionMargin(v) => if v.get_property().is_some() { *flags |= DOM_HAS_EXCLUSION_MARGIN; }
        CssProperty::ShapeMargin(v) => if v.get_property().is_some() { *flags |= DOM_HAS_SHAPE_MARGIN; }
        CssProperty::HyphenationLanguage(v) => if v.get_property().is_some() { *flags |= DOM_HAS_HYPHENATION_LANGUAGE; }
        CssProperty::UnicodeBidi(v) => if v.get_property().is_some() { *flags |= DOM_HAS_UNICODE_BIDI; }
        CssProperty::TextBoxTrim(v) => if v.get_property().is_some() { *flags |= DOM_HAS_TEXT_BOX_TRIM; }
        CssProperty::Hyphens(v) => if v.get_property().is_some() { *flags |= DOM_HAS_HYPHENS; }
        CssProperty::WordBreak(v) => if v.get_property().is_some() { *flags |= DOM_HAS_WORD_BREAK; }
        CssProperty::OverflowWrap(v) => if v.get_property().is_some() { *flags |= DOM_HAS_OVERFLOW_WRAP; }
        CssProperty::LineBreak(v) => if v.get_property().is_some() { *flags |= DOM_HAS_LINE_BREAK; }
        CssProperty::TextAlignLast(v) => if v.get_property().is_some() { *flags |= DOM_HAS_TEXT_ALIGN_LAST; }
        CssProperty::LineHeight(v) => if v.get_property().is_some() { *flags |= DOM_HAS_LINE_HEIGHT; }
        _ => {}
    }
}

// =============================================================================
// Helper encoders for dimension properties
// =============================================================================

/// Encode a `GridLine` into i16: `Auto=I16_AUTO`, Line(n)=n, Span(n)=-(n).
/// Named lines fall back to `I16_SENTINEL` (not compact-encodable).
// const fn: the `n as i16` casts are guarded by explicit +/-32000 range checks.
#[allow(clippy::cast_possible_truncation)]
const fn encode_grid_line(line: &azul_css::props::layout::grid::GridLine) -> i16 {
    use azul_css::props::layout::grid::GridLine;
    match line {
        GridLine::Auto => I16_AUTO,
        GridLine::Line(n) => {
            if *n >= -32000 && *n <= 32000 { *n as i16 } else { I16_SENTINEL }
        }
        GridLine::Span(n) => {
            if *n >= 1 && *n <= 32000 { -(*n as i16) } else { I16_SENTINEL }
        }
        GridLine::Named(_) => I16_SENTINEL,
    }
}

/// Encode a `CssPropertyValue`<LayoutWidth> into u32 compact form.
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

/// Encode a `CssPropertyValue`<LayoutHeight> into u32 compact form.
fn encode_layout_height<T: LayoutWidthLike>(val: &CssPropertyValue<T>) -> u32 {
    encode_layout_width(val)
}

/// Trait for types that can be encoded as compact u32 dimension values.
/// Implemented for `LayoutWidth`, `LayoutHeight` (which are Auto|Px|MinContent|MaxContent|Calc enums).
trait LayoutWidthLike {
    fn encode_compact_u32(&self) -> u32;
}

impl LayoutWidthLike for LayoutWidth {
    fn encode_compact_u32(&self) -> u32 {
        match self {
            Self::Auto => U32_AUTO,
            Self::Px(pv) => encode_pixel_value_u32(pv),
            Self::MinContent => U32_MIN_CONTENT,
            Self::MaxContent => U32_MAX_CONTENT,
            // FitContent/Calc are not compact-encodable → overflow to tier 3.
            Self::FitContent(_) | Self::Calc(_) => U32_SENTINEL,
        }
    }
}

impl LayoutWidthLike for LayoutHeight {
    fn encode_compact_u32(&self) -> u32 {
        match self {
            Self::Auto => U32_AUTO,
            Self::Px(pv) => encode_pixel_value_u32(pv),
            Self::MinContent => U32_MIN_CONTENT,
            Self::MaxContent => U32_MAX_CONTENT,
            // FitContent/Calc are not compact-encodable → overflow to tier 3.
            Self::FitContent(_) | Self::Calc(_) => U32_SENTINEL,
        }
    }
}

/// Encode a `CssPropertyValue` wrapping a simple `PixelValue` struct (`LayoutMinWidth`, etc.)
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

/// Encode a `CssPropertyValue`<T> where T wraps a `PixelValue`, as i16 (×10 resolved px).
/// Delegates to the canonical `azul_css::compact_cache::encode_css_pixel_as_i16`.
fn encode_css_pixel_as_i16<T: HasInnerPixelValue>(val: &CssPropertyValue<T>) -> i16 {
    let mapped = match val {
        CssPropertyValue::Exact(inner) => CssPropertyValue::Exact(inner.get_inner_pixel()),
        CssPropertyValue::Auto => CssPropertyValue::Auto,
        CssPropertyValue::Initial => CssPropertyValue::Initial,
        CssPropertyValue::Inherit => CssPropertyValue::Inherit,
        CssPropertyValue::None => CssPropertyValue::None,
        _ => return I16_SENTINEL,
    };
    azul_css::compact_cache::encode_css_pixel_as_i16(&mapped)
}

/// Encode margin: same as `encode_css_pixel_as_i16` but Auto is a distinct value.
fn encode_margin_i16<T: HasInnerPixelValue>(val: &CssPropertyValue<T>) -> i16 {
    encode_css_pixel_as_i16(val)
}

/// Encode `CssPropertyValue`<LayoutFlexBasis> — `LayoutFlexBasis` is Auto | Exact(PixelValue).
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

#[cfg(test)]
mod audit_tests {
    use super::resolve_font_size_to_px;
    use crate::dom::NodeId;
    use azul_css::compact_cache::{
        decode_pixel_value_u32, encode_pixel_value_u32, CompactNodeProps,
    };
    use azul_css::props::basic::pixel::PixelValue;

    // Happy path: an `em` font-size resolves against a valid (pre-order) parent.
    #[test]
    fn resolve_font_size_em_from_parent() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::em(2.0));
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        let pv = decode_pixel_value_u32(dims[1].font_size).unwrap();
        assert!((pv.number.get() - 40.0).abs() < 0.01, "got {}", pv.number.get());
    }

    // Root `em` (no parent) uses the 16px CSS initial value.
    #[test]
    fn resolve_font_size_root_em_uses_default() {
        let mut dims = vec![CompactNodeProps::default()];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::em(2.0));
        resolve_font_size_to_px(&mut dims, 0, None);
        let pv = decode_pixel_value_u32(dims[0].font_size).unwrap();
        assert!((pv.number.get() - 32.0).abs() < 0.01, "got {}", pv.number.get());
    }

    // A `rem` value reads the root (index 0) via the `.first()` guard without
    // panicking (previously indexed `tier2_dims[0]` directly).
    #[test]
    fn resolve_font_size_rem_reads_root() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(10.0)); // root
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::rem(3.0)); // child rem
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        let pv = decode_pixel_value_u32(dims[1].font_size).unwrap();
        assert!((pv.number.get() - 30.0).abs() < 0.01, "got {}", pv.number.get());
    }
}

// =============================================================================
// Adversarial unit tests (autotest)
//
// Inline module: the encoders below (`encode_grid_line`, `encode_layout_width`,
// `encode_pixel_prop`, `encode_css_pixel_as_i16`, `encode_margin_i16`,
// `encode_flex_basis`, `apply_css_property_to_compact`, `apply_ua_css_to_compact`,
// `update_dom_declared_flags`, `resolve_font_size_to_px`) are all private, so they
// can only be exercised from inside this module.
//
// Focus: overflow / saturation / sentinel-aliasing / round-trip fidelity, i.e. the
// places where a fixed-point codec silently turns one CSS value into a different
// one instead of panicking.
// =============================================================================
#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::too_many_lines,
    clippy::cast_lossless
)]
mod autotest_generated {
    use super::*;

    use alloc::collections::BTreeMap;

    use crate::dom::NodeType;
    use crate::styled_dom::NodeHierarchyItem;
    use azul_css::props::basic::color::ColorU;
    use azul_css::props::basic::font::{StyleFontFamily, StyleFontFamilyVec};
    use azul_css::props::basic::length::{FloatValue, PercentageValue};
    use azul_css::props::basic::pixel::PixelValue;
    use azul_css::props::layout::dimensions::LayoutMinWidth;
    use azul_css::props::layout::display::LayoutDisplay;
    use azul_css::props::layout::flex::{LayoutFlexGrow, LayoutFlexShrink};
    use azul_css::props::layout::grid::{GridLine, GridPlacement, LayoutGap, NamedGridLine};
    use azul_css::props::layout::overflow::StyleScrollbarGutter;
    use azul_css::props::layout::position::LayoutPosition;
    use azul_css::props::layout::spacing::{LayoutMarginTop, LayoutPaddingTop};
    use azul_css::props::layout::table::StyleBorderCollapse;
    use azul_css::props::style::border::{BorderStyle, StyleBorderTopStyle};
    use azul_css::props::style::effects::StyleOpacity;
    use azul_css::props::style::text::{
        StyleLineHeight, StyleTextColor, StyleTextDecoration, StyleTextIndent,
    };

    // -------------------------------------------------------------------------
    // Fixtures
    // -------------------------------------------------------------------------

    /// The four compact output slots + the font reverse-map, as one value, so a
    /// test can snapshot "everything the writer could have touched".
    struct Sink {
        tier1: u64,
        dims: CompactNodeProps,
        cold: CompactNodePropsCold,
        text: CompactTextProps,
        fonts: BTreeMap<u64, StyleFontFamilyVec>,
    }

    impl Sink {
        fn new() -> Self {
            Self {
                tier1: 0,
                dims: CompactNodeProps::default(),
                cold: CompactNodePropsCold::default(),
                text: CompactTextProps::default(),
                fonts: BTreeMap::new(),
            }
        }

        fn apply(&mut self, prop: &CssProperty) {
            apply_css_property_to_compact(
                prop,
                &mut self.tier1,
                &mut self.dims,
                &mut self.cold,
                &mut self.text,
                &mut self.fonts,
            );
        }

        fn ua(&mut self, node_type: &NodeType) {
            apply_ua_css_to_compact(
                node_type,
                &mut self.tier1,
                &mut self.dims,
                &mut self.cold,
                &mut self.text,
                &mut self.fonts,
            );
        }

        fn snapshot(&self) -> (u64, CompactNodeProps, CompactNodePropsCold, CompactTextProps) {
            (self.tier1, self.dims, self.cold, self.text)
        }
    }

    fn div_nodes(n: usize) -> Vec<NodeData> {
        (0..n).map(|_| NodeData::create_node(NodeType::Div)).collect()
    }

    /// Pre-order chain: node 0 is the root, node `i` is the child of node `i-1`.
    /// `NodeHierarchyItem` uses 1-based encoding (0 = None, n = `NodeId(n-1)`).
    fn linear_hierarchy(n: usize) -> Vec<NodeHierarchyItem> {
        (0..n)
            .map(|i| NodeHierarchyItem {
                parent: i, // i == 0 -> None; i > 0 -> NodeId(i-1)
                previous_sibling: 0,
                next_sibling: 0,
                last_child: if i + 1 < n { i + 2 } else { 0 },
            })
            .collect()
    }

    fn padding(px: f32) -> CssPropertyValue<LayoutPaddingTop> {
        CssPropertyValue::Exact(LayoutPaddingTop { inner: PixelValue::px(px) })
    }

    // -------------------------------------------------------------------------
    // encode_grid_line
    // -------------------------------------------------------------------------

    #[test]
    fn grid_line_auto_and_named_map_to_their_sentinels() {
        assert_eq!(encode_grid_line(&GridLine::Auto), I16_AUTO);
        let named = GridLine::Named(NamedGridLine {
            grid_line_name: "sidebar".into(),
            span_count: 0,
        });
        assert_eq!(encode_grid_line(&named), I16_SENTINEL);
    }

    #[test]
    fn grid_line_number_boundaries_saturate_instead_of_truncating() {
        assert_eq!(encode_grid_line(&GridLine::Line(0)), 0);
        assert_eq!(encode_grid_line(&GridLine::Line(1)), 1);
        assert_eq!(encode_grid_line(&GridLine::Line(-1)), -1);
        assert_eq!(encode_grid_line(&GridLine::Line(32_000)), 32_000);
        assert_eq!(encode_grid_line(&GridLine::Line(-32_000)), -32_000);
        // One past the guarded range: must become the sentinel, never a wrapped i16.
        assert_eq!(encode_grid_line(&GridLine::Line(32_001)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Line(-32_001)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Line(i32::MAX)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Line(i32::MIN)), I16_SENTINEL);
    }

    #[test]
    fn grid_line_span_boundaries_and_nonsense_spans() {
        assert_eq!(encode_grid_line(&GridLine::Span(1)), -1);
        assert_eq!(encode_grid_line(&GridLine::Span(32_000)), -32_000);
        // `span 0` / negative spans are not representable -> sentinel, NOT 0 (which
        // would silently mean "grid line 0").
        assert_eq!(encode_grid_line(&GridLine::Span(0)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Span(-1)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Span(32_001)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Span(i32::MAX)), I16_SENTINEL);
        assert_eq!(encode_grid_line(&GridLine::Span(i32::MIN)), I16_SENTINEL);
    }

    #[test]
    fn grid_line_in_range_values_never_alias_the_sentinel_band() {
        // A real line number that lands on >= I16_SENTINEL_THRESHOLD would decode
        // as "auto" / "overflow" and move the item to a different grid cell.
        for n in [-32_000i32, -1_000, -1, 0, 1, 1_000, 32_000] {
            let e = encode_grid_line(&GridLine::Line(n));
            assert!(
                e < I16_SENTINEL_THRESHOLD,
                "Line({n}) encoded into the sentinel band as {e}"
            );
        }
        for n in [1i32, 2, 1_000, 32_000] {
            let e = encode_grid_line(&GridLine::Span(n));
            assert!(e < 0, "Span({n}) must encode as a negative value, got {e}");
            assert!(
                e < I16_SENTINEL_THRESHOLD,
                "Span({n}) encoded into the sentinel band as {e}"
            );
        }
    }

    // -------------------------------------------------------------------------
    // encode_layout_width / encode_layout_height
    // -------------------------------------------------------------------------

    #[test]
    fn layout_width_keywords_map_to_distinct_sentinels() {
        let auto: CssPropertyValue<LayoutWidth> = CssPropertyValue::Auto;
        let none: CssPropertyValue<LayoutWidth> = CssPropertyValue::None;
        let initial: CssPropertyValue<LayoutWidth> = CssPropertyValue::Initial;
        let inherit: CssPropertyValue<LayoutWidth> = CssPropertyValue::Inherit;
        assert_eq!(encode_layout_width(&auto), U32_AUTO);
        assert_eq!(encode_layout_width(&none), U32_NONE);
        assert_eq!(encode_layout_width(&initial), U32_INITIAL);
        assert_eq!(encode_layout_width(&inherit), U32_INHERIT);
    }

    #[test]
    fn layout_width_revert_and_unset_fall_back_to_the_overflow_sentinel() {
        // `revert` / `unset` have no compact slot. They must land on U32_SENTINEL
        // (= "ask the slow path"), never on a *semantic* sentinel like AUTO.
        let revert: CssPropertyValue<LayoutWidth> = CssPropertyValue::Revert;
        let unset: CssPropertyValue<LayoutWidth> = CssPropertyValue::Unset;
        assert_eq!(encode_layout_width(&revert), U32_SENTINEL);
        assert_eq!(encode_layout_width(&unset), U32_SENTINEL);
        assert_eq!(encode_layout_height(&revert), U32_SENTINEL);
        assert_eq!(encode_layout_height(&unset), U32_SENTINEL);
    }

    #[test]
    fn layout_width_exact_keyword_variants() {
        assert_eq!(
            encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::Auto)),
            U32_AUTO
        );
        assert_eq!(
            encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::MinContent)),
            U32_MIN_CONTENT
        );
        assert_eq!(
            encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::MaxContent)),
            U32_MAX_CONTENT
        );
        // fit-content() is not compact-encodable -> tier 3
        assert_eq!(
            encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::FitContent(
                PixelValue::px(10.0)
            ))),
            U32_SENTINEL
        );
    }

    #[test]
    fn layout_width_px_round_trips() {
        for px in [0.0f32, 0.5, 1.0, 100.0, 1234.567, -50.0] {
            let enc =
                encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::Px(PixelValue::px(px))));
            let dec = decode_pixel_value_u32(enc)
                .expect("an in-range px value must not encode to a sentinel");
            assert_eq!(dec.metric, SizeMetric::Px);
            assert!(
                (dec.number.get() - px).abs() < 0.002,
                "round-trip of {px}px produced {}px",
                dec.number.get()
            );
        }
    }

    #[test]
    fn layout_width_extreme_values_saturate_to_the_overflow_sentinel() {
        // Past the 28-bit fixed-point range the encoder must bail to tier 3 rather
        // than wrapping the low bits into a small (and plausible-looking) width.
        for px in [
            1.0e9f32,
            -1.0e9,
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let enc =
                encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::Px(PixelValue::px(px))));
            assert_eq!(
                enc, U32_SENTINEL,
                "width {px}px should overflow to U32_SENTINEL, got {enc:#x}"
            );
        }
    }

    #[test]
    fn layout_width_nan_degrades_to_zero_without_panicking() {
        // `NaN as isize` saturates to 0, so a NaN width becomes 0px — deterministic
        // and finite, which is what the layout solver needs.
        let enc = encode_layout_width(&CssPropertyValue::Exact(LayoutWidth::Px(PixelValue::px(
            f32::NAN,
        ))));
        let dec = decode_pixel_value_u32(enc).expect("NaN must degrade to a value, not a sentinel");
        assert!(dec.number.get().is_finite());
        assert_eq!(dec.number.get(), 0.0);
    }

    #[test]
    fn layout_height_never_diverges_from_layout_width() {
        let vals = [
            CssPropertyValue::Exact(LayoutWidth::Auto),
            CssPropertyValue::Exact(LayoutWidth::MinContent),
            CssPropertyValue::Exact(LayoutWidth::MaxContent),
            CssPropertyValue::Exact(LayoutWidth::Px(PixelValue::px(42.0))),
            CssPropertyValue::Exact(LayoutWidth::Px(PixelValue::px(1.0e9))),
            CssPropertyValue::Unset,
        ];
        for v in &vals {
            assert_eq!(encode_layout_width(v), encode_layout_height(v));
        }
    }

    // -------------------------------------------------------------------------
    // encode_pixel_prop
    // -------------------------------------------------------------------------

    #[test]
    fn pixel_prop_keywords_map_to_distinct_sentinels() {
        let auto: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::Auto;
        let none: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::None;
        let initial: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::Initial;
        let inherit: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::Inherit;
        let revert: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::Revert;
        let unset: CssPropertyValue<LayoutMinWidth> = CssPropertyValue::Unset;
        assert_eq!(encode_pixel_prop(&auto), U32_AUTO);
        assert_eq!(encode_pixel_prop(&none), U32_NONE);
        assert_eq!(encode_pixel_prop(&initial), U32_INITIAL);
        assert_eq!(encode_pixel_prop(&inherit), U32_INHERIT);
        assert_eq!(encode_pixel_prop(&revert), U32_SENTINEL);
        assert_eq!(encode_pixel_prop(&unset), U32_SENTINEL);
    }

    #[test]
    fn pixel_prop_round_trips_value_and_metric() {
        for pv in [
            PixelValue::px(50.0),
            PixelValue::em(1.5),
            PixelValue::percent(80.0),
            PixelValue::pt(12.0),
            PixelValue::rem(2.0),
        ] {
            let enc = encode_pixel_prop(&CssPropertyValue::Exact(LayoutMinWidth { inner: pv }));
            let dec = decode_pixel_value_u32(enc).expect("must round-trip");
            assert_eq!(dec.metric, pv.metric, "metric lost in the round-trip");
            assert!(
                (dec.number.get() - pv.number.get()).abs() < 0.002,
                "value lost in the round-trip: {} -> {}",
                pv.number.get(),
                dec.number.get()
            );
        }
    }

    #[test]
    fn pixel_prop_overflow_saturates() {
        let enc = encode_pixel_prop(&CssPropertyValue::Exact(LayoutMinWidth {
            inner: PixelValue::px(1.0e9),
        }));
        assert_eq!(enc, U32_SENTINEL);
    }

    #[test]
    fn pixel_prop_exact_value_never_aliases_a_semantic_sentinel() {
        // INVARIANT: an `Exact` length may overflow to U32_SENTINEL (= "slow path"),
        // but must never collide with a sentinel that means something *else*
        // (auto / none / inherit / initial / min-content / max-content) — that turns
        // a length into a different keyword with no way to tell.
        //
        // `encode_pixel_value_u32` packs `value << 4 | metric`. For the raw
        // fixed-point value -1 (i.e. -0.001) the value bits are 0xFFFF_FFF0, so any
        // metric whose code is >= 9 (vh = 9, vmin = 10, vmax = 11) ORs straight into
        // the sentinel band:
        //     -0.001vh   -> 0xFFFF_FFF9 == U32_MAX_CONTENT
        //     -0.001vmin -> 0xFFFF_FFFA == U32_MIN_CONTENT
        //     -0.001vmax -> 0xFFFF_FFFB == U32_INITIAL
        for metric in [SizeMetric::Vh, SizeMetric::Vmin, SizeMetric::Vmax] {
            let pv = PixelValue::from_metric(metric, -0.001);
            let enc = encode_pixel_prop(&CssPropertyValue::Exact(LayoutMinWidth { inner: pv }));
            assert!(
                enc == U32_SENTINEL || enc < U32_SENTINEL_THRESHOLD,
                "an Exact viewport length encoded to {enc:#x}, which aliases a semantic sentinel",
            );
        }
    }

    // -------------------------------------------------------------------------
    // encode_css_pixel_as_i16 / encode_margin_i16
    // -------------------------------------------------------------------------

    #[test]
    fn css_pixel_i16_scales_by_ten() {
        assert_eq!(encode_css_pixel_as_i16(&padding(0.0)), 0);
        assert_eq!(encode_css_pixel_as_i16(&padding(10.5)), 105);
        assert_eq!(encode_css_pixel_as_i16(&padding(-10.5)), -105);
    }

    #[test]
    fn css_pixel_i16_boundaries() {
        // 3276.3px is the largest representable value (one below the sentinel band)
        assert_eq!(encode_css_pixel_as_i16(&padding(3276.3)), 32_763);
        // one tick further must saturate, NOT alias I16_INITIAL (32764)
        assert_eq!(encode_css_pixel_as_i16(&padding(3276.4)), I16_SENTINEL);
        // and the negative end
        assert_eq!(encode_css_pixel_as_i16(&padding(-3276.8)), -32_768);
        assert_eq!(encode_css_pixel_as_i16(&padding(-3276.9)), I16_SENTINEL);
    }

    #[test]
    fn css_pixel_i16_non_px_units_need_the_slow_path() {
        let em = CssPropertyValue::Exact(LayoutPaddingTop { inner: PixelValue::em(2.0) });
        let pct = CssPropertyValue::Exact(LayoutPaddingTop {
            inner: PixelValue::percent(50.0),
        });
        assert_eq!(encode_css_pixel_as_i16(&em), I16_SENTINEL);
        assert_eq!(encode_css_pixel_as_i16(&pct), I16_SENTINEL);
    }

    #[test]
    fn css_pixel_i16_keywords_are_distinguishable() {
        let auto: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::Auto;
        let initial: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::Initial;
        let inherit: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::Inherit;
        let none: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::None;
        let revert: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::Revert;
        let unset: CssPropertyValue<LayoutPaddingTop> = CssPropertyValue::Unset;
        assert_eq!(encode_css_pixel_as_i16(&auto), I16_AUTO);
        assert_eq!(encode_css_pixel_as_i16(&initial), I16_INITIAL);
        assert_eq!(encode_css_pixel_as_i16(&inherit), I16_INHERIT);
        // none / revert / unset have no dedicated slot -> generic sentinel
        assert_eq!(encode_css_pixel_as_i16(&none), I16_SENTINEL);
        assert_eq!(encode_css_pixel_as_i16(&revert), I16_SENTINEL);
        assert_eq!(encode_css_pixel_as_i16(&unset), I16_SENTINEL);
    }

    #[test]
    fn css_pixel_i16_nan_and_infinity_are_safe() {
        assert_eq!(encode_css_pixel_as_i16(&padding(f32::NAN)), 0);
        assert_eq!(encode_css_pixel_as_i16(&padding(f32::INFINITY)), I16_SENTINEL);
        assert_eq!(
            encode_css_pixel_as_i16(&padding(f32::NEG_INFINITY)),
            I16_SENTINEL
        );
        assert_eq!(encode_css_pixel_as_i16(&padding(f32::MAX)), I16_SENTINEL);
        assert_eq!(encode_css_pixel_as_i16(&padding(f32::MIN)), I16_SENTINEL);
    }

    #[test]
    fn css_pixel_i16_exact_value_never_aliases_a_keyword_sentinel() {
        // The i16 encoder range-checks *both* ends before narrowing, so — unlike the
        // u32 path — an Exact px value can never be mistaken for auto/inherit/initial.
        for px in [
            -3276.8f32, -100.0, -0.1, 0.0, 0.1, 100.0, 3276.3, 1.0e9, -1.0e9,
        ] {
            let e = encode_css_pixel_as_i16(&padding(px));
            assert!(
                e != I16_AUTO && e != I16_INHERIT && e != I16_INITIAL,
                "{px}px aliased a keyword sentinel ({e})"
            );
        }
    }

    #[test]
    fn margin_i16_keeps_auto_and_otherwise_matches_the_pixel_encoder() {
        let auto: CssPropertyValue<LayoutMarginTop> = CssPropertyValue::Auto;
        assert_eq!(encode_margin_i16(&auto), I16_AUTO);
        for px in [-50.0f32, 0.0, 12.5, 3276.3, 5.0e9, f32::NAN] {
            let m = CssPropertyValue::Exact(LayoutMarginTop { inner: PixelValue::px(px) });
            assert_eq!(encode_margin_i16(&m), encode_css_pixel_as_i16(&padding(px)));
        }
    }

    // -------------------------------------------------------------------------
    // encode_flex_basis
    // -------------------------------------------------------------------------

    #[test]
    fn flex_basis_all_variants() {
        assert_eq!(
            encode_flex_basis(&CssPropertyValue::Exact(LayoutFlexBasis::Auto)),
            U32_AUTO
        );
        let enc = encode_flex_basis(&CssPropertyValue::Exact(LayoutFlexBasis::Exact(
            PixelValue::px(120.0),
        )));
        let dec = decode_pixel_value_u32(enc).expect("px flex-basis must round-trip");
        assert!((dec.number.get() - 120.0).abs() < 0.002);

        assert_eq!(encode_flex_basis(&CssPropertyValue::Auto), U32_AUTO);
        assert_eq!(encode_flex_basis(&CssPropertyValue::None), U32_NONE);
        assert_eq!(encode_flex_basis(&CssPropertyValue::Initial), U32_INITIAL);
        assert_eq!(encode_flex_basis(&CssPropertyValue::Inherit), U32_INHERIT);
        assert_eq!(encode_flex_basis(&CssPropertyValue::Revert), U32_SENTINEL);
        assert_eq!(encode_flex_basis(&CssPropertyValue::Unset), U32_SENTINEL);
    }

    #[test]
    fn flex_basis_overflow_saturates() {
        assert_eq!(
            encode_flex_basis(&CssPropertyValue::Exact(LayoutFlexBasis::Exact(
                PixelValue::px(1.0e9)
            ))),
            U32_SENTINEL
        );
        assert_eq!(
            encode_flex_basis(&CssPropertyValue::Exact(LayoutFlexBasis::Exact(
                PixelValue::px(f32::INFINITY)
            ))),
            U32_SENTINEL
        );
    }

    // -------------------------------------------------------------------------
    // update_dom_declared_flags
    // -------------------------------------------------------------------------

    fn text_indent_prop() -> CssProperty {
        CssProperty::TextIndent(CssPropertyValue::Exact(StyleTextIndent::default()))
    }

    fn line_height_prop(pct: f32) -> CssProperty {
        CssProperty::LineHeight(CssPropertyValue::Exact(StyleLineHeight {
            inner: PercentageValue::new(pct),
        }))
    }

    #[test]
    fn dom_flags_set_the_right_bit_from_zero() {
        let mut flags = 0u32;
        update_dom_declared_flags(&text_indent_prop(), &mut flags);
        assert_eq!(flags, DOM_HAS_TEXT_INDENT);

        let mut flags2 = 0u32;
        update_dom_declared_flags(&line_height_prop(150.0), &mut flags2);
        assert_eq!(flags2, DOM_HAS_LINE_HEIGHT);
    }

    #[test]
    fn dom_flags_only_ever_or_never_clear() {
        // Starting from all-ones, the function must not clear a single bit.
        let mut flags = u32::MAX;
        update_dom_declared_flags(&text_indent_prop(), &mut flags);
        update_dom_declared_flags(&line_height_prop(150.0), &mut flags);
        update_dom_declared_flags(
            &CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::px(10.0))),
            &mut flags,
        );
        assert_eq!(flags, u32::MAX);
    }

    #[test]
    fn dom_flags_accumulate_and_are_idempotent() {
        let mut flags = 0u32;
        update_dom_declared_flags(&text_indent_prop(), &mut flags);
        update_dom_declared_flags(&line_height_prop(150.0), &mut flags);
        let after_two = flags;
        assert_eq!(after_two, DOM_HAS_TEXT_INDENT | DOM_HAS_LINE_HEIGHT);
        // re-applying the same properties must be a no-op
        update_dom_declared_flags(&text_indent_prop(), &mut flags);
        update_dom_declared_flags(&line_height_prop(150.0), &mut flags);
        assert_eq!(flags, after_two);
    }

    #[test]
    fn dom_flags_are_not_set_for_a_valueless_property() {
        // `line-height: initial` / `text-indent: auto` carry no Exact payload, so the
        // "declared" fast-path bit must stay clear (the slow walk would find nothing).
        let mut flags = 0u32;
        update_dom_declared_flags(&CssProperty::LineHeight(CssPropertyValue::Initial), &mut flags);
        update_dom_declared_flags(&CssProperty::TextIndent(CssPropertyValue::Auto), &mut flags);
        update_dom_declared_flags(&CssProperty::TextIndent(CssPropertyValue::Unset), &mut flags);
        assert_eq!(flags, 0);
    }

    #[test]
    fn dom_flags_ignore_unrelated_properties() {
        let mut flags = 0u32;
        update_dom_declared_flags(
            &CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::px(10.0))),
            &mut flags,
        );
        update_dom_declared_flags(
            &CssProperty::ZIndex(CssPropertyValue::Exact(LayoutZIndex::Integer(3))),
            &mut flags,
        );
        assert_eq!(flags, 0);
    }

    // -------------------------------------------------------------------------
    // apply_css_property_to_compact — tier 1 bitfield
    // -------------------------------------------------------------------------

    #[test]
    fn apply_tier1_fields_do_not_bleed_into_each_other() {
        let mut s = Sink::new();
        s.apply(&CssProperty::Display(CssPropertyValue::Exact(
            LayoutDisplay::InlineBlock,
        )));
        s.apply(&CssProperty::Position(CssPropertyValue::Exact(
            LayoutPosition::Absolute,
        )));
        // border-collapse lives at bit 52, i.e. at the far end of the bitfield
        s.apply(&CssProperty::BorderCollapse(CssPropertyValue::Exact(
            StyleBorderCollapse::Collapse,
        )));

        assert_eq!(
            (s.tier1 >> DISPLAY_SHIFT) & DISPLAY_MASK,
            u64::from(layout_display_to_u8(LayoutDisplay::InlineBlock))
        );
        assert_eq!(
            (s.tier1 >> POSITION_SHIFT) & POSITION_MASK,
            u64::from(layout_position_to_u8(LayoutPosition::Absolute))
        );
        assert_eq!(
            (s.tier1 >> BORDER_COLLAPSE_SHIFT) & BORDER_COLLAPSE_MASK,
            u64::from(border_collapse_to_u8(StyleBorderCollapse::Collapse))
        );

        let known = (DISPLAY_MASK << DISPLAY_SHIFT)
            | (POSITION_MASK << POSITION_SHIFT)
            | (BORDER_COLLAPSE_MASK << BORDER_COLLAPSE_SHIFT);
        assert_eq!(
            s.tier1 & !known,
            0,
            "tier1 = {:#x} has bits set outside the three fields that were written",
            s.tier1
        );
    }

    #[test]
    fn apply_tier1_overwrite_clears_only_its_own_field() {
        // Hostile starting state: every bit set. The clear-then-set in `set_tier1!`
        // must wipe exactly the display field and leave every neighbour intact.
        let mut s = Sink::new();
        s.tier1 = u64::MAX;
        s.apply(&CssProperty::Display(CssPropertyValue::Exact(
            LayoutDisplay::Block,
        )));
        assert_eq!(
            (s.tier1 >> DISPLAY_SHIFT) & DISPLAY_MASK,
            u64::from(layout_display_to_u8(LayoutDisplay::Block))
        );
        let others = !(DISPLAY_MASK << DISPLAY_SHIFT);
        assert_eq!(
            s.tier1 & others,
            u64::MAX & others,
            "neighbouring tier-1 fields were clobbered"
        );
    }

    #[test]
    fn apply_tier1_ignores_a_valueless_property() {
        let mut s = Sink::new();
        s.apply(&CssProperty::Display(CssPropertyValue::Inherit));
        assert_eq!(s.tier1, 0, "`display: inherit` has no Exact payload to encode");
    }

    // -------------------------------------------------------------------------
    // apply_css_property_to_compact — tier 2 dims
    // -------------------------------------------------------------------------

    #[test]
    fn apply_width_round_trips_and_touches_nothing_else() {
        let mut s = Sink::new();
        let before_cold = s.cold;
        let before_text = s.text;
        s.apply(&CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::Px(
            PixelValue::px(320.0),
        ))));
        let dec = decode_pixel_value_u32(s.dims.width).expect("width must round-trip");
        assert!((dec.number.get() - 320.0).abs() < 0.002);
        assert_eq!(s.tier1, 0, "a tier-2 property must not touch the tier-1 bitfield");
        assert_eq!(s.cold, before_cold, "a tier-2 property must not touch tier-2 cold");
        assert_eq!(s.text, before_text, "a tier-2 property must not touch tier-2b text");
        assert!(s.fonts.is_empty());
    }

    #[test]
    fn apply_flex_grow_saturates_and_rejects_negatives() {
        let mut s = Sink::new();
        s.apply(&CssProperty::FlexGrow(CssPropertyValue::Exact(LayoutFlexGrow {
            inner: FloatValue::new(2.5),
        })));
        assert_eq!(s.dims.flex_grow, 250);

        // A negative flex-grow must not wrap around into a huge positive u16.
        let mut neg = Sink::new();
        neg.apply(&CssProperty::FlexGrow(CssPropertyValue::Exact(LayoutFlexGrow {
            inner: FloatValue::new(-1.0),
        })));
        assert_eq!(neg.dims.flex_grow, U16_SENTINEL);

        // ...and neither must an absurdly large one.
        let mut big = Sink::new();
        big.apply(&CssProperty::FlexGrow(CssPropertyValue::Exact(LayoutFlexGrow {
            inner: FloatValue::new(1.0e9),
        })));
        assert_eq!(big.dims.flex_grow, U16_SENTINEL);

        // NaN degrades to 0 rather than to a wrapped value.
        let mut nan = Sink::new();
        nan.apply(&CssProperty::FlexShrink(CssPropertyValue::Exact(
            LayoutFlexShrink { inner: FloatValue::new(f32::NAN) },
        )));
        assert_eq!(nan.dims.flex_shrink, 0);
    }

    #[test]
    fn apply_gap_px_sets_both_axes_and_ignores_unresolvable_units() {
        let mut s = Sink::new();
        s.apply(&CssProperty::Gap(CssPropertyValue::Exact(LayoutGap {
            inner: PixelValue::px(8.0),
        })));
        assert_eq!(s.dims.row_gap, 80);
        assert_eq!(s.dims.column_gap, 80);

        // An `em` gap cannot be resolved without a font context — it must be left
        // untouched (so the slow path can handle it), not silently encoded as 2px.
        let mut em = Sink::new();
        em.apply(&CssProperty::Gap(CssPropertyValue::Exact(LayoutGap {
            inner: PixelValue::em(2.0),
        })));
        assert_eq!(em.dims.row_gap, 0);
        assert_eq!(em.dims.column_gap, 0);
    }

    // -------------------------------------------------------------------------
    // apply_css_property_to_compact — tier 2 cold
    // -------------------------------------------------------------------------

    #[test]
    fn apply_z_index_auto_and_in_range_values() {
        let mut s = Sink::new();
        s.apply(&CssProperty::ZIndex(CssPropertyValue::Exact(LayoutZIndex::Auto)));
        assert_eq!(s.cold.z_index, I16_AUTO);
        s.apply(&CssProperty::ZIndex(CssPropertyValue::Exact(
            LayoutZIndex::Integer(100),
        )));
        assert_eq!(s.cold.z_index, 100);
        // last value below the sentinel band
        s.apply(&CssProperty::ZIndex(CssPropertyValue::Exact(
            LayoutZIndex::Integer(32_763),
        )));
        assert_eq!(s.cold.z_index, 32_763);
    }

    #[test]
    fn apply_z_index_large_positive_saturates() {
        for z in [32_764i32, 100_000, i32::MAX] {
            let mut s = Sink::new();
            s.apply(&CssProperty::ZIndex(CssPropertyValue::Exact(
                LayoutZIndex::Integer(z),
            )));
            assert_eq!(s.cold.z_index, I16_SENTINEL, "z-index {z} should saturate");
        }
    }

    #[test]
    fn apply_z_index_large_negative_must_not_wrap_positive() {
        // The encoder range-checks only the UPPER bound:
        //     if *z >= I16_SENTINEL_THRESHOLD { I16_SENTINEL } else { *z as i16 }
        // so a large negative z-index truncates instead of saturating, e.g.
        //     z-index: -40000  ->  -40000 as i16  ==  +25536
        // which flips the node from the very back of the stacking context to the
        // front. Compare with the line-height encoder, which *does* check
        // `pct_x10 >= -32768` before narrowing.
        for z in [-32_769i32, -40_000, -99_999, i32::MIN] {
            let mut s = Sink::new();
            s.apply(&CssProperty::ZIndex(CssPropertyValue::Exact(
                LayoutZIndex::Integer(z),
            )));
            assert!(
                s.cold.z_index < 0 || s.cold.z_index == I16_SENTINEL,
                "z-index {z} encoded to {}: a negative z-index must stay negative (or \
                 saturate to the sentinel), it must never wrap to a positive value",
                s.cold.z_index,
            );
        }
    }

    #[test]
    fn apply_border_styles_pack_into_independent_nibbles() {
        let mut s = Sink::new();
        s.apply(&CssProperty::BorderTopStyle(CssPropertyValue::Exact(
            StyleBorderTopStyle { inner: BorderStyle::Solid },
        )));
        assert_eq!(
            s.cold.border_styles_packed & 0x000F,
            u16::from(border_style_to_u8(BorderStyle::Solid))
        );
        assert_eq!(
            s.cold.border_styles_packed & 0xFFF0,
            0,
            "the top-style nibble leaked into the other three sides"
        );

        // Re-applying must REPLACE the nibble, not OR into it: Solid(1) | Double(2)
        // would be Dotted(3), a different border style entirely.
        s.apply(&CssProperty::BorderTopStyle(CssPropertyValue::Exact(
            StyleBorderTopStyle { inner: BorderStyle::Double },
        )));
        assert_eq!(
            s.cold.border_styles_packed & 0x000F,
            u16::from(border_style_to_u8(BorderStyle::Double))
        );
    }

    #[test]
    fn apply_opacity_clamps_into_the_0_254_range() {
        for (pct, expected) in [
            (-1.0e9f32, 0u8),
            (-100.0, 0),
            (0.0, 0),
            (50.0, 127),
            (100.0, 254),
            (500.0, 254),
            (1.0e9, 254),
        ] {
            let mut s = Sink::new();
            s.apply(&CssProperty::Opacity(CssPropertyValue::Exact(StyleOpacity {
                inner: PercentageValue::new(pct),
            })));
            assert_eq!(s.cold.opacity, expected, "opacity: {pct}%");
            assert_ne!(
                s.cold.opacity, OPACITY_SENTINEL,
                "an explicitly set opacity must never encode as the 'unset' sentinel"
            );
        }
    }

    #[test]
    fn apply_grid_column_encodes_both_lines() {
        let mut s = Sink::new();
        s.apply(&CssProperty::GridColumn(CssPropertyValue::Exact(GridPlacement {
            grid_start: GridLine::Line(2),
            grid_end: GridLine::Span(3),
        })));
        assert_eq!(s.cold.grid_col_start, 2);
        assert_eq!(s.cold.grid_col_end, -3);
        // grid-row must be untouched by a grid-column declaration
        assert_eq!(s.cold.grid_row_start, I16_AUTO);
        assert_eq!(s.cold.grid_row_end, I16_AUTO);
    }

    #[test]
    fn apply_hot_flags_or_in_without_clobbering_each_other() {
        let mut s = Sink::new();
        s.apply(&CssProperty::TextDecoration(CssPropertyValue::Exact(
            StyleTextDecoration::Underline,
        )));
        assert_eq!(
            s.cold.hot_flags & HOT_FLAG_HAS_TEXT_DECORATION,
            HOT_FLAG_HAS_TEXT_DECORATION
        );

        // scrollbar-gutter writes a 2-bit *field* into the same byte; it must not
        // wipe the has-* bits around it.
        s.apply(&CssProperty::ScrollbarGutter(CssPropertyValue::Exact(
            StyleScrollbarGutter::Stable,
        )));
        assert_eq!(
            (s.cold.hot_flags & HOT_FLAG_SCROLLBAR_GUTTER_MASK) >> HOT_FLAG_SCROLLBAR_GUTTER_SHIFT,
            SCROLLBAR_GUTTER_STABLE
        );
        assert_eq!(
            s.cold.hot_flags & HOT_FLAG_HAS_TEXT_DECORATION,
            HOT_FLAG_HAS_TEXT_DECORATION,
            "scrollbar-gutter cleared the has-text-decoration bit"
        );

        // ...and replacing the gutter value must clear the old bits, not OR into them
        s.apply(&CssProperty::ScrollbarGutter(CssPropertyValue::Exact(
            StyleScrollbarGutter::Auto,
        )));
        assert_eq!(
            (s.cold.hot_flags & HOT_FLAG_SCROLLBAR_GUTTER_MASK) >> HOT_FLAG_SCROLLBAR_GUTTER_SHIFT,
            SCROLLBAR_GUTTER_AUTO
        );
        assert_eq!(
            s.cold.hot_flags & HOT_FLAG_HAS_TEXT_DECORATION,
            HOT_FLAG_HAS_TEXT_DECORATION
        );
    }

    #[test]
    fn apply_valueless_property_does_not_set_a_has_flag() {
        // The has-* bits exist so the getter can skip the cascade walk. A property
        // with no Exact payload must leave them clear, or every node pays for a walk
        // that would find nothing.
        let mut s = Sink::new();
        s.apply(&CssProperty::TextDecoration(CssPropertyValue::Initial));
        s.apply(&CssProperty::ScrollbarGutter(CssPropertyValue::Unset));
        assert_eq!(s.cold.hot_flags, 0);
    }

    // -------------------------------------------------------------------------
    // apply_css_property_to_compact — tier 2b text
    // -------------------------------------------------------------------------

    #[test]
    fn apply_text_color_packs_rgba_big_endian() {
        let mut s = Sink::new();
        s.apply(&CssProperty::TextColor(CssPropertyValue::Exact(StyleTextColor {
            inner: ColorU { r: 0x12, g: 0x34, b: 0x56, a: 0x78 },
        })));
        assert_eq!(s.text.text_color, 0x1234_5678);

        // Documented limitation: rgba(0,0,0,0) is indistinguishable from "unset".
        let mut transparent = Sink::new();
        transparent.apply(&CssProperty::TextColor(CssPropertyValue::Exact(
            StyleTextColor { inner: ColorU { r: 0, g: 0, b: 0, a: 0 } },
        )));
        assert_eq!(transparent.text.text_color, 0);
    }

    #[test]
    fn apply_line_height_round_trips_and_saturates_at_both_ends() {
        let mut s = Sink::new();
        s.apply(&line_height_prop(120.0));
        assert_eq!(s.text.line_height, 1200, "120% must encode as % x 10");

        // Absurd values must saturate — at BOTH ends, no wrap-around.
        for pct in [1.0e9f32, -1.0e9] {
            let mut big = Sink::new();
            big.apply(&line_height_prop(pct));
            assert_eq!(
                big.text.line_height, I16_SENTINEL,
                "line-height {pct}% should saturate to the sentinel"
            );
        }
    }

    #[test]
    fn apply_font_family_hash_is_nonzero_stable_and_registered() {
        let arial = StyleFontFamilyVec::from_vec(vec![StyleFontFamily::System("Arial".into())]);

        let mut s = Sink::new();
        s.apply(&CssProperty::FontFamily(CssPropertyValue::Exact(arial.clone())));
        let h = s.text.font_family_hash;
        assert_ne!(
            h, 0,
            "0 is the 'unset' sentinel — a set font-family must never hash to it"
        );
        assert!(
            s.fonts.contains_key(&h),
            "the hash must be registered in the reverse map, or consumers cannot resolve it"
        );

        // Same input -> same hash (the whole dirty-tracking scheme depends on this).
        let mut same = Sink::new();
        same.apply(&CssProperty::FontFamily(CssPropertyValue::Exact(arial)));
        assert_eq!(same.text.font_family_hash, h);

        // Different input -> different hash.
        let mut other = Sink::new();
        other.apply(&CssProperty::FontFamily(CssPropertyValue::Exact(
            StyleFontFamilyVec::from_vec(vec![StyleFontFamily::System("Times".into())]),
        )));
        assert_ne!(other.text.font_family_hash, h);
    }

    // -------------------------------------------------------------------------
    // apply_ua_css_to_compact
    // -------------------------------------------------------------------------

    #[test]
    fn ua_css_is_idempotent_for_every_representative_node_type() {
        let nodes = [
            NodeData::create_node(NodeType::Html),
            NodeData::create_node(NodeType::Body),
            NodeData::create_node(NodeType::Div),
            NodeData::create_node(NodeType::P),
            NodeData::create_node(NodeType::Br),
            NodeData::create_text("hello"),
        ];
        for nd in &nodes {
            let mut s = Sink::new();
            s.ua(&nd.node_type);
            let once = s.snapshot();
            s.ua(&nd.node_type);
            assert_eq!(
                s.snapshot(),
                once,
                "applying UA CSS twice must be a no-op the second time"
            );
        }
    }

    #[test]
    fn ua_css_never_touches_the_tier1_populated_bit() {
        // Bit 63 is owned by the builder, not by the UA stylesheet.
        for nt in [NodeType::Html, NodeType::Body, NodeType::Div, NodeType::P] {
            let mut s = Sink::new();
            s.ua(&nt);
            assert_eq!(s.tier1 & TIER1_POPULATED_BIT, 0);
        }
    }

    #[test]
    fn ua_css_survives_a_hostile_pre_filled_sink() {
        // Every bit set / every numeric field at an extreme: the writer must still
        // only touch its own fields and must not panic on the sentinel inputs.
        let mut s = Sink::new();
        s.tier1 = u64::MAX;
        s.dims.width = U32_SENTINEL;
        s.dims.font_size = U32_SENTINEL;
        s.dims.flex_grow = U16_SENTINEL;
        s.cold.z_index = i16::MIN;
        s.cold.opacity = OPACITY_SENTINEL;
        s.text.line_height = i16::MIN;
        s.ua(&NodeType::Div);
        assert_eq!(
            s.tier1 & TIER1_POPULATED_BIT,
            TIER1_POPULATED_BIT,
            "UA CSS must not clear bits it does not own"
        );
    }

    // -------------------------------------------------------------------------
    // build_compact_cache
    // -------------------------------------------------------------------------

    #[test]
    fn build_compact_cache_handles_zero_nodes() {
        let cache = CssPropertyCache::empty(0);
        let r = cache.build_compact_cache(&[], &[]);
        assert_eq!(r.node_count(), 0);
        assert!(r.tier2_dims.is_empty());
        assert!(r.font_dirty_nodes.is_empty());
        assert!(r.prev_font_hashes.is_empty());
    }

    #[test]
    fn build_compact_cache_tolerates_a_mismatched_prev_font_hash_slice() {
        let cache = CssPropertyCache::empty(3);
        let nodes = div_nodes(3);
        // longer than node_count, shorter than node_count, and empty — none may panic
        for prev in [vec![1u64, 2, 3, 4, 5, 6], vec![7u64], Vec::new()] {
            let r = cache.build_compact_cache(&nodes, &prev);
            assert_eq!(r.prev_font_hashes.len(), 3);
            assert_eq!(r.node_count(), 3);
        }
    }

    #[test]
    fn build_compact_cache_tolerates_short_node_data() {
        // node_count claims 4 but only 2 NodeDatas are supplied: the trailing nodes
        // must keep their defaults instead of indexing out of bounds.
        let cache = CssPropertyCache::empty(4);
        let r = cache.build_compact_cache(&div_nodes(2), &[]);
        assert_eq!(r.node_count(), 4);
        assert_eq!(r.tier2_dims.len(), 4);
        assert_eq!(r.tier2_cold.len(), 4);
        assert_eq!(r.tier2b_text.len(), 4);
        assert_eq!(r.prev_font_hashes.len(), 4);
    }

    #[test]
    fn build_compact_cache_honours_node_count_over_node_data_len() {
        let cache = CssPropertyCache::empty(2);
        let r = cache.build_compact_cache(&div_nodes(5), &[]);
        assert_eq!(r.node_count(), 2);
    }

    #[test]
    fn build_compact_cache_rebuild_with_unchanged_fonts_is_not_dirty() {
        let cache = CssPropertyCache::empty(3);
        let nodes = div_nodes(3);
        let first = cache.build_compact_cache(&nodes, &[]);
        let second = cache.build_compact_cache(&nodes, &first.prev_font_hashes);
        assert!(
            second.font_dirty_nodes.is_empty(),
            "a rebuild with identical font hashes must not re-resolve any font chain"
        );
    }

    // -------------------------------------------------------------------------
    // build_compact_cache_with_inheritance{,_debug}
    // -------------------------------------------------------------------------

    #[test]
    fn build_with_inheritance_handles_zero_nodes() {
        let cache = CssPropertyCache::empty(0);
        let r = cache.build_compact_cache_with_inheritance(&[], &[], &[]);
        assert_eq!(r.node_count(), 0);

        let mut msgs = None;
        let r2 = cache.build_compact_cache_with_inheritance_debug(&[], &[], &[], &mut msgs);
        assert_eq!(r2.node_count(), 0);
        assert!(msgs.is_none());
    }

    #[test]
    fn build_with_inheritance_propagates_font_size_down_the_chain() {
        let n = 3;
        let cache = CssPropertyCache::empty(n);
        let r = cache.build_compact_cache_with_inheritance(
            &div_nodes(n),
            &linear_hierarchy(n),
            &[],
        );
        assert_eq!(r.node_count(), n);
        // font-size is inheritable: property-less children must match the root exactly.
        assert_eq!(r.tier2_dims[1].font_size, r.tier2_dims[0].font_size);
        assert_eq!(r.tier2_dims[2].font_size, r.tier2_dims[0].font_size);
    }

    #[test]
    fn build_with_inheritance_marks_all_nodes_dirty_on_the_first_build() {
        let n = 3;
        let cache = CssPropertyCache::empty(n);
        let nodes = div_nodes(n);
        let hierarchy = linear_hierarchy(n);

        // Empty prev_font_hashes == first build for this DOM -> force ALL nodes dirty.
        let first = cache.build_compact_cache_with_inheritance(&nodes, &hierarchy, &[]);
        assert_eq!(first.font_dirty_nodes, vec![0, 1, 2]);

        // Second build with the previous hashes -> nothing changed, nothing dirty.
        let second =
            cache.build_compact_cache_with_inheritance(&nodes, &hierarchy, &first.prev_font_hashes);
        assert!(second.font_dirty_nodes.is_empty());
    }

    #[test]
    fn build_with_inheritance_global_star_rules_skip_text_nodes() {
        // Per CSS, `*` matches ELEMENTS. A text node is not an element — it may only
        // inherit from its parent, otherwise `* { padding: 5px }` would overwrite the
        // value a text node inherited from `<p>`.
        let mut cache = CssPropertyCache::empty(2);
        cache
            .global_css_props
            .push(CssProperty::PaddingTop(padding(5.0)));

        let nodes = vec![
            NodeData::create_node(NodeType::Div),
            NodeData::create_text("hi"),
        ];
        let r = cache.build_compact_cache_with_inheritance(&nodes, &linear_hierarchy(2), &[]);

        assert_eq!(
            r.tier2_dims[0].padding_top, 50,
            "the `*` rule must apply to the element"
        );
        assert_ne!(
            r.tier2_dims[1].padding_top, 50,
            "the `*` rule must NOT apply to a text node"
        );
    }

    #[test]
    fn build_with_inheritance_debug_messages_are_opt_in() {
        let n = 2;
        let cache = CssPropertyCache::empty(n);
        let nodes = div_nodes(n);
        let hierarchy = linear_hierarchy(n);

        let mut on = Some(Vec::new());
        let _ = cache.build_compact_cache_with_inheritance_debug(&nodes, &hierarchy, &[], &mut on);
        assert!(
            !on.expect("still Some").is_empty(),
            "debug logging must emit at least one cascade message"
        );

        let mut off = None;
        let _ = cache.build_compact_cache_with_inheritance_debug(&nodes, &hierarchy, &[], &mut off);
        assert!(off.is_none(), "a None sink must stay None");
    }

    // -------------------------------------------------------------------------
    // resolve_font_size_to_px
    // -------------------------------------------------------------------------

    #[test]
    fn resolve_font_size_percent_uses_the_parent() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::percent(50.0));
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        let pv = decode_pixel_value_u32(dims[1].font_size).expect("must resolve to px");
        assert_eq!(pv.metric, SizeMetric::Px);
        assert!((pv.number.get() - 10.0).abs() < 0.01, "got {}", pv.number.get());
    }

    #[test]
    fn resolve_font_size_pt_converts_to_px() {
        let mut dims = vec![CompactNodeProps::default()];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::pt(12.0));
        resolve_font_size_to_px(&mut dims, 0, None);
        let pv = decode_pixel_value_u32(dims[0].font_size).expect("must resolve to px");
        assert!(
            (pv.number.get() - 16.0).abs() < 0.01,
            "12pt should be 16px, got {}",
            pv.number.get()
        );
    }

    #[test]
    fn resolve_font_size_leaves_absolute_and_sentinel_values_alone() {
        // an already-px value must not be re-scaled by the parent
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::px(13.0));
        let before = dims[1].font_size;
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        assert_eq!(dims[1].font_size, before);

        // an explicit sentinel must survive untouched
        let mut sent = vec![CompactNodeProps::default(); 2];
        sent[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        sent[1].font_size = U32_SENTINEL;
        resolve_font_size_to_px(&mut sent, 1, Some(NodeId::new(0)));
        assert_eq!(sent[1].font_size, U32_SENTINEL);

        // ...as must the CSS-initial default (which also sits above the threshold)
        let mut def = vec![CompactNodeProps::default(); 2];
        assert_eq!(def[1].font_size, U32_INITIAL);
        resolve_font_size_to_px(&mut def, 1, Some(NodeId::new(0)));
        assert_eq!(def[1].font_size, U32_INITIAL);
    }

    #[test]
    fn resolve_font_size_negative_em_is_deterministic() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::em(-2.0));
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        let pv = decode_pixel_value_u32(dims[1].font_size).expect("must stay decodable");
        assert!(
            (pv.number.get() + 40.0).abs() < 0.01,
            "-2em of 20px should be -40px, got {}",
            pv.number.get()
        );
    }

    #[test]
    fn resolve_font_size_overflow_saturates_instead_of_wrapping() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        // 100_000em x 20px = 2_000_000px, past the 28-bit fixed-point range
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::em(100_000.0));
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        assert_eq!(
            dims[1].font_size, U32_SENTINEL,
            "an overflowing font-size must land on the tier-3 sentinel, not wrap"
        );
    }

    #[test]
    fn resolve_font_size_nan_em_degrades_to_zero() {
        let mut dims = vec![CompactNodeProps::default(); 2];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::px(20.0));
        dims[1].font_size = encode_pixel_value_u32(&PixelValue::em(f32::NAN));
        resolve_font_size_to_px(&mut dims, 1, Some(NodeId::new(0)));
        let pv = decode_pixel_value_u32(dims[1].font_size).expect("must stay decodable");
        assert!(pv.number.get().is_finite(), "a NaN font-size must not propagate");
        assert_eq!(pv.number.get(), 0.0);
    }

    #[test]
    fn resolve_font_size_root_rem_uses_the_16px_initial_value() {
        // For the ROOT node, `tier2_dims.first()` IS the node itself — and at this
        // point its font-size is still the *unresolved* rem value. The Rem arm then
        // multiplies the rem factor by itself:
        //     html { font-size: 2rem }  ->  2 * 2 = 4px   (should be 2 * 16 = 32px)
        // Every other unit handles the no-parent case correctly via `map_or(16.0, ..)`.
        let mut dims = vec![CompactNodeProps::default()];
        dims[0].font_size = encode_pixel_value_u32(&PixelValue::rem(2.0));
        resolve_font_size_to_px(&mut dims, 0, None);
        let pv = decode_pixel_value_u32(dims[0].font_size).expect("must resolve to px");
        assert!(
            (pv.number.get() - 32.0).abs() < 0.01,
            "root `font-size: 2rem` should resolve against the 16px initial value (= 32px), \
             got {}px",
            pv.number.get()
        );
    }
}
