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
    dom::{DomId, DomNodeId, EventFilter},
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition, PhysicalSize},
    gl::OptionGlContextPtr,
    hit_test::OverflowingScrollNode,
    id::{NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut, NodeId},
    prop_cache::CssPropertyCache,
    refany::RefAny,
    resources::{FontInstanceKey, IdNamespace, ImageCache, ImageMask, ImageRef, RendererResources},
    styled_dom::{NodeHierarchyItemId, NodeHierarchyItemVec, StyledDom, StyledNode, StyledNodeVec},
    task::{
        Duration as AzDuration, GetSystemTimeCallback, Instant as AzInstant, Instant,
        TerminateTimer, ThreadId, ThreadReceiver, ThreadSendMsg, TimerId,
    },
    ui_solver::PositionInfo,
    window::{
        AzStringPair, KeyboardState, MouseState, OptionChar, RawWindowHandle, UpdateFocusWarning,
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

/// Core callback type - uses usize instead of function pointer to avoid circular dependencies.
///
/// **IMPORTANT**: This is NOT actually a usize at runtime - it's a function pointer that is
/// cast to usize for storage in the data model. When invoking the callback, this usize is
/// unsafely cast back to the actual function pointer type:
/// `extern "C" fn(&mut RefAny, &mut CallbackInfo) -> Update`
///
/// This design allows azul-core to store callbacks without depending on azul-layout's CallbackInfo
/// type. The actual function pointer type is defined in azul-layout as `CallbackType`.
pub type CoreCallbackType = usize;

/// Stores a callback as usize (actually a function pointer cast to usize)
///
/// **IMPORTANT**: The `cb` field stores a function pointer disguised as usize to avoid
/// circular dependencies between azul-core and azul-layout. When creating a CoreCallback,
/// you can directly assign a function pointer - Rust will implicitly cast it to usize.
/// When invoking, the usize must be unsafely cast back to the function pointer type.
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
///
/// **IMPORTANT**: The `callback` field contains a CoreCallback whose `cb` field is actually
/// a function pointer stored as usize. You can directly assign function pointers when creating
/// CoreCallbackData - Rust will implicitly cast them. Example:
/// ```ignore
/// CoreCallbackData {
///     event: EventFilter::Hover(HoverEventFilter::MouseDown),
///     callback: CoreCallback { cb: my_callback_function }, // function pointer auto-casts to usize
///     data: RefAny::new(my_data),
/// }
/// ```
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
