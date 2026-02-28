#![allow(dead_code)]

#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::{alloc::Layout, boxed::Box, collections::BTreeMap, sync::Arc, vec::Vec};
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
    system::SystemStyle,
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
    refany::{OptionRefAny, RefAny},
    resources::{
        DpiScaleFactor, FontInstanceKey, IdNamespace, ImageCache, ImageMask, ImageRef,
        RendererResources,
    },
    styled_dom::{
        NodeHierarchyItemId, NodeHierarchyItemVec, OptionStyledDom, StyledDom, StyledNode,
        StyledNodeVec,
    },
    task::{
        Duration as AzDuration, GetSystemTimeCallback, Instant as AzInstant, Instant,
        TerminateTimer, ThreadId, ThreadReceiver, ThreadSendMsg, TimerId,
    },
    window::{
        AzStringPair, KeyboardState, MouseState, OptionChar, RawWindowHandle, UpdateFocusWarning,
        WindowFlags, WindowSize, WindowTheme,
    },
    FastBTreeSet, FastHashMap,
};

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
pub type LayoutCallbackType = extern "C" fn(RefAny, LayoutCallbackInfo) -> crate::dom::Dom;

extern "C" fn default_layout_callback(_: RefAny, _: LayoutCallbackInfo) -> crate::dom::Dom {
    crate::dom::Dom::create_body()
}

/// Wrapper around the layout callback
///
/// For FFI languages (Python, Java, etc.), the RefAny contains both:
/// - The user's application data
/// - The callback function object from the foreign language
///
/// The trampoline function (stored in `cb`) knows how to extract both
/// from the RefAny and invoke the foreign callback with the user data.
#[repr(C)]
pub struct LayoutCallback {
    pub cb: LayoutCallbackType,
    /// For FFI: stores the foreign callable (e.g., PyFunction)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

impl_callback!(LayoutCallback, LayoutCallbackType);

impl LayoutCallback {
    pub fn create<I: Into<Self>>(cb: I) -> Self {
        cb.into()
    }
}

impl Default for LayoutCallback {
    fn default() -> Self {
        Self {
            cb: default_layout_callback,
            ctx: OptionRefAny::None,
        }
    }
}

// -- virtualized view callback

pub type VirtualizedViewCallbackType = extern "C" fn(RefAny, VirtualizedViewCallbackInfo) -> VirtualizedViewCallbackReturn;

/// Callback that, given a rectangle area on the screen, returns the DOM
/// appropriate for that bounds (useful for infinite lists)
#[repr(C)]
pub struct VirtualizedViewCallback {
    pub cb: VirtualizedViewCallbackType,
    /// For FFI: stores the foreign callable (e.g., PyFunction)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}
impl_callback!(VirtualizedViewCallback, VirtualizedViewCallbackType);

impl VirtualizedViewCallback {
    pub fn create(cb: VirtualizedViewCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

/// Reason why a VirtualizedView callback is being invoked.
///
/// This helps the callback optimize its behavior based on why it's being called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, u8)]
pub enum VirtualizedViewCallbackReason {
    /// Initial render - first time the VirtualizedView appears
    InitialRender,
    /// Parent DOM was recreated (cache invalidated)
    DomRecreated,
    /// Window/VirtualizedView bounds expanded beyond current scroll_size
    BoundsExpanded,
    /// Scroll position is near an edge (within 200px threshold)
    EdgeScrolled(EdgeType),
    /// Scroll position extends beyond current scroll_size
    ScrollBeyondContent,
}

/// Which edge triggered a scroll-based re-invocation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum EdgeType {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug)]
#[repr(C)]
pub struct VirtualizedViewCallbackInfo {
    pub reason: VirtualizedViewCallbackReason,
    pub system_fonts: *const FcFontCache,
    pub image_cache: *const ImageCache,
    pub window_theme: WindowTheme,
    pub bounds: HidpiAdjustedBounds,
    pub scroll_size: LogicalSize,
    pub scroll_offset: LogicalPosition,
    pub virtual_scroll_size: LogicalSize,
    pub virtual_scroll_offset: LogicalPosition,
    /// Pointer to the callable (OptionRefAny) for FFI language bindings (Python, etc.)
    /// Set by the caller before invoking the callback. Native Rust callbacks have this as null.
    callable_ptr: *const OptionRefAny,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut c_void,
}

impl Clone for VirtualizedViewCallbackInfo {
    fn clone(&self) -> Self {
        Self {
            reason: self.reason,
            system_fonts: self.system_fonts,
            image_cache: self.image_cache,
            window_theme: self.window_theme,
            bounds: self.bounds,
            scroll_size: self.scroll_size,
            scroll_offset: self.scroll_offset,
            virtual_scroll_size: self.virtual_scroll_size,
            virtual_scroll_offset: self.virtual_scroll_offset,
            callable_ptr: self.callable_ptr,
            _abi_mut: self._abi_mut,
        }
    }
}

impl VirtualizedViewCallbackInfo {
    pub fn new<'a>(
        reason: VirtualizedViewCallbackReason,
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
            reason,
            system_fonts: system_fonts as *const FcFontCache,
            image_cache: image_cache as *const ImageCache,
            window_theme,
            bounds,
            scroll_size,
            scroll_offset,
            virtual_scroll_size,
            virtual_scroll_offset,
            callable_ptr: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    /// Set the callable pointer for FFI language bindings
    pub fn set_callable_ptr(&mut self, callable: &OptionRefAny) {
        self.callable_ptr = callable as *const OptionRefAny;
    }

    /// Get the callable for FFI language bindings (Python, etc.)
    pub fn get_ctx(&self) -> OptionRefAny {
        if self.callable_ptr.is_null() {
            OptionRefAny::None
        } else {
            unsafe { (*self.callable_ptr).clone() }
        }
    }

    pub fn get_bounds(&self) -> HidpiAdjustedBounds {
        self.bounds
    }

    fn internal_get_system_fonts<'a>(&'a self) -> &'a FcFontCache {
        unsafe { &*self.system_fonts }
    }
    fn internal_get_image_cache<'a>(&'a self) -> &'a ImageCache {
        unsafe { &*self.image_cache }
    }
}

