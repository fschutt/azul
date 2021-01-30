#![allow(dead_code)]

use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
    collections::BTreeMap,
    hash::Hash,
    ffi::c_void,
    alloc::Layout,
};
use azul_css::{
    CssProperty, LayoutPoint, OptionLayoutPoint, AzString, LayoutRect,
    LayoutSize, CssPath, OptionLayoutRect
};
use crate::{
    FastHashMap,
    app_resources::{AppResources, IdNamespace},
    styled_dom::StyledDom,
    ui_solver::{OverflowingScrollNode, LayoutedRectangle, LayoutResult},
    styled_dom::{DomId, AzNodeId, AzNodeVec},
    id_tree::NodeId,
    window::{
        WindowSize, WindowState, FullWindowState, LogicalPosition,
        LogicalSize, PhysicalSize, UpdateFocusWarning, WindowCreateOptions,
        RawWindowHandle, KeyboardState, MouseState,
    },
    task::{
        Timer, Thread, TimerId, ThreadId, AzInstantPtr,
        TerminateTimer, ThreadSender, ThreadReceiver,
    },
};
#[cfg(feature = "opengl")]
use crate::gl::{OptionTexture, GlContextPtr};

/// Specifies if the screen should be updated after the callback function has returned
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UpdateScreen {
    /// The screen does not need to redraw after the callback has been called
    DoNothing = 0,
    /// After the callback is called, the screen needs to redraw (layout() function being called again)
    RegenerateStyledDomForCurrentWindow = 1,
    /// The layout has to be re-calculated for all windows
    RegenerateStyledDomForAllWindows = 2,
}

#[derive(Debug)]
#[repr(C)]
pub struct RefCountInner {
    pub num_copies: usize,
    pub num_refs: usize,
    pub num_mutable_refs: usize,
}

impl RefCountInner {
    const fn initial() -> Self {
        Self {
            num_copies: 1,
            num_refs: 0,
            num_mutable_refs: 0,
        }
    }
}

#[derive(Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(C)]
pub struct RefCount {
    pub ptr: *const RefCountInner,
}

impl fmt::Debug for RefCount {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.downcast().fmt(f)
    }
}

impl Clone for RefCount {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
        }
    }
}

impl Drop for RefCount {
    fn drop(&mut self) {
        let previous = self.downcast().num_copies;
        if previous == 1 {
            let _ = unsafe { Box::from_raw(self.ptr as *mut RefCountInner) };
        }
    }
}

impl RefCount {

    fn new() -> Self {
        RefCount { ptr: Box::into_raw(Box::new(RefCountInner::initial())) }
    }
    fn downcast(&self) -> &RefCountInner { unsafe { &*self.ptr } }
    fn downcast_mut(&mut self) -> &mut RefCountInner { unsafe { &mut *(self.ptr as *mut RefCountInner) } }

    /// Runtime check to check whether this `RefAny` can be borrowed
    pub fn can_be_shared(&self) -> bool {
        self.downcast().num_mutable_refs == 0
    }

    /// Runtime check to check whether this `RefAny` can be borrowed mutably
    pub fn can_be_shared_mut(&self) -> bool {
        let info = self.downcast();
        info.num_mutable_refs == 0 && info.num_refs == 0
    }

    pub fn increase_ref(&mut self) {
        self.downcast_mut().num_refs += 1;
    }

    pub fn decrease_ref(&mut self) {
        self.downcast_mut().num_refs -= 1;
    }

    pub fn increase_refmut(&mut self) {
        self.downcast_mut().num_mutable_refs += 1;
    }

    pub fn decrease_refmut(&mut self) {
        self.downcast_mut().num_mutable_refs -= 1;
    }
}

struct Dummy {
    _reserved: usize,
}

static DUMMY: Dummy = Dummy { _reserved: 0 };

extern "C" fn destruct_dummy(_: *mut c_void) { }

#[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(C)]
pub struct RefAny {
    pub _internal_ptr: *const c_void,
    pub _internal_len: usize,
    pub _internal_layout_size: usize,
    pub _internal_layout_align: usize,
    pub type_id: u64,
    // Special field: in order to avoid cloning the RefAny
    pub is_dead: bool,
    pub type_name: AzString,
    pub sharing_info: RefCount,
    pub custom_destructor: extern "C" fn(*mut c_void),
}

impl_option!(RefAny, OptionRefAny, copy = false, clone = false, [Debug, Hash, PartialEq, PartialOrd, Ord, Eq]);

// the refcount of RefAny is atomic, therefore `RefAny` is not `Sync`, but it is `Send`
unsafe impl Send for RefAny { }
// necessary for rayon to work
unsafe impl Sync for RefAny { }

impl RefAny {

    /// Creates a new, type-erased pointer by casting the `T` value into a `Vec<u8>` and saving the length + type ID
    pub fn new<T: 'static>(value: T) -> Self {

        extern "C" fn default_custom_destructor<U: 'static>(ptr: &mut c_void) {
            use std::{mem, ptr};

            // note: in the default constructor, we do not need to check whether U == T

