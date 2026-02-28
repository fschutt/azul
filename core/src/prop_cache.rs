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
//!
//! 1. Checks if the property is cached
//! 2. If not, resolves it from CSS rules + inline styles
//! 3. Caches the result for subsequent frames
//!
//! # Thread Safety
//!
//! Not thread-safe. Each window has its own cache instance.

extern crate alloc;

use alloc::{boxed::Box, string::String, vec::Vec};

use crate::dom::NodeType;

/// Tracks the origin of a CSS property value.
/// Used to correctly implement the CSS cascade and inheritance rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssPropertyOrigin {
    /// Property was inherited from parent node (only for inheritable properties)
    Inherited,
    /// Property is the node's own value (from UA CSS, CSS file, inline style, or user override)
    Own,
}

/// A CSS property with its origin tracking.
#[derive(Debug, Clone, PartialEq)]
pub struct CssPropertyWithOrigin {
    pub property: CssProperty,
    pub origin: CssPropertyOrigin,
}

use azul_css::{
    css::{Css, CssPath},
    props::{
        basic::{StyleFontFamily, StyleFontFamilyVec, StyleFontSize},
        layout::{LayoutDisplay, LayoutHeight, LayoutWidth},
        property::{
            BoxDecorationBreakValue, BreakInsideValue, CaretAnimationDurationValue,
            CaretColorValue, CaretWidthValue, ClipPathValue, ColumnCountValue, ColumnFillValue,
            ColumnRuleColorValue, ColumnRuleStyleValue, ColumnRuleWidthValue, ColumnSpanValue,
            ColumnWidthValue, ContentValue, CounterIncrementValue, CounterResetValue, CssProperty,
            CssPropertyType, FlowFromValue, FlowIntoValue, LayoutAlignContentValue,
            LayoutAlignItemsValue, LayoutAlignSelfValue, LayoutBorderBottomWidthValue,
            LayoutBorderLeftWidthValue, LayoutBorderRightWidthValue, LayoutBorderSpacingValue,
            LayoutBorderTopWidthValue, LayoutBoxSizingValue, LayoutClearValue,
            LayoutColumnGapValue, LayoutDisplayValue, LayoutFlexBasisValue,
            LayoutFlexDirectionValue, LayoutFlexGrowValue, LayoutFlexShrinkValue,
            LayoutFlexWrapValue, LayoutFloatValue, LayoutGapValue, LayoutGridAutoColumnsValue,
            LayoutGridAutoFlowValue, LayoutGridAutoRowsValue, LayoutGridColumnValue,
            LayoutGridRowValue, LayoutGridTemplateColumnsValue, LayoutGridTemplateRowsValue,
            LayoutHeightValue, LayoutInsetBottomValue, LayoutJustifyContentValue,
            LayoutJustifyItemsValue, LayoutJustifySelfValue, LayoutLeftValue,
            LayoutMarginBottomValue, LayoutMarginLeftValue, LayoutMarginRightValue,
            LayoutMarginTopValue, LayoutMaxHeightValue, LayoutMaxWidthValue, LayoutMinHeightValue,
            LayoutMinWidthValue, LayoutOverflowValue, LayoutPaddingBottomValue,
            LayoutPaddingLeftValue, LayoutPaddingRightValue, LayoutPaddingTopValue,
            LayoutPositionValue, LayoutRightValue, LayoutRowGapValue, LayoutScrollbarWidthValue,
            LayoutTableLayoutValue, LayoutTextJustifyValue, LayoutTopValue, LayoutWidthValue,
            LayoutWritingModeValue, LayoutZIndexValue, OrphansValue, PageBreakValue,
            ScrollbarStyleValue, ScrollbarFadeDelayValue, ScrollbarFadeDurationValue,
            ScrollbarVisibilityModeValue, SelectionBackgroundColorValue, SelectionColorValue,
            SelectionRadiusValue, ShapeImageThresholdValue, ShapeInsideValue, ShapeMarginValue,
            ShapeOutsideValue, StringSetValue, StyleBackfaceVisibilityValue,
            StyleBackgroundContentVecValue, StyleBackgroundPositionVecValue,
            StyleBackgroundRepeatVecValue, StyleBackgroundSizeVecValue,
            StyleBorderBottomColorValue, StyleBorderBottomLeftRadiusValue,
            StyleBorderBottomRightRadiusValue, StyleBorderBottomStyleValue,
            StyleBorderCollapseValue, StyleBorderLeftColorValue, StyleBorderLeftStyleValue,
            StyleBorderRightColorValue, StyleBorderRightStyleValue, StyleBorderTopColorValue,
            StyleBorderTopLeftRadiusValue, StyleBorderTopRightRadiusValue,
            StyleBorderTopStyleValue, StyleBoxShadowValue, StyleCaptionSideValue, StyleCursorValue,
            StyleDirectionValue, StyleEmptyCellsValue, StyleExclusionMarginValue,
            StyleFilterVecValue, StyleFontFamilyVecValue, StyleFontSizeValue, StyleFontStyleValue,
            StyleFontValue, StyleFontWeightValue, StyleHangingPunctuationValue,
            StyleHyphenationLanguageValue, StyleHyphensValue, StyleInitialLetterValue,
            StyleLetterSpacingValue, StyleLineClampValue, StyleLineHeightValue,
            StyleListStylePositionValue, StyleListStyleTypeValue, StyleMixBlendModeValue,
            StyleOpacityValue, StylePerspectiveOriginValue, StyleScrollbarColorValue,
            StyleTabSizeValue, StyleTextAlignValue, StyleTextColorValue,
            StyleTextCombineUprightValue, StyleTextDecorationValue, StyleTextIndentValue,
            StyleTransformOriginValue, StyleTransformVecValue, StyleUserSelectValue,
            StyleVerticalAlignValue, StyleVisibilityValue, StyleWhiteSpaceValue,
            StyleWordSpacingValue, WidowsValue,
        },
        style::{StyleCursor, StyleTextColor, StyleTransformOrigin},
    },
    AzString,
};

use crate::{
    dom::{NodeData, NodeId, TabIndex, TagId},
    id::{NodeDataContainer, NodeDataContainerRef},
    style::CascadeInfo,
    styled_dom::{
        NodeHierarchyItem, NodeHierarchyItemId, NodeHierarchyItemVec, ParentWithNodeDepth,
        ParentWithNodeDepthVec, StyledNodeState, TagIdToNodeIdMapping,
    },
};

use azul_css::dynamic_selector::{
    CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelectorContext,
};

/// Macro to match on any CssProperty variant and access the inner CssPropertyValue<T>.
/// This allows generic operations on cascade keywords without writing 190+ match arms.
macro_rules! match_property_value {
    ($property:expr, $value:ident, $expr:expr) => {
        match $property {
            CssProperty::CaretColor($value) => $expr,
            CssProperty::CaretAnimationDuration($value) => $expr,
            CssProperty::SelectionBackgroundColor($value) => $expr,
            CssProperty::SelectionColor($value) => $expr,
            CssProperty::SelectionRadius($value) => $expr,
            CssProperty::TextColor($value) => $expr,
            CssProperty::FontSize($value) => $expr,
            CssProperty::FontFamily($value) => $expr,
            CssProperty::FontWeight($value) => $expr,
            CssProperty::FontStyle($value) => $expr,
            CssProperty::TextAlign($value) => $expr,
            CssProperty::TextJustify($value) => $expr,
            CssProperty::VerticalAlign($value) => $expr,
            CssProperty::LetterSpacing($value) => $expr,
            CssProperty::TextIndent($value) => $expr,
            CssProperty::InitialLetter($value) => $expr,
            CssProperty::LineClamp($value) => $expr,
            CssProperty::HangingPunctuation($value) => $expr,
            CssProperty::TextCombineUpright($value) => $expr,
            CssProperty::ExclusionMargin($value) => $expr,
            CssProperty::HyphenationLanguage($value) => $expr,
            CssProperty::LineHeight($value) => $expr,
            CssProperty::WordSpacing($value) => $expr,
            CssProperty::TabSize($value) => $expr,
            CssProperty::WhiteSpace($value) => $expr,
            CssProperty::Hyphens($value) => $expr,
            CssProperty::Direction($value) => $expr,
            CssProperty::UserSelect($value) => $expr,
            CssProperty::TextDecoration($value) => $expr,
            CssProperty::Cursor($value) => $expr,
            CssProperty::Display($value) => $expr,
            CssProperty::Float($value) => $expr,
            CssProperty::BoxSizing($value) => $expr,
            CssProperty::Width($value) => $expr,
            CssProperty::Height($value) => $expr,
            CssProperty::MinWidth($value) => $expr,
            CssProperty::MinHeight($value) => $expr,
            CssProperty::MaxWidth($value) => $expr,
            CssProperty::MaxHeight($value) => $expr,
            CssProperty::Position($value) => $expr,
            CssProperty::Top($value) => $expr,
            CssProperty::Right($value) => $expr,
            CssProperty::Left($value) => $expr,
            CssProperty::Bottom($value) => $expr,
            CssProperty::ZIndex($value) => $expr,
            CssProperty::FlexWrap($value) => $expr,
            CssProperty::FlexDirection($value) => $expr,
            CssProperty::FlexGrow($value) => $expr,
            CssProperty::FlexShrink($value) => $expr,
            CssProperty::FlexBasis($value) => $expr,
            CssProperty::JustifyContent($value) => $expr,
            CssProperty::AlignItems($value) => $expr,
            CssProperty::AlignContent($value) => $expr,
            CssProperty::AlignSelf($value) => $expr,
            CssProperty::JustifyItems($value) => $expr,
            CssProperty::JustifySelf($value) => $expr,
            CssProperty::BackgroundContent($value) => $expr,
            CssProperty::BackgroundPosition($value) => $expr,
            CssProperty::BackgroundSize($value) => $expr,
            CssProperty::BackgroundRepeat($value) => $expr,
            CssProperty::OverflowX($value) => $expr,
            CssProperty::OverflowY($value) => $expr,
            CssProperty::PaddingTop($value) => $expr,
            CssProperty::PaddingLeft($value) => $expr,
            CssProperty::PaddingRight($value) => $expr,
            CssProperty::PaddingBottom($value) => $expr,
            CssProperty::MarginTop($value) => $expr,
            CssProperty::MarginLeft($value) => $expr,
            CssProperty::MarginRight($value) => $expr,
            CssProperty::MarginBottom($value) => $expr,
            CssProperty::BorderTopLeftRadius($value) => $expr,
            CssProperty::BorderTopRightRadius($value) => $expr,
            CssProperty::BorderBottomLeftRadius($value) => $expr,
            CssProperty::BorderBottomRightRadius($value) => $expr,
            CssProperty::BorderTopColor($value) => $expr,
            CssProperty::BorderRightColor($value) => $expr,
            CssProperty::BorderLeftColor($value) => $expr,
            CssProperty::BorderBottomColor($value) => $expr,
            CssProperty::BorderTopStyle($value) => $expr,
            CssProperty::BorderRightStyle($value) => $expr,
            CssProperty::BorderLeftStyle($value) => $expr,
            CssProperty::BorderBottomStyle($value) => $expr,
            CssProperty::BorderTopWidth($value) => $expr,
            CssProperty::BorderRightWidth($value) => $expr,
            CssProperty::BorderLeftWidth($value) => $expr,
            CssProperty::BorderBottomWidth($value) => $expr,
            CssProperty::BoxShadow($value) => $expr,
            CssProperty::Opacity($value) => $expr,
            CssProperty::Transform($value) => $expr,
            CssProperty::TransformOrigin($value) => $expr,
            CssProperty::PerspectiveOrigin($value) => $expr,
            CssProperty::BackfaceVisibility($value) => $expr,
            CssProperty::MixBlendMode($value) => $expr,
            CssProperty::Filter($value) => $expr,
            CssProperty::Visibility($value) => $expr,
            CssProperty::WritingMode($value) => $expr,
            CssProperty::GridTemplateColumns($value) => $expr,
            CssProperty::GridTemplateRows($value) => $expr,
            CssProperty::GridAutoColumns($value) => $expr,
            CssProperty::GridAutoRows($value) => $expr,
            CssProperty::GridAutoFlow($value) => $expr,
            CssProperty::GridColumn($value) => $expr,
            CssProperty::GridRow($value) => $expr,
            CssProperty::GridTemplateAreas($value) => $expr,
            CssProperty::Gap($value) => $expr,
            CssProperty::ColumnGap($value) => $expr,
            CssProperty::RowGap($value) => $expr,
            CssProperty::Clear($value) => $expr,
            CssProperty::ScrollbarStyle($value) => $expr,
            CssProperty::ScrollbarWidth($value) => $expr,
            CssProperty::ScrollbarColor($value) => $expr,
            CssProperty::ListStyleType($value) => $expr,
            CssProperty::ListStylePosition($value) => $expr,
            CssProperty::Font($value) => $expr,
            CssProperty::ColumnCount($value) => $expr,
            CssProperty::ColumnWidth($value) => $expr,
            CssProperty::ColumnSpan($value) => $expr,
            CssProperty::ColumnFill($value) => $expr,
            CssProperty::ColumnRuleStyle($value) => $expr,
            CssProperty::ColumnRuleWidth($value) => $expr,
            CssProperty::ColumnRuleColor($value) => $expr,
            CssProperty::FlowInto($value) => $expr,
            CssProperty::FlowFrom($value) => $expr,
            CssProperty::ShapeOutside($value) => $expr,
            CssProperty::ShapeInside($value) => $expr,
            CssProperty::ShapeImageThreshold($value) => $expr,
            CssProperty::ShapeMargin($value) => $expr,
            CssProperty::ClipPath($value) => $expr,
            CssProperty::Content($value) => $expr,
            CssProperty::CounterIncrement($value) => $expr,
            CssProperty::CounterReset($value) => $expr,
            CssProperty::StringSet($value) => $expr,
            CssProperty::Orphans($value) => $expr,
            CssProperty::Widows($value) => $expr,
            CssProperty::PageBreakBefore($value) => $expr,
            CssProperty::PageBreakAfter($value) => $expr,
            CssProperty::PageBreakInside($value) => $expr,
            CssProperty::BreakInside($value) => $expr,
            CssProperty::BoxDecorationBreak($value) => $expr,
            CssProperty::TableLayout($value) => $expr,
            CssProperty::BorderCollapse($value) => $expr,
            CssProperty::BorderSpacing($value) => $expr,
            CssProperty::CaptionSide($value) => $expr,
            CssProperty::EmptyCells($value) => $expr,
        }
    };
}

