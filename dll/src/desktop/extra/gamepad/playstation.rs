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
//! The per-pad calibration IS applied when the caller has read it
//! (`parse_with`, 8f-i-a-i-b-i - `gamepad/mod.rs` reads the feature report
//! once per pad through `extra/hid::feature_report`); `parse` alone stays
//! nominal. The original note, kept for the history:
//! What WAS NOT applied: the per-pad calibration the kernel reads from
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
/// The CRC seed of FEATURE reports (the calibration report over Bluetooth).
const FEATURE_CRC32_SEED: u8 = 0xa3;
/// DualSense calibration: feature report 0x05, 41 bytes on both transports.
const DS_FEATURE_REPORT_CALIBRATION: u8 = 0x05;
const DS_FEATURE_REPORT_CALIBRATION_SIZE: usize = 41;
/// DualShock 4 calibration: 0x02 / 37 bytes over USB (and the dongle),
/// 0x05 / 41 bytes (CRC-tailed) over Bluetooth.
const DS4_FEATURE_REPORT_CALIBRATION_USB: u8 = 0x02;
const DS4_FEATURE_REPORT_CALIBRATION_USB_SIZE: usize = 37;
const DS4_FEATURE_REPORT_CALIBRATION_BT: u8 = 0x05;
const DS4_FEATURE_REPORT_CALIBRATION_BT_SIZE: usize = 41;
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
    /// The second finger (both pads report two points), if any.
    pub touch2: Option<(f32, f32)>,
}

/// How the pad is attached, read off the input report id: the calibration
/// report's id, size and (for the DualShock 4) layout depend on it. The DS4
/// USB dongle speaks the USB layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Usb,
    Bluetooth,
}

/// The transport an input report came over, or `None` for a report this
/// decoder does not know.
#[must_use]
pub fn transport_of(pad: PlayStationPad, bytes: &[u8]) -> Option<Transport> {
    let id = *bytes.first()?;
    match (pad, id) {
        (PlayStationPad::DualSense, DS_INPUT_REPORT_USB)
        | (PlayStationPad::DualShock4, DS4_INPUT_REPORT_USB) => Some(Transport::Usb),
        (PlayStationPad::DualSense, DS_INPUT_REPORT_BT)
        | (PlayStationPad::DualShock4, DS4_INPUT_REPORT_BT) => Some(Transport::Bluetooth),
        _ => None,
    }
}

/// One axis of the pad's factory calibration (8f-i-a-i-b-i): the kernel's
/// `ps_calibration_data` - `calibrated = (raw - bias) * numer / denom`, in
/// the pad's own raw units, BEFORE the nominal 1024 / 8192 conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisCalibration {
    pub bias: i32,
    pub sens_numer: i32,
    pub sens_denom: i32,
}

impl AxisCalibration {
    /// Nominal: the raw value as is.
    pub const IDENTITY: Self = Self {
        bias: 0,
        sens_numer: 1,
        sens_denom: 1,
    };

    fn apply(self, raw: i16) -> f32 {
        if self.sens_denom == 0 {
            return f32::from(raw);
        }
        (i32::from(raw) - self.bias) as f32 * self.sens_numer as f32 / self.sens_denom as f32
    }
}

/// The per-pad calibration the pad reports in its calibration feature
/// report (8f-i-a-i-b-i): gyro pitch / yaw / roll and accel x / y / z.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadCalibration {
    pub gyro: [AxisCalibration; 3],
    pub accel: [AxisCalibration; 3],
}

/// The calibration feature report to ask the pad for: `(report id, size)`.
#[must_use]
pub const fn calibration_report(pad: PlayStationPad, transport: Transport) -> (u8, usize) {
    match (pad, transport) {
        (PlayStationPad::DualSense, _) => {
            (DS_FEATURE_REPORT_CALIBRATION, DS_FEATURE_REPORT_CALIBRATION_SIZE)
        }
        (PlayStationPad::DualShock4, Transport::Usb) => (
            DS4_FEATURE_REPORT_CALIBRATION_USB,
            DS4_FEATURE_REPORT_CALIBRATION_USB_SIZE,
        ),
        (PlayStationPad::DualShock4, Transport::Bluetooth) => (
            DS4_FEATURE_REPORT_CALIBRATION_BT,
            DS4_FEATURE_REPORT_CALIBRATION_BT_SIZE,
        ),
    }
}

