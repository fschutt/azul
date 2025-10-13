//! Getter functions for CSS properties from the styled DOM
//!
//! This module provides clean, consistent access to CSS properties with proper
//! fallbacks and type conversions.

use azul_core::{
    dom::NodeId,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::props::{
    basic::ColorU,
    layout::{
        LayoutFlexWrap, LayoutFloat, LayoutJustifyContent, LayoutOverflow, LayoutPosition,
        LayoutWritingMode,
    },
    style::StyleTextAlign,
};

use crate::{
    solver3::{display_list::BorderRadius, layout_tree::LayoutNode},
    text3::cache::ParsedFontTrait,
};

// ============================================================================
// Core CSS Property Getters
// ============================================================================

// ============================================================================
// Core CSS Property Getters
// ============================================================================

/// Helper macro to reduce boilerplate for simple CSS property getters
macro_rules! get_css_property {
    ($fn_name:ident, $cache_method:ident, $return_type:ty, $default:expr) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> $return_type {
            styled_dom
                .css_property_cache
                .ptr
                .$cache_method(
                    &styled_dom.node_data.as_container()[node_id],
                    &node_id,
                    node_state,
                )
                .and_then(|v| v.get_property().copied())
                .unwrap_or($default)
        }
    };
}

get_css_property!(
    get_writing_mode,
    get_writing_mode,
    LayoutWritingMode,
    LayoutWritingMode::default()
);
get_css_property!(
    get_wrap,
    get_flex_wrap,
    LayoutFlexWrap,
    LayoutFlexWrap::default()
);
get_css_property!(
    get_justify_content,
    get_justify_content,
    LayoutJustifyContent,
    LayoutJustifyContent::default()
);
get_css_property!(
    get_text_align,
    get_text_align,
    StyleTextAlign,
    StyleTextAlign::default()
);
get_css_property!(get_float, get_float, LayoutFloat, LayoutFloat::None);
get_css_property!(
    get_overflow_x,
    get_overflow_x,
    LayoutOverflow,
    LayoutOverflow::Visible
);
get_css_property!(
    get_overflow_y,
    get_overflow_y,
    LayoutOverflow,
    LayoutOverflow::Visible
);
get_css_property!(
    get_position,
    get_position,
    LayoutPosition,
    LayoutPosition::Static
);

// ============================================================================
// Complex Property Getters
// ============================================================================

/// Get border radius for all four corners
pub fn get_border_radius(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> BorderRadius {
    // TODO: Use the correct percentage resolve value based on container size
    let percent_resolve = 0.0;
    let node_data = &styled_dom.node_data.as_container()[node_id];

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
        top_left: top_left.inner.to_pixels(percent_resolve),
        top_right: top_right.inner.to_pixels(percent_resolve),
        bottom_right: bottom_right.inner.to_pixels(percent_resolve),
        bottom_left: bottom_left.inner.to_pixels(percent_resolve),
    }
}

/// Get z-index for stacking context ordering
pub fn get_z_index(styled_dom: &StyledDom, node_id: Option<NodeId>) -> i32 {
    // TODO: Implement actual z-index retrieval
    let _ = (styled_dom, node_id);
    0
}

// ============================================================================
// Rendering Property Getters
// ============================================================================

/// Information about background color for a node
pub fn get_background_color<T: ParsedFontTrait>(
    _styled_dom: &StyledDom,
    _node_id: NodeId,
    _node_state: &StyledNodeState,
) -> ColorU {
    // TODO: Implement actual background color retrieval
    ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 0,
    }
}

/// Information about border rendering
pub struct BorderInfo {
    pub width: f32,
    pub color: ColorU,
}

pub fn get_border_info<T: ParsedFontTrait>(
    _styled_dom: &StyledDom,
    _node_id: NodeId,
    _node_state: &StyledNodeState,
) -> BorderInfo {
    // TODO: Implement actual border info retrieval
    BorderInfo {
        width: 0.0,
        color: ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
    }
}

// ============================================================================
// Selection and Caret Styling
// ============================================================================

/// Style information for text selection rendering
#[derive(Debug, Clone, Copy, Default)]
pub struct SelectionStyle {
    pub bg_color: ColorU,
    pub radius: f32,
}

/// Get selection style for a node
pub fn get_selection_style(styled_dom: &StyledDom, node_id: Option<NodeId>) -> SelectionStyle {
    // TODO: Read -azul-selection-* properties from the styled DOM
    let _ = (styled_dom, node_id);
    SelectionStyle::default()
}

/// Style information for caret rendering
#[derive(Debug, Clone, Copy, Default)]
pub struct CaretStyle {
    pub color: ColorU,
    pub animation_duration: u32,
}

/// Get caret style for a node
pub fn get_caret_style(styled_dom: &StyledDom, node_id: Option<NodeId>) -> CaretStyle {
    // TODO: Read caret-* properties from the styled DOM
    let _ = (styled_dom, node_id);
    CaretStyle::default()
}

// ============================================================================
// Scrollbar Information
// ============================================================================

/// Information about scrollbar requirements and dimensions
pub struct ScrollbarInfo {
    pub needs_vertical: bool,
    pub needs_horizontal: bool,
    pub scrollbar_width: f32,
    pub scrollbar_height: f32,
}

/// Get scrollbar information from a layout node
pub fn get_scrollbar_info_from_layout<T: ParsedFontTrait>(_node: &LayoutNode<T>) -> ScrollbarInfo {
    // TODO: Calculate actual scrollbar requirements based on overflow
    ScrollbarInfo {
        needs_vertical: false,
        needs_horizontal: false,
        scrollbar_width: 16.0,
        scrollbar_height: 16.0,
    }
}
