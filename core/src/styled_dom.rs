use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
};

use azul_css::{
    css::{Css, CssPath},
    parser2::CssApiWrapper,
    props::{
        basic::{StyleFontFamily, StyleFontFamilyVec, StyleFontSize},
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
            StylePerspectiveOriginValue, StyleScrollbarColorValue, StyleTabWidthValue,
            StyleTextAlignValue, StyleTextColorValue, StyleTransformOriginValue,
            StyleTransformVecValue, StyleVisibilityValue, StyleWhiteSpaceValue,
            StyleWordSpacingValue, WidowsValue,
        },
        style::StyleTextColor,
    },
    AzString,
};

use crate::{
    callbacks::{RefAny, Update},
    dom::{
        CompactDom, Dom, NodeData, NodeDataInlineCssProperty, NodeDataVec, OptionTabIndex,
        TabIndex, TagId,
    },
    id::{Node, NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut, NodeId},
    prop_cache::{CssPropertyCache, CssPropertyCachePtr},
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

impl_vec!(
    ChangedCssProperty,
    ChangedCssPropertyVec,
    ChangedCssPropertyVecDestructor
);
impl_vec_debug!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_partialord!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_clone!(
    ChangedCssProperty,
    ChangedCssPropertyVec,
    ChangedCssPropertyVecDestructor
);
impl_vec_partialeq!(ChangedCssProperty, ChangedCssPropertyVec);

#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub enum CssPropertySource {
    Css(CssPath),
    Inline,
}

/// NOTE: multiple states can be active at
///
/// TODO: use bitflags here!
#[repr(C)]
#[derive(Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub struct StyledNodeState {
    pub normal: bool,
    pub hover: bool,
    pub active: bool,
    pub focused: bool,
}

