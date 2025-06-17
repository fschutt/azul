#![allow(dead_code)]

#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::{alloc::Layout, boxed::Box, collections::BTreeMap, vec::Vec};
use core::{
    ffi::c_void,
    fmt,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};
#[cfg(feature = "std")]
use std::hash::Hash;

use azul_css::{
    AnimationInterpolationFunction, AzString, CssPath, CssProperty, CssPropertyType, FontRef,
    InterpolateResolver, LayoutRect, LayoutSize,
};
use rust_fontconfig::{FcFontCache, FontSource};

use crate::{
    app_resources::{
        FontInstanceKey, IdNamespace, ImageCache, ImageMask, ImageRef, LayoutedGlyphs,
        RendererResources, ShapedWords, WordPositions, Words,
    },
    gl::OptionGlContextPtr,
    id_tree::{NodeDataContainer, NodeId},
    styled_dom::{
        CssPropertyCache, DomId, NodeHierarchyItemId, NodeHierarchyItemVec, StyledDom, StyledNode,
        StyledNodeVec,
    },
    task::{
        CreateThreadCallback, Duration as AzDuration, ExternalSystemCallbacks,
        GetSystemTimeCallback, Instant as AzInstant, Instant, TerminateTimer, Thread, ThreadId,
        ThreadReceiver, ThreadSendMsg, ThreadSender, Timer, TimerId,
    },
    ui_solver::{
        LayoutResult, OverflowingScrollNode, PositionInfo, PositionedRectangle,
        ResolvedTextLayoutOptions, TextLayoutOptions,
    },
    window::{
        AzStringPair, FullWindowState, KeyboardState, LogicalPosition, LogicalRect, LogicalSize,
        MouseState, OptionChar, OptionLogicalPosition, PhysicalSize, RawWindowHandle,
        UpdateFocusWarning, WindowCreateOptions, WindowFlags, WindowSize, WindowState, WindowTheme,
    },
    FastBTreeSet, FastHashMap,
};

// NOTE: must be repr(C), otherwise UB
// due to zero-sized allocation in RefAny::new_c
// TODO: fix later!
#[repr(C)]
pub struct Dummy {
    pub _dummy: u8,
}

/// Specifies if the screen should be updated after the callback function has returned
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Update {
    /// The screen does not need to redraw after the callback has been called
    DoNothing,
    /// After the callback is called, the screen needs to redraw (layout() function being called
    /// again)
    RefreshDom,
    /// The layout has to be re-calculated for all windows
    RefreshDomAllWindows,
}

impl Update {
    pub fn max_self(&mut self, other: Self) {
        if *self == Update::DoNothing && other != Update::DoNothing {
            *self = other;
        } else if *self == Update::RefreshDom && other == Update::RefreshDomAllWindows {
            *self = other;
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct RefCountInner {
    pub num_copies: AtomicUsize,
    pub num_refs: AtomicUsize,
    pub num_mutable_refs: AtomicUsize,
    pub _internal_len: usize,
    pub _internal_layout_size: usize,
    pub _internal_layout_align: usize,
    pub type_id: u64,
    pub type_name: AzString,
    pub custom_destructor: extern "C" fn(*mut c_void),
}

#[derive(Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(C)]
pub struct RefCount {
    pub ptr: *const RefCountInner,
    pub run_destructor: bool,
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
            run_destructor: true,
        }
    }
}

impl Drop for RefCount {
    fn drop(&mut self) {
        self.run_destructor = false;
        // note: the owning struct of the RefCount has to do the dropping!
    }
}

#[derive(Debug, Clone)]
pub struct RefCountInnerDebug {
    pub num_copies: usize,
    pub num_refs: usize,
    pub num_mutable_refs: usize,
    pub _internal_len: usize,
    pub _internal_layout_size: usize,
    pub _internal_layout_align: usize,
    pub type_id: u64,
    pub type_name: AzString,
    pub custom_destructor: usize,
}

impl RefCount {
    fn new(ref_count: RefCountInner) -> Self {
        RefCount {
            ptr: Box::into_raw(Box::new(ref_count)),
            run_destructor: true,
        }
    }
    fn downcast(&self) -> &RefCountInner {
        unsafe { &*self.ptr }
    }

    pub fn debug_get_refcount_copied(&self) -> RefCountInnerDebug {
        let dc = self.downcast();
        RefCountInnerDebug {
            num_copies: dc.num_copies.load(AtomicOrdering::SeqCst),
            num_refs: dc.num_refs.load(AtomicOrdering::SeqCst),
            num_mutable_refs: dc.num_mutable_refs.load(AtomicOrdering::SeqCst),
            _internal_len: dc._internal_len,
            _internal_layout_size: dc._internal_layout_size,
            _internal_layout_align: dc._internal_layout_align,
            type_id: dc.type_id,
            type_name: dc.type_name.clone(),
            custom_destructor: dc.custom_destructor as usize,
        }
    }

    /// Runtime check to check whether this `RefAny` can be borrowed
    pub fn can_be_shared(&self) -> bool {
        self.downcast()
            .num_mutable_refs
            .load(AtomicOrdering::SeqCst)
            == 0
    }

    /// Runtime check to check whether this `RefAny` can be borrowed mutably
    pub fn can_be_shared_mut(&self) -> bool {
        let info = self.downcast();
        info.num_mutable_refs.load(AtomicOrdering::SeqCst) == 0
            && info.num_refs.load(AtomicOrdering::SeqCst) == 0
    }

    pub fn increase_ref(&self) {
        self.downcast()
            .num_refs
            .fetch_add(1, AtomicOrdering::SeqCst);
    }

    pub fn decrease_ref(&self) {
        self.downcast()
            .num_refs
            .fetch_sub(1, AtomicOrdering::SeqCst);
    }

    pub fn increase_refmut(&self) {
        self.downcast()
            .num_mutable_refs
            .fetch_add(1, AtomicOrdering::SeqCst);
    }

    pub fn decrease_refmut(&self) {
        self.downcast()
            .num_mutable_refs
            .fetch_sub(1, AtomicOrdering::SeqCst);
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct Ref<'a, T> {
    ptr: &'a T,
    sharing_info: RefCount,
}

impl<'a, T> Drop for Ref<'a, T> {
    fn drop(&mut self) {
        self.sharing_info.decrease_ref();
    }
}

impl<'a, T> core::ops::Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.ptr
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct RefMut<'a, T> {
    ptr: &'a mut T,
    sharing_info: RefCount,
}

impl<'a, T> Drop for RefMut<'a, T> {
    fn drop(&mut self) {
        self.sharing_info.decrease_refmut();
    }
}

impl<'a, T> core::ops::Deref for RefMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.ptr
    }
}

impl<'a, T> core::ops::DerefMut for RefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ptr
    }
}

#[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(C)]
pub struct RefAny {
    /// void* to a boxed struct or enum of type "T". RefCount stores the RTTI
    /// for this opaque type (can be downcasted by the user)
    pub _internal_ptr: *const c_void,
    /// All the metadata information is set on the refcount, so that the metadata
    /// has to only be created once per object, not once per copy
    pub sharing_info: RefCount,
    /// Instance of this copy (root = 0th copy).
    ///
    /// Necessary to distinguish between the original copy and all other clones
    pub instance_id: u64,
    pub run_destructor: bool,
}

impl_option!(
    RefAny,
    OptionRefAny,
    copy = false,
    [Debug, Hash, Clone, PartialEq, PartialOrd, Ord, Eq]
);

// the refcount of RefAny is atomic, therefore `RefAny` is not `Sync`, but it is `Send`
unsafe impl Send for RefAny {}
// library-internal only - RefAny is not Sync outside of this library!
unsafe impl Sync for RefAny {} // necessary for rayon to work

impl RefAny {
    /// Creates a new, type-erased pointer by casting the `T` value into a
    /// `Vec<u8>` and saving the length + type ID
    pub fn new<T: 'static>(value: T) -> Self {
        extern "C" fn default_custom_destructor<U: 'static>(ptr: &mut c_void) {
            use core::{mem, ptr};

            // note: in the default constructor, we do not need to check whether U == T

