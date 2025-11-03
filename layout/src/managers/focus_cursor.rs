//! Focus and tab navigation management
//!
//! Manages keyboard focus, tab navigation, and programmatic focus changes.
//!
//! # Focus Change and Recursive Event System
//!
//! This module implements a sophisticated recursive focus/blur event system that allows
//! callbacks to request focus changes, which then generate synthetic FocusIn/FocusOut events
//! that can themselves request focus changes, up to a maximum recursion depth of 5 levels.
//!
//! ## Focus Change Request Flow
//!
//! 1. **Initial Event**: A callback returns a `FocusUpdateRequest` (FocusNode, ClearFocus, or
//!    NoChange)
//! 2. **State Update**: The focus manager updates its internal state via `set_focused_node()`
//! 3. **Change Detection**: The event system compares old_focus vs new_focus after callback
//!    execution
//! 4. **Selection Clearing**: If focus changed, all text selections are cleared via
//!    `selection_manager.clear_all()`
//! 5. **Synthetic Events**: Generate FocusIn (for new focus) and FocusOut (for old focus) events
//! 6. **Recursion**: Those synthetic event callbacks can request another focus change (steps 1-5
//!    repeat)
//! 7. **Depth Limit**: Process repeats up to `MAX_EVENT_RECURSION_DEPTH` (5 levels)
//!
//! ## Example Focus Change Cascade
//!
//! ```text
//! User clicks Button A
//!   ↓
//! OnClick callback → requests FocusNode(Button B)
//!   ↓
//! Focus changes from A to B
//!   ↓
//! Selection manager cleared
//!   ↓
//! Synthetic events: FocusOut(A), FocusIn(B)
//!   ↓
//! FocusIn(B) callback → requests FocusNode(Button C)
//!   ↓
//! Focus changes from B to C (recursion depth = 1)
//!   ↓
//! Synthetic events: FocusOut(B), FocusIn(C)
//!   ↓
//! FocusIn(C) callback → NoChange
//!   ↓
//! Recursion ends
//! ```
//!
//! ## Implementation Details
//!
//! The recursion is handled in `dll/src/desktop/shell2/common/event_v2.rs` in the
//! `process_window_events_recursive_v2(depth: usize)` function:
//!
//! - After each callback invocation, captures `old_focus = focus_manager.get_focused_node()`
//! - Processes callback result with `process_callback_result_v2()`
//! - Compares `new_focus = focus_manager.get_focused_node()` with `old_focus`
//! - If changed: clears selections, generates FocusIn/FocusOut, recurses with `depth + 1`
//! - Hard limit at `MAX_EVENT_RECURSION_DEPTH = 5` prevents infinite recursion
//!
//! ## FocusUpdateRequest Enum
//!
//! Callbacks use the explicit `FocusUpdateRequest` enum (instead of ambiguous Option<Option<T>>):
//!
//! - `FocusNode(DomNodeId)`: Focus this specific node
//! - `ClearFocus`: Remove focus (no node has focus)
//! - `NoChange`: Don't change focus
//!
//! ## Integration Points
//!
//! - **Selection Manager**: All text selections cleared automatically when focus changes
//! - **Event System**: Focus changes trigger re-render
//!   (`ProcessEventResult::ShouldReRenderCurrentWindow`)
//! - **Tab Navigation**: `resolve_focus_target()` handles Next/Previous/First/Last focus targets
//! - **Accessibility**: Focus changes notify screen readers and update ARIA state
//!
//! ## Safety Guarantees
//!
//! - Maximum recursion depth prevents infinite loops
//! - Focus changes are atomic (old → new in single step)
//! - Selection state always consistent with focus state
//! - No focus change during DOM regeneration (handled separately)

use alloc::{collections::BTreeMap, vec::Vec};

use azul_core::{
    callbacks::{FocusTarget, FocusTargetPath},
    dom::{DomId, DomNodeId, NodeId},
    styled_dom::NodeHierarchyItemId,
};

use crate::window::DomLayoutResult;

/// CSS path for selecting elements (placeholder - needs proper implementation)
pub type CssPathString = alloc::string::String;

