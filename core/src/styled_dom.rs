//! `StyledDom` — the result of applying CSS styles to a DOM tree.
//!
//! This module contains [`StyledDom`], which is produced by combining a [`Dom`]
//! with a [`Css`] stylesheet via [`StyledDom::create`]. It stores the flattened
//! node hierarchy, per-node styled states, cascade information, and the CSS
//! property cache. Restyle operations (`restyle_nodes_hover`, etc.) allow
//! incremental updates when pseudo-class states change at runtime.
//!
//! `StyledDom` is the primary input to the layout engine.

use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
};

use azul_css::{
    css::Css,
    props::{
        basic::{StyleFontFamily, StyleFontFamilyVec, StyleFontSize},
        property::{
            BoxDecorationBreakValue, BreakInsideValue, CaretAnimationDurationValue,
            CaretColorValue, ColumnCountValue, ColumnFillValue, ColumnRuleColorValue,
            ColumnRuleStyleValue, ColumnRuleWidthValue, ColumnSpanValue, ColumnWidthValue,
            ContentValue, CounterIncrementValue, CounterResetValue, CssProperty, CssPropertyType,
            RelayoutScope,
            FlowFromValue, FlowIntoValue, LayoutAlignContentValue, LayoutAlignItemsValue,
            LayoutAlignSelfValue, LayoutBorderBottomWidthValue, LayoutBorderLeftWidthValue,
            LayoutBorderRightWidthValue, LayoutBorderTopWidthValue, LayoutBoxSizingValue,
            LayoutClearValue, LayoutColumnGapValue, LayoutDisplayValue, LayoutFlexBasisValue,
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
            LayoutTextJustifyValue, LayoutTopValue, LayoutWidthValue, LayoutWritingModeValue,
            LayoutZIndexValue, OrphansValue, PageBreakValue,
            SelectionBackgroundColorValue, SelectionColorValue, ShapeImageThresholdValue,
            ShapeMarginValue, ShapeOutsideValue, StringSetValue, StyleBackfaceVisibilityValue,
            StyleBackgroundContentVecValue, StyleBackgroundPositionVecValue,
            StyleBackgroundRepeatVecValue, StyleBackgroundSizeVecValue,
            StyleBorderBottomColorValue, StyleBorderBottomLeftRadiusValue,
            StyleBorderBottomRightRadiusValue, StyleBorderBottomStyleValue,
            StyleBorderLeftColorValue, StyleBorderLeftStyleValue, StyleBorderRightColorValue,
            StyleBorderRightStyleValue, StyleBorderTopColorValue, StyleBorderTopLeftRadiusValue,
            StyleBorderTopRightRadiusValue, StyleBorderTopStyleValue, StyleBoxShadowValue,
            StyleCursorValue, StyleDirectionValue, StyleFilterVecValue, StyleFontFamilyVecValue,
            StyleFontSizeValue, StyleFontValue, StyleHyphensValue, StyleLetterSpacingValue,
            StyleLineHeightValue, StyleMixBlendModeValue, StyleOpacityValue,
            StylePerspectiveOriginValue, StyleScrollbarColorValue, StyleTabSizeValue,
            StyleTextAlignValue, StyleTextColorValue, StyleTransformOriginValue,
            StyleTransformVecValue, StyleVisibilityValue, StyleWhiteSpaceValue,
            StyleWordSpacingValue, WidowsValue,
        },
        style::StyleTextColor,
    },
    AzString,
};

use crate::{
    callbacks::Update,
    dom::{Dom, DomId, NodeData, NodeDataVec, OptionTabIndex, TabIndex, TagId},
    events::{RelayoutNodes, RestyleNodes},
    id::{
        Node, NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut, NodeHierarchy,
        NodeId,
    },
    menu::Menu,
    prop_cache::{CssPropertyCache, CssPropertyCachePtr},
    refany::RefAny,
    resources::{Au, ImageCache, ImageRef, ImmediateFontId, RendererResources},
    style::{
        construct_html_cascade_tree, matches_html_element, rule_ends_with, CascadeInfo,
        CascadeInfoVec,
    },
    FastBTreeSet, OrderedMap,
};

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub struct ChangedCssProperty {
    pub previous_state: StyledNodeState,
    pub previous_prop: CssProperty,
    pub current_state: StyledNodeState,
    pub current_prop: CssProperty,
}

impl_option!(
    ChangedCssProperty,
    OptionChangedCssProperty,
    copy = false,
    [Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord]
);

impl_vec!(ChangedCssProperty, ChangedCssPropertyVec, ChangedCssPropertyVecDestructor, ChangedCssPropertyVecDestructorType, ChangedCssPropertyVecSlice, OptionChangedCssProperty);
impl_vec_debug!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_partialord!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_clone!(
    ChangedCssProperty,
    ChangedCssPropertyVec,
    ChangedCssPropertyVecDestructor
);
impl_vec_partialeq!(ChangedCssProperty, ChangedCssPropertyVec);

/// Focus state change for restyle operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusChange {
    /// Node that lost focus (if any)
    pub lost_focus: Option<NodeId>,
    /// Node that gained focus (if any)
    pub gained_focus: Option<NodeId>,
}

/// Hover state change for restyle operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverChange {
    /// Nodes that the mouse left
    pub left_nodes: Vec<NodeId>,
    /// Nodes that the mouse entered
    pub entered_nodes: Vec<NodeId>,
}

/// Active (mouse down) state change for restyle operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveChange {
    /// Nodes that were deactivated (mouse up)
    pub deactivated: Vec<NodeId>,
    /// Nodes that were activated (mouse down)
    pub activated: Vec<NodeId>,
}

/// Result of a restyle operation, indicating what needs to be updated
#[derive(Debug, Clone, Default)]
pub struct RestyleResult {
    /// Nodes whose CSS properties changed, with details of the changes
    pub changed_nodes: RestyleNodes,
    /// Whether layout needs to be recalculated (layout properties changed)
    pub needs_layout: bool,
    /// Whether display list needs regeneration (visual properties changed)
    pub needs_display_list: bool,
    /// Whether only GPU-level properties changed (opacity, transform)
    /// If true and `needs_display_list` is false, we can update via GPU without display list rebuild
    pub gpu_only_changes: bool,
    /// The highest `RelayoutScope` seen across all property changes.
    ///
    /// This enables the IFC incremental layout optimization (Phase 2):
    /// - `None`      → repaint only, zero layout work
    /// - `IfcOnly`   → only the affected IFC needs re-shaping/repositioning
    /// - `SizingOnly`→ this node's size changed, parent repositions siblings
    /// - `Full`      → full subtree relayout
    ///
    /// When `max_relayout_scope <= IfcOnly`, the layout engine can skip
    /// full `calculate_layout_for_subtree` and use the IFC fast path instead.
    pub max_relayout_scope: RelayoutScope,
}

impl RestyleResult {
    /// Returns true if any changes occurred
    #[must_use] pub fn has_changes(&self) -> bool {
        !self.changed_nodes.is_empty()
    }

    /// Merge another `RestyleResult` into this one
    pub fn merge(&mut self, other: Self) {
        for (node_id, changes) in other.changed_nodes {
            self.changed_nodes.entry(node_id).or_default().extend(changes);
        }
        self.needs_layout = self.needs_layout || other.needs_layout;
        self.needs_display_list = self.needs_display_list || other.needs_display_list;
        self.gpu_only_changes = self.gpu_only_changes && other.gpu_only_changes;
        // Keep the highest (most expensive) scope
        if other.max_relayout_scope > self.max_relayout_scope {
            self.max_relayout_scope = other.max_relayout_scope;
        }
    }
}

/// NOTE: multiple states can be active at the same time
///
/// Tracks all CSS pseudo-class states for a node.
/// Each flag is independent - a node can be both :hover and :focus simultaneously.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Hash, PartialOrd, Eq, Ord, Default)]
pub struct StyledNodeState {
    /// Element is being hovered (:hover)
    pub hover: bool,
    /// Element is active/being clicked (:active)
    pub active: bool,
    /// Element has focus (:focus)
    pub focused: bool,
    /// Element is disabled (:disabled)
    pub disabled: bool,
    /// Element is checked/selected (:checked)
    pub checked: bool,
    /// Element or descendant has focus (:focus-within)
    pub focus_within: bool,
    /// Link has been visited (:visited)
    pub visited: bool,
    /// Window is not focused (:backdrop) - GTK compatibility
    pub backdrop: bool,
    /// Element is currently being dragged (:dragging)
    pub dragging: bool,
    /// A dragged element is over this drop target (:drag-over)
    pub drag_over: bool,
}

impl fmt::Debug for StyledNodeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut v = Vec::new();
        if self.hover {
            v.push("hover");
        }
        if self.active {
            v.push("active");
        }
        if self.focused {
            v.push("focused");
        }
        if self.disabled {
            v.push("disabled");
        }
        if self.checked {
            v.push("checked");
        }
        if self.focus_within {
            v.push("focus_within");
        }
        if self.visited {
            v.push("visited");
        }
        if self.backdrop {
            v.push("backdrop");
        }
        if self.dragging {
            v.push("dragging");
        }
        if self.drag_over {
            v.push("drag_over");
        }
        if v.is_empty() {
            v.push("normal");
        }
        write!(f, "{v:?}")
    }
}

impl StyledNodeState {
    /// Creates a new state with all states set to false (normal state).
    #[must_use] pub const fn new() -> Self {
        Self {
            hover: false,
            active: false,
            focused: false,
            disabled: false,
            checked: false,
            focus_within: false,
            visited: false,
            backdrop: false,
            dragging: false,
            drag_over: false,
        }
    }

    /// Check if a specific pseudo-state is active
    #[must_use] pub const fn has_state(&self, state_type: u8) -> bool {
        match state_type {
            0 => true, // Normal is always active
            1 => self.hover,
            2 => self.active,
            3 => self.focused,
            4 => self.disabled,
            5 => self.checked,
            6 => self.focus_within,
            7 => self.visited,
            8 => self.backdrop,
            9 => self.dragging,
            10 => self.drag_over,
            _ => false,
        }
    }

    /// Returns true if no special state is active (just normal)
    #[must_use] pub const fn is_normal(&self) -> bool {
        !self.hover
            && !self.active
            && !self.focused
            && !self.disabled
            && !self.checked
            && !self.focus_within
            && !self.visited
            && !self.backdrop
            && !self.dragging
            && !self.drag_over
    }

    /// Create from `PseudoStateFlags`
    #[must_use] pub const fn from_pseudo_state_flags(flags: &azul_css::dynamic_selector::PseudoStateFlags) -> Self {
        Self {
            hover: flags.hover,
            active: flags.active,
            focused: flags.focused,
            disabled: flags.disabled,
            checked: flags.checked,
            focus_within: flags.focus_within,
            visited: flags.visited,
            backdrop: flags.backdrop,
            dragging: flags.dragging,
            drag_over: flags.drag_over,
        }
    }
}

/// A styled Dom node
// Per-DOM-node hot type passed by reference throughout the layout/style
// pipeline; kept non-Copy on purpose so it isn't silently bulk-copied and to
// avoid trivially_copy_pass_by_ref churn across the many &StyledNode callers.
#[allow(missing_copy_implementations)]
#[repr(C)]
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd)]
pub struct StyledNode {
    /// Current state of this styled node (used later for caching the style / layout)
    pub styled_node_state: StyledNodeState,
}

impl_option!(
    StyledNode,
    OptionStyledNode,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd]
);

impl_vec!(StyledNode, StyledNodeVec, StyledNodeVecDestructor, StyledNodeVecDestructorType, StyledNodeVecSlice, OptionStyledNode);
impl_vec_mut!(StyledNode, StyledNodeVec);
impl_vec_debug!(StyledNode, StyledNodeVec);
impl_vec_partialord!(StyledNode, StyledNodeVec);
impl_vec_clone!(StyledNode, StyledNodeVec, StyledNodeVecDestructor);
impl_vec_partialeq!(StyledNode, StyledNodeVec);

impl StyledNodeVec {
    /// Returns an immutable container reference for indexed access.
    #[must_use] pub fn as_container(&self) -> NodeDataContainerRef<'_, StyledNode> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    /// Returns a mutable container reference for indexed access.
    pub fn as_container_mut(&mut self) -> NodeDataContainerRefMut<'_, StyledNode> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

#[test]
#[allow(clippy::used_underscore_binding)] // intentional `_`-prefix (FFI/api.json pub field, or cfg-gated binding); access is deliberate
fn test_css_styling_with_nested_divs() {
    let s = "
        html, body, p {
            margin: 0;
            padding: 0;
        }
        #div1 {
            border: solid black;
            height: 2in;
            position: absolute;
            top: 1in;
            width: 3in;
        }
        div div {
            background: blue;
            height: 1in;
            position: fixed;
            width: 1in;
        }
    ";

    let css = azul_css::parser2::new_from_str(s);
    let mut _styled_dom = Dom::create_body()
        .with_children(
            vec![Dom::create_div()
                .with_ids_and_classes(
                    vec![crate::dom::IdOrClass::Id("div1".to_string().into())].into(),
                )
                .with_children(vec![Dom::create_div()].into())]
            .into(),
        );
    _styled_dom.add_component_css(css.0);
}

/// Regression test for the calc.c "frame ≥2 loses all backgrounds" bug:
/// `recompute_inheritance_and_compact_cache()` must reproduce the
/// `hot_flags` that `create_from_compact_dom` produced on frame 1. If the
/// recompute path silently drops to the getters-only `build_compact_cache`
/// variant, `HOT_FLAG_HAS_BACKGROUND` is never written, the renderer's
/// `has_any_background()` negative fast-path returns false for every node,
/// and every painted background vanishes on the next layout pass.
#[test]
fn test_recompute_preserves_hot_flag_has_background() {
    use azul_css::compact_cache::HOT_FLAG_HAS_BACKGROUND;

    let css_str = "
        body { margin: 0; padding: 0; }
        .painted { background: red; width: 100px; height: 100px; }
    ";
    let css = azul_css::parser2::new_from_str(css_str).0;

    let mut dom = Dom::create_body().with_children(
        vec![Dom::create_div().with_class("painted".to_string().into())].into(),
    );
    let mut styled = StyledDom::create(&mut dom, css);

    // Frame 1: find the painted node by walking its hot_flags.
    let any_bg_frame1 = {
        let cache = styled
            .css_property_cache
            .ptr
            .compact_cache
            .as_ref()
            .expect("compact_cache populated by create_from_compact_dom");
        (0..styled.node_hierarchy.as_ref().len())
            .any(|i| cache.tier2_cold[i].hot_flags & HOT_FLAG_HAS_BACKGROUND != 0)
    };
    assert!(
        any_bg_frame1,
        "frame 1: expected HOT_FLAG_HAS_BACKGROUND on the .painted node",
    );

    // Frame 2+: simulate regenerate_layout rebuilding the compact cache.
    // This is the path the calculator hit on every resize tick, and the
    // one that had silently regressed to the getter-only builder.
    styled.recompute_inheritance_and_compact_cache();

    let any_bg_frame2 = {
        let cache = styled
            .css_property_cache
            .ptr
            .compact_cache
            .as_ref()
            .expect("compact_cache rebuilt by recompute_inheritance_and_compact_cache");
        (0..styled.node_hierarchy.as_ref().len())
            .any(|i| cache.tier2_cold[i].hot_flags & HOT_FLAG_HAS_BACKGROUND != 0)
    };
    assert!(
        any_bg_frame2,
        "frame ≥2 after recompute_inheritance_and_compact_cache: \
         HOT_FLAG_HAS_BACKGROUND disappeared. The recompute path must \
         use build_compact_cache_with_inheritance (not plain \
         build_compact_cache) so apply_css_property_to_compact runs and \
         populates hot_flags for the renderer's negative fast-paths.",
    );
}

/// Calculated hash of a font-family
#[derive(Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct StyleFontFamilyHash(pub u64);

impl ::core::fmt::Debug for StyleFontFamilyHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StyleFontFamilyHash({})", self.0)
    }
}

impl StyleFontFamilyHash {
    /// Computes a 64-bit hash of a font family for cache lookups.
    #[must_use] pub fn new(family: &StyleFontFamily) -> Self {
        use core::hash::Hasher;
        let mut hasher = crate::hash::DefaultHasher::new();
        family.hash(&mut hasher);
        Self(hasher.finish())
    }
}

/// Calculated hash of a font-family
#[derive(Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct StyleFontFamiliesHash(pub u64);

impl ::core::fmt::Debug for StyleFontFamiliesHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StyleFontFamiliesHash({})", self.0)
    }
}

impl StyleFontFamiliesHash {
    /// Computes a 64-bit hash of multiple font families for cache lookups.
    #[must_use] pub fn new(families: &[StyleFontFamily]) -> Self {
        use core::hash::Hasher;
        let mut hasher = crate::hash::DefaultHasher::new();
        // Prefix with the length so that e.g. `[A, B]` and `[AB]` (or any two
        // family lists whose concatenated element hashes coincide) cannot
        // collide into the same cache key.
        families.len().hash(&mut hasher);
        for f in families {
            f.hash(&mut hasher);
        }
        Self(hasher.finish())
    }
}

/// FFI-safe representation of `Option<NodeId>` as a single `usize`.
///
/// # Encoding (1-based)
///
/// - `inner = 0` → `None` (no node)
/// - `inner = n > 0` → `Some(NodeId(n - 1))`
///
/// This type exists because C/C++ cannot use Rust's `Option` type.
/// Use [`NodeHierarchyItemId::into_crate_internal`] to decode and
/// [`NodeHierarchyItemId::from_crate_internal`] to encode.
///
/// # Difference from `NodeId`
///
/// - **`NodeId`**: A 0-based array index. `NodeId::new(0)` refers to the first node.
///   Use directly for array indexing: `nodes[node_id.index()]`.
///
/// - **`NodeHierarchyItemId`**: A 1-based encoded `Option<NodeId>`.
///   `inner = 0` means `None`, `inner = 1` means `Some(NodeId(0))`.
///   **Never use `inner` as an array index!** Always decode first.
///
/// # Warning
///
/// The `inner` field uses **1-based encoding**, not a direct index!
/// Never use `inner` directly as an array index - always decode first.
///
/// # Example
///
/// ```ignore
/// // Encoding: Option<NodeId> -> NodeHierarchyItemId
/// let opt = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(5)));
/// assert_eq!(opt.into_raw(), 6);  // 5 + 1 = 6
///
/// // Decoding: NodeHierarchyItemId -> Option<NodeId>
/// let decoded = opt.into_crate_internal();
/// assert_eq!(decoded, Some(NodeId::new(5)));
///
/// // None case
/// let none = NodeHierarchyItemId::NONE;
/// assert_eq!(none.into_raw(), 0);
/// assert_eq!(none.into_crate_internal(), None);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct NodeHierarchyItemId {
    // Uses 1-based encoding: 0 = None, n > 0 = Some(NodeId(n-1))
    // Do NOT use directly as an array index!
    inner: usize,
}

impl fmt::Debug for NodeHierarchyItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.into_crate_internal() {
            Some(n) => write!(f, "Some(NodeId({n}))"),
            None => write!(f, "None"),
        }
    }
}

impl fmt::Display for NodeHierarchyItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl NodeHierarchyItemId {
    /// Represents `None` (no node). Encoded as `inner = 0`.
    pub const NONE: Self = Self { inner: 0 };

