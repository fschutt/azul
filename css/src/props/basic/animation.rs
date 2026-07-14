//! SVG geometry primitives (points, curves, rects, vectors) and animation interpolation functions.

use crate::impl_option;

/// Precision-reducing `usize` → `f64` for Bézier sample indices. The step count
/// is tiny so no precision is actually lost; `as` is the only `usize`→`f64` form,
/// isolated here behind a documented attribute.
#[inline]
#[allow(clippy::cast_precision_loss)]
const fn idx_to_f64(v: usize) -> f64 {
    v as f64
}

/// Truncating `f64` → `f32` for SVG curve sample coordinates. Behaviour-preserving
/// (`as f32` rounds to the nearest representable value); isolates the narrowing.
#[inline]
#[allow(clippy::cast_possible_truncation)]
const fn f64_to_f32(v: f64) -> f32 {
    v as f32
}

/// Holds context needed to resolve animation interpolation relative to parent and current rects.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct InterpolateResolver {
    pub interpolate_func: AnimationInterpolationFunction,
    pub parent_rect_width: f32,
    pub parent_rect_height: f32,
    pub current_rect_width: f32,
    pub current_rect_height: f32,
}

/// A 2D point with f32 coordinates, used in SVG paths and bezier curves.
#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgPoint {
    pub x: f32,
    pub y: f32,
}

/// A cubic bezier curve defined by start, two control points, and end point.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgCubicCurve {
    pub start: SvgPoint,
    pub ctrl_1: SvgPoint,
    pub ctrl_2: SvgPoint,
    pub end: SvgPoint,
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Represents an animation timing function.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C, u8)]
pub enum AnimationInterpolationFunction {
    Ease,
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(SvgCubicCurve),
}

/// An axis-aligned rectangle with optional rounded corners.
#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgRect {
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    pub radius_top_left: f32,
    pub radius_top_right: f32,
    pub radius_bottom_left: f32,
    pub radius_bottom_right: f32,
}

/// A 2D vector with f64 coordinates, used for tangent and direction calculations.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgVector {
    pub x: f64,
    pub y: f64,
}

/// A quadratic bezier curve defined by start, one control point, and end point.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgQuadraticCurve {
    pub start: SvgPoint,
    pub ctrl: SvgPoint,
    pub end: SvgPoint,
}

impl_option!(
    SvgPoint,
    OptionSvgPoint,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl SvgPoint {
    /// Creates a new `SvgPoint` from x and y coordinates
    #[inline]
    #[must_use] pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Returns the Euclidean distance between this point and `other`.
    #[inline]
    #[must_use] pub fn distance(&self, other: Self) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        f64::from(libm::hypotf(dx, dy))
    }
}

impl SvgRect {
    /// Expands this rect to also contain `other`.
    pub fn union_with(&mut self, other: &Self) {
        let self_max_x = self.x + self.width;
        let self_max_y = self.y + self.height;
        let self_min_x = self.x;
        let self_min_y = self.y;

        let other_max_x = other.x + other.width;
        let other_max_y = other.y + other.height;
        let other_min_x = other.x;
        let other_min_y = other.y;

        let max_x = self_max_x.max(other_max_x);
        let max_y = self_max_y.max(other_max_y);
        let min_x = self_min_x.min(other_min_x);
        let min_y = self_min_y.min(other_min_y);

        self.x = min_x;
        self.y = min_y;
        self.width = max_x - min_x;
        self.height = max_y - min_y;
    }

    /// Note: does not incorporate rounded edges!
    /// Origin of x and y is assumed to be the top left corner
    #[must_use] pub fn contains_point(&self, point: SvgPoint) -> bool {
        point.x > self.x
            && point.x < self.x + self.width
            && point.y > self.y
            && point.y < self.y + self.height
    }

    /// Expands the rect with a certain amount of padding
    #[must_use] pub fn expand(
        &self,
        padding_top: f32,
        padding_bottom: f32,
        padding_left: f32,
        padding_right: f32,
    ) -> Self {
        Self {
            width: self.width + padding_left + padding_right,
            height: self.height + padding_top + padding_bottom,
            x: self.x - padding_left,
            y: self.y - padding_top,
            ..*self
        }
    }

    /// Returns the center point of the rect.
    #[must_use] pub fn get_center(&self) -> SvgPoint {
        SvgPoint {
            x: self.x + (self.width / 2.0),
            y: self.y + (self.height / 2.0),
        }
    }
}

const STEP_SIZE: usize = 20;
const STEP_SIZE_F64: f64 = 0.05;

// Bézier sampling keeps the explicit `a*b + c` forms rather than `mul_add`:
// `f32::mul_add` lowers to a software `fmaf` call (slower) on targets without
// `+fma`, and changes results bit-for-bit. (clippy::suboptimal_flops)
#[allow(clippy::suboptimal_flops)]
impl SvgCubicCurve {
    /// Creates a new `SvgCubicCurve` from start, two control points, and end point
    #[inline]
    #[must_use] pub const fn new(start: SvgPoint, ctrl_1: SvgPoint, ctrl_2: SvgPoint, end: SvgPoint) -> Self {
        Self { start, ctrl_1, ctrl_2, end }
    }

    /// Reverses the curve direction in place, swapping start/end and `ctrl_1/ctrl_2`.
    pub const fn reverse(&mut self) {
        core::mem::swap(&mut self.start, &mut self.end);
        core::mem::swap(&mut self.ctrl_1, &mut self.ctrl_2);
    }

    /// Returns the start point of the curve.
    #[must_use] pub const fn get_start(&self) -> SvgPoint {
        self.start
    }
    /// Returns the end point of the curve.
    #[must_use] pub const fn get_end(&self) -> SvgPoint {
        self.end
    }

    /// Evaluates the x coordinate of the curve at parameter `t` in [0, 1].
    #[must_use] pub fn get_x_at_t(&self, t: f64) -> f64 {
        let c_x = 3.0 * (f64::from(self.ctrl_1.x) - f64::from(self.start.x));
        let b_x = 3.0 * (f64::from(self.ctrl_2.x) - f64::from(self.ctrl_1.x)) - c_x;
        let a_x = f64::from(self.end.x) - f64::from(self.start.x) - c_x - b_x;

        (a_x * t * t * t) + (b_x * t * t) + (c_x * t) + f64::from(self.start.x)
    }

    /// Evaluates the y coordinate of the curve at parameter `t` in [0, 1].
    #[must_use] pub fn get_y_at_t(&self, t: f64) -> f64 {
        let c_y = 3.0 * (f64::from(self.ctrl_1.y) - f64::from(self.start.y));
        let b_y = 3.0 * (f64::from(self.ctrl_2.y) - f64::from(self.ctrl_1.y)) - c_y;
        let a_y = f64::from(self.end.y) - f64::from(self.start.y) - c_y - b_y;

        (a_y * t * t * t) + (b_y * t * t) + (c_y * t) + f64::from(self.start.y)
    }

    /// Returns the approximate arc length of the curve using linear sampling.
    #[must_use] pub fn get_length(&self) -> f64 {
        // NOTE: this arc length parametrization is not very precise, but fast
        let mut arc_length = 0.0;
        let mut prev_point = self.get_start();

        for i in 0..STEP_SIZE {
            let t_next = idx_to_f64(i + 1) * STEP_SIZE_F64;
            let next_point = SvgPoint {
                x: f64_to_f32(self.get_x_at_t(t_next)),
                y: f64_to_f32(self.get_y_at_t(t_next)),
            };
            arc_length += prev_point.distance(next_point);
            prev_point = next_point;
        }

        arc_length
    }

    /// Returns the parameter `t` corresponding to a given arc-length `offset`.
    #[must_use] pub fn get_t_at_offset(&self, offset: f64) -> f64 {
        // step through the line until the offset is reached,
        // then interpolate linearly between the
        // current at the last sampled point
        let mut arc_length = 0.0;
        let mut t_current = 0.0;
        let mut prev_point = self.get_start();

        for i in 0..STEP_SIZE {
            let t_next = idx_to_f64(i + 1) * STEP_SIZE_F64;
            let next_point = SvgPoint {
                x: f64_to_f32(self.get_x_at_t(t_next)),
                y: f64_to_f32(self.get_y_at_t(t_next)),
            };

            let distance = prev_point.distance(next_point);

            arc_length += distance;

            // linearly interpolate between last t and current t
            if arc_length > offset {
                let remaining = arc_length - offset;
                return t_current + ((distance - remaining) / distance) * STEP_SIZE_F64;
            }

            prev_point = next_point;
            t_current = t_next;
        }

        t_current
    }

    /// Returns the normalized tangent vector at parameter `t`.
    #[must_use] pub fn get_tangent_vector_at_t(&self, t: f64) -> SvgVector {
        // 1. Calculate the derivative of the bezier curve.
        //
        // This means that we go from 4 points to 3 points and redistribute
        // the weights of the control points according to the formula:
        //
        // w'0 = 3 * (w1-w0)
        // w'1 = 3 * (w2-w1)
        // w'2 = 3 * (w3-w2)

        let w0 = SvgPoint {
            x: self.ctrl_1.x - self.start.x,
            y: self.ctrl_1.y - self.start.y,
        };

        let w1 = SvgPoint {
            x: self.ctrl_2.x - self.ctrl_1.x,
            y: self.ctrl_2.y - self.ctrl_1.y,
        };

        let w2 = SvgPoint {
            x: self.end.x - self.ctrl_2.x,
            y: self.end.y - self.ctrl_2.y,
        };

        let quadratic_curve = SvgQuadraticCurve {
            start: w0,
            ctrl: w1,
            end: w2,
        };

        // The first derivative of a cubic bezier curve is a quadratic
        // bezier curve. Luckily, the first derivative is also the tangent
        // vector (slope) of the curve. So all we need to do is to sample the
        // quadratic curve at t
        let tangent_vector = SvgVector {
            x: quadratic_curve.get_x_at_t(t),
            y: quadratic_curve.get_y_at_t(t),
        };

        tangent_vector.normalize()
    }

    /// Returns the axis-aligned bounding box of the curve's control points.
    #[must_use] pub fn get_bounds(&self) -> SvgRect {
        let min_x = self
            .start
            .x
            .min(self.end.x)
            .min(self.ctrl_1.x)
            .min(self.ctrl_2.x);
        let max_x = self
            .start
            .x
            .max(self.end.x)
            .max(self.ctrl_1.x)
            .max(self.ctrl_2.x);

        let min_y = self
            .start
            .y
            .min(self.end.y)
            .min(self.ctrl_1.y)
            .min(self.ctrl_2.y);
        let max_y = self
            .start
            .y
            .max(self.end.y)
            .max(self.ctrl_1.y)
            .max(self.ctrl_2.y);

        let width = (max_x - min_x).abs();
        let height = (max_y - min_y).abs();

        SvgRect {
            width,
            height,
            x: min_x,
            y: min_y,
            ..SvgRect::default()
        }
    }
}

