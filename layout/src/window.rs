//! Window layout management for solver3/text3
//!
//! This module provides the high-level API for managing layout state across frames,
//! including caching, incremental updates, and display list generation.
//!
//! The main entry point is `LayoutWindow`, which encapsulates all the state needed
//! to perform layout and maintain consistency across window resizes and DOM updates.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicUsize, Ordering},
};

use azul_core::{
    callbacks::{FocusTarget, IFrameCallbackReason, Update},
    dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
    events::EasingFunction,
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
    gl::OptionGlContextPtr,
    gpu::GpuValueCache,
    hit_test::{DocumentId, ScrollPosition, ScrollbarHitId},
    refany::RefAny,
    resources::{
        Epoch, FontKey, GlTextureCache, IdNamespace, ImageCache, ImageRefHash, RendererResources,
    },
    selection::SelectionState,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::{Duration, Instant, SystemTimeDiff, ThreadId, ThreadSendMsg, TimerId},
    window::{RawWindowHandle, RendererType},
    FastBTreeSet, FastHashMap,
};
use azul_css::{parser2::CssApiWrapper, props::basic::FontRef, LayoutDebugMessage};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::{
        CallCallbacksResult, Callback, ExternalSystemCallbacks, FocusUpdateRequest, MenuCallback,
    },
    managers::{
        gpu_state::GpuStateManager,
        iframe::IFrameManager,
        scroll_state::{ScrollManager, ScrollStates},
    },
    solver3::{
        self, cache::LayoutCache as Solver3LayoutCache, display_list::DisplayList,
        layout_tree::LayoutTree,
    },
    text3::{
        cache::{FontManager, LayoutCache as TextLayoutCache, LayoutError},
        default::PathLoader,
    },
    thread::{OptionThreadReceiveMsg, Thread, ThreadReceiveMsg, ThreadWriteBackMsg},
    timer::Timer,
    window_state::{FullWindowState, WindowState},
};