/// Decode a calibration feature report (8f-i-a-i-b-i), the layout the kernel's
/// `dualsense_get_calibration_data` / `dualshock4_get_calibration_data` read.
/// BLIND per the user's ruling - written from that source, not from a pad.
///
/// Offsets (after the id at 0): gyro pitch / yaw / roll BIAS at 1 / 3 / 5;
/// then the six gyro plus / minus values - `pitch+ pitch- yaw+ yaw- roll+
/// roll-` on the DualSense and the DS4 over USB, but `pitch+ yaw+ roll+
/// pitch- yaw- roll-` on the DS4 over Bluetooth; gyro speed plus / minus at
/// 19 / 21; accel x / y / z plus / minus at 23 .. 33. Over Bluetooth the
/// last four bytes are the feature CRC (seed 0xA3). A report whose ranges
/// are zero (a pad answering before it is ready, the DS4 over Bluetooth is
/// known to) is rejected so the caller can retry or stay nominal.
#[must_use]
pub fn parse_calibration(
    pad: PlayStationPad,
    transport: Transport,
    bytes: &[u8],
) -> Option<PadCalibration> {
    let (id, size) = calibration_report(pad, transport);
    if bytes.len() != size || bytes[0] != id {
        return None;
    }
    if transport == Transport::Bluetooth && !feature_crc_ok(bytes) {
        return None;
    }
    let gyro_bias = [i16_at(bytes, 1), i16_at(bytes, 3), i16_at(bytes, 5)];
    let bt_ds4 = matches!(
        (pad, transport),
        (PlayStationPad::DualShock4, Transport::Bluetooth)
    );
    let (gyro_plus, gyro_minus) = if bt_ds4 {
        (
            [i16_at(bytes, 7), i16_at(bytes, 9), i16_at(bytes, 11)],
            [i16_at(bytes, 13), i16_at(bytes, 15), i16_at(bytes, 17)],
        )
    } else {
        (
            [i16_at(bytes, 7), i16_at(bytes, 11), i16_at(bytes, 15)],
            [i16_at(bytes, 9), i16_at(bytes, 13), i16_at(bytes, 17)],
        )
    };
    let speed_2x = i32::from(i16_at(bytes, 19)) + i32::from(i16_at(bytes, 21));
    let acc_plus = [i16_at(bytes, 23), i16_at(bytes, 27), i16_at(bytes, 31)];
    let acc_minus = [i16_at(bytes, 25), i16_at(bytes, 29), i16_at(bytes, 33)];
    if speed_2x <= 0 {
        return None;
    }
    let mut gyro = [AxisCalibration::IDENTITY; 3];
    for i in 0..3 {
        let denom = (i32::from(gyro_plus[i]) - i32::from(gyro_minus[i])).abs();
        if denom == 0 {
            return None;
        }
        gyro[i] = AxisCalibration {
            bias: i32::from(gyro_bias[i]),
            sens_numer: speed_2x * GYRO_RES_PER_DEG_S as i32,
            sens_denom: denom,
        };
    }
    let mut accel = [AxisCalibration::IDENTITY; 3];
    for i in 0..3 {
        let range_2g = i32::from(acc_plus[i]) - i32::from(acc_minus[i]);
        if range_2g == 0 {
            return None;
        }
        accel[i] = AxisCalibration {
            bias: i32::from(acc_plus[i]) - range_2g / 2,
            sens_numer: 2 * ACC_RES_PER_G as i32,
            sens_denom: range_2g,
        };
    }
    Some(PadCalibration { gyro, accel })
}

/// Decode an input report with the pad's nominal sensor resolutions.
#[must_use]
pub fn parse(pad: PlayStationPad, bytes: &[u8]) -> Option<PadSample> {
    parse_with(pad, bytes, None)
}