            unsafe {
                // copy the struct from the heap to the stack and
                // call mem::drop on U to run the destructor
                let mut stack_mem = mem::MaybeUninit::<U>::uninit();
                ptr::copy_nonoverlapping(
                    (ptr as *mut c_void) as *const U,
                    stack_mem.as_mut_ptr(),
                    mem::size_of::<U>(),
                );
                let stack_mem = stack_mem.assume_init();
                mem::drop(stack_mem);
            }
        }

        let type_name = ::core::any::type_name::<T>();
        let st = AzString::from_const_str(type_name);
        let s = Self::new_c(
            (&value as *const T) as *const c_void,
            ::core::mem::size_of::<T>(),
            Self::get_type_id_static::<T>(),
            st,
            default_custom_destructor::<T>,
        );
        ::core::mem::forget(value); // do not run the destructor of T here!
        s
    }

    /// C-ABI compatible function to create a `RefAny` across the C boundary
    pub fn new_c(
        // *const T
        ptr: *const c_void,
        // sizeof(T)
        len: usize,
        // unique ID of the type (used for type comparison when downcasting)
        type_id: u64,
        // name of the class such as "app::MyData", usually compiler- or macro-generated
        type_name: AzString,
        custom_destructor: extern "C" fn(&mut c_void),
    ) -> Self {
        use core::ptr;

        // special case: calling alloc() with 0 bytes would be undefined behaviour
        //
        // In order to invoke the destructor correctly, we need a 0-sized allocation
        // on the heap (NOT nullptr, as this would lead to UB when calling the destructor)
        let (_internal_ptr, layout) = if len == 0 {
            let _dummy: [u8; 0] = [];
            (ptr::null_mut(), Layout::for_value(&_dummy))
        } else {
            // cast the struct as bytes
            let struct_as_bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
            let layout = Layout::for_value(&*struct_as_bytes);

            // allocate + copy the struct to the heap
            let heap_struct_as_bytes = unsafe { alloc::alloc::alloc(layout) };
            unsafe {
                ptr::copy_nonoverlapping(
                    struct_as_bytes.as_ptr(),
                    heap_struct_as_bytes,
                    struct_as_bytes.len(),
                )
            };
            (heap_struct_as_bytes, layout)
        };

        let ref_count_inner = RefCountInner {
            num_copies: AtomicUsize::new(1),
            num_refs: AtomicUsize::new(0),
            num_mutable_refs: AtomicUsize::new(0),
            _internal_len: len,
            _internal_layout_size: layout.size(),
            _internal_layout_align: layout.align(),
            type_id,
            type_name,
            // fn(&mut c_void) and fn(*mut c_void) are the same, so transmute is safe
            custom_destructor: unsafe { core::mem::transmute(custom_destructor) },
        };

        Self {
            _internal_ptr: _internal_ptr as *const c_void,
            sharing_info: RefCount::new(ref_count_inner),
            instance_id: 0,
            run_destructor: true,
        }
    }

    /// Returns whether this RefAny is the only instance
    pub fn has_no_copies(&self) -> bool {
        self.sharing_info
            .downcast()
            .num_copies
            .load(AtomicOrdering::SeqCst)
            == 1
            && self
                .sharing_info
                .downcast()
                .num_refs
                .load(AtomicOrdering::SeqCst)
                == 0
            && self
                .sharing_info
                .downcast()
                .num_mutable_refs
                .load(AtomicOrdering::SeqCst)
                == 0
    }

    /// Downcasts the type-erased pointer to a type `&U`, returns `None` if the types don't match
    #[inline]
    pub fn downcast_ref<'a, U: 'static>(&'a mut self) -> Option<Ref<'a, U>> {
        let is_same_type = self.get_type_id() == Self::get_type_id_static::<U>();
        if !is_same_type {
            return None;
        }

        let can_be_shared = self.sharing_info.can_be_shared();
        if !can_be_shared {
            return None;
        }

        if self._internal_ptr.is_null() {
            return None;
        }
        self.sharing_info.increase_ref();
        Some(Ref {
            ptr: unsafe { &*(self._internal_ptr as *const U) },
            sharing_info: self.sharing_info.clone(),
        })
    }

    /// Downcasts the type-erased pointer to a type `&mut U`, returns `None` if the types don't
    /// match
    #[inline]
    pub fn downcast_mut<'a, U: 'static>(&'a mut self) -> Option<RefMut<'a, U>> {
        let is_same_type = self.get_type_id() == Self::get_type_id_static::<U>();
        if !is_same_type {
            return None;
        }

        let can_be_shared_mut = self.sharing_info.can_be_shared_mut();
        if !can_be_shared_mut {
            return None;
        }

        if self._internal_ptr.is_null() {
            return None;
        }
        self.sharing_info.increase_refmut();

        Some(RefMut {
            ptr: unsafe { &mut *(self._internal_ptr as *mut U) },
            sharing_info: self.sharing_info.clone(),
        })
    }

    // Returns the typeid of `T` as a u64 (necessary because
    // `core::any::TypeId` is not C-ABI compatible)
    #[inline]
    fn get_type_id_static<T: 'static>() -> u64 {
        use core::{any::TypeId, mem};

        // fast method to serialize the type id into a u64
        let t_id = TypeId::of::<T>();
        let struct_as_bytes = unsafe {
            core::slice::from_raw_parts(
                (&t_id as *const TypeId) as *const u8,
                mem::size_of::<TypeId>(),
            )
        };

        struct_as_bytes
            .into_iter()
            .enumerate()
            .map(|(s_pos, s)| ((*s as u64) << s_pos))
            .sum()
    }

    /// Checks whether the typeids match
    pub fn is_type(&self, type_id: u64) -> bool {
        self.sharing_info.downcast().type_id == type_id
    }

    // Returns the internal type ID
    pub fn get_type_id(&self) -> u64 {
        self.sharing_info.downcast().type_id
    }

    // Returns the type name
    pub fn get_type_name(&self) -> AzString {
        self.sharing_info.downcast().type_name.clone()
    }
}

impl Clone for RefAny {
    fn clone(&self) -> Self {
        self.sharing_info
            .downcast()
            .num_copies
            .fetch_add(1, AtomicOrdering::SeqCst);
        Self {
            _internal_ptr: self._internal_ptr,
            sharing_info: RefCount {
                ptr: self.sharing_info.ptr,
                run_destructor: true,
            },
            instance_id: self
                .sharing_info
                .downcast()
                .num_copies
                .load(AtomicOrdering::SeqCst) as u64,
            run_destructor: true,
        }
    }
}

impl Drop for RefAny {
    fn drop(&mut self) {
        use core::ptr;

        self.run_destructor = false;

        let current_copies = self
            .sharing_info
            .downcast()
            .num_copies
            .fetch_sub(1, AtomicOrdering::SeqCst);

        if current_copies != 1 {
            return;
        }

        let sharing_info = unsafe { Box::from_raw(self.sharing_info.ptr as *mut RefCountInner) };
        let sharing_info = *sharing_info; // sharing_info itself deallocates here

        if sharing_info._internal_len == 0
            || sharing_info._internal_layout_size == 0
            || self._internal_ptr.is_null()
        {
            let mut _dummy: [u8; 0] = [];
            (sharing_info.custom_destructor)(_dummy.as_ptr() as *mut c_void);
        } else {
            let layout = unsafe {
                Layout::from_size_align_unchecked(
                    sharing_info._internal_layout_size,
                    sharing_info._internal_layout_align,
                )
            };

            (sharing_info.custom_destructor)(self._internal_ptr as *mut c_void);
            unsafe {
                alloc::alloc::dealloc(self._internal_ptr as *mut u8, layout);
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
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct ScrollPosition {
    /// How big is the parent container
    /// (so that things like "scroll to left edge" can be implemented)?
    pub parent_rect: LogicalRect,
    /// How big is the scroll rect (i.e. the union of all children)?
    pub children_rect: LogicalRect,
}

#[derive(Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct DocumentId {
    pub namespace_id: IdNamespace,
    pub id: u32,
}

impl ::core::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "DocumentId {{ ns: {}, id: {} }}",
            self.namespace_id, self.id
        )
    }
}

impl ::core::fmt::Debug for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct PipelineId(pub PipelineSourceId, pub u32);

impl ::core::fmt::Display for PipelineId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PipelineId({}, {})", self.0, self.1)
    }
}

impl ::core::fmt::Debug for PipelineId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

static LAST_PIPELINE_ID: AtomicUsize = AtomicUsize::new(0);

impl PipelineId {
    pub const DUMMY: PipelineId = PipelineId(0, 0);

    pub fn new() -> Self {
        PipelineId(
            LAST_PIPELINE_ID.fetch_add(1, AtomicOrdering::SeqCst) as u32,
            0,
        )
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct HitTestItem {
    /// The hit point in the coordinate space of the "viewport" of the display item.
    /// The viewport is the scroll node formed by the root reference frame of the display item's
    /// pipeline.
    pub point_in_viewport: LogicalPosition,
    /// The coordinates of the original hit test point relative to the origin of this item.
    /// This is useful for calculating things like text offsets in the client.
    pub point_relative_to_item: LogicalPosition,
    /// Necessary to easily get the nearest IFrame node
    pub is_focusable: bool,
    /// If this hit is an IFrame node, stores the IFrames DomId + the origin of the IFrame
    pub is_iframe_hit: Option<(DomId, LogicalPosition)>,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ScrollHitTestItem {
    /// The hit point in the coordinate space of the "viewport" of the display item.
    /// The viewport is the scroll node formed by the root reference frame of the display item's
    /// pipeline.
    pub point_in_viewport: LogicalPosition,
    /// The coordinates of the original hit test point relative to the origin of this item.
    /// This is useful for calculating things like text offsets in the client.
    pub point_relative_to_item: LogicalPosition,
    /// If this hit is an IFrame node, stores the IFrames DomId + the origin of the IFrame
    pub scroll_node: OverflowingScrollNode,
}

/// Implements `Display, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Hash`
/// for a Callback with a `.0` field:
///
/// ```
/// # use azul_core::impl_callback;
/// type T = String;
/// struct MyCallback {
///     cb: fn(&T),
/// };
///
/// // impl Display, Debug, etc. for MyCallback
/// impl_callback!(MyCallback);
/// ```
///
/// This is necessary to work around for https://github.com/rust-lang/rust/issues/54508
#[macro_export]
macro_rules! impl_callback {
    ($callback_value:ident) => {
        impl ::core::fmt::Display for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{:?}", self)
            }
        }

        impl ::core::fmt::Debug for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                let callback = stringify!($callback_value);
                write!(f, "{} @ 0x{:x}", callback, self.cb as usize)
            }
        }

        impl Clone for $callback_value {
            fn clone(&self) -> Self {
                $callback_value {
                    cb: self.cb.clone(),
                }
            }
        }

        impl ::core::hash::Hash for $callback_value {
            fn hash<H>(&self, state: &mut H)
            where
                H: ::core::hash::Hasher,
            {
                state.write_usize(self.cb as usize);
            }
        }

        impl PartialEq for $callback_value {
            fn eq(&self, rhs: &Self) -> bool {
                self.cb as usize == rhs.cb as usize
            }
        }

        impl PartialOrd for $callback_value {
            fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
                Some((self.cb as usize).cmp(&(other.cb as usize)))
            }
        }

        impl Ord for $callback_value {
            fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
                (self.cb as usize).cmp(&(other.cb as usize))
            }
        }

        impl Eq for $callback_value {}

        impl Copy for $callback_value {}
    };
}

