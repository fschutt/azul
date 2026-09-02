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
    geom::{
        LogicalPosition, LogicalRect, LogicalRectVec, LogicalSize, OptionLogicalPosition,
        PhysicalSize,
    },
    gl::OptionGlContextPtr,
    hit_test::OverflowingScrollNode,
    id::{NodeDataContainer, NodeDataContainerRef, NodeDataContainerRefMut, NodeId},
    prop_cache::CssPropertyCache,
    refany::{OptionRefAny, RefAny},
    resources::{
        DpiScaleFactor, FontInstanceKey, IdNamespace, ImageCache, ImageMask, ImageRef,
        RendererResources,
    },
    styled_dom::{NodeHierarchyItemId, NodeHierarchyItemVec, StyledNode, StyledNodeVec},
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

pub type VirtualViewCallbackType =
    extern "C" fn(RefAny, VirtualViewCallbackInfo) -> VirtualViewReturn;

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

// -- caret / selection tween callbacks (system text animations)
//
// The framework animates the caret and the selection highlight between their
// previous and current geometry ("tween"). The MATH of the tween is a user-
// replaceable C-ABI function set in `AppConfig.system_animations` (defaults
// below): the framework drives a short timer, computes the linear progress
// `t = elapsed / configured duration`, and calls the function to obtain the
// geometry to RENDER this frame. While a tween is in flight the caret blink
// is suppressed (the caret stays solid while it moves).

/// Inputs for one caret-tween evaluation.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[repr(C)]
pub struct CaretTweenInfo {
    /// Caret rectangle the previous frame RENDERED (mid-flight retargets
    /// start from the interpolated position, not the old logical one).
    pub past: LogicalRect,
    /// Caret rectangle the current layout actually wants.
    pub current: LogicalRect,
    /// Linear time progress `0.0..=1.0` (elapsed / configured duration).
    /// Easing/curves are this function's job.
    pub t: f32,
}

/// Returns the caret rectangle to render at progress `info.t`.
pub type CaretTweenCallbackType = extern "C" fn(RefAny, CaretTweenInfo) -> LogicalRect;

/// User-settable caret tween interpolator (see [`CaretTweenInfo`]).
#[repr(C)]
pub struct CaretTweenCallback {
    pub cb: CaretTweenCallbackType,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}
impl_callback!(CaretTweenCallback, CaretTweenCallbackType);

impl CaretTweenCallback {
    pub fn create(cb: CaretTweenCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

/// Inputs for one selection-tween evaluation.
///
/// Carries the full PAST and CURRENT selection band geometry: all rectangles
/// of the selection highlight, in display-list order — spanning multiple
/// lines and, for a cross-block selection, multiple nodes.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SelectionTweenInfo {
    /// Selection rectangles the previous frame RENDERED.
    pub past: LogicalRectVec,
    /// Selection rectangles the current layout actually wants.
    pub current: LogicalRectVec,
    /// Linear time progress `0.0..=1.0` (elapsed / configured duration).
    pub t: f32,
}

/// Returns the selection rectangles to render at progress `info.t`.
/// MUST return exactly `info.current.len()` rectangles — a mismatched
/// length makes the framework fall back to `info.current` unanimated.
pub type SelectionTweenCallbackType = extern "C" fn(RefAny, SelectionTweenInfo) -> LogicalRectVec;

/// User-settable selection tween interpolator (see [`SelectionTweenInfo`]).
#[repr(C)]
pub struct SelectionTweenCallback {
    pub cb: SelectionTweenCallbackType,
    /// For FFI: stores the foreign callable (e.g., `PyFunction`)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}
impl_callback!(SelectionTweenCallback, SelectionTweenCallbackType);

impl SelectionTweenCallback {
    pub fn create(cb: SelectionTweenCallbackType) -> Self {
        Self {
            cb,
            ctx: OptionRefAny::None,
        }
    }
}

/// Trapezoidal velocity profile: velocity ramps up HARD over the first
/// `RAMP` of the duration, cruises at constant speed, and ramps down hard
/// over the last `RAMP` — a `/‾‾‾\` velocity curve. In position terms:
/// a brief quadratic ease-in, a LINEAR middle, a brief quadratic ease-out.
/// Chosen over ease-out-cubic for the caret/selection defaults: at the
/// very short default durations the motion should read as "barely
/// noticeable glide", not as a spring (user directive). Analytic integral,
/// exact — a cubic bezier cannot express the flat-velocity plateau.
#[inline]
fn trapezoid_ease(t: f32) -> f32 {
    const RAMP: f32 = 0.25;
    // Peak velocity so the total distance integrates to exactly 1.
    const V: f32 = 1.0 / (1.0 - RAMP);
    let t = t.clamp(0.0, 1.0);
    if t < RAMP {
        V * t * t / (2.0 * RAMP)
    } else if t <= 1.0 - RAMP {
        V * (RAMP / 2.0 + (t - RAMP))
    } else {
        let inv = 1.0 - t;
        1.0 - V * inv * inv / (2.0 * RAMP)
    }
}

#[inline]
// Plain `a + (b - a) * e`, NOT mul_add: fused multiply-add changes f32
// results, and tween geometry must be bit-reproducible across builds (the
// e2e corpus pins pixel-exact frames).
#[allow(clippy::suboptimal_flops)]
fn lerp_rect(from: LogicalRect, to: LogicalRect, e: f32) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition {
            x: from.origin.x + (to.origin.x - from.origin.x) * e,
            y: from.origin.y + (to.origin.y - from.origin.y) * e,
        },
        size: LogicalSize {
            width: from.size.width + (to.size.width - from.size.width) * e,
            height: from.size.height + (to.size.height - from.size.height) * e,
        },
    }
}