impl SvgVector {
    /// Returns the angle of the vector in degrees
    #[inline]
    #[must_use] pub fn angle_degrees(&self) -> f64 {
        (-self.y).atan2(self.x).to_degrees()
    }

    /// Returns a unit-length vector in the same direction, or zero if the length is zero.
    #[inline]
    #[must_use = "returns a new vector"]
    pub fn normalize(&self) -> Self {
        let tangent_length = libm::hypot(self.x, self.y);
        if tangent_length == 0.0 {
            return Self { x: 0.0, y: 0.0 };
        }
        Self {
            x: self.x / tangent_length,
            y: self.y / tangent_length,
        }
    }

    /// Rotate the vector 90 degrees counter-clockwise
    #[must_use = "returns a new vector"]
    #[inline]
    pub fn rotate_90deg_ccw(&self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }
}

// Explicit FP math (mul_add is slower without `+fma`); see SvgCubicCurve.
#[allow(clippy::suboptimal_flops)]
impl SvgQuadraticCurve {
    /// Creates a new `SvgQuadraticCurve` from start, control, and end points
    #[inline]
    #[must_use] pub const fn new(start: SvgPoint, ctrl: SvgPoint, end: SvgPoint) -> Self {
        Self { start, ctrl, end }
    }

    /// Reverses the curve direction in place.
    pub const fn reverse(&mut self) {
        core::mem::swap(&mut self.start, &mut self.end);
    }
    /// Returns the start point of the curve.
    #[must_use] pub const fn get_start(&self) -> SvgPoint {
        self.start
    }
    /// Returns the end point of the curve.
    #[must_use] pub const fn get_end(&self) -> SvgPoint {
        self.end
    }
    /// Returns the axis-aligned bounding box of the curve's control points.
    #[must_use] pub fn get_bounds(&self) -> SvgRect {
        let min_x = self.start.x.min(self.end.x).min(self.ctrl.x);
        let max_x = self.start.x.max(self.end.x).max(self.ctrl.x);

        let min_y = self.start.y.min(self.end.y).min(self.ctrl.y);
        let max_y = self.start.y.max(self.end.y).max(self.ctrl.y);

        let width = (max_x - min_x).abs();
        let height = (max_y - min_y).abs();

        SvgRect {
            width,
            height,
            x: min_x,
            y: min_y,
            ..SvgRect::default()
        }
    }

    /// Evaluates the x coordinate of the curve at parameter `t` in [0, 1].
    #[must_use] pub fn get_x_at_t(&self, t: f64) -> f64 {
        let one_minus = 1.0 - t;
        one_minus * one_minus * f64::from(self.start.x)
            + 2.0 * one_minus * t * f64::from(self.ctrl.x)
            + t * t * f64::from(self.end.x)
    }

    /// Evaluates the y coordinate of the curve at parameter `t` in [0, 1].
    #[must_use] pub fn get_y_at_t(&self, t: f64) -> f64 {
        let one_minus = 1.0 - t;
        one_minus * one_minus * f64::from(self.start.y)
            + 2.0 * one_minus * t * f64::from(self.ctrl.y)
            + t * t * f64::from(self.end.y)
    }

    /// Returns the approximate arc length by converting to a cubic curve.
    #[must_use] pub fn get_length(&self) -> f64 {
        self.to_cubic().get_length()
    }

    /// Returns the parameter `t` corresponding to a given arc-length `offset`.
    #[must_use] pub fn get_t_at_offset(&self, offset: f64) -> f64 {
        self.to_cubic().get_t_at_offset(offset)
    }

    /// Returns the normalized tangent vector at parameter `t`.
    #[must_use] pub fn get_tangent_vector_at_t(&self, t: f64) -> SvgVector {
        self.to_cubic().get_tangent_vector_at_t(t)
    }

    /// Converts this quadratic curve to an equivalent cubic bezier curve.
    fn to_cubic(self) -> SvgCubicCurve {
        SvgCubicCurve {
            start: self.start,
            ctrl_1: SvgPoint {
                x: self.start.x + (2.0 / 3.0) * (self.ctrl.x - self.start.x),
                y: self.start.y + (2.0 / 3.0) * (self.ctrl.y - self.start.y),
            },
            ctrl_2: SvgPoint {
                x: self.end.x + (2.0 / 3.0) * (self.ctrl.x - self.end.x),
                y: self.end.y + (2.0 / 3.0) * (self.ctrl.y - self.end.y),
            },
            end: self.end,
        }
    }
}

