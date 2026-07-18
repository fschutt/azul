//! POD types for the geolocation surface. Defined here in `azul-core`
//! so `NodeType::GeolocationProbe(GeolocationProbeConfig)` can carry the
//! config without `azul-layout` having to be a `azul-core` dependency.
//!
//! The stateful side (refcount, diff queue, latest-fix storage) lives
//! in `azul_layout::managers::geolocation::GeolocationManager` and
//! re-exports these types for the existing import paths.

/// One GPS / network-located fix. Mirrors the W3C
/// [`GeolocationPosition`](https://www.w3.org/TR/geolocation/#position_interface)
/// shape so the future web backend lands without API churn.
///
/// `accuracy_m` is the 1-sigma radius in metres. `altitude_m` /
/// `altitude_accuracy_m` / `heading_deg` / `speed_mps` are reported as
/// `f32::NAN` when the platform doesn't supply them — iOS / Android
/// always supply lat/lon but the other fields depend on hardware.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct LocationFix {
    /// Latitude in WGS-84 degrees (positive = north, negative = south).
    pub latitude_deg: f64,
    /// Longitude in WGS-84 degrees (positive = east, negative = west).
    pub longitude_deg: f64,
    /// 1-sigma horizontal accuracy radius in metres.
    pub accuracy_m: f32,
    /// Altitude above the WGS-84 ellipsoid in metres. `NaN` if not
    /// reported (the platform couldn't measure it).
    pub altitude_m: f32,
    /// 1-sigma altitude accuracy in metres. `NaN` if `altitude_m` is
    /// `NaN` or the platform doesn't report it.
    pub altitude_accuracy_m: f32,
    /// Bearing in degrees clockwise from true north, `0..360`. `NaN`
    /// if the device is stationary or the platform doesn't report it.
    pub heading_deg: f32,
    /// Ground speed in metres per second. `NaN` if not reported.
    pub speed_mps: f32,
    /// Monotonic timestamp in milliseconds since program start. Lets
    /// callers detect stale fixes without depending on wall-clock time.
    pub timestamp_ms: u64,
}

// FFI Option wrapper (mirrors OptionPenState). Lets `CallbackInfo::
// get_location_fix() -> Option<LocationFix>` cross the C ABI once the
// matching api.json type entry + getter are registered via the autofix
// workflow. Unused internally today; this is the no-codegen prerequisite
// for that exposure (see MOBILE_SESSION_LOG P3.1h).
impl_option!(LocationFix, OptionLocationFix, [Debug, Clone, Copy, PartialEq]);

impl LocationFix {
    #[must_use] pub const fn altitude(&self) -> Option<f32> {
        if self.altitude_m.is_nan() {
            None
        } else {
            Some(self.altitude_m)
        }
    }

    #[must_use] pub const fn altitude_accuracy(&self) -> Option<f32> {
        if self.altitude_accuracy_m.is_nan() {
            None
        } else {
            Some(self.altitude_accuracy_m)
        }
    }

    #[must_use] pub const fn heading(&self) -> Option<f32> {
        if self.heading_deg.is_nan() {
            None
        } else {
            Some(self.heading_deg)
        }
    }

    #[must_use] pub const fn speed(&self) -> Option<f32> {
        if self.speed_mps.is_nan() {
            None
        } else {
            Some(self.speed_mps)
        }
    }
}

/// Configuration the user attaches to a `NodeType::GeolocationProbe`
/// to tune the platform subscription. Maps to W3C `PositionOptions`
/// (`enableHighAccuracy` + `maximumAge` + `timeout`).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct GeolocationProbeConfig {
    /// `true` requests precise (GPS-driven) location. iOS maps this to
    /// `CLLocationManager.desiredAccuracy = kCLLocationAccuracyBest`;
    /// Android to `LocationRequest.PRIORITY_HIGH_ACCURACY`. Costs
    /// battery — leave `false` for city-block-level apps.
    pub high_accuracy: bool,
    /// Subscribe to *background* location updates. Requires extra
    /// per-platform manifest declarations and a separate
    /// `Capability::GeolocationBackground` permission grant. `false`
    /// is the safe default.
    pub background: bool,
    /// Reject any fix whose `accuracy_m` exceeds this radius. `0`
    /// disables the filter — every native sample is delivered.
    pub max_accuracy_m: f32,
    /// Minimum time between delivered updates, in milliseconds. `0`
    /// disables throttling (every native sample is delivered;
    /// expensive when the platform fires at 10 Hz indoors).
    pub min_interval_ms: u32,
}

