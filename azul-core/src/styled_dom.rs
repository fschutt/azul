use std::{
    fmt,
    collections::BTreeMap
};
use azul_css::{
    Css, CssPath, CssProperty, CssPropertyType,

    StyleBackgroundContentVecValue, StyleBackgroundPositionVecValue,
    StyleBackgroundSizeVecValue, StyleBackgroundRepeatVecValue,
    StyleFontSizeValue, StyleFontFamilyValue, StyleTextColorValue,
    StyleTextAlignmentHorzValue, StyleLineHeightValue, StyleLetterSpacingValue,
    StyleWordSpacingValue, StyleTabWidthValue, StyleCursorValue,
    StyleBoxShadowValue, StyleBorderTopColorValue, StyleBorderLeftColorValue,
    StyleBorderRightColorValue, StyleBorderBottomColorValue,
    StyleBorderTopStyleValue, StyleBorderLeftStyleValue,
    StyleBorderRightStyleValue, StyleBorderBottomStyleValue,
    StyleBorderTopLeftRadiusValue, StyleBorderTopRightRadiusValue,
    StyleBorderBottomLeftRadiusValue, StyleBorderBottomRightRadiusValue,
    StyleOpacityValue, StyleTransformVecValue, StyleTransformOriginValue,
    StylePerspectiveOriginValue, StyleBackfaceVisibilityValue, StyleTextColor,
    StyleFontSize,

    LayoutDisplayValue, LayoutFloatValue, LayoutBoxSizingValue,
    LayoutWidthValue,  LayoutHeightValue, LayoutMinWidthValue,
    LayoutMinHeightValue, LayoutMaxWidthValue,  LayoutMaxHeightValue,
    LayoutPositionValue, LayoutTopValue, LayoutBottomValue, LayoutRightValue,
    LayoutLeftValue, LayoutPaddingTopValue, LayoutPaddingBottomValue,
    LayoutPaddingLeftValue, LayoutPaddingRightValue, LayoutMarginTopValue,
    LayoutMarginBottomValue, LayoutMarginLeftValue, LayoutMarginRightValue,
    LayoutBorderTopWidthValue, LayoutBorderLeftWidthValue,
    LayoutBorderRightWidthValue, LayoutBorderBottomWidthValue,
    LayoutOverflowValue, LayoutFlexDirectionValue, LayoutFlexWrapValue,
    LayoutFlexGrowValue, LayoutFlexShrinkValue, LayoutJustifyContentValue,
    LayoutAlignItemsValue, LayoutAlignContentValue,
};
use crate::{
    FastHashSet, FastHashMap,
    id_tree::{NodeDataContainerRef, Node, NodeId, NodeDataContainerRefMut},
    dom::{Dom, NodeDataVec, CompactDom, TagId, OptionTabIndex},
    style::{
        CascadeInfoVec, construct_html_cascade_tree,
        matches_html_element, rule_ends_with,
    },
    app_resources::{AppResources, ImageId, Au, ImmediateFontId},
};

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub struct ChangedCssProperty {
    pub previous_state: StyledNodeState,
    pub previous_prop: CssProperty,
    pub current_state: StyledNodeState,
    pub current_prop: CssProperty,
}

impl_vec!(ChangedCssProperty, ChangedCssPropertyVec, ChangedCssPropertyVecDestructor);
impl_vec_debug!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_partialord!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_clone!(ChangedCssProperty, ChangedCssPropertyVec, ChangedCssPropertyVecDestructor);
impl_vec_partialeq!(ChangedCssProperty, ChangedCssPropertyVec);

#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub enum CssPropertySource {
    Css(CssPath),
    Inline,
}

/// NOTE: multiple states can be active at
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub struct StyledNodeState {
    pub normal: bool,
    pub hover: bool,
    pub active: bool,
    pub focused: bool,
}

impl Default for StyledNodeState {
    fn default() -> StyledNodeState {
        Self::new()
    }
}

impl StyledNodeState {
    pub const fn new() -> Self {
        StyledNodeState {
            normal: true,
            hover: false,
            active: false,
            focused: false,
        }
    }
}

/// A styled Dom node
#[repr(C)]
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct StyledNode {
    /// Current state of this styled node (used later for caching the style / layout)
    pub state: StyledNodeState,
    /// Optional tag ID
    ///
    /// NOTE: The tag ID has to be adjusted after the layout is done (due to scroll tags)
    pub tag_id: OptionTagId,
}

impl_vec!(StyledNode, StyledNodeVec, StyledNodeVecDestructor);
impl_vec_mut!(StyledNode, StyledNodeVec);
impl_vec_debug!(StyledNode, StyledNodeVec);
impl_vec_partialord!(StyledNode, StyledNodeVec);
impl_vec_clone!(StyledNode, StyledNodeVec, StyledNodeVecDestructor);
impl_vec_partialeq!(StyledNode, StyledNodeVec);

impl StyledNodeVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, StyledNode> {
        NodeDataContainerRef { internal: self.as_ref() }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, StyledNode> {
        NodeDataContainerRefMut { internal: self.as_mut() }
    }
}

#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct CssPropertyCachePtr {
    pub ptr: *mut CssPropertyCache,
}

unsafe impl Send for CssPropertyCachePtr { } // necessary to build display list in parallel
unsafe impl Sync for CssPropertyCachePtr { } // necessary to build display list in parallel

impl CssPropertyCachePtr {
    pub fn new(cache: CssPropertyCache) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(cache))
        }
    }
}

impl Clone for CssPropertyCachePtr {
    fn clone(&self) -> Self {
        let p = unsafe { &*self.ptr };
        Self::new(p.clone())
    }
}

impl Drop for CssPropertyCachePtr {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.ptr) };
    }

}
// NOTE: To avoid large memory allocations, this is a "cache" that stores all the CSS properties
// found in the DOM. This cache exists on a per-DOM basis, so it scales independent of how many
// nodes are in the DOM.
//
// If each node would carry its own CSS properties, that would unnecessarily consume memory
// because most nodes use the default properties or override only one or two properties.
//
// The cache can compute the property of any node at any given time, given the current node
// state (hover, active, focused, normal). This way we don't have to duplicate the CSS properties
// onto every single node and exchange them when the style changes. Two caches can be appended
// to each other by simply merging their NodeIds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CssPropertyCache {
    // non-default CSS properties that were set on the DOM nodes themselves (inline properties)
    pub non_default_inline_normal_props:    BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub non_default_inline_hover_props:     BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub non_default_inline_active_props:    BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub non_default_inline_focus_props:     BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,

    // non-default CSS properties that were set via a CSS file
    pub non_default_css_normal_props:       BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub non_default_css_hover_props:        BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub non_default_css_active_props:       BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub non_default_css_focus_props:        BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
}