/// Decode one raw input report from a PlayStation pad. `bytes` is the report
/// exactly as the platform handed it, report id in `bytes[0]` (hidraw,
/// IOKit and raw input all include it for a device that uses ids). `None`
/// for a report that is not an input report, is the wrong size, or fails
/// its Bluetooth CRC. The pad's calibration is applied to gyro and accel
/// when the caller has read it (8f-i-a-i-b-i); nominal otherwise.
#[must_use]
pub fn parse_with(
    pad: PlayStationPad,
    bytes: &[u8],
    calibration: Option<&PadCalibration>,
) -> Option<PadSample> {
    let id = *bytes.first()?;
    match pad {
        PlayStationPad::DualSense => match (id, bytes.len()) {
            (DS_INPUT_REPORT_USB, DS_INPUT_REPORT_USB_SIZE) => {
                parse_dualsense(&bytes[1..], calibration)
            }
            (DS_INPUT_REPORT_BT, DS_INPUT_REPORT_BT_SIZE) => {
                if !crc_ok(bytes) {
                    return None;
                }
                parse_dualsense(&bytes[2..], calibration)
            }
            _ => None,
        },
        PlayStationPad::DualShock4 => match (id, bytes.len()) {
            (DS4_INPUT_REPORT_USB, DS4_INPUT_REPORT_USB_SIZE) => {
                parse_dualshock4(&bytes[1..], calibration)
            }
            (DS4_INPUT_REPORT_BT, DS4_INPUT_REPORT_BT_SIZE) => {
                if !crc_ok(bytes) {
                    return None;
                }
                parse_dualshock4(&bytes[3..], calibration)
            }
            _ => None,
        },
    }
}

/// `struct dualsense_input_report`, starting at `p[0]` (= `x`).
fn parse_dualsense(p: &[u8], calibration: Option<&PadCalibration>) -> Option<PadSample> {
    if p.len() < 40 {
        return None;
    }
    // x y rx ry z rz seq buttons[4] reserved[4] gyro[3] accel[3] ts[4]
    // reserved2 points[2]
    let buttons = ps_buttons(p[7], p[8], p[9], true);
    let gyro = calibrated([i16_at(p, 15), i16_at(p, 17), i16_at(p, 19)], calibration.map(|c| c.gyro));
    let accel = calibrated([i16_at(p, 21), i16_at(p, 23), i16_at(p, 25)], calibration.map(|c| c.accel));
    let touch = touch_point(&p[32..36], DS_TOUCHPAD_WIDTH, DS_TOUCHPAD_HEIGHT);
    let touch2 = touch_point(&p[36..40], DS_TOUCHPAD_WIDTH, DS_TOUCHPAD_HEIGHT);
    Some(PadSample {
        buttons,
        left_stick: stick(p[0], p[1]),
        right_stick: stick(p[2], p[3]),
        left_trigger: trigger(p[4]),
        right_trigger: trigger(p[5]),
        gyro: gyro_rad_s(gyro),
        accel: accel_ms2(accel),
        touch,
        touch2,
    })
}

