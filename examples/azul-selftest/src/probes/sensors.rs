//! The motion and environment sensors, and the geolocation fix.
//!
//! The engine starts the sensors once from the layout pass and the app reads the last reading
//! (`CallbackInfo::get_sensor_reading`), so a probe waits for a reading for a few seconds. The emulator
//! feeds them (`adb emu sensor set acceleration|gyroscope|magnetic-field|light|proximity|pressure x:y:z`);
//! a phone is moved by hand. A sensor the device does not have answers UNAVAILABLE; the AGGREGATE row
//! `sensors.any` is where a broken wiring shows: the capability says available and not one sensor delivered
//! a reading.

use azul::{
    dom::{Dom, GeolocationProbeConfig},
    misc::{Capability, PermissionState},
    sensor::SensorKind,
    window::PlatformCapability,
};

use super::{describe_capability, plain, Ctx, ProbeDef, Step};
use crate::report::Verdict;

/// How long a sensor gets to deliver its first reading.
const SENSOR_WAIT_MS: u64 = 3_000;
/// How long the location probe waits for a fix.
const FIX_WAIT_MS: u64 = 15_000;

pub static PROBES: &[ProbeDef] = &[
    plain("sensors.accelerometer", "sensors", "Accelerometer", accelerometer),
    plain("sensors.gyroscope", "sensors", "Gyroscope", gyroscope),
    plain("sensors.magnetometer", "sensors", "Magnetometer", magnetometer),
    plain("sensors.light", "sensors", "Ambient light", light),
    plain("sensors.proximity", "sensors", "Proximity", proximity),
    plain("sensors.barometer", "sensors", "Barometer", barometer),
    plain("sensors.any", "sensors", "Any sensor delivered (aggregate)", any_sensor),
    ProbeDef {
        id: "sensors.geolocation",
        group: "sensors",
        title: "Geolocation fix",
        human: None,
        note: "",
        widget: Some(location_widget),
        step: geolocation,
    },
];

fn read_sensor(ctx: &mut Ctx<'_>, kind: SensorKind, name: &str) -> Step {
    if let Some(reading) = ctx.info.get_sensor_reading(kind).into_option() {
        return Step::Done(Verdict::Pass(format!(
            "x={:.3} y={:.3} z={:.3} (t={} ms) after {} ms",
            reading.x, reading.y, reading.z, reading.timestamp_ms, ctx.elapsed_ms
        )));
    }
    if ctx.elapsed_ms < SENSOR_WAIT_MS {
        return Step::Wait;
    }
    let (available, backend) = describe_capability(&PlatformCapability::sensors());
    if !available {
        return Step::Done(Verdict::Unavailable(format!("the sensors capability is not available: {backend}")));
    }
    let why = format!("no {name} reading in {SENSOR_WAIT_MS} ms (the device may not have one; backend {backend})");
    Step::Done(if ctx.unattended {
        Verdict::Unavailable(why)
    } else {
        Verdict::NeedsHuman(format!("{why}: move or touch the phone, then run the row again"))
    })
}

fn accelerometer(ctx: &mut Ctx<'_>) -> Step {
    read_sensor(ctx, SensorKind::Accelerometer, "accelerometer")
}
fn gyroscope(ctx: &mut Ctx<'_>) -> Step {
    read_sensor(ctx, SensorKind::Gyroscope, "gyroscope")
}
fn magnetometer(ctx: &mut Ctx<'_>) -> Step {
    read_sensor(ctx, SensorKind::Magnetometer, "magnetometer")
}
fn light(ctx: &mut Ctx<'_>) -> Step {
    read_sensor(ctx, SensorKind::AmbientLight, "ambient light")
}
fn proximity(ctx: &mut Ctx<'_>) -> Step {
    read_sensor(ctx, SensorKind::Proximity, "proximity")
}
fn barometer(ctx: &mut Ctx<'_>) -> Step {
    read_sensor(ctx, SensorKind::Barometer, "barometer")
}

/// The capability says available; did ANY sensor row pass? Not one passing is a wiring failure (the Java
/// glue's class lookup, a listener registered without a Looper, a manifest entry), not a phone without sensors.
fn any_sensor(ctx: &mut Ctx<'_>) -> Step {
    let (available, backend) = describe_capability(&PlatformCapability::sensors());
    if !available {
        return Step::Done(Verdict::Unavailable(format!("the sensors capability is not available: {backend}")));
    }
    let passed: Vec<&str> = ctx
        .done
        .iter()
        .filter(|row| row.id.starts_with("sensors.") && matches!(row.verdict, Verdict::Pass(_)))
        .map(|row| row.id.as_str())
        .collect();
    if passed.is_empty() {
        return Step::Done(Verdict::Fail(format!(
            "the sensors capability says available via {backend} and not one sensor delivered a reading"
        )));
    }
    Step::Done(Verdict::Pass(format!("{} delivered: {}", passed.len(), passed.join(", "))))
}

/// The probe node: the engine subscribes to the platform's location service while the node is in the DOM
/// (and asks for the permission then: a dialog comes only when this row runs).
fn location_widget(_elapsed_ms: u64) -> Dom {
    Dom::create_geolocation_probe(GeolocationProbeConfig {
        high_accuracy: true,
        background: false,
        max_accuracy_m: 100.0,
        min_interval_ms: 1_000,
    })
}

fn geolocation(ctx: &mut Ctx<'_>) -> Step {
    let (available, backend) = describe_capability(&PlatformCapability::geolocation());
    if !available {
        return Step::Done(Verdict::Unavailable(format!("the geolocation capability is not available: {backend}")));
    }
    let permission = ctx.info.get_permission_status(Capability::Geolocation);
    match permission {
        PermissionState::Denied | PermissionState::Restricted => {
            return Step::Done(Verdict::Denied(
                "the location permission is refused: grant it in the settings (adb shell pm grant <package> android.permission.ACCESS_FINE_LOCATION)"
                    .into(),
            ));
        }
        PermissionState::NotDetermined if ctx.unattended => {
            return Step::Done(Verdict::NeedsHuman(
                "the location permission was never granted: grant it (adb shell pm grant <package> android.permission.ACCESS_FINE_LOCATION), or run this row by hand and answer the dialog"
                    .into(),
            ));
        }
        _ => {}
    }
    ctx.want_widget = true;
    if let Some(fix) = ctx.info.get_location_fix().into_option() {
        return Step::Done(Verdict::Pass(format!(
            "lat={:.5} lon={:.5} accuracy={:.0} m after {} ms",
            fix.latitude_deg, fix.longitude_deg, fix.accuracy_m, ctx.elapsed_ms
        )));
    }
    if ctx.elapsed_ms < FIX_WAIT_MS {
        return Step::Wait;
    }
    Step::Done(match ctx.info.get_permission_status(Capability::Geolocation) {
        PermissionState::Granted(_) | PermissionState::EphemeralGranted(_) => Verdict::Fail(format!(
            "the permission is granted and the location service gave no fix in {FIX_WAIT_MS} ms (on the emulator: adb emu geo fix <lon> <lat>)"
        )),
        _ => Verdict::NeedsHuman("answer the location permission dialog, then run the row again".into()),
    })
}
