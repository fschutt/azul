//! POD types for the motion-sensor surface
//! (SUPER_PLAN_2 §1 feature 5 + research/03 §"Feature 5").
//!
//! The three raw sensors apps want — accelerometer, gyroscope,
//! magnetometer — each delivered as an `(x, y, z)` triple in the sensor's
//! natural unit. Defined here in `azul-core` so the manager + accessors
//! cross the FFI without `azul-layout` being a dependency. The stateful
//! side lives in `azul_layout::managers::sensors::SensorManager`.
//!
//! Coordinate frame (research/03 §coordinate-frame): right-handed,
//! +X right, +Y up, +Z out of the screen toward the user, in the device's
//! default-portrait frame (iOS keeps the device frame regardless of UI
//! orientation; Android auto-rotates only fused sensors). v1 reports the
//! raw device frame.

/// Which motion sensor a [`SensorReading`] came from.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SensorKind {
    /// Linear acceleration including gravity, in **m/s²**
    /// (iOS `CMAccelerometerData` ×9.80665, Android `TYPE_ACCELEROMETER`).
    Accelerometer,
    /// Angular velocity, in **rad/s** (iOS `CMGyroData`, Android
    /// `TYPE_GYROSCOPE`).
    Gyroscope,
    /// Geomagnetic field, in **µT** (iOS `magneticField`, Android
    /// `TYPE_MAGNETIC_FIELD`).
    Magnetometer,
}

/// One `(x, y, z)` sample from a motion sensor. Units depend on
/// [`SensorReading::kind`] (see [`SensorKind`]). All POD / `Copy`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SensorReading {
    /// Which sensor produced this reading.
    pub kind: SensorKind,
    /// X axis (device frame: right), in the kind's unit.
    pub x: f32,
    /// Y axis (device frame: up), in the kind's unit.
    pub y: f32,
    /// Z axis (device frame: out of screen toward user), in the kind's unit.
    pub z: f32,
    /// Monotonic timestamp in milliseconds since program start.
    pub timestamp_ms: u64,
}

impl SensorReading {
    /// The magnitude of the `(x, y, z)` vector — e.g. total acceleration
    /// (≈9.81 at rest for the accelerometer) or field strength.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use] pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
}

// FFI Option wrapper for `CallbackInfo::get_sensor_reading(kind) ->
// Option<SensorReading>` (mirrors `OptionLocationFix`).
impl_option!(
    SensorReading,
    OptionSensorReading,
    [Debug, Clone, Copy, PartialEq]
);

#[cfg(test)]
#[allow(clippy::float_cmp)] // exactness IS the invariant under test for several cases
mod autotest_generated {
    use core::{
        cell::Cell,
        hash::{Hash, Hasher},
    };

    use super::*;

    // --- helpers ---------------------------------------------------------

    /// `SensorReading` derives no `Default`, so every test constructs one.
    fn reading(kind: SensorKind, x: f32, y: f32, z: f32) -> SensorReading {
        SensorReading {
            kind,
            x,
            y,
            z,
            timestamp_ms: 0,
        }
    }

    fn accel(x: f32, y: f32, z: f32) -> SensorReading {
        reading(SensorKind::Accelerometer, x, y, z)
    }

    /// Relative comparison with an absolute floor, for the cases where f32
    /// addition is not associative and bit-exactness is not guaranteed.
    fn approx(a: f32, b: f32, rel: f32) -> bool {
        let scale = a.abs().max(b.abs()).max(1.0);
        (a - b).abs() <= rel * scale
    }

    /// FNV-1a, so the `Hash` invariant can be checked without `std`.
    struct Fnv(u64);
    impl Default for Fnv {
        fn default() -> Self {
            Self(0xcbf2_9ce4_8422_2325)
        }
    }
    impl Hasher for Fnv {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for b in bytes {
                self.0 ^= u64::from(*b);
                self.0 = self.0.wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = Fnv::default();
        v.hash(&mut h);
        h.finish()
    }

    // --- SensorReading::magnitude (getter) -------------------------------