/// `struct dualshock4_input_report_common` (32 bytes) starting at `p[0]`,
/// followed by `num_touch_reports` and the touch reports.
fn parse_dualshock4(p: &[u8], calibration: Option<&PadCalibration>) -> Option<PadSample> {
    if p.len() < 42 {
        return None;
    }
    // x y rx ry buttons[3] z rz ts[2] temp gyro[3] accel[3] reserved2[5]
    // status[2] reserved3 | num_touch_reports | { timestamp, points[2] } ...
    let buttons = ps_buttons(p[4], p[5], p[6], false);
    let gyro = calibrated([i16_at(p, 12), i16_at(p, 14), i16_at(p, 16)], calibration.map(|c| c.gyro));
    let accel = calibrated([i16_at(p, 18), i16_at(p, 20), i16_at(p, 22)], calibration.map(|c| c.accel));
    let num_touch_reports = p[32];
    let (touch, touch2) = if num_touch_reports > 0 {
        (
            touch_point(&p[34..38], DS4_TOUCHPAD_WIDTH, DS4_TOUCHPAD_HEIGHT),
            touch_point(&p[38..42], DS4_TOUCHPAD_WIDTH, DS4_TOUCHPAD_HEIGHT),
        )
    } else {
        (None, None)
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
        touch2,
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

/// Raw sensor words into the pad's raw units, calibrated when the pad's
/// calibration is known (8f-i-a-i-b-i), as they are otherwise.
fn calibrated(raw: [i16; 3], axes: Option<[AxisCalibration; 3]>) -> [f32; 3] {
    match axes {
        Some(a) => [a[0].apply(raw[0]), a[1].apply(raw[1]), a[2].apply(raw[2])],
        None => raw.map(f32::from),
    }
}

fn gyro_rad_s(raw: [f32; 3]) -> [f32; 3] {
    raw.map(|v| v / GYRO_RES_PER_DEG_S * core::f32::consts::PI / 180.0)
}

fn accel_ms2(raw: [f32; 3]) -> [f32; 3] {
    raw.map(|v| v / ACC_RES_PER_G * G_TO_MS2)
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
    crc_ok_with(report, input_crc32)
}

/// A feature report's trailing CRC (seed 0xA3) checks out.
fn feature_crc_ok(report: &[u8]) -> bool {
    crc_ok_with(report, feature_crc32)
}

fn crc_ok_with(report: &[u8], crc: fn(&[u8]) -> u32) -> bool {
    let Some(split) = report.len().checked_sub(4) else {
        return false;
    };
    let expected = u32::from_le_bytes([
        report[split],
        report[split + 1],
        report[split + 2],
        report[split + 3],
    ]);
    crc(&report[..split]) == expected
}

/// CRC-32 (IEEE, reflected) of the seed byte followed by `data`.
#[must_use]
pub fn input_crc32(data: &[u8]) -> u32 {
    crc32_seeded(INPUT_CRC32_SEED, data)
}

/// CRC-32 of a FEATURE report: the seed byte 0xA3 followed by `data`.
#[must_use]
pub fn feature_crc32(data: &[u8]) -> u32 {
    crc32_seeded(FEATURE_CRC32_SEED, data)
}

fn crc32_seeded(seed: u8, data: &[u8]) -> u32 {
    let mut crc: u32 = 0xffff_ffff;
    for &b in core::iter::once(&seed).chain(data) {
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

    /// A DualSense calibration report whose gyro sensitivity is exactly 2x
    /// nominal (speed_2x = 2 * range) with a pitch bias, and whose accel x
    /// range is 1.25x nominal: `parse_with` applies both, `parse` neither.
    fn ds_calibration(bt: bool) -> Vec<u8> {
        let mut b = vec![0u8; DS_FEATURE_REPORT_CALIBRATION_SIZE];
        b[0] = DS_FEATURE_REPORT_CALIBRATION;
        let put = |b: &mut Vec<u8>, at: usize, v: i16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
        put(&mut b, 1, 24); // pitch bias
        // gyro plus / minus: range 1000 per axis
        for at in [7usize, 11, 15] {
            put(&mut b, at, 500);
            put(&mut b, at + 2, -500);
        }
        // speed plus + minus = 2000 = 2 * range -> 2048 raw units per unit
        put(&mut b, 19, 1000);
        put(&mut b, 21, 1000);
        // accel: x range 2g = 10240 (1.25 * 8192), y / z nominal, centred
        put(&mut b, 23, 5120);
        put(&mut b, 25, -5120);
        put(&mut b, 27, 4096);
        put(&mut b, 29, -4096);
        put(&mut b, 31, 4096);
        put(&mut b, 33, -4096);
        if bt {
            let crc = feature_crc32(&b[..37]);
            b[37..41].copy_from_slice(&crc.to_le_bytes());
        }
        b
    }

    #[test]
    fn calibration_scales_and_biases_the_sensors_and_nominal_is_untouched() {
        let cal = parse_calibration(PlayStationPad::DualSense, Transport::Usb, &ds_calibration(false))
            .expect("a well-formed USB calibration report");
        assert_eq!(cal.gyro[0].bias, 24);
        assert_eq!(cal.gyro[0].sens_numer, 2000 * 1024);
        assert_eq!(cal.gyro[0].sens_denom, 1000);
        assert_eq!(cal.accel[0].bias, 0);
        assert_eq!(cal.accel[0].sens_numer, 16384);
        assert_eq!(cal.accel[0].sens_denom, 10240);

        let report = dualsense_usb([1024 + 24, 0, 0], [8192, 0, 0], None);
        let nominal = parse(PlayStationPad::DualSense, &report).unwrap();
        let calibrated = parse_with(PlayStationPad::DualSense, &report, Some(&cal)).unwrap();
        let deg = |rad: f32| rad * 180.0 / core::f32::consts::PI;
        // nominal: 1048 raw = 1048/1024 deg/s; calibrated: (1048-24) * 2048 / 1024 = 2048 deg/s
        assert!((deg(nominal.gyro[0]) - 1048.0 / 1024.0).abs() < 1e-4);
        assert!((deg(calibrated.gyro[0]) - 2048.0).abs() < 1e-2, "{}", deg(calibrated.gyro[0]));
        // accel x: nominal 1 g; calibrated 8192 * 16384 / 10240 / 8192 = 1.6 g
        assert!((nominal.accel[0] / G_TO_MS2 - 1.0).abs() < 1e-5);
        assert!((calibrated.accel[0] / G_TO_MS2 - 1.6).abs() < 1e-4);
    }

    #[test]
    fn a_bluetooth_calibration_report_needs_its_feature_crc() {
        let good = ds_calibration(true);
        assert!(parse_calibration(PlayStationPad::DualSense, Transport::Bluetooth, &good).is_some());
        let mut bad = good.clone();
        bad[38] ^= 0x01;
        assert!(parse_calibration(PlayStationPad::DualSense, Transport::Bluetooth, &bad).is_none());
        // The USB reading of the same bytes ignores the tail.
        assert!(parse_calibration(PlayStationPad::DualSense, Transport::Usb, &bad).is_some());
    }

    #[test]
    fn a_zero_range_report_is_rejected_not_applied() {
        // A pad answering before it is ready reports zeros; applying that
        // would divide by zero or flatten every sample.
        let mut b = vec![0u8; DS_FEATURE_REPORT_CALIBRATION_SIZE];
        b[0] = DS_FEATURE_REPORT_CALIBRATION;
        assert!(parse_calibration(PlayStationPad::DualSense, Transport::Usb, &b).is_none());
        assert_eq!(calibration_report(PlayStationPad::DualShock4, Transport::Usb), (0x02, 37));
        assert_eq!(calibration_report(PlayStationPad::DualShock4, Transport::Bluetooth), (0x05, 41));
    }

    #[test]
    fn ds4_over_bluetooth_interleaves_plus_then_minus() {
        let mut b = vec![0u8; DS4_FEATURE_REPORT_CALIBRATION_BT_SIZE];
        b[0] = DS4_FEATURE_REPORT_CALIBRATION_BT;
        let put = |b: &mut Vec<u8>, at: usize, v: i16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
        put(&mut b, 7, 500);
        put(&mut b, 9, 600);
        put(&mut b, 11, 700);
        put(&mut b, 13, -500);
        put(&mut b, 15, -600);
        put(&mut b, 17, -700);
        put(&mut b, 19, 1000);
        put(&mut b, 21, 1000);
        for at in [23usize, 27, 31] {
            put(&mut b, at, 4096);
            put(&mut b, at + 2, -4096);
        }
        let crc = feature_crc32(&b[..37]);
        b[37..41].copy_from_slice(&crc.to_le_bytes());
        let cal = parse_calibration(PlayStationPad::DualShock4, Transport::Bluetooth, &b).unwrap();
        assert_eq!(
            [cal.gyro[0].sens_denom, cal.gyro[1].sens_denom, cal.gyro[2].sens_denom],
            [1000, 1200, 1400]
        );
        assert_eq!(transport_of(PlayStationPad::DualShock4, &[DS4_INPUT_REPORT_BT]), Some(Transport::Bluetooth));
        assert_eq!(transport_of(PlayStationPad::DualShock4, &[DS4_INPUT_REPORT_USB]), Some(Transport::Usb));
        assert_eq!(transport_of(PlayStationPad::DualSense, &[0x7f]), None);
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
    /// The second point of a DualSense report (`points[1]`, bytes 36..40) is
    /// the second finger; an inactive first slot does not hide it.
    #[test]
    fn a_second_finger_lands_in_touch2() {
        let mut r = dualsense_usb([0; 3], [0; 3], Some((0, 0)));
        // second finger (payload byte 36 = report byte 37): id 5 active,
        // x = 1919, y = 0 (top-right of a y-down surface)
        r[1 + 36] = 0x05;
        r[1 + 37] = (1919u16 & 0xff) as u8;
        r[1 + 38] = (1919u16 >> 8) as u8 & 0x0f;
        r[1 + 39] = 0;
        let s = parse(PlayStationPad::DualSense, &r).unwrap();
        assert_eq!(s.touch, Some((0.0, 1.0)));
        let (x, y) = s.touch2.expect("second finger");
        assert!((x - 1.0).abs() < 0.01 && (y - 1.0).abs() < 0.01, "{x} {y}");
        // and lifting only the first finger keeps the second
        r[1 + 32] = 0x80;
        let s = parse(PlayStationPad::DualSense, &r).unwrap();
        assert_eq!(s.touch, None);
        assert!(s.touch2.is_some());
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
