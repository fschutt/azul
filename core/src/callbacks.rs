//! Callback types for the Azul UI framework.
//!
//! This module defines the callback infrastructure used by the event system,
//! layout engine, and virtual view rendering. Key design patterns:
//!
//! - **Core vs Layout callback split**: `CoreCallbackType` and
//!   `CoreRenderImageCallbackType` store function pointers as `usize` to avoid
//!   circular dependencies between `azul-core` and `azul-layout`. The actual
//!   function pointer types are defined in `azul-layout` and transmuted at
//!   invocation time.
//!
//! - **FFI callable pattern**: Callback structs carry an optional
//!   `ctx: OptionRefAny` field that holds a foreign callable (e.g. a Python
//!   function object). The `extern "C"` trampoline stored in `cb` extracts
//!   both the user data and the foreign callable from `RefAny` and dispatches
//!   the call. Native Rust code sets `ctx` to `None`.
//!
//! - **Info structs**: `LayoutCallbackInfo`, `VirtualViewCallbackInfo`, and
//!   the layout-side `CallbackInfo` provide read-only access to framework
//!   resources (fonts, images, GL context, window size) during callback
//!   invocation.

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
use rust_fontconfig::{FcFontCache, OwnedFontSource};

use crate::{
    dom::{Dom, DomId, DomNodeId, EventFilter, OptionDom},
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
        NodeHierarchyItemId, NodeHierarchyItemVec, StyledNode,
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
    FastBTreeSet, OrderedMap,
};

/// Specifies if the screen should be updated after the callback function has returned
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Update {
    /// The screen does not need to redraw after the callback has been called
    DoNothing,
    /// After the callback is called, the screen needs to redraw (`layout()` function being called
    /// again)
    RefreshDom,
    /// The layout has to be re-calculated for all windows
    RefreshDomAllWindows,
}

