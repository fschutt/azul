// Re-export DragDelta from drag module (moved in code reorganization)
pub use crate::drag::{DragDelta, OptionDragDelta};

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
    pub const fn zero() -> Self {
        Self::new(LogicalPosition::zero(), LogicalSize::zero())
    }
    pub const fn new(origin: LogicalPosition, size: LogicalSize) -> Self {
        Self { origin, size }
    }

    #[inline]
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.origin.x *= scale_factor;
        self.origin.y *= scale_factor;
        self.size.width *= scale_factor;
        self.size.height *= scale_factor;
    }

    #[inline(always)]
    pub fn max_x(&self) -> f32 {
        self.origin.x + self.size.width
    }
    #[inline(always)]
    pub fn min_x(&self) -> f32 {
        self.origin.x
    }
    #[inline(always)]
    pub fn max_y(&self) -> f32 {
        self.origin.y + self.size.height
    }
    #[inline(always)]
    pub fn min_y(&self) -> f32 {
        self.origin.y
    }

    /// Returns whether this rectangle intersects with another rectangle
    #[inline]
    pub fn intersects(&self, other: Self) -> bool {
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
    pub fn contains(&self, point: LogicalPosition) -> bool {
        point.x >= self.min_x()
            && point.x < self.max_x()
            && point.y >= self.min_y()
            && point.y < self.max_y()
    }

    /// Faster union for a Vec<LayoutRect>
    #[inline]
    pub fn union<I: Iterator<Item = Self>>(mut rects: I) -> Option<Self> {
        let first = rects.next()?;

        let mut max_width = first.size.width;
        let mut max_height = first.size.height;
        let mut min_x = first.origin.x;
        let mut min_y = first.origin.y;

        while let Some(Self {
            origin: LogicalPosition { x, y },
            size: LogicalSize { width, height },
        }) = rects.next()
        {
            let cur_lower_right_x = x + width;
            let cur_lower_right_y = y + height;
            max_width = max_width.max(cur_lower_right_x - min_x);
            max_height = max_height.max(cur_lower_right_y - min_y);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
        }

        Some(Self {
            origin: LogicalPosition { x: min_x, y: min_y },
            size: LogicalSize {
                width: max_width,
                height: max_height,
            },
        })
    }

    /// Same as `contains()`, but returns the (x, y) offset of the hit point
    ///
    /// On a regular computer this function takes ~3.2ns to run
    #[inline]
    pub fn hit_test(&self, other: &LogicalPosition) -> Option<LogicalPosition> {
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

    pub fn to_layout_rect(&self) -> LayoutRect {
        LayoutRect {
            origin: LayoutPoint::new(
                libm::roundf(self.origin.x) as isize,
                libm::roundf(self.origin.y) as isize,
            ),
            size: LayoutSize::new(
                libm::roundf(self.size.width) as isize,
                libm::roundf(self.size.height) as isize,
            ),
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

use azul_css::props::{
    basic::{LayoutPoint, LayoutRect, LayoutSize},
    layout::LayoutWritingMode,
};

#[derive(Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LogicalPosition {
    pub x: f32,
    pub y: f32,
}

impl LogicalPosition {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.x *= scale_factor;
        self.y *= scale_factor;
    }
}

impl SubAssign<LogicalPosition> for LogicalPosition {
    fn sub_assign(&mut self, other: LogicalPosition) {
        self.x -= other.x;
        self.y -= other.y;
    }
}

impl AddAssign<LogicalPosition> for LogicalPosition {
    fn add_assign(&mut self, other: LogicalPosition) {
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

const DECIMAL_MULTIPLIER: f32 = 1000.0;

impl_option!(
    LogicalPosition,
    OptionLogicalPosition,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

impl Ord for LogicalPosition {
    fn cmp(&self, other: &LogicalPosition) -> Ordering {
        let self_x = (self.x * DECIMAL_MULTIPLIER) as usize;
        let self_y = (self.y * DECIMAL_MULTIPLIER) as usize;
        let other_x = (other.x * DECIMAL_MULTIPLIER) as usize;
        let other_y = (other.y * DECIMAL_MULTIPLIER) as usize;
        self_x.cmp(&other_x).then(self_y.cmp(&other_y))
    }
}

impl Eq for LogicalPosition {}

impl Hash for LogicalPosition {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        let self_x = (self.x * DECIMAL_MULTIPLIER) as usize;
        let self_y = (self.y * DECIMAL_MULTIPLIER) as usize;
        self_x.hash(state);
        self_y.hash(state);
    }
}

impl LogicalPosition {
    pub fn main(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.y,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.x,
        }
    }

    pub fn cross(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.x,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.y,
        }
    }

    // Creates a LogicalPosition from main and cross axis dimensions.
    pub fn from_main_cross(main: f32, cross: f32, wm: LayoutWritingMode) -> Self {
        match wm {
            LayoutWritingMode::HorizontalTb => Self::new(cross, main),
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => Self::new(main, cross),
        }
    }
}

#[derive(Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LogicalSize {
    pub width: f32,
    pub height: f32,
}

impl LogicalSize {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) -> Self {
        self.width *= scale_factor;
        self.height *= scale_factor;
        *self
    }

    // Creates a LogicalSize from main and cross axis dimensions.
    pub fn from_main_cross(main: f32, cross: f32, wm: LayoutWritingMode) -> Self {
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
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

impl_option!(
    LogicalRect,
    OptionLogicalRect,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

impl Ord for LogicalSize {
    fn cmp(&self, other: &LogicalSize) -> Ordering {
        let self_width = (self.width * DECIMAL_MULTIPLIER) as usize;
        let self_height = (self.height * DECIMAL_MULTIPLIER) as usize;
        let other_width = (other.width * DECIMAL_MULTIPLIER) as usize;
        let other_height = (other.height * DECIMAL_MULTIPLIER) as usize;
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
        let self_width = (self.width * DECIMAL_MULTIPLIER) as usize;
        let self_height = (self.height * DECIMAL_MULTIPLIER) as usize;
        self_width.hash(state);
        self_height.hash(state);
    }
}

impl LogicalSize {
    pub fn main(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.height,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.width,
        }
    }

    pub fn cross(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.width,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.height,
        }
    }

    // Returns a new LogicalSize with the main-axis dimension updated.
    pub fn with_main(self, wm: LayoutWritingMode, value: f32) -> Self {
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

    pub fn with_cross(self, wm: LayoutWritingMode, value: f32) -> Self {
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
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

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
    #[inline(always)]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    #[inline(always)]
    pub fn to_physical(self, hidpi_factor: f32) -> PhysicalPosition<u32> {
        PhysicalPosition {
            x: (self.x * hidpi_factor) as u32,
            y: (self.y * hidpi_factor) as u32,
        }
    }
}

impl<T> PhysicalPosition<T> {
    #[inline(always)]
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl PhysicalPosition<i32> {
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0, 0)
    }
    #[inline(always)]
    pub fn to_logical(self, hidpi_factor: f32) -> LogicalPosition {
        LogicalPosition {
            x: self.x as f32 / hidpi_factor,
            y: self.y as f32 / hidpi_factor,
        }
    }
}

impl PhysicalPosition<f64> {
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    #[inline(always)]
    pub fn to_logical(self, hidpi_factor: f32) -> LogicalPosition {
        LogicalPosition {
            x: self.x as f32 / hidpi_factor,
            y: self.y as f32 / hidpi_factor,
        }
    }
}

impl LogicalSize {
    #[inline(always)]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    #[inline(always)]
    pub fn to_physical(self, hidpi_factor: f32) -> PhysicalSize<u32> {
        PhysicalSize {
            width: (self.width * hidpi_factor) as u32,
            height: (self.height * hidpi_factor) as u32,
        }
    }
}

impl<T> PhysicalSize<T> {
    #[inline(always)]
    pub const fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

impl PhysicalSize<u32> {
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0, 0)
    }
    #[inline(always)]
    pub fn to_logical(self, hidpi_factor: f32) -> LogicalSize {
        LogicalSize {
            width: self.width as f32 / hidpi_factor,
            height: self.height as f32 / hidpi_factor,
        }
    }
}

// =============================================================================
// CoordinateSpace - Debug marker for documenting coordinate system contexts
// =============================================================================
//
// This enum serves as DOCUMENTATION for which coordinate space a value is in.
// It does NOT enforce type-safety at compile time (no PhantomData generics).
// The purpose is to help developers understand and debug coordinate transformations.
//
// COORDINATE SPACES IN AZUL:
//
// 1. Window (absolute coordinates from window top-left)
//    - All layout primitives are initially computed in this space
//    - Origin: (0, 0) = top-left corner of the window content area
//    - Used by: Layout engine output, display list items before compositor
//
// 2. ScrollFrame (relative to scroll container origin)
//    - Used for primitives inside a WebRender scroll frame
//    - Origin: (0, 0) = top-left of scrollable content area
//    - Transformation: scroll_pos = window_pos - scroll_frame_origin
//    - The scroll_frame_origin is the Window-space position of the scroll frame
//
// 3. Parent (relative to parent node origin)  
//    - Used for relative positioning within a parent container
//    - Origin: (0, 0) = top-left of parent's content box
//
// 4. ReferenceFrame (relative to a CSS transform origin)
//    - Used for primitives inside a WebRender reference frame (transforms)
//    - Origin: Defined by the transform-origin property
//
// COMMON BUG PATTERN:
//
// The Y-offset bug in text areas was caused by passing Window-space coordinates
// to WebRender when it expected ScrollFrame-space coordinates. The scroll frame
// creates a new spatial node, so primitives must be offset by the frame origin.
//
// WRONG:  Push same offset for scroll frames (content appears at window position)
// RIGHT:  Push frame_origin as new offset (content positioned relative to frame)

/// Marker enum documenting which coordinate space a geometric value is in.
/// 
/// This is for documentation and debugging purposes only - it does not enforce
/// type safety at compile time. Use comments like `[CoordinateSpace::Window]`
/// or `[CoordinateSpace::ScrollFrame]` in code to document coordinate contexts.
/// 
/// See the module-level documentation above for details on each space.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum CoordinateSpace {
    /// Absolute coordinates from window top-left (0,0).
    /// Layout engine output is in this space.
    Window,
    
    /// Relative to scroll frame content origin.
    /// Transformation: scroll_pos = window_pos - scroll_frame_origin
    ScrollFrame,
    
    /// Relative to parent node's content box origin.
    Parent,
    
    /// Relative to a CSS transform reference frame origin.
    ReferenceFrame,
}