    /// Creates an `NodeHierarchyItemId` from a raw 1-based encoded value.
    ///
    /// # Warning
    ///
    /// The value must use 1-based encoding (0 = None, n = NodeId(n-1)).
    /// Prefer using [`NodeHierarchyItemId::from_crate_internal`] instead.
    #[inline]
    #[must_use] pub const fn from_raw(value: usize) -> Self {
        Self { inner: value }
    }

    /// Returns the raw 1-based encoded value.
    ///
    /// # Warning
    ///
    /// The returned value uses 1-based encoding. Do NOT use as an array index!
    #[inline]
    #[must_use] pub const fn into_raw(&self) -> usize {
        self.inner
    }
}

impl_option!(
    NodeHierarchyItemId,
    OptionNodeHierarchyItemId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(NodeHierarchyItemId, NodeHierarchyItemIdVec, NodeHierarchyItemIdVecDestructor, NodeHierarchyItemIdVecDestructorType, NodeHierarchyItemIdVecSlice, OptionNodeHierarchyItemId);
impl_vec_mut!(NodeHierarchyItemId, NodeHierarchyItemIdVec);
impl_vec_debug!(NodeHierarchyItemId, NodeHierarchyItemIdVec);
impl_vec_ord!(NodeHierarchyItemId, NodeHierarchyItemIdVec);
impl_vec_eq!(NodeHierarchyItemId, NodeHierarchyItemIdVec);
impl_vec_hash!(NodeHierarchyItemId, NodeHierarchyItemIdVec);
impl_vec_partialord!(NodeHierarchyItemId, NodeHierarchyItemIdVec);
impl_vec_clone!(NodeHierarchyItemId, NodeHierarchyItemIdVec, NodeHierarchyItemIdVecDestructor);
impl_vec_partialeq!(NodeHierarchyItemId, NodeHierarchyItemIdVec);

impl NodeHierarchyItemId {
    /// Decodes to `Option<NodeId>` (0 = None, n > 0 = Some(NodeId(n-1))).
    #[inline]
    #[must_use] pub const fn into_crate_internal(&self) -> Option<NodeId> {
        NodeId::from_usize(self.inner)
    }

    /// Encodes from `Option<NodeId>` (None → 0, Some(NodeId(n)) → n+1).
    #[inline]
    #[must_use] pub const fn from_crate_internal(t: Option<NodeId>) -> Self {
        Self {
            inner: NodeId::into_raw(&t),
        }
    }
}

impl From<Option<NodeId>> for NodeHierarchyItemId {
    #[inline]
    fn from(opt: Option<NodeId>) -> Self {
        Self::from_crate_internal(opt)
    }
}

impl From<NodeHierarchyItemId> for Option<NodeId> {
    #[inline]
    fn from(id: NodeHierarchyItemId) -> Self {
        id.into_crate_internal()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct NodeHierarchyItem {
    pub parent: usize,
    pub previous_sibling: usize,
    pub next_sibling: usize,
    pub last_child: usize,
}

impl_option!(
    NodeHierarchyItem,
    OptionNodeHierarchyItem,
    [Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash]
);

impl NodeHierarchyItem {
    /// Creates a zeroed hierarchy item (no parent, siblings, or children).
    #[must_use] pub const fn zeroed() -> Self {
        Self {
            parent: 0,
            previous_sibling: 0,
            next_sibling: 0,
            last_child: 0,
        }
    }
}

impl From<Node> for NodeHierarchyItem {
    fn from(node: Node) -> Self {
        Self {
            parent: NodeId::into_raw(&node.parent),
            previous_sibling: NodeId::into_raw(&node.previous_sibling),
            next_sibling: NodeId::into_raw(&node.next_sibling),
            last_child: NodeId::into_raw(&node.last_child),
        }
    }
}

impl NodeHierarchyItem {
    /// Returns the parent node ID, if any.
    #[must_use] pub const fn parent_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.parent)
    }
    /// Returns the previous sibling node ID, if any.
    #[must_use] pub const fn previous_sibling_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.previous_sibling)
    }
    /// Returns the next sibling node ID, if any.
    #[must_use] pub const fn next_sibling_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.next_sibling)
    }
    /// Returns the first child node ID (`current_node_id` + 1 if has children).
    #[must_use] pub fn first_child_id(&self, current_node_id: NodeId) -> Option<NodeId> {
        self.last_child_id().map(|_| current_node_id + 1)
    }
    /// Returns the last child node ID, if any.
    #[must_use] pub const fn last_child_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.last_child)
    }
}

impl_vec!(NodeHierarchyItem, NodeHierarchyItemVec, NodeHierarchyItemVecDestructor, NodeHierarchyItemVecDestructorType, NodeHierarchyItemVecSlice, OptionNodeHierarchyItem);
impl_vec_mut!(NodeHierarchyItem, NodeHierarchyItemVec);
impl_vec_debug!(AzNode, NodeHierarchyItemVec);
impl_vec_partialord!(AzNode, NodeHierarchyItemVec);
impl_vec_clone!(
    NodeHierarchyItem,
    NodeHierarchyItemVec,
    NodeHierarchyItemVecDestructor
);
impl_vec_partialeq!(AzNode, NodeHierarchyItemVec);

impl NodeHierarchyItemVec {
    /// Returns an immutable container reference for indexed access.
    #[must_use] pub fn as_container(&self) -> NodeDataContainerRef<'_, NodeHierarchyItem> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    /// Returns a mutable container reference for indexed access.
    pub fn as_container_mut(&mut self) -> NodeDataContainerRefMut<'_, NodeHierarchyItem> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

impl NodeDataContainerRef<'_, NodeHierarchyItem> {
    /// Returns the number of descendant nodes under the given parent.
    #[inline]
    #[must_use] pub fn subtree_len(&self, parent_id: NodeId) -> usize {
        let self_item_index = parent_id.index();
        let next_item_index = self[parent_id].next_sibling_id().map_or_else(|| self.len(), |s| s.index());
        // saturating: a malformed FastDom can leave next_sibling <= parent,
        // which would underflow-panic the subtraction.
        next_item_index.saturating_sub(self_item_index).saturating_sub(1)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct ParentWithNodeDepth {
    pub depth: usize,
    pub node_id: NodeHierarchyItemId,
}

impl_option!(
    ParentWithNodeDepth,
    OptionParentWithNodeDepth,
    [Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash]
);

impl fmt::Debug for ParentWithNodeDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{ depth: {}, node: {:?} }}",
            self.depth,
            self.node_id.into_crate_internal()
        )
    }
}

impl_vec!(ParentWithNodeDepth, ParentWithNodeDepthVec, ParentWithNodeDepthVecDestructor, ParentWithNodeDepthVecDestructorType, ParentWithNodeDepthVecSlice, OptionParentWithNodeDepth);
impl_vec_mut!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_debug!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_partialord!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_clone!(
    ParentWithNodeDepth,
    ParentWithNodeDepthVec,
    ParentWithNodeDepthVecDestructor
);
impl_vec_partialeq!(ParentWithNodeDepth, ParentWithNodeDepthVec);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
#[repr(C)]
pub struct TagIdToNodeIdMapping {
    // Hit-testing tag ID (not all nodes have a tag, only nodes that are hit-testable)
    pub tag_id: TagId,
    /// Node ID of the node that has a tag
    pub node_id: NodeHierarchyItemId,
    /// Whether this node has a tab-index field
    pub tab_index: OptionTabIndex,
}

impl_option!(
    TagIdToNodeIdMapping,
    OptionTagIdToNodeIdMapping,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Ord, PartialOrd]
);

impl_vec!(TagIdToNodeIdMapping, TagIdToNodeIdMappingVec, TagIdToNodeIdMappingVecDestructor, TagIdToNodeIdMappingVecDestructorType, TagIdToNodeIdMappingVecSlice, OptionTagIdToNodeIdMapping);
impl_vec_mut!(TagIdToNodeIdMapping, TagIdToNodeIdMappingVec);
impl_vec_debug!(TagIdToNodeIdMapping, TagIdToNodeIdMappingVec);
impl_vec_partialord!(TagIdToNodeIdMapping, TagIdToNodeIdMappingVec);
impl_vec_clone!(
    TagIdToNodeIdMapping,
    TagIdToNodeIdMappingVec,
    TagIdToNodeIdMappingVecDestructor
);
impl_vec_partialeq!(TagIdToNodeIdMapping, TagIdToNodeIdMappingVec);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ContentGroup {
    /// The parent of the current node group, i.e. either the root node (0)
    /// or the last positioned node ()
    pub root: NodeHierarchyItemId,
    /// Node ids in order of drawing
    pub children: ContentGroupVec,
}

impl_option!(
    ContentGroup,
    OptionContentGroup,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl_vec!(ContentGroup, ContentGroupVec, ContentGroupVecDestructor, ContentGroupVecDestructorType, ContentGroupVecSlice, OptionContentGroup);
impl_vec_mut!(ContentGroup, ContentGroupVec);
impl_vec_debug!(ContentGroup, ContentGroupVec);
impl_vec_partialord!(ContentGroup, ContentGroupVec);
impl_vec_clone!(ContentGroup, ContentGroupVec, ContentGroupVecDestructor);
impl_vec_partialeq!(ContentGroup, ContentGroupVec);

#[derive(Debug, PartialEq, Clone)]
#[repr(C)]
pub struct StyledDom {
    pub root: NodeHierarchyItemId,
    pub node_hierarchy: NodeHierarchyItemVec,
    pub node_data: NodeDataVec,
    pub styled_nodes: StyledNodeVec,
    pub cascade_info: CascadeInfoVec,
    pub nodes_with_window_callbacks: NodeHierarchyItemIdVec,
    pub nodes_with_datasets: NodeHierarchyItemIdVec,
    pub tag_ids_to_node_ids: TagIdToNodeIdMappingVec,
    pub non_leaf_nodes: ParentWithNodeDepthVec,
    pub css_property_cache: CssPropertyCachePtr,
    /// The ID of this DOM in the layout tree (for multi-DOM support with `VirtualViews`)
    pub dom_id: DomId,
}
impl_option!(
    StyledDom,
    OptionStyledDom,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl Default for StyledDom {
    fn default() -> Self {
        let root_node: NodeHierarchyItem = Node::ROOT.into();
        let root_node_id: NodeHierarchyItemId =
            NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO));
        Self {
            root: root_node_id,
            node_hierarchy: vec![root_node].into(),
            node_data: vec![NodeData::create_body()].into(),
            styled_nodes: vec![StyledNode::default()].into(),
            cascade_info: vec![CascadeInfo {
                index_in_parent: 0,
                is_last_child: true,
            }]
            .into(),
            tag_ids_to_node_ids: Vec::new().into(),
            non_leaf_nodes: vec![ParentWithNodeDepth {
                depth: 0,
                node_id: root_node_id,
            }]
            .into(),
            nodes_with_window_callbacks: Vec::new().into(),
            nodes_with_datasets: Vec::new().into(),
            css_property_cache: CssPropertyCachePtr::new(CssPropertyCache::empty(1)),
            dom_id: DomId::ROOT_ID,
        }
    }
}

/// Per-field heap-byte breakdown of a `StyledDom`.
#[derive(Debug, Clone, Copy, Default)]
pub struct StyledDomMemoryReport {
    pub node_count: usize,
    pub node_hierarchy_bytes: usize,
    pub node_data_bytes: usize,
    pub styled_nodes_bytes: usize,
    pub cascade_info_bytes: usize,
    pub tag_ids_bytes: usize,
    pub non_leaf_nodes_bytes: usize,
    pub callback_vecs_bytes: usize,
    pub css_property_cache: crate::prop_cache::CssPropertyCacheBreakdown,
}

impl StyledDomMemoryReport {
    #[must_use] pub const fn total_bytes(&self) -> usize {
        self.node_hierarchy_bytes
            + self.node_data_bytes
            + self.styled_nodes_bytes
            + self.cascade_info_bytes
            + self.tag_ids_bytes
            + self.non_leaf_nodes_bytes
            + self.callback_vecs_bytes
            + self.css_property_cache.total_bytes()
    }
}

