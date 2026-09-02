//! Windows generic-HID backend, over the raw-input stream 9d-i already built.
//!
//! # What was already here
//!
//! 9d-i wired `RegisterRawInputDevices`, the `WM_INPUT` window-proc arm and
//! `GetRawInputData` for relative MOUSE motion. This adds the HID usage
//! registration and the `RIM_TYPEHID` decode beside it, so no new message
//! plumbing was needed - only a second registration and a second arm.
//!
//! # hid.dll is NOT needed, contrary to the item
//!
//! The 9f-i note said `HidDevice { vendor_id, product_id }` would need
//! `HidD_GetAttributes` from hid.dll, "a library this codebase does not load at
//! all". It does not: `GetRawInputDeviceInfoW(RIDI_DEVICEINFO)` fills a
//! `RID_DEVICE_INFO_HID` with the vendor id, product id, usage page AND usage -
//! every field `HidDevice` carries except the name. Checked against the SDK
//! documentation before writing this, so no extra library is loaded.
//!
//! # The variable-length report is the real difference from the mouse arm
//!
//! `RIM_TYPEMOUSE` fits a fixed struct. `RIM_TYPEHID` carries `dwSizeHid *
//! dwCount` trailing bytes, so the buffer has to be sized from a zero-length
//! `GetRawInputData` probe first. Reading it into a fixed struct would truncate
//! every report from any device with more than a few bytes of state.

use azul_core::hid::{HidDevice, HidReport};

use crate::desktop::shell2::windows::dlopen::{
    self, RAWINPUTDEVICELIST, RAWINPUTHEADER, RAWINPUTHID, RID_DEVICE_INFO, RIDI_DEVICEINFO,
    RIDI_DEVICENAME, RID_INPUT, RIM_TYPEHID,
};

/// Cache of device identity, keyed by the `hDevice` handle that arrives in
/// every `RAWINPUTHEADER`.
///
/// Resolved once per device: `GetRawInputDeviceInfoW` is a kernel round trip
/// and a HID device can report at 1000 Hz, so doing it per report would spend
/// more time asking who sent the bytes than handling them.
static DEVICE_BY_HANDLE: std::sync::Mutex<Vec<(isize, HidDevice)>> =
    std::sync::Mutex::new(Vec::new());

/// Ask the OS what a device is.
fn describe(user32: &dlopen::User32Functions, handle: isize) -> Option<HidDevice> {
    unsafe {
        let mut info = RID_DEVICE_INFO {
            // The OS VALIDATES this; a wrong value fails the call with no
            // diagnostic, which is what the compile-time size assertion in
            // dlopen.rs guards.
            cbSize: core::mem::size_of::<RID_DEVICE_INFO>() as u32,
            ..Default::default()
        };
        let mut size = info.cbSize;
        let rc = (user32.GetRawInputDeviceInfoW)(
            handle,
            RIDI_DEVICEINFO,
            core::ptr::addr_of_mut!(info).cast(),
            &mut size,
        );
        if rc == u32::MAX || rc == 0 {
            return None;
        }
        if info.dwType != RIM_TYPEHID {
            // Mice and keyboards also appear in the device list; they are
            // handled by their own arms and are not "generic HID".
            return None;
        }

        // The interface path is the only name raw input offers. It is not a
        // product string - that WOULD need hid.dll - but it is stable, unique
        // and identifies the device to a user reading a log.
        let mut chars: u32 = 0;
        (user32.GetRawInputDeviceInfoW)(
            handle,
            RIDI_DEVICENAME,
            core::ptr::null_mut(),
            &mut chars,
        );
        let name = if chars > 0 && chars < 4096 {
            let mut buf = vec![0u16; chars as usize];
            // NOTE: for RIDI_DEVICENAME the size is a CHARACTER count, not a
            // byte count - the one command where that is true, and passing
            // bytes here would overrun or truncate.
            let mut n = chars;
            let rc = (user32.GetRawInputDeviceInfoW)(
                handle,
                RIDI_DEVICENAME,
                buf.as_mut_ptr().cast(),
                &mut n,
            );
            if rc == u32::MAX {
                String::new()
            } else {
                let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
                String::from_utf16_lossy(&buf[..end])
            }
        } else {
            String::new()
        };

        Some(HidDevice {
            // The OS reports these as DWORDs but a USB id is 16-bit; the high
            // half is always zero and truncating is the correct narrowing.
            vendor_id: info.hid.dwVendorId as u16,
            product_id: info.hid.dwProductId as u16,
            usage_page: info.hid.usUsagePage,
            usage: info.hid.usUsage,
            name: name.into(),
        })
    }
}

