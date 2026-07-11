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

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod autotest_generated {
    use core::{
        cmp::Ordering,
        hash::{Hash, Hasher},
    };

    use azul_css::props::layout::LayoutWritingMode;

    use super::*;

    /// Hostile float grid: every class that can reach a coordinate field.
    const HOSTILE: [f32; 8] = [
        f32::NAN,
        f32::NEG_INFINITY,
        f32::MIN,
        -1.0,
        0.0,
        1.0,
        f32::MAX,
        f32::INFINITY,
    ];

    const WMS: [LayoutWritingMode; 3] = [
        LayoutWritingMode::HorizontalTb,
        LayoutWritingMode::VerticalRl,
        LayoutWritingMode::VerticalLr,
    ];

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    // ---------------------------------------------------------------------
    // quantize: numeric / saturation / NaN
    // ---------------------------------------------------------------------

    #[test]
    fn quantize_zero_and_negative_zero_share_a_bucket() {
        assert_eq!(quantize(0.0), 0);
        assert_eq!(quantize(-0.0), 0);
        // Sign of zero must not split the bucket, or (0,0) and (-0,-0) would be
        // distinct HashMap keys for the same visual origin.
        assert_eq!(quantize(0.0), quantize(-0.0));
    }

    #[test]
    fn quantize_applies_the_decimal_multiplier() {
        assert_eq!(quantize(1.0), DECIMAL_MULTIPLIER as i64);
        assert_eq!(quantize(-1.0), -(DECIMAL_MULTIPLIER as i64));
        assert_eq!(quantize(1.5), 1500);
        assert_eq!(quantize(-1.5), -1500);
    }

    #[test]
    fn quantize_truncates_toward_zero_below_precision() {
        // Sub-millipixel deltas collapse into the same bucket (truncation IS the
        // documented rounding step) — and truncation is toward zero, not floor.
        assert_eq!(quantize(0.0004), 0);
        assert_eq!(quantize(-0.0004), 0);
        assert_eq!(quantize(1.0004), 1000);
        assert_eq!(quantize(-1.0004), -1000);
    }

    #[test]
    fn quantize_extremes_saturate_and_never_wrap() {
        // f32::MAX * 1000 overflows to +inf before the cast; the cast must clamp.
        assert_eq!(quantize(f32::MAX), i64::MAX);
        assert_eq!(quantize(f32::MIN), i64::MIN);
        assert_eq!(quantize(f32::INFINITY), i64::MAX);
        assert_eq!(quantize(f32::NEG_INFINITY), i64::MIN);
        // Denormal-ish tiny values must land on 0, not on a garbage bucket.
        assert_eq!(quantize(f32::MIN_POSITIVE), 0);
        assert_eq!(quantize(-f32::MIN_POSITIVE), 0);
    }

    #[test]
    fn quantize_nan_never_aliases_the_origin() {
        // The historical bug: `NaN as isize == 0` put NaN on top of (0.0, 0.0).
        assert_eq!(quantize(f32::NAN), i64::MIN);
        assert_eq!(quantize(-f32::NAN), i64::MIN);
        assert_ne!(quantize(f32::NAN), quantize(0.0));
    }

    #[test]
    fn quantize_saturation_aliases_nan_with_the_bottom_of_the_range() {
        // KNOWN, INTENTIONAL LOSSINESS: NaN, -inf and f32::MIN all collapse onto
        // the i64::MIN bucket, so they compare Equal. This keeps Ord/Eq total and
        // consistent (which is what the type contract needs), but callers cannot
        // use == to distinguish "no value" (NaN) from a huge negative coordinate.
        assert_eq!(quantize(f32::NAN), quantize(f32::NEG_INFINITY));
        assert_eq!(quantize(f32::NAN), quantize(f32::MIN));
        assert_eq!(
            LogicalPosition::new(f32::NAN, 0.0),
            LogicalPosition::new(f32::NEG_INFINITY, 0.0)
        );
    }

    #[test]
    fn quantize_is_monotonic_over_finite_inputs() {
        let ascending = [-1.0e6_f32, -1.0, -0.001, 0.0, 0.001, 1.0, 1.0e6];
        for w in ascending.windows(2) {
            assert!(
                quantize(w[0]) <= quantize(w[1]),
                "quantize inverted the order of {} and {}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn quantize_is_deterministic_across_calls() {
        for v in HOSTILE {
            assert_eq!(quantize(v), quantize(v));
        }
    }

    // ---------------------------------------------------------------------
    // Ord / Eq / Hash total-order contract over the hostile grid
    // ---------------------------------------------------------------------

    fn hostile_positions() -> [LogicalPosition; 64] {
        let mut out = [LogicalPosition::zero(); 64];
        let mut i = 0;
        for x in HOSTILE {
            for y in HOSTILE {
                out[i] = LogicalPosition::new(x, y);
                i += 1;
            }
        }
        out
    }

    #[test]
    fn ord_is_reflexive_and_antisymmetric_even_with_nan() {
        let grid = hostile_positions();
        for a in grid {
            // Eq requires reflexivity — raw f32 NaN would break it.
            assert_eq!(a.cmp(&a), Ordering::Equal);
            assert_eq!(a, a);
            for b in grid {
                assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
            }
        }
    }

    #[test]
    fn ord_is_transitive_over_the_hostile_grid() {
        let grid = hostile_positions();
        for a in grid {
            for b in grid {
                if a.cmp(&b) != Ordering::Less {
                    continue;
                }
                for c in grid {
                    if b.cmp(&c) == Ordering::Less {
                        assert_eq!(a.cmp(&c), Ordering::Less);
                    }
                }
            }
        }
    }

    #[test]
    fn partial_eq_ord_and_hash_agree_over_the_hostile_grid() {
        let grid = hostile_positions();
        for a in grid {
            for b in grid {
                let eq = a == b;
                assert_eq!(eq, a.cmp(&b) == Ordering::Equal);
                assert_eq!(Some(a.cmp(&b)), a.partial_cmp(&b));
                if eq {
                    // Hash/Eq contract: equal keys MUST hash equal, or HashMap
                    // lookups silently miss.
                    assert_eq!(hash_of(&a), hash_of(&b));
                }
            }
        }
    }

    #[test]
    fn logical_size_eq_and_hash_agree_including_nan() {
        for w in HOSTILE {
            for h in HOSTILE {
                let a = LogicalSize::new(w, h);
                let b = LogicalSize::new(w, h);
                assert_eq!(a, b);
                assert_eq!(a.cmp(&b), Ordering::Equal);
                assert_eq!(hash_of(&a), hash_of(&b));
            }
        }
    }

    #[test]
    fn logical_rect_eq_and_hash_are_quantized_through_its_fields() {
        // LogicalRect's derived PartialEq/Hash must inherit the quantized field
        // impls — a NaN-sized rect has to be a stable HashMap key.
        let a = LogicalRect::new(
            LogicalPosition::new(f32::NAN, 1.0),
            LogicalSize::new(f32::NAN, 2.0),
        );
        let b = a;
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));

        // Sub-millipixel jitter must not create a new key.
        let c = LogicalRect::new(
            LogicalPosition::new(1.0, 2.0),
            LogicalSize::new(3.0, 4.0),
        );
        let d = LogicalRect::new(
            LogicalPosition::new(1.00004, 2.00004),
            LogicalSize::new(3.00004, 4.00004),
        );
        assert_eq!(c, d);
        assert_eq!(hash_of(&c), hash_of(&d));
    }

    // ---------------------------------------------------------------------
    // Constructors / zero neutrality
    // ---------------------------------------------------------------------

    #[test]
    fn constructors_preserve_fields_for_extreme_arguments() {
        for x in HOSTILE {
            for y in HOSTILE {
                let p = LogicalPosition::new(x, y);
                assert_eq!(p.x.to_bits(), x.to_bits());
                assert_eq!(p.y.to_bits(), y.to_bits());

                let s = LogicalSize::new(x, y);
                assert_eq!(s.width.to_bits(), x.to_bits());
                assert_eq!(s.height.to_bits(), y.to_bits());

                let r = LogicalRect::new(p, s);
                assert_eq!(r.origin.x.to_bits(), x.to_bits());
                assert_eq!(r.size.height.to_bits(), y.to_bits());

                assert_eq!(ScreenPosition::new(x, y).x.to_bits(), x.to_bits());
                assert_eq!(CursorNodePosition::new(x, y).y.to_bits(), y.to_bits());
                assert_eq!(PhysicalPosition::new(x, y).x.to_bits(), x.to_bits());
                assert_eq!(PhysicalSize::new(x, y).height.to_bits(), y.to_bits());
            }
        }
    }

    #[test]
    fn zero_constructors_are_neutral_and_match_default() {
        assert_eq!(LogicalPosition::zero(), LogicalPosition::default());
        assert_eq!(LogicalSize::zero(), LogicalSize::default());
        assert_eq!(LogicalRect::zero(), LogicalRect::default());
        assert_eq!(LogicalRect::zero().origin, LogicalPosition::zero());
        assert_eq!(LogicalRect::zero().size, LogicalSize::zero());

        assert_eq!(ScreenPosition::zero(), ScreenPosition::default());
        assert_eq!(CursorNodePosition::zero(), CursorNodePosition::default());

        assert_eq!(PhysicalPosition::<i32>::zero(), PhysicalPosition::new(0, 0));
        assert_eq!(
            PhysicalPosition::<f64>::zero(),
            PhysicalPosition::new(0.0_f64, 0.0_f64)
        );
        assert_eq!(PhysicalSize::<u32>::zero(), PhysicalSize::new(0, 0));

        // A zero rect is degenerate: it contains no point at all, not even its
        // own origin, and it does not intersect itself.
        let z = LogicalRect::zero();
        assert!(!z.contains(LogicalPosition::zero()));
        assert!(!z.intersects(z));
        assert_eq!(z.min_x(), 0.0);
        assert_eq!(z.max_x(), 0.0);
        assert_eq!(z.min_y(), 0.0);
        assert_eq!(z.max_y(), 0.0);
    }

    // ---------------------------------------------------------------------
    // LogicalRect getters
    // ---------------------------------------------------------------------

    #[test]
    fn rect_getters_return_the_constructed_edges() {
        let r = LogicalRect::new(
            LogicalPosition::new(10.0, 20.0),
            LogicalSize::new(30.0, 40.0),
        );
        assert_eq!(r.min_x(), 10.0);
        assert_eq!(r.max_x(), 40.0);
        assert_eq!(r.min_y(), 20.0);
        assert_eq!(r.max_y(), 60.0);
    }

    #[test]
    fn rect_getters_do_not_panic_on_extreme_geometry() {
        for x in HOSTILE {
            for w in HOSTILE {
                let r = LogicalRect::new(
                    LogicalPosition::new(x, x),
                    LogicalSize::new(w, w),
                );
                // Pure reads: must never panic, whatever the float class.
                let _ = r.min_x();
                let _ = r.max_x();
                let _ = r.min_y();
                let _ = r.max_y();
            }
        }
        // inf + (-inf) is NaN — max_x has no guard, so it propagates NaN rather
        // than panicking. Assert the defined (non-panicking) result.
        let r = LogicalRect::new(
            LogicalPosition::new(f32::INFINITY, f32::INFINITY),
            LogicalSize::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
        );
        assert!(r.max_x().is_nan());
        assert!(r.max_y().is_nan());
    }

    // ---------------------------------------------------------------------
    // contains / hit_test / intersects
    // ---------------------------------------------------------------------

    #[test]
    fn contains_is_half_open_left_top_inclusive_right_bottom_exclusive() {
        let r = LogicalRect::new(
            LogicalPosition::new(10.0, 20.0),
            LogicalSize::new(30.0, 40.0),
        );
        assert!(r.contains(LogicalPosition::new(10.0, 20.0))); // top-left: in
        assert!(!r.contains(LogicalPosition::new(40.0, 59.0))); // right edge: out
        assert!(!r.contains(LogicalPosition::new(39.0, 60.0))); // bottom edge: out
        assert!(!r.contains(LogicalPosition::new(40.0, 60.0))); // bottom-right: out
        assert!(r.contains(LogicalPosition::new(39.999, 59.999)));
    }

    #[test]
    fn contains_and_hit_test_agree_on_the_hostile_grid() {
        // The invariant the hit_test comment promises: identical edge semantics.
        let rects = [
            LogicalRect::zero(),
            LogicalRect::new(LogicalPosition::new(10.0, 20.0), LogicalSize::new(30.0, 40.0)),
            LogicalRect::new(LogicalPosition::new(-5.0, -5.0), LogicalSize::new(10.0, 10.0)),
            // Negative extent: max < min, so it can never contain anything.
            LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(-10.0, -10.0)),
            LogicalRect::new(
                LogicalPosition::new(f32::NAN, f32::NAN),
                LogicalSize::new(f32::NAN, f32::NAN),
            ),
            LogicalRect::new(
                LogicalPosition::zero(),
                LogicalSize::new(f32::INFINITY, f32::INFINITY),
            ),
        ];
        for r in rects {
            for x in HOSTILE {
                for y in HOSTILE {
                    let p = LogicalPosition::new(x, y);
                    assert_eq!(
                        r.contains(p),
                        r.hit_test(&p).is_some(),
                        "contains/hit_test disagree for {r:?} at {p:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn contains_rejects_nan_points_and_nan_rects() {
        let r = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        );
        // Every comparison against NaN is false, so a NaN point is never inside.
        assert!(!r.contains(LogicalPosition::new(f32::NAN, 50.0)));
        assert!(!r.contains(LogicalPosition::new(50.0, f32::NAN)));
        assert!(!r.contains(LogicalPosition::new(f32::NAN, f32::NAN)));

        let nan_rect = LogicalRect::new(
            LogicalPosition::new(f32::NAN, f32::NAN),
            LogicalSize::new(f32::NAN, f32::NAN),
        );
        assert!(!nan_rect.contains(LogicalPosition::zero()));
        assert!(nan_rect.hit_test(&LogicalPosition::zero()).is_none());
    }

    #[test]
    fn contains_handles_negative_extent_rects_without_panicking() {
        // A negative width puts max_x below min_x: nothing can satisfy both bounds.
        let r = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(-10.0, -10.0),
        );
        assert!(!r.contains(LogicalPosition::zero()));
        assert!(!r.contains(LogicalPosition::new(-5.0, -5.0)));
        assert!(r.hit_test(&LogicalPosition::new(-5.0, -5.0)).is_none());
    }

    #[test]
    fn contains_at_the_coordinate_extremes() {
        let huge = LogicalRect::new(
            LogicalPosition::new(f32::MIN, f32::MIN),
            LogicalSize::new(f32::MAX, f32::MAX),
        );
        // f32::MIN + f32::MAX == 0.0 exactly, so the rect spans [MIN, 0).
        assert_eq!(huge.max_x(), 0.0);
        assert!(huge.contains(LogicalPosition::new(-1.0, -1.0)));
        assert!(!huge.contains(LogicalPosition::zero()));

        let unbounded = LogicalRect::new(
            LogicalPosition::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
            LogicalSize::new(f32::INFINITY, f32::INFINITY),
        );
        // -inf + inf == NaN, so the "infinite" rect contains nothing. Surprising,
        // but defined and panic-free.
        assert!(unbounded.max_x().is_nan());
        assert!(!unbounded.contains(LogicalPosition::zero()));
    }

    #[test]
    fn hit_test_returns_the_offset_from_the_top_left_corner() {
        let r = LogicalRect::new(
            LogicalPosition::new(10.0, 20.0),
            LogicalSize::new(30.0, 40.0),
        );
        assert_eq!(
            r.hit_test(&LogicalPosition::new(10.0, 20.0)),
            Some(LogicalPosition::new(0.0, 0.0))
        );
        assert_eq!(
            r.hit_test(&LogicalPosition::new(25.0, 45.0)),
            Some(LogicalPosition::new(15.0, 25.0))
        );
        // Right/bottom edges are exclusive.
        assert_eq!(r.hit_test(&LogicalPosition::new(40.0, 30.0)), None);
        assert_eq!(r.hit_test(&LogicalPosition::new(30.0, 60.0)), None);
    }

    #[test]
    fn hit_test_offset_is_always_non_negative_when_it_hits() {
        let r = LogicalRect::new(
            LogicalPosition::new(-100.0, -100.0),
            LogicalSize::new(200.0, 200.0),
        );
        // Dyadic values only: `origin + offset` must reconstruct the point exactly,
        // so the assertion tests hit_test's arithmetic and not f32 rounding.
        for x in [-100.0_f32, -50.0, 0.0, 50.0, 99.5] {
            for y in [-100.0_f32, -50.0, 0.0, 50.0, 99.5] {
                let hit = r.hit_test(&LogicalPosition::new(x, y)).expect("inside");
                assert!(hit.x >= 0.0 && hit.y >= 0.0, "negative offset {hit:?}");
                assert_eq!(r.origin.x + hit.x, x);
                assert_eq!(r.origin.y + hit.y, y);
            }
        }
    }

    #[test]
    fn intersects_is_symmetric_even_for_degenerate_and_nan_rects() {
        let rects = [
            LogicalRect::zero(),
            LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(10.0, 10.0)),
            LogicalRect::new(LogicalPosition::new(5.0, 5.0), LogicalSize::new(10.0, 10.0)),
            LogicalRect::new(LogicalPosition::new(10.0, 0.0), LogicalSize::new(10.0, 10.0)),
            LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(-10.0, -10.0)),
            LogicalRect::new(
                LogicalPosition::new(f32::NAN, f32::NAN),
                LogicalSize::new(f32::NAN, f32::NAN),
            ),
            LogicalRect::new(
                LogicalPosition::new(f32::MIN, f32::MIN),
                LogicalSize::new(f32::MAX, f32::MAX),
            ),
        ];
        for a in rects {
            for b in rects {
                assert_eq!(
                    a.intersects(b),
                    b.intersects(a),
                    "intersects is asymmetric for {a:?} / {b:?}"
                );
            }
        }
    }

    #[test]
    fn intersects_touching_edges_do_not_count_as_overlap() {
        let a = LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(10.0, 10.0));
        let touching = LogicalRect::new(
            LogicalPosition::new(10.0, 0.0),
            LogicalSize::new(10.0, 10.0),
        );
        let overlapping = LogicalRect::new(
            LogicalPosition::new(9.99, 0.0),
            LogicalSize::new(10.0, 10.0),
        );
        assert!(!a.intersects(touching));
        assert!(a.intersects(overlapping));
        assert!(a.intersects(a));
        // Zero-area rects never overlap anything, including themselves.
        assert!(!LogicalRect::zero().intersects(a));
    }

    #[test]
    fn intersects_with_nan_rect_is_permissive_current_behavior() {
        // DOCUMENTS A REAL QUIRK (reported, not worked around): every `<=` guard
        // in `intersects` is false against NaN, so all four early-outs are skipped
        // and a fully-NaN rect reports that it intersects EVERYTHING — while
        // `contains` on the same rect correctly reports false for every point.
        // Locked down here so a future fix has to change this deliberately.
        let nan_rect = LogicalRect::new(
            LogicalPosition::new(f32::NAN, f32::NAN),
            LogicalSize::new(f32::NAN, f32::NAN),
        );
        let normal = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(10.0, 10.0),
        );
        assert!(nan_rect.intersects(normal));
        assert!(normal.intersects(nan_rect));
        assert!(!nan_rect.contains(LogicalPosition::zero()));
    }

    // ---------------------------------------------------------------------
    // scale_for_dpi
    // ---------------------------------------------------------------------

    #[test]
    fn scale_for_dpi_by_one_is_the_identity() {
        let mut p = LogicalPosition::new(1.5, -2.5);
        p.scale_for_dpi(1.0);
        assert_eq!(p, LogicalPosition::new(1.5, -2.5));

        let mut s = LogicalSize::new(3.5, 4.5);
        assert_eq!(s.scale_for_dpi(1.0), LogicalSize::new(3.5, 4.5));

        let mut r = LogicalRect::new(
            LogicalPosition::new(1.0, 2.0),
            LogicalSize::new(3.0, 4.0),
        );
        r.scale_for_dpi(1.0);
        assert_eq!(
            r,
            LogicalRect::new(LogicalPosition::new(1.0, 2.0), LogicalSize::new(3.0, 4.0))
        );
    }

    #[test]
    fn scale_for_dpi_by_zero_collapses_to_the_origin() {
        let mut r = LogicalRect::new(
            LogicalPosition::new(10.0, 20.0),
            LogicalSize::new(30.0, 40.0),
        );
        r.scale_for_dpi(0.0);
        assert_eq!(r, LogicalRect::zero());
    }

    #[test]
    fn scale_for_dpi_by_negative_factor_mirrors_deterministically() {
        let mut r = LogicalRect::new(
            LogicalPosition::new(10.0, 20.0),
            LogicalSize::new(30.0, 40.0),
        );
        r.scale_for_dpi(-2.0);
        assert_eq!(
            r,
            LogicalRect::new(
                LogicalPosition::new(-20.0, -40.0),
                LogicalSize::new(-60.0, -80.0)
            )
        );
        // A mirrored rect has an inverted extent, so it contains nothing.
        assert!(!r.contains(LogicalPosition::new(-30.0, -50.0)));
    }

    #[test]
    fn scale_for_dpi_overflows_to_infinity_rather_than_panicking() {
        let mut s = LogicalSize::new(f32::MAX, f32::MAX);
        let out = s.scale_for_dpi(2.0);
        assert!(out.width.is_infinite() && out.width.is_sign_positive());
        assert!(out.height.is_infinite());
        // scale_for_dpi mutates in place AND returns a copy: they must match.
        assert_eq!(out, s);
    }

    #[test]
    fn scale_for_dpi_with_nan_or_inf_does_not_panic() {
        for factor in HOSTILE {
            let mut p = LogicalPosition::new(1.0, -1.0);
            p.scale_for_dpi(factor);

            let mut s = LogicalSize::new(1.0, -1.0);
            let _ = s.scale_for_dpi(factor);

            let mut r = LogicalRect::new(
                LogicalPosition::new(1.0, -1.0),
                LogicalSize::new(2.0, -2.0),
            );
            r.scale_for_dpi(factor);
        }
        // 0.0 * inf is the classic NaN trap: a zero-origin rect scaled by an
        // infinite DPI factor yields NaN coordinates, not zeros.
        let mut r = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(1.0, 1.0));
        r.scale_for_dpi(f32::INFINITY);
        assert!(r.origin.x.is_nan());
        assert!(r.size.width.is_infinite());
    }

    // ---------------------------------------------------------------------
    // DPI conversion: to_physical / to_logical
    // ---------------------------------------------------------------------

    #[test]
    fn to_physical_rounds_half_away_from_zero() {
        assert_eq!(
            LogicalPosition::new(0.5, 1.5).to_physical(1.0),
            PhysicalPosition::new(1, 2)
        );
        // 2.5 -> 3 (round-half-away), NOT 2 (banker's rounding).
        assert_eq!(
            LogicalSize::new(2.5, 3.5).to_physical(1.0),
            PhysicalSize::new(3, 4)
        );
    }

    #[test]
    fn to_physical_clamps_negatives_to_zero_instead_of_wrapping() {
        // `as u32` saturates (Rust >= 1.45), so -1.0 must become 0, NOT u32::MAX.
        assert_eq!(
            LogicalPosition::new(-1.0, -1000.0).to_physical(1.0),
            PhysicalPosition::new(0, 0)
        );
        assert_eq!(
            LogicalSize::new(-0.6, -1.0).to_physical(2.0),
            PhysicalSize::new(0, 0)
        );
        assert_eq!(
            LogicalPosition::new(1.0, 1.0).to_physical(-1.0),
            PhysicalPosition::new(0, 0)
        );
    }

    #[test]
    fn to_physical_saturates_at_u32_max_on_overflow() {
        assert_eq!(
            LogicalSize::new(f32::MAX, f32::INFINITY).to_physical(1.0),
            PhysicalSize::new(u32::MAX, u32::MAX)
        );
        // x: 1e30 * 1e30 overflows f32 to +inf, then saturates at the cast.
        // y: 0.0 * 1e30 stays 0 — saturation must not smear across components.
        assert_eq!(
            LogicalPosition::new(1.0e30, 0.0).to_physical(1.0e30),
            PhysicalPosition::new(u32::MAX, 0)
        );
    }

    #[test]
    fn to_physical_maps_nan_to_zero() {
        // `NaN as u32` == 0 by the saturating-cast rules. Defined, not UB.
        assert_eq!(
            LogicalPosition::new(f32::NAN, f32::NAN).to_physical(1.0),
            PhysicalPosition::new(0, 0)
        );
        assert_eq!(
            LogicalSize::new(f32::NAN, 5.0).to_physical(f32::NAN),
            PhysicalSize::new(0, 0)
        );
        // 0.0 * inf == NaN -> 0
        assert_eq!(
            LogicalSize::new(0.0, 0.0).to_physical(f32::INFINITY),
            PhysicalSize::new(0, 0)
        );
    }

    #[test]
    fn to_physical_never_panics_on_the_hostile_grid() {
        for v in HOSTILE {
            for f in HOSTILE {
                let _ = LogicalPosition::new(v, v).to_physical(f);
                let _ = LogicalSize::new(v, v).to_physical(f);
            }
        }
    }

    #[test]
    fn to_logical_divides_by_the_dpi_factor() {
        assert_eq!(
            PhysicalSize::new(200_u32, 100).to_logical(2.0),
            LogicalSize::new(100.0, 50.0)
        );
        assert_eq!(
            PhysicalPosition::new(-10_i32, 20).to_logical(2.0),
            LogicalPosition::new(-5.0, 10.0)
        );
        assert_eq!(
            PhysicalPosition::new(-10.0_f64, 20.0).to_logical(2.0),
            LogicalPosition::new(-5.0, 10.0)
        );
    }

    #[test]
    fn to_logical_with_zero_dpi_yields_infinity_not_a_panic() {
        // Float division by zero is defined: no divide-by-zero panic here.
        let s = PhysicalSize::new(100_u32, 100).to_logical(0.0);
        assert!(s.width.is_infinite() && s.width.is_sign_positive());

        // 0 / 0 == NaN.
        let z = PhysicalSize::<u32>::zero().to_logical(0.0);
        assert!(z.width.is_nan() && z.height.is_nan());

        let p = PhysicalPosition::new(-5_i32, 5).to_logical(0.0);
        assert!(p.x.is_infinite() && p.x.is_sign_negative());
        assert!(p.y.is_infinite() && p.y.is_sign_positive());
    }

    #[test]
    fn to_logical_at_the_integer_limits() {
        let p = PhysicalPosition::new(i32::MIN, i32::MAX).to_logical(1.0);
        assert_eq!(p.x, i32::MIN as f32);
        assert_eq!(p.y, i32::MAX as f32);

        let s = PhysicalSize::new(u32::MAX, 0_u32).to_logical(1.0);
        assert_eq!(s.width, u32::MAX as f32);
        assert_eq!(s.height, 0.0);

        // f64 -> f32 narrowing saturates to inf rather than wrapping.
        let big = PhysicalPosition::new(f64::MAX, f64::MIN).to_logical(1.0);
        assert!(big.x.is_infinite() && big.x.is_sign_positive());
        assert!(big.y.is_infinite() && big.y.is_sign_negative());
    }

    #[test]
    fn to_logical_never_panics_for_hostile_dpi_factors() {
        for f in HOSTILE {
            let _ = PhysicalPosition::new(i32::MIN, i32::MAX).to_logical(f);
            let _ = PhysicalPosition::new(f64::MAX, f64::MIN).to_logical(f);
            let _ = PhysicalSize::new(u32::MAX, 0_u32).to_logical(f);
        }
    }

    // ---------------------------------------------------------------------
    // Round-trips
    // ---------------------------------------------------------------------

    #[test]
    fn logical_size_physical_round_trip_is_lossless_for_integral_pixels() {
        for factor in [1.0_f32, 2.0, 4.0] {
            for (w, h) in [(0.0_f32, 0.0_f32), (1.0, 1.0), (100.0, 50.0), (1920.0, 1080.0)] {
                let original = LogicalSize::new(w, h);
                let round_tripped = original.to_physical(factor).to_logical(factor);
                assert_eq!(
                    original, round_tripped,
                    "round-trip lost {original:?} at dpi {factor}"
                );
            }
        }
    }

    #[test]
    fn physical_size_logical_round_trip_preserves_the_pixel_count() {
        for factor in [1.0_f32, 1.5, 2.0, 3.0] {
            for (w, h) in [(0_u32, 0_u32), (1, 1), (1920, 1080), (3840, 2160)] {
                let original = PhysicalSize::new(w, h);
                let round_tripped = original.to_logical(factor).to_physical(factor);
                assert_eq!(
                    original, round_tripped,
                    "round-trip lost {original:?} at dpi {factor}"
                );
            }
        }
    }

    #[test]
    fn screen_and_cursor_position_logical_round_trip_bit_for_bit() {
        for x in HOSTILE {
            for y in HOSTILE {
                let p = LogicalPosition::new(x, y);

                let screen = ScreenPosition::from_logical(p).to_logical();
                assert_eq!(screen.x.to_bits(), x.to_bits());
                assert_eq!(screen.y.to_bits(), y.to_bits());

                let cursor = CursorNodePosition::from_logical(p).to_logical();
                assert_eq!(cursor.x.to_bits(), x.to_bits());
                assert_eq!(cursor.y.to_bits(), y.to_bits());
            }
        }
    }

    #[test]
    fn add_sub_are_inverse_for_finite_positions() {
        let a = LogicalPosition::new(10.0, -20.0);
        let b = LogicalPosition::new(2.5, 7.5);
        assert_eq!((a + b) - b, a);

        let mut c = a;
        c += b;
        assert_eq!(c, a + b);
        c -= b;
        assert_eq!(c, a);
    }

    // ---------------------------------------------------------------------
    // Writing-mode axis mapping
    // ---------------------------------------------------------------------

    #[test]
    fn position_main_cross_round_trip_for_every_writing_mode() {
        for wm in WMS {
            for main in HOSTILE {
                for cross in HOSTILE {
                    let p = LogicalPosition::from_main_cross(main, cross, wm);
                    assert_eq!(p.main(wm).to_bits(), main.to_bits());
                    assert_eq!(p.cross(wm).to_bits(), cross.to_bits());
                }
            }
        }
    }

    #[test]
    fn size_main_cross_round_trip_for_every_writing_mode() {
        for wm in WMS {
            for main in HOSTILE {
                for cross in HOSTILE {
                    let s = LogicalSize::from_main_cross(main, cross, wm);
                    assert_eq!(s.main(wm).to_bits(), main.to_bits());
                    assert_eq!(s.cross(wm).to_bits(), cross.to_bits());
                }
            }
        }
    }

    #[test]
    fn horizontal_tb_maps_main_to_the_block_axis() {
        // In horizontal-tb the block (main) axis is vertical: main == y / height.
        let wm = LayoutWritingMode::HorizontalTb;
        let p = LogicalPosition::new(3.0, 7.0);
        assert_eq!(p.main(wm), 7.0);
        assert_eq!(p.cross(wm), 3.0);

        let s = LogicalSize::new(30.0, 70.0);
        assert_eq!(s.main(wm), 70.0);
        assert_eq!(s.cross(wm), 30.0);
    }

    #[test]
    fn vertical_modes_map_main_to_the_horizontal_axis() {
        for wm in [LayoutWritingMode::VerticalRl, LayoutWritingMode::VerticalLr] {
            let p = LogicalPosition::new(3.0, 7.0);
            assert_eq!(p.main(wm), 3.0);
            assert_eq!(p.cross(wm), 7.0);

            let s = LogicalSize::new(30.0, 70.0);
            assert_eq!(s.main(wm), 30.0);
            assert_eq!(s.cross(wm), 70.0);
        }
    }

    #[test]
    fn with_main_and_with_cross_only_touch_their_own_axis() {
        for wm in WMS {
            for v in HOSTILE {
                let s = LogicalSize::new(10.0, 20.0);

                let m = s.with_main(wm, v);
                assert_eq!(m.main(wm).to_bits(), v.to_bits());
                assert_eq!(m.cross(wm), s.cross(wm), "with_main clobbered the cross axis");

                let c = s.with_cross(wm, v);
                assert_eq!(c.cross(wm).to_bits(), v.to_bits());
                assert_eq!(c.main(wm), s.main(wm), "with_cross clobbered the main axis");
            }
        }
    }

    #[test]
    fn with_main_then_with_cross_reconstructs_from_main_cross() {
        for wm in WMS {
            let built = LogicalSize::zero().with_main(wm, 5.0).with_cross(wm, 9.0);
            assert_eq!(built, LogicalSize::from_main_cross(5.0, 9.0, wm));
        }
    }

    // ---------------------------------------------------------------------
    // Display / Debug (serializers)
    // ---------------------------------------------------------------------

    #[test]
    fn display_formats_are_well_formed_for_representative_values() {
        let p = LogicalPosition::new(1.5, -2.5);
        assert_eq!(format!("{p}"), "(1.5, -2.5)");
        assert_eq!(format!("{p:?}"), "(1.5, -2.5)");

        let s = LogicalSize::new(30.0, 40.0);
        assert_eq!(format!("{s}"), "30x40");
        assert_eq!(format!("{s:?}"), "30x40");

        let r = LogicalRect::new(p, s);
        assert_eq!(format!("{r}"), "30x40 @ (1.5, -2.5)");
        assert_eq!(format!("{r:?}"), "30x40 @ (1.5, -2.5)");

        assert_eq!(format!("{:?}", PhysicalPosition::new(1_i32, 2)), "(1, 2)");
        assert_eq!(format!("{:?}", PhysicalSize::new(1_u32, 2)), "1x2");
    }

    #[test]
    fn display_of_zero_values_is_non_empty() {
        assert!(!format!("{}", LogicalPosition::zero()).is_empty());
        assert!(!format!("{}", LogicalSize::zero()).is_empty());
        assert!(!format!("{}", LogicalRect::zero()).is_empty());
        assert_eq!(format!("{}", LogicalRect::zero()), "0x0 @ (0, 0)");
    }

    #[test]
    fn display_does_not_panic_on_nan_or_infinite_coordinates() {
        for x in HOSTILE {
            for y in HOSTILE {
                let r = LogicalRect::new(
                    LogicalPosition::new(x, y),
                    LogicalSize::new(x, y),
                );
                let shown = format!("{r}");
                assert!(!shown.is_empty());
                assert_eq!(shown, format!("{r:?}"));
            }
        }
        let nan = LogicalRect::new(
            LogicalPosition::new(f32::NAN, f32::INFINITY),
            LogicalSize::new(f32::NEG_INFINITY, f32::NAN),
        );
        assert_eq!(format!("{nan}"), "-infxNaN @ (NaN, inf)");
    }
}
