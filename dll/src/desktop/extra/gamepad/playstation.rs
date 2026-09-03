//! DualSense / DualShock 4 INPUT-REPORT parser (8f-i-a-i-b): the pad's IMU
//! and touch surface on desktop, read off the raw HID stream that 9f-i
//! landed and keyed by the per-instance identity 8f-i-a-i gave every
//! device - so two identical pads stay two pads.
//!
//! The layouts are the public ones the Linux `hid-playstation` driver
//! documents (checked against the kernel source before this was written):
//!
//! * DualSense: USB report `0x01`, 64 bytes, the input struct at offset 1;
//!   Bluetooth report `0x31`, 78 bytes, the same struct at offset 2 and a
//!   CRC32 over everything before the last four bytes, seeded with `0xA1`.
//!   Gyro is `raw / 1024` deg/s, accel `raw / 8192` g, the touch surface
//!   1920 x 1080.
//! * DualShock 4: USB report `0x01` (common struct at offset 1), Bluetooth
//!   `0x11` (offset 3, CRC-tailed the same way). Same sensor resolutions,
//!   touch surface 1920 x 942.
//!
//! Pure functions over bytes, so every branch is unit-tested with synthetic
//! reports; the platform-independent publish step lives in `mod.rs`.
//!
//! What is NOT applied: the per-pad calibration the kernel reads from
//! feature report `0x05` (bias and per-axis sensitivity). The nominal
//! resolutions are what every user-space reader without that report uses;
//! logged as 8f-i-a-i-b-i.

use azul_core::gamepad::GamepadButton;
use azul_core::hid::HidDevice;

pub const SONY_VENDOR: u16 = 0x054c;
pub const DUALSENSE: u16 = 0x0ce6;
pub const DUALSENSE_EDGE: u16 = 0x0df2;
pub const DUALSHOCK4_V1: u16 = 0x05c4;
pub const DUALSHOCK4_V2: u16 = 0x09cc;
pub const DUALSHOCK4_DONGLE: u16 = 0x0ba0;

const DS_INPUT_REPORT_USB: u8 = 0x01;
const DS_INPUT_REPORT_BT: u8 = 0x31;
const DS_INPUT_REPORT_USB_SIZE: usize = 64;
const DS_INPUT_REPORT_BT_SIZE: usize = 78;
const DS4_INPUT_REPORT_USB: u8 = 0x01;
const DS4_INPUT_REPORT_BT: u8 = 0x11;
const DS4_INPUT_REPORT_USB_SIZE: usize = 64;
const DS4_INPUT_REPORT_BT_SIZE: usize = 78;
const GYRO_RES_PER_DEG_S: f32 = 1024.0;
const ACC_RES_PER_G: f32 = 8192.0;
const DS_TOUCHPAD_WIDTH: f32 = 1920.0;
const DS_TOUCHPAD_HEIGHT: f32 = 1080.0;
const DS4_TOUCHPAD_WIDTH: f32 = 1920.0;
const DS4_TOUCHPAD_HEIGHT: f32 = 942.0;
const INPUT_CRC32_SEED: u8 = 0xa1;
const G_TO_MS2: f32 = 9.80665;

/// Which PlayStation pad a HID device is, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayStationPad {
    DualSense,
    DualShock4,
}

impl PlayStationPad {
    #[must_use]
    pub fn of(device: &HidDevice) -> Option<Self> {
        if device.vendor_id != SONY_VENDOR {
            return None;
        }
        match device.product_id {
            DUALSENSE | DUALSENSE_EDGE => Some(Self::DualSense),
            DUALSHOCK4_V1 | DUALSHOCK4_V2 | DUALSHOCK4_DONGLE => Some(Self::DualShock4),
            _ => None,
        }
    }
}

/// One decoded input report, in `GamepadState`'s units and conventions:
/// sticks `-1..1` with y UP, triggers `0..1`, gyro rad/s, accel m/s²,
/// touch normalized `0..1` with the origin BOTTOM-LEFT.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadSample {
    pub buttons: u32,
    pub left_stick: (f32, f32),
    pub right_stick: (f32, f32),
    pub left_trigger: f32,
    pub right_trigger: f32,
    pub gyro: [f32; 3],
    pub accel: [f32; 3],
    /// The first finger on the touch surface, if any.
    pub touch: Option<(f32, f32)>,
}