impl StyledDom {
    /// Approximate heap bytes retained by this `StyledDom`, broken out by field.
    #[must_use] pub fn memory_report(&self) -> StyledDomMemoryReport {
        let n = self.node_data.len();
        StyledDomMemoryReport {
            node_count: n,
            node_hierarchy_bytes: size_of_val(self.node_hierarchy.as_ref()),
            node_data_bytes: {
                let base = n * size_of::<NodeData>();
                // NodeData contains inline Vecs (callbacks, css_props, datasets)
                // that have their own heap allocations. Approximate:
                let mut inner = 0usize;
                for nd in self.node_data.as_ref() {
                    inner += nd.get_callbacks().len() * 64; // rough per-callback
                    // Each rule = path + decls Vec + conditions Vec + priority byte.
                    // Approximate at 64 bytes per rule + the heap for declarations.
                    inner += nd.style.rules.as_ref().len() * 64;
                }
                base + inner
            },
            styled_nodes_bytes: n * size_of::<StyledNode>(),
            cascade_info_bytes: n * size_of::<CascadeInfo>(),
            tag_ids_bytes: size_of_val(self.tag_ids_to_node_ids.as_ref()),
            non_leaf_nodes_bytes: size_of_val(self.non_leaf_nodes.as_ref()),
            callback_vecs_bytes:
                self.nodes_with_window_callbacks.as_ref().len() * 8
                + self.nodes_with_datasets.as_ref().len() * 8,
            css_property_cache: self.css_property_cache.ptr.memory_breakdown(),
        }
    }

    /// Creates a new `StyledDom` by applying CSS styles to a DOM tree.
    ///
    /// NOTE: After calling this function, the DOM will be reset to an empty DOM.
    // This is for memory optimization, so that the DOM does not need to be cloned.
    //
    // The CSS will be left in-place, but will be re-ordered
    pub fn create(dom: &mut Dom, css: Css) -> Self {
        use core::mem;

        let mut swap_dom = Dom::create_body();
        mem::swap(dom, &mut swap_dom);

        let compact_dom: CompactDom = swap_dom.into();
        let node_hierarchy: NodeHierarchyItemVec = compact_dom
            .node_hierarchy
            .as_ref()
            .internal
            .iter()
            .map(|i| (*i).into())
            .collect::<Vec<NodeHierarchyItem>>()
            .into();

        Self::create_from_compact_dom(compact_dom, css, node_hierarchy)
    }

    /// Creates a `StyledDom` from a `FastDom` (arena-based DOM).
    ///
    /// This skips the `convert_dom_into_compact_dom` tree→arena conversion
    /// entirely since `FastDom` already has flat `NodeHierarchyItemVec` and
    /// `NodeDataVec`. CSS is collected from `CssWithNodeIdVec`.
    #[must_use] pub fn create_from_fast_dom(fast_dom: crate::dom::FastDom) -> Self {
        use azul_css::css::Css;

        // 1. Merge CSS from CssWithNodeIdVec into a single Css, scoping each
        //    node-attached stylesheet to its owner's subtree (#47): push_front a
        //    Root([owner, owner+subtree_len]) selector so inline/XML css can't leak
        //    globally — the same scoping the recursive create_from_dom path applies
        //    via scope_inline_css. `node_id` is the owner's flat id (0 = root).
        let mut combined_rules: Vec<azul_css::css::CssRuleBlock> = Vec::new();
        let css_entries = fast_dom.css.into_library_owned_vec();
        {
            let hierarchy = fast_dom.node_hierarchy.as_container();
            for css_with_id in css_entries {
                let owner = css_with_id.node_id;
                let end = if owner < hierarchy.len() {
                    owner + hierarchy.subtree_len(NodeId::new(owner))
                } else {
                    owner
                };
                for mut rule in css_with_id.css.rules.into_library_owned_vec() {
                    rule.path.push_front_scope(owner, end);
                    combined_rules.push(rule);
                }
            }
        }
        let combined_css = if combined_rules.is_empty() {
            Css::empty()
        } else {
            Css::new(combined_rules)
        };

        // 2. Convert NodeHierarchyItemVec → NodeHierarchy (Vec<Node>)
        //    for cascade tree computation
        let node_hierarchy_items = fast_dom.node_hierarchy;
        let nodes: Vec<Node> = node_hierarchy_items.as_ref()
            .iter()
            .map(|item| Node {
                parent: NodeId::from_usize(item.parent),
                previous_sibling: NodeId::from_usize(item.previous_sibling),
                next_sibling: NodeId::from_usize(item.next_sibling),
                last_child: NodeId::from_usize(item.last_child),
            })
            .collect();
        let node_hierarchy_internal = NodeHierarchy { internal: nodes };

        // 3. Build CompactDom from the flat arenas (no conversion needed)
        let node_data_vec = fast_dom.node_data.into_library_owned_vec();
        let compact_dom = CompactDom {
            node_hierarchy: node_hierarchy_internal,
            node_data: NodeDataContainer { internal: node_data_vec },
            root: NodeId::ZERO,
        };

        // 4. Delegate to create() which handles cascade, UA CSS, etc.
        //    We need a mutable Dom to pass to create(), but we already have CompactDom.
        //    Instead, inline the cascade logic from create() with our CompactDom.
        Self::create_from_compact_dom(compact_dom, combined_css, node_hierarchy_items)
    }

    /// Internal: creates `StyledDom` from a `CompactDom` + CSS + pre-built hierarchy items.
    /// Shared by both the Slow path (create → `convert_dom_into_compact_dom` → this)
    /// and the Fast path (`create_from_fast_dom` → this).
    #[allow(clippy::similar_names)] // domain-standard coordinate/control-point names
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
    fn create_from_compact_dom(
        compact_dom: CompactDom,
        mut css: Css,
        node_hierarchy: NodeHierarchyItemVec,
    ) -> Self {
        use crate::dom::EventFilter;

        static CASCADE_BREAKDOWN: crate::sync::OnceLock<bool> = crate::sync::OnceLock::new();
        let cascade_dbg = *CASCADE_BREAKDOWN.get_or_init(crate::profile::memory_enabled);

        let node_count = compact_dom.len();

        let non_leaf_nodes = compact_dom
            .node_hierarchy
            .as_ref()
            .get_parents_sorted_by_depth();

        let mut styled_nodes = vec![
            StyledNode {
                styled_node_state: StyledNodeState::new()
            };
            node_count
        ];

        let mut css_property_cache = CssPropertyCache::empty(compact_dom.node_data.len());

        let html_tree = construct_html_cascade_tree(
            &compact_dom.node_hierarchy.as_ref(),
            &non_leaf_nodes[..],
            &compact_dom.node_data.as_ref(),
        );

        let non_leaf_nodes = non_leaf_nodes
            .iter()
            .map(|(depth, node_id)| ParentWithNodeDepth {
                depth: *depth,
                node_id: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
            })
            .collect::<Vec<_>>();

        let non_leaf_nodes: ParentWithNodeDepthVec = non_leaf_nodes.into();

        let _restyle_tag_ids = css_property_cache.restyle(
            &mut css,
            &compact_dom.node_data.as_ref(),
            &node_hierarchy,
            &non_leaf_nodes,
            &html_tree.as_ref(),
        );

        // Retain the author stylesheet on the cache (this used to `drop(css)` to
        // save ~500 KiB, but that made runtime-inserted nodes unstyleable: the
        // rules were gone, so nothing could ever re-run the cascade for them —
        // see e2e/bug-inserted-node-no-author-css.json).
        css_property_cache.retained_author_css = css;

        // Apply UA defaults + compute inherited values so consumers that
        // read `css_property_cache.computed_values` (the web/HTML
        // renderer in `dll/src/web/html_render.rs`) see resolved
        // properties. The compact cache below stores the same info in
        // a different layout for the desktop renderer; computed_values
        // is the "tall" form that the web renderer's CSS emitter
        // (`emit_css_from_cache`) walks per node.
        css_property_cache.apply_ua_css(compact_dom.node_data.as_ref().internal);
        css_property_cache.compute_inherited_values(
            node_hierarchy.as_container().internal,
            compact_dom.node_data.as_ref().internal,
        );

        let prev_font_hashes: Vec<u64> = css_property_cache.compact_cache
            .as_ref()
            .map(|c| c.prev_font_hashes.clone())
            .unwrap_or_default();
        let compact = css_property_cache.build_compact_cache_with_inheritance(
            compact_dom.node_data.as_ref().internal,
            node_hierarchy.as_container().internal,
            &prev_font_hashes,
        );
        css_property_cache.compact_cache = Some(compact);
        let pre_prune = if cascade_dbg {
            Some(css_property_cache.memory_breakdown())
        } else { None };
        css_property_cache.prune_compact_normal_props();
        if let Some(pre) = pre_prune {
            let post = css_property_cache.memory_breakdown();
            #[cfg(feature = "std")]
            eprintln!("[PRUNE] css_props {} → {} KiB  cascaded {} → {} KiB  (saved {} KiB)",
                pre.css_props_bytes / 1024, post.css_props_bytes / 1024,
                pre.cascaded_props_bytes / 1024, post.cascaded_props_bytes / 1024,
                (pre.total_bytes().saturating_sub(post.total_bytes())) / 1024);
            #[cfg(not(feature = "std"))]
            let _ = post;
        }

        let tag_ids = css_property_cache.generate_tag_ids(
            &compact_dom.node_data.as_ref(),
            &node_hierarchy,
        );

        if cascade_dbg {
            let bd = css_property_cache.memory_breakdown();
            #[cfg(feature = "std")]
            eprintln!("[CASCADE] {} nodes  cascaded_props={} KiB  css_props={} KiB  compact={} KiB  computed={} KiB  total={} KiB",
                node_count,
                bd.cascaded_props_bytes / 1024, bd.css_props_bytes / 1024,
                bd.compact_cache_bytes / 1024, bd.computed_values_bytes / 1024,
                bd.total_bytes() / 1024);
            #[cfg(not(feature = "std"))]
            let _ = bd;
        }

        // Collect callback/dataset nodes in a single pass (avoids 3 separate 50K scans).
        // For XHTML-parsed DOMs with no callbacks, this early-exits immediately.
        let has_any_callbacks = compact_dom.node_data.as_ref().internal.iter()
            .any(|c| !c.get_callbacks().is_empty() || c.get_dataset().is_some());

        let (nodes_with_window_callbacks, nodes_with_datasets) = if has_any_callbacks {
            let mut win_cbs = Vec::new();
            let mut datasets = Vec::new();
            for (node_id, c) in compact_dom.node_data.as_ref().internal.iter().enumerate() {
                let cbs = c.get_callbacks();
                let has_dataset = c.get_dataset().is_some();
                if !cbs.is_empty() || has_dataset {
                    datasets.push(NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node_id))));
                }
                for cb in cbs {
                    if let EventFilter::Window(_) = cb.event {
                        win_cbs.push(NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node_id))));
                        break;
                    }
                }
            }
            (win_cbs, datasets)
        } else {
            (Vec::new(), Vec::new())
        };
        let mut styled_dom = Self {
            root: NodeHierarchyItemId::from_crate_internal(Some(compact_dom.root)),
            node_hierarchy,
            node_data: compact_dom.node_data.internal.into(),
            cascade_info: html_tree.internal.into(),
            styled_nodes: styled_nodes.into(),
            tag_ids_to_node_ids: tag_ids.into(),
            nodes_with_window_callbacks: nodes_with_window_callbacks.into(),
            nodes_with_datasets: nodes_with_datasets.into(),
            non_leaf_nodes,
            css_property_cache: CssPropertyCachePtr::new(css_property_cache),
            dom_id: DomId::ROOT_ID,
        };
        #[cfg(feature = "table_layout")]
        if let Err(_e) = crate::dom_table::generate_anonymous_table_elements(&mut styled_dom) {
        }

        styled_dom
    }

    /// Creates a `StyledDom` from a recursive Dom tree with deferred CSS.
    ///
    /// This is the Phase 7.2 entry point: the layout callback returns a recursive
    /// `Dom` with `css: Vec<Css>` on each node. This function:
    ///
    /// 1. Collects all CSS objects from the recursive tree
    /// 2. Flattens the Dom into contiguous arrays (`CompactDom`)
    /// 3. Merges all CSS objects and runs a single cascade pass
    /// 4. Runs `apply_ua_css` → `compute_inherited_values` → `build_compact_cache`
    /// 5. Generates anonymous table elements
    #[must_use] pub fn create_from_dom(mut dom: Dom) -> Self {
        use azul_css::css::Css;

        // #47: scope each node's inline css to its subtree BEFORE collecting, so a
        // non-root node's with_css cannot leak to the whole tree. Uses the same
        // pre-order ids the flatten (convert_dom_into_compact_dom) will assign;
        // needs estimated_total_children populated first.
        dom.fixup_children_estimated();
        let mut next_scope_id = 0usize;
        scope_inline_css(&mut dom, &mut next_scope_id);

        // 1. Collect all CSS objects from the recursive Dom tree (now scoped)
        let mut all_css = Vec::new();
        collect_css_from_dom(&dom, &mut all_css);

        // 2. Merge all CSS objects into one combined Css
        let mut combined_css = if all_css.is_empty() {
            Css::empty()
        } else {
            let mut combined_rules: Vec<azul_css::css::CssRuleBlock> = Vec::new();
            for css in all_css {
                combined_rules.extend(css.rules.into_library_owned_vec());
            }
            Css::new(combined_rules)
        };

        // 3. Strip CSS from all Dom nodes before flattening
        //    (CSS is already collected, don't need it in the flat tree)
        strip_css_from_dom(&mut dom);

        // 4. Use existing StyledDom::create to flatten + cascade
        Self::create(&mut dom, combined_css)
    }

    /// Appends another `StyledDom` as a child to the `self.root`
    /// without re-styling the DOM itself
    pub fn append_child(&mut self, other: Self) {
        let self_root_id = self.root.into_crate_internal().unwrap_or(NodeId::ZERO);
        let current_root_children_count = self_root_id
            .az_children(&self.node_hierarchy.as_container())
            .count();
        self.append_child_with_index(other, current_root_children_count);
        self.finalize_non_leaf_nodes();
    }

    /// Optimized version of `append_child` that takes the child index directly
    /// instead of counting existing children (O(1) instead of O(n))
    pub fn append_child_with_index(&mut self, mut other: Self, child_index: usize) {
        // shift all the node ids in other by self.len()
        let self_len = self.node_hierarchy.as_ref().len();
        let other_len = other.node_hierarchy.as_ref().len();
        let self_root_id = self.root.into_crate_internal().unwrap_or(NodeId::ZERO);
        let other_root_id = other.root.into_crate_internal().unwrap_or(NodeId::ZERO);

        // Use provided index instead of counting children
        other.cascade_info.as_mut()[other_root_id.index()].index_in_parent =
            u32::try_from(child_index).unwrap_or(u32::MAX);
        other.cascade_info.as_mut()[other_root_id.index()].is_last_child = true;

        self.cascade_info.append(&mut other.cascade_info);

        // adjust node hierarchy
        for other in other.node_hierarchy.as_mut().iter_mut() {
            if other.parent != 0 {
                other.parent += self_len;
            }
            if other.previous_sibling != 0 {
                other.previous_sibling += self_len;
            }
            if other.next_sibling != 0 {
                other.next_sibling += self_len;
            }
            if other.last_child != 0 {
                other.last_child += self_len;
            }
        }

        other.node_hierarchy.as_container_mut()[other_root_id].parent =
            NodeId::into_raw(&Some(self_root_id));
        let current_last_child = self.node_hierarchy.as_container()[self_root_id].last_child_id();
        other.node_hierarchy.as_container_mut()[other_root_id].previous_sibling =
            NodeId::into_raw(&current_last_child);
        if let Some(current_last) = current_last_child {
            if self.node_hierarchy.as_container_mut()[current_last]
                .next_sibling_id()
                .is_some()
            {
                self.node_hierarchy.as_container_mut()[current_last].next_sibling +=
                    other_root_id.index() + other_len;
            } else {
                self.node_hierarchy.as_container_mut()[current_last].next_sibling =
                    NodeId::into_raw(&Some(NodeId::new(self_len + other_root_id.index())));
            }
        }
        self.node_hierarchy.as_container_mut()[self_root_id].last_child =
            NodeId::into_raw(&Some(NodeId::new(self_len + other_root_id.index())));

        self.node_hierarchy.append(&mut other.node_hierarchy);
        self.node_data.append(&mut other.node_data);
        self.styled_nodes.append(&mut other.styled_nodes);
        self.get_css_property_cache_mut()
            .append(other.get_css_property_cache_mut());

        // Tag IDs are globally unique (AtomicUsize counter) and never collide,
        // so we only shift node_id (which changes when DOMs are merged).
        for tag_id_node_id in &mut other.tag_ids_to_node_ids {
            tag_id_node_id.node_id.inner += self_len;
        }

        self.tag_ids_to_node_ids
            .append(&mut other.tag_ids_to_node_ids);

        for nid in &mut other.nodes_with_window_callbacks {
            nid.inner += self_len;
        }
        self.nodes_with_window_callbacks
            .append(&mut other.nodes_with_window_callbacks);

        for nid in &mut other.nodes_with_datasets {
            nid.inner += self_len;
        }
        self.nodes_with_datasets
            .append(&mut other.nodes_with_datasets);

        // edge case: if the other StyledDom consists of only one node
        // then it is not a parent itself
        if other_len != 1 {
            for other_non_leaf_node in &mut other.non_leaf_nodes {
                other_non_leaf_node.node_id.inner += self_len;
                other_non_leaf_node.depth += 1;
            }
            self.non_leaf_nodes.append(&mut other.non_leaf_nodes);
            // NOTE: Sorting deferred - call finalize_non_leaf_nodes() after all appends
        }
    }

    /// Call this after all `append_child_with_index` operations are complete
    /// to sort `non_leaf_nodes` by depth (required for correct rendering)
    pub fn finalize_non_leaf_nodes(&mut self) {
        self.non_leaf_nodes.sort_by(|a, b| a.depth.cmp(&b.depth));
    }

    /// Same as `append_child()`, but as a builder method
    #[must_use] pub fn with_child(mut self, other: Self) -> Self {
        self.append_child(other);
        self
    }

    /// Sets the context menu for the root node
    pub fn set_context_menu(&mut self, context_menu: Menu) {
        if let Some(root_id) = self.root.into_crate_internal() {
            self.node_data.as_container_mut()[root_id].set_context_menu(context_menu);
        }
    }

    /// Builder method for setting the context menu
    #[must_use] pub fn with_context_menu(mut self, context_menu: Menu) -> Self {
        self.set_context_menu(context_menu);
        self
    }

    /// Sets the menu bar for the root node
    pub fn set_menu_bar(&mut self, menu_bar: Menu) {
        if let Some(root_id) = self.root.into_crate_internal() {
            self.node_data.as_container_mut()[root_id].set_menu_bar(menu_bar);
        }
    }

    /// Builder method for setting the menu bar
    #[must_use] pub fn with_menu_bar(mut self, menu_bar: Menu) -> Self {
        self.set_menu_bar(menu_bar);
        self
    }

    /// Re-compute inherited CSS values and rebuild the compact layout cache.
    ///
    /// This MUST be called after `append_child()` merges multiple `StyledDom`s.
    /// `append_child()` concatenates the CSS property caches but does NOT
    /// re-run inheritance or rebuild the compact cache. This means:
    ///
    /// 1. **Broken inheritance**: Inherited properties (`color`, `font-size`,
    ///    `direction`) from the parent DOM do not flow into appended subtrees.
    /// 2. **Stale compact cache**: The child's tier 1/2/2b entries still reflect
    ///    the child's isolated cascade, not the composed tree.
    ///
    /// Calling this method after all `append_child()` calls fixes both issues
    /// by re-running a full depth-first inheritance pass and rebuilding the
    /// compact cache from scratch on the composed tree.
    pub fn recompute_inheritance_and_compact_cache(&mut self) {
        // Use the _with_inheritance variant: it does inheritance inline (via
        // parent-compact-field copy) AND populates hot_flags via
        // apply_css_property_to_compact.  The plain build_compact_cache would
        // leave HOT_FLAG_HAS_BACKGROUND / HAS_CLIP_PATH / extra_flags at 0,
        // causing renderer negative fast-paths to skip paint (regression
        // introduced by ff059052b).  No SIGABRT risk — _with_inheritance
        // never pushes to the flat cascaded_props storage.
        let prev_font_hashes: Vec<u64> = self.css_property_cache
            .downcast_mut()
            .compact_cache
            .as_ref()
            .map(|c| c.prev_font_hashes.clone())
            .unwrap_or_default();
        let compact = self.css_property_cache
            .downcast_mut()
            .build_compact_cache_with_inheritance(
                self.node_data.as_container().internal,
                self.node_hierarchy.as_container().internal,
                &prev_font_hashes,
            );
        self.css_property_cache.downcast_mut().compact_cache = Some(compact);
    }

    /// Re-applies CSS styles to the existing DOM structure.
    /// Grow retained author-CSS subtree scopes to cover a node just appended under
    /// `parent`. Mount/`with_css` rules carry a `Root([start, end])` scope
    /// (`push_front_scope`) that only matches nodes within a node's ORIGINAL subtree
    /// range, so a node appended afterwards falls outside every scope and
    /// `restyle_retained` cannot match it. Appending under `parent` (rightmost-spine
    /// only, so subtrees stay contiguous in the flat arena) grows `parent`'s and its
    /// ancestors' subtrees; bump the inclusive `end` of every scope that already
    /// covers `parent` out to the new node.
    #[allow(clippy::similar_names)] // new_node/parent and the p/n index locals read clearly in context
    pub fn extend_author_scopes_for_appended(&mut self, new_node: NodeId, parent: NodeId) {
        use azul_css::css::CssPathSelector;
        let p = parent.index();
        let n = new_node.index();
        let cache = self.css_property_cache.downcast_mut();
        for rule in cache.retained_author_css.rules.as_mut() {
            let mut sels = rule.path.selectors.as_ref().to_vec();
            let mut changed = false;
            for sel in &mut sels {
                if let CssPathSelector::Root(range) = sel {
                    if range.contains(p) && range.end < n {
                        range.end = n;
                        changed = true;
                    }
                }
            }
            if changed {
                rule.path.selectors = sels.into();
            }
        }
    }

    /// Re-run the author cascade from the stylesheet retained at creation /
    /// last `restyle` (`CssPropertyCache::retained_author_css`). Call after a
    /// structural DOM mutation (e.g. inserting a node) so new nodes receive
    /// author CSS; a no-op when no author stylesheet was ever attached.
    pub fn restyle_retained(&mut self) {
        let css = self
            .css_property_cache
            .downcast_mut()
            .retained_author_css
            .clone();
        if css.is_empty() {
            return;
        }
        self.restyle(css);
    }

    pub fn restyle(&mut self, mut css: Css) {
        // NOTE: the tag_ids returned by `cache.restyle` here are generated from
        // the STALE `compact_cache` (display/overflow reads) and are intentionally
        // discarded — we regenerate them below AFTER the compact cache and
        // inheritance have been recomputed (audit styled_dom.rs:1404/1426).
        let _stale_tag_ids = self.css_property_cache.downcast_mut().restyle(
            &mut css,
            &self.node_data.as_container(),
            &self.node_hierarchy,
            &self.non_leaf_nodes,
            &self.cascade_info.as_container(),
        );

        // Keep the stylesheet for later structural restyles (inserted nodes).
        self.css_property_cache.downcast_mut().retained_author_css = css;

        // Apply UA CSS properties before computing inheritance
        self.css_property_cache
            .downcast_mut()
            .apply_ua_css(self.node_data.as_container().internal);

        // Compute inherited values after restyle and apply_ua_css (resolves em, %, etc.)
        self.css_property_cache
            .downcast_mut()
            .compute_inherited_values(
                self.node_hierarchy.as_container().internal,
                self.node_data.as_container().internal,
            );

        // The old compact_cache was built from the pre-restyle CSS. If we do not
        // rebuild it, layout-hot properties (display/overflow/background/clip,
        // resolved font sizes) keep their stale values and the restyle silently
        // no-ops for them. Drop it, rebuild via the _with_inheritance path (which
        // repopulates hot_flags), and invalidate the cached resolved font sizes.
        let prev_font_hashes: Vec<u64> = self
            .css_property_cache
            .downcast_mut()
            .compact_cache
            .as_ref()
            .map(|c| c.prev_font_hashes.clone())
            .unwrap_or_default();
        self.css_property_cache.downcast_mut().compact_cache = None;
        let compact = self
            .css_property_cache
            .downcast_mut()
            .build_compact_cache_with_inheritance(
                self.node_data.as_container().internal,
                self.node_hierarchy.as_container().internal,
                &prev_font_hashes,
            );
        self.css_property_cache.downcast_mut().compact_cache = Some(compact);
        self.css_property_cache
            .downcast_mut()
            .invalidate_resolved_font_sizes();

        // Regenerate tag_ids from the freshly rebuilt compact cache so the
        // hit-test map reflects the post-restyle display/overflow values.
        let new_tag_ids = self.css_property_cache.downcast_mut().generate_tag_ids(
            &self.node_data.as_container(),
            &self.node_hierarchy,
        );
        self.tag_ids_to_node_ids = new_tag_ids.into();
    }

    /// Returns the total number of nodes in this `StyledDom`.
    #[inline]
    #[must_use] pub const fn node_count(&self) -> usize {
        self.node_data.len()
    }

    /// Returns an immutable reference to the CSS property cache.
    #[inline]
    #[must_use] pub fn get_css_property_cache(&self) -> &CssPropertyCache {
        &self.css_property_cache.ptr
    }

    /// Returns a mutable reference to the CSS property cache.
    #[inline]
    pub fn get_css_property_cache_mut(&mut self) -> &mut CssPropertyCache {
        &mut self.css_property_cache.ptr
    }

    /// Returns the current state (hover, active, focus) of a styled node.
    #[inline]
    #[must_use] pub fn get_styled_node_state(&self, node_id: &NodeId) -> StyledNodeState {
        self.styled_nodes.as_container()[*node_id]
            .styled_node_state
    }

    /// Updates hover state for nodes and returns changed CSS properties.
    #[must_use]
    pub fn restyle_nodes_hover(
        &mut self,
        nodes: &[NodeId],
        new_hover_state: bool,
    ) -> RestyleNodes {
        self.restyle_nodes_state(
            nodes,
            new_hover_state,
            |state, val| state.hover = val,
            azul_css::dynamic_selector::PseudoStateType::Hover,
        )
    }

    /// Updates active state for nodes and returns changed CSS properties.
    #[must_use]
    pub fn restyle_nodes_active(
        &mut self,
        nodes: &[NodeId],
        new_active_state: bool,
    ) -> RestyleNodes {
        self.restyle_nodes_state(
            nodes,
            new_active_state,
            |state, val| state.active = val,
            azul_css::dynamic_selector::PseudoStateType::Active,
        )
    }

    /// Updates focus state for nodes and returns changed CSS properties.
    #[must_use]
    pub fn restyle_nodes_focus(
        &mut self,
        nodes: &[NodeId],
        new_focus_state: bool,
    ) -> RestyleNodes {
        self.restyle_nodes_state(
            nodes,
            new_focus_state,
            |state, val| state.focused = val,
            azul_css::dynamic_selector::PseudoStateType::Focus,
        )
    }

    /// Generic restyle method parameterized by the state field and pseudo-state type.
    fn restyle_nodes_state(
        &mut self,
        nodes: &[NodeId],
        new_state_value: bool,
        set_state: impl Fn(&mut StyledNodeState, bool),
        pseudo_state_type: azul_css::dynamic_selector::PseudoStateType,
    ) -> RestyleNodes {
        // Drop any stale NodeIds that no longer index into this DOM (e.g. left
        // over from a previous, larger tree). Indexing styled_nodes / node_data
        // with an out-of-range id would panic. Filtering here keeps the
        // downstream zip with `old_node_states` aligned.
        let node_count = self.node_count();
        let nodes: Vec<NodeId> = nodes
            .iter()
            .copied()
            .filter(|nid| nid.index() < node_count)
            .collect();

        // save the old node state
        let old_node_states = nodes
            .iter()
            .map(|nid| {
                self.styled_nodes.as_container()[*nid]
                    .styled_node_state
            })
            .collect::<Vec<_>>();

        for nid in &nodes {
            set_state(
                &mut self.styled_nodes.as_container_mut()[*nid].styled_node_state,
                new_state_value,
            );
        }

        let css_property_cache = self.get_css_property_cache();
        let styled_nodes = self.styled_nodes.as_container();
        let node_data = self.node_data.as_container();

        // scan all properties that could have changed because of addition / removal
        let v = nodes
            .iter()
            .zip(old_node_states.iter())
            .filter_map(|(node_id, old_node_state)| {
                let mut keys_normal: Vec<_> = CssPropertyCache::prop_types_for_state(
                    css_property_cache.css_props.get_slice(node_id.index()),
                    pseudo_state_type,
                ).collect();
                let mut keys_inherited: Vec<_> = CssPropertyCache::prop_types_for_state(
                    css_property_cache.cascaded_props.get_slice(node_id.index()),
                    pseudo_state_type,
                ).collect();
                let keys_inline: Vec<CssPropertyType> = {
                    use azul_css::dynamic_selector::DynamicSelector;
                    node_data[*node_id]
                        .style
                        .iter_inline_properties()
                        .filter_map(|(prop, conds)| {
                            let matches = conds.as_slice().iter().any(|c| {
                                matches!(c, DynamicSelector::PseudoState(pst) if *pst == pseudo_state_type)
                            });
                            if matches {
                                Some(prop.get_type())
                            } else {
                                None
                            }
                        })
                        .collect()
                };
                let mut keys_inline_ref: Vec<_> = keys_inline.iter().collect();

                keys_normal.append(&mut keys_inherited);
                keys_normal.append(&mut keys_inline_ref);

                let node_properties_that_could_have_changed = keys_normal;

                if node_properties_that_could_have_changed.is_empty() {
                    return None;
                }

                let new_node_state = &styled_nodes[*node_id].styled_node_state;
                let node_data = &node_data[*node_id];

                let changes = node_properties_that_could_have_changed
                    .into_iter()
                    .filter_map(|prop| {
                        // calculate both the old and the new state
                        let old = css_property_cache.get_property_slow(
                            node_data,
                            node_id,
                            old_node_state,
                            prop,
                        );
                        let new = css_property_cache.get_property_slow(
                            node_data,
                            node_id,
                            new_node_state,
                            prop,
                        );
                        if old == new {
                            None
                        } else {
                            Some(ChangedCssProperty {
                                previous_state: *old_node_state,
                                previous_prop: old.map_or_else(|| CssProperty::auto(*prop), Clone::clone),
                                current_state: *new_node_state,
                                current_prop: new.map_or_else(|| CssProperty::auto(*prop), Clone::clone),
                            })
                        }
                    })
                    .collect::<Vec<_>>();

                if changes.is_empty() {
                    None
                } else {
                    Some((*node_id, changes))
                }
            })
            .collect::<Vec<_>>();

        v.into_iter().collect()
    }

    /// Unified entry point for all CSS restyle operations.
    ///
    /// This function synchronizes the `StyledNodeState` with runtime state
    /// and computes which CSS properties have changed. It determines whether
    /// layout, display list, or GPU-only updates are needed.
    ///
    /// # Arguments
    /// * `focus_changes` - Nodes gaining/losing focus
    /// * `hover_changes` - Nodes gaining/losing hover
    /// * `active_changes` - Nodes gaining/losing active (mouse down)
    ///
    /// # Returns
    /// * `RestyleResult` containing changed nodes and what needs updating
    #[must_use]
    pub fn restyle_on_state_change(
        &mut self,
        focus_changes: Option<FocusChange>,
        hover_changes: Option<HoverChange>,
        active_changes: Option<ActiveChange>,
    ) -> RestyleResult {
        
        // Start with GPU-only assumption; refined below as changes are analyzed.
        let mut result = RestyleResult {
            gpu_only_changes: true,
            ..RestyleResult::default()
        };

        // Helper closure to merge changes and analyze property categories
        let mut process_changes = |changes: RestyleNodes| {
            for (node_id, props) in changes {
                for change in &props {
                    let prop_type = change.current_prop.get_type();

                    // Use the granular RelayoutScope instead of the binary
                    // can_trigger_relayout(). We pass node_is_ifc_member = true
                    // conservatively: this means font/text property changes will
                    // produce IfcOnly (rather than None). Phase 2c can refine
                    // this by checking whether the node actually participates
                    // in an IFC.
                    let scope = prop_type.relayout_scope(/* node_is_ifc_member */ true);

                    // Track the highest scope seen
                    if scope > result.max_relayout_scope {
                        result.max_relayout_scope = scope;
                    }

                    // Any scope above None triggers layout
                    if scope != RelayoutScope::None {
                        result.needs_layout = true;
                        result.gpu_only_changes = false;
                    }
                    
                    // Check if this is a GPU-only property
                    if !prop_type.is_gpu_only_property() {
                        result.gpu_only_changes = false;
                    }
                    
                    // Any visual change needs display list update (unless GPU-only)
                    result.needs_display_list = true;
                }
                
                result.changed_nodes.entry(node_id).or_default().extend(props);
            }
        };

        // 1. Process focus changes
        if let Some(focus) = focus_changes {
            if let Some(old) = focus.lost_focus {
                let changes = self.restyle_nodes_focus(&[old], false);
                process_changes(changes);
            }
            if let Some(new) = focus.gained_focus {
                let changes = self.restyle_nodes_focus(&[new], true);
                process_changes(changes);
            }
        }

        // 2. Process hover changes
        if let Some(hover) = hover_changes {
            if !hover.left_nodes.is_empty() {
                let changes = self.restyle_nodes_hover(&hover.left_nodes, false);
                process_changes(changes);
            }
            if !hover.entered_nodes.is_empty() {
                let changes = self.restyle_nodes_hover(&hover.entered_nodes, true);
                process_changes(changes);
            }
        }

        // 3. Process active changes
        if let Some(active) = active_changes {
            if !active.deactivated.is_empty() {
                let changes = self.restyle_nodes_active(&active.deactivated, false);
                process_changes(changes);
            }
            if !active.activated.is_empty() {
                let changes = self.restyle_nodes_active(&active.activated, true);
                process_changes(changes);
            }
        }

        // If no changes, reset display_list flag
        if result.changed_nodes.is_empty() {
            result.needs_display_list = false;
            result.gpu_only_changes = false;
        }
        
        // If layout is needed, display list is also needed
        if result.needs_layout {
            result.needs_display_list = true;
            result.gpu_only_changes = false;
        }

        result
    }

    /// Overrides CSS properties for a single node from user code (typically a
    /// callback). Writes into `CssPropertyCache::user_overridden_properties`,
    /// which `get_property_slow` / `get_property_fast` / `get_computed_value`
    /// consult at higher priority than the static CSS cascade — making this
    /// the fast path for animating a handful of properties per frame.
    ///
    /// Passing `CssProperty::Initial` for a property removes any override for
    /// that type, restoring the cascaded value. Returns the set of
    /// `ChangedCssProperty` entries the caller can feed into the incremental
    /// restyle pipeline.
    #[must_use]
    pub fn restyle_user_property(
        &mut self,
        node_id: &NodeId,
        new_properties: &[CssProperty],
    ) -> RestyleNodes {
        let mut map = BTreeMap::default();

        if new_properties.is_empty() {
            return map;
        }

        let node_count = self.node_data.as_ref().len();
        if node_id.index() >= node_count {
            return map;
        }

        let node_data = self.node_data.as_container();
        let node_data = &node_data[*node_id];

        let node_states = &self.styled_nodes.as_container();
        let old_node_state = &node_states[*node_id].styled_node_state;

        let changes: Vec<ChangedCssProperty> = {
            let css_property_cache = self.get_css_property_cache();

            new_properties
                .iter()
                .filter_map(|new_prop| {
                    let old_prop = css_property_cache.get_property_slow(
                        node_data,
                        node_id,
                        old_node_state,
                        &new_prop.get_type(),
                    );

                    let old_prop = old_prop.map_or_else(|| CssProperty::auto(new_prop.get_type()), Clone::clone);

                    if old_prop == *new_prop {
                        None
                    } else {
                        Some(ChangedCssProperty {
                            previous_state: *old_node_state,
                            previous_prop: old_prop,
                            // overriding a user property does not change the state
                            current_state: *old_node_state,
                            current_prop: new_prop.clone(),
                        })
                    }
                })
                .collect()
        };

        let css_property_cache_mut = self.get_css_property_cache_mut();

        // user_overridden_properties is built lazily (empty after StyledDom
        // construction). Grow to cover this node_id before indexing so the
        // override path works on any DOM, not just ones that already have
        // overrides from a prior mutation.
        if css_property_cache_mut.user_overridden_properties.len() < node_count {
            css_property_cache_mut
                .user_overridden_properties
                .resize(node_count, Vec::new());
        }

        for new_prop in new_properties {
            let prop_type = new_prop.get_type();
            let vec = &mut css_property_cache_mut
                .user_overridden_properties[node_id.index()];
            if new_prop.is_initial() {
                // CssProperty::Initial = remove overridden property
                if let Ok(idx) = vec.binary_search_by_key(&prop_type, |(k, _)| *k) {
                    vec.remove(idx);
                }
            } else {
                match vec.binary_search_by_key(&prop_type, |(k, _)| *k) {
                    Ok(idx) => vec[idx].1 = new_prop.clone(),
                    Err(idx) => vec.insert(idx, (prop_type, new_prop.clone())),
                }
            }
        }

        if !changes.is_empty() {
            map.insert(*node_id, changes);
        }

        map
    }

    /// Returns a HTML-formatted version of the DOM for easier debugging.
    ///
    /// For example, a DOM with a parent div containing a child div would return:
    ///
    /// ```xml,no_run,ignore
    /// <div id="hello">
    ///      <div id="test" />
    /// </div>
    /// ```
    #[must_use] pub fn get_html_string(&self, custom_head: &str, custom_body: &str, test_mode: bool) -> String {
        let css_property_cache = self.get_css_property_cache();

        let mut output = String::new();

        // After which nodes should a close tag be printed?
        let mut should_print_close_tag_after_node: BTreeMap<NodeId, Vec<(NodeId, usize)>> = BTreeMap::new();

        let should_print_close_tag_debug = self
            .non_leaf_nodes
            .iter()
            .filter_map(|p| {
                let parent_node_id = p.node_id.into_crate_internal()?;
                let mut total_last_child = None;
                recursive_get_last_child(
                    parent_node_id,
                    self.node_hierarchy.as_ref(),
                    &mut total_last_child,
                );
                let total_last_child = total_last_child?;
                Some((parent_node_id, (total_last_child, p.depth)))
            })
            .collect::<BTreeMap<_, _>>();

        for (parent_id, (last_child, parent_depth)) in should_print_close_tag_debug {
            should_print_close_tag_after_node
                .entry(last_child)
                .or_default()
                .push((parent_id, parent_depth));
        }

        let mut all_node_depths = self
            .non_leaf_nodes
            .iter()
            .filter_map(|p| {
                let parent_node_id = p.node_id.into_crate_internal()?;
                Some((parent_node_id, p.depth))
            })
            .collect::<BTreeMap<_, _>>();

        for (parent_node_id, parent_depth) in self
            .non_leaf_nodes
            .iter()
            .filter_map(|p| Some((p.node_id.into_crate_internal()?, p.depth)))
        {
            for child_id in parent_node_id.az_children(&self.node_hierarchy.as_container()) {
                all_node_depths.insert(child_id, parent_depth + 1);
            }
        }

        for node_id in self.node_hierarchy.as_container().linear_iter() {
            // A single-node DOM (or any node not reached as a non-leaf parent or
            // one of their children, e.g. a lone root) has no entry here; treat
            // its depth as 0 instead of panic-indexing the map.
            let depth = all_node_depths.get(&node_id).copied().unwrap_or(0);

            let node_data = &self.node_data.as_container()[node_id];
            let node_state = &self.styled_nodes.as_container()[node_id].styled_node_state;
            let tabs = String::from("    ").repeat(depth);

            output.push_str("\r\n");
            output.push_str(&tabs);
            output.push_str(&node_data.debug_print_start(css_property_cache, &node_id, node_state));

            if let Some(content) = node_data.get_node_type().format().as_ref() {
                output.push_str(content);
            }

            let node_has_children = self.node_hierarchy.as_container()[node_id]
                .first_child_id(node_id)
                .is_some();
            if !node_has_children {
                let node_data = &self.node_data.as_container()[node_id];
                output.push_str(&node_data.debug_print_end());
            }

            if let Some(close_tag_vec) = should_print_close_tag_after_node.get(&node_id) {
                let mut close_tag_vec = close_tag_vec.clone();
                close_tag_vec.sort_by(|a, b| b.1.cmp(&a.1)); // sort by depth descending
                for (close_tag_parent_id, close_tag_depth) in close_tag_vec {
                    let node_data = &self.node_data.as_container()[close_tag_parent_id];
                    let tabs = String::from("    ").repeat(close_tag_depth);
                    output.push_str("\r\n");
                    output.push_str(&tabs);
                    output.push_str(&node_data.debug_print_end());
                }
            }
        }

        if test_mode {
            output
        } else {
            format!(
                "
                <html>
                    <head>
                    <style>* {{ margin:0px; padding:0px; }}</style>
                    {custom_head}
                    </head>
                {output}
                {custom_body}
                </html>
            "
            )
        }
    }

    /// Returns nodes grouped by their rendering order (respects z-index and position).
    #[must_use] pub fn get_rects_in_rendering_order(&self) -> ContentGroup {
        Self::determine_rendering_order(
            self.non_leaf_nodes.as_ref(),
            &self.node_hierarchy.as_container(),
            &self.styled_nodes.as_container(),
            &self.node_data.as_container(),
            self.get_css_property_cache(),
        )
    }

    /// Returns the rendering order of the items (the rendering
    /// order doesn't have to be the original order)
    fn determine_rendering_order(
        non_leaf_nodes: &[ParentWithNodeDepth],
        node_hierarchy: &NodeDataContainerRef<'_, NodeHierarchyItem>,
        styled_nodes: &NodeDataContainerRef<'_, StyledNode>,
        node_data_container: &NodeDataContainerRef<'_, NodeData>,
        css_property_cache: &CssPropertyCache,
    ) -> ContentGroup {
        let children_sorted = non_leaf_nodes
            .iter()
            .filter_map(|parent| {
                Some((
                    parent.node_id,
                    sort_children_by_position(
                        parent.node_id.into_crate_internal()?,
                        node_hierarchy,
                        styled_nodes,
                        node_data_container,
                        css_property_cache,
                    ),
                ))
            })
            .collect::<Vec<_>>();

        let children_sorted: BTreeMap<NodeHierarchyItemId, Vec<NodeHierarchyItemId>> =
            children_sorted.into_iter().collect();

        let mut root_content_group = ContentGroup {
            root: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
            children: Vec::new().into(),
        };

        fill_content_group_children(&mut root_content_group, &children_sorted);

        root_content_group
    }

    /// Replaces this `StyledDom` with default and returns the old value.
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut new = Self::default();
        core::mem::swap(self, &mut new);
        new
    }

}

