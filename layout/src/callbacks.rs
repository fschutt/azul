//! Callback handling for layout events
//!
//! This module provides the CallbackInfo struct and related types for handling
//! UI callbacks. Callbacks need access to layout information (node sizes, positions,
//! hierarchy), which is why this module lives in azul-layout instead of azul-core.

// Re-export callback macro from azul-core
use alloc::{
    boxed::Box,
    collections::{btree_map::BTreeMap, VecDeque},
    sync::Arc,
    vec::Vec,
};

use azul_core::{
    animation::UpdateImageType,
    callbacks::{CoreCallback, FocusTarget, FocusTargetPath, HidpiAdjustedBounds, Update},
    dom::{DomId, DomIdVec, DomNodeId, IdOrClass, NodeId, NodeType},
    events::CallbackResultRef,
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
    gl::OptionGlContextPtr,
    gpu::GpuValueCache,
    hit_test::ScrollPosition,
    id::NodeId as CoreNodeId,
    impl_callback,
    menu::Menu,
    refany::{OptionRefAny, RefAny},
    resources::{ImageCache, ImageMask, ImageRef, RendererResources},
    selection::{Selection, SelectionRange, SelectionRangeVec, SelectionState, TextCursor},
    styled_dom::{NodeHierarchyItemId, NodeIdVec, StyledDom},
    task::{self, GetSystemTimeCallback, Instant, ThreadId, ThreadIdVec, TimerId, TimerIdVec},
    window::{KeyboardState, MouseState, RawWindowHandle, WindowFlags, WindowSize},
    FastBTreeSet, FastHashMap,
};
use azul_css::{
    css::CssPath,
    props::{
        basic::FontRef,
        property::{CssProperty, CssPropertyType, CssPropertyVec},
    },
    system::SystemStyle,
    AzString, StringVec,
};
use rust_fontconfig::FcFontCache;

use crate::{
    hit_test::FullHitTest,
    managers::{
        drag_drop::{DragDropManager, DragState, DragType},
        file_drop::FileDropManager,
        focus_cursor::FocusManager,
        gesture::{GestureAndDragManager, InputSample, PenState},
        gpu_state::GpuStateManager,
        hover::{HoverManager, InputPointId},
        iframe::IFrameManager,
        scroll_state::{AnimatedScrollState, ScrollManager},
        selection::{ClipboardContent, SelectionManager},
        text_input::{PendingTextEdit, TextInputManager},
        undo_redo::{UndoRedoManager, UndoableOperation},
    },
    text3::cache::{LayoutCache as TextLayoutCache, UnifiedLayout},
    thread::{CreateThreadCallback, Thread},
    timer::Timer,
    window::{DomLayoutResult, LayoutWindow},
    window_state::{FullWindowState, WindowCreateOptions},
};

use azul_css::{impl_option, impl_option_inner};

// ============================================================================
// FFI-safe wrapper types for tuple returns
// ============================================================================

/// FFI-safe wrapper for pen tilt angles (x_tilt, y_tilt) in degrees
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[repr(C)]
pub struct PenTilt {
    /// X-axis tilt angle in degrees (-90 to 90)
    pub x_tilt: f32,
    /// Y-axis tilt angle in degrees (-90 to 90)
    pub y_tilt: f32,
}

impl From<(f32, f32)> for PenTilt {
    fn from((x, y): (f32, f32)) -> Self {
        Self { x_tilt: x, y_tilt: y }
    }
}

impl_option!(PenTilt, OptionPenTilt, [Debug, Clone, Copy, PartialEq, PartialOrd]);

/// FFI-safe wrapper for select-all result (full_text, selected_range)
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct SelectAllResult {
    /// The full text content of the node
    pub full_text: AzString,
    /// The range that would be selected
    pub selection_range: SelectionRange,
}

impl From<(alloc::string::String, SelectionRange)> for SelectAllResult {
    fn from((text, range): (alloc::string::String, SelectionRange)) -> Self {
        Self {
            full_text: text.into(),
            selection_range: range,
        }
    }
}

impl_option!(SelectAllResult, OptionSelectAllResult, copy = false, [Debug, Clone, PartialEq]);

/// FFI-safe wrapper for delete inspection result (range_to_delete, deleted_text)
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct DeleteResult {
    /// The range that would be deleted
    pub range_to_delete: SelectionRange,
    /// The text that would be deleted
    pub deleted_text: AzString,
}

impl From<(SelectionRange, alloc::string::String)> for DeleteResult {
    fn from((range, text): (SelectionRange, alloc::string::String)) -> Self {
        Self {
            range_to_delete: range,
            deleted_text: text.into(),
        }
    }
}

impl_option!(DeleteResult, OptionDeleteResult, copy = false, [Debug, Clone, PartialEq]);

/// Represents a change made by a callback that will be applied after the callback returns
///
/// This transaction-based system provides:
/// - Clear separation between read-only queries and modifications
/// - Atomic application of all changes
/// - Easy debugging and logging of callback actions
/// - Future extensibility for new change types
#[derive(Debug, Clone)]
pub enum CallbackChange {
    // Window State Changes
    /// Modify the window state (size, position, title, etc.)
    ModifyWindowState { state: FullWindowState },
    /// Create a new window
    CreateNewWindow { options: WindowCreateOptions },
    /// Close the current window (via Update::CloseWindow return value, tracked here for logging)
    CloseWindow,

    // Focus Management
    /// Change keyboard focus to a specific node or clear focus
    SetFocusTarget { target: FocusTarget },

    // Event Propagation Control
    /// Stop event from propagating to parent nodes
    StopPropagation,
    /// Prevent default browser behavior (e.g., block text input from being applied)
    PreventDefault,

    // Timer Management
    /// Add a new timer to the window
    AddTimer { timer_id: TimerId, timer: Timer },
    /// Remove an existing timer
    RemoveTimer { timer_id: TimerId },

    // Thread Management
    /// Add a new background thread
    AddThread { thread_id: ThreadId, thread: Thread },
    /// Remove an existing thread
    RemoveThread { thread_id: ThreadId },

    // Content Modifications
    /// Change the text content of a node
    ChangeNodeText { node_id: DomNodeId, text: AzString },
    /// Change the image of a node
    ChangeNodeImage {
        dom_id: DomId,
        node_id: NodeId,
        image: ImageRef,
        update_type: UpdateImageType,
    },
    /// Re-render an image callback (for resize/animation)
    /// This triggers re-invocation of the RenderImageCallback
    UpdateImageCallback { dom_id: DomId, node_id: NodeId },
    /// Trigger re-rendering of an IFrame with a new DOM
    /// This forces the IFrame to call its callback and update the display list
    UpdateIFrame { dom_id: DomId, node_id: NodeId },
    /// Change the image mask of a node
    ChangeNodeImageMask {
        dom_id: DomId,
        node_id: NodeId,
        mask: ImageMask,
    },
    /// Change CSS properties of a node
    ChangeNodeCssProperties {
        dom_id: DomId,
        node_id: NodeId,
        properties: CssPropertyVec,
    },

    // Scroll Management
    /// Scroll a node to a specific position
    ScrollTo {
        dom_id: DomId,
        node_id: NodeHierarchyItemId,
        position: LogicalPosition,
    },

    // Image Cache Management
    /// Add an image to the image cache
    AddImageToCache { id: AzString, image: ImageRef },
    /// Remove an image from the image cache
    RemoveImageFromCache { id: AzString },

    // Font Cache Management
    /// Reload system fonts (expensive operation)
    ReloadSystemFonts,

    // Menu Management
    /// Open a context menu or dropdown menu
    /// Whether it's native or fallback depends on window.state.flags.use_native_context_menus
    OpenMenu {
        menu: Menu,
        /// Optional position override (if None, uses menu.position)
        position: Option<LogicalPosition>,
    },

    // Tooltip Management
    /// Show a tooltip at a specific position
    ///
    /// Platform-specific implementation:
    /// - Windows: Uses native tooltip window (TOOLTIPS_CLASS)
    /// - macOS: Uses NSPopover or custom NSWindow with tooltip styling
    /// - X11: Creates transient window with _NET_WM_WINDOW_TYPE_TOOLTIP
    /// - Wayland: Creates surface with zwlr_layer_shell_v1 (overlay layer)
    ShowTooltip {
        text: AzString,
        position: LogicalPosition,
    },
    /// Hide the currently displayed tooltip
    HideTooltip,

    // Text Editing
    /// Insert text at the current cursor position or replace selection
    InsertText {
        dom_id: DomId,
        node_id: NodeId,
        text: AzString,
    },
    /// Delete text backward (backspace) at cursor
    DeleteBackward { dom_id: DomId, node_id: NodeId },
    /// Delete text forward (delete key) at cursor
    DeleteForward { dom_id: DomId, node_id: NodeId },
    /// Move cursor to a specific position
    MoveCursor {
        dom_id: DomId,
        node_id: NodeId,
        cursor: TextCursor,
    },
    /// Set text selection range
    SetSelection {
        dom_id: DomId,
        node_id: NodeId,
        selection: Selection,
    },
    /// Set/override the text changeset for the current text input operation
    /// This allows callbacks to modify what text will be inserted during text input events
    SetTextChangeset { changeset: PendingTextEdit },

