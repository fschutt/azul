use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
    collections::BTreeMap,
    hash::Hash,
    ffi::c_void,
    alloc::Layout,
};
use azul_css::{LayoutPoint, OptionLayoutPoint, AzString, LayoutRect, LayoutSize, CssPath};
#[cfg(feature = "css_parser")]
use azul_css_parser::CssPathParseError;
use crate::{
    FastHashMap,
    app_resources::{AppResources, IdNamespace, Words, WordPositions, ShapedWords, LayoutedGlyphs},
    dom::{Dom, OptionDom, TagId, NodeType, NodeData},
    display_list::CachedDisplayList,
    styled_dom::StyledDom,
    ui_solver::{OverflowingScrollNode, PositionedRectangle, LayoutedRectangle, ScrolledNodes, LayoutResult},
    id_tree::{Node, NodeId},
    styled_dom::{DomId, AzNodeId},
    window::{
        WindowSize, WindowState, FullWindowState, LogicalPosition,
        KeyboardState, MouseState, LogicalSize, PhysicalSize,
        UpdateFocusWarning, CallCallbacksResult, ScrollStates,
    },
    task::{Timer, DropCheckPtr, TerminateTimer, ArcMutexRefAnyPtr, Task, TimerId},
};

#[cfg(feature = "opengl")]
use crate::gl::{OptionTexture, GlContextPtr};

/// Specifies if the screen should be updated after the callback function has returned
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UpdateScreen {
    /// After the callback is called, the screen needs to redraw (layout() function being called again)
    Redraw,
    /// The screen does not need to redraw after the callback has been called
    DontRedraw,
    /// The callback only changed the scroll positions of nodes (no re-layout is done)
    UpdateScrollStates,
    /// Update **scroll states + GPU transforms** and redraw (no re-layout is done)
    UpdateTransforms,
}

impl UpdateScreen {
    pub fn into_option(self) -> Option<()> { self.into() }
}

impl From<UpdateScreen> for Option<()> {
    fn from(o: UpdateScreen) -> Option<()> {
        match o { UpdateScreen::DontRedraw => None, _ => Some(()) }
    }
}

impl<T> From<Option<T>> for UpdateScreen {
    fn from(o: Option<T>) -> Self {
        match o { None => UpdateScreen::DontRedraw, Some(_) => UpdateScreen::Redraw }
    }
}

#[repr(C)]
pub struct RefAnySharingInfoInner {
    pub num_copies: AtomicUsize,
    pub num_refs: AtomicUsize,
    pub num_mutable_refs: AtomicUsize,
}

impl RefAnySharingInfoInner {
    const fn initial() -> Self {
        Self {
            num_copies: AtomicUsize::new(1),
            num_refs: AtomicUsize::new(0),
            num_mutable_refs: AtomicUsize::new(0),
        }
    }
}

#[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(C)]
pub struct RefAnySharingInfo {
    pub ptr: *const c_void, /* *const RefAnySharingInfoInner */
}

impl Drop for RefAnySharingInfo {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.ptr as *mut RefAnySharingInfoInner) };
    }
}

impl RefAnySharingInfo {

    fn new(r: RefAnySharingInfoInner) -> Self {
        RefAnySharingInfo { ptr: Box::into_raw(Box::new(r)) as *const c_void }
    }
    fn downcast(&self) -> &RefAnySharingInfoInner { unsafe { &*(self.ptr as *const RefAnySharingInfoInner) } }
    fn downcast_mut(&mut self) -> &mut RefAnySharingInfoInner { unsafe { &mut *(self.ptr as *mut RefAnySharingInfoInner) } }

    /// Runtime check to check whether this `RefAny` can be borrowed
    pub fn can_be_shared(&self) -> bool {
        self.downcast().num_mutable_refs.load(Ordering::SeqCst) == 0
    }