/// Default caret tween: trapezoidal-velocity lerp of origin and size
/// (hard rise, linear cruise, hard fall — see [`trapezoid_ease`]).
#[must_use]
pub extern "C" fn default_caret_tween(_data: RefAny, info: CaretTweenInfo) -> LogicalRect {
    lerp_rect(info.past, info.current, trapezoid_ease(info.t))
}

/// Default selection tween: trapezoidal-velocity lerp, rectangles paired by
/// the LINE they sit on — not by their position in the list.
///
/// Index pairing broke every UPWARD extension: growing the selection upward
/// prepends a rect, which shifts every later rect one slot, so each line lerped
/// from the geometry of the line ABOVE it and the whole band visibly slid.
/// Geometric pairing is stable under insertion at either end.
///
/// Rectangles with no counterpart on their line (a line the selection did not
/// cover before) appear at their final geometry immediately. Each past
/// rectangle is consumed at most once, so a line that bidi splits into several
/// rectangles still pairs one-to-one.
#[must_use]
pub extern "C" fn default_selection_tween(
    _data: RefAny,
    info: SelectionTweenInfo,
) -> LogicalRectVec {
    let e = trapezoid_ease(info.t);
    let past = info.past.as_ref();
    let mut taken = alloc::vec![false; past.len()];
    let out: Vec<LogicalRect> = info
        .current
        .as_ref()
        .iter()
        .map(|cur| {
            take_same_line_rect(past, &mut taken, *cur).map_or(*cur, |p| lerp_rect(p, *cur, e))
        })
        .collect();
    out.into()
}

/// The not-yet-consumed `past` rectangle sitting on the same line as `cur` —
/// the closest one vertically, ties to the earlier one — marked consumed.
/// `None` when no past rectangle shares that line.
///
/// "Same line" means the vertical CENTRES are within half the shorter
/// rectangle's height of each other: a line that shifted by a fraction of its
/// own height is still recognised (and glides there), a different line never
/// is. The test is a positive comparison, which NaN fails, so garbage geometry
/// pops instead of pairing wrongly.
fn take_same_line_rect(
    past: &[LogicalRect],
    taken: &mut [bool],
    cur: LogicalRect,
) -> Option<LogicalRect> {
    let cur_centre = cur.origin.y + cur.size.height / 2.0;
    let mut best: Option<(usize, f32)> = None;

    for (i, p) in past.iter().enumerate() {
        if taken.get(i).copied().unwrap_or(true) {
            continue;
        }
        let dy = (p.origin.y + p.size.height / 2.0 - cur_centre).abs();
        let tolerance = p.size.height.min(cur.size.height) / 2.0;
        if dy <= tolerance && best.is_none_or(|(_, best_dy)| dy < best_dy) {
            best = Some((i, dy));
        }
    }

    let (idx, _) = best?;
    if let Some(slot) = taken.get_mut(idx) {
        *slot = true;
    }
    past.get(idx).copied()
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
    /// RECT 1 - THE CONTAINER: the `VirtualView`'s on-screen box, computed by
    /// the framework from the outer DOM. You do not set this; you render into
    /// it.
    pub bounds: HidpiAdjustedBounds,
    /// RECT 2 - WHAT IS CURRENTLY MATERIALIZED, in VIRTUAL space: the window
    /// you returned last time (`origin` = where it starts in the document,
    /// `size` = its extent). Zero-sized on the first invoke.
    pub materialized: LogicalRect,
    /// RECT 3 - THE DOCUMENT, in VIRTUAL space: the extent you last declared,
    /// which is what the scrollbar currently represents.
    pub virtual_rect: LogicalRect,
    /// WHERE THE USER IS LOOKING: the live scroll offset in virtual space.
    ///
    /// This is the input your "which slice do I render?" math keys off. It was
    /// previously spelled `virtual_scroll_offset` and the engine hardcoded
    /// that to zero, so apps computing a page index from it always rendered
    /// the first page — one of the two reasons a `VirtualView` could not
    /// scroll.
    pub scroll_offset: LogicalPosition,
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
            materialized: self.materialized,
            virtual_rect: self.virtual_rect,
            scroll_offset: self.scroll_offset,
            callable_ptr: self.callable_ptr,
            measure_dom_fn: self.measure_dom_fn,
            measure_dom_ctx: self.measure_dom_ctx,
            _abi_mut: self._abi_mut,
        }
    }
}

