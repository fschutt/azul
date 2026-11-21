//! Position fixes from an external GNSS receiver, for a computer without one of its own: a Mac
//! has no GPS, and the platform location there comes from Wi-Fi (and, where macOS offers it, a
//! nearby iPhone) - fine in town, coarse or absent on a motorway. A USB or Bluetooth receiver
//! fills the gap:
//!
//! - **gpsd** (`localhost:2947`), the receiver daemon of Linux and macOS (Homebrew), which
//!   speaks JSON: AzMaps asks it to watch and reads its `TPV` (time, position, velocity) and
//!   `SKY` (satellites) reports;
//! - **NMEA 0183** read straight from a serial device (`/dev/tty.usbmodem*`, a Bluetooth serial
//!   port) as a file: `GGA` (position, fix quality, satellites, HDOP), `RMC` (position, speed,
//!   course, date), `VTG` and `GSA`. A USB receiver ignores the serial speed; a device that needs
//!   one set goes through gpsd;
//! - an NMEA **replay** file, paced by its own timestamps: the demos' and tests' receiver.
//!
//! Phones have real GPS: on iOS and Android the platform location is the source.
//!
//! The parsers are plain functions, tested without a device; the readers run on the app's
//! worker thread and hand every fix to a callback.

use serde_json::Value;

/// How good a fix is, from what the receiver says about it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Quality {
    /// The fix's dimension: 0 none, 2 a 2D fix, 3 a 3D fix.
    pub dimension: u8,
    /// Whether it is corrected (DGPS, SBAS, RTK).
    pub corrected: bool,
    /// The satellites used.
    pub satellites: Option<u32>,
    /// The horizontal dilution of precision.
    pub hdop: Option<f64>,
}

/// The grade the fix-quality indicator shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    /// No fix at all.
    None,
    /// A fix, but more than 30 m uncertain.
    Poor,
    /// 10 to 30 m.
    Fair,
    /// Within 10 m.
    Good,
}

/// One fix from a receiver.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GnssFix {
    pub at: (f64, f64),
    /// When it was taken, unix seconds (UTC), where the receiver says the date too.
    pub time_utc_s: Option<f64>,
    /// The time of day it was taken, seconds after midnight UTC (what every sentence has).
    pub time_of_day_s: Option<f64>,
    /// Its horizontal uncertainty, metres.
    pub accuracy_m: Option<f64>,
    pub speed_mps: Option<f64>,
    /// The course over ground, degrees clockwise from true north.
    pub heading_deg: Option<f64>,
    pub altitude_m: Option<f64>,
    pub quality: Quality,
}

impl GnssFix {
    /// Its grade: by its uncertainty, else by the dilution of precision.
    #[must_use]
    pub fn grade(&self) -> Grade {
        if self.quality.dimension < 2 {
            return Grade::None;
        }
        let uncertainty = self
            .accuracy_m
            .or_else(|| self.quality.hdop.map(|h| h * METRES_PER_HDOP));
        match uncertainty {
            Some(m) if m <= 10.0 => Grade::Good,
            Some(m) if m <= 30.0 => Grade::Fair,
            Some(_) => Grade::Poor,
            // A fix that says nothing about its uncertainty is not trusted to the metre.
            None => Grade::Fair,
        }
    }
}

/// The metres of uncertainty per unit of HDOP of a consumer receiver.
pub const METRES_PER_HDOP: f64 = 5.0;

/// Whether an NMEA sentence's checksum (the XOR of everything between `$` and `*`) is right; a
/// sentence without one passes.
#[must_use]
pub fn checksum_ok(line: &str) -> bool {
    let line = line.trim();
    let Some(body) = line.strip_prefix('$').or_else(|| line.strip_prefix('!')) else {
        return false;
    };
    match body.split_once('*') {
        None => true,
        Some((data, sum)) => {
            let want = sum.trim().get(..2).and_then(|hex| u8::from_str_radix(hex, 16).ok());
            want == Some(data.bytes().fold(0u8, |acc, b| acc ^ b))
        }
    }
}

