//! Basic geometry primitives for layout calculations.

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
impl_vec!(LayoutRect, LayoutRectVec, LayoutRectVecDestructor, LayoutRectVecDestructorType);
impl_vec_clone!(LayoutRect, LayoutRectVec, LayoutRectVecDestructor);
impl_vec_debug!(LayoutRect, LayoutRectVec);
impl_vec_mut!(LayoutRect, LayoutRectVec);
impl_vec_partialeq!(LayoutRect, LayoutRectVec);
impl_vec_partialord!(LayoutRect, LayoutRectVec);

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
        self.max_x() - self.min_x()
    }
    #[inline(always)]
    pub const fn height(&self) -> isize {
        self.max_y() - self.min_y()
    }

    pub const fn contains(&self, other: &LayoutPoint) -> bool {
        self.min_x() <= other.x
            && other.x < self.max_x()
            && self.min_y() <= other.y
            && other.y < self.max_y()
    }

    pub fn contains_f32(&self, other_x: f32, other_y: f32) -> bool {
        self.min_x() as f32 <= other_x
            && other_x < self.max_x() as f32
            && self.min_y() as f32 <= other_y
            && other_y < self.max_y() as f32
    }

    /// Same as `contains()`, but returns the (x, y) offset of the hit point
    ///
    /// On a regular computer this function takes ~3.2ns to run
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

    /// Faster union for a Vec<LayoutRect>
    #[inline]
    pub fn union<I: Iterator<Item = Self>>(mut rects: I) -> Option<Self> {
        let first = rects.next()?;

        let mut max_width = first.size.width;
        let mut max_height = first.size.height;
        let mut min_x = first.origin.x;
        let mut min_y = first.origin.y;

        while let Some(Self {
            origin: LayoutPoint { x, y },
            size: LayoutSize { width, height },
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
            origin: LayoutPoint { x: min_x, y: min_y },
            size: LayoutSize {
                width: max_width,
                height: max_height,
            },
        })
    }

    // Returns the scroll rect (not the union rect) of the parent / children
    #[inline]
    pub fn get_scroll_rect<I: Iterator<Item = Self>>(&self, children: I) -> Option<Self> {
        let children_union = Self::union(children)?;
        Self::union([*self, children_union].iter().map(|r| *r))
    }

    // Returns if b overlaps a
    #[inline(always)]
    pub const fn contains_rect(&self, b: &LayoutRect) -> bool {
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
