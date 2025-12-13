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

/// Represents a single step in a CSS property dependency chain.
/// Example: "10% of node 5" or "1.2em of node 3"
#[derive(Debug, Clone, PartialEq)]
pub enum CssDependencyChainStep {
    /// Value depends on a percentage of another node's resolved value
    /// e.g., font-size: 150% means 1.5 * parent's font-size
    Percent { source_node: NodeId, factor: f32 },

    /// Value depends on an em multiple of another node's font-size
    /// e.g., padding: 2em means 2.0 * current element's font-size
    Em { source_node: NodeId, factor: f32 },

    /// Value depends on a rem multiple of root node's font-size
    /// e.g., margin: 1.5rem means 1.5 * root font-size
    Rem { factor: f32 },

    /// Absolute value (px, pt, etc.) - no further dependencies
    Absolute { pixels: f32 },
}

/// A dependency chain for a CSS property value.
/// Example: [10% of node 10, then 1.2em of that, then 1.5em of that]
///
/// During layout, this chain is resolved by:
/// 1. Starting with the root dependency (e.g., node 10's resolved font-size)
/// 2. Applying each transformation in sequence
/// 3. Producing the final pixel value
#[derive(Debug, Clone, PartialEq)]
pub struct CssDependencyChain {
    /// The property type this chain is for
    pub property_type: CssPropertyType,

    /// The ordered list of dependencies, from root to leaf
    /// Empty if the value is absolute (no dependencies)
    pub steps: Vec<CssDependencyChainStep>,

    /// Cached resolved value (in pixels) from the last resolution
    /// None if the chain hasn't been resolved yet
    pub cached_pixels: Option<f32>,
}

impl CssDependencyChain {
    /// Create a new dependency chain for an absolute pixel value
    pub fn absolute(property_type: CssPropertyType, pixels: f32) -> Self {
        Self {
            property_type,
            steps: vec![CssDependencyChainStep::Absolute { pixels }],
            cached_pixels: Some(pixels),
        }
    }

    /// Create a new dependency chain for a percentage-based value
    pub fn percent(property_type: CssPropertyType, source_node: NodeId, factor: f32) -> Self {
        Self {
            property_type,
            steps: vec![CssDependencyChainStep::Percent {
                source_node,
                factor,
            }],
            cached_pixels: None,
        }
    }

    /// Create a new dependency chain for an em-based value
    pub fn em(property_type: CssPropertyType, source_node: NodeId, factor: f32) -> Self {
        Self {
            property_type,
            steps: vec![CssDependencyChainStep::Em {
                source_node,
                factor,
            }],
            cached_pixels: None,
        }
    }

    /// Create a new dependency chain for a rem-based value
    pub fn rem(property_type: CssPropertyType, factor: f32) -> Self {
        Self {
            property_type,
            steps: vec![CssDependencyChainStep::Rem { factor }],
            cached_pixels: None,
        }
    }

    /// Check if this chain depends on a specific node
    pub fn depends_on(&self, node_id: NodeId) -> bool {
        self.steps.iter().any(|step| match step {
            CssDependencyChainStep::Percent { source_node, .. } => *source_node == node_id,
            CssDependencyChainStep::Em { source_node, .. } => *source_node == node_id,
            _ => false,
        })
    }

    /// Resolve the dependency chain to a pixel value.
    ///
    /// # Arguments
    /// * `resolve_node_value` - Closure to resolve a node's property value to pixels
    /// * `root_font_size` - Root element's font-size for rem calculations
    ///
    /// # Returns
    /// The resolved pixel value, or None if any dependency couldn't be resolved
    pub fn resolve<F>(&mut self, mut resolve_node_value: F, root_font_size: f32) -> Option<f32>
    where
        F: FnMut(NodeId, CssPropertyType) -> Option<f32>,
    {
        let mut current_value: Option<f32> = None;

        for step in &self.steps {
            match step {
                CssDependencyChainStep::Absolute { pixels } => {
                    current_value = Some(*pixels);
                }
                CssDependencyChainStep::Percent {
                    source_node,
                    factor,
                } => {
                    let source_val = resolve_node_value(*source_node, self.property_type)?;
                    current_value = Some(source_val * factor);
                }
                CssDependencyChainStep::Em {
                    source_node,
                    factor,
                } => {
                    let font_size = resolve_node_value(*source_node, CssPropertyType::FontSize)?;
                    current_value = Some(font_size * factor);
                }
                CssDependencyChainStep::Rem { factor } => {
                    current_value = Some(root_font_size * factor);
                }
            }
        }

        self.cached_pixels = current_value;
        current_value
    }
}

