use std::collections::BTreeMap;
use azul_css::{Css, CssPath, CssProperty};
use crate::{
    id_tree::{NodeId, NodeDataContainerRef, NodeDataContainerRefMut},
    dom::{NodeData, DomId, NodeId, TagId},
    style::HtmlCascadeInfo,
};

/// In order to support :hover, the element must have a TagId, otherwise it
/// will be disregarded in the hit-testing. A hover group
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct HoverGroup {
    /// Whether any property in the hover group will trigger a re-layout.
    /// This is important for creating
    pub affects_layout: bool,
    /// Whether this path ends with `:active` or with `:hover`
    pub active_or_hover: ActiveHover,
}

impl_option!(HoverGroup, OptionHoverGroup, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

/// Sets whether an element needs to be selected for `:active` or for `:hover`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub enum ActiveHover {
    Active,
    Hover,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub struct CascadedCssPropertyWithSource {
    pub prop: CssProperty,
    pub source: CssPropertySource,
}

impl_vec!(CascadedCssPropertyWithSource, CascadedCssPropertyWithSourceVec);
impl_vec_debug!(CascadedCssPropertyWithSource, CascadedCssPropertyWithSourceVec);
impl_vec_partialord!(CascadedCssPropertyWithSource, CascadedCssPropertyWithSourceVec);
impl_vec_clone!(CascadedCssPropertyWithSource, CascadedCssPropertyWithSourceVec);
impl_vec_partialeq!(CascadedCssPropertyWithSource, CascadedCssPropertyWithSourceVec);

#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub enum CssPropertySource {
    Css(CssPath),
    Inline,
}

/// A styled Dom node
#[repr(C)]
#[derive(Debug, Default, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub struct StyledNode {
    /// The CSS constraints, after the cascading step
    pub css_constraints: CascadedCssPropertyWithSourceVec,
    /// Has all the necessary information about the style CSS path
    pub cascade_info: HtmlCascadeInfo,
    /// In order to hit-test :hover and :active selectors, need to insert tags for all rectangles
    /// that have a non-:hover path, for example if we have `#thing:hover`, then all nodes selected by `#thing`
    /// need to get a TagId, otherwise, they can't be hit-tested.
    pub hover_group: OptionHoverGroup,
    /// Optional tag ID
    ///
    /// NOTE: The tag ID has to be adjusted after the layout is done (due to scroll tags)
    pub tag_id: OptionTagId,
    /// Final rect style, after all css constraints are applied
    pub style: RectStyle,
    /// Final rect layout, after all css constraints are applied
    pub layout: RectLayout,
}

impl_vec!(StyledNode, StyledNodeVec);
impl_vec_debug!(StyledNode, StyledNodeVec);
impl_vec_partialord!(StyledNode, StyledNodeVec);
impl_vec_clone!(StyledNode, StyledNodeVec);
impl_vec_partialeq!(StyledNode, StyledNodeVec);

