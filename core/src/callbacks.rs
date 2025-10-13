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
    css::{CssPath, CssPropertyValue},
    props::{
        basic::{
            AnimationInterpolationFunction, FontRef, InterpolateResolver, LayoutRect, LayoutSize,
        },
        property::{CssProperty, CssPropertyType},
    },
    AzString,
};
use rust_fontconfig::{FcFontCache, FontSource};

use crate::{
    dom::EventFilter,
    gl::OptionGlContextPtr,
    id::{NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut, NodeId},
    prop_cache::CssPropertyCache,
    resources::{FontInstanceKey, IdNamespace, ImageCache, ImageMask, ImageRef, RendererResources},
    styled_dom::{
        DomId, NodeHierarchyItemId, NodeHierarchyItemVec, StyledDom, StyledNode, StyledNodeVec,
    },
    task::{
        Duration as AzDuration, GetSystemTimeCallback, Instant as AzInstant, Instant,
        TerminateTimer, ThreadId, ThreadReceiver, ThreadSendMsg, TimerId,
    },
    ui_solver::{OverflowingScrollNode, PositionInfo},
    window::{
        AzStringPair, KeyboardState, LogicalPosition, LogicalRect, LogicalSize, MouseState,
        OptionChar, OptionLogicalPosition, PhysicalSize, RawWindowHandle, UpdateFocusWarning,
        WindowFlags, WindowSize, WindowTheme,
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

// -- timer callback

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TimerCallbackReturn {
    pub should_update: Update,
    pub should_terminate: TerminateTimer,
}

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

// ============================================================================
// CORE CALLBACK TYPES (usize-based placeholders)
// ============================================================================
//
// These types use `usize` instead of function pointers to avoid creating
// a circular dependency between azul-core and azul-layout.
//
// The actual function pointers will be stored in azul-layout, which will
// use unsafe code to transmute between usize and the real function pointers.
//
// IMPORTANT: The memory layout must be identical to the real types!
// Tests for this are in azul-layout/src/callbacks.rs
//
// Naming convention: "Core" prefix indicates these are the low-level types

/// Core callback type - uses usize instead of function pointer
/// Will be converted to real function pointer in azul-layout
pub type CoreCallbackType = usize;

/// Stores a callback as usize (will be function pointer in azul-layout)
///
/// Must return an `Update` that denotes if the screen should be redrawn.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CoreCallback {
    pub cb: CoreCallbackType,
}

impl_option!(
    CoreCallback,
    OptionCoreCallback,
    [Debug, Eq, Copy, Clone, PartialEq, PartialOrd, Ord, Hash]
);

/// Data associated with a callback (event filter, callback, and user data)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CoreCallbackData {
    pub event: EventFilter,
    pub callback: CoreCallback,
    pub data: RefAny,
}

impl_vec!(
    CoreCallbackData,
    CoreCallbackDataVec,
    CoreCallbackDataVecDestructor
);
impl_vec_clone!(
    CoreCallbackData,
    CoreCallbackDataVec,
    CoreCallbackDataVecDestructor
);
impl_vec_mut!(CoreCallbackData, CoreCallbackDataVec);
impl_vec_debug!(CoreCallbackData, CoreCallbackDataVec);
impl_vec_partialord!(CoreCallbackData, CoreCallbackDataVec);
impl_vec_ord!(CoreCallbackData, CoreCallbackDataVec);
impl_vec_partialeq!(CoreCallbackData, CoreCallbackDataVec);
impl_vec_eq!(CoreCallbackData, CoreCallbackDataVec);
impl_vec_hash!(CoreCallbackData, CoreCallbackDataVec);

impl CoreCallbackDataVec {
    #[inline]
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, CoreCallbackData> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    #[inline]
    pub fn as_container_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, CoreCallbackData> {
        NodeDataContainerRefMut {
            internal: self.as_mut(),
        }
    }
}

/// Image rendering callback type - uses usize instead of function pointer
pub type CoreRenderImageCallbackType = usize;

/// Callback that returns a rendered OpenGL texture (usize placeholder)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CoreRenderImageCallback {
    pub cb: CoreRenderImageCallbackType,
}

/// Image callback with associated data
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CoreImageCallback {
    pub data: RefAny,
    pub callback: CoreRenderImageCallback,
}
