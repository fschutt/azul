//! Callback handling for layout events
//!
//! This module provides the CallbackInfo struct and related types for handling
//! UI callbacks. Callbacks need access to layout information (node sizes, positions,
//! hierarchy), which is why this module lives in azul-layout instead of azul-core.

use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};

use azul_core::{
    callbacks::{DomNodeId, FocusTarget, RefAny, ScrollPosition, Update},
    gl::OptionGlContextPtr,
    id_tree::NodeId,
    resources::{ImageCache, ImageMask, ImageRef, RendererResources, UpdateImageType},
    styled_dom::{DomId, NodeHierarchyItemId, StyledDom},
    task::{ExternalSystemCallbacks, Thread, ThreadId, Timer, TimerId},
    window::{
        FullWindowState, KeyboardState, LogicalPosition, LogicalRect, LogicalSize, MouseState,
        OptionLogicalPosition, RawWindowHandle, WindowCreateOptions, WindowFlags, WindowState,
    },
    FastBTreeSet, FastHashMap,
};
use azul_css::{
    props::property::{CssProperty, CssPropertyType},
    AzString,
};
use rust_fontconfig::FcFontCache;

use crate::window::LayoutWindow;

/// Information about the callback that is passed to the callback whenever a callback is invoked
#[derive(Debug)]
#[repr(C)]
pub struct CallbackInfo {
    /// Pointer to the LayoutWindow containing all layout results (MUTABLE for timer/thread/GPU
    /// access)
    layout_window: *mut LayoutWindow,
    /// Necessary to query FontRefs from callbacks
    renderer_resources: *const RendererResources,
    /// Previous window state
    previous_window_state: *const Option<FullWindowState>,
    /// State of the current window that the callback was called on (read only!)
    current_window_state: *const FullWindowState,
    /// User-modifiable state of the window that the callback was called on
    modifiable_window_state: *mut WindowState,
    /// An Rc to the OpenGL context, in order to be able to render to OpenGL textures
    gl_context: *const OptionGlContextPtr,
    /// Cache to add / remove / query image RefAnys from / to CSS ids
    image_cache: *mut ImageCache,
    /// System font cache (can be regenerated / refreshed in callbacks)
    system_fonts: *mut FcFontCache,
    /// Currently running timers (polling functions, run on the main thread)
    timers: *mut FastHashMap<TimerId, Timer>,
    /// Currently running threads (asynchronous functions running each on a different thread)
    threads: *mut FastHashMap<ThreadId, Thread>,
    /// Timers removed by the callback
    timers_removed: *mut FastBTreeSet<TimerId>,
    /// Threads removed by the callback
    threads_removed: *mut FastBTreeSet<ThreadId>,
    /// Handle of the current window
    current_window_handle: *const RawWindowHandle,
    /// Used to spawn new windows from callbacks. You can use `get_current_window_handle()` to
    /// spawn child windows.
    new_windows: *mut Vec<WindowCreateOptions>,
    /// Callbacks for creating threads and getting the system time (since this crate uses no_std)
    system_callbacks: *const ExternalSystemCallbacks,
    /// Sets whether the event should be propagated to the parent hit node or not
    stop_propagation: *mut bool,
    /// The callback can change the focus_target - note that the focus_target is set before the
    /// next frames' layout() function is invoked, but the current frames callbacks are not
    /// affected.
    focus_target: *mut Option<FocusTarget>,
    /// Mutable reference to a list of words / text items that were changed in the callback
    words_changed_in_callbacks: *mut BTreeMap<DomId, BTreeMap<NodeId, AzString>>,
    /// Mutable reference to a list of images that were changed in the callback
    images_changed_in_callbacks:
        *mut BTreeMap<DomId, BTreeMap<NodeId, (ImageRef, UpdateImageType)>>,
    /// Mutable reference to a list of image clip masks that were changed in the callback
    image_masks_changed_in_callbacks: *mut BTreeMap<DomId, BTreeMap<NodeId, ImageMask>>,
    /// Mutable reference to a list of CSS property changes, so that the callbacks can change CSS
    /// properties
    css_properties_changed_in_callbacks: *mut BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>,
    /// Immutable (!) reference to where the nodes are currently scrolled (current position)
    current_scroll_states: *const BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>>,
    /// Mutable map where a user can set where he wants the nodes to be scrolled to (for the next
    /// frame)
    nodes_scrolled_in_callback:
        *mut BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, LogicalPosition>>,
    /// The ID of the DOM + the node that was hit. You can use this to query
    /// information about the node, but please don't hard-code any if / else
    /// statements based on the `NodeId`
    hit_dom_node: DomNodeId,
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was
    /// hit**.
    cursor_relative_to_item: OptionLogicalPosition,
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**.
    cursor_in_viewport: OptionLogicalPosition,
}

