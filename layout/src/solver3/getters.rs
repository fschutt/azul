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
    pub fn is_clipped(&self) -> bool {
        matches!(self, MultiValue::Exact(LayoutOverflow::Hidden | LayoutOverflow::Clip))
    }
    
    pub fn is_scroll(&self) -> bool {
        matches!(self, MultiValue::Exact(LayoutOverflow::Scroll | LayoutOverflow::Auto))
    }
    
    pub fn is_auto_overflow(&self) -> bool {
        matches!(self, MultiValue::Exact(LayoutOverflow::Auto))
    }
    
    pub fn is_hidden(&self) -> bool {
        matches!(self, MultiValue::Exact(LayoutOverflow::Hidden))
    }
    
    pub fn is_hidden_or_clip(&self) -> bool {
        matches!(self, MultiValue::Exact(LayoutOverflow::Hidden | LayoutOverflow::Clip))
    }
    
    pub fn is_scroll_explicit(&self) -> bool {
        matches!(self, MultiValue::Exact(LayoutOverflow::Scroll))
    }
    
    pub fn is_visible_or_clip(&self) -> bool {
        matches!(self, MultiValue::Exact(LayoutOverflow::Visible | LayoutOverflow::Clip))
    }
}

// Implement helper methods for LayoutPosition
impl MultiValue<LayoutPosition> {
    pub fn is_absolute_or_fixed(&self) -> bool {
        matches!(self, MultiValue::Exact(LayoutPosition::Absolute | LayoutPosition::Fixed))
    }
}

// Implement helper methods for LayoutFloat
impl MultiValue<LayoutFloat> {
    pub fn is_none(&self) -> bool {
        matches!(self, MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit | MultiValue::Exact(LayoutFloat::None))
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
    ($fn_name:ident, $cache_method:ident, $ua_property:expr) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<PixelValue> {
            let node_data = &styled_dom.node_data.as_container()[node_id];
            
            // 1. Check author CSS first
            if let Some(val) = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state)
                .and_then(|v| v.get_property().copied())
            {
                return MultiValue::Exact(val.inner);
            }
            
            // 2. Check User Agent CSS
            let node_type = node_data.node_type.clone();
            if let Some(ua_prop) = azul_core::ua_css::get_ua_property(node_type, $ua_property) {
                if let Some(inner) = ua_prop.get_pixel_inner() {
                    return MultiValue::Exact(inner);
                }
            }
            
            // 3. Fallback to Auto (not set)
            MultiValue::Auto
        }
    };
}

/// Helper trait to extract PixelValue from any CssProperty variant
trait CssPropertyPixelInner {
    fn get_pixel_inner(&self) -> Option<PixelValue>;
}

