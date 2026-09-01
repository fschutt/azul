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
    // APPENDED at the end for ABI stability. The three above are the hard
    // ones — they need real per-OS backends and have them. Most of what
    // follows is DERIVED by the OS from those three, which is exactly why it
    // is worth exposing rather than making every app redo the fusion badly.
    /// Device orientation as a unit quaternion, x/y/z carrying the vector
    /// part (Android `TYPE_ROTATION_VECTOR`, iOS `CMAttitude.quaternion`).
    ///
    /// The OS fuses accelerometer, gyroscope and magnetometer to produce it,
    /// with drift correction an app cannot reproduce from the raw three.
    RotationVector,
    /// Gravity alone, in **m/s²** — the accelerometer with device motion
    /// removed (Android `TYPE_GRAVITY`, iOS `CMDeviceMotion.gravity`).
    Gravity,
    /// Device motion alone, in **m/s²** — the accelerometer with gravity
    /// removed (Android `TYPE_LINEAR_ACCELERATION`,
    /// iOS `CMDeviceMotion.userAcceleration`).
    ///
    /// `Gravity` and this always sum to `Accelerometer`; they are separate
    /// kinds because the split is what the OS's fusion buys you.
    LinearAcceleration,
    /// Illuminance in **lux**, in `x`. `y`/`z` unused.
    ///
    /// The signal behind "adapt to a dark room" — a UI dimming itself,
    /// a camera view raising exposure.
    AmbientLight,
    /// Proximity in **cm**, in `x`. `y`/`z` unused.
    ///
    /// Many phone sensors are binary and report only their maximum range or
    /// `0.0`, so treat a small value as "near" rather than as a distance.
    Proximity,
    /// Atmospheric pressure in **hPa**, in `x`. `y`/`z` unused. Used for
    /// relative altitude, which GPS gives poorly.
    Barometer,
    /// Cumulative step count since boot, in `x`. `y`/`z` unused.
    ///
    /// Monotonic and NOT resettable — an app takes differences against its
    /// own baseline rather than expecting it to start at zero.
    StepCounter,
    /// Foldable hinge angle in **degrees**, in `x`: `0.0` fully closed,
    /// `180.0` flat. `y`/`z` unused.
    ///
    /// A LAYOUT input more than a sensor. Android exposes it as
    /// `TYPE_HINGE_ANGLE` and the web as `DevicePosture`, and it is the only
    /// way to tell a book-posture fold from a laptop-posture one — which
    /// decides whether a two-pane layout should split across the crease.
    HingeAngle,
}

impl SensorKind {
    /// How many kinds exist — the length of a slot array indexed by
    /// [`Self::slot`].
    pub const COUNT: usize = 11;

    /// Dense index for this kind, for a fixed-size slot array.
    ///
    /// An array rather than one named field per kind: the set grew from 3 to
    /// 11 and would have needed a new field, two new match arms and a new
    /// accessor each time. Indexing keeps adding a kind to one line.
    #[must_use]
    pub const fn slot(self) -> usize {
        match self {
            Self::Accelerometer => 0,
            Self::Gyroscope => 1,
            Self::Magnetometer => 2,
            Self::RotationVector => 3,
            Self::Gravity => 4,
            Self::LinearAcceleration => 5,
            Self::AmbientLight => 6,
            Self::Proximity => 7,
            Self::Barometer => 8,
            Self::StepCounter => 9,
            Self::HingeAngle => 10,
        }
    }
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
    #[must_use]
    pub fn magnitude(&self) -> f32 {
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
#[path = "sensors_test.rs"]
mod sensors_test;
