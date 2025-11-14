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
    style::{StyleTextAlign, lists::{StyleListStyleType, StyleListStylePosition}},
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
    get_writing_mode,
    get_writing_mode,
    LayoutWritingMode,
    LayoutWritingMode::default()
);

// Width and Height need special handling for User Agent CSS
pub fn get_css_width(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutWidth {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    
    // 1. Check author CSS first
    if let Some(width) = styled_dom
        .css_property_cache
        .ptr
        .get_width(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
    {
        return width;
    }
    
    // 2. Check User Agent CSS
    let node_type = styled_dom.node_data.as_container()[node_id].node_type.clone();
    if let Some(ua_prop) = azul_core::ua_css::get_ua_property(node_type, azul_css::props::property::CssPropertyType::Width) {
        if let azul_css::props::property::CssProperty::Width(azul_css::css::CssPropertyValue::Exact(w)) = ua_prop {
            return *w;
        }
    }
    
    // 3. Fallback to type default
    LayoutWidth::default()  // Returns Auto, which is semantically correct
}

pub fn get_css_height(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutHeight {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    
    // 1. Check author CSS first
    if let Some(height) = styled_dom
        .css_property_cache
        .ptr
        .get_height(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
    {
        return height;
    }
    
    // 2. Check User Agent CSS
    let node_type = styled_dom.node_data.as_container()[node_id].node_type.clone();
    if let Some(ua_prop) = azul_core::ua_css::get_ua_property(node_type, azul_css::props::property::CssPropertyType::Height) {
        if let azul_css::props::property::CssProperty::Height(azul_css::css::CssPropertyValue::Exact(h)) = ua_prop {
            return *h;
        }
    }
    
    // 3. Fallback to type default
    LayoutHeight::default()  // Returns Auto, which is semantically correct
}
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

/// Get border radius for all four corners (raw CSS property values)
pub fn get_style_border_radius(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> azul_css::props::style::border_radius::StyleBorderRadius {
    use azul_css::props::{basic::PixelValue, style::border_radius::StyleBorderRadius};

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
    pub widths: crate::solver3::display_list::StyleBorderWidths,
    pub colors: crate::solver3::display_list::StyleBorderColors,
    pub styles: crate::solver3::display_list::StyleBorderStyles,
}

pub fn get_border_info<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> BorderInfo {
    use crate::solver3::display_list::{StyleBorderColors, StyleBorderStyles, StyleBorderWidths};

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

pub fn get_display_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutDisplay {
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

pub fn get_style_properties(styled_dom: &StyledDom, dom_id: NodeId) -> StyleProperties {
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
        font_selector: crate::text3::cache::FontSelector {
            family: font_family_name,
            weight: rust_fontconfig::FcWeight::Normal, // STUB for now
            style: crate::text3::cache::FontStyle::Normal, // STUB for now
            unicode_ranges: Vec::new(),
        },
        font_size_px: font_size,
        color,
        line_height,
        ..Default::default()
    }
}

pub fn get_list_style_type(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
) -> StyleListStyleType {
    let Some(id) = dom_id else {
        return StyleListStyleType::default();
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].state;
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
    let node_state = &styled_dom.styled_nodes.as_container()[id].state;
    styled_dom
        .css_property_cache
        .ptr
        .get_list_style_position(node_data, &id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

// ============================================================================
// NEW: Taffy Bridge Getters - Box Model Properties with UA CSS Fallback
// ============================================================================

use azul_css::props::{
    basic::pixel::PixelValue,
    layout::{
        LayoutBottom, LayoutLeft, LayoutMarginBottom, LayoutMarginLeft, LayoutMarginRight,
        LayoutMarginTop, LayoutMaxHeight, LayoutMaxWidth, LayoutMinHeight, LayoutMinWidth,
        LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop,
        LayoutRight, LayoutTop,
    },
};

/// Get inset (position) properties with UA CSS fallback
pub fn get_css_left(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutLeft {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    
    // 1. Check author CSS first
    if let Some(val) = styled_dom
        .css_property_cache
        .ptr
        .get_left(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
    {
        return val;
    }
    
    // 2. Check User Agent CSS
    let node_type = node_data.node_type.clone();
    if let Some(ua_prop) = azul_core::ua_css::get_ua_property(node_type, azul_css::props::property::CssPropertyType::Left) {
        if let azul_css::props::property::CssProperty::Left(azul_css::css::CssPropertyValue::Exact(v)) = ua_prop {
            return *v;
        }
    }
    
    // 3. Fallback to type default
    LayoutLeft::default()
}

pub fn get_css_right(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutRight {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    if let Some(val) = styled_dom
        .css_property_cache
        .ptr
        .get_right(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
    {
        return val;
    }
    LayoutRight::default()
}

pub fn get_css_top(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutTop {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    if let Some(val) = styled_dom
        .css_property_cache
        .ptr
        .get_top(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
    {
        return val;
    }
    LayoutTop::default()
}

pub fn get_css_bottom(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutBottom {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    if let Some(val) = styled_dom
        .css_property_cache
        .ptr
        .get_bottom(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
    {
        return val;
    }
    LayoutBottom::default()
}

/// Get min/max size properties
pub fn get_css_min_width(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutMinWidth {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_min_width(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

pub fn get_css_min_height(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutMinHeight {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_min_height(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

pub fn get_css_max_width(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutMaxWidth {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_max_width(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

pub fn get_css_max_height(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutMaxHeight {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_max_height(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

/// Get margin properties with UA CSS fallback
pub fn get_css_margin_left(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutMarginLeft {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    
    if let Some(val) = styled_dom
        .css_property_cache
        .ptr
        .get_margin_left(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
    {
        return val;
    }
    
    // Check UA CSS
    let node_type = node_data.node_type.clone();
    if let Some(ua_prop) = azul_core::ua_css::get_ua_property(node_type, azul_css::props::property::CssPropertyType::MarginLeft) {
        if let azul_css::props::property::CssProperty::MarginLeft(azul_css::css::CssPropertyValue::Exact(v)) = ua_prop {
            return *v;
        }
    }
    
    LayoutMarginLeft::default()
}

pub fn get_css_margin_right(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutMarginRight {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    
    if let Some(val) = styled_dom
        .css_property_cache
        .ptr
        .get_margin_right(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
    {
        return val;
    }
    
    let node_type = node_data.node_type.clone();
    if let Some(ua_prop) = azul_core::ua_css::get_ua_property(node_type, azul_css::props::property::CssPropertyType::MarginRight) {
        if let azul_css::props::property::CssProperty::MarginRight(azul_css::css::CssPropertyValue::Exact(v)) = ua_prop {
            return *v;
        }
    }
    
    LayoutMarginRight::default()
}

pub fn get_css_margin_top(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutMarginTop {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    
    if let Some(val) = styled_dom
        .css_property_cache
        .ptr
        .get_margin_top(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
    {
        return val;
    }
    
    let node_type = node_data.node_type.clone();
    if let Some(ua_prop) = azul_core::ua_css::get_ua_property(node_type, azul_css::props::property::CssPropertyType::MarginTop) {
        if let azul_css::props::property::CssProperty::MarginTop(azul_css::css::CssPropertyValue::Exact(v)) = ua_prop {
            return *v;
        }
    }
    
    LayoutMarginTop::default()
}

pub fn get_css_margin_bottom(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutMarginBottom {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    
    if let Some(val) = styled_dom
        .css_property_cache
        .ptr
        .get_margin_bottom(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
    {
        return val;
    }
    
    let node_type = node_data.node_type.clone();
    if let Some(ua_prop) = azul_core::ua_css::get_ua_property(node_type, azul_css::props::property::CssPropertyType::MarginBottom) {
        if let azul_css::props::property::CssProperty::MarginBottom(azul_css::css::CssPropertyValue::Exact(v)) = ua_prop {
            return *v;
        }
    }
    
    LayoutMarginBottom::default()
}

/// Get padding properties (no UA CSS fallback needed, defaults to 0)
pub fn get_css_padding_left(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutPaddingLeft {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_padding_left(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

pub fn get_css_padding_right(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutPaddingRight {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_padding_right(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

pub fn get_css_padding_top(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutPaddingTop {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_padding_top(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

pub fn get_css_padding_bottom(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> LayoutPaddingBottom {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_padding_bottom(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

/// Get border width properties (no UA CSS fallback needed, defaults to 0)
pub fn get_css_border_left_width(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> PixelValue {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_border_left_width(node_data, &node_id, node_state)
        .and_then(|v| v.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default()
}

pub fn get_css_border_right_width(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> PixelValue {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_border_right_width(node_data, &node_id, node_state)
        .and_then(|v| v.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default()
}

pub fn get_css_border_top_width(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> PixelValue {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_border_top_width(node_data, &node_id, node_state)
        .and_then(|v| v.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default()
}

pub fn get_css_border_bottom_width(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> PixelValue {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_border_bottom_width(node_data, &node_id, node_state)
        .and_then(|v| v.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default()
}
