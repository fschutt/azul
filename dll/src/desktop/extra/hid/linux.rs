//! Linux generic-HID backend via `/dev/hidraw*`.
//!
//! # Why hidraw and not libudev
//!
//! hidraw needs no library at all - it is `open`/`read`/`ioctl` on a character
//! device - so there is nothing to dlopen and nothing to link. libudev would
//! only add hotplug notification, and the enumeration below is cheap enough to
//! repeat.
//!
//! # Permissions are the real constraint
//!
//! `/dev/hidraw*` is root-only on a default install. Desktop distributions ship
//! udev rules for the devices they care about (game controllers, some HID
//! sensors) and nothing else, so an `EACCES` here is the NORMAL case for an
//! arbitrary device rather than a failure worth reporting loudly. Devices that
//! cannot be opened are skipped silently; the app sees a shorter list, which is
//! exactly what a user without the udev rule should see.
//!
//! # The ioctls
//!
//! Values are computed from the kernel's own `_IOC` encoding rather than
//! hardcoded, and the struct sizes they embed are from
//! `include/uapi/linux/hidraw.h`:
//!
//! ```c
//! struct hidraw_devinfo { __u32 bustype; __s16 vendor; __s16 product; };  // 8
//! struct hidraw_report_descriptor { __u32 size; __u8 value[4096]; };      // 4100
//! ```
//!
//! A wrong request number here does not fail loudly - `ioctl` returns EINVAL
//! and the device silently reports vendor 0 - so the encoding is spelled out
//! and unit-tested against the known-good constant for `HIDIOCGRAWINFO`.

use azul_core::hid::{HidDevice, HidReport};

/// `_IOC` direction bits (asm-generic; every architecture azul targets).
const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;
const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;

/// The kernel's `_IOC(dir, type, nr, size)`.
const fn ioc(dir: u32, ty: u32, nr: u32, size: u32) -> u32 {
    (dir << IOC_DIRSHIFT) | (ty << IOC_TYPESHIFT) | (nr << IOC_NRSHIFT) | (size << IOC_SIZESHIFT)
}

const HID_TYPE: u32 = b'H' as u32;
/// `HIDIOCGRAWINFO` = `_IOR('H', 0x03, struct hidraw_devinfo)`.
const HIDIOCGRAWINFO: u32 = ioc(IOC_READ, HID_TYPE, 0x03, 8);
/// `HIDIOCGRDESCSIZE` = `_IOR('H', 0x01, int)`.
const HIDIOCGRDESCSIZE: u32 = ioc(IOC_READ, HID_TYPE, 0x01, 4);
/// `HIDIOCGRDESC` = `_IOR('H', 0x02, struct hidraw_report_descriptor)`.
const HIDIOCGRDESC: u32 = ioc(IOC_READ, HID_TYPE, 0x02, 4 + HID_MAX_DESCRIPTOR_SIZE as u32);
/// `HIDIOCGRAWNAME(len)` = `_IOC(_IOC_READ, 'H', 0x04, len)`.
const fn hidiocgrawname(len: u32) -> u32 {
    ioc(IOC_READ, HID_TYPE, 0x04, len)
}

const HID_MAX_DESCRIPTOR_SIZE: usize = 4096;
const NAME_BUF: usize = 256;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct HidrawDevinfo {
    bustype: u32,
    vendor: i16,
    product: i16,
}

#[repr(C)]
struct HidrawReportDescriptor {
    size: u32,
    value: [u8; HID_MAX_DESCRIPTOR_SIZE],
}

