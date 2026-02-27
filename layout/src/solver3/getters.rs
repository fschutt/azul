//! Getter functions for CSS properties from the styled DOM
//!
//! This module provides clean, consistent access to CSS properties with proper
//! fallbacks and type conversions.

use azul_core::{
    dom::{NodeId, NodeType},
    geom::LogicalSize,
    id::NodeId as CoreNodeId,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            font::{StyleFontFamily, StyleFontFamilyVec, StyleFontWeight, StyleFontStyle},
            pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
            ColorU, PhysicalSize, PixelValue, PropertyContext, ResolutionContext,
        },
        layout::{
            BoxDecorationBreak, BreakInside, LayoutBoxSizing, LayoutClear, LayoutDisplay,
            LayoutFlexDirection, LayoutFlexWrap, LayoutFloat, LayoutHeight,
            LayoutJustifyContent, LayoutAlignItems, LayoutAlignContent, LayoutOverflow,
            LayoutPosition, LayoutWidth, LayoutWritingMode, Orphans, PageBreak, Widows,
            grid::GridTemplateAreas,
        },
        property::{CssProperty, CssPropertyType,
            LayoutFlexBasisValue, LayoutFlexDirectionValue, LayoutFlexWrapValue,
            LayoutFlexGrowValue, LayoutFlexShrinkValue,
            LayoutAlignItemsValue, LayoutAlignSelfValue, LayoutAlignContentValue,
            LayoutJustifyContentValue, LayoutJustifyItemsValue, LayoutJustifySelfValue,
            LayoutGapValue,
            LayoutGridTemplateColumnsValue, LayoutGridTemplateRowsValue,
            LayoutGridAutoColumnsValue, LayoutGridAutoRowsValue,
            LayoutGridAutoFlowValue, LayoutGridColumnValue, LayoutGridRowValue,
        },
        style::{
            border_radius::StyleBorderRadius,
            lists::{StyleListStylePosition, StyleListStyleType},
            StyleDirection, StyleTextAlign, StyleUserSelect, StyleVerticalAlign,
            StyleVisibility, StyleWhiteSpace,
        },
    },
};

use crate::{
    font_traits::{ParsedFontTrait, StyleProperties},
    solver3::{
        display_list::{BorderRadius, PhysicalSizeImport},
        layout_tree::LayoutNode,
        scrollbar::ScrollbarRequirements,
    },
};

// Font-size resolution helper functions

/// Helper function to get element's computed font-size
pub fn get_element_font_size(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    node_state: &StyledNodeState,
) -> f32 {
    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let cache = &styled_dom.css_property_cache.ptr;

    // Try to get pre-resolved font-size from computed_values (already in pixels)
    if let Some(vec) = cache.computed_values.get(dom_id.index()) {
        if let Ok(idx) = vec.binary_search_by_key(
            &azul_css::props::property::CssPropertyType::FontSize,
            |(k, _)| *k,
        ) {
            if let azul_css::props::property::CssProperty::FontSize(css_val) = &vec[idx].1.property {
                if let Some(fs) = css_val.get_property() {
                    if fs.inner.metric == azul_css::props::basic::length::SizeMetric::Px {
                        return fs.inner.number.get();
                    }
                }
            }
        }
    }

    // Fallback: get parent font-size (avoid recursion for root)
    let parent_font_size = styled_dom
        .node_hierarchy
        .as_container()
        .get(dom_id)
        .and_then(|node| node.parent_id())
        .map(|parent_id| get_element_font_size(styled_dom, parent_id, node_state))
        .unwrap_or(DEFAULT_FONT_SIZE);

    // Root font-size: use DEFAULT to avoid infinite recursion
    let root_font_size = if dom_id == NodeId::new(0) {
        DEFAULT_FONT_SIZE
    } else {
        get_element_font_size(styled_dom, NodeId::new(0), node_state)
    };

    // Resolve font-size with proper context
    cache
        .get_font_size(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| {
            let context = ResolutionContext {
                element_font_size: DEFAULT_FONT_SIZE,
                parent_font_size,
                root_font_size,
                containing_block_size: PhysicalSize::new(0.0, 0.0),
                element_size: None,
                viewport_size: PhysicalSize::new(0.0, 0.0),
            };

            v.inner
                .resolve_with_context(&context, PropertyContext::FontSize)
        })
        .unwrap_or(DEFAULT_FONT_SIZE)
}

/// Helper function to get parent's computed font-size.
///
/// Retrieves the parent's own `StyledNodeState` so that pseudo-class-specific
/// font-size rules (e.g. `div:hover { font-size: 32px }`) are resolved
/// against the parent's actual state, not the child's.
pub fn get_parent_font_size(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    _node_state: &StyledNodeState, // child's state — intentionally unused
) -> f32 {
    styled_dom
        .node_hierarchy
        .as_container()
        .get(dom_id)
        .and_then(|node| node.parent_id())
        .map(|parent_id| {
            let parent_state = &styled_dom.styled_nodes.as_container()[parent_id].styled_node_state;
            get_element_font_size(styled_dom, parent_id, parent_state)
        })
        .unwrap_or(azul_css::props::basic::pixel::DEFAULT_FONT_SIZE)
}

/// Helper function to get root element's font-size.
///
/// Uses the root element's own `StyledNodeState` so that pseudo-class-specific
/// rules are resolved correctly regardless of which node triggered the call.
pub fn get_root_font_size(styled_dom: &StyledDom, _node_state: &StyledNodeState) -> f32 {
    let root_id = NodeId::new(0);
    let root_state = &styled_dom.styled_nodes.as_container()[root_id].styled_node_state;
    get_element_font_size(styled_dom, root_id, root_state)
}

/// A value that can be Auto, Initial, Inherit, or an explicit value.
/// This preserves CSS cascade semantics better than Option<T>.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum MultiValue<T> {
    /// CSS 'auto' keyword
    Auto,
    /// CSS 'initial' keyword - use initial value
    Initial,
    /// CSS 'inherit' keyword - inherit from parent
    Inherit,
    /// Explicit value (e.g., "10px", "50%")
    Exact(T),
}

impl<T> MultiValue<T> {
    /// Returns true if this is an Auto value
    pub fn is_auto(&self) -> bool {
        matches!(self, MultiValue::Auto)
    }

    /// Returns true if this is an explicit value
    pub fn is_exact(&self) -> bool {
        matches!(self, MultiValue::Exact(_))
    }

    /// Gets the exact value if present
    pub fn exact(self) -> Option<T> {
        match self {
            MultiValue::Exact(v) => Some(v),
            _ => None,
        }
    }

    /// Gets the exact value or returns the provided default
    pub fn unwrap_or(self, default: T) -> T {
        match self {
            MultiValue::Exact(v) => v,
            _ => default,
        }
    }

    /// Gets the exact value or returns T::default()
    pub fn unwrap_or_default(self) -> T
    where
        T: Default,
    {
        match self {
            MultiValue::Exact(v) => v,
            _ => T::default(),
        }
    }

    /// Maps the inner value if Exact, otherwise returns self unchanged
    pub fn map<U, F>(self, f: F) -> MultiValue<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            MultiValue::Exact(v) => MultiValue::Exact(f(v)),
            MultiValue::Auto => MultiValue::Auto,
            MultiValue::Initial => MultiValue::Initial,
            MultiValue::Inherit => MultiValue::Inherit,
        }
    }
}

// Implement helper methods for LayoutOverflow specifically
impl MultiValue<LayoutOverflow> {
    /// Returns true if this overflow value causes content to be clipped.
    /// This includes Hidden, Clip, Auto, and Scroll (all values except Visible).
    pub fn is_clipped(&self) -> bool {
        matches!(
            self,
            MultiValue::Exact(
                LayoutOverflow::Hidden
                    | LayoutOverflow::Clip
                    | LayoutOverflow::Auto
                    | LayoutOverflow::Scroll
            )
        )
    }

    pub fn is_scroll(&self) -> bool {
        matches!(
            self,
            MultiValue::Exact(LayoutOverflow::Scroll | LayoutOverflow::Auto)
        )
    }

    pub fn is_auto_overflow(&self) -> bool {
        matches!(self, MultiValue::Exact(LayoutOverflow::Auto))
    }

    pub fn is_hidden(&self) -> bool {
        matches!(self, MultiValue::Exact(LayoutOverflow::Hidden))
    }

    pub fn is_hidden_or_clip(&self) -> bool {
        matches!(
            self,
            MultiValue::Exact(LayoutOverflow::Hidden | LayoutOverflow::Clip)
        )
    }

    pub fn is_scroll_explicit(&self) -> bool {
        matches!(self, MultiValue::Exact(LayoutOverflow::Scroll))
    }

    pub fn is_visible_or_clip(&self) -> bool {
        matches!(
            self,
            MultiValue::Exact(LayoutOverflow::Visible | LayoutOverflow::Clip)
        )
    }
}

// Implement helper methods for LayoutPosition
impl MultiValue<LayoutPosition> {
    pub fn is_absolute_or_fixed(&self) -> bool {
        matches!(
            self,
            MultiValue::Exact(LayoutPosition::Absolute | LayoutPosition::Fixed)
        )
    }
}

// Implement helper methods for LayoutFloat
impl MultiValue<LayoutFloat> {
    pub fn is_none(&self) -> bool {
        matches!(
            self,
            MultiValue::Auto
                | MultiValue::Initial
                | MultiValue::Inherit
                | MultiValue::Exact(LayoutFloat::None)
        )
    }
}

impl<T: Default> Default for MultiValue<T> {
    fn default() -> Self {
        MultiValue::Auto
    }
}

/// Helper macro to reduce boilerplate for simple CSS property getters
/// Returns the inner PixelValue wrapped in MultiValue
macro_rules! get_css_property_pixel {
    // Variant WITH compact cache fast path for i16-encoded resolved px properties
    ($fn_name:ident, $cache_method:ident, $ua_property:expr, compact_i16 = $compact_method:ident) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<PixelValue> {
            // FAST PATH: compact cache for normal state (O(1) array lookup)
            if node_state.is_normal() {
                if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
                    let raw = cc.$compact_method(node_id.index());
                    if raw == azul_css::compact_cache::I16_AUTO {
                        return MultiValue::Auto;
                    }
                    if raw == azul_css::compact_cache::I16_INITIAL {
                        return MultiValue::Initial;
                    }
                    if raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD {
                        // Valid value: decode i16 ×10 → px
                        return MultiValue::Exact(PixelValue::px(raw as f32 / 10.0));
                    }
                    // I16_SENTINEL or I16_INHERIT → fall through to slow path
                }
            }

            let node_data = &styled_dom.node_data.as_container()[node_id];

            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            if let Some(ref val) = author_css {
                if val.is_auto() {
                    return MultiValue::Auto;
                }
                if let Some(exact) = val.get_property().copied() {
                    return MultiValue::Exact(exact.inner);
                }
            }

            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(inner) = ua_prop.get_pixel_inner() {
                    return MultiValue::Exact(inner);
                }
            }

            MultiValue::Initial
        }
    };
    // Variant WITHOUT compact cache (original behavior)
    ($fn_name:ident, $cache_method:ident, $ua_property:expr) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<PixelValue> {
            let node_data = &styled_dom.node_data.as_container()[node_id];

            // 1. Check author CSS first (includes inline styles - highest priority)
            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            // FIX: Check for Auto FIRST - CssPropertyValue::Auto is a valid value
            // that should NOT fall through to UA CSS. Previously, get_property()
            // returned None for Auto, causing inline "margin: auto" to be ignored.
            if let Some(ref val) = author_css {
                if val.is_auto() {
                    return MultiValue::Auto;
                }
                if let Some(exact) = val.get_property().copied() {
                    return MultiValue::Exact(exact.inner);
                }
                // For Initial, Inherit, None, Revert, Unset - fall through to UA CSS
            }

            // 2. Check User Agent CSS (only if author CSS didn't set a value)
            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(inner) = ua_prop.get_pixel_inner() {
                    return MultiValue::Exact(inner);
                }
            }

            // 3. Fallback to Initial (not set)
            // IMPORTANT: Use Initial, not Auto! In CSS, the initial value for 
            // margin is 0, not auto. Using Auto here caused margins to be treated
            // as "margin: auto" which blocks align-self: stretch in flexbox.
            MultiValue::Initial
        }
    };
}

/// Helper trait to extract PixelValue from any CssProperty variant
trait CssPropertyPixelInner {
    fn get_pixel_inner(&self) -> Option<PixelValue>;
}

impl CssPropertyPixelInner for azul_css::props::property::CssProperty {
    fn get_pixel_inner(&self) -> Option<PixelValue> {
        match self {
            CssProperty::Left(CssPropertyValue::Exact(v)) => Some(v.inner),
            CssProperty::Right(CssPropertyValue::Exact(v)) => Some(v.inner),
            CssProperty::Top(CssPropertyValue::Exact(v)) => Some(v.inner),
            CssProperty::Bottom(CssPropertyValue::Exact(v)) => Some(v.inner),
            CssProperty::MarginLeft(CssPropertyValue::Exact(v)) => Some(v.inner),
            CssProperty::MarginRight(CssPropertyValue::Exact(v)) => Some(v.inner),
            CssProperty::MarginTop(CssPropertyValue::Exact(v)) => Some(v.inner),
            CssProperty::MarginBottom(CssPropertyValue::Exact(v)) => Some(v.inner),
            CssProperty::PaddingLeft(CssPropertyValue::Exact(v)) => Some(v.inner),
            CssProperty::PaddingRight(CssPropertyValue::Exact(v)) => Some(v.inner),
            CssProperty::PaddingTop(CssPropertyValue::Exact(v)) => Some(v.inner),
            CssProperty::PaddingBottom(CssPropertyValue::Exact(v)) => Some(v.inner),
            _ => None,
        }
    }
}

