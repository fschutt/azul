//! Accessibility Manager for integrating with `accesskit`.
//!
//! This module provides the `A11yManager` which:
//! - Maintains the accessibility tree state
//! - Generates `TreeUpdate`s after each layout pass
//! - Handles `ActionRequest`s from assistive technologies
//!
//! The manager translates between Azul's internal DOM representation and
//! the platform-agnostic `accesskit` tree format.

#[cfg(feature = "accessibility")]
use accesskit::{Action, ActionRequest, Node, NodeId as A11yNodeId, Rect, Role, Tree, TreeUpdate};
use azul_core::{
    dom::{AccessibilityAction, AccessibilityRole, DomId, DomNodeId, NodeId},
    styled_dom::NodeHierarchyItemId,
};

#[cfg(feature = "accessibility")]
/// Manager for accessibility tree state and updates.
///
/// The `A11yManager` sits within `LayoutWindow` and is responsible for:
/// 1. Maintaining the current accessibility tree state
/// 2. Generating `TreeUpdate`s by comparing layout results with the stored tree
/// 3. Translating `ActionRequest`s from screen readers into synthetic Azul events
pub struct A11yManager {
    /// The root node ID of the accessibility tree (represents the window).
    pub root_id: A11yNodeId,
    /// The current accessibility tree state.
    pub tree: Option<Tree>,
}

#[cfg(feature = "accessibility")]
impl A11yManager {
    /// Creates a new `A11yManager` with an empty tree containing only a root window node.
    pub fn new() -> Self {
        let root_id = A11yNodeId(0);
        Self {
            root_id,
            tree: None,
        }
    }

    /// Updates the accessibility tree based on the current layout state.
    ///
    /// This should be called after each layout pass to synchronize the
    /// accessibility tree with the visual representation.
    pub fn update_tree(
        root_id: A11yNodeId,
        layout_results: &std::collections::BTreeMap<
            azul_core::dom::DomId,
            crate::window::DomLayoutResult,
        >,
        window_title: &azul_css::AzString,
        window_size: azul_core::geom::LogicalSize,
    ) -> TreeUpdate {
        use accesskit::{Node, Rect};

        let mut nodes = Vec::new();
        let mut root_children = Vec::new();

        // Create root window node
        let mut root_node = Node::new(Role::Window);

        nodes.push((root_id, root_node));

        // Traverse all DOMs and their layout trees
        for (dom_id, layout_result) in layout_results {
            let styled_dom = &layout_result.styled_dom;

            // Process each layout node
            for layout_node in layout_result.layout_tree.nodes.iter() {
                let Some(dom_node_id) = layout_node.dom_node_id else {
                    continue;
                };

                // Generate stable A11yNodeId from DomId + NodeId
                let node_index = dom_node_id.index();
                let a11y_node_id = A11yNodeId(((dom_id.inner as u64) << 32) | node_index as u64);

                // Get accessibility info from NodeData
                let node_data = styled_dom.node_data.as_ref().get(dom_node_id.index());
                let Some(node_data) = node_data else {
                    continue;
                };
                let a11y_info = node_data.get_accessibility_info();

                // Only create accessibility nodes for elements with accessibility info
                // or semantic HTML elements
                let node_type = &node_data.node_type;
                let should_create_node = a11y_info.is_some()
                    || matches!(
                        node_type,
                        azul_core::dom::NodeType::Button
                            | azul_core::dom::NodeType::Input
                            | azul_core::dom::NodeType::TextArea
                            | azul_core::dom::NodeType::Select
                            | azul_core::dom::NodeType::A
                            | azul_core::dom::NodeType::H1
                            | azul_core::dom::NodeType::H2
                            | azul_core::dom::NodeType::H3
                            | azul_core::dom::NodeType::H4
                            | azul_core::dom::NodeType::H5
                            | azul_core::dom::NodeType::H6
                            | azul_core::dom::NodeType::Article
                            | azul_core::dom::NodeType::Section
                            | azul_core::dom::NodeType::Nav
                            | azul_core::dom::NodeType::Main
                            | azul_core::dom::NodeType::Header
                            | azul_core::dom::NodeType::Footer
                            | azul_core::dom::NodeType::Aside
                    );

                if !should_create_node {
                    continue;
                }

                // Build the accesskit Node
                let a11y_info_ref = a11y_info.as_ref().map(|b| b.as_ref());
                let node = Self::build_node(node_data, layout_node, a11y_info_ref);

                // Set up parent-child relationships
                let node_hierarchy = styled_dom.node_hierarchy.as_ref();
                let hierarchy_item = &node_hierarchy[dom_node_id.index()];

                // Collect children that have accessibility nodes
                // TODO: Properly traverse children from NodeHierarchy
                // For now, we create nodes but don't set up parent-child relationships
                // as NodeHierarchyItem doesn't have a children field (it uses last_child + sibling
                // pointers)

                // If this is a top-level node (no parent), add to root children
                let has_parent = hierarchy_item.parent != usize::MAX;
                if !has_parent {
                    root_children.push(a11y_node_id);
                }

                nodes.push((a11y_node_id, node));
            }
        }

        // Update root node with children
        // TODO: Implement this properly once we figure out accesskit 0.17 API

        // Determine focus - for now default to root
        let focus = root_id;

        // Create the tree update
        let tree_update = TreeUpdate {
            nodes,
            tree: Some(Tree::new(root_id)), // Always create new tree for now
            focus,
        };

        tree_update
    }

