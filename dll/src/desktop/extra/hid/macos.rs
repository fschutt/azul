//! macOS generic-HID backend via `IOHIDManager`.
//!
//! Everything is resolved AT RUNTIME with `dlopen`, like the ScreenCaptureKit
//! and CoreGraphics-TCC paths beside it: nothing here is a link-time
//! dependency, so a build that never touches HID pays nothing and a system
//! missing a symbol degrades instead of failing to launch.
//!
//! # Input Monitoring is the defining constraint
//!
//! `IOHIDManagerOpen` returns `kIOReturnNotPermitted` unless the user has
//! granted **Input Monitoring** in System Settings, and that permission gates
//! ALL HID access - not just keyboards, despite what its description says. So
//! on a machine where it has not been granted, this backend reports no devices
//! and no reports, which is correct behaviour rather than a failure.
//!
//! `IOHIDCheckAccess` (10.15+) is used to ask BEFORE opening, so the common
//! denied case is a quiet log rather than an error path. `IOHIDRequestAccess`
//! is still never called FROM HERE: this backend must not raise a privacy
//! dialog on its own initiative just because an app linked it. 9f-i-a-i gave
//! the app a way to ask instead - subscribing to
//! `Capability::InputMonitoring` calls `request_input_monitoring` below, which
//! is an explicit request rather than a side effect of enumerating devices.
//!
//! # Signatures
//!
//! Taken from the real SDK headers on the build machine
//! (`MacOSX.sdk/.../IOKit.framework/Headers/hid/`), not from memory:
//!
//! ```c
//! IOHIDManagerRef IOHIDManagerCreate(CFAllocatorRef, IOOptionBits);
//! void   IOHIDManagerSetDeviceMatching(IOHIDManagerRef, CFDictionaryRef);
//! IOReturn IOHIDManagerOpen(IOHIDManagerRef, IOOptionBits);
//! CFSetRef IOHIDManagerCopyDevices(IOHIDManagerRef);
//! void   IOHIDManagerRegisterInputReportCallback(IOHIDManagerRef, IOHIDReportCallback, void*);
//! void   IOHIDManagerScheduleWithRunLoop(IOHIDManagerRef, CFRunLoopRef, CFStringRef);
//! CFTypeRef IOHIDDeviceGetProperty(IOHIDDeviceRef, CFStringRef);
//! typedef void (*IOHIDReportCallback)(void* context, IOReturn result, void* sender,
//!                                     IOHIDReportType type, uint32_t reportID,
//!                                     uint8_t* report, CFIndex reportLength);
//! ```

use std::{ffi::c_void, sync::OnceLock};

use azul_core::hid::{HidDevice, HidReport};

type CFTypeRef = *const c_void;
type CFAllocatorRef = *const c_void;
type IOReturn = i32;
type IOOptionBits = u32;

/// `kIOHIDRequestTypeListenEvent` - the one that gates `IOHIDManagerOpen`.
const REQUEST_TYPE_LISTEN_EVENT: u32 = 1;
/// `kIOHIDAccessTypeGranted`.
const ACCESS_TYPE_GRANTED: u32 = 0;
/// `kIOHIDAccessTypeDenied`. `kIOHIDAccessTypeUnknown` is 2 and is the
/// fallthrough - a machine that has never been asked.
const ACCESS_TYPE_DENIED: u32 = 1;
const KERN_SUCCESS: IOReturn = 0;

struct IoKit {
    lib: libloading::Library,
    cf: libloading::Library,
}

static IOKIT: OnceLock<Option<IoKit>> = OnceLock::new();

fn iokit() -> Option<&'static IoKit> {
    IOKIT
        .get_or_init(|| {
            let lib = unsafe {
                libloading::Library::new("/System/Library/Frameworks/IOKit.framework/IOKit")
            }
            .ok()?;
            let cf = unsafe {
                libloading::Library::new(
                    "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation",
                )
            }
            .ok()?;
            Some(IoKit { lib, cf })
        })
        .as_ref()
}

/// `kIOHIDAccessType`, all three values - the permission layer needs the
/// tri-state, not just "granted".
///
/// `Unknown` is what a machine that has never been asked reports, and it is
/// distinct from `Denied`: an app may prompt for the first and must not for
/// the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMonitoringAccess {
    Granted,
    Denied,
    Unknown,
}

