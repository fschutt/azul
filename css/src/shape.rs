//! CSS Shape data structures for shape-inside, shape-outside, and clip-path
//!
//! These types are C-compatible (repr(C)) for use across FFI boundaries.

use alloc::string::String;
use crate::corety::{AzString, OptionF32};

/// Compares two f32 values for ordering, treating NaN as equal.
fn cmp_f32(a: f32, b: f32) -> core::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(core::cmp::Ordering::Equal)
}

/// A 2D point for shape coordinates (using f32 for precision)
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct ShapePoint {
    pub x: f32,
    pub y: f32,
}

impl_option!(
    ShapePoint,
    OptionShapePoint,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd]
);

impl ShapePoint {
    #[must_use] pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[must_use] pub const fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

impl Eq for ShapePoint {}

// PartialOrd delegates to Ord (NaN-as-equal) so the two stay consistent.
impl PartialOrd for ShapePoint {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ShapePoint {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.x.partial_cmp(&other.x) {
            Some(core::cmp::Ordering::Equal) => self
                .y
                .partial_cmp(&other.y)
                .unwrap_or(core::cmp::Ordering::Equal),
            other => other.unwrap_or(core::cmp::Ordering::Equal),
        }
    }
}

impl core::hash::Hash for ShapePoint {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.x.to_bits().hash(state);
        self.y.to_bits().hash(state);
    }
}

impl_vec!(ShapePoint, ShapePointVec, ShapePointVecDestructor, ShapePointVecDestructorType, ShapePointVecSlice, OptionShapePoint);
impl_vec_debug!(ShapePoint, ShapePointVec);
impl_vec_partialord!(ShapePoint, ShapePointVec);
impl_vec_ord!(ShapePoint, ShapePointVec);
impl_vec_clone!(ShapePoint, ShapePointVec, ShapePointVecDestructor);
impl_vec_partialeq!(ShapePoint, ShapePointVec);
impl_vec_eq!(ShapePoint, ShapePointVec);
impl_vec_hash!(ShapePoint, ShapePointVec);

/// A circle shape defined by center point and radius
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct ShapeCircle {
    pub center: ShapePoint,
    pub radius: f32,
}

impl Eq for ShapeCircle {}
impl core::hash::Hash for ShapeCircle {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.center.hash(state);
        self.radius.to_bits().hash(state);
    }
}
impl PartialOrd for ShapeCircle {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ShapeCircle {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.center.cmp(&other.center) {
            core::cmp::Ordering::Equal => self
                .radius
                .partial_cmp(&other.radius)
                .unwrap_or(core::cmp::Ordering::Equal),
            other => other,
        }
    }
}

/// An ellipse shape defined by center point and two radii
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct ShapeEllipse {
    pub center: ShapePoint,
    pub radius_x: f32,
    pub radius_y: f32,
}

impl Eq for ShapeEllipse {}
impl core::hash::Hash for ShapeEllipse {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.center.hash(state);
        self.radius_x.to_bits().hash(state);
        self.radius_y.to_bits().hash(state);
    }
}
impl PartialOrd for ShapeEllipse {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ShapeEllipse {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.center.cmp(&other.center) {
            core::cmp::Ordering::Equal => match self.radius_x.partial_cmp(&other.radius_x) {
                Some(core::cmp::Ordering::Equal) | None => self
                    .radius_y
                    .partial_cmp(&other.radius_y)
                    .unwrap_or(core::cmp::Ordering::Equal),
                Some(other) => other,
            },
            other => other,
        }
    }
}

/// A polygon shape defined by a list of points (in clockwise order)
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ShapePolygon {
    pub points: ShapePointVec,
}

/// An inset rectangle with optional border radius
/// Defined by insets from the reference box edges
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct ShapeInset {
    pub inset_top: f32,
    pub inset_right: f32,
    pub inset_bottom: f32,
    pub inset_left: f32,
    pub border_radius: OptionF32,
}

impl Eq for ShapeInset {}
impl core::hash::Hash for ShapeInset {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.inset_top.to_bits().hash(state);
        self.inset_right.to_bits().hash(state);
        self.inset_bottom.to_bits().hash(state);
        self.inset_left.to_bits().hash(state);
        self.border_radius.hash(state);
    }
}
impl PartialOrd for ShapeInset {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ShapeInset {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        cmp_f32(self.inset_top, other.inset_top)
            .then_with(|| cmp_f32(self.inset_right, other.inset_right))
            .then_with(|| cmp_f32(self.inset_bottom, other.inset_bottom))
            .then_with(|| cmp_f32(self.inset_left, other.inset_left))
            .then_with(|| self.border_radius.cmp(&other.border_radius))
    }
}

/// An SVG-like path for shape definitions.
/// TODO: path parsing is not yet implemented — `data` is stored but not interpreted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ShapePath {
    pub data: AzString,
}

/// Represents a CSS shape for shape-inside, shape-outside, and clip-path.
/// Used for both text layout (shape-inside/outside) and rendering clipping (clip-path).
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssShape {
    Circle(ShapeCircle),
    Ellipse(ShapeEllipse),
    Polygon(ShapePolygon),
    Inset(ShapeInset),
    Path(ShapePath),
}

impl Eq for CssShape {}