            unsafe {
                // copy the struct from the heap to the stack and call mem::drop on U to run the destructor
                let mut stack_mem = mem::zeroed::<U>();
                ptr::copy_nonoverlapping((ptr as *mut c_void) as *const U, &mut stack_mem as *mut U, mem::size_of::<U>());
                mem::drop(stack_mem);
            }
        }

        let type_name = ::std::any::type_name::<T>();
        let st = AzString::from_const_str(type_name);
        let s = Self::new_c(
            (&value as *const T) as *const c_void,
            ::std::mem::size_of::<T>(),
            Self::get_type_id_static::<T>(),
            st,
            default_custom_destructor::<T>,
        );
        ::std::mem::forget(value); // do not run the destructor of T here!
        s
    }

    pub fn new_c(ptr: *const c_void, len: usize, type_id: u64, type_name: AzString, custom_destructor: extern "C" fn(&mut c_void)) -> Self {
        use std::{alloc, ptr};

        // cast the struct as bytes
        let struct_as_bytes = unsafe { ::std::slice::from_raw_parts(ptr as *const u8, len) };

        // allocate + copy the struct to the heap
        let layout = Layout::for_value(&*struct_as_bytes);
        let heap_struct_as_bytes = unsafe { alloc::alloc(layout) };
        unsafe { ptr::copy_nonoverlapping(struct_as_bytes.as_ptr(), heap_struct_as_bytes, struct_as_bytes.len()) };

        let s = Self {
            _internal_ptr: heap_struct_as_bytes as *const c_void,
            _internal_len: len,
            _internal_layout_size: layout.size(),
            _internal_layout_align: layout.align(),
            type_id,
            is_dead: true, // NOTE: default set to true - the RefAny is not alive until "copy_into_library_memory" has been called!
            type_name,
            sharing_info: RefCount::new(),
            custom_destructor: unsafe { std::mem::transmute(custom_destructor) }, // fn(&mut c_void) and fn(*mut c_void) are the same
        };

        s
    }

    // In order to be able to modify the RefAny itself
    pub fn clone_into_library_memory(&mut self) -> Self {
        if self.is_dead {
            // does NOT bump the reference count, instead just sets the "is_dead" field to false
            Self {
                _internal_ptr: self._internal_ptr,
                _internal_len: self._internal_len,
                _internal_layout_size: self._internal_layout_size,
                _internal_layout_align: self._internal_layout_align,
                type_id: self.type_id,
                is_dead: false, // <- sets the liveness of the pointer to false
                type_name: self.type_name.clone(),
                sharing_info: self.sharing_info.clone(),
                custom_destructor: self.custom_destructor,
            }
        } else {
            self.sharing_info.downcast_mut().num_copies += 1; // bump refcount
            Self {
                _internal_ptr: self._internal_ptr,
                _internal_len: self._internal_len,
                _internal_layout_size: self._internal_layout_size,
                _internal_layout_align: self._internal_layout_align,
                type_id: self.type_id,
                is_dead: self.is_dead,
                type_name: self.type_name.clone(),
                sharing_info: self.sharing_info.clone(),
                custom_destructor: self.custom_destructor,
            }
        }
    }

    pub fn is_dummy(&self) -> bool {
        self.type_id == RefAny::get_type_id_static::<Dummy>()
    }

    pub fn is_type(&self, type_id: u64) -> bool {
        self.type_id == type_id
    }

    // Returns the typeid of `T` as a u64 (necessary because `std::any::TypeId` is not C-ABI compatible)
    #[inline]
    pub fn get_type_id_static<T: 'static>() -> u64 {
        use std::any::TypeId;
        use std::mem;

        // fast method to serialize the type id into a u64
        let t_id = TypeId::of::<T>();
        let struct_as_bytes = unsafe { ::std::slice::from_raw_parts((&t_id as *const TypeId) as *const u8, mem::size_of::<TypeId>()) };
        struct_as_bytes.into_iter().enumerate().map(|(s_pos, s)| ((*s as u64) << s_pos)).sum()
    }

    pub fn get_type_id(&self) -> u64 {
        self.type_id
    }

    pub fn get_type_name(&self) -> AzString {
        self.type_name.clone()
    }

    // Deallocates the RefAny in a safe way
    pub fn library_deallocate(mut self) {
        let _ = self.clone_into_library_memory();
        // when copy goes out of bounds, copy will run the destructor
    }
}

impl Drop for RefAny {
    fn drop(&mut self) {
        use std::alloc;
        if self.is_dead {
            // Important: if the RefAny is dead, do not run the destructor
            // nor try to access the _internal_ptr!
            return;
        } else {
            self.sharing_info.downcast_mut().num_copies -= 1;
            if self.sharing_info.downcast().num_copies == 0 {
                (self.custom_destructor)(self._internal_ptr as *mut c_void);
                unsafe { alloc::dealloc(self._internal_ptr as *mut u8, Layout::from_size_align_unchecked(self._internal_layout_size, self._internal_layout_align)); }
            }
        }
    }
}

/// This type carries no valuable semantics for WR. However, it reflects the fact that
/// clients (Servo) may generate pipelines by different semi-independent sources.
/// These pipelines still belong to the same `IdNamespace` and the same `DocumentId`.
/// Having this extra Id field enables them to generate `PipelineId` without collision.
pub type PipelineSourceId = u32;