impl Default for GeolocationProbeConfig {
    fn default() -> Self {
        Self {
            high_accuracy: false,
            background: false,
            max_accuracy_m: 0.0,
            min_interval_ms: 0,
        }
    }
}

/// Canonical bit pattern for hashing / total-ordering / equality of an f32 config
/// field: -0.0 and +0.0 collapse to the same value (they compare numerically
/// equal), and every NaN maps to one canonical NaN (so a NaN is equal to — and
/// hashes like — itself). Used by `PartialEq`, `Ord` and `Hash` so all three agree.
const fn canon_bits(f: f32) -> u32 {
    let bits = f.to_bits();
    if bits.trailing_zeros() >= 31 {
        0 // +0.0 and -0.0 collapse to +0.0
    } else if f.is_nan() {
        f32::NAN.to_bits() // all NaN payloads -> one canonical NaN
    } else {
        bits
    }
}

// PartialEq / Ord / Hash are hand-written to compare `max_accuracy_m` via
// `canon_bits` so all three agree: a derived PartialEq's raw float `==` makes a
// NaN unequal to itself, and raw `to_bits` makes -0.0 != +0.0 — either way
// breaking the Eq/Hash/Ord contracts that NodeType (which embeds this) relies on.
impl PartialEq for GeolocationProbeConfig {
    fn eq(&self, other: &Self) -> bool {
        self.high_accuracy == other.high_accuracy
            && self.background == other.background
            && canon_bits(self.max_accuracy_m) == canon_bits(other.max_accuracy_m)
            && self.min_interval_ms == other.min_interval_ms
    }
}

impl Eq for GeolocationProbeConfig {}

impl PartialOrd for GeolocationProbeConfig {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GeolocationProbeConfig {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // f32 comparison via to_bits — gives a total order even with
        // NaNs and matches NodeType::Eq + Hash requirements.
        (
            self.high_accuracy,
            self.background,
            canon_bits(self.max_accuracy_m),
            self.min_interval_ms,
        )
            .cmp(&(
                other.high_accuracy,
                other.background,
                canon_bits(other.max_accuracy_m),
                other.min_interval_ms,
            ))
    }
}

impl core::hash::Hash for GeolocationProbeConfig {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.high_accuracy.hash(state);
        self.background.hash(state);
        canon_bits(self.max_accuracy_m).hash(state);
        self.min_interval_ms.hash(state);
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::eq_op, clippy::unusual_byte_groupings)]
mod autotest_generated {
    use core::{
        cmp::Ordering,
        hash::{Hash, Hasher},
    };

    use super::*;

    // ---------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------

    /// A fix with every optional field *present* (no NaN anywhere), so a
    /// test can knock out exactly one field and watch only that getter flip.
    const fn fix_all_present() -> LocationFix {
        LocationFix {
            latitude_deg: 48.208_8,
            longitude_deg: 16.372_1,
            accuracy_m: 5.0,
            altitude_m: 171.0,
            altitude_accuracy_m: 3.0,
            heading_deg: 90.0,
            speed_mps: 1.5,
            timestamp_ms: 1_234,
        }
    }

    /// The "platform reported nothing but lat/lon" fix the module docs describe.
    const fn fix_all_absent() -> LocationFix {
        LocationFix {
            latitude_deg: 0.0,
            longitude_deg: 0.0,
            accuracy_m: 0.0,
            altitude_m: f32::NAN,
            altitude_accuracy_m: f32::NAN,
            heading_deg: f32::NAN,
            speed_mps: f32::NAN,
            timestamp_ms: 0,
        }
    }