impl core::hash::Hash for CssShape {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Self::Circle(c) => c.hash(state),
            Self::Ellipse(e) => e.hash(state),
            Self::Polygon(p) => p.hash(state),
            Self::Inset(i) => i.hash(state),
            Self::Path(p) => p.hash(state),
        }
    }
}

impl PartialOrd for CssShape {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CssShape {
    // The tie-break arms `(Self::X(_), _) => Less` / `(_, Self::X(_)) => Greater`
    // share bodies but are ORDER-DEPENDENT: they encode the variant ordering, so
    // merging them (clippy::match_same_arms) would change the comparison result.
    #[allow(clippy::match_same_arms)]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (Self::Circle(a), Self::Circle(b)) => a.cmp(b),
            (Self::Ellipse(a), Self::Ellipse(b)) => a.cmp(b),
            (Self::Polygon(a), Self::Polygon(b)) => a.cmp(b),
            (Self::Inset(a), Self::Inset(b)) => a.cmp(b),
            (Self::Path(a), Self::Path(b)) => a.cmp(b),
            // Different variants: use discriminant ordering
            (Self::Circle(_), _) => core::cmp::Ordering::Less,
            (_, Self::Circle(_)) => core::cmp::Ordering::Greater,
            (Self::Ellipse(_), _) => core::cmp::Ordering::Less,
            (_, Self::Ellipse(_)) => core::cmp::Ordering::Greater,
            (Self::Polygon(_), _) => core::cmp::Ordering::Less,
            (_, Self::Polygon(_)) => core::cmp::Ordering::Greater,
            (Self::Inset(_), Self::Path(_)) => core::cmp::Ordering::Less,
            (Self::Path(_), Self::Inset(_)) => core::cmp::Ordering::Greater,
        }
    }
}

impl CssShape {
    /// Creates a circle shape at the given position with the given radius
    #[must_use] pub const fn circle(center: ShapePoint, radius: f32) -> Self {
        Self::Circle(ShapeCircle { center, radius })
    }

    /// Creates an ellipse shape
    #[must_use] pub const fn ellipse(center: ShapePoint, radius_x: f32, radius_y: f32) -> Self {
        Self::Ellipse(ShapeEllipse {
            center,
            radius_x,
            radius_y,
        })
    }

    /// Creates a polygon from a list of points
    #[must_use] pub const fn polygon(points: ShapePointVec) -> Self {
        Self::Polygon(ShapePolygon { points })
    }

    /// Creates an inset rectangle
    #[must_use] pub const fn inset(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self::Inset(ShapeInset {
            inset_top: top,
            inset_right: right,
            inset_bottom: bottom,
            inset_left: left,
            border_radius: OptionF32::None,
        })
    }

    /// Creates an inset rectangle with rounded corners
    #[must_use] pub const fn inset_rounded(top: f32, right: f32, bottom: f32, left: f32, radius: f32) -> Self {
        Self::Inset(ShapeInset {
            inset_top: top,
            inset_right: right,
            inset_bottom: bottom,
            inset_left: left,
            border_radius: OptionF32::Some(radius),
        })
    }

    #[must_use] pub fn print_as_css_value(&self) -> String {
        use alloc::format;
        match self {
            Self::Circle(ShapeCircle { center, radius }) => {
                format!("circle({}px at {}px {}px)", radius, center.x, center.y)
            }
            Self::Ellipse(ShapeEllipse { center, radius_x, radius_y }) => {
                format!("ellipse({}px {}px at {}px {}px)", radius_x, radius_y, center.x, center.y)
            }
            Self::Polygon(ShapePolygon { points }) => {
                let pts: Vec<String> = points.as_ref().iter()
                    .map(|p| format!("{}px {}px", p.x, p.y))
                    .collect();
                format!("polygon({})", pts.join(", "))
            }
            Self::Inset(ShapeInset { inset_top, inset_right, inset_bottom, inset_left, border_radius }) => {
                let base = format!("inset({inset_top}px {inset_right}px {inset_bottom}px {inset_left}px");
                match border_radius {
                    OptionF32::Some(r) => format!("{base} round {r}px)"),
                    OptionF32::None => format!("{base})"),
                }
            }
            Self::Path(ShapePath { data }) => {
                format!("path(\"{}\")", data.as_str())
            }
        }
    }

    #[must_use] pub fn format_as_rust_code(&self) -> String {
        use alloc::format;
        match self {
            Self::Circle(ShapeCircle { center, radius }) => {
                format!(
                    "CssShape::Circle(ShapeCircle {{ center: ShapePoint::new({}_f32, {}_f32), radius: {}_f32 }})",
                    center.x, center.y, radius
                )
            }
            Self::Ellipse(ShapeEllipse { center, radius_x, radius_y }) => {
                format!(
                    "CssShape::Ellipse(ShapeEllipse {{ center: ShapePoint::new({}_f32, {}_f32), radius_x: {}_f32, radius_y: {}_f32 }})",
                    center.x, center.y, radius_x, radius_y
                )
            }
            Self::Polygon(ShapePolygon { points }) => {
                let pts: Vec<String> = points.as_ref().iter()
                    .map(|p| format!("ShapePoint::new({}_f32, {}_f32)", p.x, p.y))
                    .collect();
                format!("CssShape::Polygon(ShapePolygon {{ points: vec![{}].into() }})", pts.join(", "))
            }
            Self::Inset(ShapeInset { inset_top, inset_right, inset_bottom, inset_left, border_radius }) => {
                let br = match border_radius {
                    OptionF32::Some(r) => format!("OptionF32::Some({r}_f32)"),
                    OptionF32::None => String::from("OptionF32::None"),
                };
                format!(
                    "CssShape::Inset(ShapeInset {{ inset_top: {inset_top}_f32, inset_right: {inset_right}_f32, inset_bottom: {inset_bottom}_f32, inset_left: {inset_left}_f32, border_radius: {br} }})"
                )
            }
            Self::Path(ShapePath { data }) => {
                format!("CssShape::Path(ShapePath {{ data: AzString::from_const_str(\"{}\") }})", data.as_str())
            }
        }
    }
}

