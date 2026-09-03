//! Generic-HID plumbing: the channel a platform backend parks reports in.
//!
//! - The **platform backend** (`dll/src/desktop/extra/hid/<plat>.rs`) opens the
//!   raw HID devices and calls [`push_hid_report`] / [`set_hid_devices`].
//! - The dll **capability pump** drains both via [`drain_hid_reports`] and
//!   [`take_hid_devices`] and folds them into `HidManager`.
//! - **Callbacks** read them through `CallbackInfo::get_hid_reports()` /
//!   `get_hid_devices()`.
//!
//! `HidManager` itself lives in azul-core and is pure data; this is only the
//! cross-thread channel, mirroring `sensors.rs` verbatim so the two behave the
//! same way under a poisoned lock.
//!
//! # Why reports are a QUEUE and devices are a SNAPSHOT
//!
//! Every report matters - they are a stream of state changes from a device the
//! engine does not model, and dropping one loses an axis movement or a button
//! press outright. The device LIST is the opposite: only the newest matters,
//! and a backend that re-enumerates on hotplug would otherwise pile up
//! successively-stale copies.

use azul_core::hid::{HidDevice, HidReport};

static PENDING_REPORTS: std::sync::Mutex<Vec<HidReport>> = std::sync::Mutex::new(Vec::new());
static DEVICES: std::sync::Mutex<Option<Vec<HidDevice>>> = std::sync::Mutex::new(None);

/// Cap on the parked queue.
///
/// A HID device can report faster than the frame rate - a 1000 Hz gaming mouse
/// or a wheel with force feedback easily does - and if no callback is draining
/// (no app subscribed, or the window is not pumping) the queue would grow
/// without bound for the life of the process. Oldest are dropped, because the
/// newest reports are the ones that still describe the device's state.
const MAX_PENDING_REPORTS: usize = 4096;

/// Park a report delivered by a platform backend. Thread-safe;
/// poison-recovering, so a panicking backend thread cannot wedge input.
pub fn push_hid_report(report: HidReport) {
    let mut q = PENDING_REPORTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if q.len() >= MAX_PENDING_REPORTS {
        // Drop from the FRONT: a bounded queue that dropped the newest would
        // freeze the device's apparent state at the moment it overflowed.
        let overflow = q.len() + 1 - MAX_PENDING_REPORTS;
        q.drain(..overflow);
    }
    q.push(report);
}

/// Drain every parked report, in arrival order.
pub fn drain_hid_reports() -> Vec<HidReport> {
    let mut q = PENDING_REPORTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

/// Publish the enumerated device list, replacing any previous one.
pub fn set_hid_devices(devices: Vec<HidDevice>) {
    let mut d = DEVICES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *d = Some(devices);
}

/// Take the device list if a backend published a new one.
///
/// `None` means "unchanged", NOT "no devices" - the caller must leave the
/// manager's existing list alone, or every pass with no re-enumeration would
/// clear it.
pub fn take_hid_devices() -> Option<Vec<HidDevice>> {
    let mut d = DEVICES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    d.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(id: u8) -> HidReport {
        HidReport {
            device: HidDevice {
                vendor_id: 1,
                product_id: 2,
                usage_page: 1,
                usage: 4,
                name: "test".into(),
                serial: "".into(),
                instance: 1,
            },
            report_id: id,
            bytes: Vec::new().into(),
        }
    }

    #[test]
    fn reports_drain_in_arrival_order() {
        let _ = drain_hid_reports();
        push_hid_report(report(1));
        push_hid_report(report(2));
        let got = drain_hid_reports();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].report_id, 1);
        assert_eq!(got[1].report_id, 2);
        assert!(drain_hid_reports().is_empty(), "drain must empty the queue");
    }

    /// A 1000 Hz device with nothing draining would otherwise grow the queue
    /// for the life of the process.
    #[test]
    fn the_queue_is_bounded_and_drops_the_oldest() {
        let _ = drain_hid_reports();
        for i in 0..(MAX_PENDING_REPORTS + 10) {
            push_hid_report(report((i % 251) as u8));
        }
        let got = drain_hid_reports();
        assert_eq!(
            got.len(),
            MAX_PENDING_REPORTS,
            "the queue must stay bounded"
        );
        // The SURVIVORS must be the newest: dropping the newest instead would
        // freeze the device's apparent state at the overflow point.
        let expected_first = (10 % 251) as u8;
        assert_eq!(got[0].report_id, expected_first);
    }

    /// `None` means UNCHANGED, not empty - a caller that treated it as empty
    /// would clear the device list on every pass that did not re-enumerate.
    #[test]
    fn taking_devices_twice_reports_unchanged_the_second_time() {
        set_hid_devices(vec![report(0).device]);
        assert_eq!(take_hid_devices().map(|d| d.len()), Some(1));
        assert!(
            take_hid_devices().is_none(),
            "a second take must say UNCHANGED, not empty"
        );
    }

    #[test]
    fn an_empty_device_list_is_distinguishable_from_unchanged() {
        let _ = take_hid_devices();
        set_hid_devices(Vec::new());
        assert_eq!(
            take_hid_devices().map(|d| d.len()),
            Some(0),
            "publishing zero devices must be observable"
        );
    }
}