    /// Runtime check to check whether this `RefAny` can be borrowed mutably
    pub fn can_be_shared_mut(&self) -> bool {
        let info = self.downcast();
        info.num_mutable_refs.load(Ordering::SeqCst) == 0 && info.num_refs.load(Ordering::SeqCst) == 0
    }

    pub fn increase_ref(&mut self) {
        self.downcast_mut().num_refs.fetch_add(1, Ordering::SeqCst);
    }

    pub fn decrease_ref(&mut self) {
        self.downcast_mut().num_refs.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn increase_refmut(&mut self) {
        self.downcast_mut().num_mutable_refs.fetch_add(1, Ordering::SeqCst);
    }

    pub fn decrease_refmut(&mut self) {
        self.downcast_mut().num_mutable_refs.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(C)]
pub struct RefAny {
    pub _internal_ptr: *const c_void,
    pub _internal_len: usize,
    pub _internal_layout_size: usize,
    pub _internal_layout_align: usize,
    pub type_id: u64,
    pub type_name: AzString,
    pub _sharing_info_ptr: *const RefAnySharingInfo,
    pub custom_destructor: extern "C" fn(*const c_void),
}

// the refcount of RefAny is atomic, therefore `RefAny` is not `Send`
// however, RefAny is not Sync, since the data access is not protected by a lock!
unsafe impl Send for RefAny { }

impl Clone for RefAny {
    fn clone(&self) -> Self {
        unsafe { (&mut *(self._sharing_info_ptr as *mut RefAnySharingInfo)).downcast_mut().num_copies.fetch_add(1, Ordering::SeqCst); };
        Self {
            _internal_ptr: self._internal_ptr,
            _internal_len: self._internal_len,
            _internal_layout_size: self._internal_layout_size,
            _internal_layout_align: self._internal_layout_align,
            type_id: self.type_id,
            type_name: self.type_name.clone(),
            _sharing_info_ptr: self._sharing_info_ptr,
            custom_destructor: self.custom_destructor,
        }
    }
}

impl RefAny {

    pub fn new_c(ptr: *const c_void, len: usize, type_id: u64, type_name: AzString, custom_destructor: extern "C" fn(*const c_void)) -> Self {
        use std::{alloc, ptr};

        // cast the struct as bytes
        let struct_as_bytes = unsafe { ::std::slice::from_raw_parts(ptr as *const u8, len) };

        // allocate + copy the struct to the heap
        let layout = Layout::for_value(&*struct_as_bytes);
        let heap_struct_as_bytes = unsafe { alloc::alloc(layout) };
        unsafe { ptr::copy_nonoverlapping(struct_as_bytes.as_ptr(), heap_struct_as_bytes, struct_as_bytes.len()) };

        let sharing_info_ptr = Box::into_raw(Box::new(RefAnySharingInfo::new(RefAnySharingInfoInner::initial())));

        let s = Self {
            _internal_ptr: heap_struct_as_bytes as *const c_void,
            _internal_len: len,
            _internal_layout_size: layout.size(),
            _internal_layout_align: layout.align(),
            type_id,
            type_name,
            _sharing_info_ptr: sharing_info_ptr,
            custom_destructor,
        };

        s
    }

    pub fn is_type(&self, type_id: u64) -> bool {
        self.type_id == type_id
    }

    pub fn get_type_id(&self) -> u64 {
        self.type_id
    }

    pub fn get_type_name(&self) -> AzString {
        self.type_name.clone()
    }

    /// Runtime check to check whether this `RefAny` can be borrowed
    pub fn can_be_shared(&self) -> bool {
        let info = unsafe { &*self._sharing_info_ptr };
        info.can_be_shared()
    }

    /// Runtime check to check whether this `RefAny` can be borrowed mutably
    pub fn can_be_shared_mut(&self) -> bool {
        let info = unsafe { &mut *(self._sharing_info_ptr as *mut RefAnySharingInfo) };
        info.can_be_shared_mut()
    }

    pub fn increase_ref(&self) {
        let info = unsafe { &mut *(self._sharing_info_ptr as *mut RefAnySharingInfo) };
        info.increase_ref()
    }

    pub fn decrease_ref(&self) {
        let info = unsafe { &mut *(self._sharing_info_ptr as *mut RefAnySharingInfo) };
        info.decrease_ref()
    }

    pub fn increase_refmut(&self) {
        let info = unsafe { &mut *(self._sharing_info_ptr as *mut RefAnySharingInfo) };
        info.increase_refmut()
    }

    pub fn decrease_refmut(&self) {
        let info = unsafe { &mut *(self._sharing_info_ptr as *mut RefAnySharingInfo) };
        info.decrease_refmut()
    }
}

impl Drop for RefAny {
    fn drop(&mut self) {
        use std::alloc;
        let info = unsafe { &*self._sharing_info_ptr };
        if info.downcast().num_copies.load(Ordering::SeqCst) <= 1 {
            (self.custom_destructor)(self._internal_ptr);
            unsafe { alloc::dealloc(self._internal_ptr as *mut u8, Layout::from_size_align_unchecked(self._internal_layout_size, self._internal_layout_align)); }
            unsafe { let _ = Box::from_raw(self._sharing_info_ptr as *mut RefAnySharingInfo); }
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

macro_rules! impl_get_gl_context {() => {
    /// Returns a reference-counted pointer to the OpenGL context
    #[cfg(feature = "opengl")]
    pub fn get_gl_context(&self) -> GlContextPtr {
        self.gl_context.clone()
    }
};}

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
pub type LayoutCallbackType = extern "C" fn(RefAny, LayoutInfoPtr) -> StyledDom;

#[repr(C)]
pub struct LayoutCallback { pub cb: LayoutCallbackType }
impl_callback!(LayoutCallback);

extern "C" fn default_layout_callback(_: RefAny, _: LayoutInfoPtr) -> StyledDom { StyledDom::default() }

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

/// Information about the callback that is passed to the callback whenever a callback is invoked
pub struct CallbackInfo<'a> {
    /// Your data (the global struct which all callbacks will have access to)
    pub state: &'a RefAny,
    /// State of the current window that the callback was called on (read only!)
    pub current_window_state: &'a FullWindowState,
    /// User-modifiable state of the window that the callback was called on
    pub modifiable_window_state: &'a mut WindowState,
    /// An Rc to the OpenGL context, in order to be able to render to OpenGL textures
    #[cfg(feature = "opengl")]
    pub gl_context: &'a GlContextPtr,
    /// See [`AppState.resources`](./struct.AppState.html#structfield.resources)
    pub resources : &'a mut AppResources,
    /// Currently running timers (polling functions, run on the main thread)
    pub timers: &'a mut FastHashMap<TimerId, Timer>,
    /// Currently running tasks (asynchronous functions running each on a different thread)
    pub tasks: &'a mut Vec<Task>,
    /// Currently active, layouted rectangles
    pub layout_results: &'a [LayoutResult],
    /// Sets whether the event should be propagated to the parent hit node or not
    pub stop_propagation: &'a mut bool,
    /// The callback can change the focus_target - note that the focus_target is set before the
    /// next frames' layout() function is invoked, but the current frames callbacks are not affected.
    pub focus_target: &'a mut Option<FocusTarget>,
    /// Immutable (!) reference to where the nodes are currently scrolled (current position)
    pub current_scroll_states: &'a BTreeMap<DomId, BTreeMap<AzNodeId, ScrollPosition>>,
    /// Mutable map where a user can set where he wants the nodes to be scrolled to (for the next frame)
    pub nodes_scrolled_in_callback: &'a mut BTreeMap<DomId, BTreeMap<AzNodeId, LogicalPosition>>,
    /// The ID of the DOM + the node that was hit. You can use this to query
    /// information about the node, but please don't hard-code any if / else
    /// statements based on the `NodeId`
    pub hit_dom_node: DomNodeId,
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was hit**.
    pub cursor_relative_to_item: Option<LayoutPoint>,
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**.
    pub cursor_in_viewport: Option<LayoutPoint>,
}

/// Pointer to rust-allocated `Box<CallbackInfo<'a>>` struct
#[repr(C)] pub struct CallbackInfoPtr { pub ptr: *mut c_void }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct DomNodeId {
    pub dom: DomId,
    pub node: AzNodeId,
}

impl_option!(DomNodeId, OptionDomNodeId, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl CallbackInfoPtr {

    pub fn get_hit_node<'a>(&'a self) -> DomNodeId {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).hit_dom_node }
    }

    pub fn get_parent<'a>(&'a self, node_id: DomNodeId) -> OptionDomNodeId {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).layout_results }
        .get(node_id.dom.inner as usize)
        .and_then(|layout_result| layout_result.styled_dom.node_hierarchy.as_container().get(node_id.node.into_crate_internal()?)?.parent_id())
        .map(|nid| DomNodeId { dom: node_id.dom, node: AzNodeId::from_crate_internal(Some(nid)) })
        .into()
    }

    pub fn get_previous_sibling<'a>(&'a self, node_id: DomNodeId) -> OptionDomNodeId {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).layout_results }
        .get(node_id.dom.inner as usize)
        .and_then(|layout_result| layout_result.styled_dom.node_hierarchy.as_container().get(node_id.node.into_crate_internal()?)?.previous_sibling_id())
        .map(|nid| DomNodeId { dom: node_id.dom, node: AzNodeId::from_crate_internal(Some(nid)) })
        .into()
    }

    pub fn get_next_sibling<'a>(&'a self, node_id: DomNodeId) -> OptionDomNodeId {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).layout_results }
        .get(node_id.dom.inner as usize)
        .and_then(|layout_result| layout_result.styled_dom.node_hierarchy.as_container().get(node_id.node.into_crate_internal()?)?.next_sibling_id())
        .map(|nid| DomNodeId { dom: node_id.dom, node: AzNodeId::from_crate_internal(Some(nid)) })
        .into()
    }

    pub fn get_first_child<'a>(&'a self, node_id: DomNodeId) -> OptionDomNodeId {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).layout_results }
        .get(node_id.dom.inner as usize)
        .and_then(|layout_result| layout_result.styled_dom.node_hierarchy.as_container().get(node_id.node.into_crate_internal()?)?.first_child_id())
        .map(|nid| DomNodeId { dom: node_id.dom, node: AzNodeId::from_crate_internal(Some(nid)) })
        .into()
    }

    pub fn node_is_type<'a>(&'a self, node_id: DomNodeId, node_type: NodeType) -> bool {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).layout_results }
        .get(node_id.dom.inner as usize)
        .and_then(|layout_result| Some(layout_result.styled_dom.node_data.as_container().get(node_id.node.into_crate_internal()?)?.is_node_type(node_type)))
        .unwrap_or(false)
    }

    pub fn node_has_id<'a>(&'a self, node_id: DomNodeId, id: AzString) -> bool {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).layout_results }
        .get(node_id.dom.inner as usize)
        .and_then(|layout_result| Some(layout_result.styled_dom.node_data.as_container().get(node_id.node.into_crate_internal()?)?.has_id(id.as_str())))
        .unwrap_or(false)
    }

    pub fn node_has_class<'a>(&'a self, node_id: DomNodeId, class: AzString) -> bool {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).layout_results }
        .get(node_id.dom.inner as usize)
        .and_then(|layout_result| Some(layout_result.styled_dom.node_data.as_container().get(node_id.node.into_crate_internal()?)?.has_class(class.as_str())))
        .unwrap_or(false)
    }

    pub fn get_current_window_state<'a>(&'a self) -> FullWindowState {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).current_window_state.clone() }
    }

    pub fn get_cursor_in_viewport<'a>(&'a self) -> OptionLayoutPoint {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).cursor_in_viewport.into() }
    }

    pub fn get_cursor_relative_to_item<'a>(&'a self) -> OptionLayoutPoint {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).cursor_relative_to_item.into() }
    }

    // pub fn add_font_source()
    // pub fn remove_font_source()
    // pub fn add_image_source()
    // pub fn remove_image_source()
    // pub fn render_gl_image_mask(curve: [BezierCurve], mask_width: usize, mask_height: usize) -> ImageMask

    // put fn set_gpu_transform(&mut self, node: NodeId, transform: GpuTransform) { }
    // put fn set_gpu_opacity(&mut self, node: NodeId, opacity: f32) { }
    // put fn set_gpu_rotation(&mut self, node: NodeId, rotation: f32) { }
    // put fn set_gpu_scale(&mut self, node: NodeId, scale: f32) { }
    // pub fn exchange_image_source(old_image_source, new_image_source)
    // pub fn exchange_image_mask(old_image_mask, new_image_mask)

    pub fn add_task<'a>(&'a mut self, task: Task) {
        let self_ptr = self.ptr as *mut CallbackInfo<'a>;
        unsafe { (*self_ptr).tasks.push(task); }
    }

    pub fn add_timer<'a>(&'a mut self, timer_id: TimerId, timer: Timer) {
        let self_ptr = self.ptr as *mut CallbackInfo<'a>;
        unsafe { (*self_ptr).timers.insert(timer_id, timer); }
    }

    pub fn set_window_state<'a>(&'a mut self, new_window_state: WindowState) {
        let self_ptr = self.ptr as *mut CallbackInfo<'a>;
        unsafe { *(*self_ptr).modifiable_window_state = new_window_state; }
    }

    pub fn stop_propagation<'a>(&'a mut self) {
        let self_ptr = self.ptr as *mut CallbackInfo<'a>;
        unsafe { *(*self_ptr).stop_propagation = true; }
    }

    pub fn get_gl_context<'a>(&'a self) -> GlContextPtr {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        unsafe { (*self_ptr).gl_context.clone() }
    }
}

