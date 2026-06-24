//! Accessibility Manager for integrating with `accesskit`.
//!
//! This module provides the `A11yManager` which:
//!
//! - Maintains the accessibility tree state
//! - Generates `TreeUpdate`s after each layout pass
//! - Handles `ActionRequest`s from assistive technologies
//!
//! The manager translates between Azul's internal DOM representation and
//! the platform-agnostic `accesskit` tree format.

#[cfg(feature = "a11y")]
use std::collections::HashMap;

#[cfg(feature = "a11y")]
use accesskit::{Action, ActionRequest, Node, NodeId as A11yNodeId, Rect, Role, Tree, TreeUpdate};
use azul_core::{
    dom::{
        AccessibilityAction, AccessibilityInfo, AccessibilityRole, AccessibilityState, DomId,
        DomNodeId, NodeData, NodeId, NodeType, TextSelectionStartEnd,
    },
    geom::{LogicalPosition, LogicalSize},
};
use azul_css::AzString;

use crate::{solver3::layout_tree::LayoutNodeHot, window::DomLayoutResult};

/// Cursor/selection info passed to the a11y tree builder.
/// Used to set `text_selection` on contenteditable nodes so screen readers
/// can announce the cursor position and selection range.
#[cfg(feature = "a11y")]
#[derive(Debug, Clone, Copy)]
pub struct CursorA11yInfo {
    pub dom_id: DomId,
    pub node_id: NodeId,
    /// Byte offset of the selection anchor (start of selection, or cursor pos if no range)
    pub anchor_offset: usize,
    /// Byte offset of the selection focus (end of selection, or same as anchor for cursor)
    pub focus_offset: usize,
}

/// Manager for accessibility tree state and updates.
///
/// The `A11yManager` sits within `LayoutWindow` and is responsible for:
///
/// 1. Maintaining the current accessibility tree state
/// 2. Generating `TreeUpdate`s by comparing layout results with the stored tree
/// 3. Translating `ActionRequest`s from screen readers into synthetic Azul events
#[cfg(feature = "a11y")]
pub struct A11yManager {
    /// The root node ID of the accessibility tree (represents the window).
    pub root_id: A11yNodeId,
    /// The current accessibility tree state.
    pub tree: Option<Tree>,
    /// The last generated tree update (for platform adapter consumption).
    pub last_tree_update: Option<TreeUpdate>,
    /// Whether the full tree has been sent to the platform adapter at least once.
    /// After initialization, incremental updates can use `tree: None`.
    pub tree_initialized: bool,
}

#[cfg(feature = "a11y")]
impl Default for A11yManager {
    fn default() -> Self {
        Self::new()
    }
}