    const fn cfg(
        high_accuracy: bool,
        background: bool,
        max_accuracy_m: f32,
        min_interval_ms: u32,
    ) -> GeolocationProbeConfig {
        GeolocationProbeConfig {
            high_accuracy,
            background,
            max_accuracy_m,
            min_interval_ms,
        }
    }

    /// Bit-exact float identity (so `-0.0 != 0.0`), but NaN-tolerant: any NaN
    /// matches any NaN. A plain `==` would report `NaN != NaN` and make every
    /// round-trip assertion below vacuously fail; comparing raw payload bits
    /// would over-constrain (Rust does not guarantee NaN payload propagation).
    fn same_f32(a: f32, b: f32) -> bool {
        if a.is_nan() {
            b.is_nan()
        } else {
            a.to_bits() == b.to_bits()
        }
    }

    fn same_f64(a: f64, b: f64) -> bool {
        if a.is_nan() {
            b.is_nan()
        } else {
            a.to_bits() == b.to_bits()
        }
    }

    fn same_fix(a: &LocationFix, b: &LocationFix) -> bool {
        same_f64(a.latitude_deg, b.latitude_deg)
            && same_f64(a.longitude_deg, b.longitude_deg)
            && same_f32(a.accuracy_m, b.accuracy_m)
            && same_f32(a.altitude_m, b.altitude_m)
            && same_f32(a.altitude_accuracy_m, b.altitude_accuracy_m)
            && same_f32(a.heading_deg, b.heading_deg)
            && same_f32(a.speed_mps, b.speed_mps)
            && a.timestamp_ms == b.timestamp_ms
    }

    /// FNV-1a — a `no_std`-safe, deterministic stand-in for `DefaultHasher`
    /// (this crate is `no_std` without the `std` feature).
    struct Fnv1a(u64);

    impl Fnv1a {
        const fn new() -> Self {
            Self(0xcbf2_9ce4_8422_2325)
        }
    }

    impl Hasher for Fnv1a {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for b in bytes {
                self.0 ^= u64::from(*b);
                self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }

    fn hash_of(c: &GeolocationProbeConfig) -> u64 {
        let mut h = Fnv1a::new();
        c.hash(&mut h);
        h.finish()
    }

    /// Every interesting `f32` class, as raw bits, including four distinct NaN
    /// encodings (quiet, negative-quiet, signalling, all-ones payload).
    const F32_PATTERNS: [u32; 16] = [
        0x0000_0000, // +0.0
        0x8000_0000, // -0.0
        0x0000_0001, // smallest positive subnormal
        0x8000_0001, // smallest negative subnormal
        0x007f_ffff, // largest subnormal
        0x0080_0000, // f32::MIN_POSITIVE
        0x3f80_0000, // 1.0
        0xbf80_0000, // -1.0
        0x7f7f_ffff, // f32::MAX
        0xff7f_ffff, // f32::MIN
        0x7f80_0000, // +inf
        0xff80_0000, // -inf
        0x7fc0_0000, // quiet NaN
        0xffc0_0000, // negative quiet NaN
        0x7f80_0001, // signalling NaN
        0x7fff_ffff, // NaN, all-ones payload
    ];

    // ---------------------------------------------------------------
    // LocationFix getters — the documented contract is
    // "NaN (and only NaN) means the platform did not report the field"
    // ---------------------------------------------------------------

