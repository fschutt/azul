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
        self.origin.x.saturating_add(self.size.width)
    }
    #[inline]
    #[must_use] pub const fn min_x(&self) -> isize {
        self.origin.x
    }
    #[inline]
    #[must_use] pub const fn max_y(&self) -> isize {
        self.origin.y.saturating_add(self.size.height)
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
        let dx_left_edge = other.x.saturating_sub(self.min_x());
        let dx_right_edge = self.max_x().saturating_sub(other.x);
        let dy_top_edge = other.y.saturating_sub(self.min_y());
        let dy_bottom_edge = self.max_y().saturating_sub(other.y);
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
        let mut max_x = first.origin.x.saturating_add(first.size.width);
        let mut max_y = first.origin.y.saturating_add(first.size.height);

        for Self {
            origin: LayoutPoint { x, y },
            size: LayoutSize { width, height },
        } in iter
        {
            max_x = max_x.max(x.saturating_add(width));
            max_y = max_y.max(y.saturating_add(height));
            min_x = min_x.min(x);
            min_y = min_y.min(y);
        }

        OptionLayoutRect::Some(Self {
            origin: LayoutPoint { x: min_x, y: min_y },
            size: LayoutSize {
                width: max_x.saturating_sub(min_x),
                height: max_y.saturating_sub(min_y),
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
            && b_x.saturating_add(b_width) <= a_x.saturating_add(a_width)
            && b_y.saturating_add(b_height) <= a_y.saturating_add(a_height)
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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal, clippy::cognitive_complexity)]
mod autotest_generated {
    use core::hash::{Hash, Hasher};

    use super::*;
    use crate::cast::{f32_to_isize, isize_to_f32};

    // ------------------------------------------------------------- helpers ---

    fn point(x: isize, y: isize) -> LayoutPoint {
        LayoutPoint::new(x, y)
    }

    fn size(w: isize, h: isize) -> LayoutSize {
        LayoutSize::new(w, h)
    }

    fn rect(x: isize, y: isize, w: isize, h: isize) -> LayoutRect {
        LayoutRect::new(point(x, y), size(w, h))
    }

    fn rect_vec(rects: &[LayoutRect]) -> LayoutRectVec {
        rects.to_vec().into()
    }

    /// FNV-1a, so the Hash/Eq agreement checks need no `std` hasher.
    struct FnvHasher(u64);
    impl Hasher for FnvHasher {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for b in bytes {
                self.0 ^= u64::from(*b);
                self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
            }
        }
    }

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = FnvHasher(0xcbf2_9ce4_8422_2325);
        v.hash(&mut h);
        h.finish()
    }

    // Inverse of the `Display` impls, used for the encode==decode round-trips.
    // `-` and digits never contain `x`, `(`, `)` or ` @ `, so the splits are
    // unambiguous for every `isize`, negatives and MIN/MAX included.
    fn parse_point(s: &str) -> LayoutPoint {
        let inner = s
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .expect("LayoutPoint should be parenthesised");
        let (x, y) = inner.split_once(", ").expect("LayoutPoint needs a `, `");
        LayoutPoint::new(x.parse().expect("x"), y.parse().expect("y"))
    }

    fn parse_size(s: &str) -> LayoutSize {
        let (w, h) = s.split_once('x').expect("LayoutSize needs an `x`");
        LayoutSize::new(w.parse().expect("width"), h.parse().expect("height"))
    }

    fn parse_rect(s: &str) -> LayoutRect {
        let (sz, origin) = s.split_once(" @ ").expect("LayoutRect needs a ` @ `");
        LayoutRect::new(parse_point(origin), parse_size(sz))
    }

    /// Every value that has ever broken an `isize` boundary check.
    const EXTREMES: [isize; 9] = [
        isize::MIN,
        isize::MIN + 1,
        -1_000_000,
        -1,
        0,
        1,
        1_000_000,
        isize::MAX - 1,
        isize::MAX,
    ];

    // =================================================== constructors ========

    #[test]
    fn point_new_stores_every_extreme_verbatim() {
        for x in EXTREMES {
            for y in EXTREMES {
                let p = LayoutPoint::new(x, y);
                assert_eq!(p.x, x);
                assert_eq!(p.y, y);
                assert_eq!(p, LayoutPoint::new(x, y), "construction is not stable");
            }
        }
    }

    #[test]
    fn size_new_stores_every_extreme_verbatim_including_negative_sizes() {
        // Nothing rejects a negative width/height: the type is a plain pair.
        for w in EXTREMES {
            for h in EXTREMES {
                let s = LayoutSize::new(w, h);
                assert_eq!(s.width, w);
                assert_eq!(s.height, h);
            }
        }
    }

    #[test]
    fn rect_new_stores_origin_and_size_verbatim() {
        let r = LayoutRect::new(point(isize::MIN, isize::MAX), size(isize::MAX, isize::MIN));
        assert_eq!(r.origin, point(isize::MIN, isize::MAX));
        assert_eq!(r.size, size(isize::MAX, isize::MIN));
        // The getters that cannot overflow must agree with the fields.
        assert_eq!(r.min_x(), isize::MIN);
        assert_eq!(r.min_y(), isize::MAX);
        assert_eq!(r.width(), isize::MAX);
        assert_eq!(r.height(), isize::MIN);
    }

    #[test]
    fn zero_constructors_are_the_neutral_element_and_match_default() {
        assert_eq!(LayoutPoint::zero(), LayoutPoint::new(0, 0));
        assert_eq!(LayoutPoint::zero(), LayoutPoint::default());
        assert_eq!(LayoutSize::zero(), LayoutSize::new(0, 0));
        assert_eq!(LayoutSize::zero(), LayoutSize::default());

        // LayoutRect has no `Default`, so `zero()` is the only neutral value.
        let z = LayoutRect::zero();
        assert_eq!(z.origin, LayoutPoint::zero());
        assert_eq!(z.size, LayoutSize::zero());
        assert_eq!(z.min_x(), 0);
        assert_eq!(z.max_x(), 0);
        assert_eq!(z.min_y(), 0);
        assert_eq!(z.max_y(), 0);
        assert_eq!(z.width(), 0);
        assert_eq!(z.height(), 0);
    }

    #[test]
    fn zero_rect_is_empty_it_contains_no_point_not_even_its_own_origin() {
        // max is exclusive, so a 0x0 rect is a true empty set for `contains`...
        let z = LayoutRect::zero();
        assert!(!z.contains(&LayoutPoint::zero()));
        assert!(!z.contains_f32(0.0, 0.0));
        assert_eq!(z.hit_test(&LayoutPoint::zero()), None);
        // ...but `contains_rect` uses inclusive edges, so it still contains itself.
        assert!(z.contains_rect(&z));
    }

    #[test]
    fn constructors_are_usable_in_const_context() {
        const P: LayoutPoint = LayoutPoint::new(isize::MIN, isize::MAX);
        const S: LayoutSize = LayoutSize::new(-1, -2);
        const R: LayoutRect = LayoutRect::new(P, S);
        const Z: LayoutRect = LayoutRect::zero();
        const W: isize = R.width();

        assert_eq!(P.x, isize::MIN);
        assert_eq!(S.height, -2);
        assert_eq!(R.origin, P);
        assert_eq!(W, -1);
        assert_eq!(Z, LayoutRect::new(LayoutPoint::zero(), LayoutSize::zero()));
    }

    // =================================================== serializers =========

    #[test]
    fn display_of_extremes_is_well_formed_and_debug_delegates_to_it() {
        for x in EXTREMES {
            for y in EXTREMES {
                let p = LayoutPoint::new(x, y);
                let s = LayoutSize::new(x, y);
                let r = LayoutRect::new(p, s);

                let p_str = alloc::format!("{p}");
                let s_str = alloc::format!("{s}");
                let r_str = alloc::format!("{r}");

                assert_eq!(p_str, alloc::format!("({x}, {y})"));
                assert_eq!(s_str, alloc::format!("{x}x{y}"));
                assert_eq!(r_str, alloc::format!("{x}x{y} @ ({x}, {y})"));

                assert!(!p_str.is_empty() && !s_str.is_empty() && !r_str.is_empty());
                // Debug is `write!(f, "{self}")` — it must not diverge from Display.
                assert_eq!(alloc::format!("{p:?}"), p_str);
                assert_eq!(alloc::format!("{s:?}"), s_str);
                assert_eq!(alloc::format!("{r:?}"), r_str);
            }
        }
    }

    #[test]
    fn display_of_the_zero_values_does_not_panic_and_is_canonical() {
        assert_eq!(alloc::format!("{}", LayoutPoint::zero()), "(0, 0)");
        assert_eq!(alloc::format!("{}", LayoutSize::zero()), "0x0");
        assert_eq!(alloc::format!("{}", LayoutRect::zero()), "0x0 @ (0, 0)");
        assert_eq!(alloc::format!("{:?}", LayoutRect::zero()), "0x0 @ (0, 0)");
    }

    #[test]
    fn display_ignores_format_flags_rather_than_panicking() {
        // The impls use `write!` and never forward width/precision; assert that
        // this is a no-op instead of a panic or a truncated/padded string.
        let p = point(1, -2);
        assert_eq!(alloc::format!("{p:>40}"), "(1, -2)");
        assert_eq!(alloc::format!("{p:.1}"), "(1, -2)");
        assert_eq!(alloc::format!("{:#?}", size(3, 4)), "3x4");
    }

    // =================================================== round-trip ==========

    #[test]
    fn display_round_trips_through_a_parser_for_every_extreme() {
        for a in EXTREMES {
            for b in EXTREMES {
                let p = LayoutPoint::new(a, b);
                let s = LayoutSize::new(a, b);
                let r = LayoutRect::new(p, s);

                assert_eq!(parse_point(&alloc::format!("{p}")), p, "point {p} decoded wrong");
                assert_eq!(parse_size(&alloc::format!("{s}")), s, "size {s} decoded wrong");
                assert_eq!(parse_rect(&alloc::format!("{r}")), r, "rect {r} decoded wrong");
            }
        }
    }

    #[test]
    fn display_round_trips_for_a_negative_size_rect_where_the_x_separator_is_ambiguous_looking() {
        // "-1x-2" must not be mis-split: only digits and `-` surround the `x`.
        let r = rect(-7, -8, -1, -2);
        assert_eq!(alloc::format!("{r}"), "-1x-2 @ (-7, -8)");
        assert_eq!(parse_rect("-1x-2 @ (-7, -8)"), r);
    }

    // =================================================== getters =============

    #[test]
    fn getters_return_the_construction_values() {
        let r = rect(3, -4, 10, 20);
        assert_eq!(r.min_x(), 3);
        assert_eq!(r.min_y(), -4);
        assert_eq!(r.max_x(), 13);
        assert_eq!(r.max_y(), 16);
        assert_eq!(r.width(), 10);
        assert_eq!(r.height(), 20);
    }

    #[test]
    fn max_minus_min_is_the_extent_whenever_the_sum_does_not_overflow() {
        for x in [isize::MIN, -1, 0, 1, isize::MAX] {
            for w in [-1_000, -1, 0, 1, 1_000] {
                // Skip the combinations that would overflow `origin + size`.
                let Some(expected_max) = x.checked_add(w) else {
                    continue;
                };
                let r = rect(x, x, w, w);
                assert_eq!(r.max_x(), expected_max);
                assert_eq!(r.max_y(), expected_max);
                assert_eq!(r.max_x() - r.min_x(), r.width());
                assert_eq!(r.max_y() - r.min_y(), r.height());
            }
        }
    }

    #[test]
    fn getters_survive_the_widest_non_overflowing_rect() {
        // origin = MIN, size = MAX => max = MIN + MAX = -1. This is the largest
        // rect representable without tripping the (unchecked) `origin + size` add.
        let r = rect(isize::MIN, isize::MIN, isize::MAX, isize::MAX);
        assert_eq!(r.min_x(), isize::MIN);
        assert_eq!(r.min_y(), isize::MIN);
        assert_eq!(r.max_x(), -1);
        assert_eq!(r.max_y(), -1);
        assert_eq!(r.width(), isize::MAX);
        assert_eq!(r.height(), isize::MAX);

        // It really does span (almost) the whole negative half-space...
        assert!(r.contains(&point(isize::MIN, isize::MIN)));
        assert!(r.contains(&point(-2, -2)));
        // ...and stops one short of zero, because max is exclusive.
        assert!(!r.contains(&point(-1, -1)));
        assert!(!r.contains(&point(0, 0)));
    }

    #[test]
    fn max_getters_do_not_overflow_when_the_size_is_zero() {
        let r = rect(isize::MAX, isize::MAX, 0, 0);
        assert_eq!(r.max_x(), isize::MAX);
        assert_eq!(r.max_y(), isize::MAX);

        let r = rect(isize::MIN, isize::MIN, 0, 0);
        assert_eq!(r.max_x(), isize::MIN);
        assert_eq!(r.max_y(), isize::MIN);
    }

    // KNOWN HAZARD (reported, not weakened): `max_x`/`max_y` are a plain `+` on
    // `isize`, so an out-of-range right/bottom edge now saturates instead of
    // panicking (debug) / wrapping (release). These two tests pin that.
    #[test]
    fn max_x_saturates_instead_of_overflowing() {
        let r = core::hint::black_box(rect(isize::MAX, 0, 1, 0));
        assert_eq!(r.max_x(), isize::MAX);
    }

    #[test]
    fn max_y_saturates_instead_of_overflowing() {
        let r = core::hint::black_box(rect(0, isize::MIN, 0, -1));
        assert_eq!(r.max_y(), isize::MIN);
    }

    // =================================================== contains ============

    #[test]
    fn contains_is_min_inclusive_and_max_exclusive_on_every_edge() {
        let r = rect(10, 20, 5, 5); // x in [10, 15), y in [20, 25)
        assert!(r.contains(&point(10, 20))); // top-left corner: inside
        assert!(r.contains(&point(14, 24))); // last interior cell
        assert!(!r.contains(&point(15, 24))); // right edge: outside
        assert!(!r.contains(&point(14, 25))); // bottom edge: outside
        assert!(!r.contains(&point(15, 25))); // bottom-right corner: outside
        assert!(!r.contains(&point(9, 20)));
        assert!(!r.contains(&point(10, 19)));
    }

    #[test]
    fn contains_handles_negative_coordinates_deterministically() {
        let r = rect(-10, -10, 5, 5); // x in [-10, -5)
        assert!(r.contains(&point(-10, -10)));
        assert!(r.contains(&point(-6, -6)));
        assert!(!r.contains(&point(-5, -5)));
        assert!(!r.contains(&point(-11, -10)));
    }

    #[test]
    fn a_negative_size_rect_contains_nothing() {
        // max < min, so the half-open interval is empty for every point.
        let r = rect(0, 0, -5, -5);
        for x in -8..8 {
            for y in -8..8 {
                assert!(!r.contains(&point(x, y)), "({x}, {y}) must not be inside {r}");
                assert_eq!(r.hit_test(&point(x, y)), None);
            }
        }
    }

    #[test]
    fn contains_does_not_panic_at_the_isize_extremes_it_can_reach() {
        // `max_x()` is only evaluated once `min_x <= other.x`, so a rect anchored
        // at MAX short-circuits to false for every smaller point.
        let r = rect(isize::MAX, isize::MAX, 1, 1);
        assert!(!r.contains(&point(0, 0)));
        assert!(!r.contains(&point(isize::MIN, isize::MIN)));

        let r = rect(isize::MIN, isize::MIN, 1, 1);
        assert!(r.contains(&point(isize::MIN, isize::MIN)));
        assert!(!r.contains(&point(isize::MAX, isize::MAX)));
        assert!(!r.contains(&point(isize::MIN + 1, isize::MIN)));
    }

    // KNOWN HAZARD (reported): a rect wide enough that `origin.x + width`
    // overflows no longer makes `contains` panic: the saturating `max_x()` clamps
    // the right edge to isize::MAX, so an interior point is still inside.
    #[test]
    fn contains_does_not_panic_on_a_rect_whose_right_edge_overflows() {
        let r = core::hint::black_box(rect(1, 0, isize::MAX, 10));
        let p = core::hint::black_box(point(5, 5));
        assert!(r.contains(&p));
    }

    // =================================================== contains_f32 ========

    #[test]
    fn contains_f32_matches_contains_on_integer_coordinates() {
        for r in [rect(0, 0, 10, 10), rect(-5, -5, 3, 4), rect(0, 0, 0, 0)] {
            for x in -8..=12_isize {
                for y in -8..=12_isize {
                    assert_eq!(
                        r.contains_f32(isize_to_f32(x), isize_to_f32(y)),
                        r.contains(&point(x, y)),
                        "{r} disagrees about ({x}, {y})"
                    );
                }
            }
        }
    }

    #[test]
    fn contains_f32_is_min_inclusive_max_exclusive_for_fractional_points() {
        let r = rect(0, 0, 10, 10);
        assert!(r.contains_f32(0.0, 0.0));
        assert!(r.contains_f32(-0.0, -0.0)); // negative zero is still >= 0.0
        assert!(r.contains_f32(9.999_999, 9.999_999));
        assert!(!r.contains_f32(10.0, 5.0)); // exactly on max: excluded
        assert!(!r.contains_f32(-0.000_001, 5.0));
        assert!(!r.contains_f32(5.0, 10.0));
    }

    #[test]
    fn contains_f32_returns_false_for_nan_and_never_panics() {
        let r = rect(0, 0, 10, 10);
        // Every comparison against NaN is false, so NaN can never be "inside".
        assert!(!r.contains_f32(f32::NAN, 5.0));
        assert!(!r.contains_f32(5.0, f32::NAN));
        assert!(!r.contains_f32(f32::NAN, f32::NAN));
        assert!(!r.contains_f32(-f32::NAN, 5.0));
        assert!(!r.contains_f32(f32::from_bits(0x7fc0_1234), 5.0));
    }

    #[test]
    fn contains_f32_treats_infinities_as_outside() {
        let r = rect(0, 0, 10, 10);
        assert!(!r.contains_f32(f32::INFINITY, 5.0));
        assert!(!r.contains_f32(f32::NEG_INFINITY, 5.0));
        assert!(!r.contains_f32(5.0, f32::INFINITY));
        assert!(!r.contains_f32(5.0, f32::NEG_INFINITY));
        assert!(!r.contains_f32(f32::MAX, f32::MAX));
        assert!(!r.contains_f32(f32::MIN, f32::MIN));
    }

    #[test]
    fn contains_f32_survives_the_widest_non_overflowing_rect() {
        let r = rect(isize::MIN, isize::MIN, isize::MAX, isize::MAX);
        assert!(r.contains_f32(-1.0e18, -1.0e18));
        assert!(!r.contains_f32(0.0, 0.0));
        assert!(!r.contains_f32(f32::INFINITY, f32::INFINITY));
    }

    /// KNOWN DIVERGENCE (reported): `contains_f32` casts the edges to `f32`, so
    /// above 2^24 the edges snap to the nearest representable float and the
    /// predicate disagrees with the exact-integer `contains`.
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn contains_f32_reports_a_point_left_of_the_rect_as_inside_past_2_pow_24() {
        const TWO_POW_40: isize = 1 << 40; // f32 spacing here is 2^17 = 131072

        // Left edge is one unit right of 2^40, but rounds *down* to 2^40 in f32.
        let r = rect(TWO_POW_40 + 1, 0, 1_000_000, 1_000_000);
        let p = point(TWO_POW_40, 1);

        assert!(!r.contains(&p), "exact integer math: the point is left of the rect");
        assert!(
            r.contains_f32(isize_to_f32(TWO_POW_40), 1.0),
            "f32 math: the rounded-down left edge swallows the point"
        );
        // The rounding is what drives it: both edges land on the same float.
        assert_eq!(isize_to_f32(TWO_POW_40 + 1), isize_to_f32(TWO_POW_40));
    }

    // `contains_f32` shares the saturating `max_x()` with `contains`, so an
    // overflowing right edge no longer panics — an interior point is inside.
    #[test]
    fn contains_f32_does_not_panic_on_a_rect_whose_right_edge_overflows() {
        let r = core::hint::black_box(rect(1, 0, isize::MAX, 10));
        assert!(r.contains_f32(core::hint::black_box(5.0), 5.0));
    }

    // =================================================== hit_test ============

    #[test]
    fn hit_test_excludes_every_boundary_and_returns_the_origin_relative_offset() {
        let r = rect(10, 20, 5, 5); // strict interior: x in (10, 15), y in (20, 25)
        assert_eq!(r.hit_test(&point(11, 21)), Some(point(1, 1)));
        assert_eq!(r.hit_test(&point(14, 24)), Some(point(4, 4)));

        // The documented difference from `contains`: the min edge is excluded.
        assert!(r.contains(&point(10, 20)));
        assert_eq!(r.hit_test(&point(10, 20)), None);
        assert_eq!(r.hit_test(&point(10, 22)), None);
        assert_eq!(r.hit_test(&point(12, 20)), None);
        // ...and so is the max edge, which `contains` also excludes.
        assert_eq!(r.hit_test(&point(15, 22)), None);
        assert_eq!(r.hit_test(&point(12, 25)), None);
    }

    #[test]
    fn hit_test_some_always_implies_contains_and_the_offset_is_exact() {
        for r in [rect(0, 0, 10, 10), rect(-5, -5, 3, 4), rect(2, 2, 1, 1), rect(0, 0, 0, 0)] {
            for x in -8..=12_isize {
                for y in -8..=12_isize {
                    let p = point(x, y);
                    let strictly_inside =
                        r.min_x() < x && x < r.max_x() && r.min_y() < y && y < r.max_y();
                    assert_eq!(
                        r.hit_test(&p).is_some(),
                        strictly_inside,
                        "{r} hit_test({p}) disagrees with the strict-interior predicate"
                    );
                    if let Some(offset) = r.hit_test(&p) {
                        assert_eq!(offset, point(x - r.min_x(), y - r.min_y()));
                        assert!(r.contains(&p), "hit_test hit a point outside contains()");
                        // The offset must be strictly inside the size, never negative.
                        assert!(offset.x > 0 && offset.x < r.width());
                        assert!(offset.y > 0 && offset.y < r.height());
                    }
                }
            }
        }
    }

    #[test]
    fn hit_test_of_a_one_by_one_rect_is_always_none_because_it_has_no_interior() {
        let r = rect(0, 0, 1, 1);
        assert!(r.contains(&point(0, 0)));
        for x in -2..=2 {
            for y in -2..=2 {
                assert_eq!(r.hit_test(&point(x, y)), None);
            }
        }
    }

    // `hit_test` computes all four edge deltas up front with saturating math, so
    // a far-away point or an overflowing right edge no longer panics. Hit-testing
    // is the mouse path — these were the two most reachable overflows in the file.
    #[test]
    fn hit_test_of_a_point_far_left_of_a_perfectly_ordinary_rect_is_none() {
        let r = core::hint::black_box(rect(0, 0, 10, 10));
        let p = core::hint::black_box(point(isize::MIN, 0));
        assert_eq!(r.hit_test(&p), None);
    }

    #[test]
    fn hit_test_of_a_rect_whose_right_edge_overflows_returns_the_interior_offset() {
        let r = core::hint::black_box(rect(1, 0, isize::MAX, 10));
        let p = core::hint::black_box(point(5, 5));
        assert_eq!(r.hit_test(&p), Some(point(4, 5)));
    }

    // =================================================== contains_rect =======

    #[test]
    fn contains_rect_is_reflexive_and_uses_inclusive_edges() {
        let a = rect(0, 0, 10, 10);
        assert!(a.contains_rect(&a));
        assert!(a.contains_rect(&rect(0, 0, 5, 5)));
        assert!(a.contains_rect(&rect(5, 5, 5, 5))); // flush with the far edge
        assert!(!a.contains_rect(&rect(5, 5, 6, 5))); // one past it
        assert!(!a.contains_rect(&rect(-1, 0, 5, 5)));
        assert!(!a.contains_rect(&rect(0, -1, 5, 5)));

        // Inclusive edges mean a degenerate rect *on* the far corner counts as
        // contained, even though `contains()` rejects that same corner point.
        assert!(a.contains_rect(&rect(10, 10, 0, 0)));
        assert!(!a.contains(&point(10, 10)));
    }

    #[test]
    fn contains_rect_is_not_symmetric() {
        let big = rect(0, 0, 10, 10);
        let small = rect(2, 2, 2, 2);
        assert!(big.contains_rect(&small));
        assert!(!small.contains_rect(&big));
    }

    #[test]
    fn contains_rect_wrongly_accepts_a_negative_size_rect_that_extends_far_outside() {
        // b's far edge is computed as b_x + b_width, which a negative width drags
        // *left* of a's left edge — so the "fully contained" check passes for a
        // rect that visually spans well outside `a`. Pinned, not endorsed.
        let a = rect(0, 0, 10, 10);
        let b = rect(5, 5, -100, -100);
        assert!(a.contains_rect(&b));
    }

    #[test]
    fn contains_rect_does_not_panic_on_the_extremes_it_can_reach() {
        let full = rect(0, 0, isize::MAX, isize::MAX);
        assert!(full.contains_rect(&full)); // 0 + MAX <= 0 + MAX
        assert!(full.contains_rect(&rect(0, 0, 0, 0)));
        assert!(!full.contains_rect(&rect(-1, 0, 0, 0)));

        // The MIN-anchored half-space does not contain the origin rect: its far
        // edge is MIN + MAX = -1, which is < 0.
        let half = rect(isize::MIN, isize::MIN, isize::MAX, isize::MAX);
        assert!(!half.contains_rect(&rect(0, 0, 0, 0)));
        assert!(half.contains_rect(&rect(isize::MIN, isize::MIN, 0, 0)));
    }

    // `b_x + b_width` and `a_x + a_width` now saturate, so an overflowing far
    // edge no longer panics: b saturates to the same isize::MAX edge as a.
    #[test]
    fn contains_rect_does_not_panic_when_the_inner_rects_far_edge_overflows() {
        let a = core::hint::black_box(rect(0, 0, isize::MAX, isize::MAX));
        let b = core::hint::black_box(rect(1, 1, isize::MAX, 1));
        assert!(a.contains_rect(&b));
    }

    // =================================================== union ===============

    #[test]
    fn union_of_a_single_rect_is_that_rect_even_at_the_extremes() {
        for r in [
            rect(0, 0, 0, 0),
            rect(-7, -8, 1, 2),
            rect(3, 4, -5, -6), // negative size survives the max-minus-min round-trip
            rect(isize::MIN, isize::MIN, isize::MAX, isize::MAX),
            rect(isize::MAX, isize::MAX, 0, 0),
        ] {
            let vec = rect_vec(&[r]);
            assert_eq!(
                LayoutRect::union(vec.as_c_slice()),
                OptionLayoutRect::Some(r),
                "union([{r}]) is not the identity"
            );
        }
    }

    #[test]
    fn union_is_idempotent_and_order_independent_for_well_formed_rects() {
        let a = rect(-3, 15, 4, 4);
        let b = rect(20, -5, 5, 30);

        let ab = rect_vec(&[a, b]);
        let ba = rect_vec(&[b, a]);
        assert_eq!(
            LayoutRect::union(ab.as_c_slice()),
            LayoutRect::union(ba.as_c_slice())
        );

        let aa = rect_vec(&[a, a, a]);
        assert_eq!(LayoutRect::union(aa.as_c_slice()), OptionLayoutRect::Some(a));
    }

    #[test]
    fn union_covers_every_input_rect() {
        let rects = [rect(0, 0, 10, 10), rect(20, -5, 5, 30), rect(-3, 15, 4, 4)];
        let vec = rect_vec(&rects);
        let OptionLayoutRect::Some(u) = LayoutRect::union(vec.as_c_slice()) else {
            panic!("expected Some for a non-empty slice");
        };
        for r in rects {
            assert!(u.contains_rect(&r), "{u} does not cover {r}");
        }
        // ...and it is tight: shrinking it by one on any side breaks the cover.
        let tight = rect(u.min_x() + 1, u.min_y(), u.width() - 1, u.height());
        assert!(rects.iter().any(|r| !tight.contains_rect(r)));
    }

    #[test]
    fn union_only_reads_the_slice_it_was_given() {
        let vec = rect_vec(&[rect(0, 0, 1, 1), rect(100, 100, 1, 1), rect(-100, -100, 1, 1)]);
        // A sub-range must not pull in the neighbouring rects.
        assert_eq!(
            LayoutRect::union(vec.as_c_slice_range(0, 1)),
            OptionLayoutRect::Some(rect(0, 0, 1, 1))
        );
        assert_eq!(
            LayoutRect::union(vec.as_c_slice_range(0, 2)),
            OptionLayoutRect::Some(rect(0, 0, 101, 101))
        );
        // An empty sub-range is the empty case, not a wild pointer read.
        assert!(LayoutRect::union(vec.as_c_slice_range(1, 1)).is_none());
    }

    #[test]
    fn union_of_an_empty_and_a_default_constructed_vec_is_none() {
        let empty = LayoutRectVec::new();
        assert!(empty.is_empty());
        assert_eq!(LayoutRect::union(empty.as_c_slice()), OptionLayoutRect::None);
        assert!(LayoutRect::union(LayoutRectVecSlice::empty()).is_none());
        assert_eq!(OptionLayoutRect::default(), OptionLayoutRect::None);
    }

    #[test]
    fn union_with_negative_size_rects_folds_them_into_a_smaller_box() {
        // A negative-size rect's "max" is *left of* its origin, so union tracks
        // (5, 5) as the far corner and never covers the origin at (10, 10).
        let vec = rect_vec(&[rect(10, 10, -5, -5), rect(0, 0, 2, 2)]);
        assert_eq!(
            LayoutRect::union(vec.as_c_slice()),
            OptionLayoutRect::Some(rect(0, 0, 5, 5))
        );
    }

    #[test]
    fn union_handles_all_negative_coordinates() {
        let vec = rect_vec(&[rect(-10, -10, 2, 2), rect(-30, -5, 1, 1)]);
        assert_eq!(
            LayoutRect::union(vec.as_c_slice()),
            OptionLayoutRect::Some(rect(-30, -10, 22, 6))
        );
    }

    #[test]
    fn union_survives_the_widest_non_overflowing_pair() {
        let vec = rect_vec(&[rect(isize::MIN, isize::MIN, 0, 0), rect(-1, -1, 0, 0)]);
        assert_eq!(
            LayoutRect::union(vec.as_c_slice()),
            OptionLayoutRect::Some(rect(isize::MIN, isize::MIN, isize::MAX, isize::MAX))
        );
    }

    // `union` does three `isize` operations — `x + width` per rect and `max - min`
    // for the result extent — all saturating now, so a bounding box exceeding
    // isize::MAX clamps the extent instead of panicking (debug) / wrapping (release).
    #[test]
    fn union_spanning_the_whole_isize_range_saturates_the_extent() {
        let vec = rect_vec(&[rect(isize::MIN, 0, 0, 0), rect(isize::MAX, 0, 0, 0)]);
        assert_eq!(
            LayoutRect::union(vec.as_c_slice()),
            OptionLayoutRect::Some(rect(isize::MIN, 0, isize::MAX, 0))
        );
    }

    #[test]
    fn union_of_a_rect_whose_far_edge_overflows_saturates() {
        let vec = rect_vec(&[rect(isize::MAX, 0, 1, 0)]);
        assert_eq!(
            LayoutRect::union(vec.as_c_slice()),
            OptionLayoutRect::Some(rect(isize::MAX, 0, 0, 0))
        );
    }

    // =================================================== LayoutSize::round ===

    #[test]
    fn round_of_zero_and_negative_zero_is_the_zero_size() {
        assert_eq!(LayoutSize::round(0.0, 0.0), LayoutSize::zero());
        assert_eq!(LayoutSize::round(-0.0, -0.0), LayoutSize::zero());
        assert_eq!(LayoutSize::round(0.0, -0.0), LayoutSize::zero());
    }

    #[test]
    fn round_goes_half_away_from_zero_not_half_to_even() {
        assert_eq!(LayoutSize::round(0.5, -0.5), size(1, -1));
        assert_eq!(LayoutSize::round(1.5, -1.5), size(2, -2));
        // 2.5 -> 3 (away from zero), NOT 2 (banker's rounding).
        assert_eq!(LayoutSize::round(2.5, -2.5), size(3, -3));
        assert_eq!(LayoutSize::round(3.5, -3.5), size(4, -4));
    }

    #[test]
    fn round_truncates_toward_zero_just_below_the_half() {
        // Largest f32 strictly below 0.5; must round to 0, not 1.
        let just_below_half = f32::from_bits(0x3eff_ffff);
        assert!(just_below_half < 0.5);
        assert_eq!(
            LayoutSize::round(just_below_half, -just_below_half),
            LayoutSize::zero()
        );
        assert_eq!(LayoutSize::round(0.49, -0.49), LayoutSize::zero());
        assert_eq!(LayoutSize::round(1.49, -1.49), size(1, -1));
    }

    #[test]
    fn round_of_nan_is_zero_and_does_not_panic() {
        assert_eq!(LayoutSize::round(f32::NAN, f32::NAN), LayoutSize::zero());
        assert_eq!(LayoutSize::round(f32::NAN, 5.0), size(0, 5));
        assert_eq!(LayoutSize::round(5.0, -f32::NAN), size(5, 0));
        assert_eq!(
            LayoutSize::round(f32::from_bits(0x7fc0_1234), 1.0),
            size(0, 1)
        );
    }

    #[test]
    fn round_saturates_the_infinities_to_the_isize_bounds() {
        assert_eq!(
            LayoutSize::round(f32::INFINITY, f32::NEG_INFINITY),
            size(isize::MAX, isize::MIN)
        );
        assert_eq!(
            LayoutSize::round(f32::NEG_INFINITY, f32::INFINITY),
            size(isize::MIN, isize::MAX)
        );
    }

    #[test]
    fn round_saturates_out_of_range_finite_floats_rather_than_wrapping() {
        assert_eq!(
            LayoutSize::round(f32::MAX, f32::MIN),
            size(isize::MAX, isize::MIN)
        );
        assert_eq!(LayoutSize::round(1.0e30, -1.0e30), size(isize::MAX, isize::MIN));
    }

    #[test]
    fn round_flushes_subnormals_and_tiny_magnitudes_to_zero() {
        assert_eq!(
            LayoutSize::round(f32::MIN_POSITIVE, -f32::MIN_POSITIVE),
            LayoutSize::zero()
        );
        assert_eq!(LayoutSize::round(f32::EPSILON, f32::from_bits(1)), LayoutSize::zero());
    }

    #[test]
    fn round_is_exact_for_values_inside_the_f32_integer_range() {
        assert_eq!(LayoutSize::round(1.0e9, -1.0e9), size(1_000_000_000, -1_000_000_000));
        assert_eq!(LayoutSize::round(16_777_216.0, -16_777_216.0), size(1 << 24, -(1 << 24)));
        assert_eq!(LayoutSize::round(-1.0, 1.0), size(-1, 1));
    }

    #[test]
    fn round_agrees_with_roundf_then_cast_across_a_wide_sample() {
        let samples = [
            0.0,
            -0.0,
            0.5,
            -0.5,
            2.5,
            -2.5,
            1.4999999,
            -1.4999999,
            42.7,
            -42.7,
            16_777_215.5,
            -16_777_215.5,
            1.0e18,
            -1.0e18,
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            f32::MIN_POSITIVE,
        ];
        for w in samples {
            for h in samples {
                let got = LayoutSize::round(w, h);
                assert_eq!(got.width, f32_to_isize(libm::roundf(w)));
                assert_eq!(got.height, f32_to_isize(libm::roundf(h)));
            }
        }
    }

    #[test]
    fn round_round_trips_through_f32_for_layout_sized_values() {
        // Everything a real layout produces is well below 2^24, so round() must be
        // an exact inverse of the isize->f32 cast there.
        let mut v: isize = -4_000_000;
        while v <= 4_000_000 {
            assert_eq!(
                LayoutSize::round(isize_to_f32(v), isize_to_f32(-v)),
                size(v, -v),
                "round-trip broke at {v}"
            );
            v += 40_009; // prime-ish stride, hits odd and even alike
        }
    }

    // =================================================== derived traits ======

    #[test]
    fn point_and_size_ordering_is_lexicographic_on_their_fields() {
        assert!(point(0, 1) < point(1, 0));
        assert!(point(1, 1) < point(1, 2));
        assert_eq!(point(1, 2).cmp(&point(1, 2)), core::cmp::Ordering::Equal);
        assert!(point(isize::MIN, isize::MAX) < point(isize::MAX, isize::MIN));

        assert!(size(0, 1) < size(1, 0));
        assert!(size(-1, 0) < size(0, -1));

        // LayoutRect only derives PartialOrd: origin first, then size.
        assert!(rect(0, 0, 1, 1) < rect(0, 0, 1, 2));
        assert!(rect(0, 0, 9, 9) < rect(0, 1, 0, 0));
    }

    #[test]
    fn hash_agrees_with_eq_for_points_and_sizes() {
        assert_eq!(hash_of(&point(3, -4)), hash_of(&point(3, -4)));
        assert_eq!(hash_of(&size(3, -4)), hash_of(&size(3, -4)));
        // (x, y) and (y, x) must not collide — a field-order bug would show here.
        assert_ne!(hash_of(&point(3, -4)), hash_of(&point(-4, 3)));
        assert_ne!(hash_of(&point(0, 0)), hash_of(&point(0, 1)));
        assert_eq!(hash_of(&LayoutPoint::zero()), hash_of(&LayoutPoint::default()));
    }

    #[test]
    fn option_wrappers_default_to_none_and_round_trip_through_core_option() {
        assert!(OptionLayoutPoint::default().is_none());
        assert!(OptionLayoutSize::default().is_none());
        assert!(OptionLayoutRect::default().is_none());

        let r = rect(1, 2, 3, 4);
        let o: OptionLayoutRect = Some(r).into();
        assert!(o.is_some());
        assert_eq!(o.into_option(), Some(r));
        assert_eq!(Option::<LayoutRect>::from(OptionLayoutRect::None), None);

        let p: OptionLayoutPoint = Some(point(-1, -2)).into();
        assert_eq!(p.as_ref(), Some(&point(-1, -2)));
    }
}
