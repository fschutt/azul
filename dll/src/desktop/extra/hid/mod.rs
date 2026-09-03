//! Generic HID: the escape hatch for devices azul does not model.
//!
//! `GamepadState` assumes an Xbox-shaped controller. Flight sticks, racing
//! wheels, 6-DOF SpaceMice, foot pedals and Stream Decks are not that shape, so
//! `azul_core::hid` exposes the raw report and lets the app decode it - the
//! same trade WebHID makes.
//!
//! The consumer side has been complete for a while: `HidManager`,
//! `CallbackInfo::get_hid_devices()` / `get_hid_reports()`, and (since
//! 9g-ii-c) the `HidDeviceVec` / `HidReportVec` types that let a binding read
//! them. Nothing produced. This is the producer.
//!
//! Per-platform:
//! - **Linux**: `/dev/hidraw*`, implemented here. No library to load.
//! - **macOS**: `IOHIDManager`, implemented here. dlopen'd; needs Input Monitoring.
//! - **Windows**: `RIM_TYPEHID` through the `WM_INPUT` arm 9d-i already built.
//!   Needs NO hid.dll for the stream: `GetRawInputDeviceInfoW(RIDI_DEVICEINFO)`
//!   carries the vid/pid/usage directly. hid.dll IS loaded, lazily, for the
//!   two things raw input cannot answer - the serial STRING (8f-i-a-i-a) and
//!   feature reports (8f-i-a-i-b-i) - on a handle opened for the call.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

/// Enumerate HID devices and publish the list. Idempotent.
pub fn enumerate() {
    #[cfg(target_os = "linux")]
    linux::enumerate();
    #[cfg(target_os = "macos")]
    macos::enumerate();
    // Windows enumerates at WINDOW CREATION instead: it needs the loaded
    // `User32Functions` table, which this signature has no path to, and the
    // raw-input registration happens there anyway. Nothing to do here.
}

/// Poll every open device for queued reports. Called once per pump pass.
pub fn poll() {
    #[cfg(target_os = "linux")]
    linux::poll();
    #[cfg(target_os = "macos")]
    macos::poll();
    // Windows delivers through `WM_INPUT`, like macOS delivers through the
    // run loop. Only Linux sweeps, because hidraw has no callback.
}