/// Enumerate every raw-input device and publish the HID ones.
pub fn enumerate(user32: &dlopen::User32Functions) {
    unsafe {
        let mut count: u32 = 0;
        let entry_size = core::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
        // A null buffer asks "how many?" - the documented probe.
        if (user32.GetRawInputDeviceList)(core::ptr::null_mut(), &mut count, entry_size)
            == u32::MAX
        {
            return;
        }
        if count == 0 {
            azul_layout::managers::hid::set_hid_devices(Vec::new());
            return;
        }
        let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize];
        let got = (user32.GetRawInputDeviceList)(list.as_mut_ptr(), &mut count, entry_size);
        if got == u32::MAX {
            return;
        }
        list.truncate(got as usize);

        let mut devices = Vec::new();
        let mut by_handle = Vec::new();
        for entry in list {
            if entry.dwType != RIM_TYPEHID {
                continue;
            }
            if let Some(dev) = describe(user32, entry.hDevice) {
                by_handle.push((entry.hDevice, dev.clone()));
                devices.push(dev);
            }
        }
        {
            let mut map = DEVICE_BY_HANDLE
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *map = by_handle;
        }
        azul_layout::managers::hid::set_hid_devices(devices);
    }
}

/// Decode one `WM_INPUT` whose type is `RIM_TYPEHID` and park its reports.
///
/// Called from the existing `WM_INPUT` arm after the mouse case declines it.
pub fn handle_wm_input(user32: &dlopen::User32Functions, lparam: isize) {
    unsafe {
        // SIZE PROBE FIRST. Unlike the mouse arm, the payload length is not
        // known up front: `dwSizeHid * dwCount` bytes trail the struct, so a
        // fixed-size read would truncate every report from a device with more
        // than a few bytes of state.
        let mut size: u32 = 0;
        let header_size = core::mem::size_of::<RAWINPUTHEADER>() as u32;
        if (user32.GetRawInputData)(
            lparam,
            RID_INPUT,
            core::ptr::null_mut(),
            &mut size,
            header_size,
        ) == u32::MAX
            || size == 0
        {
            return;
        }
        // Guard against a bogus size before allocating from it.
        if size as usize > 64 * 1024 {
            return;
        }
        let mut buf = vec![0u8; size as usize];
        let got = (user32.GetRawInputData)(
            lparam,
            RID_INPUT,
            buf.as_mut_ptr().cast(),
            &mut size,
            header_size,
        );
        if got == u32::MAX || (got as usize) < core::mem::size_of::<RAWINPUTHID>() {
            return;
        }

        let raw = &*(buf.as_ptr() as *const RAWINPUTHID);
        if raw.header.dwType != RIM_TYPEHID {
            return;
        }
        let size_hid = raw.hid.dwSizeHid as usize;
        let count = raw.hid.dwCount as usize;
        if size_hid == 0 || count == 0 {
            return;
        }

        let device = {
            let map = DEVICE_BY_HANDLE
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            map.iter()
                .find(|(h, _)| *h == raw.header.hDevice)
                .map(|(_, d)| d.clone())
        };
        let device = device.unwrap_or(HidDevice {
            vendor_id: 0,
            product_id: 0,
            usage_page: 0,
            usage: 0,
            name: azul_css::AzString::from_const_str(""),
        });

        // `bRawData` is a flexible array: the reports start at its offset, and
        // there are `dwCount` of them back to back, each `dwSizeHid` long.
        // Treating the payload as ONE report would merge a coalesced batch
        // into a single nonsense report.
        let data_offset =
            core::mem::size_of::<RAWINPUTHEADER>() + core::mem::size_of::<u32>() * 2;
        for i in 0..count {
            let start = data_offset + i * size_hid;
            let end = start + size_hid;
            if end > buf.len() {
                break;
            }
            azul_layout::managers::hid::push_hid_report(HidReport {
                device: device.clone(),
                // Windows keeps the report id as the FIRST byte for devices
                // whose descriptor uses ids, and does not say which do - the
                // same ambiguity hidraw has, resolved the same way: report 0
                // and hand over the bytes exactly as they arrived.
                report_id: 0,
                bytes: buf[start..end].to_vec().into(),
            });
        }
    }
}