/// Same as `Dom`, but arena-based for more efficient memory layout and faster traversal.
#[derive(Debug, PartialEq, PartialOrd, Eq)]
pub struct CompactDom {
    /// The arena containing the hierarchical relationships (parent, child, sibling) of all nodes.
    pub node_hierarchy: NodeHierarchy,
    /// The arena containing the actual data (`NodeData`) for each node.
    pub node_data: NodeDataContainer<NodeData>,
    /// The ID of the root node of the DOM tree.
    pub root: NodeId,
}

impl CompactDom {
    /// Returns the number of nodes in this DOM.
    #[inline]
    #[must_use] pub fn len(&self) -> usize {
        self.node_hierarchy.as_ref().len()
    }

    /// Returns `true` if this DOM has no nodes.
    #[inline]
    #[must_use] pub fn is_empty(&self) -> bool {
        self.node_hierarchy.as_ref().is_empty()
    }
}

impl From<Dom> for CompactDom {
    fn from(dom: Dom) -> Self {
        convert_dom_into_compact_dom(dom)
    }
}

/// Converts a tree-based Dom into an arena-based `CompactDom` for efficient traversal.
#[must_use] pub fn convert_dom_into_compact_dom(mut dom: Dom) -> CompactDom {
    // note: somehow convert this into a non-recursive form later on!
    fn convert_dom_into_compact_dom_internal(
        dom: &mut Dom,
        node_hierarchy: &mut [Node],
        node_data: &mut Vec<NodeData>,
        parent_node_id: NodeId,
        node: Node,
        cur_node_id: &mut usize,
    ) {
        // - parent [0]
        //    - child [1]
        //    - child [2]
        //        - child of child 2 [2]
        //        - child of child 2 [4]
        //    - child [5]
        //    - child [6]
        //        - child of child 4 [7]

        // Write node into the arena here!
        node_hierarchy[parent_node_id.index()] = node;

        // MOVE the node's inline `style` AND its `extra` (NodeDataExt) box instead of relying on
        // copy_special's `self.style.clone()` / `self.extra.clone()`. Both derived Clones lower to
        // indirect-jump jump tables that remill mis-lifts on the web backend: CssProperty's clone
        // comes back with discriminant 0 (drops simple inline CSS) and for COMPLEX values (AzButton's
        // gradient; the NodeDataExt attributes Vec) the mis-lifted clone reads/writes wrong-sized data,
        // which clobbers the adjacent `style` temporary → "memory access out of bounds" later in the
        // cascade (StyledDom::create → restyle's inheritance loop reads the corrupted style). 2026-06-02:
        // copy_special_moving_complex mem::takes BOTH style+extra before copy_special, so copy_special
        // clones an EMPTY style + None extra (no broken clone runs) and restores them after. (Extra was
        // added after the AzButton ids/classes node — which lazily allocates NodeDataExt — OOB'd even
        // with the style-only take.) The Dom is consumed here, so the move is correct.
        let copy = dom.root.copy_special_moving_complex();

        node_data[parent_node_id.index()] = copy;

        *cur_node_id += 1;

        let mut previous_sibling_id = None;
        let children_len = dom.children.len();
        for (child_index, child_dom) in dom.children.as_mut().iter_mut().enumerate() {
            let child_node_id = NodeId::new(*cur_node_id);
            let is_last_child = (child_index + 1) == children_len;
            let child_dom_is_empty = child_dom.children.is_empty();
            let child_node = Node {
                parent: Some(parent_node_id),
                previous_sibling: previous_sibling_id,
                next_sibling: if is_last_child {
                    None
                } else {
                    Some(child_node_id + child_dom.estimated_total_children + 1)
                },
                last_child: if child_dom_is_empty {
                    None
                } else {
                    Some(child_node_id + child_dom.estimated_total_children)
                },
            };
            previous_sibling_id = Some(child_node_id);
            // recurse BEFORE adding the next child
            convert_dom_into_compact_dom_internal(
                child_dom,
                node_hierarchy,
                node_data,
                child_node_id,
                child_node,
                cur_node_id,
            );
        }

        // AUTHORITATIVE last_child. The per-child `last_child` set at construction used
        // `child_node_id + estimated_total_children`, which is the last node of the
        // whole SUBTREE (its deepest descendant), NOT the last DIRECT child — wrong
        // whenever that last child has children of its own. It corrupted `last_child_id()`
        // and, through it, append_child (which spliced onto the wrong node). The loop
        // above already tracked `previous_sibling_id`, which now holds the real last
        // direct child (None if there were none), so overwrite with it. This runs for
        // every node including the root, so it also corrects the root's own computation.
        node_hierarchy[parent_node_id.index()].last_child = previous_sibling_id;
    }

    // Pre-allocate all nodes (+ 1 root node)
    let sum_nodes = dom.fixup_children_estimated();

    let mut node_hierarchy = vec![Node::ROOT; sum_nodes + 1];
    let mut node_data = vec![NodeData::create_div(); sum_nodes + 1];
    let mut cur_node_id = 0;

    let root_node_id = NodeId::ZERO;
    let root_node = Node {
        parent: None,
        previous_sibling: None,
        next_sibling: None,
        last_child: if dom.children.is_empty() {
            None
        } else {
            Some(root_node_id + dom.estimated_total_children)
        },
    };

    convert_dom_into_compact_dom_internal(
        &mut dom,
        &mut node_hierarchy,
        &mut node_data,
        root_node_id,
        root_node,
        &mut cur_node_id,
    );

    CompactDom {
        node_hierarchy: NodeHierarchy {
            internal: node_hierarchy,
        },
        node_data: NodeDataContainer {
            internal: node_data,
        },
        root: root_node_id,
    }
}