    /// The core invariant, swept over every float class: a getter returns
    /// `None` **iff** its backing field is NaN, and otherwise hands back the
    /// value bit-for-bit — including infinities, subnormals and `-0.0`, which
    /// are all "reported" values and must NOT be swallowed like NaN is.
    #[test]
    fn getter_is_none_exactly_when_field_is_nan() {
        for bits in F32_PATTERNS {
            let v = f32::from_bits(bits);

            // Each getter is exercised on a fix where *only* its own field
            // carries the pattern, so a getter reading the wrong field is caught.
            let mut alt = fix_all_present();
            alt.altitude_m = v;
            let mut alt_acc = fix_all_present();
            alt_acc.altitude_accuracy_m = v;
            let mut head = fix_all_present();
            head.heading_deg = v;
            let mut spd = fix_all_present();
            spd.speed_mps = v;

            let got = [
                ("altitude", alt.altitude()),
                ("altitude_accuracy", alt_acc.altitude_accuracy()),
                ("heading", head.heading()),
                ("speed", spd.speed()),
            ];

            for (name, out) in got {
                if v.is_nan() {
                    assert!(
                        out.is_none(),
                        "{name}() must be None for NaN bits {bits:#010x}, got {out:?}"
                    );
                } else {
                    let inner = out.unwrap_or_else(|| {
                        panic!("{name}() must be Some for non-NaN bits {bits:#010x}")
                    });
                    assert_eq!(
                        inner.to_bits(),
                        bits,
                        "{name}() must return the field verbatim for {bits:#010x}"
                    );
                }
            }

            // The other three getters are untouched by the pattern under test.
            assert_eq!(alt.heading(), Some(90.0));
            assert_eq!(alt_acc.speed(), Some(1.5));
            assert_eq!(head.altitude(), Some(171.0));
            assert_eq!(spd.altitude_accuracy(), Some(3.0));
        }
    }

    /// `-0.0` is a legitimate reported value (a stationary device at sea level,
    /// a southbound heading rounded to zero). It must survive the getter with
    /// its sign bit intact rather than being normalised to `+0.0`.
    #[test]
    fn getters_preserve_negative_zero() {
        let mut fix = fix_all_present();
        fix.altitude_m = -0.0;
        fix.altitude_accuracy_m = -0.0;
        fix.heading_deg = -0.0;
        fix.speed_mps = -0.0;

        for out in [
            fix.altitude(),
            fix.altitude_accuracy(),
            fix.heading(),
            fix.speed(),
        ] {
            let v = out.expect("-0.0 is not NaN, so the field is 'reported'");
            assert_eq!(v.to_bits(), 0x8000_0000, "sign bit of -0.0 was dropped");
            assert!(v == 0.0, "-0.0 must still compare equal to 0.0");
        }
    }

    /// Infinities are not NaN, so per the documented contract they are handed
    /// through as `Some(inf)` — the getters must not "helpfully" filter them.
    #[test]
    fn getters_pass_infinities_through() {
        let mut fix = fix_all_absent();
        fix.altitude_m = f32::INFINITY;
        fix.altitude_accuracy_m = f32::NEG_INFINITY;
        fix.heading_deg = f32::INFINITY;
        fix.speed_mps = f32::NEG_INFINITY;

        assert_eq!(fix.altitude(), Some(f32::INFINITY));
        assert_eq!(fix.altitude_accuracy(), Some(f32::NEG_INFINITY));
        assert_eq!(fix.heading(), Some(f32::INFINITY));
        assert_eq!(fix.speed(), Some(f32::NEG_INFINITY));
    }

    /// The "platform reported nothing" fix, plus a saturated timestamp and
    /// out-of-range WGS-84 coordinates: nothing here may panic, and every
    /// optional getter must report absence.
    #[test]
    fn getters_on_extreme_and_all_absent_fix() {
        let mut fix = fix_all_absent();
        fix.latitude_deg = f64::MAX;
        fix.longitude_deg = f64::MIN;
        fix.accuracy_m = f32::MAX;
        fix.timestamp_ms = u64::MAX;

        assert_eq!(fix.altitude(), None);
        assert_eq!(fix.altitude_accuracy(), None);
        assert_eq!(fix.heading(), None);
        assert_eq!(fix.speed(), None);

        // Getters are `&self` — they must not have mutated the receiver.
        assert_eq!(fix.timestamp_ms, u64::MAX);
        assert!(fix.latitude_deg == f64::MAX);

        // Default-ish zeroed fix: 0.0 is not NaN, so everything is "reported".
        let zeroed = LocationFix {
            latitude_deg: 0.0,
            longitude_deg: 0.0,
            accuracy_m: 0.0,
            altitude_m: 0.0,
            altitude_accuracy_m: 0.0,
            heading_deg: 0.0,
            speed_mps: 0.0,
            timestamp_ms: 0,
        };
        assert_eq!(zeroed.altitude(), Some(0.0));
        assert_eq!(zeroed.altitude_accuracy(), Some(0.0));
        assert_eq!(zeroed.heading(), Some(0.0));
        assert_eq!(zeroed.speed(), Some(0.0));
    }