/// The top-level usage page and usage, from the head of a report descriptor.
///
/// A full descriptor parse is a large job and is not needed: the FIRST Usage
/// Page (0x05) and Usage (0x09) items in the collection describe what the
/// device claims to be, which is the pair `HidDevice` carries and the pair an
/// app matches on ("any joystick" = page 0x01 usage 0x04).
fn top_level_usage(desc: &[u8]) -> (u16, u16) {
    let mut page = 0u16;
    let mut usage = 0u16;
    let mut i = 0usize;
    while i < desc.len() {
        let prefix = desc[i];
        // Long items (0xFE) carry their own size byte and never hold a usage.
        if prefix == 0xFE {
            if i + 1 >= desc.len() {
                break;
            }
            i += 3 + desc[i + 1] as usize;
            continue;
        }
        // Short item: bSize is the low 2 bits, where 3 MEANS 4 bytes.
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        let tag = prefix & 0xFC;
        let data_start = i + 1;
        if data_start + size > desc.len() {
            break;
        }
        let mut value = 0u32;
        for (k, b) in desc[data_start..data_start + size].iter().enumerate() {
            value |= u32::from(*b) << (8 * k);
        }
        match tag {
            // Usage Page (global, tag 0x04 | type 0x01 -> 0x05).
            0x04 => {
                if page == 0 {
                    page = value as u16;
                }
            }
            // Usage (local, tag 0x08 | type 0x02 -> 0x09).
            0x08 => {
                if usage == 0 {
                    usage = value as u16;
                }
            }
            _ => {}
        }
        if page != 0 && usage != 0 {
            break;
        }
        i = data_start + size;
    }
    (page, usage)
}

// ─── The device ───────────────────────────────────────────────────────

/// One opened `/dev/hidrawN`.
struct OpenDevice {
    fd: i32,
    info: HidDevice,
}