/// Decode one raw input report from a PlayStation pad. `bytes` is the report
/// exactly as the platform handed it, report id in `bytes[0]` (hidraw,
/// IOKit and raw input all include it for a device that uses ids). `None`
/// for a report that is not an input report, is the wrong size, or fails
/// its Bluetooth CRC.
#[must_use]
pub fn parse(pad: PlayStationPad, bytes: &[u8]) -> Option<PadSample> {
    let id = *bytes.first()?;
    match pad {
        PlayStationPad::DualSense => match (id, bytes.len()) {
            (DS_INPUT_REPORT_USB, DS_INPUT_REPORT_USB_SIZE) => parse_dualsense(&bytes[1..]),
            (DS_INPUT_REPORT_BT, DS_INPUT_REPORT_BT_SIZE) => {
                if !crc_ok(bytes) {
                    return None;
                }
                parse_dualsense(&bytes[2..])
            }
            _ => None,
        },
        PlayStationPad::DualShock4 => match (id, bytes.len()) {
            (DS4_INPUT_REPORT_USB, DS4_INPUT_REPORT_USB_SIZE) => parse_dualshock4(&bytes[1..]),
            (DS4_INPUT_REPORT_BT, DS4_INPUT_REPORT_BT_SIZE) => {
                if !crc_ok(bytes) {
                    return None;
                }
                parse_dualshock4(&bytes[3..])
            }
            _ => None,
        },
    }
}

/// `struct dualsense_input_report`, starting at `p[0]` (= `x`).
fn parse_dualsense(p: &[u8]) -> Option<PadSample> {
    if p.len() < 40 {
        return None;
    }
    // x y rx ry z rz seq buttons[4] reserved[4] gyro[3] accel[3] ts[4]
    // reserved2 points[2]
    let buttons = ps_buttons(p[7], p[8], p[9], true);
    let gyro = [i16_at(p, 15), i16_at(p, 17), i16_at(p, 19)];
    let accel = [i16_at(p, 21), i16_at(p, 23), i16_at(p, 25)];
    let touch = touch_point(&p[32..36], DS_TOUCHPAD_WIDTH, DS_TOUCHPAD_HEIGHT);
    Some(PadSample {
        buttons,
        left_stick: stick(p[0], p[1]),
        right_stick: stick(p[2], p[3]),
        left_trigger: trigger(p[4]),
        right_trigger: trigger(p[5]),
        gyro: gyro_rad_s(gyro),
        accel: accel_ms2(accel),
        touch,
    })
}

/// `struct dualshock4_input_report_common` (32 bytes) starting at `p[0]`,
/// followed by `num_touch_reports` and the touch reports.
fn parse_dualshock4(p: &[u8]) -> Option<PadSample> {
    if p.len() < 42 {
        return None;
    }
    // x y rx ry buttons[3] z rz ts[2] temp gyro[3] accel[3] reserved2[5]
    // status[2] reserved3 | num_touch_reports | { timestamp, points[2] } ...
    let buttons = ps_buttons(p[4], p[5], p[6], false);
    let gyro = [i16_at(p, 12), i16_at(p, 14), i16_at(p, 16)];
    let accel = [i16_at(p, 18), i16_at(p, 20), i16_at(p, 22)];
    let num_touch_reports = p[32];
    let touch = if num_touch_reports > 0 {
        touch_point(&p[34..38], DS4_TOUCHPAD_WIDTH, DS4_TOUCHPAD_HEIGHT)
    } else {
        None
    };
    Some(PadSample {
        buttons,
        left_stick: stick(p[0], p[1]),
        right_stick: stick(p[2], p[3]),
        left_trigger: trigger(p[7]),
        right_trigger: trigger(p[8]),
        gyro: gyro_rad_s(gyro),
        accel: accel_ms2(accel),
        touch,
    })
}