// Global atomic counters for generating unique IDs
static DOCUMENT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
static ID_NAMESPACE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Helper function to create a unique DocumentId
fn new_document_id() -> DocumentId {
    let namespace_id = new_id_namespace();
    let id = DOCUMENT_ID_COUNTER.fetch_add(1, Ordering::Relaxed) as u32;
    DocumentId { namespace_id, id }
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

/// Helper function to create a unique IdNamespace
fn new_id_namespace() -> IdNamespace {
    let id = ID_NAMESPACE_COUNTER.fetch_add(1, Ordering::Relaxed) as u32;
    IdNamespace(id)
}

/// Result of a layout pass for a single DOM, before display list generation
#[derive(Debug)]
pub struct DomLayoutResult {
    /// The styled DOM that was laid out
    pub styled_dom: StyledDom,
    /// The layout tree with computed sizes and positions
    pub layout_tree: LayoutTree<FontRef>,
    /// Absolute positions of all nodes
    pub absolute_positions: BTreeMap<usize, LogicalPosition>,
    /// The viewport used for this layout
    pub viewport: LogicalRect,
    /// The generated display list for this DOM.
    pub display_list: DisplayList,
    /// Stable scroll IDs computed from node_data_hash
    /// Maps layout node index -> external scroll ID
    pub scroll_ids: BTreeMap<usize, u64>,
    /// Mapping from scroll IDs to DOM NodeIds for hit testing
    /// This allows us to map WebRender scroll IDs back to DOM nodes
    pub scroll_id_to_node_id: BTreeMap<u64, NodeId>,
}

/// State for tracking scrollbar drag interaction
#[derive(Debug, Clone)]
pub struct ScrollbarDragState {
    pub hit_id: ScrollbarHitId,
    pub initial_mouse_pos: LogicalPosition,
    pub initial_scroll_offset: LogicalPosition,
}

/// A window-level layout manager that encapsulates all layout state and caching.
///
/// This struct owns the layout and text caches, and provides methods to:
/// - Perform initial layout
/// - Incrementally update layout on DOM changes
/// - Generate display lists for rendering
/// - Handle window resizes efficiently
/// - Manage multiple DOMs (for IFrames)
pub struct LayoutWindow {
    /// Layout cache for solver3 (incremental layout tree) - for the root DOM
    pub layout_cache: Solver3LayoutCache<FontRef>,
    /// Text layout cache for text3 (shaped glyphs, line breaks, etc.)
    pub text_cache: TextLayoutCache<FontRef>,
    /// Font manager for loading and caching fonts
    pub font_manager: FontManager<FontRef, PathLoader>,
    /// Cache to store decoded images
    pub image_cache: ImageCache,
    /// Cached layout results for all DOMs (root + iframes)
    pub layout_results: BTreeMap<DomId, DomLayoutResult>,
    /// Scroll state manager for all nodes across all DOMs
    pub scroll_states: ScrollManager,
    /// Gesture and drag manager for multi-frame interactions (moved from FullWindowState)
    pub gesture_drag_manager: crate::managers::gesture::GestureAndDragManager,
    /// Focus manager for keyboard focus and tab navigation
    pub focus_manager: crate::managers::focus_cursor::FocusManager,
    /// File drop manager for cursor state and file drag-drop
    pub file_drop_manager: crate::managers::file_drop::FileDropManager,
    /// Selection manager for text selections across all DOMs
    pub selection_manager: crate::managers::selection::SelectionManager,
    /// Drag-drop manager for node and file dragging operations
    pub drag_drop_manager: crate::managers::drag_drop::DragDropManager,
    /// Hover manager for tracking hit test history over multiple frames
    pub hover_manager: crate::managers::hover::HoverManager,
    /// IFrame manager for all nodes across all DOMs
    pub iframe_manager: IFrameManager,
    /// GPU state manager for all nodes across all DOMs
    pub gpu_state_manager: GpuStateManager,
    /// Accessibility manager for screen reader support
    pub a11y_manager: crate::managers::a11y::A11yManager,
    /// Timers associated with this window
    pub timers: BTreeMap<TimerId, Timer>,
    /// Threads running in the background for this window
    pub threads: BTreeMap<ThreadId, Thread>,
    /// Currently loaded fonts and images present in this renderer (window)
    pub renderer_resources: RendererResources,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub renderer_type: Option<RendererType>,
    /// Windows state of the window of (current frame - 1): initialized to None on startup
    pub previous_window_state: Option<FullWindowState>,
    /// Window state of this current window (current frame): initialized to the state of
    /// WindowCreateOptions
    pub current_window_state: FullWindowState,
    /// A "document" in WebRender usually corresponds to one tab (i.e. in Azuls case, the whole
    /// window).
    pub document_id: DocumentId,
    /// ID namespace under which every font / image for this window is registered
    pub id_namespace: IdNamespace,
    /// The "epoch" is a frame counter, to remove outdated images, fonts and OpenGL textures when
    /// they're not in use anymore.
    pub epoch: Epoch,
    /// Currently GL textures inside the active CachedDisplayList
    pub gl_texture_cache: GlTextureCache,
    /// State for tracking scrollbar drag interaction
    currently_dragging_thumb: Option<ScrollbarDragState>,
}

fn default_duration_500ms() -> Duration {
    Duration::System(SystemTimeDiff::from_millis(500))
}

fn default_duration_200ms() -> Duration {
    Duration::System(SystemTimeDiff::from_millis(200))
}

impl LayoutWindow {
    /// Create a new layout window with empty caches.
    ///
    /// For full initialization with WindowInternal compatibility, use `new_full()`.
    pub fn new(fc_cache: FcFontCache) -> Result<Self, crate::solver3::LayoutError> {
        Ok(Self {
            layout_cache: Solver3LayoutCache {
                tree: None,
                absolute_positions: BTreeMap::new(),
                viewport: None,
                scroll_ids: BTreeMap::new(),
                scroll_id_to_node_id: BTreeMap::new(),
            },
            text_cache: TextLayoutCache::new(),
            font_manager: FontManager::new(fc_cache)?,
            image_cache: ImageCache::default(),
            layout_results: BTreeMap::new(),
            scroll_states: ScrollManager::new(),
            gesture_drag_manager: crate::managers::gesture::GestureAndDragManager::new(),
            focus_manager: crate::managers::focus_cursor::FocusManager::new(),
            file_drop_manager: crate::managers::file_drop::FileDropManager::new(),
            selection_manager: crate::managers::selection::SelectionManager::new(),
            drag_drop_manager: crate::managers::drag_drop::DragDropManager::new(),
            hover_manager: crate::managers::hover::HoverManager::new(),
            iframe_manager: IFrameManager::new(),
            gpu_state_manager: GpuStateManager::new(
                default_duration_500ms(),
                default_duration_200ms(),
            ),
            a11y_manager: crate::managers::a11y::A11yManager::new(),
            timers: BTreeMap::new(),
            threads: BTreeMap::new(),
            renderer_resources: RendererResources::default(),
            renderer_type: None,
            previous_window_state: None,
            current_window_state: FullWindowState::default(),
            document_id: new_document_id(),
            id_namespace: new_id_namespace(),
            epoch: Epoch::new(),
            gl_texture_cache: GlTextureCache::default(),
            currently_dragging_thumb: None,
        })
    }

    /// Perform layout on a styled DOM and generate a display list.
    ///
    /// This is the main entry point for layout. It handles:
    /// - Incremental layout updates using the cached layout tree
    /// - Text shaping and line breaking
    /// - IFrame callback invocation and recursive layout
    /// - Display list generation for rendering
    /// - Accessibility tree synchronization
    ///
    /// # Arguments
    /// - `styled_dom`: The styled DOM to layout
    /// - `window_state`: Current window dimensions and state
    /// - `renderer_resources`: Resources for image sizing etc.
    /// - `debug_messages`: Optional vector to collect debug/warning messages
    ///
    /// # Returns
    /// The display list ready for rendering, or an error if layout fails.
    pub fn layout_and_generate_display_list(
        &mut self,
        root_dom: StyledDom,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<(), solver3::LayoutError> {
        // Clear previous results for a full relayout
        self.layout_results.clear();

        // Start recursive layout from the root DOM
        let result = self.layout_dom_recursive(
            root_dom,
            window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        );

        // After successful layout, update the accessibility tree
        #[cfg(feature = "accessibility")]
        if result.is_ok() {
            let _tree_update = crate::managers::a11y::A11yManager::update_tree(
                self.a11y_manager.root_id,
                &self.layout_results,
                &self.current_window_state.title,
                self.current_window_state.size.dimensions,
            );
            // TODO: Pass tree_update to platform adapter
            // The adapter will submit it to the OS accessibility API
        }

        result
    }

    fn layout_dom_recursive(
        &mut self,
        mut styled_dom: StyledDom,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<(), solver3::LayoutError> {
        if styled_dom.dom_id.inner == 0 {
            styled_dom.dom_id = DomId::ROOT_ID;
        }
        let dom_id = styled_dom.dom_id;

        let viewport = LogicalRect {
            origin: LogicalPosition::zero(),
            size: window_state.size.dimensions,
        };

        let scroll_offsets = self.scroll_states.get_scroll_states_for_dom(dom_id);
        let styled_dom_clone = styled_dom.clone();
        let gpu_cache = self.gpu_state_manager.get_or_create_cache(dom_id).clone();

        let mut display_list = solver3::layout_document(
            &mut self.layout_cache,
            &mut self.text_cache,
            styled_dom,
            viewport,
            &self.font_manager,
            &scroll_offsets,
            &self.selection_manager.selections,
            debug_messages,
            Some(&gpu_cache),
            dom_id,
        )?;

        let tree = self
            .layout_cache
            .tree
            .clone()
            .ok_or(solver3::LayoutError::InvalidTree)?;

        // Get scroll IDs from cache (they were computed during layout_document)
        let scroll_ids = self.layout_cache.scroll_ids.clone();
        let scroll_id_to_node_id = self.layout_cache.scroll_id_to_node_id.clone();

        // Synchronize scrollbar transforms AFTER layout
        self.gpu_state_manager
            .update_scrollbar_transforms(dom_id, &self.scroll_states, &tree);

        // Scan for IFrames *after* the initial layout pass
        let iframes = self.scan_for_iframes(dom_id, &tree, &self.layout_cache.absolute_positions);

        for (node_id, bounds) in iframes {
            if let Some(child_dom_id) = self.invoke_iframe_callback(
                dom_id,
                node_id,
                bounds,
                window_state,
                renderer_resources,
                system_callbacks,
                debug_messages,
            ) {
                // Insert an IFrame primitive that the renderer will use
                display_list
                    .items
                    .push(crate::solver3::display_list::DisplayListItem::IFrame {
                        child_dom_id,
                        bounds,
                        clip_rect: bounds,
                    });
            }
        }

        // Store the final layout result for this DOM
        self.layout_results.insert(
            dom_id,
            DomLayoutResult {
                styled_dom: styled_dom_clone,
                layout_tree: tree,
                absolute_positions: self.layout_cache.absolute_positions.clone(),
                viewport,
                display_list,
                scroll_ids,
                scroll_id_to_node_id,
            },
        );

        Ok(())
    }

    fn scan_for_iframes(
        &self,
        dom_id: DomId,
        layout_tree: &LayoutTree<FontRef>,
        absolute_positions: &BTreeMap<usize, LogicalPosition>,
    ) -> Vec<(NodeId, LogicalRect)> {
        use azul_core::dom::NodeType;
        layout_tree
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(idx, node)| {
                let node_dom_id = node.dom_node_id?;
                let layout_result = self.layout_results.get(&dom_id)?;
                let node_data = &layout_result.styled_dom.node_data.as_container()[node_dom_id];
                if matches!(node_data.get_node_type(), NodeType::IFrame(_)) {
                    let pos = absolute_positions.get(&idx).copied().unwrap_or_default();
                    let size = node.used_size.unwrap_or_default();
                    Some((node_dom_id, LogicalRect::new(pos, size)))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Handle a window resize by updating the cached layout.
    ///
    /// This method leverages solver3's incremental layout system to efficiently
    /// relayout only the affected parts of the tree when the window size changes.
    ///
    /// Returns the new display list after the resize.
    pub fn resize_window(
        &mut self,
        styled_dom: StyledDom,
        new_size: LogicalSize,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<DisplayList, crate::solver3::LayoutError> {
        // Create a temporary FullWindowState with the new size
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = new_size;

        let dom_id = styled_dom.dom_id;

        // Reuse the main layout method - solver3 will detect the viewport
        // change and invalidate only what's necessary
        self.layout_and_generate_display_list(
            styled_dom,
            &window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        )?;

        // Retrieve the display list from the layout result
        // We need to take ownership of the display list, so we replace it with an empty one
        self.layout_results
            .get_mut(&dom_id)
            .map(|result| {
                std::mem::replace(&mut result.display_list, DisplayList { items: Vec::new() })
            })
            .ok_or(solver3::LayoutError::InvalidTree)
    }

    /// Clear all caches (useful for testing or when switching documents).
    pub fn clear_caches(&mut self) {
        self.layout_cache = Solver3LayoutCache {
            tree: None,
            absolute_positions: BTreeMap::new(),
            viewport: None,
            scroll_ids: BTreeMap::new(),
            scroll_id_to_node_id: BTreeMap::new(),
        };
        self.text_cache = TextLayoutCache::new();
        self.layout_results.clear();
        self.scroll_states = ScrollManager::new();
        self.selection_manager.clear_all();
    }

    /// Set scroll position for a node
    pub fn set_scroll_position(&mut self, dom_id: DomId, node_id: NodeId, scroll: ScrollPosition) {
        // Convert ScrollPosition to the internal representation
        #[cfg(feature = "std")]
        let now = Instant::System(std::time::Instant::now().into());
        #[cfg(not(feature = "std"))]
        let now = Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });

        self.scroll_states.update_node_bounds(
            dom_id,
            node_id,
            scroll.parent_rect,
            scroll.children_rect,
            now.clone(),
        );
        self.scroll_states
            .set_scroll_position(dom_id, node_id, scroll.children_rect.origin, now);
    }

    /// Get scroll position for a node
    pub fn get_scroll_position(&self, dom_id: DomId, node_id: NodeId) -> Option<ScrollPosition> {
        let states = self.scroll_states.get_scroll_states_for_dom(dom_id);
        states.get(&node_id).cloned()
    }

    /// Set selection state for a DOM
    pub fn set_selection(&mut self, dom_id: DomId, selection: SelectionState) {
        self.selection_manager.set_selection(dom_id, selection);
    }

    /// Get selection state for a DOM
    pub fn get_selection(&self, dom_id: DomId) -> Option<&SelectionState> {
        self.selection_manager.get_selection(&dom_id)
    }

    /// Invoke an IFrame callback and perform layout on the returned DOM.
    ///
    /// This is the entry point that looks up the necessary `IFrameNode` data before
    /// delegating to the core implementation logic.
    fn invoke_iframe_callback(
        &mut self,
        parent_dom_id: DomId,
        node_id: NodeId,
        bounds: LogicalRect,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Option<DomId> {
        eprintln!(
            "DEBUG: invoke_iframe_callback called for node {:?}",
            node_id
        );

        // Get the layout result for the parent DOM to access its styled_dom
        let layout_result = self.layout_results.get(&parent_dom_id)?;
        eprintln!(
            "DEBUG: Got layout result for parent DOM {:?}",
            parent_dom_id
        );

        // Get the node data for the IFrame element
        let node_data_container = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_container.get(node_id)?;
        eprintln!("DEBUG: Got node data at index {}", node_id.index());

        // Extract the IFrame node, cloning it to avoid borrow checker issues
        let iframe_node = match node_data.get_node_type() {
            NodeType::IFrame(iframe) => {
                eprintln!("DEBUG: Node is IFrame type");
                iframe.clone()
            }
            other => {
                eprintln!("DEBUG: Node is NOT IFrame, type = {:?}", other);
                return None;
            }
        };

        // Call the actual implementation with all necessary data
        self.invoke_iframe_callback_impl(
            parent_dom_id,
            node_id,
            &iframe_node,
            bounds,
            window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        )
    }

    /// Core implementation for invoking an IFrame callback and managing the recursive layout.
    ///
    /// This method implements the 5 conditional re-invocation rules by coordinating
    /// with the `IFrameManager` and `ScrollManager`.
    ///
    /// # Returns
    ///
    /// `Some(child_dom_id)` if the callback was invoked and the child DOM was laid out.
    /// The parent's display list generator will then use this ID to reference the child's
    /// display list. Returns `None` if the callback was not invoked.
    fn invoke_iframe_callback_impl(
        &mut self,
        parent_dom_id: DomId,
        node_id: NodeId,
        iframe_node: &azul_core::dom::IFrameNode,
        bounds: LogicalRect,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Option<DomId> {
        // Get current time from system callbacks for state updates
        let now = (system_callbacks.get_system_time_fn.cb)();

        // Update node bounds in the scroll manager. This is necessary for the IFrameManager
        // to correctly detect edge scroll conditions.
        self.scroll_states.update_node_bounds(
            parent_dom_id,
            node_id,
            bounds,
            LogicalRect::new(LogicalPosition::zero(), bounds.size), // Initial content_rect
            now.clone(),
        );

        // Check with the IFrameManager to see if re-invocation is necessary.
        // It handles all 5 conditional rules.
        let reason = match self.iframe_manager.check_reinvoke(
            parent_dom_id,
            node_id,
            &self.scroll_states,
            bounds,
        ) {
            Some(r) => r,
            None => {
                // No re-invocation needed, but we still need the child_dom_id for the display list.
                return self
                    .iframe_manager
                    .get_nested_dom_id(parent_dom_id, node_id);
            }
        };

        eprintln!(
            "DEBUG: IFrame ({:?}, {:?}) - Reason: {:?}",
            parent_dom_id, node_id, reason
        );

        let scroll_offset = self
            .scroll_states
            .get_current_offset(parent_dom_id, node_id)
            .unwrap_or_default();
        let hidpi_factor = window_state.size.get_hidpi_factor();

        // Create IFrameCallbackInfo with the most up-to-date state
        let mut callback_info = azul_core::callbacks::IFrameCallbackInfo::new(
            reason,
            &self.font_manager.fc_cache,
            &self.image_cache,
            window_state.theme,
            azul_core::callbacks::HidpiAdjustedBounds {
                logical_size: bounds.size,
                hidpi_factor,
            },
            bounds.size,
            scroll_offset,
            bounds.size,
            LogicalPosition::zero(),
        );

        // Clone the user data for the callback
        let mut callback_data = iframe_node.data.clone();

        // Invoke the user's IFrame callback
        let callback_return = (iframe_node.callback.cb)(&mut callback_data, &mut callback_info);

        // Mark the IFrame as invoked to prevent duplicate InitialRender calls
        self.iframe_manager
            .mark_invoked(parent_dom_id, node_id, reason);

        // Get the child StyledDom from the callback's return value
        let mut child_styled_dom = match callback_return.dom {
            azul_core::styled_dom::OptionStyledDom::Some(dom) => dom,
            azul_core::styled_dom::OptionStyledDom::None => {
                // If the callback returns None, it's an optimization hint.
                if reason == IFrameCallbackReason::InitialRender {
                    // For the very first render, create an empty div as a fallback.
                    let mut empty_dom = Dom::div();
                    let empty_css = CssApiWrapper::empty();
                    empty_dom.style(empty_css)
                } else {
                    // For subsequent calls, returning None means "keep the old DOM".
                    // We just need to update the scroll info and return the existing child ID.
                    self.iframe_manager.update_iframe_info(
                        parent_dom_id,
                        node_id,
                        callback_return.scroll_size,
                        callback_return.virtual_scroll_size,
                    );
                    return self
                        .iframe_manager
                        .get_nested_dom_id(parent_dom_id, node_id);
                }
            }
        };

        // Get or create a unique DomId for the IFrame's content
        let child_dom_id = self
            .iframe_manager
            .get_or_create_nested_dom_id(parent_dom_id, node_id);
        child_styled_dom.dom_id = child_dom_id;

        // Update the IFrameManager with the new scroll sizes from the callback
        self.iframe_manager.update_iframe_info(
            parent_dom_id,
            node_id,
            callback_return.scroll_size,
            callback_return.virtual_scroll_size,
        );

        // **RECURSIVE LAYOUT STEP**
        // Perform a full layout pass on the child DOM. This will recursively handle
        // any IFrames within this IFrame.
        self.layout_dom_recursive(
            child_styled_dom,
            window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        )
        .ok()?;

        Some(child_dom_id)
    }

    // Query methods for callbacks

    /// Get the size of a laid-out node
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let layout_node = layout_result.layout_tree.get(nid.index())?;
        layout_node.used_size
    }

    /// Get the position of a laid-out node
    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<LogicalPosition> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let position = layout_result.absolute_positions.get(&nid.index())?;
        Some(*position)
    }

    /// Get the parent of a node
    pub fn get_parent(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let parent_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .parent_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(parent_id)),
        })
    }

    /// Get the first child of a node
    pub fn get_first_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hierarchy_item = node_hierarchy.get(nid)?;
        let first_child_id = hierarchy_item.first_child_id(nid)?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(first_child_id)),
        })
    }

    /// Get the next sibling of a node
    pub fn get_next_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let next_sibling_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .next_sibling_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(next_sibling_id)),
        })
    }

    /// Get the previous sibling of a node
    pub fn get_previous_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let prev_sibling_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .previous_sibling_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(prev_sibling_id)),
        })
    }

    /// Get the last child of a node
    pub fn get_last_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let last_child_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .last_child_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(last_child_id)),
        })
    }

    /// Scan all fonts used in this LayoutWindow (for resource GC)
    pub fn scan_used_fonts(&self) -> BTreeSet<FontKey> {
        let mut fonts = BTreeSet::new();
        for (_dom_id, layout_result) in &self.layout_results {
            // TODO: Scan styled_dom for font references
            // This requires accessing the CSS property cache and finding all font-family properties
        }
        fonts
    }

    /// Scan all images used in this LayoutWindow (for resource GC)
    pub fn scan_used_images(&self, _css_image_cache: &ImageCache) -> BTreeSet<ImageRefHash> {
        let mut images = BTreeSet::new();
        for (_dom_id, layout_result) in &self.layout_results {
            // TODO: Scan styled_dom for image references
            // This requires scanning background-image and content properties
        }
        images
    }

    /// Helper function to convert ScrollStates to nested format for CallbackInfo
    fn get_nested_scroll_states(
        &self,
        dom_id: DomId,
    ) -> BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> {
        let mut nested = BTreeMap::new();
        let scroll_states = self.scroll_states.get_scroll_states_for_dom(dom_id);
        let mut inner = BTreeMap::new();
        for (node_id, scroll_pos) in scroll_states {
            inner.insert(
                NodeHierarchyItemId::from_crate_internal(Some(node_id)),
                scroll_pos,
            );
        }
        nested.insert(dom_id, inner);
        nested
    }

    // ===== Timer Management =====

    /// Add a timer to this window
    pub fn add_timer(&mut self, timer_id: TimerId, timer: Timer) {
        self.timers.insert(timer_id, timer);
    }

    /// Remove a timer from this window
    pub fn remove_timer(&mut self, timer_id: &TimerId) -> Option<Timer> {
        self.timers.remove(timer_id)
    }

    /// Get a reference to a timer
    pub fn get_timer(&self, timer_id: &TimerId) -> Option<&Timer> {
        self.timers.get(timer_id)
    }

    /// Get a mutable reference to a timer
    pub fn get_timer_mut(&mut self, timer_id: &TimerId) -> Option<&mut Timer> {
        self.timers.get_mut(timer_id)
    }

    /// Get all timer IDs
    pub fn get_timer_ids(&self) -> Vec<TimerId> {
        self.timers.keys().copied().collect()
    }

    /// Tick all timers (called once per frame)
    /// Returns a list of timer IDs that are ready to run
    pub fn tick_timers(&mut self, current_time: azul_core::task::Instant) -> Vec<TimerId> {
        let mut ready_timers = Vec::new();

        for (timer_id, timer) in &mut self.timers {
            // Check if timer is ready to run
            // This logic should match the timer's internal state
            // For now, we'll just collect all timer IDs
            // The actual readiness check will be done when invoking
            ready_timers.push(*timer_id);
        }

        ready_timers
    }

    // ===== Thread Management =====

    /// Add a thread to this window
    pub fn add_thread(&mut self, thread_id: ThreadId, thread: Thread) {
        self.threads.insert(thread_id, thread);
    }

    /// Remove a thread from this window
    pub fn remove_thread(&mut self, thread_id: &ThreadId) -> Option<Thread> {
        self.threads.remove(thread_id)
    }

    /// Get a reference to a thread
    pub fn get_thread(&self, thread_id: &ThreadId) -> Option<&Thread> {
        self.threads.get(thread_id)
    }

    /// Get a mutable reference to a thread
    pub fn get_thread_mut(&mut self, thread_id: &ThreadId) -> Option<&mut Thread> {
        self.threads.get_mut(thread_id)
    }

    /// Get all thread IDs
    pub fn get_thread_ids(&self) -> Vec<ThreadId> {
        self.threads.keys().copied().collect()
    }

    // ===== GPU Value Cache Management =====

    /// Get the GPU value cache for a specific DOM
    pub fn get_gpu_cache(&self, dom_id: &DomId) -> Option<&GpuValueCache> {
        self.gpu_state_manager.caches.get(dom_id)
    }

    /// Get a mutable reference to the GPU value cache for a specific DOM
    pub fn get_gpu_cache_mut(&mut self, dom_id: &DomId) -> Option<&mut GpuValueCache> {
        self.gpu_state_manager.caches.get_mut(dom_id)
    }

    /// Get or create a GPU value cache for a specific DOM
    pub fn get_or_create_gpu_cache(&mut self, dom_id: DomId) -> &mut GpuValueCache {
        self.gpu_state_manager
            .caches
            .entry(dom_id)
            .or_insert_with(GpuValueCache::default)
    }

    // ===== Layout Result Access =====

    /// Get a layout result for a specific DOM
    pub fn get_layout_result(&self, dom_id: &DomId) -> Option<&DomLayoutResult> {
        self.layout_results.get(dom_id)
    }

    /// Get a mutable layout result for a specific DOM
    pub fn get_layout_result_mut(&mut self, dom_id: &DomId) -> Option<&mut DomLayoutResult> {
        self.layout_results.get_mut(dom_id)
    }

    /// Get all DOM IDs that have layout results
    pub fn get_dom_ids(&self) -> Vec<DomId> {
        self.layout_results.keys().copied().collect()
    }

    // ===== Hit-Test Computation =====

    /// Compute the cursor type hit-test from a full hit-test
    ///
    /// This determines which mouse cursor to display based on the CSS cursor
    /// properties of the hovered nodes.
    pub fn compute_cursor_type_hit_test(
        &self,
        hit_test: &crate::hit_test::FullHitTest,
    ) -> crate::hit_test::CursorTypeHitTest {
        crate::hit_test::CursorTypeHitTest::new(hit_test, self)
    }

    // TODO: Implement compute_hit_test() once we have the actual hit-testing logic
    // This would involve:
    // 1. Converting screen coordinates to layout coordinates
    // 2. Traversing the layout tree to find nodes under the cursor
    // 3. Handling z-index and stacking contexts
    // 4. Building the FullHitTest structure

    /// Synchronize scrollbar opacity values with the GPU value cache.
    ///
    /// This method updates GPU opacity keys for all scrollbars based on scroll activity
    /// tracked by the ScrollManager. It enables smooth scrollbar fading without
    /// requiring display list regeneration.
    ///
    /// # Arguments
    ///
    /// * `dom_id` - The DOM to synchronize scrollbar opacity for
    /// * `layout_tree` - The layout tree containing scrollbar information
    /// * `now` - Current timestamp for calculating fade progress
    /// * `fade_delay` - Delay before scrollbar starts fading (e.g., 500ms)
    /// * `fade_duration` - Duration of the fade animation (e.g., 200ms)
    ///
    /// # Returns
    ///
    /// A vector of GPU scrollbar opacity change events

    /// Helper function to calculate scrollbar opacity based on activity time
    fn calculate_scrollbar_opacity(
        last_activity: Option<Instant>,
        now: Instant,
        fade_delay: Duration,
        fade_duration: Duration,
    ) -> f32 {
        let Some(last_activity) = last_activity else {
            return 0.0;
        };

        let time_since_activity = now.duration_since(&last_activity);

        // Phase 1: Scrollbar stays fully visible during fade_delay
        if time_since_activity.div(&fade_delay) < 1.0 {
            return 1.0;
        }

        // Phase 2: Fade out over fade_duration
        let time_into_fade_ms = time_since_activity.div(&fade_delay) - 1.0;
        let fade_progress = (time_into_fade_ms * fade_duration.div(&fade_duration)).min(1.0);

        // Phase 3: Fully faded
        (1.0 - fade_progress).max(0.0)
    }

    /// Synchronize scrollbar opacity values with the GPU value cache.
    ///
    /// Static method that takes individual components instead of &mut self to avoid borrow
    /// conflicts.
    pub fn synchronize_scrollbar_opacity(
        gpu_state_manager: &mut GpuStateManager,
        scroll_states: &ScrollManager,
        dom_id: DomId,
        layout_tree: &LayoutTree<FontRef>,
        system_callbacks: &ExternalSystemCallbacks,
        fade_delay: azul_core::task::Duration,
        fade_duration: azul_core::task::Duration,
    ) -> Vec<azul_core::gpu::GpuScrollbarOpacityEvent> {
        use azul_core::{gpu::GpuScrollbarOpacityEvent, resources::OpacityKey};

        let mut events = Vec::new();
        let gpu_cache = gpu_state_manager.caches.entry(dom_id).or_default();

        // Get current time from system callbacks
        let now = (system_callbacks.get_system_time_fn.cb)();

        // Iterate over all nodes with scrollbar info
        for (node_idx, node) in layout_tree.nodes.iter().enumerate() {
            // Check if node needs scrollbars
            let scrollbar_info = match &node.scrollbar_info {
                Some(info) => info,
                None => continue,
            };

            let node_id = match node.dom_node_id {
                Some(nid) => nid,
                None => continue, // Skip anonymous boxes
            };

            // Calculate current opacity from ScrollManager
            let vertical_opacity = if scrollbar_info.needs_vertical {
                Self::calculate_scrollbar_opacity(
                    scroll_states.get_last_activity_time(dom_id, node_id),
                    now.clone(),
                    fade_delay,
                    fade_duration,
                )
            } else {
                0.0
            };

            let horizontal_opacity = if scrollbar_info.needs_horizontal {
                Self::calculate_scrollbar_opacity(
                    scroll_states.get_last_activity_time(dom_id, node_id),
                    now.clone(),
                    fade_delay,
                    fade_duration,
                )
            } else {
                0.0
            };

            // Handle vertical scrollbar
            if scrollbar_info.needs_vertical && vertical_opacity > 0.001 {
                let key = (dom_id, node_id);
                let existing = gpu_cache.scrollbar_v_opacity_values.get(&key);

                match existing {
                    None => {
                        let opacity_key = OpacityKey::unique();
                        gpu_cache.scrollbar_v_opacity_keys.insert(key, opacity_key);
                        gpu_cache
                            .scrollbar_v_opacity_values
                            .insert(key, vertical_opacity);
                        events.push(GpuScrollbarOpacityEvent::VerticalAdded(
                            dom_id,
                            node_id,
                            opacity_key,
                            vertical_opacity,
                        ));
                    }
                    Some(&old_opacity) if (old_opacity - vertical_opacity).abs() > 0.001 => {
                        let opacity_key = gpu_cache.scrollbar_v_opacity_keys[&key];
                        gpu_cache
                            .scrollbar_v_opacity_values
                            .insert(key, vertical_opacity);
                        events.push(GpuScrollbarOpacityEvent::VerticalChanged(
                            dom_id,
                            node_id,
                            opacity_key,
                            old_opacity,
                            vertical_opacity,
                        ));
                    }
                    _ => {}
                }
            } else {
                // Remove if scrollbar no longer needed or fully transparent
                let key = (dom_id, node_id);
                if let Some(opacity_key) = gpu_cache.scrollbar_v_opacity_keys.remove(&key) {
                    gpu_cache.scrollbar_v_opacity_values.remove(&key);
                    events.push(GpuScrollbarOpacityEvent::VerticalRemoved(
                        dom_id,
                        node_id,
                        opacity_key,
                    ));
                }
            }

            // Handle horizontal scrollbar (same logic)
            if scrollbar_info.needs_horizontal && horizontal_opacity > 0.001 {
                let key = (dom_id, node_id);
                let existing = gpu_cache.scrollbar_h_opacity_values.get(&key);

                match existing {
                    None => {
                        let opacity_key = OpacityKey::unique();
                        gpu_cache.scrollbar_h_opacity_keys.insert(key, opacity_key);
                        gpu_cache
                            .scrollbar_h_opacity_values
                            .insert(key, horizontal_opacity);
                        events.push(GpuScrollbarOpacityEvent::HorizontalAdded(
                            dom_id,
                            node_id,
                            opacity_key,
                            horizontal_opacity,
                        ));
                    }
                    Some(&old_opacity) if (old_opacity - horizontal_opacity).abs() > 0.001 => {
                        let opacity_key = gpu_cache.scrollbar_h_opacity_keys[&key];
                        gpu_cache
                            .scrollbar_h_opacity_values
                            .insert(key, horizontal_opacity);
                        events.push(GpuScrollbarOpacityEvent::HorizontalChanged(
                            dom_id,
                            node_id,
                            opacity_key,
                            old_opacity,
                            horizontal_opacity,
                        ));
                    }
                    _ => {}
                }
            } else {
                // Remove if scrollbar no longer needed or fully transparent
                let key = (dom_id, node_id);
                if let Some(opacity_key) = gpu_cache.scrollbar_h_opacity_keys.remove(&key) {
                    gpu_cache.scrollbar_h_opacity_values.remove(&key);
                    events.push(GpuScrollbarOpacityEvent::HorizontalRemoved(
                        dom_id,
                        node_id,
                        opacity_key,
                    ));
                }
            }
        }

        events
    }

    /// Compute stable scroll IDs for all scrollable nodes in a layout tree
    ///
    /// This should be called after layout but before display list generation.
    /// It creates stable IDs based on node_data_hash that persist across frames.
    ///
    /// Returns:
    /// - scroll_ids: Map from layout node index -> external scroll ID
    /// - scroll_id_to_node_id: Map from scroll ID -> DOM NodeId (for hit testing)
    pub fn compute_scroll_ids<T: crate::text3::cache::ParsedFontTrait>(
        layout_tree: &LayoutTree<T>,
        styled_dom: &azul_core::styled_dom::StyledDom,
    ) -> (BTreeMap<usize, u64>, BTreeMap<u64, NodeId>) {
        use azul_css::props::layout::LayoutOverflow;

        use crate::solver3::getters::{get_overflow_x, get_overflow_y};

        let mut scroll_ids = BTreeMap::new();
        let mut scroll_id_to_node_id = BTreeMap::new();

        // Iterate through all layout nodes
        for (layout_idx, node) in layout_tree.nodes.iter().enumerate() {
            let Some(dom_node_id) = node.dom_node_id else {
                continue;
            };

            // Get the node state
            let styled_node_state = styled_dom
                .styled_nodes
                .as_container()
                .get(dom_node_id)
                .map(|n| n.state.clone())
                .unwrap_or_default();

            // Check if this node has scroll overflow
            let overflow_x = get_overflow_x(styled_dom, dom_node_id, &styled_node_state);
            let overflow_y = get_overflow_y(styled_dom, dom_node_id, &styled_node_state);

            let is_scrollable = matches!(overflow_x, LayoutOverflow::Scroll | LayoutOverflow::Auto)
                || matches!(overflow_y, LayoutOverflow::Scroll | LayoutOverflow::Auto);

            if !is_scrollable {
                continue;
            }

            // Generate stable scroll ID from node_data_hash
            // Use node_data_hash to create a stable ID that persists across frames
            let scroll_id = node.node_data_hash;

            scroll_ids.insert(layout_idx, scroll_id);
            scroll_id_to_node_id.insert(scroll_id, dom_node_id);
        }

        (scroll_ids, scroll_id_to_node_id)
    }

    /// Get the layout rectangle for a specific DOM node in logical coordinates
    ///
    /// This is useful in callbacks to get the position and size of the hit node
    /// for positioning menus, tooltips, or other overlays.
    ///
    /// Returns None if the node is not currently laid out (e.g., display:none)
    pub fn get_node_layout_rect(
        &self,
        node_id: azul_core::dom::DomNodeId,
    ) -> Option<azul_core::geom::LogicalRect> {
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

        // Get the layout tree from cache
        let layout_tree = self.layout_cache.tree.as_ref()?;

        // Find the layout node index corresponding to this DOM node
        // Convert NodeHierarchyItemId to Option<NodeId> for comparison
        let target_node_id = node_id.node.into_crate_internal();
        let layout_idx = layout_tree
            .nodes
            .iter()
            .position(|node| node.dom_node_id == target_node_id)?;

        // Get the absolute position from cache
        let abs_pos = self.layout_cache.absolute_positions.get(&layout_idx)?;

        // Get the layout node for size information
        let layout_node = layout_tree.nodes.get(layout_idx)?;

        // Get the used size (the actual laid-out size)
        let used_size = layout_node.used_size?;

        // Convert to logical coordinates
        let hidpi_factor = self
            .current_window_state
            .size
            .get_hidpi_factor()
            .inner
            .get();

        Some(LogicalRect::new(
            LogicalPosition::new(
                abs_pos.x as f32 / hidpi_factor,
                abs_pos.y as f32 / hidpi_factor,
            ),
            LogicalSize::new(
                used_size.width / hidpi_factor,
                used_size.height / hidpi_factor,
            ),
        ))
    }
}