    // Cursor Movement Operations
    /// Move cursor left (arrow left)
    MoveCursorLeft {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor right (arrow right)
    MoveCursorRight {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor up (arrow up)
    MoveCursorUp {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor down (arrow down)
    MoveCursorDown {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor to line start (Home key)
    MoveCursorToLineStart {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor to line end (End key)
    MoveCursorToLineEnd {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor to document start (Ctrl+Home)
    MoveCursorToDocumentStart {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor to document end (Ctrl+End)
    MoveCursorToDocumentEnd {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },

    // Clipboard Operations (Override)
    /// Override clipboard content for copy operation
    SetCopyContent {
        target: DomNodeId,
        content: ClipboardContent,
    },
    /// Override clipboard content for cut operation
    SetCutContent {
        target: DomNodeId,
        content: ClipboardContent,
    },
    /// Override selection range for select-all operation
    SetSelectAllRange {
        target: DomNodeId,
        range: SelectionRange,
    },
}

/// Main callback type for UI event handling
pub type CallbackType = extern "C" fn(RefAny, CallbackInfo) -> Update;

/// Stores a function pointer that is executed when the given UI element is hit
///
/// Must return an `Update` that denotes if the screen should be redrawn.
#[repr(C)]
pub struct Callback {
    pub cb: CallbackType,
    /// For FFI: stores the foreign callable (e.g., PyFunction)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

impl_callback!(Callback, CallbackType);

impl Callback {
    /// Create a new callback with just a function pointer (for native Rust code)
    pub fn create<C: Into<Callback>>(cb: C) -> Self {
        cb.into()
    }

    /// Convert from CoreCallback (stored as usize) to Callback (actual function pointer)
    ///
    /// # Safety
    /// The caller must ensure that the usize in CoreCallback.cb was originally a valid
    /// function pointer of type `CallbackType`. This is guaranteed when CoreCallback
    /// is created through standard APIs, but unsafe code could violate this.
    pub fn from_core(core: CoreCallback) -> Self {
        Self {
            cb: unsafe { core::mem::transmute(core.cb) },
            ctx: OptionRefAny::None,
        }
    }

    /// Convert to CoreCallback (function pointer stored as usize)
    ///
    /// This is always safe - we're just casting the function pointer to usize for storage.
    pub fn to_core(self) -> CoreCallback {
        CoreCallback {
            cb: self.cb as usize,
            ctx: self.ctx,
        }
    }
}

/// Allow Callback to be passed to functions expecting `C: Into<CoreCallback>`
impl From<Callback> for CoreCallback {
    fn from(callback: Callback) -> Self {
        callback.to_core()
    }
}

/// Convert a raw function pointer to CoreCallback
/// 
/// This is a helper function that wraps the function pointer cast.
/// Cannot use From trait due to orphan rules (extern "C" fn is not a local type).
#[inline]
pub fn callback_type_to_core(cb: CallbackType) -> CoreCallback {
    CoreCallback {
        cb: cb as usize,
        ctx: OptionRefAny::None,
    }
}

impl Callback {
    /// Safely invoke the callback with the given data and info
    ///
    /// This is a safe wrapper around calling the function pointer directly.
    pub fn invoke(&self, data: RefAny, info: CallbackInfo) -> Update {
        (self.cb)(data, info)
    }
}

/// Safe conversion from CoreCallback to function pointer
///
/// This provides a type-safe way to convert CoreCallback.cb (usize) to the actual
/// function pointer type without using transmute directly in application code.
///
/// # Safety
/// The caller must ensure the usize was originally a valid CallbackType function pointer.
pub unsafe fn core_callback_to_fn(core: CoreCallback) -> CallbackType {
    core::mem::transmute(core.cb)
}

/// FFI-safe Option<Callback> type for C interop.
///
/// This enum provides an ABI-stable alternative to `Option<Callback>`
/// that can be safely passed across FFI boundaries.
#[derive(Debug, Eq, Clone, PartialEq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum OptionCallback {
    /// No callback is present.
    None,
    /// A callback is present.
    Some(Callback),
}

impl OptionCallback {
    /// Converts this FFI-safe option into a standard Rust `Option<Callback>`.
    pub fn into_option(self) -> Option<Callback> {
        match self {
            OptionCallback::None => None,
            OptionCallback::Some(c) => Some(c),
        }
    }

    /// Returns `true` if a callback is present.
    pub fn is_some(&self) -> bool {
        matches!(self, OptionCallback::Some(_))
    }

    /// Returns `true` if no callback is present.
    pub fn is_none(&self) -> bool {
        matches!(self, OptionCallback::None)
    }
}

impl From<Option<Callback>> for OptionCallback {
    fn from(o: Option<Callback>) -> Self {
        match o {
            None => OptionCallback::None,
            Some(c) => OptionCallback::Some(c),
        }
    }
}

impl From<OptionCallback> for Option<Callback> {
    fn from(o: OptionCallback) -> Self {
        o.into_option()
    }
}

/// Information about the callback that is passed to the callback whenever a callback is invoked
///
/// # Architecture
///
/// CallbackInfo uses a transaction-based system:
/// - **Read-only pointers**: Access to layout data, window state, managers for queries
/// - **Change vector**: All modifications are recorded as CallbackChange items
/// - **Processing**: Changes are applied atomically after callback returns
///
/// This design provides clear separation between queries and modifications, makes debugging
/// easier, and allows for future extensibility.

/// Reference data container for CallbackInfo (all read-only fields)
///
/// This struct consolidates all readonly references that callbacks need to query window state.
/// By grouping these into a single struct, we reduce the number of parameters to
/// CallbackInfo::new() from 13 to 3, making the API more maintainable and easier to extend.
///
/// This is pure syntax sugar - the struct lives on the stack in the caller and is passed by
/// reference.
pub struct CallbackInfoRefData<'a> {
    /// Pointer to the LayoutWindow containing all layout results (READ-ONLY for queries)
    pub layout_window: &'a LayoutWindow,
    /// Necessary to query FontRefs from callbacks
    pub renderer_resources: &'a RendererResources,
    /// Previous window state (for detecting changes)
    pub previous_window_state: &'a Option<FullWindowState>,
    /// State of the current window that the callback was called on (read only!)
    pub current_window_state: &'a FullWindowState,
    /// An Rc to the OpenGL context, in order to be able to render to OpenGL textures
    pub gl_context: &'a OptionGlContextPtr,
    /// Immutable reference to where the nodes are currently scrolled (current position)
    pub current_scroll_manager: &'a BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>>,
    /// Handle of the current window
    pub current_window_handle: &'a RawWindowHandle,
    /// Callbacks for creating threads and getting the system time (since this crate uses no_std)
    pub system_callbacks: &'a ExternalSystemCallbacks,
    /// Platform-specific system style (colors, spacing, etc.)
    /// Arc allows safe cloning in callbacks without unsafe pointer manipulation
    pub system_style: Arc<SystemStyle>,
    /// The callable for FFI language bindings (Python, etc.)
    /// Cloned from the Callback struct before invocation. Native Rust callbacks have this as None.
    pub ctx: OptionRefAny,
}

/// CallbackInfo is a lightweight wrapper around pointers to stack-local data.
/// It can be safely copied because it only contains pointers - the underlying
/// data lives on the stack and outlives the callback invocation.
/// This allows callbacks to "consume" CallbackInfo by value while the caller
/// retains access to the same underlying data.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CallbackInfo {
    // Read-only Data (Query Access)
    /// Single reference to all readonly reference data
    /// This consolidates 8 individual parameters into 1, improving API ergonomics
    ref_data: *const CallbackInfoRefData<'static>,
    // Context Info (Immutable Event Data)
    /// The ID of the DOM + the node that was hit
    hit_dom_node: DomNodeId,
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was
    /// hit**
    cursor_relative_to_item: OptionLogicalPosition,
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**
    cursor_in_viewport: OptionLogicalPosition,
    // Transaction Container (New System)
    /// All changes made by the callback, applied atomically after callback returns
    changes: *mut Vec<CallbackChange>,
}

impl CallbackInfo {
    pub fn new<'a>(
        ref_data: &'a CallbackInfoRefData<'a>,
        changes: &'a mut Vec<CallbackChange>,
        hit_dom_node: DomNodeId,
        cursor_relative_to_item: OptionLogicalPosition,
        cursor_in_viewport: OptionLogicalPosition,
    ) -> Self {
        Self {
            // Read-only data (single reference to consolidated refs)
            // SAFETY: We cast away the lifetime 'a to 'static because CallbackInfo
            // only lives for the duration of the callback, which is shorter than 'a
            ref_data: unsafe { core::mem::transmute(ref_data) },

            // Context info (immutable event data)
            hit_dom_node,
            cursor_relative_to_item,
            cursor_in_viewport,

            // Transaction container (new system)
            changes: changes as *mut Vec<CallbackChange>,
        }
    }

    /// Get the callable for FFI language bindings (Python, etc.)
    ///
    /// Returns the cloned OptionRefAny if a callable was set, or None if this
    /// is a native Rust callback.
    pub fn get_ctx(&self) -> OptionRefAny {
        unsafe { (*self.ref_data).ctx.clone() }
    }

    /// Returns the OpenGL context if available
    pub fn get_gl_context(&self) -> OptionGlContextPtr {
        unsafe { (*self.ref_data).gl_context.clone() }
    }

    // Helper methods for transaction system

    /// Push a change to be applied after the callback returns
    /// This is the primary method for modifying window state from callbacks
    pub fn push_change(&mut self, change: CallbackChange) {
        unsafe { (*self.changes).push(change) }
    }

    // Modern Api (using CallbackChange transactions)

    /// Add a timer to this window (applied after callback returns)
    pub fn add_timer(&mut self, timer_id: TimerId, timer: Timer) {
        self.push_change(CallbackChange::AddTimer { timer_id, timer });
    }

    /// Remove a timer from this window (applied after callback returns)
    pub fn remove_timer(&mut self, timer_id: TimerId) {
        self.push_change(CallbackChange::RemoveTimer { timer_id });
    }

    /// Add a thread to this window (applied after callback returns)
    pub fn add_thread(&mut self, thread_id: ThreadId, thread: Thread) {
        self.push_change(CallbackChange::AddThread { thread_id, thread });
    }

    /// Remove a thread from this window (applied after callback returns)
    pub fn remove_thread(&mut self, thread_id: ThreadId) {
        self.push_change(CallbackChange::RemoveThread { thread_id });
    }

    /// Stop event propagation (applied after callback returns)
    pub fn stop_propagation(&mut self) {
        self.push_change(CallbackChange::StopPropagation);
    }

    /// Set keyboard focus target (applied after callback returns)
    pub fn set_focus(&mut self, target: FocusTarget) {
        self.push_change(CallbackChange::SetFocusTarget { target });
    }

    /// Create a new window (applied after callback returns)
    pub fn create_window(&mut self, options: WindowCreateOptions) {
        self.push_change(CallbackChange::CreateNewWindow { options });
    }

    /// Close the current window (applied after callback returns)
    pub fn close_window(&mut self) {
        self.push_change(CallbackChange::CloseWindow);
    }

    /// Modify the window state (applied after callback returns)
    pub fn modify_window_state(&mut self, state: FullWindowState) {
        self.push_change(CallbackChange::ModifyWindowState { state });
    }

    /// Change the text content of a node (applied after callback returns)
    ///
    /// This method was previously called `set_string_contents` in older API versions.
    ///
    /// # Arguments
    /// * `node_id` - The text node to modify (DomNodeId containing both DOM and node IDs)
    /// * `text` - The new text content
    pub fn change_node_text(&mut self, node_id: DomNodeId, text: AzString) {
        self.push_change(CallbackChange::ChangeNodeText { node_id, text });
    }

    /// Change the image of a node (applied after callback returns)
    pub fn change_node_image(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        image: ImageRef,
        update_type: UpdateImageType,
    ) {
        self.push_change(CallbackChange::ChangeNodeImage {
            dom_id,
            node_id,
            image,
            update_type,
        });
    }

    /// Re-render an image callback (for resize/animation updates)
    ///
    /// This triggers re-invocation of the RenderImageCallback associated with the node.
    /// Useful for:
    /// - Responding to window resize (image needs to match new size)
    /// - Animation frames (update OpenGL texture each frame)
    /// - Interactive content (user input changes rendering)
    pub fn update_image_callback(&mut self, dom_id: DomId, node_id: NodeId) {
        self.push_change(CallbackChange::UpdateImageCallback { dom_id, node_id });
    }

    /// Trigger re-rendering of an IFrame (applied after callback returns)
    ///
    /// This forces the IFrame to call its layout callback with reason `DomRecreated`
    /// and submit a new display list to WebRender. The IFrame's pipeline will be updated
    /// without affecting other parts of the window.
    ///
    /// Useful for:
    /// - Live preview panes (update when source code changes)
    /// - Dynamic content that needs manual refresh
    /// - Editor previews (re-parse and display new DOM)
    pub fn trigger_iframe_rerender(&mut self, dom_id: DomId, node_id: NodeId) {
        self.push_change(CallbackChange::UpdateIFrame { dom_id, node_id });
    }

    // Dom Tree Navigation

    /// Find a node by ID attribute in the layout tree
    ///
    /// Returns the NodeId of the first node with the given ID attribute, or None if not found.
    pub fn get_node_id_by_id_attribute(&self, dom_id: DomId, id: &str) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let styled_dom = &layout_result.styled_dom;

        // Search through all nodes to find one with matching ID attribute
        for (node_idx, node_data) in styled_dom.node_data.as_ref().iter().enumerate() {
            for id_or_class in node_data.ids_and_classes.as_ref() {
                if let IdOrClass::Id(node_id_str) = id_or_class {
                    if node_id_str.as_str() == id {
                        return Some(NodeId::new(node_idx));
                    }
                }
            }
        }

        None
    }

    /// Get the parent node of the given node
    ///
    /// Returns None if the node has no parent (i.e., it's the root node)
    pub fn get_parent_node(&self, dom_id: DomId, node_id: NodeId) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
        let node = node_hierarchy.as_ref().get(node_id.index())?;
        node.parent_id()
    }

    /// Get the next sibling of the given node
    ///
    /// Returns None if the node has no next sibling
    pub fn get_next_sibling_node(&self, dom_id: DomId, node_id: NodeId) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
        let node = node_hierarchy.as_ref().get(node_id.index())?;
        node.next_sibling_id()
    }

    /// Get the previous sibling of the given node
    ///
    /// Returns None if the node has no previous sibling
    pub fn get_previous_sibling_node(&self, dom_id: DomId, node_id: NodeId) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
        let node = node_hierarchy.as_ref().get(node_id.index())?;
        node.previous_sibling_id()
    }

