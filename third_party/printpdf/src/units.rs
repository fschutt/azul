//! Typed lengths and the conversions between them.
//!
//! PDF measures everything in points: 1 pt = 1/72 inch = 0.3528 mm. The wrappers keep
//! millimeters and points from being mixed up; convert with `From` / `Into`:
//!
//! ```
//! use printpdf::{Mm, Pt, Px};
//!
//! let a4_width: Pt = Mm(210.0).into();
//! assert_eq!(a4_width, Pt(595.276));
//! let back: Mm = a4_width.into();
//! assert_eq!(back, Mm(210.0));
//!
//! // A pixel only has a size once a resolution is given.
//! assert_eq!(Px(300).into_pt(300.0), Pt(72.0));
//! ```
//!
//! All three support `+`, `-` and their assigning forms; `Pt` and `Mm` also support `*` and `/`
//! by an `f32`, and dividing two values of the same unit gives an `f32` ratio.

use std::{cmp::Ordering, num::FpCategory};

use serde_derive::{Deserialize, Serialize};

macro_rules! impl_partialeq {
    ($t:ty) => {
        impl PartialEq for $t {
            // Equal when the values agree to three decimals, to absorb floating point
            // inaccuracy. Non-finite values are never equal.
            fn eq(&self, other: &$t) -> bool {
                if (self.0.classify() == FpCategory::Zero
                    || self.0.classify() == FpCategory::Normal)
                    && (other.0.classify() == FpCategory::Zero
                        || other.0.classify() == FpCategory::Normal)
                {
                    (self.0 * 1000.0).round() == (other.0 * 1000.0).round()
                } else {
                    false
                }
            }
        }
    };
}

macro_rules! impl_ord {
    ($t:ty) => {
        impl PartialOrd for $t {
            fn partial_cmp(&self, other: &$t) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for $t {
            fn cmp(&self, other: &$t) -> Ordering {
                self.0.total_cmp(&other.0)
            }
        }
    };
}

/// A length in millimeters. 1 mm is about 2.8346 [`Pt`].
///
/// Equality compares to three decimals. Ordering is exact and follows [`f32::total_cmp`]: -0.0
/// sorts before 0.0, and NaN after every number (before every number if its sign bit is set).
/// Serialized as the plain number.
#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Mm(pub f32);

impl Mm {
    /// Converts to points; the same as `Pt::from(self)`.
    pub fn into_pt(&self) -> Pt {
        let pt: Pt = (*self).into();
        pt
    }
}

impl From<Pt> for Mm {
    fn from(value: Pt) -> Mm {
        Mm(value.0 * 0.352_778_f32)
    }
}

impl Eq for Mm {}

impl_partialeq!(Mm);
impl_ord!(Mm);

/// A length in points, the unit of PDF: 1 pt is 1/72 inch, about 0.3528 [`Mm`].
///
/// Equality compares to three decimals. Ordering is exact and follows [`f32::total_cmp`]: -0.0
/// sorts before 0.0, and NaN after every number (before every number if its sign bit is set).
/// Serialized as the plain number.
#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pt(pub f32);

impl Pt {
    /// Converts to pixels at `dpi` dots per inch, rounded to the nearest whole pixel.
    pub fn into_px(self, dpi: f32) -> Px {
        let mm = self.0 / 2.834646_f32;
        let px_f = mm * (dpi / 25.4_f32);

        Px(px_f.round() as usize)
    }
}

impl From<Mm> for Pt {
    fn from(value: Mm) -> Pt {
        Pt(value.0 * 2.834_646_f32)
    }
}

impl From<Pt> for ::lopdf::Object {
    fn from(value: Pt) -> Self {
        Self::Real(value.0)
    }
}

impl Eq for Pt {}

impl_partialeq!(Pt);
impl_ord!(Pt);

/// A length in pixels. It only has a physical size together with a resolution (DPI), see
/// [`Px::into_pt`]. `+` and `-` saturate at `usize::MAX` and 0.
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Px(pub usize);

impl Px {
    /// Converts to points for an image printed at `dpi` dots per inch (`px * 72 / dpi`).
    pub fn into_pt(self, dpi: f32) -> Pt {
        Mm(self.0 as f32 * (25.4 / dpi)).into()
    }
}

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

macro_rules! impl_add_self {
    ($type:ident) => {
        impl Add for $type {
            type Output = Self;
            fn add(self, other: Self) -> Self {
                Self {
                    0: self.0 + other.0,
                }
            }
        }
    };
}

macro_rules! impl_add_assign_self {
    ($type:ident) => {
        impl AddAssign for $type {
            fn add_assign(&mut self, other: Self) {
                self.0 += other.0;
            }
        }
    };
}

macro_rules! impl_sub_assign_self {
    ($type:ident) => {
        impl SubAssign for $type {
            fn sub_assign(&mut self, other: Self) {
                self.0 -= other.0;
            }
        }
    };
}

macro_rules! impl_sub_self {
    ($type:ident) => {
        impl Sub for $type {
            type Output = Self;
            fn sub(self, other: Self) -> Self {
                Self {
                    0: self.0 - other.0,
                }
            }
        }
    };
}

macro_rules! impl_mul_f32 {
    ($type:ident) => {
        impl Mul<f32> for $type {
            type Output = Self;
            fn mul(self, other: f32) -> Self {
                Self { 0: self.0 * other }
            }
        }
    };
}

macro_rules! impl_mul_assign_f32 {
    ($type:ident) => {
        impl MulAssign<f32> for $type {
            fn mul_assign(&mut self, other: f32) {
                self.0 *= other;
            }
        }
    };
}

macro_rules! impl_div {
    ($type:ident) => {
        impl Div<$type> for $type {
            type Output = f32;
            fn div(self, other: $type) -> Self::Output {
                self.0 / other.0
            }
        }
        impl Div<f32> for $type {
            type Output = Self;
            fn div(self, other: f32) -> Self::Output {
                Self { 0: self.0 / other }
            }
        }
    };
}

macro_rules! impl_div_assign_f32 {
    ($type:ident) => {
        impl DivAssign<f32> for $type {
            fn div_assign(&mut self, other: f32) {
                self.0 /= other;
            }
        }
    };
}

impl Add for Px {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Px(self.0.saturating_add(other.0))
    }
}