/// Result of a layout operation
pub struct LayoutResult {
    pub display_list: DisplayList,
    pub warnings: Vec<String>,
}

impl LayoutResult {
    pub fn new(display_list: DisplayList, warnings: Vec<String>) -> Self {
        Self {
            display_list,
            warnings,
        }
    }
}

impl LayoutWindow {
    /// Runs a single timer, similar to CallbacksOfHitTest.call()
    ///
    /// NOTE: The timer has to be selected first by the calling code and verified
    /// that it is ready to run
    #[cfg(feature = "std")]
    pub fn run_single_timer(
        &mut self,
        timer_id: usize,
        frame_start: Instant,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_style: std::sync::Arc<azul_css::system::SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use std::collections::BTreeMap;

        use azul_core::{callbacks::Update, task::TerminateTimer, FastBTreeSet, FastHashMap};

        use crate::callbacks::{CallCallbacksResult, CallbackInfo};

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: FocusUpdateRequest::NoChange,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            cursor_changed: false,
            stop_propagation: false,
            prevent_default: false,
        };

        let mut ret_modified_window_state: WindowState = current_window_state.clone().into();
        let ret_window_state = ret_modified_window_state.clone();
        let mut ret_timers = FastHashMap::new();
        let mut ret_timers_removed = FastBTreeSet::new();
        let mut ret_threads = FastHashMap::new();
        let mut ret_threads_removed = FastBTreeSet::new();
        let mut ret_words_changed = BTreeMap::new();
        let mut ret_images_changed = BTreeMap::new();
        let mut ret_image_masks_changed = BTreeMap::new();
        let mut ret_css_properties_changed = BTreeMap::new();
        let mut ret_nodes_scrolled_in_callbacks = BTreeMap::new();