    /// The getters are declared `const fn`; that is part of the public API, so
    /// pin it — a non-const rewrite would be a silent breaking change.
    #[test]
    fn getters_are_const_evaluable() {
        const PRESENT: LocationFix = fix_all_present();
        const ABSENT: LocationFix = fix_all_absent();

        const ALT: Option<f32> = PRESENT.altitude();
        const ALT_ACC: Option<f32> = PRESENT.altitude_accuracy();
        const HEADING: Option<f32> = PRESENT.heading();
        const SPEED: Option<f32> = PRESENT.speed();
        const NO_ALT: Option<f32> = ABSENT.altitude();
        const NO_HEADING: Option<f32> = ABSENT.heading();

        assert_eq!(ALT, Some(171.0));
        assert_eq!(ALT_ACC, Some(3.0));
        assert_eq!(HEADING, Some(90.0));
        assert_eq!(SPEED, Some(1.5));
        assert_eq!(NO_ALT, None);
        assert_eq!(NO_HEADING, None);
    }

    /// `LocationFix` derives `PartialEq` over raw floats, so a fix carrying an
    /// unreported (NaN) field is *not* equal to itself. Pinned deliberately:
    /// the type is (correctly) not `Eq`/`Hash`, and any future use as a map key
    /// or in a `!=`-based change check would be broken by this. Callers must
    /// compare via the getters, as `same_fix` does.
    #[test]
    fn location_fix_partial_eq_is_not_reflexive_when_fields_are_absent() {
        let absent = fix_all_absent();
        assert!(absent != absent, "IEEE semantics: NaN != NaN");
        assert!(same_fix(&absent, &absent), "but it is the same fix");

        let present = fix_all_present();
        assert!(present == present, "a fully-reported fix is self-equal");
        assert!(same_fix(&present, &present));

        // `-0.0 == 0.0` under PartialEq, but the two are distinguishable.
        let mut neg_zero = fix_all_present();
        neg_zero.altitude_m = -0.0;
        let mut pos_zero = fix_all_present();
        pos_zero.altitude_m = 0.0;
        assert!(neg_zero == pos_zero);
        assert!(!same_fix(&neg_zero, &pos_zero));
    }

    // ---------------------------------------------------------------
    // OptionLocationFix — the FFI wrapper: encode == decode
    // ---------------------------------------------------------------

    /// Round-trip `Option<LocationFix>` -> `OptionLocationFix` -> back, for both
    /// variants, plus the full wrapper surface (`replace` must return the
    /// *previous* value, `map`/`and_then` must short-circuit on `None`).
    #[test]
    fn option_location_fix_roundtrips() {
        assert!(OptionLocationFix::default().is_none());
        assert!(!OptionLocationFix::default().is_some());
        assert_eq!(
            Option::<LocationFix>::from(OptionLocationFix::default()),
            None
        );

        let fix = fix_all_present();
        let wrapped: OptionLocationFix = Some(fix).into();
        assert!(wrapped.is_some());
        assert!(!wrapped.is_none());
        assert_eq!(wrapped.as_ref(), Some(&fix));
        assert_eq!(wrapped.as_option(), Some(&fix));
        assert_eq!(wrapped.into_option(), Some(fix));

        let decoded = Option::<LocationFix>::from(wrapped).expect("Some in, Some out");
        assert!(same_fix(&decoded, &fix), "encode == decode");

        let none: OptionLocationFix = Option::<LocationFix>::None.into();
        assert!(none.is_none());
        assert_eq!(none.as_ref(), None);
        assert_eq!(none.map(|f| f.timestamp_ms), None);
        assert_eq!(none.and_then(|f| f.altitude()), None);
        assert_eq!(wrapped.map(|f| f.timestamp_ms), Some(1_234));
        assert_eq!(wrapped.and_then(|f| f.altitude()), Some(171.0));

        // `replace` has mem::replace semantics: it yields the OLD value.
        let mut slot = OptionLocationFix::None;
        assert!(slot.replace(fix).is_none());
        assert_eq!(slot.as_option(), Some(&fix));

        let other = fix_all_absent();
        let prev = slot.replace(other);
        assert!(prev.is_some());
        assert!(same_fix(
            &Option::<LocationFix>::from(prev).expect("previous fix"),
            &fix
        ));
        assert!(same_fix(
            slot.as_option().expect("current fix"),
            &fix_all_absent()
        ));

        // `as_mut` hands out the live payload.
        if let Some(inner) = slot.as_mut() {
            inner.timestamp_ms = u64::MAX;
        }
        assert_eq!(slot.as_option().expect("still Some").timestamp_ms, u64::MAX);
    }

