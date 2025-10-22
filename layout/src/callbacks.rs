//! Callback handling for layout events
//!
//! This module provides the CallbackInfo struct and related types for handling
//! UI callbacks. Callbacks need access to layout information (node sizes, positions,
//! hierarchy), which is why this module lives in azul-layout instead of azul-core.

use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};

// Re-export callback macro from azul-core
use azul_core::impl_callback;
use azul_core::{
    animation::UpdateImageType,
    callbacks::{CoreCallback, FocusTarget, Update},
    dom::{DomId, DomNodeId, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
    gl::OptionGlContextPtr,
    hit_test::ScrollPosition,
    refany::RefAny,
    resources::{ImageCache, ImageMask, ImageRef, RendererResources},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::{ThreadId, TimerId},
    window::{KeyboardState, MouseState, RawWindowHandle, WindowFlags, WindowSize},
    FastBTreeSet, FastHashMap,
};
use azul_css::{
    props::property::{CssProperty, CssPropertyType},
    AzString,
};
use rust_fontconfig::FcFontCache;

use crate::{
    thread::Thread,
    timer::Timer,
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions, WindowState},
};

/// Main callback type for UI event handling
pub type CallbackType = extern "C" fn(&mut RefAny, &mut CallbackInfo) -> Update;

/// Stores a function pointer that is executed when the given UI element is hit
///
/// Must return an `Update` that denotes if the screen should be redrawn.
#[repr(C)]
pub struct Callback {
    pub cb: CallbackType,
}

impl_callback!(Callback);

impl Callback {
    pub fn from_core(core: CoreCallback) -> Self {
        Self {
            cb: unsafe { core::mem::transmute(core.cb) },
        }
    }
}

/// Optional Callback
#[derive(Debug, Eq, Copy, Clone, PartialEq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum OptionCallback {
    None,
    Some(Callback),
}

impl OptionCallback {
    pub fn into_option(self) -> Option<Callback> {
        match self {
            OptionCallback::None => None,
            OptionCallback::Some(c) => Some(c),
        }
    }

    pub fn is_some(&self) -> bool {
        matches!(self, OptionCallback::Some(_))
    }

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

    // ===== Node Hierarchy Navigation =====

    pub fn get_hit_node(&self) -> DomNodeId {
        self.hit_dom_node
    }