impl core::fmt::Debug for StyledNodeState {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let mut v = Vec::new();
        if self.normal {
            v.push("normal");
        }
        if self.hover {
            v.push("hover");
        }
        if self.active {
            v.push("active");
        }
        if self.focused {
            v.push("focused");
        }
        write!(f, "{:?}", v)
    }
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
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
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
    let dom = Dom::body()
        .with_children(
            vec![Dom::div()
                .with_ids_and_classes(
                    vec![crate::dom::IdOrClass::Id("div1".to_string().into())].into(),
                )
                .with_children(vec![Dom::div()].into())]
            .into(),
        )
        .style(CssApiWrapper { css: css.0 });
    println!(
        "styled dom: {:#?}",
        dom.get_html_string("", "", false)
            .lines()
            .collect::<Vec<_>>()
    );
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
    pub fn new(families: &[StyleFontFamily]) -> Self {
        use highway::{HighwayHash, HighwayHasher, Key};
        let mut hasher = HighwayHasher::new(Key([0; 4]));
        for f in families.iter() {
            f.hash(&mut hasher);
        }
        Self(hasher.finalize64())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct DomId {
    pub inner: usize,
}

impl fmt::Display for DomId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl DomId {
    pub const ROOT_ID: DomId = DomId { inner: 0 };
}

impl Default for DomId {
    fn default() -> DomId {
        DomId::ROOT_ID
    }
}

impl_option!(
    DomId,
    OptionDomId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct NodeHierarchyItemId {
    pub inner: usize,
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
    pub const NONE: NodeHierarchyItemId = NodeHierarchyItemId { inner: 0 };
}

impl_option!(
    NodeHierarchyItemId,
    OptionNodeId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(NodeHierarchyItemId, NodeIdVec, NodeIdVecDestructor);
impl_vec_mut!(NodeHierarchyItemId, NodeIdVec);
impl_vec_debug!(NodeHierarchyItemId, NodeIdVec);
impl_vec_ord!(NodeHierarchyItemId, NodeIdVec);
impl_vec_eq!(NodeHierarchyItemId, NodeIdVec);
impl_vec_hash!(NodeHierarchyItemId, NodeIdVec);
impl_vec_partialord!(NodeHierarchyItemId, NodeIdVec);
impl_vec_clone!(NodeHierarchyItemId, NodeIdVec, NodeIdVecDestructor);
impl_vec_partialeq!(NodeHierarchyItemId, NodeIdVec);

impl NodeHierarchyItemId {
    #[inline]
    pub const fn into_crate_internal(&self) -> Option<NodeId> {
        NodeId::from_usize(self.inner)
    }

    #[inline]
    pub const fn from_crate_internal(t: Option<NodeId>) -> Self {
        Self {
            inner: NodeId::into_usize(&t),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AzTagId {
    pub inner: u64,
}

impl_option!(
    AzTagId,
    OptionTagId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl AzTagId {
    pub const fn into_crate_internal(&self) -> TagId {
        TagId(self.inner)
    }
    pub const fn from_crate_internal(t: TagId) -> Self {
        AzTagId { inner: t.0 }
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

impl NodeHierarchyItem {
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
            parent: NodeId::into_usize(&node.parent),
            previous_sibling: NodeId::into_usize(&node.previous_sibling),
            next_sibling: NodeId::into_usize(&node.next_sibling),
            last_child: NodeId::into_usize(&node.last_child),
        }
    }
}

impl NodeHierarchyItem {
    pub fn parent_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.parent)
    }
    pub fn previous_sibling_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.previous_sibling)
    }
    pub fn next_sibling_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.next_sibling)
    }
    pub fn first_child_id(&self, current_node_id: NodeId) -> Option<NodeId> {
        self.last_child_id().map(|_| current_node_id + 1)
    }
    pub fn last_child_id(&self) -> Option<NodeId> {
        NodeId::from_usize(self.last_child)
    }
}

impl_vec!(
    NodeHierarchyItem,
    NodeHierarchyItemVec,
    NodeHierarchyItemVecDestructor
);
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
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, NodeHierarchyItem> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, NodeHierarchyItem> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

impl<'a> NodeDataContainerRef<'a, NodeHierarchyItem> {
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

impl_vec!(
    ParentWithNodeDepth,
    ParentWithNodeDepthVec,
    ParentWithNodeDepthVecDestructor
);
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
    pub tag_id: AzTagId,
    /// Node ID of the node that has a tag
    pub node_id: NodeHierarchyItemId,
    /// Whether this node has a tab-index field
    pub tab_index: OptionTabIndex,
    /// Parents of this NodeID, sorted in depth order, necessary for efficient hit-testing
    pub parent_node_ids: NodeIdVec,
}

impl_vec!(
    TagIdToNodeIdMapping,
    TagIdToNodeIdMappingVec,
    TagIdToNodeIdMappingVecDestructor
);
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

impl_vec!(ContentGroup, ContentGroupVec, ContentGroupVecDestructor);
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
    pub nodes_with_window_callbacks: NodeIdVec,
    pub nodes_with_not_callbacks: NodeIdVec,
    pub nodes_with_datasets: NodeIdVec,
    pub tag_ids_to_node_ids: TagIdToNodeIdMappingVec,
    pub non_leaf_nodes: ParentWithNodeDepthVec,
    pub css_property_cache: CssPropertyCachePtr,
    /// The ID of this DOM in the layout tree (for multi-DOM support with IFrames)
    pub dom_id: DomId,
}