/// An NMEA coordinate (`ddmm.mmmm` / `dddmm.mmmm` and its hemisphere) in degrees.
#[must_use]
pub fn nmea_degrees(value: &str, hemisphere: &str) -> Option<f64> {
    let raw: f64 = value.trim().parse().ok()?;
    let degrees = (raw / 100.0).trunc();
    let minutes = raw - degrees * 100.0;
    if !(0.0..60.0).contains(&minutes) {
        return None;
    }
    let value = degrees + minutes / 60.0;
    match hemisphere.trim() {
        "S" | "W" => Some(-value),
        _ => Some(value),
    }
}

/// An NMEA time of day (`hhmmss` or `hhmmss.ss`) in seconds after midnight.
fn time_of_day(field: &str) -> Option<f64> {
    let field = field.trim();
    if field.len() < 6 || !field.is_char_boundary(6) {
        return None;
    }
    let h: f64 = field[0..2].parse().ok()?;
    let m: f64 = field[2..4].parse().ok()?;
    let s: f64 = field[4..].parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

/// An NMEA date (`ddmmyy`) as days since 1970 (years from 80 on are the 1900s).
fn days_since_1970(field: &str) -> Option<i64> {
    let field = field.trim();
    if field.len() != 6 {
        return None;
    }
    let day: u32 = field[0..2].parse().ok()?;
    let month: u32 = field[2..4].parse().ok()?;
    let yy: i32 = field[4..6].parse().ok()?;
    let year = if yy >= 80 { 1900 + yy } else { 2000 + yy };
    let date = chrono::NaiveDate::from_ymd_opt(year, month, day)?;
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1)?;
    Some(date.signed_duration_since(epoch).num_days())
}

