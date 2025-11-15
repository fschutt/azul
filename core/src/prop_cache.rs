//! CSS property cache for efficient style resolution and animation.
//!
//! This module implements a cache layer between the raw CSS stylesheet and the rendered DOM.
//! It resolves CSS properties for each node, handling:
//!
//! - **Cascade resolution**: Computes final values from CSS rules, inline styles, and inheritance
//! - **Pseudo-class states**: Caches styles for `:hover`, `:active`, `:focus`, etc.
//! - **Animation support**: Tracks animating properties for smooth interpolation
//! - **Performance**: Avoids re-parsing and re-resolving unchanged properties
//!
//! # Architecture
//!
//! The cache is organized per-node and per-property-type. Each property has a dedicated
//! getter method that:
//! 1. Checks if the property is cached
//! 2. If not, resolves it from CSS rules + inline styles
//! 3. Caches the result for subsequent frames
//!
//! # Memory
//!
//! The cache size grows with DOM size × number of distinct property values.
//! Properties with default values are not cached to save memory.
//!
//! # Thread Safety
//!
//! Not thread-safe. Each window has its own cache instance.

extern crate alloc;

use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};

use azul_css::{
    css::{Css, CssPath},
    props::{
        basic::{StyleFontFamily, StyleFontFamilyVec, StyleFontSize},
        layout::{LayoutDisplay, LayoutHeight, LayoutWidth},
        property::{
            BoxDecorationBreakValue, BreakInsideValue, CaretAnimationDurationValue,
            CaretColorValue, ColumnCountValue, ColumnFillValue, ColumnRuleColorValue,
            ColumnRuleStyleValue, ColumnRuleWidthValue, ColumnSpanValue, ColumnWidthValue,
            ContentValue, CounterIncrementValue, CounterResetValue, CssProperty, CssPropertyType,
            FlowFromValue, FlowIntoValue, LayoutAlignContentValue, LayoutAlignItemsValue,
            LayoutAlignSelfValue, LayoutBorderBottomWidthValue, LayoutBorderLeftWidthValue,
            LayoutBorderRightWidthValue, LayoutBorderTopWidthValue, LayoutBottomValue,
            LayoutBoxSizingValue, LayoutClearValue, LayoutColumnGapValue, LayoutDisplayValue,
            LayoutFlexBasisValue, LayoutFlexDirectionValue, LayoutFlexGrowValue,
            LayoutFlexShrinkValue, LayoutFlexWrapValue, LayoutFloatValue, LayoutGapValue,
            LayoutGridAutoColumnsValue, LayoutGridAutoFlowValue, LayoutGridAutoRowsValue,
            LayoutGridColumnValue, LayoutGridRowValue, LayoutGridTemplateColumnsValue,
            LayoutGridTemplateRowsValue, LayoutHeightValue, LayoutJustifyContentValue,
            LayoutJustifyItemsValue, LayoutJustifySelfValue, LayoutLeftValue,
            LayoutMarginBottomValue, LayoutMarginLeftValue, LayoutMarginRightValue,
            LayoutMarginTopValue, LayoutMaxHeightValue, LayoutMaxWidthValue, LayoutMinHeightValue,
            LayoutMinWidthValue, LayoutOverflowValue, LayoutPaddingBottomValue,
            LayoutPaddingLeftValue, LayoutPaddingRightValue, LayoutPaddingTopValue,
            LayoutPositionValue, LayoutRightValue, LayoutRowGapValue, LayoutScrollbarWidthValue,
            LayoutTextJustifyValue, LayoutTopValue, LayoutWidthValue, LayoutWritingModeValue,
            LayoutZIndexValue, OrphansValue, PageBreakValue, ScrollbarStyleValue,
            SelectionBackgroundColorValue, SelectionColorValue, ShapeImageThresholdValue,
            ShapeMarginValue, ShapeOutsideValue, ShapeInsideValue, ClipPathValue, StringSetValue, StyleBackfaceVisibilityValue,
            StyleBackgroundContentVecValue, StyleBackgroundPositionVecValue,
            StyleBackgroundRepeatVecValue, StyleBackgroundSizeVecValue,
            StyleBorderBottomColorValue, StyleBorderBottomLeftRadiusValue,
            StyleBorderBottomRightRadiusValue, StyleBorderBottomStyleValue,
            StyleBorderLeftColorValue, StyleBorderLeftStyleValue, StyleBorderRightColorValue,
            StyleBorderRightStyleValue, StyleBorderTopColorValue, StyleBorderTopLeftRadiusValue,
            StyleBorderTopRightRadiusValue, StyleBorderTopStyleValue, StyleBoxShadowValue,
            StyleCursorValue, StyleDirectionValue, StyleFilterVecValue, StyleFontFamilyVecValue,
            StyleFontSizeValue, StyleFontWeightValue, StyleFontStyleValue, StyleFontValue, StyleHyphensValue, StyleLetterSpacingValue,
            StyleLineHeightValue, StyleListStylePositionValue, StyleListStyleTypeValue,
            StyleMixBlendModeValue, StyleOpacityValue,
            StylePerspectiveOriginValue, StyleScrollbarColorValue, StyleTabWidthValue,
            StyleTextAlignValue, StyleTextColorValue, StyleTextDecorationValue,
            StyleTransformOriginValue, StyleTransformVecValue, StyleUserSelectValue,
            StyleVisibilityValue, StyleWhiteSpaceValue, StyleWordSpacingValue, WidowsValue,
            LayoutTableLayoutValue, StyleBorderCollapseValue, LayoutBorderSpacingValue,
            StyleCaptionSideValue, StyleEmptyCellsValue,
        },
        style::{StyleCursor, StyleTextColor, StyleTransformOrigin},
    },
    AzString,
};

use crate::{
    dom::{NodeData, NodeDataInlineCssProperty, NodeId, TabIndex, TagId},
    id::{NodeDataContainer, NodeDataContainerRef},
    style::CascadeInfo,
    styled_dom::{
        AzTagId, NodeHierarchyItemId, NodeHierarchyItemVec, ParentWithNodeDepth,
        ParentWithNodeDepthVec, StyledNodeState, TagIdToNodeIdMapping,
    },
};

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
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CssPropertyCache {
    // number of nodes in the current DOM
    pub node_count: usize,

    // properties that were overridden in callbacks (not specific to any node state)
    pub user_overridden_properties: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,

    // non-default CSS properties that were cascaded from the parent
    pub cascaded_normal_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub cascaded_hover_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub cascaded_active_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub cascaded_focus_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,

    // non-default CSS properties that were set via a CSS file
    pub css_normal_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub css_hover_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub css_active_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
    pub css_focus_props: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssProperty>>,
}

