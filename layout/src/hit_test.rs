//! Hit-testing logic for layout windows
//!
//! This module handles determining which DOM nodes are under the mouse cursor
//! and resolving the cursor icon based on CSS cursor properties.

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
    /// This scans all hovered nodes for CSS cursor properties and returns the
    /// cursor type of the closest node with a non-default cursor.
    pub fn new(hit_test: &FullHitTest, layout_window: &LayoutWindow) -> Self {
        let mut cursor_node = None;
        let mut cursor_icon = MouseCursorType::Default;

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

            // Check each hit node for a cursor property
            for (node_id, _hit_item) in hit_nodes.regular_hit_test_nodes.iter() {
                // Query the CSS cursor property for this node
                if let Some(cursor_prop) = styled_dom.get_css_property_cache().get_cursor(
                    &node_data_container[*node_id],
                    node_id,
                    &styled_nodes[*node_id].styled_node_state,
                ) {
                    cursor_node = Some((*dom_id, *node_id));
                    let css_cursor = cursor_prop.get_property().copied().unwrap_or_default();
                    cursor_icon = translate_cursor(css_cursor);
                    // Found a node with a cursor property, use it
                    // (we could break here, but iterating all ensures we get the topmost)
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
            Some((DomId { inner: 0 }, NodeId::new(5)))
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