impl Drop for CallbackInfoPtr {
    fn drop<'a>(&mut self) {
        let _ = unsafe { Box::<CallbackInfo<'a>>::from_raw(self.ptr as *mut CallbackInfo<'a>) };
    }
}

impl std::fmt::Debug for CallbackInfoPtr {
    fn fmt<'a>(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let self_ptr = self.ptr as *const CallbackInfo<'a>;
        self_ptr.fmt(f)
    }
}

pub type CallbackReturn = UpdateScreen;
pub type CallbackType = extern "C" fn(CallbackInfoPtr) -> CallbackReturn;

impl<'a> fmt::Debug for CallbackInfo<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CallbackInfo {{
            data: {{ .. }}, \
            current_window_state: {:?}, \
            modifiable_window_state: {:?}, \
            layout_results: {:?}, \
            gl_context: {{ .. }}, \
            resources: {{ .. }}, \
            timers: {{ .. }}, \
            tasks: {{ .. }}, \
            focus_target: {:?}, \
            current_scroll_states: {:?}, \
            nodes_scrolled_in_callback: {:?}, \
            hit_dom_node: {:?}, \
            cursor_relative_to_item: {:?}, \
            cursor_in_viewport: {:?}, \
        }}",
            self.current_window_state,
            self.modifiable_window_state,
            self.layout_results,
            self.focus_target,
            self.current_scroll_states,
            self.nodes_scrolled_in_callback,
            self.hit_dom_node,
            self.cursor_relative_to_item,
            self.cursor_in_viewport,
        )
    }
}

