//! SVG geometry primitives (points, curves, rects, vectors) and animation interpolation functions.

use crate::impl_option;

/// Holds context needed to resolve animation interpolation relative to parent and current rects.
#[derive(Debug, Clone, PartialEq)]
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
    /// Creates a new SvgPoint from x and y coordinates
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Returns the Euclidean distance between this point and `other`.
    #[inline]
    pub fn distance(&self, other: Self) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        libm::hypotf(dx, dy) as f64
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
    pub fn contains_point(&self, point: SvgPoint) -> bool {
        point.x > self.x
            && point.x < self.x + self.width
            && point.y > self.y
            && point.y < self.y + self.height
    }

}

const STEP_SIZE: usize = 20;
const STEP_SIZE_F64: f64 = 0.05;

impl SvgCubicCurve {
    /// Creates a new SvgCubicCurve from start, two control points, and end point
    #[inline]
    pub const fn new(start: SvgPoint, ctrl_1: SvgPoint, ctrl_2: SvgPoint, end: SvgPoint) -> Self {
        Self { start, ctrl_1, ctrl_2, end }
    }

    /// Reverses the curve direction in place, swapping start/end and ctrl_1/ctrl_2.
    pub fn reverse(&mut self) {
        core::mem::swap(&mut self.start, &mut self.end);
        core::mem::swap(&mut self.ctrl_1, &mut self.ctrl_2);
    }

    /// Returns the start point of the curve.
    pub fn get_start(&self) -> SvgPoint {
        self.start
    }
    /// Returns the end point of the curve.
    pub fn get_end(&self) -> SvgPoint {
        self.end
    }

    /// Evaluates the x coordinate of the curve at parameter `t` in [0, 1].
    pub fn get_x_at_t(&self, t: f64) -> f64 {
        let c_x = 3.0 * (self.ctrl_1.x as f64 - self.start.x as f64);
        let b_x = 3.0 * (self.ctrl_2.x as f64 - self.ctrl_1.x as f64) - c_x;
        let a_x = self.end.x as f64 - self.start.x as f64 - c_x - b_x;

        (a_x * t * t * t) + (b_x * t * t) + (c_x * t) + self.start.x as f64
    }

    /// Evaluates the y coordinate of the curve at parameter `t` in [0, 1].
    pub fn get_y_at_t(&self, t: f64) -> f64 {
        let c_y = 3.0 * (self.ctrl_1.y as f64 - self.start.y as f64);
        let b_y = 3.0 * (self.ctrl_2.y as f64 - self.ctrl_1.y as f64) - c_y;
        let a_y = self.end.y as f64 - self.start.y as f64 - c_y - b_y;

        (a_y * t * t * t) + (b_y * t * t) + (c_y * t) + self.start.y as f64
    }

    /// Returns the approximate arc length of the curve using linear sampling.
    pub fn get_length(&self) -> f64 {
        // NOTE: this arc length parametrization is not very precise, but fast
        let mut arc_length = 0.0;
        let mut prev_point = self.get_start();

        for i in 0..STEP_SIZE {
            let t_next = (i + 1) as f64 * STEP_SIZE_F64;
            let next_point = SvgPoint {
                x: self.get_x_at_t(t_next) as f32,
                y: self.get_y_at_t(t_next) as f32,
            };
            arc_length += prev_point.distance(next_point);
            prev_point = next_point;
        }

        arc_length
    }

    /// Returns the parameter `t` corresponding to a given arc-length `offset`.
    pub fn get_t_at_offset(&self, offset: f64) -> f64 {
        // step through the line until the offset is reached,
        // then interpolate linearly between the
        // current at the last sampled point
        let mut arc_length = 0.0;
        let mut t_current = 0.0;
        let mut prev_point = self.get_start();

        for i in 0..STEP_SIZE {
            let t_next = (i + 1) as f64 * STEP_SIZE_F64;
            let next_point = SvgPoint {
                x: self.get_x_at_t(t_next) as f32,
                y: self.get_y_at_t(t_next) as f32,
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
    pub fn get_tangent_vector_at_t(&self, t: f64) -> SvgVector {
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
    pub fn get_bounds(&self) -> SvgRect {
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

}

impl SvgQuadraticCurve {
    /// Creates a new SvgQuadraticCurve from start, control, and end points
    #[inline]
    pub const fn new(start: SvgPoint, ctrl: SvgPoint, end: SvgPoint) -> Self {
        Self { start, ctrl, end }
    }

    /// Reverses the curve direction in place.
    pub fn reverse(&mut self) {
        core::mem::swap(&mut self.start, &mut self.end);
    }
    /// Returns the start point of the curve.
    pub fn get_start(&self) -> SvgPoint {
        self.start
    }
    /// Returns the end point of the curve.
    pub fn get_end(&self) -> SvgPoint {
        self.end
    }
    /// Returns the axis-aligned bounding box of the curve's control points.
    pub fn get_bounds(&self) -> SvgRect {
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
    pub fn get_x_at_t(&self, t: f64) -> f64 {
        let one_minus = 1.0 - t;
        one_minus * one_minus * self.start.x as f64
            + 2.0 * one_minus * t * self.ctrl.x as f64
            + t * t * self.end.x as f64
    }

    /// Evaluates the y coordinate of the curve at parameter `t` in [0, 1].
    pub fn get_y_at_t(&self, t: f64) -> f64 {
        let one_minus = 1.0 - t;
        one_minus * one_minus * self.start.y as f64
            + 2.0 * one_minus * t * self.ctrl.y as f64
            + t * t * self.end.y as f64
    }

    /// Returns the approximate arc length by converting to a cubic curve.
    pub fn get_length(&self) -> f64 {
        self.to_cubic().get_length()
    }

    /// Returns the parameter `t` corresponding to a given arc-length `offset`.
    pub fn get_t_at_offset(&self, offset: f64) -> f64 {
        self.to_cubic().get_t_at_offset(offset)
    }

    /// Returns the normalized tangent vector at parameter `t`.
    pub fn get_tangent_vector_at_t(&self, t: f64) -> SvgVector {
        self.to_cubic().get_tangent_vector_at_t(t)
    }

    /// Converts this quadratic curve to an equivalent cubic bezier curve.
    fn to_cubic(&self) -> SvgCubicCurve {
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
    pub const fn get_curve(self) -> SvgCubicCurve {
        match self {
            AnimationInterpolationFunction::Ease => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.25, y: 0.1 },
                ctrl_2: SvgPoint { x: 0.25, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            AnimationInterpolationFunction::Linear => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_2: SvgPoint { x: 1.0, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            AnimationInterpolationFunction::EaseIn => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.42, y: 0.0 },
                ctrl_2: SvgPoint { x: 1.0, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            AnimationInterpolationFunction::EaseOut => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_2: SvgPoint { x: 0.58, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            AnimationInterpolationFunction::EaseInOut => SvgCubicCurve {
                start: SvgPoint { x: 0.0, y: 0.0 },
                ctrl_1: SvgPoint { x: 0.42, y: 0.0 },
                ctrl_2: SvgPoint { x: 0.58, y: 1.0 },
                end: SvgPoint { x: 1.0, y: 1.0 },
            },
            AnimationInterpolationFunction::CubicBezier(c) => c,
        }
    }

    /// Evaluates the interpolation function at time `t`, returning the eased value.
    pub fn evaluate(self, t: f64) -> f32 {
        self.get_curve().get_y_at_t(t) as f32
    }
}