/// Return value for a VirtualizedView rendering callback.
///
/// Contains two size/offset pairs for lazy loading and virtualization:
///
/// - `scroll_size` / `scroll_offset`: Size and position of actually rendered content
/// - `virtual_scroll_size` / `virtual_scroll_offset`: Size for scrollbar representation
///
/// The callback is re-invoked on: initial render, parent DOM recreation, window expansion
/// beyond `scroll_size`, or scrolling near content edges (200px threshold).
///
/// Return `OptionStyledDom::None` to keep the current DOM and only update scroll bounds.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct VirtualizedViewCallbackReturn {
    /// The styled DOM with actual rendered content, or None to keep current DOM.
    ///
    /// - `OptionStyledDom::Some(dom)` - Replace current content with this new DOM
    /// - `OptionStyledDom::None` - Keep using the previous DOM, only update scroll bounds
    ///
    /// Returning `None` is an optimization when the callback determines that the
    /// current content is sufficient (e.g., already rendered ahead of scroll position).
    pub dom: OptionStyledDom,

    /// Size of the actual rendered content rectangle.
    ///
    /// This is the size of the content in the `dom` field (if Some). It may be smaller than
    /// `virtual_scroll_size` if only a subset of content is rendered (virtualization).
    ///
    /// **Example**: For a table showing rows 10-30, this might be 600px tall
    /// (20 rows x 30px each).
    pub scroll_size: LogicalSize,

    /// Offset of the actual rendered content within the virtual coordinate space.
    ///
    /// This positions the rendered content within the larger virtual space. For
    /// virtualized content, this will be non-zero to indicate where the rendered
    /// "window" starts.
    ///
    /// **Example**: For a table showing rows 10-30, this might be y=300
    /// (row 10 starts 300px from the top).
    pub scroll_offset: LogicalPosition,

    /// Size of the virtual content rectangle (for scrollbar sizing).
    ///
    /// This is the size the scrollbar will represent. It can be much larger than
    /// `scroll_size` to enable lazy loading and virtualization.
    ///
    /// **Example**: For a 1000-row table, this might be 30,000px tall
    /// (1000 rows x 30px each), even though only 20 rows are actually rendered.
    pub virtual_scroll_size: LogicalSize,

    /// Offset of the virtual content (usually zero).
    ///
    /// This is typically `(0, 0)` since the virtual space usually starts at the origin.
    /// Advanced use cases might use this for complex virtualization scenarios.
    pub virtual_scroll_offset: LogicalPosition,
}

