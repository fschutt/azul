//! Where the position comes from: the platform's location (Core Location on a Mac - Wi-Fi
//! based, coarse or absent on a motorway; real GPS on a phone), or an external receiver read on
//! a worker thread ([`location_worker`]): gpsd, NMEA from a serial device, or an NMEA file
//! replayed at its own pace (the demos' and the tests' receiver). Settings > Location picks
//! it (`--gps` for one run); the fix-quality indicator says how good the last fix is.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use azul::{
    callbacks::WriteBackCallbackType,
    option::OptionThreadSendMsg,
    prelude::*,
    task::{ThreadReceiveMsg, ThreadReceiver, ThreadSendMsg, ThreadSender, ThreadWriteBackMsg},
};
use azul_appkit::l10n::{t_args, Arg};
use azul_routing::{
    gnss::{self, GnssFix, Grade, Source},
    navigation::Fix,
};

/// The setting that names the source (`platform`, `gpsd`, `nmea:<device>`, ...).
pub const SOURCE_KEY: &str = "location_source";

/// What the location worker reads.
pub struct LocationRequest {
    /// This reader's number; the newest one's is in `latest` (a change of source lets the old
    /// reader go).
    pub id: u64,
    pub source: Source,
    /// How much faster a replay runs than it was recorded.
    pub speedup: f64,
    pub latest: Arc<AtomicU64>,
    /// The write-back of every fix.
    pub on_fix: WriteBackCallbackType,
    /// The write-back when the source ends or fails.
    pub on_ended: WriteBackCallbackType,
}

/// A fix from the worker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExternalFix {
    /// The reader's number.
    pub id: u64,
    pub fix: GnssFix,
}

/// The source ended (a replay at its end, a device gone) or failed: why, for the indicator.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceEnded {
    pub id: u64,
    /// Empty when a replay simply ended.
    pub why: String,
}

/// The location worker, an azul `Thread`: reads its source until it ends, the window lets
/// go of it, or a newer reader replaces it.
pub extern "C" fn location_worker(mut init: RefAny, mut sender: ThreadSender, mut receiver: ThreadReceiver) {
    let Some((id, source, speedup, latest, on_fix, on_ended)) = init.downcast_ref::<LocationRequest>().map(|r| {
        (r.id, r.source.clone(), r.speedup, Arc::clone(&r.latest), r.on_fix, r.on_ended)
    }) else {
        return;
    };
    let mut alive = || {
        if latest.load(Ordering::Acquire) != id {
            return false;
        }
        loop {
            match receiver.recv() {
                OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread) => return false,
                OptionThreadSendMsg::Some(_) => {}
                OptionThreadSendMsg::None => return true,
            }
        }
    };
    let mut send_fix = |fix: GnssFix| -> bool {
        let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
            on_fix,
            RefAny::new(ExternalFix { id, fix }),
        )));
        true
    };
    let ended: Result<(), String> = match &source {
        Source::Platform => Ok(()),
        Source::Gpsd(address) => {
            let mut keep_going = |fix: GnssFix| send_fix(fix);
            gnss::read_gpsd(address, &mut keep_going, &mut alive)
        }
        Source::Nmea(path) | Source::Replay(path) => match std::fs::File::open(path) {
            Ok(file) => {
                let replay = matches!(source, Source::Replay(_)).then_some(speedup.max(0.01));
                let mut keep_going = |fix: GnssFix| alive() && send_fix(fix);
                gnss::read_nmea(std::io::BufReader::new(file), replay, &mut keep_going);
                Ok(())
            }
            Err(e) => Err(format!("{path}: {e}")),
        },
    };
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_ended,
        RefAny::new(SourceEnded {
            id,
            why: ended.err().unwrap_or_default(),
        }),
    )));
}

/// A receiver's fix as the navigation takes it, taken at `time_s` on its clock.
#[must_use]
pub fn fix_of_gnss(fix: &GnssFix, time_s: f64) -> Fix {
    Fix {
        at: fix.at,
        time_s,
        accuracy_m: fix.accuracy_m,
        heading_deg: fix.heading_deg,
        speed_mps: fix.speed_mps,
    }
}

/// The platform's fix as the navigation takes it (its missing values are NaN there).
#[must_use]
pub fn fix_of_platform(lat: f64, lon: f64, accuracy_m: f32, heading_deg: f32, speed_mps: f32, time_s: f64) -> Fix {
    let known = |v: f32| v.is_finite().then_some(f64::from(v));
    Fix {
        at: (lat, lon),
        time_s,
        accuracy_m: known(accuracy_m),
        heading_deg: known(heading_deg),
        speed_mps: known(speed_mps),
    }
}