/// #47: scope every node's inline css to its own subtree. Walks the tree in the
/// SAME pre-order `convert_dom_into_compact_dom` uses to assign flat `NodeIds`, so the
/// `[flat_id, flat_id + estimated_total_children]` range pushed onto each rule (via
/// `CssPath::push_front_scope`) matches the ids the cascade will later see. After
/// this, a node's `with_css`/`set_css` rules can only match nodes inside its subtree
/// — they can no longer leak to the whole tree. `fixup_children_estimated()` must
/// have run first so `estimated_total_children` is populated/exact.
fn scope_inline_css(dom: &mut Dom, next_id: &mut usize) {
    let start = *next_id;
    let end = start + dom.estimated_total_children;
    for css in dom.css.as_mut().iter_mut() {
        for rule in css.rules.as_mut().iter_mut() {
            // push_front_scope picks node-only ([start,start]) for bare `*` rules
            // (with_css inline decls) and subtree ([start,end]) for rules with a real
            // selector (add_component_css), so descendant selectors match the subtree.
            rule.path.push_front_scope(start, end);
        }
    }
    *next_id += 1;
    for child in dom.children.as_mut().iter_mut() {
        scope_inline_css(child, next_id);
    }
}

/// Recursively collect all CSS objects from a Dom tree (depth-first).
/// Inner (deeper) CSS objects come first, outer (shallower) CSS objects come last.
/// This means outer CSS has higher cascade priority when applied in order.
fn collect_css_from_dom(dom: &Dom, out: &mut Vec<Css>) {
    // First, recurse into children (inner CSS = lower priority)
    for child in &dom.children {
        collect_css_from_dom(child, out);
    }
    // Then, add this node's CSS objects (outer CSS = higher priority)
    for css in &dom.css {
        out.push(css.clone());
    }
}

/// Recursively strip CSS from all Dom nodes (sets css to empty vec).
/// Called after collecting CSS so the `CompactDom` doesn't carry CSS data.
fn strip_css_from_dom(dom: &mut Dom) {
    dom.css = Vec::new().into();
    for child in dom.children.as_mut().iter_mut() {
        strip_css_from_dom(child);
    }
}

fn fill_content_group_children(
    group: &mut ContentGroup,
    children_sorted: &BTreeMap<NodeHierarchyItemId, Vec<NodeHierarchyItemId>>,
) {
    if let Some(c) = children_sorted.get(&group.root) {
        // returns None for leaf nodes
        group.children = c
            .iter()
            .map(|child| ContentGroup {
                root: *child,
                children: Vec::new().into(),
            })
            .collect::<Vec<ContentGroup>>()
            .into();

        for c in group.children.as_mut() {
            fill_content_group_children(c, children_sorted);
        }
    }
}