impl CoordinateSpace {
    /// Returns a human-readable description of this coordinate space.
    pub const fn description(&self) -> &'static str {
        match self {
            CoordinateSpace::Window => "Absolute window coordinates (layout engine output)",
            CoordinateSpace::ScrollFrame => "Relative to scroll frame origin (for WebRender scroll nodes)",
            CoordinateSpace::Parent => "Relative to parent node origin",
            CoordinateSpace::ReferenceFrame => "Relative to CSS transform origin",
        }
    }
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
    #[inline(always)]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    /// Convert to a raw LogicalPosition (for interop with existing code).
    #[inline(always)]
    pub const fn to_logical(self) -> LogicalPosition {
        LogicalPosition { x: self.x, y: self.y }
    }
    /// Create from a raw LogicalPosition that is known to be in screen space.
    #[inline(always)]
    pub const fn from_logical(p: LogicalPosition) -> Self {
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
    #[inline(always)]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    #[inline(always)]
    pub const fn to_logical(self) -> LogicalPosition {
        LogicalPosition { x: self.x, y: self.y }
    }
    #[inline(always)]
    pub const fn from_logical(p: LogicalPosition) -> Self {
        Self { x: p.x, y: p.y }
    }
}

impl_option!(
    CursorNodePosition,
    OptionCursorNodePosition,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);