impl_option!(
    CssShape,
    OptionCssShape,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

#[cfg(test)]
mod autotest_generated {
    // Float values are compared for exact bit/value identity on purpose: these
    // tests check that constructors and (de)serialization are lossless, not that
    // the values are approximately right.
    #![allow(
        clippy::float_cmp,
        clippy::unreadable_literal,
        clippy::cast_precision_loss
    )]

    use core::{
        cmp::Ordering,
        hash::{Hash, Hasher},
    };
    use std::collections::hash_map::DefaultHasher;

    use super::*;
    use crate::shape_parser::{parse_shape, ShapeParseError};

    /// Every f32 edge value the shape types have to survive.
    const EDGE_F32: &[f32] = &[
        0.0,
        -0.0,
        1.0,
        -1.0,
        f32::MIN,
        f32::MAX,
        f32::MIN_POSITIVE,
        f32::EPSILON,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];

    fn hash_of<T: Hash>(t: &T) -> u64 {
        let mut h = DefaultHasher::new();
        t.hash(&mut h);
        h.finish()
    }

    /// Encode a shape to CSS, decode it back. Panics if the shape does not
    /// survive its own printer -> the crate's own parser.
    fn roundtrip(shape: &CssShape) -> CssShape {
        let css = shape.print_as_css_value();
        parse_shape(&css).unwrap_or_else(|e| panic!("round-trip failed for {css:?}: {e:?}"))
    }

    fn poly(coords: &[(f32, f32)]) -> CssShape {
        let pts: Vec<ShapePoint> = coords.iter().map(|(x, y)| ShapePoint::new(*x, *y)).collect();
        CssShape::polygon(ShapePointVec::from_vec(pts))
    }

    fn path(data: &str) -> CssShape {
        CssShape::Path(ShapePath {
            data: AzString::from(data),
        })
    }

    // ---------------------------------------------------------------------
    // cmp_f32 (private, numeric)
    // ---------------------------------------------------------------------

    #[test]
    fn cmp_f32_orders_ordinary_values() {
        assert_eq!(cmp_f32(1.0, 2.0), Ordering::Less);
        assert_eq!(cmp_f32(2.0, 1.0), Ordering::Greater);
        assert_eq!(cmp_f32(2.0, 2.0), Ordering::Equal);
        assert_eq!(cmp_f32(-1.0, 1.0), Ordering::Less);
    }

    #[test]
    fn cmp_f32_treats_both_zeroes_as_equal() {
        assert_eq!(cmp_f32(0.0, -0.0), Ordering::Equal);
        assert_eq!(cmp_f32(-0.0, 0.0), Ordering::Equal);
    }

    #[test]
    fn cmp_f32_nan_is_equal_to_everything_and_never_panics() {
        // Documented contract: "treating NaN as equal".
        assert_eq!(cmp_f32(f32::NAN, f32::NAN), Ordering::Equal);
        for &v in EDGE_F32 {
            assert_eq!(cmp_f32(f32::NAN, v), Ordering::Equal);
            assert_eq!(cmp_f32(v, f32::NAN), Ordering::Equal);
        }
    }

    #[test]
    fn cmp_f32_handles_infinities_and_limits() {
        assert_eq!(cmp_f32(f32::INFINITY, f32::MAX), Ordering::Greater);
        assert_eq!(cmp_f32(f32::NEG_INFINITY, f32::MIN), Ordering::Less);
        assert_eq!(cmp_f32(f32::INFINITY, f32::INFINITY), Ordering::Equal);
        assert_eq!(
            cmp_f32(f32::NEG_INFINITY, f32::INFINITY),
            Ordering::Less
        );
        assert_eq!(cmp_f32(f32::MIN_POSITIVE, 0.0), Ordering::Greater);
        // Smallest subnormal still sorts above zero.
        assert_eq!(cmp_f32(f32::from_bits(1), 0.0), Ordering::Greater);
        assert_eq!(cmp_f32(f32::MIN, f32::MAX), Ordering::Less);
    }

    #[test]
    fn cmp_f32_is_antisymmetric_over_all_edge_values() {
        for &a in EDGE_F32 {
            for &b in EDGE_F32 {
                assert_eq!(
                    cmp_f32(a, b),
                    cmp_f32(b, a).reverse(),
                    "antisymmetry broken for ({a}, {b})"
                );
            }
        }
    }

    #[test]
    fn cmp_f32_nan_equality_is_not_transitive() {
        // Characterization of the known cost of "NaN as equal": NaN == 1.0 and
        // NaN == 2.0, yet 1.0 < 2.0. Ord's transitivity therefore does NOT hold
        // once a NaN is in the set, so slices holding NaN-bearing shapes must
        // not be `sort()`ed (std may panic on a non-total order).
        assert_eq!(cmp_f32(f32::NAN, 1.0), Ordering::Equal);
        assert_eq!(cmp_f32(f32::NAN, 2.0), Ordering::Equal);
        assert_eq!(cmp_f32(1.0, 2.0), Ordering::Less);
    }

    // ---------------------------------------------------------------------
    // ShapePoint::new / ShapePoint::zero (constructors)
    // ---------------------------------------------------------------------

    #[test]
    fn shapepoint_new_stores_fields_verbatim() {
        let p = ShapePoint::new(3.5, -7.25);
        assert_eq!(p.x, 3.5);
        assert_eq!(p.y, -7.25);
    }

    #[test]
    fn shapepoint_new_does_not_normalize_edge_values() {
        for &x in EDGE_F32 {
            for &y in EDGE_F32 {
                let p = ShapePoint::new(x, y);
                // Bit-exact: no clamping, no NaN canonicalization, no -0 flush.
                assert_eq!(p.x.to_bits(), x.to_bits());
                assert_eq!(p.y.to_bits(), y.to_bits());
            }
        }
    }

    #[test]
    fn shapepoint_new_preserves_negative_zero_sign() {
        let p = ShapePoint::new(-0.0, 0.0);
        assert_eq!(p.x.to_bits(), (-0.0f32).to_bits());
        assert_eq!(p.y.to_bits(), (0.0f32).to_bits());
        assert!(p.x.is_sign_negative());
    }

    #[test]
    fn shapepoint_zero_is_the_neutral_positive_zero() {
        let z = ShapePoint::zero();
        assert_eq!(z.x.to_bits(), 0);
        assert_eq!(z.y.to_bits(), 0);
        assert_eq!(z, ShapePoint::new(0.0, 0.0));
        assert_eq!(z.cmp(&ShapePoint::new(0.0, 0.0)), Ordering::Equal);
    }

    #[test]
    fn shapepoint_constructors_are_usable_in_const_context() {
        const P: ShapePoint = ShapePoint::new(1.0, 2.0);
        const Z: ShapePoint = ShapePoint::zero();
        assert_eq!(P.x, 1.0);
        assert_eq!(Z, ShapePoint::zero());
        // repr(C) layout guarantee relied on across the FFI boundary.
        assert_eq!(size_of::<ShapePoint>(), 8);
    }

    // ---------------------------------------------------------------------
    // ShapePoint Ord / Eq / Hash invariants
    // ---------------------------------------------------------------------

    #[test]
    fn shapepoint_ord_sorts_by_x_then_y() {
        let mut v = vec![
            ShapePoint::new(1.0, 5.0),
            ShapePoint::new(-3.0, 0.0),
            ShapePoint::new(1.0, -5.0),
            ShapePoint::new(0.0, 0.0),
        ];
        v.sort(); // no NaN involved -> total order holds, sort must not panic
        assert_eq!(
            v,
            vec![
                ShapePoint::new(-3.0, 0.0),
                ShapePoint::new(0.0, 0.0),
                ShapePoint::new(1.0, -5.0),
                ShapePoint::new(1.0, 5.0),
            ]
        );
    }

    #[test]
    fn shapepoint_cmp_is_antisymmetric_even_with_nan() {
        for &ax in EDGE_F32 {
            for &ay in EDGE_F32 {
                let a = ShapePoint::new(ax, ay);
                for &bx in EDGE_F32 {
                    let b = ShapePoint::new(bx, 1.0);
                    assert_eq!(
                        a.cmp(&b),
                        b.cmp(&a).reverse(),
                        "antisymmetry broken for {a:?} vs {b:?}"
                    );
                    // PartialOrd must agree with Ord (it delegates).
                    assert_eq!(a.partial_cmp(&b), Some(a.cmp(&b)));
                }
            }
        }
    }

    #[test]
    fn shapepoint_cmp_with_nan_x_ignores_y_entirely() {
        // Characterization: when the x comparison is indeterminate, cmp() returns
        // Equal WITHOUT looking at y -- unlike ShapeEllipse::cmp, which falls
        // through to the next field on None. The two NaN policies in this file
        // disagree; see the report.
        let a = ShapePoint::new(f32::NAN, 1.0);
        let b = ShapePoint::new(f32::NAN, 2.0);
        assert_eq!(a.cmp(&b), Ordering::Equal);

        let e1 = ShapeEllipse {
            center: ShapePoint::zero(),
            radius_x: f32::NAN,
            radius_y: 1.0,
        };
        let e2 = ShapeEllipse {
            center: ShapePoint::zero(),
            radius_x: f32::NAN,
            radius_y: 2.0,
        };
        assert_eq!(e1.cmp(&e2), Ordering::Less);
    }

    #[test]
    fn shapepoint_nan_is_ord_equal_but_partialeq_unequal() {
        // Eq is implemented for ShapePoint, but PartialEq is derived on f32, so
        // reflexivity does not actually hold for NaN while Ord reports Equal.
        let a = ShapePoint::new(f32::NAN, 0.0);
        let b = ShapePoint::new(f32::NAN, 0.0);
        assert_ne!(a, b, "derived PartialEq: NaN != NaN");
        assert_eq!(a.cmp(&b), Ordering::Equal, "Ord: NaN treated as equal");
    }

    #[test]
    fn shapepoint_hash_is_deterministic_and_bitwise() {
        let a = ShapePoint::new(1.5, -2.5);
        let b = ShapePoint::new(1.5, -2.5);
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_ne!(hash_of(&a), hash_of(&ShapePoint::new(-2.5, 1.5)));
        // NaN hashes by bits, so an identical NaN hashes identically even though
        // it does not compare PartialEq-equal to itself.
        let n1 = ShapePoint::new(f32::NAN, 0.0);
        let n2 = ShapePoint::new(f32::NAN, 0.0);
        assert_eq!(hash_of(&n1), hash_of(&n2));
    }

    // ---------------------------------------------------------------------
    // CssShape constructors (numeric)
    // ---------------------------------------------------------------------

    #[test]
    fn circle_stores_center_and_radius_including_zero_and_negative() {
        match CssShape::circle(ShapePoint::zero(), 0.0) {
            CssShape::Circle(c) => {
                assert_eq!(c.radius, 0.0);
                assert_eq!(c.center, ShapePoint::zero());
            }
            other => panic!("expected Circle, got {other:?}"),
        }
        // A negative radius is CSS-invalid but accepted verbatim here (no clamp).
        match CssShape::circle(ShapePoint::new(-1.0, -2.0), -50.0) {
            CssShape::Circle(c) => assert_eq!(c.radius, -50.0),
            other => panic!("expected Circle, got {other:?}"),
        }
    }

    #[test]
    fn circle_accepts_every_f32_edge_value_without_panicking() {
        for &r in EDGE_F32 {
            for &c in EDGE_F32 {
                let shape = CssShape::circle(ShapePoint::new(c, c), r);
                match shape {
                    CssShape::Circle(circle) => {
                        assert_eq!(circle.radius.to_bits(), r.to_bits());
                    }
                    other => panic!("expected Circle, got {other:?}"),
                }
            }
        }
    }

    #[test]
    fn ellipse_stores_both_radii_verbatim() {
        match CssShape::ellipse(ShapePoint::new(1.0, 2.0), f32::INFINITY, f32::NEG_INFINITY) {
            CssShape::Ellipse(e) => {
                assert!(e.radius_x.is_infinite() && e.radius_x.is_sign_positive());
                assert!(e.radius_y.is_infinite() && e.radius_y.is_sign_negative());
                assert_eq!(e.center, ShapePoint::new(1.0, 2.0));
            }
            other => panic!("expected Ellipse, got {other:?}"),
        }
        match CssShape::ellipse(ShapePoint::zero(), f32::NAN, f32::MAX) {
            CssShape::Ellipse(e) => {
                assert!(e.radius_x.is_nan());
                assert_eq!(e.radius_y, f32::MAX);
            }
            other => panic!("expected Ellipse, got {other:?}"),
        }
    }

    #[test]
    fn polygon_accepts_empty_and_huge_point_lists() {
        match CssShape::polygon(ShapePointVec::from_vec(Vec::new())) {
            CssShape::Polygon(p) => {
                assert!(p.points.is_empty());
                assert_eq!(p.points.len(), 0);
            }
            other => panic!("expected Polygon, got {other:?}"),
        }

        let big: Vec<ShapePoint> = (0..10_000)
            .map(|i| ShapePoint::new(i as f32, -(i as f32)))
            .collect();
        match CssShape::polygon(ShapePointVec::from_vec(big)) {
            CssShape::Polygon(p) => {
                assert_eq!(p.points.len(), 10_000);
                assert_eq!(p.points.as_ref()[9_999], ShapePoint::new(9999.0, -9999.0));
            }
            other => panic!("expected Polygon, got {other:?}"),
        }
    }

    #[test]
    fn inset_has_no_border_radius_and_keeps_side_order() {
        match CssShape::inset(1.0, 2.0, 3.0, 4.0) {
            CssShape::Inset(i) => {
                assert_eq!(i.inset_top, 1.0);
                assert_eq!(i.inset_right, 2.0);
                assert_eq!(i.inset_bottom, 3.0);
                assert_eq!(i.inset_left, 4.0);
                assert_eq!(i.border_radius, OptionF32::None);
                assert_eq!(i.border_radius.into_option(), None);
            }
            other => panic!("expected Inset, got {other:?}"),
        }
    }

    #[test]
    fn inset_accepts_min_max_and_nan_without_panicking() {
        match CssShape::inset(f32::MIN, f32::MAX, f32::NAN, f32::NEG_INFINITY) {
            CssShape::Inset(i) => {
                assert_eq!(i.inset_top, f32::MIN);
                assert_eq!(i.inset_right, f32::MAX);
                assert!(i.inset_bottom.is_nan());
                assert!(i.inset_left.is_infinite());
            }
            other => panic!("expected Inset, got {other:?}"),
        }
    }

    #[test]
    fn inset_rounded_keeps_even_a_css_invalid_negative_radius() {
        match CssShape::inset_rounded(0.0, 0.0, 0.0, 0.0, -5.0) {
            CssShape::Inset(i) => {
                assert_eq!(i.border_radius, OptionF32::Some(-5.0));
                assert_eq!(i.border_radius.into_option(), Some(-5.0));
            }
            other => panic!("expected Inset, got {other:?}"),
        }
        match CssShape::inset_rounded(0.0, 0.0, 0.0, 0.0, f32::NAN) {
            CssShape::Inset(i) => match i.border_radius {
                OptionF32::Some(r) => assert!(r.is_nan()),
                OptionF32::None => panic!("radius was dropped"),
            },
            other => panic!("expected Inset, got {other:?}"),
        }
    }

    #[test]
    fn css_shape_constructors_are_usable_in_const_context() {
        const CIRCLE: CssShape = CssShape::circle(ShapePoint::zero(), 5.0);
        const ELLIPSE: CssShape = CssShape::ellipse(ShapePoint::zero(), 1.0, 2.0);
        const INSET: CssShape = CssShape::inset(0.0, 0.0, 0.0, 0.0);
        const ROUNDED: CssShape = CssShape::inset_rounded(0.0, 0.0, 0.0, 0.0, 1.0);
        assert!(matches!(CIRCLE, CssShape::Circle(_)));
        assert!(matches!(ELLIPSE, CssShape::Ellipse(_)));
        assert!(matches!(INSET, CssShape::Inset(_)));
        assert!(matches!(ROUNDED, CssShape::Inset(_)));
    }

    // ---------------------------------------------------------------------
    // CssShape Ord / Hash invariants
    // ---------------------------------------------------------------------

    #[test]
    fn css_shape_variant_order_is_circle_ellipse_polygon_inset_path() {
        let shapes = vec![
            path("M 0 0"),
            CssShape::inset(1.0, 1.0, 1.0, 1.0),
            poly(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]),
            CssShape::ellipse(ShapePoint::zero(), 1.0, 2.0),
            CssShape::circle(ShapePoint::zero(), 1.0),
        ];
        let mut sorted = shapes;
        sorted.sort(); // finite values only -> total order, must not panic
        let discriminants: Vec<&str> = sorted
            .iter()
            .map(|s| match s {
                CssShape::Circle(_) => "circle",
                CssShape::Ellipse(_) => "ellipse",
                CssShape::Polygon(_) => "polygon",
                CssShape::Inset(_) => "inset",
                CssShape::Path(_) => "path",
            })
            .collect();
        assert_eq!(
            discriminants,
            vec!["circle", "ellipse", "polygon", "inset", "path"]
        );
    }

    #[test]
    fn css_shape_cmp_is_antisymmetric_across_variants_and_nan() {
        let shapes = vec![
            CssShape::circle(ShapePoint::zero(), f32::NAN),
            CssShape::circle(ShapePoint::new(f32::NAN, 0.0), 1.0),
            CssShape::ellipse(ShapePoint::zero(), f32::NAN, f32::INFINITY),
            poly(&[]),
            poly(&[(f32::NAN, f32::NEG_INFINITY)]),
            CssShape::inset(f32::NAN, f32::MAX, f32::MIN, -0.0),
            CssShape::inset_rounded(0.0, 0.0, 0.0, 0.0, f32::NAN),
            path(""),
            path("\u{1F600}"),
        ];
        for a in &shapes {
            for b in &shapes {
                assert_eq!(
                    a.cmp(b),
                    b.cmp(a).reverse(),
                    "antisymmetry broken for {a:?} vs {b:?}"
                );
                assert_eq!(a.partial_cmp(b), Some(a.cmp(b)));
            }
        }
    }

    #[test]
    fn css_shape_hash_separates_variants_with_identical_payloads() {
        let circle = CssShape::circle(ShapePoint::zero(), 1.0);
        let ellipse = CssShape::ellipse(ShapePoint::zero(), 1.0, 1.0);
        assert_ne!(hash_of(&circle), hash_of(&ellipse));
        assert_eq!(
            hash_of(&circle),
            hash_of(&CssShape::circle(ShapePoint::zero(), 1.0))
        );
        // Equal values must hash equally (holds for every non-NaN, non -0.0 case).
        let p1 = poly(&[(1.0, 2.0), (3.0, 4.0), (5.0, 6.0)]);
        let p2 = poly(&[(1.0, 2.0), (3.0, 4.0), (5.0, 6.0)]);
        assert_eq!(p1, p2);
        assert_eq!(hash_of(&p1), hash_of(&p2));
    }

    // ---------------------------------------------------------------------
    // print_as_css_value (getter)
    // ---------------------------------------------------------------------

    #[test]
    fn print_as_css_value_known_constructions() {
        assert_eq!(
            CssShape::circle(ShapePoint::new(100.0, 100.0), 50.0).print_as_css_value(),
            "circle(50px at 100px 100px)"
        );
        assert_eq!(
            CssShape::ellipse(ShapePoint::new(1.0, 2.0), 3.0, 4.5).print_as_css_value(),
            "ellipse(3px 4.5px at 1px 2px)"
        );
        assert_eq!(
            poly(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)]).print_as_css_value(),
            "polygon(0px 0px, 100px 0px, 100px 100px)"
        );
        assert_eq!(
            CssShape::inset(1.0, 2.0, 3.0, 4.0).print_as_css_value(),
            "inset(1px 2px 3px 4px)"
        );
        assert_eq!(
            CssShape::inset_rounded(1.0, 2.0, 3.0, 4.0, 5.0).print_as_css_value(),
            "inset(1px 2px 3px 4px round 5px)"
        );
        assert_eq!(
            path("M 0 0 L 1 1 Z").print_as_css_value(),
            "path(\"M 0 0 L 1 1 Z\")"
        );
    }

    #[test]
    fn print_as_css_value_empty_polygon_emits_empty_parens() {
        // Not valid CSS (a polygon needs >= 3 points) but must not panic.
        assert_eq!(poly(&[]).print_as_css_value(), "polygon()");
    }

    #[test]
    fn print_as_css_value_emits_non_css_tokens_for_nan_and_inf() {
        // Characterization: `NaNpx` / `infpx` are NOT valid CSS lengths. The
        // printer does not guard against non-finite inputs; see the report.
        let s = CssShape::circle(
            ShapePoint::new(f32::INFINITY, f32::NEG_INFINITY),
            f32::NAN,
        )
        .print_as_css_value();
        assert_eq!(s, "circle(NaNpx at infpx -infpx)");
    }

    #[test]
    fn print_as_css_value_handles_extreme_finite_values() {
        let s = CssShape::inset(f32::MIN, f32::MAX, f32::MIN_POSITIVE, 0.0).print_as_css_value();
        assert!(s.starts_with("inset(-"), "got {s}");
        assert!(s.ends_with("px)"), "got {s}");
        assert!(!s.contains("inf"), "MIN/MAX must not print as inf: {s}");
    }

    #[test]
    fn print_as_css_value_survives_a_huge_polygon() {
        let coords: Vec<(f32, f32)> = (0..5_000).map(|i| (i as f32, i as f32)).collect();
        let s = poly(&coords).print_as_css_value();
        assert!(s.starts_with("polygon(0px 0px, "));
        assert!(s.ends_with("4999px 4999px)"));
        assert_eq!(s.matches(", ").count(), 4_999);
    }

    #[test]
    fn print_as_css_value_does_not_escape_path_data() {
        // Characterization: a quote inside the path data is emitted raw, so the
        // printed value is no longer a well-formed CSS string. See the report.
        let s = path("a\"b").print_as_css_value();
        assert_eq!(s, "path(\"a\"b\")");
        // Unicode path data is passed through byte-for-byte.
        let uni = path("M 0 0 \u{2192} \u{1F600}").print_as_css_value();
        assert!(uni.contains('\u{1F600}'));
    }

    // ---------------------------------------------------------------------
    // format_as_rust_code (serializer)
    // ---------------------------------------------------------------------

    #[test]
    fn format_as_rust_code_known_constructions() {
        assert_eq!(
            CssShape::circle(ShapePoint::new(1.0, 2.0), 3.0).format_as_rust_code(),
            "CssShape::Circle(ShapeCircle { center: ShapePoint::new(1_f32, 2_f32), radius: 3_f32 \
             })"
        );
        assert_eq!(
            CssShape::ellipse(ShapePoint::new(1.0, 2.0), 3.0, 4.0).format_as_rust_code(),
            "CssShape::Ellipse(ShapeEllipse { center: ShapePoint::new(1_f32, 2_f32), radius_x: \
             3_f32, radius_y: 4_f32 })"
        );
        assert_eq!(
            CssShape::inset(1.0, 2.0, 3.0, 4.0).format_as_rust_code(),
            "CssShape::Inset(ShapeInset { inset_top: 1_f32, inset_right: 2_f32, inset_bottom: \
             3_f32, inset_left: 4_f32, border_radius: OptionF32::None })"
        );
        assert!(CssShape::inset_rounded(0.0, 0.0, 0.0, 0.0, 5.0)
            .format_as_rust_code()
            .contains("border_radius: OptionF32::Some(5_f32)"));
        assert_eq!(
            path("M 0 0").format_as_rust_code(),
            "CssShape::Path(ShapePath { data: AzString::from_const_str(\"M 0 0\") })"
        );
    }

    #[test]
    fn format_as_rust_code_is_non_empty_for_every_variant() {
        let shapes = vec![
            CssShape::circle(ShapePoint::zero(), 0.0),
            CssShape::ellipse(ShapePoint::zero(), 0.0, 0.0),
            poly(&[]),
            CssShape::inset(0.0, 0.0, 0.0, 0.0),
            path(""),
        ];
        for s in &shapes {
            let code = s.format_as_rust_code();
            assert!(code.starts_with("CssShape::"), "got {code}");
            assert!(code.ends_with(')'), "got {code}");
            assert!(!code.is_empty());
        }
    }

    #[test]
    fn format_as_rust_code_empty_polygon_emits_empty_vec() {
        assert_eq!(
            poly(&[]).format_as_rust_code(),
            "CssShape::Polygon(ShapePolygon { points: vec![].into() })"
        );
        assert_eq!(
            poly(&[(0.0, 0.0), (1.0, 1.0)]).format_as_rust_code(),
            "CssShape::Polygon(ShapePolygon { points: vec![ShapePoint::new(0_f32, 0_f32), \
             ShapePoint::new(1_f32, 1_f32)].into() })"
        );
    }

    #[test]
    fn format_as_rust_code_emits_uncompilable_tokens_for_nan_and_inf() {
        // Characterization of a real codegen defect: `NaN_f32` / `inf_f32` are
        // not valid Rust literals, so generated code containing a non-finite
        // shape does not compile. See the report.
        let code = CssShape::circle(ShapePoint::new(f32::INFINITY, f32::NEG_INFINITY), f32::NAN)
            .format_as_rust_code();
        assert!(code.contains("NaN_f32"), "got {code}");
        assert!(code.contains("inf_f32"), "got {code}");
        assert!(code.contains("-inf_f32"), "got {code}");
    }

    #[test]
    fn format_as_rust_code_does_not_escape_path_data() {
        // Characterization: quotes and backslashes in path data are emitted raw,
        // producing uncompilable Rust. See the report.
        let code = path("a\"b\\c").format_as_rust_code();
        assert!(code.contains("from_const_str(\"a\"b\\c\")"), "got {code}");
    }

    #[test]
    fn format_as_rust_code_survives_extreme_values() {
        for &v in EDGE_F32 {
            let code = CssShape::inset_rounded(v, v, v, v, v).format_as_rust_code();
            assert!(code.starts_with("CssShape::Inset("), "got {code}");
        }
    }

    // ---------------------------------------------------------------------
    // Round-trip: print_as_css_value -> shape_parser::parse_shape
    // ---------------------------------------------------------------------

    #[test]
    fn roundtrip_circle_ellipse_inset_and_path() {
        for shape in [
            CssShape::circle(ShapePoint::new(100.0, -25.5), 50.0),
            CssShape::circle(ShapePoint::zero(), 0.0),
            CssShape::ellipse(ShapePoint::new(-1.0, 2.0), 3.0, 4.5),
            CssShape::inset(1.0, 2.0, 3.0, 4.0),
            CssShape::inset(-1.0, -2.0, -3.0, -4.0),
            CssShape::inset_rounded(1.0, 2.0, 3.0, 4.0, 5.0),
            path("M 0 0 L 100 0 L 100 100 Z"),
            path(""),
        ] {
            assert_eq!(roundtrip(&shape), shape);
        }
    }

    #[test]
    fn roundtrip_polygon_needs_at_least_three_points() {
        let ok = poly(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]);
        assert_eq!(roundtrip(&ok), ok);

        // Printer/parser asymmetry: these print fine but the crate's own parser
        // rejects them, so a shape can survive `print` and die on re-parse.
        let two = poly(&[(0.0, 0.0), (1.0, 1.0)]).print_as_css_value();
        assert!(matches!(
            parse_shape(&two),
            Err(ShapeParseError::InvalidSyntax(_))
        ));
        let empty = poly(&[]).print_as_css_value();
        assert!(matches!(
            parse_shape(&empty),
            Err(ShapeParseError::InvalidSyntax(_))
        ));
    }

    #[test]
    fn roundtrip_preserves_extreme_finite_floats_bit_for_bit() {
        for &v in &[
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            f32::from_bits(1), // smallest positive subnormal
            1e-7,
            123_456.79,
        ] {
            let shape = CssShape::circle(ShapePoint::new(v, -v), v);
            match roundtrip(&shape) {
                CssShape::Circle(c) => {
                    assert_eq!(c.radius.to_bits(), v.to_bits(), "radius lost for {v:e}");
                    assert_eq!(c.center.x.to_bits(), v.to_bits(), "x lost for {v:e}");
                    assert_eq!(c.center.y.to_bits(), (-v).to_bits(), "y lost for {v:e}");
                }
                other => panic!("expected Circle, got {other:?}"),
            }
        }
    }

    #[test]
    fn roundtrip_of_non_finite_values_survives_but_is_not_valid_css() {
        // Rust's f32 FromStr happens to accept "inf"/"NaN", so azul's own parser
        // reads back what its printer emitted -- but no browser would.
        match roundtrip(&CssShape::circle(ShapePoint::zero(), f32::INFINITY)) {
            CssShape::Circle(c) => assert!(c.radius.is_infinite() && c.radius > 0.0),
            other => panic!("expected Circle, got {other:?}"),
        }
        match roundtrip(&CssShape::circle(ShapePoint::zero(), f32::NAN)) {
            CssShape::Circle(c) => assert!(c.radius.is_nan()),
            other => panic!("expected Circle, got {other:?}"),
        }
        match roundtrip(&CssShape::inset(
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NAN,
            0.0,
        )) {
            CssShape::Inset(i) => {
                assert!(i.inset_top.is_infinite() && i.inset_top < 0.0);
                assert!(i.inset_right.is_infinite() && i.inset_right > 0.0);
                assert!(i.inset_bottom.is_nan());
                assert_eq!(i.inset_left, 0.0);
            }
            other => panic!("expected Inset, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_path_with_hostile_and_unicode_data() {
        // A ')' inside the data is safe (the parser scans with rfind), and so is
        // a bare quote (the outer quotes are stripped positionally).
        for data in [
            "M 0 0)",
            "M 0 0 \u{2192} \u{1F600}",
            "M\n0\t0",
            "a\"b",
            "\u{0}",
        ] {
            let shape = path(data);
            assert_eq!(roundtrip(&shape), shape, "path data {data:?} did not survive");
        }
    }

    #[test]
    fn roundtrip_huge_polygon() {
        let coords: Vec<(f32, f32)> = (0..2_000).map(|i| (i as f32, -(i as f32))).collect();
        let shape = poly(&coords);
        assert_eq!(roundtrip(&shape), shape);
    }
}

