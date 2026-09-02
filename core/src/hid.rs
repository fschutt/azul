//! Generic HID — the escape hatch for devices azul does not model.
//!
//! `GamepadState` assumes an Xbox-shaped controller: two sticks, two triggers,
//! a d-pad, a fixed button set. Plenty of real input hardware is not that
//! shape — flight sticks with dozens of axes, racing wheels with pedal
//! clusters and force feedback, 6-DOF SpaceMice, foot pedals, Stream Decks,
//! MIDI controllers, barcode wedges. SDL keeps *joystick* events (arbitrary
//! axes, hats, balls) separate from *gamepad* events for exactly this reason.
//!
//! Rather than grow a taxonomy that can never be complete, this exposes the
//! raw report and lets the app decode it. That is the same trade the web made
//! with WebHID, and it is the right one here: azul cannot know what a
//! particular device's bytes mean, but the app that chose to support that
//! device does.
//!
//! This is deliberately NOT a filter family of its own — a HID report is
//! window-scoped like raw pointer motion, because it has no position and no
//! node it belongs to.

use alloc::vec::Vec;

use azul_css::{
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq,
    AzString,
};

/// Identity of one HID device.
///
/// The four numbers are the HID descriptor's own vocabulary, not azul's:
/// `usage_page` + `usage` say what KIND of device it claims to be (page 0x01
/// usage 0x04 is a joystick, 0x05 a gamepad, 0x08 a multi-axis controller),
/// while vendor and product identify the model. An app matches on whichever
/// pair it needs — usage for "any joystick", vid/pid for "this exact wheel".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct HidDevice {
    /// USB vendor id.
    pub vendor_id: u16,
    /// USB product id.
    pub product_id: u16,
    /// HID usage page of the device's top-level collection.
    pub usage_page: u16,
    /// HID usage within that page.
    pub usage: u16,
    /// Human-readable product string, empty when the device reports none.
    pub name: AzString,
}

/// One input report from a HID device.
///
/// Bytes exactly as the device sent them. Decoding is the app's job: the
/// report descriptor that explains the layout is device-specific, and a
/// framework guessing at it would be wrong more often than useful.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct HidReport {
    /// Which device sent it.
    pub device: HidDevice,
    /// Report id, or `0` for a device whose descriptor uses no ids.
    pub report_id: u8,
    /// The report payload.
    pub bytes: azul_css::U8Vec,
}

// FFI collection types. `CallbackInfo::get_hid_devices`/`get_hid_reports` hand
// these to bindings, and a borrowed `&[T]` is not C-compatible - a slice is a
// fat pointer whose layout C has no name for. Same treatment `TouchPointVec`
// and `MonitorVec` already get.
impl_option!(
    HidDevice,
    OptionHidDevice,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    HidDevice,
    HidDeviceVec,
    HidDeviceVecDestructor,
    HidDeviceVecDestructorType,
    HidDeviceVecSlice,
    OptionHidDevice
);
impl_vec_debug!(HidDevice, HidDeviceVec);
impl_vec_clone!(HidDevice, HidDeviceVec, HidDeviceVecDestructor);
impl_vec_partialeq!(HidDevice, HidDeviceVec);

impl_option!(
    HidReport,
    OptionHidReport,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);
impl_vec!(
    HidReport,
    HidReportVec,
    HidReportVecDestructor,
    HidReportVecDestructorType,
    HidReportVecSlice,
    OptionHidReport
);
impl_vec_debug!(HidReport, HidReportVec);
impl_vec_clone!(HidReport, HidReportVec, HidReportVecDestructor);
impl_vec_partialeq!(HidReport, HidReportVec);

/// Collects HID reports from the platform backends.
///
/// Poll-and-drain like the sensor and gamepad managers, for the same reason:
/// a device can report faster than the frame rate, and a callback per report
/// would swamp an app that only wants this frame's state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HidManager {
    devices: Vec<HidDevice>,
    pending: Vec<HidReport>,
}

impl HidManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the enumerated device list. Called by the backend at startup
    /// and on hotplug.
    pub fn set_devices(&mut self, devices: Vec<HidDevice>) {
        self.devices = devices;
    }

    /// The devices the platform found.
    #[must_use]
    pub fn devices(&self) -> &[HidDevice] {
        &self.devices
    }

    /// Queue an input report.
    pub fn push_report(&mut self, report: HidReport) {
        self.pending.push(report);
    }

    /// Read the queued reports without consuming them — what a callback does
    /// while the event is being dispatched.
    #[must_use]
    pub fn reports(&self) -> &[HidReport] {
        &self.pending
    }

    /// Drain the queue.
    pub fn take_reports(&mut self) -> Vec<HidReport> {
        core::mem::take(&mut self.pending)
    }
}