    /// The wrapper must survive the exact payload the platform layer produces
    /// most often: a fix whose optional fields are all NaN. `PartialEq` on the
    /// payload is useless here (NaN != NaN), so the round-trip is checked
    /// field-wise — this is the case a naive `assert_eq!` would hide.
    #[test]
    fn option_location_fix_roundtrip_survives_absent_fields() {
        let mut fix = fix_all_absent();
        fix.latitude_deg = -89.999_999_9;
        fix.longitude_deg = 179.999_999_9;
        fix.accuracy_m = f32::MIN_POSITIVE;
        fix.timestamp_ms = u64::MAX;

        let decoded = Option::<LocationFix>::from(OptionLocationFix::Some(fix))
            .expect("Some survives the round-trip");

        assert!(same_fix(&decoded, &fix), "encode == decode for a NaN-y fix");
        assert_eq!(decoded.altitude(), None);
        assert_eq!(decoded.altitude_accuracy(), None);
        assert_eq!(decoded.heading(), None);
        assert_eq!(decoded.speed(), None);
        assert_eq!(decoded.timestamp_ms, u64::MAX);

        // The wrapper's own derived PartialEq inherits the NaN quirk; assert it
        // rather than trusting `assert_eq!` to mean anything here.
        assert!(OptionLocationFix::Some(fix) != OptionLocationFix::Some(fix));
        assert!(OptionLocationFix::None == OptionLocationFix::None);
    }

    /// `#[repr(C, u8)]` means the wrapper is tag + payload with no niche
    /// packing — it must be strictly larger than the payload, or the C ABI
    /// header generated for it would be wrong.
    #[test]
    fn option_location_fix_is_tagged_not_niche_packed() {
        use core::mem::{align_of, size_of};
        assert!(
            size_of::<OptionLocationFix>() > size_of::<LocationFix>(),
            "repr(C, u8) must carry an explicit discriminant"
        );
        assert!(align_of::<OptionLocationFix>() >= align_of::<LocationFix>());
        assert!(size_of::<LocationFix>() >= 2 * size_of::<f64>() + 5 * size_of::<f32>());
    }

    // ---------------------------------------------------------------
    // GeolocationProbeConfig — Default / Ord / Hash
    // ---------------------------------------------------------------

    /// The documented default: no high accuracy, no background, no accuracy
    /// filter (`0` = "deliver every sample"), no throttle.
    #[test]
    fn probe_config_default_is_permissive_zero() {
        let d = GeolocationProbeConfig::default();
        assert!(!d.high_accuracy);
        assert!(!d.background);
        assert!(d.max_accuracy_m == 0.0);
        assert_eq!(d.max_accuracy_m.to_bits(), 0, "default must be +0.0, not -0.0");
        assert_eq!(d.min_interval_ms, 0);
        assert_eq!(d, cfg(false, false, 0.0, 0));
        assert_eq!(d.cmp(&GeolocationProbeConfig::default()), Ordering::Equal);
    }