    /// Get the first child of the given node
    ///
    /// Returns None if the node has no children
    pub fn get_first_child_node(&self, dom_id: DomId, node_id: NodeId) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
        let node = node_hierarchy.as_ref().get(node_id.index())?;
        node.first_child_id(node_id)
    }

    /// Get the last child of the given node
    ///
    /// Returns None if the node has no children
    pub fn get_last_child_node(&self, dom_id: DomId, node_id: NodeId) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
        let node = node_hierarchy.as_ref().get(node_id.index())?;
        node.last_child_id()
    }

    /// Get all direct children of the given node
    ///
    /// Returns an empty vector if the node has no children.
    /// Uses the contiguous node layout for efficient iteration.
    pub fn get_all_children_nodes(&self, dom_id: DomId, node_id: NodeId) -> NodeIdVec {
        let layout_window = self.get_layout_window();
        let layout_result = match layout_window.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return NodeIdVec::from_const_slice(&[]),
        };
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = match node_hierarchy.get(node_id) {
            Some(h) => h,
            None => return NodeIdVec::from_const_slice(&[]),
        };
        
        // Get first child - if none, return empty
        let first_child = match hier_item.first_child_id(node_id) {
            Some(fc) => fc,
            None => return NodeIdVec::from_const_slice(&[]),
        };
        
        // Collect children by walking the sibling chain
        let mut children: Vec<NodeHierarchyItemId> = Vec::new();
        children.push(NodeHierarchyItemId::from_crate_internal(Some(first_child)));
        
        let mut current = first_child;
        while let Some(next_sibling) = node_hierarchy.get(current).and_then(|h| h.next_sibling_id()) {
            children.push(NodeHierarchyItemId::from_crate_internal(Some(next_sibling)));
            current = next_sibling;
        }
        
        NodeIdVec::from(children)
    }

    /// Get the number of direct children of the given node
    /// 
    /// Uses the contiguous node layout for efficient counting.
    pub fn get_children_count(&self, dom_id: DomId, node_id: NodeId) -> usize {
        let layout_window = self.get_layout_window();
        let layout_result = match layout_window.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return 0,
        };
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = match node_hierarchy.get(node_id) {
            Some(h) => h,
            None => return 0,
        };
        
        // Get first child - if none, return 0
        let first_child = match hier_item.first_child_id(node_id) {
            Some(fc) => fc,
            None => return 0,
        };
        
        // Count children by walking the sibling chain
        let mut count = 1;
        let mut current = first_child;
        while let Some(next_sibling) = node_hierarchy.get(current).and_then(|h| h.next_sibling_id()) {
            count += 1;
            current = next_sibling;
        }
        
        count
    }

    /// Change the image mask of a node (applied after callback returns)
    pub fn change_node_image_mask(&mut self, dom_id: DomId, node_id: NodeId, mask: ImageMask) {
        self.push_change(CallbackChange::ChangeNodeImageMask {
            dom_id,
            node_id,
            mask,
        });
    }

    /// Change CSS properties of a node (applied after callback returns)
    pub fn change_node_css_properties(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        properties: CssPropertyVec,
    ) {
        self.push_change(CallbackChange::ChangeNodeCssProperties {
            dom_id,
            node_id,
            properties,
        });
    }

    /// Set a single CSS property on a node (convenience method for widgets)
    ///
    /// This is a helper method that wraps `change_node_css_properties` for the common case
    /// of setting a single property. It uses the hit node's DOM ID automatically.
    ///
    /// # Arguments
    /// * `node_id` - The node to set the property on (uses hit node's DOM ID)
    /// * `property` - The CSS property to set
    pub fn set_css_property(&mut self, node_id: DomNodeId, property: CssProperty) {
        let dom_id = node_id.dom;
        let internal_node_id = node_id.node.into_crate_internal()
            .expect("DomNodeId node should not be None");
        self.change_node_css_properties(dom_id, internal_node_id, vec![property].into());
    }

    /// Scroll a node to a specific position (applied after callback returns)
    pub fn scroll_to(
        &mut self,
        dom_id: DomId,
        node_id: NodeHierarchyItemId,
        position: LogicalPosition,
    ) {
        self.push_change(CallbackChange::ScrollTo {
            dom_id,
            node_id,
            position,
        });
    }

    /// Add an image to the image cache (applied after callback returns)
    pub fn add_image_to_cache(&mut self, id: AzString, image: ImageRef) {
        self.push_change(CallbackChange::AddImageToCache { id, image });
    }

    /// Remove an image from the image cache (applied after callback returns)
    pub fn remove_image_from_cache(&mut self, id: AzString) {
        self.push_change(CallbackChange::RemoveImageFromCache { id });
    }

    /// Reload system fonts (applied after callback returns)
    ///
    /// Note: This is an expensive operation that rebuilds the entire font cache
    pub fn reload_system_fonts(&mut self) {
        self.push_change(CallbackChange::ReloadSystemFonts);
    }

    // Text Input / Changeset Api

    /// Get the current text changeset being processed (if any)
    ///
    /// This allows callbacks to inspect what text input is about to be applied.
    /// Returns None if no text input is currently being processed.
    ///
    /// Use `set_text_changeset()` to modify the text that will be inserted,
    /// and `prevent_default()` to block the text input entirely.
    pub fn get_text_changeset(&self) -> Option<&PendingTextEdit> {
        self.get_layout_window()
            .text_input_manager
            .get_pending_changeset()
    }

    /// Set/override the text changeset for the current text input operation
    ///
    /// This allows you to modify what text will be inserted during text input events.
    /// Typically used in combination with `prevent_default()` to transform user input.
    ///
    /// # Arguments
    /// * `changeset` - The modified text changeset to apply
    pub fn set_text_changeset(&mut self, changeset: PendingTextEdit) {
        self.push_change(CallbackChange::SetTextChangeset { changeset });
    }

    /// Prevent the default text input from being applied
    ///
    /// When called in a TextInput callback, prevents the typed text from being inserted.
    /// Useful for custom validation, filtering, or text transformation.
    pub fn prevent_default(&mut self) {
        self.push_change(CallbackChange::PreventDefault);
    }

    /// Open a menu (context menu or dropdown)
    ///
    /// The menu will be displayed either as a native menu or a fallback DOM-based menu
    /// depending on the window's `use_native_context_menus` flag.
    /// Uses the position specified in the menu itself.
    ///
    /// # Arguments
    /// * `menu` - The menu to display
    pub fn open_menu(&mut self, menu: Menu) {
        self.push_change(CallbackChange::OpenMenu {
            menu,
            position: None,
        });
    }

    /// Open a menu at a specific position
    ///
    /// # Arguments
    /// * `menu` - The menu to display
    /// * `position` - The position where the menu should appear (overrides menu's position)
    pub fn open_menu_at(&mut self, menu: Menu, position: LogicalPosition) {
        self.push_change(CallbackChange::OpenMenu {
            menu,
            position: Some(position),
        });
    }

    // Tooltip Api

    /// Show a tooltip at the current cursor position
    ///
    /// Displays a simple text tooltip near the mouse cursor.
    /// The tooltip will be shown using platform-specific native APIs where available.
    ///
    /// Platform implementations:
    /// - **Windows**: Uses `TOOLTIPS_CLASS` Win32 control
    /// - **macOS**: Uses `NSPopover` or custom `NSWindow` with tooltip styling
    /// - **X11**: Creates transient window with `_NET_WM_WINDOW_TYPE_TOOLTIP`
    /// - **Wayland**: Uses `zwlr_layer_shell_v1` with overlay layer
    ///
    /// # Arguments
    /// * `text` - The tooltip text to display
    pub fn show_tooltip(&mut self, text: AzString) {
        let position = self
            .get_cursor_relative_to_viewport()
            .into_option()
            .unwrap_or_else(LogicalPosition::zero);
        self.push_change(CallbackChange::ShowTooltip { text, position });
    }

    /// Show a tooltip at a specific position
    ///
    /// # Arguments
    /// * `text` - The tooltip text to display
    /// * `position` - The position where the tooltip should appear (in window coordinates)
    pub fn show_tooltip_at(&mut self, text: AzString, position: LogicalPosition) {
        self.push_change(CallbackChange::ShowTooltip { text, position });
    }

    /// Hide the currently displayed tooltip
    pub fn hide_tooltip(&mut self) {
        self.push_change(CallbackChange::HideTooltip);
    }

    // Text Editing Api (transactional)

    /// Insert text at the current cursor position in a text node
    ///
    /// This operation is transactional - the text will be inserted after the callback returns.
    /// If there's a selection, it will be replaced with the inserted text.
    ///
    /// # Arguments
    /// * `dom_id` - The DOM containing the text node
    /// * `node_id` - The node to insert text into
    /// * `text` - The text to insert
    pub fn insert_text(&mut self, dom_id: DomId, node_id: NodeId, text: AzString) {
        self.push_change(CallbackChange::InsertText {
            dom_id,
            node_id,
            text,
        });
    }

    /// Move the text cursor to a specific position
    ///
    /// # Arguments
    /// * `dom_id` - The DOM containing the text node
    /// * `node_id` - The node containing the cursor
    /// * `cursor` - The new cursor position
    pub fn move_cursor(&mut self, dom_id: DomId, node_id: NodeId, cursor: TextCursor) {
        self.push_change(CallbackChange::MoveCursor {
            dom_id,
            node_id,
            cursor,
        });
    }

    /// Set the text selection range
    ///
    /// # Arguments
    /// * `dom_id` - The DOM containing the text node
    /// * `node_id` - The node containing the selection
    /// * `selection` - The new selection (can be a cursor or range)
    pub fn set_selection(&mut self, dom_id: DomId, node_id: NodeId, selection: Selection) {
        self.push_change(CallbackChange::SetSelection {
            dom_id,
            node_id,
            selection,
        });
    }

    /// Open a menu positioned relative to a specific DOM node
    ///
    /// This is useful for dropdowns, combo boxes, and context menus that should appear
    /// near a specific UI element. The menu will be positioned below the node by default.
    ///
    /// # Arguments
    /// * `menu` - The menu to display
    /// * `node_id` - The DOM node to position the menu relative to
    ///
    /// # Returns
    /// * `true` if the menu was queued for opening
    /// * `false` if the node doesn't exist or has no layout information
    pub fn open_menu_for_node(&mut self, menu: Menu, node_id: DomNodeId) -> bool {
        // Get the node's bounding rectangle
        if let Some(rect) = self.get_node_rect(node_id) {
            // Position menu at bottom-left of the node
            let position = LogicalPosition::new(rect.origin.x, rect.origin.y + rect.size.height);
            self.push_change(CallbackChange::OpenMenu {
                menu,
                position: Some(position),
            });
            true
        } else {
            false
        }
    }

    /// Open a menu positioned relative to the currently hit node
    ///
    /// Convenience method for opening a menu at the element that triggered the callback.
    /// Equivalent to `open_menu_for_node(menu, info.get_hit_node())`.
    ///
    /// # Arguments
    /// * `menu` - The menu to display
    ///
    /// # Returns
    /// * `true` if the menu was queued for opening
    /// * `false` if no node is currently hit or it has no layout information
    pub fn open_menu_for_hit_node(&mut self, menu: Menu) -> bool {
        let hit_node = self.get_hit_node();
        self.open_menu_for_node(menu, hit_node)
    }

    // Internal accessors

    /// Get reference to the underlying LayoutWindow for queries
    ///
    /// This provides read-only access to layout data, node hierarchies, managers, etc.
    /// All modifications should go through CallbackChange transactions via push_change().
    pub fn get_layout_window(&self) -> &LayoutWindow {
        unsafe { (*self.ref_data).layout_window }
    }

    /// Internal helper: Get the inline text layout for a given node
    ///
    /// This efficiently looks up the text layout by following the chain:
    /// LayoutWindow → layout_results → LayoutTree → dom_to_layout → LayoutNode →
    /// inline_layout_result
    ///
    /// Returns None if:
    /// - The DOM doesn't exist in layout_results
    /// - The node doesn't have a layout node mapping
    /// - The layout node doesn't have inline text layout
    fn get_inline_layout_for_node(&self, node_id: &DomNodeId) -> Option<&Arc<UnifiedLayout>> {
        let layout_window = self.get_layout_window();

        // Get the layout result for this DOM
        let layout_result = layout_window.layout_results.get(&node_id.dom)?;

        // Convert NodeHierarchyItemId to NodeId
        let dom_node_id = node_id.node.into_crate_internal()?;

        // Look up the layout node index(es) for this DOM node
        let layout_indices = layout_result.layout_tree.dom_to_layout.get(&dom_node_id)?;

        // Get the first layout node (a DOM node can generate multiple layout nodes,
        // but for text we typically only care about the first one)
        let layout_index = *layout_indices.first()?;

        // Get the layout node and its inline layout result
        let layout_node = layout_result.layout_tree.nodes.get(layout_index)?;
        layout_node
            .inline_layout_result
            .as_ref()
            .map(|c| c.get_layout())
    }

    // Public query Api
    // All methods below delegate to LayoutWindow for read-only access
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        self.get_layout_window().get_node_size(node_id)
    }

    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<LogicalPosition> {
        self.get_layout_window().get_node_position(node_id)
    }

    /// Get the bounding rectangle of a node (position + size)
    ///
    /// This is particularly useful for menu positioning, where you need
    /// to know where a UI element is to popup a menu relative to it.
    pub fn get_node_rect(&self, node_id: DomNodeId) -> Option<LogicalRect> {
        let position = self.get_node_position(node_id)?;
        let size = self.get_node_size(node_id)?;
        Some(LogicalRect::new(position, size))
    }

    /// Get the bounding rectangle of the hit node
    ///
    /// Convenience method that combines get_hit_node() and get_node_rect().
    /// Useful for menu positioning based on the clicked element.
    pub fn get_hit_node_rect(&self) -> Option<LogicalRect> {
        let hit_node = self.get_hit_node();
        self.get_node_rect(hit_node)
    }

    // Timer Management (Query APIs)

    /// Get a reference to a timer
    pub fn get_timer(&self, timer_id: &TimerId) -> Option<&Timer> {
        self.get_layout_window().get_timer(timer_id)
    }

    /// Get all timer IDs
    pub fn get_timer_ids(&self) -> TimerIdVec {
        self.get_layout_window().get_timer_ids()
    }

    // Thread Management (Query APIs)

    /// Get a reference to a thread
    pub fn get_thread(&self, thread_id: &ThreadId) -> Option<&Thread> {
        self.get_layout_window().get_thread(thread_id)
    }

    /// Get all thread IDs
    pub fn get_thread_ids(&self) -> ThreadIdVec {
        self.get_layout_window().get_thread_ids()
    }

    // Gpu Value Cache Management (Query APIs)

    /// Get the GPU value cache for a specific DOM
    pub fn get_gpu_cache(&self, dom_id: &DomId) -> Option<&GpuValueCache> {
        self.get_layout_window().get_gpu_cache(dom_id)
    }

    // Layout Result Access (Query APIs)

    /// Get a layout result for a specific DOM
    pub fn get_layout_result(&self, dom_id: &DomId) -> Option<&DomLayoutResult> {
        self.get_layout_window().get_layout_result(dom_id)
    }

    /// Get all DOM IDs that have layout results
    pub fn get_dom_ids(&self) -> DomIdVec {
        self.get_layout_window().get_dom_ids()
    }

    // Node Hierarchy Navigation

    pub fn get_hit_node(&self) -> DomNodeId {
        self.hit_dom_node
    }

    /// Check if a node is anonymous (generated for table layout)
    fn is_node_anonymous(&self, dom_id: &DomId, node_id: NodeId) -> bool {
        let layout_window = self.get_layout_window();
        let layout_result = match layout_window.get_layout_result(dom_id) {
            Some(lr) => lr,
            None => return false,
        };
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = match node_data_cont.get(node_id) {
            Some(nd) => nd,
            None => return false,
        };
        node_data.is_anonymous()
    }

    pub fn get_parent(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;

        // Skip anonymous parent nodes - walk up the tree until we find a non-anonymous node
        let mut current_parent_id = hier_item.parent_id()?;
        loop {
            if !self.is_node_anonymous(&node_id.dom, current_parent_id) {
                return Some(DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(current_parent_id)),
                });
            }

            // This parent is anonymous, try its parent
            let parent_hier_item = node_hierarchy.get(current_parent_id)?;
            current_parent_id = parent_hier_item.parent_id()?;
        }
    }

    pub fn get_previous_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;

        // Skip anonymous siblings - walk backwards until we find a non-anonymous node
        let mut current_sibling_id = hier_item.previous_sibling_id()?;
        loop {
            if !self.is_node_anonymous(&node_id.dom, current_sibling_id) {
                return Some(DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(current_sibling_id)),
                });
            }

            // This sibling is anonymous, try the previous one
            let sibling_hier_item = node_hierarchy.get(current_sibling_id)?;
            current_sibling_id = sibling_hier_item.previous_sibling_id()?;
        }
    }

    pub fn get_next_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;

        // Skip anonymous siblings - walk forwards until we find a non-anonymous node
        let mut current_sibling_id = hier_item.next_sibling_id()?;
        loop {
            if !self.is_node_anonymous(&node_id.dom, current_sibling_id) {
                return Some(DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(current_sibling_id)),
                });
            }

            // This sibling is anonymous, try the next one
            let sibling_hier_item = node_hierarchy.get(current_sibling_id)?;
            current_sibling_id = sibling_hier_item.next_sibling_id()?;
        }
    }

    pub fn get_first_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;

        // Get first child, then skip anonymous nodes
        let mut current_child_id = hier_item.first_child_id(node_id_internal)?;
        loop {
            if !self.is_node_anonymous(&node_id.dom, current_child_id) {
                return Some(DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(current_child_id)),
                });
            }

            // This child is anonymous, try the next sibling
            let child_hier_item = node_hierarchy.get(current_child_id)?;
            current_child_id = child_hier_item.next_sibling_id()?;
        }
    }

    pub fn get_last_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;

        // Get last child, then skip anonymous nodes by walking backwards
        let mut current_child_id = hier_item.last_child_id()?;
        loop {
            if !self.is_node_anonymous(&node_id.dom, current_child_id) {
                return Some(DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(current_child_id)),
                });
            }

            // This child is anonymous, try the previous sibling
            let child_hier_item = node_hierarchy.get(current_child_id)?;
            current_child_id = child_hier_item.previous_sibling_id()?;
        }
    }

    // Node Data and State

    pub fn get_dataset(&mut self, node_id: DomNodeId) -> Option<RefAny> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;
        node_data.get_dataset().clone().into_option()
    }

    pub fn get_node_id_of_root_dataset(&mut self, search_key: RefAny) -> Option<DomNodeId> {
        let mut found: Option<(u64, DomNodeId)> = None;
        let search_type_id = search_key.get_type_id();

        for dom_id in self.get_dom_ids().as_ref().iter().copied() {
            let layout_window = self.get_layout_window();
            let layout_result = match layout_window.get_layout_result(&dom_id) {
                Some(lr) => lr,
                None => continue,
            };

            let node_data_cont = layout_result.styled_dom.node_data.as_container();
            for (node_idx, node_data) in node_data_cont.iter().enumerate() {
                if let Some(dataset) = node_data.get_dataset().clone().into_option() {
                    if dataset.get_type_id() == search_type_id {
                        let node_id = DomNodeId {
                            dom: dom_id,
                            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node_idx))),
                        };
                        let instance_id = dataset.instance_id;

                        match found {
                            None => found = Some((instance_id, node_id)),
                            Some((prev_instance, _)) => {
                                if instance_id < prev_instance {
                                    found = Some((instance_id, node_id));
                                }
                            }
                        }
                    }
                }
            }
        }

        found.map(|s| s.1)
    }

    pub fn get_string_contents(&self, node_id: DomNodeId) -> Option<AzString> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;

        if let NodeType::Text(ref text) = node_data.get_node_type() {
            Some(text.clone())
        } else {
            None
        }
    }

    /// Get the tag name of a node (e.g., "div", "p", "span")
    ///
    /// Returns the HTML tag name as a string for the given node.
    /// For text nodes, returns "text". For image nodes, returns "img".
    pub fn get_node_tag_name(&self, node_id: DomNodeId) -> Option<AzString> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;

        let tag = node_data.get_node_type().get_path();
        Some(tag.to_string().into())
    }

    /// Get an attribute value from a node by attribute name
    ///
    /// # Arguments
    /// * `node_id` - The node to query
    /// * `attr_name` - The attribute name (e.g., "id", "class", "href", "data-custom", "aria-label")
    ///
    /// Returns the attribute value if found, None otherwise.
    /// This searches the strongly-typed AttributeVec on the node.
    pub fn get_node_attribute(&self, node_id: DomNodeId, attr_name: &str) -> Option<AzString> {
        use azul_core::dom::AttributeType;
        
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;

        // Check the strongly-typed attributes vec
        for attr in node_data.attributes.as_ref() {
            match (attr_name, attr) {
                ("id", AttributeType::Id(v)) => return Some(v.clone()),
                ("class", AttributeType::Class(v)) => return Some(v.clone()),
                ("aria-label", AttributeType::AriaLabel(v)) => return Some(v.clone()),
                ("aria-labelledby", AttributeType::AriaLabelledBy(v)) => return Some(v.clone()),
                ("aria-describedby", AttributeType::AriaDescribedBy(v)) => return Some(v.clone()),
                ("role", AttributeType::AriaRole(v)) => return Some(v.clone()),
                ("href", AttributeType::Href(v)) => return Some(v.clone()),
                ("rel", AttributeType::Rel(v)) => return Some(v.clone()),
                ("target", AttributeType::Target(v)) => return Some(v.clone()),
                ("src", AttributeType::Src(v)) => return Some(v.clone()),
                ("alt", AttributeType::Alt(v)) => return Some(v.clone()),
                ("title", AttributeType::Title(v)) => return Some(v.clone()),
                ("name", AttributeType::Name(v)) => return Some(v.clone()),
                ("value", AttributeType::Value(v)) => return Some(v.clone()),
                ("type", AttributeType::InputType(v)) => return Some(v.clone()),
                ("placeholder", AttributeType::Placeholder(v)) => return Some(v.clone()),
                ("max", AttributeType::Max(v)) => return Some(v.clone()),
                ("min", AttributeType::Min(v)) => return Some(v.clone()),
                ("step", AttributeType::Step(v)) => return Some(v.clone()),
                ("pattern", AttributeType::Pattern(v)) => return Some(v.clone()),
                ("autocomplete", AttributeType::Autocomplete(v)) => return Some(v.clone()),
                ("scope", AttributeType::Scope(v)) => return Some(v.clone()),
                ("lang", AttributeType::Lang(v)) => return Some(v.clone()),
                ("dir", AttributeType::Dir(v)) => return Some(v.clone()),
                ("required", AttributeType::Required) => return Some("true".into()),
                ("disabled", AttributeType::Disabled) => return Some("true".into()),
                ("readonly", AttributeType::Readonly) => return Some("true".into()),
                ("checked", AttributeType::Checked) => return Some("true".into()),
                ("selected", AttributeType::Selected) => return Some("true".into()),
                ("hidden", AttributeType::Hidden) => return Some("true".into()),
                ("focusable", AttributeType::Focusable) => return Some("true".into()),
                ("minlength", AttributeType::MinLength(v)) => return Some(v.to_string().into()),
                ("maxlength", AttributeType::MaxLength(v)) => return Some(v.to_string().into()),
                ("colspan", AttributeType::ColSpan(v)) => return Some(v.to_string().into()),
                ("rowspan", AttributeType::RowSpan(v)) => return Some(v.to_string().into()),
                ("tabindex", AttributeType::TabIndex(v)) => return Some(v.to_string().into()),
                ("contenteditable", AttributeType::ContentEditable(v)) => return Some(v.to_string().into()),
                ("draggable", AttributeType::Draggable(v)) => return Some(v.to_string().into()),
                // Handle data-* attributes
                (name, AttributeType::Data(nv)) if name.starts_with("data-") && nv.attr_name.as_str() == &name[5..] => {
                    return Some(nv.value.clone());
                }
                // Handle aria-* state/property attributes
                (name, AttributeType::AriaState(nv)) if name == format!("aria-{}", nv.attr_name.as_str()) => {
                    return Some(nv.value.clone());
                }
                (name, AttributeType::AriaProperty(nv)) if name == format!("aria-{}", nv.attr_name.as_str()) => {
                    return Some(nv.value.clone());
                }
                // Handle custom attributes
                (name, AttributeType::Custom(nv)) if nv.attr_name.as_str() == name => {
                    return Some(nv.value.clone());
                }
                _ => continue,
            }
        }

        // Fallback: check ids_and_classes for "id" and "class"
        if attr_name == "id" {
            for id_or_class in node_data.ids_and_classes.as_ref() {
                if let IdOrClass::Id(id) = id_or_class {
                    return Some(id.clone());
                }
            }
        }

        if attr_name == "class" {
            let classes: Vec<&str> = node_data.ids_and_classes.as_ref()
                .iter()
                .filter_map(|ioc| {
                    if let IdOrClass::Class(class) = ioc {
                        Some(class.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if !classes.is_empty() {
                return Some(classes.join(" ").into());
            }
        }

        None
    }

    /// Get all classes of a node as a vector of strings
    pub fn get_node_classes(&self, node_id: DomNodeId) -> StringVec {
        let layout_window = match self.get_layout_window().get_layout_result(&node_id.dom) {
            Some(lr) => lr,
            None => return StringVec::from_const_slice(&[]),
        };
        let node_id_internal = match node_id.node.into_crate_internal() {
            Some(n) => n,
            None => return StringVec::from_const_slice(&[]),
        };
        let node_data_cont = layout_window.styled_dom.node_data.as_container();
        let node_data = match node_data_cont.get(node_id_internal) {
            Some(n) => n,
            None => return StringVec::from_const_slice(&[]),
        };

        let classes: Vec<AzString> = node_data.ids_and_classes.as_ref()
            .iter()
            .filter_map(|ioc| {
                if let IdOrClass::Class(class) = ioc {
                    Some(class.clone())
                } else {
                    None
                }
            })
            .collect();
        
        StringVec::from(classes)
    }

    /// Get the ID attribute of a node (if it has one)
    pub fn get_node_id(&self, node_id: DomNodeId) -> Option<AzString> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;

        for id_or_class in node_data.ids_and_classes.as_ref() {
            if let IdOrClass::Id(id) = id_or_class {
                return Some(id.clone());
            }
        }
        None
    }

    // Text Selection Management

    /// Get the current selection state for a DOM
    pub fn get_selection(&self, dom_id: &DomId) -> Option<&SelectionState> {
        self.get_layout_window()
            .selection_manager
            .get_selection(dom_id)
    }

    /// Check if a DOM has any selection
    pub fn has_selection(&self, dom_id: &DomId) -> bool {
        self.get_layout_window()
            .selection_manager
            .has_selection(dom_id)
    }

    /// Get the primary cursor for a DOM (first in selection list)
    pub fn get_primary_cursor(&self, dom_id: &DomId) -> Option<TextCursor> {
        self.get_layout_window()
            .selection_manager
            .get_primary_cursor(dom_id)
    }

    /// Get all selection ranges (excludes plain cursors)
    pub fn get_selection_ranges(&self, dom_id: &DomId) -> SelectionRangeVec {
        self.get_layout_window()
            .selection_manager
            .get_ranges(dom_id)
            .into()
    }

    /// Get direct access to the text layout cache
    ///
    /// Note: This provides direct read-only access to the text layout cache, but you need
    /// to know the CacheId for the specific text node you want. Currently there's
    /// no direct mapping from NodeId to CacheId exposed in the public API.
    ///
    /// For text modifications, use CallbackChange transactions:
    /// - `change_node_text()` for changing text content
    /// - `set_selection()` for setting selections
    /// - `get_selection()`, `get_primary_cursor()` for reading selections
    ///
    /// Future: Add NodeId -> CacheId mapping to enable node-specific layout access
    pub fn get_text_cache(&self) -> &TextLayoutCache {
        &self.get_layout_window().text_cache
    }

    // Window State Access

    /// Get full current window state (immutable reference)
    pub fn get_current_window_state(&self) -> &FullWindowState {
        // SAFETY: current_window_state is a valid pointer for the lifetime of CallbackInfo
        unsafe { (*self.ref_data).current_window_state }
    }

    /// Get current window flags
    pub fn get_current_window_flags(&self) -> WindowFlags {
        self.get_current_window_state().flags.clone()
    }

    /// Get current keyboard state
    pub fn get_current_keyboard_state(&self) -> KeyboardState {
        self.get_current_window_state().keyboard_state.clone()
    }

    /// Get current mouse state
    pub fn get_current_mouse_state(&self) -> MouseState {
        self.get_current_window_state().mouse_state.clone()
    }

    /// Get full previous window state (immutable reference)
    pub fn get_previous_window_state(&self) -> &Option<FullWindowState> {
        unsafe { (*self.ref_data).previous_window_state }
    }

    /// Get previous window flags
    pub fn get_previous_window_flags(&self) -> Option<WindowFlags> {
        Some(self.get_previous_window_state().as_ref()?.flags.clone())
    }

    /// Get previous keyboard state
    pub fn get_previous_keyboard_state(&self) -> Option<KeyboardState> {
        Some(
            self.get_previous_window_state()
                .as_ref()?
                .keyboard_state
                .clone(),
        )
    }

    /// Get previous mouse state
    pub fn get_previous_mouse_state(&self) -> Option<MouseState> {
        Some(
            self.get_previous_window_state()
                .as_ref()?
                .mouse_state
                .clone(),
        )
    }

    // Cursor and Input

    pub fn get_cursor_relative_to_node(&self) -> OptionLogicalPosition {
        self.cursor_relative_to_item
    }

    pub fn get_cursor_relative_to_viewport(&self) -> OptionLogicalPosition {
        self.cursor_in_viewport
    }

    pub fn get_current_window_handle(&self) -> RawWindowHandle {
        unsafe { (*self.ref_data).current_window_handle.clone() }
    }

    /// Get the system style (for menu rendering, CSD, etc.)
    /// This is useful for creating custom menus or other system-styled UI.
    pub fn get_system_style(&self) -> Arc<SystemStyle> {
        unsafe { (*self.ref_data).system_style.clone() }
    }

    /// Get the current cursor position in logical coordinates relative to the window
    pub fn get_cursor_position(&self) -> Option<LogicalPosition> {
        self.cursor_in_viewport.into_option()
    }

    /// Get the layout rectangle of the currently hit node (in logical coordinates)
    pub fn get_hit_node_layout_rect(&self) -> Option<LogicalRect> {
        self.get_layout_window()
            .get_node_layout_rect(self.hit_dom_node)
    }

    // Css Property Access

    /// Get the computed CSS property for a specific DOM node
    ///
    /// This queries the CSS property cache and returns the resolved property value
    /// for the given node, taking into account:
    /// - User overrides (from callbacks)
    /// - Node state (:hover, :active, :focus)
    /// - CSS rules from stylesheets
    /// - Cascaded properties from parents
    /// - Inline styles
    ///
    /// # Arguments
    /// * `node_id` - The DOM node to query
    /// * `property_type` - The CSS property type to retrieve
    ///
    /// # Returns
    /// * `Some(CssProperty)` if the property is set on this node
    /// * `None` if the property is not set (will use default value)
    pub fn get_computed_css_property(
        &self,
        node_id: DomNodeId,
        property_type: CssPropertyType,
    ) -> Option<CssProperty> {
        let layout_window = self.get_layout_window();

        // Get the layout result for this DOM
        let layout_result = layout_window.layout_results.get(&node_id.dom)?;

        // Get the styled DOM
        let styled_dom = &layout_result.styled_dom;

        // Convert DomNodeId to NodeId using proper decoding
        let internal_node_id = node_id.node.into_crate_internal()?;

        // Get the node data
        let node_data_container = styled_dom.node_data.as_container();
        let node_data = node_data_container.get(internal_node_id)?;

        // Get the styled node state
        let styled_nodes_container = styled_dom.styled_nodes.as_container();
        let styled_node = styled_nodes_container.get(internal_node_id)?;
        let node_state = &styled_node.styled_node_state;

        // Query the CSS property cache
        let css_property_cache = &styled_dom.css_property_cache.ptr;
        css_property_cache
            .get_property(node_data, &internal_node_id, node_state, &property_type)
            .cloned()
    }

    /// Get the computed width of a node from CSS
    ///
    /// Convenience method for getting the CSS width property.
    pub fn get_computed_width(&self, node_id: DomNodeId) -> Option<CssProperty> {
        self.get_computed_css_property(node_id, CssPropertyType::Width)
    }

    /// Get the computed height of a node from CSS
    ///
    /// Convenience method for getting the CSS height property.
    pub fn get_computed_height(&self, node_id: DomNodeId) -> Option<CssProperty> {
        self.get_computed_css_property(node_id, CssPropertyType::Height)
    }

    // System Callbacks

    pub fn get_system_time_fn(&self) -> GetSystemTimeCallback {
        unsafe { (*self.ref_data).system_callbacks.get_system_time_fn }
    }

    pub fn get_current_time(&self) -> task::Instant {
        let cb = self.get_system_time_fn();
        (cb.cb)()
    }

    /// Get immutable reference to the renderer resources
    ///
    /// This provides access to fonts, images, and other rendering resources.
    /// Useful for custom rendering or screenshot functionality.
    pub fn get_renderer_resources(&self) -> &RendererResources {
        unsafe { (*self.ref_data).renderer_resources }
    }

    // Screenshot API

    /// Take a CPU-rendered screenshot of the current window content
    ///
    /// This renders the current display list to a PNG image using CPU rendering.
    /// The screenshot captures the window content as it would appear on screen,
    /// without window decorations.
    ///
    /// # Arguments
    /// * `dom_id` - The DOM to screenshot (use the main DOM ID for the full window)
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - PNG-encoded image data
    /// * `Err(String)` - Error message if rendering failed
    ///
    /// # Example
    /// ```ignore
    /// fn on_click(info: &mut CallbackInfo) -> Update {
    ///     let dom_id = info.get_hit_node().dom;
    ///     match info.take_screenshot(dom_id) {
    ///         Ok(png_data) => {
    ///             std::fs::write("screenshot.png", png_data).unwrap();
    ///         }
    ///         Err(e) => eprintln!("Screenshot failed: {}", e),
    ///     }
    ///     Update::DoNothing
    /// }
    /// ```
    pub fn take_screenshot(&self, dom_id: DomId) -> Result<alloc::vec::Vec<u8>, AzString> {
        use crate::cpurender::{render, RenderOptions};
        
        let layout_window = self.get_layout_window();
        let renderer_resources = self.get_renderer_resources();
        
        // Get the layout result for this DOM
        let layout_result = layout_window.layout_results.get(&dom_id)
            .ok_or_else(|| AzString::from("DOM not found in layout results"))?;
        
        // Get viewport dimensions
        let viewport = &layout_result.viewport;
        let width = viewport.size.width;
        let height = viewport.size.height;
        
        if width <= 0.0 || height <= 0.0 {
            return Err(AzString::from("Invalid viewport dimensions"));
        }
        
        // Get the display list
        let display_list = &layout_result.display_list;
        
        // Get DPI factor from window state
        let dpi_factor = self.get_current_window_state()
            .size
            .get_hidpi_factor()
            .inner
            .get();
        
        // Render to pixmap
        let opts = RenderOptions {
            width,
            height,
            dpi_factor,
        };
        
        let pixmap = render(display_list, renderer_resources, opts)
            .map_err(|e| AzString::from(e))?;
        
        // Encode to PNG
        let png_data = pixmap.encode_png()
            .map_err(|e| AzString::from(alloc::format!("PNG encoding failed: {}", e)))?;
        
        Ok(png_data)
    }

    /// Take a screenshot and save it directly to a file
    ///
    /// Convenience method that combines `take_screenshot` with file writing.
    ///
    /// # Arguments
    /// * `dom_id` - The DOM to screenshot
    /// * `path` - The file path to save the PNG to
    ///
    /// # Returns
    /// * `Ok(())` - Screenshot saved successfully
    /// * `Err(String)` - Error message if rendering or saving failed
    #[cfg(feature = "std")]
    pub fn take_screenshot_to_file(&self, dom_id: DomId, path: &str) -> Result<(), AzString> {
        let png_data = self.take_screenshot(dom_id)?;
        std::fs::write(path, png_data)
            .map_err(|e| AzString::from(alloc::format!("Failed to write file: {}", e)))?;
        Ok(())
    }

    /// Take a native OS-level screenshot of the window including window decorations
    ///
    /// **NOTE**: This is a stub implementation. For full native screenshot support,
    /// use the `NativeScreenshotExt` trait from the `azul-dll` crate, which uses
    /// runtime dynamic loading (dlopen) to avoid static linking dependencies.
    ///
    /// # Returns
    /// * `Err(String)` - Always returns an error directing to use the extension trait
    #[cfg(feature = "std")]
    pub fn take_native_screenshot(&self, _path: &str) -> Result<(), AzString> {
        Err(AzString::from(
            "Native screenshot requires the NativeScreenshotExt trait from azul-dll crate. \
             Import it with: use azul::desktop::NativeScreenshotExt;"
        ))
    }

    /// Take a native OS-level screenshot and return the PNG data as bytes
    ///
    /// **NOTE**: This is a stub implementation. For full native screenshot support,
    /// use the `NativeScreenshotExt` trait from the `azul-dll` crate.
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - PNG-encoded image data
    /// * `Err(String)` - Error message if screenshot failed
    #[cfg(feature = "std")]
    pub fn take_native_screenshot_bytes(&self) -> Result<alloc::vec::Vec<u8>, AzString> {
        // Create a temporary file, take screenshot, read bytes, delete file
        let temp_path = std::env::temp_dir().join("azul_screenshot_temp.png");
        let temp_path_str = temp_path.to_string_lossy().to_string();
        
        self.take_native_screenshot(&temp_path_str)?;
        
        let bytes = std::fs::read(&temp_path)
            .map_err(|e| AzString::from(alloc::format!("Failed to read screenshot: {}", e)))?;
        
        let _ = std::fs::remove_file(&temp_path);
        
        Ok(bytes)
    }

    /// Take a native OS-level screenshot and return as a Base64 data URI
    ///
    /// Returns the screenshot as a "data:image/png;base64,..." string that can
    /// be directly used in HTML img tags or JSON responses.
    ///
    /// # Returns
    /// * `Ok(String)` - Base64 data URI string
    /// * `Err(String)` - Error message if screenshot failed
    ///
    #[cfg(feature = "std")]
    pub fn take_native_screenshot_base64(&self) -> Result<AzString, AzString> {
        let png_bytes = self.take_native_screenshot_bytes()?;
        let base64_str = base64_encode(&png_bytes);
        Ok(AzString::from(alloc::format!("data:image/png;base64,{}", base64_str)))
    }

    /// Take a CPU-rendered screenshot and return as a Base64 data URI
    ///
    /// Returns the screenshot as a "data:image/png;base64,..." string.
    /// This is the software-rendered version without window decorations.
    ///
    /// # Returns
    /// * `Ok(String)` - Base64 data URI string
    /// * `Err(String)` - Error message if rendering failed
    pub fn take_screenshot_base64(&self, dom_id: DomId) -> Result<AzString, AzString> {
        let png_bytes = self.take_screenshot(dom_id)?;
        let base64_str = base64_encode(&png_bytes);
        Ok(AzString::from(alloc::format!("data:image/png;base64,{}", base64_str)))
    }

    // Manager Access (Read-Only)

    /// Get immutable reference to the scroll manager
    ///
    /// Use this to query scroll state for nodes without modifying it.
    /// To request programmatic scrolling, use `nodes_scrolled_in_callback`.
    pub fn get_scroll_manager(&self) -> &ScrollManager {
        unsafe { &(*self.ref_data).layout_window.scroll_manager }
    }

    /// Get immutable reference to the gesture and drag manager
    ///
    /// Use this to query current gesture/drag state (e.g., "is this node being dragged?",
    /// "what files are being dropped?", "is a long-press active?").
    ///
    /// The manager is updated by the event loop and provides read-only query access
    /// to callbacks for gesture-aware UI behavior.
    pub fn get_gesture_drag_manager(&self) -> &GestureAndDragManager {
        unsafe { &(*self.ref_data).layout_window.gesture_drag_manager }
    }

    /// Get immutable reference to the focus manager
    ///
    /// Use this to query which node currently has focus and whether focus
    /// is being moved to another node.
    pub fn get_focus_manager(&self) -> &FocusManager {
        &self.get_layout_window().focus_manager
    }

    /// Get a reference to the undo/redo manager
    ///
    /// This allows user callbacks to query the undo/redo state and intercept
    /// undo/redo operations via preventDefault().
    pub fn get_undo_redo_manager(&self) -> &UndoRedoManager {
        &self.get_layout_window().undo_redo_manager
    }

    /// Get immutable reference to the hover manager
    ///
    /// Use this to query which nodes are currently hovered at various input points
    /// (mouse, touch points, pen).
    pub fn get_hover_manager(&self) -> &HoverManager {
        &self.get_layout_window().hover_manager
    }

    /// Get immutable reference to the text input manager
    ///
    /// Use this to query text selection state, cursor positions, and IME composition.
    pub fn get_text_input_manager(&self) -> &TextInputManager {
        &self.get_layout_window().text_input_manager
    }

    /// Get immutable reference to the selection manager
    ///
    /// Use this to query text selections across multiple nodes.
    pub fn get_selection_manager(&self) -> &SelectionManager {
        &self.get_layout_window().selection_manager
    }

    /// Check if a specific node is currently focused
    pub fn is_node_focused(&self, node_id: DomNodeId) -> bool {
        self.get_focus_manager().has_focus(&node_id)
    }

    /// Check if any node in a specific DOM is focused
    pub fn is_dom_focused(&self, dom_id: DomId) -> bool {
        self.get_focused_node()
            .map(|n| n.dom == dom_id)
            .unwrap_or(false)
    }

    // Pen/Stylus Query Methods

    /// Get current pen/stylus state if a pen is active
    pub fn get_pen_state(&self) -> Option<&PenState> {
        self.get_gesture_drag_manager().get_pen_state()
    }

    /// Get current pen pressure (0.0 to 1.0)
    /// Returns None if no pen is active, Some(0.5) for mouse
    pub fn get_pen_pressure(&self) -> Option<f32> {
        self.get_pen_state().map(|pen| pen.pressure)
    }

    /// Get current pen tilt angles (x_tilt, y_tilt) in degrees
    /// Returns None if no pen is active
    pub fn get_pen_tilt(&self) -> Option<PenTilt> {
        self.get_pen_state().map(|pen| pen.tilt)
    }

    /// Check if pen is currently in contact with surface
    pub fn is_pen_in_contact(&self) -> bool {
        self.get_pen_state()
            .map(|pen| pen.in_contact)
            .unwrap_or(false)
    }

    /// Check if pen is in eraser mode
    pub fn is_pen_eraser(&self) -> bool {
        self.get_pen_state()
            .map(|pen| pen.is_eraser)
            .unwrap_or(false)
    }

    /// Check if pen barrel button is pressed
    pub fn is_pen_barrel_button_pressed(&self) -> bool {
        self.get_pen_state()
            .map(|pen| pen.barrel_button_pressed)
            .unwrap_or(false)
    }

    /// Get the last recorded input sample (for event_id and detailed input data)
    pub fn get_last_input_sample(&self) -> Option<&InputSample> {
        let manager = self.get_gesture_drag_manager();
        manager
            .get_current_session()
            .and_then(|session| session.last_sample())
    }

    /// Get the event ID of the current event
    pub fn get_current_event_id(&self) -> Option<u64> {
        self.get_last_input_sample().map(|sample| sample.event_id)
    }

    // Focus Management Methods

    /// Set focus to a specific DOM node by ID
    pub fn set_focus_to_node(&mut self, dom_id: DomId, node_id: NodeId) {
        self.set_focus(FocusTarget::Id(DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        }));
    }

    /// Set focus to a node matching a CSS path
    pub fn set_focus_to_path(&mut self, dom_id: DomId, css_path: CssPath) {
        self.set_focus(FocusTarget::Path(FocusTargetPath {
            dom: dom_id,
            css_path,
        }));
    }

    /// Move focus to next focusable element in tab order
    pub fn focus_next(&mut self) {
        self.set_focus(FocusTarget::Next);
    }

    /// Move focus to previous focusable element in tab order
    pub fn focus_previous(&mut self) {
        self.set_focus(FocusTarget::Previous);
    }

    /// Move focus to first focusable element
    pub fn focus_first(&mut self) {
        self.set_focus(FocusTarget::First);
    }

    /// Move focus to last focusable element
    pub fn focus_last(&mut self) {
        self.set_focus(FocusTarget::Last);
    }

    /// Remove focus from all elements
    pub fn clear_focus(&mut self) {
        self.set_focus(FocusTarget::NoFocus);
    }

    // Manager Access Methods

    /// Check if a drag gesture is currently active
    ///
    /// Convenience method that queries the gesture manager.
    pub fn is_dragging(&self) -> bool {
        self.get_gesture_drag_manager().is_dragging()
    }

    /// Get the currently focused node (if any)
    ///
    /// Returns None if no node has focus.
    pub fn get_focused_node(&self) -> Option<DomNodeId> {
        self.get_layout_window()
            .focus_manager
            .get_focused_node()
            .copied()
    }

    /// Check if a specific node has focus
    pub fn has_focus(&self, node_id: DomNodeId) -> bool {
        self.get_layout_window().focus_manager.has_focus(&node_id)
    }

    /// Get the currently hovered file (if drag-drop is in progress)
    ///
    /// Returns None if no file is being hovered over the window.
    pub fn get_hovered_file(&self) -> Option<&azul_css::AzString> {
        self.get_layout_window()
            .file_drop_manager
            .get_hovered_file()
    }

    /// Get the currently dropped file (if a file was just dropped)
    ///
    /// This is a one-shot value that is cleared after event processing.
    /// Returns None if no file was dropped this frame.
    pub fn get_dropped_file(&self) -> Option<&azul_css::AzString> {
        self.get_layout_window()
            .file_drop_manager
            .dropped_file
            .as_ref()
    }

    /// Check if a node or file drag is currently active
    ///
    /// Returns true if either a node drag or file drag is in progress.
    pub fn is_drag_active(&self) -> bool {
        self.get_layout_window().drag_drop_manager.is_dragging()
    }

    /// Check if a node drag is specifically active
    pub fn is_node_drag_active(&self) -> bool {
        self.get_layout_window()
            .drag_drop_manager
            .is_dragging_node()
    }

    /// Check if a file drag is specifically active
    pub fn is_file_drag_active(&self) -> bool {
        self.get_layout_window()
            .drag_drop_manager
            .is_dragging_file()
    }

    /// Get the current drag/drop state (if any)
    ///
    /// Returns None if no drag is active, or Some with drag details.
    pub fn get_drag_state(&self) -> Option<&DragState> {
        self.get_layout_window().drag_drop_manager.get_drag_state()
    }

    // Hover Manager Access

    /// Get the current mouse cursor hit test result (most recent frame)
    pub fn get_current_hit_test(&self) -> Option<&FullHitTest> {
        self.get_hover_manager().get_current(&InputPointId::Mouse)
    }

    /// Get mouse cursor hit test from N frames ago (0 = current, 1 = previous, etc.)
    pub fn get_hit_test_frame(&self, frames_ago: usize) -> Option<&FullHitTest> {
        self.get_hover_manager()
            .get_frame(&InputPointId::Mouse, frames_ago)
    }

    /// Get the full mouse cursor hit test history (up to 5 frames)
    ///
    /// Returns None if no mouse history exists yet
    pub fn get_hit_test_history(&self) -> Option<&VecDeque<FullHitTest>> {
        self.get_hover_manager().get_history(&InputPointId::Mouse)
    }

    /// Check if there's sufficient mouse history for gesture detection (at least 2 frames)
    pub fn has_sufficient_history_for_gestures(&self) -> bool {
        self.get_hover_manager()
            .has_sufficient_history_for_gestures(&InputPointId::Mouse)
    }

    // File Drop Manager Access

    /// Get immutable reference to the file drop manager
    pub fn get_file_drop_manager(&self) -> &FileDropManager {
        &self.get_layout_window().file_drop_manager
    }

    /// Get all selections across all DOMs
    pub fn get_all_selections(&self) -> &BTreeMap<DomId, SelectionState> {
        self.get_selection_manager().get_all_selections()
    }

    // Drag-Drop Manager Access

    /// Get immutable reference to the drag-drop manager
    pub fn get_drag_drop_manager(&self) -> &DragDropManager {
        &self.get_layout_window().drag_drop_manager
    }

    /// Get the node being dragged (if any)
    pub fn get_dragged_node(&self) -> Option<DomNodeId> {
        self.get_drag_drop_manager()
            .get_drag_state()
            .and_then(|state| {
                if state.drag_type == DragType::Node {
                    state.source_node.into_option()
                } else {
                    None
                }
            })
    }

    /// Get the file path being dragged (if any)
    pub fn get_dragged_file(&self) -> Option<&AzString> {
        self.get_drag_drop_manager()
            .get_drag_state()
            .and_then(|state| {
                if state.drag_type == DragType::File {
                    state.file_path.as_ref()
                } else {
                    None
                }
            })
    }

    // Scroll Manager Query Methods

    /// Get the current scroll offset for the hit node (if it's scrollable)
    /// 
    /// Convenience method that uses the `hit_dom_node` from this callback.
    /// Use `get_scroll_offset_for_node` if you need to query a specific node.
    pub fn get_scroll_offset(&self) -> Option<LogicalPosition> {
        self.get_scroll_offset_for_node(self.hit_dom_node.dom, self.hit_dom_node.node.into_crate_internal().unwrap())
    }

    /// Get the current scroll offset for a specific node (if it's scrollable)
    pub fn get_scroll_offset_for_node(&self, dom_id: DomId, node_id: NodeId) -> Option<LogicalPosition> {
        self.get_scroll_manager()
            .get_current_offset(dom_id, node_id)
    }

    /// Get the scroll delta for a node in the current frame (for scroll event detection)
    pub fn get_scroll_delta(&self, dom_id: DomId, node_id: NodeId) -> Option<LogicalPosition> {
        self.get_scroll_manager().get_scroll_delta(dom_id, node_id)
    }

    /// Check if a node had scroll activity this frame
    pub fn had_scroll_activity(&self, dom_id: DomId, node_id: NodeId) -> bool {
        self.get_scroll_manager()
            .had_scroll_activity_for_node(dom_id, node_id)
    }

    /// Get the scroll state (container rect, content rect, current offset) for a node
    pub fn get_scroll_state(&self, dom_id: DomId, node_id: NodeId) -> Option<&AnimatedScrollState> {
        self.get_scroll_manager().get_scroll_state(dom_id, node_id)
    }

    // Gpu State Manager Access

    /// Get immutable reference to the GPU state manager
    pub fn get_gpu_state_manager(&self) -> &GpuStateManager {
        &self.get_layout_window().gpu_state_manager
    }

    // IFrame Manager Access

    /// Get immutable reference to the IFrame manager
    pub fn get_iframe_manager(&self) -> &IFrameManager {
        &self.get_layout_window().iframe_manager
    }

    // Changeset Inspection/Modification Methods
    // These methods allow callbacks to inspect pending operations and modify them before execution

    /// Inspect a pending copy operation
    ///
    /// Returns the clipboard content that would be copied if the operation proceeds.
    /// Use this to validate or transform clipboard content before copying.
    pub fn inspect_copy_changeset(&self, target: DomNodeId) -> Option<ClipboardContent> {
        let layout_window = self.get_layout_window();
        let dom_id = &target.dom;
        layout_window.get_selected_content_for_clipboard(dom_id)
    }

    /// Inspect a pending cut operation
    ///
    /// Returns the clipboard content that would be cut (copied + deleted).
    /// Use this to validate or transform content before cutting.
    pub fn inspect_cut_changeset(&self, target: DomNodeId) -> Option<ClipboardContent> {
        // Cut uses same content extraction as copy
        self.inspect_copy_changeset(target)
    }

    /// Inspect the current selection range that would be affected by paste
    ///
    /// Returns the selection range that will be replaced when pasting.
    /// Returns None if no selection exists (paste will insert at cursor).
    pub fn inspect_paste_target_range(&self, target: DomNodeId) -> Option<SelectionRange> {
        let layout_window = self.get_layout_window();
        let dom_id = &target.dom;
        layout_window
            .selection_manager
            .get_ranges(dom_id)
            .first()
            .copied()
    }

    /// Inspect what text would be selected by Select All operation
    ///
    /// Returns the full text content and the range that would be selected.
    pub fn inspect_select_all_changeset(
        &self,
        target: DomNodeId,
    ) -> Option<SelectAllResult> {
        use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};

        let layout_window = self.get_layout_window();
        let node_id = target.node.into_crate_internal()?;

        // Get text content
        let content = layout_window.get_text_before_textinput(target.dom, node_id);
        let text = layout_window.extract_text_from_inline_content(&content);

        // Create selection range from start to end
        let start_cursor = TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        };

        let end_cursor = TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: text.len() as u32,
            },
            affinity: CursorAffinity::Leading,
        };

        let range = SelectionRange {
            start: start_cursor,
            end: end_cursor,
        };

        Some(SelectAllResult {
            full_text: text.into(),
            selection_range: range,
        })
    }

    /// Inspect what would be deleted by a backspace/delete operation
    ///
    /// Uses the pure functions from `text3::edit::inspect_delete()` to determine
    /// what would be deleted without actually performing the deletion.
    ///
    /// Returns (range_to_delete, deleted_text).
    /// - forward=true: Delete key (delete character after cursor)
    /// - forward=false: Backspace key (delete character before cursor)
    pub fn inspect_delete_changeset(
        &self,
        target: DomNodeId,
        forward: bool,
    ) -> Option<DeleteResult> {
        let layout_window = self.get_layout_window();
        let dom_id = &target.dom;
        let node_id = target.node.into_crate_internal()?;

        // Get the inline content for this node
        let content = layout_window.get_text_before_textinput(target.dom, node_id);

        // Get current selection state
        let selection =
            if let Some(range) = layout_window.selection_manager.get_ranges(dom_id).first() {
                Selection::Range(*range)
            } else if let Some(cursor) = layout_window.cursor_manager.get_cursor() {
                Selection::Cursor(*cursor)
            } else {
                return None; // No cursor or selection
            };

        // Use text3::edit::inspect_delete to determine what would be deleted
        crate::text3::edit::inspect_delete(&content, &selection, forward)
            .map(|(range, text)| DeleteResult {
                range_to_delete: range,
                deleted_text: text.into(),
            })
    }

    /// Inspect a pending undo operation
    ///
    /// Returns the operation that would be undone, allowing inspection
    /// of what state will be restored.
    pub fn inspect_undo_operation(&self, node_id: NodeId) -> Option<&UndoableOperation> {
        self.get_undo_redo_manager().peek_undo(node_id)
    }

    /// Inspect a pending redo operation
    ///
    /// Returns the operation that would be reapplied.
    pub fn inspect_redo_operation(&self, node_id: NodeId) -> Option<&UndoableOperation> {
        self.get_undo_redo_manager().peek_redo(node_id)
    }

    /// Check if undo is available for a specific node
    ///
    /// Returns true if there is at least one undoable operation in the stack.
    pub fn can_undo(&self, node_id: NodeId) -> bool {
        self.get_undo_redo_manager()
            .get_stack(node_id)
            .map(|stack| stack.can_undo())
            .unwrap_or(false)
    }

    /// Check if redo is available for a specific node
    ///
    /// Returns true if there is at least one redoable operation in the stack.
    pub fn can_redo(&self, node_id: NodeId) -> bool {
        self.get_undo_redo_manager()
            .get_stack(node_id)
            .map(|stack| stack.can_redo())
            .unwrap_or(false)
    }

    /// Get the text that would be restored by undo for a specific node
    ///
    /// Returns the pre-state text content that would be restored if undo is performed.
    /// Returns None if no undo operation is available.
    pub fn get_undo_text(&self, node_id: NodeId) -> Option<AzString> {
        self.get_undo_redo_manager()
            .peek_undo(node_id)
            .map(|op| op.pre_state.text_content.clone())
    }

    /// Get the text that would be restored by redo for a specific node
    ///
    /// Returns the pre-state text content that would be restored if redo is performed.
    /// Returns None if no redo operation is available.
    pub fn get_redo_text(&self, node_id: NodeId) -> Option<AzString> {
        self.get_undo_redo_manager()
            .peek_redo(node_id)
            .map(|op| op.pre_state.text_content.clone())
    }

    // Clipboard Helper Methods

    /// Get clipboard content from system clipboard (available during paste operations)
    ///
    /// This returns content that was read from the system clipboard when Ctrl+V was pressed.
    /// It's only available in On::Paste callbacks or similar clipboard-related callbacks.
    ///
    /// Use this to inspect what will be pasted before allowing or modifying the paste operation.
    ///
    /// # Returns
    /// * `Some(&ClipboardContent)` - If paste is in progress and clipboard has content
    /// * `None` - If no paste operation is active or clipboard is empty
    pub fn get_clipboard_content(&self) -> Option<&ClipboardContent> {
        unsafe {
            (*self.ref_data)
                .layout_window
                .clipboard_manager
                .get_paste_content()
        }
    }

    /// Override clipboard content for copy/cut operations
    ///
    /// This sets custom content that will be written to the system clipboard.
    /// Use this in On::Copy or On::Cut callbacks to modify what gets copied.
    ///
    /// # Arguments
    /// * `content` - The clipboard content to write to system clipboard
    pub fn set_clipboard_content(&mut self, content: ClipboardContent) {
        // Queue the clipboard content to be set after callback returns
        // This will be picked up by the clipboard manager
        self.push_change(CallbackChange::SetCopyContent {
            target: self.hit_dom_node,
            content,
        });
    }

    /// Set/modify the clipboard content before a copy operation
    ///
    /// Use this to transform clipboard content before copying.
    /// The change is queued and will be applied after the callback returns,
    /// if preventDefault() was not called.
    pub fn set_copy_content(&mut self, target: DomNodeId, content: ClipboardContent) {
        self.push_change(CallbackChange::SetCopyContent { target, content });
    }

    /// Set/modify the clipboard content before a cut operation
    ///
    /// Similar to set_copy_content but for cut operations.
    /// The change is queued and will be applied after the callback returns.
    pub fn set_cut_content(&mut self, target: DomNodeId, content: ClipboardContent) {
        self.push_change(CallbackChange::SetCutContent { target, content });
    }

    /// Override the selection range for select-all operation
    ///
    /// Use this to limit what gets selected (e.g., only select visible text).
    /// The change is queued and will be applied after the callback returns.
    pub fn set_select_all_range(&mut self, target: DomNodeId, range: SelectionRange) {
        self.push_change(CallbackChange::SetSelectAllRange { target, range });
    }

    /// Get the current text content of a node
    ///
    /// Helper for inspecting text before operations.
    pub fn get_node_text_content(&self, target: DomNodeId) -> Option<String> {
        let layout_window = self.get_layout_window();
        let node_id = target.node.into_crate_internal()?;
        let content = layout_window.get_text_before_textinput(target.dom, node_id);
        Some(layout_window.extract_text_from_inline_content(&content))
    }

    /// Get the current cursor position in a node
    ///
    /// Returns the text cursor position if the node is focused.
    pub fn get_node_cursor_position(&self, target: DomNodeId) -> Option<TextCursor> {
        let layout_window = self.get_layout_window();

        // Check if this node is focused
        if !layout_window.focus_manager.has_focus(&target) {
            return None;
        }

        layout_window.cursor_manager.get_cursor().copied()
    }

    /// Get the current selection ranges in a node
    ///
    /// Returns all active selection ranges for the specified DOM.
    pub fn get_node_selection_ranges(&self, target: DomNodeId) -> SelectionRangeVec {
        let layout_window = self.get_layout_window();
        layout_window
            .selection_manager
            .get_ranges(&target.dom)
            .into()
    }

    /// Check if a specific node has an active selection
    ///
    /// This checks if the specific node (identified by DomNodeId) has a selection,
    /// as opposed to has_selection(DomId) which checks the entire DOM.
    pub fn node_has_selection(&self, target: DomNodeId) -> bool {
        self.get_node_selection_ranges(target).as_ref().is_empty() == false
    }

    /// Get the length of text in a node
    ///
    /// Useful for bounds checking in custom operations.
    pub fn get_node_text_length(&self, target: DomNodeId) -> Option<usize> {
        self.get_node_text_content(target).map(|text| text.len())
    }

    // Cursor Movement Inspection/Override Methods

    /// Inspect where the cursor would move when pressing left arrow
    ///
    /// Returns the new cursor position that would result from moving left.
    /// Returns None if the cursor is already at the start of the document.
    ///
    /// # Arguments
    /// * `target` - The node containing the cursor
    pub fn inspect_move_cursor_left(&self, target: DomNodeId) -> Option<TextCursor> {
        let layout_window = self.get_layout_window();
        let cursor = layout_window.cursor_manager.get_cursor()?;

        // Get the text layout directly via layout_results → LayoutTree → LayoutNode →
        // inline_layout_result
        let layout = self.get_inline_layout_for_node(&target)?;

        // Use the text3::cache cursor movement logic
        let new_cursor = layout.move_cursor_left(*cursor, &mut None);

        // Only return if cursor actually moved
        if new_cursor != *cursor {
            Some(new_cursor)
        } else {
            None
        }
    }

    /// Inspect where the cursor would move when pressing right arrow
    ///
    /// Returns the new cursor position that would result from moving right.
    /// Returns None if the cursor is already at the end of the document.
    pub fn inspect_move_cursor_right(&self, target: DomNodeId) -> Option<TextCursor> {
        let layout_window = self.get_layout_window();
        let cursor = layout_window.cursor_manager.get_cursor()?;

        // Get the text layout directly via layout_results → LayoutTree → LayoutNode →
        // inline_layout_result
        let layout = self.get_inline_layout_for_node(&target)?;

        // Use the text3::cache cursor movement logic
        let new_cursor = layout.move_cursor_right(*cursor, &mut None);

        // Only return if cursor actually moved
        if new_cursor != *cursor {
            Some(new_cursor)
        } else {
            None
        }
    }

    /// Inspect where the cursor would move when pressing up arrow
    ///
    /// Returns the new cursor position that would result from moving up one line.
    /// Returns None if the cursor is already on the first line.
    pub fn inspect_move_cursor_up(&self, target: DomNodeId) -> Option<TextCursor> {
        let layout_window = self.get_layout_window();
        let cursor = layout_window.cursor_manager.get_cursor()?;

        // Get the text layout directly via layout_results → LayoutTree → LayoutNode →
        // inline_layout_result
        let layout = self.get_inline_layout_for_node(&target)?;

        // Use the text3::cache cursor movement logic
        // goal_x maintains horizontal position when moving vertically
        let new_cursor = layout.move_cursor_up(*cursor, &mut None, &mut None);

        // Only return if cursor actually moved
        if new_cursor != *cursor {
            Some(new_cursor)
        } else {
            None
        }
    }

    /// Inspect where the cursor would move when pressing down arrow
    ///
    /// Returns the new cursor position that would result from moving down one line.
    /// Returns None if the cursor is already on the last line.
    pub fn inspect_move_cursor_down(&self, target: DomNodeId) -> Option<TextCursor> {
        let layout_window = self.get_layout_window();
        let cursor = layout_window.cursor_manager.get_cursor()?;

        // Get the text layout directly via layout_results → LayoutTree → LayoutNode →
        // inline_layout_result
        let layout = self.get_inline_layout_for_node(&target)?;

        // Use the text3::cache cursor movement logic
        // goal_x maintains horizontal position when moving vertically
        let new_cursor = layout.move_cursor_down(*cursor, &mut None, &mut None);

        // Only return if cursor actually moved
        if new_cursor != *cursor {
            Some(new_cursor)
        } else {
            None
        }
    }

    /// Inspect where the cursor would move when pressing Home key
    ///
    /// Returns the cursor position at the start of the current line.
    pub fn inspect_move_cursor_to_line_start(&self, target: DomNodeId) -> Option<TextCursor> {
        let layout_window = self.get_layout_window();
        let cursor = layout_window.cursor_manager.get_cursor()?;

        // Get the text layout directly via layout_results → LayoutTree → LayoutNode →
        // inline_layout_result
        let layout = self.get_inline_layout_for_node(&target)?;

        // Use the text3::cache cursor movement logic
        let new_cursor = layout.move_cursor_to_line_start(*cursor, &mut None);

        // Always return the result (might be same as input if already at line start)
        Some(new_cursor)
    }

    /// Inspect where the cursor would move when pressing End key
    ///
    /// Returns the cursor position at the end of the current line.
    pub fn inspect_move_cursor_to_line_end(&self, target: DomNodeId) -> Option<TextCursor> {
        let layout_window = self.get_layout_window();
        let cursor = layout_window.cursor_manager.get_cursor()?;

        // Get the text layout directly via layout_results → LayoutTree → LayoutNode →
        // inline_layout_result
        let layout = self.get_inline_layout_for_node(&target)?;

        // Use the text3::cache cursor movement logic
        let new_cursor = layout.move_cursor_to_line_end(*cursor, &mut None);

        // Always return the result (might be same as input if already at line end)
        Some(new_cursor)
    }

    /// Inspect where the cursor would move when pressing Ctrl+Home
    ///
    /// Returns the cursor position at the start of the document.
    pub fn inspect_move_cursor_to_document_start(&self, target: DomNodeId) -> Option<TextCursor> {
        use azul_core::selection::{CursorAffinity, GraphemeClusterId};

        Some(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        })
    }

    /// Inspect where the cursor would move when pressing Ctrl+End
    ///
    /// Returns the cursor position at the end of the document.
    pub fn inspect_move_cursor_to_document_end(&self, target: DomNodeId) -> Option<TextCursor> {
        use azul_core::selection::{CursorAffinity, GraphemeClusterId};

        let text_len = self.get_node_text_length(target)?;

        Some(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: text_len as u32,
            },
            affinity: CursorAffinity::Leading,
        })
    }

    /// Inspect what text would be deleted by backspace (including Shift+Backspace)
    ///
    /// Returns (range_to_delete, deleted_text).
    /// This is a convenience wrapper around inspect_delete_changeset(target, false).
    pub fn inspect_backspace(&self, target: DomNodeId) -> Option<DeleteResult> {
        self.inspect_delete_changeset(target, false)
    }

    /// Inspect what text would be deleted by delete key
    ///
    /// Returns (range_to_delete, deleted_text).
    /// This is a convenience wrapper around inspect_delete_changeset(target, true).
    pub fn inspect_delete(&self, target: DomNodeId) -> Option<DeleteResult> {
        self.inspect_delete_changeset(target, true)
    }

    // Cursor Movement Override Methods
    // These methods queue cursor movement operations to be applied after the callback

    /// Move cursor left (arrow left key)
    ///
    /// # Arguments
    /// * `target` - The node containing the cursor
    /// * `extend_selection` - If true, extends selection (Shift+Left); if false, moves cursor
    pub fn move_cursor_left(&mut self, target: DomNodeId, extend_selection: bool) {
        self.push_change(CallbackChange::MoveCursorLeft {
            dom_id: target.dom,
            node_id: target.node.into_crate_internal().unwrap_or(NodeId::ZERO),
            extend_selection,
        });
    }

    /// Move cursor right (arrow right key)
    pub fn move_cursor_right(&mut self, target: DomNodeId, extend_selection: bool) {
        self.push_change(CallbackChange::MoveCursorRight {
            dom_id: target.dom,
            node_id: target.node.into_crate_internal().unwrap_or(NodeId::ZERO),
            extend_selection,
        });
    }

    /// Move cursor up (arrow up key)
    pub fn move_cursor_up(&mut self, target: DomNodeId, extend_selection: bool) {
        self.push_change(CallbackChange::MoveCursorUp {
            dom_id: target.dom,
            node_id: target.node.into_crate_internal().unwrap_or(NodeId::ZERO),
            extend_selection,
        });
    }

    /// Move cursor down (arrow down key)
    pub fn move_cursor_down(&mut self, target: DomNodeId, extend_selection: bool) {
        self.push_change(CallbackChange::MoveCursorDown {
            dom_id: target.dom,
            node_id: target.node.into_crate_internal().unwrap_or(NodeId::ZERO),
            extend_selection,
        });
    }

    /// Move cursor to line start (Home key)
    pub fn move_cursor_to_line_start(&mut self, target: DomNodeId, extend_selection: bool) {
        self.push_change(CallbackChange::MoveCursorToLineStart {
            dom_id: target.dom,
            node_id: target.node.into_crate_internal().unwrap_or(NodeId::ZERO),
            extend_selection,
        });
    }

    /// Move cursor to line end (End key)
    pub fn move_cursor_to_line_end(&mut self, target: DomNodeId, extend_selection: bool) {
        self.push_change(CallbackChange::MoveCursorToLineEnd {
            dom_id: target.dom,
            node_id: target.node.into_crate_internal().unwrap_or(NodeId::ZERO),
            extend_selection,
        });
    }

    /// Move cursor to document start (Ctrl+Home)
    pub fn move_cursor_to_document_start(&mut self, target: DomNodeId, extend_selection: bool) {
        self.push_change(CallbackChange::MoveCursorToDocumentStart {
            dom_id: target.dom,
            node_id: target.node.into_crate_internal().unwrap_or(NodeId::ZERO),
            extend_selection,
        });
    }

    /// Move cursor to document end (Ctrl+End)
    pub fn move_cursor_to_document_end(&mut self, target: DomNodeId, extend_selection: bool) {
        self.push_change(CallbackChange::MoveCursorToDocumentEnd {
            dom_id: target.dom,
            node_id: target.node.into_crate_internal().unwrap_or(NodeId::ZERO),
            extend_selection,
        });
    }

    /// Delete text backward (backspace or Shift+Backspace)
    ///
    /// Queues a backspace operation to be applied after the callback.
    /// Use inspect_backspace() to see what would be deleted.
    pub fn delete_backward(&mut self, target: DomNodeId) {
        self.push_change(CallbackChange::DeleteBackward {
            dom_id: target.dom,
            node_id: target.node.into_crate_internal().unwrap_or(NodeId::ZERO),
        });
    }

    /// Delete text forward (delete key)
    ///
    /// Queues a delete operation to be applied after the callback.
    /// Use inspect_delete() to see what would be deleted.
    pub fn delete_forward(&mut self, target: DomNodeId) {
        self.push_change(CallbackChange::DeleteForward {
            dom_id: target.dom,
            node_id: target.node.into_crate_internal().unwrap_or(NodeId::ZERO),
        });
    }
}

