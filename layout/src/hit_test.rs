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
    /// Finds the frontmost (lowest depth) node with a cursor property.
    ///
    /// ## Algorithm
    ///
    /// 1. First, check cursor_hit_test_nodes (direct text run hits with TAG_TYPE_CURSOR)
    ///    - These are hit-test areas pushed for text runs with source_node_id
    ///    - The cursor type is encoded directly in the hit-test tag
    /// 2. Then, check regular_hit_test_nodes (DOM nodes with explicit cursor property)
    /// 3. The frontmost (lowest depth) cursor wins
    ///
    /// This approach eliminates the need for text-child-detection hacks because
    /// text runs now have their own hit-test areas with the correct cursor type.
    pub fn new(hit_test: &FullHitTest, layout_window: &LayoutWindow) -> Self {
        use azul_core::hit_test_tag::CursorType;
        
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

            // PHASE 1: Check cursor_hit_test_nodes first (direct text run hits)
            // These are hit-test areas for text runs that have the cursor type
            // encoded directly in the tag, eliminating the need for CSS lookups.
            for (node_id, cursor_hit) in hit_nodes.cursor_hit_test_nodes.iter() {
                let node_depth = cursor_hit.hit_depth;
                
                // Only consider if it's in front of our current best
                if node_depth >= best_depth {
                    continue;
                }
                
                // Convert CursorType to MouseCursorType
                let mouse_cursor = translate_cursor_type(cursor_hit.cursor_type);
                
                // Only use this cursor if it's not the default
                // (allows containers behind text to show their cursor if text has default)
                if mouse_cursor != MouseCursorType::Default {
                    cursor_node = Some((*dom_id, *node_id));
                    cursor_icon = mouse_cursor;
                    best_depth = node_depth;
                }
            }

            // PHASE 2: Check regular_hit_test_nodes (DOM nodes with CSS cursor property)
            for (node_id, hit_item) in hit_nodes.regular_hit_test_nodes.iter() {
                let node_depth = hit_item.hit_depth;
                
                // Only consider this node if it's in front of our current best
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
                }
            }
        }

        Self {
            cursor_node,
            cursor_icon,
        }
    }
}

/// Translate CursorType (from hit-test tag) to MouseCursorType
fn translate_cursor_type(cursor_type: azul_core::hit_test_tag::CursorType) -> MouseCursorType {
    use azul_core::hit_test_tag::CursorType;
    
    match cursor_type {
        CursorType::Default => MouseCursorType::Default,
        CursorType::Pointer => MouseCursorType::Hand,
        CursorType::Text => MouseCursorType::Text,
        CursorType::Crosshair => MouseCursorType::Crosshair,
        CursorType::Move => MouseCursorType::Move,
        CursorType::NotAllowed => MouseCursorType::NotAllowed,
        CursorType::Grab => MouseCursorType::Grab,
        CursorType::Grabbing => MouseCursorType::Grabbing,
        CursorType::EResize => MouseCursorType::EResize,
        CursorType::WResize => MouseCursorType::WResize,
        CursorType::NResize => MouseCursorType::NResize,
        CursorType::SResize => MouseCursorType::SResize,
        CursorType::EwResize => MouseCursorType::EwResize,
        CursorType::NsResize => MouseCursorType::NsResize,
        CursorType::NeswResize => MouseCursorType::NeswResize,
        CursorType::NwseResize => MouseCursorType::NwseResize,
        CursorType::ColResize => MouseCursorType::ColResize,
        CursorType::RowResize => MouseCursorType::RowResize,
        CursorType::Wait => MouseCursorType::Wait,
        CursorType::Help => MouseCursorType::Help,
        CursorType::Progress => MouseCursorType::Progress,
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
