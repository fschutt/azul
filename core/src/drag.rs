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

use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;

use crate::dom::{DomId, DomNodeId, NodeId, OptionDomNodeId};
use crate::geom::LogicalPosition;
use crate::selection::TextCursor;
use crate::window::WindowPosition;

use azul_css::{AzString, OptionString, StringVec};

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
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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
    /// Whether the current drop target has accepted the drop via accept_drop()
    pub drop_accepted: bool,
    /// Drop effect set by the drop target
    pub drop_effect: DropEffect,
}

/// Window move drag state.
///
/// Tracks the window being moved via titlebar drag.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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
    /// No auto-scroll needed
    #[default]
    None,
    /// Scroll up (mouse near top edge)
    Up,
    /// Scroll down (mouse near bottom edge)
    Down,
    /// Scroll left (mouse near left edge)
    Left,
    /// Scroll right (mouse near right edge)
    Right,
    /// Scroll up-left (mouse near top-left corner)
    UpLeft,
    /// Scroll up-right (mouse near top-right corner)
    UpRight,
    /// Scroll down-left (mouse near bottom-left corner)
    DownLeft,
    /// Scroll down-right (mouse near bottom-right corner)
    DownRight,
}

/// Drop effect (what happens when dropped)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub enum DropEffect {
    /// No effect
    #[default]
    None,
    /// Copy the data
    Copy,
    /// Move the data
    Move,
    /// Create link
    Link,
}

/// Drag data (like HTML5 DataTransfer)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DragData {
    /// MIME type -> data mapping
    ///
    /// e.g., "text/plain" -> "Hello World"
    pub data: BTreeMap<AzString, Vec<u8>>,
    /// Allowed drag operations
    pub effect_allowed: DragEffect,
}

/// Drag/drop effect (like HTML5 dropEffect)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub enum DragEffect {
    /// No drop allowed
    #[default]
    None,
    /// Copy operation
    Copy,
    /// Move operation
    Move,
    /// Link/shortcut operation
    Link,
}

impl DragData {
    /// Create new empty drag data
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
            effect_allowed: DragEffect::Copy,
        }
    }

    /// Set data for a MIME type
    pub fn set_data(&mut self, mime_type: impl Into<AzString>, data: Vec<u8>) {
        self.data.insert(mime_type.into(), data);
    }

    /// Get data for a MIME type
    pub fn get_data(&self, mime_type: &str) -> Option<&[u8]> {
        self.data.get(&AzString::from(mime_type)).map(|v| v.as_slice())
    }

    /// Set plain text data
    pub fn set_text(&mut self, text: impl Into<AzString>) {
        let text_str = text.into();
        self.set_data("text/plain", text_str.as_str().as_bytes().to_vec());
    }

    /// Get plain text data
    pub fn get_text(&self) -> Option<AzString> {
        self.get_data("text/plain")
            .map(|bytes| AzString::from(core::str::from_utf8(bytes).unwrap_or("")))
    }
}

/// The unified drag context.
///
/// This struct wraps `ActiveDragType` and provides common metadata
/// that applies to all drag operations.
#[derive(Debug, Clone, PartialEq)]
pub struct DragContext {
    /// The specific type of drag operation
    pub drag_type: ActiveDragType,
    /// Session ID from gesture detection (links back to GestureManager)
    pub session_id: u64,
    /// Whether the drag has been cancelled (e.g., Escape pressed)
    pub cancelled: bool,
}

impl DragContext {
    /// Create a new drag context
    pub fn new(drag_type: ActiveDragType, session_id: u64) -> Self {
        Self {
            drag_type,
            session_id,
            cancelled: false,
        }
    }