impl AnimationInterpolationFunction {
    /// Returns the cubic bezier curve corresponding to this timing function.
    #[must_use]
    pub const fn get_curve(self) -> SvgCubicCurve {
        match self {
            Self::Ease => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.25, y: 0.1 },
                ctrl_2: SvgPoint { x: 0.25, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            Self::Linear => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_2: SvgPoint { x: 1.0, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            Self::EaseIn => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.42, y: 0.0 },
                ctrl_2: SvgPoint { x: 1.0, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            Self::EaseOut => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_2: SvgPoint { x: 0.58, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            Self::EaseInOut => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.42, y: 0.0 },
                ctrl_2: SvgPoint { x: 0.58, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            Self::CubicBezier(c) => c,
        }
    }

    /// Evaluates the interpolation function at time `t`, returning the eased value.
    #[must_use] pub fn evaluate(self, t: f64) -> f32 {
        f64_to_f32(self.get_curve().get_y_at_t(t))
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod autotest_generated {
    use super::*;

    // ---- helpers -----------------------------------------------------------

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    fn approx_f32(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn p(x: f32, y: f32) -> SvgPoint {
        SvgPoint::new(x, y)
    }

    /// A curve whose control points are all exactly representable in binary f32,
    /// so endpoint evaluation is bit-exact.
    fn exact_curve() -> SvgCubicCurve {
        SvgCubicCurve::new(p(0.0, 0.0), p(0.25, 0.5), p(0.75, 0.5), p(1.0, 1.0))
    }

    /// Degenerate curve: every control point identical (zero arc length).
    fn degenerate_curve() -> SvgCubicCurve {
        SvgCubicCurve::new(p(5.0, 5.0), p(5.0, 5.0), p(5.0, 5.0), p(5.0, 5.0))
    }

    const ALL_VARIANTS: [AnimationInterpolationFunction; 5] = [
        AnimationInterpolationFunction::Ease,
        AnimationInterpolationFunction::Linear,
        AnimationInterpolationFunction::EaseIn,
        AnimationInterpolationFunction::EaseOut,
        AnimationInterpolationFunction::EaseInOut,
    ];

    /// Nasty f64 inputs fed to every `t` / `offset` parameter.
    const NASTY_F64: [f64; 12] = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        2.0,
        1e-300,
        1e300,
        f64::MAX,
        f64::MIN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];

    // ---- 1. idx_to_f64 (numeric: zero / min_max / overflow) ----------------

    #[test]
    fn idx_to_f64_zero_and_small_values_are_exact() {
        assert_eq!(idx_to_f64(0), 0.0);
        assert_eq!(idx_to_f64(1), 1.0);
        assert_eq!(idx_to_f64(20), 20.0);
        assert_eq!(idx_to_f64(STEP_SIZE), 20.0);
    }

    #[test]
    fn idx_to_f64_is_strictly_monotonic_over_the_sampling_range() {
        for i in 0..STEP_SIZE {
            assert!(
                idx_to_f64(i + 1) > idx_to_f64(i),
                "not monotonic at i = {i}"
            );
        }
    }

    #[test]
    fn idx_to_f64_at_usize_max_does_not_panic_and_stays_finite() {
        // usize::MAX exceeds f64's 2^53 exact-integer range: the cast must round,
        // not trap. The only guarantee we rely on is "finite, positive, no panic".
        let v = idx_to_f64(usize::MAX);
        assert!(v.is_finite(), "usize::MAX must not become inf/NaN: {v}");
        assert!(v > 0.0);
        assert!(v >= idx_to_f64(STEP_SIZE));
    }

    #[test]
    fn idx_to_f64_covers_the_full_bezier_domain() {
        // The sampling loop relies on STEP_SIZE * STEP_SIZE_F64 == 1.0; if this
        // ever drifts, get_length()/get_t_at_offset() silently truncate the curve.
        assert!(approx(idx_to_f64(STEP_SIZE) * STEP_SIZE_F64, 1.0, 1e-12));
    }

    // ---- 2. f64_to_f32 (numeric: zero / negative / overflow / nan_inf) -----

    #[test]
    fn f64_to_f32_zero_preserves_sign() {
        assert_eq!(f64_to_f32(0.0), 0.0_f32);
        assert!(f64_to_f32(0.0).is_sign_positive());
        assert!(f64_to_f32(-0.0).is_sign_negative());
    }

    #[test]
    fn f64_to_f32_overflow_saturates_to_infinity_not_a_panic() {
        // f64::MAX has no f32 representation: IEEE round-to-nearest gives +-inf.
        assert_eq!(f64_to_f32(f64::MAX), f32::INFINITY);
        assert_eq!(f64_to_f32(f64::MIN), f32::NEG_INFINITY);
        assert_eq!(f64_to_f32(1e300), f32::INFINITY);
        assert_eq!(f64_to_f32(-1e300), f32::NEG_INFINITY);
    }

    #[test]
    fn f64_to_f32_underflow_flushes_to_signed_zero() {
        let tiny = f64_to_f32(1e-300);
        assert_eq!(tiny, 0.0_f32);
        assert!(tiny.is_sign_positive());

        let neg_tiny = f64_to_f32(-1e-300);
        assert_eq!(neg_tiny, 0.0_f32);
        assert!(neg_tiny.is_sign_negative(), "sign must survive underflow");
    }

    #[test]
    fn f64_to_f32_nan_and_inf_are_defined_and_do_not_panic() {
        assert!(f64_to_f32(f64::NAN).is_nan());
        assert_eq!(f64_to_f32(f64::INFINITY), f32::INFINITY);
        assert_eq!(f64_to_f32(f64::NEG_INFINITY), f32::NEG_INFINITY);
    }

    #[test]
    fn f64_to_f32_round_trips_values_that_originate_as_f32() {
        // encode == decode: every f32 widened to f64 must narrow back unchanged.
        for original in [
            0.0_f32,
            1.0,
            -1.0,
            0.25,
            0.1,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            f32::EPSILON,
        ] {
            assert_eq!(
                f64_to_f32(f64::from(original)),
                original,
                "round-trip failed for {original}"
            );
        }
    }

    // ---- 3. SvgPoint::new (constructor) ------------------------------------

    #[test]
    fn svg_point_new_stores_fields_verbatim_including_extremes() {
        for (x, y) in [
            (0.0_f32, 0.0_f32),
            (-1.5, 2.5),
            (f32::MAX, f32::MIN),
            (f32::MIN_POSITIVE, -f32::MIN_POSITIVE),
            (f32::INFINITY, f32::NEG_INFINITY),
        ] {
            let pt = SvgPoint::new(x, y);
            assert_eq!(pt.x, x);
            assert_eq!(pt.y, y);
        }

        let nan_point = SvgPoint::new(f32::NAN, f32::NAN);
        assert!(nan_point.x.is_nan() && nan_point.y.is_nan());
        // NaN != NaN, so a NaN point is not even equal to itself.
        assert_ne!(nan_point, nan_point);
    }

    #[test]
    fn svg_point_default_is_the_origin() {
        assert_eq!(SvgPoint::default(), p(0.0, 0.0));
    }

    // ---- 4. SvgPoint::distance (other) -------------------------------------

    #[test]
    fn distance_basic_values_and_identity() {
        assert_eq!(p(0.0, 0.0).distance(p(3.0, 4.0)), 5.0);
        assert_eq!(p(0.0, 0.0).distance(p(0.0, 0.0)), 0.0);
        assert_eq!(p(-3.0, -4.0).distance(p(0.0, 0.0)), 5.0);
    }

    #[test]
    fn distance_is_symmetric() {
        let a = p(-12.5, 7.25);
        let b = p(3.0, -9.75);
        assert_eq!(a.distance(b), b.distance(a));
    }

    #[test]
    fn distance_overflows_to_infinity_because_the_delta_is_computed_in_f32() {
        // dx = f32::MAX - (-f32::MAX) overflows f32 *before* the f64 widening,
        // so the f64 return type cannot rescue the result. Must be inf, not a panic.
        let d = p(-f32::MAX, 0.0).distance(p(f32::MAX, 0.0));
        assert!(d.is_infinite() && d > 0.0, "expected +inf, got {d}");
    }

    #[test]
    fn distance_between_extreme_corners_never_underreports() {
        // Whatever hypotf does at the top of the f32 range, the distance must be
        // at least as large as the largest single component delta.
        let d = p(0.0, 0.0).distance(p(f32::MAX, f32::MAX));
        assert!(!d.is_nan());
        assert!(d >= f64::from(f32::MAX), "distance underreported: {d}");
    }

    #[test]
    fn distance_with_nan_or_inf_coordinates_does_not_panic() {
        // IEEE-754 / C99: hypot(NaN, inf) == inf, hypot(NaN, finite) == NaN.
        assert!(p(0.0, 0.0).distance(p(f32::NAN, 1.0)).is_nan());
        assert!(p(f32::NAN, f32::NAN).distance(p(0.0, 0.0)).is_nan());
        assert!(p(0.0, 0.0).distance(p(f32::INFINITY, 0.0)).is_infinite());
        assert!(
            p(0.0, 0.0)
                .distance(p(f32::NAN, f32::INFINITY))
                .is_infinite()
        );
    }

    // ---- 5. SvgRect::union_with (other) ------------------------------------

    fn rect(width: f32, height: f32, x: f32, y: f32) -> SvgRect {
        SvgRect {
            width,
            height,
            x,
            y,
            ..SvgRect::default()
        }
    }

    #[test]
    fn union_with_expands_to_cover_both_rects() {
        let mut a = rect(10.0, 10.0, 0.0, 0.0);
        a.union_with(&rect(10.0, 10.0, 20.0, 30.0));
        assert_eq!(a, rect(30.0, 40.0, 0.0, 0.0));
    }

    #[test]
    fn union_with_self_is_idempotent() {
        let mut a = rect(10.0, 20.0, -5.0, -7.0);
        let before = a;
        a.union_with(&before);
        assert_eq!(a, before);
        a.union_with(&before);
        assert_eq!(a, before, "union must be idempotent");
    }

    #[test]
    fn union_with_contained_rect_leaves_the_outer_rect_unchanged() {
        let mut outer = rect(100.0, 100.0, 0.0, 0.0);
        let before = outer;
        outer.union_with(&rect(1.0, 1.0, 50.0, 50.0));
        assert_eq!(outer, before);
    }

    #[test]
    fn union_with_default_rect_always_drags_the_origin_in() {
        // A default SvgRect is a degenerate point at (0,0) - unioning with it is
        // NOT a no-op, it forces the result to contain the origin.
        let mut a = rect(5.0, 5.0, 10.0, 10.0);
        a.union_with(&SvgRect::default());
        assert_eq!(a, rect(15.0, 15.0, 0.0, 0.0));
    }

    #[test]
    fn union_with_nan_rect_is_a_no_op_because_min_max_ignore_nan() {
        // f32::min/max return the non-NaN operand, so a fully poisoned rect
        // cannot corrupt the accumulator. Pin that down.
        let mut a = rect(10.0, 10.0, 0.0, 0.0);
        let before = a;
        a.union_with(&rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN));
        assert_eq!(a, before, "NaN rect must not poison the union");
    }

    #[test]
    fn union_with_infinite_rect_yields_infinite_extent_without_panicking() {
        let mut a = rect(10.0, 10.0, 0.0, 0.0);
        a.union_with(&rect(f32::INFINITY, f32::INFINITY, 0.0, 0.0));
        assert!(a.width.is_infinite() && a.height.is_infinite());
        assert_eq!(a.x, 0.0);
        assert_eq!(a.y, 0.0);
    }

    #[test]
    fn union_with_extreme_opposite_rects_does_not_panic() {
        let mut a = rect(f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        a.union_with(&rect(f32::MAX, f32::MAX, f32::MAX, f32::MAX));
        // max_x - min_x overflows f32 -> inf; the point is that it must not trap.
        assert!(!a.width.is_nan());
        assert!(!a.height.is_nan());
    }

    // ---- 6. SvgRect::contains_point (numeric) ------------------------------

    #[test]
    fn contains_point_is_strictly_exclusive_on_every_edge() {
        let r = rect(10.0, 10.0, 0.0, 0.0);
        assert!(r.contains_point(p(5.0, 5.0)));
        // corners + edges are all *outside* (the impl uses > / <, not >= / <=)
        assert!(!r.contains_point(p(0.0, 0.0)));
        assert!(!r.contains_point(p(10.0, 10.0)));
        assert!(!r.contains_point(p(0.0, 5.0)));
        assert!(!r.contains_point(p(10.0, 5.0)));
        assert!(!r.contains_point(p(5.0, 0.0)));
        assert!(!r.contains_point(p(5.0, 10.0)));
    }

    #[test]
    fn contains_point_zero_sized_rect_contains_nothing() {
        let r = SvgRect::default();
        assert!(!r.contains_point(p(0.0, 0.0)));
        assert!(!r.contains_point(p(1.0, 1.0)));
        assert!(!r.contains_point(p(-1.0, -1.0)));
    }

    #[test]
    fn contains_point_negative_size_rect_contains_nothing() {
        // width < 0 makes `x > self.x && x < self.x + width` unsatisfiable.
        let r = rect(-10.0, -10.0, 0.0, 0.0);
        for pt in [p(0.0, 0.0), p(-5.0, -5.0), p(5.0, 5.0), p(-10.0, -10.0)] {
            assert!(!r.contains_point(pt), "{pt:?} must not be contained");
        }
    }

    #[test]
    fn contains_point_negative_origin_quadrant_works() {
        let r = rect(10.0, 10.0, -20.0, -20.0);
        assert!(r.contains_point(p(-15.0, -15.0)));
        assert!(!r.contains_point(p(-25.0, -15.0)));
        assert!(!r.contains_point(p(0.0, 0.0)));
    }

    #[test]
    fn contains_point_with_nan_coordinates_is_false_not_a_panic() {
        let r = rect(10.0, 10.0, 0.0, 0.0);
        assert!(!r.contains_point(p(f32::NAN, 5.0)));
        assert!(!r.contains_point(p(5.0, f32::NAN)));
        assert!(!r.contains_point(p(f32::NAN, f32::NAN)));

        // ... and a NaN *rect* also swallows everything (all comparisons false).
        let nan_rect = rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN);
        assert!(!nan_rect.contains_point(p(0.0, 0.0)));
    }

    #[test]
    fn contains_point_infinite_rect_contains_finite_points_but_not_infinity() {
        let r = rect(f32::INFINITY, f32::INFINITY, 0.0, 0.0);
        assert!(r.contains_point(p(1e30, 1e30)));
        assert!(!r.contains_point(p(f32::INFINITY, f32::INFINITY)));
        assert!(!r.contains_point(p(-1.0, 1.0)));
    }

    #[test]
    fn contains_point_at_f32_extremes_does_not_panic() {
        let r = rect(f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        // Unlike integers, `f32::MIN == -f32::MAX` exactly, so x + width is exactly
        // 0.0 -- no overflow to +inf. That puts (0,0) exactly ON the rect's corner,
        // and `contains_point` is strictly exclusive on every edge (see
        // `contains_point_is_strictly_exclusive_on_every_edge`), so it is NOT inside.
        let _ = r.contains_point(p(f32::MAX, f32::MAX));
        let _ = r.contains_point(p(f32::MIN, f32::MIN));
        assert!(!r.contains_point(p(0.0, 0.0)));
    }

    // ---- 7. SvgRect::expand (numeric) --------------------------------------

    #[test]
    fn expand_by_zero_is_the_identity() {
        let r = SvgRect {
            width: 10.0,
            height: 20.0,
            x: 1.0,
            y: 2.0,
            radius_top_left: 3.0,
            radius_top_right: 4.0,
            radius_bottom_left: 5.0,
            radius_bottom_right: 6.0,
        };
        assert_eq!(r.expand(0.0, 0.0, 0.0, 0.0), r);
    }

    #[test]
    fn expand_grows_the_rect_and_preserves_the_corner_radii() {
        let r = SvgRect {
            width: 10.0,
            height: 10.0,
            x: 0.0,
            y: 0.0,
            radius_top_left: 3.0,
            radius_top_right: 4.0,
            radius_bottom_left: 5.0,
            radius_bottom_right: 6.0,
        };
        let e = r.expand(1.0, 2.0, 4.0, 8.0);
        assert_eq!(e.width, 10.0 + 4.0 + 8.0);
        assert_eq!(e.height, 10.0 + 1.0 + 2.0);
        assert_eq!(e.x, -4.0);
        assert_eq!(e.y, -1.0);
        // `..*self` must carry the radii over untouched.
        assert_eq!(e.radius_top_left, 3.0);
        assert_eq!(e.radius_top_right, 4.0);
        assert_eq!(e.radius_bottom_left, 5.0);
        assert_eq!(e.radius_bottom_right, 6.0);
    }

    #[test]
    fn expand_with_negative_padding_shrinks_and_may_invert_the_rect() {
        let r = rect(10.0, 10.0, 0.0, 0.0);
        assert_eq!(r.expand(-1.0, -1.0, -1.0, -1.0), rect(8.0, 8.0, 1.0, 1.0));

        // Over-shrinking is *not* clamped: the width goes negative.
        let inverted = r.expand(-100.0, -100.0, -100.0, -100.0);
        assert!(inverted.width < 0.0, "expand does not clamp to zero");
        assert!(!inverted.contains_point(p(5.0, 5.0)));
    }

    #[test]
    fn expand_overflow_saturates_to_infinity_instead_of_panicking() {
        let r = rect(f32::MAX, f32::MAX, 0.0, 0.0);
        let e = r.expand(f32::MAX, f32::MAX, f32::MAX, f32::MAX);
        // width = MAX + MAX + MAX overflows -> +inf ...
        assert!(e.width.is_infinite() && e.width > 0.0);
        assert!(e.height.is_infinite() && e.height > 0.0);
        // ... but the origin is a single subtraction, which stays in range.
        assert_eq!(e.x, -f32::MAX);
        assert_eq!(e.y, -f32::MAX);
        assert!(e.x.is_finite() && e.y.is_finite());
    }

    #[test]
    fn expand_with_nan_padding_poisons_the_rect_but_does_not_panic() {
        let r = rect(10.0, 10.0, 0.0, 0.0);
        let e = r.expand(f32::NAN, 0.0, 0.0, 0.0);
        assert!(e.height.is_nan());
        assert!(e.y.is_nan());
        // NaN dimensions make the rect vacuous rather than crashing consumers.
        assert!(!e.contains_point(p(5.0, 5.0)));
    }

    #[test]
    fn expand_with_infinite_padding_produces_infinite_extent() {
        let r = rect(1.0, 1.0, 0.0, 0.0);
        let e = r.expand(f32::INFINITY, f32::INFINITY, f32::INFINITY, f32::INFINITY);
        assert!(e.width.is_infinite());
        assert!(e.x.is_infinite() && e.x < 0.0);
    }

    // ---- 8. SvgRect::get_center (getter) -----------------------------------

    #[test]
    fn get_center_of_a_known_rect() {
        assert_eq!(rect(10.0, 20.0, 2.0, 4.0).get_center(), p(7.0, 14.0));
        assert_eq!(rect(1.0, 1.0, 0.0, 0.0).get_center(), p(0.5, 0.5));
    }

    #[test]
    fn get_center_of_default_rect_is_the_origin() {
        assert_eq!(SvgRect::default().get_center(), SvgPoint::default());
    }

    #[test]
    fn get_center_of_a_contained_rect_is_inside_it() {
        let r = rect(10.0, 10.0, -3.0, 7.5);
        assert!(r.contains_point(r.get_center()));
    }

    #[test]
    fn get_center_at_extremes_does_not_panic() {
        let inf = rect(f32::INFINITY, f32::INFINITY, 0.0, 0.0).get_center();
        assert!(inf.x.is_infinite() && inf.y.is_infinite());

        // width/2 keeps f32::MAX in range, so no overflow here.
        let huge = rect(f32::MAX, f32::MAX, 0.0, 0.0).get_center();
        assert!(huge.x.is_finite() && huge.y.is_finite());

        let nan = rect(f32::NAN, f32::NAN, 0.0, 0.0).get_center();
        assert!(nan.x.is_nan() && nan.y.is_nan());
    }

    // ---- 9-12. SvgCubicCurve new / reverse / get_start / get_end -----------

    #[test]
    fn cubic_new_stores_all_four_control_points_verbatim() {
        let c = SvgCubicCurve::new(p(1.0, 2.0), p(3.0, 4.0), p(5.0, 6.0), p(7.0, 8.0));
        assert_eq!(c.start, p(1.0, 2.0));
        assert_eq!(c.ctrl_1, p(3.0, 4.0));
        assert_eq!(c.ctrl_2, p(5.0, 6.0));
        assert_eq!(c.end, p(7.0, 8.0));
        assert_eq!(c.get_start(), c.start);
        assert_eq!(c.get_end(), c.end);
    }

    #[test]
    fn cubic_new_accepts_extreme_control_points() {
        let c = SvgCubicCurve::new(
            p(f32::MIN, f32::MAX),
            p(f32::INFINITY, f32::NEG_INFINITY),
            p(f32::MIN_POSITIVE, -0.0),
            p(0.0, 0.0),
        );
        assert!(c.get_start().x.is_finite());
        assert!(c.ctrl_1.x.is_infinite());
        assert_eq!(c.get_end(), p(0.0, 0.0));
    }

    #[test]
    fn cubic_reverse_swaps_the_endpoints_and_the_control_points() {
        let mut c = SvgCubicCurve::new(p(1.0, 2.0), p(3.0, 4.0), p(5.0, 6.0), p(7.0, 8.0));
        c.reverse();
        assert_eq!(c.start, p(7.0, 8.0));
        assert_eq!(c.ctrl_1, p(5.0, 6.0));
        assert_eq!(c.ctrl_2, p(3.0, 4.0));
        assert_eq!(c.end, p(1.0, 2.0));
    }

    #[test]
    fn cubic_reverse_twice_is_the_identity() {
        let original = exact_curve();
        let mut c = original;
        c.reverse();
        assert_ne!(c, original);
        c.reverse();
        assert_eq!(c, original, "reverse must be an involution");
    }

    #[test]
    fn cubic_reverse_mirrors_the_parameterization() {
        // round-trip: reversed(t) == original(1 - t)
        let original = exact_curve();
        let mut reversed = original;
        reversed.reverse();
        for step in 0..=10 {
            let t = f64::from(step) / 10.0;
            assert!(approx(
                reversed.get_x_at_t(t),
                original.get_x_at_t(1.0 - t),
                1e-12
            ));
            assert!(approx(
                reversed.get_y_at_t(t),
                original.get_y_at_t(1.0 - t),
                1e-12
            ));
        }
    }

    #[test]
    fn cubic_reverse_on_a_degenerate_curve_does_not_panic() {
        let mut c = degenerate_curve();
        c.reverse();
        assert_eq!(c, degenerate_curve());
    }

    // ---- 13-14. SvgCubicCurve::get_x_at_t / get_y_at_t (numeric) -----------

    #[test]
    fn cubic_endpoints_are_hit_exactly_at_t_0_and_t_1() {
        let c = exact_curve();
        assert_eq!(c.get_x_at_t(0.0), f64::from(c.start.x));
        assert_eq!(c.get_y_at_t(0.0), f64::from(c.start.y));
        assert!(approx(c.get_x_at_t(1.0), f64::from(c.end.x), 1e-12));
        assert!(approx(c.get_y_at_t(1.0), f64::from(c.end.y), 1e-12));
    }

    #[test]
    fn cubic_negative_zero_t_behaves_like_zero() {
        let c = exact_curve();
        assert_eq!(c.get_x_at_t(-0.0), c.get_x_at_t(0.0));
        assert_eq!(c.get_y_at_t(-0.0), c.get_y_at_t(0.0));
    }

    #[test]
    fn cubic_stays_within_the_control_hull_for_t_in_unit_range() {
        // A bezier curve never leaves the convex hull of its control points.
        let c = exact_curve();
        let bounds = c.get_bounds();
        for step in 0..=20 {
            let t = f64::from(step) / 20.0;
            let x = c.get_x_at_t(t);
            let y = c.get_y_at_t(t);
            assert!(
                x >= f64::from(bounds.x) - 1e-9
                    && x <= f64::from(bounds.x + bounds.width) + 1e-9,
                "x left the hull at t = {t}: {x}"
            );
            assert!(
                y >= f64::from(bounds.y) - 1e-9
                    && y <= f64::from(bounds.y + bounds.height) + 1e-9,
                "y left the hull at t = {t}: {y}"
            );
        }
    }

    #[test]
    fn cubic_evaluation_extrapolates_outside_the_unit_range_without_clamping() {
        // t is NOT clamped: t < 0 / t > 1 extrapolate the polynomial.
        let c = AnimationInterpolationFunction::Linear.get_curve();
        // y(t) = -2t^3 + 3t^2  =>  y(-1) = 5, y(2) = -4
        assert_eq!(c.get_y_at_t(-1.0), 5.0);
        assert_eq!(c.get_y_at_t(2.0), -4.0);
    }

    #[test]
    fn cubic_evaluation_at_nan_and_inf_is_defined_and_never_panics() {
        let c = exact_curve();
        assert!(c.get_x_at_t(f64::NAN).is_nan());
        assert!(c.get_y_at_t(f64::NAN).is_nan());

        for t in NASTY_F64 {
            let x = c.get_x_at_t(t);
            let y = c.get_y_at_t(t);
            // finite t inside [0,1] must produce finite output; everything else
            // may blow up, but only ever into inf/NaN - never into a panic.
            if (0.0..=1.0).contains(&t) {
                assert!(x.is_finite() && y.is_finite(), "finite t={t} gave {x}/{y}");
            }
        }
    }

    #[test]
    fn cubic_evaluation_at_huge_t_overflows_instead_of_returning_a_bogus_finite() {
        let c = AnimationInterpolationFunction::Linear.get_curve();
        for t in [f64::MAX, f64::MIN, 1e300, -1e300, f64::INFINITY] {
            assert!(
                !c.get_x_at_t(t).is_finite(),
                "t = {t} must not produce a finite x"
            );
            assert!(!c.get_y_at_t(t).is_finite());
        }
    }

    #[test]
    fn cubic_with_infinite_control_points_yields_nan_not_a_panic() {
        let c = SvgCubicCurve::new(
            p(f32::INFINITY, 0.0),
            p(0.0, 0.0),
            p(0.0, 0.0),
            p(1.0, 1.0),
        );
        // inf appears in every coefficient -> inf - inf == NaN somewhere.
        assert!(!c.get_x_at_t(0.5).is_finite());
    }

    // ---- 15. SvgCubicCurve::get_length (getter) ----------------------------

    #[test]
    fn cubic_length_of_the_linear_timing_curve_is_the_unit_diagonal() {
        // The Linear curve traces y = x from (0,0) to (1,1) => length = sqrt(2).
        let len = AnimationInterpolationFunction::Linear.get_curve().get_length();
        assert!(
            approx(len, core::f64::consts::SQRT_2, 1e-4),
            "expected ~sqrt(2), got {len}"
        );
    }

    #[test]
    fn cubic_length_of_a_degenerate_curve_is_exactly_zero() {
        assert_eq!(degenerate_curve().get_length(), 0.0);
    }

    #[test]
    fn cubic_length_is_non_negative_and_at_least_the_chord() {
        let c = exact_curve();
        let chord = c.get_start().distance(c.get_end());
        let len = c.get_length();
        assert!(len >= 0.0);
        assert!(
            len >= chord - 1e-6,
            "arc length {len} shorter than chord {chord}"
        );
    }

    #[test]
    fn cubic_length_is_invariant_under_reverse() {
        let mut c = exact_curve();
        let forward = c.get_length();
        c.reverse();
        assert!(approx(c.get_length(), forward, 1e-5));
    }

    #[test]
    fn cubic_length_at_extremes_does_not_panic() {
        let inf = SvgCubicCurve::new(
            p(f32::MIN, f32::MIN),
            p(0.0, 0.0),
            p(0.0, 0.0),
            p(f32::MAX, f32::MAX),
        )
        .get_length();
        assert!(!inf.is_nan());
        assert!(inf > 0.0);

        let nan = SvgCubicCurve::new(
            p(f32::NAN, f32::NAN),
            p(0.0, 0.0),
            p(0.0, 0.0),
            p(1.0, 1.0),
        )
        .get_length();
        assert!(nan.is_nan() || nan >= 0.0);
    }

    // ---- 16. SvgCubicCurve::get_t_at_offset (numeric) ----------------------

    #[test]
    fn cubic_t_at_offset_zero_is_zero() {
        let c = AnimationInterpolationFunction::Linear.get_curve();
        assert_eq!(c.get_t_at_offset(0.0), 0.0);
    }

    #[test]
    fn cubic_t_at_half_length_is_the_midpoint_of_the_linear_curve() {
        // The Linear curve is symmetric around t = 0.5, so half the arc length
        // must map back to t ~ 0.5 (within one sampling step of 0.05).
        let c = AnimationInterpolationFunction::Linear.get_curve();
        let t = c.get_t_at_offset(c.get_length() / 2.0);
        assert!(approx(t, 0.5, 0.06), "expected t ~ 0.5, got {t}");
    }

    #[test]
    fn cubic_t_at_offset_is_monotonic_and_bounded_across_the_curve() {
        let c = exact_curve();
        let len = c.get_length();
        let mut prev = f64::NEG_INFINITY;
        for step in 0..=10 {
            let offset = len * f64::from(step) / 10.0;
            let t = c.get_t_at_offset(offset);
            assert!(t >= -1e-9 && t <= 1.0 + 1e-9, "t out of range: {t}");
            assert!(t >= prev - 1e-9, "t went backwards: {prev} -> {t}");
            prev = t;
        }
    }

    #[test]
    fn cubic_t_at_offset_beyond_the_curve_saturates_at_one() {
        let c = AnimationInterpolationFunction::Linear.get_curve();
        for offset in [10.0, 1e300, f64::MAX, f64::INFINITY] {
            let t = c.get_t_at_offset(offset);
            assert!(
                approx(t, 1.0, 1e-9),
                "offset {offset} should saturate at t = 1, got {t}"
            );
        }
    }

    #[test]
    fn cubic_t_at_offset_with_nan_falls_through_to_one() {
        // `arc_length > NaN` is always false, so the loop runs to completion and
        // returns the final t. Deterministic (never NaN), which is what matters.
        let c = AnimationInterpolationFunction::Linear.get_curve();
        let t = c.get_t_at_offset(f64::NAN);
        assert!(!t.is_nan(), "NaN offset must not leak into the result");
        assert!(approx(t, 1.0, 1e-9), "got {t}");
    }

    #[test]
    fn cubic_t_at_negative_offset_extrapolates_backwards_without_clamping() {
        // Not clamped to 0: the linear interpolation runs backwards past the start.
        let c = AnimationInterpolationFunction::Linear.get_curve();
        let t = c.get_t_at_offset(-1.0);
        assert!(t.is_finite(), "expected a finite (negative) t, got {t}");
        assert!(t < 0.0, "negative offset should yield t < 0, got {t}");
    }

    #[test]
    fn cubic_t_at_offset_on_a_degenerate_curve_divides_by_zero_but_does_not_panic() {
        // Every sample distance is 0. With a negative offset the guard
        // `arc_length > offset` fires and (distance - remaining) / distance
        // becomes -1.0 / 0.0 => -inf. It must stay a float edge case, not a trap.
        let c = degenerate_curve();
        let t = c.get_t_at_offset(-1.0);
        assert!(
            t.is_infinite() && t < 0.0,
            "zero-length curve + negative offset should give -inf, got {t}"
        );

        // A zero offset never trips the guard, so the loop runs out at t = 1.
        let t0 = c.get_t_at_offset(0.0);
        assert!(approx(t0, 1.0, 1e-9), "got {t0}");
        assert!(!t0.is_nan());
    }

    #[test]
    fn cubic_t_at_offset_survives_every_nasty_input() {
        let c = exact_curve();
        for offset in NASTY_F64 {
            let t = c.get_t_at_offset(offset);
            // The only hard requirement: no panic, and non-negative offsets
            // never produce NaN.
            if offset >= 0.0 {
                assert!(!t.is_nan(), "offset {offset} produced NaN");
            }
        }
    }

    // ---- 17. SvgCubicCurve::get_tangent_vector_at_t (numeric) --------------

    #[test]
    fn cubic_tangent_of_the_linear_curve_points_along_the_diagonal() {
        let c = AnimationInterpolationFunction::Linear.get_curve();
        let v = c.get_tangent_vector_at_t(0.5);
        let expected = core::f64::consts::FRAC_1_SQRT_2;
        assert!(approx(v.x, expected, 1e-12), "x = {}", v.x);
        assert!(approx(v.y, expected, 1e-12), "y = {}", v.y);
    }

    #[test]
    fn cubic_tangent_is_a_unit_vector_or_exactly_zero() {
        let c = exact_curve();
        for step in 0..=20 {
            let t = f64::from(step) / 20.0;
            let v = c.get_tangent_vector_at_t(t);
            let len = libm::hypot(v.x, v.y);
            assert!(
                len == 0.0 || approx(len, 1.0, 1e-9),
                "tangent at t = {t} has length {len}"
            );
        }
    }

    #[test]
    fn cubic_tangent_at_a_cusp_degenerates_to_the_zero_vector() {
        // Linear's derivative vanishes at t = 0 and t = 1 (ctrl_1 == start,
        // ctrl_2 == end), so normalize() must hand back (0, 0), not NaN.
        let c = AnimationInterpolationFunction::Linear.get_curve();
        for t in [0.0, 1.0] {
            let v = c.get_tangent_vector_at_t(t);
            assert_eq!(v.x, 0.0, "t = {t}");
            assert_eq!(v.y, 0.0, "t = {t}");
        }
    }

    #[test]
    fn cubic_tangent_of_a_degenerate_curve_is_the_zero_vector() {
        let v = degenerate_curve().get_tangent_vector_at_t(0.5);
        assert_eq!(v.x, 0.0);
        assert_eq!(v.y, 0.0);
    }

    #[test]
    fn cubic_tangent_at_nan_t_is_nan_not_a_panic() {
        let v = exact_curve().get_tangent_vector_at_t(f64::NAN);
        assert!(v.x.is_nan() && v.y.is_nan());
    }

    #[test]
    fn cubic_tangent_survives_every_nasty_t() {
        let c = exact_curve();
        for t in NASTY_F64 {
            let v = c.get_tangent_vector_at_t(t);
            // normalize() may only ever emit values in [-1, 1] - or NaN.
            assert!(
                v.x.is_nan() || (-1.0..=1.0).contains(&v.x),
                "t = {t} gave x = {}",
                v.x
            );
            assert!(
                v.y.is_nan() || (-1.0..=1.0).contains(&v.y),
                "t = {t} gave y = {}",
                v.y
            );
        }
    }

    // ---- 18. SvgCubicCurve::get_bounds (getter) ----------------------------

    #[test]
    fn cubic_bounds_of_a_known_curve() {
        let c = AnimationInterpolationFunction::Linear.get_curve();
        assert_eq!(c.get_bounds(), rect(1.0, 1.0, 0.0, 0.0));
    }

    #[test]
    fn cubic_bounds_are_never_negative_and_ignore_the_radii() {
        let c = SvgCubicCurve::new(p(10.0, 10.0), p(-5.0, 30.0), p(0.0, -2.0), p(3.0, 3.0));
        let b = c.get_bounds();
        assert_eq!(b.x, -5.0);
        assert_eq!(b.y, -2.0);
        assert_eq!(b.width, 15.0);
        assert_eq!(b.height, 32.0);
        assert!(b.width >= 0.0 && b.height >= 0.0);
        assert_eq!(b.radius_top_left, 0.0);
        assert_eq!(b.radius_bottom_right, 0.0);
    }

    #[test]
    fn cubic_bounds_of_a_degenerate_curve_are_a_zero_size_rect() {
        let b = degenerate_curve().get_bounds();
        assert_eq!(b, rect(0.0, 0.0, 5.0, 5.0));
    }

    #[test]
    fn cubic_bounds_contain_every_sampled_curve_point() {
        let c = exact_curve();
        let b = c.get_bounds();
        for step in 1..20 {
            let t = f64::from(step) / 20.0;
            let pt = p(
                f64_to_f32(c.get_x_at_t(t)),
                f64_to_f32(c.get_y_at_t(t)),
            );
            assert!(
                pt.x >= b.x && pt.x <= b.x + b.width,
                "x outside bounds at t = {t}"
            );
            assert!(
                pt.y >= b.y && pt.y <= b.y + b.height,
                "y outside bounds at t = {t}"
            );
        }
    }

    #[test]
    fn cubic_bounds_with_infinite_points_do_not_panic() {
        let c = SvgCubicCurve::new(
            p(f32::NEG_INFINITY, 0.0),
            p(0.0, 0.0),
            p(0.0, 0.0),
            p(f32::INFINITY, 1.0),
        );
        let b = c.get_bounds();
        assert!(b.width.is_infinite());
        assert!(b.x.is_infinite() && b.x < 0.0);
    }

    #[test]
    fn cubic_bounds_ignore_nan_control_points() {
        // f32::min/max discard NaN, so the box collapses onto the finite points.
        let c = SvgCubicCurve::new(
            p(0.0, 0.0),
            p(f32::NAN, f32::NAN),
            p(2.0, 4.0),
            p(1.0, 1.0),
        );
        let b = c.get_bounds();
        assert!(!b.width.is_nan(), "NaN leaked into the bounds width");
        assert_eq!(b.x, 0.0);
        assert_eq!(b.width, 2.0);
        assert_eq!(b.height, 4.0);
    }

    // ---- 19. SvgVector::angle_degrees (getter) -----------------------------

    fn vec2(x: f64, y: f64) -> SvgVector {
        SvgVector { x, y }
    }

    #[test]
    fn angle_degrees_of_the_cardinal_directions() {
        // NB: y is screen-space (down is positive), so the impl negates it.
        assert!(approx(vec2(1.0, 0.0).angle_degrees(), 0.0, 1e-12));
        assert!(approx(vec2(0.0, -1.0).angle_degrees(), 90.0, 1e-12));
        assert!(approx(vec2(0.0, 1.0).angle_degrees(), -90.0, 1e-12));
        assert!(approx(vec2(1.0, -1.0).angle_degrees(), 45.0, 1e-12));
        assert!(approx(vec2(-1.0, 0.0).angle_degrees().abs(), 180.0, 1e-12));
    }

    #[test]
    fn angle_degrees_is_always_within_plus_minus_180() {
        for (x, y) in [
            (1.0, 2.0),
            (-1.0, -2.0),
            (1e300, -1e300),
            (1e-300, 1e-300),
            (f64::MAX, f64::MIN),
        ] {
            let a = vec2(x, y).angle_degrees();
            assert!(
                (-180.0..=180.0).contains(&a),
                "angle out of range for ({x}, {y}): {a}"
            );
        }
    }

    #[test]
    fn angle_degrees_of_the_zero_vector_is_defined() {
        // atan2(-0.0, 0.0) == -0.0 -> 0 degrees. Must not be NaN.
        let a = vec2(0.0, 0.0).angle_degrees();
        assert!(!a.is_nan());
        assert_eq!(a, 0.0);
    }

    #[test]
    fn angle_degrees_of_infinite_vectors_is_finite() {
        // atan2(-inf, inf) == -pi/4
        let a = vec2(f64::INFINITY, f64::INFINITY).angle_degrees();
        assert!(approx(a, -45.0, 1e-12), "got {a}");
    }

    #[test]
    fn angle_degrees_of_nan_is_nan_not_a_panic() {
        assert!(vec2(f64::NAN, 1.0).angle_degrees().is_nan());
        assert!(vec2(1.0, f64::NAN).angle_degrees().is_nan());
    }

    // ---- 20. SvgVector::normalize (getter) ---------------------------------

    #[test]
    fn normalize_of_a_known_vector() {
        let v = vec2(3.0, 4.0).normalize();
        assert!(approx(v.x, 0.6, 1e-12));
        assert!(approx(v.y, 0.8, 1e-12));
        assert!(approx(libm::hypot(v.x, v.y), 1.0, 1e-12));
    }

    #[test]
    fn normalize_of_the_zero_vector_returns_zero_not_nan() {
        let v = vec2(0.0, 0.0).normalize();
        assert_eq!(v.x, 0.0);
        assert_eq!(v.y, 0.0);

        let v = vec2(-0.0, -0.0).normalize();
        assert!(!v.x.is_nan() && !v.y.is_nan());
    }

    #[test]
    fn normalize_is_idempotent() {
        let once = vec2(-7.0, 24.0).normalize();
        let twice = once.normalize();
        assert!(approx(once.x, twice.x, 1e-12));
        assert!(approx(once.y, twice.y, 1e-12));
    }

    #[test]
    fn normalize_of_a_tiny_vector_does_not_underflow_to_zero() {
        let v = vec2(f64::MIN_POSITIVE, 0.0).normalize();
        assert!(approx(v.x, 1.0, 1e-12), "tiny vector collapsed: {}", v.x);
        assert_eq!(v.y, 0.0);
    }

    #[test]
    fn normalize_of_a_huge_vector_stays_bounded() {
        // hypot(MAX, MAX) overflows f64, so the result is either the unit vector
        // (if hypot rescales) or exactly zero (if the length saturates to inf).
        // Either way it must stay bounded and symmetric - never NaN or > 1.
        let v = vec2(f64::MAX, f64::MAX).normalize();
        assert!(!v.x.is_nan() && !v.y.is_nan());
        assert_eq!(v.x, v.y, "symmetry broken");
        let len = libm::hypot(v.x, v.y);
        assert!(
            len == 0.0 || approx(len, 1.0, 1e-9),
            "normalize returned a non-unit, non-zero vector of length {len}"
        );
    }

    #[test]
    fn normalize_of_an_infinite_vector_yields_nan_not_a_panic() {
        // hypot(inf, 1) == inf  =>  inf / inf == NaN, 1 / inf == 0.
        let v = vec2(f64::INFINITY, 1.0).normalize();
        assert!(v.x.is_nan(), "expected NaN, got {}", v.x);
        assert_eq!(v.y, 0.0);
    }

    #[test]
    fn normalize_of_a_nan_vector_is_nan_not_a_panic() {
        let v = vec2(f64::NAN, 0.0).normalize();
        assert!(v.x.is_nan());
    }

    // ---- 21. SvgVector::rotate_90deg_ccw (getter) --------------------------

    #[test]
    fn rotate_90deg_ccw_of_the_cardinal_directions() {
        let v = vec2(1.0, 0.0).rotate_90deg_ccw();
        assert_eq!(v.x, 0.0); // -0.0 == 0.0
        assert_eq!(v.y, 1.0);

        let v = vec2(0.0, 1.0).rotate_90deg_ccw();
        assert_eq!(v.x, -1.0);
        assert_eq!(v.y, 0.0);
    }

    #[test]
    fn rotate_90deg_ccw_four_times_is_the_identity() {
        let original = vec2(1.5, -2.5);
        let v = original
            .rotate_90deg_ccw()
            .rotate_90deg_ccw()
            .rotate_90deg_ccw()
            .rotate_90deg_ccw();
        assert_eq!(v, original);
    }

    #[test]
    fn rotate_90deg_ccw_preserves_length_and_turns_by_90_degrees() {
        let original = vec2(3.0, 4.0);
        let rotated = original.rotate_90deg_ccw();
        assert_eq!(
            libm::hypot(original.x, original.y),
            libm::hypot(rotated.x, rotated.y)
        );
        // dot product of perpendicular vectors is zero
        assert_eq!(original.x * rotated.x + original.y * rotated.y, 0.0);
    }

    #[test]
    fn rotate_90deg_ccw_of_extremes_does_not_panic() {
        let v = vec2(f64::MAX, f64::MIN).rotate_90deg_ccw();
        assert_eq!(v.x, f64::MAX);
        assert_eq!(v.y, f64::MAX);

        let v = vec2(f64::NAN, f64::INFINITY).rotate_90deg_ccw();
        assert!(v.x.is_infinite() && v.x < 0.0);
        assert!(v.y.is_nan());
    }

    // ---- 22-26. SvgQuadraticCurve new / reverse / getters ------------------

    fn quad() -> SvgQuadraticCurve {
        SvgQuadraticCurve::new(p(0.0, 0.0), p(10.0, 20.0), p(30.0, 0.0))
    }

    #[test]
    fn quadratic_new_stores_all_three_control_points_verbatim() {
        let q = SvgQuadraticCurve::new(p(1.0, 2.0), p(3.0, 4.0), p(5.0, 6.0));
        assert_eq!(q.start, p(1.0, 2.0));
        assert_eq!(q.ctrl, p(3.0, 4.0));
        assert_eq!(q.end, p(5.0, 6.0));
        assert_eq!(q.get_start(), q.start);
        assert_eq!(q.get_end(), q.end);
    }

    #[test]
    fn quadratic_new_accepts_extreme_control_points() {
        let q = SvgQuadraticCurve::new(
            p(f32::MAX, f32::MIN),
            p(f32::INFINITY, f32::NAN),
            p(0.0, 0.0),
        );
        assert_eq!(q.get_start().x, f32::MAX);
        assert!(q.ctrl.y.is_nan());
        assert_eq!(q.get_end(), p(0.0, 0.0));
    }

    #[test]
    fn quadratic_reverse_swaps_only_the_endpoints() {
        let mut q = quad();
        q.reverse();
        assert_eq!(q.start, p(30.0, 0.0));
        assert_eq!(q.ctrl, p(10.0, 20.0), "ctrl must stay put");
        assert_eq!(q.end, p(0.0, 0.0));
    }

    #[test]
    fn quadratic_reverse_twice_is_the_identity() {
        let mut q = quad();
        q.reverse();
        q.reverse();
        assert_eq!(q, quad(), "reverse must be an involution");
    }

    #[test]
    fn quadratic_reverse_mirrors_the_parameterization() {
        let original = quad();
        let mut reversed = original;
        reversed.reverse();
        for step in 0..=10 {
            let t = f64::from(step) / 10.0;
            assert!(approx(
                reversed.get_x_at_t(t),
                original.get_x_at_t(1.0 - t),
                1e-12
            ));
            assert!(approx(
                reversed.get_y_at_t(t),
                original.get_y_at_t(1.0 - t),
                1e-12
            ));
        }
    }

    #[test]
    fn quadratic_bounds_of_a_known_curve_are_the_control_hull_not_the_tight_box() {
        let q = SvgQuadraticCurve::new(p(0.0, 0.0), p(5.0, -10.0), p(10.0, 0.0));
        let b = q.get_bounds();
        assert_eq!(b, rect(10.0, 10.0, 0.0, -10.0));

        // The curve itself only reaches y = -5 at its apex: get_bounds() is the
        // control polygon, deliberately looser than the true extent.
        assert_eq!(q.get_y_at_t(0.5), -5.0);
        assert!(b.contains_point(p(5.0, -5.0)));
    }

    #[test]
    fn quadratic_bounds_of_a_degenerate_curve_are_zero_sized() {
        let q = SvgQuadraticCurve::new(p(2.0, 3.0), p(2.0, 3.0), p(2.0, 3.0));
        assert_eq!(q.get_bounds(), rect(0.0, 0.0, 2.0, 3.0));
    }

    #[test]
    fn quadratic_bounds_ignore_nan_and_survive_infinities() {
        let q = SvgQuadraticCurve::new(p(0.0, 0.0), p(f32::NAN, f32::NAN), p(4.0, 8.0));
        let b = q.get_bounds();
        assert!(!b.width.is_nan());
        assert_eq!(b, rect(4.0, 8.0, 0.0, 0.0));

        let q = SvgQuadraticCurve::new(p(f32::NEG_INFINITY, 0.0), p(0.0, 0.0), p(1.0, 1.0));
        assert!(q.get_bounds().width.is_infinite());
    }

    // ---- 27-28. SvgQuadraticCurve::get_x_at_t / get_y_at_t (numeric) -------

    #[test]
    fn quadratic_endpoints_are_hit_exactly() {
        let q = quad();
        assert_eq!(q.get_x_at_t(0.0), f64::from(q.start.x));
        assert_eq!(q.get_y_at_t(0.0), f64::from(q.start.y));
        assert_eq!(q.get_x_at_t(1.0), f64::from(q.end.x));
        assert_eq!(q.get_y_at_t(1.0), f64::from(q.end.y));
    }

    #[test]
    fn quadratic_midpoint_matches_the_closed_form() {
        // B(0.5) = (start + 2*ctrl + end) / 4
        let q = quad();
        let expected_x = (f64::from(q.start.x) + 2.0 * f64::from(q.ctrl.x) + f64::from(q.end.x)) / 4.0;
        let expected_y = (f64::from(q.start.y) + 2.0 * f64::from(q.ctrl.y) + f64::from(q.end.y)) / 4.0;
        assert!(approx(q.get_x_at_t(0.5), expected_x, 1e-12));
        assert!(approx(q.get_y_at_t(0.5), expected_y, 1e-12));
    }

    #[test]
    fn quadratic_extrapolates_outside_the_unit_range_without_clamping() {
        let q = SvgQuadraticCurve::new(p(0.0, 0.0), p(0.0, 0.0), p(1.0, 1.0));
        // B(t) = t^2  =>  B(2) = 4, B(-1) = 1
        assert_eq!(q.get_x_at_t(2.0), 4.0);
        assert_eq!(q.get_x_at_t(-1.0), 1.0);
    }

    #[test]
    fn quadratic_evaluation_at_nan_and_inf_never_panics() {
        let q = quad();
        assert!(q.get_x_at_t(f64::NAN).is_nan());
        assert!(q.get_y_at_t(f64::NAN).is_nan());
        for t in NASTY_F64 {
            let x = q.get_x_at_t(t);
            let y = q.get_y_at_t(t);
            if (0.0..=1.0).contains(&t) {
                assert!(x.is_finite() && y.is_finite(), "finite t={t} gave {x}/{y}");
            }
        }
    }

    #[test]
    fn quadratic_evaluation_at_huge_t_overflows_rather_than_lying() {
        let q = quad();
        for t in [f64::MAX, f64::MIN, 1e300, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                !q.get_x_at_t(t).is_finite(),
                "t = {t} must not produce a finite x"
            );
        }
    }

    // ---- 29-31. SvgQuadraticCurve length / t_at_offset / tangent -----------

    #[test]
    fn quadratic_length_of_a_straight_line_matches_the_chord() {
        // A quadratic with the ctrl point on the chord traces a straight line.
        let q = SvgQuadraticCurve::new(p(0.0, 0.0), p(1.5, 2.0), p(3.0, 4.0));
        assert!(approx(q.get_length(), 5.0, 1e-3), "got {}", q.get_length());
    }

    #[test]
    fn quadratic_length_of_a_degenerate_curve_is_zero() {
        let q = SvgQuadraticCurve::new(p(1.0, 1.0), p(1.0, 1.0), p(1.0, 1.0));
        assert_eq!(q.get_length(), 0.0);
    }

    #[test]
    fn quadratic_length_is_at_least_the_chord_and_invariant_under_reverse() {
        let mut q = quad();
        let len = q.get_length();
        let chord = q.get_start().distance(q.get_end());
        assert!(len >= chord - 1e-6, "arc {len} < chord {chord}");
        q.reverse();
        assert!(approx(q.get_length(), len, 1e-4));
    }

    #[test]
    fn quadratic_t_at_offset_zero_is_zero_and_huge_saturates_at_one() {
        let q = quad();
        assert_eq!(q.get_t_at_offset(0.0), 0.0);
        assert!(approx(q.get_t_at_offset(f64::MAX), 1.0, 1e-9));
        assert!(approx(q.get_t_at_offset(f64::INFINITY), 1.0, 1e-9));
    }

    #[test]
    fn quadratic_t_at_offset_with_nan_is_deterministic() {
        let t = quad().get_t_at_offset(f64::NAN);
        assert!(!t.is_nan());
        assert!(approx(t, 1.0, 1e-9), "got {t}");
    }

    #[test]
    fn quadratic_t_at_offset_is_monotonic_and_bounded() {
        let q = quad();
        let len = q.get_length();
        let mut prev = f64::NEG_INFINITY;
        for step in 0..=10 {
            let t = q.get_t_at_offset(len * f64::from(step) / 10.0);
            assert!(t >= -1e-9 && t <= 1.0 + 1e-9, "t out of range: {t}");
            assert!(t >= prev - 1e-9, "t went backwards: {prev} -> {t}");
            prev = t;
        }
    }

    #[test]
    fn quadratic_tangent_is_unit_length_or_zero() {
        let q = quad();
        for step in 0..=20 {
            let t = f64::from(step) / 20.0;
            let v = q.get_tangent_vector_at_t(t);
            let len = libm::hypot(v.x, v.y);
            assert!(
                len == 0.0 || approx(len, 1.0, 1e-9),
                "tangent at t = {t} has length {len}"
            );
        }
    }

    #[test]
    fn quadratic_tangent_of_a_straight_line_is_constant() {
        let q = SvgQuadraticCurve::new(p(0.0, 0.0), p(1.5, 2.0), p(3.0, 4.0));
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let v = q.get_tangent_vector_at_t(t);
            assert!(approx(v.x, 0.6, 1e-6), "t = {t}: x = {}", v.x);
            assert!(approx(v.y, 0.8, 1e-6), "t = {t}: y = {}", v.y);
        }
    }

    #[test]
    fn quadratic_tangent_at_nan_t_is_nan_not_a_panic() {
        let v = quad().get_tangent_vector_at_t(f64::NAN);
        assert!(v.x.is_nan() && v.y.is_nan());
    }

    // ---- 32. SvgQuadraticCurve::to_cubic (private) -------------------------

    #[test]
    fn to_cubic_preserves_the_endpoints() {
        let q = quad();
        let c = q.to_cubic();
        assert_eq!(c.start, q.start);
        assert_eq!(c.end, q.end);
    }

    #[test]
    fn to_cubic_produces_an_equivalent_curve() {
        // Degree elevation must not change the traced path:
        // C(t) == Q(t) for every t (within f32 control-point rounding).
        let q = quad();
        let c = q.to_cubic();
        for step in 0..=20 {
            let t = f64::from(step) / 20.0;
            assert!(
                approx(c.get_x_at_t(t), q.get_x_at_t(t), 1e-4),
                "x mismatch at t = {t}: {} vs {}",
                c.get_x_at_t(t),
                q.get_x_at_t(t)
            );
            assert!(
                approx(c.get_y_at_t(t), q.get_y_at_t(t), 1e-4),
                "y mismatch at t = {t}: {} vs {}",
                c.get_y_at_t(t),
                q.get_y_at_t(t)
            );
        }
    }

    #[test]
    fn to_cubic_of_a_degenerate_curve_is_degenerate() {
        let q = SvgQuadraticCurve::new(p(7.0, 7.0), p(7.0, 7.0), p(7.0, 7.0));
        let c = q.to_cubic();
        assert_eq!(c.start, p(7.0, 7.0));
        assert_eq!(c.ctrl_1, p(7.0, 7.0));
        assert_eq!(c.ctrl_2, p(7.0, 7.0));
        assert_eq!(c.end, p(7.0, 7.0));
        assert_eq!(c.get_length(), 0.0);
    }

    #[test]
    fn to_cubic_with_extreme_points_does_not_panic() {
        let q = SvgQuadraticCurve::new(p(f32::MIN, 0.0), p(f32::MAX, 0.0), p(0.0, 0.0));
        let c = q.to_cubic();
        // ctrl_1.x = MIN + (2/3) * (MAX - MIN); the inner (MAX - MIN) overflows
        // f32 to +inf, so the elevated control point escapes to +inf rather than
        // trapping. The endpoints are copied verbatim and stay exact.
        assert!(
            c.ctrl_1.x.is_infinite() && c.ctrl_1.x > 0.0,
            "expected +inf, got {}",
            c.ctrl_1.x
        );
        // ctrl_2.x = 0 + (2/3) * MAX stays in range.
        assert!(c.ctrl_2.x.is_finite() && c.ctrl_2.x > 0.0);
        assert_eq!(c.start, p(f32::MIN, 0.0));
        assert_eq!(c.end, p(0.0, 0.0));

        let q = SvgQuadraticCurve::new(p(f32::NAN, 0.0), p(0.0, 0.0), p(1.0, 1.0));
        assert!(q.to_cubic().ctrl_1.x.is_nan());
    }

    // ---- 33. AnimationInterpolationFunction::get_curve ---------------------

    #[test]
    fn get_curve_round_trips_a_custom_cubic_bezier() {
        // encode == decode
        let custom = SvgCubicCurve::new(p(0.0, 0.0), p(0.1, 0.9), p(0.9, 0.1), p(1.0, 1.0));
        assert_eq!(
            AnimationInterpolationFunction::CubicBezier(custom).get_curve(),
            custom
        );
    }

    #[test]
    fn get_curve_round_trips_even_a_nonsensical_cubic_bezier() {
        let nasty = SvgCubicCurve::new(
            p(f32::NAN, f32::INFINITY),
            p(f32::MAX, f32::MIN),
            p(-0.0, 0.0),
            p(1e30, -1e30),
        );
        let out = AnimationInterpolationFunction::CubicBezier(nasty).get_curve();
        // NaN breaks PartialEq, so compare field-wise.
        assert!(out.start.x.is_nan());
        assert!(out.start.y.is_infinite());
        assert_eq!(out.ctrl_1, nasty.ctrl_1);
        assert_eq!(out.end, nasty.end);
    }

    #[test]
    fn every_builtin_timing_curve_runs_from_0_0_to_1_1() {
        for f in ALL_VARIANTS {
            let c = f.get_curve();
            assert_eq!(c.get_start(), p(0.0, 0.0), "{f:?} does not start at (0,0)");
            assert_eq!(c.get_end(), p(1.0, 1.0), "{f:?} does not end at (1,1)");
        }
    }

    #[test]
    fn every_builtin_timing_curve_keeps_its_control_points_in_the_unit_box() {
        // CSS requires the x of both control points to sit in [0, 1].
        for f in ALL_VARIANTS {
            let c = f.get_curve();
            for ctrl in [c.ctrl_1, c.ctrl_2] {
                assert!(
                    (0.0..=1.0).contains(&ctrl.x),
                    "{f:?} has an out-of-range ctrl x: {}",
                    ctrl.x
                );
                assert!((0.0..=1.0).contains(&ctrl.y), "{f:?}: {}", ctrl.y);
            }
        }
    }

    // ---- 34. AnimationInterpolationFunction::evaluate (numeric) ------------

    #[test]
    fn evaluate_at_the_endpoints_is_exactly_0_and_1() {
        for f in ALL_VARIANTS {
            assert_eq!(f.evaluate(0.0), 0.0, "{f:?} at t = 0");
            assert!(
                approx_f32(f.evaluate(1.0), 1.0, 1e-6),
                "{f:?} at t = 1: {}",
                f.evaluate(1.0)
            );
        }
    }

    #[test]
    fn evaluate_is_monotonically_non_decreasing_on_the_unit_interval() {
        for f in ALL_VARIANTS {
            let mut prev = f32::NEG_INFINITY;
            for step in 0..=100 {
                let t = f64::from(step) / 100.0;
                let v = f.evaluate(t);
                assert!(v >= prev - 1e-6, "{f:?} went backwards at t = {t}");
                prev = v;
            }
        }
    }

    #[test]
    fn evaluate_stays_within_0_1_on_the_unit_interval() {
        for f in ALL_VARIANTS {
            for step in 0..=100 {
                let t = f64::from(step) / 100.0;
                let v = f.evaluate(t);
                assert!(
                    (-1e-6..=1.0 + 1e-6).contains(&v),
                    "{f:?} left [0,1] at t = {t}: {v}"
                );
            }
        }
    }

    #[test]
    fn evaluate_samples_the_curve_by_parameter_t_not_by_progress_x() {
        // ADVERSARIAL / SPEC NOTE: `evaluate` feeds `t` straight into the bezier's
        // *parameter*, instead of solving x(t) == t first (as CSS timing functions
        // require). The observable consequence pinned here: `Linear` is not linear.
        // y(t) = -2t^3 + 3t^2  =>  y(0.25) = 0.15625, not 0.25.
        let linear = AnimationInterpolationFunction::Linear;
        assert_eq!(linear.evaluate(0.5), 0.5);
        assert_eq!(linear.evaluate(0.25), 0.15625);
        assert_eq!(linear.evaluate(0.75), 0.84375);
        assert!(
            linear.evaluate(0.25) != 0.25,
            "if this ever becomes 0.25, evaluate() started doing the x-inversion"
        );
    }

    #[test]
    fn evaluate_cannot_distinguish_four_of_the_five_timing_functions() {
        // ADVERSARIAL / SPEC NOTE: Linear, EaseIn, EaseOut and EaseInOut all share
        // the same *y* control points (0, 0, 1, 1) and differ only in x. Because
        // `evaluate` never inverts x, all four collapse onto the same output.
        // Only `Ease` (ctrl_1.y = 0.1) differs.
        let same = [
            AnimationInterpolationFunction::Linear,
            AnimationInterpolationFunction::EaseIn,
            AnimationInterpolationFunction::EaseOut,
            AnimationInterpolationFunction::EaseInOut,
        ];
        for step in 0..=10 {
            let t = f64::from(step) / 10.0;
            let reference = same[0].evaluate(t);
            for f in same {
                assert_eq!(f.evaluate(t), reference, "{f:?} vs Linear at t = {t}");
            }
        }
        assert!(
            AnimationInterpolationFunction::Ease.evaluate(0.5)
                != AnimationInterpolationFunction::Linear.evaluate(0.5),
            "Ease must at least differ from Linear"
        );
    }

    #[test]
    fn evaluate_outside_the_unit_interval_extrapolates_without_clamping() {
        // t is not clamped, so animations driven past their duration overshoot.
        let linear = AnimationInterpolationFunction::Linear;
        assert_eq!(linear.evaluate(-1.0), 5.0);
        assert_eq!(linear.evaluate(2.0), -4.0);
    }

    #[test]
    fn evaluate_at_nan_is_nan_for_every_variant() {
        for f in ALL_VARIANTS {
            assert!(f.evaluate(f64::NAN).is_nan(), "{f:?}");
        }
    }

    #[test]
    fn evaluate_at_extreme_t_never_panics_and_never_lies() {
        for f in ALL_VARIANTS {
            for t in [f64::MAX, f64::MIN, 1e300, -1e300, f64::INFINITY, f64::NEG_INFINITY] {
                let v = f.evaluate(t);
                assert!(
                    !v.is_finite(),
                    "{f:?} at t = {t} returned a plausible-looking {v}"
                );
            }
        }
    }

    #[test]
    fn evaluate_of_a_nan_cubic_bezier_is_nan_not_a_panic() {
        let f = AnimationInterpolationFunction::CubicBezier(SvgCubicCurve::new(
            p(0.0, f32::NAN),
            p(0.0, 0.0),
            p(1.0, 1.0),
            p(1.0, 1.0),
        ));
        assert!(f.evaluate(0.5).is_nan());
    }

    #[test]
    fn evaluate_of_a_huge_cubic_bezier_stays_in_f32_range_inside_the_unit_interval() {
        // On [0, 1] a bezier is a convex combination of its control points, so it
        // can never exceed the largest one: no overflow is possible here.
        let f = AnimationInterpolationFunction::CubicBezier(SvgCubicCurve::new(
            p(0.0, 0.0),
            p(0.0, f32::MAX),
            p(1.0, f32::MAX),
            p(1.0, f32::MAX),
        ));
        for step in 0..=10 {
            let v = f.evaluate(f64::from(step) / 10.0);
            assert!(v.is_finite(), "overflowed inside [0,1] at step {step}: {v}");
            assert!((0.0..=f32::MAX).contains(&v));
        }
    }

    #[test]
    fn evaluate_of_a_huge_cubic_bezier_saturates_to_infinity_when_extrapolated() {
        // Outside [0, 1] the convex-hull bound is gone. y(t) = MAX*t^3 - 3*MAX*t^2
        // + 3*MAX*t, so y(3) = 9 * f32::MAX -- far past the f32 range. The f64 ->
        // f32 narrowing in evaluate() must saturate to +inf rather than wrap.
        let f = AnimationInterpolationFunction::CubicBezier(SvgCubicCurve::new(
            p(0.0, 0.0),
            p(0.0, f32::MAX),
            p(1.0, f32::MAX),
            p(1.0, f32::MAX),
        ));
        let v = f.evaluate(3.0);
        assert!(v.is_infinite() && v > 0.0, "expected +inf, got {v}");
    }

    #[test]
    fn evaluate_of_a_degenerate_flat_bezier_is_constant_zero() {
        let f = AnimationInterpolationFunction::CubicBezier(SvgCubicCurve::new(
            p(0.0, 0.0),
            p(0.0, 0.0),
            p(1.0, 0.0),
            p(1.0, 0.0),
        ));
        for step in 0..=10 {
            let t = f64::from(step) / 10.0;
            assert_eq!(f.evaluate(t), 0.0, "t = {t}");
        }
    }

    // ---- OptionSvgPoint (impl_option round-trip) ---------------------------

    #[test]
    fn option_svg_point_round_trips_through_std_option() {
        let pt = p(1.5, -2.5);

        let some: OptionSvgPoint = Some(pt).into();
        assert!(some.is_some());
        assert!(!some.is_none());
        assert_eq!(some.as_ref(), Some(&pt));
        assert_eq!(Option::<SvgPoint>::from(some), Some(pt));

        let none: OptionSvgPoint = OptionSvgPoint::None;
        assert!(none.is_none());
        assert_eq!(none.as_ref(), None);
        assert_eq!(Option::<SvgPoint>::from(none), None);

        assert!(OptionSvgPoint::default().is_none());
    }

    #[test]
    fn option_svg_point_replace_returns_the_previous_value() {
        let mut o = OptionSvgPoint::None;
        let prev = o.replace(p(1.0, 2.0));
        assert!(prev.is_none());
        assert!(o.is_some());

        let prev = o.replace(p(3.0, 4.0));
        assert_eq!(prev.as_ref(), Some(&p(1.0, 2.0)));
        assert_eq!(o.as_ref(), Some(&p(3.0, 4.0)));
    }

    // ---- InterpolateResolver ------------------------------------------------

    #[test]
    fn interpolate_resolver_stores_its_fields_verbatim() {
        let r = InterpolateResolver {
            interpolate_func: AnimationInterpolationFunction::EaseInOut,
            parent_rect_width: 100.0,
            parent_rect_height: f32::NAN,
            current_rect_width: f32::INFINITY,
            current_rect_height: -0.0,
        };
        assert_eq!(r.interpolate_func, AnimationInterpolationFunction::EaseInOut);
        assert_eq!(r.parent_rect_width, 100.0);
        assert!(r.parent_rect_height.is_nan());
        assert!(r.current_rect_width.is_infinite());
        assert!(r.current_rect_height.is_sign_negative());
        // NaN field => the derived PartialEq is not reflexive.
        assert_ne!(r, r);
    }
}