        let mut should_terminate = TerminateTimer::Continue;
        let mut new_focus_target = None;

        let current_scroll_states_nested = self.get_nested_scroll_states(DomId::ROOT_ID);

        // Check if timer exists and get node_id before borrowing self mutably
        let timer_exists = self.timers.contains_key(&TimerId { id: timer_id });
        let timer_node_id = self
            .timers
            .get(&TimerId { id: timer_id })
            .and_then(|t| t.node_id.into_option());

        if timer_exists {
            let mut stop_propagation = false;

            // TODO: store the hit DOM of the timer?
            let hit_dom_node = match timer_node_id {
                Some(s) => s,
                None => DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::from_crate_internal(None),
                },
            };
            let cursor_relative_to_item = OptionLogicalPosition::None;
            let cursor_in_viewport = OptionLogicalPosition::None;

            let callback_info = CallbackInfo::new(
                self,
                renderer_resources,
                previous_window_state,
                current_window_state,
                &mut ret_modified_window_state,
                gl_context,
                image_cache,
                system_fonts,
                system_style,
                &mut ret_timers,
                &mut ret_threads,
                &mut ret_timers_removed,
                &mut ret_threads_removed,
                current_window_handle,
                &mut ret.windows_created,
                system_callbacks,
                &mut stop_propagation,
                &mut new_focus_target,
                &mut ret_words_changed,
                &mut ret_images_changed,
                &mut ret_image_masks_changed,
                &mut ret_css_properties_changed,
                &current_scroll_states_nested,
                &mut ret_nodes_scrolled_in_callbacks,
                hit_dom_node,
                cursor_relative_to_item,
                cursor_in_viewport,
            );