    /// Create a text selection drag
    pub fn text_selection(
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
    pub fn scrollbar_thumb(
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
    pub fn node_drag(
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
    pub fn window_move(
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
    pub fn file_drop(files: Vec<AzString>, position: LogicalPosition, session_id: u64) -> Self {
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
    pub fn update_position(&mut self, position: LogicalPosition) {
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
    pub fn current_position(&self) -> LogicalPosition {
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
    pub fn start_position(&self) -> LogicalPosition {
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
    pub fn is_text_selection(&self) -> bool {
        matches!(self.drag_type, ActiveDragType::TextSelection(_))
    }

    /// Check if this is a scrollbar thumb drag
    pub fn is_scrollbar_thumb(&self) -> bool {
        matches!(self.drag_type, ActiveDragType::ScrollbarThumb(_))
    }

    /// Check if this is a node drag
    pub fn is_node_drag(&self) -> bool {
        matches!(self.drag_type, ActiveDragType::Node(_))
    }

    /// Check if this is a window move drag
    pub fn is_window_move(&self) -> bool {
        matches!(self.drag_type, ActiveDragType::WindowMove(_))
    }

    /// Check if this is a file drop
    pub fn is_file_drop(&self) -> bool {
        matches!(self.drag_type, ActiveDragType::FileDrop(_))
    }

    /// Get as text selection drag (if applicable)
    pub fn as_text_selection(&self) -> Option<&TextSelectionDrag> {
        match &self.drag_type {
            ActiveDragType::TextSelection(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as mutable text selection drag (if applicable)
    pub fn as_text_selection_mut(&mut self) -> Option<&mut TextSelectionDrag> {
        match &mut self.drag_type {
            ActiveDragType::TextSelection(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as scrollbar thumb drag (if applicable)
    pub fn as_scrollbar_thumb(&self) -> Option<&ScrollbarThumbDrag> {
        match &self.drag_type {
            ActiveDragType::ScrollbarThumb(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as mutable scrollbar thumb drag (if applicable)
    pub fn as_scrollbar_thumb_mut(&mut self) -> Option<&mut ScrollbarThumbDrag> {
        match &mut self.drag_type {
            ActiveDragType::ScrollbarThumb(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as node drag (if applicable)
    pub fn as_node_drag(&self) -> Option<&NodeDrag> {
        match &self.drag_type {
            ActiveDragType::Node(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as mutable node drag (if applicable)
    pub fn as_node_drag_mut(&mut self) -> Option<&mut NodeDrag> {
        match &mut self.drag_type {
            ActiveDragType::Node(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as window move drag (if applicable)
    pub fn as_window_move(&self) -> Option<&WindowMoveDrag> {
        match &self.drag_type {
            ActiveDragType::WindowMove(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as file drop (if applicable)
    pub fn as_file_drop(&self) -> Option<&FileDropDrag> {
        match &self.drag_type {
            ActiveDragType::FileDrop(drag) => Some(drag),
            _ => None,
        }
    }

    /// Get as mutable file drop (if applicable)
    pub fn as_file_drop_mut(&mut self) -> Option<&mut FileDropDrag> {
        match &mut self.drag_type {
            ActiveDragType::FileDrop(drag) => Some(drag),
            _ => None,
        }
    }

    /// Calculate scroll delta for scrollbar thumb drag
    ///
    /// Returns the new scroll offset based on current mouse position.
    pub fn calculate_scrollbar_scroll_offset(&self) -> Option<f32> {
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

    /// Remap NodeIds stored in this drag context after DOM reconciliation.
    ///
    /// When the DOM is regenerated during an active drag, NodeIds can change.
    /// This updates all stored NodeIds using the old→new mapping.
    /// Returns `false` if a critical NodeId was removed (drag should be cancelled).
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
                // Drop target remap
                if let Some(dt) = drag.current_drop_target.into_option() {
                    if dt.dom == dom_id {
                        if let Some(old_nid) = dt.node.into_crate_internal() {
                            if let Some(&new_nid) = node_id_map.get(&old_nid) {
                                drag.current_drop_target = Some(DomNodeId {
                                    dom: dom_id,
                                    node: crate::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(new_nid)),
                                }).into();
                            } else {
                                drag.current_drop_target = OptionDomNodeId::None;
                            }
                        }
                    }
                }
                true
            }
            // WindowMove, WindowResize, and FileDrop don't reference DOM NodeIds
            ActiveDragType::WindowMove(_) | ActiveDragType::WindowResize(_) => true,
            ActiveDragType::FileDrop(ref mut drag) => {
                if let Some(dt) = drag.drop_target.into_option() {
                    if dt.dom == dom_id {
                        if let Some(old_nid) = dt.node.into_crate_internal() {
                            if let Some(&new_nid) = node_id_map.get(&old_nid) {
                                drag.drop_target = Some(DomNodeId {
                                    dom: dom_id,
                                    node: crate::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(new_nid)),
                                }).into();
                            } else {
                                drag.drop_target = OptionDomNodeId::None;
                            }
                        }
                    }
                }
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
    #[inline(always)]
    pub const fn new(dx: f32, dy: f32) -> Self {
        Self { dx, dy }
    }
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
}

impl_option!(
    DragDelta,
    OptionDragDelta,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);