    /// basic_access: exactly representable Pythagorean triples must come back
    /// bit-exact — every intermediate (square, sum, sqrt) is exact in f32.
    #[test]
    fn magnitude_pythagorean_is_exact() {
        assert_eq!(accel(3.0, 4.0, 0.0).magnitude(), 5.0);
        assert_eq!(accel(0.0, 3.0, 4.0).magnitude(), 5.0);
        assert_eq!(accel(1.0, 2.0, 2.0).magnitude(), 3.0);
        assert_eq!(accel(0.0, 0.0, 0.0).magnitude(), 0.0);
    }

    /// invariant: a single non-zero axis yields |v| exactly, for values whose
    /// square is exactly representable. Holds on all three axes.
    #[test]
    fn magnitude_single_axis_equals_abs() {
        for v in [1.0_f32, -1.0, 0.5, -3.25, 7.5, -1024.0, 65_536.0] {
            assert_eq!(accel(v, 0.0, 0.0).magnitude(), v.abs(), "x = {v}");
            assert_eq!(accel(0.0, v, 0.0).magnitude(), v.abs(), "y = {v}");
            assert_eq!(accel(0.0, 0.0, v).magnitude(), v.abs(), "z = {v}");
        }
    }

    /// edge_access: an all-negative-zero reading must not produce `-0.0`
    /// (squaring clears the sign, so the result is `+0.0`).
    #[test]
    fn magnitude_negative_zero_yields_positive_zero() {
        let m = accel(-0.0, -0.0, -0.0).magnitude();
        assert_eq!(m, 0.0);
        assert!(m.is_sign_positive(), "expected +0.0, got {m:?}");
    }

    /// invariant: magnitude depends only on |x|,|y|,|z| — all 8 sign
    /// combinations of the same triple agree bit-for-bit.
    #[test]
    fn magnitude_is_sign_invariant() {
        let base = accel(1.5, 2.5, 3.5).magnitude();
        for &sx in &[1.0_f32, -1.0] {
            for &sy in &[1.0_f32, -1.0] {
                for &sz in &[1.0_f32, -1.0] {
                    let m = accel(1.5 * sx, 2.5 * sy, 3.5 * sz).magnitude();
                    assert_eq!(m.to_bits(), base.to_bits(), "signs {sx}/{sy}/{sz}");
                }
            }
        }
    }

    /// invariant: for any non-NaN input the result is non-negative — never a
    /// negative number, never a `-0.0`.
    #[test]
    fn magnitude_is_never_negative() {
        let vals = [
            0.0_f32,
            -0.0,
            1.0,
            -1.0,
            9.81,
            -9.81,
            1e-20,
            -1e-20,
            1e20,
            -1e20,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];
        for &x in &vals {
            for &y in &vals {
                for &z in &vals {
                    let m = accel(x, y, z).magnitude();
                    assert!(!m.is_nan(), "unexpected NaN for ({x}, {y}, {z})");
                    assert!(m >= 0.0, "negative magnitude {m} for ({x}, {y}, {z})");
                    assert!(
                        m.is_sign_positive(),
                        "negatively-signed magnitude {m:?} for ({x}, {y}, {z})"
                    );
                }
            }
        }
    }

    /// edge_access: a NaN on any axis poisons the result (no panic, NaN out).
    #[test]
    fn magnitude_nan_on_any_axis_is_nan() {
        assert!(accel(f32::NAN, 0.0, 0.0).magnitude().is_nan());
        assert!(accel(0.0, f32::NAN, 0.0).magnitude().is_nan());
        assert!(accel(0.0, 0.0, f32::NAN).magnitude().is_nan());
        assert!(accel(f32::NAN, f32::NAN, f32::NAN).magnitude().is_nan());
        // NaN wins over infinity: inf*inf = inf, but inf + NaN = NaN.
        assert!(accel(f32::INFINITY, f32::NAN, 0.0).magnitude().is_nan());
        assert!(
            accel(f32::NEG_INFINITY, 0.0, f32::NAN)
                .magnitude()
                .is_nan()
        );
    }

    /// edge_access: an infinite axis (either sign) yields `+inf`, not NaN.
    #[test]
    fn magnitude_infinite_axis_is_positive_infinity() {
        for inf in [f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(accel(inf, 0.0, 0.0).magnitude(), f32::INFINITY);
            assert_eq!(accel(0.0, inf, 0.0).magnitude(), f32::INFINITY);
            assert_eq!(accel(1.0, 2.0, inf).magnitude(), f32::INFINITY);
        }
        assert_eq!(
            accel(f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY).magnitude(),
            f32::INFINITY
        );
    }