#[allow(unused_macros)]
macro_rules! impl_get_gl_context {
    () => {
        /// Returns a reference-counted pointer to the OpenGL context
        pub fn get_gl_context(&self) -> OptionGlContextPtr {
            Some(self.gl_context.clone())
        }
    };
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct DomNodeId {
    pub dom: DomId,
    pub node: NodeHierarchyItemId,
}

impl_option!(
    DomNodeId,
    OptionDomNodeId,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl DomNodeId {
    pub const ROOT: DomNodeId = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::NONE,
    };
}
// -- layout callback

/// Callback function pointer (has to be a function pointer in
/// order to be compatible with C APIs later on).
///
/// IMPORTANT: The callback needs to deallocate the `RefAnyPtr` and `LayoutCallbackInfoPtr`,
/// otherwise that memory is leaked. If you use the official auto-generated
/// bindings, this is already done for you.
///
/// NOTE: The original callback was `fn(&self, LayoutCallbackInfo) -> Dom`
/// which then evolved to `fn(&RefAny, LayoutCallbackInfo) -> Dom`.
/// The indirection is necessary because of the memory management
/// around the C API
///
/// See azul-core/ui_state.rs:298 for how the memory is managed
/// across the callback boundary.
pub type LayoutCallbackType = extern "C" fn(&mut RefAny, &mut LayoutCallbackInfo) -> StyledDom;

#[repr(C)]
pub struct LayoutCallbackInner {
    pub cb: LayoutCallbackType,
}
impl_callback!(LayoutCallbackInner);

extern "C" fn default_layout_callback(_: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {
    StyledDom::default()
}

/// In order to interact with external VMs (Java, Python, etc.)
/// the callback is often stored as a "function object"
///
/// In order to callback into external languages, the layout
/// callback has to be able to carry some extra data
/// (the first argument), which usually contains the function object
/// i.e. in the Python VM a PyCallable / PyAny
pub type MarshaledLayoutCallbackType = extern "C" fn(
    /* marshal_data */ &mut RefAny,
    /* app_data */ &mut RefAny,
    &mut LayoutCallbackInfo,
) -> StyledDom;

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum LayoutCallback {
    Raw(LayoutCallbackInner),
    Marshaled(MarshaledLayoutCallback),
}

impl Default for LayoutCallback {
    fn default() -> Self {
        Self::Raw(LayoutCallbackInner {
            cb: default_layout_callback,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct MarshaledLayoutCallback {
    pub marshal_data: RefAny,
    pub cb: MarshaledLayoutCallbackInner,
}

#[repr(C)]
pub struct MarshaledLayoutCallbackInner {
    pub cb: MarshaledLayoutCallbackType,
}

impl_callback!(MarshaledLayoutCallbackInner);

// -- normal callback

/// Stores a function pointer that is executed when the given UI element is hit
///
/// Must return an `Update` that denotes if the screen should be redrawn.
/// The style is not affected by this, so if you make changes to the window's style
/// inside the function, the screen will not be automatically redrawn, unless you return
/// an `Update::Redraw` from the function
#[repr(C)]
pub struct Callback {
    pub cb: CallbackType,
}
impl_callback!(Callback);

impl_option!(
    Callback,
    OptionCallback,
    [Debug, Eq, Copy, Clone, PartialEq, PartialOrd, Ord, Hash]
);

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct InlineTextHit {
    // if the unicode_codepoint is None, it's usually a mark glyph that was hit
    pub unicode_codepoint: OptionChar, // Option<char>

    // position of the cursor relative to X
    pub hit_relative_to_inline_text: LogicalPosition,
    pub hit_relative_to_line: LogicalPosition,
    pub hit_relative_to_text_content: LogicalPosition,
    pub hit_relative_to_glyph: LogicalPosition,

    // relative to text
    pub line_index_relative_to_text: usize,
    pub word_index_relative_to_text: usize,
    pub text_content_index_relative_to_text: usize,
    pub glyph_index_relative_to_text: usize,
    pub char_index_relative_to_text: usize,

    // relative to line
    pub word_index_relative_to_line: usize,
    pub text_content_index_relative_to_line: usize,
    pub glyph_index_relative_to_line: usize,
    pub char_index_relative_to_line: usize,

    // relative to text content (word)
    pub glyph_index_relative_to_word: usize,
    pub char_index_relative_to_word: usize,
}

impl_vec!(InlineTextHit, InlineTextHitVec, InlineTextHitVecDestructor);
impl_vec_clone!(InlineTextHit, InlineTextHitVec, InlineTextHitVecDestructor);
impl_vec_debug!(InlineTextHit, InlineTextHitVec);
impl_vec_partialeq!(InlineTextHit, InlineTextHitVec);
impl_vec_partialord!(InlineTextHit, InlineTextHitVec);

/// inline text so that hit-testing is easier
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct InlineText {
    /// List of lines, relative to (0.0, 0.0) representing the top left corner of the line
    pub lines: InlineLineVec,
    /// Size of the text content, may be larger than the
    /// position of lines due to descending glyphs
    pub content_size: LogicalSize,
    /// Size of the font used to layout this line
    pub font_size_px: f32,
    /// Index of the last word
    pub last_word_index: usize,
    /// NOTE: descender is NEGATIVE (pixels from baseline to font size)
    pub baseline_descender_px: f32,
}

impl_option!(
    InlineText,
    OptionInlineText,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl InlineText {
    /// Returns the final, positioned glyphs from an inline text
    ///
    /// NOTE: It seems that at least in webrender, the glyphs have to be
    /// positioned in relation to the screen (instead of relative to the parent container)
    ///
    /// The text_origin gets added to each glyph
    ///
    /// NOTE: The lines in the text are relative to the TOP left corner (of the text, i.e.
    /// relative to the text_origin), but the word position is relative to the BOTTOM left
    /// corner (of the line bounds)
    pub fn get_layouted_glyphs(&self) -> LayoutedGlyphs {
        use crate::display_list::GlyphInstance;

        let default: InlineGlyphVec = Vec::new().into();
        let default_ref = &default;

        // descender_px is NEGATIVE
        let baseline_descender_px = LogicalPosition::new(0.0, self.baseline_descender_px);

        LayoutedGlyphs {
            glyphs: self
                .lines
                .iter()
                .flat_map(move |line| {
                    // bottom left corner of line rect
                    let line_origin = line.bounds.origin;

                    line.words.iter().flat_map(move |word| {
                        let (glyphs, mut word_origin) = match word {
                            InlineWord::Tab | InlineWord::Return | InlineWord::Space => {
                                (default_ref, LogicalPosition::zero())
                            }
                            InlineWord::Word(text_contents) => {
                                (&text_contents.glyphs, text_contents.bounds.origin)
                            }
                        };

                        word_origin.y = 0.0;

                        glyphs.iter().map(move |glyph| GlyphInstance {
                            index: glyph.glyph_index,
                            point: {
                                line_origin
                                    + baseline_descender_px
                                    + word_origin
                                    + glyph.bounds.origin
                            },
                            size: glyph.bounds.size,
                        })
                    })
                })
                .collect::<Vec<GlyphInstance>>(),
        }
    }

    /// Hit tests all glyphs, returns the hit glyphs - note that the result may
    /// be empty (no glyphs hit), or it may contain more than one result
    /// (overlapping glyphs - more than one glyph hit)
    ///
    /// Usually the result will contain a single `InlineTextHit`
    pub fn hit_test(&self, position: LogicalPosition) -> Vec<InlineTextHit> {
        let bounds = LogicalRect::new(LogicalPosition::zero(), self.content_size);

        let hit_relative_to_inline_text = match bounds.hit_test(&position) {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut global_char_hit = 0;
        let mut global_word_hit = 0;
        let mut global_glyph_hit = 0;
        let mut global_text_content_hit = 0;

        // NOTE: this function cannot exit early, since it has to
        // iterate through all lines

        let font_size_px = self.font_size_px;
        let descender_px = self.baseline_descender_px;

        self.lines
        .iter() // TODO: par_iter
        .enumerate()
        .flat_map(|(line_index, line)| {

            let char_at_line_start = global_char_hit;
            let word_at_line_start = global_word_hit;
            let glyph_at_line_start = global_glyph_hit;
            let text_content_at_line_start = global_text_content_hit;

            let mut line_bounds = line.bounds.clone();
            line_bounds.origin.y -= line.bounds.size.height;

            line_bounds.hit_test(&hit_relative_to_inline_text)
            .map(|mut hit_relative_to_line| {

                line.words
                .iter() // TODO: par_iter
                .flat_map(|word| {

                    let char_at_text_content_start = global_char_hit;
                    let glyph_at_text_content_start = global_glyph_hit;

                    let word_result = word
                    .get_text_content()
                    .and_then(|text_content| {

                        let mut text_content_bounds = text_content.bounds.clone();
                        text_content_bounds.origin.y = 0.0;

                        text_content_bounds
                        .hit_test(&hit_relative_to_line)
                        .map(|mut hit_relative_to_text_content| {

                            text_content.glyphs
                            .iter() // TODO: par_iter
                            .flat_map(|glyph| {

                                let mut glyph_bounds = glyph.bounds;
                                glyph_bounds.origin.y = text_content.bounds.size.height + descender_px - glyph.bounds.size.height;

                                let result = glyph_bounds
                                .hit_test(&hit_relative_to_text_content)
                                .map(|hit_relative_to_glyph| {
                                    InlineTextHit {
                                        unicode_codepoint: glyph.unicode_codepoint,

                                        hit_relative_to_inline_text,
                                        hit_relative_to_line,
                                        hit_relative_to_text_content,
                                        hit_relative_to_glyph,

                                        line_index_relative_to_text: line_index,
                                        word_index_relative_to_text: global_word_hit,
                                        text_content_index_relative_to_text: global_text_content_hit,
                                        glyph_index_relative_to_text: global_glyph_hit,
                                        char_index_relative_to_text: global_char_hit,

                                        word_index_relative_to_line: global_word_hit - word_at_line_start,
                                        text_content_index_relative_to_line: global_text_content_hit - text_content_at_line_start,
                                        glyph_index_relative_to_line: global_glyph_hit - glyph_at_line_start,
                                        char_index_relative_to_line: global_char_hit - char_at_line_start,

                                        glyph_index_relative_to_word: global_glyph_hit - glyph_at_text_content_start,
                                        char_index_relative_to_word: global_char_hit - char_at_text_content_start,
                                    }
                                });

                                if glyph.has_codepoint() {
                                    global_char_hit += 1;
                                }

                                global_glyph_hit += 1;

                                result
                            })
                            .collect::<Vec<_>>()
                        })
                    }).unwrap_or_default();

                    if word.has_text_content() {
                        global_text_content_hit += 1;
                    }

                    global_word_hit += 1;

                    word_result.into_iter()
                })
                .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .into_iter()

        })
        .collect::<Vec<_>>()
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct InlineLine {
    pub words: InlineWordVec,
    pub bounds: LogicalRect,
}

impl_vec!(InlineLine, InlineLineVec, InlineLineVecDestructor);
impl_vec_clone!(InlineLine, InlineLineVec, InlineLineVecDestructor);
impl_vec_debug!(InlineLine, InlineLineVec);
impl_vec_partialeq!(InlineLine, InlineLineVec);
impl_vec_partialord!(InlineLine, InlineLineVec);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum InlineWord {
    Tab,
    Return,
    Space,
    Word(InlineTextContents),
}

impl InlineWord {
    pub fn has_text_content(&self) -> bool {
        self.get_text_content().is_some()
    }
    pub fn get_text_content(&self) -> Option<&InlineTextContents> {
        match self {
            InlineWord::Tab | InlineWord::Return | InlineWord::Space => None,
            InlineWord::Word(tc) => Some(tc),
        }
    }
}

impl_vec!(InlineWord, InlineWordVec, InlineWordVecDestructor);
impl_vec_clone!(InlineWord, InlineWordVec, InlineWordVecDestructor);
impl_vec_debug!(InlineWord, InlineWordVec);
impl_vec_partialeq!(InlineWord, InlineWordVec);
impl_vec_partialord!(InlineWord, InlineWordVec);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct InlineTextContents {
    pub glyphs: InlineGlyphVec,
    pub bounds: LogicalRect,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct InlineGlyph {
    pub bounds: LogicalRect,
    pub unicode_codepoint: OptionChar,
    pub glyph_index: u32,
}

impl InlineGlyph {
    pub fn has_codepoint(&self) -> bool {
        self.unicode_codepoint.is_some()
    }
}

impl_vec!(InlineGlyph, InlineGlyphVec, InlineGlyphVecDestructor);
impl_vec_clone!(InlineGlyph, InlineGlyphVec, InlineGlyphVecDestructor);
impl_vec_debug!(InlineGlyph, InlineGlyphVec);
impl_vec_partialeq!(InlineGlyph, InlineGlyphVec);
impl_vec_partialord!(InlineGlyph, InlineGlyphVec);

/// Information about the callback that is passed to the callback whenever a callback is invoked
#[derive(Debug)]
#[repr(C)]
pub struct CallbackInfo {
    /// Pointer to the layout results vector start
    layout_results: *const LayoutResult,
    /// Number of layout results
    layout_results_count: usize,
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
    /// Extension for future ABI stability (referenced data)
    _abi_ref: *const c_void,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut c_void,
}

impl CallbackInfo {
    // this function is necessary to get rid of the lifetimes and to make CallbackInfo C-compatible
    //
    // since the call_callbacks() function is the only function
    #[inline]
    pub fn new<'a, 'b>(
        layout_results: &'a [LayoutResult],
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
            layout_results: layout_results.as_ptr(),
            layout_results_count: layout_results.len(),
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
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    fn internal_get_layout_results<'a>(&'a self) -> &'a [LayoutResult] {
        unsafe { core::slice::from_raw_parts(self.layout_results, self.layout_results_count) }
    }
    fn internal_get_renderer_resources<'a>(&'a self) -> &'a RendererResources {
        unsafe { &*self.renderer_resources }
    }
    fn internal_get_previous_window_state<'a>(&'a self) -> &'a Option<FullWindowState> {
        unsafe { &*self.previous_window_state }
    }
    fn internal_get_current_window_state<'a>(&'a self) -> &'a FullWindowState {
        unsafe { &*self.current_window_state }
    }
    fn internal_get_modifiable_window_state<'a>(&'a mut self) -> &'a mut WindowState {
        unsafe { &mut *self.modifiable_window_state }
    }
    fn internal_get_gl_context<'a>(&'a self) -> &'a OptionGlContextPtr {
        unsafe { &*self.gl_context }
    }
    fn internal_get_image_cache<'a>(&'a mut self) -> &'a mut ImageCache {
        unsafe { &mut *self.image_cache }
    }
    fn internal_get_image_cache_ref<'a>(&'a self) -> &'a ImageCache {
        unsafe { &*self.image_cache }
    }
    fn internal_get_system_fonts<'a>(&'a mut self) -> &'a mut FcFontCache {
        unsafe { &mut *self.system_fonts }
    }
    fn internal_get_timers<'a>(&'a mut self) -> &'a mut FastHashMap<TimerId, Timer> {
        unsafe { &mut *self.timers }
    }
    fn internal_get_threads<'a>(&'a mut self) -> &'a mut FastHashMap<ThreadId, Thread> {
        unsafe { &mut *self.threads }
    }
    fn internal_get_timers_removed<'a>(&'a mut self) -> &'a mut FastBTreeSet<TimerId> {
        unsafe { &mut *self.timers_removed }
    }
    fn internal_get_threads_removed<'a>(&'a mut self) -> &'a mut FastBTreeSet<ThreadId> {
        unsafe { &mut *self.threads_removed }
    }
    fn internal_get_new_windows<'a>(&'a mut self) -> &'a mut Vec<WindowCreateOptions> {
        unsafe { &mut *self.new_windows }
    }
    fn internal_get_current_window_handle<'a>(&'a self) -> &'a RawWindowHandle {
        unsafe { &*self.current_window_handle }
    }
    fn internal_get_extern_system_callbacks<'a>(&'a self) -> &'a ExternalSystemCallbacks {
        unsafe { &*self.system_callbacks }
    }
    fn internal_get_stop_propagation<'a>(&'a mut self) -> &'a mut bool {
        unsafe { &mut *self.stop_propagation }
    }
    fn internal_get_focus_target<'a>(&'a mut self) -> &'a mut Option<FocusTarget> {
        unsafe { &mut *self.focus_target }
    }
    fn internal_get_current_scroll_states<'a>(
        &'a self,
    ) -> &'a BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> {
        unsafe { &*self.current_scroll_states }
    }
    fn internal_get_css_properties_changed_in_callbacks<'a>(
        &'a mut self,
    ) -> &'a mut BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>> {
        unsafe { &mut *self.css_properties_changed_in_callbacks }
    }
    fn internal_get_nodes_scrolled_in_callback<'a>(
        &'a mut self,
    ) -> &'a mut BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, LogicalPosition>> {
        unsafe { &mut *self.nodes_scrolled_in_callback }
    }
    fn internal_get_hit_dom_node<'a>(&'a self) -> DomNodeId {
        self.hit_dom_node
    }
    fn internal_get_cursor_relative_to_item<'a>(&'a self) -> OptionLogicalPosition {
        self.cursor_relative_to_item
    }
    fn internal_get_cursor_in_viewport<'a>(&'a self) -> OptionLogicalPosition {
        self.cursor_in_viewport
    }
    fn internal_get_words_changed_in_callbacks<'a>(
        &'a mut self,
    ) -> &'a mut BTreeMap<DomId, BTreeMap<NodeId, AzString>> {
        unsafe { &mut *self.words_changed_in_callbacks }
    }
    fn internal_get_images_changed_in_callbacks<'a>(
        &'a mut self,
    ) -> &'a mut BTreeMap<DomId, BTreeMap<NodeId, (ImageRef, UpdateImageType)>> {
        unsafe { &mut *self.images_changed_in_callbacks }
    }
    fn internal_get_image_masks_changed_in_callbacks<'a>(
        &'a mut self,
    ) -> &'a mut BTreeMap<DomId, BTreeMap<NodeId, ImageMask>> {
        unsafe { &mut *self.image_masks_changed_in_callbacks }
    }

    // public functions
    pub fn get_hit_node(&self) -> DomNodeId {
        self.internal_get_hit_dom_node()
    }
    pub fn get_system_time_fn(&self) -> GetSystemTimeCallback {
        self.internal_get_extern_system_callbacks()
            .get_system_time_fn
    }
    pub fn get_thread_create_fn(&self) -> CreateThreadCallback {
        self.internal_get_extern_system_callbacks().create_thread_fn
    }
    pub fn get_cursor_relative_to_node(&self) -> OptionLogicalPosition {
        self.internal_get_cursor_relative_to_item()
    }
    pub fn get_cursor_relative_to_viewport(&self) -> OptionLogicalPosition {
        self.internal_get_cursor_in_viewport()
    }
    pub fn get_current_window_state(&self) -> WindowState {
        self.internal_get_current_window_state().clone().into()
    }
    pub fn get_current_window_flags(&self) -> WindowFlags {
        self.internal_get_current_window_state().flags.clone()
    }
    pub fn get_current_keyboard_state(&self) -> KeyboardState {
        self.internal_get_current_window_state()
            .keyboard_state
            .clone()
    }
    pub fn get_current_mouse_state(&self) -> MouseState {
        self.internal_get_current_window_state().mouse_state.clone()
    }
    pub fn get_previous_window_state(&self) -> Option<WindowState> {
        Some(
            self.internal_get_previous_window_state()
                .as_ref()?
                .clone()
                .into(),
        )
    }
    pub fn get_previous_keyboard_state(&self) -> Option<KeyboardState> {
        Some(
            self.internal_get_previous_window_state()
                .as_ref()?
                .keyboard_state
                .clone(),
        )
    }
    pub fn get_previous_mouse_state(&self) -> Option<MouseState> {
        Some(
            self.internal_get_previous_window_state()
                .as_ref()?
                .mouse_state
                .clone(),
        )
    }
    pub fn get_current_window_handle(&self) -> RawWindowHandle {
        self.internal_get_current_window_handle().clone()
    }
    pub fn get_current_time(&self) -> Instant {
        (self
            .internal_get_extern_system_callbacks()
            .get_system_time_fn
            .cb)()
    }
    pub fn get_gl_context(&self) -> OptionGlContextPtr {
        self.internal_get_gl_context().clone()
    }

    pub fn get_scroll_position(&self, node_id: DomNodeId) -> Option<LogicalPosition> {
        self.internal_get_current_scroll_states()
            .get(&node_id.dom)?
            .get(&node_id.node)
            .map(|sp| {
                LogicalPosition::new(
                    sp.children_rect.origin.x - sp.parent_rect.origin.x,
                    sp.children_rect.origin.y - sp.parent_rect.origin.y,
                )
            })
    }

    pub fn set_scroll_position(&mut self, node_id: DomNodeId, scroll_position: LogicalPosition) {
        self.internal_get_nodes_scrolled_in_callback()
            .entry(node_id.dom)
            .or_insert_with(|| BTreeMap::new())
            .insert(node_id.node, scroll_position);
    }

    pub fn get_parent(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let nid_internal = match node_id.node.into_crate_internal() {
            Some(id) => id,
            None => return None, // Invalid input NodeHierarchyItemId
        };

        let layout_result = match self.internal_get_layout_results().get(node_id.dom.inner) {
            Some(lr) => lr,
            None => return None, // DomId not found in layout_results
        };

        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let node_data_container = layout_result.styled_dom.node_data.as_container();

        let mut current_search_id = nid_internal;

        // Traverse up the hierarchy until a non-anonymous parent is found or the root is reached.
        while let Some(parent_id) = node_hierarchy.get(current_search_id).and_then(|n| n.parent_id()) {
            // Check if the parent node data exists and if it's marked as anonymous.
            // If it is, we skip it and continue searching up the hierarchy.
            if node_data_container.get(parent_id).map_or(false, |nd| nd.is_anonymous) {
                current_search_id = parent_id; // Continue search from this anonymous parent.
            } else {
                // Found a non-anonymous parent, return it.
                return Some(DomNodeId {
                    dom: node_id.dom, // The DomId remains the same context
                    node: NodeHierarchyItemId::from_crate_internal(Some(parent_id)),
                });
            }
        }
        None // No non-anonymous parent found up to the root.
    }

    pub fn get_previous_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let nid = node_id.node.into_crate_internal()?;
        self.internal_get_layout_results()
            .get(node_id.dom.inner)?
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(node_id.node.into_crate_internal()?)?
            .previous_sibling_id()
            .map(|nid| DomNodeId {
                dom: node_id.dom,
                node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
            })
    }

    pub fn get_next_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let nid = node_id.node.into_crate_internal()?;
        self.internal_get_layout_results()
            .get(node_id.dom.inner)?
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(node_id.node.into_crate_internal()?)?
            .next_sibling_id()
            .map(|nid| DomNodeId {
                dom: node_id.dom,
                node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
            })
    }

    pub fn get_first_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let nid = node_id.node.into_crate_internal()?;
        self.internal_get_layout_results()
            .get(node_id.dom.inner)?
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .first_child_id(nid)
            .map(|nid| DomNodeId {
                dom: node_id.dom,
                node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
            })
    }

    pub fn get_last_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let nid = node_id.node.into_crate_internal()?;
        self.internal_get_layout_results()
            .get(node_id.dom.inner)?
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(node_id.node.into_crate_internal()?)?
            .last_child_id()
            .map(|nid| DomNodeId {
                dom: node_id.dom,
                node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
            })
    }

    pub fn get_dataset(&mut self, node_id: DomNodeId) -> Option<RefAny> {
        self.internal_get_layout_results()
            .get(node_id.dom.inner)?
            .styled_dom
            .node_data
            .as_container()
            .get(node_id.node.into_crate_internal()?)?
            .dataset
            .as_ref()
            .map(|s| s.clone())
    }

    pub fn get_node_id_of_root_dataset(&mut self, search_key: RefAny) -> Option<DomNodeId> {
        let mut found: Option<(u64, DomNodeId)> = None;

        for (dom_node_id, layout_result) in self.internal_get_layout_results().iter().enumerate() {
            for (node_id, node_data) in layout_result
                .styled_dom
                .node_data
                .as_container()
                .iter()
                .enumerate()
            {
                if let Some(dataset) = node_data.dataset.as_ref() {
                    // dataset RefAny has to point to the same instance
                    if dataset._internal_ptr as usize != search_key._internal_ptr as usize {
                        continue;
                    }

                    let node_id = DomNodeId {
                        dom: DomId { inner: dom_node_id },
                        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node_id))),
                    };

                    if (dataset.instance_id as u64)
                        < found.as_ref().map(|s| s.0).unwrap_or(u64::MAX)
                    {
                        found = Some((dataset.instance_id, node_id))
                    }
                }
            }
        }

        found.map(|s| s.1)
    }

    pub fn set_window_state(&mut self, new_state: WindowState) {
        *self.internal_get_modifiable_window_state() = new_state;
    }

    pub fn set_window_flags(&mut self, new_flags: WindowFlags) {
        self.internal_get_modifiable_window_state().flags = new_flags;
    }

    pub fn set_css_property(&mut self, node_id: DomNodeId, prop: CssProperty) {
        if let Some(nid) = node_id.node.into_crate_internal() {
            self.internal_get_css_properties_changed_in_callbacks()
                .entry(node_id.dom)
                .or_insert_with(|| BTreeMap::new())
                .entry(nid)
                .or_insert_with(|| Vec::new())
                .push(prop);
        }
    }

    pub fn set_focus(&mut self, target: FocusTarget) {
        *self.internal_get_focus_target() = Some(target);
    }

    pub fn get_string_contents(&self, node_id: DomNodeId) -> Option<AzString> {
        self.internal_get_layout_results()
            .get(node_id.dom.inner)?
            .words_cache
            .get(&node_id.node.into_crate_internal()?)
            .map(|words| words.internal_str.clone())
    }

    pub fn set_string_contents(&mut self, node_id: DomNodeId, new_string_contents: AzString) {
        if let Some(nid) = node_id.node.into_crate_internal() {
            self.internal_get_words_changed_in_callbacks()
                .entry(node_id.dom)
                .or_insert_with(|| BTreeMap::new())
                .insert(nid, new_string_contents);
        }
    }

    pub fn get_inline_text(&self, node_id: DomNodeId) -> Option<InlineText> {
        let nid = node_id.node.into_crate_internal()?;
        let layout_result = self.internal_get_layout_results().get(node_id.dom.inner)?;
        let words = layout_result.words_cache.get(&nid)?;
        let shaped_words = layout_result.shaped_words_cache.get(&nid)?;
        let word_positions = layout_result.positioned_words_cache.get(&nid)?;
        let positioned_rectangles = layout_result.rects.as_ref();
        let positioned_rectangle = positioned_rectangles.get(nid)?;
        let (_, inline_text_layout) = positioned_rectangle.resolved_text_layout_options.as_ref()?;
        Some(crate::app_resources::get_inline_text(
            &words,
            &shaped_words,
            &word_positions,
            &inline_text_layout,
        ))
    }

    /// Returns the FontRef for the given NodeId
    pub fn get_font_ref(&self, node_id: DomNodeId) -> Option<FontRef> {
        use crate::styled_dom::StyleFontFamiliesHash;

        let layout_result = self.internal_get_layout_results().get(node_id.dom.inner)?;
        let renderer_resources = self.internal_get_renderer_resources();

        let node_data = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data.get(node_id.node.into_crate_internal()?)?;

        if !node_data.is_text_node() {
            return None;
        }

        let nid = node_id.node.into_crate_internal()?;
        let styled_nodes = layout_result.styled_dom.styled_nodes.as_container();
        styled_nodes
            .get(nid)
            .map(|s| {
                layout_result
                    .styled_dom
                    .css_property_cache
                    .ptr
                    .get_font_id_or_default(node_data, &nid, &s.state)
            })
            .map(|css_font_families| StyleFontFamiliesHash::new(css_font_families.as_ref()))
            .and_then(|css_font_families_hash| {
                renderer_resources.get_font_family(&css_font_families_hash)
            })
            .and_then(|css_font_family| renderer_resources.get_font_key(&css_font_family))
            .and_then(|font_key| renderer_resources.get_registered_font(&font_key))
            .map(|f| f.0.clone())
    }

    pub fn get_text_layout_options(&self, node_id: DomNodeId) -> Option<ResolvedTextLayoutOptions> {
        let layout_result = self.internal_get_layout_results().get(node_id.dom.inner)?;
        let nid = node_id.node.into_crate_internal()?;
        let positioned_rectangles = layout_result.rects.as_ref();
        let positioned_rectangle = positioned_rectangles.get(nid)?;
        let (text_layout_options, _) =
            positioned_rectangle.resolved_text_layout_options.as_ref()?;
        Some(text_layout_options.clone())
    }

    pub fn get_computed_css_property(
        &self,
        node_id: DomNodeId,
        property_type: CssPropertyType,
    ) -> Option<CssProperty> {
        let layout_result = self.internal_get_layout_results().get(node_id.dom.inner)?;

        /*
            if node_id.dom != self.get_hit_node().dom {
                return None;
            }
            let nid = node_id.node.into_crate_internal()?;
            let css_property_cache = self.internal_get_css_property_cache();
            let styled_nodes = self.internal_get_styled_node_states();
            let styled_node_state = styled_nodes.internal.get(nid)?; //
            let node_data = self.internal_get_
        */

        // TODO: can't access self.styled_dom.node_data[node_id].classes because
        // self.styled_dom.node_data[node_id].dataset may be borrowed

        None
    }

    pub fn stop_propagation(&mut self) {
        *self.internal_get_stop_propagation() = true;
    }

    pub fn create_window(&mut self, window: WindowCreateOptions) {
        self.internal_get_new_windows().push(window);
    }

    /// Starts a thread, returns Some(thread_id) if the `thread_initialize_data` is the only copy
    pub fn start_thread(
        &mut self,
        thread_initialize_data: RefAny,
        writeback_data: RefAny,
        callback: ThreadCallbackType,
    ) -> Option<ThreadId> {
        if thread_initialize_data.has_no_copies() {
            let callback = ThreadCallback { cb: callback };
            let thread_id = ThreadId::unique();
            let thread = (self
                .internal_get_extern_system_callbacks()
                .create_thread_fn
                .cb)(thread_initialize_data, writeback_data, callback);
            self.internal_get_threads().insert(thread_id, thread);
            Some(thread_id)
        } else {
            None
        }
    }

    #[cfg(feature = "std")]
    pub fn send_thread_msg(&mut self, thread_id: ThreadId, msg: ThreadSendMsg) -> bool {
        if let Some(thread) = self.internal_get_threads().get_mut(&thread_id) {
            if let Some(s) = thread.ptr.lock().ok() {
                s.sender.send(msg).is_ok()
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Removes and stops a thread, sending one last `ThreadSendMsg::TerminateThread`
    pub fn stop_thread(&mut self, thread_id: ThreadId) -> bool {
        self.internal_get_threads_removed().insert(thread_id)
    }

    pub fn start_timer(&mut self, timer: Timer) -> TimerId {
        let timer_id = TimerId::unique();
        // TODO: perform sanity checks (timer should not be created in the past, etc.)
        self.internal_get_timers().insert(timer_id, timer);
        timer_id
    }

    pub fn start_animation(
        &mut self,
        dom_node_id: DomNodeId,
        animation: Animation,
    ) -> Option<TimerId> {
        use crate::task::SystemTimeDiff;

        let layout_result = self
            .internal_get_layout_results()
            .get(dom_node_id.dom.inner)?;
        let nid = dom_node_id.node.into_crate_internal()?;

        // timer duration may not be the animation duration if the animatio is infinitely long
        let timer_duration = if animation.repeat == AnimationRepeat::NoRepeat {
            Some(animation.duration.clone())
        } else {
            None // infinite
        };

        let parent_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .parent_id()
            .unwrap_or(NodeId::ZERO);
        let current_size = layout_result.rects.as_ref().get(nid)?.size;
        let parent_size = layout_result.rects.as_ref().get(nid)?.size;

        if animation.from.get_type() != animation.to.get_type() {
            return None;
        }

        let timer_id = TimerId::unique();

        let now = self.get_current_time();

        let animation_data = AnimationData {
            from: animation.from,
            to: animation.to,
            start: now.clone(),
            repeat: animation.repeat,
            interpolate: animation.easing,
            duration: animation.duration,
            relayout_on_finish: animation.relayout_on_finish,
            parent_rect_width: parent_size.width,
            parent_rect_height: parent_size.height,
            current_rect_width: current_size.width,
            current_rect_height: current_size.height,
            get_system_time_fn: self
                .internal_get_extern_system_callbacks()
                .get_system_time_fn
                .clone(),
        };

        let timer = Timer {
            data: RefAny::new(animation_data),
            node_id: Some(dom_node_id).into(),
            created: now,
            run_count: 0,
            last_run: None.into(),
            delay: None.into(),
            interval: Some(AzDuration::System(SystemTimeDiff::from_millis(10))).into(),
            timeout: timer_duration.into(),
            callback: TimerCallback {
                cb: drive_animation_func,
            },
        };

        self.internal_get_timers().insert(timer_id, timer);

        Some(timer_id)
    }

    pub fn stop_timer(&mut self, timer_id: TimerId) -> bool {
        self.internal_get_timers_removed().insert(timer_id)
    }

    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<PositionInfo> {
        let layout_result = self.internal_get_layout_results().get(node_id.dom.inner)?;
        let nid = node_id.node.into_crate_internal()?;
        let positioned_rectangles = layout_result.rects.as_ref();
        let positioned_rectangle = positioned_rectangles.get(nid)?;
        Some(positioned_rectangle.position)
    }

    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        let layout_result = self.internal_get_layout_results().get(node_id.dom.inner)?;
        let nid = node_id.node.into_crate_internal()?;
        let positioned_rectangles = layout_result.rects.as_ref();
        let positioned_rectangle = positioned_rectangles.get(nid)?;
        Some(positioned_rectangle.size)
    }

    /// Adds an image to the internal image cache
    pub fn add_image(&mut self, css_id: AzString, image: ImageRef) {
        self.internal_get_image_cache()
            .add_css_image_id(css_id, image);
    }

    pub fn has_image(&self, css_id: &AzString) -> bool {
        self.internal_get_image_cache_ref()
            .get_css_image_id(css_id)
            .is_some()
    }

    pub fn get_image(&self, css_id: &AzString) -> Option<ImageRef> {
        self.internal_get_image_cache_ref()
            .get_css_image_id(css_id)
            .cloned()
    }

    /// Deletes an image from the internal image cache
    pub fn delete_image(&mut self, css_id: &AzString) {
        self.internal_get_image_cache().delete_css_image_id(css_id);
    }

    pub fn update_image(
        &mut self,
        node_id: DomNodeId,
        new_image: ImageRef,
        image_type: UpdateImageType,
    ) {
        if let Some(nid) = node_id.node.into_crate_internal() {
            self.internal_get_images_changed_in_callbacks()
                .entry(node_id.dom)
                .or_insert_with(|| BTreeMap::new())
                .insert(nid, (new_image, image_type));
        }
    }

    pub fn update_image_mask(&mut self, node_id: DomNodeId, new_image_mask: ImageMask) {
        if let Some(nid) = node_id.node.into_crate_internal() {
            self.internal_get_image_masks_changed_in_callbacks()
                .entry(node_id.dom)
                .or_insert_with(|| BTreeMap::new())
                .insert(nid, new_image_mask);
        }
    }
}

impl Clone for CallbackInfo {
    fn clone(&self) -> Self {
        Self {
            layout_results: self.layout_results,
            layout_results_count: self.layout_results_count,
            renderer_resources: self.renderer_resources,
            previous_window_state: self.previous_window_state,
            current_window_state: self.current_window_state,
            modifiable_window_state: self.modifiable_window_state,
            gl_context: self.gl_context,
            image_cache: self.image_cache,
            system_fonts: self.system_fonts,
            timers: self.timers,
            threads: self.threads,
            timers_removed: self.timers_removed,
            threads_removed: self.threads_removed,
            current_window_handle: self.current_window_handle,
            new_windows: self.new_windows,
            system_callbacks: self.system_callbacks,
            stop_propagation: self.stop_propagation,
            focus_target: self.focus_target,
            words_changed_in_callbacks: self.words_changed_in_callbacks,
            images_changed_in_callbacks: self.images_changed_in_callbacks,
            image_masks_changed_in_callbacks: self.image_masks_changed_in_callbacks,
            css_properties_changed_in_callbacks: self.css_properties_changed_in_callbacks,
            current_scroll_states: self.current_scroll_states,
            nodes_scrolled_in_callback: self.nodes_scrolled_in_callback,
            hit_dom_node: self.hit_dom_node,
            cursor_relative_to_item: self.cursor_relative_to_item,
            cursor_in_viewport: self.cursor_in_viewport,
            _abi_ref: self._abi_ref,
            _abi_mut: self._abi_mut,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum UpdateImageType {
    Background,
    Content,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationData {
    pub from: CssProperty,
    pub to: CssProperty,
    pub start: AzInstant,
    pub duration: AzDuration,
    pub repeat: AnimationRepeat,
    pub interpolate: AnimationInterpolationFunction,
    pub relayout_on_finish: bool,
    pub parent_rect_width: f32,
    pub parent_rect_height: f32,
    pub current_rect_width: f32,
    pub current_rect_height: f32,
    pub get_system_time_fn: GetSystemTimeCallback,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Animation {
    pub from: CssProperty,
    pub to: CssProperty,
    pub duration: AzDuration,
    pub repeat: AnimationRepeat,
    pub repeat_times: AnimationRepeatCount,
    pub easing: AnimationInterpolationFunction,
    pub relayout_on_finish: bool,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub enum AnimationRepeat {
    NoRepeat,
    Loop,
    PingPong,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash)]
#[repr(C, u8)]
pub enum AnimationRepeatCount {
    Times(usize),
    Infinite,
}

// callback that drives an animation
extern "C" fn drive_animation_func(
    anim_data: &mut RefAny,
    info: &mut TimerCallbackInfo,
) -> TimerCallbackReturn {
    let mut anim_data = match anim_data.downcast_mut::<AnimationData>() {
        Some(s) => s,
        None => {
            return TimerCallbackReturn {
                should_update: Update::DoNothing,
                should_terminate: TerminateTimer::Terminate,
            };
        }
    };

    let mut anim_data = &mut *anim_data;

    let node_id = match info.node_id.into_option() {
        Some(s) => s,
        None => {
            return TimerCallbackReturn {
                should_update: Update::DoNothing,
                should_terminate: TerminateTimer::Terminate,
            };
        }
    };

    // calculate the interpolated CSS property
    let resolver = InterpolateResolver {
        parent_rect_width: anim_data.parent_rect_width,
        parent_rect_height: anim_data.parent_rect_height,
        current_rect_width: anim_data.current_rect_width,
        current_rect_height: anim_data.current_rect_height,
        interpolate_func: anim_data.interpolate,
    };

    let anim_next_end = anim_data
        .start
        .add_optional_duration(Some(&anim_data.duration));
    let now = (anim_data.get_system_time_fn.cb)();
    let t = now.linear_interpolate(anim_data.start.clone(), anim_next_end.clone());
    let interpolated_css = anim_data.from.interpolate(&anim_data.to, t, &resolver);

    // actual animation happens here
    info.callback_info
        .set_css_property(node_id, interpolated_css);

    // if the timer has finished one iteration, what next?
    if now > anim_next_end {
        match anim_data.repeat {
            AnimationRepeat::Loop => {
                // reset timer
                anim_data.start = now;
            }
            AnimationRepeat::PingPong => {
                use core::mem;
                // swap start and end and reset timer
                mem::swap(&mut anim_data.from, &mut anim_data.to);
                anim_data.start = now;
            }
            AnimationRepeat::NoRepeat => {
                // remove / cancel timer
                return TimerCallbackReturn {
                    should_terminate: TerminateTimer::Terminate,
                    should_update: if anim_data.relayout_on_finish {
                        Update::RefreshDom
                    } else {
                        Update::DoNothing
                    },
                };
            }
        }
    }

    // if the timer has finished externally, what next?
    if info.is_about_to_finish {
        TimerCallbackReturn {
            should_terminate: TerminateTimer::Terminate,
            should_update: if anim_data.relayout_on_finish {
                Update::RefreshDom
            } else {
                Update::DoNothing
            },
        }
    } else {
        TimerCallbackReturn {
            should_terminate: TerminateTimer::Continue,
            should_update: Update::DoNothing,
        }
    }
}

pub type CallbackType = extern "C" fn(&mut RefAny, &mut CallbackInfo) -> Update;

// -- opengl callback

/// Callbacks that returns a rendered OpenGL texture
#[repr(C)]
pub struct RenderImageCallback {
    pub cb: RenderImageCallbackType,
}
impl_callback!(RenderImageCallback);

#[derive(Debug)]
#[repr(C)]
pub struct RenderImageCallbackInfo {
    /// The ID of the DOM node that the ImageCallback was attached to
    callback_node_id: DomNodeId,
    /// Bounds of the laid-out node
    bounds: HidpiAdjustedBounds,
    /// Optional OpenGL context pointer
    gl_context: *const OptionGlContextPtr,
    image_cache: *const ImageCache,
    system_fonts: *const FcFontCache,
    node_hierarchy: *const NodeHierarchyItemVec,
    words_cache: *const BTreeMap<NodeId, Words>,
    shaped_words_cache: *const BTreeMap<NodeId, ShapedWords>,
    positioned_words_cache: *const BTreeMap<NodeId, WordPositions>,
    positioned_rects: *const NodeDataContainer<PositionedRectangle>,
    /// Extension for future ABI stability (referenced data)
    _abi_ref: *const c_void,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut c_void,
}

// same as the implementations on CallbackInfo, just slightly adjusted for the
// RenderImageCallbackInfo
impl Clone for RenderImageCallbackInfo {
    fn clone(&self) -> Self {
        Self {
            callback_node_id: self.callback_node_id,
            bounds: self.bounds,
            gl_context: self.gl_context,
            image_cache: self.image_cache,
            system_fonts: self.system_fonts,
            node_hierarchy: self.node_hierarchy,
            words_cache: self.words_cache,
            shaped_words_cache: self.shaped_words_cache,
            positioned_words_cache: self.positioned_words_cache,
            positioned_rects: self.positioned_rects,
            _abi_ref: self._abi_ref,
            _abi_mut: self._abi_mut,
        }
    }
}

impl RenderImageCallbackInfo {
    pub fn new<'a>(
        gl_context: &'a OptionGlContextPtr,
        image_cache: &'a ImageCache,
        system_fonts: &'a FcFontCache,
        node_hierarchy: &'a NodeHierarchyItemVec,
        words_cache: &'a BTreeMap<NodeId, Words>,
        shaped_words_cache: &'a BTreeMap<NodeId, ShapedWords>,
        positioned_words_cache: &'a BTreeMap<NodeId, WordPositions>,
        positioned_rects: &'a NodeDataContainer<PositionedRectangle>,
        bounds: HidpiAdjustedBounds,
        callback_node_id: DomNodeId,
    ) -> Self {
        Self {
            callback_node_id,
            gl_context: gl_context as *const OptionGlContextPtr,
            image_cache: image_cache as *const ImageCache,
            system_fonts: system_fonts as *const FcFontCache,
            node_hierarchy: node_hierarchy as *const NodeHierarchyItemVec,
            words_cache: words_cache as *const BTreeMap<NodeId, Words>,
            shaped_words_cache: shaped_words_cache as *const BTreeMap<NodeId, ShapedWords>,
            positioned_words_cache: positioned_words_cache
                as *const BTreeMap<NodeId, WordPositions>,
            positioned_rects: positioned_rects as *const NodeDataContainer<PositionedRectangle>,
            bounds,
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
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
    fn internal_get_bounds<'a>(&'a self) -> HidpiAdjustedBounds {
        self.bounds
    }
    fn internal_get_node_hierarchy<'a>(&'a self) -> &'a NodeHierarchyItemVec {
        unsafe { &*self.node_hierarchy }
    }
    fn internal_get_words_cache<'a>(&'a self) -> &'a BTreeMap<NodeId, Words> {
        unsafe { &*self.words_cache }
    }
    fn internal_get_shaped_words_cache<'a>(&'a self) -> &'a BTreeMap<NodeId, ShapedWords> {
        unsafe { &*self.shaped_words_cache }
    }
    fn internal_get_positioned_words_cache<'a>(&'a self) -> &'a BTreeMap<NodeId, WordPositions> {
        unsafe { &*self.positioned_words_cache }
    }
    fn internal_get_positioned_rectangles<'a>(
        &'a self,
    ) -> &'a NodeDataContainer<PositionedRectangle> {
        unsafe { &*self.positioned_rects }
    }

    pub fn get_gl_context(&self) -> OptionGlContextPtr {
        self.internal_get_gl_context().clone()
    }
    pub fn get_bounds(&self) -> HidpiAdjustedBounds {
        self.internal_get_bounds()
    }
    pub fn get_callback_node_id(&self) -> DomNodeId {
        self.callback_node_id
    }

    // fn get_font()
    // fn get_image()

    pub fn get_inline_text(&self, node_id: DomNodeId) -> Option<InlineText> {
        if node_id.dom != self.get_callback_node_id().dom {
            return None;
        }

        let nid = node_id.node.into_crate_internal()?;
        let words = self.internal_get_words_cache();
        let words = words.get(&nid)?;
        let shaped_words = self.internal_get_shaped_words_cache();
        let shaped_words = shaped_words.get(&nid)?;
        let word_positions = self.internal_get_positioned_words_cache();
        let word_positions = word_positions.get(&nid)?;
        let positioned_rectangle = self.internal_get_positioned_rectangles();
        let positioned_rectangle = positioned_rectangle.as_ref();
        let positioned_rectangle = positioned_rectangle.get(nid)?;
        let (_, inline_text_layout) = positioned_rectangle.resolved_text_layout_options.as_ref()?;

        Some(crate::app_resources::get_inline_text(
            &words,
            &shaped_words,
            &word_positions,
            &inline_text_layout,
        ))
    }

    pub fn get_parent(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        if node_id.dom != self.get_callback_node_id().dom {
            None
        } else {
            self.internal_get_node_hierarchy()
                .as_container()
                .get(node_id.node.into_crate_internal()?)?
                .parent_id()
                .map(|nid| DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
                })
        }
    }

    pub fn get_previous_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        if node_id.dom != self.get_callback_node_id().dom {
            None
        } else {
            self.internal_get_node_hierarchy()
                .as_container()
                .get(node_id.node.into_crate_internal()?)?
                .previous_sibling_id()
                .map(|nid| DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
                })
        }
    }

    pub fn get_next_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        if node_id.dom != self.get_callback_node_id().dom {
            None
        } else {
            self.internal_get_node_hierarchy()
                .as_container()
                .get(node_id.node.into_crate_internal()?)?
                .next_sibling_id()
                .map(|nid| DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
                })
        }
    }

    pub fn get_first_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        if node_id.dom != self.get_callback_node_id().dom {
            None
        } else {
            let nid = node_id.node.into_crate_internal()?;
            self.internal_get_node_hierarchy()
                .as_container()
                .get(nid)?
                .first_child_id(nid)
                .map(|nid| DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
                })
        }
    }

    pub fn get_last_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        if node_id.dom != self.get_callback_node_id().dom {
            None
        } else {
            self.internal_get_node_hierarchy()
                .as_container()
                .get(node_id.node.into_crate_internal()?)?
                .last_child_id()
                .map(|nid| DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
                })
        }
    }
}