impl<'a> CallbackInfo<'a> {
    /// Sets whether the event should be propagated to the parent hit node or not
    ///
    /// Similar to `e.stopPropagation()` in JavaScript
    pub fn stop_propagation(&mut self) {
        *self.stop_propagation = true;
    }
}

// -- opengl callback

/// Callbacks that returns a rendered OpenGL texture
#[cfg(feature = "opengl")]
#[repr(C)]
pub struct GlCallback { pub cb: GlCallbackType }
#[cfg(feature = "opengl")]
impl_callback!(GlCallback);

#[derive(Debug)]
pub struct GlCallbackInfo<'a> {
    pub state: &'a RefAny,
    #[cfg(feature = "opengl")]
    pub gl_context: &'a GlContextPtr,
    pub resources: &'a AppResources,
    pub bounds: HidpiAdjustedBounds,
}

/// Pointer to rust-allocated `Box<GlCallbackInfo<'a>>` struct
#[repr(C)] pub struct GlCallbackInfoPtr { pub ptr: *mut c_void }

impl Drop for GlCallbackInfoPtr {
    fn drop<'a>(&mut self) {
        let _ = unsafe { Box::<GlCallbackInfo<'a>>::from_raw(self.ptr as *mut GlCallbackInfo<'a>) };
    }
}

