    #![allow(dead_code, unused_imports)]
    //! `Css` parsing module
    use crate::dll::*;
    use std::ffi::c_void;
    use crate::str::String;


    /// `CssRuleBlock` struct
    pub use crate::dll::AzCssRuleBlock as CssRuleBlock;

    impl std::fmt::Debug for CssRuleBlock { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_rule_block_fmt_debug)(self)) } }
    impl Clone for CssRuleBlock { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_deep_copy)(self) } }
    impl Drop for CssRuleBlock { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_rule_block_delete)(self); } }


    /// `CssDeclaration` struct
    pub use crate::dll::AzCssDeclaration as CssDeclaration;

    impl std::fmt::Debug for CssDeclaration { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_declaration_fmt_debug)(self)) } }
    impl Clone for CssDeclaration { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_declaration_deep_copy)(self) } }
    impl Drop for CssDeclaration { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_declaration_delete)(self); } }


    /// `DynamicCssProperty` struct
    pub use crate::dll::AzDynamicCssProperty as DynamicCssProperty;

    impl std::fmt::Debug for DynamicCssProperty { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_dynamic_css_property_fmt_debug)(self)) } }
    impl Clone for DynamicCssProperty { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_dynamic_css_property_deep_copy)(self) } }
    impl Drop for DynamicCssProperty { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dynamic_css_property_delete)(self); } }


    /// `CssPath` struct
    pub use crate::dll::AzCssPath as CssPath;

    impl std::fmt::Debug for CssPath { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_path_fmt_debug)(self)) } }
    impl Clone for CssPath { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_deep_copy)(self) } }
    impl Drop for CssPath { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_delete)(self); } }


    /// `CssPathSelector` struct
    pub use crate::dll::AzCssPathSelector as CssPathSelector;

    impl std::fmt::Debug for CssPathSelector { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_path_selector_fmt_debug)(self)) } }
    impl Clone for CssPathSelector { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_deep_copy)(self) } }
    impl Drop for CssPathSelector { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_selector_delete)(self); } }


    /// `NodeTypePath` struct
    pub use crate::dll::AzNodeTypePath as NodeTypePath;

    impl std::fmt::Debug for NodeTypePath { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_node_type_path_fmt_debug)(self)) } }
    impl Clone for NodeTypePath { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_node_type_path_deep_copy)(self) } }
    impl Drop for NodeTypePath { fn drop(&mut self) { (crate::dll::get_azul_dll().az_node_type_path_delete)(self); } }


    /// `CssPathPseudoSelector` struct
    pub use crate::dll::AzCssPathPseudoSelector as CssPathPseudoSelector;

    impl std::fmt::Debug for CssPathPseudoSelector { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_path_pseudo_selector_fmt_debug)(self)) } }
    impl Clone for CssPathPseudoSelector { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_pseudo_selector_deep_copy)(self) } }
    impl Drop for CssPathPseudoSelector { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_pseudo_selector_delete)(self); } }


    /// `CssNthChildSelector` struct
    pub use crate::dll::AzCssNthChildSelector as CssNthChildSelector;

    impl std::fmt::Debug for CssNthChildSelector { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_nth_child_selector_fmt_debug)(self)) } }
    impl Clone for CssNthChildSelector { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_nth_child_selector_deep_copy)(self) } }
    impl Drop for CssNthChildSelector { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_nth_child_selector_delete)(self); } }


    /// `CssNthChildPattern` struct
    pub use crate::dll::AzCssNthChildPattern as CssNthChildPattern;

    impl std::fmt::Debug for CssNthChildPattern { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_nth_child_pattern_fmt_debug)(self)) } }
    impl Clone for CssNthChildPattern { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_nth_child_pattern_deep_copy)(self) } }
    impl Drop for CssNthChildPattern { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_nth_child_pattern_delete)(self); } }


    /// `Stylesheet` struct
    pub use crate::dll::AzStylesheet as Stylesheet;

    impl std::fmt::Debug for Stylesheet { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_stylesheet_fmt_debug)(self)) } }
    impl Clone for Stylesheet { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_stylesheet_deep_copy)(self) } }
    impl Drop for Stylesheet { fn drop(&mut self) { (crate::dll::get_azul_dll().az_stylesheet_delete)(self); } }


    /// `Css` struct
    pub use crate::dll::AzCss as Css;

    impl Css {
        /// Loads the native style for the given operating system
        pub fn native() -> Self { (crate::dll::get_azul_dll().az_css_native)() }
        /// Returns an empty CSS style
        pub fn empty() -> Self { (crate::dll::get_azul_dll().az_css_empty)() }
        /// Returns a CSS style parsed from a `String`
        pub fn from_string(s: String) -> Self { (crate::dll::get_azul_dll().az_css_from_string)(s) }
        /// Appends a parsed stylesheet to `Css::native()`
        pub fn override_native(s: String) -> Self { (crate::dll::get_azul_dll().az_css_override_native)(s) }
    }

    impl std::fmt::Debug for Css { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_fmt_debug)(self)) } }
    impl Clone for Css { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_deep_copy)(self) } }
    impl Drop for Css { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_delete)(self); } }


    /// `ColorU` struct
    pub use crate::dll::AzColorU as ColorU;

    impl std::fmt::Debug for ColorU { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_color_u_fmt_debug)(self)) } }
    impl Clone for ColorU { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_color_u_deep_copy)(self) } }
    impl Drop for ColorU { fn drop(&mut self) { (crate::dll::get_azul_dll().az_color_u_delete)(self); } }


    /// `SizeMetric` struct
    pub use crate::dll::AzSizeMetric as SizeMetric;

    impl std::fmt::Debug for SizeMetric { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_size_metric_fmt_debug)(self)) } }
    impl Clone for SizeMetric { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_size_metric_deep_copy)(self) } }
    impl Drop for SizeMetric { fn drop(&mut self) { (crate::dll::get_azul_dll().az_size_metric_delete)(self); } }


    /// `FloatValue` struct
    pub use crate::dll::AzFloatValue as FloatValue;

    impl std::fmt::Debug for FloatValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_float_value_fmt_debug)(self)) } }
    impl Clone for FloatValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_float_value_deep_copy)(self) } }
    impl Drop for FloatValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_float_value_delete)(self); } }


    /// `PixelValue` struct
    pub use crate::dll::AzPixelValue as PixelValue;

    impl std::fmt::Debug for PixelValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_pixel_value_fmt_debug)(self)) } }
    impl Clone for PixelValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_pixel_value_deep_copy)(self) } }
    impl Drop for PixelValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_pixel_value_delete)(self); } }


    /// `PixelValueNoPercent` struct
    pub use crate::dll::AzPixelValueNoPercent as PixelValueNoPercent;

    impl std::fmt::Debug for PixelValueNoPercent { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_pixel_value_no_percent_fmt_debug)(self)) } }
    impl Clone for PixelValueNoPercent { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_pixel_value_no_percent_deep_copy)(self) } }
    impl Drop for PixelValueNoPercent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_pixel_value_no_percent_delete)(self); } }


    /// `BoxShadowClipMode` struct
    pub use crate::dll::AzBoxShadowClipMode as BoxShadowClipMode;

    impl std::fmt::Debug for BoxShadowClipMode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_box_shadow_clip_mode_fmt_debug)(self)) } }
    impl Clone for BoxShadowClipMode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_box_shadow_clip_mode_deep_copy)(self) } }
    impl Drop for BoxShadowClipMode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_box_shadow_clip_mode_delete)(self); } }


    /// `BoxShadowPreDisplayItem` struct
    pub use crate::dll::AzBoxShadowPreDisplayItem as BoxShadowPreDisplayItem;

    impl std::fmt::Debug for BoxShadowPreDisplayItem { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_fmt_debug)(self)) } }
    impl Clone for BoxShadowPreDisplayItem { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_deep_copy)(self) } }
    impl Drop for BoxShadowPreDisplayItem { fn drop(&mut self) { (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_delete)(self); } }


    /// `LayoutAlignContent` struct
    pub use crate::dll::AzLayoutAlignContent as LayoutAlignContent;

    impl std::fmt::Debug for LayoutAlignContent { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_align_content_fmt_debug)(self)) } }
    impl Clone for LayoutAlignContent { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_align_content_deep_copy)(self) } }
    impl Drop for LayoutAlignContent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_content_delete)(self); } }


    /// `LayoutAlignItems` struct
    pub use crate::dll::AzLayoutAlignItems as LayoutAlignItems;

    impl std::fmt::Debug for LayoutAlignItems { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_align_items_fmt_debug)(self)) } }
    impl Clone for LayoutAlignItems { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_align_items_deep_copy)(self) } }
    impl Drop for LayoutAlignItems { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_items_delete)(self); } }


    /// `LayoutBottom` struct
    pub use crate::dll::AzLayoutBottom as LayoutBottom;

    impl std::fmt::Debug for LayoutBottom { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_bottom_fmt_debug)(self)) } }
    impl Clone for LayoutBottom { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_bottom_deep_copy)(self) } }
    impl Drop for LayoutBottom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_bottom_delete)(self); } }


    /// `LayoutBoxSizing` struct
    pub use crate::dll::AzLayoutBoxSizing as LayoutBoxSizing;

    impl std::fmt::Debug for LayoutBoxSizing { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_box_sizing_fmt_debug)(self)) } }
    impl Clone for LayoutBoxSizing { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_box_sizing_deep_copy)(self) } }
    impl Drop for LayoutBoxSizing { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_box_sizing_delete)(self); } }


    /// `LayoutDirection` struct
    pub use crate::dll::AzLayoutDirection as LayoutDirection;

    impl std::fmt::Debug for LayoutDirection { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_direction_fmt_debug)(self)) } }
    impl Clone for LayoutDirection { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_direction_deep_copy)(self) } }
    impl Drop for LayoutDirection { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_direction_delete)(self); } }


    /// `LayoutDisplay` struct
    pub use crate::dll::AzLayoutDisplay as LayoutDisplay;

    impl std::fmt::Debug for LayoutDisplay { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_display_fmt_debug)(self)) } }
    impl Clone for LayoutDisplay { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_display_deep_copy)(self) } }
    impl Drop for LayoutDisplay { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_display_delete)(self); } }


    /// `LayoutFlexGrow` struct
    pub use crate::dll::AzLayoutFlexGrow as LayoutFlexGrow;

    impl std::fmt::Debug for LayoutFlexGrow { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_flex_grow_fmt_debug)(self)) } }
    impl Clone for LayoutFlexGrow { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_flex_grow_deep_copy)(self) } }
    impl Drop for LayoutFlexGrow { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_grow_delete)(self); } }


    /// `LayoutFlexShrink` struct
    pub use crate::dll::AzLayoutFlexShrink as LayoutFlexShrink;

    impl std::fmt::Debug for LayoutFlexShrink { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_flex_shrink_fmt_debug)(self)) } }
    impl Clone for LayoutFlexShrink { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_flex_shrink_deep_copy)(self) } }
    impl Drop for LayoutFlexShrink { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_shrink_delete)(self); } }


    /// `LayoutFloat` struct
    pub use crate::dll::AzLayoutFloat as LayoutFloat;

    impl std::fmt::Debug for LayoutFloat { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_float_fmt_debug)(self)) } }
    impl Clone for LayoutFloat { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_float_deep_copy)(self) } }
    impl Drop for LayoutFloat { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_float_delete)(self); } }


    /// `LayoutHeight` struct
    pub use crate::dll::AzLayoutHeight as LayoutHeight;

    impl std::fmt::Debug for LayoutHeight { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_height_fmt_debug)(self)) } }
    impl Clone for LayoutHeight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_height_deep_copy)(self) } }
    impl Drop for LayoutHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_height_delete)(self); } }


    /// `LayoutJustifyContent` struct
    pub use crate::dll::AzLayoutJustifyContent as LayoutJustifyContent;

    impl std::fmt::Debug for LayoutJustifyContent { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_justify_content_fmt_debug)(self)) } }
    impl Clone for LayoutJustifyContent { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_justify_content_deep_copy)(self) } }
    impl Drop for LayoutJustifyContent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_justify_content_delete)(self); } }


    /// `LayoutLeft` struct
    pub use crate::dll::AzLayoutLeft as LayoutLeft;

    impl std::fmt::Debug for LayoutLeft { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_left_fmt_debug)(self)) } }
    impl Clone for LayoutLeft { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_left_deep_copy)(self) } }
    impl Drop for LayoutLeft { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_left_delete)(self); } }


    /// `LayoutMarginBottom` struct
    pub use crate::dll::AzLayoutMarginBottom as LayoutMarginBottom;

    impl std::fmt::Debug for LayoutMarginBottom { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_margin_bottom_fmt_debug)(self)) } }
    impl Clone for LayoutMarginBottom { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_bottom_deep_copy)(self) } }
    impl Drop for LayoutMarginBottom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_bottom_delete)(self); } }


    /// `LayoutMarginLeft` struct
    pub use crate::dll::AzLayoutMarginLeft as LayoutMarginLeft;

    impl std::fmt::Debug for LayoutMarginLeft { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_margin_left_fmt_debug)(self)) } }
    impl Clone for LayoutMarginLeft { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_left_deep_copy)(self) } }
    impl Drop for LayoutMarginLeft { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_left_delete)(self); } }


    /// `LayoutMarginRight` struct
    pub use crate::dll::AzLayoutMarginRight as LayoutMarginRight;

    impl std::fmt::Debug for LayoutMarginRight { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_margin_right_fmt_debug)(self)) } }
    impl Clone for LayoutMarginRight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_right_deep_copy)(self) } }
    impl Drop for LayoutMarginRight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_right_delete)(self); } }


    /// `LayoutMarginTop` struct
    pub use crate::dll::AzLayoutMarginTop as LayoutMarginTop;

    impl std::fmt::Debug for LayoutMarginTop { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_margin_top_fmt_debug)(self)) } }
    impl Clone for LayoutMarginTop { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_top_deep_copy)(self) } }
    impl Drop for LayoutMarginTop { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_top_delete)(self); } }


    /// `LayoutMaxHeight` struct
    pub use crate::dll::AzLayoutMaxHeight as LayoutMaxHeight;

    impl std::fmt::Debug for LayoutMaxHeight { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_max_height_fmt_debug)(self)) } }
    impl Clone for LayoutMaxHeight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_max_height_deep_copy)(self) } }
    impl Drop for LayoutMaxHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_height_delete)(self); } }


    /// `LayoutMaxWidth` struct
    pub use crate::dll::AzLayoutMaxWidth as LayoutMaxWidth;

    impl std::fmt::Debug for LayoutMaxWidth { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_max_width_fmt_debug)(self)) } }
    impl Clone for LayoutMaxWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_max_width_deep_copy)(self) } }
    impl Drop for LayoutMaxWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_width_delete)(self); } }


    /// `LayoutMinHeight` struct
    pub use crate::dll::AzLayoutMinHeight as LayoutMinHeight;

    impl std::fmt::Debug for LayoutMinHeight { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_min_height_fmt_debug)(self)) } }
    impl Clone for LayoutMinHeight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_min_height_deep_copy)(self) } }
    impl Drop for LayoutMinHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_height_delete)(self); } }


    /// `LayoutMinWidth` struct
    pub use crate::dll::AzLayoutMinWidth as LayoutMinWidth;

    impl std::fmt::Debug for LayoutMinWidth { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_min_width_fmt_debug)(self)) } }
    impl Clone for LayoutMinWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_min_width_deep_copy)(self) } }
    impl Drop for LayoutMinWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_width_delete)(self); } }


    /// `LayoutPaddingBottom` struct
    pub use crate::dll::AzLayoutPaddingBottom as LayoutPaddingBottom;

    impl std::fmt::Debug for LayoutPaddingBottom { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_padding_bottom_fmt_debug)(self)) } }
    impl Clone for LayoutPaddingBottom { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_bottom_deep_copy)(self) } }
    impl Drop for LayoutPaddingBottom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_bottom_delete)(self); } }


    /// `LayoutPaddingLeft` struct
    pub use crate::dll::AzLayoutPaddingLeft as LayoutPaddingLeft;

    impl std::fmt::Debug for LayoutPaddingLeft { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_padding_left_fmt_debug)(self)) } }
    impl Clone for LayoutPaddingLeft { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_left_deep_copy)(self) } }
    impl Drop for LayoutPaddingLeft { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_left_delete)(self); } }


    /// `LayoutPaddingRight` struct
    pub use crate::dll::AzLayoutPaddingRight as LayoutPaddingRight;

    impl std::fmt::Debug for LayoutPaddingRight { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_padding_right_fmt_debug)(self)) } }
    impl Clone for LayoutPaddingRight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_right_deep_copy)(self) } }
    impl Drop for LayoutPaddingRight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_right_delete)(self); } }


    /// `LayoutPaddingTop` struct
    pub use crate::dll::AzLayoutPaddingTop as LayoutPaddingTop;

    impl std::fmt::Debug for LayoutPaddingTop { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_padding_top_fmt_debug)(self)) } }
    impl Clone for LayoutPaddingTop { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_top_deep_copy)(self) } }
    impl Drop for LayoutPaddingTop { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_top_delete)(self); } }


    /// `LayoutPosition` struct
    pub use crate::dll::AzLayoutPosition as LayoutPosition;

    impl std::fmt::Debug for LayoutPosition { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_position_fmt_debug)(self)) } }
    impl Clone for LayoutPosition { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_position_deep_copy)(self) } }
    impl Drop for LayoutPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_position_delete)(self); } }


    /// `LayoutRight` struct
    pub use crate::dll::AzLayoutRight as LayoutRight;

    impl std::fmt::Debug for LayoutRight { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_right_fmt_debug)(self)) } }
    impl Clone for LayoutRight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_right_deep_copy)(self) } }
    impl Drop for LayoutRight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_right_delete)(self); } }


    /// `LayoutTop` struct
    pub use crate::dll::AzLayoutTop as LayoutTop;

    impl std::fmt::Debug for LayoutTop { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_top_fmt_debug)(self)) } }
    impl Clone for LayoutTop { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_top_deep_copy)(self) } }
    impl Drop for LayoutTop { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_top_delete)(self); } }


    /// `LayoutWidth` struct
    pub use crate::dll::AzLayoutWidth as LayoutWidth;

    impl std::fmt::Debug for LayoutWidth { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_width_fmt_debug)(self)) } }
    impl Clone for LayoutWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_width_deep_copy)(self) } }
    impl Drop for LayoutWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_width_delete)(self); } }


    /// `LayoutWrap` struct
    pub use crate::dll::AzLayoutWrap as LayoutWrap;

    impl std::fmt::Debug for LayoutWrap { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_wrap_fmt_debug)(self)) } }
    impl Clone for LayoutWrap { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_wrap_deep_copy)(self) } }
    impl Drop for LayoutWrap { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_wrap_delete)(self); } }


    /// `Overflow` struct
    pub use crate::dll::AzOverflow as Overflow;

    impl std::fmt::Debug for Overflow { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_overflow_fmt_debug)(self)) } }
    impl Clone for Overflow { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_overflow_deep_copy)(self) } }
    impl Drop for Overflow { fn drop(&mut self) { (crate::dll::get_azul_dll().az_overflow_delete)(self); } }


    /// `PercentageValue` struct
    pub use crate::dll::AzPercentageValue as PercentageValue;

    impl std::fmt::Debug for PercentageValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_percentage_value_fmt_debug)(self)) } }
    impl Clone for PercentageValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_percentage_value_deep_copy)(self) } }
    impl Drop for PercentageValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_percentage_value_delete)(self); } }


    /// `GradientStopPre` struct
    pub use crate::dll::AzGradientStopPre as GradientStopPre;

    impl std::fmt::Debug for GradientStopPre { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_gradient_stop_pre_fmt_debug)(self)) } }
    impl Clone for GradientStopPre { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_deep_copy)(self) } }
    impl Drop for GradientStopPre { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gradient_stop_pre_delete)(self); } }


    /// `DirectionCorner` struct
    pub use crate::dll::AzDirectionCorner as DirectionCorner;

    impl std::fmt::Debug for DirectionCorner { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_direction_corner_fmt_debug)(self)) } }
    impl Clone for DirectionCorner { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_direction_corner_deep_copy)(self) } }
    impl Drop for DirectionCorner { fn drop(&mut self) { (crate::dll::get_azul_dll().az_direction_corner_delete)(self); } }


    /// `DirectionCorners` struct
    pub use crate::dll::AzDirectionCorners as DirectionCorners;

    impl std::fmt::Debug for DirectionCorners { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_direction_corners_fmt_debug)(self)) } }
    impl Clone for DirectionCorners { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_direction_corners_deep_copy)(self) } }
    impl Drop for DirectionCorners { fn drop(&mut self) { (crate::dll::get_azul_dll().az_direction_corners_delete)(self); } }


    /// `Direction` struct
    pub use crate::dll::AzDirection as Direction;

    impl std::fmt::Debug for Direction { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_direction_fmt_debug)(self)) } }
    impl Clone for Direction { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_direction_deep_copy)(self) } }
    impl Drop for Direction { fn drop(&mut self) { (crate::dll::get_azul_dll().az_direction_delete)(self); } }


    /// `ExtendMode` struct
    pub use crate::dll::AzExtendMode as ExtendMode;

    impl std::fmt::Debug for ExtendMode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_extend_mode_fmt_debug)(self)) } }
    impl Clone for ExtendMode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_extend_mode_deep_copy)(self) } }
    impl Drop for ExtendMode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_extend_mode_delete)(self); } }


    /// `LinearGradient` struct
    pub use crate::dll::AzLinearGradient as LinearGradient;

    impl std::fmt::Debug for LinearGradient { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_linear_gradient_fmt_debug)(self)) } }
    impl Clone for LinearGradient { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_linear_gradient_deep_copy)(self) } }
    impl Drop for LinearGradient { fn drop(&mut self) { (crate::dll::get_azul_dll().az_linear_gradient_delete)(self); } }


    /// `Shape` struct
    pub use crate::dll::AzShape as Shape;

    impl std::fmt::Debug for Shape { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_shape_fmt_debug)(self)) } }
    impl Clone for Shape { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_shape_deep_copy)(self) } }
    impl Drop for Shape { fn drop(&mut self) { (crate::dll::get_azul_dll().az_shape_delete)(self); } }


    /// `RadialGradient` struct
    pub use crate::dll::AzRadialGradient as RadialGradient;

    impl std::fmt::Debug for RadialGradient { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_radial_gradient_fmt_debug)(self)) } }
    impl Clone for RadialGradient { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_radial_gradient_deep_copy)(self) } }
    impl Drop for RadialGradient { fn drop(&mut self) { (crate::dll::get_azul_dll().az_radial_gradient_delete)(self); } }


    /// `CssImageId` struct
    pub use crate::dll::AzCssImageId as CssImageId;

    impl std::fmt::Debug for CssImageId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_image_id_fmt_debug)(self)) } }
    impl Clone for CssImageId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_image_id_deep_copy)(self) } }
    impl Drop for CssImageId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_image_id_delete)(self); } }


    /// `StyleBackgroundContent` struct
    pub use crate::dll::AzStyleBackgroundContent as StyleBackgroundContent;

    impl std::fmt::Debug for StyleBackgroundContent { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_background_content_fmt_debug)(self)) } }
    impl Clone for StyleBackgroundContent { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_content_deep_copy)(self) } }
    impl Drop for StyleBackgroundContent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_content_delete)(self); } }


    /// `BackgroundPositionHorizontal` struct
    pub use crate::dll::AzBackgroundPositionHorizontal as BackgroundPositionHorizontal;

    impl std::fmt::Debug for BackgroundPositionHorizontal { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_background_position_horizontal_fmt_debug)(self)) } }
    impl Clone for BackgroundPositionHorizontal { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_background_position_horizontal_deep_copy)(self) } }
    impl Drop for BackgroundPositionHorizontal { fn drop(&mut self) { (crate::dll::get_azul_dll().az_background_position_horizontal_delete)(self); } }


    /// `BackgroundPositionVertical` struct
    pub use crate::dll::AzBackgroundPositionVertical as BackgroundPositionVertical;

    impl std::fmt::Debug for BackgroundPositionVertical { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_background_position_vertical_fmt_debug)(self)) } }
    impl Clone for BackgroundPositionVertical { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_background_position_vertical_deep_copy)(self) } }
    impl Drop for BackgroundPositionVertical { fn drop(&mut self) { (crate::dll::get_azul_dll().az_background_position_vertical_delete)(self); } }


    /// `StyleBackgroundPosition` struct
    pub use crate::dll::AzStyleBackgroundPosition as StyleBackgroundPosition;

    impl std::fmt::Debug for StyleBackgroundPosition { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_background_position_fmt_debug)(self)) } }
    impl Clone for StyleBackgroundPosition { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_position_deep_copy)(self) } }
    impl Drop for StyleBackgroundPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_position_delete)(self); } }


    /// `StyleBackgroundRepeat` struct
    pub use crate::dll::AzStyleBackgroundRepeat as StyleBackgroundRepeat;

    impl std::fmt::Debug for StyleBackgroundRepeat { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_background_repeat_fmt_debug)(self)) } }
    impl Clone for StyleBackgroundRepeat { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_repeat_deep_copy)(self) } }
    impl Drop for StyleBackgroundRepeat { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_repeat_delete)(self); } }


    /// `StyleBackgroundSize` struct
    pub use crate::dll::AzStyleBackgroundSize as StyleBackgroundSize;

    impl std::fmt::Debug for StyleBackgroundSize { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_background_size_fmt_debug)(self)) } }
    impl Clone for StyleBackgroundSize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_size_deep_copy)(self) } }
    impl Drop for StyleBackgroundSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_size_delete)(self); } }


    /// `StyleBorderBottomColor` struct
    pub use crate::dll::AzStyleBorderBottomColor as StyleBorderBottomColor;

    impl std::fmt::Debug for StyleBorderBottomColor { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_bottom_color_fmt_debug)(self)) } }
    impl Clone for StyleBorderBottomColor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_color_deep_copy)(self) } }
    impl Drop for StyleBorderBottomColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_color_delete)(self); } }


    /// `StyleBorderBottomLeftRadius` struct
    pub use crate::dll::AzStyleBorderBottomLeftRadius as StyleBorderBottomLeftRadius;

    impl std::fmt::Debug for StyleBorderBottomLeftRadius { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_fmt_debug)(self)) } }
    impl Clone for StyleBorderBottomLeftRadius { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_deep_copy)(self) } }
    impl Drop for StyleBorderBottomLeftRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_delete)(self); } }


    /// `StyleBorderBottomRightRadius` struct
    pub use crate::dll::AzStyleBorderBottomRightRadius as StyleBorderBottomRightRadius;

    impl std::fmt::Debug for StyleBorderBottomRightRadius { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_fmt_debug)(self)) } }
    impl Clone for StyleBorderBottomRightRadius { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_deep_copy)(self) } }
    impl Drop for StyleBorderBottomRightRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_delete)(self); } }


    /// `BorderStyle` struct
    pub use crate::dll::AzBorderStyle as BorderStyle;

    impl std::fmt::Debug for BorderStyle { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_border_style_fmt_debug)(self)) } }
    impl Clone for BorderStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_border_style_deep_copy)(self) } }
    impl Drop for BorderStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_border_style_delete)(self); } }


    /// `StyleBorderBottomStyle` struct
    pub use crate::dll::AzStyleBorderBottomStyle as StyleBorderBottomStyle;

    impl std::fmt::Debug for StyleBorderBottomStyle { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_bottom_style_fmt_debug)(self)) } }
    impl Clone for StyleBorderBottomStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_style_deep_copy)(self) } }
    impl Drop for StyleBorderBottomStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_style_delete)(self); } }


    /// `StyleBorderBottomWidth` struct
    pub use crate::dll::AzStyleBorderBottomWidth as StyleBorderBottomWidth;

    impl std::fmt::Debug for StyleBorderBottomWidth { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_bottom_width_fmt_debug)(self)) } }
    impl Clone for StyleBorderBottomWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_width_deep_copy)(self) } }
    impl Drop for StyleBorderBottomWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_width_delete)(self); } }


    /// `StyleBorderLeftColor` struct
    pub use crate::dll::AzStyleBorderLeftColor as StyleBorderLeftColor;

    impl std::fmt::Debug for StyleBorderLeftColor { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_left_color_fmt_debug)(self)) } }
    impl Clone for StyleBorderLeftColor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_color_deep_copy)(self) } }
    impl Drop for StyleBorderLeftColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_color_delete)(self); } }


    /// `StyleBorderLeftStyle` struct
    pub use crate::dll::AzStyleBorderLeftStyle as StyleBorderLeftStyle;

    impl std::fmt::Debug for StyleBorderLeftStyle { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_left_style_fmt_debug)(self)) } }
    impl Clone for StyleBorderLeftStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_style_deep_copy)(self) } }
    impl Drop for StyleBorderLeftStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_style_delete)(self); } }


    /// `StyleBorderLeftWidth` struct
    pub use crate::dll::AzStyleBorderLeftWidth as StyleBorderLeftWidth;

    impl std::fmt::Debug for StyleBorderLeftWidth { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_left_width_fmt_debug)(self)) } }
    impl Clone for StyleBorderLeftWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_width_deep_copy)(self) } }
    impl Drop for StyleBorderLeftWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_width_delete)(self); } }


    /// `StyleBorderRightColor` struct
    pub use crate::dll::AzStyleBorderRightColor as StyleBorderRightColor;

    impl std::fmt::Debug for StyleBorderRightColor { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_right_color_fmt_debug)(self)) } }
    impl Clone for StyleBorderRightColor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_color_deep_copy)(self) } }
    impl Drop for StyleBorderRightColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_color_delete)(self); } }


    /// `StyleBorderRightStyle` struct
    pub use crate::dll::AzStyleBorderRightStyle as StyleBorderRightStyle;

    impl std::fmt::Debug for StyleBorderRightStyle { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_right_style_fmt_debug)(self)) } }
    impl Clone for StyleBorderRightStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_style_deep_copy)(self) } }
    impl Drop for StyleBorderRightStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_style_delete)(self); } }


    /// `StyleBorderRightWidth` struct
    pub use crate::dll::AzStyleBorderRightWidth as StyleBorderRightWidth;

    impl std::fmt::Debug for StyleBorderRightWidth { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_right_width_fmt_debug)(self)) } }
    impl Clone for StyleBorderRightWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_width_deep_copy)(self) } }
    impl Drop for StyleBorderRightWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_width_delete)(self); } }


    /// `StyleBorderTopColor` struct
    pub use crate::dll::AzStyleBorderTopColor as StyleBorderTopColor;

    impl std::fmt::Debug for StyleBorderTopColor { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_top_color_fmt_debug)(self)) } }
    impl Clone for StyleBorderTopColor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_color_deep_copy)(self) } }
    impl Drop for StyleBorderTopColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_color_delete)(self); } }


    /// `StyleBorderTopLeftRadius` struct
    pub use crate::dll::AzStyleBorderTopLeftRadius as StyleBorderTopLeftRadius;

    impl std::fmt::Debug for StyleBorderTopLeftRadius { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_top_left_radius_fmt_debug)(self)) } }
    impl Clone for StyleBorderTopLeftRadius { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_left_radius_deep_copy)(self) } }
    impl Drop for StyleBorderTopLeftRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_left_radius_delete)(self); } }


    /// `StyleBorderTopRightRadius` struct
    pub use crate::dll::AzStyleBorderTopRightRadius as StyleBorderTopRightRadius;

    impl std::fmt::Debug for StyleBorderTopRightRadius { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_top_right_radius_fmt_debug)(self)) } }
    impl Clone for StyleBorderTopRightRadius { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_right_radius_deep_copy)(self) } }
    impl Drop for StyleBorderTopRightRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_right_radius_delete)(self); } }


    /// `StyleBorderTopStyle` struct
    pub use crate::dll::AzStyleBorderTopStyle as StyleBorderTopStyle;

    impl std::fmt::Debug for StyleBorderTopStyle { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_top_style_fmt_debug)(self)) } }
    impl Clone for StyleBorderTopStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_style_deep_copy)(self) } }
    impl Drop for StyleBorderTopStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_style_delete)(self); } }


    /// `StyleBorderTopWidth` struct
    pub use crate::dll::AzStyleBorderTopWidth as StyleBorderTopWidth;

    impl std::fmt::Debug for StyleBorderTopWidth { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_top_width_fmt_debug)(self)) } }
    impl Clone for StyleBorderTopWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_width_deep_copy)(self) } }
    impl Drop for StyleBorderTopWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_width_delete)(self); } }


    /// `StyleCursor` struct
    pub use crate::dll::AzStyleCursor as StyleCursor;

    impl std::fmt::Debug for StyleCursor { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_cursor_fmt_debug)(self)) } }
    impl Clone for StyleCursor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_cursor_deep_copy)(self) } }
    impl Drop for StyleCursor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_cursor_delete)(self); } }


    /// `StyleFontFamily` struct
    pub use crate::dll::AzStyleFontFamily as StyleFontFamily;

    impl std::fmt::Debug for StyleFontFamily { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_font_family_fmt_debug)(self)) } }
    impl Clone for StyleFontFamily { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_font_family_deep_copy)(self) } }
    impl Drop for StyleFontFamily { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_family_delete)(self); } }


    /// `StyleFontSize` struct
    pub use crate::dll::AzStyleFontSize as StyleFontSize;

    impl std::fmt::Debug for StyleFontSize { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_font_size_fmt_debug)(self)) } }
    impl Clone for StyleFontSize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_font_size_deep_copy)(self) } }
    impl Drop for StyleFontSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_size_delete)(self); } }


    /// `StyleLetterSpacing` struct
    pub use crate::dll::AzStyleLetterSpacing as StyleLetterSpacing;

    impl std::fmt::Debug for StyleLetterSpacing { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_letter_spacing_fmt_debug)(self)) } }
    impl Clone for StyleLetterSpacing { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_letter_spacing_deep_copy)(self) } }
    impl Drop for StyleLetterSpacing { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_letter_spacing_delete)(self); } }


    /// `StyleLineHeight` struct
    pub use crate::dll::AzStyleLineHeight as StyleLineHeight;

    impl std::fmt::Debug for StyleLineHeight { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_line_height_fmt_debug)(self)) } }
    impl Clone for StyleLineHeight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_line_height_deep_copy)(self) } }
    impl Drop for StyleLineHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_line_height_delete)(self); } }


    /// `StyleTabWidth` struct
    pub use crate::dll::AzStyleTabWidth as StyleTabWidth;

    impl std::fmt::Debug for StyleTabWidth { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_tab_width_fmt_debug)(self)) } }
    impl Clone for StyleTabWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_tab_width_deep_copy)(self) } }
    impl Drop for StyleTabWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_tab_width_delete)(self); } }


    /// `StyleOpacity` struct
    pub use crate::dll::AzStyleOpacity as StyleOpacity;

    impl std::fmt::Debug for StyleOpacity { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_opacity_fmt_debug)(self)) } }
    impl Clone for StyleOpacity { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_opacity_deep_copy)(self) } }
    impl Drop for StyleOpacity { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_opacity_delete)(self); } }


    /// `StyleTransformOrigin` struct
    pub use crate::dll::AzStyleTransformOrigin as StyleTransformOrigin;

    impl std::fmt::Debug for StyleTransformOrigin { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_origin_fmt_debug)(self)) } }
    impl Clone for StyleTransformOrigin { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_origin_deep_copy)(self) } }
    impl Drop for StyleTransformOrigin { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_origin_delete)(self); } }


    /// `StylePerspectiveOrigin` struct
    pub use crate::dll::AzStylePerspectiveOrigin as StylePerspectiveOrigin;

    impl std::fmt::Debug for StylePerspectiveOrigin { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_perspective_origin_fmt_debug)(self)) } }
    impl Clone for StylePerspectiveOrigin { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_perspective_origin_deep_copy)(self) } }
    impl Drop for StylePerspectiveOrigin { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_perspective_origin_delete)(self); } }


    /// `StyleBackfaceVisibility` struct
    pub use crate::dll::AzStyleBackfaceVisibility as StyleBackfaceVisibility;

    impl std::fmt::Debug for StyleBackfaceVisibility { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_backface_visibility_fmt_debug)(self)) } }
    impl Clone for StyleBackfaceVisibility { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_backface_visibility_deep_copy)(self) } }
    impl Drop for StyleBackfaceVisibility { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_backface_visibility_delete)(self); } }


    /// `StyleTransform` struct
    pub use crate::dll::AzStyleTransform as StyleTransform;

    impl std::fmt::Debug for StyleTransform { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_fmt_debug)(self)) } }
    impl Clone for StyleTransform { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_deep_copy)(self) } }
    impl Drop for StyleTransform { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_delete)(self); } }


    /// `StyleTransformMatrix2D` struct
    pub use crate::dll::AzStyleTransformMatrix2D as StyleTransformMatrix2D;

    impl std::fmt::Debug for StyleTransformMatrix2D { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_matrix2_d_fmt_debug)(self)) } }
    impl Clone for StyleTransformMatrix2D { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_matrix2_d_deep_copy)(self) } }
    impl Drop for StyleTransformMatrix2D { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_matrix2_d_delete)(self); } }


    /// `StyleTransformMatrix3D` struct
    pub use crate::dll::AzStyleTransformMatrix3D as StyleTransformMatrix3D;

    impl std::fmt::Debug for StyleTransformMatrix3D { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_matrix3_d_fmt_debug)(self)) } }
    impl Clone for StyleTransformMatrix3D { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_matrix3_d_deep_copy)(self) } }
    impl Drop for StyleTransformMatrix3D { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_matrix3_d_delete)(self); } }


    /// `StyleTransformTranslate2D` struct
    pub use crate::dll::AzStyleTransformTranslate2D as StyleTransformTranslate2D;

    impl std::fmt::Debug for StyleTransformTranslate2D { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_translate2_d_fmt_debug)(self)) } }
    impl Clone for StyleTransformTranslate2D { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_translate2_d_deep_copy)(self) } }
    impl Drop for StyleTransformTranslate2D { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_translate2_d_delete)(self); } }


    /// `StyleTransformTranslate3D` struct
    pub use crate::dll::AzStyleTransformTranslate3D as StyleTransformTranslate3D;

    impl std::fmt::Debug for StyleTransformTranslate3D { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_translate3_d_fmt_debug)(self)) } }
    impl Clone for StyleTransformTranslate3D { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_translate3_d_deep_copy)(self) } }
    impl Drop for StyleTransformTranslate3D { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_translate3_d_delete)(self); } }


    /// `StyleTransformRotate3D` struct
    pub use crate::dll::AzStyleTransformRotate3D as StyleTransformRotate3D;

    impl std::fmt::Debug for StyleTransformRotate3D { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_rotate3_d_fmt_debug)(self)) } }
    impl Clone for StyleTransformRotate3D { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_rotate3_d_deep_copy)(self) } }
    impl Drop for StyleTransformRotate3D { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_rotate3_d_delete)(self); } }


    /// `StyleTransformScale2D` struct
    pub use crate::dll::AzStyleTransformScale2D as StyleTransformScale2D;

    impl std::fmt::Debug for StyleTransformScale2D { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_scale2_d_fmt_debug)(self)) } }
    impl Clone for StyleTransformScale2D { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_scale2_d_deep_copy)(self) } }
    impl Drop for StyleTransformScale2D { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_scale2_d_delete)(self); } }


    /// `StyleTransformScale3D` struct
    pub use crate::dll::AzStyleTransformScale3D as StyleTransformScale3D;

    impl std::fmt::Debug for StyleTransformScale3D { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_scale3_d_fmt_debug)(self)) } }
    impl Clone for StyleTransformScale3D { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_scale3_d_deep_copy)(self) } }
    impl Drop for StyleTransformScale3D { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_scale3_d_delete)(self); } }


    /// `StyleTransformSkew2D` struct
    pub use crate::dll::AzStyleTransformSkew2D as StyleTransformSkew2D;

    impl std::fmt::Debug for StyleTransformSkew2D { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_skew2_d_fmt_debug)(self)) } }
    impl Clone for StyleTransformSkew2D { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_skew2_d_deep_copy)(self) } }
    impl Drop for StyleTransformSkew2D { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_skew2_d_delete)(self); } }


    /// `StyleTextAlignmentHorz` struct
    pub use crate::dll::AzStyleTextAlignmentHorz as StyleTextAlignmentHorz;

    impl std::fmt::Debug for StyleTextAlignmentHorz { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_text_alignment_horz_fmt_debug)(self)) } }
    impl Clone for StyleTextAlignmentHorz { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_text_alignment_horz_deep_copy)(self) } }
    impl Drop for StyleTextAlignmentHorz { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_alignment_horz_delete)(self); } }


    /// `StyleTextColor` struct
    pub use crate::dll::AzStyleTextColor as StyleTextColor;

    impl std::fmt::Debug for StyleTextColor { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_text_color_fmt_debug)(self)) } }
    impl Clone for StyleTextColor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_text_color_deep_copy)(self) } }
    impl Drop for StyleTextColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_color_delete)(self); } }


    /// `StyleWordSpacing` struct
    pub use crate::dll::AzStyleWordSpacing as StyleWordSpacing;

    impl std::fmt::Debug for StyleWordSpacing { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_word_spacing_fmt_debug)(self)) } }
    impl Clone for StyleWordSpacing { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_word_spacing_deep_copy)(self) } }
    impl Drop for StyleWordSpacing { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_word_spacing_delete)(self); } }


    /// `BoxShadowPreDisplayItemValue` struct
    pub use crate::dll::AzBoxShadowPreDisplayItemValue as BoxShadowPreDisplayItemValue;

    impl std::fmt::Debug for BoxShadowPreDisplayItemValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_value_fmt_debug)(self)) } }
    impl Clone for BoxShadowPreDisplayItemValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_value_deep_copy)(self) } }
    impl Drop for BoxShadowPreDisplayItemValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_value_delete)(self); } }


    /// `LayoutAlignContentValue` struct
    pub use crate::dll::AzLayoutAlignContentValue as LayoutAlignContentValue;

    impl std::fmt::Debug for LayoutAlignContentValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_align_content_value_fmt_debug)(self)) } }
    impl Clone for LayoutAlignContentValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_align_content_value_deep_copy)(self) } }
    impl Drop for LayoutAlignContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_content_value_delete)(self); } }


    /// `LayoutAlignItemsValue` struct
    pub use crate::dll::AzLayoutAlignItemsValue as LayoutAlignItemsValue;

    impl std::fmt::Debug for LayoutAlignItemsValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_align_items_value_fmt_debug)(self)) } }
    impl Clone for LayoutAlignItemsValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_align_items_value_deep_copy)(self) } }
    impl Drop for LayoutAlignItemsValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_items_value_delete)(self); } }


    /// `LayoutBottomValue` struct
    pub use crate::dll::AzLayoutBottomValue as LayoutBottomValue;

    impl std::fmt::Debug for LayoutBottomValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_bottom_value_fmt_debug)(self)) } }
    impl Clone for LayoutBottomValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_bottom_value_deep_copy)(self) } }
    impl Drop for LayoutBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_bottom_value_delete)(self); } }


    /// `LayoutBoxSizingValue` struct
    pub use crate::dll::AzLayoutBoxSizingValue as LayoutBoxSizingValue;

    impl std::fmt::Debug for LayoutBoxSizingValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_box_sizing_value_fmt_debug)(self)) } }
    impl Clone for LayoutBoxSizingValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_box_sizing_value_deep_copy)(self) } }
    impl Drop for LayoutBoxSizingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_box_sizing_value_delete)(self); } }


    /// `LayoutDirectionValue` struct
    pub use crate::dll::AzLayoutDirectionValue as LayoutDirectionValue;

    impl std::fmt::Debug for LayoutDirectionValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_direction_value_fmt_debug)(self)) } }
    impl Clone for LayoutDirectionValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_direction_value_deep_copy)(self) } }
    impl Drop for LayoutDirectionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_direction_value_delete)(self); } }


    /// `LayoutDisplayValue` struct
    pub use crate::dll::AzLayoutDisplayValue as LayoutDisplayValue;

    impl std::fmt::Debug for LayoutDisplayValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_display_value_fmt_debug)(self)) } }
    impl Clone for LayoutDisplayValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_display_value_deep_copy)(self) } }
    impl Drop for LayoutDisplayValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_display_value_delete)(self); } }


    /// `LayoutFlexGrowValue` struct
    pub use crate::dll::AzLayoutFlexGrowValue as LayoutFlexGrowValue;

    impl std::fmt::Debug for LayoutFlexGrowValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_flex_grow_value_fmt_debug)(self)) } }
    impl Clone for LayoutFlexGrowValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_flex_grow_value_deep_copy)(self) } }
    impl Drop for LayoutFlexGrowValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_grow_value_delete)(self); } }


    /// `LayoutFlexShrinkValue` struct
    pub use crate::dll::AzLayoutFlexShrinkValue as LayoutFlexShrinkValue;

    impl std::fmt::Debug for LayoutFlexShrinkValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_flex_shrink_value_fmt_debug)(self)) } }
    impl Clone for LayoutFlexShrinkValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_flex_shrink_value_deep_copy)(self) } }
    impl Drop for LayoutFlexShrinkValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_shrink_value_delete)(self); } }


    /// `LayoutFloatValue` struct
    pub use crate::dll::AzLayoutFloatValue as LayoutFloatValue;

    impl std::fmt::Debug for LayoutFloatValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_float_value_fmt_debug)(self)) } }
    impl Clone for LayoutFloatValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_float_value_deep_copy)(self) } }
    impl Drop for LayoutFloatValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_float_value_delete)(self); } }


    /// `LayoutHeightValue` struct
    pub use crate::dll::AzLayoutHeightValue as LayoutHeightValue;

    impl std::fmt::Debug for LayoutHeightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_height_value_fmt_debug)(self)) } }
    impl Clone for LayoutHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_height_value_deep_copy)(self) } }
    impl Drop for LayoutHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_height_value_delete)(self); } }


    /// `LayoutJustifyContentValue` struct
    pub use crate::dll::AzLayoutJustifyContentValue as LayoutJustifyContentValue;

    impl std::fmt::Debug for LayoutJustifyContentValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_justify_content_value_fmt_debug)(self)) } }
    impl Clone for LayoutJustifyContentValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_justify_content_value_deep_copy)(self) } }
    impl Drop for LayoutJustifyContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_justify_content_value_delete)(self); } }


    /// `LayoutLeftValue` struct
    pub use crate::dll::AzLayoutLeftValue as LayoutLeftValue;

    impl std::fmt::Debug for LayoutLeftValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_left_value_fmt_debug)(self)) } }
    impl Clone for LayoutLeftValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_left_value_deep_copy)(self) } }
    impl Drop for LayoutLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_left_value_delete)(self); } }


    /// `LayoutMarginBottomValue` struct
    pub use crate::dll::AzLayoutMarginBottomValue as LayoutMarginBottomValue;

    impl std::fmt::Debug for LayoutMarginBottomValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_margin_bottom_value_fmt_debug)(self)) } }
    impl Clone for LayoutMarginBottomValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_bottom_value_deep_copy)(self) } }
    impl Drop for LayoutMarginBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_bottom_value_delete)(self); } }


    /// `LayoutMarginLeftValue` struct
    pub use crate::dll::AzLayoutMarginLeftValue as LayoutMarginLeftValue;

    impl std::fmt::Debug for LayoutMarginLeftValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_margin_left_value_fmt_debug)(self)) } }
    impl Clone for LayoutMarginLeftValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_left_value_deep_copy)(self) } }
    impl Drop for LayoutMarginLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_left_value_delete)(self); } }


    /// `LayoutMarginRightValue` struct
    pub use crate::dll::AzLayoutMarginRightValue as LayoutMarginRightValue;

    impl std::fmt::Debug for LayoutMarginRightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_margin_right_value_fmt_debug)(self)) } }
    impl Clone for LayoutMarginRightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_right_value_deep_copy)(self) } }
    impl Drop for LayoutMarginRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_right_value_delete)(self); } }


    /// `LayoutMarginTopValue` struct
    pub use crate::dll::AzLayoutMarginTopValue as LayoutMarginTopValue;

    impl std::fmt::Debug for LayoutMarginTopValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_margin_top_value_fmt_debug)(self)) } }
    impl Clone for LayoutMarginTopValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_top_value_deep_copy)(self) } }
    impl Drop for LayoutMarginTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_top_value_delete)(self); } }


    /// `LayoutMaxHeightValue` struct
    pub use crate::dll::AzLayoutMaxHeightValue as LayoutMaxHeightValue;

    impl std::fmt::Debug for LayoutMaxHeightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_max_height_value_fmt_debug)(self)) } }
    impl Clone for LayoutMaxHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_max_height_value_deep_copy)(self) } }
    impl Drop for LayoutMaxHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_height_value_delete)(self); } }


    /// `LayoutMaxWidthValue` struct
    pub use crate::dll::AzLayoutMaxWidthValue as LayoutMaxWidthValue;

    impl std::fmt::Debug for LayoutMaxWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_max_width_value_fmt_debug)(self)) } }
    impl Clone for LayoutMaxWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_max_width_value_deep_copy)(self) } }
    impl Drop for LayoutMaxWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_width_value_delete)(self); } }


    /// `LayoutMinHeightValue` struct
    pub use crate::dll::AzLayoutMinHeightValue as LayoutMinHeightValue;

    impl std::fmt::Debug for LayoutMinHeightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_min_height_value_fmt_debug)(self)) } }
    impl Clone for LayoutMinHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_min_height_value_deep_copy)(self) } }
    impl Drop for LayoutMinHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_height_value_delete)(self); } }


    /// `LayoutMinWidthValue` struct
    pub use crate::dll::AzLayoutMinWidthValue as LayoutMinWidthValue;

    impl std::fmt::Debug for LayoutMinWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_min_width_value_fmt_debug)(self)) } }
    impl Clone for LayoutMinWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_min_width_value_deep_copy)(self) } }
    impl Drop for LayoutMinWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_width_value_delete)(self); } }


    /// `LayoutPaddingBottomValue` struct
    pub use crate::dll::AzLayoutPaddingBottomValue as LayoutPaddingBottomValue;

    impl std::fmt::Debug for LayoutPaddingBottomValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_padding_bottom_value_fmt_debug)(self)) } }
    impl Clone for LayoutPaddingBottomValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_bottom_value_deep_copy)(self) } }
    impl Drop for LayoutPaddingBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_bottom_value_delete)(self); } }


    /// `LayoutPaddingLeftValue` struct
    pub use crate::dll::AzLayoutPaddingLeftValue as LayoutPaddingLeftValue;

    impl std::fmt::Debug for LayoutPaddingLeftValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_padding_left_value_fmt_debug)(self)) } }
    impl Clone for LayoutPaddingLeftValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_left_value_deep_copy)(self) } }
    impl Drop for LayoutPaddingLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_left_value_delete)(self); } }


    /// `LayoutPaddingRightValue` struct
    pub use crate::dll::AzLayoutPaddingRightValue as LayoutPaddingRightValue;

    impl std::fmt::Debug for LayoutPaddingRightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_padding_right_value_fmt_debug)(self)) } }
    impl Clone for LayoutPaddingRightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_right_value_deep_copy)(self) } }
    impl Drop for LayoutPaddingRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_right_value_delete)(self); } }


    /// `LayoutPaddingTopValue` struct
    pub use crate::dll::AzLayoutPaddingTopValue as LayoutPaddingTopValue;

    impl std::fmt::Debug for LayoutPaddingTopValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_padding_top_value_fmt_debug)(self)) } }
    impl Clone for LayoutPaddingTopValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_top_value_deep_copy)(self) } }
    impl Drop for LayoutPaddingTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_top_value_delete)(self); } }


    /// `LayoutPositionValue` struct
    pub use crate::dll::AzLayoutPositionValue as LayoutPositionValue;

    impl std::fmt::Debug for LayoutPositionValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_position_value_fmt_debug)(self)) } }
    impl Clone for LayoutPositionValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_position_value_deep_copy)(self) } }
    impl Drop for LayoutPositionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_position_value_delete)(self); } }


    /// `LayoutRightValue` struct
    pub use crate::dll::AzLayoutRightValue as LayoutRightValue;

    impl std::fmt::Debug for LayoutRightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_right_value_fmt_debug)(self)) } }
    impl Clone for LayoutRightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_right_value_deep_copy)(self) } }
    impl Drop for LayoutRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_right_value_delete)(self); } }


    /// `LayoutTopValue` struct
    pub use crate::dll::AzLayoutTopValue as LayoutTopValue;

    impl std::fmt::Debug for LayoutTopValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_top_value_fmt_debug)(self)) } }
    impl Clone for LayoutTopValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_top_value_deep_copy)(self) } }
    impl Drop for LayoutTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_top_value_delete)(self); } }


    /// `LayoutWidthValue` struct
    pub use crate::dll::AzLayoutWidthValue as LayoutWidthValue;

    impl std::fmt::Debug for LayoutWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_width_value_fmt_debug)(self)) } }
    impl Clone for LayoutWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_width_value_deep_copy)(self) } }
    impl Drop for LayoutWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_width_value_delete)(self); } }


    /// `LayoutWrapValue` struct
    pub use crate::dll::AzLayoutWrapValue as LayoutWrapValue;

    impl std::fmt::Debug for LayoutWrapValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_layout_wrap_value_fmt_debug)(self)) } }
    impl Clone for LayoutWrapValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_wrap_value_deep_copy)(self) } }
    impl Drop for LayoutWrapValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_wrap_value_delete)(self); } }


    /// `OverflowValue` struct
    pub use crate::dll::AzOverflowValue as OverflowValue;

    impl std::fmt::Debug for OverflowValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_overflow_value_fmt_debug)(self)) } }
    impl Clone for OverflowValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_overflow_value_deep_copy)(self) } }
    impl Drop for OverflowValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_overflow_value_delete)(self); } }


    /// `StyleBackgroundContentValue` struct
    pub use crate::dll::AzStyleBackgroundContentValue as StyleBackgroundContentValue;

    impl std::fmt::Debug for StyleBackgroundContentValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_background_content_value_fmt_debug)(self)) } }
    impl Clone for StyleBackgroundContentValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_content_value_deep_copy)(self) } }
    impl Drop for StyleBackgroundContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_content_value_delete)(self); } }


    /// `StyleBackgroundPositionValue` struct
    pub use crate::dll::AzStyleBackgroundPositionValue as StyleBackgroundPositionValue;

    impl std::fmt::Debug for StyleBackgroundPositionValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_background_position_value_fmt_debug)(self)) } }
    impl Clone for StyleBackgroundPositionValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_position_value_deep_copy)(self) } }
    impl Drop for StyleBackgroundPositionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_position_value_delete)(self); } }


    /// `StyleBackgroundRepeatValue` struct
    pub use crate::dll::AzStyleBackgroundRepeatValue as StyleBackgroundRepeatValue;

    impl std::fmt::Debug for StyleBackgroundRepeatValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_background_repeat_value_fmt_debug)(self)) } }
    impl Clone for StyleBackgroundRepeatValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_repeat_value_deep_copy)(self) } }
    impl Drop for StyleBackgroundRepeatValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_repeat_value_delete)(self); } }


    /// `StyleBackgroundSizeValue` struct
    pub use crate::dll::AzStyleBackgroundSizeValue as StyleBackgroundSizeValue;

    impl std::fmt::Debug for StyleBackgroundSizeValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_background_size_value_fmt_debug)(self)) } }
    impl Clone for StyleBackgroundSizeValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_size_value_deep_copy)(self) } }
    impl Drop for StyleBackgroundSizeValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_size_value_delete)(self); } }


    /// `StyleBorderBottomColorValue` struct
    pub use crate::dll::AzStyleBorderBottomColorValue as StyleBorderBottomColorValue;

    impl std::fmt::Debug for StyleBorderBottomColorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_bottom_color_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderBottomColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_color_value_deep_copy)(self) } }
    impl Drop for StyleBorderBottomColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_color_value_delete)(self); } }


    /// `StyleBorderBottomLeftRadiusValue` struct
    pub use crate::dll::AzStyleBorderBottomLeftRadiusValue as StyleBorderBottomLeftRadiusValue;

    impl std::fmt::Debug for StyleBorderBottomLeftRadiusValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderBottomLeftRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_value_deep_copy)(self) } }
    impl Drop for StyleBorderBottomLeftRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_value_delete)(self); } }


    /// `StyleBorderBottomRightRadiusValue` struct
    pub use crate::dll::AzStyleBorderBottomRightRadiusValue as StyleBorderBottomRightRadiusValue;

    impl std::fmt::Debug for StyleBorderBottomRightRadiusValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderBottomRightRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_value_deep_copy)(self) } }
    impl Drop for StyleBorderBottomRightRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_value_delete)(self); } }


    /// `StyleBorderBottomStyleValue` struct
    pub use crate::dll::AzStyleBorderBottomStyleValue as StyleBorderBottomStyleValue;

    impl std::fmt::Debug for StyleBorderBottomStyleValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_bottom_style_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderBottomStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_style_value_deep_copy)(self) } }
    impl Drop for StyleBorderBottomStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_style_value_delete)(self); } }


    /// `StyleBorderBottomWidthValue` struct
    pub use crate::dll::AzStyleBorderBottomWidthValue as StyleBorderBottomWidthValue;

    impl std::fmt::Debug for StyleBorderBottomWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_bottom_width_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderBottomWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_width_value_deep_copy)(self) } }
    impl Drop for StyleBorderBottomWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_width_value_delete)(self); } }


    /// `StyleBorderLeftColorValue` struct
    pub use crate::dll::AzStyleBorderLeftColorValue as StyleBorderLeftColorValue;

    impl std::fmt::Debug for StyleBorderLeftColorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_left_color_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderLeftColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_color_value_deep_copy)(self) } }
    impl Drop for StyleBorderLeftColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_color_value_delete)(self); } }


    /// `StyleBorderLeftStyleValue` struct
    pub use crate::dll::AzStyleBorderLeftStyleValue as StyleBorderLeftStyleValue;

    impl std::fmt::Debug for StyleBorderLeftStyleValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_left_style_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderLeftStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_style_value_deep_copy)(self) } }
    impl Drop for StyleBorderLeftStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_style_value_delete)(self); } }


    /// `StyleBorderLeftWidthValue` struct
    pub use crate::dll::AzStyleBorderLeftWidthValue as StyleBorderLeftWidthValue;

    impl std::fmt::Debug for StyleBorderLeftWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_left_width_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderLeftWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_width_value_deep_copy)(self) } }
    impl Drop for StyleBorderLeftWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_width_value_delete)(self); } }


    /// `StyleBorderRightColorValue` struct
    pub use crate::dll::AzStyleBorderRightColorValue as StyleBorderRightColorValue;

    impl std::fmt::Debug for StyleBorderRightColorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_right_color_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderRightColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_color_value_deep_copy)(self) } }
    impl Drop for StyleBorderRightColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_color_value_delete)(self); } }


    /// `StyleBorderRightStyleValue` struct
    pub use crate::dll::AzStyleBorderRightStyleValue as StyleBorderRightStyleValue;

    impl std::fmt::Debug for StyleBorderRightStyleValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_right_style_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderRightStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_style_value_deep_copy)(self) } }
    impl Drop for StyleBorderRightStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_style_value_delete)(self); } }


    /// `StyleBorderRightWidthValue` struct
    pub use crate::dll::AzStyleBorderRightWidthValue as StyleBorderRightWidthValue;

    impl std::fmt::Debug for StyleBorderRightWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_right_width_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderRightWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_width_value_deep_copy)(self) } }
    impl Drop for StyleBorderRightWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_width_value_delete)(self); } }


    /// `StyleBorderTopColorValue` struct
    pub use crate::dll::AzStyleBorderTopColorValue as StyleBorderTopColorValue;

    impl std::fmt::Debug for StyleBorderTopColorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_top_color_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderTopColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_color_value_deep_copy)(self) } }
    impl Drop for StyleBorderTopColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_color_value_delete)(self); } }


    /// `StyleBorderTopLeftRadiusValue` struct
    pub use crate::dll::AzStyleBorderTopLeftRadiusValue as StyleBorderTopLeftRadiusValue;

    impl std::fmt::Debug for StyleBorderTopLeftRadiusValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_top_left_radius_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderTopLeftRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_left_radius_value_deep_copy)(self) } }
    impl Drop for StyleBorderTopLeftRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_left_radius_value_delete)(self); } }


    /// `StyleBorderTopRightRadiusValue` struct
    pub use crate::dll::AzStyleBorderTopRightRadiusValue as StyleBorderTopRightRadiusValue;

    impl std::fmt::Debug for StyleBorderTopRightRadiusValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_top_right_radius_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderTopRightRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_right_radius_value_deep_copy)(self) } }
    impl Drop for StyleBorderTopRightRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_right_radius_value_delete)(self); } }


    /// `StyleBorderTopStyleValue` struct
    pub use crate::dll::AzStyleBorderTopStyleValue as StyleBorderTopStyleValue;

    impl std::fmt::Debug for StyleBorderTopStyleValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_top_style_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderTopStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_style_value_deep_copy)(self) } }
    impl Drop for StyleBorderTopStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_style_value_delete)(self); } }


    /// `StyleBorderTopWidthValue` struct
    pub use crate::dll::AzStyleBorderTopWidthValue as StyleBorderTopWidthValue;

    impl std::fmt::Debug for StyleBorderTopWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_border_top_width_value_fmt_debug)(self)) } }
    impl Clone for StyleBorderTopWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_width_value_deep_copy)(self) } }
    impl Drop for StyleBorderTopWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_width_value_delete)(self); } }


    /// `StyleCursorValue` struct
    pub use crate::dll::AzStyleCursorValue as StyleCursorValue;

    impl std::fmt::Debug for StyleCursorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_cursor_value_fmt_debug)(self)) } }
    impl Clone for StyleCursorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_cursor_value_deep_copy)(self) } }
    impl Drop for StyleCursorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_cursor_value_delete)(self); } }


    /// `StyleFontFamilyValue` struct
    pub use crate::dll::AzStyleFontFamilyValue as StyleFontFamilyValue;

    impl std::fmt::Debug for StyleFontFamilyValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_font_family_value_fmt_debug)(self)) } }
    impl Clone for StyleFontFamilyValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_font_family_value_deep_copy)(self) } }
    impl Drop for StyleFontFamilyValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_family_value_delete)(self); } }


    /// `StyleFontSizeValue` struct
    pub use crate::dll::AzStyleFontSizeValue as StyleFontSizeValue;

    impl std::fmt::Debug for StyleFontSizeValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_font_size_value_fmt_debug)(self)) } }
    impl Clone for StyleFontSizeValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_font_size_value_deep_copy)(self) } }
    impl Drop for StyleFontSizeValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_size_value_delete)(self); } }


    /// `StyleLetterSpacingValue` struct
    pub use crate::dll::AzStyleLetterSpacingValue as StyleLetterSpacingValue;

    impl std::fmt::Debug for StyleLetterSpacingValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_letter_spacing_value_fmt_debug)(self)) } }
    impl Clone for StyleLetterSpacingValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_letter_spacing_value_deep_copy)(self) } }
    impl Drop for StyleLetterSpacingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_letter_spacing_value_delete)(self); } }


    /// `StyleLineHeightValue` struct
    pub use crate::dll::AzStyleLineHeightValue as StyleLineHeightValue;

    impl std::fmt::Debug for StyleLineHeightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_line_height_value_fmt_debug)(self)) } }
    impl Clone for StyleLineHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_line_height_value_deep_copy)(self) } }
    impl Drop for StyleLineHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_line_height_value_delete)(self); } }


    /// `StyleTabWidthValue` struct
    pub use crate::dll::AzStyleTabWidthValue as StyleTabWidthValue;

    impl std::fmt::Debug for StyleTabWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_tab_width_value_fmt_debug)(self)) } }
    impl Clone for StyleTabWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_tab_width_value_deep_copy)(self) } }
    impl Drop for StyleTabWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_tab_width_value_delete)(self); } }


    /// `StyleTextAlignmentHorzValue` struct
    pub use crate::dll::AzStyleTextAlignmentHorzValue as StyleTextAlignmentHorzValue;

    impl std::fmt::Debug for StyleTextAlignmentHorzValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_text_alignment_horz_value_fmt_debug)(self)) } }
    impl Clone for StyleTextAlignmentHorzValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_text_alignment_horz_value_deep_copy)(self) } }
    impl Drop for StyleTextAlignmentHorzValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_alignment_horz_value_delete)(self); } }


    /// `StyleTextColorValue` struct
    pub use crate::dll::AzStyleTextColorValue as StyleTextColorValue;

    impl std::fmt::Debug for StyleTextColorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_text_color_value_fmt_debug)(self)) } }
    impl Clone for StyleTextColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_text_color_value_deep_copy)(self) } }
    impl Drop for StyleTextColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_color_value_delete)(self); } }


    /// `StyleWordSpacingValue` struct
    pub use crate::dll::AzStyleWordSpacingValue as StyleWordSpacingValue;

    impl std::fmt::Debug for StyleWordSpacingValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_word_spacing_value_fmt_debug)(self)) } }
    impl Clone for StyleWordSpacingValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_word_spacing_value_deep_copy)(self) } }
    impl Drop for StyleWordSpacingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_word_spacing_value_delete)(self); } }


    /// `StyleOpacityValue` struct
    pub use crate::dll::AzStyleOpacityValue as StyleOpacityValue;

    impl std::fmt::Debug for StyleOpacityValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_opacity_value_fmt_debug)(self)) } }
    impl Clone for StyleOpacityValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_opacity_value_deep_copy)(self) } }
    impl Drop for StyleOpacityValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_opacity_value_delete)(self); } }


    /// `StyleTransformVecValue` struct
    pub use crate::dll::AzStyleTransformVecValue as StyleTransformVecValue;

    impl std::fmt::Debug for StyleTransformVecValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_vec_value_fmt_debug)(self)) } }
    impl Clone for StyleTransformVecValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_vec_value_deep_copy)(self) } }
    impl Drop for StyleTransformVecValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_vec_value_delete)(self); } }


    /// `StyleTransformOriginValue` struct
    pub use crate::dll::AzStyleTransformOriginValue as StyleTransformOriginValue;

    impl std::fmt::Debug for StyleTransformOriginValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_transform_origin_value_fmt_debug)(self)) } }
    impl Clone for StyleTransformOriginValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_origin_value_deep_copy)(self) } }
    impl Drop for StyleTransformOriginValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_origin_value_delete)(self); } }


    /// `StylePerspectiveOriginValue` struct
    pub use crate::dll::AzStylePerspectiveOriginValue as StylePerspectiveOriginValue;

    impl std::fmt::Debug for StylePerspectiveOriginValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_perspective_origin_value_fmt_debug)(self)) } }
    impl Clone for StylePerspectiveOriginValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_perspective_origin_value_deep_copy)(self) } }
    impl Drop for StylePerspectiveOriginValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_perspective_origin_value_delete)(self); } }


    /// `StyleBackfaceVisibilityValue` struct
    pub use crate::dll::AzStyleBackfaceVisibilityValue as StyleBackfaceVisibilityValue;

    impl std::fmt::Debug for StyleBackfaceVisibilityValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_style_backface_visibility_value_fmt_debug)(self)) } }
    impl Clone for StyleBackfaceVisibilityValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_backface_visibility_value_deep_copy)(self) } }
    impl Drop for StyleBackfaceVisibilityValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_backface_visibility_value_delete)(self); } }


    /// Parsed CSS key-value pair
    pub use crate::dll::AzCssProperty as CssProperty;

    impl std::fmt::Debug for CssProperty { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_css_property_fmt_debug)(self)) } }
    impl Clone for CssProperty { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_property_deep_copy)(self) } }
    impl Drop for CssProperty { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_property_delete)(self); } }