impl CssPropertyPixelInner for azul_css::props::property::CssProperty {
    fn get_pixel_inner(&self) -> Option<PixelValue> {
        use azul_css::props::property::CssProperty;
        use azul_css::css::CssPropertyValue;
        
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
    ($fn_name:ident, $cache_method:ident, $return_type:ty, $ua_property:expr) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<$return_type> {
            let node_data = &styled_dom.node_data.as_container()[node_id];
            
            // 1. Check author CSS first
            if let Some(val) = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state)
                .and_then(|v| v.get_property().copied())
            {
                return MultiValue::Exact(val);
            }
            
            // 2. Check User Agent CSS
            let node_type = node_data.node_type.clone();
            if let Some(ua_prop) = azul_core::ua_css::get_ua_property(node_type, $ua_property) {
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
use azul_css::css::CssPropertyValue;

impl ExtractPropertyValue<LayoutWidth> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutWidth> {
        match self {
            Self::Width(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutHeight> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<LayoutHeight> {
        match self {
            Self::Height(CssPropertyValue::Exact(v)) => Some(*v),
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

impl ExtractPropertyValue<PixelValue> for azul_css::props::property::CssProperty {
    fn extract(&self) -> Option<PixelValue> {
        self.get_pixel_inner()
    }
}

get_css_property!(
    get_writing_mode,
    get_writing_mode,
    LayoutWritingMode,
    azul_css::props::property::CssPropertyType::WritingMode
);

get_css_property!(
    get_css_width,
    get_width,
    LayoutWidth,
    azul_css::props::property::CssPropertyType::Width
);

get_css_property!(
    get_css_height,
    get_height,
    LayoutHeight,
    azul_css::props::property::CssPropertyType::Height
);

get_css_property!(
    get_wrap,
    get_flex_wrap,
    LayoutFlexWrap,
    azul_css::props::property::CssPropertyType::FlexWrap
);

get_css_property!(
    get_justify_content,
    get_justify_content,
    LayoutJustifyContent,
    azul_css::props::property::CssPropertyType::JustifyContent
);

get_css_property!(
    get_text_align,
    get_text_align,
    StyleTextAlign,
    azul_css::props::property::CssPropertyType::TextAlign
);

get_css_property!(
    get_float,
    get_float,
    LayoutFloat,
    azul_css::props::property::CssPropertyType::Float
);

get_css_property!(
    get_overflow_x,
    get_overflow_x,
    LayoutOverflow,
    azul_css::props::property::CssPropertyType::OverflowX
);

get_css_property!(
    get_overflow_y,
    get_overflow_y,
    LayoutOverflow,
    azul_css::props::property::CssPropertyType::OverflowY
);

get_css_property!(
    get_position,
    get_position,
    LayoutPosition,
    azul_css::props::property::CssPropertyType::Position
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

get_css_property!(
    get_display_property_internal,
    get_display,
    LayoutDisplay,
    azul_css::props::property::CssPropertyType::Display
);

pub fn get_display_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> MultiValue<LayoutDisplay> {
    let Some(id) = dom_id else {
        return MultiValue::Exact(LayoutDisplay::Inline);
    };
    let node_state = &styled_dom.styled_nodes.as_container()[id].state;
    get_display_property_internal(styled_dom, id, node_state)
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

/// Get inset (position) properties - returns MultiValue<PixelValue>
get_css_property_pixel!(get_css_left, get_left, azul_css::props::property::CssPropertyType::Left);
get_css_property_pixel!(get_css_right, get_right, azul_css::props::property::CssPropertyType::Right);
get_css_property_pixel!(get_css_top, get_top, azul_css::props::property::CssPropertyType::Top);
get_css_property_pixel!(get_css_bottom, get_bottom, azul_css::props::property::CssPropertyType::Bottom);

/// Get margin properties - returns MultiValue<PixelValue>
get_css_property_pixel!(get_css_margin_left, get_margin_left, azul_css::props::property::CssPropertyType::MarginLeft);
get_css_property_pixel!(get_css_margin_right, get_margin_right, azul_css::props::property::CssPropertyType::MarginRight);
get_css_property_pixel!(get_css_margin_top, get_margin_top, azul_css::props::property::CssPropertyType::MarginTop);
get_css_property_pixel!(get_css_margin_bottom, get_margin_bottom, azul_css::props::property::CssPropertyType::MarginBottom);

/// Get padding properties - returns MultiValue<PixelValue>
get_css_property_pixel!(get_css_padding_left, get_padding_left, azul_css::props::property::CssPropertyType::PaddingLeft);
get_css_property_pixel!(get_css_padding_right, get_padding_right, azul_css::props::property::CssPropertyType::PaddingRight);
get_css_property_pixel!(get_css_padding_top, get_padding_top, azul_css::props::property::CssPropertyType::PaddingTop);
get_css_property_pixel!(get_css_padding_bottom, get_padding_bottom, azul_css::props::property::CssPropertyType::PaddingBottom);

/// Get min/max size properties
get_css_property!(
    get_css_min_width,
    get_min_width,
    LayoutMinWidth,
    azul_css::props::property::CssPropertyType::MinWidth
);

get_css_property!(
    get_css_min_height,
    get_min_height,
    LayoutMinHeight,
    azul_css::props::property::CssPropertyType::MinHeight
);

get_css_property!(
    get_css_max_width,
    get_max_width,
    LayoutMaxWidth,
    azul_css::props::property::CssPropertyType::MaxWidth
);

get_css_property!(
    get_css_max_height,
    get_max_height,
    LayoutMaxHeight,
    azul_css::props::property::CssPropertyType::MaxHeight
);

/// Get border width properties (no UA CSS fallback needed, defaults to 0)
get_css_property_pixel!(get_css_border_left_width, get_border_left_width, azul_css::props::property::CssPropertyType::BorderLeftWidth);
get_css_property_pixel!(get_css_border_right_width, get_border_right_width, azul_css::props::property::CssPropertyType::BorderRightWidth);
get_css_property_pixel!(get_css_border_top_width, get_border_top_width, azul_css::props::property::CssPropertyType::BorderTopWidth);
get_css_property_pixel!(get_css_border_bottom_width, get_border_bottom_width, azul_css::props::property::CssPropertyType::BorderBottomWidth);