            // Now we can borrow the timer mutably
            let timer = self.timers.get_mut(&TimerId { id: timer_id }).unwrap();
            let tcr = timer.invoke(&callback_info, &system_callbacks.get_system_time_fn);

            ret.callbacks_update_screen = tcr.should_update;
            should_terminate = tcr.should_terminate;

            if !ret_timers.is_empty() {
                ret.timers = Some(ret_timers);
            }
            if !ret_threads.is_empty() {
                ret.threads = Some(ret_threads);
            }
            if ret_modified_window_state != ret_window_state {
                ret.modified_window_state = Some(ret_modified_window_state);
            }
            if !ret_threads_removed.is_empty() {
                ret.threads_removed = Some(ret_threads_removed);
            }
            if !ret_timers_removed.is_empty() {
                ret.timers_removed = Some(ret_timers_removed);
            }
            if !ret_words_changed.is_empty() {
                ret.words_changed = Some(ret_words_changed);
            }
            if !ret_images_changed.is_empty() {
                ret.images_changed = Some(ret_images_changed);
            }
            if !ret_image_masks_changed.is_empty() {
                ret.image_masks_changed = Some(ret_image_masks_changed);
            }
            if !ret_css_properties_changed.is_empty() {
                ret.css_properties_changed = Some(ret_css_properties_changed);
            }
            if !ret_nodes_scrolled_in_callbacks.is_empty() {
                ret.nodes_scrolled_in_callbacks = Some(ret_nodes_scrolled_in_callbacks);
            }
        }

        if let Some(ft) = new_focus_target {
            if let Ok(new_focus_node) = crate::managers::focus_cursor::resolve_focus_target(
                &ft,
                &self.layout_results,
                self.focus_manager.get_focused_node().copied(),
            ) {
                ret.update_focused_node = match new_focus_node {
                    Some(node) => FocusUpdateRequest::FocusNode(node),
                    None => FocusUpdateRequest::ClearFocus,
                };
            }
        }

        if should_terminate == TerminateTimer::Terminate {
            ret.timers_removed
                .get_or_insert_with(|| std::collections::BTreeSet::new())
                .insert(TimerId { id: timer_id });
        }

        return ret;
    }

    #[cfg(feature = "std")]
    pub fn run_all_threads(
        &mut self,
        data: &mut RefAny,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_style: std::sync::Arc<azul_css::system::SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use std::collections::BTreeSet;

        use azul_core::{callbacks::Update, refany::RefAny};

        use crate::{
            callbacks::{CallCallbacksResult, CallbackInfo},
            thread::{OptionThreadReceiveMsg, ThreadReceiveMsg, ThreadWriteBackMsg},
        };

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: FocusUpdateRequest::NoChange,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            cursor_changed: false,
            stop_propagation: false,
            prevent_default: false,
        };

        let mut ret_modified_window_state: WindowState = current_window_state.clone().into();
        let ret_window_state = ret_modified_window_state.clone();
        let mut ret_timers = FastHashMap::new();
        let mut ret_timers_removed = FastBTreeSet::new();
        let mut ret_threads = FastHashMap::new();
        let mut ret_threads_removed = FastBTreeSet::new();
        let mut ret_words_changed = BTreeMap::new();
        let mut ret_images_changed = BTreeMap::new();
        let mut ret_image_masks_changed = BTreeMap::new();
        let mut ret_css_properties_changed = BTreeMap::new();
        let mut ret_nodes_scrolled_in_callbacks = BTreeMap::new();
        let mut new_focus_target = None;
        let mut stop_propagation = false;
        let current_scroll_states = self.get_nested_scroll_states(DomId::ROOT_ID);

        // Collect thread IDs first to avoid borrowing self.threads while accessing self
        let thread_ids: Vec<ThreadId> = self.threads.keys().copied().collect();

        for thread_id in thread_ids {
            let thread = match self.threads.get_mut(&thread_id) {
                Some(t) => t,
                None => continue,
            };

            let hit_dom_node = DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(None),
            };
            let cursor_relative_to_item = OptionLogicalPosition::None;
            let cursor_in_viewport = OptionLogicalPosition::None;

            // Lock the mutex, extract data, then drop the guard before creating CallbackInfo
            let (msg, writeback_data_ptr, is_finished) = {
                let thread_inner = &mut *match thread.ptr.lock().ok() {
                    Some(s) => s,
                    None => {
                        ret.threads_removed
                            .get_or_insert_with(|| BTreeSet::default())
                            .insert(thread_id);
                        continue;
                    }
                };

                let _ = thread_inner.sender_send(ThreadSendMsg::Tick);
                let update = thread_inner.receiver_try_recv();
                let msg = match update {
                    OptionThreadReceiveMsg::None => continue,
                    OptionThreadReceiveMsg::Some(s) => s,
                };

                let writeback_data_ptr = &mut thread_inner.writeback_data as *mut _;
                let is_finished = thread_inner.is_finished();

                (msg, writeback_data_ptr, is_finished)
                // MutexGuard is dropped here
            };

            let ThreadWriteBackMsg { mut data, callback } = match msg {
                ThreadReceiveMsg::Update(update_screen) => {
                    ret.callbacks_update_screen.max_self(update_screen);
                    continue;
                }
                ThreadReceiveMsg::WriteBack(t) => t,
            };

            let mut callback_info = CallbackInfo::new(
                self,
                renderer_resources,
                previous_window_state,
                current_window_state,
                &mut ret_modified_window_state,
                gl_context,
                image_cache,
                system_fonts,
                system_style.clone(),
                &mut ret_timers,
                &mut ret_threads,
                &mut ret_timers_removed,
                &mut ret_threads_removed,
                current_window_handle,
                &mut ret.windows_created,
                system_callbacks,
                &mut stop_propagation,
                &mut new_focus_target,
                &mut ret_words_changed,
                &mut ret_images_changed,
                &mut ret_image_masks_changed,
                &mut ret_css_properties_changed,
                &current_scroll_states,
                &mut ret_nodes_scrolled_in_callbacks,
                hit_dom_node,
                cursor_relative_to_item,
                cursor_in_viewport,
            );

            let callback_update = (callback.cb)(
                unsafe { &mut *writeback_data_ptr },
                &mut data,
                &mut callback_info,
            );
            ret.callbacks_update_screen.max_self(callback_update);

            if is_finished {
                ret.threads_removed
                    .get_or_insert_with(|| BTreeSet::default())
                    .insert(thread_id);
            }
        }

        if !ret_timers.is_empty() {
            ret.timers = Some(ret_timers);
        }
        if !ret_threads.is_empty() {
            ret.threads = Some(ret_threads);
        }
        if ret_modified_window_state != ret_window_state {
            ret.modified_window_state = Some(ret_modified_window_state);
        }
        if !ret_threads_removed.is_empty() {
            ret.threads_removed = Some(ret_threads_removed);
        }
        if !ret_timers_removed.is_empty() {
            ret.timers_removed = Some(ret_timers_removed);
        }
        if !ret_words_changed.is_empty() {
            ret.words_changed = Some(ret_words_changed);
        }
        if !ret_images_changed.is_empty() {
            ret.images_changed = Some(ret_images_changed);
        }
        if !ret_image_masks_changed.is_empty() {
            ret.image_masks_changed = Some(ret_image_masks_changed);
        }
        if !ret_css_properties_changed.is_empty() {
            ret.css_properties_changed = Some(ret_css_properties_changed);
        }
        if !ret_nodes_scrolled_in_callbacks.is_empty() {
            ret.nodes_scrolled_in_callbacks = Some(ret_nodes_scrolled_in_callbacks);
        }

        if let Some(ft) = new_focus_target {
            if let Ok(new_focus_node) = crate::managers::focus_cursor::resolve_focus_target(
                &ft,
                &self.layout_results,
                self.focus_manager.get_focused_node().copied(),
            ) {
                ret.update_focused_node = match new_focus_node {
                    Some(node) => FocusUpdateRequest::FocusNode(node),
                    None => FocusUpdateRequest::ClearFocus,
                };
            }
        }

        return ret;
    }

    /// Invokes a single callback (used for on_window_create, on_window_shutdown, etc.)
    pub fn invoke_single_callback(
        &mut self,
        callback: &mut Callback,
        data: &mut RefAny,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_style: std::sync::Arc<azul_css::system::SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use azul_core::{callbacks::Update, refany::RefAny};

        use crate::callbacks::{CallCallbacksResult, Callback, CallbackInfo};

        let hit_dom_node = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(None),
        };

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: FocusUpdateRequest::NoChange,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            cursor_changed: false,
            stop_propagation: false,
            prevent_default: false,
        };

        let mut ret_modified_window_state: WindowState = current_window_state.clone().into();
        let ret_window_state = ret_modified_window_state.clone();
        let mut ret_timers = FastHashMap::new();
        let mut ret_timers_removed = FastBTreeSet::new();
        let mut ret_threads = FastHashMap::new();
        let mut ret_threads_removed = FastBTreeSet::new();
        let mut ret_words_changed = BTreeMap::new();
        let mut ret_images_changed = BTreeMap::new();
        let mut ret_image_masks_changed = BTreeMap::new();
        let mut ret_css_properties_changed = BTreeMap::new();
        let mut ret_nodes_scrolled_in_callbacks = BTreeMap::new();
        let mut new_focus_target = None;
        let mut stop_propagation = false;
        let current_scroll_states = self.get_nested_scroll_states(DomId::ROOT_ID);

        let cursor_relative_to_item = OptionLogicalPosition::None;
        let cursor_in_viewport = OptionLogicalPosition::None;

        let mut callback_info = CallbackInfo::new(
            self,
            renderer_resources,
            previous_window_state,
            current_window_state,
            &mut ret_modified_window_state,
            gl_context,
            image_cache,
            system_fonts,
            system_style,
            &mut ret_timers,
            &mut ret_threads,
            &mut ret_timers_removed,
            &mut ret_threads_removed,
            current_window_handle,
            &mut ret.windows_created,
            system_callbacks,
            &mut stop_propagation,
            &mut new_focus_target,
            &mut ret_words_changed,
            &mut ret_images_changed,
            &mut ret_image_masks_changed,
            &mut ret_css_properties_changed,
            &current_scroll_states,
            &mut ret_nodes_scrolled_in_callbacks,
            hit_dom_node,
            cursor_relative_to_item,
            cursor_in_viewport,
        );

        ret.callbacks_update_screen = (callback.cb)(data, &mut callback_info);

        if !ret_timers.is_empty() {
            ret.timers = Some(ret_timers);
        }
        if !ret_threads.is_empty() {
            ret.threads = Some(ret_threads);
        }
        if ret_modified_window_state != ret_window_state {
            ret.modified_window_state = Some(ret_modified_window_state);
        }
        if !ret_threads_removed.is_empty() {
            ret.threads_removed = Some(ret_threads_removed);
        }
        if !ret_timers_removed.is_empty() {
            ret.timers_removed = Some(ret_timers_removed);
        }
        if !ret_words_changed.is_empty() {
            ret.words_changed = Some(ret_words_changed);
        }
        if !ret_images_changed.is_empty() {
            ret.images_changed = Some(ret_images_changed);
        }
        if !ret_image_masks_changed.is_empty() {
            ret.image_masks_changed = Some(ret_image_masks_changed);
        }
        if !ret_css_properties_changed.is_empty() {
            ret.css_properties_changed = Some(ret_css_properties_changed);
        }
        if !ret_nodes_scrolled_in_callbacks.is_empty() {
            ret.nodes_scrolled_in_callbacks = Some(ret_nodes_scrolled_in_callbacks);
        }

        if let Some(ft) = new_focus_target {
            if let Ok(new_focus_node) = crate::managers::focus_cursor::resolve_focus_target(
                &ft,
                &self.layout_results,
                self.focus_manager.get_focused_node().copied(),
            ) {
                ret.update_focused_node = match new_focus_node {
                    Some(node) => FocusUpdateRequest::FocusNode(node),
                    None => FocusUpdateRequest::ClearFocus,
                };
            }
        }

        return ret;
    }

    /// Invokes a menu callback
    pub fn invoke_menu_callback(
        &mut self,
        menu_callback: &mut MenuCallback,
        hit_dom_node: DomNodeId,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_style: std::sync::Arc<azul_css::system::SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use azul_core::callbacks::Update;

        use crate::callbacks::{CallCallbacksResult, CallbackInfo, MenuCallback};

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: FocusUpdateRequest::NoChange,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            cursor_changed: false,
            stop_propagation: false,
            prevent_default: false,
        };

        let mut ret_modified_window_state: WindowState = current_window_state.clone().into();
        let ret_window_state = ret_modified_window_state.clone();
        let mut ret_timers = FastHashMap::new();
        let mut ret_timers_removed = FastBTreeSet::new();
        let mut ret_threads = FastHashMap::new();
        let mut ret_threads_removed = FastBTreeSet::new();
        let mut ret_words_changed = BTreeMap::new();
        let mut ret_images_changed = BTreeMap::new();
        let mut ret_image_masks_changed = BTreeMap::new();
        let mut ret_css_properties_changed = BTreeMap::new();
        let mut ret_nodes_scrolled_in_callbacks = BTreeMap::new();
        let mut new_focus_target = None;
        let mut stop_propagation = false;
        let current_scroll_states = self.get_nested_scroll_states(DomId::ROOT_ID);

        let cursor_relative_to_item = OptionLogicalPosition::None;
        let cursor_in_viewport = OptionLogicalPosition::None;

        let mut callback_info = CallbackInfo::new(
            self,
            renderer_resources,
            previous_window_state,
            current_window_state,
            &mut ret_modified_window_state,
            gl_context,
            image_cache,
            system_fonts,
            system_style,
            &mut ret_timers,
            &mut ret_threads,
            &mut ret_timers_removed,
            &mut ret_threads_removed,
            current_window_handle,
            &mut ret.windows_created,
            system_callbacks,
            &mut stop_propagation,
            &mut new_focus_target,
            &mut ret_words_changed,
            &mut ret_images_changed,
            &mut ret_image_masks_changed,
            &mut ret_css_properties_changed,
            &current_scroll_states,
            &mut ret_nodes_scrolled_in_callbacks,
            hit_dom_node,
            cursor_relative_to_item,
            cursor_in_viewport,
        );

        ret.callbacks_update_screen =
            (menu_callback.callback.cb)(&mut menu_callback.data, &mut callback_info);

        if !ret_timers.is_empty() {
            ret.timers = Some(ret_timers);
        }
        if !ret_threads.is_empty() {
            ret.threads = Some(ret_threads);
        }
        if ret_modified_window_state != ret_window_state {
            ret.modified_window_state = Some(ret_modified_window_state);
        }
        if !ret_threads_removed.is_empty() {
            ret.threads_removed = Some(ret_threads_removed);
        }
        if !ret_timers_removed.is_empty() {
            ret.timers_removed = Some(ret_timers_removed);
        }
        if !ret_words_changed.is_empty() {
            ret.words_changed = Some(ret_words_changed);
        }
        if !ret_images_changed.is_empty() {
            ret.images_changed = Some(ret_images_changed);
        }
        if !ret_image_masks_changed.is_empty() {
            ret.image_masks_changed = Some(ret_image_masks_changed);
        }
        if !ret_css_properties_changed.is_empty() {
            ret.css_properties_changed = Some(ret_css_properties_changed);
        }
        if !ret_nodes_scrolled_in_callbacks.is_empty() {
            ret.nodes_scrolled_in_callbacks = Some(ret_nodes_scrolled_in_callbacks);
        }

        if let Some(ft) = new_focus_target {
            if let Ok(new_focus_node) = crate::managers::focus_cursor::resolve_focus_target(
                &ft,
                &self.layout_results,
                self.focus_manager.get_focused_node().copied(),
            ) {
                ret.update_focused_node = match new_focus_node {
                    Some(node) => FocusUpdateRequest::FocusNode(node),
                    None => FocusUpdateRequest::ClearFocus,
                };
            }
        }

        return ret;
    }
}