impl std::fmt::Debug for GlCallbackInfoPtr {
    fn fmt<'a>(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let self_ptr = self.ptr as *const GlCallbackInfo<'a>;
        self_ptr.fmt(f)
    }
}

#[cfg(feature = "opengl")]
#[repr(C)]
#[derive(Debug)]
pub struct GlCallbackReturn { pub texture: OptionTexture }

#[cfg(feature = "opengl")]
pub type GlCallbackType = extern "C" fn(GlCallbackInfoPtr) -> GlCallbackReturn;

// -- iframe callback

/// Callback that, given a rectangle area on the screen, returns the DOM
/// appropriate for that bounds (useful for infinite lists)
#[repr(C)]
pub struct IFrameCallback { pub cb: IFrameCallbackType }
impl_callback!(IFrameCallback);

#[derive(Debug)]
pub struct IFrameCallbackInfo<'a> {
    pub state: &'a RefAny,
    pub resources: &'a AppResources,
    pub bounds: HidpiAdjustedBounds,
}

/// Pointer to rust-allocated `Box<IFrameCallbackInfo<'a>>` struct
#[repr(C)] pub struct IFrameCallbackInfoPtr { pub ptr: *mut c_void }

impl Drop for IFrameCallbackInfoPtr {
    fn drop<'a>(&mut self) {
        let _ = unsafe { Box::<IFrameCallbackInfo<'a>>::from_raw(self.ptr as *mut IFrameCallbackInfo<'a>) };
    }
}