    pub fn get_parent(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.internal_get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = NodeId::new(node_id.node.inner);
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;
        let parent_id = hier_item.parent_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId {
                inner: parent_id.index(),
            },
        })
    }

    pub fn get_previous_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.internal_get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = NodeId::new(node_id.node.inner);
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;
        let sibling_id = hier_item.previous_sibling_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId {
                inner: sibling_id.index(),
            },
        })
    }

    pub fn get_next_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.internal_get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = NodeId::new(node_id.node.inner);
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;
        let sibling_id = hier_item.next_sibling_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId {
                inner: sibling_id.index(),
            },
        })
    }

    pub fn get_first_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.internal_get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = NodeId::new(node_id.node.inner);
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;
        let child_id = hier_item.first_child_id(node_id_internal)?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId {
                inner: child_id.index(),
            },
        })
    }

    pub fn get_last_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.internal_get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = NodeId::new(node_id.node.inner);
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;
        let child_id = hier_item.last_child_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId {
                inner: child_id.index(),
            },
        })
    }

    // ===== Node Data and State =====

    pub fn get_dataset(&mut self, node_id: DomNodeId) -> Option<RefAny> {
        let layout_window = self.internal_get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = NodeId::new(node_id.node.inner);
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;
        node_data.get_dataset().clone().into_option()
    }

    pub fn get_node_id_of_root_dataset(&mut self, search_key: RefAny) -> Option<DomNodeId> {
        let mut found: Option<(u64, DomNodeId)> = None;
        let search_type_id = search_key.get_type_id();

        for dom_id in self.get_dom_ids() {
            let layout_window = self.internal_get_layout_window();
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
                            node: NodeHierarchyItemId { inner: node_idx },
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
        let layout_window = self.internal_get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = NodeId::new(node_id.node.inner);
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;

        if let NodeType::Text(ref text) = node_data.get_node_type() {
            Some(text.clone())
        } else {
            None
        }
    }

    pub fn set_string_contents(&mut self, node_id: DomNodeId, new_string_contents: AzString) {
        let node_id_internal = NodeId::new(node_id.node.inner);
        unsafe {
            (*self.words_changed_in_callbacks)
                .entry(node_id.dom)
                .or_insert_with(BTreeMap::new)
                .insert(node_id_internal, new_string_contents);
        }
    }

    // ===== Window State Access =====

    pub fn get_current_window_state(&self) -> WindowState {
        unsafe { (*self.current_window_state).clone().into() }
    }

    pub fn get_current_window_flags(&self) -> WindowFlags {
        unsafe { (*self.current_window_state).flags.clone() }
    }

    pub fn get_current_keyboard_state(&self) -> KeyboardState {
        unsafe { (*self.current_window_state).keyboard_state.clone() }
    }

    pub fn get_current_mouse_state(&self) -> MouseState {
        unsafe { (*self.current_window_state).mouse_state.clone() }
    }

    pub fn get_previous_window_state(&self) -> Option<WindowState> {
        unsafe {
            (*self.previous_window_state)
                .as_ref()
                .map(|s| s.clone().into())
        }
    }

    pub fn get_previous_keyboard_state(&self) -> Option<KeyboardState> {
        unsafe {
            (*self.previous_window_state)
                .as_ref()
                .map(|s| s.keyboard_state.clone())
        }
    }

    pub fn get_previous_mouse_state(&self) -> Option<MouseState> {
        unsafe {
            (*self.previous_window_state)
                .as_ref()
                .map(|s| s.mouse_state.clone())
        }
    }

    pub fn set_window_state(&mut self, new_state: WindowState) {
        unsafe {
            *self.modifiable_window_state = new_state;
        }
    }

    pub fn set_window_flags(&mut self, new_flags: WindowFlags) {
        unsafe {
            (*self.modifiable_window_state).flags = new_flags;
        }
    }

    // ===== CSS and Styling =====

    pub fn set_css_property(&mut self, node_id: DomNodeId, prop: CssProperty) {
        let node_id_internal = NodeId::new(node_id.node.inner);
        unsafe {
            (*self.css_properties_changed_in_callbacks)
                .entry(node_id.dom)
                .or_insert_with(BTreeMap::new)
                .entry(node_id_internal)
                .or_insert_with(Vec::new)
                .push(prop);
        }
    }

    // ===== Focus Management =====

    pub fn set_focus(&mut self, target: FocusTarget) {
        unsafe {
            *self.focus_target = Some(target);
        }
    }

    // ===== Cursor and Input =====

    pub fn get_cursor_relative_to_node(&self) -> OptionLogicalPosition {
        self.cursor_relative_to_item
    }

    pub fn get_cursor_relative_to_viewport(&self) -> OptionLogicalPosition {
        self.cursor_in_viewport
    }

    pub fn stop_propagation(&mut self) {
        unsafe {
            *self.stop_propagation = true;
        }
    }

    // ===== Window Creation =====

    pub fn create_window(&mut self, window: WindowCreateOptions) {
        unsafe {
            (*self.new_windows).push(window);
        }
    }

    pub fn get_current_window_handle(&self) -> RawWindowHandle {
        unsafe { (*self.current_window_handle).clone() }
    }

    // ===== CSS Property Access =====

    pub fn get_computed_css_property(
        &self,
        _node_id: DomNodeId,
        _property_type: azul_css::props::property::CssPropertyType,
    ) -> Option<azul_css::props::property::CssProperty> {
        // TODO: Implement CSS property access from the new layout system
        // The old system used positioned_rectangles.resolved_css_properties
        // The new system uses layout_tree - needs proper implementation
        None
    }

    // ===== System Callbacks =====

    pub fn get_system_time_fn(&self) -> azul_core::task::GetSystemTimeCallback {
        unsafe { (*self.system_callbacks).get_system_time_fn }
    }

    pub fn get_current_time(&self) -> azul_core::task::Instant {
        let cb = self.get_system_time_fn();
        (cb.cb)()
    }
}

/// Config necessary for threading + animations to work in no_std environments
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ExternalSystemCallbacks {
    pub create_thread_fn: crate::thread::CreateThreadCallback,
    pub get_system_time_fn: azul_core::task::GetSystemTimeCallback,
}