impl CallbackInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn new<'a>(
        layout_window: &'a mut LayoutWindow,
        renderer_resources: &'a RendererResources,
        previous_window_state: &'a Option<FullWindowState>,
        current_window_state: &'a FullWindowState,
        modifiable_window_state: &'a mut WindowState,
        gl_context: &'a OptionGlContextPtr,
        image_cache: &'a mut ImageCache,
        system_fonts: &'a mut FcFontCache,
        timers: &'a mut FastHashMap<TimerId, Timer>,
        threads: &'a mut FastHashMap<ThreadId, Thread>,
        timers_removed: &'a mut FastBTreeSet<TimerId>,
        threads_removed: &'a mut FastBTreeSet<ThreadId>,
        current_window_handle: &'a RawWindowHandle,
        new_windows: &'a mut Vec<WindowCreateOptions>,
        system_callbacks: &'a ExternalSystemCallbacks,
        stop_propagation: &'a mut bool,
        focus_target: &'a mut Option<FocusTarget>,
        words_changed_in_callbacks: &'a mut BTreeMap<DomId, BTreeMap<NodeId, AzString>>,
        images_changed_in_callbacks: &'a mut BTreeMap<
            DomId,
            BTreeMap<NodeId, (ImageRef, UpdateImageType)>,
        >,
        image_masks_changed_in_callbacks: &'a mut BTreeMap<DomId, BTreeMap<NodeId, ImageMask>>,
        css_properties_changed_in_callbacks: &'a mut BTreeMap<
            DomId,
            BTreeMap<NodeId, Vec<CssProperty>>,
        >,
        current_scroll_states: &'a BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>>,
        nodes_scrolled_in_callback: &'a mut BTreeMap<
            DomId,
            BTreeMap<NodeHierarchyItemId, LogicalPosition>,
        >,
        hit_dom_node: DomNodeId,
        cursor_relative_to_item: OptionLogicalPosition,
        cursor_in_viewport: OptionLogicalPosition,
    ) -> Self {
        Self {
            layout_window: layout_window as *mut LayoutWindow,
            renderer_resources: renderer_resources as *const RendererResources,
            previous_window_state: previous_window_state as *const Option<FullWindowState>,
            current_window_state: current_window_state as *const FullWindowState,
            modifiable_window_state: modifiable_window_state as *mut WindowState,
            gl_context: gl_context as *const OptionGlContextPtr,
            image_cache: image_cache as *mut ImageCache,
            system_fonts: system_fonts as *mut FcFontCache,
            timers: timers as *mut FastHashMap<TimerId, Timer>,
            threads: threads as *mut FastHashMap<ThreadId, Thread>,
            timers_removed: timers_removed as *mut FastBTreeSet<TimerId>,
            threads_removed: threads_removed as *mut FastBTreeSet<ThreadId>,
            new_windows: new_windows as *mut Vec<WindowCreateOptions>,
            current_window_handle: current_window_handle as *const RawWindowHandle,
            system_callbacks: system_callbacks as *const ExternalSystemCallbacks,
            stop_propagation: stop_propagation as *mut bool,
            focus_target: focus_target as *mut Option<FocusTarget>,
            words_changed_in_callbacks: words_changed_in_callbacks
                as *mut BTreeMap<DomId, BTreeMap<NodeId, AzString>>,
            images_changed_in_callbacks: images_changed_in_callbacks
                as *mut BTreeMap<DomId, BTreeMap<NodeId, (ImageRef, UpdateImageType)>>,
            image_masks_changed_in_callbacks: image_masks_changed_in_callbacks
                as *mut BTreeMap<DomId, BTreeMap<NodeId, ImageMask>>,
            css_properties_changed_in_callbacks: css_properties_changed_in_callbacks
                as *mut BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>,
            current_scroll_states: current_scroll_states
                as *const BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>>,
            nodes_scrolled_in_callback: nodes_scrolled_in_callback
                as *mut BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, LogicalPosition>>,
            hit_dom_node,
            cursor_relative_to_item,
            cursor_in_viewport,
        }
    }

    // Internal accessors
    fn internal_get_layout_window(&self) -> &LayoutWindow {
        unsafe { &*self.layout_window }
    }

    fn internal_get_layout_window_mut(&mut self) -> &mut LayoutWindow {
        unsafe { &mut *self.layout_window }
    }

    // Public API methods - delegates to LayoutWindow
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        self.internal_get_layout_window().get_node_size(node_id)
    }

    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<LogicalPosition> {
        self.internal_get_layout_window().get_node_position(node_id)
    }

    // ===== Timer Management =====

    /// Add a timer to this window
    pub fn add_timer(&mut self, timer_id: TimerId, timer: Timer) {
        self.internal_get_layout_window_mut()
            .add_timer(timer_id, timer);
    }

    /// Remove a timer from this window
    pub fn remove_timer(&mut self, timer_id: &TimerId) -> Option<Timer> {
        self.internal_get_layout_window_mut().remove_timer(timer_id)
    }

    /// Get a reference to a timer
    pub fn get_timer(&self, timer_id: &TimerId) -> Option<&Timer> {
        self.internal_get_layout_window().get_timer(timer_id)
    }

    /// Get a mutable reference to a timer
    pub fn get_timer_mut(&mut self, timer_id: &TimerId) -> Option<&mut Timer> {
        self.internal_get_layout_window_mut()
            .get_timer_mut(timer_id)
    }

    /// Get all timer IDs
    pub fn get_timer_ids(&self) -> Vec<TimerId> {
        self.internal_get_layout_window().get_timer_ids()
    }

    // ===== Thread Management =====

    /// Add a thread to this window
    pub fn add_thread(&mut self, thread_id: ThreadId, thread: Thread) {
        self.internal_get_layout_window_mut()
            .add_thread(thread_id, thread);
    }

    /// Remove a thread from this window
    pub fn remove_thread(&mut self, thread_id: &ThreadId) -> Option<Thread> {
        self.internal_get_layout_window_mut()
            .remove_thread(thread_id)
    }

    /// Get a reference to a thread
    pub fn get_thread(&self, thread_id: &ThreadId) -> Option<&Thread> {
        self.internal_get_layout_window().get_thread(thread_id)
    }

    /// Get a mutable reference to a thread
    pub fn get_thread_mut(&mut self, thread_id: &ThreadId) -> Option<&mut Thread> {
        self.internal_get_layout_window_mut()
            .get_thread_mut(thread_id)
    }

    /// Get all thread IDs
    pub fn get_thread_ids(&self) -> Vec<ThreadId> {
        self.internal_get_layout_window().get_thread_ids()
    }

    // ===== GPU Value Cache Management =====

    /// Get the GPU value cache for a specific DOM
    pub fn get_gpu_cache(&self, dom_id: &DomId) -> Option<&azul_core::gpu::GpuValueCache> {
        self.internal_get_layout_window().get_gpu_cache(dom_id)
    }

    /// Get a mutable reference to the GPU value cache for a specific DOM
    pub fn get_gpu_cache_mut(
        &mut self,
        dom_id: &DomId,
    ) -> Option<&mut azul_core::gpu::GpuValueCache> {
        self.internal_get_layout_window_mut()
            .get_gpu_cache_mut(dom_id)
    }

    /// Get or create a GPU value cache for a specific DOM
    pub fn get_or_create_gpu_cache(&mut self, dom_id: DomId) -> &mut azul_core::gpu::GpuValueCache {
        self.internal_get_layout_window_mut()
            .get_or_create_gpu_cache(dom_id)
    }

    // ===== Layout Result Access =====

    /// Get a layout result for a specific DOM
    pub fn get_layout_result(&self, dom_id: &DomId) -> Option<&crate::window::DomLayoutResult> {
        self.internal_get_layout_window().get_layout_result(dom_id)
    }

    /// Get a mutable layout result for a specific DOM
    pub fn get_layout_result_mut(
        &mut self,
        dom_id: &DomId,
    ) -> Option<&mut crate::window::DomLayoutResult> {
        self.internal_get_layout_window_mut()
            .get_layout_result_mut(dom_id)
    }

    /// Get all DOM IDs that have layout results
    pub fn get_dom_ids(&self) -> Vec<DomId> {
        self.internal_get_layout_window().get_dom_ids()
    }

    // TODO: Add more query methods as needed:
    // - get_computed_css_property
    // - get_parent
    // - get_first_child
    // - get_scroll_position
    // etc.
}
