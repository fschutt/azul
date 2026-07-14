//! Internal numeric-cast helpers.
//!
//! Each function isolates one `as` conversion that clippy flags
//! (`cast_precision_loss` / `cast_possible_truncation` / `cast_sign_loss` /
//! `cast_possible_wrap`) behind a single documented `#[allow]`, so call sites
//! stay lint-clean without scattering raw casts or per-file helpers. Every one is
//! a behaviour-preserving wrapper around `as` (float→int saturates, int→int wraps
//! per Rust semantics); they exist to *name the intent*, not to change behaviour.

/// `isize` → `f32`. Loses precision only for magnitudes above 2^24; layout
/// coordinates and CSS dimensions stay far within that range.
#[inline]
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub(crate) const fn isize_to_f32(v: isize) -> f32 {
    v as f32
}

/// `usize` → `f32`. Loses precision only above 2^24 (counts/lengths stay small).
#[inline]
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub(crate) const fn usize_to_f32(v: usize) -> f32 {
    v as f32
}

/// `i32` → `f32`. Loses precision only for magnitudes above 2^24.
#[inline]
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub(crate) const fn i32_to_f32(v: i32) -> f32 {
    v as f32
}

/// `f32` → `isize` (truncating). `as` saturates NaN→0 and out-of-range to the
/// `isize` bounds; callers that want rounding `.round()`/`.floor()` first.
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub(crate) const fn f32_to_isize(v: f32) -> isize {
    v as isize
}

/// `f32` → `i32` (truncating). `as` saturates NaN→0 and out-of-range to `i32`
/// bounds; callers that want rounding `.round()` first.
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub(crate) const fn f32_to_i32(v: f32) -> i32 {
    v as i32
}

