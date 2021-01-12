use std::{
    fmt,
    collections::BTreeMap
};
use azul_css::{Css, CssPath, RectStyle, RectLayout, CssProperty};
use crate::{
    id_tree::{NodeDataContainerRef, Node, NodeId, NodeHierarchyRef, NodeDataContainerRefMut},
    dom::{Dom, IFrameNode, GlTextureNode, CompactDom, NodeData, TagId, OptionTabIndex},
    style::{
        CascadeInfoVec, construct_html_cascade_tree,
        matches_html_element, apply_style_property, classify_css_path,
    },
};

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub struct ChangedCssProperty {
    pub previous_state: StyledNodeState,
    pub previous_prop: CssProperty,
    pub current_state: StyledNodeState,
    pub current_prop: CssProperty,
}

impl_vec!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_debug!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_partialord!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_clone!(ChangedCssProperty, ChangedCssPropertyVec);
impl_vec_partialeq!(ChangedCssProperty, ChangedCssPropertyVec);

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

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub enum StyledNodeState {
    Uninitialized,
    Normal,
    Hover,
    Active,
    Focused,
}

impl Default for StyledNodeState {
    fn default() -> StyledNodeState { StyledNodeState::Uninitialized }
}

/// A styled Dom node
#[repr(C)]
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct StyledNode {
    /// The CSS constraints, after the cascading step
    pub css_constraints: CascadedCssPropertyWithSourceVec,
    /// The `:hover` CSS constraints that have to be applied on top of the normal css constraints
    pub hover_css_constraints: CascadedCssPropertyWithSourceVec,
    /// The `:active` CSS constraints that have to be applied on top of the `:hover` css constraints
    pub active_css_constraints: CascadedCssPropertyWithSourceVec,
    /// The `:focus` CSS constraints that have to be applied on top of the `:active` + `:hover` css constraints
    pub focus_css_constraints: CascadedCssPropertyWithSourceVec,
    /// Current state of this styled node (used later for caching the style / layout)
    pub state: StyledNodeState,
    /// Optional tag ID
    ///
    /// NOTE: The tag ID has to be adjusted after the layout is done (due to scroll tags)
    pub tag_id: OptionTagId,
    /// Final rect style, after all css constraints are applied
    pub style: RectStyle,
    /// Final rect layout, after all css constraints are applied
    pub layout: RectLayout,
}

impl StyledNode {

    // Returns true if the node needs to be relayouted if it is hovered over
    pub fn needs_hover_relayout(&self) -> bool {
        self.hover_css_constraints.as_ref().iter().any(|css_constraint| css_constraint.prop.get_type().can_trigger_relayout())
    }

    // Returns true if the node needs to be restyled if it is hovered over
    pub fn needs_hover_restyle(&self) -> bool {
        !self.hover_css_constraints.is_empty()
    }

    pub fn needs_active_relayout(&self) -> bool {
        self.needs_hover_relayout() ||
        self.active_css_constraints.as_ref().iter().any(|css_constraint| css_constraint.prop.get_type().can_trigger_relayout())
    }

    pub fn needs_active_restyle(&self) -> bool {
        self.needs_hover_restyle() || !self.active_css_constraints.is_empty()
    }

    pub fn needs_focus_relayout(&self) -> bool {
        self.focus_css_constraints.as_ref().iter().any(|css_constraint| css_constraint.prop.get_type().can_trigger_relayout())
    }

    pub fn needs_focus_restyle(&self) -> bool {
        !self.focus_css_constraints.is_empty()
    }

    /// Adjusts the property only in the `node.style` or `node.layout`, compares the property against
    /// the current value and returns if the css property has changed.
    pub fn restyle_single_property(&mut self, prop: &CssProperty) -> Option<ChangedCssProperty> {
        apply_style_property(&mut self.style, &mut self.layout, prop)
        .map(|prev_prop| {
            ChangedCssProperty {
                previous_state: self.state.clone(),
                previous_prop: prev_prop,
                current_state: self.state.clone(),
                current_prop: prop.clone(),
            }
        })
    }

    pub fn restyle_normal(&mut self) -> ChangedCssPropertyVec {

        let mut changed_css_props = Vec::new();

        if self.state == StyledNodeState::Normal {
            return changed_css_props.into();
        }

        for prop in self.css_constraints.iter() {
            if let Some(prev_prop) = apply_style_property(&mut self.style, &mut self.layout, &prop.prop) {
                changed_css_props.push(ChangedCssProperty {
                    previous_state: self.state.clone(),
                    previous_prop: prev_prop,
                    current_state: StyledNodeState::Normal,
                    current_prop: prop.prop.clone(),
                })
            }
        }

        self.state = StyledNodeState::Normal;
        changed_css_props.into()
    }