/// Information about a scroll frame, given to the user by the framework
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ScrollPosition {
    /// How big is the scroll rect (i.e. the union of all children)?
    pub scroll_frame_rect: LayoutRect,
    /// How big is the parent container (so that things like "scroll to left edge" can be implemented)?
    pub parent_rect: LayoutedRectangle,
    /// Where (measured from the top left corner) is the frame currently scrolled to?
    pub scroll_location: LogicalPosition,
}

#[derive(Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct DocumentId {
    pub namespace_id: IdNamespace,
    pub id: u32
}

impl ::std::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DocumentId {{ ns: {}, id: {} }}", self.namespace_id, self.id)
    }
}

impl ::std::fmt::Debug for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}


#[derive(Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct PipelineId(pub PipelineSourceId, pub u32);

impl ::std::fmt::Display for PipelineId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PipelineId({}, {})", self.0, self.1)
    }
}

impl ::std::fmt::Debug for PipelineId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

static LAST_PIPELINE_ID: AtomicUsize = AtomicUsize::new(0);

impl PipelineId {
    pub const DUMMY: PipelineId = PipelineId(0, 0);

    pub fn new() -> Self {
        PipelineId(LAST_PIPELINE_ID.fetch_add(1, Ordering::SeqCst) as u32, 0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct HitTestItem {
    /// The hit point in the coordinate space of the "viewport" of the display item.
    /// The viewport is the scroll node formed by the root reference frame of the display item's pipeline.
    pub point_in_viewport: LayoutPoint,
    /// The coordinates of the original hit test point relative to the origin of this item.
    /// This is useful for calculating things like text offsets in the client.
    pub point_relative_to_item: LayoutPoint,
    /// Necessary to easily get the nearest IFrame node
    pub is_focusable: bool,
    /// If this hit is an IFrame node, stores the IFrames DomId + the origin of the IFrame
    pub is_iframe_hit: Option<(DomId, LayoutPoint)>,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ScrollHitTestItem {
    /// The hit point in the coordinate space of the "viewport" of the display item.
    /// The viewport is the scroll node formed by the root reference frame of the display item's pipeline.
    pub point_in_viewport: LayoutPoint,
    /// The coordinates of the original hit test point relative to the origin of this item.
    /// This is useful for calculating things like text offsets in the client.
    pub point_relative_to_item: LayoutPoint,
    /// If this hit is an IFrame node, stores the IFrames DomId + the origin of the IFrame
    pub scroll_node: OverflowingScrollNode,
}

/// Implements `Display, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Hash`
/// for a Callback with a `.0` field:
///
/// ```
/// struct MyCallback(fn (&T));
///
/// // impl Display, Debug, etc. for MyCallback
/// impl_callback!(MyCallback);
/// ```
///
/// This is necessary to work around for https://github.com/rust-lang/rust/issues/54508
#[macro_export]
macro_rules! impl_callback {($callback_value:ident) => (

    impl ::std::fmt::Display for $callback_value {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{:?}", self)
        }
    }

    impl ::std::fmt::Debug for $callback_value {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let callback = stringify!($callback_value);
            write!(f, "{} @ 0x{:x}", callback, self.cb as usize)
        }
    }

    impl Clone for $callback_value {
        fn clone(&self) -> Self {
            $callback_value { cb: self.cb.clone() }
        }
    }

    impl ::std::hash::Hash for $callback_value {
        fn hash<H>(&self, state: &mut H) where H: ::std::hash::Hasher {
            state.write_usize(self.cb as usize);
        }
    }

    impl PartialEq for $callback_value {
        fn eq(&self, rhs: &Self) -> bool {
            self.cb as usize == rhs.cb as usize
        }
    }

    impl PartialOrd for $callback_value {
        fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
            Some((self.cb as usize).cmp(&(other.cb as usize)))
        }
    }

    impl Ord for $callback_value {
        fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
            (self.cb as usize).cmp(&(other.cb as usize))
        }
    }

    impl Eq for $callback_value { }

    impl Copy for $callback_value { }
)}

#[allow(unused_macros)]
macro_rules! impl_get_gl_context {() => {
    /// Returns a reference-counted pointer to the OpenGL context
    #[cfg(feature = "opengl")]
    pub fn get_gl_context(&self) -> GlContextPtr {
        self.gl_context.clone()
    }
};}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct DomNodeId {
    pub dom: DomId,
    pub node: AzNodeId,
}

