//! Basic geometry primitives (`LayoutPoint`, `LayoutSize`, `LayoutRect`) for
//! layout calculations, using `isize` coordinates (as opposed to the `f32`-based
//! logical coordinates in `core::geom`).

use core::fmt;

use crate::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_vec_partialeq,
    impl_vec_partialord,
};

/// Only used for calculations: Point coordinate (x, y) in layout space.
#[derive(Copy, Default, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct LayoutPoint {
    pub x: isize,
    pub y: isize,
}

impl fmt::Debug for LayoutPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}
impl fmt::Display for LayoutPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl LayoutPoint {
    #[inline]
    #[must_use] pub const fn new(x: isize, y: isize) -> Self {
        Self { x, y }
    }
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(0, 0)
    }
}

impl_option!(
    LayoutPoint,
    OptionLayoutPoint,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd]
);

/// Only used for calculations: Size (width, height) in layout space.
#[derive(Copy, Default, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct LayoutSize {
    pub width: isize,
    pub height: isize,
}

impl fmt::Debug for LayoutSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}
impl fmt::Display for LayoutSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl LayoutSize {
    #[inline]
    #[must_use] pub const fn new(width: isize, height: isize) -> Self {
        Self { width, height }
    }
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(0, 0)
    }
    #[inline]
    #[must_use] pub fn round(width: f32, height: f32) -> Self {
        Self {
            width: crate::cast::f32_to_isize(libm::roundf(width)),
            height: crate::cast::f32_to_isize(libm::roundf(height)),
        }
    }
}

impl_option!(
    LayoutSize,
    OptionLayoutSize,
    [Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash]
);

/// Only used for calculations: Rectangle (x, y, width, height) in layout space.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct LayoutRect {
    pub origin: LayoutPoint,
    pub size: LayoutSize,
}

impl_option!(
    LayoutRect,
    OptionLayoutRect,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd]
);
impl_vec!(LayoutRect, LayoutRectVec, LayoutRectVecDestructor, LayoutRectVecDestructorType, LayoutRectVecSlice, OptionLayoutRect);
impl_vec_clone!(LayoutRect, LayoutRectVec, LayoutRectVecDestructor);
impl_vec_debug!(LayoutRect, LayoutRectVec);
impl_vec_mut!(LayoutRect, LayoutRectVec);
impl_vec_partialeq!(LayoutRect, LayoutRectVec);
impl_vec_partialord!(LayoutRect, LayoutRectVec);

impl fmt::Debug for LayoutRect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}
impl fmt::Display for LayoutRect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} @ {}", self.size, self.origin)
    }
}

impl LayoutRect {
    #[inline]
    #[must_use] pub const fn new(origin: LayoutPoint, size: LayoutSize) -> Self {
        Self { origin, size }
    }
    #[inline]
    #[must_use] pub const fn zero() -> Self {
        Self::new(LayoutPoint::zero(), LayoutSize::zero())
    }
    #[inline]
    #[must_use] pub const fn max_x(&self) -> isize {
        self.origin.x + self.size.width
    }
    #[inline]
    #[must_use] pub const fn min_x(&self) -> isize {
        self.origin.x
    }
    #[inline]
    #[must_use] pub const fn max_y(&self) -> isize {
        self.origin.y + self.size.height
    }
    #[inline]
    #[must_use] pub const fn min_y(&self) -> isize {
        self.origin.y
    }
    #[inline]
    #[must_use] pub const fn width(&self) -> isize {
        self.size.width
    }
    #[inline]
    #[must_use] pub const fn height(&self) -> isize {
        self.size.height
    }

    #[must_use] pub const fn contains(&self, other: &LayoutPoint) -> bool {
        self.min_x() <= other.x
            && other.x < self.max_x()
            && self.min_y() <= other.y
            && other.y < self.max_y()
    }

    #[must_use] pub fn contains_f32(&self, other_x: f32, other_y: f32) -> bool {
        crate::cast::isize_to_f32(self.min_x()) <= other_x
            && other_x < crate::cast::isize_to_f32(self.max_x())
            && crate::cast::isize_to_f32(self.min_y()) <= other_y
            && other_y < crate::cast::isize_to_f32(self.max_y())
    }

    /// Like `contains()`, but returns the (x, y) offset of the hit point
    /// relative to the rectangle origin. Unlike `contains()`, points exactly
    /// on the boundary are excluded (returns `None`).
    #[inline]
    #[must_use] pub const fn hit_test(&self, other: &LayoutPoint) -> Option<LayoutPoint> {
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

    /// Returns the bounding rectangle that covers every rectangle in the slice,
    /// or `OptionLayoutRect::None` if the slice is empty.
    #[inline]
    #[must_use] pub fn union(rects: LayoutRectVecSlice) -> OptionLayoutRect {
        let mut iter = rects.as_slice().iter().copied();
        let Some(first) = iter.next() else {
            return OptionLayoutRect::None;
        };

        let mut min_x = first.origin.x;
        let mut min_y = first.origin.y;
        let mut max_x = first.origin.x + first.size.width;
        let mut max_y = first.origin.y + first.size.height;

        for Self {
            origin: LayoutPoint { x, y },
            size: LayoutSize { width, height },
        } in iter
        {
            max_x = max_x.max(x + width);
            max_y = max_y.max(y + height);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
        }

        OptionLayoutRect::Some(Self {
            origin: LayoutPoint { x: min_x, y: min_y },
            size: LayoutSize {
                width: max_x - min_x,
                height: max_y - min_y,
            },
        })
    }

    /// Returns true if `b` is fully contained inside `self`.
    #[inline]
    #[must_use] pub const fn contains_rect(&self, b: &Self) -> bool {
        let a = self;

        let a_x = a.origin.x;
        let a_y = a.origin.y;
        let a_width = a.size.width;
        let a_height = a.size.height;

        let b_x = b.origin.x;
        let b_y = b.origin.y;
        let b_width = b.size.width;
        let b_height = b.size.height;

        b_x >= a_x
            && b_y >= a_y
            && b_x + b_width <= a_x + a_width
            && b_y + b_height <= a_y + a_height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: isize, y: isize, w: isize, h: isize) -> LayoutRect {
        LayoutRect::new(LayoutPoint::new(x, y), LayoutSize::new(w, h))
    }

    #[test]
    fn union_slice_returns_bounding_rect() {
        let vec: LayoutRectVec =
            alloc::vec![rect(0, 0, 10, 10), rect(20, -5, 5, 30), rect(-3, 15, 4, 4)].into();
        let slice = vec.as_c_slice();

        match LayoutRect::union(slice) {
            OptionLayoutRect::Some(r) => {
                assert_eq!(r, rect(-3, -5, 28, 30));
            }
            OptionLayoutRect::None => panic!("expected Some bounding rect"),
        }
    }

    #[test]
    fn union_empty_slice_returns_none() {
        let vec: LayoutRectVec = LayoutRectVec::new();
        let slice = vec.as_c_slice();
        assert!(matches!(LayoutRect::union(slice), OptionLayoutRect::None));
    }
}
