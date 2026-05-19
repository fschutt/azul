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

impl LocationFix {
    pub fn altitude(&self) -> Option<f32> {
        if self.altitude_m.is_nan() {
            None
        } else {
            Some(self.altitude_m)
        }
    }

    pub fn altitude_accuracy(&self) -> Option<f32> {
        if self.altitude_accuracy_m.is_nan() {
            None
        } else {
            Some(self.altitude_accuracy_m)
        }
    }

    pub fn heading(&self) -> Option<f32> {
        if self.heading_deg.is_nan() {
            None
        } else {
            Some(self.heading_deg)
        }
    }

    pub fn speed(&self) -> Option<f32> {
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
#[derive(Debug, Clone, Copy, PartialEq)]
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
            self.max_accuracy_m.to_bits(),
            self.min_interval_ms,
        )
            .cmp(&(
                other.high_accuracy,
                other.background,
                other.max_accuracy_m.to_bits(),
                other.min_interval_ms,
            ))
    }
}

impl core::hash::Hash for GeolocationProbeConfig {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.high_accuracy.hash(state);
        self.background.hash(state);
        self.max_accuracy_m.to_bits().hash(state);
        self.min_interval_ms.hash(state);
    }
}