/// Callback that - given the width and height of the expected image - renders an image
pub type RenderImageCallbackType =
    extern "C" fn(&mut RefAny, &mut RenderImageCallbackInfo) -> ImageRef;

// -- iframe callback

pub type IFrameCallbackType =
    extern "C" fn(&mut RefAny, &mut IFrameCallbackInfo) -> IFrameCallbackReturn;

/// Callback that, given a rectangle area on the screen, returns the DOM
/// appropriate for that bounds (useful for infinite lists)
#[repr(C)]
pub struct IFrameCallback {
    pub cb: IFrameCallbackType,
}
impl_callback!(IFrameCallback);

#[derive(Debug)]
#[repr(C)]
pub struct IFrameCallbackInfo {
    pub system_fonts: *const FcFontCache,
    pub image_cache: *const ImageCache,
    pub window_theme: WindowTheme,
    pub bounds: HidpiAdjustedBounds,
    pub scroll_size: LogicalSize,
    pub scroll_offset: LogicalPosition,
    pub virtual_scroll_size: LogicalSize,
    pub virtual_scroll_offset: LogicalPosition,
    /// Extension for future ABI stability (referenced data)
    _abi_ref: *const c_void,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut c_void,
}

impl Clone for IFrameCallbackInfo {
    fn clone(&self) -> Self {
        Self {
            system_fonts: self.system_fonts,
            image_cache: self.image_cache,
            window_theme: self.window_theme,
            bounds: self.bounds,
            scroll_size: self.scroll_size,
            scroll_offset: self.scroll_offset,
            virtual_scroll_size: self.virtual_scroll_size,
            virtual_scroll_offset: self.virtual_scroll_offset,
            _abi_ref: self._abi_ref,
            _abi_mut: self._abi_mut,
        }
    }
}

