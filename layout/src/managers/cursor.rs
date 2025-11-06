//! Text cursor management
//!
//! Manages text cursor position and state for contenteditable elements.
//!
//! # Cursor Lifecycle
//!
//! The cursor is automatically managed in response to focus changes:
//!
//! 1. **Focus lands on contenteditable node**: Cursor initialized at end of text
//! 2. **Focus moves to non-editable node**: Cursor automatically cleared
//! 3. **Focus clears entirely**: Cursor automatically cleared
//!
//! ## Automatic Cursor Initialization
//!
//! When focus is set to a contenteditable node via `FocusManager::set_focused_node()`,
//! the event system (in `window.rs`) checks if the node is contenteditable and calls
//! `CursorManager::initialize_cursor_at_end()` to place the cursor at the end of the text.
//!
//! This happens for:
//! - User clicks on contenteditable element
//! - Tab navigation to contenteditable element
//! - Programmatic focus via `AccessibilityAction::Focus`
//! - Focus from screen reader commands
//!
//! ## Integration with Text Layout
//!
//! The cursor manager uses the `TextLayoutCache` to determine:
//! - Total number of grapheme clusters in the text
//! - Position of the last grapheme cluster (for cursor-at-end)
//! - Bounding rectangles for scroll-into-view
//!
//! ## Scroll-Into-View
//!
//! When a cursor is set, the system automatically checks if it's visible in the
//! viewport. If not, it uses the `ScrollManager` to scroll the minimum amount
//! needed to bring the cursor into view.
//!
//! ## Multi-Cursor Support
//!
//! While the core `TextCursor` type supports multi-cursor editing (used in
//! `text3::edit`), the `CursorManager` currently manages a single cursor for
//! accessibility and user interaction. Multi-cursor scenarios are handled at
//! the `SelectionManager` level with multiple `Selection::Cursor` items.

use azul_core::{
    dom::{DomId, NodeId},
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
};

/// Manager for text cursor position and rendering
#[derive(Debug, Clone, PartialEq)]
pub struct CursorManager {
    /// Current cursor position (if any)
    pub cursor: Option<TextCursor>,
    /// DOM and node where the cursor is located
    pub cursor_location: Option<CursorLocation>,
}

/// Location of a cursor within the DOM
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorLocation {
    pub dom_id: DomId,
    pub node_id: NodeId,
}

impl Default for CursorManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorManager {
    /// Create a new cursor manager with no cursor
    pub fn new() -> Self {
        Self {
            cursor: None,
            cursor_location: None,
        }
    }

    /// Get the current cursor position
    pub fn get_cursor(&self) -> Option<&TextCursor> {
        self.cursor.as_ref()
    }

    /// Get the current cursor location
    pub fn get_cursor_location(&self) -> Option<&CursorLocation> {
        self.cursor_location.as_ref()
    }

    /// Set the cursor position manually
    ///
    /// This is used for programmatic cursor positioning. For automatic
    /// initialization when focusing a contenteditable element, use
    /// `initialize_cursor_at_end()`.
    pub fn set_cursor(&mut self, cursor: Option<TextCursor>, location: Option<CursorLocation>) {
        self.cursor = cursor;
        self.cursor_location = location;
    }

    /// Clear the cursor
    ///
    /// This is automatically called when focus moves to a non-editable node
    /// or when focus is cleared entirely.
    pub fn clear(&mut self) {
        self.cursor = None;
        self.cursor_location = None;
    }

    /// Check if there is an active cursor
    pub fn has_cursor(&self) -> bool {
        self.cursor.is_some()
    }