/// The indicator's line for a receiver's fix: `3D fix · 9 satellites · ±5 m`.
#[must_use]
pub fn quality_text(fix: &GnssFix) -> String {
    if fix.quality.dimension < 2 {
        return t_args("azmaps-gps-no-fix", &[]);
    }
    let mut parts = vec![t_args(
        "azmaps-gps-fix",
        &[("dimension", Arg::from(u32::from(fix.quality.dimension)))],
    )];
    if let Some(count) = fix.quality.satellites {
        parts.push(t_args("azmaps-gps-satellites", &[("count", Arg::from(count))]));
    }
    let uncertainty = fix
        .accuracy_m
        .or_else(|| fix.quality.hdop.map(|h| h * gnss::METRES_PER_HDOP));
    if let Some(metres) = uncertainty {
        parts.push(uncertainty_text(metres));
    }
    parts.join(" \u{b7} ")
}

/// `±5 m`.
fn uncertainty_text(metres: f64) -> String {
    t_args(
        "azmaps-gps-uncertainty",
        &[("metres", Arg::from(format!("{:.0}", metres.max(0.0).round())))],
    )
}

/// The indicator's line for the platform's fix: `±65 m`.
#[must_use]
pub fn platform_quality_text(accuracy_m: f32) -> String {
    if accuracy_m.is_finite() {
        uncertainty_text(f64::from(accuracy_m))
    } else {
        t_args("azmaps-gps-no-fix", &[])
    }
}

/// The indicator's colour for a grade, `(r, g, b)`: green, amber, red, grey.
#[must_use]
pub const fn grade_colour(grade: Grade) -> (u8, u8, u8) {
    match grade {
        Grade::Good => (0x2e, 0x9d, 0x48),
        Grade::Fair => (0xe0, 0x9b, 0x1a),
        Grade::Poor => (0xd3, 0x3a, 0x2f),
        Grade::None => (0x8a, 0x8f, 0x98),
    }
}

/// The grade of the platform's fix, by its uncertainty.
#[must_use]
pub fn platform_grade(accuracy_m: f32) -> Grade {
    if !accuracy_m.is_finite() {
        Grade::None
    } else if accuracy_m <= 10.0 {
        Grade::Good
    } else if accuracy_m <= 30.0 {
        Grade::Fair
    } else {
        Grade::Poor
    }
}

/// The source for this run: `--gps`, else the setting, else the platform's.
#[must_use]
pub fn source_of(flag: Option<&str>, setting: Option<&str>) -> Source {
    [flag, setting]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|t| !t.is_empty())
        .map_or(Source::Platform, Source::parse)
}

#[cfg(test)]
mod tests {
    use azul_routing::gnss::Quality;

    use super::*;

    fn gnss_fix() -> GnssFix {
        GnssFix {
            at: (52.5163, 13.3777),
            time_utc_s: Some(1_792_052_316.0),
            time_of_day_s: Some(30_000.0),
            accuracy_m: Some(4.5),
            speed_mps: Some(13.9),
            heading_deg: Some(91.5),
            altitude_m: Some(40.0),
            quality: Quality {
                dimension: 3,
                corrected: false,
                satellites: Some(9),
                hdop: Some(0.9),
            },
        }
    }

    #[test]
    fn a_receivers_fix_and_the_platforms_are_the_navigations() {
        let fix = fix_of_gnss(&gnss_fix(), 12.0);
        assert_eq!(fix.at, (52.5163, 13.3777));
        assert_eq!(fix.time_s, 12.0);
        assert_eq!(fix.accuracy_m, Some(4.5));
        assert_eq!(fix.heading_deg, Some(91.5));
        assert_eq!(fix.speed_mps, Some(13.9));
        let platform = fix_of_platform(52.5, 13.4, 65.0, f32::NAN, f32::NAN, 3.0);
        assert_eq!(platform.accuracy_m, Some(65.0));
        assert_eq!(platform.heading_deg, None, "NaN is not known");
        assert_eq!(platform.speed_mps, None);
    }

    #[test]
    fn the_indicator_says_how_good_the_fix_is() {
        crate::l10n::in_english();
        assert_eq!(quality_text(&gnss_fix()), "3D fix \u{b7} 9 satellites \u{b7} \u{b1}5 m");
        let mut no_fix = gnss_fix();
        no_fix.quality.dimension = 0;
        assert_eq!(quality_text(&no_fix), "No fix");
        assert_eq!(platform_quality_text(65.4), "\u{b1}65 m");
        assert_eq!(platform_grade(8.0), Grade::Good);
        assert_eq!(platform_grade(150.0), Grade::Poor);
        assert_eq!(platform_grade(f32::NAN), Grade::None);
        assert_ne!(grade_colour(Grade::Good), grade_colour(Grade::Poor));
    }

    #[test]
    fn the_switch_wins_over_the_setting() {
        assert_eq!(source_of(Some("gpsd"), Some("nmea:/dev/x")), Source::Gpsd(gnss::GPSD_ADDRESS.to_string()));
        assert_eq!(source_of(None, Some("nmea:/dev/x")), Source::Nmea("/dev/x".to_string()));
        assert_eq!(source_of(None, None), Source::Platform);
        assert_eq!(source_of(Some(" "), None), Source::Platform);
    }
}