/// Returns the CSS-specified initial value for a given property type.
/// These are the default values defined by the CSS specification, not UA stylesheet values.
fn get_initial_value(property_type: CssPropertyType) -> Option<CssProperty> {
    use azul_css::css::CssPropertyValue;

    // For now, we return None for most properties and implement only the most critical ones.
    // This can be expanded as needed.
    match property_type {
        // Most properties: return None (no initial value implemented yet)
        // This means cascade keywords will fall back to parent values or remain unresolved
        _ => None,
    }
}

/// A CSS property tagged with its pseudo-state and property type.
/// Replaces the per-pseudo-state BTreeMap approach: instead of 6 BTreeMaps
/// per node (Normal/Hover/Active/Focus/Dragging/DragOver), we store one Vec
/// per node and tag each property with its state. Lookups use `.iter().find()`.
#[derive(Debug, Clone, PartialEq)]
pub struct StatefulCssProperty {
    pub state: azul_css::dynamic_selector::PseudoStateType,
    pub prop_type: CssPropertyType,
    pub property: CssProperty,
}

/// Compact, pre-sorted representation of all inline CSS properties for a single unique style set.
///
/// Stores properties separated by pseudo-state in sorted Vecs for O(log n) binary search.
/// Only handles pure pseudo-state conditions (consistent with `get_property_slow`).
/// Properties with complex conditions (@os, @media, etc.) are excluded since
/// `get_property_slow` cannot match them anyway.
///
/// One entry per unique inline style set; multiple nodes with identical `css_props`
/// content share the same entry via `CssPropertyCache::inline_style_keys`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompactInlineProps {
    /// Unconditional properties (`apply_if` is empty). Sorted by CssPropertyType.
    pub normal: Vec<(CssPropertyType, CssProperty)>,
    /// Properties that apply on `:hover`. Sorted by CssPropertyType.
    pub hover: Vec<(CssPropertyType, CssProperty)>,
    /// Properties that apply on `:active`. Sorted by CssPropertyType.
    pub active: Vec<(CssPropertyType, CssProperty)>,
    /// Properties that apply on `:focus`. Sorted by CssPropertyType.
    pub focus: Vec<(CssPropertyType, CssProperty)>,
    /// Properties that apply while `:dragging`. Sorted by CssPropertyType.
    pub dragging: Vec<(CssPropertyType, CssProperty)>,
    /// Properties that apply on `:drag-over`. Sorted by CssPropertyType.
    pub drag_over: Vec<(CssPropertyType, CssProperty)>,
}

impl CompactInlineProps {
    /// Look up a property by type in one of the sorted state slices.
    #[inline]
    pub fn find_in_state<'a>(
        props: &'a [(CssPropertyType, CssProperty)],
        prop_type: &CssPropertyType,
    ) -> Option<&'a CssProperty> {
        props
            .binary_search_by_key(prop_type, |(k, _)| *k)
            .ok()
            .map(|idx| &props[idx].1)
    }

    /// Insert or overwrite a property in a sorted state slice ("last wins" semantics).
    fn insert_sorted(
        props: &mut Vec<(CssPropertyType, CssProperty)>,
        prop_type: CssPropertyType,
        property: CssProperty,
    ) {
        match props.binary_search_by_key(&prop_type, |(k, _)| *k) {
            Ok(idx) => props[idx] = (prop_type, property),
            Err(idx) => props.insert(idx, (prop_type, property)),
        }
    }
}

/// Deduplicated table of inline CSS property sets.
///
/// Built once during `StyledDom::create()` from all nodes' `css_props`.
/// Nodes with identical inline styles share the same `CompactInlineProps` entry,
/// referenced via `CssPropertyCache::inline_style_keys`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InlineStyleTable {
    /// Dense storage of unique compact inline style sets.
    pub entries: Vec<CompactInlineProps>,
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
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CssPropertyCache {
    // number of nodes in the current DOM
    pub node_count: usize,

    // properties that were overridden in callbacks (not specific to any node state)
    pub user_overridden_properties: Vec<Vec<(CssPropertyType, CssProperty)>>,

    // non-default CSS properties that were cascaded from the parent,
    // unified across all pseudo-states (Normal, Hover, Active, Focus, Dragging, DragOver).
    // Each inner Vec contains StatefulCssProperty entries tagged with their pseudo-state.
    pub cascaded_props: Vec<Vec<StatefulCssProperty>>,

    // non-default CSS properties that were set via a CSS file,
    // unified across all pseudo-states.
    pub css_props: Vec<Vec<StatefulCssProperty>>,

    // Pre-resolved inherited properties (sorted Vec per node, keyed by CssPropertyType)
    pub computed_values: Vec<Vec<(CssPropertyType, CssPropertyWithOrigin)>>,

    // Pre-resolved property cache: Vec indexed by node ID, inner Vec sorted by CssPropertyType.
    // NOTE: This is now stored inside compact_cache.tier3_overflow instead of as a separate field.
    // pub resolved_cache: Vec<Vec<(CssPropertyType, CssProperty)>>,

    // Compact layout cache: three-tier numeric encoding for O(1) layout lookups.
    // Built once after restyle + apply_ua_css + compute_inherited_values.
    pub compact_cache: Option<azul_css::compact_cache::CompactLayoutCache>,

    /// Deduplicated inline style table.
    /// Built once during StyledDom::create() before build_resolved_cache().
    /// Maps unique inline style content to a compact pre-sorted representation.
    pub inline_style_table: InlineStyleTable,

    /// Per-node index into `inline_style_table.entries`.
    /// `u32::MAX` means this node has no inline styles.
    /// Length equals `node_count` after `build_inline_style_table()` is called.
    pub inline_style_keys: Vec<u32>,
}