/// Config necessary for threading + animations to work in no_std environments
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ExternalSystemCallbacks {
    pub create_thread_fn: CreateThreadCallback,
    pub get_system_time_fn: GetSystemTimeCallback,
}

impl ExternalSystemCallbacks {
    #[cfg(not(feature = "std"))]
    pub fn rust_internal() -> Self {
        use crate::thread::create_thread_libstd;

        Self {
            create_thread_fn: CreateThreadCallback {
                cb: create_thread_libstd,
            },
            get_system_time_fn: GetSystemTimeCallback {
                cb: azul_core::task::get_system_time_libstd,
            },
        }
    }

    #[cfg(feature = "std")]
    pub fn rust_internal() -> Self {
        use crate::thread::create_thread_libstd;

        Self {
            create_thread_fn: CreateThreadCallback {
                cb: create_thread_libstd,
            },
            get_system_time_fn: GetSystemTimeCallback {
                cb: azul_core::task::get_system_time_libstd,
            },
        }
    }
}

/// Request to change focus, returned from callbacks
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusUpdateRequest {
    /// Focus a specific node
    FocusNode(DomNodeId),
    /// Clear focus (no node has focus)
    ClearFocus,
    /// No focus change requested
    NoChange,
}

impl FocusUpdateRequest {
    /// Check if this represents a focus change
    pub fn is_change(&self) -> bool {
        !matches!(self, FocusUpdateRequest::NoChange)
    }