/// A field as a number.
fn number(field: Option<&&str>) -> Option<f64> {
    field.and_then(|f| f.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

/// The NMEA reader's memory between sentences: an `RMC` gives position, speed and course, a
/// `GGA` of the same moment the quality; a receiver that sends no `RMC` has its fixes made from
/// `GGA` alone.
#[derive(Debug, Clone, Default)]
pub struct Nmea {
    /// The quality of the latest `GGA` / `GSA`.
    quality: Quality,
    /// The time of day of the latest `GGA`.
    gga_time: Option<f64>,
    /// The speed and course of the latest `VTG`.
    vtg: Option<(Option<f64>, Option<f64>)>,
    /// The altitude of the latest `GGA`.
    gga_altitude: Option<f64>,
    /// Whether the receiver sends `RMC` at all.
    seen_rmc: bool,
}

/// Metres per second in a knot.
const KNOT_MPS: f64 = 0.514_444;

impl Nmea {
    /// Reads one line; a fix when the line completes one.
    pub fn feed(&mut self, line: &str) -> Option<GnssFix> {
        if !checksum_ok(line) {
            return None;
        }
        let body = line.trim().trim_start_matches(['$', '!']);
        let data = body.split('*').next().unwrap_or("");
        let fields: Vec<&str> = data.split(',').collect();
        let kind = fields.first()?;
        // A talker (`GP`, `GN`, `GL`, ...) and the sentence's three letters.
        if kind.len() != 5 || !kind.is_ascii() {
            return None;
        }
        match &kind[2..] {
            "GGA" => self.gga(&fields),
            "RMC" => self.rmc(&fields),
            "VTG" => {
                let course = number(fields.get(1));
                let speed = number(fields.get(7)).map(|kmh| kmh / 3.6);
                self.vtg = Some((speed, course));
                None
            }
            "GSA" => {
                let dimension = fields.get(2).and_then(|f| f.trim().parse::<u8>().ok()).unwrap_or(0);
                self.quality.dimension = if dimension >= 2 { dimension } else { 0 };
                if let Some(hdop) = number(fields.get(16)) {
                    self.quality.hdop = Some(hdop);
                }
                None
            }
            _ => None,
        }
    }

    fn gga(&mut self, fields: &[&str]) -> Option<GnssFix> {
        let fix_kind = fields.get(6).and_then(|f| f.trim().parse::<u8>().ok()).unwrap_or(0);
        let time = fields.get(1).and_then(|f| time_of_day(f));
        let altitude = number(fields.get(9));
        self.gga_time = time;
        self.gga_altitude = altitude;
        if let Some(satellites) = fields.get(7).and_then(|f| f.trim().parse::<u32>().ok()) {
            self.quality.satellites = Some(satellites);
        }
        if let Some(hdop) = number(fields.get(8)) {
            self.quality.hdop = Some(hdop);
        }
        // 2 DGPS, 4 RTK fixed, 5 RTK float: corrected; 0 no fix.
        self.quality.corrected = matches!(fix_kind, 2 | 4 | 5);
        if fix_kind == 0 {
            self.quality.dimension = 0;
            return None;
        }
        if self.quality.dimension < 2 {
            self.quality.dimension = if altitude.is_some() { 3 } else { 2 };
        }
        if self.seen_rmc {
            return None;
        }
        let lat = nmea_degrees(fields.get(2)?, fields.get(3).copied().unwrap_or(""))?;
        let lon = nmea_degrees(fields.get(4)?, fields.get(5).copied().unwrap_or(""))?;
        let (speed, course) = self.vtg.unwrap_or((None, None));
        Some(GnssFix {
            at: (lat, lon),
            time_utc_s: None,
            time_of_day_s: time,
            accuracy_m: self.quality.hdop.map(|h| h * METRES_PER_HDOP),
            speed_mps: speed,
            heading_deg: course,
            altitude_m: altitude,
            quality: self.quality,
        })
    }

    fn rmc(&mut self, fields: &[&str]) -> Option<GnssFix> {
        self.seen_rmc = true;
        if fields.get(2).map(|f| f.trim()) != Some("A") {
            return None;
        }
        let lat = nmea_degrees(fields.get(3)?, fields.get(4).copied().unwrap_or(""))?;
        let lon = nmea_degrees(fields.get(5)?, fields.get(6).copied().unwrap_or(""))?;
        let time = fields.get(1).and_then(|f| time_of_day(f));
        let days = fields.get(9).and_then(|f| days_since_1970(f));
        #[allow(clippy::cast_precision_loss)] // days since 1970 fit an f64
        let time_utc_s = match (days, time) {
            (Some(days), Some(time)) => Some(days as f64 * 86_400.0 + time),
            _ => None,
        };
        let mut quality = self.quality;
        if quality.dimension < 2 {
            quality.dimension = 2;
        }
        // The altitude of the GGA of the same moment.
        let altitude = if self.gga_time.is_some() && self.gga_time == time {
            self.gga_altitude
        } else {
            None
        };
        Some(GnssFix {
            at: (lat, lon),
            time_utc_s,
            time_of_day_s: time,
            accuracy_m: quality.hdop.map(|h| h * METRES_PER_HDOP),
            speed_mps: number(fields.get(7)).map(|knots| knots * KNOT_MPS),
            heading_deg: number(fields.get(8)),
            altitude_m: altitude,
            quality,
        })
    }
}

/// The fix of a gpsd `TPV` report (`None` for other reports, and for a `TPV` without a fix);
/// a `SKY` report updates `quality` (satellites used, HDOP).
pub fn gpsd_report(line: &str, quality: &mut Quality) -> Option<GnssFix> {
    let doc: Value = serde_json::from_str(line.trim()).ok()?;
    let number = |key: &str| doc.get(key).and_then(Value::as_f64).filter(|v| v.is_finite());
    match doc.get("class").and_then(Value::as_str)? {
        "SKY" => {
            let used = doc.get("uSat").and_then(Value::as_u64).or_else(|| {
                let satellites = doc.get("satellites")?.as_array()?;
                let used = satellites
                    .iter()
                    .filter(|s| s.get("used").and_then(Value::as_bool) == Some(true))
                    .count();
                u64::try_from(used).ok().filter(|n| *n > 0)
            });
            if let Some(used) = used.and_then(|n| u32::try_from(n).ok()) {
                quality.satellites = Some(used);
            }
            if let Some(hdop) = number("hdop") {
                quality.hdop = Some(hdop);
            }
            None
        }
        "TPV" => {
            let mode = doc.get("mode").and_then(Value::as_u64).unwrap_or(0);
            if mode < 2 {
                quality.dimension = 0;
                return None;
            }
            quality.dimension = u8::try_from(mode).unwrap_or(3);
            let at = (number("lat")?, number("lon")?);
            let accuracy = number("eph").or_else(|| match (number("epx"), number("epy")) {
                (Some(x), Some(y)) => Some(x.max(y)),
                _ => None,
            });
            #[allow(clippy::cast_precision_loss)] // unix seconds fit an f64
            let time_utc_s = doc
                .get("time")
                .and_then(Value::as_str)
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.timestamp() as f64 + f64::from(t.timestamp_subsec_millis()) / 1000.0);
            Some(GnssFix {
                at,
                time_utc_s,
                time_of_day_s: time_utc_s.map(|t| t.rem_euclid(86_400.0)),
                accuracy_m: accuracy,
                speed_mps: number("speed"),
                heading_deg: number("track"),
                altitude_m: number("altMSL").or_else(|| number("alt")),
                quality: *quality,
            })
        }
        _ => None,
    }
}

/// The request that makes gpsd report in JSON.
pub const GPSD_WATCH: &str = "?WATCH={\"enable\":true,\"json\":true};\n";

/// The default address of gpsd.
pub const GPSD_ADDRESS: &str = "127.0.0.1:2947";

/// Where the fixes come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The platform's location (Core Location, the phone's GPS).
    Platform,
    /// gpsd at this `host:port`.
    Gpsd(String),
    /// NMEA from this serial device.
    Nmea(String),
    /// An NMEA file replayed at its own pace.
    Replay(String),
}

