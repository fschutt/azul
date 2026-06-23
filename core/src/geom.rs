//! Logical and physical coordinate types for the GUI toolkit.
//!
//! Provides DPI-independent (`Logical*`) and pixel-level (`Physical*`) geometry
//! types used throughout layout, rendering, windowing, and hit testing.
//! Logical coordinates are scaled by a DPI factor to produce physical coordinates.

// Re-export DragDelta from drag module (moved in code reorganization)
pub use crate::drag::{DragDelta, OptionDragDelta};

/// An axis-aligned rectangle in logical (DPI-independent) coordinates.
#[derive(Copy, Default, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct LogicalRect {
    pub origin: LogicalPosition,
    pub size: LogicalSize,
}

impl core::fmt::Debug for LogicalRect {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{} @ {}", self.size, self.origin)
    }
}

impl core::fmt::Display for LogicalRect {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{} @ {}", self.size, self.origin)
    }
}

impl LogicalRect {
    #[must_use] pub const fn zero() -> Self {
        Self::new(LogicalPosition::zero(), LogicalSize::zero())
    }
    #[must_use] pub const fn new(origin: LogicalPosition, size: LogicalSize) -> Self {
        Self { origin, size }
    }

    /// Scales all coordinates in-place by the given DPI scale factor.
    #[inline]
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.origin.x *= scale_factor;
        self.origin.y *= scale_factor;
        self.size.width *= scale_factor;
        self.size.height *= scale_factor;
    }

    /// Returns the maximum x coordinate (origin.x + width).
    #[inline]
    #[must_use] pub fn max_x(&self) -> f32 {
        self.origin.x + self.size.width
    }
    /// Returns the minimum x coordinate (origin.x).
    #[inline]
    #[must_use] pub const fn min_x(&self) -> f32 {
        self.origin.x
    }
    /// Returns the maximum y coordinate (origin.y + height).
    #[inline]
    #[must_use] pub fn max_y(&self) -> f32 {
        self.origin.y + self.size.height
    }
    /// Returns the minimum y coordinate (origin.y).
    #[inline]
    #[must_use] pub const fn min_y(&self) -> f32 {
        self.origin.y
    }

    /// Returns whether this rectangle intersects with another rectangle
    #[inline]
    #[must_use] pub fn intersects(&self, other: Self) -> bool {
        // Check if one rectangle is to the left of the other
        if self.max_x() <= other.min_x() || other.max_x() <= self.min_x() {
            return false;
        }

        // Check if one rectangle is above the other
        if self.max_y() <= other.min_y() || other.max_y() <= self.min_y() {
            return false;
        }

        // If we got here, the rectangles must intersect
        true
    }

    /// Returns whether this rectangle contains the given point
    #[inline]
    #[must_use] pub fn contains(&self, point: LogicalPosition) -> bool {
        point.x >= self.min_x()
            && point.x < self.max_x()
            && point.y >= self.min_y()
            && point.y < self.max_y()
    }

    /// Same as `contains()`, but returns the (x, y) offset of the hit point
    ///
    /// On a regular computer this function takes ~3.2ns to run
    #[inline]
    #[must_use] pub fn hit_test(&self, other: &LogicalPosition) -> Option<LogicalPosition> {
        let dx_left_edge = other.x - self.min_x();
        let dx_right_edge = self.max_x() - other.x;
        let dy_top_edge = other.y - self.min_y();
        let dy_bottom_edge = self.max_y() - other.y;
        if dx_left_edge > 0.0 && dx_right_edge > 0.0 && dy_top_edge > 0.0 && dy_bottom_edge > 0.0 {
            Some(LogicalPosition::new(dx_left_edge, dy_top_edge))
        } else {
            None
        }
    }

}

impl_vec!(LogicalRect, LogicalRectVec, LogicalRectVecDestructor, LogicalRectVecDestructorType, LogicalRectVecSlice, OptionLogicalRect);
impl_vec_clone!(LogicalRect, LogicalRectVec, LogicalRectVecDestructor);
impl_vec_debug!(LogicalRect, LogicalRectVec);
impl_vec_partialeq!(LogicalRect, LogicalRectVec);
impl_vec_partialord!(LogicalRect, LogicalRectVec);
impl_vec_ord!(LogicalRect, LogicalRectVec);
impl_vec_hash!(LogicalRect, LogicalRectVec);
impl_vec_eq!(LogicalRect, LogicalRectVec);