macro_rules! get_property {
    ($self_id:expr, $node_id:expr, $node_state:expr, $css_property_type:expr, $as_downcast_fn:ident) => {
        {
            let mut prop = None;

            // reverse order so we can early return
            // :focus > :active > :hover > :normal

            if $node_state.focused {
                if let Some(s) = $self_id.non_default_inline_focus_props.get($node_id).and_then(|map| map.get(&$css_property_type).and_then(|p| p.$as_downcast_fn())) {
                    prop = Some(s);
                    return prop;
                } else if let Some(s) = $self_id.non_default_css_focus_props.get($node_id).and_then(|map| map.get(&$css_property_type).and_then(|p| p.$as_downcast_fn())) {
                    prop = Some(s);
                    return prop;
                }
            }

            if $node_state.active {
                if let Some(s) = $self_id.non_default_inline_active_props.get($node_id).and_then(|map| map.get(&$css_property_type).and_then(|p| p.$as_downcast_fn())) {
                    prop = Some(s);
                    return prop;
                } else if let Some(s) = $self_id.non_default_css_active_props.get($node_id).and_then(|map| map.get(&$css_property_type).and_then(|p| p.$as_downcast_fn())) {
                    prop = Some(s);
                    return prop;
                }
            }

            if $node_state.hover {
                if let Some(s) = $self_id.non_default_inline_hover_props.get($node_id).and_then(|map| map.get(&$css_property_type).and_then(|p| p.$as_downcast_fn())) {
                    prop = Some(s);
                    return prop;
                } else if let Some(s) = $self_id.non_default_css_hover_props.get($node_id).and_then(|map| map.get(&$css_property_type).and_then(|p| p.$as_downcast_fn())) {
                    prop = Some(s);
                    return prop;
                }
            }

            if $node_state.normal {
                if let Some(s) = $self_id.non_default_inline_normal_props.get($node_id).and_then(|map| map.get(&$css_property_type).and_then(|p| p.$as_downcast_fn())) {
                    prop = Some(s);
                    return prop;
                } else if let Some(s) = $self_id.non_default_css_normal_props.get($node_id).and_then(|map| map.get(&$css_property_type).and_then(|p| p.$as_downcast_fn())) {
                    prop = Some(s);
                    return prop;
                }
            }

            prop
        }
    }
}

impl CssPropertyCache {

    pub fn append(&mut self, other: Self, node_id_shift: usize) {

        macro_rules! append_btreemap {($field_name:ident) => {{
            for (node_id, tree) in other.$field_name.into_iter() {
                self.$field_name
                .entry(node_id + node_id_shift)
                .or_insert_with(|| BTreeMap::default())
                .extend(tree.into_iter());
            }
        }};}

        append_btreemap!(non_default_inline_normal_props);
        append_btreemap!(non_default_inline_hover_props);
        append_btreemap!(non_default_inline_active_props);
        append_btreemap!(non_default_inline_focus_props);
        append_btreemap!(non_default_css_normal_props);
        append_btreemap!(non_default_css_hover_props);
        append_btreemap!(non_default_css_active_props);
        append_btreemap!(non_default_css_focus_props);
    }

    pub fn is_horizontal_overflow_visible(&self, node_id: &NodeId, node_state: &StyledNodeState) -> bool {
        self.get_overflow_x(node_id, node_state).and_then(|p| p.get_property_or_default()).unwrap_or_default().is_overflow_visible()
    }

    pub fn is_vertical_overflow_visible(&self, node_id: &NodeId, node_state: &StyledNodeState) -> bool {
        self.get_overflow_y(node_id, node_state).and_then(|p| p.get_property_or_default()).unwrap_or_default().is_overflow_visible()
    }

    pub fn get_text_color_or_default(&self, node_id: &NodeId, node_state: &StyledNodeState) -> StyleTextColor {
        use crate::ui_solver::DEFAULT_TEXT_COLOR;
        self.get_text_color(node_id, node_state).and_then(|fs| fs.get_property().cloned()).unwrap_or(DEFAULT_TEXT_COLOR)
    }

    pub fn get_font_id_or_default(&self, node_id: &NodeId, node_state: &StyledNodeState) -> &str {
        use crate::ui_solver::DEFAULT_FONT_ID;
        let font_id = self.get_font_family(node_id, node_state).and_then(|family| family.get_property()?.fonts.get(0));
        font_id.map(|f| f.as_str()).unwrap_or(DEFAULT_FONT_ID)
    }

    pub fn get_font_size_or_default(&self, node_id: &NodeId, node_state: &StyledNodeState) -> StyleFontSize {
        use crate::ui_solver::DEFAULT_FONT_SIZE;
        self.get_font_size(node_id, node_state).and_then(|fs| fs.get_property().cloned()).unwrap_or(DEFAULT_FONT_SIZE)
    }

    pub fn has_border(&self, node_id: &NodeId, node_state: &StyledNodeState) -> bool {
        self.get_border_left_width(node_id, node_state).is_some() ||
        self.get_border_right_width(node_id, node_state).is_some() ||
        self.get_border_top_width(node_id, node_state).is_some() ||
        self.get_border_bottom_width(node_id, node_state).is_some()
    }

    pub fn has_box_shadow(&self, node_id: &NodeId, node_state: &StyledNodeState) -> bool {
        self.get_box_shadow_left(node_id, node_state).is_some() ||
        self.get_box_shadow_right(node_id, node_state).is_some() ||
        self.get_box_shadow_top(node_id, node_state).is_some() ||
        self.get_box_shadow_bottom(node_id, node_state).is_some()
    }

