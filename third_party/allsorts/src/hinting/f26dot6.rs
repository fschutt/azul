/// 26.6 fixed-point number used in TrueType hinting.
///
/// 26 integer bits + 6 fractional bits = 1/64 pixel precision.
/// This is the standard coordinate format used by the TrueType bytecode interpreter.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct F26Dot6(pub i32);

impl F26Dot6 {
    pub const ZERO: F26Dot6 = F26Dot6(0);
    pub const ONE: F26Dot6 = F26Dot6(64);

    #[inline]
    pub fn from_i32(v: i32) -> Self {
        F26Dot6(v << 6)
    }

    #[inline]
    pub fn from_bits(v: i32) -> Self {
        F26Dot6(v)
    }

    #[inline]
    pub fn to_bits(self) -> i32 {
        self.0
    }

    #[inline]
    pub fn to_i32(self) -> i32 {
        self.0 >> 6
    }

    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / 64.0
    }

    #[inline]
    pub fn from_f64(v: f64) -> Self {
        F26Dot6((v * 64.0) as i32)
    }

    /// Scale an FUnit value to F26Dot6 pixels.
    ///
    /// `scale` is ppem * 64 / units_per_em, pre-computed as a fixed-point multiplier.
    /// Uses FreeType-compatible signed rounding (FT_MulFix): take absolute values,
    /// round with positive bias, re-apply sign. This prevents the rounding asymmetry
    /// of `(negative + 0x8000) >> 16` which rounds ties toward zero instead of
    /// away from zero.
    #[inline]
    pub fn from_funits(funits: i32, scale: i64) -> Self {
        let mut s: i64 = 1;
        let mut a = funits as i64;
        let mut b = scale;
        if a < 0 { a = -a; s = -s; }
        if b < 0 { b = -b; s = -s; }
        let c = (a * b + 0x8000) >> 16;
        F26Dot6((if s > 0 { c } else { -c }) as i32)
    }

    #[inline]
    pub fn floor(self) -> Self {
        F26Dot6(self.0 & !63)
    }

    #[inline]
    pub fn ceil(self) -> Self {
        F26Dot6((self.0 + 63) & !63)
    }

    #[inline]
    pub fn round(self) -> Self {
        F26Dot6((self.0 + 32) & !63)
    }

    #[inline]
    pub fn abs(self) -> Self {
        F26Dot6(self.0.abs())
    }
}

impl std::ops::Add for F26Dot6 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        F26Dot6(self.0 + rhs.0)
    }
}

impl std::ops::Sub for F26Dot6 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        F26Dot6(self.0 - rhs.0)
    }
}

impl std::ops::Neg for F26Dot6 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        F26Dot6(-self.0)
    }
}

impl std::ops::AddAssign for F26Dot6 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl std::ops::SubAssign for F26Dot6 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

/// 2.14 fixed-point number for unit vectors.
///
/// 14 fractional bits: 0x4000 = 1.0, range approximately [-2, 2).
/// Used to represent the projection vector and freedom vector in the graphics state.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct F2Dot14(pub i32);

impl F2Dot14 {
    pub const ZERO: F2Dot14 = F2Dot14(0);
    pub const ONE: F2Dot14 = F2Dot14(0x4000);

    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / 16384.0
    }

    #[inline]
    pub fn from_f64(v: f64) -> Self {
        F2Dot14((v * 16384.0) as i32)
    }

    #[inline]
    pub fn from_bits(v: i32) -> Self {
        F2Dot14(v)
    }

    #[inline]
    pub fn to_bits(self) -> i32 {
        self.0
    }
}

/// Multiply two F2Dot14 values, returning F2Dot14.
#[inline]
pub fn mul_2dot14(a: F2Dot14, b: F2Dot14) -> F2Dot14 {
    F2Dot14(((a.0 as i64 * b.0 as i64 + 0x2000) >> 14) as i32)
}

/// Multiply F26Dot6 by F2Dot14, returning F26Dot6.
#[inline]
pub fn mul_f26dot6_by_f2dot14(a: F26Dot6, b: F2Dot14) -> F26Dot6 {
    F26Dot6(((a.0 as i64 * b.0 as i64 + 0x2000) >> 14) as i32)
}

/// Compute the length of a 2D vector (F2Dot14 components), returned as F2Dot14.
pub fn vec_length(x: F2Dot14, y: F2Dot14) -> F2Dot14 {
    let x = x.0 as f64 / 16384.0;
    let y = y.0 as f64 / 16384.0;
    let len = (x * x + y * y).sqrt();
    F2Dot14::from_f64(len)
}

/// Compute scale factor for converting FUnits to F26Dot6.
///
/// Returns a 16.16 fixed-point scale factor: `ppem * 64 / units_per_em`.
/// This is used with `F26Dot6::from_funits`.
pub fn compute_scale(ppem: u16, units_per_em: u16) -> i64 {
    if units_per_em == 0 {
        return 0;
    }
    // We want: funits * ppem * 64 / units_per_em
    // Precompute: ppem * 64 * 65536 / units_per_em (as 16.16 fixed point)
    // Add upem/2 for proper rounding, matching FreeType's FT_DivFix.
    let upem = units_per_em as i64;
    (((ppem as i64) << 22) + (upem >> 1)) / upem
}