/// The three button bytes both pads share: `b0` = hat (low nibble) +
/// square / cross / circle / triangle, `b1` = L1 R1 L2 R2 share options L3
/// R3, `b2` = PS, touchpad click, and (DualSense only) mute.
fn ps_buttons(b0: u8, b1: u8, b2: u8, dualsense: bool) -> u32 {
    use GamepadButton as B;
    let mut out = 0u32;
    let mut set = |on: bool, b: B| {
        if on {
            out |= b.bit();
        }
    };
    // Hat: 0 up, 1 up-right, 2 right, 3 down-right, 4 down, 5 down-left,
    // 6 left, 7 up-left, 8 released.
    let hat = b0 & 0x0f;
    set(matches!(hat, 7 | 0 | 1), B::DPadUp);
    set(matches!(hat, 1 | 2 | 3), B::DPadRight);
    set(matches!(hat, 3 | 4 | 5), B::DPadDown);
    set(matches!(hat, 5 | 6 | 7), B::DPadLeft);
    set(b0 & 0x10 != 0, B::West); // square
    set(b0 & 0x20 != 0, B::South); // cross
    set(b0 & 0x40 != 0, B::East); // circle
    set(b0 & 0x80 != 0, B::North); // triangle
    set(b1 & 0x01 != 0, B::LeftBumper);
    set(b1 & 0x02 != 0, B::RightBumper);
    set(b1 & 0x04 != 0, B::LeftTrigger);
    set(b1 & 0x08 != 0, B::RightTrigger);
    set(b1 & 0x10 != 0, B::Select); // share / create
    set(b1 & 0x20 != 0, B::Start); // options
    set(b1 & 0x40 != 0, B::LeftThumb);
    set(b1 & 0x80 != 0, B::RightThumb);
    set(b2 & 0x01 != 0, B::Mode); // PS
    set(b2 & 0x02 != 0, B::Touchpad);
    if dualsense {
        set(b2 & 0x04 != 0, B::Misc1); // mute
    }
    out
}

fn i16_at(p: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([p[at], p[at + 1]])
}

/// A stick byte pair to `-1..1`, y flipped to UP (HID counts y downward).
fn stick(x: u8, y: u8) -> (f32, f32) {
    let n = |v: u8| ((f32::from(v) - 127.5) / 127.5).clamp(-1.0, 1.0);
    (n(x), -n(y))
}

fn trigger(v: u8) -> f32 {
    f32::from(v) / 255.0
}

fn gyro_rad_s(raw: [i16; 3]) -> [f32; 3] {
    raw.map(|v| f32::from(v) / GYRO_RES_PER_DEG_S * core::f32::consts::PI / 180.0)
}

fn accel_ms2(raw: [i16; 3]) -> [f32; 3] {
    raw.map(|v| f32::from(v) / ACC_RES_PER_G * G_TO_MS2)
}

/// `struct dualsense_touch_point` / `dualshock4_touch_point`: `contact`
/// (bit 7 SET = no finger; low bits = a finger id that increments per
/// touch), then 12-bit x and y packed into three bytes. Normalized with the
/// origin bottom-left, as `GamepadState::touchpad_y` specifies.
fn touch_point(q: &[u8], width: f32, height: f32) -> Option<(f32, f32)> {
    if q.len() < 4 || q[0] & 0x80 != 0 {
        return None;
    }
    let x = u16::from(q[1]) | (u16::from(q[2] & 0x0f) << 8);
    let y = u16::from(q[2] >> 4) | (u16::from(q[3]) << 4);
    Some((
        (f32::from(x) / width).clamp(0.0, 1.0),
        (1.0 - f32::from(y) / height).clamp(0.0, 1.0),
    ))
}

/// The Bluetooth reports' trailing CRC32 (little-endian, over everything
/// before it, seeded with the input seed byte), as `ps_check_crc32` checks
/// it in the kernel.
fn crc_ok(report: &[u8]) -> bool {
    let Some(split) = report.len().checked_sub(4) else {
        return false;
    };
    let expected = u32::from_le_bytes([
        report[split],
        report[split + 1],
        report[split + 2],
        report[split + 3],
    ]);
    input_crc32(&report[..split]) == expected
}

