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
//! ## Cursor Blinking
//!
//! The cursor blinks at ~530ms intervals when a contenteditable element has focus.
//! Blinking is managed by a system timer (`CURSOR_BLINK_TIMER_ID`) that:
//!
//! - Starts when focus lands on a contenteditable element
//! - Stops when focus moves away
//! - Resets (cursor becomes visible) on any user input (keyboard, mouse)
//! - After ~530ms of no input, the cursor toggles visibility
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
    task::Instant,
};

/// Default cursor blink interval in milliseconds
pub const CURSOR_BLINK_INTERVAL_MS: u64 = 530;

/// Manager for text cursor position and rendering
#[derive(Debug, Clone)]
pub struct CursorManager {
    /// Current cursor position (if any)
    pub cursor: Option<TextCursor>,
    /// DOM and node where the cursor is located
    pub cursor_location: Option<CursorLocation>,
    /// Whether the cursor is currently visible (toggled by blink timer)
    pub is_visible: bool,
    /// Timestamp of the last user input event (keyboard, mouse click in text)
    /// Used to determine whether to blink or stay solid while typing
    pub last_input_time: Option<Instant>,
    /// Whether the cursor blink timer is currently active
    pub blink_timer_active: bool,
}

/// Location of a cursor within the DOM
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorLocation {
    pub dom_id: DomId,
    pub node_id: NodeId,
}

impl PartialEq for CursorManager {
    fn eq(&self, other: &Self) -> bool {
        // Ignore is_visible and last_input_time for equality - they're transient state
        self.cursor == other.cursor && self.cursor_location == other.cursor_location
    }
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
            is_visible: false,
            last_input_time: None,
            blink_timer_active: false,
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
        // Make cursor visible when set
        if cursor.is_some() {
            self.is_visible = true;
        }
    }
    
    /// Set the cursor position with timestamp for blink reset
    pub fn set_cursor_with_time(&mut self, cursor: Option<TextCursor>, location: Option<CursorLocation>, now: Instant) {
        self.cursor = cursor;
        self.cursor_location = location;
        if cursor.is_some() {
            self.is_visible = true;
            self.last_input_time = Some(now);
        }
    }

    /// Clear the cursor
    ///
    /// This is automatically called when focus moves to a non-editable node
    /// or when focus is cleared entirely.
    pub fn clear(&mut self) {
        self.cursor = None;
        self.cursor_location = None;
        self.is_visible = false;
        self.last_input_time = None;
        self.blink_timer_active = false;
    }

    /// Check if there is an active cursor
    pub fn has_cursor(&self) -> bool {
        self.cursor.is_some()
    }
    
    /// Check if the cursor should be drawn (has cursor AND is visible)
    pub fn should_draw_cursor(&self) -> bool {
        self.cursor.is_some() && self.is_visible
    }
    
    /// Reset the blink state on user input
    ///
    /// This makes the cursor visible and records the input time.
    /// The blink timer will keep the cursor visible until `CURSOR_BLINK_INTERVAL_MS`
    /// has passed since this time.
    pub fn reset_blink_on_input(&mut self, now: Instant) {
        self.is_visible = true;
        self.last_input_time = Some(now);
    }
    
    /// Toggle cursor visibility (called by blink timer)
    ///
    /// Returns the new visibility state.
    pub fn toggle_visibility(&mut self) -> bool {
        self.is_visible = !self.is_visible;
        self.is_visible
    }
    
    /// Set cursor visibility directly
    pub fn set_visibility(&mut self, visible: bool) {
        self.is_visible = visible;
    }
    
    /// Check if enough time has passed since last input to start blinking
    ///
    /// Returns true if the cursor should blink (toggle visibility),
    /// false if it should stay solid (user is actively typing).
    pub fn should_blink(&self, now: &Instant) -> bool {
        use azul_core::task::{Duration, SystemTimeDiff};
        
        match &self.last_input_time {
            Some(last_input) => {
                let elapsed = now.duration_since(last_input);
                let blink_interval = Duration::System(SystemTimeDiff::from_millis(CURSOR_BLINK_INTERVAL_MS));
                // If elapsed time is greater than blink interval, allow blinking
                elapsed.greater_than(&blink_interval)
            }
            None => true, // No input recorded, allow blinking
        }
    }
    
    /// Mark the blink timer as active
    pub fn set_blink_timer_active(&mut self, active: bool) {
        self.blink_timer_active = active;
    }
    
    /// Check if the blink timer is active
    pub fn is_blink_timer_active(&self) -> bool {
        self.blink_timer_active
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
        eprintln!("[DEBUG] initialize_cursor_at_end: dom_id={:?}, node_id={:?}, has_layout={}", dom_id, node_id, text_layout.is_some());
        
        // Get the text layout for this node
        let Some(layout) = text_layout else {
            // No text layout - set cursor at start
            eprintln!("[DEBUG] No text layout, setting cursor at start");
            self.cursor = Some(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Trailing,
            });
            self.cursor_location = Some(CursorLocation { dom_id, node_id });
            self.is_visible = true; // Make cursor visible immediately
            eprintln!("[DEBUG] Cursor set: {:?}", self.cursor);
            return true;
        };

        // Find the last grapheme cluster in items
        let mut last_cluster_id: Option<GraphemeClusterId> = None;
        eprintln!("[DEBUG] Layout has {} items", layout.items.len());

        // Iterate through all items to find the last cluster
        for item in layout.items.iter().rev() {
            if let crate::text3::cache::ShapedItem::Cluster(cluster) = &item.item {
                last_cluster_id = Some(cluster.source_cluster_id);
                eprintln!("[DEBUG] Found last cluster: {:?}", last_cluster_id);
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
        self.is_visible = true; // Make cursor visible immediately
        eprintln!("[DEBUG] Cursor initialized: cursor={:?}, location={:?}", self.cursor, self.cursor_location);

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
    
    /// Get the DomNodeId where the cursor is located (for cross-frame tracking)
    pub fn get_cursor_node(&self) -> Option<azul_core::dom::DomNodeId> {
        self.cursor_location.as_ref().map(|loc| {
            azul_core::dom::DomNodeId {
                dom: loc.dom_id,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(loc.node_id)),
            }
        })
    }
    
    /// Update the NodeId for the cursor location (after DOM reconciliation)
    ///
    /// This is called when the DOM is regenerated and NodeIds change.
    /// The cursor position within the text is preserved.
    pub fn update_node_id(&mut self, new_node: azul_core::dom::DomNodeId) {
        if let Some(ref mut loc) = self.cursor_location {
            if let Some(new_id) = new_node.node.into_crate_internal() {
                loc.dom_id = new_node.dom;
                loc.node_id = new_id;
            }
        }
    }
    
    /// Remap NodeIds after DOM reconciliation
    ///
    /// When the DOM is regenerated, NodeIds can change. This method updates
    /// the cursor location to use the new NodeId based on the provided mapping.
    pub fn remap_node_ids(
        &mut self,
        dom_id: DomId,
        node_id_map: &std::collections::BTreeMap<NodeId, NodeId>,
    ) {
        if let Some(ref mut loc) = self.cursor_location {
            if loc.dom_id == dom_id {
                if let Some(&new_node_id) = node_id_map.get(&loc.node_id) {
                    loc.node_id = new_node_id;
                } else {
                    // Node was removed, clear cursor location
                    self.cursor_location = None;
                }
            }
        }
    }
}