impl IFrameCallbackInfo {
    pub fn new<'a>(
        system_fonts: &'a FcFontCache,
        image_cache: &'a ImageCache,
        window_theme: WindowTheme,
        bounds: HidpiAdjustedBounds,
        scroll_size: LogicalSize,
        scroll_offset: LogicalPosition,
        virtual_scroll_size: LogicalSize,
        virtual_scroll_offset: LogicalPosition,
    ) -> Self {
        Self {
            system_fonts: system_fonts as *const FcFontCache,
            image_cache: image_cache as *const ImageCache,
            window_theme,
            bounds,
            scroll_size,
            scroll_offset,
            virtual_scroll_size,
            virtual_scroll_offset,
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    pub fn get_bounds(&self) -> HidpiAdjustedBounds {
        self.bounds
    }

    // fn get_font()
    // fn get_image()

    fn internal_get_system_fonts<'a>(&'a self) -> &'a FcFontCache {
        unsafe { &*self.system_fonts }
    }
    fn internal_get_image_cache<'a>(&'a self) -> &'a ImageCache {
        unsafe { &*self.image_cache }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct IFrameCallbackReturn {
    pub dom: StyledDom,
    pub scroll_size: LogicalSize,
    pub scroll_offset: LogicalPosition,
    pub virtual_scroll_size: LogicalSize,
    pub virtual_scroll_offset: LogicalPosition,
}

