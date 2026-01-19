//! Hit-testing logic for layout windows
//!
//! This module handles determining which DOM nodes are under the mouse cursor
//! and resolving the cursor icon based on CSS cursor properties.
//!
//! ## Cursor Resolution Algorithm
//!
//! WebRender returns hit-test results in **front-to-back** order:
//! - `depth = 0` is the frontmost/topmost element (closest to the user)
//! - Higher depth values are further back in the z-order
//!
//! The algorithm finds the **frontmost** node that has an explicit CSS `cursor`
//! property set. If no node has a cursor property, we check if the node has
//! text children and use their cursor property (typically `cursor:text`).
//!
//! ## Design Principles
//!
//! 1. **Frontmost priority**: The node closest to the user (lowest depth) takes
//!    precedence. This matches browser behavior where a button's cursor:pointer
//!    overrides any parent's cursor setting.
//!
//! 2. **Text-child inheritance**: Text nodes are inline and don't get hit-test areas.
//!    Their container inherits the text node's cursor if the container has no explicit
//!    cursor property. This shows I-beam cursor over text containers.
//!
//! 3. **Explicit cursor wins**: If a container has an explicit cursor property
//!    (like `cursor:pointer` on a button), it overrides any text-child cursor.

use std::collections::BTreeMap;

// Re-export FullHitTest from azul_core for backwards compatibility
pub use azul_core::hit_test::FullHitTest;
use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    hit_test::{HitTest, HitTestItem},
    window::MouseCursorType,
};
use azul_css::props::style::StyleCursor;

use crate::window::LayoutWindow;

/// Result of cursor type hit-testing, determines which mouse cursor to display
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CursorTypeHitTest {
    /// The node that has a non-default cursor property (if any)
    pub cursor_node: Option<(DomId, NodeId)>,
    /// The mouse cursor type to display
    pub cursor_icon: MouseCursorType,
}

impl CursorTypeHitTest {
    /// Create a new cursor type hit-test from a full hit-test and layout window
    ///
    /// Finds the frontmost (lowest depth) node with a CSS `cursor` property
    /// and returns the corresponding cursor type. If no node has a cursor property
    /// directly, we check if the node has text children that have `cursor:text`.
    ///
    /// ## Algorithm
    ///
    /// 1. Sort hit nodes by depth (frontmost first)
    /// 2. For each hit node (in front-to-back order):
    ///    a. If the node has an explicit cursor property → use it and stop
    ///    b. If the node has text children → check their cursor property
    /// 3. The first match wins (frontmost node with a cursor)
    ///
    /// ## Text Child Detection
    ///
    /// Text nodes are inline and don't get their own hit-test areas. When hovering
    /// over text, the hit-test returns the text's container node. We need to check
    /// if the container has text children and use their cursor (typically I-beam).
    ///
    /// This detection ONLY applies to the frontmost hit node. If a button with
    /// `cursor:pointer` is in front, its cursor wins regardless of text behind it.
    pub fn new(hit_test: &FullHitTest, layout_window: &LayoutWindow) -> Self {
        use azul_core::dom::NodeType;
        
        let mut cursor_node = None;
        let mut cursor_icon = MouseCursorType::Default;
        // Start with MAX so any node with a cursor property will be selected
        let mut best_depth: u32 = u32::MAX;

        // Iterate through all hovered nodes across all DOMs
        for (dom_id, hit_nodes) in hit_test.hovered_nodes.iter() {
            // Get the layout result for this DOM
            let layout_result = match layout_window.get_layout_result(dom_id) {
                Some(lr) => lr,
                None => continue,
            };

            let styled_dom = &layout_result.styled_dom;
            let node_data_container = styled_dom.node_data.as_container();
            let styled_nodes = styled_dom.styled_nodes.as_container();
            let node_hierarchy = styled_dom.node_hierarchy.as_container();

            // Check each hit node for a cursor property
            // We want the FRONTMOST node (lowest depth) that has a cursor property
            for (node_id, hit_item) in hit_nodes.regular_hit_test_nodes.iter() {
                let node_depth = hit_item.hit_depth;
                
                // Only consider this node if it's in front of our current best
                // (lower depth = closer to user = higher priority)
                if node_depth >= best_depth {
                    continue;
                }
                
                // Query the CSS cursor property for this node
                let cursor_prop = styled_dom.get_css_property_cache().get_cursor(
                    &node_data_container[*node_id],
                    node_id,
                    &styled_nodes[*node_id].styled_node_state,
                );
                
                // If this node has an explicit cursor property, use it
                if let Some(cursor_prop) = cursor_prop {
                    let css_cursor = cursor_prop.get_property().copied().unwrap_or_default();
                    cursor_node = Some((*dom_id, *node_id));
                    cursor_icon = translate_cursor(css_cursor);
                    best_depth = node_depth;
                    continue;
                }
                
                // No explicit cursor on this node - check if it has text children
                // Text nodes are inline and don't get hit-test areas, so we need to
                // check the container's children to see if we're over text.
                let hier = &node_hierarchy[*node_id];
                if let Some(first_child) = hier.first_child_id(*node_id) {
                    let mut child_id = Some(first_child);
                    while let Some(cid) = child_id {
                        let child_data = &node_data_container[cid];
                        if matches!(child_data.get_node_type(), NodeType::Text(_)) {
                            // Found a text child - check its cursor property
                            let child_cursor = styled_dom.get_css_property_cache().get_cursor(
                                child_data,
                                &cid,
                                &styled_nodes[cid].styled_node_state,
                            );
                            
                            if let Some(child_cursor_prop) = child_cursor {
                                let css_cursor = child_cursor_prop.get_property().copied().unwrap_or_default();
                                cursor_node = Some((*dom_id, cid));
                                cursor_icon = translate_cursor(css_cursor);
                                best_depth = node_depth;
                                break;
                            }
                        }
                        child_id = node_hierarchy[cid].next_sibling_id();
                    }
                }
            }
        }

        Self {
            cursor_node,
            cursor_icon,
        }
    }
}