#[cfg(test)]
mod tests {
    use azul_core::{
        dom::DomId,
        gpu::GpuValueCache,
        task::{Instant, ThreadId, TimerId},
    };

    use super::*;
    use crate::{thread::Thread, timer::Timer};

    #[test]
    fn test_timer_add_remove() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let timer_id = TimerId { id: 1 };
        let timer = Timer::default();

        // Add timer
        window.add_timer(timer_id, timer);
        assert!(window.get_timer(&timer_id).is_some());
        assert_eq!(window.get_timer_ids().len(), 1);

        // Remove timer
        let removed = window.remove_timer(&timer_id);
        assert!(removed.is_some());
        assert!(window.get_timer(&timer_id).is_none());
        assert_eq!(window.get_timer_ids().len(), 0);
    }

    #[test]
    fn test_timer_get_mut() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let timer_id = TimerId { id: 1 };
        let timer = Timer::default();

        window.add_timer(timer_id, timer);

        // Get mutable reference
        let timer_mut = window.get_timer_mut(&timer_id);
        assert!(timer_mut.is_some());
    }

    #[test]
    fn test_multiple_timers() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let timer1 = TimerId { id: 1 };
        let timer2 = TimerId { id: 2 };
        let timer3 = TimerId { id: 3 };

        window.add_timer(timer1, Timer::default());
        window.add_timer(timer2, Timer::default());
        window.add_timer(timer3, Timer::default());

        assert_eq!(window.get_timer_ids().len(), 3);

        window.remove_timer(&timer2);
        assert_eq!(window.get_timer_ids().len(), 2);
        assert!(window.get_timer(&timer1).is_some());
        assert!(window.get_timer(&timer2).is_none());
        assert!(window.get_timer(&timer3).is_some());
    }

    // Thread management tests removed - Thread::default() not available
    // and threads require complex setup. Thread management is tested
    // through integration tests instead.

    #[test]
    fn test_gpu_cache_management() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId { inner: 0 };

        // Initially empty
        assert!(window.get_gpu_cache(&dom_id).is_none());

        // Get or create
        let cache = window.get_or_create_gpu_cache(dom_id);
        assert!(cache.transform_keys.is_empty());

        // Now exists
        assert!(window.get_gpu_cache(&dom_id).is_some());

        // Can get mutable reference
        let cache_mut = window.get_gpu_cache_mut(&dom_id);
        assert!(cache_mut.is_some());
    }

    #[test]
    fn test_gpu_cache_multiple_doms() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let dom1 = DomId { inner: 0 };
        let dom2 = DomId { inner: 1 };

        window.get_or_create_gpu_cache(dom1);
        window.get_or_create_gpu_cache(dom2);

        assert!(window.get_gpu_cache(&dom1).is_some());
        assert!(window.get_gpu_cache(&dom2).is_some());
    }

    #[test]
    fn test_compute_cursor_type_empty_hit_test() {
        use crate::hit_test::FullHitTest;

        let fc_cache = FcFontCache::default();
        let window = LayoutWindow::new(fc_cache).unwrap();

        let empty_hit = FullHitTest::empty(None);
        let cursor_test = window.compute_cursor_type_hit_test(&empty_hit);

        // Empty hit test should result in default cursor
        assert_eq!(
            cursor_test.cursor_icon,
            azul_core::window::MouseCursorType::Default
        );
        assert!(cursor_test.cursor_node.is_none());
    }

    #[test]
    fn test_layout_result_access() {
        let fc_cache = FcFontCache::default();
        let window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId { inner: 0 };

        // Initially no layout results
        assert!(window.get_layout_result(&dom_id).is_none());
        assert_eq!(window.get_dom_ids().len(), 0);
    }

    // === ScrollManager and IFrame Integration Tests ===

    #[test]
    fn test_scroll_manager_initialization() {
        let fc_cache = FcFontCache::default();
        let window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId::ROOT_ID;
        let node_id = NodeId::new(0);

        // Initially no scroll states
        let scroll_offsets = window.scroll_states.get_scroll_states_for_dom(dom_id);
        assert!(scroll_offsets.is_empty());

        // No current offset
        let offset = window.scroll_states.get_current_offset(dom_id, node_id);
        assert_eq!(offset, None);
    }

    #[test]
    fn test_scroll_manager_tick_updates_activity() {
        use azul_core::task::{Duration, Instant, SystemTimeDiff};

        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId::ROOT_ID;
        let node_id = NodeId::new(0);

        // Create a scroll event
        let scroll_event = crate::managers::scroll_state::ScrollEvent {
            dom_id,
            node_id,
            delta: LogicalPosition::new(10.0, 20.0),
            source: azul_core::events::EventSource::User,
            duration: None,
            easing: EasingFunction::Linear,
        };

        #[cfg(feature = "std")]
        let now = Instant::System(std::time::Instant::now().into());
        #[cfg(not(feature = "std"))]
        let now = Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });

        let did_scroll = window
            .scroll_states
            .process_scroll_event(scroll_event, now.clone());

        // process_scroll_event should return true for successful scroll
        assert!(did_scroll);
    }

    #[test]
    fn test_scroll_manager_programmatic_scroll() {
        use azul_core::task::{Duration, Instant, SystemTimeDiff};

        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId::ROOT_ID;
        let node_id = NodeId::new(0);

        #[cfg(feature = "std")]
        let now = Instant::System(std::time::Instant::now().into());
        #[cfg(not(feature = "std"))]
        let now = Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });

        // Programmatic scroll with animation
        window.scroll_states.scroll_to(
            dom_id,
            node_id,
            LogicalPosition::new(100.0, 200.0),
            Duration::System(SystemTimeDiff::from_millis(300)),
            EasingFunction::EaseOut,
            now.clone(),
        );

        let tick_result = window.scroll_states.tick(now);

        // Programmatic scroll should start animation
        assert!(tick_result.needs_repaint);
    }

    #[test]
    fn test_scroll_manager_iframe_edge_detection() {
        // Note: This test is disabled because the new IFrame architecture
        // moved edge detection logic to IFrameManager. The old ScrollManager
        // API (update_iframe_scroll_info, iframes_to_update) no longer exists.
        // Edge detection is now tested through IFrameManager::check_reinvoke.

        // TODO: Rewrite this test to use the new IFrameManager API once
        // we have a proper test setup for IFrames.
    }

    #[test]
    fn test_scroll_manager_iframe_invocation_tracking() {
        // Note: This test is disabled because IFrame invocation tracking
        // moved to IFrameManager. The ScrollManager no longer tracks
        // which IFrames have been invoked.

        // TODO: Rewrite this test to use IFrameManager::mark_invoked
        // and IFrameManager::check_reinvoke.
    }

    #[test]
    fn test_scrollbar_opacity_fading() {
        // Note: This test is disabled because scrollbar opacity calculation
        // is now done through a helper function in LayoutWindow, not
        // through ScrollManager.get_scrollbar_opacity().

        // The new architecture separates scroll state from opacity calculation.
        // ScrollManager tracks last_activity_time, and LayoutWindow has a
        // calculate_scrollbar_opacity() helper that computes fade based on time.

        // TODO: Rewrite this test to use LayoutWindow::calculate_scrollbar_opacity
        // with ScrollManager::get_last_activity_time.
    }

    #[test]
    fn test_iframe_callback_reason_initial_render() {
        // Note: This test is disabled because the frame lifecycle API
        // (begin_frame, end_frame, had_new_doms) was removed from ScrollManager.

        // IFrame callback reasons are now determined by IFrameManager::check_reinvoke
        // which checks if an IFrame has been invoked before.

        // TODO: Rewrite to test IFrameManager::check_reinvoke with InitialRender.
    }

    #[test]
    fn test_gpu_cache_scrollbar_opacity_keys() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId::ROOT_ID;
        let node_id = NodeId::new(0);

        // Get or create GPU cache
        let gpu_cache = window.get_or_create_gpu_cache(dom_id);

        // Initially no scrollbar opacity keys
        assert!(gpu_cache.scrollbar_v_opacity_keys.is_empty());
        assert!(gpu_cache.scrollbar_h_opacity_keys.is_empty());

        // Add a vertical scrollbar opacity key
        let opacity_key = azul_core::resources::OpacityKey::unique();
        gpu_cache
            .scrollbar_v_opacity_keys
            .insert((dom_id, node_id), opacity_key);
        gpu_cache
            .scrollbar_v_opacity_values
            .insert((dom_id, node_id), 1.0);

        // Verify it was added
        assert_eq!(gpu_cache.scrollbar_v_opacity_keys.len(), 1);
        assert_eq!(
            gpu_cache.scrollbar_v_opacity_values.get(&(dom_id, node_id)),
            Some(&1.0)
        );
    }

    #[test]
    fn test_frame_lifecycle_begin_end() {
        // Note: This test is disabled because begin_frame/end_frame API
        // was removed from ScrollManager. Frame lifecycle is now managed
        // at a higher level.

        // The new ScrollManager focuses purely on scroll state and animations.
        // Frame tracking (had_scroll_activity, had_programmatic_scroll) was
        // removed as it's no longer needed with the new architecture.

        // TODO: If frame lifecycle tracking is needed, it should be
        // implemented at the LayoutWindow level, not in ScrollManager.
    }
}