impl Default for IFrameCallbackReturn {
    fn default() -> IFrameCallbackReturn {
        IFrameCallbackReturn {
            dom: StyledDom::default(),
            scroll_size: LogicalSize::zero(),
            scroll_offset: LogicalPosition::zero(),
            virtual_scroll_size: LogicalSize::zero(),
            virtual_scroll_offset: LogicalPosition::zero(),
        }
    }
}

// --  thread callback
pub type ThreadCallbackType = extern "C" fn(RefAny, ThreadSender, ThreadReceiver);

#[repr(C)]
pub struct ThreadCallback {
    pub cb: ThreadCallbackType,
}
impl_callback!(ThreadCallback);

// -- timer callback

/// Callback that can runs on every frame on the main thread - can modify the app data model
#[repr(C)]
pub struct TimerCallback {
    pub cb: TimerCallbackType,
}
impl_callback!(TimerCallback);

#[derive(Debug)]
#[repr(C)]
pub struct TimerCallbackInfo {
    /// Callback info for this timer
    pub callback_info: CallbackInfo,
    /// If the timer is attached to a DOM node, this will contain the node ID
    pub node_id: OptionDomNodeId,
    /// Time when the frame was started rendering
    pub frame_start: Instant,
    /// How many times this callback has been called
    pub call_count: usize,
    /// Set to true ONCE on the LAST invocation of the timer (if the timer has a timeout set)
    /// This is useful to rebuild the DOM once the timer (usually an animation) has finished.
    pub is_about_to_finish: bool,
    /// Extension for future ABI stability (referenced data)
    pub(crate) _abi_ref: *const c_void,
    /// Extension for future ABI stability (mutable data)
    pub(crate) _abi_mut: *mut c_void,
}