/// CRC-32 (IEEE, reflected) of the seed byte followed by `data`.
#[must_use]
pub fn input_crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xffff_ffff;
    for &b in core::iter::once(&INPUT_CRC32_SEED).chain(data) {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dualsense_usb(gyro: [i16; 3], accel: [i16; 3], touch: Option<(u16, u16)>) -> Vec<u8> {
        let mut r = vec![0u8; DS_INPUT_REPORT_USB_SIZE];
        r[0] = DS_INPUT_REPORT_USB;
        let p = &mut r[1..];
        p[0] = 255; // left stick x fully right
        p[1] = 0; // left stick y fully UP (HID y counts downward)
        p[2] = 128;
        p[3] = 128;
        p[4] = 255; // L2 fully pressed
        p[7] = 0x20 | 2; // cross + hat right
        p[8] = 0x01; // L1
        p[9] = 0x04; // mute
        for (i, g) in gyro.iter().enumerate() {
            p[15 + 2 * i..17 + 2 * i].copy_from_slice(&g.to_le_bytes());
        }
        for (i, a) in accel.iter().enumerate() {
            p[21 + 2 * i..23 + 2 * i].copy_from_slice(&a.to_le_bytes());
        }
        match touch {
            Some((x, y)) => {
                p[32] = 0x03; // finger id 3, active
                p[33] = (x & 0xff) as u8;
                p[34] = ((x >> 8) as u8 & 0x0f) | (((y & 0x0f) as u8) << 4);
                p[35] = (y >> 4) as u8;
            }
            None => p[32] = 0x80,
        }
        r
    }

    #[test]
    fn a_dualsense_usb_report_decodes_sticks_buttons_imu_and_touch() {
        let r = dualsense_usb([1024, -2048, 512], [8192, 0, -8192], Some((960, 540)));
        let s = parse(PlayStationPad::DualSense, &r).expect("input report");
        assert!((s.left_stick.0 - 1.0).abs() < 1e-3);
        assert!((s.left_stick.1 - 1.0).abs() < 1e-3, "HID y down becomes y up");
        assert!((s.right_stick.0).abs() < 0.01);
        assert!((s.left_trigger - 1.0).abs() < 1e-6);
        assert_eq!(s.right_trigger, 0.0);
        let b = s.buttons;
        assert!(b & GamepadButton::South.bit() != 0, "cross");
        assert!(b & GamepadButton::DPadRight.bit() != 0, "hat 2 = right");
        assert!(b & GamepadButton::DPadUp.bit() == 0);
        assert!(b & GamepadButton::LeftBumper.bit() != 0);
        assert!(b & GamepadButton::Misc1.bit() != 0, "mute");
        // 1024 raw = 1 deg/s = 0.01745 rad/s; -2048 = -2 deg/s.
        assert!((s.gyro[0] - 0.017_453).abs() < 1e-5);
        assert!((s.gyro[1] + 0.034_907).abs() < 1e-5);
        // 8192 raw = 1 g = 9.80665 m/s².
        assert!((s.accel[0] - 9.80665).abs() < 1e-4);
        assert_eq!(s.accel[1], 0.0);
        assert!((s.accel[2] + 9.80665).abs() < 1e-4);
        // (960, 540) of 1920 x 1080 = the centre, y flipped to bottom-left.
        let (tx, ty) = s.touch.expect("finger");
        assert!((tx - 0.5).abs() < 1e-3);
        assert!((ty - 0.5).abs() < 1e-3);
    }

    #[test]
    fn a_lifted_finger_is_no_touch() {
        let r = dualsense_usb([0; 3], [0; 3], None);
        assert_eq!(parse(PlayStationPad::DualSense, &r).unwrap().touch, None);
    }

    #[test]
    fn the_wrong_size_or_id_is_not_an_input_report() {
        let mut r = dualsense_usb([0; 3], [0; 3], None);
        assert!(parse(PlayStationPad::DualSense, &r[..63]).is_none());
        r[0] = 0x05;
        assert!(parse(PlayStationPad::DualSense, &r).is_none());
        assert!(parse(PlayStationPad::DualSense, &[]).is_none());
    }

    /// The Bluetooth report is the USB payload at offset 2 with a CRC tail:
    /// a good CRC parses to the same sample, a corrupted one is dropped.
    #[test]
    fn a_bluetooth_report_is_crc_checked() {
        let usb = dualsense_usb([1024, 0, 0], [0, 8192, 0], Some((0, 0)));
        let mut bt = vec![0u8; DS_INPUT_REPORT_BT_SIZE];
        bt[0] = DS_INPUT_REPORT_BT;
        bt[1] = 0x01; // seq / flags byte
        bt[2..2 + 63].copy_from_slice(&usb[1..]);
        let crc = input_crc32(&bt[..DS_INPUT_REPORT_BT_SIZE - 4]);
        bt[DS_INPUT_REPORT_BT_SIZE - 4..].copy_from_slice(&crc.to_le_bytes());
        let from_bt = parse(PlayStationPad::DualSense, &bt).expect("crc ok");
        let from_usb = parse(PlayStationPad::DualSense, &usb).unwrap();
        assert_eq!(from_bt, from_usb);
        // Top-left touch: x 0, y 0 of a y-down surface = bottom-left origin y 1.
        assert_eq!(from_bt.touch, Some((0.0, 1.0)));
        bt[20] ^= 0xff;
        assert!(parse(PlayStationPad::DualSense, &bt).is_none(), "corrupted");
    }

    /// The CRC is IEEE CRC-32 over the seed byte then the data; the kernel's
    /// `ps_check_crc32` computes exactly this. Pinned with the well-known
    /// value of "123456789" (0xcbf43926) by feeding the seed as data.
    #[test]
    fn the_crc_is_ieee_crc32_with_the_seed_prefixed() {
        // input_crc32(data) == crc32(seed ++ data); check the plain CRC via
        // an empty seedless run: crc32([0xa1]) must match a direct compute.
        fn crc32(data: &[u8]) -> u32 {
            let mut crc: u32 = 0xffff_ffff;
            for &b in data {
                crc ^= u32::from(b);
                for _ in 0..8 {
                    let mask = (crc & 1).wrapping_neg();
                    crc = (crc >> 1) ^ (0xedb8_8320 & mask);
                }
            }
            !crc
        }
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926, "the reference vector");
        let mut seeded = vec![INPUT_CRC32_SEED];
        seeded.extend_from_slice(b"hello");
        assert_eq!(input_crc32(b"hello"), crc32(&seeded));
    }

    #[test]
    fn a_dualshock4_usb_report_decodes_with_its_own_offsets() {
        let mut r = vec![0u8; DS4_INPUT_REPORT_USB_SIZE];
        r[0] = DS4_INPUT_REPORT_USB;
        let p = &mut r[1..];
        p[4] = 0x80 | 4; // triangle + hat down
        p[5] = 0x20; // options
        p[6] = 0x02; // touchpad click
        p[7] = 128; // L2 half
        p[12..14].copy_from_slice(&(-1024i16).to_le_bytes()); // gyro x = -1 deg/s
        p[22..24].copy_from_slice(&4096i16.to_le_bytes()); // accel z = 0.5 g
        p[32] = 1; // one touch report
        p[34] = 0x00; // finger 0 active
        p[35] = 0x80;
        p[36] = 0x07; // x = 0x780 = 1920 -> right edge; y low nibble 0
        p[37] = 0x00; // y = 0 -> top -> bottom-left origin y 1
        let s = parse(PlayStationPad::DualShock4, &r).expect("input report");
        assert!(s.buttons & GamepadButton::North.bit() != 0);
        assert!(s.buttons & GamepadButton::DPadDown.bit() != 0);
        assert!(s.buttons & GamepadButton::Start.bit() != 0);
        assert!(s.buttons & GamepadButton::Touchpad.bit() != 0);
        assert!((s.left_trigger - 128.0 / 255.0).abs() < 1e-6);
        assert!((s.gyro[0] + 0.017_453).abs() < 1e-5);
        assert!((s.accel[2] - 4.903_325).abs() < 1e-4);
        assert_eq!(s.touch, Some((1.0, 1.0)));
    }

    #[test]
    fn only_sony_pads_are_recognised() {
        let dev = |v: u16, p: u16| HidDevice {
            vendor_id: v,
            product_id: p,
            usage_page: 1,
            usage: 5,
            name: "".into(),
            serial: "".into(),
            instance: 1,
        };
        assert_eq!(PlayStationPad::of(&dev(SONY_VENDOR, DUALSENSE)), Some(PlayStationPad::DualSense));
        assert_eq!(
            PlayStationPad::of(&dev(SONY_VENDOR, DUALSHOCK4_V2)),
            Some(PlayStationPad::DualShock4)
        );
        assert_eq!(PlayStationPad::of(&dev(0x045e, DUALSENSE)), None, "not Sony");
        assert_eq!(PlayStationPad::of(&dev(SONY_VENDOR, 0x1234)), None);
    }
}
