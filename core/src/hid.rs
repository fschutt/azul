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
    /// The device's SERIAL NUMBER as it reports it (USB `iSerial`; a
    /// DualSense reports its Bluetooth address), empty when it reports none
    /// or the platform does not expose it (Windows, see the backend).
    pub serial: AzString,
    /// PER-INSTANCE identity (8f-i-a-i, user ruling: two identical pads must
    /// stay distinct for multiplayer). Stable for the device's connected
    /// lifetime and never `0` for a real device. Derived from vendor,
    /// product and serial when a serial is reported - so it survives a
    /// reconnect - and from the platform's own handle for the device
    /// otherwise, which is unique among the devices present but may change
    /// when the device is re-plugged. Reports carry it, so an app keyed on
    /// this field reads each pad's stream apart from its twin's.
    pub instance: u64,
}

impl HidDevice {
    /// The reconnect-stable instance id of a device with a serial number:
    /// an FNV-1a hash over vendor, product and serial. `None` when the
    /// device reports no serial - the caller falls back to its platform
    /// handle through [`Self::handle_instance`].
    #[must_use]
    pub fn serial_instance(vendor_id: u16, product_id: u16, serial: &str) -> Option<u64> {
        if serial.is_empty() {
            return None;
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in vendor_id
            .to_le_bytes()
            .iter()
            .chain(product_id.to_le_bytes().iter())
            .chain(serial.as_bytes())
        {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        Some(h | 1)
    }

    /// An instance id from a platform handle (a device path, a kernel
    /// object address, a hidraw number) for a device with no serial. Never
    /// `0`, so "no identity" stays distinguishable from a real one.
    #[must_use]
    pub fn handle_instance(handle: &[u8]) -> u64 {
        let mut h: u64 = 0x84222325_cbf29ce4;
        for b in handle {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h | 1
    }

    /// The instance id for this device's identity: the serial-derived one
    /// when a serial is present, else the handle-derived one.
    #[must_use]
    pub fn instance_for(vendor_id: u16, product_id: u16, serial: &str, handle: &[u8]) -> u64 {
        Self::serial_instance(vendor_id, product_id, serial)
            .unwrap_or_else(|| Self::handle_instance(handle))
    }
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

#[cfg(test)]
mod instance_tests {
    use super::*;

    /// Two identical pads (same vendor and product) with different serials
    /// get different, reconnect-stable ids; the same pad gets the same id
    /// every time.
    #[test]
    fn identical_pads_with_different_serials_stay_distinct() {
        let a = HidDevice::serial_instance(0x054c, 0x0ce6, "AA:BB:CC:DD:EE:01").unwrap();
        let b = HidDevice::serial_instance(0x054c, 0x0ce6, "AA:BB:CC:DD:EE:02").unwrap();
        assert_ne!(a, b);
        assert_eq!(
            a,
            HidDevice::serial_instance(0x054c, 0x0ce6, "AA:BB:CC:DD:EE:01").unwrap(),
            "stable across reconnects"
        );
        assert_ne!(a, 0);
    }

    /// No serial: the platform handle decides, and it is never zero.
    #[test]
    fn a_device_without_a_serial_falls_back_to_its_handle() {
        assert_eq!(HidDevice::serial_instance(1, 2, ""), None);
        let x = HidDevice::instance_for(1, 2, "", b"/dev/hidraw3");
        let y = HidDevice::instance_for(1, 2, "", b"/dev/hidraw4");
        assert_ne!(x, y);
        assert_ne!(x, 0);
        assert_eq!(x, HidDevice::handle_instance(b"/dev/hidraw3"));
        // A serial outranks the handle.
        assert_eq!(
            HidDevice::instance_for(1, 2, "S1", b"/dev/hidraw3"),
            HidDevice::serial_instance(1, 2, "S1").unwrap()
        );
    }
}
