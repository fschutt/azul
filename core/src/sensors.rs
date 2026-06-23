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
