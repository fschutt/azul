//! The float functions std puts on `f32` and `f64` (`sqrt`, `sin`,
//! `mul_add`, ...), for builds without std, where core has none of them.
//!
//! A file that calls them imports [`FloatMath`] only in those builds
//! (`#[cfg(not(feature = "std"))] use crate::float_math::FloatMath as _;`).
//! With std the inherent methods are found first, so std builds compute
//! exactly what they always did; without std the same calls reach libm.

pub(crate) trait FloatMath: Sized {
    fn round(self) -> Self;
    fn floor(self) -> Self;
    fn ceil(self) -> Self;
    fn trunc(self) -> Self;
    fn fract(self) -> Self;
    fn sqrt(self) -> Self;
    fn sin(self) -> Self;
    fn cos(self) -> Self;
    fn tan(self) -> Self;
    fn acos(self) -> Self;
    fn atan2(self, other: Self) -> Self;
    fn hypot(self, other: Self) -> Self;
    fn mul_add(self, a: Self, b: Self) -> Self;
    fn powi(self, n: i32) -> Self;
    fn powf(self, n: Self) -> Self;
}

impl FloatMath for f32 {
    fn round(self) -> Self {
        libm::roundf(self)
    }
    fn floor(self) -> Self {
        libm::floorf(self)
    }
    fn ceil(self) -> Self {
        libm::ceilf(self)
    }
    fn trunc(self) -> Self {
        libm::truncf(self)
    }
    fn fract(self) -> Self {
        self - libm::truncf(self)
    }
    fn sqrt(self) -> Self {
        libm::sqrtf(self)
    }
    fn sin(self) -> Self {
        libm::sinf(self)
    }
    fn cos(self) -> Self {
        libm::cosf(self)
    }
    fn tan(self) -> Self {
        libm::tanf(self)
    }
    fn acos(self) -> Self {
        libm::acosf(self)
    }
    fn atan2(self, other: Self) -> Self {
        libm::atan2f(self, other)
    }
    fn hypot(self, other: Self) -> Self {
        libm::hypotf(self, other)
    }
    fn mul_add(self, a: Self, b: Self) -> Self {
        libm::fmaf(self, a, b)
    }
    // An exponent too big for f32 to hold exactly has long overflowed the
    // power (or sent it to zero), so the rounding of `n` does not show.
    #[allow(clippy::cast_precision_loss)]
    fn powi(self, n: i32) -> Self {
        libm::powf(self, n as f32)
    }
    fn powf(self, n: Self) -> Self {
        libm::powf(self, n)
    }
}

impl FloatMath for f64 {
    fn round(self) -> Self {
        libm::round(self)
    }
    fn floor(self) -> Self {
        libm::floor(self)
    }
    fn ceil(self) -> Self {
        libm::ceil(self)
    }
    fn trunc(self) -> Self {
        libm::trunc(self)
    }
    fn fract(self) -> Self {
        self - libm::trunc(self)
    }
    fn sqrt(self) -> Self {
        libm::sqrt(self)
    }
    fn sin(self) -> Self {
        libm::sin(self)
    }
    fn cos(self) -> Self {
        libm::cos(self)
    }
    fn tan(self) -> Self {
        libm::tan(self)
    }
    fn acos(self) -> Self {
        libm::acos(self)
    }
    fn atan2(self, other: Self) -> Self {
        libm::atan2(self, other)
    }
    fn hypot(self, other: Self) -> Self {
        libm::hypot(self, other)
    }
    fn mul_add(self, a: Self, b: Self) -> Self {
        libm::fma(self, a, b)
    }
    fn powi(self, n: i32) -> Self {
        libm::pow(self, f64::from(n))
    }
    fn powf(self, n: Self) -> Self {
        libm::pow(self, n)
    }
}