    /// overflow: the naive `sqrt(x² + y² + z²)` overflows for large-but-finite
    /// inputs — the squares exceed `f32::MAX` even though the true magnitude
    /// is finite (`|f32::MAX|` would fit). Documents the limitation: the
    /// result saturates to `+inf` rather than panicking or wrapping.
    #[test]
    fn magnitude_overflows_to_infinity_on_huge_finite_input() {
        // True magnitude is exactly f32::MAX (finite), but MAX*MAX == inf.
        assert!(accel(f32::MAX, 0.0, 0.0).magnitude().is_infinite());
        assert!(accel(0.0, f32::MIN, 0.0).magnitude().is_infinite());
        assert!(
            accel(f32::MAX, f32::MAX, f32::MAX)
                .magnitude()
                .is_infinite()
        );
    }

    /// boundary: the exact cliff where squaring stops fitting in f32.
    /// `1.8e19² ≈ 3.24e38 < f32::MAX`, `1.9e19² ≈ 3.61e38 > f32::MAX`.
    #[test]
    fn magnitude_overflow_cliff_per_term_and_in_the_sum() {
        // Below the cliff: finite, and accurate.
        let below = accel(1.8e19, 0.0, 0.0).magnitude();
        assert!(below.is_finite(), "expected finite, got {below}");
        assert!(approx(below, 1.8e19, 1e-5), "{below} !~ 1.8e19");

        // Above the cliff: a single term overflows.
        assert!(accel(1.9e19, 0.0, 0.0).magnitude().is_infinite());

        // Each term fits, but the *sum* is what overflows: 3 x 1.69e38.
        let two_terms = accel(1.3e19, 1.3e19, 0.0).magnitude();
        assert!(two_terms.is_finite(), "expected finite, got {two_terms}");
        assert!(approx(two_terms, 1.838_477e19, 1e-4), "{two_terms}");
        assert!(
            accel(1.3e19, 1.3e19, 1.3e19).magnitude().is_infinite(),
            "sum of three 1.69e38 squares must overflow to +inf"
        );
    }

    /// underflow: the mirror image — squaring flushes tiny inputs to zero, so
    /// the magnitude of a non-zero vector reads back as exactly 0.0. No panic,
    /// no NaN; the reading is simply below the formula's resolution.
    #[test]
    fn magnitude_underflows_to_zero_on_tiny_input() {
        assert_eq!(accel(1e-30, 0.0, 0.0).magnitude(), 0.0);
        assert_eq!(accel(0.0, -1e-30, 1e-30).magnitude(), 0.0);
        assert_eq!(accel(f32::MIN_POSITIVE, 0.0, 0.0).magnitude(), 0.0);
        // Smallest positive subnormal.
        assert_eq!(accel(f32::from_bits(1), 0.0, 0.0).magnitude(), 0.0);
    }

    /// boundary: just above the underflow cliff (x² lands in the subnormal
    /// range) the result is still finite and roughly right.
    #[test]
    fn magnitude_near_underflow_cliff_is_finite_and_close() {
        let m = accel(1e-19, 0.0, 0.0).magnitude();
        assert!(m.is_finite() && m > 0.0, "expected small positive, got {m}");
        assert!(approx(m, 1e-19, 1e-2), "{m} !~ 1e-19");
    }

    /// invariant: scaling by a power of two is exact in IEEE-754, so
    /// `magnitude(2v) == 2 * magnitude(v)` bit-for-bit (absent over/underflow).
    #[test]
    fn magnitude_scaling_by_power_of_two_is_exact() {
        let (x, y, z) = (1.5_f32, -2.25_f32, 0.75_f32);
        let base = accel(x, y, z).magnitude();
        let doubled = accel(2.0 * x, 2.0 * y, 2.0 * z).magnitude();
        let halved = accel(0.5 * x, 0.5 * y, 0.5 * z).magnitude();
        assert_eq!(doubled, 2.0 * base);
        assert_eq!(halved, 0.5 * base);
    }

