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
//!
//! - User clicks on contenteditable element
//! - Tab navigation to contenteditable element
//! - Programmatic focus via `AccessibilityAction::Focus`
//! - Focus from screen reader commands
//!
//! ## Integration with Text Layout
//!
//! The cursor manager uses the `TextLayoutCache` to determine:
//!
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
    /// # Returns
    ///
    /// `true` if cursor was successfully initialized, `false` if the node has no text
    /// or text layout is not available.
    pub fn initialize_cursor_at_end(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        text_layout: Option<&alloc::sync::Arc<crate::text3::cache::UnifiedLayout>>,
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