    /// Initialize cursor at the end of the text in the given node
    ///
    /// This is called automatically when focus lands on a contenteditable element.
    /// It queries the text layout to find the position of the last grapheme
    /// cluster and places the cursor there.
    ///
    /// # Arguments
    ///
    /// * `dom_id` - The DOM containing the node
    /// * `node_id` - The node to initialize the cursor in
    /// * `text_layout` - The text layout result for the node
    ///
    /// # Returns
    ///
    /// `true` if cursor was successfully initialized, `false` if the node has no text
    /// or text layout is not available.
    pub fn initialize_cursor_at_end<T: crate::text3::cache::ParsedFontTrait>(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        text_layout: Option<&alloc::sync::Arc<crate::text3::cache::UnifiedLayout<T>>>,
    ) -> bool {
        // Get the text layout for this node
        let Some(layout) = text_layout else {
            // No text layout - set cursor at start
            self.cursor = Some(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Trailing,
            });
            self.cursor_location = Some(CursorLocation { dom_id, node_id });
            return true;
        };

        // Find the last grapheme cluster in items
        let mut last_cluster_id: Option<GraphemeClusterId> = None;

        // Iterate through all items to find the last cluster
        for item in layout.items.iter().rev() {
            if let crate::text3::cache::ShapedItem::Cluster(cluster) = &item.item {
                last_cluster_id = Some(cluster.source_cluster_id);
                break;
            }
        }