impl Default for VirtualizedViewCallbackReturn {
    fn default() -> VirtualizedViewCallbackReturn {
        VirtualizedViewCallbackReturn {
            dom: OptionStyledDom::None,
            scroll_size: LogicalSize::zero(),
            scroll_offset: LogicalPosition::zero(),
            virtual_scroll_size: LogicalSize::zero(),
            virtual_scroll_offset: LogicalPosition::zero(),
        }
    }
}

impl VirtualizedViewCallbackReturn {
    /// Creates a new VirtualizedViewCallbackReturn with updated DOM content.
    ///
    /// Use this when the callback has rendered new content to display.
    ///
    /// # Arguments
    /// - `dom` - The new styled DOM to render
    /// - `scroll_size` - Size of the actual rendered content
    /// - `scroll_offset` - Position of rendered content in virtual space
    /// - `virtual_scroll_size` - Size for scrollbar representation
    /// - `virtual_scroll_offset` - Usually `LogicalPosition::zero()`
    pub fn with_dom(
        dom: StyledDom,
        scroll_size: LogicalSize,
        scroll_offset: LogicalPosition,
        virtual_scroll_size: LogicalSize,
        virtual_scroll_offset: LogicalPosition,
    ) -> Self {
        Self {
            dom: OptionStyledDom::Some(dom),
            scroll_size,
            scroll_offset,
            virtual_scroll_size,
            virtual_scroll_offset,
        }
    }

    /// Creates a return value that keeps the current DOM unchanged.
    ///
    /// Use this when the callback determines that the existing content
    /// is sufficient (e.g., already rendered ahead of scroll position).
    /// This is an optimization to avoid rebuilding the DOM unnecessarily.
    ///
    /// # Arguments
    /// - `scroll_size` - Size of the current rendered content
    /// - `scroll_offset` - Position of current content in virtual space
    /// - `virtual_scroll_size` - Size for scrollbar representation
    /// - `virtual_scroll_offset` - Usually `LogicalPosition::zero()`
    pub fn keep_current(
        scroll_size: LogicalSize,
        scroll_offset: LogicalPosition,
        virtual_scroll_size: LogicalSize,
        virtual_scroll_offset: LogicalPosition,
    ) -> Self {
        Self {
            dom: OptionStyledDom::None,
            scroll_size,
            scroll_offset,
            virtual_scroll_size,
            virtual_scroll_offset,
        }
    }

    /// DEPRECATED: Use `with_dom()` instead for new content, or `keep_current()` to maintain
    /// existing content.
    ///
    /// This method is kept for backward compatibility but will be removed in a future version.
    #[deprecated(
        since = "1.0.0",
        note = "Use `with_dom()` for new content or `keep_current()` for no update"
    )]
    pub fn new(
        dom: StyledDom,
        scroll_size: LogicalSize,
        scroll_offset: LogicalPosition,
        virtual_scroll_size: LogicalSize,
        virtual_scroll_offset: LogicalPosition,
    ) -> Self {
        Self::with_dom(
            dom,
            scroll_size,
            scroll_offset,
            virtual_scroll_size,
            virtual_scroll_offset,
        )
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

impl TimerCallbackReturn {
    /// Creates a new TimerCallbackReturn with the given update and terminate flags.
    pub fn create(should_update: Update, should_terminate: TerminateTimer) -> Self {
        Self {
            should_update,
            should_terminate,
        }
    }

    /// Timer continues running, no DOM update needed.
    pub fn continue_unchanged() -> Self {
        Self {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Continue,
        }
    }

    /// Timer continues running and DOM should be refreshed.
    pub fn continue_and_refresh_dom() -> Self {
        Self {
            should_update: Update::RefreshDom,
            should_terminate: TerminateTimer::Continue,
        }
    }

    /// Timer should stop, no DOM update needed.
    pub fn terminate_unchanged() -> Self {
        Self {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        }
    }

    /// Timer should stop and DOM should be refreshed.
    pub fn terminate_and_refresh_dom() -> Self {
        Self {
            should_update: Update::RefreshDom,
            should_terminate: TerminateTimer::Terminate,
        }
    }
}

impl Default for TimerCallbackReturn {
    fn default() -> Self {
        Self::continue_unchanged()
    }
}

/// Gives the `layout()` function access to the `RendererResources` and the `Window`
/// (for querying images and fonts, as well as width / height)
#[derive(Debug)]
#[repr(C)]
/// Reference data container for LayoutCallbackInfo (all read-only fields)
///
/// This struct consolidates all readonly references that layout callbacks need to query state.
/// By grouping these into a single struct, we reduce the number of parameters to
/// LayoutCallbackInfo::new() from 6 to 2, making the API more maintainable and easier to extend.
///
/// This is pure syntax sugar - the struct lives on the stack in the caller and is passed by
/// reference.
pub struct LayoutCallbackInfoRefData<'a> {
    /// Allows the layout() function to reference image IDs
    pub image_cache: &'a ImageCache,
    /// OpenGL context so that the layout() function can render textures
    pub gl_context: &'a OptionGlContextPtr,
    /// Reference to the system font cache
    pub system_fonts: &'a FcFontCache,
    /// Platform-specific system style (colors, spacing, etc.)
    /// Used for CSD rendering and menu windows.
    pub system_style: Arc<SystemStyle>,
}

