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
    refany::RefAny,
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

/// Reason why an IFrame callback is being invoked.
///
/// This helps the callback optimize its behavior based on why it's being called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum IFrameCallbackReason {
    /// Initial render - first time the IFrame appears
    InitialRender,
    /// Parent DOM was recreated (cache invalidated)
    DomRecreated,
    /// Window/IFrame bounds expanded beyond current scroll_size
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
pub struct IFrameCallbackInfo {
    pub reason: IFrameCallbackReason,
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
            reason: self.reason,
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
        reason: IFrameCallbackReason,
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
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
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

/// Return value for an IFrame rendering callback.
///
/// # Dual Size Model
///
/// IFrame callbacks return two size/offset pairs that enable lazy loading and virtualization:
///
/// ## Actual Content (`scroll_size` + `scroll_offset`)
///
/// The size and position of content that has **actually been rendered**. This is
/// the content currently present in the returned DOM.
///
/// **Example**: A table view might render only 20 visible rows out of 1000 total rows.
///
/// ## Virtual Content (`virtual_scroll_size` + `virtual_scroll_offset`)
///
/// The size and position of content that the IFrame **pretends to have**. This is
/// used for scrollbar sizing and positioning, allowing the scrollbar to represent
/// the full dataset even when only a subset is rendered.
///
/// **Example**: The same table might pretend to have all 1000 rows for scrollbar sizing.
///
/// # Conditional Re-invocation
///
/// The IFrame callback will be re-invoked **only when necessary** to avoid performance overhead:
///
/// 1. **Initial render** - First time the IFrame appears in the layout
/// 2. **Parent DOM recreated** - The parent DOM was rebuilt from scratch (not just re-laid-out)
/// 3. **Window resize (expansion only)** - Window grows and IFrame bounds exceed `scroll_size`
///    - Only triggers **ONCE** per expansion (when bounds become uncovered)
///    - Does **NOT** trigger when window shrinks (content is clipped, not re-rendered)
///    - Does **NOT** trigger if expanded area is still within existing `scroll_size`
/// 4. **Scroll near edge** - User scrolls within threshold (default 200px) of content edge
///    - Only triggers **ONCE** per edge approach (prevents repeated calls)
///    - Flag resets when: scroll moves away from edge, or callback returns expanded content
/// 5. **Programmatic scroll** - `set_scroll_position()` scrolls beyond rendered `scroll_size`
///    - Same constraints as rule #4 (threshold and once-per-edge)
///
/// ## Window Resize Example
///
/// ```text
/// Frame 0: IFrame bounds = 800×600, scroll_size = 800×600 (perfectly covered)
/// Frame 1: Window resizes to 1000×700 (larger)
///   -> IFrame bounds = 1000×700
///   -> Bounds no longer fully covered by scroll_size (800×600)
///   -> ✅ RE-INVOKE callback once
///   
/// Frame 2: Window resizes to 1100×800 (even larger)
///   -> If callback returned scroll_size = 1100×800, fully covered again
///   -> Do NOT re-invoke (content covers new bounds)
///   -> If callback returned scroll_size = 1000×700, not fully covered
///   -> RE-INVOKE again (new uncovered area)
///
/// Frame 3: Window resizes to 900×650 (smaller)
///   -> Bounds now smaller than scroll_size
///   -> Do NOT re-invoke (content is just clipped by scrollbars)
/// ```
///
/// ## Scroll Near Edge Example
///
/// ```text
/// scroll_size = 1000×2000 (width × height)
/// Container = 800×600
/// Threshold = 200px
/// Current scroll_offset = 0×0
///
/// User scrolls to scroll_offset = 0×1500:
///   -> Bottom edge at 1500 + 600 = 2100
///   -> Within 200px of scroll_size.height (2000)
///   -> Distance from edge: 2100 - 2000 = 100px < 200px
///   -> ✅ RE-INVOKE callback to load more content
///
/// Callback returns:
///   -> New scroll_size = 1000×4000 (doubled)
///   -> Flag reset (edge no longer near)
///   -> User continues scrolling without re-invoke until near new edge
/// ```
///
/// # Optimization: Returning None
///
/// If the callback determines that no new content is needed (e.g., sufficient content
/// has already been rendered ahead of the scroll position), it can return
/// `OptionStyledDom::None` for the `dom` field. This signals the layout engine to
/// keep using the current DOM and only update the scroll bounds.
///
/// ```rust,ignore
/// fn my_iframe_callback(data: &mut MyData, info: &mut IFrameCallbackInfo) -> IFrameCallbackReturn {
///     let current_scroll = info.scroll_offset;
///     
///     // Check if we've already rendered content that covers this scroll position
///     if data.already_rendered_area_covers(current_scroll, info.bounds.size) {
///         return IFrameCallbackReturn {
///             dom: OptionStyledDom::None,  // Keep current DOM
///             scroll_size: data.current_scroll_size,
///             scroll_offset: data.current_scroll_offset,
///             virtual_scroll_size: data.virtual_size,
///             virtual_scroll_offset: LogicalPosition::zero(),
///         };
///     }
///     
///     // Otherwise, render new content
///     let new_dom = data.render_more_content(...);
///     IFrameCallbackReturn {
///         dom: OptionStyledDom::Some(new_dom),
///         ...
///     }
/// }
/// ```
///
/// # Example: Basic IFrame
///
/// ```rust,ignore
/// fn my_iframe_callback(data: &mut MyData, info: &mut IFrameCallbackInfo) -> IFrameCallbackReturn {
///     let dom = Dom::body()
///         .with_child(Dom::text("Hello from IFrame!"));
///     
///     let styled_dom = dom.style(Css::empty());
///     
///     IFrameCallbackReturn {
///         // The rendered content
///         dom: OptionStyledDom::Some(styled_dom),
///         
///         // Size of actual rendered content (matches container)
///         scroll_size: info.bounds.size,
///         
///         // Content starts at top-left
///         scroll_offset: LogicalPosition::zero(),
///         
///         // Virtual size same as actual (no virtualization needed)
///         virtual_scroll_size: info.bounds.size,
///         virtual_scroll_offset: LogicalPosition::zero(),
///     }
/// }
/// ```
///
/// # Example: Virtualized Table (Lazy Loading)
///
/// ```rust,ignore
/// struct TableData {
///     total_rows: usize,        // 1000 rows in full dataset
///     row_height: f32,          // 30px per row
///     visible_rows: Vec<Row>,   // Currently rendered rows (e.g., rows 0-29)
///     first_visible_row: usize, // Index of first rendered row
/// }
///
/// fn table_iframe_callback(data: &mut TableData, info: &mut IFrameCallbackInfo) -> IFrameCallbackReturn {
///     let container_height = info.bounds.size.height;
///     let scroll_y = info.scroll_offset.y;
///     
///     // Calculate which rows should be visible based on scroll position
///     let first_row = (scroll_y / data.row_height) as usize;
///     let visible_count = (container_height / data.row_height).ceil() as usize + 2; // +2 for buffer
///     
///     // Fetch and render only the visible rows
///     data.visible_rows = data.fetch_rows(first_row, visible_count);
///     data.first_visible_row = first_row;
///     
///     let dom = Dom::body()
///         .with_children(
///             data.visible_rows.iter().map(|row| {
///                 Dom::div()
///                     .with_child(Dom::text(row.text.clone()))
///                     .with_inline_css_props(css_property_vec![
///                         ("height", format!("{}px", data.row_height)),
///                     ])
///             }).collect()
///         );
///     
///     IFrameCallbackReturn {
///         dom: OptionStyledDom::Some(dom.style(Css::empty())),
///         
///         // ACTUAL: Size of the ~30 rendered rows (e.g., 900px tall)
///         scroll_size: LogicalSize::new(
///             info.bounds.size.width,
///             data.visible_rows.len() as f32 * data.row_height,
///         ),
///         
///         // ACTUAL: Where these rows start in virtual space (e.g., y=300 if showing rows 10-30)
///         scroll_offset: LogicalPosition::new(
///             0.0,
///             first_row as f32 * data.row_height,
///         ),
///         
///         // VIRTUAL: Size if all 1000 rows were rendered (30,000px tall)
///         virtual_scroll_size: LogicalSize::new(
///             info.bounds.size.width,
///             data.total_rows as f32 * data.row_height,
///         ),
///         
///         // VIRTUAL: Usually starts at origin
///         virtual_scroll_offset: LogicalPosition::zero(),
///     }
/// }
/// ```
///
/// In this example:
/// - Only 20-30 rows are rendered at a time (~600-900px of DOM nodes)
/// - The scrollbar represents all 1000 rows (30,000px virtual height)
/// - When user scrolls near the bottom of rendered content, callback is re-invoked
/// - New rows are rendered, and `scroll_size`/`scroll_offset` are updated
/// - User experiences seamless scrolling through the full dataset
///
/// # How the Layout Engine Uses These Values
///
/// ## For Rendering
/// - Uses `scroll_size` to determine the actual size of the IFrame's content box
/// - Uses `scroll_offset` to position the content within the virtual space
/// - Clips rendering to the visible viewport
///
/// ## For Scrollbars
/// - Uses `virtual_scroll_size` to calculate scrollbar thumb size and track length
/// - Uses `virtual_scroll_offset` as the base for scroll position calculations
/// - User sees scrollbar representing full virtual size, not just rendered content
///
/// ## For Re-invocation Checks
/// - Compares viewport bounds against `scroll_size` to detect edge proximity
/// - Compares current scroll position against `scroll_offset + scroll_size` bounds
/// - Triggers callback when user scrolls beyond the rendered content threshold
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct IFrameCallbackReturn {
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
    /// (20 rows × 30px each).
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
    /// (1000 rows × 30px each), even though only 20 rows are actually rendered.
    pub virtual_scroll_size: LogicalSize,

    /// Offset of the virtual content (usually zero).
    ///
    /// This is typically `(0, 0)` since the virtual space usually starts at the origin.
    /// Advanced use cases might use this for complex virtualization scenarios.
    pub virtual_scroll_offset: LogicalPosition,
}

impl Default for IFrameCallbackReturn {
    fn default() -> IFrameCallbackReturn {
        IFrameCallbackReturn {
            dom: OptionStyledDom::None,
            scroll_size: LogicalSize::zero(),
            scroll_offset: LogicalPosition::zero(),
            virtual_scroll_size: LogicalSize::zero(),
            virtual_scroll_offset: LogicalPosition::zero(),
        }
    }
}

impl IFrameCallbackReturn {
    /// Creates a new IFrameCallbackReturn with updated DOM content.
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
    /// Extension for future ABI stability (referenced data)
    _abi_ref: *const core::ffi::c_void,
    /// Extension for future ABI stability (mutable data)
    _abi_mut: *mut core::ffi::c_void,
}

impl Clone for LayoutCallbackInfo {
    fn clone(&self) -> Self {
        Self {
            ref_data: self.ref_data,
            window_size: self.window_size,
            theme: self.theme,
            _abi_ref: self._abi_ref,
            _abi_mut: self._abi_mut,
        }
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
            _abi_ref: core::ptr::null(),
            _abi_mut: core::ptr::null_mut(),
        }
    }

    /// Get a clone of the system style Arc
    pub fn get_system_style(&self) -> Arc<SystemStyle> {
        unsafe { (*self.ref_data).system_style.clone() }
    }

    /// Get the ABI extension pointer (for future extensibility)
    pub fn get_abi_ref(&self) -> *const c_void {
        self._abi_ref
    }

    /// Set the ABI extension pointer (for future extensibility)
    ///
    /// # Safety
    /// The caller must ensure the pointer remains valid for the lifetime
    /// of this LayoutCallbackInfo and is properly cleaned up.
    pub unsafe fn set_abi_ref(&mut self, ptr: *const c_void) {
        self._abi_ref = ptr;
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
    ///
    /// # Example
    /// ```ignore
    /// if info.window_width_less_than(750.0) {
    ///     // Show mobile view
    /// } else {
    ///     // Show desktop view
    /// }
    /// ```
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
/// Necessary when invoking `IFrameCallbacks` and `RenderImageCallbacks`, so
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

// -- image rendering callback

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
