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
//! - **Windows**: `RIM_TYPEHID` through the `WM_INPUT` arm 9d-i already built,
//!   plus `hid.dll` for the vid/pid - 9f-i-b.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;

/// Enumerate HID devices and publish the list. Idempotent.
pub fn enumerate() {
    #[cfg(target_os = "linux")]
    linux::enumerate();
    #[cfg(target_os = "macos")]
    macos::enumerate();
}

/// Poll every open device for queued reports. Called once per pump pass.
pub fn poll() {
    #[cfg(target_os = "linux")]
    linux::poll();
    #[cfg(target_os = "macos")]
    macos::poll();
}
