//! Unified drag context for all drag operations.
//!
//! This module provides a single, coherent way to handle all drag operations:
//! - Text selection drag
//! - Scrollbar thumb drag
//! - Node drag-and-drop
//! - Window drag/resize
//! - File drop from OS
//!
//! The `DragContext` struct tracks the current drag state and provides
//! a unified interface for event processing.

use alloc::vec::Vec;

use crate::dom::{DomId, DomNodeId, NodeId, OptionDomNodeId};
use crate::geom::LogicalPosition;
use crate::selection::TextCursor;
use crate::window::WindowPosition;

use azul_css::{AzString, StringVec, U8Vec};

/// Type of the active drag operation.
///
/// This enum unifies all drag types into a single discriminated union,
/// making it easy to handle different drag behaviors in one place.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ActiveDragType {
    /// Text selection drag - user is selecting text by dragging
    TextSelection(TextSelectionDrag),
    /// Scrollbar thumb drag - user is dragging a scrollbar thumb
    ScrollbarThumb(ScrollbarThumbDrag),
    /// Node drag-and-drop - user is dragging a DOM node
    Node(NodeDrag),
    /// Window drag - user is moving the window (titlebar drag)
    WindowMove(WindowMoveDrag),
    /// Window resize - user is resizing the window (edge/corner drag)
    WindowResize(WindowResizeDrag),
    /// File drop from OS - user is dragging file(s) from the OS
    FileDrop(FileDropDrag),
}

/// Text selection drag state.
///
/// Tracks the anchor point (where selection started) and current position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct TextSelectionDrag {
    /// DOM ID where the selection started
    pub dom_id: DomId,
    /// The IFC root node where selection started (e.g., <p> element)
    pub anchor_ifc_node: NodeId,
    /// The anchor cursor position (fixed during drag)
    pub anchor_cursor: Option<TextCursor>,
    /// Mouse position where drag started
    pub start_mouse_position: LogicalPosition,
    /// Current mouse position
    pub current_mouse_position: LogicalPosition,
    /// Whether we should auto-scroll (mouse near edge of scroll container)
    pub auto_scroll_direction: AutoScrollDirection,
    /// The scroll container that should be auto-scrolled (if any)
    pub auto_scroll_container: Option<NodeId>,
}

/// Scrollbar thumb drag state.
///
/// Tracks which scrollbar is being dragged and the current offset.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct ScrollbarThumbDrag {
    /// DOM ID that `scroll_container_node` belongs to. Used to scope
    /// `remap_node_ids` so a reconciliation of a *different* DOM can't remap
    /// this drag's node id against an unrelated DOM's old→new map.
    pub dom_id: DomId,
    /// The scroll container node being scrolled
    pub scroll_container_node: NodeId,
    /// Whether this is the vertical or horizontal scrollbar
    pub axis: ScrollbarAxis,
    /// Mouse Y position where drag started (for calculating delta)
    pub start_mouse_position: LogicalPosition,
    /// Scroll offset when drag started
    pub start_scroll_offset: f32,
    /// Current mouse position
    pub current_mouse_position: LogicalPosition,
    /// Track length in pixels (for calculating scroll ratio)
    pub track_length_px: f32,
    /// Content length in pixels (for calculating scroll ratio)
    pub content_length_px: f32,
    /// Viewport length in pixels (for calculating scroll ratio)
    pub viewport_length_px: f32,
}

/// Which scrollbar axis is being dragged
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum ScrollbarAxis {
    Vertical,
    Horizontal,
}

/// Node drag-and-drop state.
///
/// Tracks a DOM node being dragged for reordering or moving.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct NodeDrag {
    /// DOM ID of the node being dragged
    pub dom_id: DomId,
    /// Node ID being dragged
    pub node_id: NodeId,
    /// Position where drag started
    pub start_position: LogicalPosition,
    /// Current drag position
    pub current_position: LogicalPosition,
    /// Offset from node origin to click point (for correct visual positioning)
    pub drag_offset: LogicalPosition,
    /// Optional: DOM node currently under cursor (drop target)
    pub current_drop_target: OptionDomNodeId,
    /// Previous drop target (for generating DragEnter/DragLeave events)
    pub previous_drop_target: OptionDomNodeId,
    /// Drag data (MIME types and content)
    pub drag_data: DragData,
    /// Whether the current drop target has accepted the drop via `accept_drop()`
    pub drop_accepted: bool,
    /// Drop effect set by the drop target
    pub drop_effect: DropEffect,
}

/// Window move drag state.
///
/// Tracks the window being moved via titlebar drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct WindowMoveDrag {
    /// Position where window drag started (in screen coordinates)
    pub start_position: LogicalPosition,
    /// Current drag position
    pub current_position: LogicalPosition,
    /// Initial window position before drag
    pub initial_window_position: WindowPosition,
}

/// Window resize drag state.
///
/// Tracks the window being resized via edge/corner drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct WindowResizeDrag {
    /// Which edge/corner is being dragged
    pub edge: WindowResizeEdge,
    /// Position where resize started
    pub start_position: LogicalPosition,
    /// Current drag position
    pub current_position: LogicalPosition,
    /// Initial window size before resize
    pub initial_width: u32,
    /// Initial window height before resize
    pub initial_height: u32,
}

/// Which edge or corner of the window is being resized
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum WindowResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// File drop from OS drag state.
///
/// Tracks files being dragged from the operating system.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct FileDropDrag {
    /// Files being dragged (as string paths)
    pub files: StringVec,
    /// Current position of drag cursor
    pub position: LogicalPosition,
    /// DOM node under cursor (potential drop target)
    pub drop_target: OptionDomNodeId,
    /// Allowed drop effect
    pub drop_effect: DropEffect,
}

/// Direction for auto-scrolling during drag operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub enum AutoScrollDirection {
    #[default]
    None,
    Up,
    Down,
    Left,
    Right,
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
}

/// Drop effect — the operation that will happen if the data is dropped
/// on the current target (HTML5 `DataTransfer.dropEffect`).
///
/// This is a strict subset of `DragEffect`: a drop target selects one of
/// these four outcomes, which must also be allowed by the source's
/// `effect_allowed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub enum DropEffect {
    /// No drop allowed / the drop is rejected. Default.
    #[default]
    None,
    /// Drop will copy the data (source retains its copy).
    Copy,
    /// Drop will create a link/shortcut to the data.
    Link,
    /// Drop will move the data (source should remove its copy).
    Move,
}

/// Allowed drag effects — the set of operations the drag source permits
/// (HTML5 `DataTransfer.effectAllowed`).
///
/// The drop target's `DropEffect` must be a member of this set for the
/// drop to succeed. Semantic superset of `DropEffect` that adds the
/// HTML5 combined-permission values (`CopyLink`, `CopyMove`, `LinkMove`,
/// `All`) and the pre-drag `Uninitialized` sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub enum DragEffect {
    /// Allowed set has not been initialized yet (equivalent to `All` in
    /// most user agents). Default for fresh drags.
    #[default]
    Uninitialized,
    /// No drop is permitted.
    None,
    /// Only Copy is permitted.
    Copy,
    /// Copy or Link is permitted.
    CopyLink,
    /// Copy or Move is permitted.
    CopyMove,
    /// Only Link is permitted.
    Link,
    /// Link or Move is permitted.
    LinkMove,
    /// Only Move is permitted.
    Move,
    /// Any of Copy, Link, or Move is permitted.
    All,
}

/// FFI-safe (`mime_type`, `data`) pair used by [`DragData`] in place of
/// a `BTreeMap<AzString, Vec<u8>>` entry.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct MimeTypeData {
    pub mime_type: AzString,
    pub data: U8Vec,
}

impl_option!(
    MimeTypeData,
    OptionMimeTypeData,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]
);

impl_vec!(
    MimeTypeData,
    MimeTypeDataVec,
    MimeTypeDataVecDestructor,
    MimeTypeDataVecDestructorType,
    MimeTypeDataVecSlice,
    OptionMimeTypeData
);
impl_vec_mut!(MimeTypeData, MimeTypeDataVec);
impl_vec_debug!(MimeTypeData, MimeTypeDataVec);
impl_vec_partialord!(MimeTypeData, MimeTypeDataVec);
impl_vec_ord!(MimeTypeData, MimeTypeDataVec);
impl_vec_clone!(MimeTypeData, MimeTypeDataVec, MimeTypeDataVecDestructor);
impl_vec_partialeq!(MimeTypeData, MimeTypeDataVec);
impl_vec_eq!(MimeTypeData, MimeTypeDataVec);
impl_vec_hash!(MimeTypeData, MimeTypeDataVec);

/// Drag data (HTML5 `DataTransfer`).
///
/// Holds the payload(s) being transferred during a drag operation, keyed
/// by MIME type, plus the set of operations the source allows.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DragData {
    /// MIME type -> data mapping (vec-of-pairs for FFI compatibility).
    ///
    /// e.g., `"text/plain" -> "Hello World"`.
    pub data: MimeTypeDataVec,
    /// Set of drag operations the source permits for this drag.
    pub effect_allowed: DragEffect,
}

