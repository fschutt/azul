//! Basic geometry primitives (`LayoutPoint`, `LayoutSize`, `LayoutRect`) for
//! layout calculations, using `isize` coordinates (as opposed to the `f32`-based
//! logical coordinates in `core::geom`).

use core::fmt;

use crate::impl_option;

/// Only used for calculations: Point coordinate (x, y) in layout space.
#[derive(Copy, Default, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct LayoutPoint {
    pub x: isize,
    pub y: isize,
}

impl fmt::Debug for LayoutPoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}
impl fmt::Display for LayoutPoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl LayoutPoint {
    #[inline(always)]
    pub const fn new(x: isize, y: isize) -> Self {
        Self { x, y }
    }
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0, 0)
    }
}

impl_option!(
    LayoutPoint,
    OptionLayoutPoint,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

/// Only used for calculations: Size (width, height) in layout space.
#[derive(Copy, Default, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct LayoutSize {
    pub width: isize,
    pub height: isize,
}

impl fmt::Debug for LayoutSize {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}
impl fmt::Display for LayoutSize {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl LayoutSize {
    #[inline(always)]
    pub const fn new(width: isize, height: isize) -> Self {
        Self { width, height }
    }
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(0, 0)
    }
    #[inline]
    pub fn round(width: f32, height: f32) -> Self {
        Self {
            width: libm::roundf(width) as isize,
            height: libm::roundf(height) as isize,
        }
    }
}

impl_option!(
    LayoutSize,
    OptionLayoutSize,
    [Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash]
);

/// Only used for calculations: Rectangle (x, y, width, height) in layout space.
#[derive(Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutRect {
    pub origin: LayoutPoint,
    pub size: LayoutSize,
}

impl_option!(
    LayoutRect,
    OptionLayoutRect,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);
impl fmt::Debug for LayoutRect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}
impl fmt::Display for LayoutRect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} @ {}", self.size, self.origin)
    }
}

impl LayoutRect {
    #[inline(always)]
    pub const fn new(origin: LayoutPoint, size: LayoutSize) -> Self {
        Self { origin, size }
    }
    #[inline(always)]
    pub const fn zero() -> Self {
        Self::new(LayoutPoint::zero(), LayoutSize::zero())
    }
    #[inline(always)]
    pub const fn max_x(&self) -> isize {
        self.origin.x + self.size.width
    }
    #[inline(always)]
    pub const fn min_x(&self) -> isize {
        self.origin.x
    }
    #[inline(always)]
    pub const fn max_y(&self) -> isize {
        self.origin.y + self.size.height
    }
    #[inline(always)]
    pub const fn min_y(&self) -> isize {
        self.origin.y
    }
    #[inline(always)]
    pub const fn width(&self) -> isize {
        self.size.width
    }
    #[inline(always)]
    pub const fn height(&self) -> isize {
        self.size.height
    }

    pub const fn contains(&self, other: &LayoutPoint) -> bool {
        self.min_x() <= other.x
            && other.x < self.max_x()
            && self.min_y() <= other.y
            && other.y < self.max_y()
    }

    /// Like `contains()`, but returns the (x, y) offset of the hit point
    /// relative to the rectangle origin. Unlike `contains()`, points exactly
    /// on the boundary are excluded (returns `None`).
    #[inline]
    pub const fn hit_test(&self, other: &LayoutPoint) -> Option<LayoutPoint> {
        let dx_left_edge = other.x - self.min_x();
        let dx_right_edge = self.max_x() - other.x;
        let dy_top_edge = other.y - self.min_y();
        let dy_bottom_edge = self.max_y() - other.y;
        if dx_left_edge > 0 && dx_right_edge > 0 && dy_top_edge > 0 && dy_bottom_edge > 0 {
            Some(LayoutPoint::new(dx_left_edge, dy_top_edge))
        } else {
            None
        }
    }

}