impl Update {
    pub fn max_self(&mut self, other: Self) {
        if (*self == Self::DoNothing && other != Self::DoNothing)
            || (*self == Self::RefreshDom && other == Self::RefreshDomAllWindows)
        {
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
/// The memory management across the callback boundary is handled by
/// the caller (see `LayoutCallback` and `LayoutCallbackInfo`).
pub type LayoutCallbackType = extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom;

extern "C" fn default_layout_callback(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
}

/// Wrapper around the layout callback
///
/// For FFI languages (Python, Java, etc.), the `RefAny` contains both:
/// - The user's application data
/// - The callback function object from the foreign language
///
/// The trampoline function (stored in `cb`) knows how to extract both
/// from the `RefAny` and invoke the foreign callback with the user data.
#[repr(C)]
pub struct LayoutCallback {
    pub cb: LayoutCallbackType,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

impl_callback!(LayoutCallback, LayoutCallbackType);

impl LayoutCallback {
    pub fn create<I: Into<Self>>(cb: I) -> Self {
        cb.into()
    }
}

// Host-invoker plumbing for managed-FFI bindings (Lua, Ruby, Perl, …):
// expands to a static `az_layout_callback_thunk` (the `cb` we hand to the
// framework when the host calls `LayoutCallback::create_from_host_handle`),
// an `AzLayoutCallback_createFromHostHandle` C-ABI export, plus the
// `AzApp_setLayoutCallbackInvoker` setter the host calls once at module
// load. See `crate::host_invoker` for the design.
crate::impl_managed_callback! {
    wrapper:        LayoutCallback,
    info_ty:        LayoutCallbackInfo,
    return_ty:      Dom,
    default_ret:    Dom::create_body(),
    invoker_static: LAYOUT_CALLBACK_INVOKER,
    invoker_ty:     AzLayoutCallbackInvoker,
    thunk_fn:       az_layout_callback_thunk,
    setter_fn:      AzApp_setLayoutCallbackInvoker,
    from_handle_fn: AzLayoutCallback_createFromHostHandle,
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

pub type VirtualViewCallbackType = extern "C" fn(RefAny, VirtualViewCallbackInfo) -> VirtualViewReturn;

/// Callback that, given a rectangle area on the screen, returns the DOM
/// appropriate for that bounds (useful for infinite lists)
#[repr(C)]
pub struct VirtualViewCallback {
    pub cb: VirtualViewCallbackType,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}
impl_callback!(VirtualViewCallback, VirtualViewCallbackType);

// Host-invoker plumbing for VirtualViewCallback. See `crate::host_invoker`.
crate::impl_managed_callback! {
    wrapper:        VirtualViewCallback,
    info_ty:        VirtualViewCallbackInfo,
    return_ty:      VirtualViewReturn,
    default_ret:    VirtualViewReturn::default(),
    invoker_static: VIRTUAL_VIEW_CALLBACK_INVOKER,
    invoker_ty:     AzVirtualViewCallbackInvoker,
    thunk_fn:       az_virtual_view_callback_thunk,
    setter_fn:      AzApp_setVirtualViewCallbackInvoker,
    from_handle_fn: AzVirtualViewCallback_createFromHostHandle,
}

impl VirtualViewCallback {
    pub fn create(cb: VirtualViewCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

/// Reason why a `VirtualView` callback is being invoked.
///
/// This helps the callback optimize its behavior based on why it's being called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, u8)]
pub enum VirtualViewCallbackReason {
    /// Initial render - first time the `VirtualView` appears
    InitialRender,
    /// Parent DOM was recreated (cache invalidated)
    DomRecreated,
    /// Window/VirtualView bounds expanded beyond current `scroll_size`
    BoundsExpanded,
    /// Scroll position is near an edge (within `EDGE_THRESHOLD`, currently 200px)
    EdgeScrolled(EdgeType),
    /// Scroll position extends beyond current `scroll_size`
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
pub struct VirtualViewCallbackInfo {
    pub reason: VirtualViewCallbackReason,
    pub system_fonts: *const FcFontCache,
    pub image_cache: *const ImageCache,
    pub window_theme: WindowTheme,
    pub bounds: HidpiAdjustedBounds,
    pub scroll_size: LogicalSize,
    pub scroll_offset: LogicalPosition,
    pub virtual_scroll_size: LogicalSize,
    pub virtual_scroll_offset: LogicalPosition,
    /// Pointer to the callable (`OptionRefAny`) for FFI language bindings (Python, etc.)
    /// Set by the caller before invoking the callback. Native Rust callbacks have this as null.
    callable_ptr: *const OptionRefAny,
    /// Headless DOM measurement hook (see [`Self::measure_dom`]): a
    /// layout-crate trampoline (a [`MeasureDomFn`] stored as an opaque
    /// pointer, null = no hook) + its `LayoutWindow` context, injected at
    /// invoke time. Null on paths that cannot measure (then `measure_dom`
    /// returns zero).
    measure_dom_fn: *const c_void,
    measure_dom_ctx: *mut c_void,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut c_void,
}

/// Trampoline signature for [`VirtualViewCallbackInfo::measure_dom`]:
/// `(layout_window_ctx, dom, available) -> content extent`. The `Dom` is
/// passed by pointer and CONSUMED (moved out) by the trampoline.
pub type MeasureDomFn = extern "C" fn(*mut c_void, *mut Dom, LogicalSize) -> LogicalSize;

impl Clone for VirtualViewCallbackInfo {
    #[allow(clippy::used_underscore_binding)] // intentional `_`-prefix (FFI/api.json pub field, or cfg-gated binding); access is deliberate
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
            measure_dom_fn: self.measure_dom_fn,
            measure_dom_ctx: self.measure_dom_ctx,
            _abi_mut: self._abi_mut,
        }
    }
}

impl VirtualViewCallbackInfo {
    #[must_use] pub const fn new<'a>(
        reason: VirtualViewCallbackReason,
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
            system_fonts: core::ptr::from_ref::<FcFontCache>(system_fonts),
            image_cache: core::ptr::from_ref::<ImageCache>(image_cache),
            window_theme,
            bounds,
            scroll_size,
            scroll_offset,
            virtual_scroll_size,
            virtual_scroll_offset,
            callable_ptr: core::ptr::null(),
            measure_dom_fn: core::ptr::null(),
            measure_dom_ctx: core::ptr::null_mut(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    /// Set the callable pointer for FFI language bindings
    pub const fn set_callable_ptr(&mut self, callable: &OptionRefAny) {
        self.callable_ptr = core::ptr::from_ref::<OptionRefAny>(callable);
    }

    /// Inject the headless-measure trampoline (called by the layout crate
    /// right before the user callback is invoked).
    pub fn set_measure_dom_fn(&mut self, f: MeasureDomFn, ctx: *mut c_void) {
        self.measure_dom_fn = f as *const c_void;
        self.measure_dom_ctx = ctx;
    }

    /// Measure a DOM headlessly: style + lay it out against `available`
    /// constraints using the host window's fonts and system style, without
    /// touching the live layout. Returns the union of all node bounds.
    ///
    /// Use a very tall `available.height` (e.g. `1_000_000.0`) to obtain a
    /// DOM's natural height at a fixed width - the building block for
    /// virtual-scroll sizing: measure one (or a few) item template(s), then
    /// `virtual_scroll_size.height = item_height * item_count` and render
    /// only the visible window of items. Each call is a full cold layout
    /// pass, so cache measured sizes per item template.
    ///
    /// Returns `LogicalSize::zero()` when no measure hook was injected.
    #[must_use] pub fn measure_dom(&self, dom: Dom, available: LogicalSize) -> LogicalSize {
        if self.measure_dom_fn.is_null() {
            return LogicalSize::zero();
        }
        // SAFETY: measure_dom_fn is only ever set via set_measure_dom_fn,
        // which stores a valid MeasureDomFn.
        let f: MeasureDomFn = unsafe { core::mem::transmute(self.measure_dom_fn) };
        let mut dom = core::mem::ManuallyDrop::new(dom);
        f(self.measure_dom_ctx, core::ptr::from_mut::<Dom>(&mut dom), available)
    }

    /// Get the callable for FFI language bindings (Python, etc.)
    #[must_use] pub fn get_ctx(&self) -> OptionRefAny {
        if self.callable_ptr.is_null() {
            OptionRefAny::None
        } else {
            unsafe { (*self.callable_ptr).clone() }
        }
    }

    #[must_use] pub const fn get_bounds(&self) -> HidpiAdjustedBounds {
        self.bounds
    }

    const fn internal_get_system_fonts(&self) -> &FcFontCache {
        unsafe { &*self.system_fonts }
    }
    const fn internal_get_image_cache(&self) -> &ImageCache {
        unsafe { &*self.image_cache }
    }
}

/// Return value for a `VirtualView` rendering callback.
///
/// Contains two size/offset pairs for lazy loading and virtualization:
///
/// - `scroll_size` / `scroll_offset`: Size and position of actually rendered content
/// - `virtual_scroll_size` / `virtual_scroll_offset`: Size for scrollbar representation
///
/// The callback is re-invoked on: initial render, parent DOM recreation, window expansion
/// beyond `scroll_size`, or scrolling near content edges (`EDGE_THRESHOLD`, currently 200px).
///
/// Return `OptionDom::None` to keep the current DOM and only update scroll bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct VirtualViewReturn {
    /// The DOM with actual rendered content, or None to keep current DOM.
    ///
    /// - `OptionDom::Some(dom)` - Replace current content with this new DOM
    /// - `OptionDom::None` - Keep using the previous DOM, only update scroll bounds
    ///
    /// Returning `None` is an optimization when the callback determines that the
    /// current content is sufficient (e.g., already rendered ahead of scroll position).
    pub dom: OptionDom,

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

impl Default for VirtualViewReturn {
    fn default() -> Self {
        Self {
            dom: OptionDom::None,
            scroll_size: LogicalSize::zero(),
            scroll_offset: LogicalPosition::zero(),
            virtual_scroll_size: LogicalSize::zero(),
            virtual_scroll_offset: LogicalPosition::zero(),
        }
    }
}

impl VirtualViewReturn {
    /// Creates a new `VirtualViewReturn` with updated DOM content.
    ///
    /// Use this when the callback has rendered new content to display.
    ///
    /// # Arguments
    /// - `dom` - The new DOM to render
    /// - `scroll_size` - Size of the actual rendered content
    /// - `scroll_offset` - Position of rendered content in virtual space
    /// - `virtual_scroll_size` - Size for scrollbar representation
    /// - `virtual_scroll_offset` - Usually `LogicalPosition::zero()`
    #[must_use] pub const fn with_dom(
        dom: Dom,
        scroll_size: LogicalSize,
        scroll_offset: LogicalPosition,
        virtual_scroll_size: LogicalSize,
        virtual_scroll_offset: LogicalPosition,
    ) -> Self {
        Self {
            dom: OptionDom::Some(dom),
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
    #[must_use] pub const fn keep_current(
        scroll_size: LogicalSize,
        scroll_offset: LogicalPosition,
        virtual_scroll_size: LogicalSize,
        virtual_scroll_offset: LogicalPosition,
    ) -> Self {
        Self {
            dom: OptionDom::None,
            scroll_size,
            scroll_offset,
            virtual_scroll_size,
            virtual_scroll_offset,
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

impl TimerCallbackReturn {
    /// Creates a new `TimerCallbackReturn` with the given update and terminate flags.
    #[must_use] pub const fn create(should_update: Update, should_terminate: TerminateTimer) -> Self {
        Self {
            should_update,
            should_terminate,
        }
    }

    /// Timer continues running, no DOM update needed.
    #[must_use] pub const fn continue_unchanged() -> Self {
        Self {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Continue,
        }
    }

    /// Timer continues running and DOM should be refreshed.
    #[must_use] pub const fn continue_and_refresh_dom() -> Self {
        Self {
            should_update: Update::RefreshDom,
            should_terminate: TerminateTimer::Continue,
        }
    }

    /// Timer should stop, no DOM update needed.
    #[must_use] pub const fn terminate_unchanged() -> Self {
        Self {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        }
    }

    /// Timer should stop and DOM should be refreshed.
    #[must_use] pub const fn terminate_and_refresh_dom() -> Self {
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
///
#[derive(Debug)]
#[repr(C)]
/// Reference data container for `LayoutCallbackInfo` (all read-only fields)
///
/// This struct consolidates all readonly references that layout callbacks need to query state.
/// By grouping these into a single struct, we reduce the number of parameters to
/// `LayoutCallbackInfo::new()` from 6 to 2, making the API more maintainable and easier to extend.
///
/// This is pure syntax sugar - the struct lives on the stack in the caller and is passed by
/// reference.
pub struct LayoutCallbackInfoRefData<'a> {
    /// Allows the `layout()` function to reference image IDs
    pub image_cache: &'a ImageCache,
    /// OpenGL context so that the `layout()` function can render textures
    pub gl_context: &'a OptionGlContextPtr,
    /// Reference to the system font cache
    pub system_fonts: &'a FcFontCache,
    /// Platform-specific system style (colors, spacing, etc.)
    /// Used for CSD rendering and menu windows.
    pub system_style: Arc<SystemStyle>,
    /// Active route match (if routing is configured).
    /// Contains the matched pattern and extracted parameters.
    pub active_route: Option<&'a crate::resources::RouteMatch>,
}

/// What triggered the current `layout()` invocation.
///
/// The framework re-invokes the layout callback for any change that may
/// produce a structurally different DOM (resize across a CSS breakpoint,
/// theme toggle, route switch, callback returning `Update::RefreshDom`).
/// `LayoutCallbackInfo::relayout_reason()` exposes which trigger this
/// particular call corresponds to so the callback can branch - for
/// example, skip expensive analytics on `Resize` calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum RelayoutReason {
    /// First layout call for this window.
    #[default]
    Initial,
    /// A user callback returned `Update::RefreshDom`.
    RefreshDom,
    /// Window size changed across a CSS breakpoint or DPI scale change.
    /// The callback can branch on `info.window_width_*` to emit a
    /// different tree (e.g. hamburger menu vs sidebar).
    Resize,
    /// System theme changed (light/dark).
    ThemeChange,
    /// `CallbackInfo::switch_route` or `set_route_param` produced a new
    /// route match. The callback should branch on
    /// `info.get_active_route()`.
    RouteChange,
    /// Catch-all for relayouts that don't fit one of the above categories.
    Other,
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
    /// What triggered this `layout()` call. Read via `relayout_reason()`.
    pub relayout_reason: RelayoutReason,
    /// Pointer to the callable (`OptionRefAny`) for FFI language bindings (Python, etc.)
    /// Set by the caller before invoking the callback. Native Rust callbacks have this as null.
    callable_ptr: *const OptionRefAny,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut c_void,
}

impl Clone for LayoutCallbackInfo {
    #[allow(clippy::used_underscore_binding)] // intentional `_`-prefix (FFI/api.json pub field, or cfg-gated binding); access is deliberate
    fn clone(&self) -> Self {
        Self {
            ref_data: self.ref_data,
            window_size: self.window_size,
            theme: self.theme,
            relayout_reason: self.relayout_reason,
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
            .field("relayout_reason", &self.relayout_reason)
            .finish_non_exhaustive()
    }
}

impl LayoutCallbackInfo {
    #[must_use] pub const fn new<'a>(
        ref_data: &'a LayoutCallbackInfoRefData<'a>,
        window_size: WindowSize,
        theme: WindowTheme,
    ) -> Self {
        Self::new_with_reason(ref_data, window_size, theme, RelayoutReason::Initial)
    }

    // the `as *const ...<'static>` is a deliberate 'a -> 'static lifetime launder
    // on the raw pointer (see SAFETY note below), not a redundant cast.
    #[allow(clippy::unnecessary_cast)]
    #[must_use] pub const fn new_with_reason<'a>(
        ref_data: &'a LayoutCallbackInfoRefData<'a>,
        window_size: WindowSize,
        theme: WindowTheme,
        relayout_reason: RelayoutReason,
    ) -> Self {
        Self {
            // SAFETY: We cast away the lifetime 'a to 'static because LayoutCallbackInfo
            // only lives for the duration of the callback, which is shorter than 'a
            ref_data: core::ptr::from_ref::<LayoutCallbackInfoRefData<'a>>(ref_data)
                as *const LayoutCallbackInfoRefData<'static>,
            window_size,
            theme,
            relayout_reason,
            callable_ptr: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    /// Returns what triggered the current `layout()` invocation.
    #[must_use] pub const fn relayout_reason(&self) -> RelayoutReason {
        self.relayout_reason
    }

    /// Set the callable pointer for FFI language bindings
    pub const fn set_callable_ptr(&mut self, callable: &OptionRefAny) {
        self.callable_ptr = core::ptr::from_ref::<OptionRefAny>(callable);
    }

    /// Get the callable for FFI language bindings (Python, etc.)
    #[must_use] pub fn get_ctx(&self) -> OptionRefAny {
        if self.callable_ptr.is_null() {
            OptionRefAny::None
        } else {
            unsafe { (*self.callable_ptr).clone() }
        }
    }

    /// Get a clone of the system style Arc
    #[must_use] pub fn get_system_style(&self) -> Arc<SystemStyle> {
        unsafe { (*self.ref_data).system_style.clone() }
    }