impl StyledNodeVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, StyledNode> {
        NodeDataContainerRef { inner: self.as_ref() }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, StyledNode> {
        NodeDataContainerRefMut { inner: self.as_mut() }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AzDomId { pub inner: u32 }

impl_option!(AzDomId, OptionDomId, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl AzDomId {
    pub const fn into_crate_internal(&self) -> DomId { DomId(self.inner) }
    pub const fn from_crate_internal(t: DomId) -> Self { AzDomId { inner: t.0 } }
}


#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AzNodeId { pub inner: u32 }

impl_option!(AzNodeId, OptionNodeId, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl AzNodeId {
    #[inline]
    pub const fn into_crate_internal(&self) -> Option<NodeId> {
        match self.inner {
            0 => None,
            i => Some(NodeId::new(i)),
        }
    }

    #[inline]
    pub const fn from_crate_internal(t: Option<NodeId>) -> Self {
        match t {
            Some(nid) => Self { inner: nid.inner.get() },
            None => Self { inner: 0 },
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AzTagId { pub inner: u32 }

impl_option!(AzTagId, OptionTagId, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl AzTagId {
    pub const fn into_crate_internal(&self) -> TagId { TagId(self.inner) }
    pub const fn from_crate_internal(t: TagId) -> Self { AzTagId { inner: t.0 } }
}


#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
#[repr(C)]
pub struct AzNode {
    pub parent: u32,
    pub previous_sibling: u32,
    pub next_sibling: u32,
    pub first_child: u32,
    pub last_child: u32,
}

impl From<Node> for AzNode {
    fn from(node: Node) -> AzNode {
        AzNode {
            parent: NodeId::into_u32(&node.parent),
            previous_sibling: NodeId::into_u32(&node.previous_sibling),
            next_sibling: NodeId::into_u32(&node.next_sibling),
            first_child: NodeId::into_u32(&node.first_child),
            last_child: NodeId::into_u32(&node.last_child),
        }
    }
}
impl AzNode {
    pub fn parent_id(&self) -> Option<NodeId> { NodeId::from_u32(self.parent) }
    pub fn previous_sibling_id(&self) -> Option<NodeId> { NodeId::from_u32(self.previous_sibling) }
    pub fn next_sibling_id(&self) -> Option<NodeId> { NodeId::from_u32(self.next_sibling) }
    pub fn first_child_id(&self) -> Option<NodeId> { NodeId::from_u32(self.first_child) }
    pub fn last_child_id(&self) -> Option<NodeId> { NodeId::from_u32(self.last_child) }
}

impl_vec!(AzNode, AzNodeVec);
impl_vec_debug!(AzNode, AzNodeVec);
impl_vec_partialord!(AzNode, AzNodeVec);
impl_vec_clone!(AzNode, AzNodeVec);
impl_vec_partialeq!(AzNode, AzNodeVec);

impl AzNodeVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, AzNode> {
        NodeDataContainerRef { inner: self.as_ref() }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, AzNode> {
        NodeDataContainerRefMut { inner: self.as_mut() }
    }
}

impl_vec!(NodeData, NodeDataVec);
impl_vec_debug!(NodeData, NodeDataVec);
impl_vec_partialord!(NodeData, NodeDataVec);
impl_vec_clone!(NodeData, NodeDataVec);
impl_vec_partialeq!(NodeData, NodeDataVec);

impl NodeDataVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, NodeData> {
        NodeDataContainerRef { inner: self.as_ref() }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, NodeData> {
        NodeDataContainerRefMut { inner: self.as_mut() }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
#[repr(C)]
pub struct ParentWithNodeDepth {
    pub depth: u32,
    pub node_id: AzNodeId,
}

impl_vec!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_debug!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_partialord!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_clone!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_partialeq!(ParentWithNodeDepth, ParentWithNodeDepthVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
#[repr(C)]
pub struct TagToNodeIdMapping {
    // Hit-testing tag
    pub tag_id: AzTagId,
    pub node_id: AzNodeId,
    pub tab_index: OptionTabIndex,
    pub hover_group: OptionHoverGroup,
}

impl_vec!(TagToNodeIdMapping, TagToNodeIdMappingVec);
impl_vec_debug!(TagToNodeIdMapping, TagToNodeIdMappingVec);
impl_vec_partialord!(TagToNodeIdMapping, TagToNodeIdMappingVec);
impl_vec_clone!(TagToNodeIdMapping, TagToNodeIdMappingVec);
impl_vec_partialeq!(TagToNodeIdMapping, TagToNodeIdMappingVec);

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
#[repr(C)]
pub struct StyleOptions {
    pub focused_node: OptionAzNodeId,
    pub hovered_nodes: AzNodeIdVec,
    pub is_mouse_down: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
#[repr(C)]
pub struct StyledDom {
    pub root: AzNodeId,
    pub node_hierarchy: AzNodeArena,
    pub node_data: NodeDataVec,
    pub styled_nodes: StyledNodeVec,
    pub tag_ids_to_node_ids: TagIdsToNodeIdsMappingVec,
    pub non_leaf_nodes: ParentWithNodeDepthVec,
    pub rects_in_rendering_order: ContentGroup,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ContentGroup {
    /// The parent of the current node group, i.e. either the root node (0)
    /// or the last positioned node ()
    pub root: AzNodeId,
    /// Node ids in order of drawing
    pub children: ContentGroupVec,
}

impl_vec!(ContentGroup, ContentGroupVec);
impl_vec_debug!(ContentGroup, ContentGroupVec);
impl_vec_partialord!(ContentGroup, ContentGroupVec);
impl_vec_clone!(ContentGroup, ContentGroupVec);
impl_vec_partialeq!(ContentGroup, ContentGroupVec);

impl StyledDom {

    pub fn new(dom: Dom, css: &Css, style_options: StyleOptions) -> Self {

        let compact_dom: CompactDom = dom.into();
        let non_leaf_nodes = compact_dom.arena.node_hierarchy.get_parents_sorted_by_depth();

        // set the tag = is the item focusable or does it have a hit
        let html_tree = construct_html_cascade_tree(
            &compact_dom.arena.node_hierarchy,
            &non_leaf_nodes,
            style_options.focused_node.into_option().clone(),
            hovered_nodes.translate(),
            style_options.is_mouse_down,
        );

        // In order to hit-test :hover and :active nodes, need to select them
        // first (to insert their TagId later)
        let hover_groups = match_hover_selectors(
            collect_hover_groups(css),
            &compact_dom.arena.node_hierarchy,
            &compact_dom.arena.node_data,
            &html_tree,
        );

        let mut tag_ids = Vec::new();

        // First, apply all rules normally (no inheritance) of CSS values
        // This is an O(n^2) operation, but it can be parallelized in the future
        let mut styled_nodes = compact_dom.arena.node_data.transform(|node, node_id| {

            let css_rules = css.rules()
            .filter(|rule| matches_html_element(&rule.path, node_id, &ui_state.dom.arena.node_hierarchy, &ui_state.dom.arena.node_data, &html_tree))
            .flat_map(|matched_rule| {
                let matched_path = matched_rule.path.clone();
                matched_rule.declarations.clone().into_iter().map(move |declaration| CascadedCssPropertyWithSource {
                    prop: match declaration {
                        CssDeclaration::Static(s) => s,
                        CssDeclaration::Dynamic(d) => d.default_value, // TODO: No variable support yet!
                    },
                    source: CssPropertySource::Css(matched_path.clone())
                })
            })
            .collect();

            let hover_group = hover_groups.get(node_id);
            let should_auto_insert_tabindex =
                node.callbacks.iter().any(|cb| cb.is_focus_callback()) ||
                node.default_callbacks.iter().any(|cb| cb.is_focus_callback());

            let tab_index = match node.get_tab_index() {
                Some(s) => Some(s),
                None => if should_auto_insert_tabindex { Some(TabIndex::Auto) } else { None }
            };

            let node_has_only_window_callbacks = node.get_callbacks().iter().all(|cb| cb.is_window_callback());

            let node_should_have_tag =
                tab_index.is_some() ||
                hover_group.is_some() ||
                node.get_is_draggable() ||
                !node_has_only_window_callbacks;

            let tag = if node_should_have_tag {
                let tag_id = TagId::new();
                tag_ids.push(TagToNodeIdMapping {
                    tag_id: tag_id,
                    node_id: node_id,
                    tag_index: tab_index.into(),
                    hover_group: hover_group.into(),
                });
                Some(tag_id)
            } else { None };

            StyledNode {
                css_constraints: css_rules,
                cascade_info: html_tree[node_id],
                hover_group: hover_groups.get(node_id),
                tag_id: tag_id.into(),
                hover_group: hover_group.into(),
                tab_index: tab_index.into(),
                rect_layout: RectLayout::default(),
                rect_style: RectStyle::default(),
            }
        });

        // Then, inherit all values of the parent to the children, but only if the property is
        // inheritable and isn't yet set. NOTE: This step can't be parallelized!
        for (_depth, parent_id) in non_leaf_nodes {

            let inherited_rules =
                styled_nodes[parent_id].css_constraints
                .iter()
                .filter(|prop| prop.prop.get_type().is_inheritable())
                .cloned()
                .collect::<BTreeSet<CascadedCssPropertyWithSource>>();

            if inherited_rules.is_empty() {
                continue;
            }

            for child_id in parent_id.children(&ui_state.dom.arena.node_hierarchy) {
                for inherited_rule in &inherited_rules {
                    // Only override the rule if the child already has an inherited rule, don't override it
                    let inherited_rule_type = inherited_rule.prop.get_type();
                    let child_css_constraints = &mut styled_nodes[child_id].css_constraints;

                    if !child_css_constraints.iter().any(|i| i.prop.get_type() == inherited_rule_type) {
                        child_css_constraints.push(inherited_rule.clone());
                    }
                }
            }
        }

        // Last but not least, apply the inline styles
        for node_id in styled_nodes.linear_iter() {
            styled_nodes[node_id].css_constraints.extend(
                ui_state.dom.arena.node_data[node_id]
                .get_inline_css_props()
                .iter()
                .map(|is|
                    CascadedCssPropertyWithSource {
                        prop: is.clone(),
                        source: CssPropertySource::Inline,
                    }
                )
            );
        }

        // After all styles are applied, compute the final style
        compact_dom.arena.node_data.internal.iter_mut().for_each(|node| {
            // calculate the final rect_layout and rect_style
            for prop in node.css_constraints.iter() {
                apply_style_property(&mut node.rect_style, &mut node.rect_layout, prop);
            }
        });

        let non_leaf_nodes = non_leaf_nodes.iter().map(|(depth, node_id)| ParentWithNodeDepth { depth, node_id: node_id.into() }).collect::<Vec<_>>();

        let rects_in_rendering_order = determine_rendering_order(&non_leaf_nodes, &compact_dom.node_hierarchy, &styled_nodes);

        StyledDom {
            root: compact_dom.root,
            node_hierarchy: compact_dom.node_hierarchy.inner.iter().map(Into::into).collect::<Vec<Node>>().into(),
            node_data: compact_dom.node_data.inner.into(),
            styled_nodes: styled_nodes.into(),
            tag_ids_to_node_ids: tag_ids.into(),
            non_leaf_nodes: non_leaf_nodes.into(),
            rects_in_rendering_order,
        }
    }

    /// Appends another `StyledDom` to the `self.root` without re-styling the DOM itself
    pub fn append(&mut self, other: Self) {

        // shift all the node ids in other by self.len()
        let self_len = self.node_hierarchy.len();
        let other_len = other.node_hierarchy.len();
        let self_tag_len = self.tag_ids_to_node_ids.len();

        let last_child = self.node_hierarchy[self.root.inner].last_child;

        self.node_hierarchy[self.root.inner].last_child.inner += other.root.inner;

        for node in other.node_hierarchy.iter_mut() {
            node.parent.inner += self_len;
            node.previous_sibling.inner += self_len;
            node.next_sibling.inner += self_len;
            node.first_child.inner += self_len;
            node.last_child.inner += self_len;
        }

        other.node_hierarchy[other.root_id].parent = self.root_id;
        self.node_hierarchy[last_child].next_sibling.inner = self_len + other.root.inner;
        other.node_hierarchy[other.root_id].previous_sibling = last_child;

        self.node_hierarchy.append(&mut other.node_hierarchy);
        self.node_data.append(&mut other.node_data);
        self.styled_nodes.append(&mut other.styled_nodes);

        for tag_id_node_id in other.tag_ids_to_node_ids {
            tag_id_node_id.tag_id += self_tag_len;
            tag_id_node_id.node_id += self_len;
        }

        self.tag_ids_to_node_ids.append(&mut other.tag_ids_to_node_ids);
        self.non_leaf_nodes = self.node_hierarchy.get_parents_sorted_by_depth().into();
    }

    /// Scans the `StyledDom` for iframe callbacks
    pub(crate) fn scan_for_iframe_callbacks(&self) -> Vec<(NodeId, &IFrameNode)> {
        use crate::dom::NodeType::IFrame;
        self.node_hierarchy.iter().filter_map(|node_id| {
            let node_data = &self.node_data[node_id];
            match node_data.get_node_type() {
                IFrame(cb) => Some((node_id, cb)),
                _ => None,
            }
        }).collect()
    }

    /// Scans the `StyledDom` for OpenGL callbacks
    #[cfg(feature = "opengl")]
    pub(crate) fn scan_for_gltexture_callbacks(&self) -> Vec<(NodeId, &GlTextureNode)> {
        use crate::dom::NodeType::GlTexture;
        self.node_hierarchy.iter().filter_map(|node_id| {
            let node_data = &self.node_data[node_id];
            match node_data.get_node_type() {
                GlTexture(cb) => Some((node_id, cb)),
                _ => None,
            }
        }).collect()
    }

    /// Returns the rendering order of the items (the rendering order doesn't have to be the original order)
    fn determine_rendering_order(non_leaf_nodes: &[ParentWithNodeDepth], node_hierarchy: &NodeHierarchy, styled_nodes: &[StyledNode]) -> ContentGroup {

        fn fill_content_group_children(group: &mut ContentGroup, children_sorted: &BTreeMap<AzNodeId, Vec<AzNodeId>>) {
            if let Some(c) = children_sorted.get(&group.root) { // returns None for leaf nodes
                group.children = c
                    .iter()
                    .map(|child| ContentGroup { root: *child, children: Vec::new() })
                    .collect();

                for c in &mut group.children {
                    fill_content_group_children(c, children_sorted);
                }
            }
        }

        fn sort_children_by_position(parent: AzNodeId, node_hierarchy: &NodeHierarchy, rectangles: &NodeDataContainer<StyledNode>) -> Vec<AzNodeId> {

            use azul_css::LayoutPosition::*;

            let mut not_absolute_children = parent
                .children(node_hierarchy)
                .filter(|id| rectangles[*id].layout.position.and_then(|p| p.get_property_or_default()).unwrap_or_default() != Absolute)
                .collect::<Vec<NodeId>>();

            let mut absolute_children = parent
                .children(node_hierarchy)
                .filter(|id| rectangles[*id].layout.position.and_then(|p| p.get_property_or_default()).unwrap_or_default() == Absolute)
                .collect::<Vec<NodeId>>();

            // Append the position:absolute children after the regular children
            not_absolute_children.append(&mut absolute_children);
            not_absolute_children
        }

        let children_sorted: BTreeMap<AzNodeId, Vec<AzNodeId>> =
        non_leaf_nodes
            .iter()
            .map(|parent| (parent.node_id, sort_children_by_position(parent.node_id, node_hierarchy, styled_nodes)))
            .collect();

        let mut root_content_group = ContentGroup { root: NodeId::ZERO, children: Vec::new() };
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