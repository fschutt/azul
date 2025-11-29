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

use crate::{solver3::layout_tree::LayoutNode, window::DomLayoutResult};

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
    ) -> TreeUpdate {
        let mut nodes = Vec::new();
        let mut root_children = Vec::new();

        // Map from (DomId, NodeId) to A11yNodeId for building parent-child relationships
        let mut node_id_map: HashMap<(u32, u32), A11yNodeId> = HashMap::new();

        // Map to collect children for each parent
        let mut parent_children_map: HashMap<A11yNodeId, Vec<A11yNodeId>> = HashMap::new();

        // Create root window node
        let mut root_node = Node::new(Role::Window);
        root_node.set_label(window_title.as_str());

        // Traverse all DOMs and their layout trees - FIRST PASS: Create all nodes
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
                let should_create_node =
                    a11y_info.is_some() || node_data.node_type.is_semantic_for_accessibility();

                if !should_create_node {
                    continue;
                }

                // Build the accesskit Node
                let a11y_info_ref = a11y_info.as_ref().map(|b| b.as_ref());
                let node = Self::build_node(node_data, layout_node, a11y_info_ref);

                // Store node ID mapping
                node_id_map.insert((dom_id.inner as u32, node_index as u32), a11y_node_id);

                nodes.push((a11y_node_id, node));
            }
        }

        // Second pass: Build parent-child relationships
        for (dom_id, layout_result) in layout_results {
            let styled_dom = &layout_result.styled_dom;
            let node_hierarchy = styled_dom.node_hierarchy.as_ref();

            for layout_node in layout_result.layout_tree.nodes.iter() {
                let Some(dom_node_id) = layout_node.dom_node_id else {
                    continue;
                };

                let node_index = dom_node_id.index();
                let a11y_node_id = match node_id_map.get(&(dom_id.inner as u32, node_index as u32))
                {
                    Some(id) => *id,
                    None => continue, // Node was filtered out
                };

                let hierarchy_item = &node_hierarchy[node_index];

                // Find accessible parent by walking up the tree
                let mut parent_node_index = hierarchy_item.parent;
                let mut accessible_parent_id = None;

                while parent_node_index != usize::MAX {
                    if let Some(parent_a11y_id) =
                        node_id_map.get(&(dom_id.inner as u32, parent_node_index as u32))
                    {
                        accessible_parent_id = Some(*parent_a11y_id);
                        break;
                    }
                    // Parent doesn't have a11y node, walk up further
                    let parent_item = &node_hierarchy[parent_node_index];
                    parent_node_index = parent_item.parent;
                }

                // Add this node as child to its accessible parent
                if let Some(parent_id) = accessible_parent_id {
                    parent_children_map
                        .entry(parent_id)
                        .or_insert_with(Vec::new)
                        .push(a11y_node_id);
                } else {
                    // No accessible parent - this is a top-level node
                    root_children.push(a11y_node_id);
                }
            }
        }

        // Third pass: Set children on all nodes
        for (node_id, node) in nodes.iter_mut() {
            if let Some(children) = parent_children_map.get(node_id) {
                node.set_children(children.clone());
            }
        }

        // Set children on root node (first node in list)
        if let Some((node_id, root_node)) = nodes.first_mut() {
            if *node_id == root_id {
                root_node.set_children(root_children);
            }
        }

        // Determine focus - for now default to root
        let focus = root_id;

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
        layout_node: &LayoutNode,
        a11y_info: Option<&AccessibilityInfo>,
    ) -> Node {
        // Set role based on NodeType or AccessibilityInfo
        let role = if let Some(info) = a11y_info {
            Self::map_role(&info.role)
        } else {
            Self::node_type_to_role(&node_data.node_type)
        };

        let mut builder = Node::new(role);

        // Set node properties based on AccessibilityInfo and NodeType
        if let Some(info) = a11y_info {
            // Name/Label
            if let Some(name) = info.name.as_option() {
                builder.set_label(name.as_str());
            }

            // Value (for inputs, sliders, etc.)
            if let Some(value) = info.value.as_option() {
                builder.set_value(value.as_str());
            }

            // States from AccessibilityStateVec
            for state in info.states.as_ref() {
                match state {
                    AccessibilityState::Unavailable => {
                        builder.set_disabled();
                    }
                    AccessibilityState::Readonly => {
                        builder.set_read_only();
                    }
                    AccessibilityState::Checked => {
                        builder.set_toggled(accesskit::Toggled::True);
                    }
                    AccessibilityState::Expanded => {
                        builder.set_expanded(true);
                    }
                    AccessibilityState::Collapsed => {
                        builder.set_expanded(false);
                    }
                    _ => {
                        // Other states: Focused (handled by focus manager),
                        // Selected, Focusable, etc.
                    }
                }
            }
        }

        // Extract text content for Text and StaticText nodes
        if let NodeType::Text(text) = &node_data.node_type {
            builder.set_label(text.as_str());
        }

        // Set bounds from layout node
        if let (Some(pos), Some(size)) = (layout_node.relative_position, layout_node.used_size) {
            builder.set_bounds(Rect {
                x0: pos.x as f64,
                y0: pos.y as f64,
                x1: (pos.x + size.width) as f64,
                y1: (pos.y + size.height) as f64,
            });
        }

        builder
    }

    /// Maps an HTML `NodeType` to an accesskit `Role`.
    ///
    /// Used when no explicit accessibility info is provided to infer
    /// the appropriate role from semantic HTML elements.
    const fn node_type_to_role(node_type: &NodeType) -> Role {
        match node_type {
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
                start: selection.anchor.character_index,
                end: selection.focus.character_index,
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
