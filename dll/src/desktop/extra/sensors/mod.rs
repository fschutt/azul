//! Platform dispatcher for motion-sensor subscriptions
//! (SUPER_PLAN_2 §1 feature 5 + research/03).
//!
//! Cross-platform state lives in
//! `azul_layout::managers::sensors::SensorManager`. The subscription is
//! continuous and push-driven (unlike biometric's request/reply): the
//! backend registers once and the OS streams samples on its own thread.
//!
//! | Platform | Subscribe | Sample → channel |
//! |----------|-----------|------------------|
//! | iOS / macOS | `CMMotionManager` start*Updates (objc2-core-motion) | update handler block → `push_sensor_reading` |
//! | Android | `SensorManager.registerListener` (JNI via `AzulSensors`) | `onSensorChanged` → `nativeOnSensorReading` → `push_sensor_reading` |
//! | Linux | iio sysfs (`/sys/bus/iio/devices`, pull) | `poll` reads raw*scale → `push_sensor_reading` |
//! | Windows | — (no motion sensors wired yet) | — |
//!
//! [`ensure_started`] kicks the subscription exactly once per process from
//! the layout pass (OnceLock-guarded — registering is a native call, so we
//! don't redo it at frame rate). Samples land in azul-layout's
//! process-global channel; the layout pass drains them (`drain_sensor_readings`)
//! into the manager, where `CallbackInfo::get_sensor_reading` reads them.
//!
//! Apple delivers via a per-frame [`poll`] of CoreMotion's pull API; Android
//! is push-driven (the JNI `onSensorChanged` callback parks samples directly),
//! so `poll` is Apple-only. As with `AzulBiometric`, the Android
//! `AzulSensors.java` helper itself is a deferred (non-Rust) batch — until it
//! ships, `find_class` fails and no Android samples flow, but the Rust path
//! is complete.

#[cfg(any(target_os = "ios", target_os = "macos"))]
pub mod apple;
#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

/// Start the device's motion-sensor subscription once per process. Called
/// from `regenerate_layout` every frame; the OnceLock makes only the first
/// call do the native registration (CoreMotion start / JNI `registerListener`),
/// after which it's a cheap atomic read.
pub fn ensure_started() {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    STARTED.get_or_init(start);
}

fn start() {
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    apple::start();
    #[cfg(target_os = "android")]
    android::start();
    #[cfg(target_os = "linux")]
    linux::start();
    #[cfg(target_os = "windows")]
    windows::start();
    // Other targets: no motion sensors wired — `get_sensor_reading` stays `None`.
}

/// Pull the latest sample from each sensor into the async channel. Called
/// once per layout pass (after [`ensure_started`]). Apple-only: CoreMotion's
/// pull API needs a per-frame read, whereas Android pushes from its JNI
/// callback. A no-op until [`ensure_started`] has run and on Android/desktop.
pub fn poll() {
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    apple::poll();
    #[cfg(target_os = "linux")]
    linux::poll();
    #[cfg(target_os = "windows")]
    windows::poll();
}
