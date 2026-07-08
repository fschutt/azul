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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} @ {}", self.size, self.origin)
    }
}

impl core::fmt::Display for LogicalRect {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
        // Edge semantics must match `contains`: left/top inclusive (`>= min`),
        // right/bottom exclusive (`< max`). Previously all four edges were
        // exclusive, so a point exactly on the left/top edge hit-tested as a
        // miss even though `contains` reported it inside — dropping/duplicating
        // hits on shared edges between adjacent rects.
        if dx_left_edge >= 0.0 && dx_right_edge > 0.0 && dy_top_edge >= 0.0 && dy_bottom_edge > 0.0 {
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
// PartialEq is hand-implemented over `quantize()` (see below) so that equality
// agrees with the quantized `Ord`/`Hash`. A derived field-wise `PartialEq`
// compared raw f32, so `a == b` could be false while `a.cmp(b) == Equal`,
// breaking `BTreeMap`/`HashMap` lookups keyed on these types.
#[derive(Default, Copy, Clone)]
#[repr(C)]
pub struct LogicalPosition {
    pub x: f32,
    pub y: f32,
}

impl PartialEq for LogicalPosition {
    fn eq(&self, other: &Self) -> bool {
        quantize(self.x) == quantize(other.x) && quantize(self.y) == quantize(other.y)
    }
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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl core::fmt::Display for LogicalPosition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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

/// Quantizes an f32 coordinate to fixed-point for stable `Ord`/`Hash`/`PartialEq`
/// (comparing raw f32 bit patterns would be unstable / non-total).
// intentional fixed-point quantization: the truncation IS the rounding step.
#[allow(clippy::cast_possible_truncation)]
fn quantize(value: f32) -> i64 {
    // NaN has no meaningful position in a total order. Map it to a single fixed
    // sentinel (`i64::MIN`) so all NaNs compare equal to each other and sort
    // below every real value — and, critically, do NOT collide with `0.0`
    // (the old `NaN as isize == 0` behaviour aliased NaN onto the origin).
    if value.is_nan() {
        return i64::MIN;
    }
    // `f32 as i64` saturates on overflow (since Rust 1.45), so an out-of-range
    // coordinate clamps to `i64::{MIN,MAX}` instead of wrapping. `isize` was
    // only 32-bit on wasm32, so a large coordinate overflowed there — `i64` is
    // wide enough on every target.
    (value * DECIMAL_MULTIPLIER) as i64
}

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
        let self_x = quantize(self.x);
        let self_y = quantize(self.y);
        let other_x = quantize(other.x);
        let other_y = quantize(other.y);
        self_x.cmp(&other_x).then(self_y.cmp(&other_y))
    }
}

impl Eq for LogicalPosition {}

impl Hash for LogicalPosition {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        let self_x = quantize(self.x);
        let self_y = quantize(self.y);
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
// PartialEq is hand-implemented over `quantize()` to agree with the quantized
// `Ord`/`Hash` (see `LogicalPosition` for the rationale).
#[derive(Default, Copy, Clone)]
#[repr(C)]
pub struct LogicalSize {
    pub width: f32,
    pub height: f32,
}

impl PartialEq for LogicalSize {
    fn eq(&self, other: &Self) -> bool {
        quantize(self.width) == quantize(other.width)
            && quantize(self.height) == quantize(other.height)
    }
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
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl core::fmt::Display for LogicalSize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
        let self_width = quantize(self.width);
        let self_height = quantize(self.height);
        let other_width = quantize(other.width);
        let other_height = quantize(other.height);
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
        let self_width = quantize(self.width);
        let self_height = quantize(self.height);
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
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
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
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
    #[allow(clippy::cast_precision_loss)]
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
    #[allow(clippy::cast_possible_truncation)]
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
    #[allow(clippy::cast_precision_loss)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp::Ordering;

    #[test]
    fn hit_test_edges_match_contains() {
        let r = LogicalRect::new(LogicalPosition::new(10.0, 20.0), LogicalSize::new(30.0, 40.0));
        // left/top edge: inclusive in both
        let tl = LogicalPosition::new(10.0, 20.0);
        assert!(r.contains(tl));
        assert!(r.hit_test(&tl).is_some());
        // just inside
        let inside = LogicalPosition::new(11.0, 21.0);
        assert!(r.contains(inside));
        assert!(r.hit_test(&inside).is_some());
        // right/bottom edge: exclusive in both
        let br = LogicalPosition::new(40.0, 60.0);
        assert!(!r.contains(br));
        assert!(r.hit_test(&br).is_none());
        // outside left
        let out = LogicalPosition::new(9.0, 20.0);
        assert!(!r.contains(out));
        assert!(r.hit_test(&out).is_none());
    }

    #[test]
    fn hit_test_offset_is_from_top_left() {
        let r = LogicalRect::new(LogicalPosition::new(10.0, 20.0), LogicalSize::new(30.0, 40.0));
        let hit = r.hit_test(&LogicalPosition::new(15.0, 25.0)).unwrap();
        assert_eq!(hit, LogicalPosition::new(5.0, 5.0));
    }

    #[test]
    fn quantize_nan_is_distinct_from_zero() {
        assert_eq!(quantize(f32::NAN), i64::MIN);
        assert_ne!(quantize(f32::NAN), quantize(0.0));
    }

    #[test]
    fn partial_eq_agrees_with_ord_and_hash() {
        use core::hash::{Hash, Hasher};
        // Two values within the same quantization bucket must be == AND cmp==Equal.
        let a = LogicalPosition::new(1.00000, 2.00000);
        let b = LogicalPosition::new(1.00004, 2.00004); // < 0.001 apart
        assert_eq!(a, b);
        assert_eq!(a.cmp(&b), Ordering::Equal);

        let hash_of = |p: &LogicalPosition| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            p.hash(&mut h);
            h.finish()
        };
        assert_eq!(hash_of(&a), hash_of(&b));

        // NaN equals NaN under the quantized PartialEq (i64::MIN bucket) — this
        // is what stops the every-frame Resize loop upstream.
        let n1 = LogicalSize::new(f32::NAN, 1.0);
        let n2 = LogicalSize::new(f32::NAN, 1.0);
        assert_eq!(n1, n2);
    }

    #[test]
    fn quantize_saturates_instead_of_wrapping() {
        // Huge coordinate must saturate, not wrap to a small/negative bucket.
        assert_eq!(quantize(f32::INFINITY), i64::MAX);
        assert_eq!(quantize(f32::NEG_INFINITY), i64::MIN);
    }
}
