//! Window layout management for solver3/text3
//!
//! This module provides the high-level API for managing layout state across frames,
//! including caching, incremental updates, and display list generation.
//!
//! The main entry point is `LayoutWindow`, which encapsulates all the state needed
//! to perform layout and maintain consistency across window resizes and DOM updates.

use std::collections::{BTreeMap, BTreeSet};

use azul_core::{
    callbacks::{DocumentId, DomNodeId, ExternalSystemCallbacks, FocusTarget, ScrollPosition},
    dom::NodeId,
    gl::OptionGlContextPtr,
    gpu::GpuValueCache,
    resources::{
        Epoch, FontKey, GlTextureCache, IdNamespace, ImageCache, ImageRefHash, RenderCallbacks,
        RendererResources,
    },
    selection::SelectionState,
    styled_dom::{DomId, NodeHierarchyItemId, StyledDom},
    task::{Instant, Thread, ThreadId, Timer, TimerId},
    ui_solver::OptionLogicalPosition,
    window::{
        FullWindowState, LogicalPosition, LogicalRect, LogicalSize, RawWindowHandle, RendererType,
        WindowState,
    },
    FastBTreeSet, FastHashMap,
};
use azul_css::LayoutDebugMessage;
use rust_fontconfig::FcFontCache;

use crate::{
    font::parsed::ParsedFont,
    solver3::{
        self, cache::LayoutCache as Solver3LayoutCache, display_list::DisplayList,
        layout_tree::LayoutTree,
    },
    text3::{
        cache::{FontManager, LayoutCache as TextLayoutCache},
        default::PathLoader,
    },
};

/// Tracks the state of an IFrame for conditional re-invocation
#[derive(Debug, Clone)]
struct IFrameState {
    /// The bounds of the iframe node at last callback invocation
    bounds: LogicalRect,
    /// The scroll offset at last callback invocation  
    scroll_offset: LogicalPosition,
    /// The DomId assigned to this iframe's content
    dom_id: DomId,
}

/// Result of a layout pass for a single DOM, before display list generation
#[derive(Debug, Clone)]
pub struct DomLayoutResult {
    /// The styled DOM that was laid out
    pub styled_dom: StyledDom,
    /// The layout tree with computed sizes and positions
    pub layout_tree: LayoutTree<ParsedFont>,
    /// Absolute positions of all nodes
    pub absolute_positions: BTreeMap<usize, LogicalPosition>,
    /// The viewport used for this layout
    pub viewport: LogicalRect,
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
    pub layout_cache: Solver3LayoutCache<ParsedFont>,
    /// Text layout cache for text3 (shaped glyphs, line breaks, etc.)
    pub text_cache: TextLayoutCache<ParsedFont>,
    /// Font manager for loading and caching fonts
    pub font_manager: FontManager<ParsedFont, PathLoader>,
    /// Cached layout results for all DOMs (root + iframes)
    /// Maps DomId -> DomLayoutResult
    pub layout_results: BTreeMap<DomId, DomLayoutResult>,
    /// Scroll states for all nodes across all DOMs
    /// Maps (DomId, NodeId) -> ScrollPosition
    pub scroll_states: BTreeMap<(DomId, NodeId), ScrollPosition>,
    /// Selection states for all DOMs
    /// Maps DomId -> SelectionState
    pub selections: BTreeMap<DomId, SelectionState>,
    /// IFrame states for conditional re-invocation
    /// Maps (parent_dom_id, iframe_node_id) -> IFrameState
    pub iframe_states: BTreeMap<(DomId, NodeId), IFrameState>,
    /// Counter for generating unique DomIds for iframes
    pub next_dom_id: u64,
    /// Timers associated with this window
    pub timers: BTreeMap<TimerId, Timer>,
    /// Threads running in the background for this window
    pub threads: BTreeMap<ThreadId, Thread>,
    /// GPU value cache for CSS transforms and opacity
    pub gpu_value_cache: BTreeMap<DomId, GpuValueCache>,