    /// invariant: magnitude is symmetric in its axes (f32 addition is not
    /// associative, so only up to a relative epsilon).
    #[test]
    fn magnitude_is_permutation_stable() {
        let (x, y, z) = (0.1_f32, 12_345.678_f32, -0.000_31_f32);
        let base = accel(x, y, z).magnitude();
        for (a, b, c) in [(x, z, y), (y, x, z), (y, z, x), (z, x, y), (z, y, x)] {
            let m = accel(a, b, c).magnitude();
            assert!(approx(m, base, 1e-6), "{m} !~ {base} for ({a}, {b}, {c})");
        }
    }

    /// basic_access: the documented value — an accelerometer at rest reads
    /// ~9.81 m/s² total, whichever axis gravity lands on.
    #[test]
    fn magnitude_accelerometer_at_rest_is_about_9_81() {
        for r in [
            accel(0.0, -9.81, 0.0),
            accel(9.81, 0.0, 0.0),
            accel(0.0, 0.0, -9.81),
            accel(5.663_8, -5.663_8, -5.663_8), // tilted: 9.81 / sqrt(3) per axis
        ] {
            let m = r.magnitude();
            assert!(approx(m, 9.81, 1e-3), "{m} !~ 9.81 for {r:?}");
        }
    }

    /// invariant: `kind` and `timestamp_ms` are not part of the computation —
    /// including at the extremes of `u64`.
    #[test]
    fn magnitude_ignores_kind_and_timestamp() {
        let expect = accel(1.0, 2.0, 2.0).magnitude().to_bits();
        for kind in [
            SensorKind::Accelerometer,
            SensorKind::Gyroscope,
            SensorKind::Magnetometer,
        ] {
            for timestamp_ms in [0_u64, 1, u64::MAX / 2, u64::MAX] {
                let r = SensorReading {
                    kind,
                    x: 1.0,
                    y: 2.0,
                    z: 2.0,
                    timestamp_ms,
                };
                assert_eq!(r.magnitude().to_bits(), expect, "{kind:?} @ {timestamp_ms}");
            }
        }
    }

    /// invariant: `&self` getter — repeated calls are bit-identical and the
    /// reading is left untouched.
    #[test]
    fn magnitude_is_deterministic_and_leaves_the_reading_intact() {
        let r = accel(0.1, -0.2, 9.79);
        let before = r;
        let a = r.magnitude();
        let b = r.magnitude();
        assert_eq!(a.to_bits(), b.to_bits());
        assert_eq!(r, before, "magnitude() must not mutate the reading");
    }

    // --- SensorReading / SensorKind POD invariants ------------------------

    /// A NaN axis makes the derived `PartialEq` non-reflexive — callers that
    /// dedup readings by equality must not assume `r == r`.
    #[test]
    fn reading_with_nan_axis_is_not_equal_to_itself() {
        let nan = accel(f32::NAN, 0.0, 0.0);
        let bit_identical = nan; // Copy, so this is the very same bit pattern
        assert_ne!(nan, bit_identical, "a NaN axis makes PartialEq non-reflexive");

        // ... and the FFI wrapper inherits that.
        let opt = OptionSensorReading::Some(nan);
        let opt_copy = opt;
        assert_ne!(opt, opt_copy);

        // A non-NaN reading *is* reflexive.
        let ok = accel(1.0, 2.0, 2.0);
        assert_eq!(ok, accel(1.0, 2.0, 2.0));
    }

    /// The derived `Ord` follows declaration order, and `Hash` agrees with
    /// `Eq` (equal values hash equal, the three variants are distinguishable).
    #[test]
    fn sensor_kind_ordering_and_hash_are_consistent() {
        use SensorKind::{Accelerometer, Gyroscope, Magnetometer};
        assert!(Accelerometer < Gyroscope);
        assert!(Gyroscope < Magnetometer);
        assert!(Accelerometer < Magnetometer);

        assert_eq!(hash_of(&Accelerometer), hash_of(&Accelerometer));
        assert_ne!(hash_of(&Accelerometer), hash_of(&Gyroscope));
        assert_ne!(hash_of(&Gyroscope), hash_of(&Magnetometer));

        // Copy, not move.
        let k = Gyroscope;
        let copied = k;
        assert_eq!(k, copied);
    }

    /// FFI layout: `SensorKind` is a `repr(C)` field-less enum, i.e. a C `int`.
    #[test]
    fn sensor_kind_is_a_c_int() {
        assert_eq!(core::mem::size_of::<SensorKind>(), 4);
        assert!(
            core::mem::size_of::<OptionSensorReading>() > core::mem::size_of::<SensorReading>()
        );
    }