impl DragData {
    /// Create new empty drag data
    #[must_use] pub const fn new() -> Self {
        Self {
            data: MimeTypeDataVec::new(),
            effect_allowed: DragEffect::Uninitialized,
        }
    }

    /// Set data for a MIME type. Replaces any existing entry for the
    /// same MIME type.
    pub fn set_data(&mut self, mime_type: impl Into<AzString>, data: Vec<u8>) {
        let mime_type = mime_type.into();
        let value: U8Vec = data.into();
        if let Some(entry) = self
            .data
            .as_mut()
            .iter_mut()
            .find(|e| e.mime_type == mime_type)
        {
            entry.data = value;
        } else {
            self.data.push(MimeTypeData {
                mime_type,
                data: value,
            });
        }
    }

    /// Get data for a MIME type
    #[must_use] pub fn get_data(&self, mime_type: &str) -> Option<&[u8]> {
        self.data
            .as_ref()
            .iter()
            .find(|e| e.mime_type.as_str() == mime_type)
            .map(|e| e.data.as_ref())
    }

    /// Set plain text data
    pub fn set_text(&mut self, text: impl Into<AzString>) {
        let text_str = text.into();
        self.set_data("text/plain", text_str.as_str().as_bytes().to_vec());
    }

    /// Get plain text data
    #[must_use] pub fn get_text(&self) -> Option<AzString> {
        self.get_data("text/plain")
            .map(|bytes| AzString::from(core::str::from_utf8(bytes).unwrap_or("")))
    }
}

/// The unified drag context.
///
/// This struct wraps `ActiveDragType` and provides common metadata
/// that applies to all drag operations.
///
/// Note: this type is Rust-only and not exposed through the C API.
#[derive(Debug, Clone, PartialEq)]
pub struct DragContext {
    /// The specific type of drag operation
    pub drag_type: ActiveDragType,
    /// Session ID from gesture detection (links back to `GestureManager`)
    pub session_id: u64,
    /// Whether the drag has been cancelled (e.g., Escape pressed)
    pub cancelled: bool,
}

