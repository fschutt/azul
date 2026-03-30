//! Unified text editing manager
//!
//! Merges CursorManager and SelectionManager into a single source of truth
//! for all text editing state. Every mutation that affects visual output sets
//! `display_list_dirty = true`, ensuring the display list is always regenerated.
//!
//! This eliminates bugs caused by forgetting to update one manager when
//! another changes (e.g., click updates selection but not cursor, or arrow
//! keys move cursor but don't regenerate display list).

use super::cursor::CursorManager;
use super::selection::SelectionManager;

/// Unified text editing manager.
///
/// Owns both cursor and selection state, plus a dirty flag that ensures
/// the display list is regenerated after any mutation that affects rendering.
#[derive(Debug, Clone)]
pub struct TextEditManager {
    /// Cursor position, blink state, and IME preedit
    pub cursor_manager: CursorManager,
    /// Selection ranges (legacy + anchor/focus model) and click state
    pub selection_manager: SelectionManager,
    /// Set to true by any mutation that changes visual output.
    /// The event loop checks this and calls `regenerate_display_list_for_dom()`.
    pub display_list_dirty: bool,
}

impl Default for TextEditManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for TextEditManager {
    fn eq(&self, other: &Self) -> bool {
        self.cursor_manager == other.cursor_manager
            && self.selection_manager == other.selection_manager
    }
}

impl TextEditManager {
    /// Create a new text edit manager with no active editing state
    pub fn new() -> Self {
        Self {
            cursor_manager: CursorManager::new(),
            selection_manager: SelectionManager::new(),
            display_list_dirty: false,
        }
    }

    /// Check and clear the display_list_dirty flag.
    ///
    /// Returns true if the display list needs regeneration.
    /// Clears the flag so subsequent calls return false until the next mutation.
    pub fn take_display_list_dirty(&mut self) -> bool {
        let v = self.display_list_dirty;
        self.display_list_dirty = false;
        v
    }

    /// Mark that the display list needs regeneration.
    pub fn mark_dirty(&mut self) {
        self.display_list_dirty = true;
    }
}