    const fn internal_get_image_cache(&self) -> &ImageCache {
        unsafe { (*self.ref_data).image_cache }
    }
    const fn internal_get_system_fonts(&self) -> &FcFontCache {
        unsafe { (*self.ref_data).system_fonts }
    }
    const fn internal_get_gl_context(&self) -> &OptionGlContextPtr {
        unsafe { (*self.ref_data).gl_context }
    }

    #[must_use] pub fn get_gl_context(&self) -> OptionGlContextPtr {
        self.internal_get_gl_context().clone()
    }

    #[must_use] pub fn get_system_fonts(&self) -> Vec<AzStringPair> {
        let fc_cache = self.internal_get_system_fonts();

        fc_cache
            .list()
            .into_iter()
            .filter_map(|(pattern, font_id)| {
                let source = fc_cache.get_font_by_id(&font_id)?;
                match source {
                    OwnedFontSource::Memory(_) => None,
                    OwnedFontSource::Disk(d) => Some((pattern.name.as_ref()?.clone(), d.path)),
                }
            })
            .map(|(k, v)| AzStringPair {
                key: k.into(),
                value: v.into(),
            })
            .collect()
    }

    #[must_use] pub fn get_image(&self, image_id: &AzString) -> Option<ImageRef> {
        self.internal_get_image_cache()
            .get_css_image_id(image_id)
            .cloned()
    }

    /// Get the active route match (pattern + extracted parameters).
    ///
    /// Returns `None` if no routes are configured or no route is active.
    #[must_use] pub const fn get_active_route(&self) -> Option<&crate::resources::RouteMatch> {
        unsafe { (*self.ref_data).active_route }
    }

    /// Get a route parameter by key (e.g. `get_route_param("id")` for `/user/:id`).
    ///
    /// Returns `None` if no route is active or the parameter doesn't exist.
    #[must_use] pub fn get_route_param(&self, key: &str) -> Option<&AzString> {
        self.get_active_route()?.get_param(key)
    }

    // Responsive layout helper methods
    /// Returns true if the window width is less than the given pixel value
    #[must_use] pub fn window_width_less_than(&self, px: f32) -> bool {
        self.window_size.dimensions.width < px
    }

    /// Returns true if the window width is greater than the given pixel value
    #[must_use] pub fn window_width_greater_than(&self, px: f32) -> bool {
        self.window_size.dimensions.width > px
    }

    /// Returns true if the window width is between min and max (inclusive)
    #[must_use] pub fn window_width_between(&self, min_px: f32, max_px: f32) -> bool {
        let width = self.window_size.dimensions.width;
        width >= min_px && width <= max_px
    }

    /// Returns true if the window height is less than the given pixel value
    #[must_use] pub fn window_height_less_than(&self, px: f32) -> bool {
        self.window_size.dimensions.height < px
    }

    /// Returns true if the window height is greater than the given pixel value
    #[must_use] pub fn window_height_greater_than(&self, px: f32) -> bool {
        self.window_size.dimensions.height > px
    }

    /// Returns true if the window height is between min and max (inclusive)
    #[must_use] pub fn window_height_between(&self, min_px: f32, max_px: f32) -> bool {
        let height = self.window_size.dimensions.height;
        height >= min_px && height <= max_px
    }

    /// Returns the current window width in pixels
    #[must_use] pub const fn get_window_width(&self) -> f32 {
        self.window_size.dimensions.width
    }

    /// Returns the current window height in pixels
    #[must_use] pub const fn get_window_height(&self) -> f32 {
        self.window_size.dimensions.height
    }

    /// Returns the current window DPI scale factor (1.0 = 96 DPI, 2.0 = 192 DPI)
    #[allow(clippy::cast_precision_loss)] // bounded DPI/dimension/number conversion
    #[must_use] pub fn get_dpi_factor(&self) -> f32 {
        self.window_size.dpi as f32 / 96.0
    }
}

/// Information about the bounds of a laid-out div rectangle.
///
/// Necessary when invoking `VirtualViewCallbacks` and `RenderImageCallbacks`, so
/// that they can change what their content is based on their size.
#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct HidpiAdjustedBounds {
    pub logical_size: LogicalSize,
    pub hidpi_factor: DpiScaleFactor,
}

impl HidpiAdjustedBounds {
    #[inline]
    #[allow(clippy::cast_precision_loss)] // bounded DPI/dimension/number conversion
    #[must_use] pub const fn from_bounds(bounds: LayoutSize, hidpi_factor: DpiScaleFactor) -> Self {
        let logical_size = LogicalSize::new(bounds.width as f32, bounds.height as f32);
        Self {
            logical_size,
            hidpi_factor,
        }
    }

    #[must_use] pub fn get_physical_size(&self) -> PhysicalSize<u32> {
        self.get_logical_size()
            .to_physical(self.get_hidpi_factor().inner.get())
    }

    #[must_use] pub const fn get_logical_size(&self) -> LogicalSize {
        self.logical_size
    }

    #[must_use] pub const fn get_hidpi_factor(&self) -> DpiScaleFactor {
        self.hidpi_factor
    }
}

/// Defines the `focus_targeted` node ID for the next frame
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
//
// Naming convention: "Core" prefix indicates these are the low-level types

/// Core callback type - uses usize instead of function pointer to avoid circular dependencies.
///
/// **IMPORTANT**: This is NOT actually a usize at runtime - it's a function pointer that is
/// cast to usize for storage in the data model. When invoking the callback, this usize is
/// unsafely cast back to the actual function pointer type:
/// `extern "C" fn(RefAny, CallbackInfo) -> Update`
///
/// This design allows azul-core to store callbacks without depending on azul-layout's `CallbackInfo`
/// type. The actual function pointer type is defined in azul-layout as `CallbackType`.
pub type CoreCallbackType = usize;

/// Stores a callback as usize (actually a function pointer cast to usize)
///
/// **IMPORTANT**: The `cb` field stores a function pointer disguised as usize to avoid
/// circular dependencies between azul-core and azul-layout. When creating a `CoreCallback`,
/// you can directly assign a function pointer - Rust will implicitly cast it to usize.
/// When invoking, the usize must be unsafely cast back to the function pointer type.
///
/// Must return an `Update` that denotes if the screen should be redrawn.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CoreCallback {
    pub cb: CoreCallbackType,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

