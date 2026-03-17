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
    styled_dom::NodeHierarchyItemId,
};
use azul_css::AzString;

use crate::{solver3::layout_tree::LayoutNodeHot, window::DomLayoutResult};

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
}

#[cfg(feature = "a11y")]
impl A11yManager {
    /// Creates a new `A11yManager` with an empty tree containing only a root window node.
    pub fn new() -> Self {
        let root_id = A11yNodeId(0);
        Self {
            root_id,
            tree: None,
            last_tree_update: None,
        }
    }

    /// Updates the accessibility tree based on the current layout state.
    ///
    /// This should be called after each layout pass to synchronize the
    /// accessibility tree with the visual representation.
    pub fn update_tree(
        root_id: A11yNodeId,
        layout_results: &std::collections::BTreeMap<DomId, DomLayoutResult>,
        window_title: &AzString,
        window_size: LogicalSize,
        focused_node: Option<azul_core::dom::DomNodeId>,
        hidpi_factor: f32,
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
                let a11y_node_id = A11yNodeId(((dom_id.inner as u64) << 32) | (dom_idx as u64) + 1);

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

                let a11y_info_ref = a11y_info.as_ref().map(|b| b.as_ref());
                let mut node = match layout_info {
                    Some((layout_node, _layout_idx, abs_pos)) => {
                        Self::build_node(node_data, layout_node, abs_pos, a11y_info_ref, hidpi_factor, window_size)
                    }
                    None => {
                        let role = if let Some(info) = a11y_info_ref {
                            Self::map_role(&info.role)
                        } else {
                            Self::node_type_to_role(&node_data.node_type)
                        };
                        let mut builder = Node::new(role);
                        if let NodeType::Text(text) = &node_data.node_type {
                            builder.set_label(text.as_str());
                        }
                        builder
                    }
                };

                // Collect child text content and promote to this node's label or value.
                // VoiceOver reads the node's label/value — it doesn't automatically
                // concatenate child text nodes. Without this, headings, paragraphs,
                // list items, etc. would be announced as empty containers.
                {
                    let hierarchy_item = &node_hierarchy[dom_idx];
                    let mut text_content = String::new();
                    // Recursively collect text from immediate children
                    let mut child = hierarchy_item.first_child_id(NodeId::new(dom_idx));
                    while let Some(child_id) = child {
                        if let Some(child_data) = node_data_slice.get(child_id.index()) {
                            if let NodeType::Text(t) = &child_data.node_type {
                                if !text_content.is_empty() { text_content.push(' '); }
                                text_content.push_str(t.as_str());
                            }
                        }
                        if child_id.index() >= node_hierarchy.len() { break; }
                        child = node_hierarchy[child_id.index()].next_sibling_id();
                    }

                    if !text_content.is_empty() {
                        if node_data.is_contenteditable()
                            || matches!(node_data.node_type, NodeType::TextArea | NodeType::Input)
                        {
                            // Text inputs: set as AXValue (editable content)
                            node.set_value(text_content.as_str());
                        } else {
                            // Everything else (headings, paragraphs, list items, links,
                            // buttons, table cells, etc.): set as label so VoiceOver reads it
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
                        .or_insert_with(Vec::new)
                        .push(a11y_node_id);
                } else {
                    root_children.push(a11y_node_id);
                }
            }
        }

        // Third pass: Set children on all nodes (including root)
        for (node_id, node) in nodes.iter_mut() {
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
                    .map(|(id, _)| *id)
                    .unwrap_or(root_id)
            });

        // Create the tree update
        let tree_update = TreeUpdate {
            nodes,
            tree: Some(Tree::new(root_id)),
            focus,
        };

        tree_update
    }

    /// Builds an accesskit Node from Azul's NodeData and layout information.
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
        }

        // DOM attribute overrides
        if let Some(label) = node_data.get_accessible_label() {
            builder.set_label(label);
        }
        if let Some(value) = node_data.get_accessible_value() {
            builder.set_value(value);
        }
        if let Some(placeholder) = node_data.get_placeholder() {
            builder.set_placeholder(placeholder);
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

            let s = hidpi_factor as f64;
            let ww = window_size.width as f64 * s;
            let wh = window_size.height as f64 * s;

            let x0 = ((pos.x + pad_left) as f64 * s).max(0.0).min(ww);
            let y0 = ((pos.y + pad_top) as f64 * s).max(0.0).min(wh);
            let x1 = ((pos.x + size.width - pad_right) as f64 * s).max(0.0).min(ww);
            let y1 = ((pos.y + size.height - pad_bottom) as f64 * s).max(0.0).min(wh);

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

        builder
    }

    /// Maps an HTML `NodeType` to an accesskit `Role`.
    ///
    /// Every role used here must pass accesskit's `common_filter` (i.e. NOT be
    /// `GenericContainer` or `TextRun`) or VoiceOver will skip the node entirely.
    /// Use `Group` for structural containers, `Paragraph` for text blocks, `Label`
    /// for inline text, and semantic roles for everything else.
    const fn node_type_to_role(node_type: &NodeType) -> Role {
        match node_type {
            // === Text content ===
            NodeType::Text(_) => Role::Label,
            NodeType::P => Role::Paragraph,
            NodeType::Pre => Role::Pre,
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
            NodeType::Dt => Role::DescriptionListTerm,
            NodeType::Dd => Role::DescriptionListDetail,
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

    /// Maps Azul's AccessibilityRole to accesskit's Role.
    fn map_role(role: &AccessibilityRole) -> Role {
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

    /// Handles an action request from an assistive technology.
    ///
    /// Translates the accesskit ActionRequest into a (DomNodeId, Action) pair
    /// that can be used to generate synthetic events in the Azul event system.
    pub fn handle_action_request(
        &self,
        request: ActionRequest,
    ) -> Option<(DomNodeId, AccessibilityAction)> {
        use azul_css::{props::basic::FloatValue, AzString};

        // Decode the A11yNodeId back into DomId + NodeId.
        //
        // The A11yNodeId encodes both values in a single u64:
        //
        //   - Upper 32 bits: DomId (which DOM tree the node belongs to)
        //   - Lower 32 bits: NodeId (index within that DOM tree)
        //
        // This encoding matches the format used in update_tree().
        let dom_id = DomId {
            inner: (request.target.0 >> 32) as usize,
        };
        let node_id = NodeId::new((request.target.0 & 0xFFFF_FFFF) as usize);
        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        let dom_node_id = DomNodeId {
            dom: dom_id,
            node: hierarchy_id,
        };

        Some((dom_node_id, map_accesskit_action(request)?))
    }
}

/// Maps an accesskit `Action` and optional `ActionData` to an Azul `AccessibilityAction`.
///
/// Returns `None` if the action requires data that was not provided or is invalid.
#[cfg(feature = "a11y")]
fn map_accesskit_action(request: ActionRequest) -> Option<AccessibilityAction> {
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