impl Clone for TimerCallbackInfo {
    fn clone(&self) -> Self {
        Self {
            callback_info: self.callback_info.clone(),
            node_id: self.node_id,
            frame_start: self.frame_start.clone(),
            call_count: self.call_count,
            is_about_to_finish: self.is_about_to_finish,
            _abi_ref: self._abi_ref,
            _abi_mut: self._abi_mut,
        }
    }
}

pub type WriteBackCallbackType = extern "C" fn(
    /* original data */ &mut RefAny,
    /* data to write back */ &mut RefAny,
    &mut CallbackInfo,
) -> Update;

/// Callback that can runs when a thread receives a `WriteBack` message
#[repr(C)]
pub struct WriteBackCallback {
    pub cb: WriteBackCallbackType,
}
impl_callback!(WriteBackCallback);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TimerCallbackReturn {
    pub should_update: Update,
    pub should_terminate: TerminateTimer,
}

pub type TimerCallbackType = extern "C" fn(
    /* timer internal data */ &mut RefAny,
    &mut TimerCallbackInfo,
) -> TimerCallbackReturn;

/// Gives the `layout()` function access to the `RendererResources` and the `Window`
/// (for querying images and fonts, as well as width / height)
#[derive(Debug)]
#[repr(C)]
pub struct LayoutCallbackInfo {
    /// Window size (so that apps can return a different UI depending on
    /// the window size - mobile / desktop view). Should be later removed
    /// in favor of "resize" handlers and @media queries.
    pub window_size: WindowSize,
    /// Registers whether the UI is dependent on the window theme
    pub theme: WindowTheme,
    /// Allows the layout() function to reference image IDs
    image_cache: *const ImageCache,
    /// OpenGL context so that the layout() function can render textures
    pub gl_context: *const OptionGlContextPtr,
    /// Reference to the system font cache
    system_fonts: *const FcFontCache,
    /// Extension for future ABI stability (referenced data)
    _abi_ref: *const c_void,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut c_void,
}