impl CssPropertyCache {
    /// Restyles the CSS property cache with a new CSS file
    #[must_use]
    pub fn restyle(
        &mut self,
        css: &mut Css,
        node_data: &NodeDataContainerRef<NodeData>,
        node_hierarchy: &NodeHierarchyItemVec,
        non_leaf_nodes: &ParentWithNodeDepthVec,
        html_tree: &NodeDataContainerRef<CascadeInfo>,
    ) -> Vec<TagIdToNodeIdMapping> {
        use azul_css::{
            css::{CssDeclaration, CssPathPseudoSelector::*},
            props::layout::LayoutDisplay,
        };

        let css_is_empty = css.is_empty();

        if !css_is_empty {
            css.sort_by_specificity();

            macro_rules! filter_rules {($expected_pseudo_selector:expr, $node_id:expr) => {{
                css
                .rules() // can not be parallelized due to specificity order matching
                .filter(|rule_block| crate::style::rule_ends_with(&rule_block.path, $expected_pseudo_selector))
                .filter(|rule_block| crate::style::matches_html_element(
                    &rule_block.path,
                    $node_id,
                    &node_hierarchy.as_container(),
                    &node_data,
                    &html_tree,
                    $expected_pseudo_selector
                ))
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
                .map(|prop| prop.clone())
                .collect::<Vec<CssProperty>>()
            }};}

            // NOTE: This is wrong, but fast
            //
            // Get all nodes that end with `:hover`, `:focus` or `:active`
            // and copy the respective styles to the `hover_css_constraints`, etc. respectively
            //
            // NOTE: This won't work correctly for paths with `.blah:hover > #thing`
            // but that can be fixed later

            // go through each HTML node (in parallel) and see which CSS rules match
            let css_normal_rules: NodeDataContainer<(NodeId, Vec<CssProperty>)> = node_data
                .transform_nodeid_multithreaded_optional(|node_id| {
                    let r = filter_rules!(None, node_id);
                    if r.is_empty() {
                        None
                    } else {
                        Some((node_id, r))
                    }
                });

            let css_hover_rules: NodeDataContainer<(NodeId, Vec<CssProperty>)> = node_data
                .transform_nodeid_multithreaded_optional(|node_id| {
                    let r = filter_rules!(Some(Hover), node_id);
                    if r.is_empty() {
                        None
                    } else {
                        Some((node_id, r))
                    }
                });

            let css_active_rules: NodeDataContainer<(NodeId, Vec<CssProperty>)> = node_data
                .transform_nodeid_multithreaded_optional(|node_id| {
                    let r = filter_rules!(Some(Active), node_id);
                    if r.is_empty() {
                        None
                    } else {
                        Some((node_id, r))
                    }
                });

            let css_focus_rules: NodeDataContainer<(NodeId, Vec<CssProperty>)> = node_data
                .transform_nodeid_multithreaded_optional(|node_id| {
                    let r = filter_rules!(Some(Focus), node_id);
                    if r.is_empty() {
                        None
                    } else {
                        Some((node_id, r))
                    }
                });

            self.css_normal_props = css_normal_rules
                .internal
                .into_iter()
                .map(|(n, map)| {
                    (
                        n,
                        map.into_iter()
                            .map(|prop| (prop.get_type(), prop))
                            .collect(),
                    )
                })
                .collect();

            self.css_hover_props = css_hover_rules
                .internal
                .into_iter()
                .map(|(n, map)| {
                    (
                        n,
                        map.into_iter()
                            .map(|prop| (prop.get_type(), prop))
                            .collect(),
                    )
                })
                .collect();

            self.css_active_props = css_active_rules
                .internal
                .into_iter()
                .map(|(n, map)| {
                    (
                        n,
                        map.into_iter()
                            .map(|prop| (prop.get_type(), prop))
                            .collect(),
                    )
                })
                .collect();

            self.css_focus_props = css_focus_rules
                .internal
                .into_iter()
                .map(|(n, map)| {
                    (
                        n,
                        map.into_iter()
                            .map(|prop| (prop.get_type(), prop))
                            .collect(),
                    )
                })
                .collect();
        }

        // Inheritance: Inherit all values of the parent to the children, but
        // only if the property is inheritable and isn't yet set
        for ParentWithNodeDepth { depth: _, node_id } in non_leaf_nodes.iter() {
            let parent_id = match node_id.into_crate_internal() {
                Some(s) => s,
                None => continue,
            };

            // Inherit CSS properties from map A -> map B
            // map B will be populated with all inherited CSS properties
            macro_rules! inherit_props {
                ($from_inherit_map:expr, $to_inherit_map:expr) => {
                    let parent_inheritable_css_props =
                        $from_inherit_map.get(&parent_id).and_then(|map| {
                            let parent_inherit_props = map
                                .iter()
                                .filter(|(css_prop_type, _)| css_prop_type.is_inheritable())
                                .map(|(css_prop_type, css_prop)| (*css_prop_type, css_prop.clone()))
                                .collect::<Vec<(CssPropertyType, CssProperty)>>();
                            if parent_inherit_props.is_empty() {
                                None
                            } else {
                                Some(parent_inherit_props)
                            }
                        });

                    match parent_inheritable_css_props {
                        Some(pi) => {
                            // only override the rule if the child does not already have an
                            // inherited rule
                            for child_id in parent_id.az_children(&node_hierarchy.as_container()) {
                                let child_map = $to_inherit_map
                                    .entry(child_id)
                                    .or_insert_with(|| BTreeMap::new());

                                for (inherited_rule_type, inherited_rule_value) in pi.iter() {
                                    let _ = child_map
                                        .entry(*inherited_rule_type)
                                        .or_insert_with(|| inherited_rule_value.clone());
                                }
                            }
                        }
                        None => {}
                    }
                };
            }

            // Same as inherit_props, but filters along the inline node data instead
            macro_rules! inherit_inline_css_props {($filter_type:ident, $to_inherit_map:expr) => {
                let parent_inheritable_css_props = &node_data[parent_id]
                .inline_css_props
                .iter()
                 // test whether the property is a [normal, hover, focus, active] property
                .filter_map(|css_prop| if let NodeDataInlineCssProperty::$filter_type(p) = css_prop { Some(p) } else { None })
                // test whether the property is inheritable
                .filter(|css_prop| css_prop.get_type().is_inheritable())
                .cloned()
                .collect::<Vec<CssProperty>>();

                if !parent_inheritable_css_props.is_empty() {
                    // only override the rule if the child does not already have an inherited rule
                    for child_id in parent_id.az_children(&node_hierarchy.as_container()) {
                        let child_map = $to_inherit_map.entry(child_id).or_insert_with(|| BTreeMap::new());
                        for inherited_rule in parent_inheritable_css_props.iter() {
                            let _ = child_map
                            .entry(inherited_rule.get_type())
                            .or_insert_with(|| inherited_rule.clone());
                        }
                    }
                }

            };}

            // strongest inheritance first

            // Inherit inline CSS properties
            inherit_inline_css_props!(Normal, self.cascaded_normal_props);
            inherit_inline_css_props!(Hover, self.cascaded_hover_props);
            inherit_inline_css_props!(Active, self.cascaded_active_props);
            inherit_inline_css_props!(Focus, self.cascaded_focus_props);

            // Inherit the CSS properties from the CSS file
            if !css_is_empty {
                inherit_props!(self.css_normal_props, self.cascaded_normal_props);
                inherit_props!(self.css_hover_props, self.cascaded_hover_props);
                inherit_props!(self.css_active_props, self.cascaded_active_props);
                inherit_props!(self.css_focus_props, self.cascaded_focus_props);
            }

            // Inherit properties that were inherited in a previous iteration of the loop
            inherit_props!(self.cascaded_normal_props, self.cascaded_normal_props);
            inherit_props!(self.cascaded_hover_props, self.cascaded_hover_props);
            inherit_props!(self.cascaded_active_props, self.cascaded_active_props);
            inherit_props!(self.cascaded_focus_props, self.cascaded_focus_props);
        }

        // When restyling, the tag / node ID mappings may change, regenerate them
        // See if the node should have a hit-testing tag ID
        let default_node_state = StyledNodeState::default();

        // In order to hit-test `:hover` and `:active` selectors,
        // we need to insert "tag IDs" for all rectangles
        // that have a non-normal path ending, for example if we have
        // `#thing:hover`, then all nodes selected by `#thing`
        // need to get a TagId, otherwise, they can't be hit-tested.

        // NOTE: restyling a DOM may change the :hover nodes, which is
        // why the tag IDs have to be re-generated on every .restyle() call!
        node_data
            .internal
            .iter()
            .enumerate()
            .filter_map(|(node_id, node_data)| {
                let node_id = NodeId::new(node_id);

                let should_auto_insert_tabindex = node_data
                    .get_callbacks()
                    .iter()
                    .any(|cb| cb.event.is_focus_callback());

                let tab_index = match node_data.get_tab_index() {
                    Some(s) => Some(*s),
                    None => {
                        if should_auto_insert_tabindex {
                            Some(TabIndex::Auto)
                        } else {
                            None
                        }
                    }
                };

                let mut node_should_have_tag = false;

                // workaround for "goto end" - early break if
                // one of the conditions is true
                loop {
                    // check for display: none
                    let display = self
                        .get_display(&node_data, &node_id, &default_node_state)
                        .and_then(|p| p.get_property_or_default())
                        .unwrap_or_default();

                    if display == LayoutDisplay::None {
                        node_should_have_tag = false;
                        break;
                    }

                    if node_data.has_context_menu() {
                        node_should_have_tag = true;
                        break;
                    }

                    if tab_index.is_some() {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for context menu
                    if node_data.get_context_menu().is_some() {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for :hover
                    let node_has_hover_props =
                        node_data.inline_css_props.as_ref().iter().any(|p| match p {
                            NodeDataInlineCssProperty::Hover(_) => true,
                            _ => false,
                        }) || self.css_hover_props.get(&node_id).is_some()
                            || self.cascaded_hover_props.get(&node_id).is_some();

                    if node_has_hover_props {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for :active
                    let node_has_active_props =
                        node_data.inline_css_props.as_ref().iter().any(|p| match p {
                            NodeDataInlineCssProperty::Active(_) => true,
                            _ => false,
                        }) || self.css_active_props.get(&node_id).is_some()
                            || self.cascaded_active_props.get(&node_id).is_some();

                    if node_has_active_props {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for :focus
                    let node_has_focus_props =
                        node_data.inline_css_props.as_ref().iter().any(|p| match p {
                            NodeDataInlineCssProperty::Focus(_) => true,
                            _ => false,
                        }) || self.css_focus_props.get(&node_id).is_some()
                            || self.cascaded_focus_props.get(&node_id).is_some();

                    if node_has_focus_props {
                        node_should_have_tag = true;
                        break;
                    }

                    // check whether any Hover(), Active() or Focus() callbacks are present
                    let node_only_window_callbacks = node_data.get_callbacks().is_empty()
                        || node_data
                            .get_callbacks()
                            .iter()
                            .all(|cb| cb.event.is_window_callback());

                    if !node_only_window_callbacks {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for non-default cursor: property - needed for hit-testing cursor
                    let node_has_non_default_cursor = self
                        .get_cursor(&node_data, &node_id, &default_node_state)
                        .is_some();

                    if node_has_non_default_cursor {
                        node_should_have_tag = true;
                        break;
                    }

                    break;
                }

                if !node_should_have_tag {
                    None
                } else {
                    Some(TagIdToNodeIdMapping {
                        tag_id: AzTagId::from_crate_internal(TagId::unique()),
                        node_id: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
                        tab_index: tab_index.into(),
                        parent_node_ids: {
                            let mut parents = Vec::new();
                            let mut cur_parent = node_hierarchy.as_container()[node_id].parent_id();
                            while let Some(c) = cur_parent.clone() {
                                parents.push(NodeHierarchyItemId::from_crate_internal(Some(c)));
                                cur_parent = node_hierarchy.as_container()[c].parent_id();
                            }
                            parents.reverse(); // parents sorted in depth-increasing order
                            parents.into()
                        },
                    })
                }
            })
            .collect()
    }

    pub fn get_computed_css_style_string(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> String {
        let mut s = String::new();
        if let Some(p) = self.get_background_content(&node_data, node_id, node_state) {
            s.push_str(&format!("background: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_background_position(&node_data, node_id, node_state) {
            s.push_str(&format!("background-position: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_background_size(&node_data, node_id, node_state) {
            s.push_str(&format!("background-size: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_background_repeat(&node_data, node_id, node_state) {
            s.push_str(&format!("background-repeat: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_font_size(&node_data, node_id, node_state) {
            s.push_str(&format!("font-size: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_font_family(&node_data, node_id, node_state) {
            s.push_str(&format!("font-family: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_text_color(&node_data, node_id, node_state) {
            s.push_str(&format!("color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_text_align(&node_data, node_id, node_state) {
            s.push_str(&format!("text-align: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_line_height(&node_data, node_id, node_state) {
            s.push_str(&format!("line-height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_letter_spacing(&node_data, node_id, node_state) {
            s.push_str(&format!("letter-spacing: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_word_spacing(&node_data, node_id, node_state) {
            s.push_str(&format!("word-spacing: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_tab_width(&node_data, node_id, node_state) {
            s.push_str(&format!("tab-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_cursor(&node_data, node_id, node_state) {
            s.push_str(&format!("cursor: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_box_shadow_left(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "-azul-box-shadow-left: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_box_shadow_right(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "-azul-box-shadow-right: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_box_shadow_top(&node_data, node_id, node_state) {
            s.push_str(&format!("-azul-box-shadow-top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_box_shadow_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "-azul-box-shadow-bottom: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_top_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-top-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_left_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-left-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_right_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-right-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_bottom_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-bottom-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_top_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-top-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_left_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-left-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_right_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-right-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_bottom_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-bottom-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_top_left_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-top-left-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_top_right_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-top-right-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_bottom_left_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-bottom-left-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_bottom_right_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-bottom-right-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_opacity(&node_data, node_id, node_state) {
            s.push_str(&format!("opacity: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_transform(&node_data, node_id, node_state) {
            s.push_str(&format!("transform: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_transform_origin(&node_data, node_id, node_state) {
            s.push_str(&format!("transform-origin: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_perspective_origin(&node_data, node_id, node_state) {
            s.push_str(&format!("perspective-origin: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_backface_visibility(&node_data, node_id, node_state) {
            s.push_str(&format!("backface-visibility: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_hyphens(&node_data, node_id, node_state) {
            s.push_str(&format!("hyphens: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_direction(&node_data, node_id, node_state) {
            s.push_str(&format!("direction: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_white_space(&node_data, node_id, node_state) {
            s.push_str(&format!("white-space: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_display(&node_data, node_id, node_state) {
            s.push_str(&format!("display: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_float(&node_data, node_id, node_state) {
            s.push_str(&format!("float: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_box_sizing(&node_data, node_id, node_state) {
            s.push_str(&format!("box-sizing: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_width(&node_data, node_id, node_state) {
            s.push_str(&format!("width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_height(&node_data, node_id, node_state) {
            s.push_str(&format!("height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_min_width(&node_data, node_id, node_state) {
            s.push_str(&format!("min-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_min_height(&node_data, node_id, node_state) {
            s.push_str(&format!("min-height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_max_width(&node_data, node_id, node_state) {
            s.push_str(&format!("max-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_max_height(&node_data, node_id, node_state) {
            s.push_str(&format!("max-height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_position(&node_data, node_id, node_state) {
            s.push_str(&format!("position: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_top(&node_data, node_id, node_state) {
            s.push_str(&format!("top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!("bottom: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_right(&node_data, node_id, node_state) {
            s.push_str(&format!("right: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_left(&node_data, node_id, node_state) {
            s.push_str(&format!("left: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_top(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-bottom: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_left(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-left: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_right(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-right: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_top(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-bottom: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_left(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-left: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_right(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-right: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_top_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-top-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_left_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-left-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_right_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-right-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_bottom_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-bottom-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_overflow_x(&node_data, node_id, node_state) {
            s.push_str(&format!("overflow-x: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_overflow_y(&node_data, node_id, node_state) {
            s.push_str(&format!("overflow-y: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_direction(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-direction: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_wrap(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-wrap: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_grow(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-grow: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_shrink(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-shrink: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_justify_content(&node_data, node_id, node_state) {
            s.push_str(&format!("justify-content: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_align_items(&node_data, node_id, node_state) {
            s.push_str(&format!("align-items: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_align_content(&node_data, node_id, node_state) {
            s.push_str(&format!("align-content: {};", p.get_css_value_fmt()));
        }
        s
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Clone)]
pub struct CssPropertyCachePtr {
    pub ptr: Box<CssPropertyCache>,
    pub run_destructor: bool,
}

impl CssPropertyCachePtr {
    pub fn new(cache: CssPropertyCache) -> Self {
        Self {
            ptr: Box::new(cache),
            run_destructor: true,
        }
    }
    pub fn downcast_mut<'a>(&'a mut self) -> &'a mut CssPropertyCache {
        &mut *self.ptr
    }
}

impl Drop for CssPropertyCachePtr {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl CssPropertyCache {
    pub fn empty(node_count: usize) -> Self {
        Self {
            node_count,
            user_overridden_properties: BTreeMap::new(),

            cascaded_normal_props: BTreeMap::new(),
            cascaded_hover_props: BTreeMap::new(),
            cascaded_active_props: BTreeMap::new(),
            cascaded_focus_props: BTreeMap::new(),

            css_normal_props: BTreeMap::new(),
            css_hover_props: BTreeMap::new(),
            css_active_props: BTreeMap::new(),
            css_focus_props: BTreeMap::new(),
        }
    }

    pub fn append(&mut self, other: &mut Self) {
        macro_rules! append_css_property_vec {
            ($field_name:ident) => {{
                let mut s = BTreeMap::new();
                core::mem::swap(&mut s, &mut other.$field_name);
                for (node_id, property_map) in s.into_iter() {
                    self.$field_name
                        .insert(node_id + self.node_count, property_map);
                }
            }};
        }

        append_css_property_vec!(user_overridden_properties);
        append_css_property_vec!(cascaded_normal_props);
        append_css_property_vec!(cascaded_hover_props);
        append_css_property_vec!(cascaded_active_props);
        append_css_property_vec!(cascaded_focus_props);
        append_css_property_vec!(css_normal_props);
        append_css_property_vec!(css_hover_props);
        append_css_property_vec!(css_active_props);
        append_css_property_vec!(css_focus_props);

        self.node_count += other.node_count;
    }

    pub fn is_horizontal_overflow_visible(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_x(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_visible()
    }

    pub fn is_vertical_overflow_visible(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_y(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_visible()
    }

    pub fn is_horizontal_overflow_hidden(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_x(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_hidden()
    }

    pub fn is_vertical_overflow_hidden(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_y(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_hidden()
    }

    pub fn get_text_color_or_default(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> StyleTextColor {
        use crate::ui_solver::DEFAULT_TEXT_COLOR;
        self.get_text_color(node_data, node_id, node_state)
            .and_then(|fs| fs.get_property().cloned())
            .unwrap_or(DEFAULT_TEXT_COLOR)
    }

    /// Returns the font ID of the
    pub fn get_font_id_or_default(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> StyleFontFamilyVec {
        use crate::ui_solver::DEFAULT_FONT_ID;
        let default_font_id = vec![StyleFontFamily::System(AzString::from_const_str(
            DEFAULT_FONT_ID,
        ))]
        .into();
        let font_family_opt = self.get_font_family(node_data, node_id, node_state);

        font_family_opt
            .as_ref()
            .and_then(|family| Some(family.get_property()?.clone()))
            .unwrap_or(default_font_id)
    }

    pub fn get_font_size_or_default(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> StyleFontSize {
        use crate::ui_solver::DEFAULT_FONT_SIZE;
        self.get_font_size(node_data, node_id, node_state)
            .and_then(|fs| fs.get_property().cloned())
            .unwrap_or(DEFAULT_FONT_SIZE)
    }

    pub fn has_border(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_border_left_width(node_data, node_id, node_state)
            .is_some()
            || self
                .get_border_right_width(node_data, node_id, node_state)
                .is_some()
            || self
                .get_border_top_width(node_data, node_id, node_state)
                .is_some()
            || self
                .get_border_bottom_width(node_data, node_id, node_state)
                .is_some()
    }

    pub fn has_box_shadow(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_box_shadow_left(node_data, node_id, node_state)
            .is_some()
            || self
                .get_box_shadow_right(node_data, node_id, node_state)
                .is_some()
            || self
                .get_box_shadow_top(node_data, node_id, node_state)
                .is_some()
            || self
                .get_box_shadow_bottom(node_data, node_id, node_state)
                .is_some()
    }

    pub fn get_property<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        css_property_type: &CssPropertyType,
    ) -> Option<&CssProperty> {
        // NOTE: This function is slow, but it is going to be called on every
        // node in parallel, so it should be rather fast in the end

        // First test if there is some user-defined override for the property
        if let Some(p) = self
            .user_overridden_properties
            .get(node_id)
            .and_then(|n| n.get(css_property_type))
        {
            return Some(p);
        }

        if !(node_state.normal || node_state.active || node_state.hover || node_state.focused) {
            return None;
        }

        // If that fails, see if there is an inline CSS property that matches
        // :focus > :active > :hover > :normal
        if node_state.focused {
            if let Some(p) = self
                .css_focus_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }

            if let Some(p) = node_data
                .inline_css_props
                .as_ref()
                .iter()
                .find_map(|css_prop| {
                    if let NodeDataInlineCssProperty::Focus(p) = css_prop {
                        if p.get_type() == *css_property_type {
                            return Some(p);
                        }
                    }
                    None
                })
            {
                return Some(p);
            }

            if let Some(p) = self
                .cascaded_focus_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }
        }

        if node_state.active {
            if let Some(p) = self
                .css_active_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }

            if let Some(p) = node_data
                .inline_css_props
                .as_ref()
                .iter()
                .find_map(|css_prop| {
                    if let NodeDataInlineCssProperty::Active(p) = css_prop {
                        if p.get_type() == *css_property_type {
                            return Some(p);
                        }
                    }
                    None
                })
            {
                return Some(p);
            }

            if let Some(p) = self
                .cascaded_active_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }
        }

        if node_state.hover {
            if let Some(p) = self
                .css_hover_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }

            if let Some(p) = node_data
                .inline_css_props
                .as_ref()
                .iter()
                .find_map(|css_prop| {
                    if let NodeDataInlineCssProperty::Hover(p) = css_prop {
                        if p.get_type() == *css_property_type {
                            return Some(p);
                        }
                    }
                    None
                })
            {
                return Some(p);
            }

            if let Some(p) = self
                .cascaded_hover_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }
        }

        if node_state.normal {
            if let Some(p) = self
                .css_normal_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }

            if let Some(p) = node_data
                .inline_css_props
                .as_ref()
                .iter()
                .find_map(|css_prop| {
                    if let NodeDataInlineCssProperty::Normal(p) = css_prop {
                        if p.get_type() == *css_property_type {
                            return Some(p);
                        }
                    }
                    None
                })
            {
                return Some(p);
            }

            if let Some(p) = self
                .cascaded_normal_props
                .get(node_id)
                .and_then(|map| map.get(css_property_type))
            {
                return Some(p);
            }
        }

        // User-agent stylesheet fallback (lowest precedence)
        // Check if the node type has a default value for this property
        crate::ua_css::get_ua_property(node_data.node_type.clone(), *css_property_type)
    }

    pub fn get_background_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundContent,
        )
        .and_then(|p| p.as_background_content())
    }

    // Method for getting hyphens property
    pub fn get_hyphens<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleHyphensValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Hyphens)
            .and_then(|p| p.as_hyphens())
    }

    // Method for getting direction property
    pub fn get_direction<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleDirectionValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Direction)
            .and_then(|p| p.as_direction())
    }

    // Method for getting white-space property
    pub fn get_white_space<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleWhiteSpaceValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::WhiteSpace)
            .and_then(|p| p.as_white_space())
    }
    pub fn get_background_position<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundPositionVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundPosition,
        )
        .and_then(|p| p.as_background_position())
    }
    pub fn get_background_size<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundSizeVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundSize,
        )
        .and_then(|p| p.as_background_size())
    }
    pub fn get_background_repeat<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundRepeatVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundRepeat,
        )
        .and_then(|p| p.as_background_repeat())
    }
    pub fn get_font_size<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontSizeValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FontSize)
            .and_then(|p| p.as_font_size())
    }
    pub fn get_font_family<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontFamilyVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FontFamily)
            .and_then(|p| p.as_font_family())
    }
    pub fn get_font_weight<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontWeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FontWeight)
            .and_then(|p| p.as_font_weight())
    }
    pub fn get_font_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontStyleValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FontStyle)
            .and_then(|p| p.as_font_style())
    }
    pub fn get_text_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextColorValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextColor)
            .and_then(|p| p.as_text_color())
    }
    // Method for getting caret-color property
    pub fn get_caret_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CaretColorValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::CaretColor)
            .and_then(|p| p.as_caret_color())
    }

    // Method for getting caret-animation-duration property
    pub fn get_caret_animation_duration<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CaretAnimationDurationValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::CaretAnimationDuration,
        )
        .and_then(|p| p.as_caret_animation_duration())
    }

    // Method for getting selection-background-color property
    pub fn get_selection_background_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a SelectionBackgroundColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::SelectionBackgroundColor,
        )
        .and_then(|p| p.as_selection_background_color())
    }

    // Method for getting selection-color property
    pub fn get_selection_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a SelectionColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::SelectionColor,
        )
        .and_then(|p| p.as_selection_color())
    }

    // Method for getting text-justify property
    pub fn get_text_justify<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutTextJustifyValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TextJustify,
        )
        .and_then(|p| p.as_text_justify())
    }

    // Method for getting z-index property
    pub fn get_z_index<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutZIndexValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ZIndex)
            .and_then(|p| p.as_z_index())
    }

    // Method for getting flex-basis property
    pub fn get_flex_basis<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexBasisValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexBasis)
            .and_then(|p| p.as_flex_basis())
    }

    // Method for getting column-gap property
    pub fn get_column_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutColumnGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ColumnGap)
            .and_then(|p| p.as_column_gap())
    }

    // Method for getting row-gap property
    pub fn get_row_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutRowGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::RowGap)
            .and_then(|p| p.as_row_gap())
    }

    // Method for getting grid-template-columns property
    pub fn get_grid_template_columns<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridTemplateColumnsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridTemplateColumns,
        )
        .and_then(|p| p.as_grid_template_columns())
    }

    // Method for getting grid-template-rows property
    pub fn get_grid_template_rows<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridTemplateRowsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridTemplateRows,
        )
        .and_then(|p| p.as_grid_template_rows())
    }

    // Method for getting grid-auto-columns property
    pub fn get_grid_auto_columns<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridAutoColumnsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridAutoColumns,
        )
        .and_then(|p| p.as_grid_auto_columns())
    }

    // Method for getting grid-auto-rows property
    pub fn get_grid_auto_rows<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridAutoRowsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridAutoRows,
        )
        .and_then(|p| p.as_grid_auto_rows())
    }

    // Method for getting grid-column property
    pub fn get_grid_column<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridColumnValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::GridColumn)
            .and_then(|p| p.as_grid_column())
    }

    // Method for getting grid-row property
    pub fn get_grid_row<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridRowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::GridRow)
            .and_then(|p| p.as_grid_row())
    }

    // Method for getting grid-auto-flow property
    pub fn get_grid_auto_flow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridAutoFlowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridAutoFlow,
        )
        .and_then(|p| p.as_grid_auto_flow())
    }

    // Method for getting justify-self property
    pub fn get_justify_self<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutJustifySelfValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::JustifySelf,
        )
        .and_then(|p| p.as_justify_self())
    }

    // Method for getting justify-items property
    pub fn get_justify_items<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutJustifyItemsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::JustifyItems,
        )
        .and_then(|p| p.as_justify_items())
    }

    // Method for getting gap property
    pub fn get_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Gap)
            .and_then(|p| p.as_gap())
    }

    // Method for getting grid-gap property
    pub fn get_grid_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::GridGap)
            .and_then(|p| p.as_grid_gap())
    }

    // Method for getting align-self property
    pub fn get_align_self<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutAlignSelfValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::AlignSelf)
            .and_then(|p| p.as_align_self())
    }

    // Method for getting font property
    pub fn get_font<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Font)
            .and_then(|p| p.as_font())
    }

    // Method for getting writing-mode property
    pub fn get_writing_mode<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutWritingModeValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::WritingMode,
        )
        .and_then(|p| p.as_writing_mode())
    }

    // Method for getting clear property
    pub fn get_clear<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutClearValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Clear)
            .and_then(|p| p.as_clear())
    }

    // Method for getting shape-outside property
    pub fn get_shape_outside<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeOutsideValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ShapeOutside)
            .and_then(|p| p.as_shape_outside())
    }

    // Method for getting shape-inside property
    pub fn get_shape_inside<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeInsideValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ShapeInside)
            .and_then(|p| p.as_shape_inside())
    }

    // Method for getting clip-path property
    pub fn get_clip_path<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ClipPathValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ClipPath)
            .and_then(|p| p.as_clip_path())
    }

    // Method for getting scrollbar-style property
    pub fn get_scrollbar_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ScrollbarStyleValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Scrollbar)
            .and_then(|p| p.as_scrollbar())
    }

    // Method for getting scrollbar-width property
    pub fn get_scrollbar_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutScrollbarWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarWidth,
        )
        .and_then(|p| p.as_scrollbar_width())
    }

    // Method for getting scrollbar-color property
    pub fn get_scrollbar_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleScrollbarColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarColor,
        )
        .and_then(|p| p.as_scrollbar_color())
    }

    // Method for getting visibility property
    pub fn get_visibility<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleVisibilityValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Visibility)
            .and_then(|p| p.as_visibility())
    }

    // Method for getting break-before property
    pub fn get_break_before<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a PageBreakValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BreakBefore,
        )
        .and_then(|p| p.as_break_before())
    }

    // Method for getting break-after property
    pub fn get_break_after<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a PageBreakValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::BreakAfter)
            .and_then(|p| p.as_break_after())
    }

    // Method for getting break-inside property
    pub fn get_break_inside<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a BreakInsideValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BreakInside,
        )
        .and_then(|p| p.as_break_inside())
    }

    // Method for getting orphans property
    pub fn get_orphans<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a OrphansValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Orphans)
            .and_then(|p| p.as_orphans())
    }

    // Method for getting widows property
    pub fn get_widows<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a WidowsValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Widows)
            .and_then(|p| p.as_widows())
    }

    // Method for getting box-decoration-break property
    pub fn get_box_decoration_break<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a BoxDecorationBreakValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxDecorationBreak,
        )
        .and_then(|p| p.as_box_decoration_break())
    }

    // Method for getting column-count property
    pub fn get_column_count<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnCountValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnCount,
        )
        .and_then(|p| p.as_column_count())
    }

    // Method for getting column-width property
    pub fn get_column_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnWidth,
        )
        .and_then(|p| p.as_column_width())
    }

    // Method for getting column-span property
    pub fn get_column_span<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnSpanValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ColumnSpan)
            .and_then(|p| p.as_column_span())
    }

    // Method for getting column-fill property
    pub fn get_column_fill<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnFillValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ColumnFill)
            .and_then(|p| p.as_column_fill())
    }

    // Method for getting column-rule-width property
    pub fn get_column_rule_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnRuleWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnRuleWidth,
        )
        .and_then(|p| p.as_column_rule_width())
    }

    // Method for getting column-rule-style property
    pub fn get_column_rule_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnRuleStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnRuleStyle,
        )
        .and_then(|p| p.as_column_rule_style())
    }

    // Method for getting column-rule-color property
    pub fn get_column_rule_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnRuleColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnRuleColor,
        )
        .and_then(|p| p.as_column_rule_color())
    }

    // Method for getting flow-into property
    pub fn get_flow_into<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a FlowIntoValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlowInto)
            .and_then(|p| p.as_flow_into())
    }

    // Method for getting flow-from property
    pub fn get_flow_from<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a FlowFromValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlowFrom)
            .and_then(|p| p.as_flow_from())
    }

    // Method for getting shape-margin property
    pub fn get_shape_margin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeMarginValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ShapeMargin,
        )
        .and_then(|p| p.as_shape_margin())
    }

    // Method for getting shape-image-threshold property
    pub fn get_shape_image_threshold<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeImageThresholdValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ShapeImageThreshold,
        )
        .and_then(|p| p.as_shape_image_threshold())
    }

    // Method for getting content property
    pub fn get_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ContentValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Content)
            .and_then(|p| p.as_content())
    }

    // Method for getting counter-reset property
    pub fn get_counter_reset<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CounterResetValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::CounterReset,
        )
        .and_then(|p| p.as_counter_reset())
    }

    // Method for getting counter-increment property
    pub fn get_counter_increment<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CounterIncrementValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::CounterIncrement,
        )
        .and_then(|p| p.as_counter_increment())
    }

    // Method for getting string-set property
    pub fn get_string_set<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StringSetValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::StringSet)
            .and_then(|p| p.as_string_set())
    }
    pub fn get_text_align<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextAlignValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextAlign)
            .and_then(|p| p.as_text_align())
    }
    pub fn get_user_select<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleUserSelectValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::UserSelect)
            .and_then(|p| p.as_user_select())
    }
    pub fn get_text_decoration<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextDecorationValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TextDecoration,
        )
        .and_then(|p| p.as_text_decoration())
    }
    pub fn get_line_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleLineHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::LineHeight)
            .and_then(|p| p.as_line_height())
    }
    pub fn get_letter_spacing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleLetterSpacingValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::LetterSpacing,
        )
        .and_then(|p| p.as_letter_spacing())
    }
    pub fn get_word_spacing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleWordSpacingValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::WordSpacing,
        )
        .and_then(|p| p.as_word_spacing())
    }
    pub fn get_tab_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTabWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TabWidth)
            .and_then(|p| p.as_tab_width())
    }
    pub fn get_cursor<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleCursorValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Cursor)
            .and_then(|p| p.as_cursor())
    }
    pub fn get_box_shadow_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowLeft,
        )
        .and_then(|p| p.as_box_shadow_left())
    }
    pub fn get_box_shadow_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowRight,
        )
        .and_then(|p| p.as_box_shadow_right())
    }
    pub fn get_box_shadow_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowTop,
        )
        .and_then(|p| p.as_box_shadow_top())
    }
    pub fn get_box_shadow_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowBottom,
        )
        .and_then(|p| p.as_box_shadow_bottom())
    }
    pub fn get_border_top_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopColor,
        )
        .and_then(|p| p.as_border_top_color())
    }
    pub fn get_border_left_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderLeftColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderLeftColor,
        )
        .and_then(|p| p.as_border_left_color())
    }
    pub fn get_border_right_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderRightColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderRightColor,
        )
        .and_then(|p| p.as_border_right_color())
    }
    pub fn get_border_bottom_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomColor,
        )
        .and_then(|p| p.as_border_bottom_color())
    }
    pub fn get_border_top_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopStyle,
        )
        .and_then(|p| p.as_border_top_style())
    }
    pub fn get_border_left_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderLeftStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderLeftStyle,
        )
        .and_then(|p| p.as_border_left_style())
    }
    pub fn get_border_right_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderRightStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderRightStyle,
        )
        .and_then(|p| p.as_border_right_style())
    }
    pub fn get_border_bottom_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomStyle,
        )
        .and_then(|p| p.as_border_bottom_style())
    }
    pub fn get_border_top_left_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopLeftRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopLeftRadius,
        )
        .and_then(|p| p.as_border_top_left_radius())
    }
    pub fn get_border_top_right_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopRightRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopRightRadius,
        )
        .and_then(|p| p.as_border_top_right_radius())
    }
    pub fn get_border_bottom_left_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomLeftRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomLeftRadius,
        )
        .and_then(|p| p.as_border_bottom_left_radius())
    }
    pub fn get_border_bottom_right_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomRightRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomRightRadius,
        )
        .and_then(|p| p.as_border_bottom_right_radius())
    }
    pub fn get_opacity<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleOpacityValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Opacity)
            .and_then(|p| p.as_opacity())
    }
    pub fn get_transform<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTransformVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Transform)
            .and_then(|p| p.as_transform())
    }
    pub fn get_transform_origin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTransformOriginValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TransformOrigin,
        )
        .and_then(|p| p.as_transform_origin())
    }
    pub fn get_perspective_origin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StylePerspectiveOriginValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PerspectiveOrigin,
        )
        .and_then(|p| p.as_perspective_origin())
    }
    pub fn get_backface_visibility<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackfaceVisibilityValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackfaceVisibility,
        )
        .and_then(|p| p.as_backface_visibility())
    }
    pub fn get_display<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutDisplayValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Display)
            .and_then(|p| p.as_display())
    }
    pub fn get_float<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFloatValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Float)
            .and_then(|p| p.as_float())
    }
    pub fn get_box_sizing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBoxSizingValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::BoxSizing)
            .and_then(|p| p.as_box_sizing())
    }
    pub fn get_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Width)
            .and_then(|p| p.as_width())
    }
    pub fn get_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Height)
            .and_then(|p| p.as_height())
    }
    pub fn get_min_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMinWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MinWidth)
            .and_then(|p| p.as_min_width())
    }
    pub fn get_min_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMinHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MinHeight)
            .and_then(|p| p.as_min_height())
    }
    pub fn get_max_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMaxWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MaxWidth)
            .and_then(|p| p.as_max_width())
    }
    pub fn get_max_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMaxHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MaxHeight)
            .and_then(|p| p.as_max_height())
    }
    pub fn get_position<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPositionValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Position)
            .and_then(|p| p.as_position())
    }
    pub fn get_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutTopValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Top)
            .and_then(|p| p.as_top())
    }
    pub fn get_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBottomValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Bottom)
            .and_then(|p| p.as_bottom())
    }
    pub fn get_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutRightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Right)
            .and_then(|p| p.as_right())
    }
    pub fn get_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutLeftValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Left)
            .and_then(|p| p.as_left())
    }
    pub fn get_padding_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingTopValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::PaddingTop)
            .and_then(|p| p.as_padding_top())
    }
    pub fn get_padding_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingBottomValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PaddingBottom,
        )
        .and_then(|p| p.as_padding_bottom())
    }
    pub fn get_padding_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingLeftValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PaddingLeft,
        )
        .and_then(|p| p.as_padding_left())
    }
    pub fn get_padding_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingRightValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PaddingRight,
        )
        .and_then(|p| p.as_padding_right())
    }
    pub fn get_margin_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginTopValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MarginTop)
            .and_then(|p| p.as_margin_top())
    }
    pub fn get_margin_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginBottomValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::MarginBottom,
        )
        .and_then(|p| p.as_margin_bottom())
    }
    pub fn get_margin_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginLeftValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MarginLeft)
            .and_then(|p| p.as_margin_left())
    }
    pub fn get_margin_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginRightValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::MarginRight,
        )
        .and_then(|p| p.as_margin_right())
    }
    pub fn get_border_top_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderTopWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopWidth,
        )
        .and_then(|p| p.as_border_top_width())
    }
    pub fn get_border_left_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderLeftWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderLeftWidth,
        )
        .and_then(|p| p.as_border_left_width())
    }
    pub fn get_border_right_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderRightWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderRightWidth,
        )
        .and_then(|p| p.as_border_right_width())
    }
    pub fn get_border_bottom_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderBottomWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomWidth,
        )
        .and_then(|p| p.as_border_bottom_width())
    }
    pub fn get_overflow_x<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutOverflowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::OverflowX)
            .and_then(|p| p.as_overflow_x())
    }
    pub fn get_overflow_y<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutOverflowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::OverflowY)
            .and_then(|p| p.as_overflow_y())
    }
    pub fn get_flex_direction<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexDirectionValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::FlexDirection,
        )
        .and_then(|p| p.as_flex_direction())
    }
    pub fn get_flex_wrap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexWrapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexWrap)
            .and_then(|p| p.as_flex_wrap())
    }
    pub fn get_flex_grow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexGrowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexGrow)
            .and_then(|p| p.as_flex_grow())
    }
    pub fn get_flex_shrink<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexShrinkValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexShrink)
            .and_then(|p| p.as_flex_shrink())
    }
    pub fn get_justify_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutJustifyContentValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::JustifyContent,
        )
        .and_then(|p| p.as_justify_content())
    }
    pub fn get_align_items<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutAlignItemsValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::AlignItems)
            .and_then(|p| p.as_align_items())
    }
    pub fn get_align_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutAlignContentValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::AlignContent,
        )
        .and_then(|p| p.as_align_content())
    }
    pub fn get_mix_blend_mode<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleMixBlendModeValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::MixBlendMode,
        )
        .and_then(|p| p.as_mix_blend_mode())
    }
    pub fn get_filter<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFilterVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Filter)
            .and_then(|p| p.as_filter())
    }
    pub fn get_backdrop_filter<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFilterVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Filter)
            .and_then(|p| p.as_backdrop_filter())
    }
    pub fn get_text_shadow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextShadow)
            .and_then(|p| p.as_text_shadow())
    }
    pub fn get_list_style_type<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleListStyleTypeValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ListStyleType)
            .and_then(|p| p.as_list_style_type())
    }
    pub fn get_list_style_position<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleListStylePositionValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ListStylePosition)
            .and_then(|p| p.as_list_style_position())
    }
    pub fn get_table_layout<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutTableLayoutValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TableLayout)
            .and_then(|p| p.as_table_layout())
    }
    pub fn get_border_collapse<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderCollapseValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::BorderCollapse)
            .and_then(|p| p.as_border_collapse())
    }
    pub fn get_border_spacing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderSpacingValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::BorderSpacing)
            .and_then(|p| p.as_border_spacing())
    }
    pub fn get_caption_side<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleCaptionSideValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::CaptionSide)
            .and_then(|p| p.as_caption_side())
    }
    pub fn get_empty_cells<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleEmptyCellsValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::EmptyCells)
            .and_then(|p| p.as_empty_cells())
    }

    // Width calculation methods
    pub fn calc_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_width(node_data, node_id, styled_node_state)
            .and_then(|w| match w.get_property()? {
                LayoutWidth::Px(px) => Some(px.to_pixels(reference_width)),
                _ => Some(0.0), // min-content/max-content not resolved here
            })
            .unwrap_or(0.0)
    }

    pub fn calc_min_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_min_width(node_data, node_id, styled_node_state)
            .and_then(|w| Some(w.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_max_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> Option<f32> {
        self.get_max_width(node_data, node_id, styled_node_state)
            .and_then(|w| Some(w.get_property()?.inner.to_pixels(reference_width)))
    }

    // Height calculation methods
    pub fn calc_height(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_height(node_data, node_id, styled_node_state)
            .and_then(|h| match h.get_property()? {
                LayoutHeight::Px(px) => Some(px.to_pixels(reference_height)),
                _ => Some(0.0), // min-content/max-content not resolved here
            })
            .unwrap_or(0.0)
    }

    pub fn calc_min_height(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_min_height(node_data, node_id, styled_node_state)
            .and_then(|h| Some(h.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    pub fn calc_max_height(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> Option<f32> {
        self.get_max_height(node_data, node_id, styled_node_state)
            .and_then(|h| Some(h.get_property()?.inner.to_pixels(reference_height)))
    }

    // Position calculation methods
    pub fn calc_left(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> Option<f32> {
        self.get_left(node_data, node_id, styled_node_state)
            .and_then(|l| Some(l.get_property()?.inner.to_pixels(reference_width)))
    }

    pub fn calc_right(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> Option<f32> {
        self.get_right(node_data, node_id, styled_node_state)
            .and_then(|r| Some(r.get_property()?.inner.to_pixels(reference_width)))
    }

    pub fn calc_top(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> Option<f32> {
        self.get_top(node_data, node_id, styled_node_state)
            .and_then(|t| Some(t.get_property()?.inner.to_pixels(reference_height)))
    }

    pub fn calc_bottom(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> Option<f32> {
        self.get_bottom(node_data, node_id, styled_node_state)
            .and_then(|b| Some(b.get_property()?.inner.to_pixels(reference_height)))
    }

    // Border calculation methods
    pub fn calc_border_left_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_border_left_width(node_data, node_id, styled_node_state)
            .and_then(|b| Some(b.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_border_right_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_border_right_width(node_data, node_id, styled_node_state)
            .and_then(|b| Some(b.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_border_top_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_border_top_width(node_data, node_id, styled_node_state)
            .and_then(|b| Some(b.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    pub fn calc_border_bottom_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_border_bottom_width(node_data, node_id, styled_node_state)
            .and_then(|b| Some(b.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    // Padding calculation methods
    pub fn calc_padding_left(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_padding_left(node_data, node_id, styled_node_state)
            .and_then(|p| Some(p.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_padding_right(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_padding_right(node_data, node_id, styled_node_state)
            .and_then(|p| Some(p.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_padding_top(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_padding_top(node_data, node_id, styled_node_state)
            .and_then(|p| Some(p.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    pub fn calc_padding_bottom(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_padding_bottom(node_data, node_id, styled_node_state)
            .and_then(|p| Some(p.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    // Margin calculation methods
    pub fn calc_margin_left(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_margin_left(node_data, node_id, styled_node_state)
            .and_then(|m| Some(m.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_margin_right(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_margin_right(node_data, node_id, styled_node_state)
            .and_then(|m| Some(m.get_property()?.inner.to_pixels(reference_width)))
            .unwrap_or(0.0)
    }

    pub fn calc_margin_top(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_margin_top(node_data, node_id, styled_node_state)
            .and_then(|m| Some(m.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }

    pub fn calc_margin_bottom(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_margin_bottom(node_data, node_id, styled_node_state)
            .and_then(|m| Some(m.get_property()?.inner.to_pixels(reference_height)))
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::NodeType;

    #[test]
    fn test_ua_css_p_tag_properties() {
        // Create an empty CssPropertyCache
        let cache = CssPropertyCache::empty(1);
        
        // Create a minimal <p> tag NodeData using public API
        let mut node_data = NodeData::new(NodeType::P);
        
        let node_id = NodeId::new(0);
        let node_state = StyledNodeState::default();
        
        // Test that <p> has display: block from UA CSS
        let display = cache.get_display(&node_data, &node_id, &node_state);
        assert!(display.is_some(), "Expected <p> to have display property from UA CSS");
        if let Some(d) = display {
            if let Some(display_value) = d.get_property() {
                assert_eq!(*display_value, LayoutDisplay::Block, 
                    "Expected <p> to have display: block, got {:?}", display_value);
            }
        }
        
        // Test that <p> has width: 100% from UA CSS
        let width = cache.get_width(&node_data, &node_id, &node_state);
        assert!(width.is_some(), "Expected <p> to have width property from UA CSS");
        if let Some(w) = width {
            println!("Width value: {:?}", w);
        }
        
        // Test that <p> does NOT have a default height from UA CSS
        // (height should be auto, which means None)
        let height = cache.get_height(&node_data, &node_id, &node_state);
        println!("Height for <p> tag: {:?}", height);
        
        // Height should be None because <p> should use auto height
        assert!(height.is_none(), 
            "Expected <p> to NOT have explicit height (should be auto), but got {:?}", height);
    }

    #[test]
    fn test_ua_css_body_tag_properties() {
        let cache = CssPropertyCache::empty(1);
        
        let node_data = NodeData::new(NodeType::Body);
        
        let node_id = NodeId::new(0);
        let node_state = StyledNodeState::default();
        
        // Test that <body> has width: 100% from UA CSS
        let width = cache.get_width(&node_data, &node_id, &node_state);
        assert!(width.is_some(), "Expected <body> to have width: 100% from UA CSS");
        
        // Test that <body> has height: 100% from UA CSS
        let height = cache.get_height(&node_data, &node_id, &node_state);
        assert!(height.is_some(), "Expected <body> to have height: 100% from UA CSS");
        
        // Test margins are zero
        let margin_top = cache.get_margin_top(&node_data, &node_id, &node_state);
        assert!(margin_top.is_some(), "Expected <body> to have margin-top: 0 from UA CSS");
    }
}