impl std::fmt::Debug for IFrameCallbackInfoPtr {
    fn fmt<'a>(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let self_ptr = self.ptr as *const IFrameCallbackInfo<'a>;
        self_ptr.fmt(f)
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct IFrameCallbackReturn {
    pub styled_dom: StyledDom
}

pub type IFrameCallbackType = extern "C" fn(IFrameCallbackInfoPtr) -> IFrameCallbackReturn;

// -- thread callback
pub type ThreadCallbackType = extern "C" fn(RefAny) -> RefAny;

// --  task callback
pub type TaskCallbackType = extern "C" fn(ArcMutexRefAnyPtr, DropCheckPtr) -> UpdateScreen;

// -- timer callback

/// Callback that can runs on every frame on the main thread - can modify the app data model
#[repr(C)]
pub struct TimerCallback { pub cb: TimerCallbackType }
impl_callback!(TimerCallback);

#[derive(Debug)]
pub struct TimerCallbackInfo<'a> {
    pub state: &'a mut RefAny,
    pub app_resources: &'a mut AppResources,
}

/// Pointer to rust-allocated `Box<TimerCallbackInfo<'a>>` struct
#[repr(C)] pub struct TimerCallbackInfoPtr { pub ptr: *const c_void }

impl Drop for TimerCallbackInfoPtr {
    fn drop<'a>(&mut self) {
        let _ = unsafe { Box::<TimerCallbackInfo<'a>>::from_raw(self.ptr as *mut TimerCallbackInfo<'a>) };
    }
}