use core::{
    cmp::Ordering,
    hash::{Hash, Hasher},
    ops::{self, AddAssign, SubAssign},
};

use azul_css::props::layout::LayoutWritingMode;

/// A 2D position in logical (DPI-independent) coordinates.
#[derive(Default, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct LogicalPosition {
    pub x: f32,
    pub y: f32,
}

impl LogicalPosition {
    /// Scales the position in-place by the given DPI scale factor.
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.x *= scale_factor;
        self.y *= scale_factor;
    }
}

impl SubAssign<Self> for LogicalPosition {
    fn sub_assign(&mut self, other: Self) {
        self.x -= other.x;
        self.y -= other.y;
    }
}

impl AddAssign<Self> for LogicalPosition {
    fn add_assign(&mut self, other: Self) {
        self.x += other.x;
        self.y += other.y;
    }
}

impl core::fmt::Debug for LogicalPosition {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl core::fmt::Display for LogicalPosition {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl ops::Add for LogicalPosition {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl ops::Sub for LogicalPosition {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

/// Multiplier for converting f32 coordinates to integers in Ord/Hash impls.
/// Provides ~0.001 precision, sufficient for sub-pixel layout coordinates.
const DECIMAL_MULTIPLIER: f32 = 1000.0;

impl_option!(
    LogicalPosition,
    OptionLogicalPosition,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd]
);

// PartialOrd delegates to the quantized Ord (the derived field-wise PartialOrd
// compared raw f32 and diverged from this quantized order — a latent bug).
impl PartialOrd for LogicalPosition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for LogicalPosition {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_x = (self.x * DECIMAL_MULTIPLIER) as isize;
        let self_y = (self.y * DECIMAL_MULTIPLIER) as isize;
        let other_x = (other.x * DECIMAL_MULTIPLIER) as isize;
        let other_y = (other.y * DECIMAL_MULTIPLIER) as isize;
        self_x.cmp(&other_x).then(self_y.cmp(&other_y))
    }
}

impl Eq for LogicalPosition {}

impl Hash for LogicalPosition {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        let self_x = (self.x * DECIMAL_MULTIPLIER) as isize;
        let self_y = (self.y * DECIMAL_MULTIPLIER) as isize;
        self_x.hash(state);
        self_y.hash(state);
    }
}

impl LogicalPosition {
    /// Returns the main-axis component for the given writing mode.
    #[must_use] pub const fn main(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.y,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.x,
        }
    }

    /// Returns the cross-axis component for the given writing mode.
    #[must_use] pub const fn cross(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.x,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.y,
        }
    }

    /// Creates a `LogicalPosition` from main and cross axis dimensions.
    #[must_use] pub const fn from_main_cross(main: f32, cross: f32, wm: LayoutWritingMode) -> Self {
        match wm {
            LayoutWritingMode::HorizontalTb => Self::new(cross, main),
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => Self::new(main, cross),
        }
    }
}

/// A 2D size in logical (DPI-independent) coordinates.
#[derive(Default, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct LogicalSize {
    pub width: f32,
    pub height: f32,
}

impl LogicalSize {
    /// Scales the size in-place by the given DPI scale factor and returns self.
    // Mutates in place; the returned copy is only for optional chaining, so callers
    // may legitimately discard it (e.g. ui_solver) — #[must_use] would be wrong here.
    #[allow(clippy::return_self_not_must_use)]
    pub fn scale_for_dpi(&mut self, scale_factor: f32) -> Self {
        self.width *= scale_factor;
        self.height *= scale_factor;
        *self
    }