    // === Fields from old WindowInternal (for integration) ===
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
            },
            text_cache: TextLayoutCache::new(),
            font_manager: FontManager::new(fc_cache)?,
            layout_results: BTreeMap::new(),
            scroll_states: BTreeMap::new(),
            selections: BTreeMap::new(),
            iframe_states: BTreeMap::new(),
            next_dom_id: 1, // Start at 1, 0 is reserved for ROOT_ID
            timers: BTreeMap::new(),
            threads: BTreeMap::new(),
            gpu_value_cache: BTreeMap::new(),
            renderer_resources: RendererResources::default(),
            renderer_type: None,
            previous_window_state: None,
            current_window_state: FullWindowState::default(),
            document_id: DocumentId::new(),
            id_namespace: IdNamespace::new(),
            epoch: Epoch::new(),
            gl_texture_cache: GlTextureCache::default(),
        })
    }

    /// Perform layout on a styled DOM and generate a display list.
    ///
    /// This is the main entry point for layout. It handles:
    /// - Incremental layout updates using the cached layout tree
    /// - Text shaping and line breaking
    /// - IFrame callback invocation and recursive layout
    /// - Display list generation for rendering
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
        mut styled_dom: StyledDom,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<DisplayList, crate::solver3::LayoutError> {
        // Assign root DomId if not set
        if styled_dom.dom_id.inner == 0 {
            styled_dom.dom_id = DomId::ROOT_ID;
        }
        let dom_id = styled_dom.dom_id;

        // Prepare viewport from window dimensions
        let viewport = LogicalRect {
            origin: LogicalPosition::new(0.0, 0.0),
            size: window_state.size.dimensions,
        };

        // Get scroll offsets for this DOM from our tracked state
        let scroll_offsets = self
            .scroll_states
            .iter()
            .filter(|((d, _), _)| *d == dom_id)
            .map(|((_, node_id), scroll_pos)| (*node_id, scroll_pos.clone()))
            .collect();

        // Clone the styled_dom before moving it
        let styled_dom_clone = styled_dom.clone();

        // Call the solver3 layout engine
        let display_list = solver3::layout_document(
            &mut self.layout_cache,
            &mut self.text_cache,
            styled_dom,
            viewport,
            &self.font_manager,
            &scroll_offsets,
            &self.selections,
            debug_messages,
        )?;

        // Store the layout result
        if let Some(tree) = self.layout_cache.tree.clone() {
            self.layout_results.insert(
                dom_id,
                DomLayoutResult {
                    styled_dom: styled_dom_clone,
                    layout_tree: tree,
                    absolute_positions: self.layout_cache.absolute_positions.clone(),
                    viewport,
                },
            );
        }

        Ok(display_list)
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
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<DisplayList, crate::solver3::LayoutError> {
        // Create a temporary FullWindowState with the new size
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = new_size;

        // Reuse the main layout method - solver3 will detect the viewport
        // change and invalidate only what's necessary
        self.layout_and_generate_display_list(
            styled_dom,
            &window_state,
            renderer_resources,
            debug_messages,
        )
    }

    /// Clear all caches (useful for testing or when switching documents).
    pub fn clear_caches(&mut self) {
        self.layout_cache = Solver3LayoutCache {
            tree: None,
            absolute_positions: BTreeMap::new(),
            viewport: None,
        };
        self.text_cache = TextLayoutCache::new();
        self.layout_results.clear();
        self.scroll_states.clear();
        self.selections.clear();
        self.iframe_states.clear();
        self.next_dom_id = 1;
    }

    /// Get a layout result for a specific DOM
    pub fn get_layout_result(&self, dom_id: DomId) -> Option<&DomLayoutResult> {
        self.layout_results.get(&dom_id)
    }

    /// Set scroll position for a node
    pub fn set_scroll_position(&mut self, dom_id: DomId, node_id: NodeId, scroll: ScrollPosition) {
        self.scroll_states.insert((dom_id, node_id), scroll);
    }

    /// Get scroll position for a node
    pub fn get_scroll_position(&self, dom_id: DomId, node_id: NodeId) -> Option<ScrollPosition> {
        self.scroll_states.get(&(dom_id, node_id)).cloned()
    }

    /// Set selection state for a DOM
    pub fn set_selection(&mut self, dom_id: DomId, selection: SelectionState) {
        self.selections.insert(dom_id, selection);
    }

    /// Get selection state for a DOM
    pub fn get_selection(&self, dom_id: DomId) -> Option<&SelectionState> {
        self.selections.get(&dom_id)
    }

    /// Generate a new unique DomId for an iframe
    fn allocate_dom_id(&mut self) -> DomId {
        let id = self.next_dom_id as usize;
        self.next_dom_id += 1;
        DomId { inner: id }
    }

    // Query methods for callbacks

    /// Get the size of a laid-out node
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let positioned_rectangle = layout_result.absolute_positions.get(nid)?;
        Some(LogicalSize::new(
            positioned_rectangle.size.width as f32,
            positioned_rectangle.size.height as f32,
        ))
    }

    /// Get the position of a laid-out node
    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<LogicalPosition> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let positioned_rectangle = layout_result.absolute_positions.get(nid)?;
        Some(LogicalPosition::new(
            positioned_rectangle.origin.x as f32,
            positioned_rectangle.origin.y as f32,
        ))
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
        let hierarchy_item = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?;
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
        self.gpu_value_cache.get(dom_id)
    }

    /// Get a mutable reference to the GPU value cache for a specific DOM
    pub fn get_gpu_cache_mut(&mut self, dom_id: &DomId) -> Option<&mut GpuValueCache> {
        self.gpu_value_cache.get_mut(dom_id)
    }

    /// Get or create a GPU value cache for a specific DOM
    pub fn get_or_create_gpu_cache(&mut self, dom_id: DomId) -> &mut GpuValueCache {
        self.gpu_value_cache
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
}