impl_option!(DomNodeId, OptionDomNodeId, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl DomNodeId {
    pub const ROOT: DomNodeId = DomNodeId {
        dom: DomId::ROOT_ID,
        node: AzNodeId::NONE,
    };
}
// -- layout callback

/// Callback function pointer (has to be a function pointer in
/// order to be compatible with C APIs later on).
///
/// IMPORTANT: The callback needs to deallocate the `RefAnyPtr` and `LayoutInfoPtr`,
/// otherwise that memory is leaked. If you use the official auto-generated
/// bindings, this is already done for you.
///
/// NOTE: The original callback was `fn(&self, LayoutInfo) -> Dom`
/// which then evolved to `fn(&RefAny, LayoutInfo) -> Dom`.
/// The indirection is necessary because of the memory management
/// around the C API
///
/// See azul-core/ui_state.rs:298 for how the memory is managed
/// across the callback boundary.
pub type LayoutCallbackType = extern "C" fn(&mut RefAny, LayoutInfo) -> StyledDom;

#[repr(C)]
pub struct LayoutCallback { pub cb: LayoutCallbackType }
impl_callback!(LayoutCallback);

extern "C" fn default_layout_callback(_: &mut RefAny, _: LayoutInfo) -> StyledDom { StyledDom::default() }

impl Default for LayoutCallback {
    fn default() -> Self {
        Self { cb: default_layout_callback }
    }
}
// -- normal callback

/// Stores a function pointer that is executed when the given UI element is hit
///
/// Must return an `UpdateScreen` that denotes if the screen should be redrawn.
/// The style is not affected by this, so if you make changes to the window's style
/// inside the function, the screen will not be automatically redrawn, unless you return
/// an `UpdateScreen::Redraw` from the function
#[repr(C)]
pub struct Callback { pub cb: CallbackType }
impl_callback!(Callback);

impl_option!(Callback, OptionCallback, [Debug, Eq, Copy, Clone, PartialEq, PartialOrd, Ord, Hash]);

/// Information about the callback that is passed to the callback whenever a callback is invoked
#[derive(Debug)]
#[repr(C)]
pub struct CallbackInfo {
    /// State of the current window that the callback was called on (read only!)
    current_window_state: *const FullWindowState,
    /// User-modifiable state of the window that the callback was called on
    modifiable_window_state: *mut WindowState,
    /// An Rc to the OpenGL context, in order to be able to render to OpenGL textures
    #[cfg(feature = "opengl")]
    gl_context: *const GlContextPtr,
    /// See [`AppState.resources`](./struct.AppState.html#structfield.resources)
    resources : *mut AppResources,
    /// Currently running timers (polling functions, run on the main thread)
    timers: *mut FastHashMap<TimerId, Timer>,
    /// Currently running threads (asynchronous functions running each on a different thread)
    threads: *mut FastHashMap<ThreadId, Thread>,
    /// Used to spawn new windows from callbacks. You can use `get_current_window_handle()` to spawn child windows.
    new_windows: *mut Vec<WindowCreateOptions>,
    /// Handle of the current window
    current_window_handle: *const RawWindowHandle,
    /// Currently active, layouted rectangles
    node_hierarchy: *const BTreeMap<DomId, AzNodeVec>,
    /// Sets whether the event should be propagated to the parent hit node or not
    stop_propagation: *mut bool,
    /// The callback can change the focus_target - note that the focus_target is set before the
    /// next frames' layout() function is invoked, but the current frames callbacks are not affected.
    focus_target: *mut Option<FocusTarget>,
    /// Immutable (!) reference to where the nodes are currently scrolled (current position)
    current_scroll_states: *const BTreeMap<DomId, BTreeMap<AzNodeId, ScrollPosition>>,
    /// Mutable reference to a list of CSS property changes, so that the callbacks can change CSS properties
    css_properties_changed_in_callbacks: *mut BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>,
    /// Mutable map where a user can set where he wants the nodes to be scrolled to (for the next frame)
    nodes_scrolled_in_callback: *mut BTreeMap<DomId, BTreeMap<AzNodeId, LogicalPosition>>,
    /// The ID of the DOM + the node that was hit. You can use this to query
    /// information about the node, but please don't hard-code any if / else
    /// statements based on the `NodeId`
    hit_dom_node: DomNodeId,
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was hit**.
    cursor_relative_to_item: OptionLayoutPoint,
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**.
    cursor_in_viewport: OptionLayoutPoint,
}

impl CallbackInfo {

    // this function is necessary to get rid of the lifetimes and to make CallbackInfo C-compatible
    //
    // since the call_callbacks() function is the only function
    #[cfg(feature = "opengl")]
    #[inline]
    pub fn new<'a>(
       current_window_state: &'a FullWindowState,
       modifiable_window_state: &'a mut WindowState,
       gl_context: &'a GlContextPtr,
       resources : &'a mut AppResources,
       timers: &'a mut FastHashMap<TimerId, Timer>,
       threads: &'a mut FastHashMap<ThreadId, Thread>,
       new_windows: &'a mut Vec<WindowCreateOptions>,
       current_window_handle: &'a RawWindowHandle,
       node_hierarchy: &'a BTreeMap<DomId, AzNodeVec>,
       stop_propagation: &'a mut bool,
       focus_target: &'a mut Option<FocusTarget>,
       current_scroll_states: &'a BTreeMap<DomId, BTreeMap<AzNodeId, ScrollPosition>>,
       css_properties_changed_in_callbacks: &'a mut BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>,
       nodes_scrolled_in_callback: &'a mut BTreeMap<DomId, BTreeMap<AzNodeId, LogicalPosition>>,
       hit_dom_node: DomNodeId,
       cursor_relative_to_item: OptionLayoutPoint,
       cursor_in_viewport: OptionLayoutPoint,
    ) -> Self {
        Self {
            current_window_state: current_window_state as *const FullWindowState,
            modifiable_window_state: modifiable_window_state as *mut WindowState,
            gl_context: gl_context as *const GlContextPtr,
            resources: resources as *mut AppResources,
            timers: timers as *mut FastHashMap<TimerId, Timer>,
            threads: threads as *mut FastHashMap<ThreadId, Thread>,
            new_windows: new_windows as *mut Vec<WindowCreateOptions>,
            current_window_handle: current_window_handle as *const RawWindowHandle,
            node_hierarchy: node_hierarchy as *const BTreeMap<DomId, AzNodeVec>,
            stop_propagation: stop_propagation as *mut bool,
            focus_target: focus_target as *mut Option<FocusTarget>,
            current_scroll_states: current_scroll_states as *const BTreeMap<DomId, BTreeMap<AzNodeId, ScrollPosition>>,
            css_properties_changed_in_callbacks: css_properties_changed_in_callbacks as *mut BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>,
            nodes_scrolled_in_callback: nodes_scrolled_in_callback as *mut BTreeMap<DomId, BTreeMap<AzNodeId, LogicalPosition>>,
            hit_dom_node: hit_dom_node,
            cursor_relative_to_item: cursor_relative_to_item,
            cursor_in_viewport: cursor_in_viewport,
        }
    }

    fn internal_get_current_window_state<'a>(&'a self) -> &'a FullWindowState { unsafe { &*self.current_window_state } }
    fn internal_get_modifiable_window_state<'a>(&'a mut self)-> &'a mut WindowState { unsafe { &mut *self.modifiable_window_state } }
    fn internal_get_gl_context<'a>(&'a self) -> &'a GlContextPtr { unsafe { &*self.gl_context } }
    fn internal_get_resources<'a>(&'a self) -> &'a mut AppResources { unsafe { &mut *self.resources } }
    fn internal_get_timers<'a>(&'a self) -> &'a mut FastHashMap<TimerId, Timer> { unsafe { &mut *self.timers } }
    fn internal_get_threads<'a>(&'a self) -> &'a mut FastHashMap<ThreadId, Thread> { unsafe { &mut *self.threads } }
    fn internal_get_new_windows<'a>(&'a self) -> &'a mut Vec<WindowCreateOptions> { unsafe { &mut *self.new_windows } }
    fn internal_get_current_window_handle<'a>(&'a self) -> &'a RawWindowHandle { unsafe { &*self.current_window_handle } }
    fn internal_get_node_hierarchy<'a>(&'a self) -> &'a BTreeMap<DomId, AzNodeVec> { unsafe { &*self.node_hierarchy } }
    fn internal_get_stop_propagation<'a>(&'a self) -> &'a mut bool { unsafe { &mut *self.stop_propagation } }
    fn internal_get_focus_target<'a>(&'a self) -> &'a mut Option<FocusTarget> { unsafe { &mut *self.focus_target } }
    fn internal_get_current_scroll_states<'a>(&'a self) -> &'a BTreeMap<DomId, BTreeMap<AzNodeId, ScrollPosition>> { unsafe { &*self.current_scroll_states } }
    fn internal_get_css_properties_changed_in_callbacks<'a>(&'a self) -> &'a mut BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>> { unsafe { &mut *self.css_properties_changed_in_callbacks } }
    fn internal_get_nodes_scrolled_in_callback<'a>(&'a self) -> &'a mut BTreeMap<DomId, BTreeMap<AzNodeId, LogicalPosition>> { unsafe { &mut *self.nodes_scrolled_in_callback } }
    fn internal_get_hit_dom_node<'a>(&'a self) -> DomNodeId { self.hit_dom_node }
    fn internal_get_cursor_relative_to_item<'a>(&'a self) -> OptionLayoutPoint { self.cursor_relative_to_item }
    fn internal_get_cursor_in_viewport<'a>(&'a self) -> OptionLayoutPoint { self.cursor_in_viewport }

    pub fn get_hit_node(&self) -> DomNodeId { self.internal_get_hit_dom_node() }

    pub fn get_cursor_relative_to_node(&self) -> OptionLayoutPoint { self.internal_get_cursor_relative_to_item() }

    pub fn get_cursor_relative_to_viewport(&self) -> OptionLayoutPoint { self.internal_get_cursor_in_viewport() }

    pub fn get_window_state(&self) -> WindowState { self.internal_get_current_window_state().clone().into() }

    pub fn get_keyboard_state(&self) -> KeyboardState { self.internal_get_current_window_state().keyboard_state.clone() }

    pub fn get_mouse_state(&self) -> MouseState { self.internal_get_current_window_state().mouse_state.clone() }

    pub fn get_current_window_handle(&self) -> RawWindowHandle { self.internal_get_current_window_handle().clone() }

    pub fn get_gl_context(&self) -> GlContextPtr { self.internal_get_gl_context().clone() }

    pub fn get_parent(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        self.internal_get_node_hierarchy()
        .get(&node_id.dom)
        .and_then(|node_hierarchy| node_hierarchy.as_container().get(node_id.node.into_crate_internal()?)?.parent_id())
        .map(|nid| DomNodeId { dom: node_id.dom, node: AzNodeId::from_crate_internal(Some(nid)) })
    }

    pub fn get_previous_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        self.internal_get_node_hierarchy()
        .get(&node_id.dom)
        .and_then(|node_hierarchy| node_hierarchy.as_container().get(node_id.node.into_crate_internal()?)?.previous_sibling_id())
        .map(|nid| DomNodeId { dom: node_id.dom, node: AzNodeId::from_crate_internal(Some(nid)) })
    }

    pub fn get_next_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        self.internal_get_node_hierarchy()
        .get(&node_id.dom)
        .and_then(|node_hierarchy| node_hierarchy.as_container().get(node_id.node.into_crate_internal()?)?.next_sibling_id())
        .map(|nid| DomNodeId { dom: node_id.dom, node: AzNodeId::from_crate_internal(Some(nid)) })
    }

    pub fn get_first_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        self.internal_get_node_hierarchy()
        .get(&node_id.dom)
        .and_then(|node_hierarchy| node_hierarchy.as_container().get(node_id.node.into_crate_internal()?)?.first_child_id())
        .map(|nid| DomNodeId { dom: node_id.dom, node: AzNodeId::from_crate_internal(Some(nid)) })
    }

    pub fn get_last_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        self.internal_get_node_hierarchy()
        .get(&node_id.dom)
        .and_then(|node_hierarchy| node_hierarchy.as_container().get(node_id.node.into_crate_internal()?)?.last_child_id())
        .map(|nid| DomNodeId { dom: node_id.dom, node: AzNodeId::from_crate_internal(Some(nid)) })
    }

    /*
    pub fn get_dataset(&mut self, node_id: DomNodeId) -> OptionRefAny {
        self.internal_get_layout_results()
        .get_mut(node_id.dom.inner as usize)
        .and_then(|layout_result| match &mut layout_result.styled_dom.node_data.as_container_mut().get_mut(node_id.node.into_crate_internal()?)?.dataset {
            OptionRefAny::Some(s) => Some(s.clone_into_library_memory()),
            OptionRefAny::None => None,
        })
        .into()
    }
    */

    pub fn set_window_state(&mut self, new_state: WindowState) { *self.internal_get_modifiable_window_state() = new_state; }

    pub fn set_css_property(&mut self, node_id: DomNodeId, prop: CssProperty) {
        if let Some(nid) = node_id.node.into_crate_internal() {
            self.internal_get_css_properties_changed_in_callbacks()
            .entry(node_id.dom)
            .or_insert_with(|| BTreeMap::new())
            .entry(nid)
            .or_insert_with(|| Vec::new()).push(prop);
        }
    }

    pub fn set_focus(&mut self, target: FocusTarget) {
        *self.internal_get_focus_target() = Some(target);
    }

    pub fn stop_propagation(&mut self) {
        *self.internal_get_stop_propagation() = true;
    }

    pub fn create_window(&mut self, window: WindowCreateOptions) {
        self.internal_get_new_windows().push(window);
    }

    pub fn start_thread(&mut self, id: ThreadId, thread_initialize_data: RefAny, writeback_data: RefAny, callback: ThreadCallbackType) {
        let thread = Thread::new(thread_initialize_data, writeback_data, callback);
        self.internal_get_threads().insert(id, thread);
    }

    pub fn start_timer(&mut self, id: TimerId, timer: Timer) {
        self.internal_get_timers().insert(id, timer);
    }

    // pub fn add_font_source()
    // pub fn remove_font_source()
    // pub fn add_image_source()
    // pub fn remove_image_source()

    // pub fn exchange_image_source(old_image_source, new_image_source)
    // pub fn exchange_image_mask(old_image_mask, new_image_mask)
}