    /// Creates a `LogicalSize` from main and cross axis dimensions.
    #[must_use] pub const fn from_main_cross(main: f32, cross: f32, wm: LayoutWritingMode) -> Self {
        match wm {
            LayoutWritingMode::HorizontalTb => Self::new(cross, main),
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => Self::new(main, cross),
        }
    }
}

impl core::fmt::Debug for LogicalSize {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl core::fmt::Display for LogicalSize {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl_option!(
    LogicalSize,
    OptionLogicalSize,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd]
);

impl_option!(
    LogicalRect,
    OptionLogicalRect,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd]
);

// PartialOrd delegates to the quantized Ord (the derived field-wise PartialOrd
// compared raw f32 and diverged from this quantized order — a latent bug).
impl PartialOrd for LogicalSize {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for LogicalSize {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_width = (self.width * DECIMAL_MULTIPLIER) as isize;
        let self_height = (self.height * DECIMAL_MULTIPLIER) as isize;
        let other_width = (other.width * DECIMAL_MULTIPLIER) as isize;
        let other_height = (other.height * DECIMAL_MULTIPLIER) as isize;
        self_width
            .cmp(&other_width)
            .then(self_height.cmp(&other_height))
    }
}

impl Eq for LogicalSize {}

impl Hash for LogicalSize {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        let self_width = (self.width * DECIMAL_MULTIPLIER) as isize;
        let self_height = (self.height * DECIMAL_MULTIPLIER) as isize;
        self_width.hash(state);
        self_height.hash(state);
    }
}

impl LogicalSize {
    /// Returns the main-axis dimension for the given writing mode.
    #[must_use] pub const fn main(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.height,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.width,
        }
    }

    /// Returns the cross-axis dimension for the given writing mode.
    #[must_use] pub const fn cross(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.width,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.height,
        }
    }

    /// Returns a new `LogicalSize` with the main-axis dimension updated.
    #[must_use] pub const fn with_main(self, wm: LayoutWritingMode, value: f32) -> Self {
        match wm {
            LayoutWritingMode::HorizontalTb => Self {
                height: value,
                ..self
            },
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => Self {
                width: value,
                ..self
            },
        }
    }

    /// Returns a new `LogicalSize` with the cross-axis dimension updated.
    #[must_use] pub const fn with_cross(self, wm: LayoutWritingMode, value: f32) -> Self {
        match wm {
            LayoutWritingMode::HorizontalTb => Self {
                width: value,
                ..self
            },
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => Self {
                height: value,
                ..self
            },
        }
    }
}

/// A 2D position in physical (pixel) coordinates.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PhysicalPosition<T> {
    pub x: T,
    pub y: T,
}

impl<T: ::core::fmt::Display> ::core::fmt::Debug for PhysicalPosition<T> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

pub type PhysicalPositionI32 = PhysicalPosition<i32>;
impl_option!(
    PhysicalPositionI32,
    OptionPhysicalPositionI32,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd]
);

/// A 2D size in physical (pixel) coordinates.
#[derive(Ord, Hash, Eq, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct PhysicalSize<T> {
    pub width: T,
    pub height: T,
}

impl<T: ::core::fmt::Display> ::core::fmt::Debug for PhysicalSize<T> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

pub type PhysicalSizeU32 = PhysicalSize<u32>;
impl_option!(
    PhysicalSizeU32,
    OptionPhysicalSizeU32,
    [Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]
);
pub type PhysicalSizeF32 = PhysicalSize<f32>;
impl_option!(
    PhysicalSizeF32,
    OptionPhysicalSizeF32,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

impl LogicalPosition {
    #[inline]
    #[must_use] pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    /// Converts to physical pixel coordinates by multiplying by the DPI factor.
    #[inline]
    #[must_use] pub fn to_physical(self, hidpi_factor: f32) -> PhysicalPosition<u32> {
        PhysicalPosition {
            x: libm::roundf(self.x * hidpi_factor) as u32,
            y: libm::roundf(self.y * hidpi_factor) as u32,
        }
    }
}