// --- Cross-Paragraph Cursor Navigation API ---
impl LayoutWindow {
    /// Finds the next text node in the DOM tree after the given node.
    ///
    /// This function performs a depth-first traversal to find the next node
    /// that contains text content and is selectable (user-select != none).
    ///
    /// # Arguments
    /// * `dom_id` - The ID of the DOM containing the current node
    /// * `current_node` - The current node ID to start searching from
    ///
    /// # Returns
    /// * `Some((DomId, NodeId))` - The next text node if found
    /// * `None` - If no next text node exists
    pub fn find_next_text_node(
        &self,
        dom_id: &DomId,
        current_node: NodeId,
    ) -> Option<(DomId, NodeId)> {
        let layout_result = self.get_layout_result(dom_id)?;
        let styled_dom = &layout_result.styled_dom;

        // Start from the next node in document order
        let start_idx = current_node.index() + 1;
        let node_hierarchy = &styled_dom.node_hierarchy;

        for i in start_idx..node_hierarchy.len() {
            let node_id = NodeId::new(i);

            // Check if node has text content
            if self.node_has_text_content(styled_dom, node_id) {
                // Check if text is selectable
                if self.is_text_selectable(styled_dom, node_id) {
                    return Some((*dom_id, node_id));
                }
            }
        }

        None
    }

