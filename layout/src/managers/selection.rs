//! Text selection state management
//!
//! Manages text selection ranges across all DOMs.

use alloc::collections::BTreeMap;

use azul_core::{dom::DomId, selection::SelectionState};

/// Manager for text selections across all DOMs
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionManager {
    /// Selection state for each DOM
    /// Maps DomId -> SelectionState
    pub selections: BTreeMap<DomId, SelectionState>,
}

impl Default for SelectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectionManager {
    /// Create a new selection manager
    pub fn new() -> Self {
        Self {
            selections: BTreeMap::new(),
        }
    }

    /// Get the selection state for a DOM
    pub fn get_selection(&self, dom_id: &DomId) -> Option<&SelectionState> {
        self.selections.get(dom_id)
    }

    /// Get mutable selection state for a DOM
    pub fn get_selection_mut(&mut self, dom_id: &DomId) -> Option<&mut SelectionState> {
        self.selections.get_mut(dom_id)
    }

    /// Set the selection state for a DOM
    pub fn set_selection(&mut self, dom_id: DomId, selection: SelectionState) {
        self.selections.insert(dom_id, selection);
    }

    /// Clear the selection for a DOM
    pub fn clear_selection(&mut self, dom_id: &DomId) {
        self.selections.remove(dom_id);
    }

    /// Clear all selections
    pub fn clear_all(&mut self) {
        self.selections.clear();
    }

    /// Get all selections
    pub fn get_all_selections(&self) -> &BTreeMap<DomId, SelectionState> {
        &self.selections
    }

    /// Check if any DOM has an active selection
    pub fn has_any_selection(&self) -> bool {
        // SelectionState doesn't have is_empty in azul-core, so check if selections exist
        !self.selections.is_empty()
    }
}
