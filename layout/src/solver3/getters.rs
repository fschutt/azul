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
        LayoutDisplay, LayoutFlexWrap, LayoutFloat, LayoutHeight, LayoutJustifyContent,
        LayoutOverflow, LayoutPosition, LayoutWidth, LayoutWritingMode,
    },
    style::StyleTextAlign,
};

use crate::{
    solver3::{display_list::BorderRadius, layout_tree::LayoutNode, scrollbar::ScrollbarInfo},
    text3::cache::{ParsedFontTrait, StyleProperties},
};

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
    get_css_width,
    get_width,
    LayoutWidth,
    LayoutWidth::default()
);
get_css_property!(
    get_css_height,
    get_height,
    LayoutHeight,
    LayoutHeight::default()
);
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

// Complex Property Getters

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

// Rendering Property Getters

/// Information about background color for a node
pub fn get_background_color(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> ColorU {
    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Get the background content from the styled DOM
    styled_dom
        .css_property_cache
        .ptr
        .get_background_content(node_data, &node_id, node_state)
        .and_then(|bg| bg.get_property())
        .and_then(|bg_vec| bg_vec.get(0)) // Use .get() method on the Vec type
        .and_then(|first_bg| match first_bg {
            azul_css::props::style::StyleBackgroundContent::Color(color) => Some(color.clone()),
            _ => None,
        })
        .unwrap_or(ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0, // Transparent by default
        })
}

/// Information about border rendering
pub struct BorderInfo {
    pub width: f32,
    pub color: ColorU,
}

pub fn get_border_info<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> BorderInfo {
    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Get border width (using top border as representative, could average all sides)
    let width = styled_dom
        .css_property_cache
        .ptr
        .get_border_top_width(node_data, &node_id, node_state)
        .and_then(|w| w.get_property().cloned())
        .map(|w| w.inner.to_pixels(0.0))
        .unwrap_or(0.0);

    // Get border color (using top border as representative)
    let color = styled_dom
        .css_property_cache
        .ptr
        .get_border_top_color(node_data, &node_id, node_state)
        .and_then(|c| c.get_property().cloned())
        .map(|c| c.inner)
        .unwrap_or(ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        });

    BorderInfo { width, color }
}

// Selection and Caret Styling

/// Style information for text selection rendering
#[derive(Debug, Clone, Copy, Default)]
pub struct SelectionStyle {
    pub bg_color: ColorU,
    pub radius: f32,
}

/// Get selection style for a node
pub fn get_selection_style(styled_dom: &StyledDom, node_id: Option<NodeId>) -> SelectionStyle {
    let Some(node_id) = node_id else {
        return SelectionStyle::default();
    };

    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_state = &StyledNodeState::default();

    let bg_color = styled_dom
        .css_property_cache
        .ptr
        .get_selection_background_color(node_data, &node_id, node_state)
        .and_then(|c| c.get_property().cloned())
        .map(|c| c.inner)
        .unwrap_or(ColorU {
            r: 100,
            g: 149,
            b: 237, // Cornflower blue - typical selection color
            a: 128, // Semi-transparent
        });

    SelectionStyle {
        bg_color,
        radius: 0.0, // TODO: Could add a custom -azul-selection-radius property
    }
}

/// Style information for caret rendering
#[derive(Debug, Clone, Copy, Default)]
pub struct CaretStyle {
    pub color: ColorU,
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
            r: 0,
            g: 0,
            b: 0,
            a: 255, // Black caret by default
        });

    let animation_duration = styled_dom
        .css_property_cache
        .ptr
        .get_caret_animation_duration(node_data, &node_id, node_state)
        .and_then(|d| d.get_property().cloned())
        .map(|d| d.inner.inner) // Duration.inner is the u32 milliseconds value
        .unwrap_or(500); // 500ms blink by default

    CaretStyle {
        color,
        animation_duration,
    }
}

// Scrollbar Information

/// Get scrollbar information from a layout node
pub fn get_scrollbar_info_from_layout<T: ParsedFontTrait>(node: &LayoutNode<T>) -> ScrollbarInfo {
    // Check if there's inline content that might overflow
    let has_inline_content = node.inline_layout_result.is_some();

    // For now, we assume standard scrollbar dimensions
    // TODO: Calculate actual overflow by comparing:
    //   - Content size (from inline_layout_result or child positions)
    //   - Container size (from used_size)
    //   - Then check if content exceeds container bounds
    // This requires access to the full layout tree and positioned children

    ScrollbarInfo {
        needs_vertical: false,
        needs_horizontal: false,
        scrollbar_width: if has_inline_content { 16.0 } else { 0.0 },
        scrollbar_height: if has_inline_content { 16.0 } else { 0.0 },
    }
}

// TODO: STUB helper functions that would be needed for the above code.
pub(crate) fn get_display_property(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
) -> LayoutDisplay {
    let Some(id) = dom_id else {
        return LayoutDisplay::Inline;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].state;
    styled_dom
        .css_property_cache
        .ptr
        .get_display(node_data, &id, node_state)
        .and_then(|d| d.get_property().copied())
        .unwrap_or(LayoutDisplay::Inline)
}

// TODO: STUB helper
pub(crate) fn get_style_properties(styled_dom: &StyledDom, dom_id: NodeId) -> StyleProperties {
    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].state;
    let cache = &styled_dom.css_property_cache.ptr;

    let font_family_name = cache
        .get_font_family(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .and_then(|v| v.get(0).map(|f| f.as_string()))
        .unwrap_or_else(|| "sans-serif".to_string());

    let font_size = cache
        .get_font_size(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| v.inner.to_pixels(16.0))
        .unwrap_or(16.0);

    let color = cache
        .get_text_color(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| v.inner)
        .unwrap_or_default();

    let line_height = cache
        .get_line_height(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().cloned())
        .map(|v| v.inner.normalized() * font_size)
        .unwrap_or(font_size * 1.2);

    StyleProperties {
        font_ref: crate::text3::cache::FontRef {
            family: font_family_name,
            weight: rust_fontconfig::FcWeight::Normal, // Stub for now
            style: crate::text3::cache::FontStyle::Normal, // Stub for now
            unicode_ranges: Vec::new(),
        },
        font_size_px: font_size,
        color,
        line_height,
        ..Default::default()
    }
}