impl Clone for LayoutCallbackInfo {
    fn clone(&self) -> Self {
        Self {
            window_size: self.window_size,
            theme: self.theme,
            image_cache: self.image_cache,
            gl_context: self.gl_context,
            system_fonts: self.system_fonts,
            _abi_ref: self._abi_ref,
            _abi_mut: self._abi_mut,
        }
    }
}

impl LayoutCallbackInfo {
    pub fn new<'a>(
        window_size: WindowSize,
        theme: WindowTheme,
        image_cache: &'a ImageCache,
        gl_context: &'a OptionGlContextPtr,
        fc_cache: &'a FcFontCache,
    ) -> Self {
        Self {
            window_size,
            theme,
            image_cache: image_cache as *const ImageCache,
            gl_context: gl_context as *const OptionGlContextPtr,
            system_fonts: fc_cache as *const FcFontCache,
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    fn internal_get_image_cache<'a>(&'a self) -> &'a ImageCache {
        unsafe { &*self.image_cache }
    }
    fn internal_get_system_fonts<'a>(&'a self) -> &'a FcFontCache {
        unsafe { &*self.system_fonts }
    }
    fn internal_get_gl_context<'a>(&'a self) -> &'a OptionGlContextPtr {
        unsafe { &*self.gl_context }
    }

    pub fn get_gl_context(&self) -> OptionGlContextPtr {
        self.internal_get_gl_context().clone()
    }

    pub fn get_system_fonts(&self) -> Vec<AzStringPair> {
        let fc_cache = self.internal_get_system_fonts();

        fc_cache
            .list()
            .iter()
            .filter_map(|(pattern, font_id)| {
                let source = fc_cache.get_font_by_id(font_id)?;
                match source {
                    FontSource::Memory(f) => None,
                    FontSource::Disk(d) => Some((pattern.name.as_ref()?.clone(), d.path.clone())),
                }
            })
            .map(|(k, v)| AzStringPair {
                key: k.into(),
                value: v.into(),
            })
            .collect()
    }

    pub fn get_image(&self, image_id: &AzString) -> Option<ImageRef> {
        self.internal_get_image_cache()
            .get_css_image_id(image_id)
            .cloned()
    }
}

/// Information about the bounds of a laid-out div rectangle.
///
/// Necessary when invoking `IFrameCallbacks` and `RenderImageCallbacks`, so
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
        // NOTE: hidpi factor, not system_hidpi_factor!
        self.get_logical_size().to_physical(self.hidpi_factor)
    }

    pub fn get_logical_size(&self) -> LogicalSize {
        self.logical_size
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
    Previous,
    Next,
    First,
    Last,
    NoFocus,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct FocusTargetPath {
    pub dom: DomId,
    pub css_path: CssPath,
}

impl FocusTarget {
    pub fn resolve(
        &self,
        layout_results: &[LayoutResult],
        current_focus: Option<DomNodeId>,
    ) -> Result<Option<DomNodeId>, UpdateFocusWarning> {
        use crate::{callbacks::FocusTarget::*, style::matches_html_element};

        if layout_results.is_empty() {
            return Ok(None);
        }

        macro_rules! search_for_focusable_node_id {
            (
                $layout_results:expr,
                $start_dom_id:expr,
                $start_node_id:expr,
                $get_next_node_fn:ident
            ) => {{
                let mut start_dom_id = $start_dom_id;
                let mut start_node_id = $start_node_id;

                let min_dom_id = DomId::ROOT_ID;
                let max_dom_id = DomId {
                    inner: layout_results.len() - 1,
                };

                // iterate through all DOMs
                loop {
                    // 'outer_dom_iter

                    let layout_result = $layout_results
                        .get(start_dom_id.inner)
                        .ok_or(UpdateFocusWarning::FocusInvalidDomId(start_dom_id.clone()))?;

                    let node_id_valid = layout_result
                        .styled_dom
                        .node_data
                        .as_container()
                        .get(start_node_id)
                        .is_some();

                    if !node_id_valid {
                        return Err(UpdateFocusWarning::FocusInvalidNodeId(
                            NodeHierarchyItemId::from_crate_internal(Some(start_node_id.clone())),
                        ));
                    }

                    if layout_result.styled_dom.node_data.is_empty() {
                        return Err(UpdateFocusWarning::FocusInvalidDomId(start_dom_id.clone()));
                        // ???
                    }

                    let max_node_id = NodeId::new(layout_result.styled_dom.node_data.len() - 1);
                    let min_node_id = NodeId::ZERO;

                    // iterate through nodes in DOM
                    loop {
                        let current_node_id =
                            NodeId::new(start_node_id.index().$get_next_node_fn(1))
                                .max(min_node_id)
                                .min(max_node_id);

                        if layout_result.styled_dom.node_data.as_container()[current_node_id]
                            .is_focusable()
                        {
                            return Ok(Some(DomNodeId {
                                dom: start_dom_id,
                                node: NodeHierarchyItemId::from_crate_internal(Some(
                                    current_node_id,
                                )),
                            }));
                        }

                        if current_node_id == min_node_id && current_node_id < start_node_id {
                            // going in decreasing (previous) direction
                            if start_dom_id == min_dom_id {
                                // root node / root dom encountered
                                return Ok(None);
                            } else {
                                start_dom_id.inner -= 1;
                                start_node_id = NodeId::new(
                                    $layout_results[start_dom_id.inner]
                                        .styled_dom
                                        .node_data
                                        .len()
                                        - 1,
                                );
                                break; // continue 'outer_dom_iter
                            }
                        } else if current_node_id == max_node_id && current_node_id > start_node_id
                        {
                            // going in increasing (next) direction
                            if start_dom_id == max_dom_id {
                                // last dom / last node encountered
                                return Ok(None);
                            } else {
                                start_dom_id.inner += 1;
                                start_node_id = NodeId::ZERO;
                                break; // continue 'outer_dom_iter
                            }
                        } else {
                            start_node_id = current_node_id;
                        }
                    }
                }
            }};
        }

        match self {
            Path(FocusTargetPath { dom, css_path }) => {
                let layout_result = layout_results
                    .get(dom.inner)
                    .ok_or(UpdateFocusWarning::FocusInvalidDomId(dom.clone()))?;
                let html_node_tree = &layout_result.styled_dom.cascade_info;
                let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
                let node_data = &layout_result.styled_dom.node_data;
                let resolved_node_id = html_node_tree
                    .as_container()
                    .linear_iter()
                    .find(|node_id| {
                        matches_html_element(
                            css_path,
                            *node_id,
                            &node_hierarchy.as_container(),
                            &node_data.as_container(),
                            &html_node_tree.as_container(),
                            None,
                        )
                    })
                    .ok_or(UpdateFocusWarning::CouldNotFindFocusNode(css_path.clone()))?;
                Ok(Some(DomNodeId {
                    dom: dom.clone(),
                    node: NodeHierarchyItemId::from_crate_internal(Some(resolved_node_id)),
                }))
            }
            Id(dom_node_id) => {
                let layout_result = layout_results.get(dom_node_id.dom.inner).ok_or(
                    UpdateFocusWarning::FocusInvalidDomId(dom_node_id.dom.clone()),
                )?;
                let node_is_valid = dom_node_id
                    .node
                    .into_crate_internal()
                    .map(|o| {
                        layout_result
                            .styled_dom
                            .node_data
                            .as_container()
                            .get(o)
                            .is_some()
                    })
                    .unwrap_or(false);

                if !node_is_valid {
                    Err(UpdateFocusWarning::FocusInvalidNodeId(
                        dom_node_id.node.clone(),
                    ))
                } else {
                    Ok(Some(dom_node_id.clone()))
                }
            }
            Previous => {
                let last_layout_dom_id = DomId {
                    inner: layout_results.len() - 1,
                };

                // select the previous focusable element or `None`
                // if this was the first focusable element in the DOM
                let (current_focus_dom, current_focus_node_id) = match current_focus {
                    Some(s) => match s.node.into_crate_internal() {
                        Some(n) => (s.dom, n),
                        None => {
                            if let Some(layout_result) = layout_results.get(s.dom.inner) {
                                (
                                    s.dom,
                                    NodeId::new(layout_result.styled_dom.node_data.len() - 1),
                                )
                            } else {
                                (
                                    last_layout_dom_id,
                                    NodeId::new(
                                        layout_results[last_layout_dom_id.inner]
                                            .styled_dom
                                            .node_data
                                            .len()
                                            - 1,
                                    ),
                                )
                            }
                        }
                    },
                    None => (
                        last_layout_dom_id,
                        NodeId::new(
                            layout_results[last_layout_dom_id.inner]
                                .styled_dom
                                .node_data
                                .len()
                                - 1,
                        ),
                    ),
                };

                search_for_focusable_node_id!(
                    layout_results,
                    current_focus_dom,
                    current_focus_node_id,
                    saturating_sub
                );
            }
            Next => {
                // select the previous focusable element or `None`
                // if this was the first focusable element in the DOM, select the first focusable
                // element
                let (current_focus_dom, current_focus_node_id) = match current_focus {
                    Some(s) => match s.node.into_crate_internal() {
                        Some(n) => (s.dom, n),
                        None => {
                            if layout_results.get(s.dom.inner).is_some() {
                                (s.dom, NodeId::ZERO)
                            } else {
                                (DomId::ROOT_ID, NodeId::ZERO)
                            }
                        }
                    },
                    None => (DomId::ROOT_ID, NodeId::ZERO),
                };

                search_for_focusable_node_id!(
                    layout_results,
                    current_focus_dom,
                    current_focus_node_id,
                    saturating_add
                );
            }
            First => {
                let (current_focus_dom, current_focus_node_id) = (DomId::ROOT_ID, NodeId::ZERO);
                search_for_focusable_node_id!(
                    layout_results,
                    current_focus_dom,
                    current_focus_node_id,
                    saturating_add
                );
            }
            Last => {
                let last_layout_dom_id = DomId {
                    inner: layout_results.len() - 1,
                };
                let (current_focus_dom, current_focus_node_id) = (
                    last_layout_dom_id,
                    NodeId::new(
                        layout_results[last_layout_dom_id.inner]
                            .styled_dom
                            .node_data
                            .len()
                            - 1,
                    ),
                );
                search_for_focusable_node_id!(
                    layout_results,
                    current_focus_dom,
                    current_focus_node_id,
                    saturating_add
                );
            }
            NoFocus => Ok(None),
        }
    }
}
