//! **Node** drag and drop state management (legacy compatibility shim)
//!
//! This module maintains the old API types for backwards compatibility.
//! Internally, it now uses the unified `DragContext` from `azul_core::drag`.
//!
//! The primary drag-and-drop system is `GestureAndDragManager` in
//! `managers/gesture.rs`. This `DragDropManager` is a read-only mirror
//! whose `active_drag` field is populated by event-processing code in
//! `event.rs`.

use azul_core::dom::{DomNodeId, OptionDomNodeId};
use azul_core::drag::{ActiveDragType, DragContext};
use azul_css::{impl_option, impl_option_inner, OptionString};

/// Type of drag operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum DragType {
    /// Dragging a DOM node
    Node,
    /// Dragging a file from OS
    File,
}

/// State of an active drag operation
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct DragState {
    /// Type of drag
    pub drag_type: DragType,
    /// Source node (for node dragging)
    pub source_node: OptionDomNodeId,
    /// Current drop target (if hovering over valid drop zone)
    pub current_drop_target: OptionDomNodeId,
    /// File path (for file dragging)
    pub file_path: OptionString,
}

impl DragState {
    /// Create DragState from a DragContext (for backwards compatibility)
    pub fn from_context(ctx: &DragContext) -> Option<Self> {
        match &ctx.drag_type {
            ActiveDragType::Node(node_drag) => Some(DragState {
                drag_type: DragType::Node,
                source_node: OptionDomNodeId::Some(DomNodeId {
                    dom: node_drag.dom_id,
                    node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node_drag.node_id)),
                }),
                current_drop_target: node_drag.current_drop_target,
                file_path: OptionString::None,
            }),
            ActiveDragType::FileDrop(file_drop) => Some(DragState {
                drag_type: DragType::File,
                source_node: OptionDomNodeId::None,
                current_drop_target: file_drop.drop_target,
                file_path: file_drop.files.as_ref().first().cloned().into(),
            }),
            _ => None, // Other drag types don't map to the old API
        }
    }
}

impl_option!(
    DragState,
    OptionDragState,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// Manager for drag-and-drop operations
///
/// **DEPRECATED**: Use `GestureAndDragManager` with `DragContext` instead.
/// This type is kept for backwards compatibility only.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DragDropManager {
    /// Currently active drag operation (using new unified system)
    pub active_drag: Option<DragContext>,
}

impl DragDropManager {
    /// Create a new drag-drop manager
    pub fn new() -> Self {
        Self { active_drag: None }
    }

    /// End the drag operation and return the final context
    pub fn end_drag(&mut self) -> Option<DragContext> {
        self.active_drag.take()
    }

    /// Check if a drag operation is active
    pub fn is_dragging(&self) -> bool {
        self.active_drag.is_some()
    }

    /// Check if currently dragging a node
    pub fn is_dragging_node(&self) -> bool {
        self.active_drag.as_ref().is_some_and(|d| d.is_node_drag())
    }

    /// Check if currently dragging a file
    pub fn is_dragging_file(&self) -> bool {
        self.active_drag.as_ref().is_some_and(|d| d.is_file_drop())
    }

    /// Get the active drag context
    pub fn get_drag_context(&self) -> Option<&DragContext> {
        self.active_drag.as_ref()
    }

    /// Get the active drag state (old API for backwards compatibility)
    pub fn get_drag_state(&self) -> Option<DragState> {
        self.active_drag.as_ref().and_then(DragState::from_context)
    }

}