    /// Builds an accesskit Node from Azul's NodeData and layout information.
    fn build_node(
        node_data: &azul_core::dom::NodeData,
        layout_node: &crate::solver3::layout_tree::LayoutNode<
            impl crate::text3::cache::ParsedFontTrait,
        >,
        a11y_info: Option<&azul_core::dom::AccessibilityInfo>,
    ) -> Node {
        use azul_core::dom::NodeType;

        // Set role based on NodeType or AccessibilityInfo
        let role = if let Some(info) = a11y_info {
            Self::map_role(&info.role)
        } else {
            // Infer role from NodeType
            match &node_data.node_type {
                NodeType::Button => Role::Button,
                NodeType::Input => Role::TextInput,
                NodeType::TextArea => Role::MultilineTextInput,
                NodeType::Select => Role::ComboBox,
                NodeType::A => Role::Link,
                NodeType::H1
                | NodeType::H2
                | NodeType::H3
                | NodeType::H4
                | NodeType::H5
                | NodeType::H6 => Role::Heading,
                NodeType::Article => Role::Article,
                NodeType::Section => Role::Section,
                NodeType::Nav => Role::Navigation,
                NodeType::Main => Role::Main,
                NodeType::Header => Role::Header,
                NodeType::Footer => Role::Footer,
                NodeType::Aside => Role::Complementary,
                _ => Role::GenericContainer,
            }
        };

        let node = Node::new(role);

        // TODO: Set properties once we understand accesskit 0.17 API better
        // For now, just return a basic node with the role
        // In a future iteration, we'll add:
        // - Name/label from AccessibilityInfo
        // - Description
        // - Value
        // - States (focusable, disabled, readonly, checked, expanded)
        // - Bounds from layout_node

        node
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
            AccessibilityRole::Separator => Role::GenericContainer, /* No Separator in accesskit
                                                                      * 0.17 */
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
            AccessibilityRole::StaticText => Role::Label, // StaticText -> Label in accesskit 0.17
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
            AccessibilityRole::Animation => Role::GenericContainer, /* No Animation in accesskit
                                                                      * 0.17 */
            AccessibilityRole::Equation => Role::Math,
            AccessibilityRole::ButtonDropdown => Role::Button,
            AccessibilityRole::ButtonMenu => Role::Button, // No MenuButton in accesskit 0.17
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
        // Decode the A11yNodeId back into DomId + NodeId
        // NodeId format: ((dom_id as u64) << 32) | node_id as u64
        let dom_id = DomId {
            inner: (request.target.0 >> 32) as usize,
        };
        let node_id = NodeId::new((request.target.0 & 0xFFFFFFFF) as usize);
        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        let dom_node_id = DomNodeId {
            dom: dom_id,
            node: hierarchy_id,
        };

        // Map accesskit Action to AccessibilityAction
        // Note: accesskit 0.17 uses different action names than the custom AccessibilityAction enum
        let action = match request.action {
            Action::Focus => AccessibilityAction::Focus,
            Action::ScrollIntoView => AccessibilityAction::ScrollIntoView,
            Action::Increment => AccessibilityAction::Increment,
            Action::Decrement => AccessibilityAction::Decrement,
            Action::ShowContextMenu => AccessibilityAction::ShowContextMenu,
            Action::ScrollBackward => AccessibilityAction::ScrollBackward,
            Action::ScrollForward => AccessibilityAction::ScrollForward,
            Action::ScrollUp => AccessibilityAction::ScrollUp,
            Action::ScrollDown => AccessibilityAction::ScrollDown,
            Action::ScrollLeft => AccessibilityAction::ScrollLeft,
            Action::ScrollRight => AccessibilityAction::ScrollRight,
            // For any unmapped action, treat as default action
            _ => AccessibilityAction::Default,
        };

        Some((dom_node_id, action))
    }
}

#[cfg(not(feature = "accessibility"))]
/// Stub implementation when accessibility feature is disabled.
pub struct A11yManager {
    _private: (),
}

#[cfg(not(feature = "accessibility"))]
impl A11yManager {
    pub fn new() -> Self {
        Self { _private: () }
    }
}