impl DragContext {
    /// Create a new drag context
    #[must_use] pub const fn new(drag_type: ActiveDragType, session_id: u64) -> Self {
        Self {
            drag_type,
            session_id,
            cancelled: false,
        }
    }

    /// Create a text selection drag
    #[must_use] pub const fn text_selection(
        dom_id: DomId,
        anchor_ifc_node: NodeId,
        start_mouse_position: LogicalPosition,
        session_id: u64,
    ) -> Self {
        Self::new(
            ActiveDragType::TextSelection(TextSelectionDrag {
                dom_id,
                anchor_ifc_node,
                anchor_cursor: None,
                start_mouse_position,
                current_mouse_position: start_mouse_position,
                auto_scroll_direction: AutoScrollDirection::None,
                auto_scroll_container: None,
            }),
            session_id,
        )
    }

    /// Create a scrollbar thumb drag
    #[must_use] pub const fn scrollbar_thumb(
        dom_id: DomId,
        scroll_container_node: NodeId,
        axis: ScrollbarAxis,
        start_mouse_position: LogicalPosition,
        start_scroll_offset: f32,
        track_length_px: f32,
        content_length_px: f32,
        viewport_length_px: f32,
        session_id: u64,
    ) -> Self {
        Self::new(
            ActiveDragType::ScrollbarThumb(ScrollbarThumbDrag {
                dom_id,
                scroll_container_node,
                axis,
                start_mouse_position,
                start_scroll_offset,
                current_mouse_position: start_mouse_position,
                track_length_px,
                content_length_px,
                viewport_length_px,
            }),
            session_id,
        )
    }

    /// Create a node drag
    #[must_use] pub const fn node_drag(
        dom_id: DomId,
        node_id: NodeId,
        start_position: LogicalPosition,
        drag_data: DragData,
        session_id: u64,
    ) -> Self {
        Self::new(
            ActiveDragType::Node(NodeDrag {
                dom_id,
                node_id,
                start_position,
                current_position: start_position,
                drag_offset: LogicalPosition::zero(),
                current_drop_target: OptionDomNodeId::None,
                previous_drop_target: OptionDomNodeId::None,
                drag_data,
                drop_accepted: false,
                drop_effect: DropEffect::None,
            }),
            session_id,
        )
    }

    /// Create a window move drag
    #[must_use] pub const fn window_move(
        start_position: LogicalPosition,
        initial_window_position: WindowPosition,
        session_id: u64,
    ) -> Self {
        Self::new(
            ActiveDragType::WindowMove(WindowMoveDrag {
                start_position,
                current_position: start_position,
                initial_window_position,
            }),
            session_id,
        )
    }

    /// Create a file drop drag
    #[must_use] pub fn file_drop(files: Vec<AzString>, position: LogicalPosition, session_id: u64) -> Self {
        Self::new(
            ActiveDragType::FileDrop(FileDropDrag {
                files: files.into(),
                position,
                drop_target: OptionDomNodeId::None,
                drop_effect: DropEffect::Copy,
            }),
            session_id,
        )
    }

    /// Update the current mouse position for all drag types
    pub const fn update_position(&mut self, position: LogicalPosition) {
        match &mut self.drag_type {
            ActiveDragType::TextSelection(ref mut drag) => {
                drag.current_mouse_position = position;
            }
            ActiveDragType::ScrollbarThumb(ref mut drag) => {
                drag.current_mouse_position = position;
            }
            ActiveDragType::Node(ref mut drag) => {
                drag.current_position = position;
            }
            ActiveDragType::WindowMove(ref mut drag) => {
                drag.current_position = position;
            }
            ActiveDragType::WindowResize(ref mut drag) => {
                drag.current_position = position;
            }
            ActiveDragType::FileDrop(ref mut drag) => {
                drag.position = position;
            }
        }
    }

    /// Get the current mouse position
    #[must_use] pub const fn current_position(&self) -> LogicalPosition {
        match &self.drag_type {
            ActiveDragType::TextSelection(drag) => drag.current_mouse_position,
            ActiveDragType::ScrollbarThumb(drag) => drag.current_mouse_position,
            ActiveDragType::Node(drag) => drag.current_position,
            ActiveDragType::WindowMove(drag) => drag.current_position,
            ActiveDragType::WindowResize(drag) => drag.current_position,
            ActiveDragType::FileDrop(drag) => drag.position,
        }
    }

    /// Get the start position
    #[must_use] pub const fn start_position(&self) -> LogicalPosition {
        match &self.drag_type {
            ActiveDragType::TextSelection(drag) => drag.start_mouse_position,
            ActiveDragType::ScrollbarThumb(drag) => drag.start_mouse_position,
            ActiveDragType::Node(drag) => drag.start_position,
            ActiveDragType::WindowMove(drag) => drag.start_position,
            ActiveDragType::WindowResize(drag) => drag.start_position,
            ActiveDragType::FileDrop(drag) => drag.position, // No start for file drops
        }
    }

    /// Check if this is a text selection drag
    #[must_use] pub const fn is_text_selection(&self) -> bool {
        matches!(self.drag_type, ActiveDragType::TextSelection(_))
    }

    /// Check if this is a scrollbar thumb drag
    #[must_use] pub const fn is_scrollbar_thumb(&self) -> bool {
        matches!(self.drag_type, ActiveDragType::ScrollbarThumb(_))
    }

    /// Check if this is a node drag
    #[must_use] pub const fn is_node_drag(&self) -> bool {
        matches!(self.drag_type, ActiveDragType::Node(_))
    }

    /// Check if this is a window move drag
    #[must_use] pub const fn is_window_move(&self) -> bool {
        matches!(self.drag_type, ActiveDragType::WindowMove(_))
    }

    /// Check if this is a file drop
    #[must_use] pub const fn is_file_drop(&self) -> bool {
        matches!(self.drag_type, ActiveDragType::FileDrop(_))
    }

    /// Get as text selection drag (if applicable)
    #[must_use] pub const fn as_text_selection(&self) -> Option<&TextSelectionDrag> {
        match &self.drag_type {
            ActiveDragType::TextSelection(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as mutable text selection drag (if applicable)
    pub const fn as_text_selection_mut(&mut self) -> Option<&mut TextSelectionDrag> {
        match &mut self.drag_type {
            ActiveDragType::TextSelection(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as scrollbar thumb drag (if applicable)
    #[must_use] pub const fn as_scrollbar_thumb(&self) -> Option<&ScrollbarThumbDrag> {
        match &self.drag_type {
            ActiveDragType::ScrollbarThumb(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as mutable scrollbar thumb drag (if applicable)
    pub const fn as_scrollbar_thumb_mut(&mut self) -> Option<&mut ScrollbarThumbDrag> {
        match &mut self.drag_type {
            ActiveDragType::ScrollbarThumb(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as node drag (if applicable)
    #[must_use] pub const fn as_node_drag(&self) -> Option<&NodeDrag> {
        match &self.drag_type {
            ActiveDragType::Node(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as mutable node drag (if applicable)
    pub const fn as_node_drag_mut(&mut self) -> Option<&mut NodeDrag> {
        match &mut self.drag_type {
            ActiveDragType::Node(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as window move drag (if applicable)
    #[must_use] pub const fn as_window_move(&self) -> Option<&WindowMoveDrag> {
        match &self.drag_type {
            ActiveDragType::WindowMove(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as file drop (if applicable)
    #[must_use] pub const fn as_file_drop(&self) -> Option<&FileDropDrag> {
        match &self.drag_type {
            ActiveDragType::FileDrop(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as mutable file drop (if applicable)
    pub const fn as_file_drop_mut(&mut self) -> Option<&mut FileDropDrag> {
        match &mut self.drag_type {
            ActiveDragType::FileDrop(drag) => Some(drag),
            _ => None,
        }
    }

    /// Calculate scroll delta for scrollbar thumb drag
    ///
    /// Returns the new scroll offset based on current mouse position.
    #[must_use] pub fn calculate_scrollbar_scroll_offset(&self) -> Option<f32> {
        let drag = self.as_scrollbar_thumb()?;
        
        // Calculate mouse delta along the drag axis
        let mouse_delta = match drag.axis {
            ScrollbarAxis::Vertical => {
                drag.current_mouse_position.y - drag.start_mouse_position.y
            }
            ScrollbarAxis::Horizontal => {
                drag.current_mouse_position.x - drag.start_mouse_position.x
            }
        };

        // Calculate the scrollable range
        let scrollable_range = drag.content_length_px - drag.viewport_length_px;
        if scrollable_range <= 0.0 || drag.track_length_px <= 0.0 {
            return Some(drag.start_scroll_offset);
        }

        // Calculate thumb length (proportional to viewport/content ratio)
        let thumb_length = (drag.viewport_length_px / drag.content_length_px) * drag.track_length_px;
        let scrollable_track = drag.track_length_px - thumb_length;

        if scrollable_track <= 0.0 {
            return Some(drag.start_scroll_offset);
        }

        // Convert mouse delta to scroll delta
        let scroll_ratio = mouse_delta / scrollable_track;
        let scroll_delta = scroll_ratio * scrollable_range;

        // Calculate new scroll offset
        let new_offset = drag.start_scroll_offset + scroll_delta;

        // Clamp to valid range
        Some(new_offset.clamp(0.0, scrollable_range))
    }

    /// Remap a drop target's `NodeId` using the old→new mapping.
    /// Clears the target if the old `NodeId` was removed.
    fn remap_drop_target(
        target: &mut OptionDomNodeId,
        dom_id: DomId,
        node_id_map: &alloc::collections::BTreeMap<NodeId, NodeId>,
    ) {
        let dt = match target.into_option() {
            Some(dt) if dt.dom == dom_id => dt,
            _ => return,
        };
        let Some(old_nid) = dt.node.into_crate_internal() else {
            return;
        };
        if let Some(&new_nid) = node_id_map.get(&old_nid) {
            *target = Some(DomNodeId {
                dom: dom_id,
                node: crate::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(new_nid)),
            }).into();
        } else {
            *target = OptionDomNodeId::None;
        }
    }

    /// Remap `NodeIds` stored in this drag context after DOM reconciliation.
    ///
    /// When the DOM is regenerated during an active drag, `NodeIds` can change.
    /// This updates all stored `NodeIds` using the old→new mapping.
    /// Returns `false` if a critical `NodeId` was removed (drag should be cancelled).
    pub fn remap_node_ids(
        &mut self,
        dom_id: DomId,
        node_id_map: &alloc::collections::BTreeMap<NodeId, NodeId>,
    ) -> bool {
        match &mut self.drag_type {
            ActiveDragType::TextSelection(ref mut drag) => {
                if drag.dom_id != dom_id {
                    return true;
                }
                if let Some(&new_id) = node_id_map.get(&drag.anchor_ifc_node) {
                    drag.anchor_ifc_node = new_id;
                } else {
                    return false; // anchor node removed
                }
                if let Some(ref mut container) = drag.auto_scroll_container {
                    if let Some(&new_id) = node_id_map.get(container) {
                        *container = new_id;
                    } else {
                        drag.auto_scroll_container = None;
                    }
                }
                true
            }
            ActiveDragType::ScrollbarThumb(ref mut drag) => {
                // Scope the remap to the DOM this drag belongs to: a different
                // DOM's reconciliation must not touch our scroll container id.
                if drag.dom_id != dom_id {
                    return true;
                }
                if let Some(&new_id) = node_id_map.get(&drag.scroll_container_node) {
                    drag.scroll_container_node = new_id;
                    true
                } else {
                    false // scroll container removed
                }
            }
            ActiveDragType::Node(ref mut drag) => {
                if drag.dom_id != dom_id {
                    return true;
                }
                if let Some(&new_id) = node_id_map.get(&drag.node_id) {
                    drag.node_id = new_id;
                } else {
                    return false; // dragged node removed
                }
                // Drop target remap — both current AND previous, otherwise a
                // stale `previous_drop_target` keeps a pre-reconciliation NodeId
                // and later generates spurious DragEnter/DragLeave against a
                // node that no longer exists (or a different node reusing the id).
                Self::remap_drop_target(&mut drag.current_drop_target, dom_id, node_id_map);
                Self::remap_drop_target(&mut drag.previous_drop_target, dom_id, node_id_map);
                true
            }
            // WindowMove, WindowResize, and FileDrop don't reference DOM NodeIds
            ActiveDragType::WindowMove(_) | ActiveDragType::WindowResize(_) => true,
            ActiveDragType::FileDrop(ref mut drag) => {
                Self::remap_drop_target(&mut drag.drop_target, dom_id, node_id_map);
                true
            }
        }
    }
}

azul_css::impl_option!(
    DragContext,
    OptionDragContext,
    copy = false,
    [Debug, Clone, PartialEq]
);


/// Drag offset from the cursor position at drag start (logical pixels).
/// `dx`/`dy` are the delta from drag start to current position.
#[derive(Default, Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct DragDelta {
    pub dx: f32,
    pub dy: f32,
}

impl DragDelta {
    #[inline]
    #[must_use] pub const fn new(dx: f32, dy: f32) -> Self {
        Self { dx, dy }
    }
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
}

impl_option!(
    DragDelta,
    OptionDragDelta,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

#[cfg(test)]
mod audit_tests {
    use super::*;
    use crate::styled_dom::NodeHierarchyItemId;

    fn node_map(from: usize, to: usize) -> alloc::collections::BTreeMap<NodeId, NodeId> {
        let mut m = alloc::collections::BTreeMap::new();
        m.insert(NodeId::new(from), NodeId::new(to));
        m
    }

    fn dnid(dom: usize, node: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: dom },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
        }
    }

    #[test]
    fn scrollbar_remap_scoped_to_dom() {
        let mut ctx = DragContext::scrollbar_thumb(
            DomId { inner: 0 },
            NodeId::new(3),
            ScrollbarAxis::Vertical,
            LogicalPosition::zero(),
            0.0, 100.0, 300.0, 100.0,
            1,
        );
        // Reconciling a *different* DOM (id 1) must not touch our node id.
        let ok = ctx.remap_node_ids(DomId { inner: 1 }, &node_map(3, 99));
        assert!(ok);
        assert_eq!(ctx.as_scrollbar_thumb().unwrap().scroll_container_node, NodeId::new(3));

        // Reconciling our own DOM (id 0) remaps it.
        let ok2 = ctx.remap_node_ids(DomId { inner: 0 }, &node_map(3, 99));
        assert!(ok2);
        assert_eq!(ctx.as_scrollbar_thumb().unwrap().scroll_container_node, NodeId::new(99));
    }

    #[test]
    fn node_drag_remaps_previous_drop_target() {
        let mut ctx = DragContext::node_drag(
            DomId { inner: 0 },
            NodeId::new(1),
            LogicalPosition::zero(),
            DragData::new(),
            2,
        );
        {
            let nd = ctx.as_node_drag_mut().unwrap();
            nd.current_drop_target = Some(dnid(0, 5)).into();
            nd.previous_drop_target = Some(dnid(0, 6)).into();
        }
        // Map: dragged node 1->1, drop targets 5->50, 6->60.
        let mut m = alloc::collections::BTreeMap::new();
        m.insert(NodeId::new(1), NodeId::new(1));
        m.insert(NodeId::new(5), NodeId::new(50));
        m.insert(NodeId::new(6), NodeId::new(60));
        assert!(ctx.remap_node_ids(DomId { inner: 0 }, &m));

        let nd = ctx.as_node_drag().unwrap();
        let cur = nd.current_drop_target.into_option().unwrap().node.into_crate_internal().unwrap();
        let prev = nd.previous_drop_target.into_option().unwrap().node.into_crate_internal().unwrap();
        assert_eq!(cur, NodeId::new(50));
        assert_eq!(prev, NodeId::new(60)); // previously left stale (bug)
    }
}

#[cfg(test)]
mod autotest_generated {
    use alloc::collections::BTreeMap;
    use alloc::string::{String, ToString};

    use super::*;
    use crate::geom::PhysicalPosition;
    use crate::styled_dom::NodeHierarchyItemId;

    // ---------------------------------------------------------------- helpers

    fn dom(i: usize) -> DomId {
        DomId { inner: i }
    }

    fn dnid(dom_idx: usize, node: usize) -> DomNodeId {
        DomNodeId {
            dom: dom(dom_idx),
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
        }
    }

    /// A drop target that points at a DOM but carries *no* node id (the `None`
    /// encoding of `NodeHierarchyItemId`).
    fn dnid_no_node(dom_idx: usize) -> DomNodeId {
        DomNodeId {
            dom: dom(dom_idx),
            node: NodeHierarchyItemId::from_crate_internal(None),
        }
    }

    fn nid_map(pairs: &[(usize, usize)]) -> BTreeMap<NodeId, NodeId> {
        let mut m = BTreeMap::new();
        for (from, to) in pairs {
            m.insert(NodeId::new(*from), NodeId::new(*to));
        }
        m
    }

    /// Vertical scrollbar drag anchored at (0, 0), no mouse movement yet.
    fn vscroll(
        start_scroll_offset: f32,
        track_length_px: f32,
        content_length_px: f32,
        viewport_length_px: f32,
    ) -> DragContext {
        DragContext::scrollbar_thumb(
            dom(0),
            NodeId::new(1),
            ScrollbarAxis::Vertical,
            LogicalPosition::zero(),
            start_scroll_offset,
            track_length_px,
            content_length_px,
            viewport_length_px,
            7,
        )
    }

    fn resize_ctx() -> DragContext {
        DragContext::new(
            ActiveDragType::WindowResize(WindowResizeDrag {
                edge: WindowResizeEdge::BottomRight,
                start_position: LogicalPosition::new(1.0, 2.0),
                current_position: LogicalPosition::new(1.0, 2.0),
                initial_width: u32::MAX,
                initial_height: 0,
            }),
            u64::MAX,
        )
    }

    // ============================================================ DragData
    // parser-ish surface: get_data / set_data / set_text / get_text

    #[test]
    fn dragdata_new_is_empty_and_matches_default() {
        let d = DragData::new();
        assert_eq!(d.data.len(), 0);
        assert!(d.data.is_empty());
        assert_eq!(d.effect_allowed, DragEffect::Uninitialized);
        assert_eq!(d, DragData::default());
        assert!(d.get_data("text/plain").is_none());
        assert!(d.get_text().is_none());
    }

    #[test]
    fn get_data_valid_minimal_positive_control() {
        let mut d = DragData::new();
        d.set_data("text/plain", b"hi".to_vec());
        assert_eq!(d.get_data("text/plain"), Some(&b"hi"[..]));
    }

    #[test]
    fn get_data_empty_key_on_empty_and_populated_returns_none() {
        let empty = DragData::new();
        assert!(empty.get_data("").is_none());

        let mut d = DragData::new();
        d.set_data("text/plain", b"x".to_vec());
        assert!(d.get_data("").is_none());
    }

    #[test]
    fn get_data_empty_key_is_a_real_key_when_stored() {
        // "" is not special-cased: it is a perfectly good (if silly) map key.
        let mut d = DragData::new();
        d.set_data("", b"empty-key".to_vec());
        assert_eq!(d.get_data(""), Some(&b"empty-key"[..]));
        assert!(d.get_data("text/plain").is_none());
    }

    #[test]
    fn get_data_whitespace_only_keys_return_none() {
        let mut d = DragData::new();
        d.set_data("text/plain", b"x".to_vec());
        for k in ["   ", "\t\n", "\r\n", "\u{a0}", "\u{2028}"] {
            assert!(d.get_data(k).is_none(), "whitespace key {k:?} matched");
        }
    }

    #[test]
    fn get_data_garbage_bytes_return_none_without_panicking() {
        let mut d = DragData::new();
        d.set_data("text/plain", b"x".to_vec());
        for k in [
            "\u{0}",
            "\u{0}\u{1}\u{2}\u{7f}",
            "\u{feff}",
            "%%%;;;///",
            "text/plain\u{0}",
            "\u{0}text/plain",
        ] {
            assert!(d.get_data(k).is_none(), "garbage key {k:?} matched");
        }
    }

    #[test]
    fn get_data_leading_trailing_junk_is_not_trimmed() {
        let mut d = DragData::new();
        d.set_data("text/plain", b"x".to_vec());
        // Lookup is an exact byte-for-byte match: no trimming, no tolerance.
        assert!(d.get_data("  text/plain  ").is_none());
        assert!(d.get_data("text/plain;garbage").is_none());
        assert!(d.get_data("text/plain ").is_none());
        assert!(d.get_data(" text/plain").is_none());
        assert_eq!(d.get_data("text/plain"), Some(&b"x"[..]));
    }

    #[test]
    fn get_data_is_case_sensitive() {
        // MIME types are case-insensitive per RFC 2045, but this map is not.
        let mut d = DragData::new();
        d.set_data("text/plain", b"x".to_vec());
        assert!(d.get_data("TEXT/PLAIN").is_none());
        assert!(d.get_data("Text/Plain").is_none());
    }

    #[test]
    fn get_data_boundary_number_strings_return_none() {
        let mut d = DragData::new();
        d.set_data("text/plain", b"x".to_vec());
        for k in [
            "0",
            "-0",
            "9223372036854775807",  // i64::MAX
            "-9223372036854775808", // i64::MIN
            "18446744073709551615", // u64::MAX
            "1e400",
            "NaN",
            "inf",
            "-inf",
            "1.7976931348623157e308",
            "5e-324",
        ] {
            assert!(d.get_data(k).is_none(), "numeric key {k:?} matched");
        }
    }

    #[test]
    fn get_data_unicode_keys_round_trip() {
        let mut d = DragData::new();
        let emoji = "application/x-\u{1F600};charset=utf-8";
        let combining = "text/e\u{0301}"; // e + combining acute
        d.set_data(emoji, b"grin".to_vec());
        d.set_data(combining, b"acute".to_vec());

        assert_eq!(d.get_data(emoji), Some(&b"grin"[..]));
        assert_eq!(d.get_data(combining), Some(&b"acute"[..]));
        // NFC-equivalent but byte-distinct key must NOT match (no normalization).
        assert!(d.get_data("text/\u{e9}").is_none());
        assert_eq!(d.data.len(), 2);
    }

    #[test]
    fn get_data_extremely_long_key_does_not_panic_or_hang() {
        let huge: String = "a".repeat(1_000_000);
        let mut d = DragData::new();
        d.set_data("text/plain", b"x".to_vec());
        // Miss against a 1M-char key.
        assert!(d.get_data(&huge).is_none());
        // Round-trip the 1M-char key itself.
        d.set_data(huge.as_str(), b"huge-key".to_vec());
        assert_eq!(d.get_data(&huge), Some(&b"huge-key"[..]));
        // A 1M-char key that differs only in the last byte must miss.
        let mut nearly = huge.clone();
        let _ = nearly.pop();
        nearly.push('b');
        assert!(d.get_data(&nearly).is_none());
    }

    #[test]
    fn get_data_deeply_nested_brackets_do_not_stack_overflow() {
        // The lookup is a linear scan, not a recursive-descent parse: 10k
        // nested brackets must be inert.
        let nested: String = "[".repeat(10_000);
        let mut d = DragData::new();
        assert!(d.get_data(&nested).is_none());
        d.set_data(nested.as_str(), b"nested".to_vec());
        assert_eq!(d.get_data(&nested), Some(&b"nested"[..]));
    }

    #[test]
    fn set_data_replaces_existing_entry_for_same_mime() {
        let mut d = DragData::new();
        d.set_data("text/plain", b"first".to_vec());
        d.set_data("text/plain", b"second".to_vec());
        assert_eq!(d.data.len(), 1, "duplicate MIME key was appended");
        assert_eq!(d.get_data("text/plain"), Some(&b"second"[..]));
    }

    #[test]
    fn set_data_empty_payload_is_some_empty_not_none() {
        let mut d = DragData::new();
        d.set_data("application/octet-stream", Vec::new());
        // Presence and emptiness are distinguishable.
        assert_eq!(d.get_data("application/octet-stream"), Some(&b""[..]));
        assert!(d.get_data("application/octet-stream").is_some());
    }

    #[test]
    fn set_data_huge_payload_round_trips() {
        let mut d = DragData::new();
        let payload = alloc::vec![0xABu8; 1 << 20]; // 1 MiB
        d.set_data("application/octet-stream", payload);
        let got = d.get_data("application/octet-stream").unwrap();
        assert_eq!(got.len(), 1 << 20);
        assert!(got.iter().all(|b| *b == 0xAB));
    }

    #[test]
    fn set_data_binary_payload_is_not_utf8_validated() {
        let mut d = DragData::new();
        d.set_data("application/octet-stream", alloc::vec![0xFF, 0x00, 0xFE]);
        assert_eq!(d.get_data("application/octet-stream"), Some(&[0xFFu8, 0x00, 0xFE][..]));
    }

    #[test]
    fn set_data_many_distinct_mime_types_all_retrievable() {
        let mut d = DragData::new();
        for i in 0..1_000usize {
            let mut key = String::from("application/x-");
            key.push_str(&i.to_string());
            d.set_data(key.as_str(), alloc::vec![(i % 251) as u8]);
        }
        assert_eq!(d.data.len(), 1_000);
        assert_eq!(d.get_data("application/x-0"), Some(&[0u8][..]));
        assert_eq!(d.get_data("application/x-999"), Some(&[(999 % 251) as u8][..]));
        assert!(d.get_data("application/x-1000").is_none());
    }

    #[test]
    fn set_text_get_text_round_trip_unicode() {
        for s in [
            "hello",
            "",
            "\u{1F600}\u{1F4A9}",
            "e\u{0301}\u{0300}\u{0308}", // stacked combining marks
            "\u{200B}zero-width",
            "line1\nline2\r\n\ttab",
            "\u{0}interior nul\u{0}",
        ] {
            let mut d = DragData::new();
            d.set_text(s);
            let got = d.get_text().expect("text/plain must be present");
            assert_eq!(got.as_str(), s, "round-trip failed for {s:?}");
        }
    }

    #[test]
    fn set_text_empty_is_some_not_none() {
        let mut d = DragData::new();
        d.set_text("");
        assert!(d.get_text().is_some());
        assert_eq!(d.get_text().unwrap().as_str(), "");
        assert_eq!(d.get_data("text/plain"), Some(&b""[..]));
    }

    #[test]
    fn set_text_huge_string_round_trips() {
        let huge: String = "\u{1F600}".repeat(100_000); // 400_000 bytes
        let mut d = DragData::new();
        d.set_text(huge.as_str());
        let got = d.get_text().unwrap();
        assert_eq!(got.as_str().len(), 400_000);
        assert_eq!(got.as_str(), huge.as_str());
        assert_eq!(d.data.len(), 1);
    }

    #[test]
    fn set_text_twice_replaces_and_does_not_grow() {
        let mut d = DragData::new();
        d.set_text("one");
        d.set_text("two");
        assert_eq!(d.data.len(), 1);
        assert_eq!(d.get_text().unwrap().as_str(), "two");
    }

    #[test]
    fn set_text_then_set_data_on_same_mime_wins() {
        let mut d = DragData::new();
        d.set_text("text");
        d.set_data("text/plain", b"raw".to_vec());
        assert_eq!(d.data.len(), 1);
        assert_eq!(d.get_text().unwrap().as_str(), "raw");
    }

    #[test]
    fn get_text_on_invalid_utf8_yields_empty_string_not_panic() {
        // Documents the `unwrap_or("")` fallback: invalid UTF-8 under
        // "text/plain" is silently reported as an EMPTY string, not None and
        // not a panic. (Lossy data: the bytes are still there via get_data.)
        let mut d = DragData::new();
        d.set_data("text/plain", alloc::vec![0xFF, 0xFE, 0x80]);
        let got = d.get_text().expect("entry exists, so Some");
        assert_eq!(got.as_str(), "");
        assert_eq!(d.get_data("text/plain"), Some(&[0xFFu8, 0xFE, 0x80][..]));
    }

    #[test]
    fn get_text_truncated_utf8_yields_empty_string() {
        let mut d = DragData::new();
        // First 3 bytes of a 4-byte emoji.
        d.set_data("text/plain", alloc::vec![0xF0, 0x9F, 0x98]);
        assert_eq!(d.get_text().unwrap().as_str(), "");
    }

    #[test]
    fn get_text_is_none_when_only_other_mimes_present() {
        let mut d = DragData::new();
        d.set_data("text/html", b"<b>x</b>".to_vec());
        assert!(d.get_text().is_none());
    }

    // ==================================================== DragContext ctors

    #[test]
    fn drag_context_new_preserves_session_id_and_is_not_cancelled() {
        for sid in [0u64, 1, u64::MAX] {
            let ctx = DragContext::new(
                ActiveDragType::WindowMove(WindowMoveDrag {
                    start_position: LogicalPosition::zero(),
                    current_position: LogicalPosition::zero(),
                    initial_window_position: WindowPosition::Uninitialized,
                }),
                sid,
            );
            assert_eq!(ctx.session_id, sid);
            assert!(!ctx.cancelled);
        }
    }

    #[test]
    fn text_selection_invariants_at_extremes() {
        let pos = LogicalPosition::new(f32::MIN, f32::MAX);
        let ctx = DragContext::text_selection(
            dom(usize::MAX),
            NodeId::new(usize::MAX),
            pos,
            u64::MAX,
        );
        let ts = ctx.as_text_selection().expect("must be a text selection");
        assert_eq!(ts.dom_id, dom(usize::MAX));
        assert_eq!(ts.anchor_ifc_node, NodeId::new(usize::MAX));
        assert!(ts.anchor_cursor.is_none());
        assert!(ts.auto_scroll_container.is_none());
        assert_eq!(ts.auto_scroll_direction, AutoScrollDirection::None);
        // start == current at construction, bit-for-bit (quantized PartialEq
        // would happily accept a saturated mismatch here, so compare raw bits).
        assert_eq!(ts.start_mouse_position.x.to_bits(), f32::MIN.to_bits());
        assert_eq!(ts.start_mouse_position.y.to_bits(), f32::MAX.to_bits());
        assert_eq!(
            ts.current_mouse_position.x.to_bits(),
            ts.start_mouse_position.x.to_bits()
        );
        assert_eq!(ctx.session_id, u64::MAX);
    }

    #[test]
    fn text_selection_with_nan_position_does_not_panic() {
        let ctx = DragContext::text_selection(
            dom(0),
            NodeId::ZERO,
            LogicalPosition::new(f32::NAN, f32::NEG_INFINITY),
            0,
        );
        let ts = ctx.as_text_selection().unwrap();
        assert!(ts.start_mouse_position.x.is_nan());
        assert!(ts.current_mouse_position.x.is_nan());
        assert_eq!(ts.start_mouse_position.y, f32::NEG_INFINITY);
    }

    #[test]
    fn scrollbar_thumb_stores_all_float_metrics_verbatim_incl_nan_inf() {
        let ctx = DragContext::scrollbar_thumb(
            dom(3),
            NodeId::new(9),
            ScrollbarAxis::Horizontal,
            LogicalPosition::new(f32::INFINITY, f32::NEG_INFINITY),
            f32::NAN,
            f32::INFINITY,
            -0.0,
            f32::MAX,
            0,
        );
        let sb = ctx.as_scrollbar_thumb().unwrap();
        assert_eq!(sb.dom_id, dom(3));
        assert_eq!(sb.scroll_container_node, NodeId::new(9));
        assert_eq!(sb.axis, ScrollbarAxis::Horizontal);
        assert!(sb.start_scroll_offset.is_nan(), "NaN was mangled at construction");
        assert_eq!(sb.track_length_px, f32::INFINITY);
        assert!(sb.content_length_px.is_sign_negative(), "-0.0 lost its sign");
        assert_eq!(sb.content_length_px, 0.0);
        assert_eq!(sb.viewport_length_px, f32::MAX);
        assert_eq!(sb.current_mouse_position.x, f32::INFINITY);
    }

    #[test]
    fn scrollbar_thumb_all_zero_is_constructible() {
        let ctx = vscroll(0.0, 0.0, 0.0, 0.0);
        assert!(ctx.is_scrollbar_thumb());
        let sb = ctx.as_scrollbar_thumb().unwrap();
        assert_eq!(sb.start_scroll_offset, 0.0);
        assert_eq!(sb.track_length_px, 0.0);
    }

    #[test]
    fn node_drag_invariants_hold_after_construction() {
        let mut data = DragData::new();
        data.set_text("payload");
        let ctx = DragContext::node_drag(
            dom(2),
            NodeId::new(usize::MAX),
            LogicalPosition::new(-1.5, 2.5),
            data,
            u64::MAX,
        );
        let nd = ctx.as_node_drag().unwrap();
        assert_eq!(nd.dom_id, dom(2));
        assert_eq!(nd.node_id, NodeId::new(usize::MAX));
        assert_eq!(nd.start_position, nd.current_position);
        assert_eq!(nd.drag_offset, LogicalPosition::zero());
        assert!(nd.current_drop_target.into_option().is_none());
        assert!(nd.previous_drop_target.into_option().is_none());
        assert!(!nd.drop_accepted);
        assert_eq!(nd.drop_effect, DropEffect::None);
        assert_eq!(nd.drag_data.get_text().unwrap().as_str(), "payload");
    }

    #[test]
    fn node_drag_with_empty_drag_data_is_fine() {
        let ctx = DragContext::node_drag(
            dom(0),
            NodeId::ZERO,
            LogicalPosition::new(f32::NAN, f32::NAN),
            DragData::new(),
            0,
        );
        let nd = ctx.as_node_drag().unwrap();
        assert!(nd.drag_data.data.is_empty());
        assert!(nd.start_position.x.is_nan());
        assert!(nd.current_position.x.is_nan());
    }

    #[test]
    fn window_move_preserves_initial_window_position_extremes() {
        for wp in [
            WindowPosition::Uninitialized,
            WindowPosition::Initialized(PhysicalPosition {
                x: i32::MIN,
                y: i32::MAX,
            }),
            WindowPosition::Initialized(PhysicalPosition { x: 0, y: 0 }),
        ] {
            let ctx = DragContext::window_move(
                LogicalPosition::new(f32::MAX, f32::MIN),
                wp,
                u64::MAX,
            );
            let wm = ctx.as_window_move().unwrap();
            assert_eq!(wm.initial_window_position, wp);
            assert_eq!(wm.start_position.x.to_bits(), f32::MAX.to_bits());
            assert_eq!(
                wm.current_position.y.to_bits(),
                wm.start_position.y.to_bits()
            );
        }
    }

    #[test]
    fn file_drop_empty_file_list_is_allowed() {
        let ctx = DragContext::file_drop(Vec::new(), LogicalPosition::zero(), 0);
        let fd = ctx.as_file_drop().unwrap();
        assert_eq!(fd.files.len(), 0);
        assert!(fd.drop_target.into_option().is_none());
        assert_eq!(fd.drop_effect, DropEffect::Copy);
    }

    #[test]
    fn file_drop_unicode_and_pathological_filenames_round_trip() {
        let files = alloc::vec![
            AzString::from(""),
            AzString::from("/tmp/\u{1F4C1}/f\u{0301}ile.txt"),
            AzString::from("C:\\Windows\\..\\..\\etc\\passwd"),
            AzString::from("a".repeat(4096)),
            AzString::from("with\nnewline\tand\u{0}nul"),
        ];
        let ctx = DragContext::file_drop(files, LogicalPosition::new(1.0, 2.0), 42);
        let fd = ctx.as_file_drop().unwrap();
        assert_eq!(fd.files.len(), 5);
        assert_eq!(fd.files.as_slice()[0].as_str(), "");
        assert_eq!(fd.files.as_slice()[3].as_str().len(), 4096);
        assert_eq!(fd.files.as_slice()[4].as_str(), "with\nnewline\tand\u{0}nul");
        assert_eq!(ctx.session_id, 42);
    }

    #[test]
    fn file_drop_ten_thousand_files_does_not_hang() {
        let mut files = Vec::with_capacity(10_000);
        for i in 0..10_000usize {
            files.push(AzString::from(i.to_string()));
        }
        let ctx = DragContext::file_drop(files, LogicalPosition::zero(), 1);
        assert_eq!(ctx.as_file_drop().unwrap().files.len(), 10_000);
    }

    // ================================================ predicates / accessors

    fn one_of_each() -> [DragContext; 6] {
        [
            DragContext::text_selection(dom(0), NodeId::ZERO, LogicalPosition::zero(), 0),
            vscroll(0.0, 100.0, 200.0, 100.0),
            DragContext::node_drag(
                dom(0),
                NodeId::ZERO,
                LogicalPosition::zero(),
                DragData::new(),
                0,
            ),
            DragContext::window_move(
                LogicalPosition::zero(),
                WindowPosition::Uninitialized,
                0,
            ),
            resize_ctx(),
            DragContext::file_drop(Vec::new(), LogicalPosition::zero(), 0),
        ]
    }

    #[test]
    fn predicates_are_mutually_exclusive_across_every_variant() {
        for (i, ctx) in one_of_each().iter().enumerate() {
            let flags = [
                ctx.is_text_selection(),
                ctx.is_scrollbar_thumb(),
                ctx.is_node_drag(),
                ctx.is_window_move(),
                ctx.is_file_drop(),
            ];
            let set = flags.iter().filter(|f| **f).count();
            if i == 4 {
                // WindowResize has no predicate: every is_* must be false.
                assert_eq!(set, 0, "WindowResize matched a predicate");
            } else {
                assert_eq!(set, 1, "variant {i} matched {set} predicates, expected 1");
                assert!(flags[if i == 5 { 4 } else { i }], "wrong predicate for {i}");
            }
        }
    }

    #[test]
    fn as_accessors_return_none_for_every_non_matching_variant() {
        for (i, ctx) in one_of_each().iter().enumerate() {
            assert_eq!(ctx.as_text_selection().is_some(), i == 0);
            assert_eq!(ctx.as_scrollbar_thumb().is_some(), i == 1);
            assert_eq!(ctx.as_node_drag().is_some(), i == 2);
            assert_eq!(ctx.as_window_move().is_some(), i == 3);
            assert_eq!(ctx.as_file_drop().is_some(), i == 5);
        }
    }

    #[test]
    fn as_mut_accessors_return_none_for_every_non_matching_variant() {
        for (i, ctx) in one_of_each().iter_mut().enumerate() {
            assert_eq!(ctx.as_text_selection_mut().is_some(), i == 0);
            assert_eq!(ctx.as_scrollbar_thumb_mut().is_some(), i == 1);
            assert_eq!(ctx.as_node_drag_mut().is_some(), i == 2);
            assert_eq!(ctx.as_file_drop_mut().is_some(), i == 5);
        }
    }

    #[test]
    fn as_text_selection_mut_writes_are_visible_through_shared_getter() {
        let mut ctx =
            DragContext::text_selection(dom(0), NodeId::new(1), LogicalPosition::zero(), 0);
        {
            let ts = ctx.as_text_selection_mut().unwrap();
            ts.auto_scroll_direction = AutoScrollDirection::DownRight;
            ts.auto_scroll_container = Some(NodeId::new(77));
        }
        let ts = ctx.as_text_selection().unwrap();
        assert_eq!(ts.auto_scroll_direction, AutoScrollDirection::DownRight);
        assert_eq!(ts.auto_scroll_container, Some(NodeId::new(77)));
    }

    #[test]
    fn as_scrollbar_thumb_mut_writes_are_visible_through_shared_getter() {
        let mut ctx = vscroll(0.0, 100.0, 200.0, 100.0);
        ctx.as_scrollbar_thumb_mut().unwrap().start_scroll_offset = f32::NAN;
        assert!(ctx.as_scrollbar_thumb().unwrap().start_scroll_offset.is_nan());
    }

    #[test]
    fn as_node_drag_mut_writes_are_visible_through_shared_getter() {
        let mut ctx = DragContext::node_drag(
            dom(0),
            NodeId::ZERO,
            LogicalPosition::zero(),
            DragData::new(),
            0,
        );
        {
            let nd = ctx.as_node_drag_mut().unwrap();
            nd.drop_accepted = true;
            nd.drop_effect = DropEffect::Move;
            nd.current_drop_target = Some(dnid(0, 4)).into();
        }
        let nd = ctx.as_node_drag().unwrap();
        assert!(nd.drop_accepted);
        assert_eq!(nd.drop_effect, DropEffect::Move);
        assert_eq!(
            nd.current_drop_target
                .into_option()
                .unwrap()
                .node
                .into_crate_internal(),
            Some(NodeId::new(4))
        );
    }

    #[test]
    fn as_file_drop_mut_writes_are_visible_through_shared_getter() {
        let mut ctx = DragContext::file_drop(Vec::new(), LogicalPosition::zero(), 0);
        ctx.as_file_drop_mut().unwrap().drop_effect = DropEffect::Link;
        assert_eq!(ctx.as_file_drop().unwrap().drop_effect, DropEffect::Link);
    }

    // ============================================== update / position getters

    #[test]
    fn update_position_moves_current_for_every_variant_and_leaves_start_alone() {
        let new_pos = LogicalPosition::new(123.5, -456.25);
        for (i, mut ctx) in one_of_each().into_iter().enumerate() {
            let start_before = ctx.start_position();
            ctx.update_position(new_pos);
            assert_eq!(ctx.current_position(), new_pos, "variant {i} did not move");
            if i == 5 {
                // FileDrop has a single `position` field: start aliases current,
                // so updating the position also moves the reported start.
                assert_eq!(ctx.start_position(), new_pos);
            } else {
                assert_eq!(ctx.start_position(), start_before, "variant {i} start moved");
            }
        }
    }

    #[test]
    fn update_position_with_nan_and_inf_is_stored_verbatim() {
        for mut ctx in one_of_each() {
            ctx.update_position(LogicalPosition::new(f32::NAN, f32::INFINITY));
            let cur = ctx.current_position();
            assert!(cur.x.is_nan());
            assert_eq!(cur.y, f32::INFINITY);

            ctx.update_position(LogicalPosition::new(f32::MIN, f32::NEG_INFINITY));
            let cur = ctx.current_position();
            assert_eq!(cur.x.to_bits(), f32::MIN.to_bits());
            assert_eq!(cur.y, f32::NEG_INFINITY);
        }
    }

    #[test]
    fn update_position_is_idempotent_and_last_write_wins() {
        let mut ctx = vscroll(0.0, 100.0, 200.0, 100.0);
        for i in 0..1000u32 {
            ctx.update_position(LogicalPosition::new(i as f32, -(i as f32)));
        }
        assert_eq!(ctx.current_position(), LogicalPosition::new(999.0, -999.0));
        assert_eq!(ctx.start_position(), LogicalPosition::zero());
    }

    #[test]
    fn window_resize_position_getters_work_without_a_predicate() {
        let mut ctx = resize_ctx();
        assert_eq!(ctx.start_position(), LogicalPosition::new(1.0, 2.0));
        assert_eq!(ctx.current_position(), LogicalPosition::new(1.0, 2.0));
        ctx.update_position(LogicalPosition::new(-3.0, -4.0));
        assert_eq!(ctx.current_position(), LogicalPosition::new(-3.0, -4.0));
        assert_eq!(ctx.start_position(), LogicalPosition::new(1.0, 2.0));
        // No as_window_resize() accessor exists; the others must all say None.
        assert!(ctx.as_text_selection().is_none());
        assert!(ctx.as_window_move().is_none());
    }

    // ===================================== calculate_scrollbar_scroll_offset

    #[test]
    fn scroll_offset_is_none_for_non_scrollbar_drags() {
        for (i, ctx) in one_of_each().iter().enumerate() {
            if i == 1 {
                continue;
            }
            assert!(
                ctx.calculate_scrollbar_scroll_offset().is_none(),
                "variant {i} returned Some"
            );
        }
    }

    #[test]
    fn scroll_offset_basic_vertical_half_track() {
        // track=100, content=200, viewport=100 => range=100, thumb=50,
        // scrollable_track=50. A 25px drag is half the scrollable track =>
        // half the range = 50.
        let mut ctx = vscroll(0.0, 100.0, 200.0, 100.0);
        ctx.update_position(LogicalPosition::new(0.0, 25.0));
        assert_eq!(ctx.calculate_scrollbar_scroll_offset(), Some(50.0));
    }

    #[test]
    fn scroll_offset_horizontal_uses_x_and_ignores_y() {
        let mut ctx = DragContext::scrollbar_thumb(
            dom(0),
            NodeId::new(1),
            ScrollbarAxis::Horizontal,
            LogicalPosition::zero(),
            0.0,
            100.0,
            200.0,
            100.0,
            0,
        );
        // Pure vertical movement must not scroll a horizontal scrollbar.
        ctx.update_position(LogicalPosition::new(0.0, 9999.0));
        assert_eq!(ctx.calculate_scrollbar_scroll_offset(), Some(0.0));
        ctx.update_position(LogicalPosition::new(25.0, 9999.0));
        assert_eq!(ctx.calculate_scrollbar_scroll_offset(), Some(50.0));
    }

    #[test]
    fn scroll_offset_clamps_to_range_on_huge_and_infinite_drags() {
        let mut ctx = vscroll(0.0, 100.0, 200.0, 100.0); // range = 100
        for (y, expect) in [
            (1.0e30f32, 100.0f32),
            (f32::MAX, 100.0),
            (f32::INFINITY, 100.0),
            (-1.0e30, 0.0),
            (f32::MIN, 0.0),
            (f32::NEG_INFINITY, 0.0),
        ] {
            ctx.update_position(LogicalPosition::new(0.0, y));
            assert_eq!(
                ctx.calculate_scrollbar_scroll_offset(),
                Some(expect),
                "y = {y}"
            );
        }
    }

    #[test]
    fn scroll_offset_result_always_within_range_for_finite_inputs() {
        let mut ctx = vscroll(30.0, 80.0, 500.0, 120.0);
        let range = 500.0 - 120.0;
        for y in [-1e9f32, -1.0, 0.0, 0.5, 1.0, 37.0, 1e9] {
            ctx.update_position(LogicalPosition::new(0.0, y));
            let off = ctx.calculate_scrollbar_scroll_offset().unwrap();
            assert!(
                (0.0..=range).contains(&off),
                "offset {off} escaped [0, {range}] for y = {y}"
            );
        }
    }

    #[test]
    fn scroll_offset_out_of_range_start_offset_is_clamped_back_in() {
        // Even with zero mouse movement, a bogus start offset must be clamped.
        let ctx = vscroll(9999.0, 100.0, 200.0, 100.0);
        assert_eq!(ctx.calculate_scrollbar_scroll_offset(), Some(100.0));
        let ctx = vscroll(-9999.0, 100.0, 200.0, 100.0);
        assert_eq!(ctx.calculate_scrollbar_scroll_offset(), Some(0.0));
    }

    #[test]
    fn scroll_offset_non_scrollable_content_returns_start_offset_unchanged() {
        // content <= viewport => nothing to scroll; the (possibly out-of-range)
        // start offset is returned verbatim, WITHOUT clamping.
        let mut ctx = vscroll(7.0, 100.0, 50.0, 100.0);
        ctx.update_position(LogicalPosition::new(0.0, 1000.0));
        assert_eq!(ctx.calculate_scrollbar_scroll_offset(), Some(7.0));

        let ctx = vscroll(7.0, 100.0, 100.0, 100.0); // range == 0
        assert_eq!(ctx.calculate_scrollbar_scroll_offset(), Some(7.0));

        let ctx = vscroll(7.0, 100.0, 0.0, 0.0); // all zero metrics
        assert_eq!(ctx.calculate_scrollbar_scroll_offset(), Some(7.0));
    }

    #[test]
    fn scroll_offset_zero_or_negative_track_returns_start_offset() {
        for track in [0.0f32, -1.0, -1e30, f32::NEG_INFINITY] {
            let mut ctx = vscroll(7.0, track, 200.0, 100.0);
            ctx.update_position(LogicalPosition::new(0.0, 50.0));
            assert_eq!(
                ctx.calculate_scrollbar_scroll_offset(),
                Some(7.0),
                "track = {track}"
            );
        }
    }

    #[test]
    fn scroll_offset_negative_content_and_viewport_return_start_offset() {
        let mut ctx = vscroll(3.0, 100.0, -200.0, -100.0);
        ctx.update_position(LogicalPosition::new(0.0, 50.0));
        // range = -200 - (-100) = -100 <= 0 => early out.
        assert_eq!(ctx.calculate_scrollbar_scroll_offset(), Some(3.0));
    }

    #[test]
    fn scroll_offset_nan_start_offset_propagates_nan_without_panicking() {
        let mut ctx = vscroll(f32::NAN, 100.0, 200.0, 100.0);
        ctx.update_position(LogicalPosition::new(0.0, 10.0));
        let off = ctx.calculate_scrollbar_scroll_offset().expect("Some");
        // NaN start offset is neither clamped nor rejected: it leaks through.
        assert!(off.is_nan());
    }

    #[test]
    fn scroll_offset_nan_track_length_propagates_nan_without_panicking() {
        let mut ctx = vscroll(0.0, f32::NAN, 200.0, 100.0);
        ctx.update_position(LogicalPosition::new(0.0, 10.0));
        let off = ctx.calculate_scrollbar_scroll_offset().expect("Some");
        // NaN track passes the `track_length_px <= 0.0` guard (NaN compares
        // false) and poisons the result. min/max of the clamp stay finite, so
        // no panic — just a NaN scroll offset handed to the caller.
        assert!(off.is_nan());
    }

    #[test]
    fn scroll_offset_nan_mouse_position_propagates_nan_without_panicking() {
        let mut ctx = vscroll(0.0, 100.0, 200.0, 100.0);
        ctx.update_position(LogicalPosition::new(0.0, f32::NAN));
        let off = ctx.calculate_scrollbar_scroll_offset().expect("Some");
        assert!(off.is_nan());
    }

    #[test]
    fn scroll_offset_infinite_content_yields_nan_on_zero_mouse_delta() {
        // range = inf, thumb = 0, ratio = 0 => scroll_delta = 0.0 * inf = NaN.
        // Not a panic (clamp's min/max are 0.0/inf), but the returned offset is
        // NaN even though the mouse never moved.
        let ctx = vscroll(0.0, 100.0, f32::INFINITY, 100.0);
        let off = ctx.calculate_scrollbar_scroll_offset().expect("Some");
        assert!(off.is_nan(), "expected the 0 * inf NaN, got {off}");
    }

    // BUG: f32::clamp(min, max) asserts `min <= max`, and every NaN comparison
    // is false — so a NaN `scrollable_range` (from a NaN or inf/inf content or
    // viewport metric) makes `new_offset.clamp(0.0, scrollable_range)` PANIC
    // inside a getter. A layout that produced a NaN content length would take
    // the whole app down on the next scrollbar drag event. The guards at the
    // top of the function do not catch it because `NaN <= 0.0` is false.
    #[test]
    #[should_panic]
    fn scroll_offset_nan_content_length_panics_in_clamp() {
        let mut ctx = vscroll(0.0, 100.0, f32::NAN, 100.0);
        ctx.update_position(LogicalPosition::new(0.0, 10.0));
        let _ = ctx.calculate_scrollbar_scroll_offset();
    }

    #[test]
    #[should_panic]
    fn scroll_offset_nan_viewport_length_panics_in_clamp() {
        let mut ctx = vscroll(0.0, 100.0, 200.0, f32::NAN);
        ctx.update_position(LogicalPosition::new(0.0, 10.0));
        let _ = ctx.calculate_scrollbar_scroll_offset();
    }

    #[test]
    #[should_panic]
    fn scroll_offset_infinite_content_and_viewport_panics_in_clamp() {
        // inf - inf = NaN scrollable_range => same clamp assertion.
        let mut ctx = vscroll(0.0, 100.0, f32::INFINITY, f32::INFINITY);
        ctx.update_position(LogicalPosition::new(0.0, 10.0));
        let _ = ctx.calculate_scrollbar_scroll_offset();
    }

    // ================================================ remap_node_ids / targets

    #[test]
    fn remap_text_selection_other_dom_is_a_no_op_and_succeeds() {
        let mut ctx =
            DragContext::text_selection(dom(0), NodeId::new(5), LogicalPosition::zero(), 0);
        assert!(ctx.remap_node_ids(dom(1), &nid_map(&[(5, 500)])));
        assert_eq!(ctx.as_text_selection().unwrap().anchor_ifc_node, NodeId::new(5));
    }

    #[test]
    fn remap_text_selection_missing_anchor_cancels_the_drag() {
        let mut ctx =
            DragContext::text_selection(dom(0), NodeId::new(5), LogicalPosition::zero(), 0);
        assert!(!ctx.remap_node_ids(dom(0), &BTreeMap::new()));
        assert!(!ctx.remap_node_ids(dom(0), &nid_map(&[(6, 7)])));
        // The anchor is left untouched when the remap fails.
        assert_eq!(ctx.as_text_selection().unwrap().anchor_ifc_node, NodeId::new(5));
    }

    #[test]
    fn remap_text_selection_clears_only_the_missing_auto_scroll_container() {
        let mut ctx =
            DragContext::text_selection(dom(0), NodeId::new(5), LogicalPosition::zero(), 0);
        ctx.as_text_selection_mut().unwrap().auto_scroll_container = Some(NodeId::new(9));

        // Container survives if it is in the map.
        assert!(ctx.remap_node_ids(dom(0), &nid_map(&[(5, 50), (9, 90)])));
        let ts = ctx.as_text_selection().unwrap();
        assert_eq!(ts.anchor_ifc_node, NodeId::new(50));
        assert_eq!(ts.auto_scroll_container, Some(NodeId::new(90)));

        // Container is dropped (but the drag survives) if it is gone.
        assert!(ctx.remap_node_ids(dom(0), &nid_map(&[(50, 51)])));
        let ts = ctx.as_text_selection().unwrap();
        assert_eq!(ts.anchor_ifc_node, NodeId::new(51));
        assert_eq!(ts.auto_scroll_container, None);
    }

    #[test]
    fn remap_scrollbar_empty_map_cancels_the_drag() {
        let mut ctx = vscroll(0.0, 100.0, 200.0, 100.0); // node 1, dom 0
        assert!(!ctx.remap_node_ids(dom(0), &BTreeMap::new()));
        assert_eq!(
            ctx.as_scrollbar_thumb().unwrap().scroll_container_node,
            NodeId::new(1)
        );
    }

    #[test]
    fn remap_node_drag_missing_node_cancels_and_leaves_targets_alone() {
        let mut ctx = DragContext::node_drag(
            dom(0),
            NodeId::new(1),
            LogicalPosition::zero(),
            DragData::new(),
            0,
        );
        ctx.as_node_drag_mut().unwrap().current_drop_target = Some(dnid(0, 5)).into();

        // Map covers the drop target but NOT the dragged node => cancel.
        assert!(!ctx.remap_node_ids(dom(0), &nid_map(&[(5, 50)])));
        let nd = ctx.as_node_drag().unwrap();
        assert_eq!(nd.node_id, NodeId::new(1));
        assert_eq!(
            nd.current_drop_target
                .into_option()
                .unwrap()
                .node
                .into_crate_internal(),
            Some(NodeId::new(5)),
            "targets must not be half-remapped on a cancelled drag"
        );
    }

    #[test]
    fn remap_node_drag_clears_drop_targets_that_were_removed() {
        let mut ctx = DragContext::node_drag(
            dom(0),
            NodeId::new(1),
            LogicalPosition::zero(),
            DragData::new(),
            0,
        );
        {
            let nd = ctx.as_node_drag_mut().unwrap();
            nd.current_drop_target = Some(dnid(0, 5)).into();
            nd.previous_drop_target = Some(dnid(0, 6)).into();
        }
        // Only the dragged node survives; both targets are gone.
        assert!(ctx.remap_node_ids(dom(0), &nid_map(&[(1, 100)])));
        let nd = ctx.as_node_drag().unwrap();
        assert_eq!(nd.node_id, NodeId::new(100));
        assert!(nd.current_drop_target.into_option().is_none());
        assert!(nd.previous_drop_target.into_option().is_none());
    }

    #[test]
    fn remap_does_not_touch_drop_targets_belonging_to_another_dom() {
        let mut ctx = DragContext::node_drag(
            dom(0),
            NodeId::new(1),
            LogicalPosition::zero(),
            DragData::new(),
            0,
        );
        // Drop target lives in DOM 1 (a different DOM than the drag).
        ctx.as_node_drag_mut().unwrap().current_drop_target = Some(dnid(1, 5)).into();

        assert!(ctx.remap_node_ids(dom(0), &nid_map(&[(1, 100), (5, 500)])));
        let nd = ctx.as_node_drag().unwrap();
        assert_eq!(nd.node_id, NodeId::new(100));
        let dt = nd.current_drop_target.into_option().unwrap();
        assert_eq!(dt.dom, dom(1));
        assert_eq!(dt.node.into_crate_internal(), Some(NodeId::new(5)));
    }

    #[test]
    fn remap_drop_target_without_a_node_id_is_left_intact() {
        // A DomNodeId whose node encodes `None` must not be cleared or panic.
        let mut ctx = DragContext::node_drag(
            dom(0),
            NodeId::new(1),
            LogicalPosition::zero(),
            DragData::new(),
            0,
        );
        ctx.as_node_drag_mut().unwrap().current_drop_target = Some(dnid_no_node(0)).into();

        assert!(ctx.remap_node_ids(dom(0), &nid_map(&[(1, 1)])));
        let dt = ctx
            .as_node_drag()
            .unwrap()
            .current_drop_target
            .into_option()
            .expect("target must survive");
        assert_eq!(dt.dom, dom(0));
        assert_eq!(dt.node.into_crate_internal(), None);
    }

    #[test]
    fn remap_drop_target_directly_handles_none_and_foreign_doms() {
        // Exercise the private helper on its own.
        let mut none_target = OptionDomNodeId::None;
        DragContext::remap_drop_target(&mut none_target, dom(0), &nid_map(&[(1, 2)]));
        assert!(none_target.into_option().is_none());

        let mut foreign: OptionDomNodeId = Some(dnid(9, 1)).into();
        DragContext::remap_drop_target(&mut foreign, dom(0), &nid_map(&[(1, 2)]));
        let dt = foreign.into_option().unwrap();
        assert_eq!(dt.dom, dom(9));
        assert_eq!(dt.node.into_crate_internal(), Some(NodeId::new(1)));

        // Empty map on a matching DOM clears the target.
        let mut matching: OptionDomNodeId = Some(dnid(0, 1)).into();
        DragContext::remap_drop_target(&mut matching, dom(0), &BTreeMap::new());
        assert!(matching.into_option().is_none());
    }

    #[test]
    fn remap_file_drop_always_survives_and_clears_stale_targets() {
        let mut ctx = DragContext::file_drop(
            alloc::vec![AzString::from("/tmp/a")],
            LogicalPosition::zero(),
            0,
        );
        // No target: nothing to do, drag survives.
        assert!(ctx.remap_node_ids(dom(0), &BTreeMap::new()));
        assert!(ctx.as_file_drop().unwrap().drop_target.into_option().is_none());

        // Target present and mapped => remapped.
        ctx.as_file_drop_mut().unwrap().drop_target = Some(dnid(0, 5)).into();
        assert!(ctx.remap_node_ids(dom(0), &nid_map(&[(5, 55)])));
        assert_eq!(
            ctx.as_file_drop()
                .unwrap()
                .drop_target
                .into_option()
                .unwrap()
                .node
                .into_crate_internal(),
            Some(NodeId::new(55))
        );

        // Target removed => cleared, but the file drop itself still survives.
        assert!(ctx.remap_node_ids(dom(0), &BTreeMap::new()));
        assert!(ctx.as_file_drop().unwrap().drop_target.into_option().is_none());
        assert_eq!(ctx.as_file_drop().unwrap().files.len(), 1);
    }

    #[test]
    fn remap_window_drags_always_succeed() {
        let mut wm = DragContext::window_move(
            LogicalPosition::zero(),
            WindowPosition::Uninitialized,
            0,
        );
        assert!(wm.remap_node_ids(dom(0), &BTreeMap::new()));
        assert!(wm.remap_node_ids(dom(usize::MAX), &nid_map(&[(1, 2)])));

        let mut wr = resize_ctx();
        assert!(wr.remap_node_ids(dom(0), &BTreeMap::new()));
        assert_eq!(wr.current_position(), LogicalPosition::new(1.0, 2.0));
    }

    #[test]
    fn remap_with_a_hundred_thousand_entries_does_not_hang() {
        let mut m = BTreeMap::new();
        for i in 0..100_000usize {
            m.insert(NodeId::new(i), NodeId::new(i + 1));
        }
        let mut ctx = vscroll(0.0, 100.0, 200.0, 100.0); // node 1
        assert!(ctx.remap_node_ids(dom(0), &m));
        assert_eq!(
            ctx.as_scrollbar_thumb().unwrap().scroll_container_node,
            NodeId::new(2)
        );
    }

    #[test]
    fn remap_identity_map_is_a_fixed_point() {
        let mut ctx = DragContext::node_drag(
            dom(0),
            NodeId::new(1),
            LogicalPosition::new(1.0, 2.0),
            DragData::new(),
            0,
        );
        {
            let nd = ctx.as_node_drag_mut().unwrap();
            nd.current_drop_target = Some(dnid(0, 5)).into();
            nd.previous_drop_target = Some(dnid(0, 5)).into();
        }
        let before = ctx.clone();
        let m = nid_map(&[(1, 1), (5, 5)]);
        assert!(ctx.remap_node_ids(dom(0), &m));
        assert!(ctx.remap_node_ids(dom(0), &m)); // twice, for good measure
        assert_eq!(ctx, before);
    }

    // ==================================================== DragDelta

    #[test]
    fn drag_delta_new_stores_extremes_verbatim() {
        for (dx, dy) in [
            (0.0f32, 0.0f32),
            (f32::MAX, f32::MIN),
            (f32::INFINITY, f32::NEG_INFINITY),
            (f32::MIN_POSITIVE, -f32::MIN_POSITIVE),
        ] {
            let d = DragDelta::new(dx, dy);
            assert_eq!(d.dx.to_bits(), dx.to_bits());
            assert_eq!(d.dy.to_bits(), dy.to_bits());
        }
    }

    #[test]
    fn drag_delta_zero_is_the_neutral_default() {
        let z = DragDelta::zero();
        assert_eq!(z.dx, 0.0);
        assert_eq!(z.dy, 0.0);
        assert!(!z.dx.is_sign_negative(), "zero() must be +0.0, not -0.0");
        assert_eq!(z, DragDelta::default());
        assert_eq!(z, DragDelta::new(0.0, 0.0));
        // IEEE-754: -0.0 == +0.0, so a negative zero delta still equals zero().
        assert_eq!(DragDelta::new(-0.0, -0.0), z);
        // ...but the sign bit is preserved in the stored field.
        assert!(DragDelta::new(-0.0, -0.0).dx.is_sign_negative());
    }

    #[test]
    fn drag_delta_nan_is_not_equal_to_itself() {
        // Derived PartialEq on f32 => NaN != NaN. Callers must not use
        // equality to detect "no movement" on a NaN delta.
        let n = DragDelta::new(f32::NAN, f32::NAN);
        assert_ne!(n, DragDelta::new(f32::NAN, f32::NAN));
        assert_ne!(n, DragDelta::zero());
        assert!(n.dx.is_nan() && n.dy.is_nan());
        // PartialOrd is also useless on NaN: no ordering at all.
        assert!(n.partial_cmp(&DragDelta::zero()).is_none());
    }
}