    /// Finds the previous text node in the DOM tree before the given node.
    ///
    /// This function performs a reverse depth-first traversal to find the previous node
    /// that contains text content and is selectable.
    ///
    /// # Arguments
    /// * `dom_id` - The ID of the DOM containing the current node
    /// * `current_node` - The current node ID to start searching from
    ///
    /// # Returns
    /// * `Some((DomId, NodeId))` - The previous text node if found
    /// * `None` - If no previous text node exists
    pub fn find_prev_text_node(
        &self,
        dom_id: &DomId,
        current_node: NodeId,
    ) -> Option<(DomId, NodeId)> {
        let layout_result = self.get_layout_result(dom_id)?;
        let styled_dom = &layout_result.styled_dom;

        // Start from the previous node in reverse document order
        let current_idx = current_node.index();

        for i in (0..current_idx).rev() {
            let node_id = NodeId::new(i);

            // Check if node has text content
            if self.node_has_text_content(styled_dom, node_id) {
                // Check if text is selectable
                if self.is_text_selectable(styled_dom, node_id) {
                    return Some((*dom_id, node_id));
                }
            }
        }

        None
    }

    /// Checks if a node has text content.
    fn node_has_text_content(&self, styled_dom: &StyledDom, node_id: NodeId) -> bool {
        use azul_core::dom::NodeType;

        // Check if node itself is a text node
        let node_data_container = styled_dom.node_data.as_container();
        let node_type = node_data_container[node_id].get_node_type();
        if matches!(node_type, NodeType::Text(_)) {
            return true;
        }

        // Check if node has text children
        let hierarchy_container = styled_dom.node_hierarchy.as_container();
        let node_item = &hierarchy_container[node_id];

        // Iterate through children
        let mut current_child = node_item.first_child_id(node_id);
        while let Some(child_id) = current_child {
            let child_type = node_data_container[child_id].get_node_type();
            if matches!(child_type, NodeType::Text(_)) {
                return true;
            }

            // Move to next sibling
            current_child = hierarchy_container[child_id].next_sibling_id();
        }

        false
    }

    /// Checks if text in a node is selectable based on CSS user-select property.
    ///
    /// TODO: Currently always returns true. In the future, this should check
    /// the CSS user-select property once it's available in the CssPropertyCache API.
    fn is_text_selectable(&self, _styled_dom: &StyledDom, _node_id: NodeId) -> bool {
        // Default: text is selectable
        // TODO: Check user-select CSS property:
        // let node_data = &styled_dom.node_data.as_container()[node_id];
        // let node_state = &styled_dom.styled_nodes.as_container()[node_id].state;
        // styled_dom.css_property_cache.ptr.get_user_select(node_data, &node_id, node_state)
        true
    }
}