fn sort_children_by_position(
    parent: NodeId,
    node_hierarchy: &NodeDataContainerRef<'_, NodeHierarchyItem>,
    rectangles: &NodeDataContainerRef<'_, StyledNode>,
    node_data_container: &NodeDataContainerRef<'_, NodeData>,
    css_property_cache: &CssPropertyCache,
) -> Vec<NodeHierarchyItemId> {
    use azul_css::props::layout::LayoutPosition::Absolute;

    let children_positions = parent
        .az_children(node_hierarchy)
        .map(|nid| {
            let position = css_property_cache
                .get_position(
                    &node_data_container[nid],
                    &nid,
                    &rectangles[nid].styled_node_state,
                )
                .and_then(|p| (*p).get_property_or_default())
                .unwrap_or_default();
            let id = NodeHierarchyItemId::from_crate_internal(Some(nid));
            (id, position)
        })
        .collect::<Vec<_>>();

    let mut not_absolute_children = children_positions
        .iter()
        .filter_map(|(node_id, position)| {
            if *position == Absolute {
                None
            } else {
                Some(*node_id)
            }
        })
        .collect::<Vec<_>>();

    let mut absolute_children = children_positions
        .iter()
        .filter_map(|(node_id, position)| {
            if *position == Absolute {
                Some(*node_id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    // Append the position:absolute children after the regular children
    not_absolute_children.append(&mut absolute_children);
    not_absolute_children
}

// calls get_last_child() recursively until the last child of the last child of the ... has been
// found
fn recursive_get_last_child(
    node_id: NodeId,
    node_hierarchy: &[NodeHierarchyItem],
    target: &mut Option<NodeId>,
) {
    match node_hierarchy[node_id.index()].last_child_id() {
        None => (),
        Some(s) => {
            *target = Some(s);
            recursive_get_last_child(s, node_hierarchy, target);
        }
    }
}

// ============================================================================
// DOM TRAVERSAL FOR MULTI-NODE SELECTION
// ============================================================================

/// Determine if `node_a` comes before `node_b` in document order.
///
/// Document order is defined as pre-order depth-first traversal order.
/// This is equivalent to the order nodes appear in HTML source.
///
/// ## Algorithm
/// 1. Find the path from root to each node
/// 2. Find the Lowest Common Ancestor (LCA)
/// 3. At the divergence point, the child that appears first in sibling order comes first
#[must_use] pub fn is_before_in_document_order(
    hierarchy: &NodeHierarchyItemVec,
    node_a: NodeId,
    node_b: NodeId,
) -> bool {
    if node_a == node_b {
        return false;
    }
    
    let hierarchy = hierarchy.as_container();
    
    // Get paths from root to each node (stored as root-first order)
    let path_a = get_path_to_root(&hierarchy, node_a);
    let path_b = get_path_to_root(&hierarchy, node_b);
    
    // Find divergence point (last common ancestor)
    let min_len = path_a.len().min(path_b.len());
    
    for i in 0..min_len {
        if path_a[i] != path_b[i] {
            // Found divergence - check which sibling comes first
            let child_towards_a = path_a[i];
            let child_towards_b = path_b[i];
            
            // A smaller NodeId index means it was created earlier in DOM construction,
            // which means it comes first in document order for siblings
            return child_towards_a.index() < child_towards_b.index();
        }
    }
    
    // One path is a prefix of the other - the shorter path (ancestor) comes first
    path_a.len() < path_b.len()
}

/// Get the path from root to a node, returned in root-first order.
fn get_path_to_root(
    hierarchy: &NodeDataContainerRef<'_, NodeHierarchyItem>,
    node: NodeId,
) -> Vec<NodeId> {
    let mut path = Vec::new();
    let mut current = Some(node);
    
    while let Some(node_id) = current {
        path.push(node_id);
        current = hierarchy.get(node_id).and_then(NodeHierarchyItem::parent_id);
    }
    
    // Reverse to get root-first order
    path.reverse();
    path
}

/// Collect all nodes between start and end (inclusive) in document order.
///
/// This performs a pre-order depth-first traversal starting from the root,
/// collecting nodes once we've seen `start` and stopping at `end`.
///
/// ## Parameters
/// * `hierarchy` - The node hierarchy
/// * `start_node` - First node in document order
/// * `end_node` - Last node in document order
///
/// ## Returns
/// Vector of `NodeIds` in document order, from start to end (inclusive)
#[must_use] pub fn collect_nodes_in_document_order(
    hierarchy: &NodeHierarchyItemVec,
    start_node: NodeId,
    end_node: NodeId,
) -> Vec<NodeId> {
    if start_node == end_node {
        return vec![start_node];
    }
    
    let hierarchy_container = hierarchy.as_container();
    let hierarchy_slice = hierarchy.as_ref();
    
    let mut result = Vec::new();
    let mut in_range = false;
    
    // Pre-order DFS using a stack
    // We need to traverse in document order, which is pre-order DFS
    let mut stack: Vec<NodeId> = vec![NodeId::ZERO]; // Start from root
    
    while let Some(current) = stack.pop() {
        // Check if we've entered the range
        if current == start_node {
            in_range = true;
        }
        
        // Collect if in range
        if in_range {
            result.push(current);
        }
        
        // Check if we've exited the range
        if current == end_node {
            break;
        }
        
        // Push children in reverse order so they pop in correct order
        // (first child should be processed first)
        if let Some(item) = hierarchy_container.get(current) {
            // Get first child
            if let Some(first_child) = item.first_child_id(current) {
                // Collect all children by following next_sibling
                let mut children = Vec::new();
                let mut child = Some(first_child);
                while let Some(child_id) = child {
                    children.push(child_id);
                    child = hierarchy_container.get(child_id).and_then(NodeHierarchyItem::next_sibling_id);
                }
                // Push in reverse order for correct DFS order
                for child_id in children.into_iter().rev() {
                    stack.push(child_id);
                }
            }
        }
    }
    
    result
}

/// Check if two `StyledDom`s are structurally equivalent for layout purposes.
///
/// Returns `true` if the DOMs have the same structure, node types, classes,
/// IDs, inline styles, and callback event registrations — meaning the
/// layout output would be identical.
///
/// Image callback nodes are compared by function pointer and `RefAny` type ID
/// rather than heap pointer, since each `layout()` call creates new `ImageRef`
/// allocations even when the callback is the same.
///
/// This is used to short-circuit the expensive layout pipeline when the DOM
/// hasn't actually changed (e.g., an animation timer fires but only the GL
/// texture content changed, not the DOM structure).
#[must_use] pub fn is_layout_equivalent(old: &StyledDom, new: &StyledDom) -> bool {
    use crate::dom::NodeType;
    use crate::resources::DecodedImage;

    // Quick check: node count must match
    let old_nodes = old.node_data.as_ref();
    let new_nodes = new.node_data.as_ref();
    if old_nodes.len() != new_nodes.len() {
        return false;
    }

    // Check hierarchy (parent/child/sibling structure)
    let old_hier = old.node_hierarchy.as_ref();
    let new_hier = new.node_hierarchy.as_ref();
    if old_hier.len() != new_hier.len() {
        return false;
    }
    if old_hier != new_hier {
        return false;
    }

    // Per-node comparison
    for (old_node, new_node) in old_nodes.iter().zip(new_nodes.iter()) {

        // Compare node type discriminant
        if core::mem::discriminant(&old_node.node_type)
            != core::mem::discriminant(&new_node.node_type)
        {
            return false;
        }

        // Compare node type content (with special handling for image callbacks)
        match (&old_node.node_type, &new_node.node_type) {
            (NodeType::Image(old_img), NodeType::Image(new_img)) => {
                match (old_img.get_data(), new_img.get_data()) {
                    (DecodedImage::Callback(old_cb), DecodedImage::Callback(new_cb)) => {
                        // Compare callback function pointer (stable across frames)
                        if old_cb.callback.cb != new_cb.callback.cb {
                            return false;
                        }
                        // Compare RefAny type ID (not instance pointer)
                        if old_cb.refany.get_type_id() != new_cb.refany.get_type_id() {
                            return false;
                        }
                    }
                    _ => {
                        // Raw images / GL textures: compare by pointer identity
                        if old_img != new_img {
                            return false;
                        }
                    }
                }
            }
            _ => {
                if old_node.node_type != new_node.node_type {
                    return false;
                }
            }
        }

        // Compare IDs and classes (now stored in attributes as AttributeType::Id/Class)
        {
            use crate::dom::AttributeType;
            let old_ids_classes: Vec<_> = old_node.attributes().as_ref().iter()
                .filter(|a| matches!(a, AttributeType::Id(_) | AttributeType::Class(_)))
                .collect();
            let new_ids_classes: Vec<_> = new_node.attributes().as_ref().iter()
                .filter(|a| matches!(a, AttributeType::Id(_) | AttributeType::Class(_)))
                .collect();
            if old_ids_classes != new_ids_classes {
                return false;
            }
        }

        // Compare inline CSS (direct layout input)
        if old_node.style != new_node.style {
            return false;
        }

        // Compare callback event types (affects hit-test tags)
        // We compare only event types, not function pointers or data
        let old_cbs = old_node.callbacks.as_ref();
        let new_cbs = new_node.callbacks.as_ref();
        if old_cbs.len() != new_cbs.len() {
            return false;
        }
        for (old_cb, new_cb) in old_cbs.iter().zip(new_cbs.iter()) {
            if old_cb.event != new_cb.event {
                return false;
            }
        }

        // Compare attributes (some affect layout, e.g. colspan)
        if old_node.attributes().as_ref() != new_node.attributes().as_ref() {
            return false;
        }
    }

    // Compare styled node states (hover/focus/active flags affect CSS resolution)
    let old_styled = old.styled_nodes.as_ref();
    let new_styled = new.styled_nodes.as_ref();
    if old_styled.len() != new_styled.len() {
        return false;
    }
    if old_styled != new_styled {
        return false;
    }

    true
}

#[cfg(test)]
mod audit_tests {
    use super::*;
    use azul_css::props::basic::StyleFontFamily;

    fn fam(name: &str) -> StyleFontFamily {
        StyleFontFamily::System(name.to_string().into())
    }

    #[test]
    fn style_font_families_hash_is_length_sensitive() {
        // The length prefix guarantees that lists of different lengths cannot
        // collide, and that hashing is deterministic.
        let a = StyleFontFamiliesHash::new(&[fam("Arial")]);
        let a2 = StyleFontFamiliesHash::new(&[fam("Arial")]);
        assert_eq!(a, a2, "hash must be deterministic");

        let two = StyleFontFamiliesHash::new(&[fam("Arial"), fam("Helvetica")]);
        assert_ne!(a, two, "different-length family lists must not collide");

        let empty = StyleFontFamiliesHash::new(&[]);
        assert_ne!(empty, a);
        assert_ne!(empty, two);

        // Order still matters.
        let rev = StyleFontFamiliesHash::new(&[fam("Helvetica"), fam("Arial")]);
        assert_ne!(two, rev);
    }
}

#[cfg(test)]
#[allow(clippy::too_many_lines)]
mod autotest_generated {
    use azul_css::{
        dynamic_selector::PseudoStateFlags,
        props::basic::StyleFontFamily,
    };

    use super::*;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    /// Builds a `NodeHierarchyItem` directly from the RAW (1-based) encoding:
    /// `0` = none, `n` = `NodeId(n - 1)`.
    const fn raw_item(parent: usize, prev: usize, next: usize, last: usize) -> NodeHierarchyItem {
        NodeHierarchyItem {
            parent,
            previous_sibling: prev,
            next_sibling: next,
            last_child: last,
        }
    }

    /// `<body>` with `n` leaf `<div>` children, cascaded against an empty stylesheet.
    /// Node ids are `0 = body`, `1..=n` = the children.
    fn flat_body(n: usize) -> StyledDom {
        let children: Vec<Dom> = (0..n).map(|_| Dom::create_div()).collect();
        let mut dom = Dom::create_body().with_children(children.into());
        StyledDom::create(&mut dom, Css::empty())
    }

    /// `<body> > <div> > <div>` — the last direct child of the root is itself a parent.
    fn nested_body() -> StyledDom {
        let mut dom = Dom::create_body().with_children(
            vec![Dom::create_div().with_children(vec![Dom::create_div()].into())].into(),
        );
        StyledDom::create(&mut dom, Css::empty())
    }

    fn parse_css(s: &str) -> Css {
        azul_css::parser2::new_from_str(s).0
    }

    fn family(name: &str) -> StyleFontFamily {
        StyleFontFamily::System(name.to_string().into())
    }

    const fn pseudo_flags(all: bool) -> PseudoStateFlags {
        PseudoStateFlags {
            hover: all,
            active: all,
            focused: all,
            disabled: all,
            checked: all,
            focus_within: all,
            visited: all,
            backdrop: all,
            dragging: all,
            drag_over: all,
        }
    }

    fn empty_menu() -> Menu {
        let items: Vec<crate::menu::MenuItem> = Vec::new();
        Menu::create(items.into())
    }

    // ---------------------------------------------------------------------
    // RestyleResult (predicate + merge)
    // ---------------------------------------------------------------------

    #[test]
    fn restyle_result_default_reports_no_changes() {
        let r = RestyleResult::default();
        assert!(!r.has_changes());
        assert!(!r.needs_layout);
        assert!(!r.needs_display_list);
        assert!(!r.gpu_only_changes);
        assert_eq!(r.max_relayout_scope, RelayoutScope::None);
    }

    #[test]
    fn restyle_result_has_changes_keys_off_node_map_not_property_count() {
        // A node entry with an EMPTY change list still counts as "changed":
        // has_changes() only looks at the node map, never at the inner Vec.
        let mut r = RestyleResult::default();
        r.changed_nodes.insert(NodeId::ZERO, Vec::new());
        assert!(r.has_changes());

        r.changed_nodes.clear();
        assert!(!r.has_changes());
    }

    #[test]
    fn restyle_result_merge_ors_layout_flags_and_ands_gpu_only() {
        let mut a = RestyleResult {
            needs_layout: false,
            needs_display_list: false,
            gpu_only_changes: true,
            ..RestyleResult::default()
        };
        let b = RestyleResult {
            needs_layout: true,
            needs_display_list: true,
            gpu_only_changes: true,
            ..RestyleResult::default()
        };
        a.merge(b);
        assert!(a.needs_layout, "needs_layout is OR-ed");
        assert!(a.needs_display_list, "needs_display_list is OR-ed");
        assert!(a.gpu_only_changes, "true && true stays true");

        // ...and a single non-GPU-only participant clears the flag.
        let mut c = RestyleResult {
            gpu_only_changes: true,
            ..RestyleResult::default()
        };
        c.merge(RestyleResult {
            gpu_only_changes: false,
            ..RestyleResult::default()
        });
        assert!(!c.gpu_only_changes, "gpu_only_changes is AND-ed");
    }

    #[test]
    fn restyle_result_merge_keeps_the_most_expensive_scope() {
        let mut low = RestyleResult {
            max_relayout_scope: RelayoutScope::None,
            ..RestyleResult::default()
        };
        low.merge(RestyleResult {
            max_relayout_scope: RelayoutScope::Full,
            ..RestyleResult::default()
        });
        assert_eq!(low.max_relayout_scope, RelayoutScope::Full);

        // ...and merging a cheaper scope must NOT downgrade it.
        let mut high = RestyleResult {
            max_relayout_scope: RelayoutScope::Full,
            ..RestyleResult::default()
        };
        high.merge(RestyleResult {
            max_relayout_scope: RelayoutScope::IfcOnly,
            ..RestyleResult::default()
        });
        assert_eq!(high.max_relayout_scope, RelayoutScope::Full);
    }

    #[test]
    fn restyle_result_merge_of_default_is_not_the_identity_for_gpu_only() {
        // `RestyleResult::default()` has gpu_only_changes == false, and merge()
        // AND-s that flag — so merging an EMPTY result still clears it. Pinned
        // here because it is a genuine footgun for callers that merge in a loop.
        let mut a = RestyleResult {
            gpu_only_changes: true,
            ..RestyleResult::default()
        };
        a.merge(RestyleResult::default());
        assert!(!a.gpu_only_changes);
        assert!(!a.has_changes());
    }

    #[test]
    fn restyle_result_merge_concatenates_changes_for_the_same_node() {
        let prop = |t| ChangedCssProperty {
            previous_state: StyledNodeState::new(),
            previous_prop: CssProperty::auto(t),
            current_state: StyledNodeState::new(),
            current_prop: CssProperty::initial(t),
        };

        let mut a = RestyleResult::default();
        a.changed_nodes
            .insert(NodeId::ZERO, vec![prop(CssPropertyType::Width)]);

        let mut b = RestyleResult::default();
        b.changed_nodes
            .insert(NodeId::ZERO, vec![prop(CssPropertyType::Height)]);
        b.changed_nodes
            .insert(NodeId::new(1), vec![prop(CssPropertyType::Opacity)]);

        a.merge(b);

        assert_eq!(a.changed_nodes.len(), 2);
        assert_eq!(
            a.changed_nodes[&NodeId::ZERO].len(),
            2,
            "changes for the same node are appended, not replaced"
        );
        assert_eq!(a.changed_nodes[&NodeId::new(1)].len(), 1);
        assert!(a.has_changes());
    }

    // ---------------------------------------------------------------------
    // StyledNodeState (constructor + predicates)
    // ---------------------------------------------------------------------

    #[test]
    fn styled_node_state_new_is_all_false_and_normal() {
        let s = StyledNodeState::new();
        assert!(s.is_normal());
        assert!(!s.hover);
        assert!(!s.active);
        assert!(!s.focused);
        assert!(!s.disabled);
        assert!(!s.checked);
        assert!(!s.focus_within);
        assert!(!s.visited);
        assert!(!s.backdrop);
        assert!(!s.dragging);
        assert!(!s.drag_over);
        assert_eq!(s, StyledNodeState::default());
    }

    #[test]
    fn styled_node_state_has_state_zero_is_always_true() {
        // 0 == "Normal", which is active regardless of the other flags.
        assert!(StyledNodeState::new().has_state(0));
        assert!(StyledNodeState::from_pseudo_state_flags(&pseudo_flags(true)).has_state(0));
    }

    #[test]
    fn styled_node_state_has_state_maps_every_index_exactly_once() {
        // Each setter must light up exactly one state index in 1..=10.
        let setters: [(u8, fn(&mut StyledNodeState)); 10] = [
            (1, |s| s.hover = true),
            (2, |s| s.active = true),
            (3, |s| s.focused = true),
            (4, |s| s.disabled = true),
            (5, |s| s.checked = true),
            (6, |s| s.focus_within = true),
            (7, |s| s.visited = true),
            (8, |s| s.backdrop = true),
            (9, |s| s.dragging = true),
            (10, |s| s.drag_over = true),
        ];

        for (expected_idx, set) in setters {
            let mut s = StyledNodeState::new();
            set(&mut s);
            assert!(!s.is_normal(), "state {expected_idx} must not be 'normal'");
            for idx in 1..=10u8 {
                assert_eq!(
                    s.has_state(idx),
                    idx == expected_idx,
                    "state index {idx} misreported for setter {expected_idx}"
                );
            }
        }
    }

    #[test]
    fn styled_node_state_has_state_is_false_for_every_out_of_range_u8() {
        let all_on = StyledNodeState::from_pseudo_state_flags(&pseudo_flags(true));
        for idx in 11..=u8::MAX {
            assert!(!StyledNodeState::new().has_state(idx));
            assert!(
                !all_on.has_state(idx),
                "unknown state index {idx} must be inactive even when every flag is set"
            );
        }
    }

    #[test]
    fn styled_node_state_from_pseudo_state_flags_roundtrips_every_field() {
        let all_on = StyledNodeState::from_pseudo_state_flags(&pseudo_flags(true));
        assert!(!all_on.is_normal());
        for idx in 0..=10u8 {
            assert!(all_on.has_state(idx), "state {idx} should be active");
        }

        let all_off = StyledNodeState::from_pseudo_state_flags(&pseudo_flags(false));
        assert!(all_off.is_normal());
        assert_eq!(all_off, StyledNodeState::new());
    }

    #[test]
    fn styled_node_state_debug_lists_active_states_and_normal_when_empty() {
        assert_eq!(format!("{:?}", StyledNodeState::new()), "[\"normal\"]");

        let mut s = StyledNodeState::new();
        s.hover = true;
        s.drag_over = true;
        let dbg = format!("{s:?}");
        assert!(dbg.contains("hover"), "{dbg}");
        assert!(dbg.contains("drag_over"), "{dbg}");
        assert!(!dbg.contains("normal"), "{dbg}");
    }

    // ---------------------------------------------------------------------
    // StyledNodeVec containers
    // ---------------------------------------------------------------------

    #[test]
    fn styled_node_vec_empty_container_is_empty_and_get_returns_none() {
        let v: StyledNodeVec = Vec::new().into();
        let c = v.as_container();
        assert_eq!(c.len(), 0);
        assert!(c.is_empty());
        assert!(c.get(NodeId::ZERO).is_none());
        assert!(c.get(NodeId::new(usize::MAX)).is_none());
    }

    #[test]
    fn styled_node_vec_container_mut_writes_are_visible_through_container() {
        let mut v: StyledNodeVec = vec![StyledNode::default(), StyledNode::default()].into();
        {
            let mut c = v.as_container_mut();
            c[NodeId::new(1)].styled_node_state.hover = true;
        }
        let c = v.as_container();
        assert_eq!(c.len(), 2);
        assert!(!c[NodeId::ZERO].styled_node_state.hover);
        assert!(c[NodeId::new(1)].styled_node_state.hover);
        assert!(c.get(NodeId::new(2)).is_none());
    }

    // ---------------------------------------------------------------------
    // Font family hashes
    // ---------------------------------------------------------------------

    #[test]
    fn style_font_family_hash_is_deterministic_and_input_sensitive() {
        assert_eq!(
            StyleFontFamilyHash::new(&family("Arial")),
            StyleFontFamilyHash::new(&family("Arial"))
        );
        assert_ne!(
            StyleFontFamilyHash::new(&family("Arial")),
            StyleFontFamilyHash::new(&family("Ariaĺ"))
        );
        // Same string, different variant → different cache key.
        assert_ne!(
            StyleFontFamilyHash::new(&StyleFontFamily::System("x".to_string().into())),
            StyleFontFamilyHash::new(&StyleFontFamily::File("x".to_string().into()))
        );
    }

    #[test]
    fn style_font_family_hash_handles_empty_unicode_and_huge_names() {
        let empty = family("");
        let unicode = family("🦀 ノート ﷽ عربى");
        let huge = family(&"A".repeat(100_000));

        // No panic, and each distinct input is stable across calls.
        assert_eq!(StyleFontFamilyHash::new(&empty), StyleFontFamilyHash::new(&empty));
        assert_eq!(
            StyleFontFamilyHash::new(&unicode),
            StyleFontFamilyHash::new(&unicode)
        );
        assert_eq!(StyleFontFamilyHash::new(&huge), StyleFontFamilyHash::new(&huge));
        assert_ne!(StyleFontFamilyHash::new(&empty), StyleFontFamilyHash::new(&unicode));
        assert_ne!(StyleFontFamilyHash::new(&empty), StyleFontFamilyHash::new(&huge));
    }

    #[test]
    fn style_font_families_hash_empty_slice_is_stable_and_distinct() {
        let empty = StyleFontFamiliesHash::new(&[]);
        assert_eq!(empty, StyleFontFamiliesHash::new(&[]));
        assert_ne!(empty, StyleFontFamiliesHash::new(&[family("")]));
    }

    #[test]
    fn style_font_families_hash_scales_to_large_lists_and_is_length_sensitive() {
        let big: Vec<StyleFontFamily> = (0..1000).map(|i| family(&format!("font-{i}"))).collect();
        let one_shorter = &big[..999];

        assert_eq!(
            StyleFontFamiliesHash::new(&big),
            StyleFontFamiliesHash::new(&big),
            "hashing 1000 families must be deterministic"
        );
        assert_ne!(
            StyleFontFamiliesHash::new(&big),
            StyleFontFamiliesHash::new(one_shorter),
            "the length prefix must separate [0..1000) from [0..999)"
        );
    }

    // ---------------------------------------------------------------------
    // NodeHierarchyItemId: 1-based encode/decode round-trip
    // ---------------------------------------------------------------------

    #[test]
    fn node_hierarchy_item_id_none_is_zero() {
        assert_eq!(NodeHierarchyItemId::NONE.into_raw(), 0);
        assert_eq!(NodeHierarchyItemId::NONE.into_crate_internal(), None);
        assert_eq!(NodeHierarchyItemId::from_crate_internal(None).into_raw(), 0);
        assert_eq!(NodeHierarchyItemId::from_raw(0).into_crate_internal(), None);
        assert_eq!(NodeHierarchyItemId::from_crate_internal(None), NodeHierarchyItemId::NONE);
    }

    #[test]
    fn node_hierarchy_item_id_encode_decode_roundtrip_at_boundaries() {
        // usize::MAX - 1 is the largest index that survives the +1 encoding.
        for idx in [0usize, 1, 2, 1023, usize::MAX / 2, usize::MAX - 1] {
            let id = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx)));
            assert_eq!(id.into_raw(), idx + 1, "1-based encoding for {idx}");
            assert_eq!(
                id.into_crate_internal(),
                Some(NodeId::new(idx)),
                "decode(encode(x)) == x for {idx}"
            );
        }
    }

    #[test]
    fn node_hierarchy_item_id_raw_roundtrip_is_identity_even_at_usize_max() {
        for raw in [0usize, 1, 2, 7, u32::MAX as usize, usize::MAX] {
            let decoded = NodeHierarchyItemId::from_raw(raw).into_crate_internal();
            let reencoded = NodeHierarchyItemId::from_crate_internal(decoded).into_raw();
            assert_eq!(reencoded, raw, "encode(decode(raw)) must be identity for {raw}");
        }
    }

    #[test]
    fn node_hierarchy_item_id_from_raw_decodes_one_based() {
        assert_eq!(
            NodeHierarchyItemId::from_raw(1).into_crate_internal(),
            Some(NodeId::ZERO),
            "raw 1 is NodeId(0), NOT NodeId(1)"
        );
        assert_eq!(
            NodeHierarchyItemId::from_raw(usize::MAX).into_crate_internal(),
            Some(NodeId::new(usize::MAX - 1))
        );
    }

    #[test]
    fn node_hierarchy_item_id_debug_and_display_agree() {
        let none = NodeHierarchyItemId::NONE;
        assert_eq!(format!("{none:?}"), "None");
        assert_eq!(format!("{none}"), format!("{none:?}"));

        let some = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(5)));
        assert_eq!(format!("{some:?}"), "Some(NodeId(5))");
        assert_eq!(format!("{some}"), format!("{some:?}"));

        // Extreme value: must not panic and must stay non-empty.
        let max = NodeHierarchyItemId::from_raw(usize::MAX);
        assert!(!format!("{max:?}").is_empty());
    }

    #[test]
    fn node_hierarchy_item_id_ordering_follows_raw_value() {
        let a = NodeHierarchyItemId::from_raw(0);
        let b = NodeHierarchyItemId::from_raw(1);
        let c = NodeHierarchyItemId::from_raw(usize::MAX);
        assert!(a < b);
        assert!(b < c);
        assert_eq!(a, NodeHierarchyItemId::NONE);
    }

    #[test]
    fn node_hierarchy_item_id_from_impls_match_the_explicit_ones() {
        let opt = Some(NodeId::new(41));
        let via_from: NodeHierarchyItemId = opt.into();
        assert_eq!(via_from, NodeHierarchyItemId::from_crate_internal(opt));

        let back: Option<NodeId> = via_from.into();
        assert_eq!(back, opt);

        let none: NodeHierarchyItemId = None.into();
        assert_eq!(none.into_raw(), 0);
    }

    // ---------------------------------------------------------------------
    // NodeHierarchyItem getters
    // ---------------------------------------------------------------------

    #[test]
    fn node_hierarchy_item_zeroed_has_no_links() {
        let z = NodeHierarchyItem::zeroed();
        assert_eq!(z.parent_id(), None);
        assert_eq!(z.previous_sibling_id(), None);
        assert_eq!(z.next_sibling_id(), None);
        assert_eq!(z.last_child_id(), None);
        assert_eq!(z.first_child_id(NodeId::ZERO), None);
        assert_eq!(z.first_child_id(NodeId::new(usize::MAX)), None);
        assert_eq!(z, NodeHierarchyItem::from(Node::ROOT));
    }

    #[test]
    fn node_hierarchy_item_getters_decode_the_one_based_fields() {
        let item = raw_item(1, 2, 3, 4);
        assert_eq!(item.parent_id(), Some(NodeId::new(0)));
        assert_eq!(item.previous_sibling_id(), Some(NodeId::new(1)));
        assert_eq!(item.next_sibling_id(), Some(NodeId::new(2)));
        assert_eq!(item.last_child_id(), Some(NodeId::new(3)));

        // first_child is derived: parent + 1, but only if the node has children.
        assert_eq!(item.first_child_id(NodeId::new(7)), Some(NodeId::new(8)));
    }

    #[test]
    fn node_hierarchy_item_getters_at_usize_max_do_not_overflow() {
        let item = raw_item(usize::MAX, usize::MAX, usize::MAX, usize::MAX);
        assert_eq!(item.parent_id(), Some(NodeId::new(usize::MAX - 1)));
        assert_eq!(item.previous_sibling_id(), Some(NodeId::new(usize::MAX - 1)));
        assert_eq!(item.next_sibling_id(), Some(NodeId::new(usize::MAX - 1)));
        assert_eq!(item.last_child_id(), Some(NodeId::new(usize::MAX - 1)));

        // NodeId's Add is saturating, so `current + 1` clamps instead of wrapping
        // to 0 (which would alias the root node).
        assert_eq!(
            item.first_child_id(NodeId::new(usize::MAX)),
            Some(NodeId::new(usize::MAX)),
            "first_child_id must saturate, never wrap to NodeId(0)"
        );
    }

    #[test]
    fn node_hierarchy_item_from_node_preserves_every_link() {
        let node = Node {
            parent: Some(NodeId::new(3)),
            previous_sibling: None,
            next_sibling: Some(NodeId::new(9)),
            last_child: Some(NodeId::new(12)),
        };
        let item: NodeHierarchyItem = node.into();
        assert_eq!(item.parent_id(), node.parent);
        assert_eq!(item.previous_sibling_id(), node.previous_sibling);
        assert_eq!(item.next_sibling_id(), node.next_sibling);
        assert_eq!(item.last_child_id(), node.last_child);
    }

    // ---------------------------------------------------------------------
    // NodeHierarchyItemVec container + subtree_len
    // ---------------------------------------------------------------------

    #[test]
    fn node_hierarchy_item_vec_containers_read_and_write() {
        let mut v: NodeHierarchyItemVec = vec![NodeHierarchyItem::zeroed(); 2].into();
        {
            let mut c = v.as_container_mut();
            c[NodeId::new(1)].parent = 1; // raw 1 == NodeId(0)
        }
        let c = v.as_container();
        assert_eq!(c.len(), 2);
        assert_eq!(c[NodeId::new(1)].parent_id(), Some(NodeId::ZERO));
        assert!(c.get(NodeId::new(2)).is_none());

        let empty: NodeHierarchyItemVec = Vec::new().into();
        assert!(empty.as_container().is_empty());
    }

    #[test]
    fn subtree_len_counts_descendants_of_a_real_tree() {
        // body(0) > div(1) > div(2)
        let sd = nested_body();
        let h = sd.node_hierarchy.as_container();
        assert_eq!(h.len(), 3);
        assert_eq!(h.subtree_len(NodeId::ZERO), 2, "root has 2 descendants");
        assert_eq!(h.subtree_len(NodeId::new(1)), 1);
        assert_eq!(h.subtree_len(NodeId::new(2)), 0, "a leaf has no descendants");
    }

    #[test]
    fn subtree_len_saturates_on_a_malformed_backwards_next_sibling() {
        // Node 2 claims its next sibling is node 0 — a backwards link a malformed
        // FastDom can produce. The subtraction must saturate, not underflow-panic.
        let v: NodeHierarchyItemVec = vec![
            raw_item(0, 0, 0, 0),
            raw_item(0, 0, 0, 0),
            raw_item(0, 0, /* next = NodeId(0) */ 1, 0),
        ]
        .into();
        let c = v.as_container();
        assert_eq!(c.subtree_len(NodeId::new(2)), 0);

        // Self-referential next_sibling (node 1 -> node 1) must also saturate.
        let v2: NodeHierarchyItemVec = vec![raw_item(0, 0, 0, 0), raw_item(0, 0, 2, 0)].into();
        assert_eq!(v2.as_container().subtree_len(NodeId::new(1)), 0);
    }

    // ---------------------------------------------------------------------
    // StyledDomMemoryReport
    // ---------------------------------------------------------------------

    #[test]
    fn memory_report_default_total_is_zero() {
        assert_eq!(StyledDomMemoryReport::default().total_bytes(), 0);
    }

    #[test]
    fn memory_report_total_bytes_sums_every_field() {
        let r = StyledDomMemoryReport {
            node_count: 3,
            node_hierarchy_bytes: 1,
            node_data_bytes: 2,
            styled_nodes_bytes: 4,
            cascade_info_bytes: 8,
            tag_ids_bytes: 16,
            non_leaf_nodes_bytes: 32,
            callback_vecs_bytes: 64,
            ..StyledDomMemoryReport::default()
        };
        assert_eq!(r.total_bytes(), 127, "node_count must NOT be part of the sum");

        // A single saturated field must not overflow the running sum.
        let extreme = StyledDomMemoryReport {
            node_data_bytes: usize::MAX,
            ..StyledDomMemoryReport::default()
        };
        assert_eq!(extreme.total_bytes(), usize::MAX);
    }

    #[test]
    fn memory_report_tracks_node_count_and_is_monotonic_in_dom_size() {
        let small = flat_body(1).memory_report();
        let large = flat_body(50).memory_report();
        assert_eq!(small.node_count, 2);
        assert_eq!(large.node_count, 51);
        assert!(large.total_bytes() > small.total_bytes());
        assert!(small.total_bytes() >= small.node_hierarchy_bytes + small.node_data_bytes);

        // Also fine on the smallest possible DOM.
        let d = StyledDom::default().memory_report();
        assert_eq!(d.node_count, 1);
        assert!(d.total_bytes() > 0);
    }

    // ---------------------------------------------------------------------
    // StyledDom construction
    // ---------------------------------------------------------------------

    #[test]
    fn default_styled_dom_is_a_single_rooted_body() {
        let sd = StyledDom::default();
        assert_eq!(sd.node_count(), 1);
        assert_eq!(sd.root.into_crate_internal(), Some(NodeId::ZERO));
        assert_eq!(sd.node_hierarchy.as_ref().len(), 1);
        assert_eq!(sd.styled_nodes.as_ref().len(), 1);
        assert_eq!(sd.cascade_info.as_ref().len(), 1);
        assert_eq!(sd.non_leaf_nodes.as_ref().len(), 1);
        assert_eq!(sd.non_leaf_nodes.as_ref()[0].depth, 0);
        assert!(sd.tag_ids_to_node_ids.as_ref().is_empty());
        assert!(sd.get_styled_node_state(&NodeId::ZERO).is_normal());
    }

    #[test]
    fn create_empties_the_source_dom() {
        // Documented: "After calling this function, the DOM will be reset to an empty DOM."
        let mut dom = Dom::create_body().with_children(vec![Dom::create_div(); 3].into());
        let sd = StyledDom::create(&mut dom, Css::empty());
        assert_eq!(sd.node_count(), 4);
        assert!(
            dom.children.as_ref().is_empty(),
            "the source Dom must be left empty (it is swapped out, not cloned)"
        );
    }

    #[test]
    fn create_keeps_every_parallel_array_the_same_length() {
        for n in [0usize, 1, 3, 64] {
            let sd = flat_body(n);
            let count = sd.node_count();
            assert_eq!(count, n + 1);
            assert_eq!(sd.node_hierarchy.as_ref().len(), count);
            assert_eq!(sd.styled_nodes.as_ref().len(), count);
            assert_eq!(sd.cascade_info.as_ref().len(), count);
        }
    }

    #[test]
    fn create_survives_malformed_truncated_and_unicode_css() {
        let cases: Vec<String> = vec![
            String::new(),
            "}}}{{{".to_string(),
            "div {".to_string(),
            "div { color: }".to_string(),
            "div { : red; }".to_string(),
            "@media".to_string(),
            "/* unterminated comment".to_string(),
            "div { width: 99999999999999999999999px; }".to_string(),
            "div { width: -0px; opacity: 1e400; }".to_string(),
            "div { width: NaNpx; height: infpx; }".to_string(),
            "* { color: #ZZZZZZ; }".to_string(),
            "日本語 { content: \"🦀\"; }".to_string(),
            ".\u{202e}rtl { color: red; }".to_string(),
            "a".repeat(10_000),
            "div { color: red; }".repeat(500),
        ];

        for case in &cases {
            let css = parse_css(case);
            let mut dom = Dom::create_body().with_children(vec![Dom::create_div()].into());
            let sd = StyledDom::create(&mut dom, css);
            assert_eq!(
                sd.node_count(),
                2,
                "CSS must never change the node count; failing input: {case:?}"
            );
        }
    }

    #[test]
    fn create_handles_deep_and_wide_doms() {
        // deep: 64 nested divs under a body
        let mut deep = Dom::create_div();
        for _ in 0..63 {
            deep = Dom::create_div().with_children(vec![deep].into());
        }
        let mut deep_body = Dom::create_body().with_children(vec![deep].into());
        let sd = StyledDom::create(&mut deep_body, Css::empty());
        assert_eq!(sd.node_count(), 65);
        assert_eq!(
            sd.non_leaf_nodes.as_ref().len(),
            64,
            "every node except the innermost leaf is a parent"
        );

        // wide: 1000 siblings
        let wide = flat_body(1000);
        assert_eq!(wide.node_count(), 1001);
        assert_eq!(wide.node_hierarchy.as_container().subtree_len(NodeId::ZERO), 1000);
        assert_eq!(wide.non_leaf_nodes.as_ref().len(), 1);
    }

    #[test]
    fn create_from_dom_collects_scoped_css_without_changing_the_tree() {
        let dom = Dom::create_body().with_children(
            vec![
                Dom::create_div().with_css("color: red"),
                Dom::create_div().with_children(vec![Dom::create_div().with_css("width: 5px")].into()),
            ]
            .into(),
        );
        let sd = StyledDom::create_from_dom(dom);
        assert_eq!(sd.node_count(), 4);
        assert_eq!(sd.node_hierarchy.as_ref().len(), 4);
        assert!(sd.get_css_property_cache().compact_cache.is_some());
    }

    #[test]
    fn create_from_dom_on_a_bare_leaf_produces_one_node() {
        let sd = StyledDom::create_from_dom(Dom::create_div());
        assert_eq!(sd.node_count(), 1);
        assert_eq!(sd.root.into_crate_internal(), Some(NodeId::ZERO));
    }

    // ---------------------------------------------------------------------
    // append_child / append_child_with_index / finalize / with_child
    // ---------------------------------------------------------------------

    #[test]
    fn append_child_grows_the_node_count_by_the_child_dom_size() {
        let mut base = flat_body(2);
        base.append_child(flat_body(3));
        assert_eq!(base.node_count(), 3 + 4);
        assert_eq!(base.node_hierarchy.as_ref().len(), 7);
        assert_eq!(base.styled_nodes.as_ref().len(), 7);
        assert_eq!(base.cascade_info.as_ref().len(), 7);
    }

    #[test]
    fn append_child_links_the_new_root_as_the_last_sibling() {
        // Flat parent: body(0) > [div(1), div(2)], then append a 1-node StyledDom.
        let mut base = flat_body(2);
        base.append_child(StyledDom::default());

        let h = base.node_hierarchy.as_container();
        let children: Vec<NodeId> = NodeId::ZERO.az_children(&h).collect();
        assert_eq!(
            children,
            vec![NodeId::new(1), NodeId::new(2), NodeId::new(3)],
            "the appended root must become the last direct child"
        );
        assert_eq!(h[NodeId::new(3)].parent_id(), Some(NodeId::ZERO));
        assert_eq!(h[NodeId::new(3)].previous_sibling_id(), Some(NodeId::new(2)));
        assert_eq!(h[NodeId::new(3)].next_sibling_id(), None);
    }

    /// ADVERSARIAL: `append_child` reads `last_child_id()` to find the current
    /// last sibling. If `last_child` names a *descendant* rather than the last
    /// *direct child*, the appended root is spliced into the wrong sibling chain
    /// and disappears from the root's children.
    #[test]
    fn append_child_keeps_the_root_children_reachable_for_a_nested_dom() {
        let mut base = nested_body(); // body(0) > div(1) > div(2)
        base.append_child(StyledDom::default());
        assert_eq!(base.node_count(), 4);

        let h = base.node_hierarchy.as_container();
        let children: Vec<NodeId> = NodeId::ZERO.az_children(&h).collect();
        assert_eq!(
            children,
            vec![NodeId::new(1), NodeId::new(3)],
            "after append_child the root must have exactly its old child plus the appended root"
        );
    }

    #[test]
    fn append_child_with_index_saturates_the_u32_cascade_index() {
        for (child_index, expected) in [
            (0usize, 0u32),
            (7, 7),
            (u32::MAX as usize, u32::MAX),
            (u32::MAX as usize + 1, u32::MAX),
            (usize::MAX, u32::MAX),
        ] {
            let mut base = flat_body(0); // single body node
            base.append_child_with_index(StyledDom::default(), child_index);

            // The appended root lands at index self_len == 1 in the merged arrays.
            assert_eq!(
                base.cascade_info.as_ref()[1].index_in_parent,
                expected,
                "child_index {child_index} must saturate to {expected}, never wrap"
            );
            assert!(base.cascade_info.as_ref()[1].is_last_child);
            assert_eq!(base.node_count(), 2);
        }
    }

    #[test]
    fn finalize_non_leaf_nodes_sorts_by_depth_and_is_idempotent() {
        let mut base = flat_body(1);
        base.append_child_with_index(flat_body(2), 1);
        base.append_child_with_index(flat_body(2), 2);
        base.finalize_non_leaf_nodes();

        let depths: Vec<usize> = base.non_leaf_nodes.as_ref().iter().map(|p| p.depth).collect();
        let mut sorted = depths.clone();
        sorted.sort_unstable();
        assert_eq!(depths, sorted, "non_leaf_nodes must be depth-ordered");

        base.finalize_non_leaf_nodes();
        let again: Vec<usize> = base.non_leaf_nodes.as_ref().iter().map(|p| p.depth).collect();
        assert_eq!(depths, again, "finalize must be idempotent");
    }

    #[test]
    fn with_child_matches_append_child() {
        let mut appended = flat_body(2);
        appended.append_child(flat_body(1));

        let built = flat_body(2).with_child(flat_body(1));

        assert_eq!(built.node_count(), appended.node_count());
        assert_eq!(
            built.node_hierarchy.as_ref(),
            appended.node_hierarchy.as_ref()
        );
    }

    #[test]
    fn swap_with_default_returns_the_old_dom_and_resets_self() {
        let mut sd = flat_body(3);
        let old = sd.swap_with_default();
        assert_eq!(old.node_count(), 4);
        assert_eq!(sd.node_count(), 1, "self must be left as the default StyledDom");
        assert_eq!(sd.root.into_crate_internal(), Some(NodeId::ZERO));
    }

    // ---------------------------------------------------------------------
    // Menus
    // ---------------------------------------------------------------------

    #[test]
    fn context_menu_and_menu_bar_are_stored_on_the_root_node() {
        let mut sd = flat_body(1);
        assert!(sd.node_data.as_container()[NodeId::ZERO].get_context_menu().is_none());

        sd.set_context_menu(empty_menu());
        sd.set_menu_bar(empty_menu());

        let data = sd.node_data.as_container();
        assert!(data[NodeId::ZERO].get_context_menu().is_some());
        assert!(data[NodeId::ZERO].get_menu_bar().is_some());

        // ...and the child must not have inherited either of them.
        assert!(data[NodeId::new(1)].get_context_menu().is_none());
        assert!(data[NodeId::new(1)].get_menu_bar().is_none());
    }

    #[test]
    fn menu_builders_are_equivalent_to_the_setters_and_dont_touch_the_tree() {
        let sd = StyledDom::default()
            .with_context_menu(empty_menu())
            .with_menu_bar(empty_menu());
        assert_eq!(sd.node_count(), 1);
        let data = sd.node_data.as_container();
        assert!(data[NodeId::ZERO].get_context_menu().is_some());
        assert!(data[NodeId::ZERO].get_menu_bar().is_some());
    }

    // ---------------------------------------------------------------------
    // restyle_nodes_* / restyle_on_state_change / restyle_user_property
    // ---------------------------------------------------------------------

    #[test]
    fn restyle_nodes_hover_sets_and_clears_the_state_flag() {
        let mut sd = flat_body(2);
        let _ = sd.restyle_nodes_hover(&[NodeId::new(1)], true);
        assert!(sd.get_styled_node_state(&NodeId::new(1)).hover);
        assert!(!sd.get_styled_node_state(&NodeId::new(2)).hover);

        let _ = sd.restyle_nodes_hover(&[NodeId::new(1)], false);
        assert!(!sd.get_styled_node_state(&NodeId::new(1)).hover);
        assert!(sd.get_styled_node_state(&NodeId::new(1)).is_normal());
    }

    #[test]
    fn restyle_nodes_active_and_focus_set_independent_flags() {
        let mut sd = flat_body(1);
        let _ = sd.restyle_nodes_active(&[NodeId::ZERO], true);
        let _ = sd.restyle_nodes_focus(&[NodeId::ZERO], true);

        let state = sd.get_styled_node_state(&NodeId::ZERO);
        assert!(state.active);
        assert!(state.focused);
        assert!(!state.hover, "hover must be untouched");
        assert!(!state.is_normal());
    }

    #[test]
    fn restyle_nodes_ignores_out_of_range_node_ids_instead_of_panicking() {
        let mut sd = flat_body(1); // valid ids: 0, 1
        let changed = sd.restyle_nodes_hover(&[NodeId::new(2), NodeId::new(usize::MAX)], true);
        assert!(changed.is_empty());
        assert!(!sd.get_styled_node_state(&NodeId::ZERO).hover);
        assert!(!sd.get_styled_node_state(&NodeId::new(1)).hover);

        // A mix of valid and stale ids must still apply the valid ones.
        let _ = sd.restyle_nodes_hover(&[NodeId::new(1), NodeId::new(999)], true);
        assert!(sd.get_styled_node_state(&NodeId::new(1)).hover);
    }

    #[test]
    fn restyle_nodes_handles_empty_and_duplicated_input() {
        let mut sd = flat_body(1);
        assert!(sd.restyle_nodes_focus(&[], true).is_empty());

        // Duplicates must be idempotent, not double-applied or panicking.
        let _ = sd.restyle_nodes_focus(&[NodeId::ZERO, NodeId::ZERO, NodeId::ZERO], true);
        assert!(sd.get_styled_node_state(&NodeId::ZERO).focused);
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn get_styled_node_state_panics_on_an_out_of_range_node_id() {
        // Documents the contract: unlike restyle_nodes_*, this getter does NOT
        // bounds-check — callers must pass an id that indexes into this DOM.
        let sd = flat_body(1);
        let _ = sd.get_styled_node_state(&NodeId::new(99));
    }

    #[test]
    fn restyle_on_state_change_with_no_changes_reports_nothing_to_do() {
        let mut sd = flat_body(2);
        let r = sd.restyle_on_state_change(None, None, None);
        assert!(!r.has_changes());
        assert!(!r.needs_layout);
        assert!(!r.needs_display_list);
        assert!(!r.gpu_only_changes);
        assert_eq!(r.max_relayout_scope, RelayoutScope::None);
    }

    #[test]
    fn restyle_on_state_change_tolerates_stale_node_ids() {
        let mut sd = flat_body(1);
        let r = sd.restyle_on_state_change(
            Some(FocusChange {
                lost_focus: Some(NodeId::new(500)),
                gained_focus: Some(NodeId::new(usize::MAX)),
            }),
            Some(HoverChange {
                left_nodes: vec![NodeId::new(700)],
                entered_nodes: vec![NodeId::new(800)],
            }),
            Some(ActiveChange {
                deactivated: vec![NodeId::new(900)],
                activated: vec![NodeId::new(1000)],
            }),
        );
        assert!(!r.has_changes(), "stale ids must be filtered, not applied");
        assert_eq!(sd.node_count(), 2);
    }

    #[test]
    fn restyle_on_state_change_applies_state_to_valid_nodes() {
        let mut sd = flat_body(1);
        let r = sd.restyle_on_state_change(
            None,
            Some(HoverChange {
                left_nodes: Vec::new(),
                entered_nodes: vec![NodeId::new(1)],
            }),
            None,
        );
        assert!(sd.get_styled_node_state(&NodeId::new(1)).hover);
        assert!(
            r.changed_nodes.keys().all(|n| *n == NodeId::new(1)),
            "only the node whose state actually changed may be reported"
        );
    }

    #[test]
    fn restyle_user_property_rejects_empty_lists_and_stale_nodes() {
        let mut sd = flat_body(1);
        assert!(sd.restyle_user_property(&NodeId::ZERO, &[]).is_empty());
        assert!(
            sd.restyle_user_property(
                &NodeId::new(50),
                &[CssProperty::auto(CssPropertyType::Width)]
            )
            .is_empty(),
            "an out-of-range node id must be a no-op, not a panic"
        );
        assert!(
            sd.get_css_property_cache()
                .user_overridden_properties
                .iter()
                .all(Vec::is_empty),
            "a rejected call must not record an override"
        );
    }

    #[test]
    fn restyle_user_property_stores_the_override_and_initial_removes_it() {
        let mut sd = flat_body(1);
        let node = NodeId::ZERO;

        let _ = sd.restyle_user_property(&node, &[CssProperty::auto(CssPropertyType::Width)]);
        {
            let overrides = &sd.get_css_property_cache().user_overridden_properties;
            assert_eq!(overrides.len(), sd.node_count(), "table grows to cover the DOM");
            assert_eq!(overrides[0].len(), 1);
            assert_eq!(overrides[0][0].0, CssPropertyType::Width);
        }

        // Re-setting the same type replaces rather than duplicating.
        let _ = sd.restyle_user_property(&node, &[CssProperty::none(CssPropertyType::Width)]);
        assert_eq!(sd.get_css_property_cache().user_overridden_properties[0].len(), 1);

        // CssProperty::Initial removes the override again.
        let _ = sd.restyle_user_property(&node, &[CssProperty::initial(CssPropertyType::Width)]);
        assert!(sd.get_css_property_cache().user_overridden_properties[0].is_empty());

        // Removing a property that was never set must not panic.
        let _ = sd.restyle_user_property(&node, &[CssProperty::initial(CssPropertyType::Height)]);
        assert!(sd.get_css_property_cache().user_overridden_properties[0].is_empty());
    }

    #[test]
    fn restyle_and_recompute_preserve_the_tree_and_rebuild_the_compact_cache() {
        let mut sd = flat_body(3);
        let before = sd.node_count();

        sd.restyle(parse_css("div { color: red; } body > div:hover { color: blue; }"));
        assert_eq!(sd.node_count(), before);
        assert!(sd.get_css_property_cache().compact_cache.is_some());

        // A second restyle with garbage CSS must not corrupt the structure.
        sd.restyle(parse_css("}}} div { : ; }"));
        assert_eq!(sd.node_count(), before);

        sd.recompute_inheritance_and_compact_cache();
        assert_eq!(sd.node_count(), before);
        assert!(sd.get_css_property_cache().compact_cache.is_some());
    }

    #[test]
    fn get_css_property_cache_mut_sees_the_same_cache_as_the_shared_getter() {
        let mut sd = flat_body(1);
        let node_count = sd.node_count();
        sd.get_css_property_cache_mut()
            .user_overridden_properties
            .resize(node_count, Vec::new());
        assert_eq!(
            sd.get_css_property_cache().user_overridden_properties.len(),
            node_count
        );
    }

    // ---------------------------------------------------------------------
    // get_html_string
    // ---------------------------------------------------------------------

    #[test]
    fn get_html_string_test_mode_omits_the_html_wrapper() {
        let sd = flat_body(2);
        let out = sd.get_html_string("HEAD_MARK", "BODY_MARK", true);
        assert!(!out.is_empty());
        assert!(!out.contains("HEAD_MARK"), "test_mode must not emit the custom head");
        assert!(!out.contains("BODY_MARK"), "test_mode must not emit the custom body");
        assert!(!out.contains("<html>"));
    }

    #[test]
    fn get_html_string_embeds_custom_head_and_body_verbatim() {
        let sd = flat_body(1);
        let head = "🦀 <meta charset=\"utf-8\"> & ünïcödé";
        let body = "x".repeat(10_000);
        let out = sd.get_html_string(head, &body, false);
        assert!(out.contains("<html>"));
        assert!(out.contains(head));
        assert!(out.contains(&body));
    }

    #[test]
    fn get_html_string_does_not_panic_on_extreme_doms() {
        // A single-node DOM has no non_leaf parent entry for its root — the depth
        // lookup must fall back to 0 rather than panic-indexing the map.
        assert!(!StyledDom::default().get_html_string("", "", true).is_empty());
        assert!(!flat_body(0).get_html_string("", "", true).is_empty());
        assert!(!nested_body().get_html_string("", "", true).is_empty());
        assert!(!flat_body(200).get_html_string("", "", true).is_empty());
    }

    // ---------------------------------------------------------------------
    // rendering order
    // ---------------------------------------------------------------------

    #[test]
    fn get_rects_in_rendering_order_is_a_permutation_of_the_children() {
        let sd = flat_body(3);
        let group = sd.get_rects_in_rendering_order();
        assert_eq!(group.root.into_crate_internal(), Some(NodeId::ZERO));

        let mut ids: Vec<usize> = group
            .children
            .as_ref()
            .iter()
            .filter_map(|c| c.root.into_crate_internal())
            .map(|n| n.index())
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2, 3], "every child appears exactly once");
    }

    #[test]
    fn get_rects_in_rendering_order_nests_grandchildren() {
        let sd = nested_body(); // body(0) > div(1) > div(2)
        let group = sd.get_rects_in_rendering_order();
        assert_eq!(group.children.as_ref().len(), 1);

        let child = &group.children.as_ref()[0];
        assert_eq!(child.root.into_crate_internal(), Some(NodeId::new(1)));
        assert_eq!(child.children.as_ref().len(), 1);
        assert_eq!(
            child.children.as_ref()[0].root.into_crate_internal(),
            Some(NodeId::new(2))
        );
    }

    #[test]
    fn determine_rendering_order_with_no_parents_yields_a_childless_root() {
        let sd = StyledDom::default();
        let hierarchy = sd.node_hierarchy.as_container();
        let styled = sd.styled_nodes.as_container();
        let data = sd.node_data.as_container();

        let group = StyledDom::determine_rendering_order(
            &[],
            &hierarchy,
            &styled,
            &data,
            sd.get_css_property_cache(),
        );
        assert_eq!(group.root.into_crate_internal(), Some(NodeId::ZERO));
        assert!(group.children.as_ref().is_empty());
    }

    #[test]
    fn sort_children_by_position_returns_every_child_of_a_leaf_free_parent() {
        let sd = flat_body(3);
        let hierarchy = sd.node_hierarchy.as_container();
        let styled = sd.styled_nodes.as_container();
        let data = sd.node_data.as_container();

        let sorted = sort_children_by_position(
            NodeId::ZERO,
            &hierarchy,
            &styled,
            &data,
            sd.get_css_property_cache(),
        );
        assert_eq!(sorted.len(), 3);

        // A leaf parent has no children at all.
        let leaf = sort_children_by_position(
            NodeId::new(3),
            &hierarchy,
            &styled,
            &data,
            sd.get_css_property_cache(),
        );
        assert!(leaf.is_empty());
    }

    #[test]
    fn fill_content_group_children_builds_the_nested_group_tree() {
        let id = |i: usize| NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i)));

        let mut sorted: BTreeMap<NodeHierarchyItemId, Vec<NodeHierarchyItemId>> = BTreeMap::new();
        sorted.insert(id(0), vec![id(1), id(2)]);
        sorted.insert(id(1), vec![id(3)]);

        let mut group = ContentGroup {
            root: id(0),
            children: Vec::new().into(),
        };
        fill_content_group_children(&mut group, &sorted);

        assert_eq!(group.children.as_ref().len(), 2);
        assert_eq!(group.children.as_ref()[0].root, id(1));
        assert_eq!(group.children.as_ref()[0].children.as_ref().len(), 1);
        assert_eq!(group.children.as_ref()[0].children.as_ref()[0].root, id(3));
        assert!(
            group.children.as_ref()[1].children.as_ref().is_empty(),
            "a node with no entry in the map is a leaf"
        );
    }

    #[test]
    fn fill_content_group_children_leaves_an_unknown_root_untouched() {
        let sorted: BTreeMap<NodeHierarchyItemId, Vec<NodeHierarchyItemId>> = BTreeMap::new();
        let mut group = ContentGroup {
            root: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(9))),
            children: Vec::new().into(),
        };
        fill_content_group_children(&mut group, &sorted);
        assert!(group.children.as_ref().is_empty());
    }

    // ---------------------------------------------------------------------
    // recursive_get_last_child / get_path_to_root
    // ---------------------------------------------------------------------

    #[test]
    fn recursive_get_last_child_descends_to_the_deepest_last_child() {
        // 0 -> 1 -> 2 (2 is a leaf)
        let items = vec![
            raw_item(0, 0, 0, 2), // node 0, last_child = NodeId(1)
            raw_item(1, 0, 0, 3), // node 1, last_child = NodeId(2)
            raw_item(2, 0, 0, 0), // node 2, leaf
        ];

        let mut target = None;
        recursive_get_last_child(NodeId::ZERO, &items, &mut target);
        assert_eq!(target, Some(NodeId::new(2)));
    }

    #[test]
    fn recursive_get_last_child_leaves_the_target_untouched_for_a_leaf() {
        let items = vec![raw_item(0, 0, 0, 0)];
        let mut target = None;
        recursive_get_last_child(NodeId::ZERO, &items, &mut target);
        assert_eq!(target, None);

        // A pre-set target is also left alone.
        let mut preset = Some(NodeId::new(7));
        recursive_get_last_child(NodeId::ZERO, &items, &mut preset);
        assert_eq!(preset, Some(NodeId::new(7)));
    }

    #[test]
    fn get_path_to_root_is_root_first_and_tolerates_unknown_nodes() {
        let sd = nested_body(); // body(0) > div(1) > div(2)
        let h = sd.node_hierarchy.as_container();

        assert_eq!(get_path_to_root(&h, NodeId::ZERO), vec![NodeId::ZERO]);
        assert_eq!(
            get_path_to_root(&h, NodeId::new(2)),
            vec![NodeId::ZERO, NodeId::new(1), NodeId::new(2)]
        );

        // An id outside the arena yields a one-element path instead of panicking.
        assert_eq!(
            get_path_to_root(&h, NodeId::new(9999)),
            vec![NodeId::new(9999)]
        );
    }

    // ---------------------------------------------------------------------
    // document order
    // ---------------------------------------------------------------------

    #[test]
    fn is_before_in_document_order_is_false_for_identical_nodes() {
        let sd = flat_body(2);
        assert!(!is_before_in_document_order(
            &sd.node_hierarchy,
            NodeId::new(1),
            NodeId::new(1)
        ));
    }

    #[test]
    fn is_before_in_document_order_orders_ancestors_and_siblings() {
        let sd = flat_body(3); // body(0) > [1, 2, 3]
        let h = &sd.node_hierarchy;

        assert!(is_before_in_document_order(h, NodeId::ZERO, NodeId::new(1)));
        assert!(!is_before_in_document_order(h, NodeId::new(1), NodeId::ZERO));
        assert!(is_before_in_document_order(h, NodeId::new(1), NodeId::new(3)));
        assert!(!is_before_in_document_order(h, NodeId::new(3), NodeId::new(1)));
    }

    #[test]
    fn is_before_in_document_order_is_antisymmetric_across_a_nested_tree() {
        let sd = nested_body();
        let h = &sd.node_hierarchy;
        for a in 0..3 {
            for b in 0..3 {
                let ab = is_before_in_document_order(h, NodeId::new(a), NodeId::new(b));
                let ba = is_before_in_document_order(h, NodeId::new(b), NodeId::new(a));
                if a == b {
                    assert!(!ab && !ba, "a node is never before itself");
                } else {
                    assert_ne!(ab, ba, "exactly one of ({a},{b}) / ({b},{a}) must hold");
                }
            }
        }
    }

    #[test]
    fn is_before_in_document_order_is_deterministic_for_unknown_nodes() {
        let sd = flat_body(1);
        let h = &sd.node_hierarchy;
        // Out-of-range ids fall back to a single-element path; the comparison must
        // still terminate and return a stable answer instead of panicking.
        assert!(is_before_in_document_order(h, NodeId::ZERO, NodeId::new(usize::MAX)));
        assert!(!is_before_in_document_order(h, NodeId::new(usize::MAX), NodeId::ZERO));
    }

    #[test]
    fn collect_nodes_in_document_order_start_equals_end() {
        let sd = flat_body(2);
        assert_eq!(
            collect_nodes_in_document_order(&sd.node_hierarchy, NodeId::new(2), NodeId::new(2)),
            vec![NodeId::new(2)]
        );
        // Even a bogus id short-circuits to itself (documented start == end path).
        assert_eq!(
            collect_nodes_in_document_order(
                &sd.node_hierarchy,
                NodeId::new(usize::MAX),
                NodeId::new(usize::MAX)
            ),
            vec![NodeId::new(usize::MAX)]
        );
    }

    #[test]
    fn collect_nodes_in_document_order_walks_the_tree_in_pre_order() {
        let sd = flat_body(3); // body(0) > [1, 2, 3]
        assert_eq!(
            collect_nodes_in_document_order(&sd.node_hierarchy, NodeId::ZERO, NodeId::new(3)),
            vec![NodeId::ZERO, NodeId::new(1), NodeId::new(2), NodeId::new(3)]
        );
        assert_eq!(
            collect_nodes_in_document_order(&sd.node_hierarchy, NodeId::new(1), NodeId::new(2)),
            vec![NodeId::new(1), NodeId::new(2)]
        );

        // Nested: body(0) > div(1) > div(2) — pre-order is 0, 1, 2.
        let nested = nested_body();
        assert_eq!(
            collect_nodes_in_document_order(&nested.node_hierarchy, NodeId::ZERO, NodeId::new(2)),
            vec![NodeId::ZERO, NodeId::new(1), NodeId::new(2)]
        );
    }

    #[test]
    fn collect_nodes_in_document_order_terminates_when_end_precedes_start() {
        // The traversal hits `end` before it ever enters the range, so it bails
        // out with an empty result rather than looping forever.
        let sd = flat_body(3);
        let out = collect_nodes_in_document_order(&sd.node_hierarchy, NodeId::new(2), NodeId::new(1));
        assert!(out.is_empty());
    }

    #[test]
    fn collect_nodes_in_document_order_with_an_unreachable_end_stops_at_the_tree_end() {
        let sd = flat_body(3);
        let out = collect_nodes_in_document_order(
            &sd.node_hierarchy,
            NodeId::new(1),
            NodeId::new(usize::MAX),
        );
        assert_eq!(
            out,
            vec![NodeId::new(1), NodeId::new(2), NodeId::new(3)],
            "an end node that is never reached must terminate at the end of the traversal"
        );
    }

    // ---------------------------------------------------------------------
    // is_layout_equivalent
    // ---------------------------------------------------------------------

    #[test]
    fn is_layout_equivalent_holds_for_independently_built_identical_doms() {
        assert!(is_layout_equivalent(&flat_body(3), &flat_body(3)));
        assert!(is_layout_equivalent(
            &StyledDom::default(),
            &StyledDom::default()
        ));
        assert!(is_layout_equivalent(&nested_body(), &nested_body()));
    }

    #[test]
    fn is_layout_equivalent_rejects_a_different_node_count() {
        assert!(!is_layout_equivalent(&flat_body(3), &flat_body(4)));
        assert!(!is_layout_equivalent(&flat_body(0), &flat_body(1)));
    }

    #[test]
    fn is_layout_equivalent_rejects_a_different_structure() {
        // Same node count (3), different shape: [body > div > div] vs [body > div, div]
        assert!(!is_layout_equivalent(&nested_body(), &flat_body(2)));
    }

    #[test]
    fn is_layout_equivalent_rejects_a_changed_class() {
        let build = |class: &str| {
            let mut dom = Dom::create_body().with_children(
                vec![Dom::create_div().with_class(class.to_string().into())].into(),
            );
            StyledDom::create(&mut dom, Css::empty())
        };
        assert!(is_layout_equivalent(&build("a"), &build("a")));
        assert!(!is_layout_equivalent(&build("a"), &build("b")));
    }

    #[test]
    fn is_layout_equivalent_rejects_a_changed_pseudo_state() {
        let base = flat_body(2);
        let mut hovered = flat_body(2);
        let _ = hovered.restyle_nodes_hover(&[NodeId::new(1)], true);
        assert!(
            !is_layout_equivalent(&base, &hovered),
            ":hover changes CSS resolution, so the DOMs are not layout-equivalent"
        );
    }

    // ---------------------------------------------------------------------
    // CompactDom + convert_dom_into_compact_dom
    // ---------------------------------------------------------------------

    #[test]
    fn compact_dom_len_and_is_empty() {
        let single = convert_dom_into_compact_dom(Dom::create_div());
        assert_eq!(single.len(), 1);
        assert!(!single.is_empty());

        let tree = convert_dom_into_compact_dom(
            Dom::create_body().with_children(vec![Dom::create_div(); 4].into()),
        );
        assert_eq!(tree.len(), 5);
        assert!(!tree.is_empty());

        // A hand-built zero-node arena is the only way to observe is_empty() == true.
        let empty = CompactDom {
            node_hierarchy: NodeHierarchy {
                internal: Vec::new(),
            },
            node_data: NodeDataContainer {
                internal: Vec::new(),
            },
            root: NodeId::ZERO,
        };
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn convert_dom_into_compact_dom_links_flat_siblings() {
        let compact = convert_dom_into_compact_dom(
            Dom::create_body().with_children(vec![Dom::create_div(); 3].into()),
        );
        assert_eq!(compact.len(), 4);
        assert_eq!(compact.root, NodeId::ZERO);

        let h = compact.node_hierarchy.as_ref();
        assert_eq!(h[NodeId::ZERO].parent, None);
        assert_eq!(h[NodeId::ZERO].last_child, Some(NodeId::new(3)));

        for i in 1..=3usize {
            assert_eq!(h[NodeId::new(i)].parent, Some(NodeId::ZERO));
            let expected_next = if i == 3 { None } else { Some(NodeId::new(i + 1)) };
            assert_eq!(h[NodeId::new(i)].next_sibling, expected_next);
            let expected_prev = if i == 1 { None } else { Some(NodeId::new(i - 1)) };
            assert_eq!(h[NodeId::new(i)].previous_sibling, expected_prev);
            assert_eq!(h[NodeId::new(i)].last_child, None, "the children are leaves");
        }
    }

    /// ADVERSARIAL: `last_child` must name the last DIRECT child — that is the
    /// contract `NodeHierarchyItem::last_child_id()` documents, the one
    /// `az_reverse_children` walks backwards from, and the one `append_child`
    /// splices new siblings onto. The flat encoding computes it as
    /// `node_id + estimated_total_children`, which is the last node of the whole
    /// SUBTREE — those coincide only when the last direct child is a leaf.
    #[test]
    fn convert_dom_into_compact_dom_last_child_is_the_last_direct_child() {
        // body(0) > div(1) > div(2): the body's only direct child is node 1.
        let sd = nested_body();
        let h = sd.node_hierarchy.as_container();

        let last_direct_child = NodeId::ZERO.az_children(&h).last();
        assert_eq!(last_direct_child, Some(NodeId::new(1)));
        assert_eq!(
            h[NodeId::ZERO].last_child_id(),
            last_direct_child,
            "last_child_id() must agree with the forward child iteration"
        );
    }

    #[test]
    fn convert_dom_into_compact_dom_handles_an_empty_and_a_deep_tree() {
        assert_eq!(convert_dom_into_compact_dom(Dom::create_body()).len(), 1);

        let mut deep = Dom::create_div();
        for _ in 0..64 {
            deep = Dom::create_div().with_children(vec![deep].into());
        }
        let compact = convert_dom_into_compact_dom(deep);
        assert_eq!(compact.len(), 65);
        // Pre-order ids: every node's parent is the node right before it.
        let h = compact.node_hierarchy.as_ref();
        for i in 1..65usize {
            assert_eq!(h[NodeId::new(i)].parent, Some(NodeId::new(i - 1)));
        }
    }

    // ---------------------------------------------------------------------
    // scope_inline_css / collect_css_from_dom / strip_css_from_dom
    // ---------------------------------------------------------------------

    #[test]
    fn scope_inline_css_advances_next_id_once_per_node() {
        let mut dom = Dom::create_body().with_children(
            vec![
                Dom::create_div().with_children(vec![Dom::create_div()].into()),
                Dom::create_div(),
            ]
            .into(),
        );
        let _ = dom.fixup_children_estimated();

        let mut next = 0usize;
        scope_inline_css(&mut dom, &mut next);
        assert_eq!(next, 4, "4 nodes → the counter must land on 4 (pre-order ids 0..3)");
    }

    #[test]
    fn scope_inline_css_from_zero_and_from_a_large_offset() {
        let mut leaf = Dom::create_div();
        let _ = leaf.fixup_children_estimated();
        let mut next = 0usize;
        scope_inline_css(&mut leaf, &mut next);
        assert_eq!(next, 1, "a single leaf consumes exactly one id");

        // A large (but non-saturating) starting id must not panic or wrap.
        let mut dom = Dom::create_body().with_children(vec![Dom::create_div(); 2].into());
        let _ = dom.fixup_children_estimated();
        let mut big = 1_000_000usize;
        scope_inline_css(&mut dom, &mut big);
        assert_eq!(big, 1_000_003);
    }

    #[test]
    fn scope_inline_css_preserves_the_rule_count_of_every_node() {
        let mut dom = Dom::create_body()
            .with_css("color: red")
            .with_children(vec![Dom::create_div().with_css("width: 5px")].into());
        let _ = dom.fixup_children_estimated();

        let rules_before: usize = dom
            .css
            .as_ref()
            .iter()
            .map(|c| c.rules.as_ref().len())
            .sum::<usize>()
            + dom.children.as_ref()[0]
                .css
                .as_ref()
                .iter()
                .map(|c| c.rules.as_ref().len())
                .sum::<usize>();
        assert!(rules_before > 0, "with_css must produce at least one rule");

        let mut next = 0usize;
        scope_inline_css(&mut dom, &mut next);

        let rules_after: usize = dom
            .css
            .as_ref()
            .iter()
            .map(|c| c.rules.as_ref().len())
            .sum::<usize>()
            + dom.children.as_ref()[0]
                .css
                .as_ref()
                .iter()
                .map(|c| c.rules.as_ref().len())
                .sum::<usize>();
        assert_eq!(
            rules_before, rules_after,
            "scoping rewrites paths in place; it must not add or drop rules"
        );
        assert_eq!(next, 2);
    }

    #[test]
    fn collect_css_from_dom_yields_inner_css_before_outer_css() {
        let outer = parse_css("div { color: red; } span { color: blue; }");
        let inner = parse_css("p { color: green; }");
        let outer_rules = outer.rules.as_ref().len();
        let inner_rules = inner.rules.as_ref().len();
        assert_ne!(
            outer_rules, inner_rules,
            "the two stylesheets must be distinguishable by rule count"
        );

        let mut child = Dom::create_div();
        child.add_component_css(inner);
        let mut dom = Dom::create_body().with_children(vec![child].into());
        dom.add_component_css(outer);

        let mut out = Vec::new();
        collect_css_from_dom(&dom, &mut out);

        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0].rules.as_ref().len(),
            inner_rules,
            "deeper CSS is collected first (lower cascade priority)"
        );
        assert_eq!(out[1].rules.as_ref().len(), outer_rules);
    }

    #[test]
    fn collect_css_from_dom_on_a_css_free_tree_appends_nothing() {
        let dom = Dom::create_body().with_children(vec![Dom::create_div(); 3].into());
        let mut out = Vec::new();
        collect_css_from_dom(&dom, &mut out);
        assert!(out.is_empty());

        // ...and an already-populated `out` is appended to, not replaced.
        let mut prefilled = vec![Css::empty()];
        collect_css_from_dom(&dom, &mut prefilled);
        assert_eq!(prefilled.len(), 1);
    }

    #[test]
    fn strip_css_from_dom_clears_every_node_recursively() {
        let mut dom = Dom::create_body()
            .with_css("color: red")
            .with_children(
                vec![Dom::create_div()
                    .with_css("width: 5px")
                    .with_children(vec![Dom::create_div().with_css("height: 5px")].into())]
                .into(),
            );
        assert!(!dom.css.as_ref().is_empty());

        strip_css_from_dom(&mut dom);

        assert!(dom.css.as_ref().is_empty());
        let child = &dom.children.as_ref()[0];
        assert!(child.css.as_ref().is_empty());
        assert!(child.children.as_ref()[0].css.as_ref().is_empty());

        // Idempotent.
        strip_css_from_dom(&mut dom);
        assert!(dom.css.as_ref().is_empty());
    }
}