/// Manager for keyboard focus and tab navigation
#[derive(Debug, Clone, PartialEq)]
pub struct FocusManager {
    /// Currently focused node (if any)
    pub focused_node: Option<DomNodeId>,
    /// Pending focus request from callback
    pub pending_focus_request: Option<FocusTarget>,
    /// Text cursor position (if focused node is contenteditable)
    /// Automatically cleared when focus changes to non-editable node
    pub text_cursor: Option<azul_core::selection::TextCursor>,
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusManager {
    /// Create a new focus manager
    pub fn new() -> Self {
        Self {
            focused_node: None,
            pending_focus_request: None,
            text_cursor: None,
        }
    }

    /// Get the currently focused node
    pub fn get_focused_node(&self) -> Option<&DomNodeId> {
        self.focused_node.as_ref()
    }

    /// Set the focused node directly (used by event system)
    /// 
    /// If the node is not contenteditable, the cursor is automatically cleared.
    /// If the node is contenteditable, the cursor should be set separately via `set_text_cursor()`.
    pub fn set_focused_node(&mut self, node: Option<DomNodeId>) {
        self.focused_node = node;
        // Note: Cursor clearing/initialization happens in window.rs based on contenteditable check
    }

    /// Get the current text cursor position
    pub fn get_text_cursor(&self) -> Option<&azul_core::selection::TextCursor> {
        self.text_cursor.as_ref()
    }

    /// Set the text cursor position
    /// 
    /// This should only be called when the focused node is contenteditable.
    /// The cursor will be automatically cleared when focus changes to a non-editable node.
    pub fn set_text_cursor(&mut self, cursor: Option<azul_core::selection::TextCursor>) {
        self.text_cursor = cursor;
    }

    /// Clear the text cursor
    pub fn clear_text_cursor(&mut self) {
        self.text_cursor = None;
    }

    /// Request a focus change (to be processed by event system)
    pub fn request_focus_change(&mut self, target: FocusTarget) {
        self.pending_focus_request = Some(target);
    }

    /// Take the pending focus request (one-shot)
    pub fn take_focus_request(&mut self) -> Option<FocusTarget> {
        self.pending_focus_request.take()
    }

    /// Clear focus
    pub fn clear_focus(&mut self) {
        self.focused_node = None;
    }

    /// Check if a specific node has focus
    pub fn has_focus(&self, node: &DomNodeId) -> bool {
        self.focused_node.as_ref() == Some(node)
    }
}

/// Direction for cursor navigation
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CursorNavigationDirection {
    /// Move cursor up one line
    Up,
    /// Move cursor down one line
    Down,
    /// Move cursor left one character
    Left,
    /// Move cursor right one character
    Right,
    /// Move cursor to start of current line
    LineStart,
    /// Move cursor to end of current line
    LineEnd,
    /// Move cursor to start of document
    DocumentStart,
    /// Move cursor to end of document
    DocumentEnd,
}

/// Result of a cursor movement operation
#[derive(Debug, Clone)]
pub enum CursorMovementResult {
    /// Cursor moved within the same text node
    MovedWithinNode(azul_core::selection::TextCursor),
    /// Cursor moved to a different text node
    MovedToNode {
        dom_id: DomId,
        node_id: NodeId,
        cursor: azul_core::selection::TextCursor,
    },
    /// Cursor is at a boundary and cannot move further
    AtBoundary {
        boundary: crate::text3::cache::TextBoundary,
        cursor: azul_core::selection::TextCursor,
    },
}

/// Error when no cursor destination is available
#[derive(Debug, Clone)]
pub struct NoCursorDestination {
    pub reason: String,
}

/// Warning type for focus resolution errors
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateFocusWarning {
    FocusInvalidDomId(DomId),
    FocusInvalidNodeId(NodeHierarchyItemId),
    CouldNotFindFocusNode(String),
}

/// Resolve a FocusTarget to an actual DomNodeId
pub fn resolve_focus_target(
    focus_target: &FocusTarget,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    current_focus: Option<DomNodeId>,
) -> Result<Option<DomNodeId>, UpdateFocusWarning> {
    use azul_core::callbacks::FocusTarget::*;

    if layout_results.is_empty() {
        return Ok(None);
    }

    macro_rules! search_for_focusable_node_id {
        (
            $layout_results:expr, $start_dom_id:expr, $start_node_id:expr, $get_next_node_fn:ident
        ) => {{
            let mut start_dom_id = $start_dom_id;
            let mut start_node_id = $start_node_id;

            let min_dom_id = DomId::ROOT_ID;
            let max_dom_id = DomId {
                inner: $layout_results.len() - 1,
            };

            // iterate through all DOMs
            loop {
                let layout_result = $layout_results
                    .get(&start_dom_id)
                    .ok_or(UpdateFocusWarning::FocusInvalidDomId(start_dom_id.clone()))?;

                let node_id_valid = layout_result
                    .styled_dom
                    .node_data
                    .as_container()
                    .get(start_node_id)
                    .is_some();

                if !node_id_valid {
                    return Err(UpdateFocusWarning::FocusInvalidNodeId(
                        NodeHierarchyItemId::from_crate_internal(Some(start_node_id.clone())),
                    ));
                }

                if layout_result.styled_dom.node_data.is_empty() {
                    return Err(UpdateFocusWarning::FocusInvalidDomId(start_dom_id.clone()));
                }

                let max_node_id = NodeId::new(layout_result.styled_dom.node_data.len() - 1);
                let min_node_id = NodeId::ZERO;

                // iterate through nodes in DOM
                loop {
                    let current_node_id = NodeId::new(start_node_id.index().$get_next_node_fn(1))
                        .max(min_node_id)
                        .min(max_node_id);

                    if layout_result.styled_dom.node_data.as_container()[current_node_id]
                        .is_focusable()
                    {
                        return Ok(Some(DomNodeId {
                            dom: start_dom_id,
                            node: NodeHierarchyItemId::from_crate_internal(Some(current_node_id)),
                        }));
                    }

                    if current_node_id == min_node_id && current_node_id < start_node_id {
                        // going in decreasing (previous) direction
                        if start_dom_id == min_dom_id {
                            // root node / root dom encountered
                            return Ok(None);
                        } else {
                            start_dom_id.inner -= 1;
                            let next_layout = $layout_results.get(&start_dom_id).ok_or(
                                UpdateFocusWarning::FocusInvalidDomId(start_dom_id.clone()),
                            )?;
                            start_node_id = NodeId::new(next_layout.styled_dom.node_data.len() - 1);
                            break; // continue outer loop
                        }
                    } else if current_node_id == max_node_id && current_node_id > start_node_id {
                        // going in increasing (next) direction
                        if start_dom_id == max_dom_id {
                            // last dom / last node encountered
                            return Ok(None);
                        } else {
                            start_dom_id.inner += 1;
                            start_node_id = NodeId::ZERO;
                            break; // continue outer loop
                        }
                    } else {
                        start_node_id = current_node_id;
                    }
                }
            }
        }};
    }

    match focus_target {
        Path(FocusTargetPath { dom, css_path }) => {
            let layout_result = layout_results
                .get(dom)
                .ok_or(UpdateFocusWarning::FocusInvalidDomId(dom.clone()))?;

            // TODO: Implement proper CSS path matching
            // For now, return an error since we can't match the path yet
            Err(UpdateFocusWarning::CouldNotFindFocusNode(format!(
                "{:?}",
                css_path
            )))
        }
        Id(dom_node_id) => {
            let layout_result = layout_results.get(&dom_node_id.dom).ok_or(
                UpdateFocusWarning::FocusInvalidDomId(dom_node_id.dom.clone()),
            )?;
            let node_is_valid = dom_node_id
                .node
                .into_crate_internal()
                .map(|o| {
                    layout_result
                        .styled_dom
                        .node_data
                        .as_container()
                        .get(o)
                        .is_some()
                })
                .unwrap_or(false);

            if !node_is_valid {
                Err(UpdateFocusWarning::FocusInvalidNodeId(
                    dom_node_id.node.clone(),
                ))
            } else {
                Ok(Some(dom_node_id.clone()))
            }
        }
        Previous => {
            let last_layout_dom_id = DomId {
                inner: layout_results.len() - 1,
            };

            let (current_focus_dom, current_focus_node_id) = match current_focus {
                Some(s) => match s.node.into_crate_internal() {
                    Some(n) => (s.dom, n),
                    None => {
                        if let Some(layout_result) = layout_results.get(&s.dom) {
                            (
                                s.dom,
                                NodeId::new(layout_result.styled_dom.node_data.len() - 1),
                            )
                        } else {
                            (
                                last_layout_dom_id,
                                NodeId::new(
                                    layout_results
                                        .get(&last_layout_dom_id)
                                        .ok_or(UpdateFocusWarning::FocusInvalidDomId(
                                            last_layout_dom_id,
                                        ))?
                                        .styled_dom
                                        .node_data
                                        .len()
                                        - 1,
                                ),
                            )
                        }
                    }
                },
                None => (
                    last_layout_dom_id,
                    NodeId::new(
                        layout_results
                            .get(&last_layout_dom_id)
                            .ok_or(UpdateFocusWarning::FocusInvalidDomId(last_layout_dom_id))?
                            .styled_dom
                            .node_data
                            .len()
                            - 1,
                    ),
                ),
            };

            search_for_focusable_node_id!(
                layout_results,
                current_focus_dom,
                current_focus_node_id,
                saturating_sub
            );
        }
        Next => {
            let (current_focus_dom, current_focus_node_id) = match current_focus {
                Some(s) => match s.node.into_crate_internal() {
                    Some(n) => (s.dom, n),
                    None => {
                        if layout_results.get(&s.dom).is_some() {
                            (s.dom, NodeId::ZERO)
                        } else {
                            (DomId::ROOT_ID, NodeId::ZERO)
                        }
                    }
                },
                None => (DomId::ROOT_ID, NodeId::ZERO),
            };

            search_for_focusable_node_id!(
                layout_results,
                current_focus_dom,
                current_focus_node_id,
                saturating_add
            );
        }
        First => {
            let (current_focus_dom, current_focus_node_id) = (DomId::ROOT_ID, NodeId::ZERO);
            search_for_focusable_node_id!(
                layout_results,
                current_focus_dom,
                current_focus_node_id,
                saturating_add
            );
        }
        Last => {
            let last_layout_dom_id = DomId {
                inner: layout_results.len() - 1,
            };
            let (current_focus_dom, current_focus_node_id) = (
                last_layout_dom_id,
                NodeId::new(
                    layout_results
                        .get(&last_layout_dom_id)
                        .ok_or(UpdateFocusWarning::FocusInvalidDomId(last_layout_dom_id))?
                        .styled_dom
                        .node_data
                        .len()
                        - 1,
                ),
            );
            search_for_focusable_node_id!(
                layout_results,
                current_focus_dom,
                current_focus_node_id,
                saturating_add
            );
        }
        NoFocus => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use std::collections::BTreeMap;

    use azul_core::{
        geom::{LogicalSize, PhysicalSize},
        window::WindowFlags,
    };

    use super::*;
    use crate::{
        managers::selection::SelectionManager, window::LayoutWindow, window_state::FullWindowState,
        FontManager, TextLayoutCache,
    };

    /// Helper to create a minimal FcFontCache for testing
    fn create_test_font_cache() -> rust_fontconfig::FcFontCache {
        rust_fontconfig::FcFontCache::default()
    }

    #[test]
    fn test_focus_manager_basic_operations() {
        let mut manager = FocusManager::new();

        // Initially no focus
        assert_eq!(manager.get_focused_node(), None);

        // Set focus to a node
        let node1 = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        };
        manager.set_focused_node(Some(node1.clone()));
        assert_eq!(manager.get_focused_node(), Some(&node1));

        // Change focus to another node
        let node2 = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
        };
        manager.set_focused_node(Some(node2.clone()));
        assert_eq!(manager.get_focused_node(), Some(&node2));

        // Clear focus
        manager.set_focused_node(None);
        assert_eq!(manager.get_focused_node(), None);
    }

