//! **Node** drag-and-drop *view types* (`DragState` / `DragType`).
//!
//! There is no drag-drop *manager* any more. The single source of truth for an
//! active drag is [`crate::managers::gesture::GestureAndDragManager::active_drag`]
//! (an `azul_core::drag::DragContext`).
//!
//! The former `DragDropManager` held a SECOND `active_drag: Option<DragContext>`
//! — a clone frozen at `InitDragVisualState` that never saw the later
//! drop-target/position updates, and that nothing remapped on a DOM rebuild.
//! Two sources of truth for one drag is a bug by construction, and the mirror
//! was write-only in practice (every reader consulted `gesture_drag_manager`
//! first, and the mirror was only ever populated *from* it), so it has been
//! deleted (2026-07-13). What remains here is the stateless conversion into the
//! public `DragState` API, which is built on demand from the live `DragContext`.

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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Create `DragState` from a `DragContext` (for backwards compatibility)
    #[must_use] pub fn from_context(ctx: &DragContext) -> Option<Self> {
        match &ctx.drag_type {
            ActiveDragType::Node(node_drag) => Some(Self {
                drag_type: DragType::Node,
                source_node: OptionDomNodeId::Some(DomNodeId {
                    dom: node_drag.dom_id,
                    node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node_drag.node_id)),
                }),
                current_drop_target: node_drag.current_drop_target,
                file_path: OptionString::None,
            }),
            ActiveDragType::FileDrop(file_drop) => Some(Self {
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
    [Debug, Clone, PartialEq, Eq]
);