/// `f32` → `u32` (truncating, sign-dropping). `as` saturates NaN→0, negatives→0,
/// out-of-range→`u32::MAX`; callers validate non-negative / `.round()` first.
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) const fn f32_to_u32(v: f32) -> u32 {
    v as u32
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod autotest_generated {
    use super::*;

    /// Largest integer an `f32` can represent exactly (`2^24`); above this the
    /// spacing between consecutive `f32`s is > 1, which is the precision loss
    /// the module docs warn about.
    const TWO_POW_24: i32 = 16_777_216;
    const TWO_POW_31: f32 = 2_147_483_648.0;
    const TWO_POW_32: f32 = 4_294_967_296.0;

    // ---------------------------------------------------------------- zero ---

    #[test]
    fn zero_maps_to_positive_zero_in_both_directions() {
        // Not just `== 0.0` (which is true for -0.0 as well): check the bits, so
        // a sign-flipping bug can't hide behind float equality.
        assert_eq!(isize_to_f32(0).to_bits(), 0_u32);
        assert_eq!(usize_to_f32(0).to_bits(), 0_u32);
        assert_eq!(i32_to_f32(0).to_bits(), 0_u32);

        assert_eq!(f32_to_isize(0.0), 0);
        assert_eq!(f32_to_i32(0.0), 0);
        assert_eq!(f32_to_u32(0.0), 0);
    }

    #[test]
    fn negative_zero_converts_to_integer_zero() {
        assert_eq!(f32_to_isize(-0.0), 0);
        assert_eq!(f32_to_i32(-0.0), 0);
        assert_eq!(f32_to_u32(-0.0), 0);
    }

    // ------------------------------------------------------------- nan/inf ---

    #[test]
    fn nan_saturates_to_zero_not_a_panic() {
        assert_eq!(f32_to_isize(f32::NAN), 0);
        assert_eq!(f32_to_i32(f32::NAN), 0);
        assert_eq!(f32_to_u32(f32::NAN), 0);

        // Negative NaN and a non-canonical NaN payload must behave identically.
        assert_eq!(f32_to_i32(-f32::NAN), 0);
        assert_eq!(f32_to_u32(-f32::NAN), 0);
        let payload_nan = f32::from_bits(0x7fc0_1234);
        assert!(payload_nan.is_nan());
        assert_eq!(f32_to_i32(payload_nan), 0);
        assert_eq!(f32_to_u32(payload_nan), 0);
    }

    #[test]
    fn infinities_saturate_to_the_integer_bounds() {
        assert_eq!(f32_to_isize(f32::INFINITY), isize::MAX);
        assert_eq!(f32_to_isize(f32::NEG_INFINITY), isize::MIN);

        assert_eq!(f32_to_i32(f32::INFINITY), i32::MAX);
        assert_eq!(f32_to_i32(f32::NEG_INFINITY), i32::MIN);

        // Unsigned: -inf clamps to 0, *not* to u32::MAX via a sign-reinterpret.
        assert_eq!(f32_to_u32(f32::INFINITY), u32::MAX);
        assert_eq!(f32_to_u32(f32::NEG_INFINITY), 0);
    }

    // ------------------------------------------------------------ overflow ---

    #[test]
    fn out_of_range_floats_saturate_rather_than_wrap() {
        // The classic UB-turned-saturation cases: a wrapping impl would give 0 /
        // i32::MIN here.
        assert_eq!(f32_to_i32(TWO_POW_31), i32::MAX);
        assert_eq!(f32_to_i32(-TWO_POW_31 - 256.0), i32::MIN);
        assert_eq!(f32_to_i32(1.0e30), i32::MAX);
        assert_eq!(f32_to_i32(-1.0e30), i32::MIN);
        assert_eq!(f32_to_i32(f32::MAX), i32::MAX);
        assert_eq!(f32_to_i32(f32::MIN), i32::MIN);

        assert_eq!(f32_to_u32(TWO_POW_32), u32::MAX);
        assert_eq!(f32_to_u32(1.0e30), u32::MAX);
        assert_eq!(f32_to_u32(f32::MAX), u32::MAX);

        assert_eq!(f32_to_isize(1.0e30), isize::MAX);
        assert_eq!(f32_to_isize(-1.0e30), isize::MIN);
        assert_eq!(f32_to_isize(f32::MAX), isize::MAX);
        assert_eq!(f32_to_isize(f32::MIN), isize::MIN);
    }

    #[test]
    fn negatives_clamp_to_zero_for_the_unsigned_cast() {
        // Sign-dropping must be a clamp, not a bit reinterpretation.
        assert_eq!(f32_to_u32(-1.0), 0);
        assert_eq!(f32_to_u32(-0.5), 0);
        assert_eq!(f32_to_u32(-1.0e30), 0);
        assert_eq!(f32_to_u32(-f32::from_bits(1)), 0); // -smallest subnormal
    }

    #[test]
    fn largest_in_range_floats_convert_exactly() {
        // Largest f32 strictly below 2^31 / 2^32 — exactly representable, so no
        // saturation should kick in and no off-by-one may appear.
        assert_eq!(f32_to_i32(2_147_483_520.0), 2_147_483_520);
        assert_eq!(f32_to_u32(4_294_967_040.0), 4_294_967_040);
    }

    // ---------------------------------------------------------- truncation ---

    #[test]
    fn fractional_values_truncate_toward_zero() {
        assert_eq!(f32_to_i32(1.9), 1);
        assert_eq!(f32_to_i32(-1.9), -1); // toward zero, not floor(-2)
        assert_eq!(f32_to_i32(0.9), 0);
        assert_eq!(f32_to_i32(-0.9), 0);
        assert_eq!(f32_to_isize(-1.9), -1);
        assert_eq!(f32_to_u32(1.9), 1);
        assert_eq!(f32_to_u32(0.9), 0);
    }

    #[test]
    fn subnormal_and_tiny_magnitudes_flush_to_zero() {
        assert_eq!(f32_to_i32(f32::MIN_POSITIVE), 0);
        assert_eq!(f32_to_u32(f32::MIN_POSITIVE), 0);
        assert_eq!(f32_to_isize(f32::from_bits(1)), 0); // smallest subnormal
        assert_eq!(f32_to_i32(f32::EPSILON), 0);
    }

    // ------------------------------------------------ int -> f32 boundaries ---

    #[test]
    fn integers_up_to_two_pow_24_are_exact() {
        // The documented "loses precision only above 2^24" claim, checked at the
        // boundary itself.
        assert_eq!(i32_to_f32(TWO_POW_24), 16_777_216.0);
        assert_eq!(f32_to_i32(i32_to_f32(TWO_POW_24)), TWO_POW_24);
        assert_eq!(i32_to_f32(TWO_POW_24 - 1), 16_777_215.0);
        assert_eq!(usize_to_f32(TWO_POW_24 as usize), 16_777_216.0);
        assert_eq!(isize_to_f32(TWO_POW_24 as isize), 16_777_216.0);
    }

    #[test]
    fn just_above_two_pow_24_loses_precision_by_round_to_even() {
        // 2^24 + 1 is not representable: it ties, and IEEE round-half-to-even
        // pulls it *down* to 2^24. 2^24 + 3 ties upward to 2^24 + 4.
        assert_eq!(i32_to_f32(TWO_POW_24 + 1), 16_777_216.0);
        assert_eq!(i32_to_f32(TWO_POW_24 + 3), 16_777_220.0);
        assert_eq!(usize_to_f32(TWO_POW_24 as usize + 1), 16_777_216.0);
        assert_eq!(isize_to_f32(TWO_POW_24 as isize + 1), 16_777_216.0);

        // Which means the round-trip is lossy exactly here — documented, not a bug.
        assert_eq!(f32_to_i32(i32_to_f32(TWO_POW_24 + 1)), TWO_POW_24);
    }

    #[test]
    fn int_min_max_convert_without_panic_and_stay_finite() {
        for v in [i32::MIN, i32::MIN + 1, -1, 0, 1, i32::MAX - 1, i32::MAX] {
            assert!(i32_to_f32(v).is_finite());
        }
        for v in [isize::MIN, isize::MIN + 1, -1, 0, 1, isize::MAX - 1, isize::MAX] {
            assert!(isize_to_f32(v).is_finite());
        }
        for v in [0_usize, 1, usize::MAX - 1, usize::MAX] {
            assert!(usize_to_f32(v).is_finite());
        }

        // i32::MIN is a power of two, so it *is* exact; i32::MAX is not, and
        // rounds *up* to 2^31 — past the range of the type it came from.
        assert_eq!(i32_to_f32(i32::MIN), -TWO_POW_31);
        assert_eq!(i32_to_f32(i32::MAX), TWO_POW_31);
        // ...i.e. it lands strictly above the largest f32 that fits in an i32.
        assert!(i32_to_f32(i32::MAX) > 2_147_483_520.0);
    }

    #[test]
    fn unsigned_max_does_not_go_negative_or_infinite() {
        let m = usize_to_f32(usize::MAX);
        assert!(m.is_finite());
        assert!(m.is_sign_positive());
        assert!(m > 0.0);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn pointer_sized_extremes_round_to_the_adjacent_power_of_two() {
        assert_eq!(isize_to_f32(isize::MAX), 9_223_372_036_854_775_808.0); // 2^63
        assert_eq!(isize_to_f32(isize::MIN), -9_223_372_036_854_775_808.0);
        assert_eq!(usize_to_f32(usize::MAX), 18_446_744_073_709_551_616.0); // 2^64
    }

    // ---------------------------------------------------------- round-trip ---

    #[test]
    fn round_trip_is_exact_within_the_exactly_representable_range() {
        // Stride sweep over [0, 2^24] plus the last 512 values before the
        // boundary, where an off-by-one in the exactness claim would show up.
        let mut v: i32 = -TWO_POW_24;
        while v <= TWO_POW_24 {
            assert_eq!(f32_to_i32(i32_to_f32(v)), v, "i32 round-trip broke at {v}");
            assert_eq!(f32_to_isize(isize_to_f32(v as isize)), v as isize);
            v = v.saturating_add(4093); // prime stride, hits odd/even alike
        }
        for v in (TWO_POW_24 - 512)..=TWO_POW_24 {
            assert_eq!(f32_to_i32(i32_to_f32(v)), v);
            assert_eq!(f32_to_u32(i32_to_f32(v)), v as u32);
        }
    }

    #[test]
    fn round_trip_survives_the_int_extremes_via_saturation() {
        // i32::MAX -> 2^31 (out of range!) -> saturates *back* to i32::MAX. The
        // two roundings cancel; assert that so a "fix" to either side gets caught.
        assert_eq!(f32_to_i32(i32_to_f32(i32::MAX)), i32::MAX);
        assert_eq!(f32_to_i32(i32_to_f32(i32::MIN)), i32::MIN);
        assert_eq!(f32_to_isize(isize_to_f32(isize::MAX)), isize::MAX);
        assert_eq!(f32_to_isize(isize_to_f32(isize::MIN)), isize::MIN);
    }

    #[test]
    fn powers_of_two_round_trip_exactly() {
        for exp in 0..31_u32 {
            let v = 1_i32 << exp;
            assert_eq!(f32_to_i32(i32_to_f32(v)), v, "2^{exp} round-trip broke");
            assert_eq!(f32_to_i32(i32_to_f32(-v)), -v);
            assert_eq!(f32_to_u32(i32_to_f32(v)), v as u32);
        }
    }

    // ---------------------------------------------------------- invariants ---

    #[test]
    fn int_to_float_is_monotonic() {
        // Must stay sorted ascending -- the windows(2) check below compares each
        // neighbouring pair. TWO_POW_24 (16_777_216) is smaller than 1e9.
        let samples = [
            isize::MIN,
            -1_000_000_000,
            -1,
            0,
            1,
            TWO_POW_24 as isize,
            1_000_000_000,
            isize::MAX,
        ];
        for w in samples.windows(2) {
            assert!(
                isize_to_f32(w[0]) <= isize_to_f32(w[1]),
                "monotonicity broke between {} and {}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn signed_and_unsigned_paths_agree_where_the_ranges_overlap() {
        for v in [0.0_f32, 1.0, 0.5, 42.7, 65_535.0, 16_777_216.0, 2_147_483_520.0] {
            assert_eq!(f32_to_u32(v), f32_to_i32(v) as u32, "disagreement at {v}");
            assert_eq!(f32_to_isize(v), f32_to_i32(v) as isize);
        }
        for v in [0_i32, 1, -1, 12_345, i32::MIN, i32::MAX] {
            assert_eq!(i32_to_f32(v), isize_to_f32(v as isize), "disagreement at {v}");
        }
    }

    #[test]
    fn sign_is_preserved_by_the_int_to_float_casts() {
        assert!(isize_to_f32(-1).is_sign_negative());
        assert!(isize_to_f32(1).is_sign_positive());
        assert!(i32_to_f32(i32::MIN).is_sign_negative());
        assert!(usize_to_f32(usize::MAX).is_sign_positive());
    }

    // -------------------------------------------------------- const context ---

    #[test]
    fn usable_in_const_context_with_the_same_saturating_semantics() {
        // These are `const fn`; const-eval must saturate identically to runtime,
        // and must not refuse to compile on NaN/out-of-range.
        const NAN_I32: i32 = f32_to_i32(f32::NAN);
        const INF_I32: i32 = f32_to_i32(f32::INFINITY);
        const NEG_U32: u32 = f32_to_u32(-5.0);
        const BIG_ISIZE: isize = f32_to_isize(1.0e30);
        const FROM_I32: f32 = i32_to_f32(-42);
        const FROM_USIZE: f32 = usize_to_f32(42);
        const FROM_ISIZE: f32 = isize_to_f32(-42);

        assert_eq!(NAN_I32, 0);
        assert_eq!(INF_I32, i32::MAX);
        assert_eq!(NEG_U32, 0);
        assert_eq!(BIG_ISIZE, isize::MAX);
        assert_eq!(FROM_I32, -42.0);
        assert_eq!(FROM_USIZE, 42.0);
        assert_eq!(FROM_ISIZE, -42.0);
    }
}