impl<T> PhysicalPosition<T> {
    #[inline]
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl PhysicalPosition<i32> {
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(0, 0)
    }
    /// Converts to logical coordinates by dividing by the DPI factor.
    #[inline]
    #[must_use] pub fn to_logical(self, hidpi_factor: f32) -> LogicalPosition {
        LogicalPosition {
            x: self.x as f32 / hidpi_factor,
            y: self.y as f32 / hidpi_factor,
        }
    }
}

impl PhysicalPosition<f64> {
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    /// Converts to logical coordinates by dividing by the DPI factor.
    #[inline]
    #[must_use] pub fn to_logical(self, hidpi_factor: f32) -> LogicalPosition {
        LogicalPosition {
            x: self.x as f32 / hidpi_factor,
            y: self.y as f32 / hidpi_factor,
        }
    }
}

impl LogicalSize {
    #[inline]
    #[must_use] pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    /// Converts to physical pixel size by multiplying by the DPI factor.
    #[inline]
    #[must_use] pub fn to_physical(self, hidpi_factor: f32) -> PhysicalSize<u32> {
        PhysicalSize {
            width: libm::roundf(self.width * hidpi_factor) as u32,
            height: libm::roundf(self.height * hidpi_factor) as u32,
        }
    }
}

impl<T> PhysicalSize<T> {
    #[inline]
    pub const fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

impl PhysicalSize<u32> {
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(0, 0)
    }
    /// Converts to logical coordinates by dividing by the DPI factor.
    #[inline]
    #[must_use] pub fn to_logical(self, hidpi_factor: f32) -> LogicalSize {
        LogicalSize {
            width: self.width as f32 / hidpi_factor,
            height: self.height as f32 / hidpi_factor,
        }
    }
}

/// Marker enum documenting which coordinate space a geometric value is in.
///
/// This is for documentation and debugging purposes only — it does not enforce
/// type safety at compile time. Use comments like `[CoordinateSpace::Window]`
/// or `[CoordinateSpace::ScrollFrame]` in code to document coordinate contexts.
///
/// **Common bug pattern:** passing `Window`-space coordinates where
/// `ScrollFrame`-space is expected (or vice versa). The scroll frame creates a
/// new spatial node, so primitives must be offset by the frame origin.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum CoordinateSpace {
    /// Absolute coordinates from window top-left (0,0).
    /// Layout engine output is in this space.
    Window,
    
    /// Relative to scroll frame content origin.
    /// Transformation: `scroll_pos` = `window_pos` - `scroll_frame_origin`
    ScrollFrame,
    
    /// Relative to parent node's content box origin.
    Parent,
    
    /// Relative to a CSS transform reference frame origin.
    ReferenceFrame,
}


// =============================================================================
// Type-safe coordinate newtypes for API clarity
// =============================================================================

/// Position in screen coordinates (logical pixels, relative to primary monitor origin).
/// On Wayland: falls back to window-local since global coords are unavailable.
#[derive(Default, Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ScreenPosition {
    pub x: f32,
    pub y: f32,
}

impl ScreenPosition {
    #[inline]
    #[must_use] pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    /// Convert to a raw `LogicalPosition` (for interop with existing code).
    #[inline]
    #[must_use] pub const fn to_logical(self) -> LogicalPosition {
        LogicalPosition { x: self.x, y: self.y }
    }
    /// Create from a raw `LogicalPosition` that is known to be in screen space.
    #[inline]
    #[must_use] pub const fn from_logical(p: LogicalPosition) -> Self {
        Self { x: p.x, y: p.y }
    }
}

impl_option!(
    ScreenPosition,
    OptionScreenPosition,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

/// Position relative to a DOM node's border box origin (logical pixels).
#[derive(Default, Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct CursorNodePosition {
    pub x: f32,
    pub y: f32,
}

impl CursorNodePosition {
    #[inline]
    #[must_use] pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    #[inline]
    #[must_use] pub const fn to_logical(self) -> LogicalPosition {
        LogicalPosition { x: self.x, y: self.y }
    }
    #[inline]
    #[must_use] pub const fn from_logical(p: LogicalPosition) -> Self {
        Self { x: p.x, y: p.y }
    }
}

impl_option!(
    CursorNodePosition,
    OptionCursorNodePosition,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);