impl AddAssign for Px {
    fn add_assign(&mut self, other: Self) {
        self.0 = self.0.saturating_add(other.0);
    }
}

impl Sub for Px {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Px(self.0.saturating_sub(other.0))
    }
}

impl SubAssign for Px {
    fn sub_assign(&mut self, other: Self) {
        self.0 = self.0.saturating_sub(other.0);
    }
}

impl_add_self!(Mm);
impl_add_self!(Pt);

impl_add_assign_self!(Mm);
impl_add_assign_self!(Pt);

impl_sub_assign_self!(Mm);
impl_sub_assign_self!(Pt);

impl_sub_self!(Mm);
impl_sub_self!(Pt);

impl_mul_f32!(Mm);
impl_mul_f32!(Pt);

impl_mul_assign_f32!(Mm);
impl_mul_assign_f32!(Pt);

impl_div!(Mm);
impl_div!(Pt);

impl_div_assign_f32!(Mm);
impl_div_assign_f32!(Pt);

#[test]
fn point_to_mm_conversion() {
    let pt1: Mm = Pt(1.0).into();
    let pt2: Mm = Pt(15.0).into();
    assert_eq!(pt1, Mm(0.352778));
    assert_eq!(pt2, Mm(5.29167));
}

#[test]
fn mm_to_point_conversion() {
    let mm1: Pt = Mm(1.0).into();
    let mm2: Pt = Mm(23.0).into();
    assert_eq!(mm1, Pt(2.834_647_4));
    assert_eq!(mm2, Pt(65.1969));
}

#[test]
fn mm_eq_zero_check() {
    let mm1: Mm = Mm(0.0);
    let mm2: Mm = Mm(0.0);
    assert_eq!(mm1, mm2);
    assert_eq!(mm1, Mm(0.0));
    assert_eq!(mm2, Mm(0.0));
}

#[test]
fn max_mm() {
    let mm_vector = [Mm(0.0), Mm(1.0), Mm(2.0)];
    assert_eq!(mm_vector.iter().max().unwrap(), &Mm(2.0));
}

#[test]
fn min_mm() {
    let mm_vector = [Mm(0.0), Mm(1.0), Mm(2.0)];
    assert_eq!(mm_vector.iter().min().unwrap(), &Mm(0.0));
}

#[test]
fn pt_eq_zero_check() {
    let pt1: Pt = Pt(0.0);
    let pt2: Pt = Pt(0.0);
    assert_eq!(pt1, pt2);
    assert_eq!(pt1, Pt(0.0));
    assert_eq!(pt2, Pt(0.0));
}

#[test]
fn max_pt() {
    let pt_vector = [Pt(0.0), Pt(1.0), Pt(2.0)];
    assert_eq!(pt_vector.iter().max().unwrap(), &Pt(2.0));
}

#[test]
fn min_pt() {
    let pt_vector = [Pt(0.0), Pt(1.0), Pt(2.0)];
    assert_eq!(pt_vector.iter().min().unwrap(), &Pt(0.0));
}