impl VirtualViewCallbackInfo {
    #[must_use]
    pub const fn new<'a>(
        reason: VirtualViewCallbackReason,
        system_fonts: &'a FcFontCache,
        image_cache: &'a ImageCache,
        window_theme: WindowTheme,
        bounds: HidpiAdjustedBounds,
        materialized: LogicalRect,
        virtual_rect: LogicalRect,
        scroll_offset: LogicalPosition,
    ) -> Self {
        Self {
            reason,
            system_fonts: core::ptr::from_ref::<FcFontCache>(system_fonts),
            image_cache: core::ptr::from_ref::<ImageCache>(image_cache),
            window_theme,
            bounds,
            materialized,
            virtual_rect,
            scroll_offset,
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
    #[must_use]
    pub fn measure_dom(&self, dom: Dom, available: LogicalSize) -> LogicalSize {
        if self.measure_dom_fn.is_null() {
            return LogicalSize::zero();
        }
        // SAFETY: measure_dom_fn is only ever set via set_measure_dom_fn,
        // which stores a valid MeasureDomFn.
        let f: MeasureDomFn = unsafe { core::mem::transmute(self.measure_dom_fn) };
        let mut dom = core::mem::ManuallyDrop::new(dom);
        f(
            self.measure_dom_ctx,
            core::ptr::from_mut::<Dom>(&mut dom),
            available,
        )
    }

    /// Get the callable for FFI language bindings (Python, etc.)
    #[must_use]
    pub fn get_ctx(&self) -> OptionRefAny {
        if self.callable_ptr.is_null() {
            OptionRefAny::None
        } else {
            unsafe { (*self.callable_ptr).clone() }
        }
    }

    #[must_use]
    pub const fn get_bounds(&self) -> HidpiAdjustedBounds {
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

    /// WHAT THIS CALLBACK MATERIALIZED, in VIRTUAL space.
    ///
    /// `origin` = where this window of content begins in the document;
    /// `size` = how much of the document it covers.
    ///
    /// One rect, not a loose offset + size: they are a single fact about a
    /// single window, and storing them apart is exactly how the origin came
    /// to be dropped on the floor (content could not be placed, so a
    /// `VirtualView` could never actually scroll).
    ///
    /// The engine places the content at
    /// `container.origin + (materialized.origin - current_scroll_offset)`.
    ///
    /// **Example**: a table showing rows 10-30 at 30px each reports
    /// `origin.y = 300`, `size.height = 600`.
    pub materialized: LogicalRect,

    /// THE WHOLE DOCUMENT, in VIRTUAL space — what the scrollbar represents.
    ///
    /// `origin` is normally zero; `size` is your current best estimate and MAY
    /// change as work completes (e.g. a background pagination pass refining a
    /// page count). Refining it is cheap and safe: **only the scrollbar reads
    /// this**, so the thumb resizes and no content moves.
    ///
    /// **Example**: a 1000-row table reports `size.height = 30_000` even
    /// though `materialized` covers 600px of it.
    pub virtual_rect: LogicalRect,
}

impl Default for VirtualViewReturn {
    fn default() -> Self {
        Self {
            dom: OptionDom::None,
            materialized: LogicalRect::zero(),
            virtual_rect: LogicalRect::zero(),
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
    /// - `materialized` - what you rendered, and where it sits in the document
    /// - `virtual_rect` - how big the document is (scrollbar sizing)
    #[must_use]
    pub const fn with_dom(dom: Dom, materialized: LogicalRect, virtual_rect: LogicalRect) -> Self {
        Self {
            dom: OptionDom::Some(dom),
            materialized,
            virtual_rect,
        }
    }

    /// Creates a return value that keeps the current DOM unchanged.
    ///
    /// Use this when the callback determines that the existing content
    /// is sufficient (e.g., already rendered ahead of scroll position).
    /// This is an optimization to avoid rebuilding the DOM unnecessarily.
    ///
    /// # Arguments
    /// - `materialized` - the window currently rendered, and where it sits
    /// - `virtual_rect` - how big the document is (scrollbar sizing)
    #[must_use]
    pub const fn keep_current(materialized: LogicalRect, virtual_rect: LogicalRect) -> Self {
        Self {
            dom: OptionDom::None,
            materialized,
            virtual_rect,
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
    #[must_use]
    pub const fn create(should_update: Update, should_terminate: TerminateTimer) -> Self {
        Self {
            should_update,
            should_terminate,
        }
    }

    /// Timer continues running, no DOM update needed.
    #[must_use]
    pub const fn continue_unchanged() -> Self {
        Self {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Continue,
        }
    }

    /// Timer continues running and DOM should be refreshed.
    #[must_use]
    pub const fn continue_and_refresh_dom() -> Self {
        Self {
            should_update: Update::RefreshDom,
            should_terminate: TerminateTimer::Continue,
        }
    }

    /// Timer should stop, no DOM update needed.
    #[must_use]
    pub const fn terminate_unchanged() -> Self {
        Self {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        }
    }

    /// Timer should stop and DOM should be refreshed.
    #[must_use]
    pub const fn terminate_and_refresh_dom() -> Self {
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
    /// #28 (d): SNAPSHOT of the system's monitors, taken (locked + cloned)
    /// by the caller right before invoking the layout callback. A snapshot —
    /// not the live `Arc<Mutex<…>>` handle — because `azul-core` is `no_std`
    /// (no Mutex) and the list is read-only during a layout pass anyway.
    /// Lets `layout()` bound how much content it builds on first layout
    /// (e.g. at most monitor-height lines / monitor-area characters), so
    /// opening a huge file can never build an unbounded DOM.
    pub monitors: crate::window::MonitorVec,
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

/// One recorded window-size query made by a `layout()` callback.
///
/// See [`LayoutCallbackInfo::window_width_less_than`] & co. The engine replays
/// these against a
/// prospective new size to decide whether a resize could change the DOM at
/// all: if no recorded answer flips (and no CSS breakpoint is crossed), the
/// callback is provably size-stable across that resize and is not re-invoked.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeQuery {
    pub axis: SizeQueryAxis,
    pub op: SizeQueryOp,
    pub threshold_px: f32,
    /// The answer given at recording time, evaluated against the size the
    /// callback actually saw.
    pub answer: bool,
}

/// Which window dimension a [`SizeQuery`] tested.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeQueryAxis {
    Width,
    Height,
}

/// The comparison a [`SizeQuery`] performed.
///
/// Four variants rather than a greater/smaller bool because the recorded
/// operator must REPLAY EXACTLY:
/// `window_width_less_than` is a strict `<` while `window_width_between`'s
/// lower bound is `>=`, and collapsing either onto the other misjudges a
/// resize landing precisely on the queried boundary — the one pixel the app
/// explicitly said it cares about.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeQueryOp {
    /// `dim < threshold` (`window_width_less_than` / `window_height_less_than`)
    LessThan,
    /// `dim > threshold` (`window_width_greater_than` / `window_height_greater_than`)
    GreaterThan,
    /// `dim >= threshold` (the lower bound of `window_*_between`)
    GreaterOrEqual,
    /// `dim <= threshold` (the upper bound of `window_*_between`)
    LessOrEqual,
}

impl SizeQuery {
    /// What this query would answer at `size` — compare with [`Self::answer`]
    /// to detect a flip. MUST mirror the operators of the recording methods
    /// exactly (see [`SizeQueryOp`]), or the engine would skip a `layout()`
    /// re-invocation right at the boundary the app asked about.
    #[must_use]
    pub fn answer_at(&self, size: LogicalSize) -> bool {
        let dim = match self.axis {
            SizeQueryAxis::Width => size.width,
            SizeQueryAxis::Height => size.height,
        };
        match self.op {
            SizeQueryOp::LessThan => dim < self.threshold_px,
            SizeQueryOp::GreaterThan => dim > self.threshold_px,
            SizeQueryOp::GreaterOrEqual => dim >= self.threshold_px,
            SizeQueryOp::LessOrEqual => dim <= self.threshold_px,
        }
    }

    /// Would this query's answer differ at `size` from the recorded one?
    #[must_use]
    pub fn flips_at(&self, size: LogicalSize) -> bool {
        self.answer_at(size) != self.answer
    }
}

/// Thread-local recorder backing the responsive helpers
/// (`LayoutCallbackInfo::window_width_less_than` & co.).
///
/// A thread-local (rather than a field on the FFI-frozen `LayoutCallbackInfo`)
/// works because the layout callback is invoked SYNCHRONOUSLY on the calling
/// thread: the engine drains the recording immediately after the callback
/// returns, on the same thread that made the queries.
///
/// Bounded, and the overflow direction matters: SILENTLY dropping queries
/// would drop exactly the flips the engine needs to see — the UNSAFE
/// direction, a resize skipping a `layout()` that would have branched. So the
/// cap does not drop; it latches an `overflowed` flag that the drain reports,
/// and the engine then treats the callback as size-dependent EVERYWHERE
/// (every resize re-invokes it — today's behaviour, merely un-optimized).
#[cfg(feature = "std")]
mod size_query_recorder {
    use super::SizeQuery;

    /// More distinct thresholds than any real breakpoint scheme uses; a
    /// callback exceeding this is generating them programmatically.
    pub(super) const SIZE_QUERY_CAP: usize = 256;

    std::thread_local! {
        static RECORDED: core::cell::RefCell<(Vec<SizeQuery>, bool)> =
            const { core::cell::RefCell::new((Vec::new(), false)) };
    }

    pub(super) fn record(q: SizeQuery) {
        RECORDED.with(|r| {
            let mut r = r.borrow_mut();
            if r.0.len() >= SIZE_QUERY_CAP {
                r.1 = true; // overflowed: the drain must report "unbounded"
            } else {
                r.0.push(q);
            }
        });
    }

    /// Drain the recording. Returns `(queries, overflowed)`; `overflowed`
    /// means the cap was hit and the list is INCOMPLETE — treat every resize
    /// as potentially DOM-changing.
    pub(super) fn take() -> (Vec<SizeQuery>, bool) {
        RECORDED.with(|r| {
            let mut r = r.borrow_mut();
            let overflowed = r.1;
            r.1 = false;
            (core::mem::take(&mut r.0), overflowed)
        })
    }
}

#[cfg(feature = "std")]
fn record_size_query(q: SizeQuery) {
    size_query_recorder::record(q);
}

/// Without `std` there is no thread-local to record into; the queries still
/// ANSWER correctly, the engine just cannot prove size-stability and falls
/// back to re-invoking `layout()` on breakpoint-relevant resizes (web builds
/// are out of scope for the resize fast path).
#[cfg(not(feature = "std"))]
fn record_size_query(_q: SizeQuery) {}

/// Drain the size queries recorded since the last drain on THIS thread.
///
/// Call immediately after a `layout()` callback returns, on the same thread.
/// `(queries, overflowed)` — on `overflowed == true` the list is incomplete
/// and the caller must treat the callback as size-dependent everywhere.
#[cfg(feature = "std")]
#[must_use]
pub fn take_recorded_size_queries() -> (alloc::vec::Vec<SizeQuery>, bool) {
    size_query_recorder::take()
}

#[cfg(not(feature = "std"))]
#[must_use]
pub fn take_recorded_size_queries() -> (alloc::vec::Vec<SizeQuery>, bool) {
    (alloc::vec::Vec::new(), false)
}

/// Which facet of the OS style a `layout()` callback read.
///
/// An "appearance change" is never one event. The light/dark polarity flips,
/// or the accent colour moves, or the UI font grows, or the icon theme is
/// swapped — and each of those reaches a different app differently. Whether
/// the change can alter what `layout()` RETURNS depends entirely on what that
/// callback read, and only the callback knows.
/// [`LayoutCallbackInfo::depends_on_system_style`] is how it says so; this
/// enum is the vocabulary.
///
/// Deliberately coarse. The distinction that pays is between the app that
/// merely mirrors light/dark (its DOM is byte-identical across two different
/// LIGHT schemes, so an accent change must not cost it a rebuild) and the app
/// that baked `colors.button_face` into inline CSS inside `layout()` (its DOM
/// is wrong the instant the palette moves). Finer facets would be more
/// precise and nobody would declare them correctly.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SystemStyleDependency {
    /// The light/dark polarity alone — [`LayoutCallbackInfo::get_theme`].
    Theme,
    /// The colour palette: text, background, accent, button, selection.
    Colors,
    /// The UI fonts — family, size, weight.
    Fonts,
    /// Sizing and spacing metrics: control sizes, scrollbar geometry,
    /// titlebar layout, input timings, focus ring.
    Metrics,
    /// The icon theme and the icon styling options.
    Icons,
    /// Accessibility and motion preferences — reduced motion, high contrast,
    /// animation speed.
    Accessibility,
    /// Everything: the callback took the whole [`SystemStyle`] and the engine
    /// cannot see which parts of it were read. The conservative answer, and
    /// what [`LayoutCallbackInfo::get_system_style`] records.
    Everything,
}

impl SystemStyleDependency {
    /// This facet's bit in a [`SystemStyleDependencies`] mask.
    #[must_use]
    pub const fn bit(self) -> u32 {
        match self {
            Self::Theme => 1 << 0,
            Self::Colors => 1 << 1,
            Self::Fonts => 1 << 2,
            Self::Metrics => 1 << 3,
            Self::Icons => 1 << 4,
            Self::Accessibility => 1 << 5,
            Self::Everything => u32::MAX,
        }
    }
}

/// The set of [`SystemStyleDependency`] facets one `layout()` call declared.
///
/// A bitmask rather than a list: the facets are few, the union is the only
/// operation, and it has to be cheap enough to fold on every declaration
/// inside a deep widget tree.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct SystemStyleDependencies {
    /// Bitmask over [`SystemStyleDependency::bit`]. `0` = nothing declared,
    /// which is NOT "depends on nothing" — see
    /// [`Self::dom_depends_on_change`].
    pub facets: u32,
}

impl SystemStyleDependencies {
    /// Nothing declared.
    #[must_use]
    pub const fn empty() -> Self {
        Self { facets: 0 }
    }

    /// Every facet — what an undeclared callback is treated as.
    #[must_use]
    pub const fn all() -> Self {
        Self { facets: u32::MAX }
    }

    /// Nothing has been declared yet.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.facets == 0
    }

    /// Fold one facet in.
    pub const fn insert(&mut self, dep: SystemStyleDependency) {
        self.facets |= dep.bit();
    }

    /// Fold another set in.
    pub const fn union(&mut self, other: Self) {
        self.facets |= other.facets;
    }

    /// Was `dep` declared? `Everything` implies every facet.
    #[must_use]
    pub const fn contains(&self, dep: SystemStyleDependency) -> bool {
        let bit = dep.bit();
        self.facets & bit == bit
    }

    /// Would a system-style change from `old` to `new` alter what the
    /// callback that declared these dependencies returns — i.e. does the
    /// change need a full `Update::RefreshDom`, or only a restyle?
    ///
    /// An EMPTY set answers `true`. "Declared nothing" is not "depends on
    /// nothing": it is the state of every callback written before this API
    /// existed, and of every callback that reads the OS style through a
    /// widget it does not control. Skipping their rebuild would leave the
    /// previous palette baked into the DOM — a silent wrong-colours bug that
    /// only a theme switch reveals. Declaring is opt-in; conservatism is the
    /// default.
    #[must_use]
    pub fn dom_depends_on_change(
        &self,
        old: &azul_css::system::SystemStyle,
        new: &azul_css::system::SystemStyle,
    ) -> bool {
        if self.is_empty() {
            return old != new;
        }
        if self.contains(SystemStyleDependency::Theme) && old.theme != new.theme {
            return true;
        }
        if self.contains(SystemStyleDependency::Colors) && old.colors != new.colors {
            return true;
        }
        if self.contains(SystemStyleDependency::Fonts) && old.fonts != new.fonts {
            return true;
        }
        if self.contains(SystemStyleDependency::Metrics)
            && (old.metrics != new.metrics
                || old.input != new.input
                || old.focus_visuals != new.focus_visuals
                || old.scrollbar != new.scrollbar
                || old.scrollbar_preferences != new.scrollbar_preferences)
        {
            return true;
        }
        if self.contains(SystemStyleDependency::Icons)
            && (old.icon_style != new.icon_style
                || old.visual_hints != new.visual_hints
                || old.linux.icon_theme != new.linux.icon_theme)
        {
            return true;
        }
        if self.contains(SystemStyleDependency::Accessibility)
            && (old.accessibility != new.accessibility
                || old.animation != new.animation
                || old.prefers_reduced_motion != new.prefers_reduced_motion
                || old.prefers_high_contrast != new.prefers_high_contrast)
        {
            return true;
        }
        false
    }
}

/// Thread-local recorder behind [`LayoutCallbackInfo::depends_on_system_style`].
///
/// Same shape, and for the same reason, as the size-query recorder above: the
/// layout callback runs SYNCHRONOUSLY on the calling thread, so the engine
/// drains what it declared right after it returns. A mask cannot overflow, so
/// unlike the size queries there is no incomplete-recording flag.
#[cfg(feature = "std")]
mod style_dep_recorder {
    use super::SystemStyleDependencies;

    std::thread_local! {
        static DECLARED: core::cell::Cell<SystemStyleDependencies> =
            const { core::cell::Cell::new(SystemStyleDependencies { facets: 0 }) };
    }

    pub(super) fn record(dep: super::SystemStyleDependency) {
        DECLARED.with(|d| {
            let mut set = d.get();
            set.insert(dep);
            d.set(set);
        });
    }

    pub(super) fn take() -> SystemStyleDependencies {
        DECLARED.with(core::cell::Cell::take)
    }
}

#[cfg(feature = "std")]
fn record_style_dependency(dep: SystemStyleDependency) {
    style_dep_recorder::record(dep);
}

/// Without `std` there is no thread-local to record into. The declarations
/// still cost nothing and the engine falls back to rebuilding on every
/// system-style change — today's behaviour, merely un-optimized.
#[cfg(not(feature = "std"))]
fn record_style_dependency(_dep: SystemStyleDependency) {}

/// Drain the system-style dependencies declared since the last drain on THIS
/// thread.
///
/// Call immediately after a `layout()` callback returns, on the same thread.
/// The empty set means the callback declared nothing — which
/// [`SystemStyleDependencies::dom_depends_on_change`] reads as "assume it
/// depends on all of it".
#[cfg(feature = "std")]
#[must_use]
pub fn take_recorded_style_dependencies() -> SystemStyleDependencies {
    style_dep_recorder::take()
}

#[cfg(not(feature = "std"))]
#[must_use]
pub fn take_recorded_style_dependencies() -> SystemStyleDependencies {
    SystemStyleDependencies::empty()
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
    #[must_use]
    pub const fn new<'a>(
        ref_data: &'a LayoutCallbackInfoRefData<'a>,
        window_size: WindowSize,
        theme: WindowTheme,
    ) -> Self {
        Self::new_with_reason(ref_data, window_size, theme, RelayoutReason::Initial)
    }

    // the `as *const ...<'static>` is a deliberate 'a -> 'static lifetime launder
    // on the raw pointer (see SAFETY note below), not a redundant cast.
    #[allow(clippy::unnecessary_cast)]
    #[must_use]
    pub const fn new_with_reason<'a>(
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
    #[must_use]
    pub const fn relayout_reason(&self) -> RelayoutReason {
        self.relayout_reason
    }

    /// Is the window's LOGICAL viewport wider than `width_px`?
    ///
    /// The structural-breakpoint helper: branch on this in `layout()` to
    /// return an entirely different DOM per form factor
    /// (`ribbon.dom_desktop()` vs `ribbon.dom_mobile()`), instead of
    /// emitting both trees and toggling visibility with `@media` rules.
    ///
    /// CONTRACT: the framework re-invokes `layout()` on every window resize
    /// (`RelayoutReason::Resize` - the regenerate path never takes the
    /// layout-equivalence shortcut when the window size changed), so the
    /// answer cannot go stale: crossing the breakpoint in either direction
    /// re-runs `layout()` and the callback returns the other tree. If a
    /// future optimization ever skips DOM regeneration on resize, it must
    /// register the thresholds queried here and force a rebuild when one is
    /// crossed - grep for this comment.
    #[must_use]
    pub fn viewport_bigger_than(&self, width_px: f32) -> bool {
        self.window_size.dimensions.width > width_px
    }

    /// Set the callable pointer for FFI language bindings
    pub const fn set_callable_ptr(&mut self, callable: &OptionRefAny) {
        self.callable_ptr = core::ptr::from_ref::<OptionRefAny>(callable);
    }

    /// Get the callable for FFI language bindings (Python, etc.)
    #[must_use]
    pub fn get_ctx(&self) -> OptionRefAny {
        if self.callable_ptr.is_null() {
            OptionRefAny::None
        } else {
            unsafe { (*self.callable_ptr).clone() }
        }
    }

    /// Declare that the DOM this callback returns depends on `dep`.
    ///
    /// THE seam between "the OS appearance changed" and "this app's DOM is
    /// now wrong". A theme switch, an accent-colour change, a UI-font resize
    /// all arrive as the same kind of event, and the engine has no way to see
    /// which of them can change what `layout()` builds — only the callback
    /// knows.
    ///
    /// Declare narrowly and a change outside what you declared costs a
    /// RESTYLE (the cascade re-resolves `system-*` colours and `@theme`
    /// conditions against the new style, warm layout caches intact) instead
    /// of a full `Update::RefreshDom` (re-invoke `layout()`, rebuild the
    /// `StyledDom`, re-cascade, re-shape every run of text).
    ///
    /// ```ignore
    /// // "I mirror light/dark and nothing else": switching between two
    /// // light colour schemes cannot change my DOM.
    /// info.depends_on_system_style(SystemStyleDependency::Theme);
    /// let dark = info.get_theme() == WindowTheme::DarkMode;
    ///
    /// // "I paint my own buttons from the OS palette": ANY palette move
    /// // invalidates my DOM, light-to-light included.
    /// info.depends_on_system_style(SystemStyleDependency::Colors);
    /// ```
    ///
    /// Declarations UNION over the whole callback, widgets included, and the
    /// union is conservative: one widget calling
    /// [`Self::get_system_style`] declares [`SystemStyleDependency::Everything`]
    /// for the entire tree, because a whole-struct read is opaque.
    ///
    /// Declaring NOTHING is not "depends on nothing" — an undeclared callback
    /// is rebuilt on every system-style change, exactly as before this API
    /// existed. Reading the `theme` field directly (`info.theme`) declares
    /// nothing either: the engine cannot see a field read, the same way it
    /// cannot see `info.window_size` being used to branch the DOM.
    #[allow(clippy::unused_self)] // C-ABI-shaped method: receiver kept for API symmetry
    pub fn depends_on_system_style(&self, dep: SystemStyleDependency) {
        record_style_dependency(dep);
    }

    /// The window's light/dark polarity, declaring
    /// [`SystemStyleDependency::Theme`].
    ///
    /// The tracked way to read what the `theme` field also holds. Use this
    /// and a change that leaves the polarity alone — a new accent colour, a
    /// different light scheme — will not rebuild the DOM.
    #[must_use]
    pub fn get_theme(&self) -> WindowTheme {
        self.depends_on_system_style(SystemStyleDependency::Theme);
        self.theme
    }

    /// Get a clone of the system style Arc.
    ///
    /// Declares [`SystemStyleDependency::Everything`]: handing out the whole
    /// struct makes the read opaque, so the honest answer is that any part of
    /// it may have reached the DOM. A callback that only wants the palette or
    /// the fonts should say so with [`Self::depends_on_system_style`] and
    /// reach for [`Self::get_system_style_untracked`].
    #[must_use]
    pub fn get_system_style(&self) -> Arc<SystemStyle> {
        self.depends_on_system_style(SystemStyleDependency::Everything);
        self.get_system_style_untracked()
    }

    /// The system style WITHOUT declaring a dependency on all of it.
    ///
    /// For a callback that has already declared what it actually reads, and
    /// for engine-internal readers (CSD, menus) whose output is rebuilt by
    /// the engine itself rather than by the app's `layout()`.
    #[must_use]
    pub fn get_system_style_untracked(&self) -> Arc<SystemStyle> {
        unsafe { (*self.ref_data).system_style.clone() }
    }

    /// #28 (d): snapshot of the system's monitors, taken by the caller right
    /// before this layout pass. Empty when the platform hasn't populated
    /// monitor info (headless, web, very early startup).
    #[must_use]
    pub fn get_monitors(&self) -> crate::window::MonitorVec {
        unsafe { (*self.ref_data).monitors.clone() }
    }

    /// #28 (d): the LARGEST monitor size in physical px — the safe upper
    /// bound for "how much content could possibly be visible at once" when
    /// the window's own monitor is not yet known at first layout. Apps use
    /// it to bound how much content the first `layout()` builds (e.g. at
    /// most monitor-height text lines, or monitor-width × monitor-height
    /// characters for a single unbroken line), so opening a huge file never
    /// builds an unbounded DOM. `None` when no monitor info is available.
    #[must_use]
    pub fn get_max_monitor_size(&self) -> azul_css::props::basic::OptionLayoutSize {
        let monitors = unsafe { &(*self.ref_data).monitors };
        let mut best: Option<LayoutSize> = None;
        for m in monitors.as_ref() {
            let s = m.size;
            let better = best.is_none_or(|b| (s.width * s.height) > (b.width * b.height));
            if better {
                best = Some(s);
            }
        }
        best.into()
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

    #[must_use]
    pub fn get_gl_context(&self) -> OptionGlContextPtr {
        self.internal_get_gl_context().clone()
    }

    #[must_use]
    pub fn get_system_fonts(&self) -> Vec<AzStringPair> {
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

    /// The window's ALREADY-BUILT system font cache.
    ///
    /// `get_system_fonts` only hands back stringified name/path pairs, which
    /// is useless to a layout callback that wants to run engine layout of
    /// its own (paginating a document, measuring for an export). Such an app
    /// had to call `build_font_cache()` and re-scan every font on the
    /// machine — measured at ~5 SECONDS on the first frame, during which the
    /// client cannot answer the compositor's configure/ping handshake and
    /// loses its surface.
    ///
    /// The cache is internally `Arc<RwLock<_>>` (rust-fontconfig 4.1+), so
    /// this clone is a handle, not a copy: the caller sees the same fonts
    /// the window already resolved, including builder-thread additions.
    #[must_use]
    pub fn get_font_cache(&self) -> FcFontCache {
        self.internal_get_system_fonts().clone()
    }

    #[must_use]
    pub fn get_image(&self, image_id: &AzString) -> Option<ImageRef> {
        self.internal_get_image_cache()
            .get_css_image_id(image_id)
            .cloned()
    }

    /// Get the active route match (pattern + extracted parameters).
    ///
    /// Returns `None` if no routes are configured or no route is active.
    #[must_use]
    pub const fn get_active_route(&self) -> Option<&crate::resources::RouteMatch> {
        unsafe { (*self.ref_data).active_route }
    }

    /// Get a route parameter by key (e.g. `get_route_param("id")` for `/user/:id`).
    ///
    /// Returns `None` if no route is active or the parameter doesn't exist.
    #[must_use]
    pub fn get_route_param(&self, key: &str) -> Option<&AzString> {
        self.get_active_route()?.get_param(key)
    }

    /// The pattern of the route this layout callback is rendering, e.g.
    /// `"/user/:id"`.
    ///
    /// `"/"` when the app configured no routes: an app without routing is on
    /// the default route, so a callback that branches on the pattern always
    /// has one string to branch on rather than an empty one.
    ///
    /// # C API
    /// ```c
    /// AzString pattern = AzLayoutCallbackInfo_getRoutePattern(&info);
    /// ```
    #[must_use]
    pub fn get_route_pattern(&self) -> AzString {
        self.get_active_route().map_or_else(
            || AzString::from_const_str("/"),
            |route| route.pattern.clone(),
        )
    }

    /// A route parameter by key, empty when the parameter or the route is
    /// absent. The owned-key, owned-return form the FFI needs;
    /// [`Self::get_route_param`] is the borrowing Rust one.
    ///
    /// # C API
    /// ```c
    /// AzString id = AzLayoutCallbackInfo_getRouteParamOrEmpty(&info,
    ///     AzString_fromConstStr("id"));
    /// ```
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn get_route_param_or_empty(&self, key: AzString) -> AzString {
        self.get_route_param(key.as_str())
            .cloned()
            .unwrap_or_else(|| AzString::from_const_str(""))
    }

    // Responsive layout helper methods.
    //
    // These are THE sanctioned way for `layout()` to branch on window size
    // (mobile vs desktop DOM shapes, instead of `display:none` stacks). Every
    // call is RECORDED, and the recording is what makes resize cheap: a resize
    // that flips none of the recorded answers (and crosses no CSS breakpoint)
    // provably cannot change what the callback returns through this channel,
    // so the engine re-flows the existing DOM instead of re-invoking it
    // (`LayoutWindow::resize_needs_full_regeneration`). Reading the size
    // imperatively (`get_window_width()`, `info.window_size`) to branch the
    // DOM is a bug in the app: the engine cannot see that read, so the DOM
    // goes stale across exactly the resizes the app cared about.

    #[allow(clippy::unused_self)] // C-ABI-shaped method: receiver kept for API symmetry
    fn record_width_query(&self, op: SizeQueryOp, threshold_px: f32, answer: bool) -> bool {
        record_size_query(SizeQuery {
            axis: SizeQueryAxis::Width,
            op,
            threshold_px,
            answer,
        });
        answer
    }

    #[allow(clippy::unused_self)] // C-ABI-shaped method: receiver kept for API symmetry
    fn record_height_query(&self, op: SizeQueryOp, threshold_px: f32, answer: bool) -> bool {
        record_size_query(SizeQuery {
            axis: SizeQueryAxis::Height,
            op,
            threshold_px,
            answer,
        });
        answer
    }

    /// Returns true if the window width is less than the given pixel value.
    /// Recorded — see the note above these helpers.
    #[must_use]
    pub fn window_width_less_than(&self, px: f32) -> bool {
        let answer = self.window_size.dimensions.width < px;
        self.record_width_query(SizeQueryOp::LessThan, px, answer)
    }

    /// Returns true if the window width is greater than the given pixel value.
    /// Recorded — see the note above these helpers.
    #[must_use]
    pub fn window_width_greater_than(&self, px: f32) -> bool {
        let answer = self.window_size.dimensions.width > px;
        self.record_width_query(SizeQueryOp::GreaterThan, px, answer)
    }

    /// Returns true if the window width is between min and max (inclusive).
    /// Recorded as its two bounds — see the note above these helpers.
    #[must_use]
    pub fn window_width_between(&self, min_px: f32, max_px: f32) -> bool {
        let width = self.window_size.dimensions.width;
        self.record_width_query(SizeQueryOp::GreaterOrEqual, min_px, width >= min_px)
            & self.record_width_query(SizeQueryOp::LessOrEqual, max_px, width <= max_px)
    }

    /// Returns true if the window height is less than the given pixel value.
    /// Recorded — see the note above these helpers.
    #[must_use]
    pub fn window_height_less_than(&self, px: f32) -> bool {
        let answer = self.window_size.dimensions.height < px;
        self.record_height_query(SizeQueryOp::LessThan, px, answer)
    }

    /// Returns true if the window height is greater than the given pixel value.
    /// Recorded — see the note above these helpers.
    #[must_use]
    pub fn window_height_greater_than(&self, px: f32) -> bool {
        let answer = self.window_size.dimensions.height > px;
        self.record_height_query(SizeQueryOp::GreaterThan, px, answer)
    }

    /// Returns true if the window height is between min and max (inclusive).
    /// Recorded as its two bounds — see the note above these helpers.
    #[must_use]
    pub fn window_height_between(&self, min_px: f32, max_px: f32) -> bool {
        let height = self.window_size.dimensions.height;
        self.record_height_query(SizeQueryOp::GreaterOrEqual, min_px, height >= min_px)
            & self.record_height_query(SizeQueryOp::LessOrEqual, max_px, height <= max_px)
    }

    /// Returns the current window width in pixels
    #[must_use]
    pub const fn get_window_width(&self) -> f32 {
        self.window_size.dimensions.width
    }

    /// Returns the current window height in pixels
    #[must_use]
    pub const fn get_window_height(&self) -> f32 {
        self.window_size.dimensions.height
    }

    /// Returns the current window DPI scale factor (1.0 = 96 DPI, 2.0 = 192 DPI)
    #[allow(clippy::cast_precision_loss)] // bounded DPI/dimension/number conversion
    #[must_use]
    pub fn get_dpi_factor(&self) -> f32 {
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
    #[must_use]
    pub const fn from_bounds(bounds: LayoutSize, hidpi_factor: DpiScaleFactor) -> Self {
        let logical_size = LogicalSize::new(bounds.width as f32, bounds.height as f32);
        Self {
            logical_size,
            hidpi_factor,
        }
    }

    #[must_use]
    pub fn get_physical_size(&self) -> PhysicalSize<u32> {
        self.get_logical_size()
            .to_physical(self.get_hidpi_factor().inner.get())
    }

    #[must_use]
    pub const fn get_logical_size(&self) -> LogicalSize {
        self.logical_size
    }

    #[must_use]
    pub const fn get_hidpi_factor(&self) -> DpiScaleFactor {
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

impl_vec!(
    CoreCallbackData,
    CoreCallbackDataVec,
    CoreCallbackDataVecDestructor,
    CoreCallbackDataVecDestructorType,
    CoreCallbackDataVecSlice,
    OptionCoreCallbackData
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
    #[must_use]
    pub fn as_container(&self) -> NodeDataContainerRef<'_, CoreCallbackData> {
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
#[path = "callbacks_test.rs"]
mod callbacks_test;