impl A11yManager {
    /// Creates a new `A11yManager` with an empty tree containing only a root window node.
    #[must_use] pub const fn new() -> Self {
        let root_id = A11yNodeId(0);
        Self {
            root_id,
            tree: None,
            last_tree_update: None,
            tree_initialized: false,
        }
    }

    /// Updates the accessibility tree based on the current layout state.
    ///
    /// This should be called after each layout pass to synchronize the
    /// accessibility tree with the visual representation.
    #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    #[must_use] pub fn update_tree(
        root_id: A11yNodeId,
        layout_results: &std::collections::BTreeMap<DomId, DomLayoutResult>,
        window_title: &AzString,
        window_size: LogicalSize,
        focused_node: Option<DomNodeId>,
        hidpi_factor: f32,
        dirty_text_overrides: &std::collections::BTreeMap<(DomId, NodeId), String>,
        cursor_info: Option<CursorA11yInfo>,
    ) -> TreeUpdate {
        let mut nodes = Vec::new();
        let mut root_children = Vec::new();

        // Map from (DomId, NodeId) to A11yNodeId for building parent-child relationships
        let mut node_id_map: HashMap<(u32, u32), A11yNodeId> = HashMap::new();

        // Map to collect children for each parent
        let mut parent_children_map: HashMap<A11yNodeId, Vec<A11yNodeId>> = HashMap::new();

        // Create root window node and add it to the nodes list
        let mut root_node = Node::new(Role::Window);
        root_node.set_label(window_title.as_str());
        nodes.push((root_id, root_node));

        for (dom_id, layout_result) in layout_results {
            let styled_dom = &layout_result.styled_dom;
            let node_hierarchy = styled_dom.node_hierarchy.as_ref();
            let node_data_slice = styled_dom.node_data.as_ref();

            // First pass: Create a11y nodes for each DOM node
            for (dom_idx, node_data) in node_data_slice.iter().enumerate() {
                let a11y_info = node_data.get_accessibility_info();

                // Include every node that has a meaningful role.
                // The only types we skip are metadata (Head, Meta, Script, Style, etc.)
                // and pseudo-elements that don't represent real content.
                let should_create_node = a11y_info.is_some()
                    || node_data.is_contenteditable()
                    || node_data.is_focusable()
                    || !matches!(node_data.node_type,
                        NodeType::Head | NodeType::Meta | NodeType::Link
                        | NodeType::Script | NodeType::Style | NodeType::Base
                        | NodeType::Before | NodeType::After | NodeType::Marker
                        | NodeType::Placeholder | NodeType::Source | NodeType::Track
                        | NodeType::Param | NodeType::Col | NodeType::ColGroup
                        | NodeType::Wbr | NodeType::Rp | NodeType::Rtc
                        | NodeType::Bdo | NodeType::Bdi | NodeType::Data
                        | NodeType::Map | NodeType::Area | NodeType::VirtualView
                    );

                if !should_create_node {
                    continue;
                }

                // Generate stable A11yNodeId: offset by 1 to avoid collision with root_id(0)
                let a11y_node_id = Self::encode_a11y_node_id(dom_id.inner, dom_idx);

                // Get layout info: absolute position from calculated_positions,
                // size from layout node. Uses dom_to_layout to map DOM → layout index.
                let dom_node_id = NodeId::new(dom_idx);
                let layout_info = layout_result.layout_tree.dom_to_layout
                    .get(&dom_node_id)
                    .and_then(|indices| indices.first())
                    .and_then(|&layout_idx| {
                        let hot = layout_result.layout_tree.get(layout_idx)?;
                        let abs_pos = layout_result.calculated_positions
                            .get(layout_idx).copied();
                        Some((hot, layout_idx, abs_pos))
                    });

                let a11y_info_ref = a11y_info;
                let mut node = if let Some((layout_node, _layout_idx, abs_pos)) = layout_info {
                    Self::build_node(node_data, layout_node, abs_pos, a11y_info_ref, hidpi_factor, window_size)
                } else {
                    let role = a11y_info_ref.map_or_else(|| Self::node_type_to_role(&node_data.node_type), |info| Self::map_role(&info.role));
                    let mut builder = Node::new(role);
                    if let NodeType::Text(text) = &node_data.node_type {
                        builder.set_label(text.as_str());
                    }
                    builder
                };

                // Collect child text and promote to this node's label or value.
                // Only do this when all children are text nodes — if the node has
                // interactive children (links, buttons, inputs), DON'T set a group
                // label, so VoiceOver navigates into the children individually.
                //
                // For edited contenteditable nodes, dirty_text_overrides has the
                // current text (from the relayout path) instead of the stale
                // StyledDom text.
                {
                    let hierarchy_item = &node_hierarchy[dom_idx];
                    let dom_node_id_key = (*dom_id, NodeId::new(dom_idx));

                    // Use dirty text override if this node was edited since last RefreshDom
                    let (text_content, has_non_text_children) = dirty_text_overrides.get(&dom_node_id_key).map_or_else(|| {
                        let mut text = String::new();
                        let mut has_non_text = false;

                        let mut child = hierarchy_item.first_child_id(NodeId::new(dom_idx));
                        while let Some(child_id) = child {
                            if let Some(child_data) = node_data_slice.get(child_id.index()) {
                                if let NodeType::Text(t) = &child_data.node_type {
                                    if !text.is_empty() { text.push(' '); }
                                    text.push_str(t.as_str());
                                } else {
                                    has_non_text = true;
                                }
                            }
                            if child_id.index() >= node_hierarchy.len() { break; }
                            child = node_hierarchy[child_id.index()].next_sibling_id();
                        }
                        (text, has_non_text)
                    }, |override_text| (override_text.clone(), false));

                    if !text_content.is_empty() {
                        if node_data.is_contenteditable()
                            || matches!(node_data.node_type, NodeType::TextArea | NodeType::Input)
                        {
                            node.set_value(text_content.as_str());
                            // Add text editing actions for contenteditable/input nodes
                            node.add_action(Action::SetTextSelection);
                            node.add_action(Action::ReplaceSelectedText);
                            node.add_action(Action::SetValue);

                            // If cursor/selection is in this node, expose to screen readers
                            if let Some(ref ci) = cursor_info {
                                if ci.dom_id == *dom_id && ci.node_id == NodeId::new(dom_idx) {
                                    let char_lengths: Vec<u8> = text_content.chars()
                                        .map(|c| c.len_utf16() as u8)
                                        .collect();
                                    node.set_character_lengths(char_lengths.clone());

                                    let byte_to_char_idx = |byte_off: usize| -> usize {
                                        text_content
                                            .char_indices()
                                            .take_while(|(b, _)| *b < byte_off)
                                            .count()
                                            .min(char_lengths.len())
                                    };

                                    let anchor_idx = byte_to_char_idx(ci.anchor_offset);
                                    let focus_idx = byte_to_char_idx(ci.focus_offset);

                                    node.set_text_selection(accesskit::TextSelection {
                                        anchor: accesskit::TextPosition {
                                            node: a11y_node_id,
                                            character_index: anchor_idx,
                                        },
                                        focus: accesskit::TextPosition {
                                            node: a11y_node_id,
                                            character_index: focus_idx,
                                        },
                                    });
                                }
                            }
                        } else if !has_non_text_children {
                            // Only promote text when there are NO interactive children.
                            // Otherwise VoiceOver reads the label instead of navigating children.
                            node.set_label(text_content.as_str());
                        }
                    }
                }

                node_id_map.insert((dom_id.inner as u32, dom_idx as u32), a11y_node_id);
                nodes.push((a11y_node_id, node));
            }

            // Second pass: Build parent-child relationships using DOM hierarchy
            for (dom_idx, _) in node_data_slice.iter().enumerate() {
                let a11y_node_id = match node_id_map.get(&(dom_id.inner as u32, dom_idx as u32)) {
                    Some(id) => *id,
                    None => continue,
                };

                let hierarchy_item = &node_hierarchy[dom_idx];

                // Walk up the DOM tree to find the nearest accessible ancestor.
                // parent_id() decodes the 1-based encoding: 0 = None, n+1 = Some(NodeId(n))
                let mut current_parent = hierarchy_item.parent_id();
                let mut accessible_parent_id = None;
                let mut iterations = 0;

                while let Some(parent_node_id) = current_parent {
                    iterations += 1;
                    if iterations > 10_000 { break; }

                    let parent_idx = parent_node_id.index();
                    if let Some(parent_a11y_id) =
                        node_id_map.get(&(dom_id.inner as u32, parent_idx as u32))
                    {
                        accessible_parent_id = Some(*parent_a11y_id);
                        break;
                    }
                    if parent_idx >= node_hierarchy.len() { break; }
                    current_parent = node_hierarchy[parent_idx].parent_id();
                }

                if let Some(parent_id) = accessible_parent_id {
                    parent_children_map
                        .entry(parent_id)
                        .or_default()
                        .push(a11y_node_id);
                } else {
                    root_children.push(a11y_node_id);
                }
            }
        }

        // Third pass: Set children on all nodes (including root)
        for (node_id, node) in &mut nodes {
            if *node_id == root_id {
                // Root window node gets top-level DOM nodes as children
                node.set_children(root_children.clone());
            } else if let Some(children) = parent_children_map.get(node_id) {
                node.set_children(children.clone());
            }
        }

        // Set focus to the currently focused DOM node (from FocusManager).
        // If no node is focused, fall back to the first visible content node.
        // VoiceOver navigates to the focused element on activation.
        let focus = focused_node
            .and_then(|dom_node_id| {
                let dom_idx = dom_node_id.node.into_crate_internal()?.index();
                node_id_map.get(&(dom_node_id.dom.inner as u32, dom_idx as u32)).copied()
            })
            .unwrap_or_else(|| {
                // Fallback: first non-container node
                nodes.iter()
                    .find(|(id, node)| {
                        *id != root_id && !matches!(node.role(), Role::GenericContainer | Role::Window)
                    })
                    .map_or(root_id, |(id, _)| *id)
            });

        // Create the tree update
        

        TreeUpdate {
            nodes,
            tree: Some(Tree::new(root_id)),
            focus,
            tree_id: accesskit::TreeId::ROOT,
        }
    }

    /// Builds an accesskit Node from Azul's `NodeData` and layout information.
    #[allow(clippy::cast_sign_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn build_node(
        node_data: &NodeData,
        layout_node: &LayoutNodeHot,
        abs_pos: Option<LogicalPosition>,
        a11y_info: Option<&AccessibilityInfo>,
        hidpi_factor: f32,
        window_size: LogicalSize,
    ) -> Node {
        // Set role based on NodeType or AccessibilityInfo.
        let role = if node_data.is_contenteditable() {
            Role::MultilineTextInput
        } else if let Some(info) = a11y_info {
            Self::map_role(&info.role)
        } else {
            Self::node_type_to_role(&node_data.node_type)
        };

        let mut builder = Node::new(role);

        // Set HTML tag name for screen readers that use it
        let tag = node_data.node_type.get_path().to_string();
        if !tag.is_empty() {
            builder.set_html_tag(tag.as_str());
        }

        // === Label and Value ===
        // Priority: explicit a11y info > DOM attributes > text content
        if let Some(info) = a11y_info {
            if let Some(name) = info.accessibility_name.as_option() {
                builder.set_label(name.as_str());
            }
            if let Some(value) = info.accessibility_value.as_option() {
                builder.set_value(value.as_str());
            }
            if let Some(desc) = info.description.as_option() {
                builder.set_description(desc.as_str());
            }
        }

        // DOM attribute overrides
        if let Some(label) = node_data.get_accessible_label() {
            builder.set_label(label);
        }
        if let Some(value) = node_data.get_accessible_value() {
            builder.set_value(value);
        }
        // Text node: set as label
        if let NodeType::Text(text) = &node_data.node_type {
            builder.set_label(text.as_str());
        }

        // === States from AccessibilityInfo ===
        if let Some(info) = a11y_info {
            for state in info.states.as_ref() {
                match state {
                    AccessibilityState::Unavailable => { builder.set_disabled(); }
                    AccessibilityState::Readonly => { builder.set_read_only(); }
                    AccessibilityState::CheckedTrue => { builder.set_toggled(accesskit::Toggled::True); }
                    AccessibilityState::CheckedFalse => { builder.set_toggled(accesskit::Toggled::False); }
                    AccessibilityState::Expanded => { builder.set_expanded(true); }
                    AccessibilityState::Collapsed => { builder.set_expanded(false); }
                    AccessibilityState::Focusable => { builder.add_action(Action::Focus); }
                    AccessibilityState::Selected => { builder.set_selected(true); }
                    AccessibilityState::Busy => { builder.set_busy(); }
                    AccessibilityState::Offscreen => { builder.set_hidden(); }
                    _ => {}
                }
            }
        }

        // === Heading level ===
        match &node_data.node_type {
            NodeType::H1 => { builder.set_level(1); }
            NodeType::H2 => { builder.set_level(2); }
            NodeType::H3 => { builder.set_level(3); }
            NodeType::H4 => { builder.set_level(4); }
            NodeType::H5 => { builder.set_level(5); }
            NodeType::H6 => { builder.set_level(6); }
            _ => {}
        }

        // Wire up HTML attributes to accesskit properties
        for attr in node_data.attributes().as_ref() {
            match attr {
                azul_core::dom::AttributeType::AriaLabel(s) => {
                    builder.set_label(s.as_str());
                }
                azul_core::dom::AttributeType::Title(s)
                | azul_core::dom::AttributeType::Alt(s) => {
                    builder.set_description(s.as_str());
                }
                azul_core::dom::AttributeType::Placeholder(s) => {
                    builder.set_placeholder(s.as_str());
                }
                azul_core::dom::AttributeType::Value(s) => {
                    builder.set_value(s.as_str());
                }
                azul_core::dom::AttributeType::Disabled => {
                    builder.set_disabled();
                }
                azul_core::dom::AttributeType::Readonly => {
                    builder.set_read_only();
                }
                azul_core::dom::AttributeType::CheckedTrue => {
                    builder.set_toggled(accesskit::Toggled::True);
                }
                azul_core::dom::AttributeType::CheckedFalse => {
                    builder.set_toggled(accesskit::Toggled::False);
                }
                azul_core::dom::AttributeType::Required => {
                    builder.set_required();
                }
                azul_core::dom::AttributeType::Hidden => {
                    builder.set_hidden();
                }
                azul_core::dom::AttributeType::Lang(s) => {
                    builder.set_language(s.as_str());
                }
                azul_core::dom::AttributeType::ColSpan(n) => {
                    builder.set_column_span(*n as usize);
                }
                azul_core::dom::AttributeType::RowSpan(n) => {
                    builder.set_row_span(*n as usize);
                }
                _ => {}
            }
        }

        // Set bounds: absolute position, offset by padding+border, scaled to physical pixels,
        // clipped to window viewport so VoiceOver highlights don't extend off-screen.
        if let (Some(pos), Some(size)) = (abs_pos, layout_node.used_size) {
            let bp = layout_node.box_props.unpack();
            let pad_left = bp.padding.left + bp.border.left;
            let pad_top = bp.padding.top + bp.border.top;
            let pad_right = bp.padding.right + bp.border.right;
            let pad_bottom = bp.padding.bottom + bp.border.bottom;

            let s = f64::from(hidpi_factor);
            let ww = f64::from(window_size.width) * s;
            let wh = f64::from(window_size.height) * s;

            let x0 = (f64::from(pos.x + pad_left) * s).max(0.0).min(ww);
            let y0 = (f64::from(pos.y + pad_top) * s).max(0.0).min(wh);
            let x1 = (f64::from(pos.x + size.width - pad_right) * s).max(0.0).min(ww);
            let y1 = (f64::from(pos.y + size.height - pad_bottom) * s).max(0.0).min(wh);

            if x1 > x0 && y1 > y0 {
                builder.set_bounds(Rect { x0, y0, x1, y1 });
            }
        }

        // Add supported actions based on the DOM node's own properties.
        // VoiceOver uses these to determine what the user can do with the element.
        if node_data.is_focusable() || node_data.is_contenteditable() {
            builder.add_action(Action::Focus);
        }
        if node_data.has_activation_behavior() {
            builder.add_action(Action::Click);
        }

        // ARIA relations + live-region from AccessibilityInfo. aria-labelledby /
        // aria-describedby reference another node; encode its id the SAME way the
        // tree walk does (encode_a11y_node_id) so the relation resolves to a real
        // node. is_live_region maps to accesskit's Live property. These were all
        // previously dropped (screen readers got no labelled-by/described-by
        // relations and no live-region announcements).
        if let Some(info) = a11y_info {
            if let azul_core::dom::OptionDomNodeId::Some(target) = info.labelled_by {
                if let Some(id) = Self::a11y_node_id_for(&target) {
                    builder.push_labelled_by(id);
                }
            }
            if let azul_core::dom::OptionDomNodeId::Some(target) = info.described_by {
                if let Some(id) = Self::a11y_node_id_for(&target) {
                    builder.push_described_by(id);
                }
            }
            if info.is_live_region {
                builder.set_live(accesskit::Live::Polite);
            }
        }

        builder
    }

    /// Encode a `(DomId.inner, node index)` pair into the stable `A11yNodeId` used
    /// throughout the tree (offset by 1 so it never collides with `root_id` 0).
    /// Shared by the tree walk and the aria-labelledby/-describedby relation
    /// mapping, so a relation always resolves to the node the walk emitted.
    const fn encode_a11y_node_id(dom_inner: usize, node_idx: usize) -> A11yNodeId {
        A11yNodeId(((dom_inner as u64) << 32) | ((node_idx as u64) + 1))
    }

    /// Map an aria-labelledby/-describedby target `DomNodeId` to its `A11yNodeId`,
    /// or `None` if the node id can't be resolved.
    fn a11y_node_id_for(target: &DomNodeId) -> Option<A11yNodeId> {
        let idx = target.node.into_crate_internal()?.index();
        Some(Self::encode_a11y_node_id(target.dom.inner, idx))
    }

    /// Maps an HTML `NodeType` to an accesskit `Role`.
    ///
    /// Every role used here must pass accesskit's `common_filter` (i.e. NOT be
    /// `GenericContainer` or `TextRun`) or `VoiceOver` will skip the node entirely.
    /// Use `Group` for structural containers, `Paragraph` for text blocks, `Label`
    /// for inline text, and semantic roles for everything else.
    // Exhaustive NodeType -> accessibility Role mapping table; many node types share
    // a Role, but one-arm-per-NodeType is intentional for readability/maintainability.
    #[allow(clippy::match_same_arms)]
    const fn node_type_to_role(node_type: &NodeType) -> Role {
        match node_type {
            // === Text content ===
            NodeType::Text(_) => Role::Label,
            NodeType::P => Role::Paragraph,
            NodeType::Pre => Role::Code,
            NodeType::BlockQuote => Role::Blockquote,
            NodeType::Code => Role::Code,
            NodeType::Em | NodeType::I => Role::Emphasis,
            NodeType::Strong | NodeType::B => Role::Strong,
            NodeType::Mark => Role::Mark,
            NodeType::Del => Role::ContentDeletion,
            NodeType::Ins => Role::ContentInsertion,
            NodeType::Abbr | NodeType::Acronym => Role::Abbr,
            NodeType::Q => Role::Blockquote,
            NodeType::Time => Role::Time,
            NodeType::Cite | NodeType::Dfn | NodeType::Var
            | NodeType::Samp | NodeType::Kbd => Role::Label,
            NodeType::Small | NodeType::Big | NodeType::Sub
            | NodeType::Sup | NodeType::U | NodeType::S => Role::Label,
            NodeType::Ruby => Role::Ruby,
            NodeType::Rt => Role::RubyAnnotation,
            NodeType::Br => Role::LineBreak,
            NodeType::Hr => Role::Splitter,

            // === Structural containers ===
            // Group (not GenericContainer) so VoiceOver can navigate into them
            NodeType::Body => Role::Group,
            NodeType::Div => Role::Group,
            NodeType::Span => Role::Group,
            NodeType::Html => Role::Group,

            // === Semantic sections ===
            NodeType::Article => Role::Article,
            NodeType::Section => Role::Section,
            NodeType::Nav => Role::Navigation,
            NodeType::Main => Role::Main,
            NodeType::Header => Role::Header,
            NodeType::Footer => Role::Footer,
            NodeType::Aside => Role::Complementary,
            NodeType::Address => Role::Group,
            NodeType::Figure => Role::Figure,
            NodeType::FigCaption => Role::FigureCaption,
            NodeType::Details => Role::Details,
            NodeType::Summary => Role::DisclosureTriangle,
            NodeType::Dialog => Role::Dialog,

            // === Headings ===
            NodeType::H1 | NodeType::H2 | NodeType::H3
            | NodeType::H4 | NodeType::H5 | NodeType::H6 => Role::Heading,

            // === Lists ===
            NodeType::Ul | NodeType::Ol | NodeType::Dir => Role::List,
            NodeType::Li => Role::ListItem,
            NodeType::Dl => Role::DescriptionList,
            NodeType::Dt => Role::Term,
            NodeType::Dd => Role::Definition,
            NodeType::Menu => Role::Menu,
            NodeType::MenuItem => Role::MenuItem,

            // === Tables ===
            NodeType::Table => Role::Table,
            NodeType::Caption => Role::Caption,
            NodeType::THead | NodeType::TBody | NodeType::TFoot => Role::RowGroup,
            NodeType::Tr => Role::Row,
            NodeType::Th => Role::ColumnHeader,
            NodeType::Td => Role::Cell,
            NodeType::ColGroup | NodeType::Col => Role::GenericContainer,

            // === Forms ===
            NodeType::Form => Role::Form,
            NodeType::FieldSet => Role::Group,
            NodeType::Legend => Role::Legend,
            NodeType::Label => Role::Label,
            NodeType::Input => Role::TextInput,
            NodeType::Button => Role::Button,
            NodeType::Select => Role::ComboBox,
            NodeType::OptGroup => Role::Group,
            NodeType::SelectOption => Role::ListBoxOption,
            NodeType::TextArea => Role::MultilineTextInput,
            NodeType::Output => Role::Status,
            NodeType::Progress => Role::ProgressIndicator,
            NodeType::Meter => Role::Meter,
            NodeType::DataList => Role::ListBox,

            // === Links ===
            NodeType::A => Role::Link,

            // === Embedded content ===
            NodeType::Image(_) => Role::Image,
            NodeType::Icon(_) => Role::Image,
            NodeType::Canvas => Role::Canvas,
            NodeType::Audio => Role::Audio,
            NodeType::Video => Role::Video,
            NodeType::Svg => Role::SvgRoot,
            NodeType::Object | NodeType::Embed => Role::EmbeddedObject,

            // === Everything else: Group (visible to VoiceOver) ===
            _ => Role::Group,
        }
    }

    /// Maps Azul's `AccessibilityRole` to accesskit's Role.
    // Exhaustive AccessibilityRole -> AccessKit Role mapping table (see node_type_to_role).
    #[allow(clippy::match_same_arms)]
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    const fn map_role(role: &AccessibilityRole) -> Role {
        match role {
            AccessibilityRole::TitleBar => Role::TitleBar,
            AccessibilityRole::MenuBar => Role::MenuBar,
            AccessibilityRole::ScrollBar => Role::ScrollBar,
            AccessibilityRole::Grip => Role::Splitter,
            AccessibilityRole::Sound => Role::Audio,
            AccessibilityRole::Cursor => Role::Caret,
            AccessibilityRole::Caret => Role::Caret,
            AccessibilityRole::Alert => Role::Alert,
            AccessibilityRole::Window => Role::Window,
            AccessibilityRole::Client => Role::GenericContainer,
            AccessibilityRole::MenuPopup => Role::Menu,
            AccessibilityRole::MenuItem => Role::MenuItem,
            AccessibilityRole::Tooltip => Role::Tooltip,
            AccessibilityRole::Application => Role::Application,
            AccessibilityRole::Document => Role::Document,
            AccessibilityRole::Pane => Role::Pane,
            AccessibilityRole::Chart => Role::Figure,
            AccessibilityRole::Dialog => Role::Dialog,
            AccessibilityRole::Border => Role::GenericContainer,
            AccessibilityRole::Grouping => Role::Group,
            AccessibilityRole::Separator => Role::GenericContainer,
            AccessibilityRole::Toolbar => Role::Toolbar,
            AccessibilityRole::StatusBar => Role::Status,
            AccessibilityRole::Table => Role::Table,
            AccessibilityRole::ColumnHeader => Role::ColumnHeader,
            AccessibilityRole::RowHeader => Role::RowHeader,
            AccessibilityRole::Column => Role::GenericContainer, // No Column in accesskit 0.17
            AccessibilityRole::Row => Role::Row,
            AccessibilityRole::Cell => Role::Cell,
            AccessibilityRole::Link => Role::Link,
            AccessibilityRole::HelpBalloon => Role::Tooltip,
            AccessibilityRole::Character => Role::GenericContainer,
            AccessibilityRole::List => Role::List,
            AccessibilityRole::ListItem => Role::ListItem,
            AccessibilityRole::Outline => Role::Tree,
            AccessibilityRole::OutlineItem => Role::TreeItem,
            AccessibilityRole::PageTab => Role::Tab,
            AccessibilityRole::PropertyPage => Role::TabPanel,
            AccessibilityRole::Indicator => Role::Meter,
            AccessibilityRole::Graphic => Role::Image,
            // StaticText -> Label in accesskit 0.17
            AccessibilityRole::StaticText => Role::Label,
            AccessibilityRole::Text => Role::TextInput,
            AccessibilityRole::PushButton => Role::Button,
            AccessibilityRole::CheckButton => Role::CheckBox,
            AccessibilityRole::RadioButton => Role::RadioButton,
            AccessibilityRole::ComboBox => Role::ComboBox,
            AccessibilityRole::DropList => Role::ListBox,
            AccessibilityRole::ProgressBar => Role::ProgressIndicator,
            AccessibilityRole::Dial => Role::Meter,
            AccessibilityRole::HotkeyField => Role::TextInput,
            AccessibilityRole::Slider => Role::Slider,
            AccessibilityRole::SpinButton => Role::SpinButton,
            AccessibilityRole::Diagram => Role::Figure,
            AccessibilityRole::Animation => Role::GenericContainer,
            AccessibilityRole::Equation => Role::Math,
            AccessibilityRole::ButtonDropdown => Role::Button,
            // No MenuButton in accesskit 0.17
            AccessibilityRole::ButtonMenu => Role::Button,
            AccessibilityRole::ButtonDropdownGrid => Role::Button,
            AccessibilityRole::Whitespace => Role::GenericContainer,
            AccessibilityRole::PageTabList => Role::TabList,
            AccessibilityRole::Clock => Role::Timer,
            AccessibilityRole::SplitButton => Role::Button,
            AccessibilityRole::IpAddress => Role::TextInput,
            AccessibilityRole::Unknown => Role::Unknown,
            AccessibilityRole::Nothing => Role::GenericContainer,
        }
    }

}