    pub fn restyle_hover(&mut self) -> ChangedCssPropertyVec {

        let mut changed_css_props = Vec::new();

        if self.state == StyledNodeState::Hover {
            return changed_css_props.into();
        }

        for prop in self.hover_css_constraints.iter() {
            if let Some(prev_prop) = apply_style_property(&mut self.style, &mut self.layout, &prop.prop) {
                changed_css_props.push(ChangedCssProperty {
                    previous_state: self.state.clone(),
                    previous_prop: prev_prop,
                    current_state: StyledNodeState::Hover,
                    current_prop: prop.prop.clone(),
                })
            }
        }

        self.state = StyledNodeState::Hover;
        changed_css_props.into()

    }

    pub fn restyle_active(&mut self) -> ChangedCssPropertyVec {

        let mut changed_css_props = Vec::new();

        if self.state == StyledNodeState::Active {
            return changed_css_props.into();
        }

        for prop in self.active_css_constraints.iter() {
            if let Some(prev_prop) = apply_style_property(&mut self.style, &mut self.layout, &prop.prop) {
                changed_css_props.push(ChangedCssProperty {
                    previous_state: self.state.clone(),
                    previous_prop: prev_prop,
                    current_state: StyledNodeState::Active,
                    current_prop: prop.prop.clone(),
                })
            }
        }

        self.state = StyledNodeState::Active;
        changed_css_props.into()
    }

    pub fn restyle_focus(&mut self) -> ChangedCssPropertyVec {

        let mut changed_css_props = Vec::new();

        if self.state == StyledNodeState::Focused {
            return changed_css_props.into();
        }

        for prop in self.focus_css_constraints.iter() {
            if let Some(prev_prop) = apply_style_property(&mut self.style, &mut self.layout, &prop.prop) {
                changed_css_props.push(ChangedCssProperty {
                    previous_state: self.state.clone(),
                    previous_prop: prev_prop,
                    current_state: StyledNodeState::Focused,
                    current_prop: prop.prop.clone(),
                })
            }
        }

        self.state = StyledNodeState::Focused;
        changed_css_props.into()
    }
}

impl_vec!(StyledNode, StyledNodeVec);
impl_vec_debug!(StyledNode, StyledNodeVec);
impl_vec_partialord!(StyledNode, StyledNodeVec);
impl_vec_clone!(StyledNode, StyledNodeVec);
impl_vec_partialeq!(StyledNode, StyledNodeVec);

impl StyledNodeVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, StyledNode> {
        NodeDataContainerRef { internal: self.as_ref() }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, StyledNode> {
        NodeDataContainerRefMut { internal: self.as_mut() }
    }

    pub fn restyle_nodes_normal(&mut self, nodes: &[NodeId]) -> Vec<ChangedCssPropertyVec> {
        nodes.iter().filter_map(|node_id| {
            Some(self.as_container_mut().get_mut(*node_id)?.restyle_normal())
        }).collect()
    }

    pub fn restyle_nodes_hover(&mut self, nodes: &[NodeId]) -> Vec<ChangedCssPropertyVec> {
        nodes.iter().filter_map(|node_id| {
            Some(self.as_container_mut().get_mut(*node_id)?.restyle_hover())
        }).collect()
    }

    pub fn restyle_nodes_active(&mut self, nodes: &[NodeId]) -> Vec<ChangedCssPropertyVec> {
        nodes.iter().filter_map(|node_id| {
            Some(self.as_container_mut().get_mut(*node_id)?.restyle_active())
        }).collect()
    }