    /// Field precedence of the hand-written `Ord`: high_accuracy, then
    /// background, then accuracy bits, then interval. Each earlier field must
    /// dominate *every* later one, so the loser is given maximal later fields.
    #[test]
    fn probe_config_ord_field_precedence() {
        let low_but_maxed = cfg(false, true, f32::from_bits(u32::MAX), u32::MAX);
        let high_but_minimal = cfg(true, false, 0.0, 0);
        assert!(low_but_maxed < high_but_minimal, "high_accuracy dominates");

        let fg_maxed = cfg(true, false, f32::from_bits(u32::MAX), u32::MAX);
        let bg_minimal = cfg(true, true, 0.0, 0);
        assert!(fg_maxed < bg_minimal, "background dominates the numeric fields");

        let small_acc = cfg(true, true, 1.0, u32::MAX);
        let big_acc = cfg(true, true, 2.0, 0);
        assert!(small_acc < big_acc, "accuracy bits dominate the interval");

        assert!(cfg(true, true, 1.0, 0) < cfg(true, true, 1.0, 1));
        assert!(cfg(true, true, 1.0, u32::MAX - 1) < cfg(true, true, 1.0, u32::MAX));
        assert_eq!(
            cfg(true, true, 1.0, u32::MAX).cmp(&cfg(true, true, 1.0, u32::MAX)),
            Ordering::Equal
        );
    }

    /// The ordering is over `to_bits`, NOT over numeric value: sign-magnitude
    /// bits mean every negative accuracy sorts *above* every positive one, and
    /// NaN sorts above +inf. Pinned because it is load-bearing (`NodeType`
    /// dedup/sort) and because it is exactly the trap a reader assumes away.
    #[test]
    fn probe_config_ord_is_bitwise_not_numeric() {
        let neg = cfg(false, false, -1.0, 0);
        let pos = cfg(false, false, 1.0, 0);
        assert!(neg.max_accuracy_m < pos.max_accuracy_m, "numerically: -1 < 1");
        assert_eq!(
            neg.cmp(&pos),
            Ordering::Greater,
            "bitwise: 0xbf80_0000 > 0x3f80_0000"
        );

        let inf = cfg(false, false, f32::INFINITY, 0);
        let nan = cfg(false, false, f32::NAN, 0);
        assert_eq!(nan.cmp(&inf), Ordering::Greater, "NaN bits sort above +inf");
        assert_eq!(inf.cmp(&pos), Ordering::Greater);
    }

    /// A NaN accuracy must not break the total order: `cmp` stays reflexive,
    /// antisymmetric and transitive, and sorting a NaN-containing slice
    /// terminates without panicking (an inconsistent comparator can make
    /// `sort_unstable` misbehave).
    #[test]
    fn probe_config_ord_is_a_total_order_with_nan() {
        let nan = cfg(false, false, f32::NAN, 7);
        assert_eq!(nan.cmp(&nan), Ordering::Equal, "cmp must be reflexive");
        assert_eq!(nan.partial_cmp(&nan), Some(Ordering::Equal));

        let mut list = [
            cfg(true, true, f32::NAN, u32::MAX),
            cfg(false, false, -0.0, 0),
            cfg(false, false, f32::INFINITY, 1),
            nan,
            cfg(true, false, f32::NEG_INFINITY, 3),
            cfg(false, true, 0.0, u32::MAX),
            GeolocationProbeConfig::default(),
        ];
        list.sort_unstable();
        for w in list.windows(2) {
            assert_ne!(
                w[0].cmp(&w[1]),
                Ordering::Greater,
                "sort_unstable must leave the slice ordered under cmp"
            );
        }

        // Antisymmetry + transitivity across every pair/triple in the slice.
        for a in list {
            for b in list {
                assert_eq!(
                    a.cmp(&b),
                    b.cmp(&a).reverse(),
                    "cmp must be antisymmetric"
                );
                for c in list {
                    if a.cmp(&b) != Ordering::Greater && b.cmp(&c) != Ordering::Greater {
                        assert_ne!(a.cmp(&c), Ordering::Greater, "cmp must be transitive");
                    }
                }
            }
        }
    }