#[repr(C)]
pub struct LayoutCallbackInfo {
    /// Single reference to all readonly reference data
    /// This consolidates 4 individual parameters into 1, improving API ergonomics
    ref_data: *const LayoutCallbackInfoRefData<'static>,
    /// Window size (so that apps can return a different UI depending on
    /// the window size - mobile / desktop view). Should be later removed
    /// in favor of "resize" handlers and @media queries.
    pub window_size: WindowSize,
    /// Registers whether the UI is dependent on the window theme
    pub theme: WindowTheme,
    /// Pointer to the callable (OptionRefAny) for FFI language bindings (Python, etc.)
    /// Set by the caller before invoking the callback. Native Rust callbacks have this as null.
    callable_ptr: *const OptionRefAny,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut core::ffi::c_void,
}

impl Clone for LayoutCallbackInfo {
    fn clone(&self) -> Self {
        Self {
            ref_data: self.ref_data,
            window_size: self.window_size,
            theme: self.theme,
            callable_ptr: self.callable_ptr,
            _abi_mut: self._abi_mut,
        }
    }
}

impl core::fmt::Debug for LayoutCallbackInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LayoutCallbackInfo")
            .field("window_size", &self.window_size)
            .field("theme", &self.theme)
            .finish_non_exhaustive()
    }
}

impl LayoutCallbackInfo {
    pub fn new<'a>(
        ref_data: &'a LayoutCallbackInfoRefData<'a>,
        window_size: WindowSize,
        theme: WindowTheme,
    ) -> Self {
        Self {
            // SAFETY: We cast away the lifetime 'a to 'static because LayoutCallbackInfo
            // only lives for the duration of the callback, which is shorter than 'a
            ref_data: unsafe { core::mem::transmute(ref_data) },
            window_size,
            theme,
            callable_ptr: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    /// Set the callable pointer for FFI language bindings
    pub fn set_callable_ptr(&mut self, callable: &OptionRefAny) {
        self.callable_ptr = callable as *const OptionRefAny;
    }

    /// Get the callable for FFI language bindings (Python, etc.)
    pub fn get_ctx(&self) -> OptionRefAny {
        if self.callable_ptr.is_null() {
            OptionRefAny::None
        } else {
            unsafe { (*self.callable_ptr).clone() }
        }
    }

    /// Get a clone of the system style Arc
    pub fn get_system_style(&self) -> Arc<SystemStyle> {
        unsafe { (*self.ref_data).system_style.clone() }
    }

    fn internal_get_image_cache<'a>(&'a self) -> &'a ImageCache {
        unsafe { (*self.ref_data).image_cache }
    }
    fn internal_get_system_fonts<'a>(&'a self) -> &'a FcFontCache {
        unsafe { (*self.ref_data).system_fonts }
    }
    fn internal_get_gl_context<'a>(&'a self) -> &'a OptionGlContextPtr {
        unsafe { (*self.ref_data).gl_context }
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

    // Responsive layout helper methods
    /// Returns true if the window width is less than the given pixel value
    pub fn window_width_less_than(&self, px: f32) -> bool {
        self.window_size.dimensions.width < px
    }

    /// Returns true if the window width is greater than the given pixel value
    pub fn window_width_greater_than(&self, px: f32) -> bool {
        self.window_size.dimensions.width > px
    }

    /// Returns true if the window width is between min and max (inclusive)
    pub fn window_width_between(&self, min_px: f32, max_px: f32) -> bool {
        let width = self.window_size.dimensions.width;
        width >= min_px && width <= max_px
    }

    /// Returns true if the window height is less than the given pixel value
    pub fn window_height_less_than(&self, px: f32) -> bool {
        self.window_size.dimensions.height < px
    }

    /// Returns true if the window height is greater than the given pixel value
    pub fn window_height_greater_than(&self, px: f32) -> bool {
        self.window_size.dimensions.height > px
    }

    /// Returns true if the window height is between min and max (inclusive)
    pub fn window_height_between(&self, min_px: f32, max_px: f32) -> bool {
        let height = self.window_size.dimensions.height;
        height >= min_px && height <= max_px
    }

    /// Returns the current window width in pixels
    pub fn get_window_width(&self) -> f32 {
        self.window_size.dimensions.width
    }

    /// Returns the current window height in pixels
    pub fn get_window_height(&self) -> f32 {
        self.window_size.dimensions.height
    }

    /// Returns the current window DPI factor
    pub fn get_dpi_factor(&self) -> f32 {
        self.window_size.dpi as f32
    }
}