pub type CallbackReturn = UpdateScreen;
pub type CallbackType = extern "C" fn(&mut RefAny, CallbackInfo) -> CallbackReturn;

// -- opengl callback

/// Callbacks that returns a rendered OpenGL texture
#[cfg(feature = "opengl")]
#[repr(C)]
pub struct GlCallback { pub cb: GlCallbackType }
#[cfg(feature = "opengl")]
impl_callback!(GlCallback);

#[derive(Debug)]
#[repr(C)]
pub struct GlCallbackInfo {
    #[cfg(feature = "opengl")]
    gl_context: *const GlContextPtr,
    resources: *const AppResources,
    bounds: HidpiAdjustedBounds,
}

#[cfg(feature = "opengl")]
pub type GlCallbackType = extern "C" fn(&RefAny, GlCallbackInfo) -> GlCallbackReturn;

impl GlCallbackInfo {
    pub fn new<'a>(
       gl_context: &'a GlContextPtr,
       resources: &'a AppResources,
       bounds: HidpiAdjustedBounds,
    ) -> Self {
        Self {
            gl_context: gl_context as *const GlContextPtr,
            resources: resources as *const AppResources,
            bounds
        }
    }

    pub fn get_gl_context(&self) -> GlContextPtr { self.internal_get_gl_context().clone() }
    pub fn get_bounds(&self) -> HidpiAdjustedBounds { self.internal_get_bounds() }

    // fn get_font()
    // fn get_image()

    fn internal_get_gl_context<'a>(&'a self) -> &'a GlContextPtr { unsafe { &*self.gl_context } }
    fn internal_get_resources<'a>(&'a self) -> &'a AppResources { unsafe { &*self.resources } }
    fn internal_get_bounds<'a>(&'a self) -> HidpiAdjustedBounds { self.bounds }
}