    /// Convert to the new focused node (Some(node) or None for clear)
    pub fn to_focused_node(&self) -> Option<Option<DomNodeId>> {
        match self {
            FocusUpdateRequest::FocusNode(node) => Some(Some(*node)),
            FocusUpdateRequest::ClearFocus => Some(None),
            FocusUpdateRequest::NoChange => None,
        }
    }

    /// Create from Option<Option<DomNodeId>> (legacy format)
    pub fn from_optional(opt: Option<Option<DomNodeId>>) -> Self {
        match opt {
            Some(Some(node)) => FocusUpdateRequest::FocusNode(node),
            Some(None) => FocusUpdateRequest::ClearFocus,
            None => FocusUpdateRequest::NoChange,
        }
    }
}

/// Result of calling callbacks, containing all state changes
#[derive(Debug)]
pub struct CallCallbacksResult {
    /// Whether the UI should be rendered due to a scroll event
    pub should_scroll_render: bool,
    /// Whether the callbacks say to rebuild the UI or not
    pub callbacks_update_screen: Update,
    /// FullWindowState that was (potentially) modified in the callbacks
    pub modified_window_state: Option<FullWindowState>,
    /// Text changes that don't require full relayout
    pub words_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, AzString>>>,
    /// Image changes (for animated images/video)
    pub images_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, (ImageRef, UpdateImageType)>>>,
    /// Clip mask changes (for vector animations)
    pub image_masks_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, ImageMask>>>,
    /// Image callback changes (for OpenGL texture updates)
    pub image_callbacks_changed: Option<BTreeMap<DomId, FastBTreeSet<NodeId>>>,
    /// CSS property changes from callbacks
    pub css_properties_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, CssPropertyVec>>>,
    /// Scroll position changes from callbacks
    pub nodes_scrolled_in_callbacks:
        Option<BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, LogicalPosition>>>,
    /// Focus change request from callback (if any)
    pub update_focused_node: FocusUpdateRequest,
    /// Timers added in callbacks
    pub timers: Option<FastHashMap<TimerId, Timer>>,
    /// Threads added in callbacks
    pub threads: Option<FastHashMap<ThreadId, Thread>>,
    /// Timers removed in callbacks
    pub timers_removed: Option<FastBTreeSet<TimerId>>,
    /// Threads removed in callbacks
    pub threads_removed: Option<FastBTreeSet<ThreadId>>,
    /// Windows created in callbacks
    pub windows_created: Vec<WindowCreateOptions>,
    /// Menus to open (context menus or dropdowns)
    pub menus_to_open: Vec<(Menu, Option<LogicalPosition>)>,
    /// Tooltips to show
    pub tooltips_to_show: Vec<(AzString, LogicalPosition)>,
    /// Whether to hide the currently displayed tooltip
    pub hide_tooltip: bool,
    /// Whether the cursor changed
    pub cursor_changed: bool,
    /// Whether stopPropagation() was called (prevents bubbling up in DOM-style event propagation)
    pub stop_propagation: bool,
    /// Whether preventDefault() was called (prevents default browser behavior)
    pub prevent_default: bool,
}

