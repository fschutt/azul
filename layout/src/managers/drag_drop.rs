//! **Node** drag and drop state management
//!
//! This module maintains the old API types for backwards compatibility.
//! Internally, it now uses the unified `DragContext` from `azul_core::drag`.

use azul_core::dom::{DomId, DomNodeId, NodeId, OptionDomNodeId};
use azul_core::drag::{ActiveDragType, DragContext};
use azul_css::{impl_option, impl_option_inner, AzString, OptionString};

// Re-export DragData for use in other modules
pub use azul_core::drag::DragData;

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

    /// Start a node drag operation
    pub fn start_node_drag(&mut self, source_node: DomNodeId) {
        self.active_drag = Some(DragContext::node_drag(
            source_node.dom,
            source_node.node.into_crate_internal().unwrap_or(NodeId::ZERO),
            azul_core::geom::LogicalPosition::zero(),
            DragData::default(),
            0,
        ));
    }

    /// Start a file drag operation
    pub fn start_file_drag(&mut self, file_path: AzString) {
        self.active_drag = Some(DragContext::file_drop(
            vec![file_path],
            azul_core::geom::LogicalPosition::zero(),
            0,
        ));
    }

    /// Update the current drop target
    pub fn set_drop_target(&mut self, target: Option<DomNodeId>) {
        if let Some(ref mut drag) = self.active_drag {
            if let Some(node_drag) = drag.as_node_drag_mut() {
                node_drag.current_drop_target = target.into();
            }
        }
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

    /// Cancel the current drag operation
    pub fn cancel_drag(&mut self) {
        self.active_drag = None;
    }
}