    pub fn restyle_nodes_focus(&mut self, nodes: &[NodeId]) -> Vec<ChangedCssPropertyVec> {
        nodes.iter().filter_map(|node_id| {
            Some(self.as_container_mut().get_mut(*node_id)?.restyle_focus())
        }).collect()
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

impl_vec!(AzNodeId, NodeIdVec);
impl_vec_debug!(AzNodeId, NodeIdVec);
impl_vec_partialord!(AzNodeId, NodeIdVec);
impl_vec_clone!(AzNodeId, NodeIdVec);
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
    pub first_child: usize,
    pub last_child: usize,
}

impl From<Node> for AzNode {
    fn from(node: Node) -> AzNode {
        AzNode {
            parent: NodeId::into_usize(&node.parent),
            previous_sibling: NodeId::into_usize(&node.previous_sibling),
            next_sibling: NodeId::into_usize(&node.next_sibling),
            first_child: NodeId::into_usize(&node.first_child),
            last_child: NodeId::into_usize(&node.last_child),
        }
    }
}
impl AzNode {
    pub fn parent_id(&self) -> Option<NodeId> { NodeId::from_usize(self.parent) }
    pub fn previous_sibling_id(&self) -> Option<NodeId> { NodeId::from_usize(self.previous_sibling) }
    pub fn next_sibling_id(&self) -> Option<NodeId> { NodeId::from_usize(self.next_sibling) }
    pub fn first_child_id(&self) -> Option<NodeId> { NodeId::from_usize(self.first_child) }
    pub fn last_child_id(&self) -> Option<NodeId> { NodeId::from_usize(self.last_child) }
}

impl_vec!(AzNode, AzNodeVec);
impl_vec_debug!(AzNode, AzNodeVec);
impl_vec_partialord!(AzNode, AzNodeVec);
impl_vec_clone!(AzNode, AzNodeVec);
impl_vec_partialeq!(AzNode, AzNodeVec);

impl AzNodeVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, AzNode> {
        NodeDataContainerRef { internal: self.as_ref() }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, AzNode> {
        NodeDataContainerRefMut { internal: self.as_mut() }
    }
}

impl_vec!(NodeData, NodeDataVec);
impl_vec_debug!(NodeData, NodeDataVec);
impl_vec_partialord!(NodeData, NodeDataVec);
impl_vec_clone!(NodeData, NodeDataVec);
impl_vec_partialeq!(NodeData, NodeDataVec);

impl NodeDataVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, NodeData> {
        NodeDataContainerRef { internal: self.as_ref() }
    }
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, NodeData> {
        NodeDataContainerRefMut { internal: self.as_mut() }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct ParentWithNodeDepth {
    pub depth: usize,
    pub node_id: AzNodeId,
}

impl_vec!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_debug!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_partialord!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_clone!(ParentWithNodeDepth, ParentWithNodeDepthVec);
impl_vec_partialeq!(ParentWithNodeDepth, ParentWithNodeDepthVec);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
#[repr(C)]
pub struct TagIdToNodeIdMapping {
    // Hit-testing tag
    pub tag_id: AzTagId,
    pub node_id: AzNodeId,
    pub tab_index: OptionTabIndex,
}

impl_vec!(TagIdToNodeIdMapping, TagIdsToNodeIdsMappingVec);
impl_vec_debug!(TagIdToNodeIdMapping, TagIdsToNodeIdsMappingVec);
impl_vec_partialord!(TagIdToNodeIdMapping, TagIdsToNodeIdsMappingVec);
impl_vec_clone!(TagIdToNodeIdMapping, TagIdsToNodeIdsMappingVec);
impl_vec_partialeq!(TagIdToNodeIdMapping, TagIdsToNodeIdsMappingVec);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyledDom {
    pub root: AzNodeId,
    pub node_hierarchy: AzNodeVec,
    pub node_data: NodeDataVec,
    pub styled_nodes: StyledNodeVec,
    pub cascade_info: CascadeInfoVec,
    pub tag_ids_to_node_ids: TagIdsToNodeIdsMappingVec,
    pub non_leaf_nodes: ParentWithNodeDepthVec,
    pub rects_in_rendering_order: ContentGroup,
}

impl Default for StyledDom {
    fn default() -> Self {
        StyledDom::new(Dom::body(), Css::empty())
    }
}

impl StyledDom {

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

    pub fn new(dom: Dom, mut css: Css) -> Self {

        use azul_css::CssDeclaration;
        use crate::dom::TabIndex;
        use std::collections::BTreeSet;

        let compact_dom: CompactDom = dom.into();
        let non_leaf_nodes = compact_dom.node_hierarchy.as_ref().get_parents_sorted_by_depth();

        let node_hierarchy: AzNodeVec = compact_dom.node_hierarchy.internal.clone().iter().map(|i| (*i).into()).collect::<Vec<AzNode>>().into();

        // set the tag = is the item focusable or does it have a hit
        let html_tree = construct_html_cascade_tree(&compact_dom.node_hierarchy.as_ref(), &non_leaf_nodes[..]);

        css.sort_by_specificity();

        // First, apply all rules normally (no inheritance) of CSS values
        // This is an O(n^2) operation, but it can be parallelized in the future
        // TODO: can be done in parallel
        let mut styled_nodes = compact_dom.node_data.as_ref().transform(|_, node_id| {

            macro_rules! filter_rules {($styled_node_state:expr) => {{
                if css.is_empty() {
                    Vec::new()
                } else {
                    css.rules()
                    .filter(|rule_block| classify_css_path(&rule_block.path) == $styled_node_state)
                    .filter(|rule_block| matches_html_element(&rule_block.path, node_id, &node_hierarchy.as_container(), &compact_dom.node_data.as_ref(), &html_tree.as_ref()))
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
                    .collect()
                }
            }};}

            let css_rules = filter_rules!(StyledNodeState::Normal);

            // NOTE: This is wrong, but fast
            //
            // Get all nodes that end with `:hover`, `:focus` or `:active`
            // and copy the respective styles to the `hover_css_constraints`, etc. respectively
            //
            // NOTE: This won't work correctly for paths with `.blah:hover > #thing`
            // but that can be fixed later

            // In order to hit-test `:hover` and `:active` selectors, need to insert tags for all rectangles
            // that have a non-:hover path, for example if we have `#thing:hover`, then all nodes selected by `#thing`
            // need to get a TagId, otherwise, they can't be hit-tested.
            let css_hover_rules = filter_rules!(StyledNodeState::Hover);
            let css_active_rules = filter_rules!(StyledNodeState::Active);
            let css_focus_rules = filter_rules!(StyledNodeState::Focused);

            StyledNode {
                css_constraints: css_rules.into(),
                hover_css_constraints: css_hover_rules.into(),
                active_css_constraints: css_active_rules.into(),
                focus_css_constraints: css_focus_rules.into(),
                tag_id: OptionTagId::None,
                state: StyledNodeState::Uninitialized,
                layout: RectLayout::default(),
                style: RectStyle::default(),
            }
        });

        // Then, inherit all values of the parent to the children, but only if the property is
        // inheritable and isn't yet set
        for (_depth, parent_id) in non_leaf_nodes.iter() {

            let inherited_rules =
                styled_nodes.as_ref()[*parent_id].css_constraints
                .iter()
                .filter(|prop| prop.prop.get_type().is_inheritable())
                .cloned()
                .collect::<BTreeSet<CascadedCssPropertyWithSource>>();

            let inherited_hover_rules =
                styled_nodes.as_ref()[*parent_id].hover_css_constraints
                .iter()
                .filter(|prop| prop.prop.get_type().is_inheritable())
                .cloned()
                .collect::<BTreeSet<CascadedCssPropertyWithSource>>();

            let inherited_active_rules =
                styled_nodes.as_ref()[*parent_id].active_css_constraints
                .iter()
                .filter(|prop| prop.prop.get_type().is_inheritable())
                .cloned()
                .collect::<BTreeSet<CascadedCssPropertyWithSource>>();

            let inherited_focus_rules =
                styled_nodes.as_ref()[*parent_id].focus_css_constraints
                .iter()
                .filter(|prop| prop.prop.get_type().is_inheritable())
                .cloned()
                .collect::<BTreeSet<CascadedCssPropertyWithSource>>();

            if inherited_rules.is_empty() &&
               inherited_hover_rules.is_empty() &&
               inherited_active_rules.is_empty() &&
               inherited_focus_rules.is_empty()
            {
                continue;
            }

            for child_id in parent_id.az_children(&node_hierarchy.as_container()) {

                for inherited_rule in &inherited_rules {
                    // Only override the rule if the child already has an inherited rule, don't override it
                    let inherited_rule_type = inherited_rule.prop.get_type();
                    let child_css_constraints = &mut styled_nodes.as_ref_mut()[child_id].css_constraints;

                    if !child_css_constraints.iter().any(|i| i.prop.get_type() == inherited_rule_type) {
                        child_css_constraints.push(inherited_rule.clone());
                    }
                }

                for inherited_rule in &inherited_hover_rules {
                    // Only override the rule if the child already has an inherited rule, don't override it
                    let inherited_rule_type = inherited_rule.prop.get_type();
                    let child_css_constraints = &mut styled_nodes.as_ref_mut()[child_id].hover_css_constraints;

                    if !child_css_constraints.iter().any(|i| i.prop.get_type() == inherited_rule_type) {
                        child_css_constraints.push(inherited_rule.clone());
                    }
                }

                for inherited_rule in &inherited_active_rules {
                    // Only override the rule if the child already has an inherited rule, don't override it
                    let inherited_rule_type = inherited_rule.prop.get_type();
                    let child_css_constraints = &mut styled_nodes.as_ref_mut()[child_id].active_css_constraints;

                    if !child_css_constraints.iter().any(|i| i.prop.get_type() == inherited_rule_type) {
                        child_css_constraints.push(inherited_rule.clone());
                    }
                }

                for inherited_rule in &inherited_focus_rules {
                    // Only override the rule if the child already has an inherited rule, don't override it
                    let inherited_rule_type = inherited_rule.prop.get_type();
                    let child_css_constraints = &mut styled_nodes.as_ref_mut()[child_id].focus_css_constraints;

                    if !child_css_constraints.iter().any(|i| i.prop.get_type() == inherited_rule_type) {
                        child_css_constraints.push(inherited_rule.clone());
                    }
                }
            }
        }

        // Last but not least, apply the inline styles
        // TODO: can be done in parallel
        for (node, node_data) in styled_nodes.internal.iter_mut().zip(compact_dom.node_data.as_ref().iter()) {
            node.css_constraints.extend(
                node_data
                .get_inline_css_props()
                .iter()
                .map(|is|
                    CascadedCssPropertyWithSource {
                        prop: is.clone(),
                        source: CssPropertySource::Inline,
                    }
                )
            );

            node.hover_css_constraints.extend(
                node_data
                .get_inline_hover_css_props()
                .iter()
                .map(|is|
                    CascadedCssPropertyWithSource {
                        prop: is.clone(),
                        source: CssPropertySource::Inline,
                    }
                )
            );

            node.active_css_constraints.extend(
                node_data
                .get_inline_active_css_props()
                .iter()
                .map(|is|
                    CascadedCssPropertyWithSource {
                        prop: is.clone(),
                        source: CssPropertySource::Inline,
                    }
                )
            );

            node.focus_css_constraints.extend(
                node_data
                .get_inline_focus_css_props()
                .iter()
                .map(|is|
                    CascadedCssPropertyWithSource {
                        prop: is.clone(),
                        source: CssPropertySource::Inline,
                    }
                )
            );
        }

        let mut tag_ids = Vec::new();

        // See if the node should have a hit-testing tag ID
        // TODO: can be done in parallel
        compact_dom.node_data.as_ref().iter().enumerate().for_each(|(node_id, node)| {
            let node_id = NodeId::new(node_id);

            let should_auto_insert_tabindex = node.get_callbacks().iter().any(|cb| cb.event.is_focus_callback());
            let tab_index = match node.get_tab_index().into_option() {
                Some(s) => Some(s),
                None => if should_auto_insert_tabindex { Some(TabIndex::Auto) } else { None }
            };

            let styled_node = &mut styled_nodes.as_ref_mut()[node_id];

            let node_has_focus_props = !node.get_inline_focus_css_props().is_empty();
            let node_has_hover_props = !node.get_inline_hover_css_props().is_empty();
            let node_has_active_props = !node.get_inline_active_css_props().is_empty();
            let node_has_not_only_window_callbacks = !node.get_callbacks().is_empty() && !node.get_callbacks().iter().all(|cb| cb.event.is_window_callback());
            let node_has_non_default_cursor = styled_node.style.cursor.as_ref().cloned().is_some();

            let node_should_have_tag =
                tab_index.is_some() ||
                node_has_hover_props ||
                node_has_focus_props ||
                node_has_active_props ||
                node.get_is_draggable() ||
                node_has_not_only_window_callbacks ||
                node_has_non_default_cursor;

            let tag_id = if node_should_have_tag {
                let tag_id = TagId::new();
                tag_ids.push(TagIdToNodeIdMapping {
                    tag_id: AzTagId::from_crate_internal(tag_id),
                    node_id: AzNodeId::from_crate_internal(Some(node_id)),
                    tab_index: tab_index.into(),
                });
                Some(tag_id)
            } else { None };

            styled_node.tag_id = tag_id.map(|z| AzTagId::from_crate_internal(z)).into();

            // Compute the final "normal" style
            let _ = styled_node.restyle_normal(); // since this is the initial restyle, ignore the changed properties
        });

        let non_leaf_nodes = non_leaf_nodes.iter()
        .map(|(depth, node_id)| ParentWithNodeDepth { depth: *depth, node_id: AzNodeId::from_crate_internal(Some(*node_id)) })
        .collect::<Vec<_>>();

        let rects_in_rendering_order = Self::determine_rendering_order(&non_leaf_nodes, &compact_dom.node_hierarchy.as_ref(), &styled_nodes.as_ref());

        StyledDom {
            root: AzNodeId::from_crate_internal(Some(compact_dom.root)),
            node_hierarchy,
            node_data: compact_dom.node_data.internal.into(),
            cascade_info: html_tree.internal.into(),
            styled_nodes: styled_nodes.internal.into(),
            tag_ids_to_node_ids: tag_ids.into(),
            non_leaf_nodes: non_leaf_nodes.into(),
            rects_in_rendering_order,
        }
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
            node.first_child += self_len;
            node.last_child += self_len;
        }

        other.node_hierarchy.as_container_mut()[other_root_id].parent = NodeId::into_usize(&Some(self.root.into_crate_internal().unwrap_or(NodeId::ZERO)));
        self.node_hierarchy.as_container_mut()[last_child_id].next_sibling = self_len + other.root.inner;
        other.node_hierarchy.as_container_mut()[other_root_id].previous_sibling = NodeId::into_usize(&Some(last_child_id));

        self.node_hierarchy.append(&mut other.node_hierarchy);
        self.node_data.append(&mut other.node_data);
        self.styled_nodes.append(&mut other.styled_nodes);

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

    /// Scans the `StyledDom` for iframe callbacks
    pub fn scan_for_iframe_callbacks(&self) -> Vec<(NodeId, &IFrameNode)> {
        use crate::dom::NodeType;
        self.node_data.as_ref().iter().enumerate().filter_map(|(node_id, node_data)| {
            match node_data.get_node_type() {
                NodeType::IFrame(cb) => Some((NodeId::new(node_id), cb)),
                _ => None,
            }
        }).collect()
    }

    /// Scans the `StyledDom` for OpenGL callbacks
    #[cfg(feature = "opengl")]
    pub(crate) fn scan_for_gltexture_callbacks(&self) -> Vec<(NodeId, &GlTextureNode)> {
        use crate::dom::NodeType;
        self.node_data.as_ref().iter().enumerate().filter_map(|(node_id, node_data)| {
            match node_data.get_node_type() {
                NodeType::GlTexture(cb) => Some((NodeId::new(node_id), cb)),
                _ => None,
            }
        }).collect()
    }

    /// Returns the rendering order of the items (the rendering order doesn't have to be the original order)
    fn determine_rendering_order(non_leaf_nodes: &[ParentWithNodeDepth], node_hierarchy: &NodeHierarchyRef, styled_nodes: &NodeDataContainerRef<StyledNode>) -> ContentGroup {

        fn fill_content_group_children(group: &mut ContentGroup, children_sorted: &BTreeMap<AzNodeId, Vec<AzNodeId>>) {
            if let Some(c) = children_sorted.get(&group.root) { // returns None for leaf nodes
                group.children = c
                    .iter()
                    .map(|child| ContentGroup { root: *child, children: Vec::new().into() })
                    .collect();

                for c in group.children.as_mut() {
                    fill_content_group_children(c, children_sorted);
                }
            }
        }

        fn sort_children_by_position(parent: NodeId, node_hierarchy: &NodeHierarchyRef, rectangles: &NodeDataContainerRef<StyledNode>) -> Vec<AzNodeId> {

            use azul_css::LayoutPosition::*;

            let mut not_absolute_children = parent
                .children(node_hierarchy)
                .filter(|id| rectangles[*id].layout.position.as_ref().and_then(|p| p.clone().get_property_or_default()).unwrap_or_default() != Absolute)
                .map(|nid| AzNodeId::from_crate_internal(Some(nid)))
                .collect::<Vec<AzNodeId>>();

            let mut absolute_children = parent
                .children(node_hierarchy)
                .filter(|id| rectangles[*id].layout.position.as_ref().and_then(|p| p.clone().get_property_or_default()).unwrap_or_default() == Absolute)
                .map(|nid| AzNodeId::from_crate_internal(Some(nid)))
                .collect::<Vec<AzNodeId>>();

            // Append the position:absolute children after the regular children
            not_absolute_children.append(&mut absolute_children);
            not_absolute_children
        }

        let children_sorted: BTreeMap<AzNodeId, Vec<AzNodeId>> =
        non_leaf_nodes
            .iter()
            .filter_map(|parent| Some((parent.node_id, sort_children_by_position(parent.node_id.into_crate_internal()?, node_hierarchy, styled_nodes))))
            .collect();

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