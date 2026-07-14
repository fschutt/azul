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
#[derive(Debug)]
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

#[cfg(feature = "a11y")]
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
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    #[must_use] pub fn update_tree(
        root_id: A11yNodeId,
        layout_results: &std::collections::BTreeMap<DomId, DomLayoutResult>,
        scroll_manager: &crate::managers::scroll_state::ScrollManager,
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

                // MWA-B10: advertise the scroll surface. The INBOUND handler
                // (LayoutWindow::process_accessibility_action) has handled
                // ScrollUp/Down/Left/Right/SetScrollOffset/ScrollIntoView all
                // along — but the tree never declared any scroll action or
                // offset, so screen readers had nothing to invoke.
                if let Some((offset, max_x, max_y)) =
                    scroll_manager.a11y_scroll_info(*dom_id, NodeId::new(dom_idx))
                {
                    node.set_scroll_x(f64::from(offset.x));
                    node.set_scroll_x_min(0.0);
                    node.set_scroll_x_max(f64::from(max_x));
                    node.set_scroll_y(f64::from(offset.y));
                    node.set_scroll_y_min(0.0);
                    node.set_scroll_y_max(f64::from(max_y));
                    node.set_clips_children();
                    if max_y > 0.0 {
                        node.add_action(Action::ScrollUp);
                        node.add_action(Action::ScrollDown);
                    }
                    if max_x > 0.0 {
                        node.add_action(Action::ScrollLeft);
                        node.add_action(Action::ScrollRight);
                    }
                    node.add_action(Action::SetScrollOffset);
                }

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

    /// MWA-B10: outbound twin of `map_accesskit_action` — declares a node's
    /// supported actions in the tree (payload-carrying variants map to their
    /// action KIND; the payload only exists on inbound requests).
    const fn map_action_to_accesskit(action: &AccessibilityAction) -> Action {
        use azul_core::a11y::AccessibilityAction as A;
        match action {
            A::Default => Action::Click,
            A::Focus => Action::Focus,
            A::Blur => Action::Blur,
            A::Collapse => Action::Collapse,
            A::Expand => Action::Expand,
            A::ScrollIntoView => Action::ScrollIntoView,
            A::Increment => Action::Increment,
            A::Decrement => Action::Decrement,
            A::ShowContextMenu => Action::ShowContextMenu,
            A::HideTooltip => Action::HideTooltip,
            A::ShowTooltip => Action::ShowTooltip,
            A::ScrollUp => Action::ScrollUp,
            A::ScrollDown => Action::ScrollDown,
            A::ScrollLeft => Action::ScrollLeft,
            A::ScrollRight => Action::ScrollRight,
            A::ReplaceSelectedText(_) => Action::ReplaceSelectedText,
            A::ScrollToPoint(_) => Action::ScrollToPoint,
            A::SetScrollOffset(_) => Action::SetScrollOffset,
            A::SetTextSelection(_) => Action::SetTextSelection,
            A::SetSequentialFocusNavigationStartingPoint => {
                Action::SetSequentialFocusNavigationStartingPoint
            }
            A::SetValue(_) | A::SetNumericValue(_) => Action::SetValue,
            A::CustomAction(_) => Action::CustomAction,
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

        // MWA-B10: declare user-supplied supported actions — the public
        // AccessibilityInfo.supported_actions field was never read, so
        // API-declared actions never reached assistive technology.
        if let Some(info) = a11y_info {
            for action in info.supported_actions.as_ref() {
                builder.add_action(Self::map_action_to_accesskit(action));
            }
        }

        // MWA-B10: every content node can be scrolled into view — the
        // inbound handler implements it; declaring it lets screen readers
        // use it for navigation.
        builder.add_action(Action::ScrollIntoView);

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

        // MWA-C-a11y: aria-live="polite|assertive" HTML attribute — arrives
        // as AriaProperty/Custom (no parsing existed; live regions were only
        // reachable through the explicit AccessibilityInfo.is_live_region
        // flag, so HTML-defined live regions never announced).
        for attr in node_data.attributes() {
            let (name, value) = match attr {
                azul_core::dom::AttributeType::AriaProperty(nv)
                | azul_core::dom::AttributeType::Custom(nv) => {
                    (nv.attr_name.as_str(), nv.value.as_str())
                }
                _ => continue,
            };
            if name.eq_ignore_ascii_case("aria-live") {
                match value.to_ascii_lowercase().as_str() {
                    "polite" => builder.set_live(accesskit::Live::Polite),
                    "assertive" => builder.set_live(accesskit::Live::Assertive),
                    _ => builder.set_live(accesskit::Live::Off),
                }
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
#[derive(Debug)]
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

#[cfg(all(test, feature = "a11y"))]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use std::collections::BTreeMap;

    use accesskit::{ActionData, Live, Point, TextPosition, TextSelection, Toggled, TreeId};
    use azul_core::{
        dom::{
            AttributeNameValue, AttributeType, FormattingContext, OptionDomNodeId,
        },
        styled_dom::NodeHierarchyItemId,
        window::OptionVirtualKeyCodeCombo,
    };
    use azul_css::{css::BoxOrStatic, props::basic::FloatValue, OptionString};

    use super::*;
    use crate::{managers::scroll_state::ScrollManager, solver3::geometry::PackedBoxProps};

    // ---------------------------------------------------------------------
    // fixtures
    // ---------------------------------------------------------------------

    /// A `LayoutNodeHot` with the given used size and packed box props.
    /// `PackedBoxProps` edges are `[top, right, bottom, left]` in tenths of a pixel.
    fn hot(used_size: Option<LogicalSize>, padding: [i16; 4], border: [i16; 4]) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: PackedBoxProps {
                padding,
                border,
                ..PackedBoxProps::default()
            },
            dom_node_id: Some(NodeId::new(0)),
            used_size,
            formatting_context: FormattingContext::Block {
                establishes_new_context: false,
            },
            parent: None,
        }
    }

    fn plain_hot() -> LayoutNodeHot {
        hot(Some(LogicalSize::new(100.0, 50.0)), [0; 4], [0; 4])
    }

    fn info(role: AccessibilityRole) -> AccessibilityInfo {
        AccessibilityInfo {
            accessibility_name: OptionString::None,
            accessibility_value: OptionString::None,
            description: OptionString::None,
            accelerator: OptionVirtualKeyCodeCombo::None,
            default_action: OptionString::None,
            states: Vec::<AccessibilityState>::new().into(),
            supported_actions: Vec::<AccessibilityAction>::new().into(),
            labelled_by: OptionDomNodeId::None,
            described_by: OptionDomNodeId::None,
            role,
            is_live_region: false,
        }
    }

    fn dom_node(dom: usize, idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: dom },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    fn text_node(s: &str) -> NodeData {
        NodeData::create_node(NodeType::Text(BoxOrStatic::heap(AzString::from(s))))
    }

    fn request(action: Action, data: Option<ActionData>) -> ActionRequest {
        ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: A11yNodeId(1),
            data,
        }
    }

    /// `update_tree` with no DOMs at all — the smallest legal input.
    fn empty_update(
        window_size: LogicalSize,
        focused_node: Option<DomNodeId>,
        hidpi_factor: f32,
        title: &str,
    ) -> TreeUpdate {
        let layout_results = BTreeMap::new();
        let scroll_manager = ScrollManager::new();
        let overrides = BTreeMap::new();
        A11yManager::update_tree(
            A11yNodeId(0),
            &layout_results,
            &scroll_manager,
            &AzString::from(title),
            window_size,
            focused_node,
            hidpi_factor,
            &overrides,
            None,
        )
    }

    // ---------------------------------------------------------------------
    // A11yManager::new / Default (constructor)
    // ---------------------------------------------------------------------

    #[test]
    fn new_starts_with_root_zero_and_uninitialized_tree() {
        let manager = A11yManager::new();
        assert_eq!(manager.root_id, A11yNodeId(0));
        assert!(manager.tree.is_none());
        assert!(manager.last_tree_update.is_none());
        assert!(
            !manager.tree_initialized,
            "a fresh manager must force a full first TreeUpdate"
        );
    }

    #[test]
    fn default_matches_new() {
        let a = A11yManager::new();
        let b = A11yManager::default();
        assert_eq!(a.root_id, b.root_id);
        assert_eq!(a.tree_initialized, b.tree_initialized);
        assert!(b.tree.is_none() && b.last_tree_update.is_none());
    }

    // ---------------------------------------------------------------------
    // encode_a11y_node_id / decode_a11y_node_id (numeric + round-trip)
    // ---------------------------------------------------------------------

    #[test]
    fn encode_decode_round_trips_over_the_representable_domain() {
        // node_idx must stay < u32::MAX so `idx + 1` cannot carry out of the
        // low 32 bits; dom_inner must stay <= u32::MAX so it cannot shift out.
        let cases: [(usize, usize); 7] = [
            (0, 0),
            (0, 1),
            (1, 0),
            (2, 3),
            (0, u32::MAX as usize - 1),
            (u32::MAX as usize, 0),
            (u32::MAX as usize, u32::MAX as usize - 1),
        ];
        for (dom, idx) in cases {
            let encoded = A11yManager::encode_a11y_node_id(dom, idx);
            let (decoded_dom, decoded_node) = decode_a11y_node_id(encoded);
            assert_eq!(decoded_dom.inner, dom, "dom round-trip for ({dom}, {idx})");
            assert_eq!(decoded_node.index(), idx, "idx round-trip for ({dom}, {idx})");
        }
    }

    #[test]
    fn encoded_ids_never_collide_with_the_root_window_id() {
        // root_id is 0; the +1 offset is the only thing keeping node 0 of dom 0
        // from being mistaken for the window itself.
        for (dom, idx) in [(0usize, 0usize), (0, 1), (5, 0), (u32::MAX as usize, 0)] {
            assert_ne!(
                A11yManager::encode_a11y_node_id(dom, idx),
                A11yNodeId(0),
                "({dom}, {idx}) must not encode to the root id"
            );
        }
    }

    #[test]
    fn encode_is_injective_for_neighbouring_ids() {
        let a = A11yManager::encode_a11y_node_id(0, 1);
        let b = A11yManager::encode_a11y_node_id(1, 0);
        let c = A11yManager::encode_a11y_node_id(0, 2);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    /// Characterisation of a known-lossy boundary: `idx + 1` carries out of the
    /// low 32 bits at `idx == u32::MAX`, silently incrementing the DomId field.
    /// Not reachable with real DOMs (a 4-billion-node tree), but pinned so the
    /// encoding contract can't drift unnoticed. Does not panic.
    #[test]
    fn encode_node_idx_at_u32_max_carries_into_the_dom_field() {
        let encoded = A11yManager::encode_a11y_node_id(0, u32::MAX as usize);
        assert_eq!(encoded, A11yNodeId(1u64 << 32));
        let (dom, node) = decode_a11y_node_id(encoded);
        assert_eq!(dom.inner, 1, "dom 0 aliases onto dom 1 at the carry");
        assert_eq!(node.index(), usize::MAX);
        // Still never collides with the root window node.
        assert_ne!(encoded, A11yNodeId(0));
    }

    /// Decoding the root id itself is nonsense (root is not a DOM node), but it
    /// must not panic: the `wrapping_sub(1)` yields the usize::MAX sentinel.
    #[test]
    fn decode_of_root_id_wraps_instead_of_panicking() {
        let (dom, node) = decode_a11y_node_id(A11yNodeId(0));
        assert_eq!(dom.inner, 0);
        assert_eq!(node.index(), usize::MAX);
    }

    #[test]
    fn decode_of_u64_max_does_not_panic() {
        let (dom, node) = decode_a11y_node_id(A11yNodeId(u64::MAX));
        assert_eq!(dom.inner, u32::MAX as usize);
        assert_eq!(node.index(), u32::MAX as usize - 1);
    }

    // ---------------------------------------------------------------------
    // a11y_node_id_for
    // ---------------------------------------------------------------------

    #[test]
    fn a11y_node_id_for_resolves_to_the_same_id_the_tree_walk_emits() {
        let target = dom_node(2, 3);
        assert_eq!(
            A11yManager::a11y_node_id_for(&target),
            Some(A11yManager::encode_a11y_node_id(2, 3)),
            "a relation must resolve to the node the walk emitted"
        );
    }

    #[test]
    fn a11y_node_id_for_returns_none_for_the_none_sentinel() {
        let target = DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::NONE,
        };
        assert_eq!(A11yManager::a11y_node_id_for(&target), None);
    }

    #[test]
    fn a11y_node_id_for_handles_extreme_dom_ids_without_panicking() {
        let target = dom_node(u32::MAX as usize, 0);
        assert_eq!(
            A11yManager::a11y_node_id_for(&target),
            Some(A11yNodeId((u32::MAX as u64) << 32 | 1))
        );
    }

    // ---------------------------------------------------------------------
    // node_type_to_role
    // ---------------------------------------------------------------------

    #[test]
    fn node_type_to_role_maps_the_documented_roles() {
        let cases: [(NodeType, Role); 18] = [
            (NodeType::Text(BoxOrStatic::heap(AzString::from("x"))), Role::Label),
            (NodeType::P, Role::Paragraph),
            (NodeType::Div, Role::Group),
            (NodeType::Body, Role::Group),
            (NodeType::A, Role::Link),
            (NodeType::Button, Role::Button),
            (NodeType::H1, Role::Heading),
            (NodeType::H6, Role::Heading),
            (NodeType::Input, Role::TextInput),
            (NodeType::TextArea, Role::MultilineTextInput),
            (NodeType::Table, Role::Table),
            (NodeType::Tr, Role::Row),
            (NodeType::Td, Role::Cell),
            (NodeType::Th, Role::ColumnHeader),
            (NodeType::Ul, Role::List),
            (NodeType::Li, Role::ListItem),
            (NodeType::Br, Role::LineBreak),
            (NodeType::Hr, Role::Splitter),
        ];
        for (node_type, expected) in cases {
            assert_eq!(
                A11yManager::node_type_to_role(&node_type),
                expected,
                "{node_type:?}"
            );
        }
    }

    #[test]
    fn node_type_to_role_falls_back_to_group_for_unmapped_types() {
        // The `_ => Role::Group` arm: metadata types that never reach the tree
        // must still produce a VoiceOver-visible role rather than a filtered one.
        for node_type in [NodeType::Script, NodeType::Style, NodeType::Meta, NodeType::Head] {
            assert_eq!(A11yManager::node_type_to_role(&node_type), Role::Group);
        }
    }

    /// The doc contract: no role emitted here may be filtered out by accesskit's
    /// `common_filter` (`GenericContainer` / `TextRun`), or VoiceOver skips the
    /// node. `Col`/`ColGroup` are the sole exceptions — see the test below.
    #[test]
    fn node_type_to_role_never_emits_a_voiceover_filtered_role() {
        let node_types = [
            NodeType::Text(BoxOrStatic::heap(AzString::from("x"))),
            NodeType::P,
            NodeType::Pre,
            NodeType::BlockQuote,
            NodeType::Code,
            NodeType::Em,
            NodeType::Strong,
            NodeType::Mark,
            NodeType::Del,
            NodeType::Ins,
            NodeType::Abbr,
            NodeType::Q,
            NodeType::Time,
            NodeType::Ruby,
            NodeType::Rt,
            NodeType::Br,
            NodeType::Hr,
            NodeType::Body,
            NodeType::Div,
            NodeType::Span,
            NodeType::Html,
            NodeType::Article,
            NodeType::Section,
            NodeType::Nav,
            NodeType::Main,
            NodeType::Header,
            NodeType::Footer,
            NodeType::Aside,
            NodeType::Figure,
            NodeType::FigCaption,
            NodeType::Details,
            NodeType::Summary,
            NodeType::Dialog,
            NodeType::H1,
            NodeType::H2,
            NodeType::H3,
            NodeType::H4,
            NodeType::H5,
            NodeType::H6,
            NodeType::Ul,
            NodeType::Ol,
            NodeType::Li,
            NodeType::Dl,
            NodeType::Dt,
            NodeType::Dd,
            NodeType::Menu,
            NodeType::MenuItem,
            NodeType::Table,
            NodeType::Caption,
            NodeType::THead,
            NodeType::TBody,
            NodeType::TFoot,
            NodeType::Tr,
            NodeType::Th,
            NodeType::Td,
            NodeType::Form,
            NodeType::FieldSet,
            NodeType::Legend,
            NodeType::Label,
            NodeType::Input,
            NodeType::Button,
            NodeType::Select,
            NodeType::SelectOption,
            NodeType::TextArea,
            NodeType::Output,
            NodeType::Progress,
            NodeType::Meter,
            NodeType::DataList,
            NodeType::A,
            NodeType::Canvas,
            NodeType::Audio,
            NodeType::Video,
            NodeType::Svg,
            NodeType::Object,
            NodeType::Embed,
            NodeType::Script,
        ];
        for node_type in node_types {
            let role = A11yManager::node_type_to_role(&node_type);
            assert!(
                role != Role::GenericContainer && role != Role::TextRun,
                "{node_type:?} -> {role:?} would be skipped by VoiceOver's common_filter"
            );
        }
    }

    /// `Col`/`ColGroup` do map to the filtered `GenericContainer` role, against the
    /// function's own doc comment. Harmless today only because `update_tree` drops
    /// both node types before they can reach the tree — pinned so that stays true.
    #[test]
    fn col_and_colgroup_map_to_the_filtered_generic_container_role() {
        assert_eq!(
            A11yManager::node_type_to_role(&NodeType::Col),
            Role::GenericContainer
        );
        assert_eq!(
            A11yManager::node_type_to_role(&NodeType::ColGroup),
            Role::GenericContainer
        );
    }

    // ---------------------------------------------------------------------
    // map_role
    // ---------------------------------------------------------------------

    #[test]
    fn map_role_is_total_and_matches_the_documented_table() {
        let cases: [(AccessibilityRole, Role); 64] = [
            (AccessibilityRole::TitleBar, Role::TitleBar),
            (AccessibilityRole::MenuBar, Role::MenuBar),
            (AccessibilityRole::ScrollBar, Role::ScrollBar),
            (AccessibilityRole::Grip, Role::Splitter),
            (AccessibilityRole::Sound, Role::Audio),
            (AccessibilityRole::Cursor, Role::Caret),
            (AccessibilityRole::Caret, Role::Caret),
            (AccessibilityRole::Alert, Role::Alert),
            (AccessibilityRole::Window, Role::Window),
            (AccessibilityRole::Client, Role::GenericContainer),
            (AccessibilityRole::MenuPopup, Role::Menu),
            (AccessibilityRole::MenuItem, Role::MenuItem),
            (AccessibilityRole::Tooltip, Role::Tooltip),
            (AccessibilityRole::Application, Role::Application),
            (AccessibilityRole::Document, Role::Document),
            (AccessibilityRole::Pane, Role::Pane),
            (AccessibilityRole::Chart, Role::Figure),
            (AccessibilityRole::Dialog, Role::Dialog),
            (AccessibilityRole::Border, Role::GenericContainer),
            (AccessibilityRole::Grouping, Role::Group),
            (AccessibilityRole::Separator, Role::GenericContainer),
            (AccessibilityRole::Toolbar, Role::Toolbar),
            (AccessibilityRole::StatusBar, Role::Status),
            (AccessibilityRole::Table, Role::Table),
            (AccessibilityRole::ColumnHeader, Role::ColumnHeader),
            (AccessibilityRole::RowHeader, Role::RowHeader),
            (AccessibilityRole::Column, Role::GenericContainer),
            (AccessibilityRole::Row, Role::Row),
            (AccessibilityRole::Cell, Role::Cell),
            (AccessibilityRole::Link, Role::Link),
            (AccessibilityRole::HelpBalloon, Role::Tooltip),
            (AccessibilityRole::Character, Role::GenericContainer),
            (AccessibilityRole::List, Role::List),
            (AccessibilityRole::ListItem, Role::ListItem),
            (AccessibilityRole::Outline, Role::Tree),
            (AccessibilityRole::OutlineItem, Role::TreeItem),
            (AccessibilityRole::PageTab, Role::Tab),
            (AccessibilityRole::PropertyPage, Role::TabPanel),
            (AccessibilityRole::Indicator, Role::Meter),
            (AccessibilityRole::Graphic, Role::Image),
            (AccessibilityRole::StaticText, Role::Label),
            (AccessibilityRole::Text, Role::TextInput),
            (AccessibilityRole::PushButton, Role::Button),
            (AccessibilityRole::CheckButton, Role::CheckBox),
            (AccessibilityRole::RadioButton, Role::RadioButton),
            (AccessibilityRole::ComboBox, Role::ComboBox),
            (AccessibilityRole::DropList, Role::ListBox),
            (AccessibilityRole::ProgressBar, Role::ProgressIndicator),
            (AccessibilityRole::Dial, Role::Meter),
            (AccessibilityRole::HotkeyField, Role::TextInput),
            (AccessibilityRole::Slider, Role::Slider),
            (AccessibilityRole::SpinButton, Role::SpinButton),
            (AccessibilityRole::Diagram, Role::Figure),
            (AccessibilityRole::Animation, Role::GenericContainer),
            (AccessibilityRole::Equation, Role::Math),
            (AccessibilityRole::ButtonDropdown, Role::Button),
            (AccessibilityRole::ButtonMenu, Role::Button),
            (AccessibilityRole::ButtonDropdownGrid, Role::Button),
            (AccessibilityRole::Whitespace, Role::GenericContainer),
            (AccessibilityRole::PageTabList, Role::TabList),
            (AccessibilityRole::Clock, Role::Timer),
            (AccessibilityRole::SplitButton, Role::Button),
            (AccessibilityRole::IpAddress, Role::TextInput),
            (AccessibilityRole::Unknown, Role::Unknown),
            (AccessibilityRole::Nothing, Role::GenericContainer),
        ];
        for (role, expected) in cases {
            assert_eq!(A11yManager::map_role(&role), expected, "{role:?}");
        }
    }

    // ---------------------------------------------------------------------
    // map_action_to_accesskit / map_accesskit_action (round-trip)
    // ---------------------------------------------------------------------

    /// Every payload-free action must survive outbound -> inbound unchanged:
    /// the tree declares `map_action_to_accesskit(a)`, the screen reader sends
    /// that action back, and `map_accesskit_action` must hand back exactly `a`.
    #[test]
    fn payload_free_actions_round_trip_through_accesskit() {
        let actions = [
            AccessibilityAction::Default,
            AccessibilityAction::Focus,
            AccessibilityAction::Blur,
            AccessibilityAction::Collapse,
            AccessibilityAction::Expand,
            AccessibilityAction::ScrollIntoView,
            AccessibilityAction::Increment,
            AccessibilityAction::Decrement,
            AccessibilityAction::ShowContextMenu,
            AccessibilityAction::HideTooltip,
            AccessibilityAction::ShowTooltip,
            AccessibilityAction::ScrollUp,
            AccessibilityAction::ScrollDown,
            AccessibilityAction::ScrollLeft,
            AccessibilityAction::ScrollRight,
            AccessibilityAction::SetSequentialFocusNavigationStartingPoint,
        ];
        for action in actions {
            let outbound = A11yManager::map_action_to_accesskit(&action);
            let inbound = map_accesskit_action(request(outbound, None));
            assert_eq!(
                inbound,
                Some(action.clone()),
                "{action:?} did not survive the outbound/inbound round-trip"
            );
        }
    }

    #[test]
    fn payload_carrying_actions_map_to_their_action_kind() {
        let cases = [
            (
                AccessibilityAction::ReplaceSelectedText(AzString::from("x")),
                Action::ReplaceSelectedText,
            ),
            (
                AccessibilityAction::ScrollToPoint(LogicalPosition::new(f32::NAN, 0.0)),
                Action::ScrollToPoint,
            ),
            (
                AccessibilityAction::SetScrollOffset(LogicalPosition::new(-1.0, f32::INFINITY)),
                Action::SetScrollOffset,
            ),
            (
                AccessibilityAction::SetTextSelection(TextSelectionStartEnd {
                    selection_start: usize::MAX,
                    selection_end: 0,
                }),
                Action::SetTextSelection,
            ),
            (
                AccessibilityAction::SetValue(AzString::from("")),
                Action::SetValue,
            ),
            (
                AccessibilityAction::SetNumericValue(FloatValue::new(f32::NAN)),
                Action::SetValue,
            ),
            (AccessibilityAction::CustomAction(i32::MIN), Action::CustomAction),
        ];
        for (action, expected) in cases {
            assert_eq!(
                A11yManager::map_action_to_accesskit(&action),
                expected,
                "{action:?}"
            );
        }
    }

    #[test]
    fn data_requiring_actions_return_none_when_data_is_missing() {
        for action in [
            Action::ReplaceSelectedText,
            Action::ScrollToPoint,
            Action::SetScrollOffset,
            Action::SetTextSelection,
            Action::SetValue,
            Action::CustomAction,
        ] {
            assert_eq!(
                map_accesskit_action(request(action, None)),
                None,
                "{action:?} must reject a request with no payload"
            );
        }
    }

    #[test]
    fn data_requiring_actions_return_none_on_mismatched_payloads() {
        let mismatches = [
            (Action::ReplaceSelectedText, ActionData::NumericValue(1.0)),
            (Action::ScrollToPoint, ActionData::NumericValue(1.0)),
            (
                Action::SetScrollOffset,
                ActionData::Value(Box::from("not a point")),
            ),
            (Action::SetTextSelection, ActionData::CustomAction(3)),
            (Action::SetValue, ActionData::CustomAction(3)),
            (Action::CustomAction, ActionData::NumericValue(1.0)),
        ];
        for (action, data) in mismatches {
            assert_eq!(
                map_accesskit_action(request(action, Some(data.clone()))),
                None,
                "{action:?} must reject payload {data:?}"
            );
        }
    }

    #[test]
    fn payload_free_actions_ignore_an_unexpected_payload() {
        // ScrollIntoView takes an *optional* hint; a stray payload must not
        // turn the request into a no-op.
        assert_eq!(
            map_accesskit_action(request(
                Action::ScrollIntoView,
                Some(ActionData::NumericValue(1.0))
            )),
            Some(AccessibilityAction::ScrollIntoView)
        );
    }

    #[test]
    fn set_value_accepts_unicode_and_empty_strings() {
        for s in ["", "héllo 🎉", "a\0b", "\u{202e}rtl"] {
            let got = map_accesskit_action(request(
                Action::SetValue,
                Some(ActionData::Value(Box::from(s))),
            ));
            assert_eq!(got, Some(AccessibilityAction::SetValue(AzString::from(s))));
        }
    }

    #[test]
    fn set_numeric_value_saturates_instead_of_panicking() {
        for v in [
            0.0_f64,
            -0.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1e300,
            -1e300,
            f64::from(f32::MAX),
        ] {
            let got = map_accesskit_action(request(
                Action::SetValue,
                Some(ActionData::NumericValue(v)),
            ));
            #[allow(clippy::cast_possible_truncation)]
            let expected = AccessibilityAction::SetNumericValue(FloatValue::new(v as f32));
            assert_eq!(got, Some(expected), "NumericValue({v}) must not panic");
        }
    }

    #[test]
    fn scroll_to_point_preserves_nan_and_saturates_out_of_range_f64() {
        let got = map_accesskit_action(request(
            Action::ScrollToPoint,
            Some(ActionData::ScrollToPoint(Point {
                x: f64::NAN,
                y: 1e308,
            })),
        ));
        let Some(AccessibilityAction::ScrollToPoint(p)) = got else {
            panic!("expected ScrollToPoint, got {got:?}");
        };
        assert!(p.x.is_nan(), "NaN must pass through, not trap");
        assert!(
            p.y.is_infinite() && p.y.is_sign_positive(),
            "1e308 must saturate to +inf, got {}",
            p.y
        );
    }

    #[test]
    fn set_scroll_offset_saturates_negative_out_of_range_f64() {
        let got = map_accesskit_action(request(
            Action::SetScrollOffset,
            Some(ActionData::SetScrollOffset(Point {
                x: -1e308,
                y: f64::NEG_INFINITY,
            })),
        ));
        let Some(AccessibilityAction::SetScrollOffset(p)) = got else {
            panic!("expected SetScrollOffset, got {got:?}");
        };
        assert!(p.x.is_infinite() && p.x.is_sign_negative());
        assert!(p.y.is_infinite() && p.y.is_sign_negative());
    }

    #[test]
    fn set_text_selection_passes_through_extreme_character_indices() {
        let got = map_accesskit_action(request(
            Action::SetTextSelection,
            Some(ActionData::SetTextSelection(TextSelection {
                anchor: TextPosition {
                    node: A11yNodeId(1),
                    character_index: usize::MAX,
                },
                focus: TextPosition {
                    node: A11yNodeId(2),
                    character_index: 0,
                },
            })),
        ));
        assert_eq!(
            got,
            Some(AccessibilityAction::SetTextSelection(TextSelectionStartEnd {
                selection_start: usize::MAX,
                selection_end: 0,
            })),
            "an inverted, out-of-range selection must be forwarded verbatim, not clamped"
        );
    }

    #[test]
    fn custom_action_forwards_extreme_ids() {
        for id in [i32::MIN, -1, 0, i32::MAX] {
            assert_eq!(
                map_accesskit_action(request(
                    Action::CustomAction,
                    Some(ActionData::CustomAction(id))
                )),
                Some(AccessibilityAction::CustomAction(id))
            );
        }
    }

    // ---------------------------------------------------------------------
    // build_node — bounds arithmetic (numeric)
    // ---------------------------------------------------------------------

    #[test]
    fn build_node_bounds_are_padding_inset_and_hidpi_scaled() {
        let node_data = NodeData::create_node(NodeType::Div);
        // padding 5px top/bottom, 2px left/right (packed as tenths of a px).
        let layout_node = hot(Some(LogicalSize::new(100.0, 50.0)), [50, 20, 50, 20], [0; 4]);
        let node = A11yManager::build_node(
            &node_data,
            &layout_node,
            Some(LogicalPosition::new(10.0, 20.0)),
            None,
            2.0,
            LogicalSize::new(1000.0, 1000.0),
        );
        let bounds = node.bounds().expect("in-viewport node must have bounds");
        assert_eq!(bounds.x0, 24.0); // (10 + 2) * 2
        assert_eq!(bounds.y0, 50.0); // (20 + 5) * 2
        assert_eq!(bounds.x1, 216.0); // (10 + 100 - 2) * 2
        assert_eq!(bounds.y1, 130.0); // (20 + 50 - 5) * 2
    }

    #[test]
    fn build_node_clips_bounds_to_the_window_viewport() {
        let node_data = NodeData::create_node(NodeType::Div);
        let layout_node = hot(Some(LogicalSize::new(10_000.0, 10_000.0)), [0; 4], [0; 4]);
        let node = A11yManager::build_node(
            &node_data,
            &layout_node,
            Some(LogicalPosition::new(-500.0, -500.0)),
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        let bounds = node.bounds().expect("clipped node still has bounds");
        assert_eq!((bounds.x0, bounds.y0), (0.0, 0.0), "off-screen origin clamps to 0");
        assert_eq!(
            (bounds.x1, bounds.y1),
            (800.0, 600.0),
            "overflowing extent clamps to the viewport"
        );
    }

    #[test]
    fn build_node_omits_bounds_when_padding_exceeds_the_used_size() {
        // Degenerate box: x1 <= x0, so accesskit must not be handed an inverted rect.
        let node_data = NodeData::create_node(NodeType::Div);
        let layout_node = hot(Some(LogicalSize::new(10.0, 10.0)), [500, 500, 500, 500], [0; 4]);
        let node = A11yManager::build_node(
            &node_data,
            &layout_node,
            Some(LogicalPosition::new(0.0, 0.0)),
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.bounds(), None);
    }

    #[test]
    fn build_node_omits_bounds_for_zero_and_nan_hidpi() {
        let node_data = NodeData::create_node(NodeType::Div);
        let layout_node = plain_hot();
        for hidpi in [0.0_f32, f32::NAN, -2.0] {
            let node = A11yManager::build_node(
                &node_data,
                &layout_node,
                Some(LogicalPosition::new(10.0, 20.0)),
                None,
                hidpi,
                LogicalSize::new(800.0, 600.0),
            );
            assert_eq!(
                node.bounds(),
                None,
                "hidpi {hidpi} collapses the rect; no bounds must be set"
            );
        }
    }

    #[test]
    fn build_node_never_emits_an_inverted_rect_for_hostile_geometry() {
        // The `x1 > x0 && y1 > y0` guard is the only thing between accesskit and
        // an inverted/degenerate rect. Sweep the nastiest float inputs at it.
        let node_data = NodeData::create_node(NodeType::Div);
        let sizes = [
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(f32::MAX, f32::MAX),
            LogicalSize::new(-100.0, -100.0),
            LogicalSize::new(f32::NAN, f32::NAN),
            LogicalSize::new(f32::INFINITY, f32::INFINITY),
        ];
        let positions = [
            LogicalPosition::new(0.0, 0.0),
            LogicalPosition::new(-f32::MAX, -f32::MAX),
            LogicalPosition::new(f32::NAN, 0.0),
            LogicalPosition::new(f32::INFINITY, f32::NEG_INFINITY),
        ];
        let windows = [
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(800.0, 600.0),
            LogicalSize::new(f32::MAX, f32::MAX),
            LogicalSize::new(f32::NAN, f32::NAN),
        ];
        for size in sizes {
            for pos in positions {
                for window in windows {
                    for hidpi in [1.0_f32, 0.5, 3.0, f32::INFINITY] {
                        let layout_node = hot(Some(size), [i16::MAX; 4], [i16::MIN; 4]);
                        let node = A11yManager::build_node(
                            &node_data,
                            &layout_node,
                            Some(pos),
                            None,
                            hidpi,
                            window,
                        );
                        if let Some(b) = node.bounds() {
                            assert!(
                                b.x1 > b.x0 && b.y1 > b.y0,
                                "inverted rect {b:?} for size={size:?} pos={pos:?} \
                                 window={window:?} hidpi={hidpi}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn build_node_omits_bounds_when_layout_info_is_missing() {
        let node_data = NodeData::create_node(NodeType::Div);
        // No absolute position.
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.bounds(), None);

        // No used size.
        let node = A11yManager::build_node(
            &node_data,
            &hot(None, [0; 4], [0; 4]),
            Some(LogicalPosition::new(0.0, 0.0)),
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.bounds(), None);
    }

    // ---------------------------------------------------------------------
    // build_node — roles, labels, states, actions (invariants)
    // ---------------------------------------------------------------------

    #[test]
    fn build_node_always_declares_scroll_into_view() {
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert!(node.supports_action(Action::ScrollIntoView));
    }

    #[test]
    fn build_node_sets_the_html_tag() {
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        let expected = NodeType::Div.get_path().to_string();
        assert_eq!(node.html_tag(), Some(expected.as_str()));
    }

    #[test]
    fn build_node_contenteditable_wins_over_a11y_role_and_gains_focus() {
        let mut node_data = NodeData::create_node(NodeType::Div);
        node_data.set_contenteditable(true);
        // Even an explicit (conflicting) role must not override editability.
        node_data.set_accessibility_info(info(AccessibilityRole::PushButton));
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            node_data.get_accessibility_info(),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.role(), Role::MultilineTextInput);
        assert!(node.supports_action(Action::Focus));
    }

    #[test]
    fn build_node_labels_text_nodes_including_unicode_and_empty() {
        for s in ["", "héllo 🎉", "a\u{202e}b"] {
            let node_data = text_node(s);
            let node = A11yManager::build_node(
                &node_data,
                &plain_hot(),
                None,
                None,
                1.0,
                LogicalSize::new(800.0, 600.0),
            );
            assert_eq!(node.role(), Role::Label);
            assert_eq!(node.label(), Some(s));
        }
    }

    #[test]
    fn build_node_sets_heading_levels_one_through_six() {
        let expected = [
            (NodeType::H1, 1),
            (NodeType::H2, 2),
            (NodeType::H3, 3),
            (NodeType::H4, 4),
            (NodeType::H5, 5),
            (NodeType::H6, 6),
        ];
        for (node_type, level) in expected {
            let node_data = NodeData::create_node(node_type);
            let node = A11yManager::build_node(
                &node_data,
                &plain_hot(),
                None,
                None,
                1.0,
                LogicalSize::new(800.0, 600.0),
            );
            assert_eq!(node.role(), Role::Heading);
            assert_eq!(node.level(), Some(level));
        }
    }

    #[test]
    fn build_node_maps_every_handled_accessibility_state() {
        let states = [
            AccessibilityState::Unavailable,
            AccessibilityState::Readonly,
            AccessibilityState::CheckedTrue,
            AccessibilityState::Expanded,
            AccessibilityState::Selected,
            AccessibilityState::Busy,
            AccessibilityState::Offscreen,
            AccessibilityState::Focusable,
        ];
        let mut a11y = info(AccessibilityRole::CheckButton);
        a11y.states = states.to_vec().into();
        let node_data = NodeData::create_node(NodeType::Div);
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.role(), Role::CheckBox);
        assert!(node.is_disabled());
        assert!(node.is_read_only());
        assert_eq!(node.toggled(), Some(Toggled::True));
        assert_eq!(node.is_expanded(), Some(true));
        assert_eq!(node.is_selected(), Some(true));
        assert!(node.is_busy());
        assert!(node.is_hidden());
        assert!(node.supports_action(Action::Focus));
    }

    #[test]
    fn build_node_collapsed_and_checked_false_are_distinct_from_absent() {
        let mut a11y = info(AccessibilityRole::CheckButton);
        a11y.states = vec![
            AccessibilityState::Collapsed,
            AccessibilityState::CheckedFalse,
        ]
        .into();
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.is_expanded(), Some(false));
        assert_eq!(node.toggled(), Some(Toggled::False));
    }

    #[test]
    fn build_node_declares_api_supplied_supported_actions() {
        let mut a11y = info(AccessibilityRole::Slider);
        a11y.supported_actions = vec![
            AccessibilityAction::Increment,
            AccessibilityAction::Decrement,
            AccessibilityAction::SetValue(AzString::from("x")),
            AccessibilityAction::CustomAction(1),
        ]
        .into();
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert!(node.supports_action(Action::Increment));
        assert!(node.supports_action(Action::Decrement));
        assert!(node.supports_action(Action::SetValue));
        assert!(node.supports_action(Action::CustomAction));
    }

    #[test]
    fn build_node_relations_resolve_to_walk_emitted_ids() {
        let mut a11y = info(AccessibilityRole::Text);
        a11y.labelled_by = OptionDomNodeId::Some(dom_node(2, 3));
        a11y.described_by = OptionDomNodeId::Some(dom_node(0, 0));
        a11y.is_live_region = true;
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(
            node.labelled_by(),
            &[A11yManager::encode_a11y_node_id(2, 3)]
        );
        assert_eq!(
            node.described_by(),
            &[A11yManager::encode_a11y_node_id(0, 0)]
        );
        assert_eq!(node.live(), Some(Live::Polite));
    }

    #[test]
    fn build_node_drops_relations_pointing_at_the_none_sentinel() {
        let mut a11y = info(AccessibilityRole::Text);
        let unresolvable = DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::NONE,
        };
        a11y.labelled_by = OptionDomNodeId::Some(unresolvable);
        a11y.described_by = OptionDomNodeId::Some(unresolvable);
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert!(
            node.labelled_by().is_empty() && node.described_by().is_empty(),
            "an unresolvable relation must be dropped, not encoded as a bogus id"
        );
    }

    // ---------------------------------------------------------------------
    // build_node — HTML attributes
    // ---------------------------------------------------------------------

    #[test]
    fn build_node_parses_aria_live_case_insensitively() {
        for (value, expected) in [
            ("polite", Live::Polite),
            ("POLITE", Live::Polite),
            ("assertive", Live::Assertive),
            ("AsSeRtIvE", Live::Assertive),
            ("off", Live::Off),
            ("", Live::Off),
            ("banana", Live::Off),
            ("🎉", Live::Off),
        ] {
            let mut node_data = NodeData::create_node(NodeType::Div);
            node_data.set_attributes(
                vec![AttributeType::AriaProperty(AttributeNameValue {
                    attr_name: AzString::from("ARIA-LIVE"),
                    value: AzString::from(value),
                })]
                .into(),
            );
            let node = A11yManager::build_node(
                &node_data,
                &plain_hot(),
                None,
                None,
                1.0,
                LogicalSize::new(800.0, 600.0),
            );
            assert_eq!(node.live(), Some(expected), "aria-live={value:?}");
        }
    }

    #[test]
    fn build_node_wires_up_html_attributes() {
        let mut node_data = NodeData::create_node(NodeType::Input);
        node_data.set_attributes(
            vec![
                AttributeType::AriaLabel(AzString::from("label")),
                AttributeType::Title(AzString::from("desc")),
                AttributeType::Placeholder(AzString::from("hint")),
                AttributeType::Value(AzString::from("val")),
                AttributeType::Disabled,
                AttributeType::Readonly,
                AttributeType::Required,
                AttributeType::Hidden,
                AttributeType::CheckedTrue,
                AttributeType::Lang(AzString::from("de")),
                AttributeType::ColSpan(2),
                AttributeType::RowSpan(3),
            ]
            .into(),
        );
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.label(), Some("label"));
        assert_eq!(node.description(), Some("desc"));
        assert_eq!(node.placeholder(), Some("hint"));
        assert_eq!(node.value(), Some("val"));
        assert!(node.is_disabled());
        assert!(node.is_read_only());
        assert!(node.is_required());
        assert!(node.is_hidden());
        assert_eq!(node.toggled(), Some(Toggled::True));
        assert_eq!(node.language(), Some("de"));
        assert_eq!(node.column_span(), Some(2));
        assert_eq!(node.row_span(), Some(3));
    }

    /// `colspan`/`rowspan` are `i32` in the DOM but `usize` in accesskit, and the
    /// conversion is an unchecked `as` cast. A negative span (HTML lets you write
    /// `colspan="-1"`) sign-extends into an astronomically large span instead of
    /// being rejected or clamped. Pinned here: no panic, but the value is garbage.
    #[test]
    fn build_node_negative_col_and_row_span_sign_extend_to_usize_max() {
        let mut node_data = NodeData::create_node(NodeType::Td);
        node_data.set_attributes(
            vec![AttributeType::ColSpan(-1), AttributeType::RowSpan(-1)].into(),
        );
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.column_span(), Some(usize::MAX));
        assert_eq!(node.row_span(), Some(usize::MAX));
    }

    #[test]
    fn build_node_zero_span_is_forwarded_unchanged() {
        let mut node_data = NodeData::create_node(NodeType::Td);
        node_data.set_attributes(vec![AttributeType::ColSpan(0)].into());
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.column_span(), Some(0));
    }

    #[test]
    fn build_node_dom_attributes_override_accessibility_info() {
        // Documented priority: explicit a11y info < DOM attributes.
        let mut a11y = info(AccessibilityRole::PushButton);
        a11y.accessibility_name = OptionString::Some(AzString::from("from-info"));
        a11y.accessibility_value = OptionString::Some(AzString::from("info-value"));
        let mut node_data = NodeData::create_node(NodeType::Button);
        node_data.set_attributes(
            vec![
                AttributeType::AriaLabel(AzString::from("from-attr")),
                AttributeType::Value(AzString::from("attr-value")),
            ]
            .into(),
        );
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.label(), Some("from-attr"));
        assert_eq!(node.value(), Some("attr-value"));
    }

    // ---------------------------------------------------------------------
    // update_tree (numeric / no-panic)
    // ---------------------------------------------------------------------

    #[test]
    fn update_tree_with_no_doms_emits_only_the_root_window_node() {
        let update = empty_update(LogicalSize::new(800.0, 600.0), None, 1.0, "title");
        assert_eq!(update.nodes.len(), 1);
        assert_eq!(update.nodes[0].0, A11yNodeId(0));
        assert_eq!(update.nodes[0].1.role(), Role::Window);
        assert_eq!(update.nodes[0].1.label(), Some("title"));
        assert!(update.nodes[0].1.children().is_empty());
        assert!(update.tree.is_some(), "the first update must carry the tree");
        assert_eq!(update.tree_id, TreeId::ROOT);
        assert_eq!(
            update.focus,
            A11yNodeId(0),
            "with no content nodes, focus must fall back to the root"
        );
    }

    #[test]
    fn update_tree_survives_degenerate_window_sizes_and_hidpi() {
        let sizes = [
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(-1.0, -1.0),
            LogicalSize::new(f32::MAX, f32::MAX),
            LogicalSize::new(f32::NAN, f32::NAN),
            LogicalSize::new(f32::INFINITY, f32::NEG_INFINITY),
        ];
        for size in sizes {
            for hidpi in [0.0_f32, 1.0, -1.0, f32::NAN, f32::INFINITY, f32::MIN] {
                let update = empty_update(size, None, hidpi, "t");
                assert_eq!(update.nodes.len(), 1, "size={size:?} hidpi={hidpi}");
                assert_eq!(update.focus, A11yNodeId(0));
            }
        }
    }

    #[test]
    fn update_tree_focus_falls_back_to_root_for_an_unresolvable_focused_node() {
        // A focused node in a DOM that isn't in layout_results at all.
        let update = empty_update(
            LogicalSize::new(800.0, 600.0),
            Some(dom_node(9, 42)),
            1.0,
            "t",
        );
        assert_eq!(update.focus, A11yNodeId(0));

        // A focused node whose NodeHierarchyItemId is the None sentinel.
        let update = empty_update(
            LogicalSize::new(800.0, 600.0),
            Some(DomNodeId {
                dom: DomId { inner: 0 },
                node: NodeHierarchyItemId::NONE,
            }),
            1.0,
            "t",
        );
        assert_eq!(update.focus, A11yNodeId(0));
    }

    #[test]
    fn update_tree_preserves_unicode_and_empty_window_titles() {
        for title in ["", "Ünïcødé 🪟", "a\u{202e}b"] {
            let update = empty_update(LogicalSize::new(800.0, 600.0), None, 1.0, title);
            assert_eq!(update.nodes[0].1.label(), Some(title));
        }
    }

    #[test]
    fn update_tree_honours_a_non_zero_root_id() {
        let layout_results = BTreeMap::new();
        let scroll_manager = ScrollManager::new();
        let overrides = BTreeMap::new();
        let root = A11yNodeId(999);
        let update = A11yManager::update_tree(
            root,
            &layout_results,
            &scroll_manager,
            &AzString::from("t"),
            LogicalSize::new(800.0, 600.0),
            None,
            1.0,
            &overrides,
            None,
        );
        assert_eq!(update.nodes[0].0, root);
        assert_eq!(update.focus, root);
        assert_eq!(
            update.tree.map(|t| t.root),
            Some(root),
            "the declared tree root must match the emitted root node"
        );
    }
}