#[cfg(feature = "opengl")]
#[repr(C)]
#[derive(Debug)]
pub struct GlCallbackReturn { pub texture: OptionTexture }


// -- iframe callback

pub type IFrameCallbackType = extern "C" fn(&mut RefAny, IFrameCallbackInfo) -> IFrameCallbackReturn;

/// Callback that, given a rectangle area on the screen, returns the DOM
/// appropriate for that bounds (useful for infinite lists)
#[repr(C)]
pub struct IFrameCallback { pub cb: IFrameCallbackType }
impl_callback!(IFrameCallback);

#[derive(Debug)]
#[repr(C)]
pub struct IFrameCallbackInfo {
    pub resources: *const AppResources,
    pub bounds: HidpiAdjustedBounds,
}

impl IFrameCallbackInfo {
    pub fn new<'a>(
       resources: &'a AppResources,
       bounds: HidpiAdjustedBounds,
    ) -> Self {
        Self {
            resources: resources as *const AppResources,
            bounds
        }
    }

    pub fn get_bounds(&self) -> HidpiAdjustedBounds { self.internal_get_bounds() }

    // fn get_font()
    // fn get_image()

    fn internal_get_resources<'a>(&'a self) -> &'a AppResources { unsafe { &*self.resources } }
    fn internal_get_bounds<'a>(&'a self) -> HidpiAdjustedBounds { self.bounds }
}