/// Information about the bounds of a laid-out div rectangle.
///
/// Necessary when invoking `VirtualizedViewCallbacks` and `RenderImageCallbacks`, so
/// that they can change what their content is based on their size.
#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct HidpiAdjustedBounds {
    pub logical_size: LogicalSize,
    pub hidpi_factor: DpiScaleFactor,
}

impl HidpiAdjustedBounds {
    #[inline(always)]
    pub fn from_bounds(bounds: LayoutSize, hidpi_factor: DpiScaleFactor) -> Self {
        let logical_size = LogicalSize::new(bounds.width as f32, bounds.height as f32);
        Self {
            logical_size,
            hidpi_factor,
        }
    }

    pub fn get_physical_size(&self) -> PhysicalSize<u32> {
        self.get_logical_size()
            .to_physical(self.get_hidpi_factor().inner.get())
    }

    pub fn get_logical_size(&self) -> LogicalSize {
        self.logical_size
    }

    pub fn get_hidpi_factor(&self) -> DpiScaleFactor {
        self.hidpi_factor.clone()
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

// -- normal callback

// core callback types (usize-based placeholders)
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
/// `extern "C" fn(RefAny, CallbackInfo) -> Update`
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CoreCallback {
    pub cb: CoreCallbackType,
    /// For FFI: stores the foreign callable (e.g., PyFunction)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

/// Allow creating CoreCallback from a raw function pointer (as usize)
/// Sets callable to None (for native Rust/C usage)
impl From<CoreCallbackType> for CoreCallback {
    fn from(cb: CoreCallbackType) -> Self {
        CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

impl_option!(
    CoreCallback,
    OptionCoreCallback,
    [Debug, Eq, Clone, PartialEq, PartialOrd, Ord, Hash]
);

/// Data associated with a callback (event filter, callback, and user data)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CoreCallbackData {
    pub event: EventFilter,
    pub callback: CoreCallback,
    pub refany: RefAny,
}

impl_option!(
    CoreCallbackData,
    OptionCoreCallbackData,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(CoreCallbackData, CoreCallbackDataVec, CoreCallbackDataVecDestructor, CoreCallbackDataVecDestructorType, CoreCallbackDataVecSlice, OptionCoreCallbackData);
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

// -- image rendering callback

/// Image rendering callback type - uses usize instead of function pointer
pub type CoreRenderImageCallbackType = usize;

/// Callback that returns a rendered OpenGL texture (usize placeholder)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CoreRenderImageCallback {
    pub cb: CoreRenderImageCallbackType,
    /// For FFI: stores the foreign callable (e.g., PyFunction)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

/// Allow creating CoreRenderImageCallback from a raw function pointer (as usize)
/// Sets callable to None (for native Rust/C usage)
impl From<CoreRenderImageCallbackType> for CoreRenderImageCallback {
    fn from(cb: CoreRenderImageCallbackType) -> Self {
        CoreRenderImageCallback {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

/// Image callback with associated data
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CoreImageCallback {
    pub refany: RefAny,
    pub callback: CoreRenderImageCallback,
}

impl_option!(
    CoreImageCallback,
    OptionCoreImageCallback,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