use azul_css::{
    css::{Css, CssPath},
    props::{
        basic::{StyleFontFamily, StyleFontFamilyVec, StyleFontSize},
        layout::{LayoutDisplay, LayoutHeight, LayoutWidth},
        property::{
            BoxDecorationBreakValue, BreakInsideValue, CaretAnimationDurationValue,
            CaretColorValue, ClipPathValue, ColumnCountValue, ColumnFillValue,
            ColumnRuleColorValue, ColumnRuleStyleValue, ColumnRuleWidthValue, ColumnSpanValue,
            ColumnWidthValue, ContentValue, CounterIncrementValue, CounterResetValue, CssProperty,
            CssPropertyType, FlowFromValue, FlowIntoValue, LayoutAlignContentValue,
            LayoutAlignItemsValue, LayoutAlignSelfValue, LayoutBorderBottomWidthValue,
            LayoutBorderLeftWidthValue, LayoutBorderRightWidthValue, LayoutBorderSpacingValue,
            LayoutBorderTopWidthValue, LayoutBottomValue, LayoutBoxSizingValue, LayoutClearValue,
            LayoutColumnGapValue, LayoutDisplayValue, LayoutFlexBasisValue,
            LayoutFlexDirectionValue, LayoutFlexGrowValue, LayoutFlexShrinkValue,
            LayoutFlexWrapValue, LayoutFloatValue, LayoutGapValue, LayoutGridAutoColumnsValue,
            LayoutGridAutoFlowValue, LayoutGridAutoRowsValue, LayoutGridColumnValue,
            LayoutGridRowValue, LayoutGridTemplateColumnsValue, LayoutGridTemplateRowsValue,
            LayoutHeightValue, LayoutJustifyContentValue, LayoutJustifyItemsValue,
            LayoutJustifySelfValue, LayoutLeftValue, LayoutMarginBottomValue,
            LayoutMarginLeftValue, LayoutMarginRightValue, LayoutMarginTopValue,
            LayoutMaxHeightValue, LayoutMaxWidthValue, LayoutMinHeightValue, LayoutMinWidthValue,
            LayoutOverflowValue, LayoutPaddingBottomValue, LayoutPaddingLeftValue,
            LayoutPaddingRightValue, LayoutPaddingTopValue, LayoutPositionValue, LayoutRightValue,
            LayoutRowGapValue, LayoutScrollbarWidthValue, LayoutTableLayoutValue,
            LayoutTextJustifyValue, LayoutTopValue, LayoutWidthValue, LayoutWritingModeValue,
            LayoutZIndexValue, OrphansValue, PageBreakValue, ScrollbarStyleValue,
            SelectionBackgroundColorValue, SelectionColorValue, SelectionRadiusValue,
            ShapeImageThresholdValue, ShapeInsideValue, ShapeMarginValue, ShapeOutsideValue,
            StringSetValue, StyleBackfaceVisibilityValue, StyleBackgroundContentVecValue,
            StyleBackgroundPositionVecValue, StyleBackgroundRepeatVecValue,
            StyleBackgroundSizeVecValue, StyleBorderBottomColorValue,
            StyleBorderBottomLeftRadiusValue, StyleBorderBottomRightRadiusValue,
            StyleBorderBottomStyleValue, StyleBorderCollapseValue, StyleBorderLeftColorValue,
            StyleBorderLeftStyleValue, StyleBorderRightColorValue, StyleBorderRightStyleValue,
            StyleBorderTopColorValue, StyleBorderTopLeftRadiusValue,
            StyleBorderTopRightRadiusValue, StyleBorderTopStyleValue, StyleBoxShadowValue,
            StyleCaptionSideValue, StyleCursorValue, StyleDirectionValue, StyleEmptyCellsValue,
            StyleExclusionMarginValue, StyleFilterVecValue, StyleFontFamilyVecValue,
            StyleFontSizeValue, StyleFontStyleValue, StyleFontValue, StyleFontWeightValue,
            StyleHangingPunctuationValue, StyleHyphenationLanguageValue, StyleHyphensValue,
            StyleInitialLetterValue, StyleLetterSpacingValue, StyleLineClampValue,
            StyleLineHeightValue, StyleListStylePositionValue, StyleListStyleTypeValue,
            StyleMixBlendModeValue, StyleOpacityValue, StylePerspectiveOriginValue,
            StyleScrollbarColorValue, StyleTabWidthValue, StyleTextAlignValue, StyleTextColorValue,
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
    dom::{NodeData, NodeDataInlineCssProperty, NodeId, TabIndex, TagId},
    id::{NodeDataContainer, NodeDataContainerRef},
    style::CascadeInfo,
    styled_dom::{
        NodeHierarchyItem, NodeHierarchyItemId, NodeHierarchyItemVec, ParentWithNodeDepth,
        ParentWithNodeDepthVec, StyledNodeState, TagIdToNodeIdMapping,
    },
};

/// Macro to match on any CssProperty variant and access the inner CssPropertyValue<T>.
/// This allows generic operations on cascade keywords without writing 190+ match arms.
///
/// # Usage
/// ```ignore
/// let has_inherit = match_property_value!(property, p, p.is_inherit());
/// ```
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
            CssProperty::TabWidth($value) => $expr,
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

    // NEW: Computed values cache - pre-resolved inherited properties
    // This cache contains the final computed values after inheritance resolution.
    // Updated whenever a property changes or the DOM structure changes.
    // Properties are stored in contiguous memory per node for efficient access.
    // Each property is tagged with its origin (Inherited vs Own) to correctly handle
    // the CSS cascade when properties are updated.
    pub computed_values: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssPropertyWithOrigin>>,

    // NEW: Dependency chains for relative values (em, %, rem, etc.)
    // Maps NodeId → PropertyType → DependencyChain
    // This allows efficient updates when a property changes:
    // 1. Find all chains that depend on the changed node
    // 2. Invalidate their cached values
    // 3. Resolve chains during layout when needed
    //
    // Example: If node 5's font-size changes from 16px to 20px:
    // - All child nodes with font-size: 1.5em need recalculation
    // - All nodes with padding: 2em that depend on node 5 need updates
    pub dependency_chains: BTreeMap<NodeId, BTreeMap<CssPropertyType, CssDependencyChain>>,
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
                        tag_id: TagId::from_crate_internal(TagId::unique()),
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

            computed_values: BTreeMap::new(),
            dependency_chains: BTreeMap::new(),
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
        append_css_property_vec!(computed_values);

        // Special handling for dependency_chains: need to adjust source_node IDs
        {
            let mut s = BTreeMap::new();
            core::mem::swap(&mut s, &mut other.dependency_chains);
            for (node_id, mut chains_map) in s.into_iter() {
                // Adjust the source_node IDs in each chain's steps
                for (_prop_type, chain) in chains_map.iter_mut() {
                    for step in chain.steps.iter_mut() {
                        match step {
                            CssDependencyChainStep::Em { source_node, .. } => {
                                *source_node = NodeId::new(source_node.index() + self.node_count);
                            }
                            CssDependencyChainStep::Percent { source_node, .. } => {
                                *source_node = NodeId::new(source_node.index() + self.node_count);
                            }
                            _ => {}
                        }
                    }
                }
                self.dependency_chains
                    .insert(node_id + self.node_count, chains_map);
            }
        }

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

            // NEW: Check computed values cache for inherited properties
            // This provides efficient access to pre-resolved inherited values
            // without needing to walk up the tree
            if css_property_type.is_inheritable() {
                if let Some(prop_with_origin) = self
                    .computed_values
                    .get(node_id)
                    .and_then(|map| map.get(css_property_type))
                {
                    return Some(&prop_with_origin.property);
                }
            }
        }

        // User-agent stylesheet fallback (lowest precedence)
        // Check if the node type has a default value for this property
        crate::ua_css::get_ua_property(&node_data.node_type, *css_property_type)
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
            SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => return None, /* Reference can't be viewport-relative */
        };

        // Resolve target based on reference
        let resolved_px = match target_pixel_value.metric {
            SizeMetric::Px => target_pixel_value.number.get(),
            SizeMetric::Pt => target_pixel_value.number.get() * 1.333333,
            SizeMetric::In => target_pixel_value.number.get() * 96.0,
            SizeMetric::Cm => target_pixel_value.number.get() * 37.7952755906,
            SizeMetric::Mm => target_pixel_value.number.get() * 3.7795275591,
            SizeMetric::Em => target_pixel_value.number.get() * reference_px,
            SizeMetric::Rem => target_pixel_value.number.get() * reference_px, /* Use reference as root font-size */
            SizeMetric::Percent => target_pixel_value.number.get() / 100.0 * reference_px,
            SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => return None, /* Need viewport context */
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

    /// Build a dependency chain for a CSS property value.
    ///
    /// This analyzes the property value and creates a chain of dependencies that can be
    /// resolved later during layout. For example:
    /// - `font-size: 16px` → Absolute chain with 16.0 pixels
    /// - `font-size: 1.5em` → Em chain depending on parent's font-size
    /// - `font-size: 150%` → Percent chain depending on parent's font-size
    /// - `padding: 2em` → Em chain depending on current node's font-size
    ///
    /// # Arguments
    /// * `node_id` - The node this property belongs to
    /// * `parent_id` - The parent node (for inheritance)
    /// * `property` - The CSS property to analyze
    ///
    /// # Returns
    /// A dependency chain, or None if the property doesn't support chaining
    fn build_dependency_chain(
        &self,
        node_id: NodeId,
        parent_id: Option<NodeId>,
        property: &CssProperty,
    ) -> Option<CssDependencyChain> {
        use azul_css::props::basic::{length::SizeMetric, pixel::PixelValue};

        let prop_type = property.get_type();

        // For now, only handle font-size dependency chains
        // Other properties will be handled during layout resolution
        if prop_type != CssPropertyType::FontSize {
            return None;
        }

        // Extract PixelValue from FontSize property
        let pixel_value = match property {
            CssProperty::FontSize(val) => val.get_property().map(|v| &v.inner)?,
            _ => return None,
        };

        let number = pixel_value.number.get();

        // For font-size: em/% refers to parent's font-size
        let source_node = parent_id?;

        match pixel_value.metric {
            SizeMetric::Px => Some(CssDependencyChain::absolute(prop_type, number)),
            SizeMetric::Pt => {
                // 1pt = 1.333333px
                Some(CssDependencyChain::absolute(prop_type, number * 1.333333))
            }
            SizeMetric::Em => Some(CssDependencyChain::em(prop_type, source_node, number)),
            SizeMetric::Rem => {
                // Rem refers to root font-size
                Some(CssDependencyChain::rem(prop_type, number))
            }
            SizeMetric::Percent => Some(CssDependencyChain::percent(
                prop_type,
                source_node,
                number / 100.0,
            )),
            SizeMetric::In => {
                // 1in = 96px
                Some(CssDependencyChain::absolute(prop_type, number * 96.0))
            }
            SizeMetric::Cm => {
                // 1cm = 37.7952755906px
                Some(CssDependencyChain::absolute(
                    prop_type,
                    number * 37.7952755906,
                ))
            }
            SizeMetric::Mm => {
                // 1mm = 3.7795275591px
                Some(CssDependencyChain::absolute(
                    prop_type,
                    number * 3.7795275591,
                ))
            }
            // Viewport units: Cannot be resolved via dependency chain, need viewport context
            // These should be resolved at layout time using ResolutionContext
            SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => {
                // For now, treat as unresolvable (need viewport size at layout time)
                None
            }
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
    /// # Arguments
    /// * `node_data` - Array of node data indexed by NodeId
    pub fn apply_ua_css(&mut self, node_data: &[NodeData]) {
        use azul_css::props::property::CssPropertyType;

        // Apply UA CSS to all nodes
        for (node_index, node) in node_data.iter().enumerate() {
            let node_id = NodeId::new(node_index);
            let node_type = &node.node_type;

            // Get all possible CSS property types and check if UA CSS defines them
            // We need to check all properties that this node type might have
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
                // Add more as needed
            ];

            for prop_type in &property_types {
                // Check if UA CSS defines this property for this node type
                if let Some(ua_prop) = crate::ua_css::get_ua_property(node_type, *prop_type) {
                    // Only insert if the property is NOT already set by inline CSS or author CSS
                    // UA CSS has LOWEST priority
                    let has_inline = node.inline_css_props.iter().any(|p| {
                        if let NodeDataInlineCssProperty::Normal(prop) = p {
                            prop.get_type() == *prop_type
                        } else {
                            false
                        }
                    });

                    let has_css = self
                        .css_normal_props
                        .get(&node_id)
                        .map(|map| map.contains_key(prop_type))
                        .unwrap_or(false);

                    let has_cascaded = self
                        .cascaded_normal_props
                        .get(&node_id)
                        .map(|map| map.contains_key(prop_type))
                        .unwrap_or(false);

                    // Insert UA CSS only if not already present (lowest priority)
                    if !has_inline && !has_css && !has_cascaded {
                        self.cascaded_normal_props
                            .entry(node_id)
                            .or_insert_with(|| BTreeMap::new())
                            .entry(*prop_type)
                            .or_insert_with(|| ua_prop.clone());
                    }
                }
            }
        }
    }

    /// Compute inherited values and dependency chains for all nodes in the DOM tree.
    ///
    /// This implements a dependency-chain-based CSS inheritance system:
    /// 1. Walk the DOM tree in depth-first order
    /// 2. For each node, compute the cascade priority (inherited → cascaded → css → inline → user)
    /// 3. For relative values (em, %, rem), create a dependency chain
    /// 4. Store both the raw property value AND its dependency chain
    /// 5. Return a list of nodes whose values changed
    ///
    /// The dependency chains are later resolved during layout when all context is available.
    /// This avoids premature resolution of % values that depend on layout dimensions.
    ///
    /// IMPORTANT: Call apply_ua_css() BEFORE this function to ensure UA CSS properties
    /// are available for inheritance (especially for text nodes).
    ///
    /// # Arguments
    /// * `node_hierarchy` - The DOM tree structure
    /// * `node_data` - Array of node data indexed by NodeId
    ///
    /// # Returns
    /// Vector of NodeIds whose computed values changed and need re-layout
    pub fn compute_inherited_values(
        &mut self,
        node_hierarchy: &[NodeHierarchyItem],
        node_data: &[NodeData],
    ) -> Vec<NodeId> {
        use alloc::vec::Vec;

        let mut changed_nodes = Vec::new();

        // Walk tree in depth-first order to ensure parents are processed before children
        for (node_index, hierarchy_item) in node_hierarchy.iter().enumerate() {
            let node_id = NodeId::new(node_index);
            let parent_id = hierarchy_item.parent_id();

            let node_type = &node_data[node_index].node_type;

            // Get parent's computed values for inheritance
            let parent_computed = parent_id.and_then(|pid| self.computed_values.get(&pid));

            // Start with empty maps for this node
            let mut node_computed_values = BTreeMap::new();
            let mut node_dependency_chains = BTreeMap::new();

            // Step 1: Inherit inheritable properties from parent
            if let Some(parent_values) = parent_computed {
                for (prop_type, prop_with_origin) in parent_values.iter() {
                    if prop_type.is_inheritable() {
                        // Mark as inherited from parent
                        node_computed_values.insert(
                            *prop_type,
                            CssPropertyWithOrigin {
                                property: prop_with_origin.property.clone(),
                                origin: CssPropertyOrigin::Inherited,
                            },
                        );

                        // DON'T inherit the dependency chain for font-size!
                        // Font-size should be inherited as a COMPUTED VALUE (pixels), not as a
                        // relative value (em). If we inherit the chain,
                        // we'll resolve "2em" twice (2em * parent = 32px, then 32px * parent =
                        // 64px). Only inherit chains for properties that
                        // truly inherit their relative values.
                        if *prop_type != CssPropertyType::FontSize {
                            if let Some(parent_chains) =
                                parent_id.and_then(|pid| self.dependency_chains.get(&pid))
                            {
                                if let Some(chain) = parent_chains.get(prop_type) {
                                    node_dependency_chains.insert(*prop_type, chain.clone());
                                }
                            }
                        }
                    }
                }
            }

            // Helper macro to process a property and resolve dependencies
            // This marks the property as Own (not inherited)
            macro_rules! process_property {
                ($prop:expr) => {{
                    let prop = $prop;
                    let prop_type = prop.get_type();

                    // Try to resolve em/% values:
                    // - For font-size: use parent's font-size as reference
                    // - For other properties with em: use current node's font-size as reference
                    // - For other properties with %: defer to layout (needs containing block)
                    let resolved_prop = if prop_type == CssPropertyType::FontSize {
                        // Font-size em/% refers to PARENT's font-size
                        // If no parent, use the default font size (16px per CSS spec)
                        use azul_css::{
                            css::CssPropertyValue,
                            props::basic::{
                                font::StyleFontSize, length::SizeMetric, pixel::PixelValue,
                            },
                        };

                        const DEFAULT_FONT_SIZE_PX: f32 = 16.0;

                        // Helper to resolve font-size with a reference value
                        let resolve_font_size = |prop: &CssProperty,
                                                 reference_px: f32|
                         -> CssProperty {
                            if let CssProperty::FontSize(css_val) = prop {
                                if let Some(font_size) = css_val.get_property() {
                                    let resolved_px = match font_size.inner.metric {
                                        SizeMetric::Px => font_size.inner.number.get(),
                                        SizeMetric::Pt => font_size.inner.number.get() * 1.333333,
                                        SizeMetric::In => font_size.inner.number.get() * 96.0,
                                        SizeMetric::Cm => {
                                            font_size.inner.number.get() * 37.7952755906
                                        }
                                        SizeMetric::Mm => {
                                            font_size.inner.number.get() * 3.7795275591
                                        }
                                        SizeMetric::Em => {
                                            font_size.inner.number.get() * reference_px
                                        }
                                        SizeMetric::Rem => {
                                            font_size.inner.number.get() * DEFAULT_FONT_SIZE_PX
                                        }
                                        SizeMetric::Percent => {
                                            font_size.inner.number.get() / 100.0 * reference_px
                                        }
                                        // Viewport units need layout context
                                        SizeMetric::Vw
                                        | SizeMetric::Vh
                                        | SizeMetric::Vmin
                                        | SizeMetric::Vmax => {
                                            return prop.clone();
                                        }
                                    };
                                    return CssProperty::FontSize(CssPropertyValue::Exact(
                                        StyleFontSize {
                                            inner: PixelValue::px(resolved_px),
                                        },
                                    ));
                                }
                            }
                            prop.clone()
                        };

                        if let Some(parent_values) = parent_computed {
                            if let Some(parent_font_size) =
                                parent_values.get(&CssPropertyType::FontSize)
                            {
                                Self::resolve_property_dependency(prop, &parent_font_size.property)
                                    .unwrap_or_else(|| {
                                        // Fallback: resolve against default if parent has relative
                                        // value
                                        resolve_font_size(prop, DEFAULT_FONT_SIZE_PX)
                                    })
                            } else {
                                // Parent exists but has no font-size: use default
                                resolve_font_size(prop, DEFAULT_FONT_SIZE_PX)
                            }
                        } else {
                            // No parent: resolve against default (16px)
                            resolve_font_size(prop, DEFAULT_FONT_SIZE_PX)
                        }
                    } else {
                        // Other properties with em refer to CURRENT element's font-size
                        // We need to look up the current element's computed font-size
                        if let Some(current_font_size) =
                            node_computed_values.get(&CssPropertyType::FontSize)
                        {
                            Self::resolve_property_dependency(prop, &current_font_size.property)
                                .unwrap_or_else(|| prop.clone())
                        } else {
                            // No font-size computed yet, store as-is
                            prop.clone()
                        }
                    };

                    // Mark as Own property (not inherited)
                    node_computed_values.insert(
                        prop_type,
                        CssPropertyWithOrigin {
                            property: resolved_prop.clone(),
                            origin: CssPropertyOrigin::Own,
                        },
                    );

                    // Build dependency chain for this property (for tracking invalidations)
                    if let Some(chain) =
                        self.build_dependency_chain(node_id, parent_id, &resolved_prop)
                    {
                        node_dependency_chains.insert(prop_type, chain);
                    }
                }};
            }

            // Helper function to check if a font-size property has a relative unit
            fn has_relative_font_size_unit(prop: &CssProperty) -> bool {
                use azul_css::props::basic::length::SizeMetric;

                if let CssProperty::FontSize(css_prop_val) = prop {
                    if let Some(font_size) = css_prop_val.get_property() {
                        match font_size.inner.metric {
                            SizeMetric::Em | SizeMetric::Rem | SizeMetric::Percent => true,
                            _ => false,
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }

            // Step 2: Apply cascaded properties (UA CSS, properties from previous inheritance
            // iterations) These are the node's OWN properties, not inherited from
            // parent Only apply if not already set OR if existing value is inherited
            // (own properties override inherited)
            //
            // EXCEPTION for FontSize: restyle() copies FontSize values to children for
            // inheritance, but those are unresolved values (like 1.5em). If we already have
            // a properly inherited FontSize (from Step 1 which gets the resolved value),
            // we should NOT override it with the unresolved value from cascaded_props.
            // The resolved value from the parent is correct; re-resolving would double-apply
            // the multiplier.
            if let Some(cascaded_props) = self.cascaded_normal_props.get(&node_id).cloned() {
                for (prop_type, prop) in cascaded_props.iter() {
                    // Special handling for FontSize: don't override inherited resolved value
                    // with unresolved relative value from restyle()
                    if *prop_type == CssPropertyType::FontSize {
                        if let Some(existing) = node_computed_values.get(prop_type) {
                            if existing.origin == CssPropertyOrigin::Inherited
                                && has_relative_font_size_unit(prop)
                            {
                                // Skip: we already have the resolved value from parent
                                continue;
                            }
                        }
                    }

                    let should_apply = match node_computed_values.get(prop_type) {
                        None => true,                                                      /* Not set yet */
                        Some(existing) => existing.origin == CssPropertyOrigin::Inherited, /* Override inherited */
                    };

                    if should_apply {
                        process_property!(prop);
                    }
                }
            }

            // Step 3: Override with CSS properties (from stylesheets)
            if let Some(css_props) = self.css_normal_props.get(&node_id) {
                for (_, prop) in css_props.iter() {
                    process_property!(prop);
                }
            }

            // Step 4: Override with inline CSS properties (from style attribute)
            for inline_prop in node_data[node_index].inline_css_props.iter() {
                if let NodeDataInlineCssProperty::Normal(prop) = inline_prop {
                    process_property!(prop);
                }
            }

            // Step 5: Override with user-overridden properties (from callbacks)
            if let Some(user_props) = self.user_overridden_properties.get(&node_id) {
                for (_, prop) in user_props.iter() {
                    process_property!(prop);
                }
            }

            // Check if computed values or chains changed
            let values_changed = self
                .computed_values
                .get(&node_id)
                .map(|old| old != &node_computed_values)
                .unwrap_or(true);

            let chains_changed = self
                .dependency_chains
                .get(&node_id)
                .map(|old| old != &node_dependency_chains)
                .unwrap_or(!node_dependency_chains.is_empty());

            if values_changed || chains_changed {
                changed_nodes.push(node_id);
            }

            // Store the computed values and chains
            self.computed_values.insert(node_id, node_computed_values);
            if !node_dependency_chains.is_empty() {
                self.dependency_chains
                    .insert(node_id, node_dependency_chains);
            }
        }

        changed_nodes
    }

    /// Resolve a dependency chain to an absolute pixel value.
    ///
    /// This walks through the chain and resolves each dependency:
    /// - Absolute values: return immediately
    /// - Em values: multiply by source node's font-size
    /// - Percent values: multiply by source node's property value
    /// - Rem values: multiply by root node's font-size
    ///
    /// # Arguments
    /// * `node_id` - The node to resolve the property for
    /// * `property_type` - The property type to resolve
    /// * `root_font_size` - Root element's font-size for rem calculations (default 16px)
    ///
    /// # Returns
    /// The resolved pixel value, or None if the chain couldn't be resolved
    pub fn resolve_dependency_chain(
        &self,
        node_id: NodeId,
        property_type: CssPropertyType,
        root_font_size: f32,
    ) -> Option<f32> {
        // Get the dependency chain for this node/property (immutable borrow)
        let chain = self
            .dependency_chains
            .get(&node_id)
            .and_then(|chains| chains.get(&property_type))?;

        // If already cached, return it
        if let Some(cached) = chain.cached_pixels {
            return Some(cached);
        }

        // We need to resolve but can't mutate - collect steps first
        let steps = chain.steps.clone();
        let mut current_value: Option<f32> = None;

        for step in &steps {
            match step {
                CssDependencyChainStep::Absolute { pixels } => {
                    current_value = Some(*pixels);
                }
                CssDependencyChainStep::Percent {
                    source_node,
                    factor,
                } => {
                    // Try to get from cached chains first
                    let source_val = self
                        .dependency_chains
                        .get(source_node)
                        .and_then(|chains| chains.get(&property_type))
                        .and_then(|chain| chain.cached_pixels)?;
                    current_value = Some(source_val * factor);
                }
                CssDependencyChainStep::Em {
                    source_node,
                    factor,
                } => {
                    // For em, we need the source node's font-size
                    let font_size = self
                        .dependency_chains
                        .get(source_node)
                        .and_then(|chains| chains.get(&CssPropertyType::FontSize))
                        .and_then(|chain| chain.cached_pixels)?;
                    current_value = Some(font_size * factor);
                }
                CssDependencyChainStep::Rem { factor } => {
                    current_value = Some(root_font_size * factor);
                }
            }
        }

        current_value
    }

    /// Update a property value and invalidate all dependent chains.
    ///
    /// When a property changes (e.g., font-size changes from 16px to 20px):
    /// 1. Update the property value in computed_values
    /// 2. Update/rebuild the dependency chain
    /// 3. Find all nodes whose chains depend on this node
    /// 4. Invalidate their cached values
    /// 5. Return list of affected nodes that need re-layout
    ///
    /// # Arguments
    /// * `node_id` - The node whose property changed
    /// * `property` - The new property value
    /// * `node_hierarchy` - DOM tree (needed to find children)
    /// * `node_data` - Node data array
    ///
    /// # Returns
    /// Vector of NodeIds that were affected and need re-layout
    pub fn update_property_and_invalidate_dependents(
        &mut self,
        node_id: NodeId,
        property: CssProperty,
        node_hierarchy: &[NodeHierarchyItem],
        node_data: &[NodeData],
    ) -> Vec<NodeId> {
        use alloc::vec::Vec;

        let prop_type = property.get_type();
        let mut affected_nodes = Vec::new();

        // Step 1: Update the property value (mark as Own since it's an override)
        self.computed_values
            .entry(node_id)
            .or_insert_with(BTreeMap::new)
            .insert(
                prop_type,
                CssPropertyWithOrigin {
                    property: property.clone(),
                    origin: CssPropertyOrigin::Own,
                },
            );

        // Step 2: Rebuild the dependency chain
        let parent_id = node_hierarchy
            .get(node_id.index())
            .and_then(|h| h.parent_id());
        if let Some(chain) = self.build_dependency_chain(node_id, parent_id, &property) {
            self.dependency_chains
                .entry(node_id)
                .or_insert_with(BTreeMap::new)
                .insert(prop_type, chain);
        }

        // Step 3: Find and invalidate all dependent chains
        for (dep_node_id, chains) in self.dependency_chains.iter_mut() {
            let mut node_affected = false;

            for (dep_prop_type, chain) in chains.iter_mut() {
                if chain.depends_on(node_id) {
                    // Invalidate the cached value
                    chain.cached_pixels = None;
                    node_affected = true;
                }
            }

            if node_affected {
                affected_nodes.push(*dep_node_id);
            }
        }

        affected_nodes.push(node_id);
        affected_nodes
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

        // Test that <p> has width: 100% from UA CSS
        let width = cache.get_width(&node_data, &node_id, &node_state);
        assert!(
            width.is_some(),
            "Expected <p> to have width property from UA CSS"
        );
        if let Some(w) = width {
            println!("Width value: {:?}", w);
        }

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

        let node_data = NodeData::new(NodeType::Body);

        let node_id = NodeId::new(0);
        let node_state = StyledNodeState::default();

        // Test that <body> has width: 100% from UA CSS
        let width = cache.get_width(&node_data, &node_id, &node_state);
        assert!(
            width.is_some(),
            "Expected <body> to have width: 100% from UA CSS"
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
