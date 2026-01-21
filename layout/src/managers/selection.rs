//! Text selection state management
//!
//! Manages text selection ranges across all DOMs using the browser-style
//! anchor/focus model for multi-node selection.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::time::Duration;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    events::SelectionManagerQuery,
    geom::{LogicalPosition, LogicalRect},
    selection::{
        Selection, SelectionAnchor, SelectionFocus, SelectionRange, SelectionState, SelectionVec,
        TextCursor, TextSelection,
    },
};
use azul_css::{impl_option, impl_option_inner, AzString, OptionString};

/// Click state for detecting double/triple clicks
#[derive(Debug, Clone, PartialEq)]
pub struct ClickState {
    /// Last clicked node
    pub last_node: Option<DomNodeId>,
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
///
/// This manager supports both the legacy per-node selection model and the new
/// browser-style anchor/focus model for multi-node selection.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionManager {
    /// Legacy selection state for each DOM (per-node model)
    /// Maps DomId -> SelectionState
    /// TODO: Deprecate once multi-node selection is fully implemented
    pub selections: BTreeMap<DomId, SelectionState>,
    
    /// New multi-node selection state using anchor/focus model
    /// Maps DomId -> TextSelection
    pub text_selections: BTreeMap<DomId, TextSelection>,
    
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
            text_selections: BTreeMap::new(),
            click_state: ClickState::default(),
        }
    }

    /// Update click count based on position and time
    /// Returns the new click count (1=single, 2=double, 3=triple)
    pub fn update_click_count(
        &mut self,
        node_id: DomNodeId,
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
        node_id: DomNodeId,
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
    pub fn set_cursor(&mut self, dom_id: DomId, node_id: DomNodeId, cursor: TextCursor) {
        let state = SelectionState {
            selections: vec![Selection::Cursor(cursor)].into(),
            node_id,
        };
        self.selections.insert(dom_id, state);
    }

    /// Set a selection range for a DOM, replacing all existing selections
    pub fn set_range(&mut self, dom_id: DomId, node_id: DomNodeId, range: SelectionRange) {
        let state = SelectionState {
            selections: vec![Selection::Range(range)].into(),
            node_id,
        };
        self.selections.insert(dom_id, state);
    }

    /// Add a selection to an existing selection state (for multi-cursor support)
    pub fn add_selection(&mut self, dom_id: DomId, node_id: DomNodeId, selection: Selection) {
        self.selections
            .entry(dom_id)
            .or_insert_with(|| SelectionState {
                selections: SelectionVec::from_const_slice(&[]),
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
            .as_slice()
            .first()
            .and_then(|s| match s {
                Selection::Cursor(c) => Some(c.clone()),
                // Primary cursor is at the end of selection
                Selection::Range(r) => Some(r.end.clone()),
            })
    }

    /// Get all selection ranges for a DOM (excludes plain cursors)
    pub fn get_ranges(&self, dom_id: &DomId) -> alloc::vec::Vec<SelectionRange> {
        self.selections
            .get(dom_id)
            .map(|state| {
                state
                    .selections
                    .as_slice()
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
        node_id: DomNodeId,
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
    
    // ========================================================================
    // NEW: Anchor/Focus model for multi-node selection
    // ========================================================================
    
    /// Start a new text selection with an anchor point.
    ///
    /// This is called on MouseDown. It creates a collapsed selection (cursor)
    /// at the anchor position. The focus will be updated during drag.
    ///
    /// ## Parameters
    /// * `dom_id` - The DOM this selection belongs to
    /// * `ifc_root_node_id` - The IFC root node where the click occurred
    /// * `cursor` - The cursor position within the IFC's UnifiedLayout
    /// * `char_bounds` - Visual bounds of the clicked character
    /// * `mouse_position` - Mouse position in viewport coordinates
    pub fn start_selection(
        &mut self,
        dom_id: DomId,
        ifc_root_node_id: NodeId,
        cursor: TextCursor,
        char_bounds: LogicalRect,
        mouse_position: LogicalPosition,
    ) {
        let selection = TextSelection::new_collapsed(
            dom_id,
            ifc_root_node_id,
            cursor,
            char_bounds,
            mouse_position,
        );
        self.text_selections.insert(dom_id, selection);
    }
    
    /// Update the focus point of an ongoing selection.
    ///
    /// This is called during MouseMove/Drag. It updates the focus position
    /// and recomputes the affected nodes between anchor and focus.
    ///
    /// ## Parameters
    /// * `dom_id` - The DOM this selection belongs to
    /// * `ifc_root_node_id` - The IFC root node where the focus is now
    /// * `cursor` - The cursor position within the IFC's UnifiedLayout
    /// * `mouse_position` - Current mouse position in viewport coordinates
    /// * `affected_nodes` - Pre-computed map of affected IFC roots to their SelectionRanges
    /// * `is_forward` - Whether anchor comes before focus in document order
    ///
    /// ## Returns
    /// * `true` if the selection was updated
    /// * `false` if no selection exists for this DOM
    pub fn update_selection_focus(
        &mut self,
        dom_id: &DomId,
        ifc_root_node_id: NodeId,
        cursor: TextCursor,
        mouse_position: LogicalPosition,
        affected_nodes: BTreeMap<NodeId, SelectionRange>,
        is_forward: bool,
    ) -> bool {
        if let Some(selection) = self.text_selections.get_mut(dom_id) {
            selection.focus = SelectionFocus {
                ifc_root_node_id,
                cursor,
                mouse_position,
            };
            selection.affected_nodes = affected_nodes;
            selection.is_forward = is_forward;
            true
        } else {
            false
        }
    }
    
    /// Get the current text selection for a DOM.
    pub fn get_text_selection(&self, dom_id: &DomId) -> Option<&TextSelection> {
        self.text_selections.get(dom_id)
    }
    
    /// Get mutable reference to the current text selection for a DOM.
    pub fn get_text_selection_mut(&mut self, dom_id: &DomId) -> Option<&mut TextSelection> {
        self.text_selections.get_mut(dom_id)
    }
    
    /// Check if a DOM has an active text selection (new model).
    pub fn has_text_selection(&self, dom_id: &DomId) -> bool {
        self.text_selections.contains_key(dom_id)
    }
    
    /// Get the selection range for a specific IFC root node.
    ///
    /// This is used by the renderer to quickly look up if a node is selected
    /// and get its selection range for `get_selection_rects()`.
    ///
    /// ## Parameters
    /// * `dom_id` - The DOM to check
    /// * `ifc_root_node_id` - The IFC root node to look up
    ///
    /// ## Returns
    /// * `Some(&SelectionRange)` if this node is part of the selection
    /// * `None` if not selected
    pub fn get_range_for_ifc_root(
        &self,
        dom_id: &DomId,
        ifc_root_node_id: &NodeId,
    ) -> Option<&SelectionRange> {
        self.text_selections
            .get(dom_id)?
            .get_range_for_node(ifc_root_node_id)
    }
    
    /// Clear the text selection for a DOM (new model).
    pub fn clear_text_selection(&mut self, dom_id: &DomId) {
        self.text_selections.remove(dom_id);
    }
    
    /// Clear all text selections (new model).
    pub fn clear_all_text_selections(&mut self) {
        self.text_selections.clear();
    }
    
    /// Get all text selections.
    pub fn get_all_text_selections(&self) -> &BTreeMap<DomId, TextSelection> {
        &self.text_selections
    }
}

// Clipboard Content Extraction

/// Styled text run for rich clipboard content
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct StyledTextRun {
    /// The actual text content
    pub text: AzString,
    /// Font family name
    pub font_family: OptionString,
    /// Font size in pixels
    pub font_size_px: f32,
    /// Text color
    pub color: azul_css::props::basic::ColorU,
    /// Whether text is bold
    pub is_bold: bool,
    /// Whether text is italic
    pub is_italic: bool,
}

azul_css::impl_vec!(
    StyledTextRun,
    StyledTextRunVec,
    StyledTextRunVecDestructor,
    StyledTextRunVecDestructorType
);
azul_css::impl_vec_debug!(StyledTextRun, StyledTextRunVec);
azul_css::impl_vec_clone!(StyledTextRun, StyledTextRunVec, StyledTextRunVecDestructor);
azul_css::impl_vec_partialeq!(StyledTextRun, StyledTextRunVec);

/// Clipboard content with both plain text and styled (HTML) representation
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ClipboardContent {
    /// Plain text representation (UTF-8)
    pub plain_text: AzString,
    /// Rich text runs with styling information
    pub styled_runs: StyledTextRunVec,
}

impl_option!(
    ClipboardContent,
    OptionClipboardContent,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl ClipboardContent {
    /// Convert styled runs to HTML for rich clipboard formats
    pub fn to_html(&self) -> String {
        let mut html = String::from("<div>");

        for run in self.styled_runs.as_slice() {
            html.push_str("<span style=\"");

            if let Some(font_family) = run.font_family.as_ref() {
                html.push_str(&format!("font-family: {}; ", font_family.as_str()));
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
                .as_str()
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

impl SelectionManagerQuery for SelectionManager {
    fn get_click_count(&self) -> u8 {
        self.click_state.click_count
    }

    fn get_drag_start_position(&self) -> Option<LogicalPosition> {
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
        //
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

impl SelectionManager {
    /// Remap NodeIds after DOM reconciliation
    ///
    /// When the DOM is regenerated, NodeIds can change. This method updates all
    /// internal state to use the new NodeIds based on the provided mapping.
    pub fn remap_node_ids(
        &mut self,
        dom_id: DomId,
        node_id_map: &std::collections::BTreeMap<azul_core::dom::NodeId, azul_core::dom::NodeId>,
    ) {
        use azul_core::styled_dom::NodeHierarchyItemId;
        
        // Update legacy selection state
        if let Some(selection_state) = self.selections.get_mut(&dom_id) {
            if let Some(old_node_id) = selection_state.node_id.node.into_crate_internal() {
                if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                    selection_state.node_id.node = NodeHierarchyItemId::from_crate_internal(Some(new_node_id));
                } else {
                    // Node was removed, clear selection for this DOM
                    self.selections.remove(&dom_id);
                    return;
                }
            }
        }
        
        // Update text_selections (new multi-node model)
        if let Some(text_selection) = self.text_selections.get_mut(&dom_id) {
            // Update anchor ifc_root_node_id
            let old_anchor_id = text_selection.anchor.ifc_root_node_id;
            if let Some(&new_node_id) = node_id_map.get(&old_anchor_id) {
                text_selection.anchor.ifc_root_node_id = new_node_id;
            } else {
                // Anchor node removed, clear selection
                self.text_selections.remove(&dom_id);
                return;
            }
            
            // Update focus ifc_root_node_id
            let old_focus_id = text_selection.focus.ifc_root_node_id;
            if let Some(&new_node_id) = node_id_map.get(&old_focus_id) {
                text_selection.focus.ifc_root_node_id = new_node_id;
            } else {
                // Focus node removed, clear selection
                self.text_selections.remove(&dom_id);
                return;
            }
            
            // Update affected_nodes map with remapped NodeIds
            let old_affected: Vec<_> = text_selection.affected_nodes.keys().cloned().collect();
            let mut new_affected = std::collections::BTreeMap::new();
            for old_node_id in old_affected {
                if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                    if let Some(range) = text_selection.affected_nodes.remove(&old_node_id) {
                        new_affected.insert(new_node_id, range);
                    }
                }
            }
            text_selection.affected_nodes = new_affected;
        }
        
        // Update click_state last_node if it's in the affected DOM
        if let Some(last_node) = &mut self.click_state.last_node {
            if last_node.dom == dom_id {
                if let Some(old_node_id) = last_node.node.into_crate_internal() {
                    if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                        last_node.node = NodeHierarchyItemId::from_crate_internal(Some(new_node_id));
                    } else {
                        // Node removed, reset click state
                        self.click_state = ClickState::default();
                    }
                }
            }
        }
    }
}
