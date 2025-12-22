//! **Node** drag and drop state management

use azul_core::dom::{DomId, DomNodeId, NodeId};
use azul_css::{impl_option, impl_option_inner, AzString};

/// Type of drag operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragType {
    /// Dragging a DOM node
    Node,
    /// Dragging a file from OS
    File,
}

/// State of an active drag operation
#[derive(Debug, Clone, PartialEq)]
pub struct DragState {
    /// Type of drag
    pub drag_type: DragType,
    /// Source node (for node dragging)
    pub source_node: Option<DomNodeId>,
    /// Current drop target (if hovering over valid drop zone)
    pub current_drop_target: Option<DomNodeId>,
    /// File path (for file dragging)
    pub file_path: Option<AzString>,
}

impl_option!(
    DragState,
    OptionDragState,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// Manager for drag-and-drop operations
#[derive(Debug, Clone, PartialEq)]
pub struct DragDropManager {
    /// Currently active drag operation
    pub active_drag: Option<DragState>,
}

impl Default for DragDropManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DragDropManager {
    /// Create a new drag-drop manager
    pub fn new() -> Self {
        Self { active_drag: None }
    }

    /// Start a node drag operation
    pub fn start_node_drag(&mut self, source_node: DomNodeId) {
        self.active_drag = Some(DragState {
            drag_type: DragType::Node,
            source_node: Some(source_node),
            current_drop_target: None,
            file_path: None,
        });
    }

    /// Start a file drag operation
    pub fn start_file_drag(&mut self, file_path: AzString) {
        self.active_drag = Some(DragState {
            drag_type: DragType::File,
            source_node: None,
            current_drop_target: None,
            file_path: Some(file_path),
        });
    }

    /// Update the current drop target
    pub fn set_drop_target(&mut self, target: Option<DomNodeId>) {
        if let Some(drag) = &mut self.active_drag {
            drag.current_drop_target = target;
        }
    }

    /// End the drag operation and return the final state
    pub fn end_drag(&mut self) -> Option<DragState> {
        self.active_drag.take()
    }

    /// Check if a drag operation is active
    pub fn is_dragging(&self) -> bool {
        self.active_drag.is_some()
    }

    /// Check if currently dragging a node
    pub fn is_dragging_node(&self) -> bool {
        matches!(
            self.active_drag,
            Some(DragState {
                drag_type: DragType::Node,
                ..
            })
        )
    }

    /// Check if currently dragging a file
    pub fn is_dragging_file(&self) -> bool {
        matches!(
            self.active_drag,
            Some(DragState {
                drag_type: DragType::File,
                ..
            })
        )
    }

    /// Get the active drag state
    pub fn get_drag_state(&self) -> Option<&DragState> {
        self.active_drag.as_ref()
    }

    /// Cancel the current drag operation
    pub fn cancel_drag(&mut self) {
        self.active_drag = None;
    }
}