/// Allow creating `CoreCallback` from a raw function pointer (as usize)
/// Sets callable to None (for native Rust/C usage)
impl From<CoreCallbackType> for CoreCallback {
    fn from(cb: CoreCallbackType) -> Self {
        Self {
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
    #[must_use] pub fn as_container(&self) -> NodeDataContainerRef<'_, CoreCallbackData> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
    #[inline]
    pub fn as_container_mut(&mut self) -> NodeDataContainerRefMut<'_, CoreCallbackData> {
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
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

/// Allow creating `CoreRenderImageCallback` from a raw function pointer (as usize)
/// Sets callable to None (for native Rust/C usage)
impl From<CoreRenderImageCallbackType> for CoreRenderImageCallback {
    fn from(cb: CoreRenderImageCallbackType) -> Self {
        Self {
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

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::unusual_byte_groupings
)]
mod autotest_generated {
    use alloc::string::String;

    use super::*;
    use crate::{
        events::HoverEventFilter,
        resources::{RawImageFormat, RouteMatch},
        window::StringPairVec,
    };

    // ---- helpers -----------------------------------------------------------

    fn s(v: &str) -> AzString {
        AzString::from(String::from(v))
    }

    fn win(width: f32, height: f32, dpi: u32) -> WindowSize {
        WindowSize {
            dimensions: LogicalSize::new(width, height),
            dpi,
            min_dimensions: None.into(),
            max_dimensions: None.into(),
        }
    }

    /// Owns everything a `LayoutCallbackInfoRefData` borrows, so that the raw
    /// pointer `LayoutCallbackInfo` launders to `'static` always points at
    /// live memory for the duration of a test.
    struct Fixture {
        fonts: FcFontCache,
        images: ImageCache,
        style: Arc<SystemStyle>,
        gl: OptionGlContextPtr,
        route: Option<RouteMatch>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                fonts: FcFontCache::default(),
                images: ImageCache::default(),
                style: Arc::new(SystemStyle::default()),
                gl: OptionGlContextPtr::None,
                route: None,
            }
        }

        fn with_route(route: RouteMatch) -> Self {
            let mut f = Self::new();
            f.route = Some(route);
            f
        }

        fn ref_data(&self) -> LayoutCallbackInfoRefData<'_> {
            LayoutCallbackInfoRefData {
                image_cache: &self.images,
                gl_context: &self.gl,
                system_fonts: &self.fonts,
                system_style: self.style.clone(),
                active_route: self.route.as_ref(),
            }
        }
    }

    /// `/user/:id` with a plain and a non-ASCII parameter.
    fn user_route() -> RouteMatch {
        RouteMatch {
            pattern: s("/user/:id"),
            params: StringPairVec::from_vec(Vec::from([
                AzStringPair {
                    key: s("id"),
                    value: s("42"),
                },
                AzStringPair {
                    key: s("\u{1F600}"),
                    value: s("emoji"),
                },
            ])),
        }
    }

    fn vv_info<'a>(
        fonts: &'a FcFontCache,
        images: &'a ImageCache,
        bounds: HidpiAdjustedBounds,
    ) -> VirtualViewCallbackInfo {
        VirtualViewCallbackInfo::new(
            VirtualViewCallbackReason::InitialRender,
            fonts,
            images,
            WindowTheme::LightMode,
            bounds,
            LogicalSize::new(100.0, 200.0),
            LogicalPosition::new(1.0, 2.0),
            LogicalSize::new(1000.0, 2000.0),
            LogicalPosition::new(3.0, 4.0),
        )
    }

    fn bounds_1x1() -> HidpiAdjustedBounds {
        HidpiAdjustedBounds::from_bounds(LayoutSize::new(1, 1), DpiScaleFactor::new(1.0))
    }

    // ---- Update::max_self --------------------------------------------------

    const ALL_UPDATES: [Update; 3] = [
        Update::DoNothing,
        Update::RefreshDom,
        Update::RefreshDomAllWindows,
    ];

    /// `max_self` must be exactly the `Ord`-max of the lattice, for every one
    /// of the 3x3 combinations (this is the whole contract, so check it
    /// exhaustively rather than sampling).
    #[test]
    fn update_max_self_is_exhaustively_ord_max() {
        for a in ALL_UPDATES {
            for b in ALL_UPDATES {
                let mut got = a;
                got.max_self(b);
                assert_eq!(
                    got,
                    core::cmp::max(a, b),
                    "max_self({a:?}, {b:?}) disagrees with Ord::max"
                );
            }
        }
    }

    #[test]
    fn update_max_self_is_idempotent_and_monotone() {
        for a in ALL_UPDATES {
            // idempotent: x.max(x) == x
            let mut got = a;
            got.max_self(a);
            assert_eq!(got, a);

            // absorbing top element: nothing can lower RefreshDomAllWindows
            let mut top = Update::RefreshDomAllWindows;
            top.max_self(a);
            assert_eq!(top, Update::RefreshDomAllWindows);

            // monotone: max_self never decreases self
            let mut m = a;
            m.max_self(Update::DoNothing);
            assert!(m >= a);
        }
    }

    /// Applying the same set of updates in any order must converge to the same
    /// value (commutativity/associativity of the fold), since callbacks fold
    /// their `Update`s in nondeterministic order.
    #[test]
    fn update_max_self_fold_is_order_independent() {
        for a in ALL_UPDATES {
            for b in ALL_UPDATES {
                for c in ALL_UPDATES {
                    let mut fwd = a;
                    fwd.max_self(b);
                    fwd.max_self(c);

                    let mut rev = c;
                    rev.max_self(b);
                    rev.max_self(a);

                    assert_eq!(fwd, rev, "fold of {a:?},{b:?},{c:?} is order-dependent");
                }
            }
        }
    }

    // ---- LayoutCallback / default_layout_callback ---------------------------

    static ALT_LAYOUT_CALLS: AtomicUsize = AtomicUsize::new(0);

    // NOTE: the body must differ from `default_layout_callback`'s, otherwise
    // identical-code-folding may merge the two symbols and the pointer
    // inequality assertion below would compare equal addresses.
    extern "C" fn alt_layout_callback(_: RefAny, _: LayoutCallbackInfo) -> Dom {
        ALT_LAYOUT_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        Dom::create_body()
    }

