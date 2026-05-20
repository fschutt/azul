//! Apple (iOS / macOS) motion-sensor backend — CoreMotion.
//!
//! Will subscribe via `CMMotionManager` (objc2-core-motion):
//! `startAccelerometerUpdatesToQueue:withHandler:` + the gyroscope and
//! magnetometer equivalents, each handler block translating its
//! `CMAccelerometerData` / `CMGyroData` / `CMMagnetometerData` sample into a
//! [`azul_core::sensors::SensorReading`] and parking it through
//! `push_sensor_reading` (mirroring how the biometric `LAContext` reply
//! block parks its result).
//!
//! This tick lands the dispatcher + the Android JNI backend; the CoreMotion
//! subscription (which adds the `objc2-core-motion` dep) is the next tick.
//! Until then `start` is a no-op and `get_sensor_reading` returns `None` on
//! Apple platforms.

/// Start CoreMotion motion-sensor updates. No-op until the CoreMotion
/// backend lands (next tick).
pub fn start() {}