/// Translate CSS cursor value to MouseCursorType
fn translate_cursor(cursor: StyleCursor) -> MouseCursorType {
    use azul_css::props::style::effects::StyleCursor;

    match cursor {
        StyleCursor::Default => MouseCursorType::Default,
        StyleCursor::Crosshair => MouseCursorType::Crosshair,
        StyleCursor::Pointer => MouseCursorType::Hand,
        StyleCursor::Move => MouseCursorType::Move,
        StyleCursor::Text => MouseCursorType::Text,
        StyleCursor::Wait => MouseCursorType::Wait,
        StyleCursor::Help => MouseCursorType::Help,
        StyleCursor::Progress => MouseCursorType::Progress,
        StyleCursor::ContextMenu => MouseCursorType::ContextMenu,
        StyleCursor::Cell => MouseCursorType::Cell,
        StyleCursor::VerticalText => MouseCursorType::VerticalText,
        StyleCursor::Alias => MouseCursorType::Alias,
        StyleCursor::Copy => MouseCursorType::Copy,
        StyleCursor::Grab => MouseCursorType::Grab,
        StyleCursor::Grabbing => MouseCursorType::Grabbing,
        StyleCursor::AllScroll => MouseCursorType::AllScroll,
        StyleCursor::ZoomIn => MouseCursorType::ZoomIn,
        StyleCursor::ZoomOut => MouseCursorType::ZoomOut,
        StyleCursor::EResize => MouseCursorType::EResize,
        StyleCursor::NResize => MouseCursorType::NResize,
        StyleCursor::SResize => MouseCursorType::SResize,
        StyleCursor::SeResize => MouseCursorType::SeResize,
        StyleCursor::WResize => MouseCursorType::WResize,
        StyleCursor::EwResize => MouseCursorType::EwResize,
        StyleCursor::NsResize => MouseCursorType::NsResize,
        StyleCursor::NeswResize => MouseCursorType::NeswResize,
        StyleCursor::NwseResize => MouseCursorType::NwseResize,
        StyleCursor::ColResize => MouseCursorType::ColResize,
        StyleCursor::RowResize => MouseCursorType::RowResize,
        StyleCursor::Unset => MouseCursorType::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::dom::DomNodeId;
    use azul_core::dom::OptionDomNodeId;

    #[test]
    fn test_full_hit_test_empty() {
        let hit_test = FullHitTest::empty(None);
        assert!(hit_test.is_empty());
        assert!(hit_test.focused_node.is_none());
    }

    #[test]
    fn test_full_hit_test_with_focused_node() {
        let focused = DomNodeId {
            dom: DomId { inner: 0 },
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                NodeId::new(5),
            )),
        };
        let hit_test = FullHitTest::empty(Some(focused));
        assert!(hit_test.is_empty()); // No hovered nodes
        assert_eq!(
            hit_test.focused_node,
            OptionDomNodeId::Some(DomNodeId {
                dom: DomId { inner: 0 },
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                    NodeId::new(5),
                )),
            })
        );
    }

    #[test]
    fn test_cursor_type_hit_test_default() {
        let cursor_test = CursorTypeHitTest::default();
        assert_eq!(cursor_test.cursor_icon, MouseCursorType::Default);
        assert!(cursor_test.cursor_node.is_none());
    }

    #[test]
    fn test_translate_cursor_mapping() {
        use azul_css::props::style::effects::StyleCursor;

        assert_eq!(
            translate_cursor(StyleCursor::Default),
            MouseCursorType::Default
        );
        assert_eq!(
            translate_cursor(StyleCursor::Pointer),
            MouseCursorType::Hand
        );
        assert_eq!(translate_cursor(StyleCursor::Text), MouseCursorType::Text);
        assert_eq!(translate_cursor(StyleCursor::Move), MouseCursorType::Move);
        assert_eq!(
            translate_cursor(StyleCursor::Crosshair),
            MouseCursorType::Crosshair
        );
    }
}