/// Generic macro for CSS properties with UA CSS fallback - returns MultiValue<T>
macro_rules! get_css_property {
    // Variant WITH compact cache fast path (for enum properties in Tier 1)
    ($fn_name:ident, $cache_method:ident, $return_type:ty, $ua_property:expr, compact = $compact_method:ident) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<$return_type> {
            // FAST PATH: compact cache for normal state (O(1) array + bitshift)
            if node_state.is_normal() {
                if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
                    return MultiValue::Exact(cc.$compact_method(node_id.index()));
                }
            }

            // SLOW PATH: full cascade resolution
            let node_data = &styled_dom.node_data.as_container()[node_id];

            // 1. Check author CSS first
            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            if let Some(val) = author_css.and_then(|v| v.get_property().cloned()) {
                return MultiValue::Exact(val);
            }

            // 2. Check User Agent CSS
            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(val) = extract_property_value::<$return_type>(ua_prop) {
                    return MultiValue::Exact(val);
                }
            }

            // 3. Fallback to Auto (not set)
            MultiValue::Auto
        }
    };
    // Variant WITH compact cache for u32-encoded dimension enums (LayoutWidth/LayoutHeight)
    // These types have Auto, Px(PixelValue), MinContent, MaxContent, Calc variants
    ($fn_name:ident, $cache_method:ident, $return_type:ty, $ua_property:expr, compact_u32_dim = $compact_raw_method:ident, $px_variant:path, $auto_variant:path, $min_content_variant:path, $max_content_variant:path) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<$return_type> {
            // FAST PATH: compact cache for normal state
            if node_state.is_normal() {
                if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
                    let raw = cc.$compact_raw_method(node_id.index());
                    match raw {
                        azul_css::compact_cache::U32_AUTO => return MultiValue::Auto,
                        azul_css::compact_cache::U32_INITIAL => return MultiValue::Initial,
                        azul_css::compact_cache::U32_NONE => return MultiValue::Auto,
                        azul_css::compact_cache::U32_MIN_CONTENT => return MultiValue::Exact($min_content_variant),
                        azul_css::compact_cache::U32_MAX_CONTENT => return MultiValue::Exact($max_content_variant),
                        azul_css::compact_cache::U32_SENTINEL | azul_css::compact_cache::U32_INHERIT => {
                            // fall through to slow path
                        }
                        _ => {
                            // Valid encoded pixel value
                            if let Some(pv) = azul_css::compact_cache::decode_pixel_value_u32(raw) {
                                return MultiValue::Exact($px_variant(pv));
                            }
                            // decode failed → slow path
                        }
                    }
                }
            }

            // SLOW PATH: full cascade resolution
            let node_data = &styled_dom.node_data.as_container()[node_id];

            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            if let Some(val) = author_css.and_then(|v| v.get_property().cloned()) {
                return MultiValue::Exact(val);
            }

            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(val) = extract_property_value::<$return_type>(ua_prop) {
                    return MultiValue::Exact(val);
                }
            }

            MultiValue::Auto
        }
    };
    // Variant WITH compact cache for u32-encoded dimension structs (LayoutMinWidth etc.)
    // These types are struct { inner: PixelValue }
    ($fn_name:ident, $cache_method:ident, $return_type:ty, $ua_property:expr, compact_u32_struct = $compact_raw_method:ident) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<$return_type> {
            // FAST PATH: compact cache for normal state
            if node_state.is_normal() {
                if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
                    let raw = cc.$compact_raw_method(node_id.index());
                    match raw {
                        azul_css::compact_cache::U32_AUTO | azul_css::compact_cache::U32_NONE => return MultiValue::Auto,
                        azul_css::compact_cache::U32_INITIAL => return MultiValue::Initial,
                        azul_css::compact_cache::U32_SENTINEL | azul_css::compact_cache::U32_INHERIT => {
                            // fall through to slow path
                        }
                        _ => {
                            if let Some(pv) = azul_css::compact_cache::decode_pixel_value_u32(raw) {
                                return MultiValue::Exact(
                                    <$return_type as azul_css::props::PixelValueTaker>::from_pixel_value(pv)
                                );
                            }
                        }
                    }
                }
            }

            // SLOW PATH
            let node_data = &styled_dom.node_data.as_container()[node_id];

            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            if let Some(val) = author_css.and_then(|v| v.get_property().cloned()) {
                return MultiValue::Exact(val);
            }

            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(val) = extract_property_value::<$return_type>(ua_prop) {
                    return MultiValue::Exact(val);
                }
            }

            MultiValue::Auto
        }
    };
    // Variant WITHOUT compact cache (original behavior)
    ($fn_name:ident, $cache_method:ident, $return_type:ty, $ua_property:expr) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<$return_type> {
            let node_data = &styled_dom.node_data.as_container()[node_id];

            // 1. Check author CSS first
            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            if let Some(val) = author_css.and_then(|v| v.get_property().cloned()) {
                return MultiValue::Exact(val);
            }

            // 2. Check User Agent CSS
            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(val) = extract_property_value::<$return_type>(ua_prop) {
                    return MultiValue::Exact(val);
                }
            }

            // 3. Fallback to Auto (not set)
            MultiValue::Auto
        }
    };
}

/// Helper trait to extract typed values from UA CSS properties
trait ExtractPropertyValue<T> {
    fn extract(&self) -> Option<T>;
}

fn extract_property_value<T>(prop: &azul_css::props::property::CssProperty) -> Option<T>
where
    azul_css::props::property::CssProperty: ExtractPropertyValue<T>,
{
    prop.extract()
}

// Implement extraction for all layout types