    pub fn get_background(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBackgroundContentVecValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Background, as_background)
    }
    pub fn get_background_position(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBackgroundPositionVecValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BackgroundPosition, as_background_position)
    }
    pub fn get_background_size(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBackgroundSizeVecValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BackgroundSize, as_background_size)
    }
    pub fn get_background_repeat(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBackgroundRepeatVecValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BackgroundRepeat, as_background_repeat)
    }
    pub fn get_font_size(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleFontSizeValue> {
        get_property!(self, node_id, node_state, CssPropertyType::FontSize, as_font_size)
    }
    pub fn get_font_family(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleFontFamilyValue> {
        get_property!(self, node_id, node_state, CssPropertyType::FontFamily, as_font_family)
    }
    pub fn get_text_color(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleTextColorValue> {
        get_property!(self, node_id, node_state, CssPropertyType::TextColor, as_text_color)
    }
    pub fn get_text_align(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleTextAlignmentHorzValue> {
        get_property!(self, node_id, node_state, CssPropertyType::TextAlign, as_text_align)
    }
    pub fn get_line_height(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleLineHeightValue> {
        get_property!(self, node_id, node_state, CssPropertyType::LineHeight, as_line_height)
    }
    pub fn get_letter_spacing(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleLetterSpacingValue> {
        get_property!(self, node_id, node_state, CssPropertyType::LetterSpacing, as_letter_spacing)
    }
    pub fn get_word_spacing(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleWordSpacingValue> {
        get_property!(self, node_id, node_state, CssPropertyType::WordSpacing, as_word_spacing)
    }
    pub fn get_tab_width(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleTabWidthValue> {
        get_property!(self, node_id, node_state, CssPropertyType::TabWidth, as_tab_width)
    }
    pub fn get_cursor(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleCursorValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Cursor, as_cursor)
    }
    pub fn get_box_shadow_left(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBoxShadowValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BoxShadowLeft, as_box_shadow_left)
    }
    pub fn get_box_shadow_right(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBoxShadowValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BoxShadowRight, as_box_shadow_right)
    }
    pub fn get_box_shadow_top(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBoxShadowValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BoxShadowTop, as_box_shadow_top)
    }
    pub fn get_box_shadow_bottom(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBoxShadowValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BoxShadowBottom, as_box_shadow_bottom)
    }
    pub fn get_border_top_color(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderTopColorValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderTopColor, as_border_top_color)
    }
    pub fn get_border_left_color(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderLeftColorValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderLeftColor, as_border_left_color)
    }
    pub fn get_border_right_color(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderRightColorValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderRightColor, as_border_right_color)
    }
    pub fn get_border_bottom_color(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderBottomColorValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderBottomColor, as_border_bottom_color)
    }
    pub fn get_border_top_style(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderTopStyleValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderTopStyle, as_border_top_style)
    }
    pub fn get_border_left_style(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderLeftStyleValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderLeftStyle, as_border_left_style)
    }
    pub fn get_border_right_style(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderRightStyleValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderRightStyle, as_border_right_style)
    }
    pub fn get_border_bottom_style(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderBottomStyleValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderBottomStyle, as_border_bottom_style)
    }
    pub fn get_border_top_left_radius(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderTopLeftRadiusValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderTopLeftRadius, as_border_top_left_radius)
    }
    pub fn get_border_top_right_radius(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderTopRightRadiusValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderTopRightRadius, as_border_top_right_radius)
    }
    pub fn get_border_bottom_left_radius(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderBottomLeftRadiusValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderBottomLeftRadius, as_border_bottom_left_radius)
    }
    pub fn get_border_bottom_right_radius(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBorderBottomRightRadiusValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderBottomRightRadius, as_border_bottom_right_radius)
    }
    pub fn get_opacity(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleOpacityValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Opacity, as_opacity)
    }
    pub fn get_transform(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleTransformVecValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Transform, as_transform)
    }
    pub fn get_transform_origin(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleTransformOriginValue> {
        get_property!(self, node_id, node_state, CssPropertyType::TransformOrigin, as_transform_origin)
    }
    pub fn get_perspective_origin(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StylePerspectiveOriginValue> {
        get_property!(self, node_id, node_state, CssPropertyType::PerspectiveOrigin, as_perspective_origin)
    }
    pub fn get_backface_visibility(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&StyleBackfaceVisibilityValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BackfaceVisibility, as_backface_visibility)
    }
    pub fn get_display(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutDisplayValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Display, as_display)
    }
    pub fn get_float(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutFloatValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Float, as_float)
    }
    pub fn get_box_sizing(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutBoxSizingValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BoxSizing, as_box_sizing)
    }
    pub fn get_width(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutWidthValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Width, as_width)
    }
    pub fn get_height(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutHeightValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Height, as_height)
    }
    pub fn get_min_width(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutMinWidthValue> {
        get_property!(self, node_id, node_state, CssPropertyType::MinWidth, as_min_width)
    }
    pub fn get_min_height(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutMinHeightValue> {
        get_property!(self, node_id, node_state, CssPropertyType::MinHeight, as_min_height)
    }
    pub fn get_max_width(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutMaxWidthValue> {
        get_property!(self, node_id, node_state, CssPropertyType::MaxWidth, as_max_width)
    }
    pub fn get_max_height(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutMaxHeightValue> {
        get_property!(self, node_id, node_state, CssPropertyType::MaxHeight, as_max_height)
    }
    pub fn get_position(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutPositionValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Position, as_position)
    }
    pub fn get_top(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutTopValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Top, as_top)
    }
    pub fn get_bottom(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutBottomValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Bottom, as_bottom)
    }
    pub fn get_right(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutRightValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Right, as_right)
    }
    pub fn get_left(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutLeftValue> {
        get_property!(self, node_id, node_state, CssPropertyType::Left, as_left)
    }
    pub fn get_padding_top(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutPaddingTopValue> {
        get_property!(self, node_id, node_state, CssPropertyType::PaddingTop, as_padding_top)
    }
    pub fn get_padding_bottom(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutPaddingBottomValue> {
        get_property!(self, node_id, node_state, CssPropertyType::PaddingBottom, as_padding_bottom)
    }
    pub fn get_padding_left(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutPaddingLeftValue> {
        get_property!(self, node_id, node_state, CssPropertyType::PaddingLeft, as_padding_left)
    }
    pub fn get_padding_right(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutPaddingRightValue> {
        get_property!(self, node_id, node_state, CssPropertyType::PaddingRight, as_padding_right)
    }
    pub fn get_margin_top(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutMarginTopValue> {
        get_property!(self, node_id, node_state, CssPropertyType::MarginTop, as_margin_top)
    }
    pub fn get_margin_bottom(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutMarginBottomValue> {
        get_property!(self, node_id, node_state, CssPropertyType::MarginBottom, as_margin_bottom)
    }
    pub fn get_margin_left(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutMarginLeftValue> {
        get_property!(self, node_id, node_state, CssPropertyType::MarginLeft, as_margin_left)
    }
    pub fn get_margin_right(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutMarginRightValue> {
        get_property!(self, node_id, node_state, CssPropertyType::MarginRight, as_margin_right)
    }
    pub fn get_border_top_width(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutBorderTopWidthValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderTopWidth, as_border_top_width)
    }
    pub fn get_border_left_width(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutBorderLeftWidthValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderLeftWidth, as_border_left_width)
    }
    pub fn get_border_right_width(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutBorderRightWidthValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderRightWidth, as_border_right_width)
    }
    pub fn get_border_bottom_width(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutBorderBottomWidthValue> {
        get_property!(self, node_id, node_state, CssPropertyType::BorderBottomWidth, as_border_bottom_width)
    }
    pub fn get_overflow_x(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutOverflowValue> {
        get_property!(self, node_id, node_state, CssPropertyType::OverflowX, as_overflow_x)
    }
    pub fn get_overflow_y(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutOverflowValue> {
        get_property!(self, node_id, node_state, CssPropertyType::OverflowY, as_overflow_y)
    }
    pub fn get_flex_direction(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutFlexDirectionValue> {
        get_property!(self, node_id, node_state, CssPropertyType::FlexDirection, as_direction)
    }
    pub fn get_flex_wrap(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutFlexWrapValue> {
        get_property!(self, node_id, node_state, CssPropertyType::FlexWrap, as_flex_wrap)
    }
    pub fn get_flex_grow(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutFlexGrowValue> {
        get_property!(self, node_id, node_state, CssPropertyType::FlexGrow, as_flex_grow)
    }
    pub fn get_flex_shrink(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutFlexShrinkValue> {
        get_property!(self, node_id, node_state, CssPropertyType::FlexShrink, as_flex_shrink)
    }
    pub fn get_justify_content(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutJustifyContentValue> {
        get_property!(self, node_id, node_state, CssPropertyType::JustifyContent, as_justify_content)
    }
    pub fn get_align_items(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutAlignItemsValue> {
        get_property!(self, node_id, node_state, CssPropertyType::AlignItems, as_align_items)
    }
    pub fn get_align_content(&self, node_id: &NodeId, node_state: &StyledNodeState) -> Option<&LayoutAlignContentValue> {
        get_property!(self, node_id, node_state, CssPropertyType::AlignContent, as_align_content)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct DomId { pub inner: usize }

impl DomId {
    pub const ROOT_ID: DomId = DomId { inner: 0 };
}

impl Default for DomId {
    fn default() -> DomId { DomId::ROOT_ID }
}

impl_option!(DomId, OptionDomId, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);



#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AzNodeId { pub inner: usize }

impl fmt::Display for AzNodeId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl AzNodeId {
    pub const NONE: AzNodeId = AzNodeId { inner: 0 };
}

impl_option!(AzNodeId, OptionNodeId, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl_vec!(AzNodeId, NodeIdVec, NodeIdVecDestructor);
impl_vec_debug!(AzNodeId, NodeIdVec);
impl_vec_partialord!(AzNodeId, NodeIdVec);
impl_vec_clone!(AzNodeId, NodeIdVec, NodeIdVecDestructor);
impl_vec_partialeq!(AzNodeId, NodeIdVec);

impl AzNodeId {
    #[inline]
    pub const fn into_crate_internal(&self) -> Option<NodeId> {
        NodeId::from_usize(self.inner)
    }

    #[inline]
    pub const fn from_crate_internal(t: Option<NodeId>) -> Self {
        Self { inner: NodeId::into_usize(&t) }
    }
}



#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AzTagId { pub inner: u64 }

impl_option!(AzTagId, OptionTagId, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl AzTagId {
    pub const fn into_crate_internal(&self) -> TagId { TagId(self.inner) }
    pub const fn from_crate_internal(t: TagId) -> Self { AzTagId { inner: t.0 } }
}



#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AzNode {
    pub parent: usize,
    pub previous_sibling: usize,
    pub next_sibling: usize,
    pub last_child: usize,
}

impl From<Node> for AzNode {
    fn from(node: Node) -> AzNode {
        AzNode {
            parent: NodeId::into_usize(&node.parent),
            previous_sibling: NodeId::into_usize(&node.previous_sibling),
            next_sibling: NodeId::into_usize(&node.next_sibling),
            last_child: NodeId::into_usize(&node.last_child),
        }
    }
}

impl AzNode {
    pub fn parent_id(&self) -> Option<NodeId> { NodeId::from_usize(self.parent) }
    pub fn previous_sibling_id(&self) -> Option<NodeId> { NodeId::from_usize(self.previous_sibling) }
    pub fn next_sibling_id(&self) -> Option<NodeId> { NodeId::from_usize(self.next_sibling) }
    pub fn first_child_id(&self) -> Option<NodeId> { self.last_child_id().and_then(|_| Some(self.parent_id()? + 1)) }
    pub fn last_child_id(&self) -> Option<NodeId> { NodeId::from_usize(self.last_child) }
}

impl_vec!(AzNode, AzNodeVec, AzNodeVecDestructor);
impl_vec_mut!(AzNode, AzNodeVec);
impl_vec_debug!(AzNode, AzNodeVec);
impl_vec_partialord!(AzNode, AzNodeVec);
impl_vec_clone!(AzNode, AzNodeVec, AzNodeVecDestructor);
impl_vec_partialeq!(AzNode, AzNodeVec);

impl AzNodeVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, AzNode> {
        NodeDataContainerRef { internal: self.as_ref() }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, AzNode> {
        NodeDataContainerRefMut { internal: self.as_mut() }
    }
}



#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct ParentWithNodeDepth {
    pub depth: usize,
    pub node_id: AzNodeId,
}

impl_vec!(ParentWithNodeDepth, ParentWithNodeDepthVec, ParentWithNodeDepthVecDestructor);
impl_vec_mut!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_debug!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_partialord!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_clone!(ParentWithNodeDepth, ParentWithNodeDepthVec, ParentWithNodeDepthVecDestructor);
impl_vec_partialeq!(ParentWithNodeDepth, ParentWithNodeDepthVec);



#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
#[repr(C)]
pub struct TagIdToNodeIdMapping {
    // Hit-testing tag
    pub tag_id: AzTagId,
    pub node_id: AzNodeId,
    pub tab_index: OptionTabIndex,
}

impl_vec!(TagIdToNodeIdMapping, TagIdsToNodeIdsMappingVec, TagIdToNodeIdMappingVecDestructor);
impl_vec_mut!(TagIdToNodeIdMapping, TagIdsToNodeIdsMappingVec);
impl_vec_debug!(TagIdToNodeIdMapping, TagIdsToNodeIdsMappingVec);
impl_vec_partialord!(TagIdToNodeIdMapping, TagIdsToNodeIdsMappingVec);
impl_vec_clone!(TagIdToNodeIdMapping, TagIdsToNodeIdsMappingVec, TagIdToNodeIdMappingVecDestructor);
impl_vec_partialeq!(TagIdToNodeIdMapping, TagIdsToNodeIdsMappingVec);



#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ContentGroup {
    /// The parent of the current node group, i.e. either the root node (0)
    /// or the last positioned node ()
    pub root: AzNodeId,
    /// Node ids in order of drawing
    pub children: ContentGroupVec,
}

impl_vec!(ContentGroup, ContentGroupVec, ContentGroupVecDestructor);
impl_vec_mut!(ContentGroup, ContentGroupVec);
impl_vec_debug!(ContentGroup, ContentGroupVec);
impl_vec_partialord!(ContentGroup, ContentGroupVec);
impl_vec_clone!(ContentGroup, ContentGroupVec, ContentGroupVecDestructor);
impl_vec_partialeq!(ContentGroup, ContentGroupVec);



#[derive(Debug, PartialEq)]
#[repr(C)]
pub struct StyledDom {
    pub root: AzNodeId,
    pub node_hierarchy: AzNodeVec,
    pub node_data: NodeDataVec,
    pub styled_nodes: StyledNodeVec,
    pub cascade_info: CascadeInfoVec,
    pub tag_ids_to_node_ids: TagIdsToNodeIdsMappingVec,
    pub non_leaf_nodes: ParentWithNodeDepthVec,
    pub css_property_cache: CssPropertyCachePtr,
}

impl Default for StyledDom {
    fn default() -> Self {
        StyledDom::new(Dom::body(), Css::empty())
    }
}

macro_rules! diff_properties {($self_val:expr, $old_properties:expr, $new_properties:expr, $node_id:expr, $current_node_state:expr, $new_node_state:expr) => {{
    if $new_properties.is_empty() && $old_properties.is_empty() {
        None
    } else if $new_properties.is_empty() {
        // all old_properties removed
        Some((*$node_id, $old_properties.into_iter().map(|(key, value)| {
            ChangedCssProperty {
                previous_state: $current_node_state.clone(),
                current_state: $new_node_state.clone(),
                previous_prop: value,
                current_prop: CssProperty::none(key),
            }
        }).collect()))
    } else if $old_properties.is_empty() {
        // all new_properties added
        Some((*$node_id, $new_properties.into_iter().map(|(key, value)| {
            ChangedCssProperty {
                previous_state: $current_node_state.clone(),
                current_state: $new_node_state.clone(),
                previous_prop: CssProperty::none(key),
                current_prop: value,
            }
        }).collect()))
    } else {
        // mix between the two
        let mut changed_properties = Vec::new();

        for (old_key, old_value) in $old_properties.iter() {
            let new_prop = $new_properties.get(&old_key).cloned().unwrap_or(CssProperty::none(*old_key));
            if *old_value != new_prop {
                changed_properties.push(ChangedCssProperty {
                    previous_state: $current_node_state.clone(),
                    current_state: $new_node_state.clone(),
                    previous_prop: old_value.clone(),
                    current_prop: new_prop,
                });
            }
        }

        for (new_key, new_value) in $new_properties.iter() {
            let old_prop = $old_properties.get(&new_key).cloned().unwrap_or(CssProperty::none(*new_key));
            if *new_value != old_prop {
                changed_properties.push(ChangedCssProperty {
                    previous_state: $current_node_state.clone(),
                    current_state: $new_node_state.clone(),
                    previous_prop: old_prop,
                    current_prop: new_value.clone(),
                });
            }
        }

        if !changed_properties.is_empty() {
            Some((*$node_id, changed_properties))
        } else {
            None
        }
    }
}};}

macro_rules! restyle_nodes {($self_val:expr, $field:ident, $new_field_state:expr, $inline_props_field:ident, $css_props_field:ident, $nodes:expr) => {{
    use rayon::prelude::*;

    let ret = $nodes
    .par_iter()
    .filter_map(|node_id| {

        let current_node_state = $self_val.styled_nodes.as_container()[*node_id].state.clone();
        let mut new_node_state = current_node_state.clone();
        new_node_state.$field = $new_field_state;

        if current_node_state == new_node_state {
            return None; // state is the same, no changes
        }

        let mut old_properties = BTreeMap::new();
        let default_map = BTreeMap::new();

        if current_node_state.$field {
            for (prop_key, prop_value) in $self_val.get_css_property_cache().$css_props_field.get(node_id).unwrap_or(&default_map).iter() {
                old_properties.insert(*prop_key, prop_value.clone());
            }
            for (prop_key, prop_value) in $self_val.get_css_property_cache().$inline_props_field.get(node_id).unwrap_or(&default_map).iter() {
                old_properties.insert(*prop_key, prop_value.clone());
            }
        }

        let mut new_properties = BTreeMap::new();

        if new_node_state.$field {
            for (prop_key, prop_value) in $self_val.get_css_property_cache().$css_props_field.get(node_id).unwrap_or(&default_map).iter() {
                new_properties.insert(*prop_key, prop_value.clone());
            }
            for (prop_key, prop_value) in $self_val.get_css_property_cache().$inline_props_field.get(node_id).unwrap_or(&default_map).iter() {
                new_properties.insert(*prop_key, prop_value.clone());
            }
        }

        diff_properties!($self_val, old_properties, new_properties, node_id, current_node_state, new_node_state)
    }).collect::<Vec<_>>();

    for node_id in $nodes {
        $self_val.styled_nodes.as_container_mut()[*node_id].state.$field = $new_field_state;
    }

    ret.into_iter().collect()
}};}

impl StyledDom {

    pub fn new(dom: Dom, css: Css) -> Self {

        use azul_css::CssDeclaration;
        use crate::dom::{TabIndex, NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec};
        use std::iter::FromIterator;
        use rayon::prelude::*;

        let mut compact_dom: CompactDom = dom.into();
        let non_leaf_nodes = compact_dom.node_hierarchy.as_ref().get_parents_sorted_by_depth();
        let node_hierarchy: AzNodeVec = compact_dom.node_hierarchy.internal.clone().iter().map(|i| (*i).into()).collect::<Vec<AzNode>>().into();
        let mut styled_nodes = vec![StyledNode { tag_id: OptionTagId::None, state: StyledNodeState::new() }; compact_dom.len()];

        // fill out the css property cache: compute the inline properties first so that
        // we can early-return in case the css is empty

        let inline_normal_props = compact_dom.node_data.as_ref_mut().transform_multithread_optional(|node_data, node_id| {
            let normal_inline_props = node_data.inline_css_props
                .as_ref()
                .par_iter()
                .filter_map(|css_prop| match css_prop { NodeDataInlineCssProperty::Normal(p) => Some((p.get_type(), p.clone())), _ => None })
                .collect::<Vec<(_, _)>>();

            if normal_inline_props.is_empty() {
                None
            } else {
                let map = BTreeMap::from_iter(normal_inline_props.into_iter());
                Some((node_id, map))
            }
        });

        let inline_hover_props = compact_dom.node_data.as_ref_mut().transform_multithread_optional(|node_data, node_id| {
            let hover_inline_props = node_data.inline_css_props
                .as_ref()
                .par_iter()
                .filter_map(|css_prop| match css_prop { NodeDataInlineCssProperty::Hover(p) => Some((p.get_type(), p.clone())), _ => None })
                .collect::<Vec<(_, _)>>();

            if hover_inline_props.is_empty() {
                None
            } else {
                let map = BTreeMap::from_iter(hover_inline_props.into_iter());
                Some((node_id, map))
            }
        });

        let inline_active_props = compact_dom.node_data.as_ref_mut().transform_multithread_optional(|node_data, node_id| {
            let active_inline_props = node_data.inline_css_props
                .as_ref()
                .par_iter()
                .filter_map(|css_prop| match css_prop { NodeDataInlineCssProperty::Active(p) => Some((p.get_type(), p.clone())), _ => None })
                .collect::<Vec<(_, _)>>();

            if active_inline_props.is_empty() {
                None
            } else {
                let map = BTreeMap::from_iter(active_inline_props.into_iter());
                Some((node_id, map))
            }
        });

        let inline_focus_props = compact_dom.node_data.as_ref_mut().transform_multithread_optional(|node_data, node_id| {
            let focus_inline_props = node_data.inline_css_props
                .as_ref()
                .par_iter()
                .filter_map(|css_prop| match css_prop { NodeDataInlineCssProperty::Focus(p) => Some((p.get_type(), p.clone())), _ => None })
                .collect::<Vec<(_, _)>>();

            node_data.inline_css_props = NodeDataInlineCssPropertyVec::new(); // no need to retain the inline CSS properties in the DOM, clear here

            if focus_inline_props.is_empty() {
                None
            } else {
                let map = BTreeMap::from_iter(focus_inline_props.into_iter());
                Some((node_id, map))
            }
        });

        let mut css_property_cache = CssPropertyCache {
            non_default_inline_normal_props: inline_normal_props.into_iter().collect(),
            non_default_inline_hover_props: inline_hover_props.into_iter().collect(),
            non_default_inline_active_props: inline_active_props.into_iter().collect(),
            non_default_inline_focus_props: inline_focus_props.into_iter().collect(),
            .. Default::default()
        };

        let html_tree = construct_html_cascade_tree(&compact_dom.node_hierarchy.as_ref(), &non_leaf_nodes[..]);

        let css_is_empty = css.is_empty();

        // apply all the styles from the CSS
        if !css_is_empty {

            use azul_css::{CssPathSelector, CssPathPseudoSelector};

            let css = css.sort_by_specificity();

            let node_hierarchy_ref = node_hierarchy.as_container();
            let node_data = compact_dom.node_data.as_ref();
            let html_tree = html_tree.as_ref();

            macro_rules! filter_rules {($styled_node_state:expr, $node_id:expr) => {{
                css
                .rules() // can not be parallelized due to specificity order matching
                .filter(|rule_block| rule_ends_with(&rule_block.path, $styled_node_state))
                .filter(|rule_block| matches_html_element(&rule_block.path, $node_id, &node_hierarchy_ref, &node_data, &html_tree))
                // rule matched, now copy all the styles of this rule
                .flat_map(|matched_rule| {
                    matched_rule.declarations
                    .iter()
                    .filter_map(move |declaration| {
                        match declaration {
                            CssDeclaration::Static(s) => Some(s),
                            CssDeclaration::Dynamic(_d) => None, // TODO: No variable support yet!
                        }
                    })
                })
                .map(|prop| (prop.get_type(), prop.clone()))
                .collect::<BTreeMap<_, _>>()
            }};}

            // NOTE: This is wrong, but fast
            //
            // Get all nodes that end with `:hover`, `:focus` or `:active`
            // and copy the respective styles to the `hover_css_constraints`, etc. respectively
            //
            // NOTE: This won't work correctly for paths with `.blah:hover > #thing`
            // but that can be fixed later

            // go through each HTML node (in parallel) and see which CSS rules match
            let css_normal_rules = compact_dom.node_data.as_ref().transform_nodeid_multithreaded_optional(|node_id| {
                let matched_rules = filter_rules!(None, node_id);

                if matched_rules.is_empty() {
                    None
                } else {
                    Some((node_id, matched_rules))
                }
            });

            let css_hover_rules = compact_dom.node_data.as_ref().transform_nodeid_multithreaded_optional(|node_id| {
                let matched_rules = filter_rules!(Some(CssPathSelector::PseudoSelector(CssPathPseudoSelector::Hover)), node_id);

                if matched_rules.is_empty() {
                    None
                } else {
                    Some((node_id, matched_rules))
                }
            });

            let css_active_rules = compact_dom.node_data.as_ref().transform_nodeid_multithreaded_optional(|node_id| {
                let matched_rules = filter_rules!(Some(CssPathSelector::PseudoSelector(CssPathPseudoSelector::Active)), node_id);

                if matched_rules.is_empty() {
                    None
                } else {
                    Some((node_id, matched_rules))
                }
            });

            let css_focus_rules = compact_dom.node_data.as_ref().transform_nodeid_multithreaded_optional(|node_id| {
                let matched_rules = filter_rules!(Some(CssPathSelector::PseudoSelector(CssPathPseudoSelector::Focus)), node_id);

                if matched_rules.is_empty() {
                    None
                } else {
                    Some((node_id, matched_rules))
                }
            });

            css_property_cache.non_default_css_normal_props = css_normal_rules.internal.into_iter().collect::<BTreeMap<_, _>>();
            css_property_cache.non_default_css_hover_props = css_hover_rules.internal.into_iter().collect::<BTreeMap<_, _>>();
            css_property_cache.non_default_css_active_props = css_active_rules.internal.into_iter().collect::<BTreeMap<_, _>>();
            css_property_cache.non_default_css_focus_props = css_focus_rules.internal.into_iter().collect::<BTreeMap<_, _>>();
        }

        // Inheritance: Inherit all values of the parent to the children, but
        // only if the property is inheritable and isn't yet set
        for (_depth, parent_id) in non_leaf_nodes.iter() {

            macro_rules! inherit_props {($inherit_map:expr) => {
                let parent_inheritable_css_props =  {

                    let parent_css_props = match $inherit_map.get(parent_id) {
                        Some(s) => s,
                        None => continue,
                    };

                    parent_css_props
                    .iter()
                    .filter(|(key, _)| key.is_inheritable())
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<Vec<(_, _)>>()
                };

                if parent_inheritable_css_props.is_empty() {
                    continue;
                }

                // only override the rule if the child already has an inherited rule
                for child_id in parent_id.az_children(&node_hierarchy.as_container()) {
                    for (inherited_rule_key, inherited_rule_value) in parent_inheritable_css_props.iter() {
                        $inherit_map.entry(child_id)
                        .or_insert_with(|| BTreeMap::new())
                        .entry(*inherited_rule_key)
                        .or_insert_with(|| inherited_rule_value.clone());
                    }
                }
            };}

            if !css_is_empty {
                inherit_props!(css_property_cache.non_default_css_normal_props);
                inherit_props!(css_property_cache.non_default_css_hover_props);
                inherit_props!(css_property_cache.non_default_css_active_props);
                inherit_props!(css_property_cache.non_default_css_focus_props);
            }

            // also inherit from the inline css props
            inherit_props!(css_property_cache.non_default_inline_normal_props);
            inherit_props!(css_property_cache.non_default_inline_hover_props);
            inherit_props!(css_property_cache.non_default_inline_active_props);
            inherit_props!(css_property_cache.non_default_inline_focus_props);
        }

        // CSS property cache is now built


        // In order to hit-test `:hover` and `:active` selectors, need to insert tags for all rectangles
        // that have a non-:hover path, for example if we have `#thing:hover`, then all nodes selected by `#thing`
        // need to get a TagId, otherwise, they can't be hit-tested.

        // See if the node should have a hit-testing tag ID
        let default_node_state = StyledNodeState::default();
        let tag_ids = compact_dom.node_data
            .as_ref()
            .internal
            .par_iter()
            .enumerate()
            .filter_map(|(node_id, node_data)| {
            let node_id = NodeId::new(node_id);

            let should_auto_insert_tabindex = node_data.get_callbacks().iter().any(|cb| cb.event.is_focus_callback());
            let tab_index = match node_data.get_tab_index().into_option() {
                Some(s) => Some(s),
                None => if should_auto_insert_tabindex { Some(TabIndex::Auto) } else { None }
            };

            let node_has_focus_props = css_property_cache.non_default_inline_focus_props.get(&node_id).is_some() ||  css_property_cache.non_default_css_focus_props.get(&node_id).is_some();
            let node_has_hover_props = css_property_cache.non_default_inline_hover_props.get(&node_id).is_some() ||  css_property_cache.non_default_css_hover_props.get(&node_id).is_some();
            let node_has_active_props = css_property_cache.non_default_inline_active_props.get(&node_id).is_some() ||  css_property_cache.non_default_css_active_props.get(&node_id).is_some();
            let node_has_not_only_window_callbacks = !node_data.get_callbacks().is_empty() && !node_data.get_callbacks().iter().all(|cb| cb.event.is_window_callback());
            let node_has_non_default_cursor = css_property_cache.get_cursor(&node_id, &default_node_state).is_some();

            let node_should_have_tag =
                tab_index.is_some() ||
                node_has_hover_props ||
                node_has_focus_props ||
                node_has_active_props ||
                node_has_not_only_window_callbacks ||
                node_has_non_default_cursor;

            if node_should_have_tag {
                let tag_id = TagId::new();
                let az_tag_id = AzTagId::from_crate_internal(tag_id);
                Some(TagIdToNodeIdMapping {
                    tag_id: az_tag_id,
                    node_id: AzNodeId::from_crate_internal(Some(node_id)),
                    tab_index: tab_index.into(),
                })
            } else {
                None
            }
        }).collect::<Vec<_>>();

        for tag_id_node_id_mapping in tag_ids.iter() {
            if let Some(nid) = tag_id_node_id_mapping.node_id.into_crate_internal() {
                styled_nodes[nid.index()].tag_id = OptionTagId::Some(tag_id_node_id_mapping.tag_id);
            }
        }

        let non_leaf_nodes = non_leaf_nodes
        .par_iter()
        .map(|(depth, node_id)| ParentWithNodeDepth { depth: *depth, node_id: AzNodeId::from_crate_internal(Some(*node_id)) })
        .collect::<Vec<_>>();

        StyledDom {
            root: AzNodeId::from_crate_internal(Some(compact_dom.root)),
            node_hierarchy,
            node_data: compact_dom.node_data.internal.into(),
            cascade_info: html_tree.internal.into(),
            styled_nodes: styled_nodes.into(),
            tag_ids_to_node_ids: tag_ids.into(),
            non_leaf_nodes: non_leaf_nodes.into(),
            css_property_cache: CssPropertyCachePtr::new(css_property_cache),
        }
    }

    pub fn node_count(&self) -> usize {
        self.node_data.len()
    }

    #[inline]
    pub fn get_css_property_cache<'a>(&'a self) -> &'a CssPropertyCache {
        unsafe { &*self.css_property_cache.ptr }
    }

    #[inline]
    pub fn get_css_property_cache_mut<'a>(&'a mut self) -> &'a mut CssPropertyCache {
        unsafe { &mut *self.css_property_cache.ptr }
    }

    // swap the internal css property cache with a default cache, moving the self.cache out
    #[inline]
    pub fn get_css_property_cache_move(&mut self) -> CssPropertyCache {
        let mut default_cache = CssPropertyCache::default();
        std::mem::swap(self.get_css_property_cache_mut(), &mut default_cache);
        default_cache
    }

    /// Appends another `StyledDom` to the `self.root` without re-styling the DOM itself
    pub fn append(&mut self, mut other: Self) {

        // shift all the node ids in other by self.len()
        let self_len = self.node_hierarchy.as_ref().len();
        let self_tag_len = self.tag_ids_to_node_ids.as_ref().len();

        let self_root_id = self.root.into_crate_internal().unwrap_or(NodeId::ZERO);
        let last_child_id = self.node_hierarchy.as_container()[self_root_id].last_child_id().unwrap_or(NodeId::ZERO);
        let other_root_id = other.root.into_crate_internal().unwrap_or(NodeId::ZERO);

        self.node_hierarchy.as_container_mut()[self_root_id].last_child += other.root.inner;

        for node in other.node_hierarchy.as_mut().iter_mut() {
            node.parent += self_len;
            node.previous_sibling += self_len;
            node.next_sibling += self_len;
            node.last_child += self_len;
        }

        other.node_hierarchy.as_container_mut()[other_root_id].parent = NodeId::into_usize(&Some(self.root.into_crate_internal().unwrap_or(NodeId::ZERO)));
        self.node_hierarchy.as_container_mut()[last_child_id].next_sibling = self_len + other.root.inner;
        other.node_hierarchy.as_container_mut()[other_root_id].previous_sibling = NodeId::into_usize(&Some(last_child_id));

        self.node_hierarchy.append(&mut other.node_hierarchy);
        self.node_data.append(&mut other.node_data);
        self.styled_nodes.append(&mut other.styled_nodes);
        self.get_css_property_cache_mut().append(other.get_css_property_cache_move(), self_len);

        for tag_id_node_id in other.tag_ids_to_node_ids.iter_mut() {
            tag_id_node_id.tag_id.inner += self_tag_len as u64;
            tag_id_node_id.node_id.inner += self_len;
        }

        self.tag_ids_to_node_ids.append(&mut other.tag_ids_to_node_ids);

        for other_non_leaf_node in other.non_leaf_nodes.iter_mut() {
            other_non_leaf_node.node_id.inner += self_len;
            other_non_leaf_node.depth += 1;
        }

        self.non_leaf_nodes.append(&mut other.non_leaf_nodes);
        self.non_leaf_nodes.sort_by(|a, b| a.depth.cmp(&b.depth));
    }

    pub fn get_styled_node_state(&self, node_id: &NodeId) -> StyledNodeState {
        self.styled_nodes.as_container()[*node_id].state.clone()
    }

    /// Scans the display list for all font IDs + their font size
    pub(crate) fn scan_for_font_keys(&self, app_resources: &AppResources) -> FastHashMap<ImmediateFontId, FastHashSet<Au>> {

        use crate::dom::NodeType::*;
        use crate::app_resources::font_size_to_au;
        use rayon::prelude::*;

        let keys = self.node_data
            .as_ref()
            .par_iter()
            .enumerate()
            .filter_map(|(node_id, node_data)| {
                let node_id = NodeId::new(node_id);
                match node_data.get_node_type() {
                    Label(_) => {
                        let css_font_id = self.get_css_property_cache().get_font_id_or_default(&node_id, &self.styled_nodes.as_container()[node_id].state);
                        let font_size = self.get_css_property_cache().get_font_size_or_default(&node_id, &self.styled_nodes.as_container()[node_id].state);
                        let font_id = match app_resources.css_ids_to_font_ids.get(css_font_id) {
                            Some(s) => ImmediateFontId::Resolved(*s),
                            None => ImmediateFontId::Unresolved(css_font_id.to_string()),
                        };
                        Some((font_id, font_size_to_au(font_size)))
                    },
                    _ => None
                }
            })
            .collect::<Vec<_>>();

        let mut map = FastHashMap::default();
        for (font_id, au) in keys.into_iter() {
            map.entry(font_id).or_insert_with(|| FastHashSet::default()).insert(au);
        }
        map
    }

    /// Scans the display list for all image keys
    pub(crate) fn scan_for_image_keys(&self, app_resources: &AppResources) -> FastHashSet<ImageId> {

        use crate::dom::NodeType::*;
        use crate::dom::OptionImageMask;
        use azul_css::StyleBackgroundContentVec;

        #[derive(Default)]
        struct ScanImageVec {
            node_type_image: Option<ImageId>,
            background_image: Vec<ImageId>,
            clip_mask: Option<ImageId>,
        }

        use rayon::prelude::*;

        let default_backgrounds: StyleBackgroundContentVec = Vec::new().into();

        let images = self.node_data.as_container().internal
        .par_iter()
        .enumerate()
        .map(|(node_id, node_data)| {

            let node_id = NodeId::new(node_id);
            let mut v = ScanImageVec::default();

            // If the node has an image content, it needs to be uploaded
            if let Image(id) = node_data.get_node_type(){
                v.node_type_image = Some(*id);
            }

            // If the node has a CSS background image, it needs to be uploaded
            if let Some(style_backgrounds) = self.get_css_property_cache().get_background(&node_id, &self.styled_nodes.as_container()[node_id].state) {
                v.background_image = style_backgrounds.get_property().unwrap_or(&default_backgrounds).iter().filter_map(|bg| {
                    let css_image_id = bg.get_css_image_id()?;
                    let image_id = app_resources.get_css_image_id(css_image_id.inner.as_str())?;
                    Some(*image_id)
                }).collect();
            }

            // If the node has a clip mask, it needs to be uploaded
            if let OptionImageMask::Some(clip_mask) = node_data.get_clip_mask() {
                v.clip_mask = Some(clip_mask.image);
            }

            v
        }).collect::<Vec<_>>();

        let mut set = FastHashSet::new();

        for scan_image in images.into_iter() {
            if let Some(n) = scan_image.node_type_image { set.insert(n); }
            if let Some(n) = scan_image.clip_mask { set.insert(n); }
            for bg in scan_image.background_image { set.insert(bg); }
        }

        set
    }

    pub fn restyle_nodes_hover_noreturn(&mut self, nodes: &[NodeId], new_hover_state: bool) {
        for node_id in nodes {
            self.styled_nodes.as_container_mut()[*node_id].state.hover = new_hover_state;
        }
    }

    pub fn restyle_nodes_active_noreturn(&mut self, nodes: &[NodeId], new_active_state: bool) {
        for node_id in nodes {
            self.styled_nodes.as_container_mut()[*node_id].state.active = new_active_state;
        }
    }

    pub fn restyle_nodes_focus_noreturn(&mut self, nodes: &[NodeId], new_focus_state: bool) {
        for node_id in nodes {
            self.styled_nodes.as_container_mut()[*node_id].state.focused = new_focus_state;
        }
    }

    #[must_use]
    pub fn restyle_nodes_hover(&mut self, nodes: &[NodeId], new_hover_state: bool) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        restyle_nodes!(self, hover, new_hover_state, non_default_inline_hover_props, non_default_css_hover_props, nodes)
    }

    #[must_use]
    pub fn restyle_nodes_active(&mut self, nodes: &[NodeId], new_active_state: bool) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        restyle_nodes!(self, active, new_active_state, non_default_inline_active_props, non_default_css_active_props, nodes)
    }

    #[must_use]
    pub fn restyle_nodes_focus(&mut self, nodes: &[NodeId], new_focus_state: bool) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        restyle_nodes!(self, focused, new_focus_state, non_default_inline_focus_props, non_default_css_focus_props, nodes)
    }

    #[must_use]
    pub fn restyle_inline_normal_props(&mut self, node_id: &NodeId, new_properties: &[CssProperty]) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        // exchange the inline properties for the node n with the new properties
        let mut old_properties = BTreeMap::new();
        let default_map = BTreeMap::new();

        for (prop_key, prop_value) in self.get_css_property_cache().non_default_css_normal_props.get(node_id).unwrap_or(&default_map).iter() {
            old_properties.insert(*prop_key, prop_value.clone());
        }
        for (prop_key, prop_value) in self.get_css_property_cache().non_default_inline_normal_props.get(node_id).unwrap_or(&default_map).iter() {
            old_properties.insert(*prop_key, prop_value.clone());
        }

        let new_properties: BTreeMap<_, _> = new_properties.iter().map(|c| (c.get_type(), c.clone())).collect();

        let current_node_state = self.styled_nodes.as_container()[*node_id].state.clone();
        let new_node_state = current_node_state.clone();
        match diff_properties!(self, new_properties, old_properties, node_id, current_node_state, new_node_state) {
            Some((node_id, prop_vec)) => {
                let mut map = BTreeMap::default();
                map.insert(node_id, prop_vec);
                map
            },
            None => BTreeMap::default(),
        }
    }

    /// Scans the `StyledDom` for iframe callbacks
    pub fn scan_for_iframe_callbacks(&self) -> Vec<NodeId> {
        use rayon::prelude::*;
        use crate::dom::NodeType;
        self.node_data
        .as_ref()
        .par_iter()
        .enumerate()
        .filter_map(|(node_id, node_data)| {
            match node_data.get_node_type() {
                NodeType::IFrame(_) => Some(NodeId::new(node_id)),
                _ => None,
            }
        }).collect()
    }

    /// Scans the `StyledDom` for OpenGL callbacks
    #[cfg(feature = "opengl")]
    pub(crate) fn scan_for_gltexture_callbacks(&self) -> Vec<NodeId> {
        use rayon::prelude::*;
        use crate::dom::NodeType;
        self.node_data
        .as_ref()
        .par_iter()
        .enumerate()
        .filter_map(|(node_id, node_data)| {
            match node_data.get_node_type() {
                NodeType::GlTexture(_) => Some(NodeId::new(node_id)),
                _ => None,
            }
        }).collect()
    }


    /// Returns a HTML-formatted version of the DOM for easier debugging, i.e.
    ///
    /// ```rust,no_run,ignore
    /// Dom::div().with_id("hello")
    ///     .with_child(Dom::div().with_id("test"))
    /// ```
    ///
    /// will return:
    ///
    /// ```xml,no_run,ignore
    /// <div id="hello">
    ///      <div id="test" />
    /// </div>
    /// ```
    pub fn get_html_string(&self) -> String {
        // TODO: This is wrong!

        let mut output = String::new();

        for ParentWithNodeDepth { depth, node_id } in self.non_leaf_nodes.iter() {

            let node_id = match node_id.into_crate_internal() {
                Some(s) => s,
                None => continue,
            };
            let node_data = &self.node_data.as_container()[node_id];
            let tabs = String::from("    ").repeat(*depth);
            let node_has_children = self.node_hierarchy.as_container()[node_id].first_child_id().is_some();

            output.push_str("\r\n");
            output.push_str(&tabs);
            output.push_str(&node_data.debug_print_start(node_has_children));

            if let Some(content) = node_data.get_node_type().get_text_content().as_ref() {
                output.push_str(content);
            }

            for child_id in node_id.az_children(&self.node_hierarchy.as_container()) {

                let node_data = &self.node_data.as_container()[child_id];
                let node_has_children = self.node_hierarchy.as_container()[child_id].first_child_id().is_some();
                let tabs = String::from("    ").repeat(*depth + 1);

                output.push_str("\r\n");
                output.push_str(&tabs);
                output.push_str(&node_data.debug_print_start(node_has_children));

                let content = node_data.get_node_type().get_text_content();
                if let Some(content) = content.as_ref() {
                    output.push_str(content);
                }

                if node_has_children || content.is_some() {
                    output.push_str(&node_data.debug_print_end());
                }
            }

            if node_has_children {
                output.push_str("\r\n");
                output.push_str(&tabs);
                output.push_str(&node_data.debug_print_end());
            }
        }

        output.trim().to_string()
    }

    pub fn get_rects_in_rendering_order(&self) -> ContentGroup {
        Self::determine_rendering_order(
            &self.non_leaf_nodes.as_ref(),
            &self.node_hierarchy.as_container(),
            &self.styled_nodes.as_container(),
            &self.get_css_property_cache()
        )
    }

    /// Returns the rendering order of the items (the rendering order doesn't have to be the original order)
    fn determine_rendering_order<'a>(
        non_leaf_nodes: &[ParentWithNodeDepth],
        node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
        styled_nodes: &NodeDataContainerRef<StyledNode>,
        css_property_cache: &CssPropertyCache,
    ) -> ContentGroup {
        use rayon::prelude::*;

        fn fill_content_group_children(group: &mut ContentGroup, children_sorted: &BTreeMap<AzNodeId, Vec<AzNodeId>>) {
            use rayon::prelude::*;

            if let Some(c) = children_sorted.get(&group.root) { // returns None for leaf nodes
                group.children = c
                    .par_iter()
                    .map(|child| ContentGroup { root: *child, children: Vec::new().into() })
                    .collect::<Vec<ContentGroup>>()
                    .into();

                for c in group.children.as_mut() {
                    fill_content_group_children(c, children_sorted);
                }
            }
        }

        fn sort_children_by_position<'a>(
            parent: NodeId,
            node_hierarchy: &NodeDataContainerRef<'a, AzNode>,
            rectangles: &NodeDataContainerRef<StyledNode>,
            css_property_cache: &CssPropertyCache,
        ) -> Vec<AzNodeId> {

            use azul_css::LayoutPosition::*;
            use rayon::prelude::*;

            let children_positions = parent
                .az_children(node_hierarchy)
                .map(|nid| {
                    let position = css_property_cache
                        .get_position(&nid, &rectangles[nid].state)
                        .and_then(|p| p.clone().get_property_or_default())
                        .unwrap_or_default();
                    let id = AzNodeId::from_crate_internal(Some(nid));
                    (id, position)
                })
                .collect::<Vec<_>>();

            let mut not_absolute_children = children_positions
                .par_iter()
                .filter_map(|(node_id, position)| if *position != Absolute { Some(*node_id) } else { None })
                .collect::<Vec<_>>();

            let mut absolute_children = children_positions
                .par_iter()
                .filter_map(|(node_id, position)| if *position == Absolute { Some(*node_id) } else { None })
                .collect::<Vec<_>>();

            // Append the position:absolute children after the regular children
            not_absolute_children.append(&mut absolute_children);
            not_absolute_children
        }

        let children_sorted = non_leaf_nodes
            .par_iter()
            .filter_map(|parent| Some((parent.node_id, sort_children_by_position(parent.node_id.into_crate_internal()?, node_hierarchy, styled_nodes, css_property_cache))))
            .collect::<Vec<_>>();

        let children_sorted: BTreeMap<AzNodeId, Vec<AzNodeId>> = children_sorted.into_iter().collect();
        let mut root_content_group = ContentGroup { root: AzNodeId::from_crate_internal(Some(NodeId::ZERO)), children: Vec::new().into() };
        fill_content_group_children(&mut root_content_group, &children_sorted);
        root_content_group
    }

    // Scans the dom for all image sources
    // pub fn get_all_image_sources(&self) -> Vec<ImageSource>

    // Scans the dom for all font instances
    // pub fn get_all_font_instances(&self) -> Vec<FontSource, FontSize>

    // Computes the diff between the two DOMs
    // pub fn diff(&self, other: &Self) -> StyledDomDiff { /**/ }

    // Restyles the DOM using a DOM diff
    // pub fn restyle(&mut self, css: &Css, style_options: StyleOptions, diff: &DomDiff) { }
}