    /// `PartialOrd` must agree with `Ord`, and — for the ordinary (finite,
    /// non-negative) configs a user actually writes — `==`, `cmp == Equal` and
    /// equal hashes must all coincide. This is the Eq/Ord/Hash contract holding
    /// on the reachable domain; the two regression tests below pin the -0.0/+0.0
    /// and NaN corners where a naive derived `PartialEq` would break it.
    #[test]
    fn probe_config_eq_ord_hash_agree_on_ordinary_configs() {
        let configs = [
            GeolocationProbeConfig::default(),
            cfg(false, false, 25.0, 1_000),
            cfg(true, false, 5.0, 0),
            cfg(true, true, 0.5, u32::MAX),
            cfg(false, true, f32::MAX, 1),
            cfg(true, true, f32::MIN_POSITIVE, 16),
        ];

        for a in configs {
            for b in configs {
                assert_eq!(a.partial_cmp(&b), Some(a.cmp(&b)), "PartialOrd must mirror Ord");
                assert_eq!(
                    a == b,
                    a.cmp(&b) == Ordering::Equal,
                    "`==` must agree with `cmp == Equal`"
                );
                if a == b {
                    assert_eq!(hash_of(&a), hash_of(&b), "Eq implies equal hashes");
                }
            }
            // Reflexivity of the whole trio.
            assert_eq!(a, a);
            assert_eq!(a.cmp(&a), Ordering::Equal);
            assert_eq!(hash_of(&a), hash_of(&a));
        }
    }

    /// Hashing must be a pure function of the four fields, must survive NaN
    /// (`to_bits` cannot panic), and must actually distinguish configs that
    /// differ only in a late field — otherwise `NodeType` dedup collapses
    /// distinct probes.
    #[test]
    fn probe_config_hash_is_stable_and_discriminating() {
        let a = cfg(true, false, 12.5, 250);
        assert_eq!(hash_of(&a), hash_of(&cfg(true, false, 12.5, 250)));

        let nan = cfg(false, false, f32::NAN, 0);
        assert_eq!(hash_of(&nan), hash_of(&nan), "hashing NaN must be stable");

        for other in [
            cfg(false, false, 12.5, 250),
            cfg(true, true, 12.5, 250),
            cfg(true, false, 12.75, 250),
            cfg(true, false, 12.5, 251),
        ] {
            assert_ne!(a, other);
            assert_ne!(
                hash_of(&a),
                hash_of(&other),
                "configs differing in one field must not collide: {other:?}"
            );
        }
    }

    // ---------------------------------------------------------------
    // Regression guards for Eq/Ord/Hash consistency of the f32 config field
    // (canonicalized bits: -0.0 == +0.0, NaN == NaN). The derived PartialEq used
    // to disagree with the hand-written Ord/Hash.
    // ---------------------------------------------------------------

    /// `impl Eq` + `impl Hash` promise: `a == b` implies `hash(a) == hash(b)`
    /// and `a.cmp(&b) == Equal`. A derived `PartialEq` would compare
    /// `max_accuracy_m` numerically (`-0.0 == 0.0`) while `Hash`/`Ord` compared
    /// raw `to_bits` (where they differ), so a `HashMap`/`BTreeMap` keyed on a
    /// `NodeType` carrying this config could fail to find an entry that compares
    /// equal. The hand-written `PartialEq`/`Ord`/`Hash` all route through
    /// `canon_bits`, keeping them consistent — this pins that.
    #[test]
    fn probe_config_eq_implies_equal_hash_and_ordering() {
        let pos = cfg(false, false, 0.0, 0);
        let neg = cfg(false, false, -0.0, 0);

        assert_eq!(pos, neg, "IEEE: -0.0 == 0.0");
        assert_eq!(hash_of(&pos), hash_of(&neg), "Eq/Hash contract");
        assert_eq!(pos.cmp(&neg), Ordering::Equal, "Eq/Ord contract");
    }

    /// `impl Eq for GeolocationProbeConfig {}` asserts reflexivity. A derived
    /// `PartialEq` comparing `max_accuracy_m` with float `==` would make a
    /// config with a NaN accuracy unequal to itself — while `Ord` (via
    /// `canon_bits`) reports `Equal` for the very same pair. The hand-written
    /// `PartialEq` uses `canon_bits` too, so both agree; this pins it.
    #[test]
    #[allow(clippy::eq_op)]
    fn probe_config_eq_is_reflexive_with_nan() {
        let c = cfg(false, false, f32::NAN, 0);

        assert_eq!(c.cmp(&c), Ordering::Equal, "Ord says equal");
        assert_eq!(c, c, "so Eq must too");
    }
}