impl Default for CallCallbacksResult {
    fn default() -> Self {
        Self {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            image_callbacks_changed: None,
            css_properties_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: FocusUpdateRequest::NoChange,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            menus_to_open: Vec::new(),
            tooltips_to_show: Vec::new(),
            hide_tooltip: false,
            cursor_changed: false,
            stop_propagation: false,
            prevent_default: false,
        }
    }
}

impl CallCallbacksResult {
    pub fn cursor_changed(&self) -> bool {
        self.cursor_changed
    }

    pub fn focus_changed(&self) -> bool {
        self.update_focused_node.is_change()
    }
}

impl azul_core::events::CallbackResultRef for CallCallbacksResult {
    fn stop_propagation(&self) -> bool {
        self.stop_propagation
    }

    fn prevent_default(&self) -> bool {
        self.prevent_default
    }

    fn should_regenerate_dom(&self) -> bool {
        use azul_core::callbacks::Update;
        matches!(
            self.callbacks_update_screen,
            Update::RefreshDom | Update::RefreshDomAllWindows
        )
    }
}

/// Menu callback: What data / function pointer should
/// be called when the menu item is clicked?
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct MenuCallback {
    pub callback: Callback,
    pub refany: RefAny,
}

/// Optional MenuCallback
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C, u8)]
pub enum OptionMenuCallback {
    None,
    Some(MenuCallback),
}

