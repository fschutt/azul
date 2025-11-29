//! Text selection state management
//!
//! Manages text selection ranges across all DOMs.

use alloc::collections::BTreeMap;
use core::time::Duration;

use azul_core::{
    dom::DomId,
    geom::LogicalPosition,
    selection::{Selection, SelectionRange, SelectionState, TextCursor},
};

/// Click state for detecting double/triple clicks
#[derive(Debug, Clone, PartialEq)]
pub struct ClickState {
    /// Last clicked node
    pub last_node: Option<azul_core::dom::DomNodeId>,
    /// Last click position
    pub last_position: LogicalPosition,
    /// Last click time (as milliseconds since some epoch)
    pub last_time_ms: u64,
    /// Current click count (1=single, 2=double, 3=triple)
    pub click_count: u8,
}

impl Default for ClickState {
    fn default() -> Self {
        Self {
            last_node: None,
            last_position: LogicalPosition { x: 0.0, y: 0.0 },
            last_time_ms: 0,
            click_count: 0,
        }
    }
}

/// Manager for text selections across all DOMs
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionManager {
    /// Selection state for each DOM
    /// Maps DomId -> SelectionState
    pub selections: BTreeMap<DomId, SelectionState>,
    /// Click state for multi-click detection
    pub click_state: ClickState,
}

impl Default for SelectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectionManager {
    /// Multi-click timeout in milliseconds
    pub const MULTI_CLICK_TIMEOUT_MS: u64 = 500;
    /// Multi-click maximum distance in pixels
    pub const MULTI_CLICK_DISTANCE_PX: f32 = 5.0;

    /// Create a new selection manager
    pub fn new() -> Self {
        Self {
            selections: BTreeMap::new(),
            click_state: ClickState::default(),
        }
    }

    /// Update click count based on position and time
    /// Returns the new click count (1=single, 2=double, 3=triple)
    pub fn update_click_count(
        &mut self,
        node_id: azul_core::dom::DomNodeId,
        position: LogicalPosition,
        current_time_ms: u64,
    ) -> u8 {
        // Check if this is part of multi-click sequence
        let should_increment = if let Some(last_node) = self.click_state.last_node {
            if last_node != node_id {
                return self.reset_click_count(node_id, position, current_time_ms);
            }

            let time_delta = current_time_ms.saturating_sub(self.click_state.last_time_ms);
            if time_delta >= Self::MULTI_CLICK_TIMEOUT_MS {
                return self.reset_click_count(node_id, position, current_time_ms);
            }

            let dx = position.x - self.click_state.last_position.x;
            let dy = position.y - self.click_state.last_position.y;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance >= Self::MULTI_CLICK_DISTANCE_PX {
                return self.reset_click_count(node_id, position, current_time_ms);
            }

            true
        } else {
            false
        };

        let click_count = if should_increment {
            // Cycle: 1 -> 2 -> 3 -> 1
            let new_count = self.click_state.click_count + 1;
            if new_count > 3 {
                1
            } else {
                new_count
            }
        } else {
            1
        };

        self.click_state = ClickState {
            last_node: Some(node_id),
            last_position: position,
            last_time_ms: current_time_ms,
            click_count,
        };

        click_count
    }