    // --- OptionSensorReading (FFI wrapper) -------------------------------

    #[test]
    fn option_default_is_none() {
        let d = OptionSensorReading::default();
        assert!(d.is_none());
        assert!(!d.is_some());
        assert_eq!(d.as_option(), None);
        assert_eq!(d.as_ref(), None);
        assert_eq!(d.into_option(), None);
    }

    /// round-trip: `Option<SensorReading> -> OptionSensorReading -> Option<_>`
    /// is the identity, for both variants.
    #[test]
    fn option_roundtrips_through_the_ffi_wrapper() {
        let r = accel(1.0, 2.0, 2.0);

        let wrapped: OptionSensorReading = Some(r).into();
        assert!(wrapped.is_some());
        assert_eq!(wrapped.into_option(), Some(r));
        assert_eq!(Option::<SensorReading>::from(wrapped), Some(r));
        assert_eq!(wrapped.as_option(), Some(&r));

        let none: OptionSensorReading = None.into();
        assert!(none.is_none());
        assert_eq!(none.into_option(), None);
        assert_eq!(Option::<SensorReading>::from(none), None);

        // is_some / is_none are strict complements.
        for o in [wrapped, none] {
            assert_ne!(o.is_some(), o.is_none());
        }
    }

    /// `replace` has `mem::replace` semantics: it returns the PREVIOUS value.
    #[test]
    fn option_replace_returns_the_previous_value() {
        let first = accel(1.0, 0.0, 0.0);
        let second = accel(0.0, 3.0, 4.0);

        let mut o = OptionSensorReading::None;
        let prev = o.replace(first);
        assert!(prev.is_none(), "replacing None must hand back None");
        assert_eq!(o.into_option(), Some(first));

        let prev = o.replace(second);
        assert_eq!(prev.into_option(), Some(first));
        assert_eq!(o.into_option(), Some(second));
        assert_eq!(o.as_option().map(SensorReading::magnitude), Some(5.0));
    }

    /// `as_mut` hands out a real mutable borrow of the payload.
    #[test]
    fn option_as_mut_mutates_the_payload() {
        let mut o = OptionSensorReading::Some(accel(0.0, 0.0, 0.0));
        assert_eq!(o.as_option().map(SensorReading::magnitude), Some(0.0));

        let slot = o.as_mut().expect("Some");
        slot.x = 3.0;
        slot.y = 4.0;
        assert_eq!(o.as_option().map(SensorReading::magnitude), Some(5.0));

        let mut none = OptionSensorReading::None;
        assert!(none.as_mut().is_none());
    }

    /// `map` / `and_then` must not run the closure on `None` (and must run it
    /// exactly once on `Some`).
    #[test]
    fn option_map_and_and_then_are_lazy_on_none() {
        let calls = Cell::new(0_u32);

        let mapped = OptionSensorReading::None.map(|r| {
            calls.set(calls.get() + 1);
            r.magnitude()
        });
        assert_eq!(mapped, None);
        assert_eq!(calls.get(), 0, "closure must not run on None");

        let chained = OptionSensorReading::None.and_then(|r| Some(r.magnitude()));
        assert_eq!(chained, None);

        let some = OptionSensorReading::Some(accel(3.0, 4.0, 0.0));
        let mapped = some.map(|r| {
            calls.set(calls.get() + 1);
            r.magnitude()
        });
        assert_eq!(mapped, Some(5.0));
        assert_eq!(calls.get(), 1, "closure must run exactly once on Some");

        assert_eq!(some.and_then(|r| Some(r.magnitude())), Some(5.0));
        assert_eq!(
            some.and_then(|_| Option::<f32>::None),
            None,
            "and_then must be able to collapse Some -> None"
        );
    }

    /// The wrapper is `Copy` — passing it by value across FFI leaves the
    /// source usable (and the copy independent).
    #[test]
    fn option_is_copy_and_the_source_survives() {
        let o = OptionSensorReading::Some(accel(0.0, 3.0, 4.0));
        let mut copy = o;
        copy.replace(accel(1.0, 0.0, 0.0));

        assert_eq!(o.as_option().map(SensorReading::magnitude), Some(5.0));
        assert_eq!(copy.as_option().map(SensorReading::magnitude), Some(1.0));
    }
}