impl Default for StyledDom {
    fn default() -> Self {
        let root_node: NodeHierarchyItem = Node::ROOT.into();
        let root_node_id: NodeHierarchyItemId =
            NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO));
        Self {
            root: root_node_id,
            node_hierarchy: vec![root_node].into(),
            node_data: vec![NodeData::body()].into(),
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
    // NOTE: After calling this function, the DOM will be reset to an empty DOM.
    // This is for memory optimization, so that the DOM does not need to be cloned.
    //
    // The CSS will be left in-place, but will be re-ordered
    pub fn new(dom: &mut Dom, mut css: CssApiWrapper) -> Self {
        use core::mem;

        use crate::dom::EventFilter;

        let mut swap_dom = Dom::body();

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
                tag_id: OptionTagId::None,
                state: StyledNodeState::new()
            };
            compact_dom.len()
        ];

        // fill out the css property cache: compute the inline properties first so that
        // we can early-return in case the css is empty

        let mut css_property_cache = CssPropertyCache::empty(compact_dom.node_data.len());

        let html_tree =
            construct_html_cascade_tree(&compact_dom.node_hierarchy.as_ref(), &non_leaf_nodes[..]);

        let non_leaf_nodes = non_leaf_nodes
            .iter()
            .map(|(depth, node_id)| ParentWithNodeDepth {
                depth: *depth,
                node_id: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
            })
            .collect::<Vec<_>>();

        let non_leaf_nodes: ParentWithNodeDepthVec = non_leaf_nodes.into();

        // apply all the styles from the CSS
        let tag_ids = css_property_cache.restyle(
            &mut css.css,
            &compact_dom.node_data.as_ref(),
            &node_hierarchy,
            &non_leaf_nodes,
            &html_tree.as_ref(),
        );

        tag_ids
            .iter()
            .filter_map(|tag_id_node_id_mapping| {
                tag_id_node_id_mapping
                    .node_id
                    .into_crate_internal()
                    .map(|node_id| (node_id, tag_id_node_id_mapping.tag_id))
            })
            .for_each(|(nid, tag_id)| {
                styled_nodes[nid.index()].tag_id = OptionTagId::Some(tag_id);
            });

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

        StyledDom {
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
            dom_id: DomId::ROOT_ID, // Will be assigned by layout engine for iframes
        }
    }

    /// Appends another `StyledDom` as a child to the `self.root`
    /// without re-styling the DOM itself
    pub fn append_child(&mut self, mut other: Self) {
        // shift all the node ids in other by self.len()
        let self_len = self.node_hierarchy.as_ref().len();
        let other_len = other.node_hierarchy.as_ref().len();
        let self_tag_len = self.tag_ids_to_node_ids.as_ref().len();
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
        for other in other.node_hierarchy.as_mut().iter_mut() {
            other.parent += self_len;
            other.previous_sibling += if other.previous_sibling == 0 {
                0
            } else {
                self_len
            };
            other.next_sibling += if other.next_sibling == 0 { 0 } else { self_len };
            other.last_child += if other.last_child == 0 { 0 } else { self_len };
        }

        other.node_hierarchy.as_container_mut()[other_root_id].parent =
            NodeId::into_usize(&Some(self_root_id));
        let current_last_child = self.node_hierarchy.as_container()[self_root_id].last_child_id();
        other.node_hierarchy.as_container_mut()[other_root_id].previous_sibling =
            NodeId::into_usize(&current_last_child);
        if let Some(current_last) = current_last_child {
            if self.node_hierarchy.as_container_mut()[current_last]
                .next_sibling_id()
                .is_some()
            {
                self.node_hierarchy.as_container_mut()[current_last].next_sibling +=
                    other_root_id.index() + 1;
            } else {
                self.node_hierarchy.as_container_mut()[current_last].next_sibling =
                    self_len + other_root_id.index() + 1;
            }
        }
        self.node_hierarchy.as_container_mut()[self_root_id].last_child =
            self_len + other_root_id.index() + 1;

        self.node_hierarchy.append(&mut other.node_hierarchy);
        self.node_data.append(&mut other.node_data);
        self.styled_nodes.append(&mut other.styled_nodes);
        self.get_css_property_cache_mut()
            .append(other.get_css_property_cache_mut());

        for tag_id_node_id in other.tag_ids_to_node_ids.iter_mut() {
            tag_id_node_id.tag_id.inner += self_tag_len as u64;
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

    /// Same as `append_child()`, but as a builder method
    pub fn with_child(&mut self, other: Self) -> Self {
        let mut s = self.swap_with_default();
        s.append_child(other);
        s
    }

    pub fn restyle(&mut self, mut css: CssApiWrapper) {
        let new_tag_ids = self.css_property_cache.downcast_mut().restyle(
            &mut css.css,
            &self.node_data.as_container(),
            &self.node_hierarchy,
            &self.non_leaf_nodes,
            &self.cascade_info.as_container(),
        );

        // Restyling may change the tag IDs
        let mut styled_nodes_mut = self.styled_nodes.as_container_mut();

        styled_nodes_mut
            .internal
            .iter_mut()
            .for_each(|styled_node| {
                styled_node.tag_id = None.into();
            });

        new_tag_ids
            .iter()
            .filter_map(|tag_id_node_id_mapping| {
                tag_id_node_id_mapping
                    .node_id
                    .into_crate_internal()
                    .map(|node_id| (node_id, tag_id_node_id_mapping.tag_id))
            })
            .for_each(|(nid, tag_id)| {
                styled_nodes_mut[nid].tag_id = Some(tag_id).into();
            });

        self.tag_ids_to_node_ids = new_tag_ids.into();
    }

    #[inline]
    pub fn node_count(&self) -> usize {
        self.node_data.len()
    }

    #[inline]
    pub fn get_css_property_cache<'a>(&'a self) -> &'a CssPropertyCache {
        &*self.css_property_cache.ptr
    }

    #[inline]
    pub fn get_css_property_cache_mut<'a>(&'a mut self) -> &'a mut CssPropertyCache {
        &mut *self.css_property_cache.ptr
    }

    #[inline]
    pub fn get_styled_node_state(&self, node_id: &NodeId) -> StyledNodeState {
        self.styled_nodes.as_container()[*node_id].state.clone()
    }

    /// Scans the display list for all font IDs + their font size
    pub fn scan_for_font_keys(
        &self,
        resources: &RendererResources,
    ) -> FastHashMap<ImmediateFontId, FastBTreeSet<Au>> {
        use crate::{dom::NodeType::*, resources::font_size_to_au};

        let keys = self
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter_map(|(node_id, node_data)| {
                let node_id = NodeId::new(node_id);
                match node_data.get_node_type() {
                    Text(_) => {
                        let css_font_ids = self.get_css_property_cache().get_font_id_or_default(
                            &node_data,
                            &node_id,
                            &self.styled_nodes.as_container()[node_id].state,
                        );

                        let font_size = self.get_css_property_cache().get_font_size_or_default(
                            &node_data,
                            &node_id,
                            &self.styled_nodes.as_container()[node_id].state,
                        );

                        let style_font_families_hash =
                            StyleFontFamiliesHash::new(css_font_ids.as_ref());

                        let existing_font_key = resources
                            .get_font_family(&style_font_families_hash)
                            .and_then(|font_family_hash| {
                                resources
                                    .get_font_key(&font_family_hash)
                                    .map(|font_key| (font_family_hash, font_key))
                            });

                        let font_id = match existing_font_key {
                            Some((hash, key)) => ImmediateFontId::Resolved((*hash, *key)),
                            None => ImmediateFontId::Unresolved(css_font_ids),
                        };

                        Some((font_id, font_size_to_au(font_size)))
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>();

        let mut map = FastHashMap::default();

        for (font_id, au) in keys.into_iter() {
            map.entry(font_id)
                .or_insert_with(|| FastBTreeSet::default())
                .insert(au);
        }

        map
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
                    &self.styled_nodes.as_container()[node_id].state,
                );

                if let Some(style_backgrounds) = opt_background_image {
                    v.background_image = style_backgrounds
                        .get_property()
                        .unwrap_or(&default_backgrounds)
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

    #[must_use]
    pub fn restyle_nodes_hover(
        &mut self,
        nodes: &[NodeId],
        new_hover_state: bool,
    ) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        // save the old node state
        let old_node_states = nodes
            .iter()
            .map(|nid| self.styled_nodes.as_container()[*nid].state.clone())
            .collect::<Vec<_>>();

        for nid in nodes.iter() {
            self.styled_nodes.as_container_mut()[*nid].state.hover = new_hover_state;
        }

        let css_property_cache = self.get_css_property_cache();
        let styled_nodes = self.styled_nodes.as_container();
        let node_data = self.node_data.as_container();

        let default_map = BTreeMap::default();

        // scan all properties that could have changed because of addition / removal
        let v = nodes
            .iter()
            .zip(old_node_states.iter())
            .filter_map(|(node_id, old_node_state)| {
                let mut keys_normal: Vec<_> = css_property_cache
                    .css_hover_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();
                let mut keys_inherited: Vec<_> = css_property_cache
                    .cascaded_hover_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();
                let keys_inline: Vec<CssPropertyType> = node_data[*node_id]
                    .inline_css_props
                    .iter()
                    .filter_map(|prop| match prop {
                        NodeDataInlineCssProperty::Hover(h) => Some(h.get_type()),
                        _ => None,
                    })
                    .collect();
                let mut keys_inline_ref = keys_inline.iter().map(|r| r).collect();

                keys_normal.append(&mut keys_inherited);
                keys_normal.append(&mut keys_inline_ref);

                let node_properties_that_could_have_changed = keys_normal;

                if node_properties_that_could_have_changed.is_empty() {
                    return None;
                }

                let new_node_state = &styled_nodes[*node_id].state;
                let node_data = &node_data[*node_id];

                let changes = node_properties_that_could_have_changed
                    .into_iter()
                    .filter_map(|prop| {
                        // calculate both the old and the new state
                        let old = css_property_cache.get_property(
                            node_data,
                            node_id,
                            old_node_state,
                            prop,
                        );
                        let new = css_property_cache.get_property(
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

    #[must_use]
    pub fn restyle_nodes_active(
        &mut self,
        nodes: &[NodeId],
        new_active_state: bool,
    ) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        // save the old node state
        let old_node_states = nodes
            .iter()
            .map(|nid| self.styled_nodes.as_container()[*nid].state.clone())
            .collect::<Vec<_>>();

        for nid in nodes.iter() {
            self.styled_nodes.as_container_mut()[*nid].state.active = new_active_state;
        }

        let css_property_cache = self.get_css_property_cache();
        let styled_nodes = self.styled_nodes.as_container();
        let node_data = self.node_data.as_container();

        let default_map = BTreeMap::default();

        // scan all properties that could have changed because of addition / removal
        let v = nodes
            .iter()
            .zip(old_node_states.iter())
            .filter_map(|(node_id, old_node_state)| {
                let mut keys_normal: Vec<_> = css_property_cache
                    .css_active_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();

                let mut keys_inherited: Vec<_> = css_property_cache
                    .cascaded_active_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();

                let keys_inline: Vec<CssPropertyType> = node_data[*node_id]
                    .inline_css_props
                    .iter()
                    .filter_map(|prop| match prop {
                        NodeDataInlineCssProperty::Active(h) => Some(h.get_type()),
                        _ => None,
                    })
                    .collect();
                let mut keys_inline_ref = keys_inline.iter().map(|r| r).collect();

                keys_normal.append(&mut keys_inherited);
                keys_normal.append(&mut keys_inline_ref);

                let node_properties_that_could_have_changed = keys_normal;

                if node_properties_that_could_have_changed.is_empty() {
                    return None;
                }

                let new_node_state = &styled_nodes[*node_id].state;
                let node_data = &node_data[*node_id];

                let changes = node_properties_that_could_have_changed
                    .into_iter()
                    .filter_map(|prop| {
                        // calculate both the old and the new state
                        let old = css_property_cache.get_property(
                            node_data,
                            node_id,
                            old_node_state,
                            prop,
                        );
                        let new = css_property_cache.get_property(
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

    #[must_use]
    pub fn restyle_nodes_focus(
        &mut self,
        nodes: &[NodeId],
        new_focus_state: bool,
    ) -> BTreeMap<NodeId, Vec<ChangedCssProperty>> {
        // save the old node state
        let old_node_states = nodes
            .iter()
            .map(|nid| self.styled_nodes.as_container()[*nid].state.clone())
            .collect::<Vec<_>>();

        for nid in nodes.iter() {
            self.styled_nodes.as_container_mut()[*nid].state.focused = new_focus_state;
        }

        let css_property_cache = self.get_css_property_cache();
        let styled_nodes = self.styled_nodes.as_container();
        let node_data = self.node_data.as_container();

        let default_map = BTreeMap::default();

        // scan all properties that could have changed because of addition / removal
        let v = nodes
            .iter()
            .zip(old_node_states.iter())
            .filter_map(|(node_id, old_node_state)| {
                let mut keys_normal: Vec<_> = css_property_cache
                    .css_focus_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();

                let mut keys_inherited: Vec<_> = css_property_cache
                    .cascaded_focus_props
                    .get(node_id)
                    .unwrap_or(&default_map)
                    .keys()
                    .collect();

                let keys_inline: Vec<CssPropertyType> = node_data[*node_id]
                    .inline_css_props
                    .iter()
                    .filter_map(|prop| match prop {
                        NodeDataInlineCssProperty::Focus(h) => Some(h.get_type()),
                        _ => None,
                    })
                    .collect();
                let mut keys_inline_ref = keys_inline.iter().map(|r| r).collect();

                keys_normal.append(&mut keys_inherited);
                keys_normal.append(&mut keys_inline_ref);

                let node_properties_that_could_have_changed = keys_normal;

                if node_properties_that_could_have_changed.is_empty() {
                    return None;
                }

                let new_node_state = &styled_nodes[*node_id].state;
                let node_data = &node_data[*node_id];

                let changes = node_properties_that_could_have_changed
                    .into_iter()
                    .filter_map(|prop| {
                        // calculate both the old and the new state
                        let old = css_property_cache.get_property(
                            node_data,
                            node_id,
                            old_node_state,
                            prop,
                        );
                        let new = css_property_cache.get_property(
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
        let old_node_state = &node_states[*node_id].state;

        let changes: Vec<ChangedCssProperty> = {
            let css_property_cache = self.get_css_property_cache();

            new_properties
                .iter()
                .filter_map(|new_prop| {
                    let old_prop = css_property_cache.get_property(
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
            if new_prop.is_initial() {
                let mut should_remove_map = false;
                if let Some(map) = css_property_cache_mut
                    .user_overridden_properties
                    .get_mut(node_id)
                {
                    // CssProperty::Initial = remove overridden property
                    map.remove(&new_prop.get_type());
                    should_remove_map = map.is_empty();
                }
                if should_remove_map {
                    css_property_cache_mut
                        .user_overridden_properties
                        .remove(node_id);
                }
            } else {
                css_property_cache_mut
                    .user_overridden_properties
                    .entry(*node_id)
                    .or_insert_with(|| BTreeMap::new())
                    .insert(new_prop.get_type(), new_prop.clone());
            }
        }

        if !changes.is_empty() {
            map.insert(*node_id, changes);
        }

        map
    }

    /// Scans the `StyledDom` for iframe callbacks
    pub fn scan_for_iframe_callbacks(&self) -> Vec<NodeId> {
        use crate::dom::NodeType;
        self.node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter_map(|(node_id, node_data)| match node_data.get_node_type() {
                NodeType::IFrame(_) => Some(NodeId::new(node_id)),
                _ => None,
            })
            .collect()
    }

    /// Scans the `StyledDom` for OpenGL callbacks
    pub(crate) fn scan_for_gltexture_callbacks(&self) -> Vec<NodeId> {
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
            let node_state = &self.styled_nodes.as_container()[node_id].state;
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

    pub fn swap_with_default(&mut self) -> Self {
        let mut new = Self::default();
        core::mem::swap(self, &mut new);
        new
    }

    // Computes the diff between the two DOMs
    // pub fn diff(&self, other: &Self) -> StyledDomDiff { /**/ }
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
                .get_position(&node_data_container[nid], &nid, &rectangles[nid].state)
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