impl ExtractPropertyValue<LayoutWidth> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutWidth> {
        match self {
            Self::Width(CssPropertyValue::Exact(v)) => Some(v.clone()),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutHeight> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutHeight> {
        match self {
            Self::Height(CssPropertyValue::Exact(v)) => Some(v.clone()),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutMinWidth> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutMinWidth> {
        match self {
            Self::MinWidth(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutMinHeight> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutMinHeight> {
        match self {
            Self::MinHeight(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutMaxWidth> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutMaxWidth> {
        match self {
            Self::MaxWidth(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutMaxHeight> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutMaxHeight> {
        match self {
            Self::MaxHeight(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutDisplay> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutDisplay> {
        match self {
            Self::Display(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutWritingMode> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutWritingMode> {
        match self {
            Self::WritingMode(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutFlexWrap> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutFlexWrap> {
        match self {
            Self::FlexWrap(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutJustifyContent> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutJustifyContent> {
        match self {
            Self::JustifyContent(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleTextAlign> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<StyleTextAlign> {
        match self {
            Self::TextAlign(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutFloat> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutFloat> {
        match self {
            Self::Float(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutClear> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutClear> {
        match self {
            Self::Clear(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutOverflow> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutOverflow> {
        match self {
            Self::OverflowX(CssPropertyValue::Exact(v)) => Some(*v),
            Self::OverflowY(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutPosition> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutPosition> {
        match self {
            Self::Position(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutBoxSizing> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutBoxSizing> {
        match self {
            Self::BoxSizing(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<PixelValue> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<PixelValue> {
        self.get_pixel_inner()
    }
}

impl ExtractPropertyValue<LayoutFlexDirection> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutFlexDirection> {
        match self {
            Self::FlexDirection(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutAlignItems> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutAlignItems> {
        match self {
            Self::AlignItems(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutAlignContent> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutAlignContent> {
        match self {
            Self::AlignContent(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleFontWeight> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<StyleFontWeight> {
        match self {
            Self::FontWeight(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleFontStyle> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<StyleFontStyle> {
        match self {
            Self::FontStyle(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleVisibility> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<StyleVisibility> {
        match self {
            Self::Visibility(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleWhiteSpace> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<StyleWhiteSpace> {
        match self {
            Self::WhiteSpace(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleDirection> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<StyleDirection> {
        match self {
            Self::Direction(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleVerticalAlign> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<StyleVerticalAlign> {
        match self {
            Self::VerticalAlign(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

get_css_property!(
    get_writing_mode,
    get_writing_mode,
    LayoutWritingMode,
    azul_css::props::property::CssPropertyType::WritingMode,
    compact = get_writing_mode
);

get_css_property!(
    get_css_width,
    get_width,
    LayoutWidth,
    azul_css::props::property::CssPropertyType::Width,
    compact_u32_dim = get_width_raw, LayoutWidth::Px, LayoutWidth::Auto, LayoutWidth::MinContent, LayoutWidth::MaxContent
);

get_css_property!(
    get_css_height,
    get_height,
    LayoutHeight,
    azul_css::props::property::CssPropertyType::Height,
    compact_u32_dim = get_height_raw, LayoutHeight::Px, LayoutHeight::Auto, LayoutHeight::MinContent, LayoutHeight::MaxContent
);

get_css_property!(
    get_wrap,
    get_flex_wrap,
    LayoutFlexWrap,
    azul_css::props::property::CssPropertyType::FlexWrap,
    compact = get_flex_wrap
);

get_css_property!(
    get_justify_content,
    get_justify_content,
    LayoutJustifyContent,
    azul_css::props::property::CssPropertyType::JustifyContent,
    compact = get_justify_content
);

get_css_property!(
    get_text_align,
    get_text_align,
    StyleTextAlign,
    azul_css::props::property::CssPropertyType::TextAlign,
    compact = get_text_align
);

get_css_property!(
    get_float,
    get_float,
    LayoutFloat,
    azul_css::props::property::CssPropertyType::Float,
    compact = get_float
);

get_css_property!(
    get_clear,
    get_clear,
    LayoutClear,
    azul_css::props::property::CssPropertyType::Clear,
    compact = get_clear
);

get_css_property!(
    get_overflow_x,
    get_overflow_x,
    LayoutOverflow,
    azul_css::props::property::CssPropertyType::OverflowX,
    compact = get_overflow_x
);

get_css_property!(
    get_overflow_y,
    get_overflow_y,
    LayoutOverflow,
    azul_css::props::property::CssPropertyType::OverflowY,
    compact = get_overflow_y
);

get_css_property!(
    get_position,
    get_position,
    LayoutPosition,
    azul_css::props::property::CssPropertyType::Position,
    compact = get_position
);

get_css_property!(
    get_css_box_sizing,
    get_box_sizing,
    LayoutBoxSizing,
    azul_css::props::property::CssPropertyType::BoxSizing,
    compact = get_box_sizing
);

get_css_property!(
    get_flex_direction,
    get_flex_direction,
    LayoutFlexDirection,
    azul_css::props::property::CssPropertyType::FlexDirection,
    compact = get_flex_direction
);

get_css_property!(
    get_align_items,
    get_align_items,
    LayoutAlignItems,
    azul_css::props::property::CssPropertyType::AlignItems,
    compact = get_align_items
);

get_css_property!(
    get_align_content,
    get_align_content,
    LayoutAlignContent,
    azul_css::props::property::CssPropertyType::AlignContent,
    compact = get_align_content
);

get_css_property!(
    get_font_weight_property,
    get_font_weight,
    StyleFontWeight,
    azul_css::props::property::CssPropertyType::FontWeight,
    compact = get_font_weight
);

get_css_property!(
    get_font_style_property,
    get_font_style,
    StyleFontStyle,
    azul_css::props::property::CssPropertyType::FontStyle,
    compact = get_font_style
);

get_css_property!(
    get_visibility,
    get_visibility,
    StyleVisibility,
    azul_css::props::property::CssPropertyType::Visibility,
    compact = get_visibility
);

get_css_property!(
    get_white_space_property,
    get_white_space,
    StyleWhiteSpace,
    azul_css::props::property::CssPropertyType::WhiteSpace,
    compact = get_white_space
);

get_css_property!(
    get_direction_property,
    get_direction,
    StyleDirection,
    azul_css::props::property::CssPropertyType::Direction,
    compact = get_direction
);

get_css_property!(
    get_vertical_align_property,
    get_vertical_align,
    StyleVerticalAlign,
    azul_css::props::property::CssPropertyType::VerticalAlign,
    compact = get_vertical_align
);
// Complex Property Getters

/// Get border radius for all four corners (raw CSS property values)
pub fn get_style_border_radius(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> azul_css::props::style::border_radius::StyleBorderRadius {
    let node_data = &styled_dom.node_data.as_container()[node_id];

    let top_left = styled_dom
        .css_property_cache
        .ptr
        .get_border_top_left_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default();

    let top_right = styled_dom
        .css_property_cache
        .ptr
        .get_border_top_right_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default();

    let bottom_right = styled_dom
        .css_property_cache
        .ptr
        .get_border_bottom_right_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default();

    let bottom_left = styled_dom
        .css_property_cache
        .ptr
        .get_border_bottom_left_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default();

    StyleBorderRadius {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    }
}

/// Get border radius for all four corners (resolved to pixels)
///
/// # Arguments
/// * `element_size` - The element's own size (width × height) for % resolution. According to CSS
///   spec, border-radius % uses element's own dimensions.
pub fn get_border_radius(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
    element_size: PhysicalSizeImport,
    viewport_size: LogicalSize,
) -> BorderRadius {
    use azul_css::props::basic::{PhysicalSize, PropertyContext, ResolutionContext};

    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Get font sizes for em/rem resolution
    let element_font_size = get_element_font_size(styled_dom, node_id, node_state);
    let parent_font_size = styled_dom
        .node_hierarchy
        .as_container()
        .get(node_id)
        .and_then(|node| node.parent_id())
        .map(|p| get_element_font_size(styled_dom, p, node_state))
        .unwrap_or(azul_css::props::basic::pixel::DEFAULT_FONT_SIZE);
    let root_font_size = get_root_font_size(styled_dom, node_state);

    // Create resolution context
    let context = ResolutionContext {
        element_font_size,
        parent_font_size,
        root_font_size,
        containing_block_size: PhysicalSize::new(0.0, 0.0), // Not used for border-radius
        element_size: Some(PhysicalSize::new(element_size.width, element_size.height)),
        viewport_size: PhysicalSize::new(viewport_size.width, viewport_size.height),
    };

    let top_left = styled_dom
        .css_property_cache
        .ptr
        .get_border_top_left_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property().cloned())
        .unwrap_or_default();

    let top_right = styled_dom
        .css_property_cache
        .ptr
        .get_border_top_right_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property().cloned())
        .unwrap_or_default();

    let bottom_right = styled_dom
        .css_property_cache
        .ptr
        .get_border_bottom_right_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property().cloned())
        .unwrap_or_default();

    let bottom_left = styled_dom
        .css_property_cache
        .ptr
        .get_border_bottom_left_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property().cloned())
        .unwrap_or_default();

    BorderRadius {
        top_left: top_left
            .inner
            .resolve_with_context(&context, PropertyContext::BorderRadius),
        top_right: top_right
            .inner
            .resolve_with_context(&context, PropertyContext::BorderRadius),
        bottom_right: bottom_right
            .inner
            .resolve_with_context(&context, PropertyContext::BorderRadius),
        bottom_left: bottom_left
            .inner
            .resolve_with_context(&context, PropertyContext::BorderRadius),
    }
}

/// Get z-index for stacking context ordering.
///
/// Returns the resolved integer z-index value:
/// - `z-index: auto` → 0 (participates in parent's stacking context)
/// - `z-index: <integer>` → that integer value
pub fn get_z_index(styled_dom: &StyledDom, node_id: Option<NodeId>) -> i32 {
    use azul_css::props::layout::position::LayoutZIndex;

    let node_id = match node_id {
        Some(id) => id,
        None => return 0,
    };

    let node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;

    // FAST PATH: compact cache for normal state
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            let raw = cc.get_z_index(node_id.index());
            if raw == azul_css::compact_cache::I16_AUTO {
                return 0;
            }
            if raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD {
                return raw as i32;
            }
            // I16_SENTINEL → fall through to slow path
        }
    }

    // SLOW PATH
    let node_data = &styled_dom.node_data.as_container()[node_id];

    styled_dom
        .css_property_cache
        .ptr
        .get_z_index(node_data, &node_id, &node_state)
        .and_then(|v| v.get_property())
        .map(|z| match z {
            LayoutZIndex::Auto => 0,
            LayoutZIndex::Integer(i) => *i,
        })
        .unwrap_or(0)
}

// Rendering Property Getters

/// Information about background color for a node
///
/// # CSS Background Propagation (Special Case for HTML Root)
///
/// According to CSS Backgrounds and Borders Module Level 3, Section "The Canvas Background
/// and the HTML `<body>` Element":
///
/// For HTML documents where the root element is `<html>`, if the computed value of
/// `background-image` on the root element is `none` AND its `background-color` is `transparent`,
/// user agents **must propagate** the computed values of the background properties from the
/// first `<body>` child element to the root element.
///
/// This behavior exists for backwards compatibility with older HTML where backgrounds were
/// typically set on `<body>` using `bgcolor` attributes, and ensures that the `<body>`
/// background covers the entire viewport/canvas even when `<body>` itself has constrained
/// dimensions.
///
/// Implementation: When requesting the background of an `<html>` node, we first check if it
/// has a transparent background with no image. If so, we look for a `<body>` child and use
/// its background instead.
pub fn get_background_color(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> ColorU {
    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Fast path: Get this node's background
    let get_node_bg = |node_id: NodeId, node_data: &azul_core::dom::NodeData| {
        styled_dom
            .css_property_cache
            .ptr
            .get_background_content(node_data, &node_id, node_state)
            .and_then(|bg| bg.get_property())
            .and_then(|bg_vec| bg_vec.get(0).cloned())
            .and_then(|first_bg| match &first_bg {
                azul_css::props::style::StyleBackgroundContent::Color(color) => Some(color.clone()),
                azul_css::props::style::StyleBackgroundContent::Image(_) => None, // Has image, not transparent
                _ => None,
            })
    };

    let own_bg = get_node_bg(node_id, node_data);

    // CSS Background Propagation: Special handling for <html> root element
    // Only check propagation if this is an Html node AND has transparent background (no
    // color/image)
    if !matches!(node_data.node_type, NodeType::Html) || own_bg.is_some() {
        // Not Html or has its own background - return own background or transparent
        return own_bg.unwrap_or(ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        });
    }

    // Html node with transparent background - check if we should propagate from <body>
    let first_child = styled_dom
        .node_hierarchy
        .as_container()
        .get(node_id)
        .and_then(|node| node.first_child_id(node_id));

    let Some(first_child) = first_child else {
        return ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
    };

    let first_child_data = &styled_dom.node_data.as_container()[first_child];

    // Check if first child is <body>
    if !matches!(first_child_data.node_type, NodeType::Body) {
        return ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
    }

    // Propagate <body>'s background to <html> (canvas)
    get_node_bg(first_child, first_child_data).unwrap_or(ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    })
}

/// Returns all background content layers for a node (colors, gradients, images).
/// This is used for rendering backgrounds that may include linear/radial/conic gradients.
///
/// CSS Background Propagation (CSS Backgrounds 3, Section 2.11.2):
/// For HTML documents, if the root `<html>` element has no background (transparent with no image),
/// propagate the background from the first `<body>` child element.
pub fn get_background_contents(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Vec<azul_css::props::style::StyleBackgroundContent> {
    use azul_core::dom::NodeType;
    use azul_css::props::style::StyleBackgroundContent;

    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Helper to get backgrounds for a node
    let get_node_backgrounds =
        |nid: NodeId, ndata: &azul_core::dom::NodeData| -> Vec<StyleBackgroundContent> {
            styled_dom
                .css_property_cache
                .ptr
                .get_background_content(ndata, &nid, node_state)
                .and_then(|bg| bg.get_property())
                .map(|bg_vec| bg_vec.iter().cloned().collect())
                .unwrap_or_default()
        };

    let own_backgrounds = get_node_backgrounds(node_id, node_data);

    // CSS Background Propagation: Special handling for <html> root element
    // Only check propagation if this is an Html node AND has no backgrounds
    if !matches!(node_data.node_type, NodeType::Html) || !own_backgrounds.is_empty() {
        return own_backgrounds;
    }

    // Html node with no backgrounds - check if we should propagate from <body>
    let first_child = styled_dom
        .node_hierarchy
        .as_container()
        .get(node_id)
        .and_then(|node| node.first_child_id(node_id));

    let Some(first_child) = first_child else {
        return own_backgrounds;
    };

    let first_child_data = &styled_dom.node_data.as_container()[first_child];

    // Check if first child is <body>
    if !matches!(first_child_data.node_type, NodeType::Body) {
        return own_backgrounds;
    }

    // Propagate <body>'s backgrounds to <html> (canvas)
    get_node_backgrounds(first_child, first_child_data)
}

/// Information about border rendering
pub struct BorderInfo {
    pub widths: crate::solver3::display_list::StyleBorderWidths,
    pub colors: crate::solver3::display_list::StyleBorderColors,
    pub styles: crate::solver3::display_list::StyleBorderStyles,
}

pub fn get_border_info(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> BorderInfo {
    use crate::solver3::display_list::{StyleBorderColors, StyleBorderStyles, StyleBorderWidths};
    use azul_css::css::CssPropertyValue;
    use azul_css::props::basic::color::ColorU;
    use azul_css::props::style::border::{
        BorderStyle, StyleBorderTopColor, StyleBorderRightColor,
        StyleBorderBottomColor, StyleBorderLeftColor,
        StyleBorderTopStyle, StyleBorderRightStyle,
        StyleBorderBottomStyle, StyleBorderLeftStyle,
    };

    // FAST PATH: compact cache for normal state
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            let idx = node_id.index();

            // Border widths (already have compact path via i16)
            let node_data = &styled_dom.node_data.as_container()[node_id];
            let widths = StyleBorderWidths {
                top: styled_dom.css_property_cache.ptr
                    .get_border_top_width(node_data, &node_id, node_state).cloned(),
                right: styled_dom.css_property_cache.ptr
                    .get_border_right_width(node_data, &node_id, node_state).cloned(),
                bottom: styled_dom.css_property_cache.ptr
                    .get_border_bottom_width(node_data, &node_id, node_state).cloned(),
                left: styled_dom.css_property_cache.ptr
                    .get_border_left_width(node_data, &node_id, node_state).cloned(),
            };

            // Border colors from compact cache
            let make_color = |raw: u32| -> Option<ColorU> {
                if raw == 0 { None } else {
                    Some(ColorU {
                        r: ((raw >> 24) & 0xFF) as u8,
                        g: ((raw >> 16) & 0xFF) as u8,
                        b: ((raw >> 8) & 0xFF) as u8,
                        a: (raw & 0xFF) as u8,
                    })
                }
            };

            let colors = StyleBorderColors {
                top: make_color(cc.get_border_top_color_raw(idx))
                    .map(|c| CssPropertyValue::Exact(StyleBorderTopColor { inner: c })),
                right: make_color(cc.get_border_right_color_raw(idx))
                    .map(|c| CssPropertyValue::Exact(StyleBorderRightColor { inner: c })),
                bottom: make_color(cc.get_border_bottom_color_raw(idx))
                    .map(|c| CssPropertyValue::Exact(StyleBorderBottomColor { inner: c })),
                left: make_color(cc.get_border_left_color_raw(idx))
                    .map(|c| CssPropertyValue::Exact(StyleBorderLeftColor { inner: c })),
            };

            // Border styles from compact cache
            let styles = StyleBorderStyles {
                top: Some(CssPropertyValue::Exact(StyleBorderTopStyle {
                    inner: cc.get_border_top_style(idx),
                })),
                right: Some(CssPropertyValue::Exact(StyleBorderRightStyle {
                    inner: cc.get_border_right_style(idx),
                })),
                bottom: Some(CssPropertyValue::Exact(StyleBorderBottomStyle {
                    inner: cc.get_border_bottom_style(idx),
                })),
                left: Some(CssPropertyValue::Exact(StyleBorderLeftStyle {
                    inner: cc.get_border_left_style(idx),
                })),
            };

            return BorderInfo { widths, colors, styles };
        }
    }

    // SLOW PATH: full cascade
    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Get all border widths
    let widths = StyleBorderWidths {
        top: styled_dom
            .css_property_cache
            .ptr
            .get_border_top_width(node_data, &node_id, node_state)
            .cloned(),
        right: styled_dom
            .css_property_cache
            .ptr
            .get_border_right_width(node_data, &node_id, node_state)
            .cloned(),
        bottom: styled_dom
            .css_property_cache
            .ptr
            .get_border_bottom_width(node_data, &node_id, node_state)
            .cloned(),
        left: styled_dom
            .css_property_cache
            .ptr
            .get_border_left_width(node_data, &node_id, node_state)
            .cloned(),
    };

    // Get all border colors
    let colors = StyleBorderColors {
        top: styled_dom
            .css_property_cache
            .ptr
            .get_border_top_color(node_data, &node_id, node_state)
            .cloned(),
        right: styled_dom
            .css_property_cache
            .ptr
            .get_border_right_color(node_data, &node_id, node_state)
            .cloned(),
        bottom: styled_dom
            .css_property_cache
            .ptr
            .get_border_bottom_color(node_data, &node_id, node_state)
            .cloned(),
        left: styled_dom
            .css_property_cache
            .ptr
            .get_border_left_color(node_data, &node_id, node_state)
            .cloned(),
    };

    // Get all border styles
    let styles = StyleBorderStyles {
        top: styled_dom
            .css_property_cache
            .ptr
            .get_border_top_style(node_data, &node_id, node_state)
            .cloned(),
        right: styled_dom
            .css_property_cache
            .ptr
            .get_border_right_style(node_data, &node_id, node_state)
            .cloned(),
        bottom: styled_dom
            .css_property_cache
            .ptr
            .get_border_bottom_style(node_data, &node_id, node_state)
            .cloned(),
        left: styled_dom
            .css_property_cache
            .ptr
            .get_border_left_style(node_data, &node_id, node_state)
            .cloned(),
    };

    BorderInfo {
        widths,
        colors,
        styles,
    }
}

/// Convert BorderInfo to InlineBorderInfo for inline elements
///
/// This resolves the CSS property values to concrete pixel values and colors
/// that can be used during text rendering.
pub fn get_inline_border_info(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
    border_info: &BorderInfo,
) -> Option<crate::text3::cache::InlineBorderInfo> {
    use crate::text3::cache::InlineBorderInfo;

    // Helper to extract pixel value from border width
    fn get_border_width_px(
        width: &Option<
            azul_css::css::CssPropertyValue<azul_css::props::style::border::LayoutBorderTopWidth>,
        >,
    ) -> f32 {
        width
            .as_ref()
            .and_then(|v| v.get_property())
            .map(|w| w.inner.number.get())
            .unwrap_or(0.0)
    }

    fn get_border_width_px_right(
        width: &Option<
            azul_css::css::CssPropertyValue<azul_css::props::style::border::LayoutBorderRightWidth>,
        >,
    ) -> f32 {
        width
            .as_ref()
            .and_then(|v| v.get_property())
            .map(|w| w.inner.number.get())
            .unwrap_or(0.0)
    }

    fn get_border_width_px_bottom(
        width: &Option<
            azul_css::css::CssPropertyValue<
                azul_css::props::style::border::LayoutBorderBottomWidth,
            >,
        >,
    ) -> f32 {
        width
            .as_ref()
            .and_then(|v| v.get_property())
            .map(|w| w.inner.number.get())
            .unwrap_or(0.0)
    }

    fn get_border_width_px_left(
        width: &Option<
            azul_css::css::CssPropertyValue<azul_css::props::style::border::LayoutBorderLeftWidth>,
        >,
    ) -> f32 {
        width
            .as_ref()
            .and_then(|v| v.get_property())
            .map(|w| w.inner.number.get())
            .unwrap_or(0.0)
    }

    // Helper to extract color from border color
    fn get_border_color_top(
        color: &Option<
            azul_css::css::CssPropertyValue<azul_css::props::style::border::StyleBorderTopColor>,
        >,
    ) -> ColorU {
        color
            .as_ref()
            .and_then(|v| v.get_property())
            .map(|c| c.inner)
            .unwrap_or(ColorU::BLACK)
    }

    fn get_border_color_right(
        color: &Option<
            azul_css::css::CssPropertyValue<azul_css::props::style::border::StyleBorderRightColor>,
        >,
    ) -> ColorU {
        color
            .as_ref()
            .and_then(|v| v.get_property())
            .map(|c| c.inner)
            .unwrap_or(ColorU::BLACK)
    }

    fn get_border_color_bottom(
        color: &Option<
            azul_css::css::CssPropertyValue<azul_css::props::style::border::StyleBorderBottomColor>,
        >,
    ) -> ColorU {
        color
            .as_ref()
            .and_then(|v| v.get_property())
            .map(|c| c.inner)
            .unwrap_or(ColorU::BLACK)
    }

    fn get_border_color_left(
        color: &Option<
            azul_css::css::CssPropertyValue<azul_css::props::style::border::StyleBorderLeftColor>,
        >,
    ) -> ColorU {
        color
            .as_ref()
            .and_then(|v| v.get_property())
            .map(|c| c.inner)
            .unwrap_or(ColorU::BLACK)
    }

    // Extract border-radius (simplified - uses the average of all corners if uniform)
    fn get_border_radius_px(
        styled_dom: &StyledDom,
        node_id: NodeId,
        node_state: &StyledNodeState,
    ) -> Option<f32> {
        let node_data = &styled_dom.node_data.as_container()[node_id];

        let top_left = styled_dom
            .css_property_cache
            .ptr
            .get_border_top_left_radius(node_data, &node_id, node_state)
            .and_then(|br| br.get_property().cloned())
            .map(|v| v.inner.number.get());

        let top_right = styled_dom
            .css_property_cache
            .ptr
            .get_border_top_right_radius(node_data, &node_id, node_state)
            .and_then(|br| br.get_property().cloned())
            .map(|v| v.inner.number.get());

        let bottom_left = styled_dom
            .css_property_cache
            .ptr
            .get_border_bottom_left_radius(node_data, &node_id, node_state)
            .and_then(|br| br.get_property().cloned())
            .map(|v| v.inner.number.get());

        let bottom_right = styled_dom
            .css_property_cache
            .ptr
            .get_border_bottom_right_radius(node_data, &node_id, node_state)
            .and_then(|br| br.get_property().cloned())
            .map(|v| v.inner.number.get());

        // If any radius is defined, use the maximum (for inline, uniform radius is most common)
        let radii: Vec<f32> = [top_left, top_right, bottom_left, bottom_right]
            .into_iter()
            .filter_map(|r| r)
            .collect();

        if radii.is_empty() {
            None
        } else {
            Some(radii.into_iter().fold(0.0f32, |a, b| a.max(b)))
        }
    }

    let top = get_border_width_px(&border_info.widths.top);
    let right = get_border_width_px_right(&border_info.widths.right);
    let bottom = get_border_width_px_bottom(&border_info.widths.bottom);
    let left = get_border_width_px_left(&border_info.widths.left);

    // Fetch padding values for inline elements
    fn resolve_padding(mv: MultiValue<PixelValue>) -> f32 {
        match mv {
            MultiValue::Exact(pv) => {
                use azul_css::props::basic::SizeMetric;
                match pv.metric {
                    SizeMetric::Px => pv.number.get(),
                    SizeMetric::Pt => pv.number.get() * 1.333333,
                    SizeMetric::Em | SizeMetric::Rem => pv.number.get() * 16.0,
                    _ => 0.0,
                }
            }
            _ => 0.0,
        }
    }

    let p_top = resolve_padding(get_css_padding_top(styled_dom, node_id, node_state));
    let p_right = resolve_padding(get_css_padding_right(styled_dom, node_id, node_state));
    let p_bottom = resolve_padding(get_css_padding_bottom(styled_dom, node_id, node_state));
    let p_left = resolve_padding(get_css_padding_left(styled_dom, node_id, node_state));

    // Only return Some if there's actually a border or padding
    let has_border = top > 0.0 || right > 0.0 || bottom > 0.0 || left > 0.0;
    let has_padding = p_top > 0.0 || p_right > 0.0 || p_bottom > 0.0 || p_left > 0.0;
    if !has_border && !has_padding {
        return None;
    }

    Some(InlineBorderInfo {
        top,
        right,
        bottom,
        left,
        top_color: get_border_color_top(&border_info.colors.top),
        right_color: get_border_color_right(&border_info.colors.right),
        bottom_color: get_border_color_bottom(&border_info.colors.bottom),
        left_color: get_border_color_left(&border_info.colors.left),
        radius: get_border_radius_px(styled_dom, node_id, node_state),
        padding_top: p_top,
        padding_right: p_right,
        padding_bottom: p_bottom,
        padding_left: p_left,
    })
}

// Selection and Caret Styling

/// Style information for text selection rendering
#[derive(Debug, Clone, Copy, Default)]
pub struct SelectionStyle {
    /// Background color of the selection highlight
    pub bg_color: ColorU,
    /// Text color when selected (overrides normal text color)
    pub text_color: Option<ColorU>,
    /// Border radius for selection rectangles
    pub radius: f32,
}

/// Get selection style for a node
pub fn get_selection_style(
    styled_dom: &StyledDom, 
    node_id: Option<NodeId>,
    system_style: Option<&std::sync::Arc<azul_css::system::SystemStyle>>,
) -> SelectionStyle {
    let Some(node_id) = node_id else {
        return SelectionStyle::default();
    };

    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_state = &StyledNodeState::default();

    // Try to get selection background from CSS, otherwise use system color, otherwise hard-coded default
    let default_bg = system_style
        .and_then(|ss| ss.colors.selection_background.as_option().copied())
        .unwrap_or(ColorU {
            r: 51,
            g: 153,
            b: 255, // Standard blue selection color
            a: 128, // Semi-transparent
        });

    let bg_color = styled_dom
        .css_property_cache
        .ptr
        .get_selection_background_color(node_data, &node_id, node_state)
        .and_then(|c| c.get_property().cloned())
        .map(|c| c.inner)
        .unwrap_or(default_bg);

    // Try to get selection text color from CSS, otherwise use system color
    let default_text = system_style
        .and_then(|ss| ss.colors.selection_text.as_option().copied());

    let text_color = styled_dom
        .css_property_cache
        .ptr
        .get_selection_color(node_data, &node_id, node_state)
        .and_then(|c| c.get_property().cloned())
        .map(|c| c.inner)
        .or(default_text);

    let radius = styled_dom
        .css_property_cache
        .ptr
        .get_selection_radius(node_data, &node_id, node_state)
        .and_then(|r| r.get_property().cloned())
        .map(|r| r.inner.to_pixels_internal(0.0, 16.0)) // percent=0, em=16px default font size
        .unwrap_or(0.0);

    SelectionStyle {
        bg_color,
        text_color,
        radius,
    }
}

/// Style information for caret rendering
#[derive(Debug, Clone, Copy, Default)]
pub struct CaretStyle {
    pub color: ColorU,
    pub width: f32,
    pub animation_duration: u32,
}

/// Get caret style for a node
pub fn get_caret_style(styled_dom: &StyledDom, node_id: Option<NodeId>) -> CaretStyle {
    let Some(node_id) = node_id else {
        return CaretStyle::default();
    };

    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_state = &StyledNodeState::default();

    let color = styled_dom
        .css_property_cache
        .ptr
        .get_caret_color(node_data, &node_id, node_state)
        .and_then(|c| c.get_property().cloned())
        .map(|c| c.inner)
        .unwrap_or(ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255, // White caret by default
        });

    let width = styled_dom
        .css_property_cache
        .ptr
        .get_caret_width(node_data, &node_id, node_state)
        .and_then(|w| w.get_property().cloned())
        .map(|w| w.inner.to_pixels_internal(0.0, 16.0)) // 16.0 as default em size
        .unwrap_or(2.0); // 2px width by default

    let animation_duration = styled_dom
        .css_property_cache
        .ptr
        .get_caret_animation_duration(node_data, &node_id, node_state)
        .and_then(|d| d.get_property().cloned())
        .map(|d| d.inner.inner) // Duration.inner is the u32 milliseconds value
        .unwrap_or(500); // 500ms blink by default

    CaretStyle {
        color,
        width,
        animation_duration,
    }
}

// Scrollbar Information

/// Get scrollbar information from a layout node.
///
/// Scrollbar requirements are computed during the layout phase in two paths:
/// - BFC layout: `compute_scrollbar_info()` + `merge_scrollbar_info()` in cache.rs
/// - Taffy layout: set in the measure callback in taffy_bridge.rs
///
/// If neither path set `scrollbar_info`, the node genuinely does not need
/// scrollbars. The previous heuristic (>3 children = force overflow) caused
/// false-positive scrollbars on normal containers.
pub fn get_scrollbar_info_from_layout(node: &LayoutNode) -> ScrollbarRequirements {
    node.scrollbar_info
        .clone()
        .unwrap_or_default()
}

/// Resolve the **layout-effective** scrollbar width for a node, in pixels.
///
/// This combines three inputs:
/// 1. CSS `scrollbar-width` property on the node (`auto` → 16, `thin` → 8, `none` → 0)
/// 2. OS-level `ScrollbarPreferences.visibility` (overlay scrollbars → 0 layout reservation)
/// 3. Custom `-azul-scrollbar-style` width override
///
/// For **overlay** scrollbars (macOS `WhenScrolling`, or equivalent), this returns `0.0`
/// because overlay scrollbars are painted on top of content and do not consume layout space.
/// The scrollbar is still *rendered*, but no space is reserved during layout.
///
/// During display-list generation, use `get_scrollbar_style()` instead — that returns
/// the full visual style including the *paint* width (which may be non-zero for overlay).
pub fn get_layout_scrollbar_width_px<T: crate::font_traits::ParsedFontTrait>(
    ctx: &crate::solver3::LayoutContext<'_, T>,
    dom_id: NodeId,
    styled_node_state: &StyledNodeState,
) -> f32 {
    use azul_css::props::style::scrollbar::LayoutScrollbarWidth;

    // Check OS-level preference: overlay scrollbars reserve no layout space.
    if let Some(ref sys) = ctx.system_style {
        use azul_css::system::ScrollbarVisibility;
        match sys.scrollbar_preferences.visibility {
            ScrollbarVisibility::WhenScrolling => return 0.0, // overlay
            ScrollbarVisibility::Always | ScrollbarVisibility::Automatic => {}
        }
    }

    // Per-node CSS resolution
    get_scrollbar_width_px(ctx.styled_dom, dom_id, styled_node_state)
}

get_css_property!(
    get_display_property_internal,
    get_display,
    LayoutDisplay,
    azul_css::props::property::CssPropertyType::Display,
    compact = get_display
);

pub fn get_display_property(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
) -> MultiValue<LayoutDisplay> {
    let Some(id) = dom_id else {
        return MultiValue::Exact(LayoutDisplay::Inline);
    };
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    get_display_property_internal(styled_dom, id, node_state)
}

/// Reads the CSS `vertical-align` property for a DOM node and converts it to
/// the text3 `VerticalAlign` enum used during inline layout.
pub fn get_vertical_align_for_node(
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> crate::text3::cache::VerticalAlign {
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let va = match get_vertical_align_property(styled_dom, dom_id, node_state) {
        MultiValue::Exact(v) => v,
        _ => StyleVerticalAlign::default(),
    };
    match va {
        StyleVerticalAlign::Baseline => crate::text3::cache::VerticalAlign::Baseline,
        StyleVerticalAlign::Top => crate::text3::cache::VerticalAlign::Top,
        StyleVerticalAlign::Middle => crate::text3::cache::VerticalAlign::Middle,
        StyleVerticalAlign::Bottom => crate::text3::cache::VerticalAlign::Bottom,
        StyleVerticalAlign::Sub => crate::text3::cache::VerticalAlign::Sub,
        StyleVerticalAlign::Superscript => crate::text3::cache::VerticalAlign::Super,
        StyleVerticalAlign::TextTop => crate::text3::cache::VerticalAlign::TextTop,
        StyleVerticalAlign::TextBottom => crate::text3::cache::VerticalAlign::TextBottom,
    }
}

pub fn get_style_properties(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    system_style: Option<&std::sync::Arc<azul_css::system::SystemStyle>>,
) -> StyleProperties {
    use azul_css::props::basic::{PhysicalSize, PropertyContext, ResolutionContext};

    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let cache = &styled_dom.css_property_cache.ptr;

    // NEW: Get ALL fonts from CSS font-family, not just first
    use azul_css::props::basic::font::{StyleFontFamily, StyleFontFamilyVec};

    let font_families = cache
        .get_font_family(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .unwrap_or_else(|| {
            // Default to serif (same as browser default)
            StyleFontFamilyVec::from_vec(vec![StyleFontFamily::System("serif".into())])
        });

    // Get parent's font-size for proper em resolution in font-size property
    let parent_font_size = styled_dom
        .node_hierarchy
        .as_container()
        .get(dom_id)
        .and_then(|node| {
            let parent_id = CoreNodeId::from_usize(node.parent)?;
            // Recursively get parent's font-size
            cache
                .get_font_size(
                    &styled_dom.node_data.as_container()[parent_id],
                    &parent_id,
                    &styled_dom.styled_nodes.as_container()[parent_id].styled_node_state,
                )
                .and_then(|v| v.get_property().cloned())
                .map(|v| {
                    // If parent also has em/rem, we'd need to recurse, but for now use fallback
                    use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
                    v.inner.to_pixels_internal(0.0, DEFAULT_FONT_SIZE)
                })
        })
        .unwrap_or(azul_css::props::basic::pixel::DEFAULT_FONT_SIZE);

    let root_font_size = get_root_font_size(styled_dom, node_state);

    // Create resolution context for font-size (em refers to parent)
    let font_size_context = ResolutionContext {
        element_font_size: azul_css::props::basic::pixel::DEFAULT_FONT_SIZE, /* Not used for font-size property */
        parent_font_size,
        root_font_size,
        containing_block_size: PhysicalSize::new(0.0, 0.0),
        element_size: None,
        viewport_size: PhysicalSize::new(0.0, 0.0), // TODO: Pass viewport from LayoutContext
    };

    // Get font-size: either from this node's CSS, or inherit from parent
    // font-size is an inheritable property, so if the node doesn't have
    // an explicit font-size, it should inherit from the parent (not default to 16px)
    let font_size = {
        // FAST PATH: compact cache for normal state
        let mut fast_font_size = None;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                let raw = cc.get_font_size_raw(dom_id.index());
                if raw != azul_css::compact_cache::U32_SENTINEL
                    && raw != azul_css::compact_cache::U32_INHERIT
                    && raw != azul_css::compact_cache::U32_INITIAL
                {
                    if let Some(pv) = azul_css::compact_cache::decode_pixel_value_u32(raw) {
                        fast_font_size = Some(pv.resolve_with_context(
                            &font_size_context,
                            PropertyContext::FontSize,
                        ));
                    }
                }
            }
        }
        fast_font_size.unwrap_or_else(|| {
            cache
                .get_font_size(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().cloned())
                .map(|v| {
                    v.inner
                        .resolve_with_context(&font_size_context, PropertyContext::FontSize)
                })
                .unwrap_or(parent_font_size)
        })
    };

    let color_from_cache = {
        // FAST PATH: compact cache for text color
        let mut fast_color = None;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                let raw = cc.get_text_color_raw(dom_id.index());
                if raw != 0 {
                    // Decode 0xRRGGBBAA → ColorU
                    fast_color = Some(ColorU {
                        r: (raw >> 24) as u8,
                        g: (raw >> 16) as u8,
                        b: (raw >> 8) as u8,
                        a: raw as u8,
                    });
                }
            }
        }
        fast_color.or_else(|| {
            cache
                .get_text_color(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().cloned())
                .map(|v| v.inner)
        })
    };

    // Use system text color as fallback (respects dark/light mode)
    let system_text_color = system_style
        .and_then(|ss| ss.colors.text.as_option().copied())
        .unwrap_or(ColorU::BLACK); // Ultimate fallback if no system style
    
    let color = color_from_cache.unwrap_or(system_text_color);

    let line_height = {
        // FAST PATH: compact cache for line-height (stored as normalized × 1000 i16)
        let mut fast_lh = None;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                if let Some(normalized) = cc.get_line_height(dom_id.index()) {
                    // normalized is the raw i16 / 1000.0 value from decode_resolved_px_i16
                    // But line_height encoding is special: percentage × 10 as i16
                    // decode: i16 / 10.0 → raw percentage value (not /100!)
                    // Wait - get_line_height uses decode_resolved_px_i16 which does val / 10.0
                    // Builder stores: normalized() * 1000.0 as i16
                    // So decoded = i16 / 10.0 = normalized() * 100.0
                    // We need normalized() * font_size, so: decoded / 100.0 * font_size
                    fast_lh = Some(normalized / 100.0 * font_size);
                }
            }
        }
        fast_lh.unwrap_or_else(|| {
            cache
                .get_line_height(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().cloned())
                .map(|v| v.inner.normalized() * font_size)
                .unwrap_or(font_size * 1.2)
        })
    };

    // Get background color for INLINE elements only
    // CSS background-color is NOT inherited. For block-level elements (th, td, div, etc.),
    // the background is painted separately by paint_element_background() in display_list.rs.
    // Only inline elements (span, em, strong, a, etc.) should have their background color
    // propagated through StyleProperties for the text rendering pipeline.
    use azul_css::props::layout::LayoutDisplay;
    let display = cache
        .get_display(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .unwrap_or(LayoutDisplay::Inline);

    // For inline and inline-block elements, get background content and border info
    // Block elements have their backgrounds/borders painted by display_list.rs
    let (background_color, background_content, border) =
        if matches!(display, LayoutDisplay::Inline | LayoutDisplay::InlineBlock) {
            let bg = get_background_color(styled_dom, dom_id, node_state);
            let bg_color = if bg.a > 0 { Some(bg) } else { None };

            // Get full background contents (including gradients)
            let bg_contents = get_background_contents(styled_dom, dom_id, node_state);

            // Get border info for inline elements
            let border_info = get_border_info(styled_dom, dom_id, node_state);
            let inline_border =
                get_inline_border_info(styled_dom, dom_id, node_state, &border_info);

            (bg_color, bg_contents, inline_border)
        } else {
            // Block-level elements: background/border is painted by display_list.rs
            // via push_backgrounds_and_border() in DisplayListBuilder
            (None, Vec::new(), None)
        };

    // Query font-weight from CSS cache
    let font_weight = match get_font_weight_property(styled_dom, dom_id, node_state) {
        MultiValue::Exact(v) => v,
        _ => StyleFontWeight::Normal,
    };

    // Query font-style from CSS cache
    let font_style = match get_font_style_property(styled_dom, dom_id, node_state) {
        MultiValue::Exact(v) => v,
        _ => StyleFontStyle::Normal,
    };

    // Convert StyleFontWeight/StyleFontStyle to fontconfig types
    let fc_weight = super::fc::convert_font_weight(font_weight);
    let fc_style = super::fc::convert_font_style(font_style);

    // Check if any font family is a FontRef - if so, use FontStack::Ref
    // This allows embedded fonts (like Material Icons) to bypass fontconfig
    let font_stack = {
        // Look for a Ref in the font families
        let font_ref = (0..font_families.len())
            .find_map(|i| {
                match font_families.get(i).unwrap() {
                    azul_css::props::basic::font::StyleFontFamily::Ref(r) => Some(r.clone()),
                    _ => None,
                }
            });
        
        // Get platform for resolving system font types
        let platform = system_style.map(|ss| &ss.platform);

        if let Some(font_ref) = font_ref {
            // Use FontStack::Ref for embedded fonts
            FontStack::Ref(font_ref)
        } else {
            // Build regular font stack from all font families
            let mut stack = Vec::with_capacity(font_families.len() + 3);

            for i in 0..font_families.len() {
                let family = font_families.get(i).unwrap();

                // Handle SystemFontType specially - resolve to actual OS font names
                // (e.g., "system:ui" → ["System Font", "Helvetica Neue", "Lucida Grande"] on macOS)
                if let azul_css::props::basic::font::StyleFontFamily::SystemType(system_type) = family {
                    if let Some(platform) = platform {
                        let font_names = system_type.get_fallback_chain(platform);
                        let system_weight = if system_type.is_bold() {
                            rust_fontconfig::FcWeight::Bold
                        } else {
                            fc_weight
                        };
                        let system_style_val = if system_type.is_italic() {
                            crate::text3::cache::FontStyle::Italic
                        } else {
                            fc_style
                        };
                        for font_name in font_names {
                            stack.push(crate::text3::cache::FontSelector {
                                family: font_name.to_string(),
                                weight: system_weight,
                                style: system_style_val,
                                unicode_ranges: Vec::new(),
                            });
                        }
                    } else {
                        // No platform info - fall back to generic sans-serif
                        stack.push(crate::text3::cache::FontSelector {
                            family: "sans-serif".to_string(),
                            weight: fc_weight,
                            style: fc_style,
                            unicode_ranges: Vec::new(),
                        });
                    }
                } else {
                    stack.push(crate::text3::cache::FontSelector {
                        family: family.as_string(),
                        weight: fc_weight,
                        style: fc_style,
                        unicode_ranges: Vec::new(),
                    });
                }
            }

            // Add generic fallbacks (serif/sans-serif will be resolved based on Unicode ranges later)
            let generic_fallbacks = ["sans-serif", "serif", "monospace"];
            for fallback in &generic_fallbacks {
                if !stack
                    .iter()
                    .any(|f| f.family.to_lowercase() == fallback.to_lowercase())
                {
                    stack.push(crate::text3::cache::FontSelector {
                        family: fallback.to_string(),
                        weight: rust_fontconfig::FcWeight::Normal,
                        style: crate::text3::cache::FontStyle::Normal,
                        unicode_ranges: Vec::new(),
                    });
                }
            }

            FontStack::Stack(stack)
        }
    };

    // Get letter-spacing from CSS
    let letter_spacing = {
        // FAST PATH: compact cache for letter-spacing (i16 resolved px × 10)
        let mut fast_ls = None;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                if let Some(px_val) = cc.get_letter_spacing(dom_id.index()) {
                    fast_ls = Some(crate::text3::cache::Spacing::Px(px_val.round() as i32));
                }
            }
        }
        fast_ls.unwrap_or_else(|| {
            cache
                .get_letter_spacing(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().cloned())
                .map(|v| {
                    let px_value = v.inner.resolve_with_context(&font_size_context, PropertyContext::FontSize);
                    crate::text3::cache::Spacing::Px(px_value.round() as i32)
                })
                .unwrap_or_default()
        })
    };

    // Get word-spacing from CSS
    let word_spacing = {
        // FAST PATH: compact cache for word-spacing (i16 resolved px × 10)
        let mut fast_ws = None;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                if let Some(px_val) = cc.get_word_spacing(dom_id.index()) {
                    fast_ws = Some(crate::text3::cache::Spacing::Px(px_val.round() as i32));
                }
            }
        }
        fast_ws.unwrap_or_else(|| {
            cache
                .get_word_spacing(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().cloned())
                .map(|v| {
                    let px_value = v.inner.resolve_with_context(&font_size_context, PropertyContext::FontSize);
                    crate::text3::cache::Spacing::Px(px_value.round() as i32)
                })
                .unwrap_or_default()
        })
    };

    // Get text-decoration from CSS
    let text_decoration = cache
        .get_text_decoration(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| crate::text3::cache::TextDecoration::from_css(v))
        .unwrap_or_default();

    // Get tab-size (tab-size) from CSS
    let tab_size = {
        // FAST PATH: compact cache for tab-size (i16 resolved px × 10)
        let mut fast_tab = None;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                let raw = cc.get_tab_size_raw(dom_id.index());
                if raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD {
                    fast_tab = Some(raw as f32 / 10.0);
                }
            }
        }
        fast_tab.unwrap_or_else(|| {
            cache
                .get_tab_size(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().cloned())
                .map(|v| v.inner.number.get())
                .unwrap_or(8.0)
        })
    };

    let properties = StyleProperties {
        font_stack,
        font_size_px: font_size,
        color,
        background_color,
        background_content,
        border,
        line_height,
        letter_spacing,
        word_spacing,
        text_decoration,
        tab_size,
        // These still use defaults - could be extended in future:
        // font_features, font_variations, text_transform, writing_mode, 
        // text_orientation, text_combine_upright, font_variant_*
        ..Default::default()
    };

    properties
}

pub fn get_list_style_type(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> StyleListStyleType {
    let Some(id) = dom_id else {
        return StyleListStyleType::default();
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_list_style_type(node_data, &id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

pub fn get_list_style_position(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
) -> StyleListStylePosition {
    let Some(id) = dom_id else {
        return StyleListStylePosition::default();
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_list_style_position(node_data, &id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

// New: Taffy Bridge Getters - Box Model Properties with Ua Css Fallback

use azul_css::props::layout::{
    LayoutInsetBottom, LayoutLeft, LayoutMarginBottom, LayoutMarginLeft, LayoutMarginRight,
    LayoutMarginTop, LayoutMaxHeight, LayoutMaxWidth, LayoutMinHeight, LayoutMinWidth,
    LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutRight,
    LayoutTop,
};

/// Get inset (position) properties - returns MultiValue<PixelValue>
get_css_property_pixel!(
    get_css_left,
    get_left,
    azul_css::props::property::CssPropertyType::Left,
    compact_i16 = get_left
);
get_css_property_pixel!(
    get_css_right,
    get_right,
    azul_css::props::property::CssPropertyType::Right,
    compact_i16 = get_right
);
get_css_property_pixel!(
    get_css_top,
    get_top,
    azul_css::props::property::CssPropertyType::Top,
    compact_i16 = get_top
);
get_css_property_pixel!(
    get_css_bottom,
    get_bottom,
    azul_css::props::property::CssPropertyType::Bottom,
    compact_i16 = get_bottom
);

/// Get margin properties - returns MultiValue<PixelValue>
get_css_property_pixel!(
    get_css_margin_left,
    get_margin_left,
    azul_css::props::property::CssPropertyType::MarginLeft,
    compact_i16 = get_margin_left_raw
);
get_css_property_pixel!(
    get_css_margin_right,
    get_margin_right,
    azul_css::props::property::CssPropertyType::MarginRight,
    compact_i16 = get_margin_right_raw
);
get_css_property_pixel!(
    get_css_margin_top,
    get_margin_top,
    azul_css::props::property::CssPropertyType::MarginTop,
    compact_i16 = get_margin_top_raw
);
get_css_property_pixel!(
    get_css_margin_bottom,
    get_margin_bottom,
    azul_css::props::property::CssPropertyType::MarginBottom,
    compact_i16 = get_margin_bottom_raw
);

/// Get padding properties - returns MultiValue<PixelValue>
get_css_property_pixel!(
    get_css_padding_left,
    get_padding_left,
    azul_css::props::property::CssPropertyType::PaddingLeft,
    compact_i16 = get_padding_left_raw
);
get_css_property_pixel!(
    get_css_padding_right,
    get_padding_right,
    azul_css::props::property::CssPropertyType::PaddingRight,
    compact_i16 = get_padding_right_raw
);
get_css_property_pixel!(
    get_css_padding_top,
    get_padding_top,
    azul_css::props::property::CssPropertyType::PaddingTop,
    compact_i16 = get_padding_top_raw
);
get_css_property_pixel!(
    get_css_padding_bottom,
    get_padding_bottom,
    azul_css::props::property::CssPropertyType::PaddingBottom,
    compact_i16 = get_padding_bottom_raw
);

/// Get min/max size properties
get_css_property!(
    get_css_min_width,
    get_min_width,
    LayoutMinWidth,
    azul_css::props::property::CssPropertyType::MinWidth,
    compact_u32_struct = get_min_width_raw
);

get_css_property!(
    get_css_min_height,
    get_min_height,
    LayoutMinHeight,
    azul_css::props::property::CssPropertyType::MinHeight,
    compact_u32_struct = get_min_height_raw
);

get_css_property!(
    get_css_max_width,
    get_max_width,
    LayoutMaxWidth,
    azul_css::props::property::CssPropertyType::MaxWidth,
    compact_u32_struct = get_max_width_raw
);

get_css_property!(
    get_css_max_height,
    get_max_height,
    LayoutMaxHeight,
    azul_css::props::property::CssPropertyType::MaxHeight,
    compact_u32_struct = get_max_height_raw
);

/// Get border width properties (no UA CSS fallback needed, defaults to 0)
get_css_property_pixel!(
    get_css_border_left_width,
    get_border_left_width,
    azul_css::props::property::CssPropertyType::BorderLeftWidth,
    compact_i16 = get_border_left_width_raw
);
get_css_property_pixel!(
    get_css_border_right_width,
    get_border_right_width,
    azul_css::props::property::CssPropertyType::BorderRightWidth,
    compact_i16 = get_border_right_width_raw
);
get_css_property_pixel!(
    get_css_border_top_width,
    get_border_top_width,
    azul_css::props::property::CssPropertyType::BorderTopWidth,
    compact_i16 = get_border_top_width_raw
);
get_css_property_pixel!(
    get_css_border_bottom_width,
    get_border_bottom_width,
    azul_css::props::property::CssPropertyType::BorderBottomWidth,
    compact_i16 = get_border_bottom_width_raw
);

// Fragmentation (page breaking) properties

/// Get break-before property for paged media
pub fn get_break_before(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> PageBreak {
    let Some(id) = dom_id else {
        return PageBreak::Auto;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_break_before(node_data, &id, node_state)
        .and_then(|v| v.get_property().cloned())
        .unwrap_or(PageBreak::Auto)
}

/// Get break-after property for paged media
pub fn get_break_after(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> PageBreak {
    let Some(id) = dom_id else {
        return PageBreak::Auto;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_break_after(node_data, &id, node_state)
        .and_then(|v| v.get_property().cloned())
        .unwrap_or(PageBreak::Auto)
}

/// Check if a PageBreak value forces a page break (always, page, left, right, etc.)
pub fn is_forced_page_break(page_break: PageBreak) -> bool {
    matches!(
        page_break,
        PageBreak::Always
            | PageBreak::Page
            | PageBreak::Left
            | PageBreak::Right
            | PageBreak::Recto
            | PageBreak::Verso
            | PageBreak::All
    )
}

/// Get break-inside property for paged media
pub fn get_break_inside(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> BreakInside {
    let Some(id) = dom_id else {
        return BreakInside::Auto;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_break_inside(node_data, &id, node_state)
        .and_then(|v| v.get_property().cloned())
        .unwrap_or(BreakInside::Auto)
}

/// Get orphans property (minimum lines at bottom of page)
pub fn get_orphans(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> u32 {
    let Some(id) = dom_id else {
        return 2; // Default value
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_orphans(node_data, &id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|o| o.inner)
        .unwrap_or(2)
}

/// Get widows property (minimum lines at top of page)
pub fn get_widows(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> u32 {
    let Some(id) = dom_id else {
        return 2; // Default value
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_widows(node_data, &id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|w| w.inner)
        .unwrap_or(2)
}

/// Get box-decoration-break property
pub fn get_box_decoration_break(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
) -> BoxDecorationBreak {
    let Some(id) = dom_id else {
        return BoxDecorationBreak::Slice;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_box_decoration_break(node_data, &id, node_state)
        .and_then(|v| v.get_property().cloned())
        .unwrap_or(BoxDecorationBreak::Slice)
}

// Helper functions for break properties

/// Check if a PageBreak value is avoid
pub fn is_avoid_page_break(page_break: &PageBreak) -> bool {
    matches!(page_break, PageBreak::Avoid | PageBreak::AvoidPage)
}

/// Check if a BreakInside value prevents breaks
pub fn is_avoid_break_inside(break_inside: &BreakInside) -> bool {
    matches!(
        break_inside,
        BreakInside::Avoid | BreakInside::AvoidPage | BreakInside::AvoidColumn
    )
}

// Font Chain Resolution - Pre-Layout Font Loading

use std::collections::HashMap;

use rust_fontconfig::{FcFontCache, FcWeight, FontFallbackChain, PatternMatch};

use crate::text3::cache::{FontChainKey, FontChainKeyOrRef, FontSelector, FontStack, FontStyle};

/// Result of collecting font stacks from a StyledDom
/// Contains all unique font stacks and the mapping from StyleFontFamiliesHash to FontChainKey
#[derive(Debug, Clone)]
pub struct CollectedFontStacks {
    /// All unique font stacks found in the document (system/file fonts via fontconfig)
    pub font_stacks: Vec<Vec<FontSelector>>,
    /// Map from the font stack hash to the index in font_stacks
    pub hash_to_index: HashMap<u64, usize>,
    /// Direct FontRefs that bypass fontconfig (e.g., embedded icon fonts)
    /// These are keyed by their pointer address for uniqueness
    pub font_refs: HashMap<usize, azul_css::props::basic::font::FontRef>,
}

/// Resolved font chains ready for use in layout
/// This is the result of resolving font stacks against FcFontCache
#[derive(Debug, Clone)]
pub struct ResolvedFontChains {
    /// Map from FontChainKeyOrRef to the resolved FontFallbackChain
    /// For FontChainKeyOrRef::Ref variants, the FontFallbackChain contains
    /// a single-font chain that covers the entire Unicode range.
    pub chains: HashMap<FontChainKeyOrRef, FontFallbackChain>,
}

impl ResolvedFontChains {
    /// Get a font chain by its key
    pub fn get(&self, key: &FontChainKeyOrRef) -> Option<&FontFallbackChain> {
        self.chains.get(key)
    }
    
    /// Get a font chain by FontChainKey (for system fonts)
    pub fn get_by_chain_key(&self, key: &FontChainKey) -> Option<&FontFallbackChain> {
        self.chains.get(&FontChainKeyOrRef::Chain(key.clone()))
    }

    /// Get a font chain for a font stack (via fontconfig)
    pub fn get_for_font_stack(&self, font_stack: &[FontSelector]) -> Option<&FontFallbackChain> {
        let key = FontChainKeyOrRef::Chain(FontChainKey::from_selectors(font_stack));
        self.chains.get(&key)
    }
    
    /// Get a font chain for a FontRef pointer
    pub fn get_for_font_ref(&self, ptr: usize) -> Option<&FontFallbackChain> {
        self.chains.get(&FontChainKeyOrRef::Ref(ptr))
    }

    /// Consume self and return the inner HashMap with FontChainKeyOrRef keys
    ///
    /// This is useful when you need access to both Chain and Ref variants.
    pub fn into_inner(self) -> HashMap<FontChainKeyOrRef, FontFallbackChain> {
        self.chains
    }

    /// Consume self and return only the fontconfig-resolved chains
    /// 
    /// This filters out FontRef entries and returns only the chains
    /// resolved via fontconfig. This is what FontManager expects.
    pub fn into_fontconfig_chains(self) -> HashMap<FontChainKey, FontFallbackChain> {
        self.chains
            .into_iter()
            .filter_map(|(key, chain)| {
                match key {
                    FontChainKeyOrRef::Chain(chain_key) => Some((chain_key, chain)),
                    FontChainKeyOrRef::Ref(_) => None,
                }
            })
            .collect()
    }

    /// Get the number of resolved chains
    pub fn len(&self) -> usize {
        self.chains.len()
    }

    /// Check if there are no resolved chains
    pub fn is_empty(&self) -> bool {
        self.chains.is_empty()
    }
    
    /// Get the number of direct FontRefs
    pub fn font_refs_len(&self) -> usize {
        self.chains.keys().filter(|k| k.is_ref()).count()
    }
}

/// Collect all unique font stacks from a StyledDom
///
/// This is a pure function that iterates over all nodes in the DOM and
/// extracts the font-family property from each node that has text content.
///
/// # Arguments
/// * `styled_dom` - The styled DOM to extract font stacks from
/// * `platform` - The current platform for resolving system font types
///
/// # Returns
/// A `CollectedFontStacks` containing all unique font stacks and a hash-to-index mapping
pub fn collect_font_stacks_from_styled_dom(
    styled_dom: &StyledDom,
    platform: &azul_css::system::Platform,
) -> CollectedFontStacks {
    let mut font_stacks = Vec::new();
    let mut hash_to_index: HashMap<u64, usize> = HashMap::new();
    let mut seen_hashes = std::collections::HashSet::new();
    let mut font_refs: HashMap<usize, azul_css::props::basic::font::FontRef> = HashMap::new();

    let node_data_container = styled_dom.node_data.as_container();
    let styled_nodes_container = styled_dom.styled_nodes.as_container();
    let cache = &styled_dom.css_property_cache.ptr;

    // Iterate over all nodes
    for (node_idx, node_data) in node_data_container.internal.iter().enumerate() {
        // Only process text nodes (they are the ones that need fonts)
        if !matches!(node_data.node_type, NodeType::Text(_)) {
            continue;
        }

        let dom_id = match NodeId::from_usize(node_idx) {
            Some(id) => id,
            None => continue,
        };

        let node_state = &styled_nodes_container[dom_id].styled_node_state;

        // Get font families from CSS
        let font_families = cache
            .get_font_family(node_data, &dom_id, node_state)
            .and_then(|v| v.get_property().cloned())
            .unwrap_or_else(|| {
                StyleFontFamilyVec::from_vec(vec![StyleFontFamily::System("serif".into())])
            });

        // Check if the first font family is a FontRef (direct embedded font)
        // If so, we don't need to go through fontconfig - just collect the FontRef
        if let Some(first_family) = font_families.get(0) {
            if let StyleFontFamily::Ref(font_ref) = first_family {
                let ptr = font_ref.parsed as usize;
                if !font_refs.contains_key(&ptr) {
                    font_refs.insert(ptr, font_ref.clone());
                }
                // Skip the normal font stack processing for FontRef
                continue;
            }
        }

        // Get font weight and style
        let font_weight = match get_font_weight_property(styled_dom, dom_id, node_state) {
            MultiValue::Exact(v) => v,
            _ => StyleFontWeight::Normal,
        };

        let font_style = match get_font_style_property(styled_dom, dom_id, node_state) {
            MultiValue::Exact(v) => v,
            _ => StyleFontStyle::Normal,
        };

        // Convert to fontconfig types
        let mut fc_weight = super::fc::convert_font_weight(font_weight);
        let mut fc_style = super::fc::convert_font_style(font_style);

        // Build font stack (only for non-Ref font families)
        let mut font_stack = Vec::with_capacity(font_families.len() + 3);

        for i in 0..font_families.len() {
            let family = font_families.get(i).unwrap();
            // Skip FontRef entries in the stack - they're handled separately
            if matches!(family, StyleFontFamily::Ref(_)) {
                continue;
            }
            
            // Handle SystemFontType specially - resolve to actual font names
            // and apply the font weight/style from the system font type
            if let StyleFontFamily::SystemType(system_type) = family {
                // Get platform-specific font names using the provided platform
                let font_names = system_type.get_fallback_chain(platform);
                
                // Override weight/style based on system font type
                let system_weight = if system_type.is_bold() {
                    FcWeight::Bold
                } else {
                    fc_weight
                };
                let system_style = if system_type.is_italic() {
                    FontStyle::Italic
                } else {
                    fc_style
                };
                
                // Add each font name from the fallback chain
                for font_name in font_names {
                    font_stack.push(FontSelector {
                        family: font_name.to_string(),
                        weight: system_weight,
                        style: system_style,
                        unicode_ranges: Vec::new(),
                    });
                }
            } else {
                font_stack.push(FontSelector {
                    family: family.as_string(),
                    weight: fc_weight,
                    style: fc_style,
                    unicode_ranges: Vec::new(),
                });
            }
        }

        // Add generic fallbacks
        let generic_fallbacks = ["sans-serif", "serif", "monospace"];
        for fallback in &generic_fallbacks {
            if !font_stack
                .iter()
                .any(|f| f.family.to_lowercase() == fallback.to_lowercase())
            {
                font_stack.push(FontSelector {
                    family: fallback.to_string(),
                    weight: FcWeight::Normal,
                    style: FontStyle::Normal,
                    unicode_ranges: Vec::new(),
                });
            }
        }

        // Skip empty font stacks (can happen if all families were FontRefs)
        if font_stack.is_empty() {
            continue;
        }

        // Compute hash for deduplication
        let key = FontChainKey::from_selectors(&font_stack);
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut hasher);
            hasher.finish()
        };

        // Only add if not seen before
        if !seen_hashes.contains(&hash) {
            seen_hashes.insert(hash);
            let idx = font_stacks.len();
            font_stacks.push(font_stack);
            hash_to_index.insert(hash, idx);
        }
    }

    CollectedFontStacks {
        font_stacks,
        hash_to_index,
        font_refs,
    }
}

/// Resolve all font chains for the collected font stacks
///
/// This is a pure function that takes the collected font stacks and resolves
/// them against the FcFontCache to produce FontFallbackChains.
///
/// # Arguments
/// * `collected` - The collected font stacks from `collect_font_stacks_from_styled_dom`
/// * `fc_cache` - The fontconfig cache to resolve fonts against
///
/// # Returns
/// A `ResolvedFontChains` containing all resolved font chains
pub fn resolve_font_chains(
    collected: &CollectedFontStacks,
    fc_cache: &FcFontCache,
) -> ResolvedFontChains {
    let mut chains = HashMap::new();

    // Resolve system/file font stacks via fontconfig
    for font_stack in &collected.font_stacks {
        if font_stack.is_empty() {
            continue;
        }

        // Build font families list
        let font_families: Vec<String> = font_stack
            .iter()
            .map(|s| s.family.clone())
            .filter(|f| !f.is_empty())
            .collect();

        let font_families = if font_families.is_empty() {
            vec!["sans-serif".to_string()]
        } else {
            font_families
        };

        let weight = font_stack[0].weight;
        let is_italic = font_stack[0].style == FontStyle::Italic;
        let is_oblique = font_stack[0].style == FontStyle::Oblique;

        let cache_key = FontChainKeyOrRef::Chain(FontChainKey {
            font_families: font_families.clone(),
            weight,
            italic: is_italic,
            oblique: is_oblique,
        });

        // Skip if already resolved
        if chains.contains_key(&cache_key) {
            continue;
        }

        // Resolve the font chain
        // IMPORTANT: Use False (not DontCare) when style is Normal.
        // DontCare means "accept italic too" which can match italic fonts.
        // False means "must NOT be italic" which correctly prefers Normal.
        let italic = if is_italic {
            PatternMatch::True
        } else {
            PatternMatch::False
        };
        let oblique = if is_oblique {
            PatternMatch::True
        } else {
            PatternMatch::False
        };

        let mut trace = Vec::new();
        let chain =
            fc_cache.resolve_font_chain(&font_families, weight, italic, oblique, &mut trace);

        chains.insert(cache_key, chain);
    }

    // Create single-font chains for direct FontRefs
    // These bypass fontconfig and cover the entire Unicode range
    // NOTE: FontRefs are handled differently - they don't go through fontconfig at all.
    // The shaping code checks style.font_stack for FontStack::Ref and uses the font directly.
    // We just need to record that we have these font refs for font loading purposes.
    for (ptr, _font_ref) in &collected.font_refs {
        let cache_key = FontChainKeyOrRef::Ref(*ptr);
        
        // For FontRef, we create an empty pattern that will be handled specially
        // during shaping. The font data is already available via the FontRef pointer.
        // We don't insert anything - the shaping code handles FontStack::Ref directly.
        let _ = cache_key; // Mark as used
    }

    ResolvedFontChains { chains }
}

/// Convenience function that collects and resolves font chains in one call
///
/// # Arguments
/// * `styled_dom` - The styled DOM to extract font stacks from
/// * `fc_cache` - The fontconfig cache to resolve fonts against
/// * `platform` - The current platform for resolving system font types
///
/// # Returns
/// A `ResolvedFontChains` containing all resolved font chains
pub fn collect_and_resolve_font_chains(
    styled_dom: &StyledDom,
    fc_cache: &FcFontCache,
    platform: &azul_css::system::Platform,
) -> ResolvedFontChains {
    let collected = collect_font_stacks_from_styled_dom(styled_dom, platform);
    resolve_font_chains(&collected, fc_cache)
}

/// Register all embedded FontRefs from the styled DOM in the FontManager
/// 
/// This must be called BEFORE layout so that the fonts are available
/// for WebRender resource registration after layout.
pub fn register_embedded_fonts_from_styled_dom<T: crate::font_traits::ParsedFontTrait>(
    styled_dom: &StyledDom,
    font_manager: &crate::text3::cache::FontManager<T>,
    platform: &azul_css::system::Platform,
) {
    let collected = collect_font_stacks_from_styled_dom(styled_dom, platform);
    for (_ptr, font_ref) in &collected.font_refs {
        font_manager.register_embedded_font(font_ref);
    }
}

// Font Loading Functions

use std::collections::HashSet;

use rust_fontconfig::FontId;

/// Extract all unique FontIds from resolved font chains
///
/// This function collects all FontIds that are referenced in the font chains,
/// which represents the complete set of fonts that may be needed for rendering.
pub fn collect_font_ids_from_chains(chains: &ResolvedFontChains) -> HashSet<FontId> {
    let mut font_ids = HashSet::new();

    for chain in chains.chains.values() {
        // Collect from CSS fallbacks
        for group in &chain.css_fallbacks {
            for font in &group.fonts {
                font_ids.insert(font.id);
            }
        }

        // Collect from Unicode fallbacks
        for font in &chain.unicode_fallbacks {
            font_ids.insert(font.id);
        }
    }

    font_ids
}

/// Compute which fonts need to be loaded (diff with already loaded fonts)
///
/// # Arguments
/// * `required_fonts` - Set of FontIds that are needed
/// * `already_loaded` - Set of FontIds that are already loaded
///
/// # Returns
/// Set of FontIds that need to be loaded
pub fn compute_fonts_to_load(
    required_fonts: &HashSet<FontId>,
    already_loaded: &HashSet<FontId>,
) -> HashSet<FontId> {
    required_fonts.difference(already_loaded).cloned().collect()
}

/// Result of loading fonts
#[derive(Debug)]
pub struct FontLoadResult<T> {
    /// Successfully loaded fonts
    pub loaded: HashMap<FontId, T>,
    /// FontIds that failed to load, with error messages
    pub failed: Vec<(FontId, String)>,
}

/// Load fonts from disk using the provided loader function
///
/// This is a generic function that works with any font loading implementation.
/// The `load_fn` parameter should be a function that takes font bytes and an index,
/// and returns a parsed font or an error.
///
/// # Arguments
/// * `font_ids` - Set of FontIds to load
/// * `fc_cache` - The fontconfig cache to get font paths from
/// * `load_fn` - Function to load and parse font bytes
///
/// # Returns
/// A `FontLoadResult` containing successfully loaded fonts and any failures
pub fn load_fonts_from_disk<T, F>(
    font_ids: &HashSet<FontId>,
    fc_cache: &FcFontCache,
    load_fn: F,
) -> FontLoadResult<T>
where
    F: Fn(&[u8], usize) -> Result<T, crate::text3::cache::LayoutError>,
{
    let mut loaded = HashMap::new();
    let mut failed = Vec::new();

    for font_id in font_ids {
        // Get font bytes from fc_cache
        let font_bytes = match fc_cache.get_font_bytes(font_id) {
            Some(bytes) => bytes,
            None => {
                failed.push((
                    *font_id,
                    format!("Could not get font bytes for {:?}", font_id),
                ));
                continue;
            }
        };

        // Get font index (for font collections like .ttc files)
        let font_index = fc_cache
            .get_font_by_id(font_id)
            .and_then(|source| match source {
                rust_fontconfig::FontSource::Disk(path) => Some(path.font_index),
                rust_fontconfig::FontSource::Memory(font) => Some(font.font_index),
            })
            .unwrap_or(0) as usize;

        // Load the font using the provided function
        match load_fn(&font_bytes, font_index) {
            Ok(font) => {
                loaded.insert(*font_id, font);
            }
            Err(e) => {
                failed.push((
                    *font_id,
                    format!("Failed to parse font {:?}: {:?}", font_id, e),
                ));
            }
        }
    }

    FontLoadResult { loaded, failed }
}

/// Convenience function to load all required fonts for a styled DOM
///
/// This function:
/// 1. Collects all font stacks from the DOM
/// 2. Resolves them to font chains
/// 3. Extracts all required FontIds
/// 4. Computes which fonts need to be loaded (diff with already loaded)
/// 5. Loads the missing fonts
///
/// # Arguments
/// * `styled_dom` - The styled DOM to extract font requirements from
/// * `fc_cache` - The fontconfig cache
/// * `already_loaded` - Set of FontIds that are already loaded
/// * `load_fn` - Function to load and parse font bytes
/// * `platform` - The current platform for resolving system font types
///
/// # Returns
/// A tuple of (ResolvedFontChains, FontLoadResult)
pub fn resolve_and_load_fonts<T, F>(
    styled_dom: &StyledDom,
    fc_cache: &FcFontCache,
    already_loaded: &HashSet<FontId>,
    load_fn: F,
    platform: &azul_css::system::Platform,
) -> (ResolvedFontChains, FontLoadResult<T>)
where
    F: Fn(&[u8], usize) -> Result<T, crate::text3::cache::LayoutError>,
{
    // Step 1-2: Collect and resolve font chains
    let chains = collect_and_resolve_font_chains(styled_dom, fc_cache, platform);

    // Step 3: Extract all required FontIds
    let required_fonts = collect_font_ids_from_chains(&chains);

    // Step 4: Compute diff
    let fonts_to_load = compute_fonts_to_load(&required_fonts, already_loaded);

    // Step 5: Load missing fonts
    let load_result = load_fonts_from_disk(&fonts_to_load, fc_cache, load_fn);

    (chains, load_result)
}

// ============================================================================
// Scrollbar Style Getters
// ============================================================================

use azul_css::props::style::scrollbar::{
    LayoutScrollbarWidth, ScrollbarVisibilityMode,
    StyleScrollbarColor,
};

/// Computed scrollbar style for a node.
///
/// All visual defaults (colors, width) come from the UA CSS conditional rules
/// in `core/src/ua_css.rs` — individual `CssPropertyWithConditions` entries for
/// `scrollbar-color` and `scrollbar-width`, keyed on `@os` / `@theme`.
///
/// Overlay behaviour (fade timing, visibility, clip) is derived from the
/// resolved `scrollbar-width` mode:
///   - `thin`  → overlay:  fade 500/200 ms, `WhenScrolling`, clip = true
///   - `auto`  → classic:  no fade, `Always`, clip = false
///   - `none`  → hidden:   no fade, `Always`, clip = false
///
/// Per-node CSS overrides (in priority order):
///   1. `-azul-scrollbar-style`  (full `ScrollbarInfo` override)
///   2. `scrollbar-width`        (overrides width + overlay mode)
///   3. `scrollbar-color`        (overrides thumb / track colours)
#[derive(Debug, Clone)]
pub struct ComputedScrollbarStyle {
    /// The scrollbar width mode (auto/thin/none)
    pub width_mode: LayoutScrollbarWidth,
    /// Actual width in pixels (resolved from width_mode or scrollbar-style)
    pub width_px: f32,
    /// Thumb color
    pub thumb_color: ColorU,
    /// Track color
    pub track_color: ColorU,
    /// Button color (for scroll arrows)
    pub button_color: ColorU,
    /// Corner color (where scrollbars meet)
    pub corner_color: ColorU,
    /// Whether to clip the scrollbar to the container's border-radius
    pub clip_to_container_border: bool,
    /// Delay in ms before scrollbar starts fading out (0 = never fade)
    pub fade_delay_ms: u32,
    /// Duration of fade-out animation in ms (0 = instant)
    pub fade_duration_ms: u32,
    /// Scrollbar visibility mode (always / when-scrolling / auto)
    pub visibility: ScrollbarVisibilityMode,
}

impl Default for ComputedScrollbarStyle {
    fn default() -> Self {
        // Evaluate UA CSS rules with a default context (no OS info).
        // Picks the unconditional fallback: classic light, auto width.
        let ctx = azul_css::dynamic_selector::DynamicSelectorContext::default();
        let ua = azul_core::ua_css::evaluate_ua_scrollbar_css(&ctx);
        Self::from_ua_resolved(&ua)
    }
}

impl ComputedScrollbarStyle {
    /// Build from resolved UA scrollbar CSS properties.
    ///
    /// Each property is read individually from the resolved UA CSS.
    fn from_ua_resolved(ua: &azul_core::ua_css::ResolvedUaScrollbar) -> Self {
        let width_mode = ua.width.unwrap_or(LayoutScrollbarWidth::Auto);
        let visibility = ua.visibility.unwrap_or(ScrollbarVisibilityMode::Always);
        let fade_delay_ms = ua.fade_delay.map(|d| d.ms).unwrap_or(0);
        let fade_duration_ms = ua.fade_duration.map(|d| d.ms).unwrap_or(0);

        let width_px = match width_mode {
            LayoutScrollbarWidth::Thin => 8.0,
            LayoutScrollbarWidth::Auto => 12.0,
            LayoutScrollbarWidth::None => 0.0,
        };

        let clip = visibility == ScrollbarVisibilityMode::WhenScrolling;

        let (thumb_color, track_color) = match ua.color {
            Some(StyleScrollbarColor::Custom(c)) => (c.thumb, c.track),
            _ => (ColorU::TRANSPARENT, ColorU::TRANSPARENT),
        };

        Self {
            width_mode,
            width_px,
            thumb_color,
            track_color,
            button_color: ColorU::TRANSPARENT,
            corner_color: ColorU::TRANSPARENT,
            clip_to_container_border: clip,
            fade_delay_ms,
            fade_duration_ms,
            visibility,
        }
    }
}

/// Get the computed scrollbar style for a node.
///
/// Resolution order (later wins):
///   1. UA scrollbar CSS (`CssPropertyWithConditions` in `ua_css.rs`,
///      evaluated via `@os` / `@theme` conditions)
///   2. CSS `-azul-scrollbar-style` (full `ScrollbarInfo` customisation)
///   3. CSS `scrollbar-width`  (overrides width only)
///   4. CSS `scrollbar-color`  (overrides thumb / track colours)
///   5. CSS `-azul-scrollbar-visibility` (overrides visibility + clip)
///   6. CSS `-azul-scrollbar-fade-delay` (overrides fade delay)
///   7. CSS `-azul-scrollbar-fade-duration` (overrides fade duration)
///
/// When `system_style` is `None`, falls back to the unconditional UA rule
/// (classic light scrollbar).
pub fn get_scrollbar_style(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
    system_style: Option<&azul_css::system::SystemStyle>,
) -> ComputedScrollbarStyle {
    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Step 1: Evaluate UA scrollbar CSS using the DynamicSelector system.
    let ctx = match system_style {
        Some(sys) => {
            azul_css::dynamic_selector::DynamicSelectorContext::from_system_style(sys)
        }
        None => azul_css::dynamic_selector::DynamicSelectorContext::default(),
    };
    let ua = azul_core::ua_css::evaluate_ua_scrollbar_css(&ctx);
    let mut result = ComputedScrollbarStyle::from_ua_resolved(&ua);

    // Step 2: Check for -azul-scrollbar-style (full customization)
    if let Some(scrollbar_style) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_style(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.width_px = match scrollbar_style.horizontal.width {
            azul_css::props::layout::dimensions::LayoutWidth::Px(px) => {
                px.to_pixels_internal(16.0, 16.0)
            }
            _ => 16.0,
        };
        result.thumb_color = extract_color_from_background(&scrollbar_style.horizontal.thumb);
        result.track_color = extract_color_from_background(&scrollbar_style.horizontal.track);
        result.button_color = extract_color_from_background(&scrollbar_style.horizontal.button);
        result.corner_color = extract_color_from_background(&scrollbar_style.horizontal.corner);
        result.clip_to_container_border = scrollbar_style.horizontal.clip_to_container_border;
    }

    // Step 3: Check for scrollbar-width (overrides width only, not overlay)
    if let Some(scrollbar_width) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_width(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.width_mode = *scrollbar_width;
        result.width_px = match scrollbar_width {
            LayoutScrollbarWidth::Auto => 12.0,
            LayoutScrollbarWidth::Thin => 8.0,
            LayoutScrollbarWidth::None => 0.0,
        };
    }

    // Step 4: Check for scrollbar-color (overrides thumb/track colors)
    if let Some(scrollbar_color) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_color(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        match scrollbar_color {
            StyleScrollbarColor::Auto => { /* keep */ }
            StyleScrollbarColor::Custom(custom) => {
                result.thumb_color = custom.thumb;
                result.track_color = custom.track;
            }
        }
    }

    // Step 5: Check for -azul-scrollbar-visibility
    if let Some(vis) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_visibility(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.visibility = *vis;
        result.clip_to_container_border = *vis == ScrollbarVisibilityMode::WhenScrolling;
    }

    // Step 6: Check for -azul-scrollbar-fade-delay
    if let Some(delay) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_fade_delay(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.fade_delay_ms = delay.ms;
    }

    // Step 7: Check for -azul-scrollbar-fade-duration
    if let Some(dur) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_fade_duration(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.fade_duration_ms = dur.ms;
    }

    result
}

/// Helper to extract a solid color from a StyleBackgroundContent
fn extract_color_from_background(
    bg: &azul_css::props::style::background::StyleBackgroundContent,
) -> ColorU {
    use azul_css::props::style::background::StyleBackgroundContent;
    match bg {
        StyleBackgroundContent::Color(c) => *c,
        _ => ColorU::TRANSPARENT,
    }
}

/// Check if a node should clip its scrollbar to the container's border-radius
pub fn should_clip_scrollbar_to_border(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> bool {
    let style = get_scrollbar_style(styled_dom, node_id, node_state, None);
    style.clip_to_container_border
}

/// Get the scrollbar width in pixels for a node
pub fn get_scrollbar_width_px(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> f32 {
    let style = get_scrollbar_style(styled_dom, node_id, node_state, None);
    style.width_px
}

/// Checks if text in a node is selectable based on CSS `user-select` property.
///
/// Returns `true` if the text can be selected (default behavior),
/// `false` if `user-select: none` is set.
pub fn is_text_selectable(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> bool {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    
    styled_dom
        .css_property_cache
        .ptr
        .get_user_select(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .map(|us| *us != StyleUserSelect::None)
        .unwrap_or(true) // Default: text is selectable
}

/// Checks if a node has the `contenteditable` attribute set directly.
///
/// Returns `true` if:
/// - The node has `contenteditable: true` set via `.set_contenteditable(true)`
/// - OR the node has `contenteditable` attribute set to `true`
///
/// This does NOT check inheritance - use `is_node_contenteditable_inherited` for that.
pub fn is_node_contenteditable(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    use azul_core::dom::AttributeType;
    
    let node_data = &styled_dom.node_data.as_container()[node_id];
    
    // First check the direct contenteditable field (primary method)
    if node_data.is_contenteditable() {
        return true;
    }
    
    // Also check the attribute for backwards compatibility
    // Only return true if the attribute value is explicitly true
    node_data.attributes.as_ref().iter().any(|attr| {
        matches!(attr, AttributeType::ContentEditable(true))
    })
}
// =============================================================================
// Additional ExtractPropertyValue impls (not in compact cache tier 1/2)
// =============================================================================

use azul_css::props::layout::text::LayoutTextJustify;
use azul_css::props::layout::table::{LayoutTableLayout, StyleBorderCollapse, StyleCaptionSide};
use azul_css::props::style::text::StyleHyphens;
use azul_css::props::style::effects::StyleCursor;

impl ExtractPropertyValue<LayoutTextJustify> for CssProperty {
    fn extract(&self) -> Option<LayoutTextJustify> {
        match self {
            Self::TextJustify(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleHyphens> for CssProperty {
    fn extract(&self) -> Option<StyleHyphens> {
        match self {
            Self::Hyphens(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutTableLayout> for CssProperty {
    fn extract(&self) -> Option<LayoutTableLayout> {
        match self {
            Self::TableLayout(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleBorderCollapse> for CssProperty {
    fn extract(&self) -> Option<StyleBorderCollapse> {
        match self {
            Self::BorderCollapse(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleCaptionSide> for CssProperty {
    fn extract(&self) -> Option<StyleCaptionSide> {
        match self {
            Self::CaptionSide(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleCursor> for CssProperty {
    fn extract(&self) -> Option<StyleCursor> {
        match self {
            Self::Cursor(CssPropertyValue::Exact(v)) => Some(v.clone()),
            _ => None,
        }
    }
}

// =============================================================================
// Additional macro-based getters (not covered by compact cache fast-path getters)
// =============================================================================

get_css_property!(
    get_text_justify,
    get_text_justify,
    LayoutTextJustify,
    CssPropertyType::TextJustify
);

get_css_property!(
    get_hyphens,
    get_hyphens,
    StyleHyphens,
    CssPropertyType::Hyphens
);

get_css_property!(
    get_table_layout,
    get_table_layout,
    LayoutTableLayout,
    CssPropertyType::TableLayout
);

get_css_property!(
    get_border_collapse,
    get_border_collapse,
    StyleBorderCollapse,
    CssPropertyType::BorderCollapse,
    compact = get_border_collapse
);

get_css_property!(
    get_caption_side,
    get_caption_side,
    StyleCaptionSide,
    CssPropertyType::CaptionSide
);

get_css_property!(
    get_cursor_property,
    get_cursor,
    StyleCursor,
    CssPropertyType::Cursor
);

// =============================================================================
// Handwritten getters (Option<T>, special logic, or non-standard returns)
// =============================================================================

/// Get height property value for IFC text layout height reference.
pub fn get_height_value(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<LayoutHeight> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_height(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get shape-inside property. Returns Option<ShapeInside> (cloned).
pub fn get_shape_inside(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::layout::shape::ShapeInside> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_shape_inside(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get shape-outside property. Returns Option<ShapeOutside> (cloned).
pub fn get_shape_outside(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::layout::shape::ShapeOutside> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_shape_outside(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get line-height as the full StyleLineHeight value for caller resolution.
pub fn get_line_height_value(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleLineHeight> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_line_height(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get text-indent as the full StyleTextIndent value for caller resolution.
pub fn get_text_indent_value(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleTextIndent> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_text_indent(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get column-count property. Returns Option<ColumnCount>.
pub fn get_column_count(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::layout::column::ColumnCount> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_column_count(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get column-gap as PixelValue. Returns Option.
pub fn get_column_gap_value(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::layout::spacing::LayoutColumnGap> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_column_gap(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get initial-letter property. Returns Option<StyleInitialLetter>.
pub fn get_initial_letter(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleInitialLetter> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_initial_letter(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get line-clamp property. Returns Option<StyleLineClamp>.
pub fn get_line_clamp(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleLineClamp> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_line_clamp(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get hanging-punctuation property. Returns Option<StyleHangingPunctuation>.
pub fn get_hanging_punctuation(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleHangingPunctuation> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_hanging_punctuation(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get text-combine-upright property. Returns Option<StyleTextCombineUpright>.
pub fn get_text_combine_upright(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleTextCombineUpright> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_text_combine_upright(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get exclusion-margin value. Returns f32 (default 0.0).
pub fn get_exclusion_margin(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> f32 {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_exclusion_margin(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .map(|v| v.inner.get() as f32)
        .unwrap_or(0.0)
}

/// Get hyphenation-language property. Returns Option<StyleHyphenationLanguage>.
pub fn get_hyphenation_language(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::azul_exclusion::StyleHyphenationLanguage> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_hyphenation_language(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get border-spacing property.
pub fn get_border_spacing(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> azul_css::props::layout::table::LayoutBorderSpacing {
    use azul_css::props::basic::pixel::PixelValue;

    // FAST PATH: compact cache for normal state
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            let h_raw = cc.get_border_spacing_h_raw(node_id.index());
            let v_raw = cc.get_border_spacing_v_raw(node_id.index());
            // Both 0 means no border-spacing set (default)
            // Sentinel means non-px unit → slow path
            if h_raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD
                && v_raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD
            {
                return azul_css::props::layout::table::LayoutBorderSpacing {
                    horizontal: PixelValue::px(h_raw as f32 / 10.0),
                    vertical: PixelValue::px(v_raw as f32 / 10.0),
                };
            }
        }
    }

    // SLOW PATH
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_border_spacing(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
        .unwrap_or_default()
}

/// Get opacity value. Returns f32 (default 1.0).
pub fn get_opacity(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> f32 {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_opacity(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .map(|v| v.inner.normalized())
        .unwrap_or(1.0)
}

/// Get filter property. Returns Option with cloned filter list.
pub fn get_filter(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::filter::StyleFilterVec> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_filter(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get backdrop-filter property. Returns Option with cloned filter list.
pub fn get_backdrop_filter(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::filter::StyleFilterVec> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_backdrop_filter(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get box-shadow for left side. Returns Option<StyleBoxShadow> (cloned).
pub fn get_box_shadow_left(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::box_shadow::StyleBoxShadow> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_box_shadow_left(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get box-shadow for right side. Returns Option<StyleBoxShadow> (cloned).
pub fn get_box_shadow_right(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::box_shadow::StyleBoxShadow> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_box_shadow_right(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get box-shadow for top side. Returns Option<StyleBoxShadow> (cloned).
pub fn get_box_shadow_top(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::box_shadow::StyleBoxShadow> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_box_shadow_top(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get box-shadow for bottom side. Returns Option<StyleBoxShadow> (cloned).
pub fn get_box_shadow_bottom(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::box_shadow::StyleBoxShadow> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_box_shadow_bottom(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get text-shadow property. Returns Option<StyleBoxShadow> (cloned).
pub fn get_text_shadow(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::box_shadow::StyleBoxShadow> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_text_shadow(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get transform property. Returns Option (non-empty transform list, cloned).
pub fn get_transform(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::transform::StyleTransformVec> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_transform(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get display property (raw). Returns Option<LayoutDisplay>.
pub fn get_display_raw(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<LayoutDisplay> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_display(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
}

/// Get counter-reset property. Returns Option<CounterReset> (cloned).
pub fn get_counter_reset(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::content::CounterReset> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_counter_reset(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get counter-increment property. Returns Option<CounterIncrement> (cloned).
pub fn get_counter_increment(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::content::CounterIncrement> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom.css_property_cache.ptr
        .get_counter_increment(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// W3C-conformant contenteditable inheritance check.
///
/// In the W3C model, the `contenteditable` attribute is **inherited**:
/// - A node is editable if it has `contenteditable="true"` set directly
/// - OR if its parent has `isContentEditable` as true
/// - UNLESS the node explicitly sets `contenteditable="false"`
///
/// This function traverses up the DOM tree to determine editability.
///
/// # Returns
///
/// - `true` if the node is editable (either directly or via inheritance)
/// - `false` if the node is not editable or has `contenteditable="false"`
///
/// # Example
///
/// ```html
/// <div contenteditable="true">
///   A                              <!-- editable (inherited) -->
///   <div contenteditable="false">
///     B                            <!-- NOT editable (explicitly false) -->
///   </div>
///   C                              <!-- editable (inherited) -->
/// </div>
/// ```
pub fn is_node_contenteditable_inherited(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    use azul_core::dom::AttributeType;
    
    let node_data_container = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    
    let mut current_node_id = Some(node_id);
    
    while let Some(nid) = current_node_id {
        let node_data = &node_data_container[nid];
        
        // First check the direct contenteditable field (set via set_contenteditable())
        // This takes precedence as it's the API-level setting
        if node_data.is_contenteditable() {
            return true;
        }
        
        // Then check for explicit contenteditable attribute on this node
        // This handles HTML-style contenteditable="true" or contenteditable="false"
        for attr in node_data.attributes.as_ref().iter() {
            if let AttributeType::ContentEditable(is_editable) = attr {
                // If explicitly set to true, node is editable
                // If explicitly set to false, node is NOT editable (blocks inheritance)
                return *is_editable;
            }
        }
        
        // No explicit setting on this node, check parent for inheritance
        current_node_id = hierarchy.get(nid).and_then(|h| h.parent_id());
    }
    
    // Reached root without finding contenteditable - not editable
    false
}

/// Find the contenteditable ancestor of a node.
///
/// When focus lands on a text node inside a contenteditable container,
/// we need to find the actual container that has the `contenteditable` attribute.
///
/// # Returns
///
/// - `Some(node_id)` of the contenteditable ancestor (may be the node itself)
/// - `None` if no contenteditable ancestor exists
pub fn find_contenteditable_ancestor(styled_dom: &StyledDom, node_id: NodeId) -> Option<NodeId> {
    use azul_core::dom::AttributeType;
    
    let node_data_container = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    
    let mut current_node_id = Some(node_id);
    
    while let Some(nid) = current_node_id {
        let node_data = &node_data_container[nid];
        
        // First check the direct contenteditable field (set via set_contenteditable())
        if node_data.is_contenteditable() {
            return Some(nid);
        }
        
        // Then check for contenteditable attribute on this node
        for attr in node_data.attributes.as_ref().iter() {
            if let AttributeType::ContentEditable(is_editable) = attr {
                if *is_editable {
                    return Some(nid);
                } else {
                    // Explicitly not editable - stop search
                    return None;
                }
            }
        }
        
        // Check parent
        current_node_id = hierarchy.get(nid).and_then(|h| h.parent_id());
    }
    
    None
}

// --- Taffy bridge property getters ---
//
// These getters return `Option<CssPropertyValue<T>>` (cloned from cache) for use
// by taffy_bridge.rs. The conversion from CssPropertyValue to taffy types is done
// in taffy_bridge.rs itself. Routing access through these functions centralizes
// all CSS property lookups for future cache optimizations (e.g., FxHash migration).

macro_rules! get_css_property_value {
    ($fn_name:ident, $cache_method:ident, $ret_type:ty) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> Option<$ret_type> {
            let node_data = &styled_dom.node_data.as_container()[node_id];
            styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state)
                .cloned()
        }
    };
}

// Flexbox properties
get_css_property_value!(get_flex_direction_prop, get_flex_direction, LayoutFlexDirectionValue);
get_css_property_value!(get_flex_wrap_prop, get_flex_wrap, LayoutFlexWrapValue);
get_css_property_value!(get_flex_grow_prop, get_flex_grow, LayoutFlexGrowValue);
get_css_property_value!(get_flex_shrink_prop, get_flex_shrink, LayoutFlexShrinkValue);
get_css_property_value!(get_flex_basis_prop, get_flex_basis, LayoutFlexBasisValue);

// Alignment properties
get_css_property_value!(get_align_items_prop, get_align_items, LayoutAlignItemsValue);
get_css_property_value!(get_align_self_prop, get_align_self, LayoutAlignSelfValue);
get_css_property_value!(get_align_content_prop, get_align_content, LayoutAlignContentValue);
get_css_property_value!(get_justify_content_prop, get_justify_content, LayoutJustifyContentValue);
get_css_property_value!(get_justify_items_prop, get_justify_items, LayoutJustifyItemsValue);
get_css_property_value!(get_justify_self_prop, get_justify_self, LayoutJustifySelfValue);

// Gap
get_css_property_value!(get_gap_prop, get_gap, LayoutGapValue);

// Grid properties
get_css_property_value!(get_grid_template_rows_prop, get_grid_template_rows, LayoutGridTemplateRowsValue);
get_css_property_value!(get_grid_template_columns_prop, get_grid_template_columns, LayoutGridTemplateColumnsValue);
get_css_property_value!(get_grid_auto_rows_prop, get_grid_auto_rows, LayoutGridAutoRowsValue);
get_css_property_value!(get_grid_auto_columns_prop, get_grid_auto_columns, LayoutGridAutoColumnsValue);
get_css_property_value!(get_grid_auto_flow_prop, get_grid_auto_flow, LayoutGridAutoFlowValue);
get_css_property_value!(get_grid_column_prop, get_grid_column, LayoutGridColumnValue);
get_css_property_value!(get_grid_row_prop, get_grid_row, LayoutGridRowValue);

/// Get grid-template-areas property.
/// Uses the generic `get_property()` since CssPropertyCache lacks a specific getter.
/// Returns the inner `GridTemplateAreas` value (already unwrapped from CssPropertyValue).
pub fn get_grid_template_areas_prop(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<GridTemplateAreas> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_property(node_data, &node_id, node_state, &CssPropertyType::GridTemplateAreas)
        .and_then(|p| {
            if let CssProperty::GridTemplateAreas(v) = p {
                v.get_property().cloned()
            } else {
                None
            }
        })
}