    #[test]
    fn default_layout_callback_returns_body_and_does_not_panic() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, win(0.0, 0.0, 0), WindowTheme::DarkMode);

        // extreme arg: zero-sized window, zero DPI, empty caches
        let dom = default_layout_callback(RefAny::new(0u32), info);
        assert_eq!(dom, Dom::create_body());
    }

    #[test]
    fn layout_callback_create_stores_the_given_fn_and_null_ctx() {
        let from_default = LayoutCallback::create(default_layout_callback as LayoutCallbackType);
        assert!(
            from_default.ctx.is_none(),
            "native-Rust create() must leave the FFI ctx empty"
        );
        assert_eq!(from_default, LayoutCallback::default());

        // create() must actually store its argument, not silently fall back
        // to the default callback.
        let from_alt = LayoutCallback::create(alt_layout_callback as LayoutCallbackType);
        assert!(from_alt.ctx.is_none());
        assert_ne!(
            from_alt, from_default,
            "create() ignored its argument (or the two fns were ICF-folded)"
        );

        // the stored pointer is callable and is the one we passed in
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let before = ALT_LAYOUT_CALLS.load(AtomicOrdering::SeqCst);
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), WindowTheme::LightMode);
        let _ = (from_alt.cb)(RefAny::new(()), info);
        assert_eq!(ALT_LAYOUT_CALLS.load(AtomicOrdering::SeqCst), before + 1);
    }

    // ---- VirtualViewCallback ------------------------------------------------

    extern "C" fn vv_keep_current_cb(_: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
        VirtualViewReturn::keep_current(
            info.scroll_size,
            info.scroll_offset,
            info.virtual_scroll_size,
            info.virtual_scroll_offset,
        )
    }

    #[test]
    fn virtual_view_callback_create_round_trips_through_the_fn_ptr() {
        let cb = VirtualViewCallback::create(vv_keep_current_cb as VirtualViewCallbackType);
        assert!(cb.ctx.is_none());

        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let info = vv_info(&fonts, &images, bounds_1x1());

        let ret = (cb.cb)(RefAny::new(0u8), info);
        assert!(ret.dom.is_none());
        assert_eq!(ret.scroll_size, LogicalSize::new(100.0, 200.0));
        assert_eq!(ret.scroll_offset, LogicalPosition::new(1.0, 2.0));
        assert_eq!(ret.virtual_scroll_size, LogicalSize::new(1000.0, 2000.0));
        assert_eq!(ret.virtual_scroll_offset, LogicalPosition::new(3.0, 4.0));
    }

    // ---- VirtualViewCallbackInfo -------------------------------------------

    #[test]
    fn virtual_view_callback_info_new_holds_its_fields() {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let bounds = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(800, 600),
            DpiScaleFactor::new(2.0),
        );
        let info = vv_info(&fonts, &images, bounds);

        assert_eq!(info.reason, VirtualViewCallbackReason::InitialRender);
        assert_eq!(info.window_theme, WindowTheme::LightMode);
        assert_eq!(info.get_bounds().get_logical_size(), LogicalSize::new(800.0, 600.0));
        assert_eq!(info.get_bounds().get_hidpi_factor(), DpiScaleFactor::new(2.0));
        assert_eq!(info.scroll_size, LogicalSize::new(100.0, 200.0));

        // the raw pointers must alias the borrows we handed in
        assert!(core::ptr::eq(info.internal_get_system_fonts(), &fonts));
        assert!(core::ptr::eq(info.internal_get_image_cache(), &images));

        // FFI ctx starts empty and the measure hook starts absent
        assert!(info.get_ctx().is_none());
        assert_eq!(
            info.measure_dom(Dom::create_body(), LogicalSize::new(10.0, 10.0)),
            LogicalSize::zero()
        );

        // clone must not disturb any of that
        let cloned = info.clone();
        assert_eq!(cloned.reason, info.reason);
        assert!(core::ptr::eq(cloned.internal_get_system_fonts(), &fonts));
        assert!(cloned.get_ctx().is_none());
    }

    #[test]
    fn virtual_view_callback_info_new_survives_nan_and_infinite_geometry() {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let info = VirtualViewCallbackInfo::new(
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
            &fonts,
            &images,
            WindowTheme::DarkMode,
            HidpiAdjustedBounds::from_bounds(
                LayoutSize::new(isize::MAX, isize::MIN),
                DpiScaleFactor::new(f32::NAN),
            ),
            LogicalSize::new(f32::NAN, f32::INFINITY),
            LogicalPosition::new(f32::NEG_INFINITY, f32::MAX),
            LogicalSize::new(f32::MIN, 0.0),
            LogicalPosition::new(-0.0, f32::EPSILON),
        );

        // extreme values are stored verbatim, not silently clamped
        assert!(info.scroll_size.width.is_nan());
        assert!(info.scroll_size.height.is_infinite());
        assert!(info.scroll_offset.x.is_infinite() && info.scroll_offset.x.is_sign_negative());
        assert_eq!(info.virtual_scroll_size.width, f32::MIN);
        assert_eq!(info.reason, VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom));

        // and none of the getters panic on that instance
        assert!(info.get_ctx().is_none());
        assert!(info.get_bounds().get_logical_size().width > 0.0);
    }

    #[test]
    fn virtual_view_callback_info_get_ctx_clones_without_double_free() {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let mut info = vv_info(&fonts, &images, bounds_1x1());

        // null callable_ptr -> None (the native-Rust path)
        assert!(info.get_ctx().is_none());

        let callable = OptionRefAny::Some(RefAny::new(0xDEAD_BEEF_u32));
        info.set_callable_ptr(&callable);

        // repeated get_ctx() must hand out independent clones; dropping them
        // all must not corrupt the original RefAny's refcount.
        for _ in 0..64 {
            let got = info.get_ctx();
            assert!(got.is_some());
            drop(got);
        }

        let mut got = info.get_ctx();
        match got {
            OptionRefAny::Some(ref mut r) => {
                let inner = r.downcast_ref::<u32>().expect("ctx should hold a u32");
                assert_eq!(*inner, 0xDEAD_BEEF_u32);
            }
            OptionRefAny::None => panic!("callable_ptr was set, get_ctx() returned None"),
        }
        drop(got);

        // the original is still alive and intact after all those clones dropped
        let mut orig = callable;
        match orig {
            OptionRefAny::Some(ref mut r) => {
                assert_eq!(*r.downcast_ref::<u32>().unwrap(), 0xDEAD_BEEF_u32);
            }
            OptionRefAny::None => panic!("original callable was consumed"),
        }
    }

    // ---- measure_dom --------------------------------------------------------

    static MEASURE_CALLS: AtomicUsize = AtomicUsize::new(0);

    /// Test trampoline. Per the `MeasureDomFn` contract the `Dom` is passed by
    /// pointer and **consumed** (moved out) here.
    extern "C" fn test_measure_dom_fn(
        ctx: *mut c_void,
        dom: *mut Dom,
        available: LogicalSize,
    ) -> LogicalSize {
        MEASURE_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        // SAFETY: `measure_dom` always passes a valid, owned-but-ManuallyDrop
        // Dom; taking it by value here is exactly the documented contract.
        let dom = unsafe { core::ptr::read(dom) };
        drop(dom);
        if !ctx.is_null() {
            // SAFETY: the only caller below passes a `&mut u32`.
            unsafe {
                *ctx.cast::<u32>() = 0xABCD;
            }
        }
        LogicalSize::new(available.width * 2.0, available.height / 2.0)
    }

    #[test]
    fn measure_dom_without_hook_returns_zero_for_every_input() {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let info = vv_info(&fonts, &images, bounds_1x1());

        // zero / negative / NaN / infinite / huge available sizes must all take
        // the null-hook early-out without panicking (and must drop the Dom).
        for available in [
            LogicalSize::zero(),
            LogicalSize::new(-1.0, -1.0),
            LogicalSize::new(f32::NAN, f32::NAN),
            LogicalSize::new(f32::INFINITY, f32::NEG_INFINITY),
            LogicalSize::new(f32::MAX, f32::MIN),
            LogicalSize::new(1.0, 1_000_000.0),
        ] {
            assert_eq!(
                info.measure_dom(Dom::create_body(), available),
                LogicalSize::zero()
            );
        }
    }

    #[test]
    fn measure_dom_with_hook_forwards_ctx_and_available_and_consumes_the_dom() {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let mut info = vv_info(&fonts, &images, bounds_1x1());

        let mut ctx_val: u32 = 0;
        info.set_measure_dom_fn(
            test_measure_dom_fn,
            core::ptr::from_mut(&mut ctx_val).cast::<c_void>(),
        );

        // NOTE: `>` not `== before + 1` - other tests share this static and
        // run in parallel, so only monotonicity is safe to assert here.
        let before = MEASURE_CALLS.load(AtomicOrdering::SeqCst);
        let out = info.measure_dom(Dom::create_body(), LogicalSize::new(100.0, 40.0));

        assert!(MEASURE_CALLS.load(AtomicOrdering::SeqCst) > before);
        assert_eq!(out, LogicalSize::new(200.0, 20.0));
        assert_eq!(ctx_val, 0xABCD, "measure ctx pointer was not forwarded");

        // the documented virtual-scroll sizing idiom: measure at a huge height
        let natural = info.measure_dom(Dom::create_body(), LogicalSize::new(320.0, 1_000_000.0));
        assert_eq!(natural, LogicalSize::new(640.0, 500_000.0));

        // NaN / infinite constraints reach the hook unmodified and come back
        // as NaN/inf rather than panicking or being clamped
        let nan = info.measure_dom(Dom::create_body(), LogicalSize::new(f32::NAN, 4.0));
        assert!(nan.width.is_nan());
        assert_eq!(nan.height, 2.0);

        let inf = info.measure_dom(Dom::create_body(), LogicalSize::new(f32::INFINITY, 4.0));
        assert!(inf.width.is_infinite());
    }

    #[test]
    fn measure_dom_hook_can_be_replaced_and_last_writer_wins() {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let mut info = vv_info(&fonts, &images, bounds_1x1());

        info.set_measure_dom_fn(test_measure_dom_fn, core::ptr::null_mut());
        // null ctx must be tolerated by the trampoline contract
        let first = info.measure_dom(Dom::create_body(), LogicalSize::new(2.0, 8.0));
        assert_eq!(first, LogicalSize::new(4.0, 4.0));

        let mut ctx_val: u32 = 0;
        info.set_measure_dom_fn(
            test_measure_dom_fn,
            core::ptr::from_mut(&mut ctx_val).cast::<c_void>(),
        );
        let second = info.measure_dom(Dom::create_body(), LogicalSize::new(2.0, 8.0));
        assert_eq!(second, first);
        assert_eq!(ctx_val, 0xABCD);
    }

    // ---- VirtualViewReturn --------------------------------------------------

    #[test]
    fn virtual_view_return_with_dom_and_keep_current_hold_their_fields() {
        let ss = LogicalSize::new(600.0, 30.0);
        let so = LogicalPosition::new(0.0, 300.0);
        let vss = LogicalSize::new(600.0, 30_000.0);
        let vso = LogicalPosition::zero();

        let with = VirtualViewReturn::with_dom(Dom::create_body(), ss, so, vss, vso);
        assert!(with.dom.is_some(), "with_dom must produce OptionDom::Some");
        assert_eq!(with.scroll_size, ss);
        assert_eq!(with.scroll_offset, so);
        assert_eq!(with.virtual_scroll_size, vss);
        assert_eq!(with.virtual_scroll_offset, vso);
        assert_eq!(with.dom, OptionDom::Some(Dom::create_body()));

        let keep = VirtualViewReturn::keep_current(ss, so, vss, vso);
        assert!(keep.dom.is_none(), "keep_current must produce OptionDom::None");
        assert_eq!(keep.scroll_size, ss);
        assert_eq!(keep.scroll_offset, so);
        assert_eq!(keep.virtual_scroll_size, vss);
        assert_eq!(keep.virtual_scroll_offset, vso);

        // the two constructors differ *only* in the dom field
        assert_ne!(with, keep);

        // default is the "keep everything, render nothing" zero value
        let d = VirtualViewReturn::default();
        assert_eq!(
            d,
            VirtualViewReturn::keep_current(
                LogicalSize::zero(),
                LogicalPosition::zero(),
                LogicalSize::zero(),
                LogicalPosition::zero()
            )
        );
    }

    #[test]
    fn virtual_view_return_keep_current_passes_extreme_values_through_unclamped() {
        // zero
        let z = VirtualViewReturn::keep_current(
            LogicalSize::zero(),
            LogicalPosition::zero(),
            LogicalSize::zero(),
            LogicalPosition::zero(),
        );
        assert_eq!(z.scroll_size, LogicalSize::zero());
        assert_eq!(z.virtual_scroll_size, LogicalSize::zero());

        // negative + f32 limits: stored verbatim (no saturation, no panic)
        let n = VirtualViewReturn::keep_current(
            LogicalSize::new(-1.0, -0.0),
            LogicalPosition::new(f32::MIN, f32::MAX),
            LogicalSize::new(f32::MAX, f32::MIN_POSITIVE),
            LogicalPosition::new(-f32::EPSILON, 0.0),
        );
        assert_eq!(n.scroll_size.width, -1.0);
        assert_eq!(n.scroll_offset.x, f32::MIN);
        assert_eq!(n.scroll_offset.y, f32::MAX);
        assert_eq!(n.virtual_scroll_size.width, f32::MAX);
        assert_eq!(n.virtual_scroll_size.height, f32::MIN_POSITIVE);

        // NaN / inf: stored verbatim; NaN makes the struct unequal to itself
        // under PartialEq, so probe the fields directly.
        let x = VirtualViewReturn::keep_current(
            LogicalSize::new(f32::NAN, f32::INFINITY),
            LogicalPosition::new(f32::NEG_INFINITY, f32::NAN),
            LogicalSize::new(f32::INFINITY, f32::NAN),
            LogicalPosition::new(f32::NAN, f32::NEG_INFINITY),
        );
        assert!(x.scroll_size.width.is_nan());
        assert!(x.scroll_size.height.is_infinite() && x.scroll_size.height.is_sign_positive());
        assert!(x.scroll_offset.x.is_infinite() && x.scroll_offset.x.is_sign_negative());
        assert!(x.scroll_offset.y.is_nan());
        assert!(x.virtual_scroll_offset.y.is_infinite());
        assert!(x.dom.is_none());
    }

    // ---- TimerCallbackReturn ------------------------------------------------

    #[test]
    fn timer_callback_return_constructors_match_their_documented_flags() {
        let c = TimerCallbackReturn::continue_unchanged();
        assert_eq!(c.should_update, Update::DoNothing);
        assert_eq!(c.should_terminate, TerminateTimer::Continue);

        let cr = TimerCallbackReturn::continue_and_refresh_dom();
        assert_eq!(cr.should_update, Update::RefreshDom);
        assert_eq!(cr.should_terminate, TerminateTimer::Continue);

        let t = TimerCallbackReturn::terminate_unchanged();
        assert_eq!(t.should_update, Update::DoNothing);
        assert_eq!(t.should_terminate, TerminateTimer::Terminate);

        let tr = TimerCallbackReturn::terminate_and_refresh_dom();
        assert_eq!(tr.should_update, Update::RefreshDom);
        assert_eq!(tr.should_terminate, TerminateTimer::Terminate);

        // all four are distinct - no constructor is a copy-paste of another
        let all = [c, cr, t, tr];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                assert_eq!(i == j, a == b, "constructors {i} and {j} collide");
            }
        }

        // Default is documented as "continue, no update"
        assert_eq!(TimerCallbackReturn::default(), c);
    }

    #[test]
    fn timer_callback_return_create_round_trips_every_flag_combination() {
        for u in ALL_UPDATES {
            for t in [TerminateTimer::Continue, TerminateTimer::Terminate] {
                let r = TimerCallbackReturn::create(u, t);
                assert_eq!(r.should_update, u);
                assert_eq!(r.should_terminate, t);
            }
        }

        // the named constructors agree with the generic one
        assert_eq!(
            TimerCallbackReturn::create(Update::DoNothing, TerminateTimer::Continue),
            TimerCallbackReturn::continue_unchanged()
        );
        assert_eq!(
            TimerCallbackReturn::create(Update::RefreshDom, TerminateTimer::Terminate),
            TimerCallbackReturn::terminate_and_refresh_dom()
        );

        // RefreshDomAllWindows is reachable through create() even though no
        // named constructor exposes it
        let all_windows =
            TimerCallbackReturn::create(Update::RefreshDomAllWindows, TerminateTimer::Terminate);
        assert_eq!(all_windows.should_update, Update::RefreshDomAllWindows);
    }

    // ---- LayoutCallbackInfo: construction + getters --------------------------

    #[test]
    fn layout_callback_info_new_defaults_to_initial_reason_and_holds_fields() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, win(1280.0, 720.0, 192), WindowTheme::DarkMode);

        assert_eq!(info.relayout_reason(), RelayoutReason::Initial);
        assert_eq!(info.theme, WindowTheme::DarkMode);
        assert_eq!(info.get_window_width(), 1280.0);
        assert_eq!(info.get_window_height(), 720.0);
        assert_eq!(info.get_dpi_factor(), 2.0);
        assert!(info.get_ctx().is_none());

        // the borrowed resources are reachable through the laundered pointer
        assert!(core::ptr::eq(info.internal_get_image_cache(), &fx.images));
        assert!(core::ptr::eq(info.internal_get_system_fonts(), &fx.fonts));
        assert!(core::ptr::eq(info.internal_get_gl_context(), &fx.gl));
        assert!(info.get_gl_context().is_none());
    }

    #[test]
    fn layout_callback_info_new_with_reason_round_trips_every_reason() {
        let fx = Fixture::new();
        let rd = fx.ref_data();

        for reason in [
            RelayoutReason::Initial,
            RelayoutReason::RefreshDom,
            RelayoutReason::Resize,
            RelayoutReason::ThemeChange,
            RelayoutReason::RouteChange,
            RelayoutReason::Other,
        ] {
            let info = LayoutCallbackInfo::new_with_reason(
                &rd,
                WindowSize::default(),
                WindowTheme::LightMode,
                reason,
            );
            assert_eq!(info.relayout_reason(), reason);
            // clone must preserve it
            assert_eq!(info.clone().relayout_reason(), reason);
        }

        assert_eq!(RelayoutReason::default(), RelayoutReason::Initial);
    }

    #[test]
    fn layout_callback_info_get_system_style_shares_the_arc() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), WindowTheme::LightMode);

        let a = info.get_system_style();
        let b = info.get_system_style();
        // it is a clone of the *same* Arc, not a fresh deep copy
        assert!(Arc::ptr_eq(&a, &b));
        assert!(Arc::ptr_eq(&a, &fx.style));

        // repeated cloning must not leak/underflow the refcount
        let before = Arc::strong_count(&fx.style);
        for _ in 0..128 {
            drop(info.get_system_style());
        }
        assert_eq!(Arc::strong_count(&fx.style), before);
    }

    #[test]
    fn layout_callback_info_get_ctx_is_none_until_set_then_clones_safely() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let mut info = LayoutCallbackInfo::new(&rd, WindowSize::default(), WindowTheme::LightMode);

        assert!(info.get_ctx().is_none(), "native path must have a null ctx");

        let callable = OptionRefAny::Some(RefAny::new(7u64));
        info.set_callable_ptr(&callable);

        for _ in 0..64 {
            assert!(info.get_ctx().is_some());
        }

        let mut got = info.get_ctx();
        match got {
            OptionRefAny::Some(ref mut r) => assert_eq!(*r.downcast_ref::<u64>().unwrap(), 7),
            OptionRefAny::None => panic!("get_ctx() lost the callable"),
        }
        drop(got);

        // a clone of the info keeps pointing at the same callable
        let cloned = info.clone();
        assert!(cloned.get_ctx().is_some());
    }

    #[test]
    fn layout_callback_info_get_system_fonts_is_empty_for_an_empty_cache() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), WindowTheme::LightMode);

        // an empty FcFontCache must yield an empty list, not panic
        let fonts: Vec<AzStringPair> = info.get_system_fonts();
        assert!(fonts.is_empty());
        // and be stable across calls
        assert_eq!(info.get_system_fonts().len(), fonts.len());
    }

    // ---- LayoutCallbackInfo::get_image --------------------------------------

    #[test]
    fn get_image_returns_none_for_missing_empty_and_hostile_ids() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), WindowTheme::LightMode);

        assert!(info.get_image(&s("")).is_none());
        assert!(info.get_image(&s("   ")).is_none());
        assert!(info.get_image(&s("nope")).is_none());
        assert!(info.get_image(&s("\u{1F600}\u{0301}")).is_none());
        assert!(info.get_image(&s("\0")).is_none());
        assert!(info.get_image(&s(&"x".repeat(100_000))).is_none());
    }

    #[test]
    fn get_image_finds_an_inserted_id_and_is_exact_match() {
        let mut fx = Fixture::new();
        fx.images.add_css_image_id(
            s("logo"),
            ImageRef::null_image(2, 2, RawImageFormat::RGBA8, Vec::new()),
        );
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), WindowTheme::LightMode);

        assert!(info.get_image(&s("logo")).is_some(), "positive control");

        // lookup is exact: no trimming, no case folding, no prefix matching
        assert!(info.get_image(&s("Logo")).is_none());
        assert!(info.get_image(&s(" logo")).is_none());
        assert!(info.get_image(&s("logo ")).is_none());
        assert!(info.get_image(&s("log")).is_none());
        assert!(info.get_image(&s("logos")).is_none());
    }

    // ---- LayoutCallbackInfo::get_active_route / get_route_param -------------

    #[test]
    fn get_route_param_returns_none_when_no_route_is_active() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), WindowTheme::LightMode);

        assert!(info.get_active_route().is_none());

        // every hostile key must take the `?` early-out, never panic
        for key in ["", " ", "\t\n", "id", "\u{1F600}", "\0", "../../etc/passwd"] {
            assert!(info.get_route_param(key).is_none(), "key {key:?}");
        }
    }

    #[test]
    fn get_route_param_valid_minimal_and_unicode_positive_controls() {
        let fx = Fixture::with_route(user_route());
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), WindowTheme::LightMode);

        let route = info.get_active_route().expect("route was configured");
        assert_eq!(route.pattern.as_str(), "/user/:id");

        // positive control
        assert_eq!(info.get_route_param("id").map(AzString::as_str), Some("42"));
        // multibyte key round-trips
        assert_eq!(
            info.get_route_param("\u{1F600}").map(AzString::as_str),
            Some("emoji")
        );
    }

    #[test]
    fn get_route_param_rejects_malformed_keys_without_trimming_or_folding() {
        let fx = Fixture::with_route(user_route());
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), WindowTheme::LightMode);

        // empty / whitespace-only
        assert!(info.get_route_param("").is_none());
        assert!(info.get_route_param("   ").is_none());
        assert!(info.get_route_param("\t\n").is_none());

        // leading/trailing junk is NOT trimmed, and lookup is case-sensitive
        assert!(info.get_route_param(" id").is_none());
        assert!(info.get_route_param("id ").is_none());
        assert!(info.get_route_param("  id  ").is_none());
        assert!(info.get_route_param("id;garbage").is_none());
        assert!(info.get_route_param("ID").is_none());
        assert!(info.get_route_param("Id").is_none());

        // no prefix / substring matching
        assert!(info.get_route_param("i").is_none());
        assert!(info.get_route_param("idd").is_none());

        // garbage bytes, NUL, control chars
        assert!(info.get_route_param("\0").is_none());
        assert!(info.get_route_param("id\0").is_none());
        assert!(info.get_route_param("\u{7F}\u{1}\u{2}").is_none());

        // boundary numeric strings
        for key in [
            "0",
            "-0",
            "9223372036854775807",
            "-9223372036854775808",
            "18446744073709551616",
            "NaN",
            "inf",
            "-inf",
            "1e400",
            "0.0000000000000000001",
        ] {
            assert!(info.get_route_param(key).is_none(), "key {key:?}");
        }

        // non-ASCII that is *not* a param, incl. combining marks
        assert!(info.get_route_param("i\u{0301}d").is_none());
        assert!(info.get_route_param("\u{1F600}\u{1F600}").is_none());
    }

    #[test]
    fn get_route_param_handles_pathological_key_sizes_and_nesting() {
        let fx = Fixture::with_route(user_route());
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), WindowTheme::LightMode);

        // extremely long key: must return None quickly, not hang or overflow
        let huge = "x".repeat(1_000_000);
        assert!(info.get_route_param(&huge).is_none());

        // a long key that *shares a prefix* with a real param
        let long_id = alloc::format!("id{}", "0".repeat(1_000_000));
        assert!(info.get_route_param(&long_id).is_none());

        // deeply nested brackets: the lookup is a flat scan, so this must not
        // recurse or stack-overflow
        let nested = "[".repeat(10_000) + &"]".repeat(10_000);
        assert!(info.get_route_param(&nested).is_none());
    }

    #[test]
    fn get_route_param_preserves_huge_and_unicode_values() {
        let big = "v".repeat(200_000);
        let route = RouteMatch {
            pattern: s("/blob/:data"),
            params: StringPairVec::from_vec(Vec::from([AzStringPair {
                key: s("data"),
                value: s(&big),
            }])),
        };
        let fx = Fixture::with_route(route);
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), WindowTheme::LightMode);

        let got = info.get_route_param("data").expect("param exists");
        assert_eq!(got.as_str().len(), 200_000);
    }

    // ---- LayoutCallbackInfo: responsive predicates ---------------------------

    #[test]
    fn window_predicates_obey_trichotomy_and_the_between_identity() {
        let fx = Fixture::new();
        let rd = fx.ref_data();

        let probes = [
            0.0f32,
            -0.0,
            1.0,
            -1.0,
            640.0,
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];

        for &dim in &probes {
            let info = LayoutCallbackInfo::new(&rd, win(dim, dim, 96), WindowTheme::LightMode);

            for &px in &probes {
                let lt = info.window_width_less_than(px);
                let gt = info.window_width_greater_than(px);
                let eq = info.get_window_width() == px;

                // exactly one of <, >, == holds for non-NaN operands
                assert_eq!(
                    u8::from(lt) + u8::from(gt) + u8::from(eq),
                    1,
                    "trichotomy broken for width {dim} vs {px}"
                );

                // height predicates mirror the width ones on a square window
                assert_eq!(info.window_height_less_than(px), lt);
                assert_eq!(info.window_height_greater_than(px), gt);

                for &px2 in &probes {
                    // between(a, b) == !(w < a) && !(w > b)
                    assert_eq!(
                        info.window_width_between(px, px2),
                        !info.window_width_less_than(px) && !info.window_width_greater_than(px2),
                        "between identity broken for width {dim} in [{px}, {px2}]"
                    );
                    assert_eq!(
                        info.window_height_between(px, px2),
                        info.window_width_between(px, px2)
                    );
                }
            }
        }
    }

    #[test]
    fn window_predicates_with_inverted_and_degenerate_ranges() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, win(640.0, 480.0, 96), WindowTheme::LightMode);

        // inverted range is always empty
        assert!(!info.window_width_between(1000.0, 100.0));
        assert!(!info.window_height_between(1000.0, 100.0));

        // degenerate (min == max) range is inclusive on both ends
        assert!(info.window_width_between(640.0, 640.0));
        assert!(info.window_height_between(480.0, 480.0));
        assert!(!info.window_width_between(639.9, 639.95));

        // inclusive boundaries
        assert!(info.window_width_between(640.0, 1000.0));
        assert!(info.window_width_between(0.0, 640.0));

        // strictness at the exact boundary
        assert!(!info.window_width_less_than(640.0));
        assert!(!info.window_width_greater_than(640.0));
        assert!(info.window_width_less_than(640.001));
        assert!(info.window_width_greater_than(639.999));

        // the widest possible range contains a finite width
        assert!(info.window_width_between(f32::NEG_INFINITY, f32::INFINITY));
    }

    #[test]
    fn window_predicates_are_all_false_for_nan_probes() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, win(640.0, 480.0, 96), WindowTheme::LightMode);

        // every comparison against NaN is false - no panic, no accidental `true`
        assert!(!info.window_width_less_than(f32::NAN));
        assert!(!info.window_width_greater_than(f32::NAN));
        assert!(!info.window_width_between(f32::NAN, f32::NAN));
        assert!(!info.window_width_between(f32::NAN, 10_000.0));
        assert!(!info.window_width_between(0.0, f32::NAN));

        assert!(!info.window_height_less_than(f32::NAN));
        assert!(!info.window_height_greater_than(f32::NAN));
        assert!(!info.window_height_between(f32::NAN, f32::NAN));
        assert!(!info.window_height_between(f32::NAN, 10_000.0));
        assert!(!info.window_height_between(0.0, f32::NAN));
    }

    #[test]
    fn window_predicates_are_all_false_for_a_nan_sized_window() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(
            &rd,
            win(f32::NAN, f32::NAN, 96),
            WindowTheme::LightMode,
        );

        assert!(info.get_window_width().is_nan());
        assert!(info.get_window_height().is_nan());

        // a NaN window is neither smaller, larger, nor within any range
        for px in [0.0f32, 640.0, f32::MAX, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!info.window_width_less_than(px));
            assert!(!info.window_width_greater_than(px));
            assert!(!info.window_height_less_than(px));
            assert!(!info.window_height_greater_than(px));
            assert!(!info.window_width_between(f32::NEG_INFINITY, px));
            assert!(!info.window_height_between(px, f32::INFINITY));
        }
    }

    #[test]
    fn get_dpi_factor_at_zero_and_u32_limits() {
        let fx = Fixture::new();
        let rd = fx.ref_data();

        // 96 DPI is the 1.0 baseline
        let base = LayoutCallbackInfo::new(&rd, win(1.0, 1.0, 96), WindowTheme::LightMode);
        assert_eq!(base.get_dpi_factor(), 1.0);

        let hidpi = LayoutCallbackInfo::new(&rd, win(1.0, 1.0, 192), WindowTheme::LightMode);
        assert_eq!(hidpi.get_dpi_factor(), 2.0);

        // dpi = 0 must not divide-by-zero-panic; it yields 0.0
        let zero = LayoutCallbackInfo::new(&rd, win(1.0, 1.0, 0), WindowTheme::LightMode);
        assert_eq!(zero.get_dpi_factor(), 0.0);

        // u32::MAX must not overflow the f32 cast - it stays finite
        let max = LayoutCallbackInfo::new(&rd, win(1.0, 1.0, u32::MAX), WindowTheme::LightMode);
        let f = max.get_dpi_factor();
        assert!(f.is_finite() && f > 0.0, "dpi factor {f} is not finite");
        assert_eq!(f, (u32::MAX as f32) / 96.0);

        // dpi = 1 rounds to a tiny-but-positive factor rather than 0
        let one = LayoutCallbackInfo::new(&rd, win(1.0, 1.0, 1), WindowTheme::LightMode);
        assert!(one.get_dpi_factor() > 0.0);
    }

    // ---- HidpiAdjustedBounds -------------------------------------------------

    #[test]
    fn hidpi_adjusted_bounds_from_bounds_holds_its_fields() {
        let b = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(800, 600),
            DpiScaleFactor::new(1.5),
        );
        assert_eq!(b.get_logical_size(), LogicalSize::new(800.0, 600.0));
        assert_eq!(b.get_hidpi_factor(), DpiScaleFactor::new(1.5));
        assert_eq!(b.logical_size, b.get_logical_size());
        assert_eq!(b.hidpi_factor, b.get_hidpi_factor());

        let p = b.get_physical_size();
        assert_eq!(p.width, 1200);
        assert_eq!(p.height, 900);
    }

    #[test]
    fn hidpi_adjusted_bounds_at_zero() {
        let b = HidpiAdjustedBounds::from_bounds(LayoutSize::new(0, 0), DpiScaleFactor::new(1.0));
        assert_eq!(b.get_logical_size(), LogicalSize::zero());
        let p = b.get_physical_size();
        assert_eq!(p.width, 0);
        assert_eq!(p.height, 0);

        // a zero scale factor collapses any size to 0x0 without panicking
        let z = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(1920, 1080),
            DpiScaleFactor::new(0.0),
        );
        let zp = z.get_physical_size();
        assert_eq!(zp.width, 0);
        assert_eq!(zp.height, 0);
    }

    /// `get_physical_size` funnels through `roundf(x) as u32`, which is a
    /// *saturating* float->int cast in Rust: negatives clamp to 0, huge values
    /// clamp to u32::MAX, NaN becomes 0. Pin that down so a future refactor to
    /// an unchecked cast (UB) or a panicking one is caught.
    #[test]
    fn hidpi_adjusted_bounds_physical_size_saturates_on_negative_input() {
        let b = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(-100, -50),
            DpiScaleFactor::new(1.0),
        );
        assert_eq!(b.get_logical_size(), LogicalSize::new(-100.0, -50.0));

        let p = b.get_physical_size();
        assert_eq!(p.width, 0, "negative logical width must clamp to 0, not wrap");
        assert_eq!(p.height, 0, "negative logical height must clamp to 0, not wrap");

        // negative scale factor on a positive size clamps the same way
        let neg_scale = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(100, 100),
            DpiScaleFactor::new(-2.0),
        );
        let np = neg_scale.get_physical_size();
        assert_eq!(np.width, 0);
        assert_eq!(np.height, 0);
    }

    #[test]
    fn hidpi_adjusted_bounds_physical_size_saturates_at_the_upper_limit() {
        // isize::MAX logical px * 1.0 overflows u32 -> must saturate, not wrap
        let b = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(isize::MAX, isize::MAX),
            DpiScaleFactor::new(1.0),
        );
        let p = b.get_physical_size();
        assert_eq!(p.width, u32::MAX);
        assert_eq!(p.height, u32::MAX);

        // isize::MIN saturates downwards to 0
        let min = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(isize::MIN, isize::MIN),
            DpiScaleFactor::new(1.0),
        );
        let mp = min.get_physical_size();
        assert_eq!(mp.width, 0);
        assert_eq!(mp.height, 0);

        // a huge scale factor on a modest size also saturates
        let huge_scale = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(1000, 1000),
            DpiScaleFactor::new(f32::MAX),
        );
        let hp = huge_scale.get_physical_size();
        assert_eq!(hp.width, u32::MAX);
        assert_eq!(hp.height, u32::MAX);
    }

    /// `DpiScaleFactor` stores its f32 in a fixed-point `isize` (x1000), so
    /// NaN quantizes to 0 and +/-inf quantize to the isize limits. Assert the
    /// *observable* consequence rather than a panic.
    #[test]
    fn hidpi_adjusted_bounds_physical_size_with_nan_and_infinite_scale() {
        let nan = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(100, 100),
            DpiScaleFactor::new(f32::NAN),
        );
        // NaN -> fixed-point 0 -> 0.0 scale -> 0x0 physical
        assert_eq!(nan.get_hidpi_factor().inner.get(), 0.0);
        let np = nan.get_physical_size();
        assert_eq!(np.width, 0);
        assert_eq!(np.height, 0);

        let inf = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(100, 100),
            DpiScaleFactor::new(f32::INFINITY),
        );
        // +inf -> saturated fixed-point -> huge (but finite) scale
        assert!(inf.get_hidpi_factor().inner.get().is_finite());
        let ip = inf.get_physical_size();
        assert_eq!(ip.width, u32::MAX);
        assert_eq!(ip.height, u32::MAX);

        let neg_inf = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(100, 100),
            DpiScaleFactor::new(f32::NEG_INFINITY),
        );
        let nip = neg_inf.get_physical_size();
        assert_eq!(nip.width, 0);
        assert_eq!(nip.height, 0);
    }

    #[test]
    fn hidpi_adjusted_bounds_physical_size_rounds_to_nearest() {
        // 0.5px rounds away from zero (libm::roundf), not truncates
        let b = HidpiAdjustedBounds::from_bounds(LayoutSize::new(3, 3), DpiScaleFactor::new(1.5));
        let p = b.get_physical_size();
        assert_eq!(p.width, 5, "3 * 1.5 = 4.5 must round to 5");
        assert_eq!(p.height, 5);

        // idempotent: repeated calls give the same answer
        let p2 = b.get_physical_size();
        assert_eq!(p.width, p2.width);
        assert_eq!(p.height, p2.height);
    }

    // ---- CoreCallbackDataVec -------------------------------------------------

    fn cb_data(cb: usize) -> CoreCallbackData {
        CoreCallbackData {
            event: EventFilter::Hover(HoverEventFilter::MouseOver),
            callback: CoreCallback::from(cb),
            refany: RefAny::new(cb),
        }
    }

    #[test]
    fn core_callback_data_vec_as_container_on_empty_vecs_does_not_panic() {
        // both the const-empty and the heap-empty representation must produce
        // a valid (length-0) container - a null-ptr slice here would be UB
        let empty = CoreCallbackDataVec::new();
        assert_eq!(empty.as_container().len(), 0);
        assert!(empty.as_container().internal.is_empty());

        let from_empty_vec = CoreCallbackDataVec::from_vec(Vec::new());
        assert_eq!(from_empty_vec.as_container().len(), 0);

        let mut mut_empty = CoreCallbackDataVec::from_vec(Vec::new());
        assert!(mut_empty.as_container_mut().internal.is_empty());
    }

    #[test]
    fn core_callback_data_vec_as_container_matches_the_backing_vec() {
        let v = CoreCallbackDataVec::from_vec(Vec::from([cb_data(1), cb_data(2), cb_data(3)]));

        let c = v.as_container();
        assert_eq!(c.len(), 3);
        assert_eq!(c.len(), v.len());
        assert_eq!(c.internal[0].callback.cb, 1);
        assert_eq!(c.internal[2].callback.cb, 3);

        // the container borrows - it does not copy
        assert!(core::ptr::eq(c.internal.as_ptr(), v.as_slice().as_ptr()));
    }

    #[test]
    fn core_callback_data_vec_as_container_mut_writes_through() {
        let mut v = CoreCallbackDataVec::from_vec(Vec::from([cb_data(1), cb_data(2)]));

        {
            let mut c = v.as_container_mut();
            assert_eq!(c.internal.len(), 2);
            c.internal[0].callback.cb = 99;
            c.internal[1].event = EventFilter::Hover(HoverEventFilter::MouseDown);
        }

        // mutations are visible through the immutable container
        let c = v.as_container();
        assert_eq!(c.internal[0].callback.cb, 99);
        assert_eq!(
            c.internal[1].event,
            EventFilter::Hover(HoverEventFilter::MouseDown)
        );
        assert_eq!(c.len(), 2);
    }
}