impl CssPropertyCache {
    /// Look up a CSS property for a specific pseudo-state in a stateful property vec.
    /// Requires the vec to be sorted by (state, prop_type).
    #[inline]
    fn find_in_stateful<'a>(
        props: &'a [StatefulCssProperty],
        state: azul_css::dynamic_selector::PseudoStateType,
        prop_type: &CssPropertyType,
    ) -> Option<&'a CssProperty> {
        let key = (state, *prop_type);
        props.binary_search_by_key(&key, |p| (p.state, p.prop_type))
            .ok()
            .map(|idx| &props[idx].property)
    }

    /// Check if any properties exist for a specific pseudo-state in a stateful property vec.
    /// Requires the vec to be sorted by (state, prop_type).
    #[inline]
    fn has_state_props(
        props: &[StatefulCssProperty],
        state: azul_css::dynamic_selector::PseudoStateType,
    ) -> bool {
        // All entries with the same state are contiguous. Use partition_point
        // to find the first entry >= state, then check if it matches.
        let i = props.partition_point(|p| p.state < state);
        i < props.len() && props[i].state == state
    }

    /// Collect all property types for a specific pseudo-state.
    pub(crate) fn prop_types_for_state<'a>(
        props: &'a [StatefulCssProperty],
        state: azul_css::dynamic_selector::PseudoStateType,
    ) -> impl Iterator<Item = &'a CssPropertyType> + 'a {
        props.iter().filter(move |p| p.state == state).map(|p| &p.prop_type)
    }
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

            let css_dragging_rules: NodeDataContainer<(NodeId, Vec<CssProperty>)> = node_data
                .transform_nodeid_multithreaded_optional(|node_id| {
                    let r = filter_rules!(Some(Dragging), node_id);
                    if r.is_empty() {
                        None
                    } else {
                        Some((node_id, r))
                    }
                });

            let css_drag_over_rules: NodeDataContainer<(NodeId, Vec<CssProperty>)> = node_data
                .transform_nodeid_multithreaded_optional(|node_id| {
                    let r = filter_rules!(Some(DragOver), node_id);
                    if r.is_empty() {
                        None
                    } else {
                        Some((node_id, r))
                    }
                });

            // Assign CSS rules to unified Vec-based storage (indexed by NodeId)
            // Each rule gets tagged with its pseudo-state
            macro_rules! assign_css_rules_stateful {
                ($rules:expr, $state:expr) => {{
                    for (n, props) in $rules.internal.into_iter() {
                        let node_vec = &mut self.css_props[n.index()];
                        for prop in props.into_iter() {
                            node_vec.push(StatefulCssProperty {
                                state: $state,
                                prop_type: prop.get_type(),
                                property: prop,
                            });
                        }
                    }
                }};
            }

            // Clear all css_props before re-assigning
            for entry in self.css_props.iter_mut() { entry.clear(); }

            use azul_css::dynamic_selector::PseudoStateType;
            assign_css_rules_stateful!(css_normal_rules, PseudoStateType::Normal);
            assign_css_rules_stateful!(css_hover_rules, PseudoStateType::Hover);
            assign_css_rules_stateful!(css_active_rules, PseudoStateType::Active);
            assign_css_rules_stateful!(css_focus_rules, PseudoStateType::Focus);
            assign_css_rules_stateful!(css_dragging_rules, PseudoStateType::Dragging);
            assign_css_rules_stateful!(css_drag_over_rules, PseudoStateType::DragOver);
        }

        // Inheritance: Inherit all values of the parent to the children, but
        // only if the property is inheritable and isn't yet set
        for ParentWithNodeDepth { depth: _, node_id } in non_leaf_nodes.iter() {
            let parent_id = match node_id.into_crate_internal() {
                Some(s) => s,
                None => continue,
            };

            use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};

            let all_states = [
                PseudoStateType::Normal,
                PseudoStateType::Hover,
                PseudoStateType::Active,
                PseudoStateType::Focus,
                PseudoStateType::Dragging,
                PseudoStateType::DragOver,
            ];

            for &state in &all_states {
                // 1. Inherit inline CSS properties from parent for this pseudo-state
                let parent_inheritable_inline: Vec<(CssPropertyType, CssProperty)> = node_data[parent_id]
                    .css_props
                    .iter()
                    .filter(|css_prop| {
                        let conditions = css_prop.apply_if.as_slice();
                        if conditions.is_empty() {
                            state == PseudoStateType::Normal
                        } else {
                            conditions.iter().all(|c| {
                                matches!(c, DynamicSelector::PseudoState(s) if *s == state)
                            })
                        }
                    })
                    .map(|css_prop| &css_prop.property)
                    .filter(|css_prop| css_prop.get_type().is_inheritable())
                    .map(|p| (p.get_type(), p.clone()))
                    .collect();

                // 2. Inherit CSS stylesheet properties from parent for this pseudo-state
                let parent_inheritable_css: Vec<(CssPropertyType, CssProperty)> = if !css_is_empty {
                    self.css_props[parent_id.index()]
                        .iter()
                        .filter(|p| p.state == state && p.prop_type.is_inheritable())
                        .map(|p| (p.prop_type, p.property.clone()))
                        .collect()
                } else {
                    Vec::new()
                };

                // 3. Inherit cascaded properties from parent for this pseudo-state
                let parent_inheritable_cascaded: Vec<(CssPropertyType, CssProperty)> =
                    self.cascaded_props[parent_id.index()]
                        .iter()
                        .filter(|p| p.state == state && p.prop_type.is_inheritable())
                        .map(|p| (p.prop_type, p.property.clone()))
                        .collect();

                // Combine all inheritable props (inline first = strongest, cascaded last)
                // Only insert if child doesn't already have that (state, prop_type) combo
                if parent_inheritable_inline.is_empty()
                    && parent_inheritable_css.is_empty()
                    && parent_inheritable_cascaded.is_empty()
                {
                    continue;
                }

                for child_id in parent_id.az_children(&node_hierarchy.as_container()) {
                    let child_vec = &mut self.cascaded_props[child_id.index()];
                    for (prop_type, prop_value) in parent_inheritable_inline
                        .iter()
                        .chain(parent_inheritable_css.iter())
                        .chain(parent_inheritable_cascaded.iter())
                    {
                        // or_insert: only insert if child doesn't already have this (state, prop_type)
                        if !child_vec.iter().any(|p| p.state == state && p.prop_type == *prop_type) {
                            child_vec.push(StatefulCssProperty {
                                state,
                                prop_type: *prop_type,
                                property: prop_value.clone(),
                            });
                        }
                    }
                }
            }
        }

        // Sort css_props and cascaded_props by (state, prop_type) for binary search lookups.
        // NOTE: Only sort css_props here. cascaded_props will be sorted after apply_ua_css()
        // since apply_ua_css() adds more entries to cascaded_props.
        for v in self.css_props.iter_mut() {
            v.sort_unstable_by_key(|p| (p.state, p.prop_type));
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
        
        // Keep a reference to the node data container for use in the closure
        let node_data_container = &node_data.internal;
        
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
                    let node_has_hover_props = {
                        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
                        node_data.css_props.as_ref().iter().any(|p| {
                            p.apply_if.as_slice().iter().any(|c| {
                                matches!(c, DynamicSelector::PseudoState(PseudoStateType::Hover))
                            })
                        })
                    } || Self::has_state_props(
                            self.css_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
                            azul_css::dynamic_selector::PseudoStateType::Hover,
                        )
                        || Self::has_state_props(
                            self.cascaded_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
                            azul_css::dynamic_selector::PseudoStateType::Hover,
                        );

                    if node_has_hover_props {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for :active
                    let node_has_active_props = {
                        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
                        node_data.css_props.as_ref().iter().any(|p| {
                            p.apply_if.as_slice().iter().any(|c| {
                                matches!(c, DynamicSelector::PseudoState(PseudoStateType::Active))
                            })
                        })
                    } || Self::has_state_props(
                            self.css_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
                            azul_css::dynamic_selector::PseudoStateType::Active,
                        )
                        || Self::has_state_props(
                            self.cascaded_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
                            azul_css::dynamic_selector::PseudoStateType::Active,
                        );

                    if node_has_active_props {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for :focus
                    let node_has_focus_props = {
                        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
                        node_data.css_props.as_ref().iter().any(|p| {
                            p.apply_if.as_slice().iter().any(|c| {
                                matches!(c, DynamicSelector::PseudoState(PseudoStateType::Focus))
                            })
                        })
                    } || Self::has_state_props(
                            self.css_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
                            azul_css::dynamic_selector::PseudoStateType::Focus,
                        )
                        || Self::has_state_props(
                            self.cascaded_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
                            azul_css::dynamic_selector::PseudoStateType::Focus,
                        );

                    if node_has_focus_props {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for :dragging
                    let node_has_dragging_props = {
                        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
                        node_data.css_props.as_ref().iter().any(|p| {
                            p.apply_if.as_slice().iter().any(|c| {
                                matches!(c, DynamicSelector::PseudoState(PseudoStateType::Dragging))
                            })
                        })
                    } || Self::has_state_props(
                            self.css_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
                            azul_css::dynamic_selector::PseudoStateType::Dragging,
                        )
                        || Self::has_state_props(
                            self.cascaded_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
                            azul_css::dynamic_selector::PseudoStateType::Dragging,
                        );

                    if node_has_dragging_props {
                        node_should_have_tag = true;
                        break;
                    }

                    // check for :drag-over
                    let node_has_drag_over_props = {
                        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
                        node_data.css_props.as_ref().iter().any(|p| {
                            p.apply_if.as_slice().iter().any(|c| {
                                matches!(c, DynamicSelector::PseudoState(PseudoStateType::DragOver))
                            })
                        })
                    } || Self::has_state_props(
                            self.css_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
                            azul_css::dynamic_selector::PseudoStateType::DragOver,
                        )
                        || Self::has_state_props(
                            self.cascaded_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
                            azul_css::dynamic_selector::PseudoStateType::DragOver,
                        );

                    if node_has_drag_over_props {
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

                    // check for overflow: scroll or overflow: auto - needed for scroll hit-testing
                    // Nodes with these overflow values need hit-test tags so that
                    // scroll wheel events can be correctly assigned to scrollable containers
                    let node_has_overflow_scroll = {
                        use azul_css::props::layout::LayoutOverflow;
                        let overflow_x = self
                            .get_overflow_x(&node_data, &node_id, &default_node_state)
                            .and_then(|p| p.get_property_or_default());
                        let overflow_y = self
                            .get_overflow_y(&node_data, &node_id, &default_node_state)
                            .and_then(|p| p.get_property_or_default());

                        let x_scrollable = matches!(
                            overflow_x,
                            Some(LayoutOverflow::Scroll | LayoutOverflow::Auto)
                        );
                        let y_scrollable = matches!(
                            overflow_y,
                            Some(LayoutOverflow::Scroll | LayoutOverflow::Auto)
                        );
                        x_scrollable || y_scrollable
                    };

                    if node_has_overflow_scroll {
                        node_should_have_tag = true;
                        break;
                    }

                    // Check for selectable text - nodes that contain text children and
                    // user-select != none need hit-test tags for text selection support.
                    // The cursor resolution algorithm in CursorTypeHitTest ensures that
                    // explicit cursor properties (e.g., cursor:pointer on button) take
                    // precedence over the implicit I-beam from text children.
                    let node_has_selectable_text = {
                        use azul_css::props::style::StyleUserSelect;
                        use crate::dom::NodeType;
                        
                        // Check if this node has immediate text children
                        let has_text_children = {
                            let hier = node_hierarchy.as_container()[node_id];
                            let mut has_text = false;
                            if let Some(first_child) = hier.first_child_id(node_id) {
                                let mut child_id = Some(first_child);
                                while let Some(cid) = child_id {
                                    let child_data = &node_data_container[cid.index()];
                                    if matches!(child_data.get_node_type(), NodeType::Text(_)) {
                                        has_text = true;
                                        break;
                                    }
                                    child_id = node_hierarchy.as_container()[cid].next_sibling_id();
                                }
                            }
                            has_text
                        };
                        
                        if has_text_children {
                            // Check user-select property on this container (default is selectable)
                            let user_select = self
                                .get_user_select(&node_data, &node_id, &default_node_state)
                                .and_then(|p| p.get_property().cloned())
                                .unwrap_or(StyleUserSelect::Auto);
                            
                            !matches!(user_select, StyleUserSelect::None)
                        } else {
                            false
                        }
                    };

                    if node_has_selectable_text {
                        node_should_have_tag = true;
                        break;
                    }

                    break;
                }

                if !node_should_have_tag {
                    None
                } else {
                    Some(TagIdToNodeIdMapping {
                        tag_id: TagId::from_crate_internal(TagId::unique()),
                        node_id: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
                        tab_index: tab_index.into(),
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
        if let Some(p) = self.get_tab_size(&node_data, node_id, node_state) {
            s.push_str(&format!("tab-size: {};", p.get_css_value_fmt()));
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
            user_overridden_properties: vec![Vec::new(); node_count],

            cascaded_props: vec![Vec::new(); node_count],
            css_props: vec![Vec::new(); node_count],

            computed_values: vec![Vec::new(); node_count],
            compact_cache: None,
            inline_style_table: InlineStyleTable::default(),
            inline_style_keys: Vec::new(),
        }
    }

    pub fn append(&mut self, other: &mut Self) {
        macro_rules! append_css_property_vec {
            ($field_name:ident) => {{
                self.$field_name.extend(other.$field_name.drain(..));
            }};
        }

        append_css_property_vec!(user_overridden_properties);
        append_css_property_vec!(cascaded_props);
        append_css_property_vec!(css_props);
        append_css_property_vec!(computed_values);

        self.node_count += other.node_count;

        // Invalidate compact cache and inline style table since node IDs shifted.
        // Both will be rebuilt on the next StyledDom::create() / build_inline_style_table() call.
        self.compact_cache = None;
        self.inline_style_table = InlineStyleTable::default();
        self.inline_style_keys.clear();
    }

    /// Build the deduplicated inline style table from the given node data.
    ///
    /// Must be called after all nodes have been added but before `build_resolved_cache()`.
    /// Nodes with identical `css_props` content share one `CompactInlineProps` entry,
    /// referenced by index via `inline_style_keys`.
    ///
    /// Only pure pseudo-state conditions are preserved in the compact form
    /// (consistent with `get_property_slow`). Properties with @os, @media,
    /// or multi-condition rules are excluded — they are evaluated only via
    /// `get_property_with_context`.
    pub fn build_inline_style_table(&mut self, node_data: &[NodeData]) {
        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
        use core::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        use std::collections::HashMap;

        let mut hash_to_key: HashMap<u64, u32> = HashMap::new();
        let mut table = InlineStyleTable::default();
        let mut keys: Vec<u32> = Vec::with_capacity(node_data.len());

        for nd in node_data.iter() {
            let css_props_slice = nd.css_props.as_ref();

            if css_props_slice.is_empty() {
                keys.push(u32::MAX);
                continue;
            }

            // Hash the css_props content for deduplication.
            // We hash property type discriminants + condition structure, which is
            // sufficient to distinguish different inline style sets.
            let mut hasher = DefaultHasher::new();
            for prop in css_props_slice.iter() {
                // Hash property type (stable discriminant)
                prop.property.get_type().hash(&mut hasher);
                // Hash condition structure
                for cond in prop.apply_if.as_slice().iter() {
                    core::mem::discriminant(cond).hash(&mut hasher);
                    if let DynamicSelector::PseudoState(ps) = cond {
                        (*ps as u8).hash(&mut hasher);
                    }
                }
                prop.apply_if.as_slice().len().hash(&mut hasher);
            }
            let hash = hasher.finish();

            let key = *hash_to_key.entry(hash).or_insert_with(|| {
                let compact = Self::build_compact_inline_props(css_props_slice);
                let idx = table.entries.len() as u32;
                table.entries.push(compact);
                idx
            });

            keys.push(key);
        }

        self.inline_style_table = table;
        self.inline_style_keys = keys;
    }

    /// Convert a `CssPropertyWithConditions` slice into a `CompactInlineProps`.
    ///
    /// Only includes properties with:
    /// - Empty `apply_if` → Normal state
    /// - Exactly one `DynamicSelector::PseudoState(_)` condition → that state
    ///
    /// Properties with complex or multi-condition `apply_if` are excluded
    /// (they can't be matched by `get_property_slow` anyway).
    fn build_compact_inline_props(
        props: &[azul_css::dynamic_selector::CssPropertyWithConditions],
    ) -> CompactInlineProps {
        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};

        let mut result = CompactInlineProps::default();

        for prop in props.iter() {
            let conditions = prop.apply_if.as_slice();

            let pseudo_state = if conditions.is_empty() {
                PseudoStateType::Normal
            } else if conditions.len() == 1 {
                match &conditions[0] {
                    DynamicSelector::PseudoState(ps) => *ps,
                    _ => continue, // Non-pseudo-state single condition → skip
                }
            } else {
                // Multiple conditions: get_property_slow() can't match these
                continue;
            };

            let prop_type = prop.property.get_type();
            let target = match pseudo_state {
                PseudoStateType::Normal => &mut result.normal,
                PseudoStateType::Hover => &mut result.hover,
                PseudoStateType::Active => &mut result.active,
                PseudoStateType::Focus => &mut result.focus,
                PseudoStateType::Dragging => &mut result.dragging,
                PseudoStateType::DragOver => &mut result.drag_over,
                _ => &mut result.normal, // Disabled, Checked, etc. → treat as normal
            };
            CompactInlineProps::insert_sorted(target, prop_type, prop.property.clone());
        }

        result
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
        // Direct cascade resolution — no tier3_overflow clone needed.
        // Fix 3: tier3_overflow removed; the cascade layers (user_overridden_properties,
        // compact inline table, css_props, cascaded_props, UA CSS) are already sorted
        // Vecs that support O(log N) binary search, and the compact inline table gives
        // O(1) for inline styles. This avoids the per-node Vec<CssProperty> clone that
        // tier3_overflow required, saving ~5 MB for 500 nodes and eliminating the
        // build_resolved_cache() startup cost (~O(N × P × log P)).
        self.get_property_slow(node_data, node_id, node_state, css_property_type)
    }

    /// Full cascade resolution without using the resolved cache.
    /// Used by restyle functions and get_property().
    pub(crate) fn get_property_slow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        css_property_type: &CssPropertyType,
    ) -> Option<&'a CssProperty> {

        use azul_css::dynamic_selector::PseudoStateType;

        // First test if there is some user-defined override for the property
        if let Some(v) = self.user_overridden_properties.get(node_id.index()) {
            if let Ok(idx) = v.binary_search_by_key(css_property_type, |(k, _)| *k) {
                return Some(&v[idx].1);
            }
        }

        // Look up the compact inline style entry for this node (O(1)).
        // Falls back to None if build_inline_style_table() has not been called yet,
        // in which case we fall through to the linear-scan path at the end.
        let inline_compact: Option<&CompactInlineProps> = self
            .inline_style_keys
            .get(node_id.index())
            .copied()
            .filter(|&k| k != u32::MAX)
            .and_then(|k| self.inline_style_table.entries.get(k as usize));

        // Helper for fallback linear scan (used when inline_style_table is not yet built).
        fn matches_pseudo_state_slow(
            prop: &azul_css::dynamic_selector::CssPropertyWithConditions,
            state: PseudoStateType,
        ) -> bool {
            use azul_css::dynamic_selector::DynamicSelector;
            let conditions = prop.apply_if.as_slice();
            if conditions.is_empty() {
                state == PseudoStateType::Normal
            } else {
                conditions
                    .iter()
                    .all(|c| matches!(c, DynamicSelector::PseudoState(s) if *s == state))
            }
        }

        // Macro: check inline compact table (fast path) or linear scan (fallback),
        // then stylesheet + cascade for one pseudo-state.
        macro_rules! check_state {
            ($state_field:ident, $state_enum:expr, $compact_field:ident) => {{
                if node_state.$state_field {
                    // PRIORITY 1: Inline CSS
                    if let Some(compact) = inline_compact {
                        // Fast path: O(log n) binary search in pre-sorted compact table
                        if let Some(p) = CompactInlineProps::find_in_state(
                            &compact.$compact_field,
                            css_property_type,
                        ) {
                            return Some(p);
                        }
                    } else {
                        // Fallback: O(n) linear scan (table not yet built)
                        if let Some(p) =
                            node_data.css_props.as_ref().iter().find_map(|css_prop| {
                                if matches_pseudo_state_slow(css_prop, $state_enum)
                                    && css_prop.property.get_type() == *css_property_type
                                {
                                    Some(&css_prop.property)
                                } else {
                                    None
                                }
                            })
                        {
                            return Some(p);
                        }
                    }

                    // PRIORITY 2: CSS stylesheet properties
                    if let Some(p) = Self::find_in_stateful(
                        self.css_props
                            .get(node_id.index())
                            .map(|v| v.as_slice())
                            .unwrap_or(&[]),
                        $state_enum,
                        css_property_type,
                    ) {
                        return Some(p);
                    }

                    // PRIORITY 3: Cascaded/inherited properties
                    if let Some(p) = Self::find_in_stateful(
                        self.cascaded_props
                            .get(node_id.index())
                            .map(|v| v.as_slice())
                            .unwrap_or(&[]),
                        $state_enum,
                        css_property_type,
                    ) {
                        return Some(p);
                    }
                }
            }};
        }

        // Priority order: :focus > :active > :dragging > :drag-over > :hover > normal
        check_state!(focused,  PseudoStateType::Focus,    focus);
        check_state!(active,   PseudoStateType::Active,   active);
        check_state!(dragging, PseudoStateType::Dragging, dragging);
        check_state!(drag_over,PseudoStateType::DragOver, drag_over);
        check_state!(hover,    PseudoStateType::Hover,    hover);

        // Normal/fallback: always checked as the base layer.
        // PRIORITY 1: Inline CSS
        if let Some(compact) = inline_compact {
            // Fast path: O(log n) binary search
            if let Some(p) =
                CompactInlineProps::find_in_state(&compact.normal, css_property_type)
            {
                return Some(p);
            }
        } else {
            // Fallback: O(n) linear scan (table not yet built)
            if let Some(p) = node_data.css_props.as_ref().iter().find_map(|css_prop| {
                if matches_pseudo_state_slow(css_prop, PseudoStateType::Normal)
                    && css_prop.property.get_type() == *css_property_type
                {
                    Some(&css_prop.property)
                } else {
                    None
                }
            }) {
                return Some(p);
            }
        }

        // PRIORITY 2: CSS stylesheet properties
        if let Some(p) = Self::find_in_stateful(
            self.css_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
            PseudoStateType::Normal,
            css_property_type,
        ) {
            return Some(p);
        }

        // PRIORITY 3: Cascaded/inherited properties
        if let Some(p) = Self::find_in_stateful(
            self.cascaded_props.get(node_id.index()).map(|v| v.as_slice()).unwrap_or(&[]),
            PseudoStateType::Normal,
            css_property_type,
        ) {
            return Some(p);
        }

        // Check computed values cache for inherited properties
        // Sorted Vec with binary search
        if css_property_type.is_inheritable() {
            if let Some(vec) = self.computed_values.get(node_id.index()) {
                if let Ok(idx) = vec.binary_search_by_key(css_property_type, |(k, _)| *k) {
                    return Some(&vec[idx].1.property);
                }
            }
        }

        // User-agent stylesheet fallback (lowest precedence)
        // Check if the node type has a default value for this property
        crate::ua_css::get_ua_property(&node_data.node_type, *css_property_type)
    }

    /// Get a CSS property using DynamicSelectorContext for evaluation.
    ///
    /// This is the new API that supports @media queries, @container queries,
    /// OS-specific styles, and all pseudo-states via `CssPropertyWithConditions`.
    ///
    /// The evaluation follows "last wins" semantics - properties are evaluated
    /// in reverse order and the first matching property wins.
    pub fn get_property_with_context<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        context: &DynamicSelectorContext,
        css_property_type: &CssPropertyType,
    ) -> Option<&CssProperty> {
        // First test if there is some user-defined override for the property
        if let Some(v) = self.user_overridden_properties.get(node_id.index()) {
            if let Ok(idx) = v.binary_search_by_key(css_property_type, |(k, _)| *k) {
                return Some(&v[idx].1);
            }
        }

        // Check inline CSS properties with DynamicSelectorContext evaluation.
        // Iterate in REVERSE order - "last found wins" semantics.
        // This replaces the old Focus > Active > Hover > Normal priority chain.
        if let Some(prop_with_conditions) =
            node_data.css_props.as_ref().iter().rev().find(|prop| {
                prop.property.get_type() == *css_property_type && prop.matches(context)
            })
        {
            return Some(&prop_with_conditions.property);
        }

        // Fall back to CSS file and cascaded properties
        let legacy_state = StyledNodeState::from_pseudo_state_flags(&context.pseudo_state);
        if let Some(p) = self.get_property(node_data, node_id, &legacy_state, css_property_type) {
            return Some(p);
        }

        None
    }

    /// Check if any properties with conditions would change between two contexts.
    /// This is used for re-layout detection on viewport/container resize.
    pub fn check_properties_changed(
        node_data: &NodeData,
        old_context: &DynamicSelectorContext,
        new_context: &DynamicSelectorContext,
    ) -> bool {
        for prop in node_data.css_props.as_ref().iter() {
            let was_active = prop.matches(old_context);
            let is_active = prop.matches(new_context);
            if was_active != is_active {
                return true;
            }
        }
        false
    }

    /// Check if any layout-affecting properties would change between two contexts.
    /// This is a more targeted check for re-layout detection.
    pub fn check_layout_properties_changed(
        node_data: &NodeData,
        old_context: &DynamicSelectorContext,
        new_context: &DynamicSelectorContext,
    ) -> bool {
        for prop in node_data.css_props.as_ref().iter() {
            // Skip non-layout-affecting properties
            if !prop.is_layout_affecting() {
                continue;
            }

            let was_active = prop.matches(old_context);
            let is_active = prop.matches(new_context);
            if was_active != is_active {
                return true;
            }
        }
        false
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
    // Method for getting text-indent property
    pub fn get_text_indent<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextIndentValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextIndent)
            .and_then(|p| p.as_text_indent())
    }
    // Method for getting initial-letter property
    pub fn get_initial_letter<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleInitialLetterValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::InitialLetter,
        )
        .and_then(|p| p.as_initial_letter())
    }
    // Method for getting line-clamp property
    pub fn get_line_clamp<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleLineClampValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::LineClamp)
            .and_then(|p| p.as_line_clamp())
    }
    // Method for getting hanging-punctuation property
    pub fn get_hanging_punctuation<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleHangingPunctuationValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::HangingPunctuation,
        )
        .and_then(|p| p.as_hanging_punctuation())
    }
    // Method for getting text-combine-upright property
    pub fn get_text_combine_upright<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextCombineUprightValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TextCombineUpright,
        )
        .and_then(|p| p.as_text_combine_upright())
    }
    // Method for getting -azul-exclusion-margin property
    pub fn get_exclusion_margin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleExclusionMarginValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ExclusionMargin,
        )
        .and_then(|p| p.as_exclusion_margin())
    }
    // Method for getting -azul-hyphenation-language property
    pub fn get_hyphenation_language<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleHyphenationLanguageValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::HyphenationLanguage,
        )
        .and_then(|p| p.as_hyphenation_language())
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

    // Method for getting -azul-caret-width property
    pub fn get_caret_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CaretWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::CaretWidth)
            .and_then(|p| p.as_caret_width())
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

    // Method for getting -azul-selection-radius property
    pub fn get_selection_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a SelectionRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::SelectionRadius,
        )
        .and_then(|p| p.as_selection_radius())
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
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ShapeOutside,
        )
        .and_then(|p| p.as_shape_outside())
    }

    // Method for getting shape-inside property
    pub fn get_shape_inside<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeInsideValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ShapeInside,
        )
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

    // Method for getting -azul-scrollbar-visibility property
    pub fn get_scrollbar_visibility<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ScrollbarVisibilityModeValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarVisibility,
        )
        .and_then(|p| p.as_scrollbar_visibility())
    }

    // Method for getting -azul-scrollbar-fade-delay property
    pub fn get_scrollbar_fade_delay<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ScrollbarFadeDelayValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarFadeDelay,
        )
        .and_then(|p| p.as_scrollbar_fade_delay())
    }

    // Method for getting -azul-scrollbar-fade-duration property
    pub fn get_scrollbar_fade_duration<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ScrollbarFadeDurationValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarFadeDuration,
        )
        .and_then(|p| p.as_scrollbar_fade_duration())
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
    pub fn get_vertical_align<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleVerticalAlignValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::VerticalAlign,
        )
        .and_then(|p| p.as_vertical_align())
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
    pub fn get_tab_size<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTabSizeValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TabSize)
            .and_then(|p| p.as_tab_size())
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
    ) -> Option<&'a LayoutInsetBottomValue> {
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
        self.get_property(node_data, node_id, node_state, &CssPropertyType::BackdropFilter)
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
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ListStyleType,
        )
        .and_then(|p| p.as_list_style_type())
    }
    pub fn get_list_style_position<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleListStylePositionValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ListStylePosition,
        )
        .and_then(|p| p.as_list_style_position())
    }
    pub fn get_table_layout<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutTableLayoutValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TableLayout,
        )
        .and_then(|p| p.as_table_layout())
    }
    pub fn get_border_collapse<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderCollapseValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderCollapse,
        )
        .and_then(|p| p.as_border_collapse())
    }
    pub fn get_border_spacing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderSpacingValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderSpacing,
        )
        .and_then(|p| p.as_border_spacing())
    }
    pub fn get_caption_side<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleCaptionSideValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::CaptionSide,
        )
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
                LayoutWidth::Px(px) => Some(px.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                )),
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
            .and_then(|w| {
                Some(w.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|w| {
                Some(w.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
                LayoutHeight::Px(px) => Some(px.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                )),
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
            .and_then(|h| {
                Some(h.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|h| {
                Some(h.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|l| {
                Some(l.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
    }

    pub fn calc_right(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> Option<f32> {
        self.get_right(node_data, node_id, styled_node_state)
            .and_then(|r| {
                Some(r.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
    }

    pub fn calc_top(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> Option<f32> {
        self.get_top(node_data, node_id, styled_node_state)
            .and_then(|t| {
                Some(t.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
    }

    pub fn calc_bottom(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> Option<f32> {
        self.get_bottom(node_data, node_id, styled_node_state)
            .and_then(|b| {
                Some(b.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|b| {
                Some(b.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|b| {
                Some(b.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|b| {
                Some(b.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|b| {
                Some(b.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|p| {
                Some(p.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|p| {
                Some(p.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|p| {
                Some(p.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|p| {
                Some(p.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|m| {
                Some(m.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|m| {
                Some(m.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|m| {
                Some(m.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
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
            .and_then(|m| {
                Some(m.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    /// Helper function to resolve a CSS property value that may depend on another property.
    ///
    /// This attempts to compute a final pixel value from a property that uses relative units
    /// (em, %, etc.) by referencing another property value.
    ///
    /// # Arguments
    /// * `target_property` - The property to resolve (e.g., child's font-size: 2em)
    /// * `reference_property` - The property it depends on (e.g., parent's font-size: 16px)
    ///
    /// # Returns
    /// * `Some(CssProperty)` - A new property with absolute pixel values
    /// * `None` - If the property can't be resolved (missing data, incompatible types, etc.)
    ///
    /// # Examples
    /// - `resolve_property_dependency(font-size: 2em, font-size: 16px)` → `font-size: 32px`
    /// - `resolve_property_dependency(font-size: 150%, font-size: 20px)` → `font-size: 30px`
    /// - `resolve_property_dependency(padding: 2em, font-size: 16px)` → `padding: 32px`

    /// Resolves CSS cascade keywords (inherit, initial, revert, unset) for a property.
    ///
    /// According to CSS Cascade spec (https://css-tricks.com/inherit-initial-unset-revert/):
    /// - `inherit`: Use the parent's computed value (or initial value if no parent)
    /// - `initial`: Use the CSS-defined initial value (default for that property type)
    /// - `revert`: Roll back to the user-agent stylesheet value (if any)
    /// - `unset`: Behaves as `inherit` for inherited properties, `initial` for non-inherited
    ///   properties
    ///
    /// # Arguments
    /// * `property` - The property to resolve
    /// * `property_type` - The type of the property
    /// * `node_type` - The node type (for UA CSS lookup)
    /// * `parent_value` - The parent's computed value (for inheritance)
    /// * `ua_value` - The user-agent stylesheet value (for revert)
    ///
    /// # Returns
    /// * `Some(CssProperty)` - The resolved property
    /// * `None` - If the keyword doesn't apply or can't be resolved
    fn resolve_cascade_keyword(
        property: &CssProperty,
        property_type: CssPropertyType,
        _node_type: &crate::dom::NodeType,
        parent_value: Option<&CssProperty>,
        ua_value: Option<&'static CssProperty>,
    ) -> Option<CssProperty> {
        // For now, implement basic inheritance
        // Check if this is an inheritable property and return parent value
        if property_type.is_inheritable() {
            return parent_value.cloned().or_else(|| ua_value.cloned());
        } else {
            return ua_value.cloned();
        }
    }

    fn resolve_property_dependency(
        target_property: &CssProperty,
        reference_property: &CssProperty,
    ) -> Option<CssProperty> {
        use azul_css::{
            css::CssPropertyValue,
            props::{
                basic::{font::StyleFontSize, length::SizeMetric, pixel::PixelValue},
                layout::*,
                style::{SelectionRadius, StyleLetterSpacing, StyleWordSpacing},
            },
        };

        // Extract PixelValue from various property types (returns owned value)
        let get_pixel_value = |prop: &CssProperty| -> Option<PixelValue> {
            match prop {
                CssProperty::FontSize(val) => val.get_property().map(|v| v.inner),
                CssProperty::LetterSpacing(val) => val.get_property().map(|v| v.inner),
                CssProperty::WordSpacing(val) => val.get_property().map(|v| v.inner),
                CssProperty::PaddingLeft(val) => val.get_property().map(|v| v.inner),
                CssProperty::PaddingRight(val) => val.get_property().map(|v| v.inner),
                CssProperty::PaddingTop(val) => val.get_property().map(|v| v.inner),
                CssProperty::PaddingBottom(val) => val.get_property().map(|v| v.inner),
                CssProperty::MarginLeft(val) => val.get_property().map(|v| v.inner),
                CssProperty::MarginRight(val) => val.get_property().map(|v| v.inner),
                CssProperty::MarginTop(val) => val.get_property().map(|v| v.inner),
                CssProperty::MarginBottom(val) => val.get_property().map(|v| v.inner),
                CssProperty::MinWidth(val) => val.get_property().map(|v| v.inner),
                CssProperty::MinHeight(val) => val.get_property().map(|v| v.inner),
                CssProperty::MaxWidth(val) => val.get_property().map(|v| v.inner),
                CssProperty::MaxHeight(val) => val.get_property().map(|v| v.inner),
                CssProperty::SelectionRadius(val) => val.get_property().map(|v| v.inner),
                _ => None,
            }
        };

        let target_pixel_value = get_pixel_value(target_property)?;
        let reference_pixel_value = get_pixel_value(reference_property)?;

        // Convert reference to absolute pixels first
        let reference_px = match reference_pixel_value.metric {
            SizeMetric::Px => reference_pixel_value.number.get(),
            SizeMetric::Pt => reference_pixel_value.number.get() * 1.333333,
            SizeMetric::In => reference_pixel_value.number.get() * 96.0,
            SizeMetric::Cm => reference_pixel_value.number.get() * 37.7952755906,
            SizeMetric::Mm => reference_pixel_value.number.get() * 3.7795275591,
            SizeMetric::Em => return None, // Reference can't be relative
            SizeMetric::Rem => return None, // Reference can't be relative
            SizeMetric::Percent => return None, // Reference can't be relative
            // Reference can't be viewport-relative
            SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => return None,
        };

        // Resolve target based on reference
        let resolved_px = match target_pixel_value.metric {
            SizeMetric::Px => target_pixel_value.number.get(),
            SizeMetric::Pt => target_pixel_value.number.get() * 1.333333,
            SizeMetric::In => target_pixel_value.number.get() * 96.0,
            SizeMetric::Cm => target_pixel_value.number.get() * 37.7952755906,
            SizeMetric::Mm => target_pixel_value.number.get() * 3.7795275591,
            SizeMetric::Em => target_pixel_value.number.get() * reference_px,
            // Use reference as root font-size
            SizeMetric::Rem => target_pixel_value.number.get() * reference_px,
            SizeMetric::Percent => target_pixel_value.number.get() / 100.0 * reference_px,
            // Need viewport context
            SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => return None,
        };

        // Create a new property with the resolved value
        let resolved_pixel_value = PixelValue::px(resolved_px);

        match target_property {
            CssProperty::FontSize(_) => Some(CssProperty::FontSize(CssPropertyValue::Exact(
                StyleFontSize {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::LetterSpacing(_) => Some(CssProperty::LetterSpacing(
                CssPropertyValue::Exact(StyleLetterSpacing {
                    inner: resolved_pixel_value,
                }),
            )),
            CssProperty::WordSpacing(_) => Some(CssProperty::WordSpacing(CssPropertyValue::Exact(
                StyleWordSpacing {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::PaddingLeft(_) => Some(CssProperty::PaddingLeft(CssPropertyValue::Exact(
                LayoutPaddingLeft {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::PaddingRight(_) => Some(CssProperty::PaddingRight(
                CssPropertyValue::Exact(LayoutPaddingRight {
                    inner: resolved_pixel_value,
                }),
            )),
            CssProperty::PaddingTop(_) => Some(CssProperty::PaddingTop(CssPropertyValue::Exact(
                LayoutPaddingTop {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::PaddingBottom(_) => Some(CssProperty::PaddingBottom(
                CssPropertyValue::Exact(LayoutPaddingBottom {
                    inner: resolved_pixel_value,
                }),
            )),
            CssProperty::MarginLeft(_) => Some(CssProperty::MarginLeft(CssPropertyValue::Exact(
                LayoutMarginLeft {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MarginRight(_) => Some(CssProperty::MarginRight(CssPropertyValue::Exact(
                LayoutMarginRight {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MarginTop(_) => Some(CssProperty::MarginTop(CssPropertyValue::Exact(
                LayoutMarginTop {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MarginBottom(_) => Some(CssProperty::MarginBottom(
                CssPropertyValue::Exact(LayoutMarginBottom {
                    inner: resolved_pixel_value,
                }),
            )),
            CssProperty::MinWidth(_) => Some(CssProperty::MinWidth(CssPropertyValue::Exact(
                LayoutMinWidth {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MinHeight(_) => Some(CssProperty::MinHeight(CssPropertyValue::Exact(
                LayoutMinHeight {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MaxWidth(_) => Some(CssProperty::MaxWidth(CssPropertyValue::Exact(
                LayoutMaxWidth {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MaxHeight(_) => Some(CssProperty::MaxHeight(CssPropertyValue::Exact(
                LayoutMaxHeight {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::SelectionRadius(_) => Some(CssProperty::SelectionRadius(
                CssPropertyValue::Exact(SelectionRadius {
                    inner: resolved_pixel_value,
                }),
            )),
            _ => None,
        }
    }

    /// Applies user-agent (UA) CSS properties to the cascade before inheritance.
    ///
    /// UA CSS has the lowest priority in the cascade, so it should only be applied
    /// if the node doesn't already have the property from inline styles or author CSS.
    ///
    /// This is critical for text nodes: UA CSS properties (like font-weight: bold for H1)
    /// must be in the cascade maps so they can be inherited by child text nodes.
    ///
    /// Uses a bitset per node to avoid O(n²) scanning of property vecs.
    pub fn apply_ua_css(&mut self, node_data: &[NodeData]) {
        use azul_css::props::property::CssPropertyType;
        use azul_css::dynamic_selector::PseudoStateType;

        let node_count = node_data.len();
        if node_count == 0 {
            return;
        }

        // Build a bitset per node: which CssPropertyType values are already set (Normal state).
        // CssPropertyType has ~152 variants, so we need [u128; 2] per node.
        let mut prop_set: Vec<[u128; 2]> = vec![[0u128; 2]; node_count];

        // Mark properties from css_props (author CSS, Normal state)
        for (node_idx, props) in self.css_props.iter().enumerate() {
            for p in props.iter() {
                if p.state == PseudoStateType::Normal {
                    let d = p.prop_type as u8 as usize;
                    if d < 128 {
                        prop_set[node_idx][0] |= 1u128 << d;
                    } else {
                        prop_set[node_idx][1] |= 1u128 << (d - 128);
                    }
                }
            }
        }

        // Mark properties from cascaded_props (Normal state)
        for (node_idx, props) in self.cascaded_props.iter().enumerate() {
            for p in props.iter() {
                if p.state == PseudoStateType::Normal {
                    let d = p.prop_type as u8 as usize;
                    if d < 128 {
                        prop_set[node_idx][0] |= 1u128 << d;
                    } else {
                        prop_set[node_idx][1] |= 1u128 << (d - 128);
                    }
                }
            }
        }

        // Mark properties from inline CSS (NodeData.css_props, unconditional = Normal)
        for (node_idx, node) in node_data.iter().enumerate() {
            for p in node.css_props.iter() {
                let is_normal = p.apply_if.as_slice().is_empty();
                if is_normal {
                    let d = p.property.get_type() as u8 as usize;
                    if d < 128 {
                        prop_set[node_idx][0] |= 1u128 << d;
                    } else {
                        prop_set[node_idx][1] |= 1u128 << (d - 128);
                    }
                }
            }
        }

        // All UA property types that get_ua_property() may return Some for
        let property_types = [
            CssPropertyType::Display,
            CssPropertyType::Width,
            CssPropertyType::Height,
            CssPropertyType::FontSize,
            CssPropertyType::FontWeight,
            CssPropertyType::FontFamily,
            CssPropertyType::MarginTop,
            CssPropertyType::MarginBottom,
            CssPropertyType::MarginLeft,
            CssPropertyType::MarginRight,
            CssPropertyType::PaddingTop,
            CssPropertyType::PaddingBottom,
            CssPropertyType::PaddingLeft,
            CssPropertyType::PaddingRight,
            CssPropertyType::BorderTopStyle,
            CssPropertyType::BorderTopWidth,
            CssPropertyType::BorderTopColor,
            CssPropertyType::BreakInside,
            CssPropertyType::BreakAfter,
            CssPropertyType::ListStyleType,
            CssPropertyType::CounterReset,
            CssPropertyType::TextDecoration,
            CssPropertyType::TextAlign,
            CssPropertyType::VerticalAlign,
            CssPropertyType::Cursor,
        ];

        // Apply UA CSS: only insert for property types not yet set (bitset check = O(1))
        for (node_index, node) in node_data.iter().enumerate() {
            let node_type = &node.node_type;

            for prop_type in &property_types {
                // Check bitset: if already set, skip entirely
                let d = *prop_type as u8 as usize;
                let has_prop = if d < 128 {
                    (prop_set[node_index][0] & (1u128 << d)) != 0
                } else {
                    (prop_set[node_index][1] & (1u128 << (d - 128))) != 0
                };

                if has_prop {
                    continue;
                }

                // Check if UA CSS defines this property for this node type
                if let Some(ua_prop) = crate::ua_css::get_ua_property(node_type, *prop_type) {
                    self.cascaded_props[node_index].push(StatefulCssProperty {
                        state: PseudoStateType::Normal,
                        prop_type: *prop_type,
                        property: ua_prop.clone(),
                    });

                    // Mark as set in the bitset (prevent duplicate insertion for same node)
                    if d < 128 {
                        prop_set[node_index][0] |= 1u128 << d;
                    } else {
                        prop_set[node_index][1] |= 1u128 << (d - 128);
                    }
                }
            }
        }
    }

    /// Sort cascaded_props by (state, prop_type) for binary search lookups.
    /// Must be called after apply_ua_css() which adds entries to cascaded_props.
    pub fn sort_cascaded_props(&mut self) {
        for v in self.cascaded_props.iter_mut() {
            v.sort_unstable_by_key(|p| (p.state, p.prop_type));
        }
    }

    /// Compute inherited values for all nodes in the DOM tree.
    ///
    /// Implements CSS inheritance: walk tree depth-first, apply cascade priority
    /// (inherited → cascaded → css → inline → user), create dependency chains for
    /// relative values. Call `apply_ua_css()` before this function.
    pub fn compute_inherited_values(
        &mut self,
        node_hierarchy: &[NodeHierarchyItem],
        node_data: &[NodeData],
    ) -> Vec<NodeId> {
        node_hierarchy
            .iter()
            .enumerate()
            .filter_map(|(node_index, hierarchy_item)| {
                let node_id = NodeId::new(node_index);
                let parent_id = hierarchy_item.parent_id();
                let parent_computed: Option<Vec<(CssPropertyType, CssPropertyWithOrigin)>> =
                    parent_id.and_then(|pid| self.computed_values.get(pid.index()).cloned());

                let mut ctx = InheritanceContext {
                    node_id,
                    parent_id,
                    computed_values: Vec::new(),
                };

                // Step 1: Inherit from parent
                if let Some(ref parent_values) = parent_computed {
                    self.inherit_from_parent(&mut ctx, parent_values);
                }

                // Steps 2-5: Apply cascade in priority order
                self.apply_cascade_properties(
                    &mut ctx,
                    node_id,
                    &parent_computed,
                    node_data,
                    node_index,
                );

                // Check for changes and store
                let changed = self.store_if_changed(&ctx);
                changed.then_some(node_id)
            })
            .collect()
    }

    /// Inherit inheritable properties from parent node
    fn inherit_from_parent(
        &self,
        ctx: &mut InheritanceContext,
        parent_values: &[(CssPropertyType, CssPropertyWithOrigin)],
    ) {
        for (prop_type, prop_with_origin) in
            parent_values.iter().filter(|(pt, _)| pt.is_inheritable())
        {
            let entry = (*prop_type, CssPropertyWithOrigin {
                property: prop_with_origin.property.clone(),
                origin: CssPropertyOrigin::Inherited,
            });
            // Insert into sorted vec
            match ctx.computed_values.binary_search_by_key(prop_type, |(k, _)| *k) {
                Ok(idx) => ctx.computed_values[idx] = entry,
                Err(idx) => ctx.computed_values.insert(idx, entry),
            }
        }
    }

    /// Apply all cascade properties in priority order
    fn apply_cascade_properties(
        &self,
        ctx: &mut InheritanceContext,
        node_id: NodeId,
        parent_computed: &Option<Vec<(CssPropertyType, CssPropertyWithOrigin)>>,
        node_data: &[NodeData],
        node_index: usize,
    ) {
        // Step 2: Cascaded properties (UA CSS)
        if let Some(cascaded_vec) = self.cascaded_props.get(node_id.index()) {
            for p in cascaded_vec.iter() {
                if p.state == azul_css::dynamic_selector::PseudoStateType::Normal {
                    if self.should_apply_cascaded(&ctx.computed_values, p.prop_type, &p.property) {
                        self.process_property(ctx, &p.property, parent_computed);
                    }
                }
            }
        }

        // Step 3: CSS properties (stylesheets)
        if let Some(css_vec) = self.css_props.get(node_id.index()) {
            for p in css_vec.iter() {
                if p.state == azul_css::dynamic_selector::PseudoStateType::Normal {
                    self.process_property(ctx, &p.property, parent_computed);
                }
            }
        }

        // Step 4: Inline CSS properties
        for inline_prop in node_data[node_index].css_props.iter() {
            // Only apply unconditional (normal) properties
            if inline_prop.apply_if.as_slice().is_empty() {
                self.process_property(ctx, &inline_prop.property, parent_computed);
            }
        }

        // Step 5: User-overridden properties
        if let Some(user_props) = self.user_overridden_properties.get(node_id.index()) {
            for (_, prop) in user_props.iter() {
                self.process_property(ctx, &prop, parent_computed);
            }
        }
    }

    /// Check if a cascaded property should be applied
    fn should_apply_cascaded(
        &self,
        computed: &[(CssPropertyType, CssPropertyWithOrigin)],
        prop_type: CssPropertyType,
        prop: &CssProperty,
    ) -> bool {
        // Skip relative font-size if we already have inherited resolved value
        if prop_type == CssPropertyType::FontSize {
            if let Ok(idx) = computed.binary_search_by_key(&prop_type, |(k, _)| *k) {
                if computed[idx].1.origin == CssPropertyOrigin::Inherited
                    && Self::has_relative_font_size_unit(prop)
                {
                    return false;
                }
            }
        }

        match computed.binary_search_by_key(&prop_type, |(k, _)| *k) {
            Err(_) => true,
            Ok(idx) => computed[idx].1.origin == CssPropertyOrigin::Inherited,
        }
    }

    /// Process a single property: resolve and store
    fn process_property(
        &self,
        ctx: &mut InheritanceContext,
        prop: &CssProperty,
        parent_computed: &Option<Vec<(CssPropertyType, CssPropertyWithOrigin)>>,
    ) {
        let prop_type = prop.get_type();

        let resolved = if prop_type == CssPropertyType::FontSize {
            self.resolve_font_size_property(prop, parent_computed)
        } else {
            self.resolve_other_property(prop, &ctx.computed_values)
        };

        let entry = (prop_type, CssPropertyWithOrigin {
            property: resolved,
            origin: CssPropertyOrigin::Own,
        });
        match ctx.computed_values.binary_search_by_key(&prop_type, |(k, _)| *k) {
            Ok(idx) => ctx.computed_values[idx] = entry,
            Err(idx) => ctx.computed_values.insert(idx, entry),
        }
    }

    /// Resolve font-size property (uses parent's font-size as reference)
    fn resolve_font_size_property(
        &self,
        prop: &CssProperty,
        parent_computed: &Option<Vec<(CssPropertyType, CssPropertyWithOrigin)>>,
    ) -> CssProperty {
        const DEFAULT_FONT_SIZE_PX: f32 = 16.0;

        let parent_font_size = parent_computed
            .as_ref()
            .and_then(|p| {
                p.binary_search_by_key(&CssPropertyType::FontSize, |(k, _)| *k)
                    .ok()
                    .map(|idx| &p[idx].1)
            });

        match parent_font_size {
            Some(pfs) => Self::resolve_property_dependency(prop, &pfs.property)
                .unwrap_or_else(|| Self::resolve_font_size_to_pixels(prop, DEFAULT_FONT_SIZE_PX)),
            None => Self::resolve_font_size_to_pixels(prop, DEFAULT_FONT_SIZE_PX),
        }
    }

    /// Resolve other properties (uses current node's font-size as reference)
    fn resolve_other_property(
        &self,
        prop: &CssProperty,
        computed: &[(CssPropertyType, CssPropertyWithOrigin)],
    ) -> CssProperty {
        computed
            .binary_search_by_key(&CssPropertyType::FontSize, |(k, _)| *k)
            .ok()
            .and_then(|idx| Self::resolve_property_dependency(prop, &computed[idx].1.property))
            .unwrap_or_else(|| prop.clone())
    }

    /// Convert font-size to absolute pixels
    fn resolve_font_size_to_pixels(prop: &CssProperty, reference_px: f32) -> CssProperty {
        use azul_css::{
            css::CssPropertyValue,
            props::basic::{font::StyleFontSize, length::SizeMetric, pixel::PixelValue},
        };

        const DEFAULT_FONT_SIZE_PX: f32 = 16.0;

        let CssProperty::FontSize(css_val) = prop else {
            return prop.clone();
        };

        let Some(font_size) = css_val.get_property() else {
            return prop.clone();
        };

        let resolved_px = match font_size.inner.metric {
            SizeMetric::Px => font_size.inner.number.get(),
            SizeMetric::Pt => font_size.inner.number.get() * 1.333333,
            SizeMetric::In => font_size.inner.number.get() * 96.0,
            SizeMetric::Cm => font_size.inner.number.get() * 37.7952755906,
            SizeMetric::Mm => font_size.inner.number.get() * 3.7795275591,
            SizeMetric::Em => font_size.inner.number.get() * reference_px,
            SizeMetric::Rem => font_size.inner.number.get() * DEFAULT_FONT_SIZE_PX,
            SizeMetric::Percent => font_size.inner.number.get() / 100.0 * reference_px,
            SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => {
                return prop.clone();
            }
        };

        CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
            inner: PixelValue::px(resolved_px),
        }))
    }

    /// Check if font-size has relative unit (em, rem, %)
    fn has_relative_font_size_unit(prop: &CssProperty) -> bool {
        use azul_css::props::basic::length::SizeMetric;

        let CssProperty::FontSize(css_val) = prop else {
            return false;
        };

        css_val
            .get_property()
            .map(|fs| {
                matches!(
                    fs.inner.metric,
                    SizeMetric::Em | SizeMetric::Rem | SizeMetric::Percent
                )
            })
            .unwrap_or(false)
    }

    /// Store computed values if changed, returns true if values were updated
    fn store_if_changed(&mut self, ctx: &InheritanceContext) -> bool {
        let values_changed = self
            .computed_values
            .get(ctx.node_id.index())
            .map(|old| old != &ctx.computed_values)
            .unwrap_or(true);

        self.computed_values[ctx.node_id.index()] = ctx.computed_values.clone();

        values_changed
    }
}

/// Context for computing inherited values for a single node
struct InheritanceContext {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    computed_values: Vec<(CssPropertyType, CssPropertyWithOrigin)>,
}

impl CssPropertyCache {

    /// Property types that may have User-Agent CSS defaults.
    /// Used by build_resolved_cache to ensure UA CSS properties are included.
    const UA_PROPERTY_TYPES: &'static [CssPropertyType] = &[
        CssPropertyType::Display,
        CssPropertyType::Width,
        CssPropertyType::Height,
        CssPropertyType::FontSize,
        CssPropertyType::FontWeight,
        CssPropertyType::FontFamily,
        CssPropertyType::MarginTop,
        CssPropertyType::MarginBottom,
        CssPropertyType::MarginLeft,
        CssPropertyType::MarginRight,
        CssPropertyType::PaddingTop,
        CssPropertyType::PaddingBottom,
        CssPropertyType::PaddingLeft,
        CssPropertyType::PaddingRight,
        CssPropertyType::BreakInside,
        CssPropertyType::BreakBefore,
        CssPropertyType::BreakAfter,
        CssPropertyType::BorderCollapse,
        CssPropertyType::BorderSpacing,
        CssPropertyType::TextAlign,
        CssPropertyType::VerticalAlign,
        CssPropertyType::ListStyleType,
        CssPropertyType::ListStylePosition,
    ];

    /// Build a pre-resolved cache of all CSS properties for every node.
    ///
    /// After calling restyle(), apply_ua_css(), and compute_inherited_values(),
    /// call this to pre-resolve the CSS cascade for all nodes. This builds a flat
    /// Vec<Vec<(CssPropertyType, CssProperty)>> where:
    /// - Outer Vec is indexed by node ID (O(1) access)
    /// - Inner Vec is sorted by CssPropertyType (O(log m) binary search, m ≈ 5-10)
    ///
    /// This replaces 18+ BTreeMap lookups per get_property() call with
    /// a single Vec index + binary search, typically a 5-10x speedup.
    ///
    /// The returned cache is used to populate compact_cache.tier3_overflow.
    pub fn build_resolved_cache(
        &self,
        node_data: &[NodeData],
        styled_nodes: &[crate::styled_dom::StyledNode],
    ) -> Vec<Vec<(CssPropertyType, CssProperty)>> {
        use alloc::collections::BTreeSet;
        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};

        let node_count = node_data.len();
        let mut resolved = Vec::with_capacity(node_count);

        for node_index in 0..node_count {
            let node_id = NodeId::new(node_index);
            let nd = &node_data[node_index];
            let node_state = &styled_nodes[node_index].styled_node_state;

            // Collect all property types that might be set for this node.
            // BTreeSet<CssPropertyType> is cheap (u8-sized keys, small set per node).
            let mut prop_types = BTreeSet::new();

            if let Some(v) = self.user_overridden_properties.get(node_id.index()) {
                prop_types.extend(v.iter().map(|(k, _)| *k));
            }

            // Collect inline CSS property types.
            // Fast path: use the pre-built compact table (O(m) where m = unique types per state).
            // Fallback: linear scan of node_data.css_props (before table is built).
            let inline_key = self.inline_style_keys.get(node_index).copied().unwrap_or(u32::MAX);
            if inline_key != u32::MAX {
                if let Some(compact) = self.inline_style_table.entries.get(inline_key as usize) {
                    // Always include normal-state inline properties
                    for (pt, _) in &compact.normal {
                        prop_types.insert(*pt);
                    }
                    if node_state.hover {
                        for (pt, _) in &compact.hover { prop_types.insert(*pt); }
                    }
                    if node_state.active {
                        for (pt, _) in &compact.active { prop_types.insert(*pt); }
                    }
                    if node_state.focused {
                        for (pt, _) in &compact.focus { prop_types.insert(*pt); }
                    }
                    if node_state.dragging {
                        for (pt, _) in &compact.dragging { prop_types.insert(*pt); }
                    }
                    if node_state.drag_over {
                        for (pt, _) in &compact.drag_over { prop_types.insert(*pt); }
                    }
                }
            } else {
                // Fallback: linear scan (table not yet built or node has no inline styles)
                for css_prop in nd.css_props.iter() {
                    let conditions = css_prop.apply_if.as_slice();
                    let matches = if conditions.is_empty() {
                        true
                    } else {
                        conditions.iter().all(|c| match c {
                            DynamicSelector::PseudoState(PseudoStateType::Hover) => node_state.hover,
                            DynamicSelector::PseudoState(PseudoStateType::Active) => node_state.active,
                            DynamicSelector::PseudoState(PseudoStateType::Focus) => node_state.focused,
                            DynamicSelector::PseudoState(PseudoStateType::Dragging) => node_state.dragging,
                            DynamicSelector::PseudoState(PseudoStateType::DragOver) => node_state.drag_over,
                            _ => false,
                        })
                    };
                    if matches {
                        prop_types.insert(css_prop.property.get_type());
                    }
                }
            }
            if let Some(v) = self.css_props.get(node_id.index()) {
                for p in v.iter() {
                    let state_active = match p.state {
                        PseudoStateType::Normal => true,
                        PseudoStateType::Hover => node_state.hover,
                        PseudoStateType::Active => node_state.active,
                        PseudoStateType::Focus => node_state.focused,
                        PseudoStateType::Dragging => node_state.dragging,
                        PseudoStateType::DragOver => node_state.drag_over,
                        _ => false,
                    };
                    if state_active {
                        prop_types.insert(p.prop_type);
                    }
                }
            }
            if let Some(v) = self.cascaded_props.get(node_id.index()) {
                for p in v.iter() {
                    let state_active = match p.state {
                        PseudoStateType::Normal => true,
                        PseudoStateType::Hover => node_state.hover,
                        PseudoStateType::Active => node_state.active,
                        PseudoStateType::Focus => node_state.focused,
                        PseudoStateType::Dragging => node_state.dragging,
                        PseudoStateType::DragOver => node_state.drag_over,
                        _ => false,
                    };
                    if state_active {
                        prop_types.insert(p.prop_type);
                    }
                }
            }
            // UA CSS
            for pt in Self::UA_PROPERTY_TYPES {
                if crate::ua_css::get_ua_property(&nd.node_type, *pt).is_some() {
                    prop_types.insert(*pt);
                }
            }

            // Resolve each property through the cascade. get_property_slow
            // returns a reference; we only clone the winning value per type.
            let mut props: Vec<(CssPropertyType, CssProperty)> =
                Vec::with_capacity(prop_types.len());
            for prop_type in &prop_types {
                if let Some(prop) = self.get_property_slow(nd, &node_id, node_state, prop_type) {
                    props.push((*prop_type, prop.clone()));
                }
            }
            // Props are already sorted because BTreeSet iterates in Ord order.
            resolved.push(props);
        }

        resolved
    }

    /// Invalidate the resolved cache for a single node.
    /// Call this when a node's state changes (e.g., hover on/off) or
    /// when a property is overridden dynamically.
    /// Rebuilds the compact cache tier3_overflow entry for a single node.
    pub fn invalidate_resolved_node(
        &mut self,
        node_id: NodeId,
        node_data: &NodeData,
        styled_node: &crate::styled_dom::StyledNode,
    ) {
        let idx = node_id.index();

        // Check that compact_cache exists and has this node
        match &self.compact_cache {
            Some(c) if idx < c.node_count() => {},
            _ => return,
        }

        let node_state = &styled_node.styled_node_state;

        // Build the resolved properties using shared reference (&self via get_property_slow)
        let mut prop_types = alloc::collections::BTreeSet::new();

        if let Some(v) = self.user_overridden_properties.get(node_id.index()) {
            prop_types.extend(v.iter().map(|(k, _)| *k));
        }
        for css_prop in node_data.css_props.iter() {
            prop_types.insert(css_prop.property.get_type());
        }
        if let Some(v) = self.css_props.get(node_id.index()) {
            for p in v.iter() {
                prop_types.insert(p.prop_type);
            }
        }
        if let Some(v) = self.cascaded_props.get(node_id.index()) {
            for p in v.iter() {
                prop_types.insert(p.prop_type);
            }
        }
        if let Some(map) = self.computed_values.get(node_id.index()) {
            prop_types.extend(map.iter().map(|(k, _)| *k));
        }
        for pt in Self::UA_PROPERTY_TYPES {
            if crate::ua_css::get_ua_property(&node_data.node_type, *pt).is_some() {
                prop_types.insert(*pt);
            }
        }

        // Resolve all properties first (immutable borrow of self)
        let mut props = Vec::with_capacity(prop_types.len());
        for prop_type in &prop_types {
            if let Some(prop) = self.get_property_slow(node_data, &node_id, node_state, prop_type) {
                props.push((*prop_type, prop.clone()));
            }
        }

        // Now mutably borrow compact_cache to update it
        if let Some(compact) = &mut self.compact_cache {
            compact.set_overflow_props(idx, props);
        }
    }

    /// Clear the entire compact cache. Call after major DOM changes.
    pub fn invalidate_resolved_cache(&mut self) {
        self.compact_cache = None;
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
        let mut node_data = NodeData::create_node(NodeType::P);

        let node_id = NodeId::new(0);
        let node_state = StyledNodeState::default();

        // Test that <p> has display: block from UA CSS
        let display = cache.get_display(&node_data, &node_id, &node_state);
        assert!(
            display.is_some(),
            "Expected <p> to have display property from UA CSS"
        );
        if let Some(d) = display {
            if let Some(display_value) = d.get_property() {
                assert_eq!(
                    *display_value,
                    LayoutDisplay::Block,
                    "Expected <p> to have display: block, got {:?}",
                    display_value
                );
            }
        }

        // NOTE: <p> does NOT have width: 100% in standard UA CSS
        // Block elements have width: auto by default, which means "fill available width"
        // but it's not the same as width: 100%. The difference is critical for flexbox.
        let width = cache.get_width(&node_data, &node_id, &node_state);
        // Width should be None because <p> should use auto width (no explicit width property)
        assert!(
            width.is_none(),
            "Expected <p> to NOT have explicit width (should be auto), but got {:?}",
            width
        );

        // Test that <p> does NOT have a default height from UA CSS
        // (height should be auto, which means None)
        let height = cache.get_height(&node_data, &node_id, &node_state);
        println!("Height for <p> tag: {:?}", height);

        // Height should be None because <p> should use auto height
        assert!(
            height.is_none(),
            "Expected <p> to NOT have explicit height (should be auto), but got {:?}",
            height
        );
    }

    #[test]
    fn test_ua_css_body_tag_properties() {
        let cache = CssPropertyCache::empty(1);

        let node_data = NodeData::create_node(NodeType::Body);

        let node_id = NodeId::new(0);
        let node_state = StyledNodeState::default();

        // NOTE: <body> does NOT have width: 100% in standard UA CSS
        // It inherits from the Initial Containing Block (ICB) and has width: auto
        let width = cache.get_width(&node_data, &node_id, &node_state);
        // Width should be None because <body> should use auto width (no explicit width property)
        assert!(
            width.is_none(),
            "Expected <body> to NOT have explicit width (should be auto), but got {:?}",
            width
        );

        // Note: height: 100% was removed from UA CSS (ua_css.rs:506 commented out)
        // This is correct - <body> should have height: auto by default per CSS spec
        let height = cache.get_height(&node_data, &node_id, &node_state);
        assert!(
            height.is_none(),
            "<body> should not have explicit height from UA CSS (should be auto)"
        );

        // Test margins (body has 8px margins from UA CSS)
        let margin_top = cache.get_margin_top(&node_data, &node_id, &node_state);
        assert!(
            margin_top.is_some(),
            "Expected <body> to have margin-top from UA CSS"
        );
    }
}