/// Ask the system whether Input Monitoring is granted, WITHOUT prompting.
///
/// `Granted` when `IOHIDCheckAccess` is missing (pre-10.15), where no TCC gate
/// existed - the same rule the screen-capture preflight beside this uses, so an
/// older system is never blocked by a check it cannot answer.
#[must_use]
pub fn input_monitoring_access() -> InputMonitoringAccess {
    let Some(k) = iokit() else {
        return InputMonitoringAccess::Unknown;
    };
    unsafe {
        let Ok(check) = k
            .lib
            .get::<unsafe extern "C" fn(u32) -> u32>(b"IOHIDCheckAccess\0")
        else {
            return InputMonitoringAccess::Granted;
        };
        match check(REQUEST_TYPE_LISTEN_EVENT) {
            ACCESS_TYPE_GRANTED => InputMonitoringAccess::Granted,
            ACCESS_TYPE_DENIED => InputMonitoringAccess::Denied,
            _ => InputMonitoringAccess::Unknown,
        }
    }
}

/// Raise the system's Input Monitoring prompt.
///
/// THE ONLY CALLER IS AN EXPLICIT `Capability::InputMonitoring` SUBSCRIBE, and
/// that is the whole point of 9f-i-a-i. This backend still never prompts on its
/// own initiative just because an app linked it - the app has to ask, exactly
/// as it does for Camera or Microphone.
///
/// `IOHIDRequestAccess` returns immediately; the user's answer arrives later
/// and the next `input_monitoring_access` poll picks it up. macOS shows the
/// prompt only once per app, so a second call on a denied system does nothing
/// visible - which is why the permission layer must not treat silence as a
/// failure.
pub fn request_input_monitoring() -> bool {
    let Some(k) = iokit() else {
        return false;
    };
    unsafe {
        let Ok(request) = k
            .lib
            .get::<unsafe extern "C" fn(u32) -> bool>(b"IOHIDRequestAccess\0")
        else {
            // Pre-10.15: nothing to request, because nothing gates it.
            return true;
        };
        request(REQUEST_TYPE_LISTEN_EVENT)
    }
}

/// Whether Input Monitoring is granted.
fn access_granted(k: &IoKit) -> bool {
    let _ = k;
    input_monitoring_access() != InputMonitoringAccess::Denied
}

/// Read an integer device property, or `0` when absent.
unsafe fn int_property(k: &IoKit, device: CFTypeRef, key: &[u8]) -> i64 {
    let Ok(get_property) =
        k.lib
            .get::<unsafe extern "C" fn(CFTypeRef, CFTypeRef) -> CFTypeRef>(
                b"IOHIDDeviceGetProperty\0",
            )
    else {
        return 0;
    };
    let Some(cfkey) = cfstring(k, key) else {
        return 0;
    };
    let value = get_property(device, cfkey);
    release(k, cfkey);
    if value.is_null() {
        return 0;
    }
    let Ok(number_get) = k
        .cf
        .get::<unsafe extern "C" fn(CFTypeRef, i64, *mut i64) -> bool>(b"CFNumberGetValue\0")
    else {
        return 0;
    };
    let mut out: i64 = 0;
    // kCFNumberSInt64Type = 4.
    if number_get(value, 4, &mut out) {
        out
    } else {
        0
    }
}

unsafe fn string_property(k: &IoKit, device: CFTypeRef, key: &[u8]) -> String {
    let Ok(get_property) =
        k.lib
            .get::<unsafe extern "C" fn(CFTypeRef, CFTypeRef) -> CFTypeRef>(
                b"IOHIDDeviceGetProperty\0",
            )
    else {
        return String::new();
    };
    let Some(cfkey) = cfstring(k, key) else {
        return String::new();
    };
    let value = get_property(device, cfkey);
    release(k, cfkey);
    if value.is_null() {
        return String::new();
    }
    let Ok(get_cstring) = k
        .cf
        .get::<unsafe extern "C" fn(CFTypeRef, *mut u8, isize, u32) -> bool>(
            b"CFStringGetCString\0",
        )
    else {
        return String::new();
    };
    let mut buf = [0u8; 256];
    // kCFStringEncodingUTF8 = 0x0800_0100.
    if get_cstring(value, buf.as_mut_ptr(), buf.len() as isize, 0x0800_0100) {
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).into_owned()
    } else {
        String::new()
    }
}