impl std::fmt::Debug for TimerCallbackInfoPtr {
    fn fmt<'a>(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let self_ptr = self.ptr as *const TimerCallbackInfo<'a>;
        self_ptr.fmt(f)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TimerCallbackReturn {
    pub should_update: UpdateScreen,
    pub should_terminate: TerminateTimer,
}

pub type TimerCallbackType = extern "C" fn(TimerCallbackInfoPtr) -> TimerCallbackReturn;

/// Pointer to rust-allocated `Box<LayoutInfo<'a>>` struct
#[repr(C)] pub struct LayoutInfoPtr { pub ptr: *mut c_void }

impl Drop for LayoutInfoPtr {
    fn drop<'a>(&mut self) {
        let _ = unsafe { Box::<LayoutInfo<'a>>::from_raw(self.ptr as *mut LayoutInfo<'a>) };
    }
}

impl std::fmt::Debug for LayoutInfoPtr {
    fn fmt<'a>(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let self_ptr = self.ptr as *const LayoutInfo<'a>;
        self_ptr.fmt(f)
    }
}

/// Gives the `layout()` function access to the `AppResources` and the `Window`
/// (for querying images and fonts, as well as width / height)
#[derive(Debug)]
pub struct LayoutInfo<'a> {
    /// Window size (so that apps can return a different UI depending on
    /// the window size - mobile / desktop view). Should be later removed
    /// in favor of "resize" handlers and @media queries.
    pub window_size: &'a WindowSize,
    /// Optimization for resizing: If a DOM has no Iframes and the window size
    /// does not change the state of the UI, then resizing the window will not
    /// result in calls to the .layout() function (since the resulting UI would
    /// stay the same).
    ///
    /// Stores "stops" in logical pixels where the UI needs to be re-generated
    /// should the width of the window change.
    pub window_size_width_stops: &'a mut Vec<f32>,
    /// Same as `window_size_width_stops` but for the height of the window.
    pub window_size_height_stops: &'a mut Vec<f32>,
    /// Allows the layout() function to reference app resources such as FontIDs or ImageIDs
    pub resources: &'a AppResources,
}

impl<'a> LayoutInfo<'a> {

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
        self.window_size_width_stops.push(width);
        self.window_size.get_logical_size().width > width
    }

    pub fn window_width_smaller_than(&mut self, width: f32) -> bool {
        self.window_size_width_stops.push(width);
        self.window_size.get_logical_size().width < width
    }

    pub fn window_height_larger_than(&mut self, height: f32) -> bool {
        self.window_size_height_stops.push(height);
        self.window_size.get_logical_size().height > height
    }

    pub fn window_height_smaller_than(&mut self, height: f32) -> bool {
        self.window_size_height_stops.push(height);
        self.window_size.get_logical_size().height < height
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
        // NOTE: hidpi factor, not winit_hidpi_factor!
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
pub enum FocusTarget {
    Id(DomNodeId),
    Path((DomId, CssPath)),
    NoFocus,
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
            Path((dom_id, css_path)) => {
                let layout_result = layout_results.get(dom_id.inner).ok_or(UpdateFocusWarning::FocusInvalidDomId(dom_id.clone()))?;
                let html_node_tree = &layout_result.styled_dom.cascade_info;
                let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
                let node_data = &layout_result.styled_dom.node_data;
                let resolved_node_id = html_node_tree
                    .as_container()
                    .linear_iter()
                    .find(|node_id| matches_html_element(css_path, *node_id, &node_hierarchy.as_container(), &node_data.as_container(), &html_node_tree.as_container()))
                    .ok_or(UpdateFocusWarning::CouldNotFindFocusNode(css_path.clone()))?;
                Ok(Some(DomNodeId { dom: dom_id.clone(), node: AzNodeId::from_crate_internal(Some(resolved_node_id)) }))
            },
        }
    }
}
