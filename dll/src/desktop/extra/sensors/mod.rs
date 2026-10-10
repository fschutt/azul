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
//! | Windows | `Windows.Devices.Sensors` `GetDefault()` | `poll` reads `GetCurrentReading()` → `push_sensor_reading` |
//!
//! All ELEVEN `SensorKind`s are now produced on Android, and the fused ones
//! (rotation vector, gravity, linear acceleration) on all four platforms.
//! The remaining gaps are per-platform and deliberate - see 8e-i-a-i/ii in
//! `scripts/INPUT_WIRING_PROGRESS.md`. Two backends are no longer purely
//! polled: the WinRT hinge sensor and the two iOS push-only sensors deliver
//! through callbacks into the same channel, which is exactly why the channel
//! exists.
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
//!
//! # The device-state readings
//!
//! Beside the motion sensors live the readings an app asks before background work, each a
//! synchronous query of a cached or cheap value: [`power`] (`PowerState`: on mains power,
//! seconds since the last input), [`battery`] (`BatteryState`: a battery, its charger, its
//! level, the power-saver switch, the thermal state) and [`network`] (`NetworkState`:
//! connected, its kind, metered, constrained, the hotspot estimate). api.json's `sensor`
//! module exposes them, through `crate::unified::sensors`.
//!
//! # Privacy: what the device-state readings read
//!
//! They exist so an app can be gentle with the user's battery and data plan, and they are
//! built so that using them reveals nothing about the user:
//!
//! - READ: whether the computer runs from mains power, and the seconds since the last input
//!   (not which input, not what it was); whether there is a system battery, whether it is on
//!   its charger, its level in percent, the system's power-saver switch, and the system's own
//!   thermal level (on Linux the thermal zones' temperatures are compared with their own trip
//!   points and not kept); whether the network is connected, its kind (wired, Wi-Fi,
//!   cellular, other), and the system's own flags on it - metered, constrained - from which the
//!   hotspot ESTIMATE is made.
//! - NEVER READ: a network's name (SSID) or access point (BSSID), the carrier, a phone number,
//!   an IP or MAC address, a location, the battery's model, serial number or health, or any
//!   other identifier of the device, the network or the user. No platform call that answers
//!   one is made, so none of these readings needs a location, phone-state or Wi-Fi permission.
//! - NOTHING LEAVES THE DEVICE: a reading stays in the process that asked for it. azul sends
//!   none of it anywhere, logs none of it and writes none of it to a file (a headless run only
//!   READS its switch files). An app that shares a summary with its peers shares one number it
//!   computed itself - AzDrive's and AzMeet's `client_health` (azul-appkit), 0 to 100 - never
//!   the parts.

/// Unit conversions, compiled on every platform so the arithmetic each
/// backend depends on is covered by tests that actually run here.
pub mod units;

/// The device's battery and temperature for background work (`BatteryState`: present,
/// charging, its level, Low Power Mode, the thermal state), kept current by a monitor thread;
/// a fixed reading (or a test's switch file) in headless runs. Its platform files sit in
/// `battery/`.
pub mod battery;
/// The computer's network state for background transfers (`NetworkState`: connected, metered,
/// constrained, its kind), kept current by a platform monitor; a fixed reading (or a test's
/// switch file) in headless runs. Its platform files sit in `network/`, split as the motion
/// backends are.
pub mod network;
/// The computer's power state for background work (`PowerState`: on mains power, seconds
/// since the last input), per platform; a fixed reading in headless runs.
pub mod power;

/// A headless or E2E run (`AZ_BACKEND=headless`, `AZ_E2E_TEST`; the biometric module's test,
/// the same variables): the device-state readings answer fixed values or a test's switch file,
/// never the machine's own.
pub(crate) fn headless_run() -> bool {
    std::env::var("AZ_BACKEND").as_deref() == Ok("headless") || std::env::var("AZ_E2E_TEST").is_ok()
}

/// The words of a headless run's switch file at `path` (the battery's, the network's), read at
/// every query so a test changes them while the app runs; `None` without a file, or one that
/// cannot be read.
pub(crate) fn switch_file_words(path: Option<&std::path::Path>) -> Option<String> {
    path.and_then(|path| std::fs::read_to_string(path).ok())
}

/// One reading through JNI on this thread, attached to the app's Java VM for it (detached
/// again after, which frees the reading's local references); `None` before the activity
/// published its VM and when `read` cannot ask (its pending exception cleared). The network's
/// and the battery's monitor threads read their services this way.
#[cfg(target_os = "android")]
pub(crate) fn with_activity<T>(
    read: impl FnOnce(&mut jni::JNIEnv<'_>, &jni::objects::JObject<'_>) -> Option<T>,
) -> Option<T> {
    let vm_ptr = crate::desktop::shell2::android::java_vm_ptr();
    let activity_ptr = crate::desktop::shell2::android::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return None;
    }
    // SAFETY: the VM and the activity (a global reference the activity layer keeps alive) the
    // activity published.
    let vm = unsafe { jni::JavaVM::from_raw(vm_ptr.cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    // SAFETY: as above.
    let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr.cast()) };
    let answer = read(&mut env, &activity);
    if answer.is_none() {
        let _ = env.exception_clear();
    }
    answer
}

/// The activity's system service `name` (`Context.getSystemService`: "connectivity",
/// "batterymanager", "power"); `None` when it cannot be asked or there is none.
#[cfg(target_os = "android")]
pub(crate) fn system_service<'local>(
    env: &mut jni::JNIEnv<'local>,
    activity: &jni::objects::JObject<'_>,
    name: &str,
) -> Option<jni::objects::JObject<'local>> {
    use jni::objects::{JObject, JValue};

    let name = JObject::from(env.new_string(name).ok()?);
    let service = env
        .call_method(
            activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&name)],
        )
        .and_then(|v| v.l())
        .ok()?;
    (!service.is_null()).then_some(service)
}

#[cfg(target_os = "android")]
pub mod android;
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub mod apple;
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
    // Brackets the native motion-sensor registration. A reading never arriving
    // is normal (a desktop with no accelerometer → get_sensor_reading stays
    // None, NOT a crash); this log lets the self-test report "unavailable" vs.
    // pinpoint a backend that aborts during start (C4).
    crate::plog_info!("[sensors] starting motion-sensor backend");
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    apple::start();
    #[cfg(target_os = "android")]
    android::start();
    #[cfg(target_os = "linux")]
    linux::start();
    #[cfg(target_os = "windows")]
    windows::start();
    // Other targets: no motion sensors wired — `get_sensor_reading` stays `None`.
    crate::plog_info!("[sensors] motion-sensor backend start complete");
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