impl OptionMenuCallback {
    pub fn into_option(self) -> Option<MenuCallback> {
        match self {
            OptionMenuCallback::None => None,
            OptionMenuCallback::Some(c) => Some(c),
        }
    }

    pub fn is_some(&self) -> bool {
        matches!(self, OptionMenuCallback::Some(_))
    }

    pub fn is_none(&self) -> bool {
        matches!(self, OptionMenuCallback::None)
    }
}

impl From<Option<MenuCallback>> for OptionMenuCallback {
    fn from(o: Option<MenuCallback>) -> Self {
        match o {
            None => OptionMenuCallback::None,
            Some(c) => OptionMenuCallback::Some(c),
        }
    }
}

impl From<OptionMenuCallback> for Option<MenuCallback> {
    fn from(o: OptionMenuCallback) -> Self {
        o.into_option()
    }
}

// -- RenderImage callbacks

/// Callback type that renders an OpenGL texture
///
/// **IMPORTANT**: In azul-core, this is stored as `CoreRenderImageCallbackType = usize`
/// to avoid circular dependencies. The actual function pointer is cast to usize for
/// storage in the data model, then unsafely cast back to this type when invoked.
pub type RenderImageCallbackType = extern "C" fn(RefAny, RenderImageCallbackInfo) -> ImageRef;