unsafe fn cfstring(k: &IoKit, bytes: &[u8]) -> Option<CFTypeRef> {
    let create = k
        .cf
        .get::<unsafe extern "C" fn(CFAllocatorRef, *const u8, u32) -> CFTypeRef>(
            b"CFStringCreateWithCString\0",
        )
        .ok()?;
    let mut owned = bytes.to_vec();
    if owned.last() != Some(&0) {
        owned.push(0);
    }
    let s = create(core::ptr::null(), owned.as_ptr(), 0x0800_0100);
    if s.is_null() {
        None
    } else {
        Some(s)
    }
}

unsafe fn release(k: &IoKit, obj: CFTypeRef) {
    if obj.is_null() {
        return;
    }
    if let Ok(f) = k.cf.get::<unsafe extern "C" fn(CFTypeRef)>(b"CFRelease\0") {
        f(obj);
    }
}

// ─── The manager ──────────────────────────────────────────────────────

/// The live `IOHIDManagerRef`, kept resident so its run-loop source stays
/// scheduled. Raw pointer behind a lock: `IOHIDManagerRef` is a CF object,
/// not `Send`, but it is only ever touched from the main thread - the
/// capability pump - and the lock exists to satisfy the static, not to make
/// concurrent use safe.
static MANAGER: std::sync::Mutex<usize> = std::sync::Mutex::new(0);

/// The devices whose identity we resolved, keyed by `IOHIDDeviceRef`, so the
/// report callback can name the sender without re-reading its properties on
/// every report (a CF round trip per report at up to 1000 Hz).
static DEVICE_BY_REF: std::sync::Mutex<Vec<(usize, HidDevice)>> =
    std::sync::Mutex::new(Vec::new());

/// `IOHIDReportCallback`. Runs on the run loop, i.e. the main thread, so it
/// only parks into the channel - the pump folds it in later.
extern "C" fn input_report_callback(
    _context: *mut c_void,
    result: IOReturn,
    sender: *mut c_void,
    _report_type: u32,
    report_id: u32,
    report: *mut u8,
    report_length: isize,
) {
    if result != KERN_SUCCESS || report.is_null() || report_length <= 0 {
        return;
    }
    let bytes = unsafe { core::slice::from_raw_parts(report, report_length as usize) }.to_vec();
    let device = {
        let map = DEVICE_BY_REF
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.iter()
            .find(|(r, _)| *r == sender as usize)
            .map(|(_, d)| d.clone())
    };
    // A report from a device we never resolved is still a report; naming it
    // with an empty device is more useful than dropping the bytes.
    let device = device.unwrap_or(HidDevice {
        vendor_id: 0,
        product_id: 0,
        usage_page: 0,
        usage: 0,
        name: azul_css::AzString::from_const_str(""),
    });
    azul_layout::managers::hid::push_hid_report(HidReport {
        device,
        report_id: u8::try_from(report_id).unwrap_or(0),
        bytes: bytes.into(),
    });
}