impl ExternalSystemCallbacks {
    #[cfg(not(feature = "std"))]
    pub fn rust_internal() -> Self {
        use crate::thread::create_thread_libstd;

        Self {
            create_thread_fn: crate::thread::CreateThreadCallback {
                cb: create_thread_libstd,
            },
            get_system_time_fn: azul_core::task::GetSystemTimeCallback {
                cb: azul_core::task::get_system_time_libstd,
            },
        }
    }

    #[cfg(feature = "std")]
    pub fn rust_internal() -> Self {
        use crate::thread::create_thread_libstd;

        Self {
            create_thread_fn: crate::thread::CreateThreadCallback {
                cb: create_thread_libstd,
            },
            get_system_time_fn: azul_core::task::GetSystemTimeCallback {
                cb: azul_core::task::get_system_time_libstd,
            },
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
    /// WindowState that was (potentially) modified in the callbacks
    pub modified_window_state: Option<WindowState>,
    /// Text changes that don't require full relayout
    pub words_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, AzString>>>,
    /// Image changes (for animated images/video)
    pub images_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, (ImageRef, UpdateImageType)>>>,
    /// Clip mask changes (for vector animations)
    pub image_masks_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, ImageMask>>>,
    /// CSS property changes from callbacks
    pub css_properties_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>>,
    /// Scroll position changes from callbacks
    pub nodes_scrolled_in_callbacks:
        Option<BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, LogicalPosition>>>,
    /// Whether the focused node was changed
    pub update_focused_node: Option<Option<DomNodeId>>,
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
    /// Whether the cursor changed
    pub cursor_changed: bool,
    /// Whether stopPropagation() was called (prevents bubbling up in DOM-style event propagation)
    pub stop_propagation: bool,
    /// Whether preventDefault() was called (prevents default browser behavior)
    pub prevent_default: bool,
}

impl CallCallbacksResult {
    pub fn cursor_changed(&self) -> bool {
        self.cursor_changed
    }

    pub fn focus_changed(&self) -> bool {
        self.update_focused_node.is_some()
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
    pub data: RefAny,
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

// -- render image callback

/// Callback type that renders an OpenGL texture
///
/// **IMPORTANT**: In azul-core, this is stored as `CoreRenderImageCallbackType = usize`
/// to avoid circular dependencies. The actual function pointer is cast to usize for
/// storage in the data model, then unsafely cast back to this type when invoked.
pub type RenderImageCallbackType =
    extern "C" fn(&mut RefAny, &mut RenderImageCallbackInfo) -> ImageRef;

/// Callback that returns a rendered OpenGL texture
///
/// **IMPORTANT**: In azul-core, this is stored as `CoreRenderImageCallback` with
/// a `cb: usize` field. When creating callbacks in the data model, function pointers
/// are cast to usize. This type is used in azul-layout where we can safely work
/// with the actual function pointer type.
#[repr(C)]
pub struct RenderImageCallback {
    pub cb: RenderImageCallbackType,
}

impl_callback!(RenderImageCallback);

/// Information passed to image rendering callbacks
#[derive(Debug)]
#[repr(C)]
pub struct RenderImageCallbackInfo {
    /// The ID of the DOM node that the ImageCallback was attached to
    callback_node_id: DomNodeId,
    /// Bounds of the laid-out node
    bounds: azul_core::callbacks::HidpiAdjustedBounds,
    /// Optional OpenGL context pointer
    gl_context: *const OptionGlContextPtr,
    /// Image cache for looking up images
    image_cache: *const ImageCache,
    /// System font cache
    system_fonts: *const FcFontCache,
    /// Extension for future ABI stability (referenced data)
    _abi_ref: *const core::ffi::c_void,
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
            _abi_ref: self._abi_ref,
            _abi_mut: self._abi_mut,
        }
    }
}

impl RenderImageCallbackInfo {
    pub fn new<'a>(
        callback_node_id: DomNodeId,
        bounds: azul_core::callbacks::HidpiAdjustedBounds,
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
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    pub fn get_callback_node_id(&self) -> DomNodeId {
        self.callback_node_id
    }

    pub fn get_bounds(&self) -> azul_core::callbacks::HidpiAdjustedBounds {
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