/// Callback that returns a rendered OpenGL texture
///
/// **IMPORTANT**: In azul-core, this is stored as `CoreRenderImageCallback` with
/// a `cb: usize` field. When creating callbacks in the data model, function pointers
/// are cast to usize. This type is used in azul-layout where we can safely work
/// with the actual function pointer type.
#[repr(C)]
pub struct RenderImageCallback {
    pub cb: RenderImageCallbackType,
    /// For FFI: stores the foreign callable (e.g., PyFunction)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

impl_callback!(RenderImageCallback, RenderImageCallbackType);

impl RenderImageCallback {
    /// Create a new callback with just a function pointer (for native Rust code)
    pub fn create(cb: RenderImageCallbackType) -> Self {
        Self { cb, ctx: OptionRefAny::None }
    }

    /// Convert from the core crate's `CoreRenderImageCallback` (which stores cb as usize)
    /// back to the layout crate's typed function pointer.
    ///
    /// # Safety
    ///
    /// This is safe because we ensure that the usize in CoreRenderImageCallback
    /// was originally created from a valid RenderImageCallbackType function pointer.
    pub fn from_core(core_callback: &azul_core::callbacks::CoreRenderImageCallback) -> Self {
        Self {
            cb: unsafe { core::mem::transmute(core_callback.cb) },
            ctx: core_callback.ctx.clone(),
        }
    }

    /// Convert to CoreRenderImageCallback (function pointer stored as usize)
    ///
    /// This is always safe - we're just casting the function pointer to usize for storage.
    pub fn to_core(self) -> azul_core::callbacks::CoreRenderImageCallback {
        azul_core::callbacks::CoreRenderImageCallback {
            cb: self.cb as usize,
            ctx: self.ctx,
        }
    }
}

/// Allow RenderImageCallback to be passed to functions expecting `C: Into<CoreRenderImageCallback>`
impl From<RenderImageCallback> for azul_core::callbacks::CoreRenderImageCallback {
    fn from(callback: RenderImageCallback) -> Self {
        callback.to_core()
    }
}

/// Information passed to image rendering callbacks
#[derive(Debug)]
#[repr(C)]
pub struct RenderImageCallbackInfo {
    /// The ID of the DOM node that the ImageCallback was attached to
    callback_node_id: DomNodeId,
    /// Bounds of the laid-out node
    bounds: HidpiAdjustedBounds,
    /// Optional OpenGL context pointer
    gl_context: *const OptionGlContextPtr,
    /// Image cache for looking up images
    image_cache: *const ImageCache,
    /// System font cache
    system_fonts: *const FcFontCache,
    /// Pointer to callable (Python/FFI callback function)
    callable_ptr: *const OptionRefAny,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut core::ffi::c_void,
}

impl Clone for RenderImageCallbackInfo {
    fn clone(&self) -> Self {
        Self {
            callback_node_id: self.callback_node_id,
            bounds: self.bounds,
            gl_context: self.gl_context,
            image_cache: self.image_cache,
            system_fonts: self.system_fonts,
            callable_ptr: self.callable_ptr,
            _abi_mut: self._abi_mut,
        }
    }
}

impl RenderImageCallbackInfo {
    pub fn new<'a>(
        callback_node_id: DomNodeId,
        bounds: HidpiAdjustedBounds,
        gl_context: &'a OptionGlContextPtr,
        image_cache: &'a ImageCache,
        system_fonts: &'a FcFontCache,
    ) -> Self {
        Self {
            callback_node_id,
            bounds,
            gl_context: gl_context as *const OptionGlContextPtr,
            image_cache: image_cache as *const ImageCache,
            system_fonts: system_fonts as *const FcFontCache,
            callable_ptr: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    /// Get the callable for FFI language bindings (Python, etc.)
    pub fn get_ctx(&self) -> OptionRefAny {
        if self.callable_ptr.is_null() {
            OptionRefAny::None
        } else {
            unsafe { (*self.callable_ptr).clone() }
        }
    }

    /// Set the callable pointer (called before invoking callback)
    pub unsafe fn set_callable_ptr(&mut self, ptr: *const OptionRefAny) {
        self.callable_ptr = ptr;
    }

    pub fn get_callback_node_id(&self) -> DomNodeId {
        self.callback_node_id
    }

    pub fn get_bounds(&self) -> HidpiAdjustedBounds {
        self.bounds
    }

    fn internal_get_gl_context<'a>(&'a self) -> &'a OptionGlContextPtr {
        unsafe { &*self.gl_context }
    }

    fn internal_get_image_cache<'a>(&'a self) -> &'a ImageCache {
        unsafe { &*self.image_cache }
    }

    fn internal_get_system_fonts<'a>(&'a self) -> &'a FcFontCache {
        unsafe { &*self.system_fonts }
    }

    pub fn get_gl_context(&self) -> OptionGlContextPtr {
        self.internal_get_gl_context().clone()
    }
}

// ============================================================================
// Result types for FFI
// ============================================================================

/// Result type for functions returning U8Vec or a String error
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum ResultU8VecString {
    Ok(azul_css::U8Vec),
    Err(AzString),
}

impl From<Result<alloc::vec::Vec<u8>, AzString>> for ResultU8VecString {
    fn from(result: Result<alloc::vec::Vec<u8>, AzString>) -> Self {
        match result {
            Ok(v) => ResultU8VecString::Ok(v.into()),
            Err(e) => ResultU8VecString::Err(e),
        }
    }
}

/// Result type for functions returning () or a String error  
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum ResultVoidString {
    Ok,
    Err(AzString),
}

impl From<Result<(), AzString>> for ResultVoidString {
    fn from(result: Result<(), AzString>) -> Self {
        match result {
            Ok(()) => ResultVoidString::Ok,
            Err(e) => ResultVoidString::Err(e),
        }
    }
}

/// Result type for functions returning String or a String error  
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum ResultStringString {
    Ok(AzString),
    Err(AzString),
}

impl From<Result<AzString, AzString>> for ResultStringString {
    fn from(result: Result<AzString, AzString>) -> Self {
        match result {
            Ok(s) => ResultStringString::Ok(s),
            Err(e) => ResultStringString::Err(e),
        }
    }
}

// ============================================================================
// Base64 encoding helper
// ============================================================================

const BASE64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes to Base64 string
fn base64_encode(input: &[u8]) -> alloc::string::String {
    let mut output = alloc::string::String::with_capacity((input.len() + 2) / 3 * 4);
    
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        
        let n = (b0 << 16) | (b1 << 8) | b2;
        
        output.push(BASE64_ALPHABET[(n >> 18) & 0x3F] as char);
        output.push(BASE64_ALPHABET[(n >> 12) & 0x3F] as char);
        
        if chunk.len() > 1 {
            output.push(BASE64_ALPHABET[(n >> 6) & 0x3F] as char);
        } else {
            output.push('=');
        }
        
        if chunk.len() > 2 {
            output.push(BASE64_ALPHABET[n & 0x3F] as char);
        } else {
            output.push('=');
        }
    }
    
    output
}