/// Create the manager, open it, enumerate, and schedule the report callback.
///
/// Idempotent: a second call is a no-op while a manager is alive.
pub fn enumerate() {
    let Some(k) = iokit() else {
        return;
    };
    let mut slot = MANAGER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *slot != 0 {
        return;
    }

    if !access_granted(k) {
        // The NORMAL outcome on a machine where the user has not granted
        // Input Monitoring. Published as an empty list so `get_hid_devices()`
        // answers definitively rather than looking like it never ran.
        crate::plog_info!(
            "[hid] Input Monitoring not granted - no HID devices will be reported. Grant it in \
             System Settings > Privacy & Security > Input Monitoring."
        );
        azul_layout::managers::hid::set_hid_devices(Vec::new());
        return;
    }

    unsafe {
        let Ok(create) = k
            .lib
            .get::<unsafe extern "C" fn(CFAllocatorRef, IOOptionBits) -> CFTypeRef>(
                b"IOHIDManagerCreate\0",
            )
        else {
            return;
        };
        let manager = create(core::ptr::null(), 0);
        if manager.is_null() {
            return;
        }

        // NULL matching dictionary = every HID device. Narrowing it here would
        // defeat the point: this backend exists for the devices azul does NOT
        // model, so it cannot know what to match on.
        if let Ok(set_matching) =
            k.lib
                .get::<unsafe extern "C" fn(CFTypeRef, CFTypeRef)>(b"IOHIDManagerSetDeviceMatching\0")
        {
            set_matching(manager, core::ptr::null());
        }

        let Ok(open) = k
            .lib
            .get::<unsafe extern "C" fn(CFTypeRef, IOOptionBits) -> IOReturn>(b"IOHIDManagerOpen\0")
        else {
            release(k, manager);
            return;
        };
        if open(manager, 0) != KERN_SUCCESS {
            // kIOReturnNotPermitted, usually - the TCC check above can pass
            // and the open still fail if the grant was revoked in between.
            crate::plog_info!("[hid] IOHIDManagerOpen refused - Input Monitoring likely denied");
            release(k, manager);
            azul_layout::managers::hid::set_hid_devices(Vec::new());
            return;
        }

        // Identity for every device, resolved ONCE. The report callback looks
        // the sender up in this map rather than re-reading CF properties at up
        // to 1000 Hz.
        let mut devices = Vec::new();
        let mut by_ref = Vec::new();
        if let (Ok(copy_devices), Ok(set_get_count), Ok(set_get_values)) = (
            k.lib
                .get::<unsafe extern "C" fn(CFTypeRef) -> CFTypeRef>(b"IOHIDManagerCopyDevices\0"),
            k.cf
                .get::<unsafe extern "C" fn(CFTypeRef) -> isize>(b"CFSetGetCount\0"),
            k.cf
                .get::<unsafe extern "C" fn(CFTypeRef, *mut CFTypeRef)>(b"CFSetGetValues\0"),
        ) {
            let set = copy_devices(manager);
            if !set.is_null() {
                let count = set_get_count(set);
                if count > 0 {
                    let mut refs: Vec<CFTypeRef> = vec![core::ptr::null(); count as usize];
                    set_get_values(set, refs.as_mut_ptr());
                    for dref in refs {
                        if dref.is_null() {
                            continue;
                        }
                        let dev = HidDevice {
                            vendor_id: int_property(k, dref, b"VendorID") as u16,
                            product_id: int_property(k, dref, b"ProductID") as u16,
                            usage_page: int_property(k, dref, b"PrimaryUsagePage") as u16,
                            usage: int_property(k, dref, b"PrimaryUsage") as u16,
                            name: string_property(k, dref, b"Product").into(),
                        };
                        by_ref.push((dref as usize, dev.clone()));
                        devices.push(dev);
                    }
                }
                release(k, set);
            }
        }
        {
            let mut map = DEVICE_BY_REF
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *map = by_ref;
        }
        azul_layout::managers::hid::set_hid_devices(devices);

        // Reports arrive through the RUN LOOP, which is why there is no
        // `poll()` on this platform: the callback fires on the main thread
        // between frames and parks into the same channel the Linux sweep uses.
        if let (Ok(register), Ok(schedule), Ok(current_loop), Ok(default_mode)) = (
            k.lib.get::<unsafe extern "C" fn(
                CFTypeRef,
                extern "C" fn(*mut c_void, IOReturn, *mut c_void, u32, u32, *mut u8, isize),
                *mut c_void,
            )>(b"IOHIDManagerRegisterInputReportCallback\0"),
            k.lib
                .get::<unsafe extern "C" fn(CFTypeRef, CFTypeRef, CFTypeRef)>(
                    b"IOHIDManagerScheduleWithRunLoop\0",
                ),
            k.cf
                .get::<unsafe extern "C" fn() -> CFTypeRef>(b"CFRunLoopGetCurrent\0"),
            k.cf
                .get::<*const CFTypeRef>(b"kCFRunLoopDefaultMode\0"),
        ) {
            register(manager, input_report_callback, core::ptr::null_mut());
            schedule(manager, current_loop(), **default_mode);
        }

        *slot = manager as usize;
        crate::plog_info!("[hid] IOHIDManager scheduled");
    }
}

/// No-op on macOS: reports are delivered by the run loop, not polled.
///
/// The Linux backend sweeps its file descriptors here because hidraw has no
/// callback; IOKit does, so a sweep would have nothing to read.
pub fn poll() {}
