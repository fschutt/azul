use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
};

use azul_css::{
    css::{Css, CssPath},
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
    FastBTreeSet, FastHashMap,
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
#[derive(Debug, Clone, PartialEq)]
pub struct FocusChange {
    /// Node that lost focus (if any)
    pub lost_focus: Option<NodeId>,
    /// Node that gained focus (if any)
    pub gained_focus: Option<NodeId>,
}

/// Hover state change for restyle operations
#[derive(Debug, Clone, PartialEq)]
pub struct HoverChange {
    /// Nodes that the mouse left
    pub left_nodes: Vec<NodeId>,
    /// Nodes that the mouse entered
    pub entered_nodes: Vec<NodeId>,
}

/// Active (mouse down) state change for restyle operations
#[derive(Debug, Clone, PartialEq)]
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
    pub changed_nodes: BTreeMap<NodeId, Vec<ChangedCssProperty>>,
    /// Whether layout needs to be recalculated (layout properties changed)
    pub needs_layout: bool,
    /// Whether display list needs regeneration (visual properties changed)
    pub needs_display_list: bool,
    /// Whether only GPU-level properties changed (opacity, transform)
    /// If true and needs_display_list is false, we can update via GPU without display list rebuild
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
    pub fn has_changes(&self) -> bool {
        !self.changed_nodes.is_empty()
    }

    /// Merge another RestyleResult into this one
    pub fn merge(&mut self, other: RestyleResult) {
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

#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub enum CssPropertySource {
    Css(CssPath),
    Inline,
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

impl core::fmt::Debug for StyledNodeState {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
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
        write!(f, "{:?}", v)
    }
}

impl StyledNodeState {
    /// Creates a new state with all states set to false (normal state).
    pub const fn new() -> Self {
        StyledNodeState {
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
    pub fn has_state(&self, state_type: u8) -> bool {
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
    pub fn is_normal(&self) -> bool {
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

    /// Convert to PseudoStateFlags for use with dynamic selectors
    pub fn to_pseudo_state_flags(&self) -> azul_css::dynamic_selector::PseudoStateFlags {
        azul_css::dynamic_selector::PseudoStateFlags {
            hover: self.hover,
            active: self.active,
            focused: self.focused,
            disabled: self.disabled,
            checked: self.checked,
            focus_within: self.focus_within,
            visited: self.visited,
            backdrop: self.backdrop,
            dragging: self.dragging,
            drag_over: self.drag_over,
        }
    }

    /// Create from PseudoStateFlags (reverse of to_pseudo_state_flags)
    pub fn from_pseudo_state_flags(flags: &azul_css::dynamic_selector::PseudoStateFlags) -> Self {
        StyledNodeState {
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
#[repr(C)]
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct StyledNode {
    /// Current state of this styled node (used later for caching the style / layout)
    pub styled_node_state: StyledNodeState,
}

impl_option!(
    StyledNode,
    OptionStyledNode,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl_vec!(StyledNode, StyledNodeVec, StyledNodeVecDestructor, StyledNodeVecDestructorType, StyledNodeVecSlice, OptionStyledNode);
impl_vec_mut!(StyledNode, StyledNodeVec);
impl_vec_debug!(StyledNode, StyledNodeVec);
impl_vec_partialord!(StyledNode, StyledNodeVec);
impl_vec_clone!(StyledNode, StyledNodeVec, StyledNodeVecDestructor);
impl_vec_partialeq!(StyledNode, StyledNodeVec);

impl StyledNodeVec {
    /// Returns an immutable container reference for indexed access.
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, StyledNode> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    /// Returns a mutable container reference for indexed access.
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, StyledNode> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

#[test]
fn test_it() {
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
    let _styled_dom = Dom::create_body()
        .with_children(
            vec![Dom::create_div()
                .with_ids_and_classes(
                    vec![crate::dom::IdOrClass::Id("div1".to_string().into())].into(),
                )
                .with_children(vec![Dom::create_div()].into())]
            .into(),
        )
        .style(css.0);
}

/// Calculated hash of a font-family
#[derive(Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct StyleFontFamilyHash(pub u64);

impl ::core::fmt::Debug for StyleFontFamilyHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "StyleFontFamilyHash({})", self.0)
    }
}

impl StyleFontFamilyHash {
    /// Computes a 64-bit hash of a font family for cache lookups.
    pub fn new(family: &StyleFontFamily) -> Self {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        family.hash(&mut hasher);
        Self(hasher.finalize64())
    }
}

/// Calculated hash of a font-family
#[derive(Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct StyleFontFamiliesHash(pub u64);

impl ::core::fmt::Debug for StyleFontFamiliesHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "StyleFontFamiliesHash({})", self.0)
    }
}

impl StyleFontFamiliesHash {
    /// Computes a 64-bit hash of multiple font families for cache lookups.
    pub fn new(families: &[StyleFontFamily]) -> Self {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        for f in families.iter() {
            f.hash(&mut hasher);
        }
        Self(hasher.finalize64())
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.into_crate_internal() {
            Some(n) => write!(f, "Some(NodeId({}))", n),
            None => write!(f, "None"),
        }
    }
}

impl fmt::Display for NodeHierarchyItemId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl NodeHierarchyItemId {
    /// Represents `None` (no node). Encoded as `inner = 0`.
    pub const NONE: NodeHierarchyItemId = NodeHierarchyItemId { inner: 0 };

    /// Creates an `NodeHierarchyItemId` from a raw 1-based encoded value.
    ///
    /// # Warning
    ///
    /// The value must use 1-based encoding (0 = None, n = NodeId(n-1)).
    /// Prefer using [`NodeHierarchyItemId::from_crate_internal`] instead.
    #[inline]
    pub const fn from_raw(value: usize) -> Self {
        Self { inner: value }
    }

    /// Returns the raw 1-based encoded value.
    ///
    /// # Warning
    ///
    /// The returned value uses 1-based encoding. Do NOT use as an array index!
    #[inline]
    pub const fn into_raw(&self) -> usize {
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
    pub const fn into_crate_internal(&self) -> Option<NodeId> {
        NodeId::from_usize(self.inner)
    }

    /// Encodes from `Option<NodeId>` (None → 0, Some(NodeId(n)) → n+1).
    #[inline]
    pub const fn from_crate_internal(t: Option<NodeId>) -> Self {
        Self {
            inner: NodeId::into_raw(&t),
        }
    }
}

impl From<Option<NodeId>> for NodeHierarchyItemId {
    #[inline]
    fn from(opt: Option<NodeId>) -> Self {
        NodeHierarchyItemId::from_crate_internal(opt)
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
    pub const fn zeroed() -> Self {
        Self {
            parent: 0,
            previous_sibling: 0,
            next_sibling: 0,
            last_child: 0,
        }
    }
}

impl From<Node> for NodeHierarchyItem {
    fn from(node: Node) -> NodeHierarchyItem {
        NodeHierarchyItem {
            parent: NodeId::into_raw(&node.parent),
            previous_sibling: NodeId::into_raw(&node.previous_sibling),
            next_sibling: NodeId::into_raw(&node.next_sibling),
            last_child: NodeId::into_raw(&node.last_child),
        }
    }
}

impl NodeHierarchyItem {
    /// Returns the parent node ID, if any.
    pub fn parent_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.parent)
    }
    /// Returns the previous sibling node ID, if any.
    pub fn previous_sibling_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.previous_sibling)
    }
    /// Returns the next sibling node ID, if any.
    pub fn next_sibling_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.next_sibling)
    }
    /// Returns the first child node ID (current_node_id + 1 if has children).
    pub fn first_child_id(&self, current_node_id: NodeId) -> Option<NodeId> {
        self.last_child_id().map(|_| current_node_id + 1)
    }
    /// Returns the last child node ID, if any.
    pub fn last_child_id(&self) -> Option<NodeId> {
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
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, NodeHierarchyItem> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    /// Returns a mutable container reference for indexed access.
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, NodeHierarchyItem> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

impl<'a> NodeDataContainerRef<'a, NodeHierarchyItem> {
    /// Returns the number of descendant nodes under the given parent.
    #[inline]
    pub fn subtree_len(&self, parent_id: NodeId) -> usize {
        let self_item_index = parent_id.index();
        let next_item_index = match self[parent_id].next_sibling_id() {
            None => self.len(),
            Some(s) => s.index(),
        };
        next_item_index - self_item_index - 1
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

impl core::fmt::Debug for ParentWithNodeDepth {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
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

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
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
    pub nodes_with_not_callbacks: NodeHierarchyItemIdVec,
    pub nodes_with_datasets: NodeHierarchyItemIdVec,
    pub tag_ids_to_node_ids: TagIdToNodeIdMappingVec,
    pub non_leaf_nodes: ParentWithNodeDepthVec,
    pub css_property_cache: CssPropertyCachePtr,
    /// The ID of this DOM in the layout tree (for multi-DOM support with VirtualizedViews)
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
            nodes_with_not_callbacks: Vec::new().into(),
            nodes_with_datasets: Vec::new().into(),
            css_property_cache: CssPropertyCachePtr::new(CssPropertyCache::empty(1)),
            dom_id: DomId::ROOT_ID,
        }
    }
}

impl StyledDom {
    /// Creates a new StyledDom by applying CSS styles to a DOM tree.
    ///
    /// NOTE: After calling this function, the DOM will be reset to an empty DOM.
    // This is for memory optimization, so that the DOM does not need to be cloned.
    //
    // The CSS will be left in-place, but will be re-ordered
    pub fn create(dom: &mut Dom, mut css: Css) -> Self {
        use core::mem;

        use crate::dom::EventFilter;

        let t0 = std::time::Instant::now();

        let mut swap_dom = Dom::create_body();

        mem::swap(dom, &mut swap_dom);

        let compact_dom: CompactDom = swap_dom.into();
        let non_leaf_nodes = compact_dom
            .node_hierarchy
            .as_ref()
            .get_parents_sorted_by_depth();
        let node_hierarchy: NodeHierarchyItemVec = compact_dom
            .node_hierarchy
            .as_ref()
            .internal
            .iter()
            .map(|i| (*i).into())
            .collect::<Vec<NodeHierarchyItem>>()
            .into();

        let mut styled_nodes = vec![
            StyledNode {
                styled_node_state: StyledNodeState::new()
            };
            compact_dom.len()
        ];

        // fill out the css property cache: compute the inline properties first so that
        // we can early-return in case the css is empty

        let mut css_property_cache = CssPropertyCache::empty(compact_dom.node_data.len());

        let html_tree =
            construct_html_cascade_tree(
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

        // apply all the styles from the CSS
        let t_restyle = std::time::Instant::now();
        let tag_ids = css_property_cache.restyle(
            &mut css,
            &compact_dom.node_data.as_ref(),
            &node_hierarchy,
            &non_leaf_nodes,
            &html_tree.as_ref(),
        );
        let restyle_ms = t_restyle.elapsed().as_secs_f64() * 1000.0;

        // Apply UA CSS properties to all nodes (lowest priority in cascade)
        // This MUST be done before compute_inherited_values() so that UA CSS
        // properties can be inherited by child nodes (especially text nodes)
        let t_ua = std::time::Instant::now();
        css_property_cache.apply_ua_css(compact_dom.node_data.as_ref().internal);
        // Sort cascaded_props after apply_ua_css() added UA entries
        css_property_cache.sort_cascaded_props();
        let ua_ms = t_ua.elapsed().as_secs_f64() * 1000.0;

        // Compute inherited values for all nodes (resolves em, %, etc.)
        // This must be called after restyle() and apply_ua_css() to ensure
        // CSS properties are available for inheritance
        let t_inherit = std::time::Instant::now();
        css_property_cache.compute_inherited_values(
            node_hierarchy.as_container().internal,
            compact_dom.node_data.as_ref().internal,
        );
        let inherit_ms = t_inherit.elapsed().as_secs_f64() * 1000.0;

        // Build compact layout cache (tier1/2/2b for layout-hot properties)
        // Non-compact properties use the slow cascade path via get_property_slow()
        let t_compact = std::time::Instant::now();
        let compact = css_property_cache.build_compact_cache(
            compact_dom.node_data.as_ref().internal,
        );
        css_property_cache.compact_cache = Some(compact);
        let compact_ms = t_compact.elapsed().as_secs_f64() * 1000.0;

        let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let _ = (compact_dom.len(), restyle_ms, ua_ms, inherit_ms, compact_ms, total_ms);

        // Pre-filter all EventFilter::Window and EventFilter::Not nodes
        // since we need them in the CallbacksOfHitTest::new function
        let nodes_with_window_callbacks = compact_dom
            .node_data
            .as_ref()
            .internal
            .iter()
            .enumerate()
            .filter_map(|(node_id, c)| {
                let node_has_none_callbacks = c.get_callbacks().iter().any(|cb| match cb.event {
                    EventFilter::Window(_) => true,
                    _ => false,
                });
                if node_has_none_callbacks {
                    Some(NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(
                        node_id,
                    ))))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let nodes_with_not_callbacks = compact_dom
            .node_data
            .as_ref()
            .internal
            .iter()
            .enumerate()
            .filter_map(|(node_id, c)| {
                let node_has_none_callbacks = c.get_callbacks().iter().any(|cb| match cb.event {
                    EventFilter::Not(_) => true,
                    _ => false,
                });
                if node_has_none_callbacks {
                    Some(NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(
                        node_id,
                    ))))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // collect nodes with either dataset or callback properties
        let nodes_with_datasets = compact_dom
            .node_data
            .as_ref()
            .internal
            .iter()
            .enumerate()
            .filter_map(|(node_id, c)| {
                if !c.get_callbacks().is_empty() || c.get_dataset().is_some() {
                    Some(NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(
                        node_id,
                    ))))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let mut styled_dom = StyledDom {
            root: NodeHierarchyItemId::from_crate_internal(Some(compact_dom.root)),
            node_hierarchy,
            node_data: compact_dom.node_data.internal.into(),
            cascade_info: html_tree.internal.into(),
            styled_nodes: styled_nodes.into(),
            tag_ids_to_node_ids: tag_ids.into(),
            nodes_with_window_callbacks: nodes_with_window_callbacks.into(),
            nodes_with_not_callbacks: nodes_with_not_callbacks.into(),
            nodes_with_datasets: nodes_with_datasets.into(),
            non_leaf_nodes,
            css_property_cache: CssPropertyCachePtr::new(css_property_cache),
            dom_id: DomId::ROOT_ID, // Will be assigned by layout engine for virtualized views
        };

        // Generate anonymous table elements if needed (CSS 2.2 Section 17.2.1)
        // This must happen after CSS cascade but before layout
        // Anonymous nodes are marked with is_anonymous=true and are skipped by CallbackInfo
        #[cfg(feature = "table_layout")]
        if let Err(_e) = crate::dom_table::generate_anonymous_table_elements(&mut styled_dom) {
            // Warning: Failed to generate anonymous table elements
        }

        styled_dom
    }

    /// Appends another `StyledDom` as a child to the `self.root`
    /// without re-styling the DOM itself
    pub fn append_child(&mut self, mut other: Self) {
        // shift all the node ids in other by self.len()
        let self_len = self.node_hierarchy.as_ref().len();
        let other_len = other.node_hierarchy.as_ref().len();
        let self_root_id = self.root.into_crate_internal().unwrap_or(NodeId::ZERO);
        let other_root_id = other.root.into_crate_internal().unwrap_or(NodeId::ZERO);

        // iterate through the direct root children and adjust the cascade_info
        let current_root_children_count = self_root_id
            .az_children(&self.node_hierarchy.as_container())
            .count();

        other.cascade_info.as_mut()[other_root_id.index()].index_in_parent =
            current_root_children_count as u32;
        other.cascade_info.as_mut()[other_root_id.index()].is_last_child = true;

        self.cascade_info.append(&mut other.cascade_info);

        // adjust node hierarchy
        // Note: 0 means "no node" (None) in the 1-based encoding used by from_usize/into_usize
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
        for tag_id_node_id in other.tag_ids_to_node_ids.iter_mut() {
            tag_id_node_id.node_id.inner += self_len;
        }

        self.tag_ids_to_node_ids
            .append(&mut other.tag_ids_to_node_ids);

        for nid in other.nodes_with_window_callbacks.iter_mut() {
            nid.inner += self_len;
        }
        self.nodes_with_window_callbacks
            .append(&mut other.nodes_with_window_callbacks);

        for nid in other.nodes_with_not_callbacks.iter_mut() {
            nid.inner += self_len;
        }
        self.nodes_with_not_callbacks
            .append(&mut other.nodes_with_not_callbacks);

        for nid in other.nodes_with_datasets.iter_mut() {
            nid.inner += self_len;
        }
        self.nodes_with_datasets
            .append(&mut other.nodes_with_datasets);

        // edge case: if the other StyledDom consists of only one node
        // then it is not a parent itself
        if other_len != 1 {
            for other_non_leaf_node in other.non_leaf_nodes.iter_mut() {
                other_non_leaf_node.node_id.inner += self_len;
                other_non_leaf_node.depth += 1;
            }
            self.non_leaf_nodes.append(&mut other.non_leaf_nodes);
            self.non_leaf_nodes.sort_by(|a, b| a.depth.cmp(&b.depth));
        }
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
        other.cascade_info.as_mut()[other_root_id.index()].index_in_parent = child_index as u32;
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
        for tag_id_node_id in other.tag_ids_to_node_ids.iter_mut() {
            tag_id_node_id.node_id.inner += self_len;
        }

        self.tag_ids_to_node_ids
            .append(&mut other.tag_ids_to_node_ids);

        for nid in other.nodes_with_window_callbacks.iter_mut() {
            nid.inner += self_len;
        }
        self.nodes_with_window_callbacks
            .append(&mut other.nodes_with_window_callbacks);

        for nid in other.nodes_with_not_callbacks.iter_mut() {
            nid.inner += self_len;
        }
        self.nodes_with_not_callbacks
            .append(&mut other.nodes_with_not_callbacks);

        for nid in other.nodes_with_datasets.iter_mut() {
            nid.inner += self_len;
        }
        self.nodes_with_datasets
            .append(&mut other.nodes_with_datasets);

        // edge case: if the other StyledDom consists of only one node
        // then it is not a parent itself
        if other_len != 1 {
            for other_non_leaf_node in other.non_leaf_nodes.iter_mut() {
                other_non_leaf_node.node_id.inner += self_len;
                other_non_leaf_node.depth += 1;
            }
            self.non_leaf_nodes.append(&mut other.non_leaf_nodes);
            // NOTE: Sorting deferred - call finalize_non_leaf_nodes() after all appends
        }
    }

    /// Call this after all append_child_with_index operations are complete
    /// to sort non_leaf_nodes by depth (required for correct rendering)
    pub fn finalize_non_leaf_nodes(&mut self) {
        self.non_leaf_nodes.sort_by(|a, b| a.depth.cmp(&b.depth));
    }

    /// Same as `append_child()`, but as a builder method
    pub fn with_child(mut self, other: Self) -> Self {
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
    pub fn with_context_menu(mut self, context_menu: Menu) -> Self {
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
    pub fn with_menu_bar(mut self, menu_bar: Menu) -> Self {
        self.set_menu_bar(menu_bar);
        self
    }

    /// Re-applies CSS styles to the existing DOM structure.
    pub fn restyle(&mut self, mut css: Css) {
        let new_tag_ids = self.css_property_cache.downcast_mut().restyle(
            &mut css,
            &self.node_data.as_container(),
            &self.node_hierarchy,
            &self.non_leaf_nodes,
            &self.cascade_info.as_container(),
        );

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

        self.tag_ids_to_node_ids = new_tag_ids.into();
    }

    /// Returns the total number of nodes in this StyledDom.
    #[inline]
    pub fn node_count(&self) -> usize {
        self.node_data.len()
    }

    /// Returns an immutable reference to the CSS property cache.
    #[inline]
    pub fn get_css_property_cache<'a>(&'a self) -> &'a CssPropertyCache {
        &*self.css_property_cache.ptr
    }

    /// Returns a mutable reference to the CSS property cache.
    #[inline]
    pub fn get_css_property_cache_mut<'a>(&'a mut self) -> &'a mut CssPropertyCache {
        &mut *self.css_property_cache.ptr
    }

    /// Returns the current state (hover, active, focus) of a styled node.
    #[inline]
    pub fn get_styled_node_state(&self, node_id: &NodeId) -> StyledNodeState {
        self.styled_nodes.as_container()[*node_id]
            .styled_node_state
            .clone()
    }

    /// Scans the display list for all image keys
    pub fn scan_for_image_keys(&self, css_image_cache: &ImageCache) -> FastBTreeSet<ImageRef> {
        use azul_css::props::style::StyleBackgroundContentVec;

        use crate::{dom::NodeType::*, resources::OptionImageMask};

        #[derive(Default)]
        struct ScanImageVec {
            node_type_image: Option<ImageRef>,
            background_image: Vec<ImageRef>,
            clip_mask: Option<ImageRef>,
        }

        let default_backgrounds: StyleBackgroundContentVec = Vec::new().into();

        let images = self
            .node_data
            .as_container()
            .internal
            .iter()
            .enumerate()
            .map(|(node_id, node_data)| {
                let node_id = NodeId::new(node_id);
                let mut v = ScanImageVec::default();

                // If the node has an image content, it needs to be uploaded
                if let Image(id) = node_data.get_node_type() {
                    v.node_type_image = Some(id.clone());
                }

                // If the node has a CSS background image, it needs to be uploaded
                let opt_background_image = self.get_css_property_cache().get_background_content(
                    &node_data,
                    &node_id,
                    &self.styled_nodes.as_container()[node_id].styled_node_state,
                );

                if let Some(style_backgrounds) = opt_background_image {
                    let bos_default = azul_css::css::BoxOrStatic::heap(default_backgrounds.clone());
                    v.background_image = style_backgrounds
                        .get_property()
                        .unwrap_or(&bos_default)
                        .iter()
                        .filter_map(|bg| {
                            use azul_css::props::style::StyleBackgroundContent::*;
                            let css_image_id = match bg {
                                Image(i) => i,
                                _ => return None,
                            };
                            let image_ref = css_image_cache.get_css_image_id(css_image_id)?;
                            Some(image_ref.clone())
                        })
                        .collect();
                }

                // If the node has a clip mask, it needs to be uploaded
                if let Some(clip_mask) = node_data.get_clip_mask() {
                    v.clip_mask = Some(clip_mask.image.clone());
                }

                v
            })
            .collect::<Vec<_>>();

        let mut set = FastBTreeSet::new();

        for scan_image in images.into_iter() {
            if let Some(n) = scan_image.node_type_image {
                set.insert(n);
            }
            if let Some(n) = scan_image.clip_mask {
                set.insert(n);
            }
            for bg in scan_image.background_image {
                set.insert(bg);
            }
        }

        set
    }

    /// Updates hover state for nodes and returns changed CSS properties.
    #[must_use]
    pub fn restyle_nodes_hover(
        &mut self,
        nodes: &[NodeId],
        new_hover_state: bool,
    ) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        // save the old node state
        let old_node_states = nodes
            .iter()
            .map(|nid| {
                self.styled_nodes.as_container()[*nid]
                    .styled_node_state
                    .clone()
            })
            .collect::<Vec<_>>();

        for nid in nodes.iter() {
            self.styled_nodes.as_container_mut()[*nid]
                .styled_node_state
                .hover = new_hover_state;
        }

        let css_property_cache = self.get_css_property_cache();
        let styled_nodes = self.styled_nodes.as_container();
        let node_data = self.node_data.as_container();

        let empty_vec = Vec::new();

        // scan all properties that could have changed because of addition / removal
        let v = nodes
            .iter()
            .zip(old_node_states.iter())
            .filter_map(|(node_id, old_node_state)| {
                let mut keys_normal: Vec<_> = CssPropertyCache::prop_types_for_state(
                    css_property_cache.css_props.get(node_id.index()).unwrap_or(&empty_vec),
                    azul_css::dynamic_selector::PseudoStateType::Hover,
                ).collect();
                let mut keys_inherited: Vec<_> = CssPropertyCache::prop_types_for_state(
                    css_property_cache.cascaded_props.get(node_id.index()).unwrap_or(&empty_vec),
                    azul_css::dynamic_selector::PseudoStateType::Hover,
                ).collect();
                let keys_inline: Vec<CssPropertyType> = {
                    use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
                    node_data[*node_id]
                        .css_props
                        .iter()
                        .filter_map(|prop| {
                            let is_hover = prop.apply_if.as_slice().iter().any(|c| {
                                matches!(c, DynamicSelector::PseudoState(PseudoStateType::Hover))
                            });
                            if is_hover {
                                Some(prop.property.get_type())
                            } else {
                                None
                            }
                        })
                        .collect()
                };
                let mut keys_inline_ref = keys_inline.iter().map(|r| r).collect();

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
                                previous_state: old_node_state.clone(),
                                previous_prop: match old {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
                                current_state: new_node_state.clone(),
                                current_prop: match new {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
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

    /// Updates active state for nodes and returns changed CSS properties.
    #[must_use]
    pub fn restyle_nodes_active(
        &mut self,
        nodes: &[NodeId],
        new_active_state: bool,
    ) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        // save the old node state
        let old_node_states = nodes
            .iter()
            .map(|nid| {
                self.styled_nodes.as_container()[*nid]
                    .styled_node_state
                    .clone()
            })
            .collect::<Vec<_>>();

        for nid in nodes.iter() {
            self.styled_nodes.as_container_mut()[*nid]
                .styled_node_state
                .active = new_active_state;
        }

        let css_property_cache = self.get_css_property_cache();
        let styled_nodes = self.styled_nodes.as_container();
        let node_data = self.node_data.as_container();

        let empty_vec = Vec::new();

        // scan all properties that could have changed because of addition / removal
        let v = nodes
            .iter()
            .zip(old_node_states.iter())
            .filter_map(|(node_id, old_node_state)| {
                let mut keys_normal: Vec<_> = CssPropertyCache::prop_types_for_state(
                    css_property_cache.css_props.get(node_id.index()).unwrap_or(&empty_vec),
                    azul_css::dynamic_selector::PseudoStateType::Active,
                ).collect();

                let mut keys_inherited: Vec<_> = CssPropertyCache::prop_types_for_state(
                    css_property_cache.cascaded_props.get(node_id.index()).unwrap_or(&empty_vec),
                    azul_css::dynamic_selector::PseudoStateType::Active,
                ).collect();

                let keys_inline: Vec<CssPropertyType> = {
                    use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
                    node_data[*node_id]
                        .css_props
                        .iter()
                        .filter_map(|prop| {
                            let is_active = prop.apply_if.as_slice().iter().any(|c| {
                                matches!(c, DynamicSelector::PseudoState(PseudoStateType::Active))
                            });
                            if is_active {
                                Some(prop.property.get_type())
                            } else {
                                None
                            }
                        })
                        .collect()
                };
                let mut keys_inline_ref = keys_inline.iter().map(|r| r).collect();

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
                                previous_state: old_node_state.clone(),
                                previous_prop: match old {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
                                current_state: new_node_state.clone(),
                                current_prop: match new {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
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

    /// Updates focus state for nodes and returns changed CSS properties.
    #[must_use]
    pub fn restyle_nodes_focus(
        &mut self,
        nodes: &[NodeId],
        new_focus_state: bool,
    ) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        
        // save the old node state
        let old_node_states = nodes
            .iter()
            .map(|nid| {
                let state = self.styled_nodes.as_container()[*nid]
                    .styled_node_state
                    .clone();
                state
            })
            .collect::<Vec<_>>();

        for nid in nodes.iter() {
            self.styled_nodes.as_container_mut()[*nid]
                .styled_node_state
                .focused = new_focus_state;
        }

        let css_property_cache = self.get_css_property_cache();
        let styled_nodes = self.styled_nodes.as_container();
        let node_data = self.node_data.as_container();

        let empty_vec = Vec::new();

        // scan all properties that could have changed because of addition / removal
        let v = nodes
            .iter()
            .zip(old_node_states.iter())
            .filter_map(|(node_id, old_node_state)| {
                let mut keys_normal: Vec<_> = CssPropertyCache::prop_types_for_state(
                    css_property_cache.css_props.get(node_id.index()).unwrap_or(&empty_vec),
                    azul_css::dynamic_selector::PseudoStateType::Focus,
                ).collect();
                

                let mut keys_inherited: Vec<_> = CssPropertyCache::prop_types_for_state(
                    css_property_cache.cascaded_props.get(node_id.index()).unwrap_or(&empty_vec),
                    azul_css::dynamic_selector::PseudoStateType::Focus,
                ).collect();
                

                let keys_inline: Vec<CssPropertyType> = {
                    use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
                    node_data[*node_id]
                        .css_props
                        .iter()
                        .filter_map(|prop| {
                            let is_focus = prop.apply_if.as_slice().iter().any(|c| {
                                matches!(c, DynamicSelector::PseudoState(PseudoStateType::Focus))
                            });
                            if is_focus {
                                Some(prop.property.get_type())
                            } else {
                                None
                            }
                        })
                        .collect()
                };
                let mut keys_inline_ref = keys_inline.iter().map(|r| r).collect();

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
                                previous_state: old_node_state.clone(),
                                previous_prop: match old {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
                                current_state: new_node_state.clone(),
                                current_prop: match new {
                                    None => CssProperty::auto(*prop),
                                    Some(s) => s.clone(),
                                },
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
    /// This function synchronizes the StyledNodeState with runtime state
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
        
        let mut result = RestyleResult::default();
        result.gpu_only_changes = true; // Start with GPU-only assumption

        // Helper closure to merge changes and analyze property categories
        let mut process_changes = |changes: BTreeMap<NodeId, Vec<ChangedCssProperty>>| {
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

    /// Overrides CSS properties for a node and returns changed properties.
    // Inserts a property into the self.user_overridden_properties
    #[must_use]
    pub fn restyle_user_property(
        &mut self,
        node_id: &NodeId,
        new_properties: &[CssProperty],
    ) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        let mut map = BTreeMap::default();

        if new_properties.is_empty() {
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

                    let old_prop = match old_prop {
                        None => CssProperty::auto(new_prop.get_type()),
                        Some(s) => s.clone(),
                    };

                    if old_prop == *new_prop {
                        None
                    } else {
                        Some(ChangedCssProperty {
                            previous_state: old_node_state.clone(),
                            previous_prop: old_prop,
                            // overriding a user property does not change the state
                            current_state: old_node_state.clone(),
                            current_prop: new_prop.clone(),
                        })
                    }
                })
                .collect()
        };

        let css_property_cache_mut = self.get_css_property_cache_mut();

        for new_prop in new_properties.iter() {
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

    /// Scans the `StyledDom` for virtualized view callbacks
    pub fn scan_for_virtualized_view_callbacks(&self) -> Vec<NodeId> {
        use crate::dom::NodeType;
        self.node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter_map(|(node_id, node_data)| match node_data.get_node_type() {
                NodeType::VirtualizedView => Some(NodeId::new(node_id)),
                _ => None,
            })
            .collect()
    }

    /// Scans the `StyledDom` for OpenGL callbacks
    pub fn scan_for_gltexture_callbacks(&self) -> Vec<NodeId> {
        use crate::dom::NodeType;
        self.node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter_map(|(node_id, node_data)| {
                use crate::resources::DecodedImage;
                match node_data.get_node_type() {
                    NodeType::Image(image_ref) => {
                        if let DecodedImage::Callback(_) = image_ref.get_data() {
                            Some(NodeId::new(node_id))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .collect()
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
    pub fn get_html_string(&self, custom_head: &str, custom_body: &str, test_mode: bool) -> String {
        let css_property_cache = self.get_css_property_cache();

        let mut output = String::new();

        // After which nodes should a close tag be printed?
        let mut should_print_close_tag_after_node = BTreeMap::new();

        let should_print_close_tag_debug = self
            .non_leaf_nodes
            .iter()
            .filter_map(|p| {
                let parent_node_id = p.node_id.into_crate_internal()?;
                let mut total_last_child = None;
                recursive_get_last_child(
                    parent_node_id,
                    &self.node_hierarchy.as_ref(),
                    &mut total_last_child,
                );
                let total_last_child = total_last_child?;
                Some((parent_node_id, (total_last_child, p.depth)))
            })
            .collect::<BTreeMap<_, _>>();

        for (parent_id, (last_child, parent_depth)) in should_print_close_tag_debug {
            should_print_close_tag_after_node
                .entry(last_child)
                .or_insert_with(|| Vec::new())
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
            let depth = all_node_depths[&node_id];

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

        if !test_mode {
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
        } else {
            output
        }
    }

    /// Returns the node ID of all sub-children of a node
    pub fn get_subtree(&self, parent: NodeId) -> Vec<NodeId> {
        let mut total_last_child = None;
        recursive_get_last_child(parent, &self.node_hierarchy.as_ref(), &mut total_last_child);
        if let Some(last) = total_last_child {
            (parent.index()..=last.index())
                .map(|id| NodeId::new(id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Returns node IDs of all parent nodes in the subtree (nodes with children).
    // Same as get_subtree, but only returns parents
    pub fn get_subtree_parents(&self, parent: NodeId) -> Vec<NodeId> {
        let mut total_last_child = None;
        recursive_get_last_child(parent, &self.node_hierarchy.as_ref(), &mut total_last_child);
        if let Some(last) = total_last_child {
            (parent.index()..=last.index())
                .filter_map(|id| {
                    if self.node_hierarchy.as_ref()[id].last_child_id().is_some() {
                        Some(NodeId::new(id))
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Returns nodes grouped by their rendering order (respects z-index and position).
    pub fn get_rects_in_rendering_order(&self) -> ContentGroup {
        Self::determine_rendering_order(
            &self.non_leaf_nodes.as_ref(),
            &self.node_hierarchy.as_container(),
            &self.styled_nodes.as_container(),
            &self.node_data.as_container(),
            &self.get_css_property_cache(),
        )
    }

    /// Returns the rendering order of the items (the rendering
    /// order doesn't have to be the original order)
    fn determine_rendering_order<'a>(
        non_leaf_nodes: &[ParentWithNodeDepth],
        node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
        styled_nodes: &NodeDataContainerRef<StyledNode>,
        node_data_container: &NodeDataContainerRef<NodeData>,
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

    /// Replaces this StyledDom with default and returns the old value.
    pub fn swap_with_default(&mut self) -> Self {
        let mut new = Self::default();
        core::mem::swap(self, &mut new);
        new
    }

    // Computes the diff between the two DOMs
    // pub fn diff(&self, other: &Self) -> StyledDomDiff { /**/ }
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
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.node_hierarchy.as_ref().len()
    }
}

impl From<Dom> for CompactDom {
    fn from(dom: Dom) -> Self {
        convert_dom_into_compact_dom(dom)
    }
}

/// Converts a tree-based Dom into an arena-based CompactDom for efficient traversal.
pub fn convert_dom_into_compact_dom(mut dom: Dom) -> CompactDom {
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
        node_hierarchy[parent_node_id.index()] = node.clone();

        let copy = dom.root.copy_special();

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
    }

    // Pre-allocate all nodes (+ 1 root node)
    const DEFAULT_NODE_DATA: NodeData = NodeData::create_div();

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

fn sort_children_by_position<'a>(
    parent: NodeId,
    node_hierarchy: &NodeDataContainerRef<'a, NodeHierarchyItem>,
    rectangles: &NodeDataContainerRef<StyledNode>,
    node_data_container: &NodeDataContainerRef<NodeData>,
    css_property_cache: &CssPropertyCache,
) -> Vec<NodeHierarchyItemId> {
    use azul_css::props::layout::LayoutPosition::*;

    let children_positions = parent
        .az_children(node_hierarchy)
        .map(|nid| {
            let position = css_property_cache
                .get_position(
                    &node_data_container[nid],
                    &nid,
                    &rectangles[nid].styled_node_state,
                )
                .and_then(|p| p.clone().get_property_or_default())
                .unwrap_or_default();
            let id = NodeHierarchyItemId::from_crate_internal(Some(nid));
            (id, position)
        })
        .collect::<Vec<_>>();

    let mut not_absolute_children = children_positions
        .iter()
        .filter_map(|(node_id, position)| {
            if *position != Absolute {
                Some(*node_id)
            } else {
                None
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
        None => return,
        Some(s) => {
            *target = Some(s);
            recursive_get_last_child(s, node_hierarchy, target);
        }
    }
}

// ============================================================================
// DOM TRAVERSAL FOR MULTI-NODE SELECTION
// ============================================================================

/// Determine if node_a comes before node_b in document order.
///
/// Document order is defined as pre-order depth-first traversal order.
/// This is equivalent to the order nodes appear in HTML source.
///
/// ## Algorithm
/// 1. Find the path from root to each node
/// 2. Find the Lowest Common Ancestor (LCA)
/// 3. At the divergence point, the child that appears first in sibling order comes first
pub fn is_before_in_document_order(
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
        current = hierarchy.get(node_id).and_then(|h| h.parent_id());
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
/// Vector of NodeIds in document order, from start to end (inclusive)
pub fn collect_nodes_in_document_order(
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
                    child = hierarchy_container.get(child_id).and_then(|h| h.next_sibling_id());
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
pub fn is_layout_equivalent(old: &StyledDom, new: &StyledDom) -> bool {
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
            let old_ids_classes: Vec<_> = old_node.attributes.as_ref().iter()
                .filter(|a| matches!(a, AttributeType::Id(_) | AttributeType::Class(_)))
                .collect();
            let new_ids_classes: Vec<_> = new_node.attributes.as_ref().iter()
                .filter(|a| matches!(a, AttributeType::Id(_) | AttributeType::Class(_)))
                .collect();
            if old_ids_classes != new_ids_classes {
                return false;
            }
        }

        // Compare inline CSS properties (direct layout input)
        if old_node.css_props.as_ref() != new_node.css_props.as_ref() {
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
        if old_node.attributes.as_ref() != new_node.attributes.as_ref() {
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