        // Set cursor at the end of the text
        self.cursor = Some(TextCursor {
            cluster_id: last_cluster_id.unwrap_or(GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            }),
            affinity: CursorAffinity::Trailing,
        });

        self.cursor_location = Some(CursorLocation { dom_id, node_id });

        true
    }

    /// Initialize cursor at the start of the text in the given node
    ///
    /// This can be used for specific navigation scenarios (e.g., Ctrl+Home).
    pub fn initialize_cursor_at_start(&mut self, dom_id: DomId, node_id: NodeId) {
        self.cursor = Some(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Trailing,
        });

        self.cursor_location = Some(CursorLocation { dom_id, node_id });
    }

    /// Move the cursor to a specific position
    ///
    /// This is used by text editing operations and keyboard navigation.
    pub fn move_cursor_to(&mut self, cursor: TextCursor, dom_id: DomId, node_id: NodeId) {
        self.cursor = Some(cursor);
        self.cursor_location = Some(CursorLocation { dom_id, node_id });
    }

    /// Check if the cursor is in a specific node
    pub fn is_cursor_in_node(&self, dom_id: DomId, node_id: NodeId) -> bool {
        self.cursor_location
            .as_ref()
            .map(|loc| loc.dom_id == dom_id && loc.node_id == node_id)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_manager_basic_operations() {
        let mut manager = CursorManager::new();

        // Initially no cursor
        assert_eq!(manager.get_cursor(), None);
        assert_eq!(manager.get_cursor_location(), None);
        assert!(!manager.has_cursor());

        // Set cursor
        let cursor = TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 5,
            },
            affinity: CursorAffinity::Leading,
        };
        let location = CursorLocation {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(1),
        };

        manager.set_cursor(Some(cursor.clone()), Some(location.clone()));

        assert_eq!(manager.get_cursor(), Some(&cursor));
        assert_eq!(manager.get_cursor_location(), Some(&location));
        assert!(manager.has_cursor());

        // Clear cursor
        manager.clear();

        assert_eq!(manager.get_cursor(), None);
        assert_eq!(manager.get_cursor_location(), None);
        assert!(!manager.has_cursor());
    }

    #[test]
    fn test_initialize_cursor_at_start() {
        let mut manager = CursorManager::new();

        manager.initialize_cursor_at_start(DomId::ROOT_ID, NodeId::new(5));

        assert!(manager.has_cursor());
        let cursor = manager.get_cursor().unwrap();
        assert_eq!(cursor.cluster_id.source_run, 0);
        assert_eq!(cursor.cluster_id.start_byte_in_run, 0);
        assert_eq!(cursor.affinity, CursorAffinity::Trailing);

        let location = manager.get_cursor_location().unwrap();
        assert_eq!(location.dom_id, DomId::ROOT_ID);
        assert_eq!(location.node_id, NodeId::new(5));
    }

    #[test]
    fn test_move_cursor_to() {
        let mut manager = CursorManager::new();

        // Initial position
        let cursor1 = TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        };
        manager.move_cursor_to(cursor1.clone(), DomId::ROOT_ID, NodeId::new(1));

        assert_eq!(manager.get_cursor(), Some(&cursor1));
        assert_eq!(
            manager.get_cursor_location(),
            Some(&CursorLocation {
                dom_id: DomId::ROOT_ID,
                node_id: NodeId::new(1),
            })
        );

        // Move to new position
        let cursor2 = TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 1,
                start_byte_in_run: 10,
            },
            affinity: CursorAffinity::Trailing,
        };
        manager.move_cursor_to(cursor2.clone(), DomId::ROOT_ID, NodeId::new(2));

        assert_eq!(manager.get_cursor(), Some(&cursor2));
        assert_eq!(
            manager.get_cursor_location(),
            Some(&CursorLocation {
                dom_id: DomId::ROOT_ID,
                node_id: NodeId::new(2),
            })
        );
    }

    #[test]
    fn test_is_cursor_in_node() {
        let mut manager = CursorManager::new();

        // No cursor initially
        assert!(!manager.is_cursor_in_node(DomId::ROOT_ID, NodeId::new(1)));

        // Set cursor in node 1
        manager.initialize_cursor_at_start(DomId::ROOT_ID, NodeId::new(1));

        assert!(manager.is_cursor_in_node(DomId::ROOT_ID, NodeId::new(1)));
        assert!(!manager.is_cursor_in_node(DomId::ROOT_ID, NodeId::new(2)));
        assert!(!manager.is_cursor_in_node(DomId { inner: 1 }, NodeId::new(1)));
    }

    #[test]
    fn test_cursor_affinity() {
        let mut manager = CursorManager::new();

        // Leading affinity
        let cursor_leading = TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 5,
            },
            affinity: CursorAffinity::Leading,
        };
        manager.set_cursor(
            Some(cursor_leading.clone()),
            Some(CursorLocation {
                dom_id: DomId::ROOT_ID,
                node_id: NodeId::new(1),
            }),
        );

        assert_eq!(
            manager.get_cursor().unwrap().affinity,
            CursorAffinity::Leading
        );

        // Trailing affinity
        let cursor_trailing = TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 5,
            },
            affinity: CursorAffinity::Trailing,
        };
        manager.set_cursor(
            Some(cursor_trailing.clone()),
            Some(CursorLocation {
                dom_id: DomId::ROOT_ID,
                node_id: NodeId::new(1),
            }),
        );

        assert_eq!(
            manager.get_cursor().unwrap().affinity,
            CursorAffinity::Trailing
        );
    }

    #[test]
    fn test_cursor_manager_clear_resets_all_state() {
        let mut manager = CursorManager::new();

        // Set cursor with location
        let cursor = TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 2,
                start_byte_in_run: 15,
            },
            affinity: CursorAffinity::Leading,
        };
        let location = CursorLocation {
            dom_id: DomId { inner: 3 },
            node_id: NodeId::new(42),
        };
        manager.set_cursor(Some(cursor), Some(location));

        // Verify state is set
        assert!(manager.has_cursor());
        assert!(manager.get_cursor().is_some());
        assert!(manager.get_cursor_location().is_some());

        // Clear
        manager.clear();

        // Verify all state is cleared
        assert!(!manager.has_cursor());
        assert!(manager.get_cursor().is_none());
        assert!(manager.get_cursor_location().is_none());
    }

    #[test]
    fn test_cursor_location_equality() {
        let loc1 = CursorLocation {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(5),
        };
        let loc2 = CursorLocation {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(5),
        };
        let loc3 = CursorLocation {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(6),
        };
        let loc4 = CursorLocation {
            dom_id: DomId { inner: 1 },
            node_id: NodeId::new(5),
        };

        assert_eq!(loc1, loc2);
        assert_ne!(loc1, loc3);
        assert_ne!(loc1, loc4);
        assert_ne!(loc3, loc4);
    }
}