impl Source {
    /// The source a setting names: `platform`, `gpsd`, `gpsd:<host:port>`, `nmea:<device>`,
    /// `replay:<file>`; anything else is the platform's.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let text = text.trim();
        if text == "gpsd" {
            return Self::Gpsd(GPSD_ADDRESS.to_string());
        }
        let named = |prefix: &str| {
            text.strip_prefix(prefix)
                .map(str::trim)
                .filter(|rest| !rest.is_empty())
                .map(str::to_string)
        };
        if let Some(address) = named("gpsd:") {
            Self::Gpsd(address)
        } else if let Some(device) = named("nmea:") {
            Self::Nmea(device)
        } else if let Some(file) = named("replay:") {
            Self::Replay(file)
        } else {
            Self::Platform
        }
    }

    /// The setting's text for it (what [`Source::parse`] reads back).
    #[must_use]
    pub fn setting(&self) -> String {
        match self {
            Self::Platform => "platform".to_string(),
            Self::Gpsd(address) => format!("gpsd:{address}"),
            Self::Nmea(device) => format!("nmea:{device}"),
            Self::Replay(file) => format!("replay:{file}"),
        }
    }
}

/// The serial devices a receiver may be at: `/dev/tty.usbmodem*`, `/dev/tty.usbserial*`, a
/// Bluetooth port (`/dev/tty.*GPS*`, `/dev/rfcomm*`), `/dev/ttyUSB*`, `/dev/ttyACM*`.
#[must_use]
pub fn serial_devices() -> Vec<String> {
    serial_devices_in(std::path::Path::new("/dev"))
}

/// [`serial_devices`] in `dir`.
#[must_use]
pub fn serial_devices_in(dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            // macOS lists each serial port twice, `tty.*` (wait for the carrier) and `cu.*`:
            // the `tty.*` one reads a receiver's stream.
            let wanted = (name.starts_with("tty.")
                && ["usbmodem", "usbserial", "gps", "gnss"].iter().any(|w| lower.contains(w)))
                || name.starts_with("ttyUSB")
                || name.starts_with("ttyACM")
                || name.starts_with("rfcomm");
            wanted.then(|| entry.path().to_string_lossy().into_owned())
        })
        .collect();
    found.sort();
    found
}

/// How long to wait before the next fix of a replay: the time between their times of day
/// (across midnight too), divided by `speedup`; a second when a time is missing.
#[must_use]
pub fn replay_delay_s(previous: Option<f64>, next: Option<f64>, speedup: f64) -> f64 {
    let speedup = if speedup > 0.0 { speedup } else { 1.0 };
    let gap = match (previous, next) {
        (Some(a), Some(b)) => {
            let gap = if b < a { b + 86_400.0 - a } else { b - a };
            // A recording paused for minutes goes on after a second.
            if gap > 600.0 {
                1.0
            } else {
                gap
            }
        }
        _ => 1.0,
    };
    gap / speedup
}