impl Drop for OpenDevice {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

/// Open one hidraw node and read its identity.
///
/// `None` on any failure, which includes the COMMON case of `EACCES` on a
/// device the running user has no udev rule for.
/// `HIDIOCGRAWUNIQ` = `_IOR('H', 0x08, len)`: the device's unique id - the
/// USB serial, a Bluetooth address - as a string. Kernel 5.13+; on an older
/// kernel the ioctl fails and the serial stays empty, which the instance id
/// falls back from.
const fn hidiocgrawuniq(len: u32) -> u32 {
    ioc(IOC_READ, HID_TYPE, 0x08, len)
}

/// `HIDIOCGFEATURE(len)`: read a feature report, `buf[0]` = the report id.
const fn hidiocgfeature(len: u32) -> u32 {
    ioc(IOC_WRITE | IOC_READ, HID_TYPE, 0x07, len)
}

/// A feature report of the open device with identity `instance`
/// (8f-i-a-i-b-i). hidraw does not need the fd writable for a GET.
#[must_use]
pub fn feature_report(instance: u64, report_id: u8, len: usize) -> Option<Vec<u8>> {
    if len == 0 || len > 0x3fff {
        return None;
    }
    OPEN.with(|slot| {
        let devices = slot.borrow();
        let dev = devices.iter().find(|d| d.info.instance == instance)?;
        let mut buf = vec![0u8; len];
        buf[0] = report_id;
        let n = unsafe {
            libc::ioctl(
                dev.fd,
                hidiocgfeature(len as u32) as libc::c_ulong,
                buf.as_mut_ptr(),
            )
        };
        if n < 0 {
            return None;
        }
        buf.truncate((n as usize).min(len));
        Some(buf)
    })
}

fn open_device(path: &std::path::Path) -> Option<OpenDevice> {
    use std::os::unix::ffi::OsStrExt;

    let mut c_path = Vec::with_capacity(path.as_os_str().as_bytes().len() + 1);
    c_path.extend_from_slice(path.as_os_str().as_bytes());
    c_path.push(0);

    // NONBLOCK is essential: a blocking read on a device that is not reporting
    // would park the polling thread forever, and the poll below is a sweep
    // across every device rather than a per-device thread.
    let fd = unsafe { libc::open(c_path.as_ptr().cast(), libc::O_RDONLY | libc::O_NONBLOCK) };
    if fd < 0 {
        return None;
    }
    let dev = OpenDevice {
        fd,
        info: HidDevice {
            vendor_id: 0,
            product_id: 0,
            usage_page: 0,
            usage: 0,
            name: azul_css::AzString::from_const_str(""),
            serial: azul_css::AzString::from_const_str(""),
            instance: 0,
        },
    };

    let mut devinfo = HidrawDevinfo::default();
    if unsafe {
        libc::ioctl(
            fd,
            HIDIOCGRAWINFO as libc::c_ulong,
            std::ptr::addr_of_mut!(devinfo),
        )
    } < 0
    {
        return None;
    }

    let mut name_buf = [0u8; NAME_BUF];
    let name = if unsafe {
        libc::ioctl(
            fd,
            hidiocgrawname(NAME_BUF as u32) as libc::c_ulong,
            name_buf.as_mut_ptr(),
        )
    } >= 0
    {
        let end = name_buf.iter().position(|b| *b == 0).unwrap_or(NAME_BUF);
        String::from_utf8_lossy(&name_buf[..end]).into_owned()
    } else {
        // A device with no name string is still a usable device.
        String::new()
    };

    // The report descriptor gives the usage page/usage pair. Its absence is
    // not fatal - the vid/pid still identify the model.
    let (usage_page, usage) = read_top_level_usage(fd).unwrap_or((0, 0));

    // The serial (8f-i-a-i): what tells two identical pads apart. Kernel
    // 5.13+ answers `HIDIOCGRAWUNIQ`; an older one fails the ioctl and the
    // instance id falls back to the hidraw path.
    let mut uniq_buf = [0u8; NAME_BUF];
    let serial = if unsafe {
        libc::ioctl(
            fd,
            hidiocgrawuniq(NAME_BUF as u32) as libc::c_ulong,
            uniq_buf.as_mut_ptr(),
        )
    } >= 0
    {
        let end = uniq_buf.iter().position(|b| *b == 0).unwrap_or(NAME_BUF);
        String::from_utf8_lossy(&uniq_buf[..end]).trim().to_owned()
    } else {
        String::new()
    };
    // The kernel types these as SIGNED 16-bit, but a USB id is an
    // unsigned 16-bit number: a vendor above 0x7FFF (Logitech's 0xC000
    // range, say) arrives negative and would print as a huge u16 if
    // widened rather than reinterpreted.
    let vendor_id = devinfo.vendor as u16;
    let product_id = devinfo.product as u16;
    let instance =
        HidDevice::instance_for(vendor_id, product_id, &serial, path.as_os_str().as_bytes());

    let mut dev = dev;
    dev.info = HidDevice {
        vendor_id,
        product_id,
        usage_page,
        usage,
        name: name.into(),
        serial: serial.into(),
        instance,
    };
    Some(dev)
}

fn read_top_level_usage(fd: i32) -> Option<(u16, u16)> {
    let mut size: i32 = 0;
    if unsafe {
        libc::ioctl(
            fd,
            HIDIOCGRDESCSIZE as libc::c_ulong,
            std::ptr::addr_of_mut!(size),
        )
    } < 0
        || size <= 0
    {
        return None;
    }
    // Boxed: the struct is 4100 bytes and a stack copy per device on a
    // small-stack thread is asking for trouble.
    let mut desc = Box::new(HidrawReportDescriptor {
        size: size as u32,
        value: [0u8; HID_MAX_DESCRIPTOR_SIZE],
    });
    if unsafe {
        libc::ioctl(
            fd,
            HIDIOCGRDESC as libc::c_ulong,
            std::ptr::addr_of_mut!(*desc),
        )
    } < 0
    {
        return None;
    }
    let len = (desc.size as usize).min(HID_MAX_DESCRIPTOR_SIZE);
    Some(top_level_usage(&desc.value[..len]))
}

// ─── Enumeration + polling ────────────────────────────────────────────

thread_local! {
    static OPEN: std::cell::RefCell<Vec<OpenDevice>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Re-enumerate `/dev/hidraw*` and publish the device list.
///
/// Called on start and on demand; hidraw offers no hotplug signal without
/// libudev, so re-enumeration is how a newly plugged device is noticed.
pub fn enumerate() {
    let mut devices = Vec::new();
    let mut open = Vec::new();
    let Ok(entries) = std::fs::read_dir("/dev") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("hidraw") {
            continue;
        }
        if let Some(dev) = open_device(&entry.path()) {
            devices.push(dev.info.clone());
            open.push(dev);
        }
    }
    OPEN.with(|slot| *slot.borrow_mut() = open);
    azul_layout::managers::hid::set_hid_devices(devices);
}

/// Read whatever every open device has queued, without blocking.
///
/// Called once per capability-pump pass. Every device is swept rather than
/// given a thread: HID reports are small and infrequent relative to a frame,
/// and a thread per device would be dozens of threads on a machine with a
/// keyboard, mouse, headset and controller attached.
pub fn poll() {
    OPEN.with(|slot| {
        let devices = slot.borrow();
        for dev in devices.iter() {
            // Bounded per device per pass: a device reporting faster than the
            // frame rate must not let one device starve the others, and the
            // channel drops the oldest anyway.
            for _ in 0..16 {
                let mut buf = [0u8; 64];
                let n = unsafe {
                    libc::read(dev.fd, buf.as_mut_ptr().cast(), buf.len())
                };
                if n <= 0 {
                    // EAGAIN on a non-blocking fd = nothing queued, which is
                    // the usual answer and not an error.
                    break;
                }
                let n = n as usize;
                azul_layout::managers::hid::push_hid_report(HidReport {
                    device: dev.info.clone(),
                    // hidraw prepends the report id ONLY on devices whose
                    // descriptor uses ids; the kernel does not tell us which,
                    // so the id is reported as 0 and the bytes are handed over
                    // exactly as they arrived. That matches the documented
                    // contract on `HidReport::bytes` ("bytes exactly as the
                    // device sent them") and is what WebHID does too.
                    report_id: 0,
                    bytes: buf[..n].to_vec().into(),
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wrong ioctl request number does NOT fail loudly - `ioctl` returns
    /// EINVAL and the device silently reports vendor 0 - so the encoding is
    /// pinned against the value the kernel headers produce.
    #[test]
    fn the_ioctl_encoding_matches_the_kernel() {
        // _IOR('H', 0x03, struct hidraw_devinfo /* 8 bytes */)
        assert_eq!(HIDIOCGRAWINFO, 0x8008_4803);
        // _IOR('H', 0x01, int)
        assert_eq!(HIDIOCGRDESCSIZE, 0x8004_4801);
        // _IOC(_IOC_READ, 'H', 0x04, 256)
        assert_eq!(hidiocgrawname(256), 0x8100_4804);
    }

    /// The pair an app matches on. Page 0x01 usage 0x04 is "joystick".
    #[test]
    fn the_top_level_usage_is_read_from_the_descriptor_head() {
        // Usage Page (Generic Desktop) 0x05 0x01, Usage (Joystick) 0x09 0x04
        let desc = [0x05, 0x01, 0x09, 0x04, 0xA1, 0x01];
        assert_eq!(top_level_usage(&desc), (0x01, 0x04));
    }

    /// A two-byte usage page (vendor-defined, 0xFF00) must not be truncated
    /// to its first byte.
    #[test]
    fn a_two_byte_usage_page_is_read_whole() {
        // Usage Page 0x06 (size 2) 0x00 0xFF, Usage 0x09 0x01
        let desc = [0x06, 0x00, 0xFF, 0x09, 0x01];
        assert_eq!(top_level_usage(&desc), (0xFF00, 0x01));
    }

    /// A truncated descriptor must not read past its end.
    #[test]
    fn a_truncated_descriptor_does_not_overrun() {
        // Claims a 4-byte payload with only 1 byte present.
        let desc = [0x07, 0x01];
        assert_eq!(top_level_usage(&desc), (0, 0));
    }

    /// A long item (0xFE) is skipped by its own length rather than parsed.
    #[test]
    fn a_long_item_is_skipped_by_its_declared_size() {
        let mut desc = vec![0xFE, 0x02, 0x00, 0xAA, 0xBB];
        desc.extend_from_slice(&[0x05, 0x01, 0x09, 0x02]);
        assert_eq!(top_level_usage(&desc), (0x01, 0x02));
    }

    #[test]
    fn an_empty_descriptor_is_zero_not_a_panic() {
        assert_eq!(top_level_usage(&[]), (0, 0));
    }
}