#[derive(Debug, PartialEq)]
#[repr(C)]
pub struct IFrameCallbackReturn {
    pub styled_dom: StyledDom,
    pub size: LayoutRect,
    pub virtual_size: OptionLayoutRect,
}

// --  thread callback
pub type ThreadCallbackType = extern "C" fn(RefAny, ThreadSender, ThreadReceiver);

#[repr(C)]
pub struct ThreadCallback { pub cb: ThreadCallbackType }
impl_callback!(ThreadCallback);

// -- timer callback

/// Callback that can runs on every frame on the main thread - can modify the app data model
#[repr(C)]
pub struct TimerCallback { pub cb: TimerCallbackType }
impl_callback!(TimerCallback);

#[derive(Debug)]
#[repr(C)]
pub struct TimerCallbackInfo {
    /// Callback info for this timer
    pub callback_info: CallbackInfo,
    /// Time when the frame was started rendering
    pub frame_start: AzInstantPtr,
    /// How many times this callback has been called
    pub call_count: usize,
}

pub type WriteBackCallbackType = extern "C" fn(/* original data */ &mut RefAny, /*data to write back*/ RefAny, CallbackInfo) -> UpdateScreen;

/// Callback that can runs when a thread receives a `WriteBack` message
#[repr(C)]
pub struct WriteBackCallback { pub cb: WriteBackCallbackType }
impl_callback!(WriteBackCallback);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TimerCallbackReturn {
    pub should_update: UpdateScreen,
    pub should_terminate: TerminateTimer,
}

pub type TimerCallbackType = extern "C" fn(/* application data */ &mut RefAny, /* timer internal data */ &mut RefAny, TimerCallbackInfo) -> TimerCallbackReturn;

/// Gives the `layout()` function access to the `AppResources` and the `Window`
/// (for querying images and fonts, as well as width / height)
#[derive(Debug)]
#[repr(C)]
pub struct LayoutInfo {
    /// Window size (so that apps can return a different UI depending on
    /// the window size - mobile / desktop view). Should be later removed
    /// in favor of "resize" handlers and @media queries.
    window_size: *const WindowSize,
    /// Optimization for resizing: If a DOM has no Iframes and the window size
    /// does not change the state of the UI, then resizing the window will not
    /// result in calls to the .layout() function (since the resulting UI would
    /// stay the same).
    ///
    /// Stores "stops" in logical pixels where the UI needs to be re-generated
    /// should the width of the window change.
    window_size_width_stops: *mut Vec<f32>,
    /// Same as `window_size_width_stops` but for the height of the window.
    window_size_height_stops: *mut Vec<f32>,
    /// Allows the layout() function to reference app resources such as FontIDs or ImageIDs
    resources: *const AppResources,
}

impl LayoutInfo {