    /// Reset click count to 1 (new click sequence)
    fn reset_click_count(
        &mut self,
        node_id: azul_core::dom::DomNodeId,
        position: LogicalPosition,
        current_time_ms: u64,
    ) -> u8 {
        self.click_state = ClickState {
            last_node: Some(node_id),
            last_position: position,
            last_time_ms: current_time_ms,
            click_count: 1,
        };
        1
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

    /// Set a single cursor for a DOM, replacing all existing selections
    pub fn set_cursor(
        &mut self,
        dom_id: DomId,
        node_id: azul_core::dom::DomNodeId,
        cursor: TextCursor,
    ) {
        let mut state = SelectionState {
            selections: alloc::vec![Selection::Cursor(cursor)],
            node_id,
        };
        self.selections.insert(dom_id, state);
    }

    /// Set a selection range for a DOM, replacing all existing selections
    pub fn set_range(
        &mut self,
        dom_id: DomId,
        node_id: azul_core::dom::DomNodeId,
        range: SelectionRange,
    ) {
        let state = SelectionState {
            selections: alloc::vec![Selection::Range(range)],
            node_id,
        };
        self.selections.insert(dom_id, state);
    }

    /// Add a selection to an existing selection state (for multi-cursor support)
    pub fn add_selection(
        &mut self,
        dom_id: DomId,
        node_id: azul_core::dom::DomNodeId,
        selection: Selection,
    ) {
        self.selections
            .entry(dom_id)
            .or_insert_with(|| SelectionState {
                selections: alloc::vec![],
                node_id,
            })
            .add(selection);
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
        !self.selections.is_empty()
    }

    /// Check if a specific DOM has a selection
    pub fn has_selection(&self, dom_id: &DomId) -> bool {
        self.selections.contains_key(dom_id)
    }

    /// Get the primary cursor for a DOM (first cursor in selection list)
    pub fn get_primary_cursor(&self, dom_id: &DomId) -> Option<TextCursor> {
        self.selections
            .get(dom_id)?
            .selections
            .first()
            .and_then(|s| match s {
                Selection::Cursor(c) => Some(*c),
                // Primary cursor is at the end of selection
                Selection::Range(r) => Some(r.end),
            })
    }

    /// Get all selection ranges for a DOM (excludes plain cursors)
    pub fn get_ranges(&self, dom_id: &DomId) -> alloc::vec::Vec<SelectionRange> {
        self.selections
            .get(dom_id)
            .map(|state| {
                state
                    .selections
                    .iter()
                    .filter_map(|s| match s {
                        Selection::Range(r) => Some(r.clone()),
                        Selection::Cursor(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Analyze a click event and return what type of text selection should be performed
    ///
    /// This is used by the event system to determine if a click should trigger
    /// text selection (single/double/triple click).
    ///
    /// ## Returns
    ///
    /// - `Some(1)` - Single click (place cursor)
    /// - `Some(2)` - Double click (select word)
    /// - `Some(3)` - Triple click (select paragraph/line)
    /// - `None` - Not a text selection click (click count > 3 or timeout/distance exceeded)
    pub fn analyze_click_for_selection(
        &self,
        node_id: azul_core::dom::DomNodeId,
        position: LogicalPosition,
        current_time_ms: u64,
    ) -> Option<u8> {
        let click_state = &self.click_state;

        // Check if this continues a multi-click sequence
        if let Some(last_node) = click_state.last_node {
            if last_node != node_id {
                return Some(1); // Different node = new single click
            }

            let time_delta = current_time_ms.saturating_sub(click_state.last_time_ms);
            if time_delta >= Self::MULTI_CLICK_TIMEOUT_MS {
                return Some(1); // Timeout = new single click
            }

            let dx = position.x - click_state.last_position.x;
            let dy = position.y - click_state.last_position.y;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance >= Self::MULTI_CLICK_DISTANCE_PX {
                return Some(1); // Too far = new single click
            }
        } else {
            return Some(1); // No previous click = single click
        }

        // Continue multi-click sequence
        let next_count = click_state.click_count + 1;
        if next_count > 3 {
            Some(1) // Cycle back to single click
        } else {
            Some(next_count)
        }
    }
}

// Clipboard Content Extraction

/// Styled text run for rich clipboard content
#[derive(Debug, Clone, PartialEq)]
pub struct StyledTextRun {
    /// The actual text content
    pub text: String,
    /// Font family name
    pub font_family: Option<String>,
    /// Font size in pixels
    pub font_size_px: f32,
    /// Text color
    pub color: azul_css::props::basic::ColorU,
    /// Whether text is bold
    pub is_bold: bool,
    /// Whether text is italic
    pub is_italic: bool,
}

/// Clipboard content with both plain text and styled (HTML) representation
#[derive(Debug, Clone, PartialEq)]
pub struct ClipboardContent {
    /// Plain text representation (UTF-8)
    pub plain_text: String,
    /// Rich text runs with styling information
    pub styled_runs: alloc::vec::Vec<StyledTextRun>,
}

impl ClipboardContent {
    /// Convert styled runs to HTML for rich clipboard formats
    pub fn to_html(&self) -> String {
        let mut html = String::from("<div>");

        for run in &self.styled_runs {
            html.push_str("<span style=\"");

            if let Some(font_family) = &run.font_family {
                html.push_str(&format!("font-family: {}; ", font_family));
            }
            html.push_str(&format!("font-size: {}px; ", run.font_size_px));
            html.push_str(&format!(
                "color: rgba({}, {}, {}, {}); ",
                run.color.r,
                run.color.g,
                run.color.b,
                run.color.a as f32 / 255.0
            ));
            if run.is_bold {
                html.push_str("font-weight: bold; ");
            }
            if run.is_italic {
                html.push_str("font-style: italic; ");
            }

            html.push_str("\">");
            // Escape HTML entities
            let escaped = run
                .text
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            html.push_str(&escaped);
            html.push_str("</span>");
        }

        html.push_str("</div>");
        html
    }
}

// Trait Implementations for Event Filtering

impl azul_core::events::SelectionManagerQuery for SelectionManager {
    fn get_click_count(&self) -> u8 {
        self.click_state.click_count
    }

    fn get_drag_start_position(&self) -> Option<azul_core::geom::LogicalPosition> {
        // If left mouse button is down and we have a last click position,
        // that's our drag start position
        if self.click_state.click_count > 0 {
            Some(self.click_state.last_position)
        } else {
            None
        }
    }

    fn has_selection(&self) -> bool {
        // Check if any selection exists via:
        // 1. Click count > 0 (single/double/triple click created selection)
        // 2. Drag start position exists (drag selection in progress)
        // 3. Any DOM has non-empty selection state

        if self.click_state.click_count > 0 {
            return true;
        }

        // Check if any DOM has an active selection
        for (_dom_id, selection_state) in &self.selections {
            if !selection_state.selections.is_empty() {
                return true;
            }
        }

        false
    }
}