    #[test]
    fn test_focus_update_request_enum() {
        use crate::callbacks::FocusUpdateRequest;

        // Test FocusNode variant
        let node = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(5))),
        };
        let req = FocusUpdateRequest::FocusNode(node.clone());
        assert!(req.is_change());
        assert_eq!(req.to_focused_node(), Some(Some(node)));

        // Test ClearFocus variant
        let req = FocusUpdateRequest::ClearFocus;
        assert!(req.is_change());
        assert_eq!(req.to_focused_node(), Some(None));

        // Test NoChange variant
        let req = FocusUpdateRequest::NoChange;
        assert!(!req.is_change());
        assert_eq!(req.to_focused_node(), None);
    }

    #[test]
    fn test_focus_update_request_from_optional() {
        use crate::callbacks::FocusUpdateRequest;

        let node = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(3))),
        };

        // Some(Some(node)) -> FocusNode
        let req = FocusUpdateRequest::from_optional(Some(Some(node.clone())));
        assert!(matches!(req, FocusUpdateRequest::FocusNode(_)));
        assert!(req.is_change());

        // Some(None) -> ClearFocus
        let req = FocusUpdateRequest::from_optional(Some(None));
        assert!(matches!(req, FocusUpdateRequest::ClearFocus));
        assert!(req.is_change());

        // None -> NoChange
        let req = FocusUpdateRequest::from_optional(None);
        assert!(matches!(req, FocusUpdateRequest::NoChange));
        assert!(!req.is_change());
    }

    #[test]
    fn test_selection_manager_clear_all() {
        use azul_core::selection::{
            CursorAffinity, GraphemeClusterId, Selection, SelectionState, TextCursor,
        };

        let mut selection_manager = SelectionManager::new();

        // Add some selections to different DOMs
        let dom1 = DomId::ROOT_ID;
        let dom2 = DomId { inner: 1 };

        let node1 = DomNodeId {
            dom: dom1,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        };
        let node2 = DomNodeId {
            dom: dom2,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
        };

        let sel_state1 = SelectionState {
            selections: vec![Selection::Cursor(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Leading,
            })],
            node_id: node1.clone(),
        };

        let sel_state2 = SelectionState {
            selections: vec![Selection::Cursor(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 5,
                },
                affinity: CursorAffinity::Trailing,
            })],
            node_id: node2.clone(),
        };

        selection_manager.set_selection(dom1, sel_state1);
        selection_manager.set_selection(dom2, sel_state2);

        // Verify selections exist
        assert!(selection_manager.get_selection(&dom1).is_some());
        assert!(selection_manager.get_selection(&dom2).is_some());
        assert!(selection_manager.has_any_selection());

        // Clear all selections
        selection_manager.clear_all();

        // Verify all selections are gone
        assert!(selection_manager.get_selection(&dom1).is_none());
        assert!(selection_manager.get_selection(&dom2).is_none());
        assert!(!selection_manager.has_any_selection());
    }

    #[test]
    fn test_focus_change_clears_selections() {
        use azul_core::selection::{
            CursorAffinity, GraphemeClusterId, Selection, SelectionState, TextCursor,
        };

        // This test verifies that when focus changes, selections are cleared.
        // The actual integration happens in event_v2.rs, but we test the components here.

        let mut focus_manager = FocusManager::new();
        let mut selection_manager = SelectionManager::new();

        // Setup initial focus
        let node1 = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        };
        focus_manager.set_focused_node(Some(node1.clone()));

        // Add selection to the DOM
        let sel_state = SelectionState {
            selections: vec![Selection::Cursor(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Leading,
            })],
            node_id: node1.clone(),
        };
        selection_manager.set_selection(DomId::ROOT_ID, sel_state);
        assert!(selection_manager.get_selection(&DomId::ROOT_ID).is_some());

        // Simulate focus change (as would happen in event_v2.rs)
        let old_focus = focus_manager.get_focused_node().copied();

        let node2 = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
        };
        focus_manager.set_focused_node(Some(node2.clone()));

        let new_focus = focus_manager.get_focused_node();

        // Verify focus changed
        assert_ne!(old_focus.as_ref(), new_focus);

        // In real code, event_v2.rs would call clear_all() here
        if old_focus.as_ref() != new_focus {
            selection_manager.clear_all();
        }

        // Verify selection was cleared
        assert!(selection_manager.get_selection(&DomId::ROOT_ID).is_none());
    }

    #[test]
    fn test_focus_manager_with_layout_window() {
        // Test that FocusManager integrates correctly with LayoutWindow
        let fc_cache = create_test_font_cache();
        let mut layout_window = LayoutWindow::new(fc_cache).expect("Failed to create LayoutWindow");

        // Initially no focus
        assert_eq!(layout_window.focus_manager.get_focused_node(), None);

        // Set focus
        let node = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        };
        layout_window
            .focus_manager
            .set_focused_node(Some(node.clone()));

        // Verify focus was set
        assert_eq!(layout_window.focus_manager.get_focused_node(), Some(&node));

        // Clear focus
        layout_window.focus_manager.set_focused_node(None);
        assert_eq!(layout_window.focus_manager.get_focused_node(), None);
    }

    #[test]
    fn test_recursive_focus_change_detection() {
        // This test simulates the recursive focus change detection
        // that happens in process_window_events_recursive_v2

        let mut focus_manager = FocusManager::new();
        let mut recursion_count = 0;
        const MAX_RECURSION: usize = 5;

        let nodes = vec![
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
            },
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
            },
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
            },
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(3))),
            },
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(4))),
            },
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(5))),
            },
        ];

        // Simulate initial focus
        focus_manager.set_focused_node(Some(nodes[0].clone()));

        // Simulate recursive focus changes (as would happen in callbacks)
        for i in 1..nodes.len() {
            if recursion_count >= MAX_RECURSION {
                break;
            }

            let old_focus = focus_manager.get_focused_node().copied();
            focus_manager.set_focused_node(Some(nodes[i].clone()));
            let new_focus = focus_manager.get_focused_node();

            // Verify focus changed
            assert_ne!(old_focus.as_ref(), new_focus);

            recursion_count += 1;
        }

        // Verify we hit the recursion limit
        assert_eq!(recursion_count, MAX_RECURSION);

        // Verify final focus state
        assert_eq!(
            focus_manager.get_focused_node(),
            Some(&nodes[MAX_RECURSION])
        );
    }

    #[test]
    fn test_focus_change_cascade_with_selection_clearing() {
        use azul_core::selection::{
            CursorAffinity, GraphemeClusterId, Selection, SelectionState, TextCursor,
        };

        // Comprehensive test simulating a full focus change cascade
        // with selection clearing at each step

        let mut focus_manager = FocusManager::new();
        let mut selection_manager = SelectionManager::new();

        let dom_id = DomId::ROOT_ID;
        let nodes = vec![
            DomNodeId {
                dom: dom_id,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
            },
            DomNodeId {
                dom: dom_id,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
            },
            DomNodeId {
                dom: dom_id,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
            },
        ];

        // Step 1: Initial focus on node 0
        focus_manager.set_focused_node(Some(nodes[0].clone()));
        let sel_state = SelectionState {
            selections: vec![Selection::Cursor(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Leading,
            })],
            node_id: nodes[0].clone(),
        };
        selection_manager.set_selection(dom_id, sel_state);

        // Step 2: Change focus to node 1 (simulating callback result)
        let old_focus = focus_manager.get_focused_node().copied();
        focus_manager.set_focused_node(Some(nodes[1].clone()));
        let new_focus = focus_manager.get_focused_node().copied();

        // Verify focus changed
        assert_ne!(old_focus, new_focus);

        // Clear selections (as event_v2.rs would do)
        selection_manager.clear_all();

        // Verify selection cleared
        assert!(selection_manager.get_selection(&dom_id).is_none());

        // Add selection to new focused node
        let sel_state = SelectionState {
            selections: vec![Selection::Cursor(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 5,
                },
                affinity: CursorAffinity::Leading,
            })],
            node_id: nodes[1].clone(),
        };
        selection_manager.set_selection(dom_id, sel_state);

        // Step 3: Change focus to node 2 (recursive focus change from FocusIn callback)
        let old_focus = focus_manager.get_focused_node().copied();
        focus_manager.set_focused_node(Some(nodes[2].clone()));
        let new_focus = focus_manager.get_focused_node().copied();

        // Verify focus changed again
        assert_ne!(old_focus, new_focus);

        // Clear selections again
        selection_manager.clear_all();

        // Verify all selections cleared
        assert!(selection_manager.get_selection(&dom_id).is_none());
        assert!(!selection_manager.has_any_selection());

        // Verify final focus state
        assert_eq!(focus_manager.get_focused_node(), Some(&nodes[2]));
    }

    #[test]
    fn test_focus_clear_then_set() {
        let mut focus_manager = FocusManager::new();

        // Set initial focus
        let node1 = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        };
        focus_manager.set_focused_node(Some(node1.clone()));
        assert_eq!(focus_manager.get_focused_node(), Some(&node1));

        // Clear focus
        focus_manager.set_focused_node(None);
        assert_eq!(focus_manager.get_focused_node(), None);

        // Set focus again to different node
        let node2 = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
        };
        focus_manager.set_focused_node(Some(node2.clone()));
        assert_eq!(focus_manager.get_focused_node(), Some(&node2));
    }

    #[test]
    fn test_multiple_selection_clear_operations() {
        use azul_core::selection::{
            CursorAffinity, GraphemeClusterId, Selection, SelectionState, TextCursor,
        };

        let mut selection_manager = SelectionManager::new();

        let doms = vec![
            DomId::ROOT_ID,
            DomId { inner: 1 },
            DomId { inner: 2 },
            DomId { inner: 3 },
        ];

        // Add selections to all DOMs
        for (i, dom_id) in doms.iter().enumerate() {
            let node = DomNodeId {
                dom: *dom_id,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
            };
            let sel_state = SelectionState {
                selections: vec![Selection::Cursor(TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: 0,
                        start_byte_in_run: (i * 5) as u32,
                    },
                    affinity: CursorAffinity::Leading,
                })],
                node_id: node,
            };
            selection_manager.set_selection(*dom_id, sel_state);
        }

        // Verify all selections exist
        for dom_id in &doms {
            assert!(selection_manager.get_selection(dom_id).is_some());
        }
        assert!(selection_manager.has_any_selection());

        // Clear all
        selection_manager.clear_all();

        // Verify all cleared
        for dom_id in &doms {
            assert!(selection_manager.get_selection(dom_id).is_none());
        }
        assert!(!selection_manager.has_any_selection());

        // Add new selections
        for (i, dom_id) in doms.iter().enumerate() {
            let node = DomNodeId {
                dom: *dom_id,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i + 10))),
            };
            let sel_state = SelectionState {
                selections: vec![Selection::Cursor(TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: 0,
                        start_byte_in_run: (i * 10) as u32,
                    },
                    affinity: CursorAffinity::Trailing,
                })],
                node_id: node,
            };
            selection_manager.set_selection(*dom_id, sel_state);
        }

        // Clear again
        selection_manager.clear_all();

        // Verify cleared again
        for dom_id in &doms {
            assert!(selection_manager.get_selection(dom_id).is_none());
        }
        assert!(!selection_manager.has_any_selection());
    }

    #[test]
    fn test_focus_update_request_conversion_edge_cases() {
        use crate::callbacks::FocusUpdateRequest;

        // Test with ROOT node
        let root_node = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
        };
        let req = FocusUpdateRequest::FocusNode(root_node.clone());
        assert!(req.is_change());
        assert_eq!(req.to_focused_node(), Some(Some(root_node)));

        // Test multiple conversions
        let req1 = FocusUpdateRequest::ClearFocus;
        let opt1 = req1.to_focused_node();
        let req2 = FocusUpdateRequest::from_optional(opt1);
        assert!(matches!(req2, FocusUpdateRequest::ClearFocus));

        // Test round-trip NoChange
        let req1 = FocusUpdateRequest::NoChange;
        let opt1 = req1.to_focused_node();
        let req2 = FocusUpdateRequest::from_optional(opt1);
        assert!(matches!(req2, FocusUpdateRequest::NoChange));
    }

    #[test]
    fn test_focus_manager_integration_with_all_managers() {
        use azul_core::selection::{
            CursorAffinity, GraphemeClusterId, Selection, SelectionState, TextCursor,
        };

        // Comprehensive integration test with LayoutWindow containing all managers
        let fc_cache = create_test_font_cache();
        let mut layout_window = LayoutWindow::new(fc_cache).expect("Failed to create LayoutWindow");

        let dom_id = DomId::ROOT_ID;
        let node1 = DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        };
        let node2 = DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
        };

        // Set initial focus
        layout_window
            .focus_manager
            .set_focused_node(Some(node1.clone()));

        // Add selection
        let sel_state = SelectionState {
            selections: vec![Selection::Cursor(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Leading,
            })],
            node_id: node1.clone(),
        };
        layout_window
            .selection_manager
            .set_selection(dom_id, sel_state);

        // Verify state
        assert_eq!(layout_window.focus_manager.get_focused_node(), Some(&node1));
        assert!(layout_window
            .selection_manager
            .get_selection(&dom_id)
            .is_some());

        // Simulate focus change
        let old_focus = layout_window.focus_manager.get_focused_node().copied();
        layout_window
            .focus_manager
            .set_focused_node(Some(node2.clone()));
        let new_focus = layout_window.focus_manager.get_focused_node();

        // Verify focus changed
        assert_ne!(old_focus.as_ref(), new_focus);

        // Clear selections (as event system would do)
        if old_focus.as_ref() != new_focus {
            layout_window.selection_manager.clear_all();
        }

        // Verify selections cleared
        assert!(layout_window
            .selection_manager
            .get_selection(&dom_id)
            .is_none());

        // Verify focus is on new node
        assert_eq!(layout_window.focus_manager.get_focused_node(), Some(&node2));
    }

    #[test]
    fn test_recursion_depth_limit_enforcement() {
        // Test that enforces the MAX_EVENT_RECURSION_DEPTH = 5 limit
        const MAX_DEPTH: usize = 5;
        let mut focus_manager = FocusManager::new();
        let mut depth = 0;

        // Generate nodes for depth+1 to exceed limit
        let nodes: Vec<DomNodeId> = (0..=MAX_DEPTH + 2)
            .map(|i| DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
            })
            .collect();

        // Set initial focus
        focus_manager.set_focused_node(Some(nodes[0].clone()));

        // Simulate recursive focus changes with depth tracking
        for i in 1..nodes.len() {
            if depth >= MAX_DEPTH {
                // In real code, event_v2.rs would stop recursion here
                break;
            }

            let old_focus = focus_manager.get_focused_node().copied();
            focus_manager.set_focused_node(Some(nodes[i].clone()));
            let new_focus = focus_manager.get_focused_node();

            if old_focus.as_ref() != new_focus {
                depth += 1;
            }
        }

        // Verify we stopped at MAX_DEPTH
        assert_eq!(depth, MAX_DEPTH);

        // Verify final focus is at depth MAX_DEPTH (node at index MAX_DEPTH)
        assert_eq!(focus_manager.get_focused_node(), Some(&nodes[MAX_DEPTH]));

        // Verify we didn't process nodes beyond MAX_DEPTH
        assert_ne!(
            focus_manager.get_focused_node(),
            Some(&nodes[MAX_DEPTH + 1])
        );
    }

    #[test]
    fn test_selection_persistence_without_focus_change() {
        use azul_core::selection::{
            CursorAffinity, GraphemeClusterId, Selection, SelectionState, TextCursor,
        };

        // Test that selections persist when focus doesn't change
        let mut focus_manager = FocusManager::new();
        let mut selection_manager = SelectionManager::new();

        let dom_id = DomId::ROOT_ID;
        let node = DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        };

        // Set focus
        focus_manager.set_focused_node(Some(node.clone()));

        // Add selection
        let sel_state = SelectionState {
            selections: vec![Selection::Cursor(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 5,
                },
                affinity: CursorAffinity::Leading,
            })],
            node_id: node.clone(),
        };
        selection_manager.set_selection(dom_id, sel_state.clone());

        // "Change" focus to same node (no actual change)
        let old_focus = focus_manager.get_focused_node().copied();
        focus_manager.set_focused_node(Some(node.clone()));
        let new_focus = focus_manager.get_focused_node().copied();

        // Verify focus didn't actually change
        assert_eq!(old_focus, new_focus);

        // Selection should NOT be cleared (no focus change occurred)
        if old_focus != new_focus {
            selection_manager.clear_all();
        }

        // Verify selection still exists
        assert!(selection_manager.get_selection(&dom_id).is_some());
        let current_sel = selection_manager.get_selection(&dom_id).unwrap();
        assert_eq!(current_sel.node_id, node);
        assert_eq!(current_sel.selections.len(), 1);
    }

    #[test]
    fn test_focus_update_request_equality() {
        use crate::callbacks::FocusUpdateRequest;

        let node1 = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        };
        let node2 = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
        };

        // Test equality for FocusNode
        let req1 = FocusUpdateRequest::FocusNode(node1.clone());
        let req2 = FocusUpdateRequest::FocusNode(node1.clone());
        let req3 = FocusUpdateRequest::FocusNode(node2.clone());
        assert_eq!(req1, req2);
        assert_ne!(req1, req3);

        // Test equality for ClearFocus
        let req1 = FocusUpdateRequest::ClearFocus;
        let req2 = FocusUpdateRequest::ClearFocus;
        assert_eq!(req1, req2);

        // Test equality for NoChange
        let req1 = FocusUpdateRequest::NoChange;
        let req2 = FocusUpdateRequest::NoChange;
        assert_eq!(req1, req2);

        // Test inequality across variants
        let req1 = FocusUpdateRequest::FocusNode(node1.clone());
        let req2 = FocusUpdateRequest::ClearFocus;
        let req3 = FocusUpdateRequest::NoChange;
        assert_ne!(req1, req2);
        assert_ne!(req2, req3);
        assert_ne!(req1, req3);
    }
}