    pub fn new<'a>(
        window_size: &'a WindowSize,
        window_size_width_stops: &'a mut Vec<f32>,
        window_size_height_stops: &'a mut Vec<f32>,
        resources: &'a AppResources,
    ) -> Self {
        Self {
            window_size: window_size as *const WindowSize,
            window_size_width_stops: window_size_width_stops as *mut Vec<f32>,
            window_size_height_stops: window_size_height_stops as *mut Vec<f32>,
            resources: resources as *const AppResources,
        }
    }

    fn internal_get_window_size<'a>(&'a self) -> &'a WindowSize { unsafe { &*self.window_size } }
    fn internal_get_window_size_width_stops<'a>(&'a self) -> &'a mut Vec<f32> { unsafe { &mut *self.window_size_width_stops } }
    fn internal_get_window_size_height_stops<'a>(&'a self) -> &'a mut Vec<f32> { unsafe { &mut *self.window_size_height_stops } }
    fn internal_get_resources<'a>(&'a self) -> &'a AppResources { unsafe { &*self.resources } }

    /// Returns whether the window width is larger than `width`,
    /// but sets an internal "dirty" flag - so that the UI is re-generated when
    /// the window is resized above or below `width`.
    ///
    /// For example:
    ///
    /// ```rust,no_run,ignore
    /// fn layout(info: LayoutInfo<T>) -> Dom {
    ///     if info.window_width_larger_than(720.0) {
    ///         render_desktop_ui()
    ///     } else {
    ///         render_mobile_ui()
    ///     }
    /// }
    /// ```
    ///
    /// Here, the UI is dependent on the width of the window, so if the window
    /// resizes above or below 720px, the `layout()` function needs to be called again.
    /// Internally Azul stores the `720.0` and only calls the `.layout()` function
    /// again if the window resizes above or below the value.
    ///
    /// NOTE: This should be later depreceated into `On::Resize` handlers and
    /// `@media` queries.
    pub fn window_width_larger_than(&mut self, width: f32) -> bool {
        self.internal_get_window_size_width_stops().push(width);
        self.internal_get_window_size().get_logical_size().width > width
    }

    pub fn window_width_smaller_than(&mut self, width: f32) -> bool {
        self.internal_get_window_size_width_stops().push(width);
        self.internal_get_window_size().get_logical_size().width < width
    }

    pub fn window_height_larger_than(&mut self, height: f32) -> bool {
        self.internal_get_window_size_height_stops().push(height);
        self.internal_get_window_size().get_logical_size().height > height
    }

    pub fn window_height_smaller_than(&mut self, height: f32) -> bool {
        self.internal_get_window_size_height_stops().push(height);
        self.internal_get_window_size().get_logical_size().height < height
    }
}

/// Information about the bounds of a laid-out div rectangle.
///
/// Necessary when invoking `IFrameCallbacks` and `GlCallbacks`, so
/// that they can change what their content is based on their size.
#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct HidpiAdjustedBounds {
    pub logical_size: LogicalSize,
    pub hidpi_factor: f32,
}

impl HidpiAdjustedBounds {

    #[inline(always)]
    pub fn from_bounds(bounds: LayoutSize, hidpi_factor: f32) -> Self {
        let logical_size = LogicalSize::new(bounds.width as f32, bounds.height as f32);
        Self {
            logical_size,
            hidpi_factor,
        }
    }

    pub fn get_physical_size(&self) -> PhysicalSize<u32> {
        self.get_logical_size().to_physical(self.hidpi_factor)
    }

    pub fn get_logical_size(&self) -> LogicalSize {
        // NOTE: hidpi factor, not system_hidpi_factor!
        LogicalSize::new(
            self.logical_size.width * self.hidpi_factor,
            self.logical_size.height * self.hidpi_factor
        )
    }

    pub fn get_hidpi_factor(&self) -> f32 {
        self.hidpi_factor
    }
}

/// Defines the focus_targeted node ID for the next frame
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum FocusTarget {
    Id(DomNodeId),
    Path(FocusTargetPath),
    PreviousFocusItem,
    NextFocusItem,
    NoFocus,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FocusTargetPath {
    pub dom: DomId,
    pub css_path: CssPath,
}

impl FocusTarget {
    pub fn resolve(&self, layout_results: &[LayoutResult]) -> Result<Option<DomNodeId>, UpdateFocusWarning> {

        use crate::callbacks::FocusTarget::*;
        use crate::style::matches_html_element;

        match self {
            Id(dom_node_id) => {
                let layout_result = layout_results.get(dom_node_id.dom.inner).ok_or(UpdateFocusWarning::FocusInvalidDomId(dom_node_id.dom.clone()))?;
                let node_is_valid = dom_node_id.node
                    .into_crate_internal()
                    .map(|o| layout_result.styled_dom.node_data.as_container().get(o).is_some())
                    .unwrap_or(false);

                if !node_is_valid {
                    Err(UpdateFocusWarning::FocusInvalidNodeId(dom_node_id.node.clone()))
                } else {
                    Ok(Some(dom_node_id.clone()))
                }
            },
            NoFocus => Ok(None),
            PreviousFocusItem => Ok(None), // TODO - select the next focusable element or `None` if this was the first focusable element in the DOM
            NextFocusItem => Ok(None), // TODO - select the previous focusable element or `None` if this was the last focusable element in the DOM
            Path(FocusTargetPath { dom, css_path }) => {
                let layout_result = layout_results.get(dom.inner).ok_or(UpdateFocusWarning::FocusInvalidDomId(dom.clone()))?;
                let html_node_tree = &layout_result.styled_dom.cascade_info;
                let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
                let node_data = &layout_result.styled_dom.node_data;
                let resolved_node_id = html_node_tree
                    .as_container()
                    .linear_iter()
                    .find(|node_id| matches_html_element(css_path, *node_id, &node_hierarchy.as_container(), &node_data.as_container(), &html_node_tree.as_container()))
                    .ok_or(UpdateFocusWarning::CouldNotFindFocusNode(css_path.clone()))?;
                Ok(Some(DomNodeId { dom: dom.clone(), node: AzNodeId::from_crate_internal(Some(resolved_node_id)) }))
            },
        }
    }
}