/// Decodes an `A11yNodeId` back into its `(DomId, NodeId)` components.
///
/// The `A11yNodeId` encodes both values in a single u64:
/// - Upper 32 bits: `DomId` (which DOM tree the node belongs to)
/// - Lower 32 bits: `NodeId + 1` (index within that DOM tree, offset by 1 to avoid
///   colliding with the accesskit root node id, matching the encoding in `update_tree`)
#[cfg(feature = "a11y")]
#[must_use] pub const fn decode_a11y_node_id(a11y_node_id: A11yNodeId) -> (DomId, NodeId) {
    let raw = a11y_node_id.0;
    let dom_id = DomId {
        inner: (raw >> 32) as usize,
    };
    let node_id = NodeId::new(((raw & 0xFFFF_FFFF).wrapping_sub(1)) as usize);
    (dom_id, node_id)
}

/// Maps an accesskit `ActionRequest` to an Azul `AccessibilityAction`.
///
/// Returns `None` if the action requires data that was not provided or is invalid.
#[cfg(feature = "a11y")]
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[must_use] pub fn map_accesskit_action(request: ActionRequest) -> Option<AccessibilityAction> {
    use azul_css::{props::basic::FloatValue, AzString};

    let action = match request.action {
        Action::Click => AccessibilityAction::Default,
        Action::Focus => AccessibilityAction::Focus,
        Action::Blur => AccessibilityAction::Blur,
        Action::Collapse => AccessibilityAction::Collapse,
        Action::Expand => AccessibilityAction::Expand,
        Action::ScrollIntoView => AccessibilityAction::ScrollIntoView,
        Action::Increment => AccessibilityAction::Increment,
        Action::Decrement => AccessibilityAction::Decrement,
        Action::ShowContextMenu => AccessibilityAction::ShowContextMenu,
        Action::HideTooltip => AccessibilityAction::HideTooltip,
        Action::ShowTooltip => AccessibilityAction::ShowTooltip,
        Action::ScrollUp => AccessibilityAction::ScrollUp,
        Action::ScrollDown => AccessibilityAction::ScrollDown,
        Action::ScrollLeft => AccessibilityAction::ScrollLeft,
        Action::ScrollRight => AccessibilityAction::ScrollRight,
        Action::SetSequentialFocusNavigationStartingPoint => {
            AccessibilityAction::SetSequentialFocusNavigationStartingPoint
        }
        Action::ReplaceSelectedText => {
            let accesskit::ActionData::Value(value) = request.data? else {
                return None;
            };
            AccessibilityAction::ReplaceSelectedText(AzString::from(value.as_ref()))
        }
        Action::ScrollToPoint => {
            let accesskit::ActionData::ScrollToPoint(point) = request.data? else {
                return None;
            };
            AccessibilityAction::ScrollToPoint(LogicalPosition {
                x: point.x as f32,
                y: point.y as f32,
            })
        }
        Action::SetScrollOffset => {
            let accesskit::ActionData::SetScrollOffset(point) = request.data? else {
                return None;
            };
            AccessibilityAction::SetScrollOffset(LogicalPosition {
                x: point.x as f32,
                y: point.y as f32,
            })
        }
        Action::SetTextSelection => {
            let accesskit::ActionData::SetTextSelection(selection) = request.data? else {
                return None;
            };
            AccessibilityAction::SetTextSelection(TextSelectionStartEnd {
                selection_start: selection.anchor.character_index,
                selection_end: selection.focus.character_index,
            })
        }
        Action::SetValue => match request.data? {
            accesskit::ActionData::Value(value) => {
                AccessibilityAction::SetValue(AzString::from(value.as_ref()))
            }
            accesskit::ActionData::NumericValue(value) => {
                AccessibilityAction::SetNumericValue(FloatValue::new(value as f32))
            }
            _ => return None,
        },
        Action::CustomAction => {
            let accesskit::ActionData::CustomAction(id) = request.data? else {
                return None;
            };
            AccessibilityAction::CustomAction(id)
        }
    };

    Some(action)
}