/// Result of a layout operation,包含display list和可能的warnings/debug信息.
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
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use std::collections::BTreeMap;

        use azul_core::{
            app_resources::ImageCache,
            task::TerminateTimer,
            window::{CallCallbacksResult, Update},
            FastBTreeSet, FastHashMap,
        };

        use crate::callbacks::CallbackInfo;

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: None,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            cursor_changed: false,
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

        let current_scroll_states = self.get_current_scroll_states();

        if let Some(timer) = self.timers.get_mut(&TimerId { id: timer_id }) {
            let mut stop_propagation = false;

            // TODO: store the hit DOM of the timer?
            let hit_dom_node = match timer.node_id.into_option() {
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

            let tcr = timer.invoke(
                callback_info,
                frame_start.clone(),
                system_callbacks.get_system_time_fn,
            );

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
            if let Ok(new_focus_node) =
                ft.resolve(&self.layout_results, current_window_state.focused_node)
            {
                ret.update_focused_node = Some(new_focus_node);
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
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use std::collections::BTreeSet;

        use azul_core::{
            callbacks::RefAny,
            task::{OptionThreadReceiveMsg, ThreadReceiveMsg, ThreadSendMsg, ThreadWriteBackMsg},
            window::{CallCallbacksResult, Update},
        };

        use crate::callbacks::CallbackInfo;

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: None,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            cursor_changed: false,
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
        let current_scroll_states = self.get_current_scroll_states();

        for (thread_id, thread) in self.threads.iter_mut() {
            let hit_dom_node = DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(None),
            };
            let cursor_relative_to_item = OptionLogicalPosition::None;
            let cursor_in_viewport = OptionLogicalPosition::None;

            let thread = &mut *match thread.ptr.lock().ok() {
                Some(s) => s,
                None => {
                    ret.threads_removed
                        .get_or_insert_with(|| BTreeSet::default())
                        .insert(*thread_id);
                    continue;
                }
            };

            let _ = thread.sender_send(ThreadSendMsg::Tick);
            let update = thread.receiver_try_recv();
            let msg = match update {
                OptionThreadReceiveMsg::None => continue,
                OptionThreadReceiveMsg::Some(s) => s,
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

            let callback_update =
                (callback.cb)(&mut thread.writeback_data, &mut data, &mut callback_info);
            ret.callbacks_update_screen.max_self(callback_update);

            if thread.is_finished() {
                ret.threads_removed
                    .get_or_insert_with(|| BTreeSet::default())
                    .insert(*thread_id);
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
            if let Ok(new_focus_node) =
                ft.resolve(&self.layout_results, current_window_state.focused_node)
            {
                ret.update_focused_node = Some(new_focus_node);
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
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use azul_core::{
            callbacks::{Callback, RefAny},
            window::{CallCallbacksResult, Update},
        };

        use crate::callbacks::CallbackInfo;

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
            update_focused_node: None,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            cursor_changed: false,
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
        let current_scroll_states = self.get_current_scroll_states();

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
            if let Ok(new_focus_node) =
                ft.resolve(&self.layout_results, current_window_state.focused_node)
            {
                ret.update_focused_node = Some(new_focus_node);
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
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use azul_core::{
            callbacks::MenuCallback,
            window::{CallCallbacksResult, Update},
        };

        use crate::callbacks::CallbackInfo;

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: None,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            cursor_changed: false,
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
        let current_scroll_states = self.get_current_scroll_states();

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
            if let Ok(new_focus_node) =
                ft.resolve(&self.layout_results, current_window_state.focused_node)
            {
                ret.update_focused_node = Some(new_focus_node);
            }
        }

        return ret;
    }
}

#[cfg(test)]
mod tests {
    use azul_core::{
        gpu::GpuValueCache,
        styled_dom::DomId,
        task::{Instant, Thread, ThreadId, Timer, TimerId},
    };

    use super::*;

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

    #[test]
    fn test_thread_add_remove() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let thread_id = ThreadId::unique();
        let thread = Thread::default();

        // Add thread
        window.add_thread(thread_id, thread);
        assert!(window.get_thread(&thread_id).is_some());
        assert_eq!(window.get_thread_ids().len(), 1);

        // Remove thread
        let removed = window.remove_thread(&thread_id);
        assert!(removed.is_some());
        assert!(window.get_thread(&thread_id).is_none());
        assert_eq!(window.get_thread_ids().len(), 0);
    }

    #[test]
    fn test_thread_get_mut() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let thread_id = ThreadId::unique();
        let thread = Thread::default();

        window.add_thread(thread_id, thread);

        // Get mutable reference
        let thread_mut = window.get_thread_mut(&thread_id);
        assert!(thread_mut.is_some());
    }

    #[test]
    fn test_multiple_threads() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let thread1 = ThreadId::unique();
        let thread2 = ThreadId::unique();

        window.add_thread(thread1, Thread::default());
        window.add_thread(thread2, Thread::default());

        assert_eq!(window.get_thread_ids().len(), 2);

        window.remove_thread(&thread1);
        assert_eq!(window.get_thread_ids().len(), 1);
        assert!(window.get_thread(&thread1).is_none());
        assert!(window.get_thread(&thread2).is_some());
    }

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
}