/// Reads NMEA lines from `reader` until it ends or `on_fix` says to stop (returns `false`);
/// with `speedup` (a replay) the fixes come at their own pace times it, else as fast as the
/// device sends them.
pub fn read_nmea(
    reader: impl std::io::BufRead,
    speedup: Option<f64>,
    on_fix: &mut dyn FnMut(GnssFix) -> bool,
) {
    let mut nmea = Nmea::default();
    let mut last_time: Option<f64> = None;
    let mut first = true;
    for line in reader.lines() {
        let Ok(line) = line else {
            break;
        };
        let Some(fix) = nmea.feed(&line) else {
            continue;
        };
        if let (Some(speedup), false) = (speedup, first) {
            let wait = replay_delay_s(last_time, fix.time_of_day_s, speedup).min(10.0);
            std::thread::sleep(std::time::Duration::from_secs_f64(wait.max(0.0)));
        }
        first = false;
        last_time = fix.time_of_day_s;
        if !on_fix(fix) {
            return;
        }
    }
}

/// Reads gpsd at `address` until `on_fix` says to stop or the connection ends; `alive` is
/// asked between reports (at least once a second), so the reader can be let go while gpsd says
/// nothing.
///
/// # Errors
///
/// gpsd cannot be reached.
pub fn read_gpsd(
    address: &str,
    on_fix: &mut dyn FnMut(GnssFix) -> bool,
    alive: &mut dyn FnMut() -> bool,
) -> Result<(), String> {
    use std::{
        io::{BufRead, BufReader, Write},
        net::{TcpStream, ToSocketAddrs},
        time::Duration,
    };
    let target = address
        .to_socket_addrs()
        .map_err(|e| format!("{address}: {e}"))?
        .next()
        .ok_or_else(|| format!("{address}: no address"))?;
    let mut stream = TcpStream::connect_timeout(&target, Duration::from_secs(3))
        .map_err(|e| format!("gpsd at {address}: {e}"))?;
    // A read gives up after a second, so `alive` is asked while gpsd says nothing.
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .map_err(|e| e.to_string())?;
    stream
        .write_all(GPSD_WATCH.as_bytes())
        .map_err(|e| format!("gpsd at {address}: {e}"))?;
    let mut reader = BufReader::new(stream);
    let mut quality = Quality::default();
    let mut line = String::new();
    loop {
        if !alive() {
            return Ok(());
        }
        match reader.read_line(&mut line) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                // A line cut by the timeout is completed by the next read.
                if !line.ends_with('\n') {
                    continue;
                }
                let report = gpsd_report(&line, &mut quality);
                line.clear();
                if let Some(fix) = report {
                    if !on_fix(fix) {
                        return Ok(());
                    }
                }
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(format!("gpsd at {address}: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GGA: &str = "$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47";
    const RMC: &str = "$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A";

    #[test]
    fn a_sentence_with_a_wrong_checksum_is_refused() {
        assert!(checksum_ok(GGA));
        assert!(checksum_ok(RMC));
        assert!(!checksum_ok("$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*48"));
        assert!(checksum_ok("$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,"));
    }

    #[test]
    fn nmea_coordinates_are_degrees_and_minutes() {
        let lat = nmea_degrees("4807.038", "N").expect("a latitude");
        assert!((lat - 48.1173).abs() < 1e-4, "{lat}");
        let lon = nmea_degrees("01131.000", "W").expect("a longitude");
        assert!((lon + 11.516_666).abs() < 1e-5, "{lon}");
        assert_eq!(nmea_degrees("", "N"), None);
        assert_eq!(nmea_degrees("abc", "N"), None);
    }

    #[test]
    fn rmc_and_gga_make_one_fix_with_speed_course_and_quality() {
        let mut nmea = Nmea::default();
        assert_eq!(nmea.feed(GGA).map(|f| f.at.0.round()), Some(48.0), "GGA alone, until RMC shows up");
        let fix = nmea.feed(RMC).expect("a fix");
        assert!((fix.at.0 - 48.1173).abs() < 1e-4 && (fix.at.1 - 11.516_666).abs() < 1e-5);
        assert!((fix.speed_mps.expect("a speed") - 22.4 * 0.514_444).abs() < 1e-3);
        assert!((fix.heading_deg.expect("a course") - 84.4).abs() < 1e-9);
        assert_eq!(fix.quality.satellites, Some(8));
        assert!((fix.accuracy_m.expect("an uncertainty") - 0.9 * METRES_PER_HDOP).abs() < 1e-9);
        assert_eq!(fix.time_of_day_s, Some(12.0 * 3600.0 + 35.0 * 60.0 + 19.0));
        // 23 March 1994, 12:35:19 UTC.
        assert_eq!(fix.time_utc_s, Some(764_426_119.0));
        assert_eq!(fix.altitude_m, Some(545.4));
        // Once RMC is known, GGA only updates the quality.
        assert_eq!(nmea.feed(GGA), None);
    }

    #[test]
    fn a_void_rmc_or_a_gga_without_a_fix_is_no_fix() {
        let mut nmea = Nmea::default();
        assert_eq!(nmea.feed("$GPRMC,123520,V,,,,,,,230394,,*39"), None);
        assert_eq!(nmea.feed("$GPGGA,123520,,,,,0,00,,,M,,M,,*61"), None);
        assert_eq!(nmea.feed("not nmea at all"), None);
        assert_eq!(nmea.feed(""), None);
    }

    #[test]
    fn any_talker_is_read_and_gsa_says_the_dimension() {
        let mut nmea = Nmea::default();
        let _ = nmea.feed("$GNGSA,A,3,04,05,,09,12,,,24,,,,,2.5,1.3,2.1*27");
        let fix = nmea
            .feed("$GNRMC,081836,A,5231.000,N,01322.000,E,000.5,054.7,151026,,*02")
            .expect("a fix");
        assert!((fix.at.0 - 52.516_666).abs() < 1e-5);
        assert_eq!(fix.quality.dimension, 3);
        assert!((fix.quality.hdop.expect("hdop") - 1.3).abs() < 1e-9);
    }

    #[test]
    fn the_grade_follows_the_uncertainty() {
        let mut fix = GnssFix {
            quality: Quality { dimension: 3, ..Quality::default() },
            accuracy_m: Some(4.0),
            ..GnssFix::default()
        };
        assert_eq!(fix.grade(), Grade::Good);
        fix.accuracy_m = Some(25.0);
        assert_eq!(fix.grade(), Grade::Fair);
        fix.accuracy_m = Some(120.0);
        assert_eq!(fix.grade(), Grade::Poor);
        fix.quality.dimension = 0;
        assert_eq!(fix.grade(), Grade::None);
    }

    #[test]
    fn a_gpsd_tpv_is_a_fix_and_its_sky_the_quality() {
        let mut quality = Quality::default();
        let sky = r#"{"class":"SKY","device":"/dev/ttyACM0","hdop":0.8,"uSat":9,"satellites":[]}"#;
        assert_eq!(gpsd_report(sky, &mut quality), None);
        assert_eq!(quality.satellites, Some(9));
        let tpv = r#"{"class":"TPV","device":"/dev/ttyACM0","mode":3,"time":"2026-10-15T08:18:36.000Z","lat":52.5163,"lon":13.3777,"alt":40.1,"track":91.5,"speed":13.9,"eph":6.2}"#;
        let fix = gpsd_report(tpv, &mut quality).expect("a fix");
        assert_eq!(fix.at, (52.5163, 13.3777));
        assert_eq!(fix.speed_mps, Some(13.9));
        assert_eq!(fix.heading_deg, Some(91.5));
        assert_eq!(fix.accuracy_m, Some(6.2));
        assert_eq!(fix.quality.dimension, 3);
        assert_eq!(fix.quality.satellites, Some(9));
        assert_eq!(fix.time_utc_s, Some(1_792_052_316.0));
        let no_fix = r#"{"class":"TPV","device":"/dev/ttyACM0","mode":1}"#;
        assert_eq!(gpsd_report(no_fix, &mut quality), None);
        assert_eq!(gpsd_report(r#"{"class":"VERSION","release":"3.25"}"#, &mut quality), None);
    }

    #[test]
    fn a_source_is_named_by_its_setting() {
        assert_eq!(Source::parse("platform"), Source::Platform);
        assert_eq!(Source::parse(""), Source::Platform);
        assert_eq!(Source::parse("gpsd"), Source::Gpsd(GPSD_ADDRESS.to_string()));
        assert_eq!(Source::parse("gpsd:10.0.0.2:2947"), Source::Gpsd("10.0.0.2:2947".to_string()));
        assert_eq!(Source::parse("nmea:/dev/tty.usbmodem1101"), Source::Nmea("/dev/tty.usbmodem1101".to_string()));
        assert_eq!(Source::parse("replay:/tmp/drive.nmea"), Source::Replay("/tmp/drive.nmea".to_string()));
        for source in [Source::Platform, Source::Gpsd("h:1".to_string()), Source::Nmea("/dev/x".to_string()), Source::Replay("/f".to_string())] {
            assert_eq!(Source::parse(&source.setting()), source);
        }
    }

    #[test]
    fn the_serial_devices_a_receiver_may_be_at_are_listed() {
        let dir = std::env::temp_dir().join(format!("azul-routing-dev-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        for name in ["tty.usbmodem1101", "cu.usbmodem1101", "tty.Bluetooth-Incoming-Port", "ttyACM0", "null", "tty.GNSS-GPS"] {
            let _ = std::fs::write(dir.join(name), b"");
        }
        let found: Vec<String> = serial_devices_in(&dir)
            .into_iter()
            .map(|p| p.rsplit('/').next().unwrap_or("").to_string())
            .collect();
        assert_eq!(found, ["tty.GNSS-GPS", "tty.usbmodem1101", "ttyACM0"], "sorted, no cu.*, no other devices");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_replay_waits_as_long_as_the_receiver_did() {
        assert!((replay_delay_s(Some(100.0), Some(101.0), 1.0) - 1.0).abs() < 1e-9);
        assert!((replay_delay_s(Some(100.0), Some(102.0), 4.0) - 0.5).abs() < 1e-9);
        assert!((replay_delay_s(Some(86_399.0), Some(0.0), 1.0) - 1.0).abs() < 1e-9, "across midnight");
        assert!((replay_delay_s(None, Some(5.0), 1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_replay_file_gives_its_fixes_in_order() {
        let text = format!("{GGA}\n{RMC}\n$GPRMC,123520,A,4807.100,N,01131.100,E,022.4,084.4,230394,003.1,W\n");
        let mut fixes = Vec::new();
        read_nmea(std::io::Cursor::new(text), Some(1_000.0), &mut |fix| {
            fixes.push(fix);
            true
        });
        assert_eq!(fixes.len(), 3, "GGA, then the two RMC");
        assert!(fixes[2].at.0 > fixes[1].at.0);
        let mut first = Vec::new();
        read_nmea(std::io::Cursor::new(format!("{RMC}\n{RMC}\n")), None, &mut |fix| {
            first.push(fix);
            false
        });
        assert_eq!(first.len(), 1, "stopped after the first");
    }

    #[test]
    fn gpsd_is_asked_to_watch_and_its_reports_are_read() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let address = listener.local_addr().expect("an address").to_string();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("a client");
            let mut reader = BufReader::new(stream.try_clone().expect("a stream"));
            let mut stream = stream;
            let _ = writeln!(stream, r#"{{"class":"VERSION","release":"3.25"}}"#);
            let mut asked = String::new();
            let _ = reader.read_line(&mut asked);
            let _ = writeln!(stream, r#"{{"class":"TPV","mode":3,"lat":52.5,"lon":13.4,"speed":10.0}}"#);
            asked
        });
        let mut fixes = Vec::new();
        read_gpsd(&address, &mut |fix| {
            fixes.push(fix);
            false
        }, &mut || true)
        .expect("gpsd read");
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0].at, (52.5, 13.4));
        assert!(server.join().expect("the server").starts_with("?WATCH="));
        assert!(read_gpsd(&crate::test_server::closed_port().replace("http://", ""), &mut |_| true, &mut || true).is_err());
    }
}