/// Stub implementation when accessibility feature is disabled.
#[cfg(not(feature = "a11y"))]
pub struct A11yManager {
    _private: (),
}

#[cfg(not(feature = "a11y"))]
impl A11yManager {
    /// Creates a new stub `A11yManager` (no-op when accessibility is disabled).
    pub fn new() -> Self {
        Self { _private: () }
    }
}

#[cfg(all(test, feature = "a11y"))]
mod a11y_relation_tests {
    use super::A11yManager;
    use accesskit::NodeId as A11yNodeId;

    /// The a11y node-id encoding must stay in lockstep with the tree walk:
    /// `(dom.inner << 32) | (idx + 1)`. `labelled_by/described_by` relations encode
    /// their targets the same way, so any drift here would point a relation at
    /// the wrong (or a nonexistent) node.
    #[test]
    fn a11y_node_id_encoding_is_stable_and_offset() {
        assert_eq!(A11yManager::encode_a11y_node_id(0, 0), A11yNodeId(1));
        assert_eq!(A11yManager::encode_a11y_node_id(0, 5), A11yNodeId(6));
        assert_eq!(
            A11yManager::encode_a11y_node_id(2, 3),
            A11yNodeId((2u64 << 32) | 4)
        );
        // Never collides with the root window node (id 0).
        assert_ne!(A11yManager::encode_a11y_node_id(0, 0), A11yNodeId(0));
    }
}
