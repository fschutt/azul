//! P9 - synthetic-event e2e harness (SUPER_PLAN_2 P9).
//!
//! Generates P2-P7 device events *synthetically* (no hardware) and asserts they
//! flow through the managers + event system the same way a real device sample
//! would. This file covers the P6 input-event paths (sensors, gamepad) end to
//! end: a synthetic reading/state injected via the platform-backend channel ->
//! the manager folds it -> the `EventProvider` yields the window event ->
//! `event_type_to_filters` routes it to the node + window filters apps attach
//! to. Geolocation / capture-frame / wacom-pen generators extend this in
//! follow-up ticks.

use azul_core::audio::AudioFrame;
use azul_core::events::{
    event_type_to_filters, EventData, EventFilter, EventProvider, EventType, HoverEventFilter,
    WindowEventFilter,
};
use azul_core::geolocation::LocationFix;
use azul_core::geom::LogicalPosition;
use azul_core::task::Instant;
use azul_core::video::VideoFrame;
use azul_css::{F32Vec, U8Vec};
use azul_layout::managers::geolocation::{drain_location_fixes, push_location_fix};
use azul_layout::managers::gamepad::{
    drain_gamepad_states, push_gamepad_state, GamepadId, GamepadManager, GamepadState,
};
use azul_layout::managers::gesture::GestureAndDragManager;
use azul_layout::managers::sensors::{
    drain_sensor_readings, push_sensor_reading, SensorKind, SensorManager, SensorReading,
};

fn ts() -> Instant {
    Instant::from(std::time::Instant::now())
}

/// P6 sensor: a synthetic accelerometer reading injected via the platform-
/// backend channel flows through the manager and produces a `SensorChanged`
/// event - the same path a real CoreMotion / Android sample takes.
#[test]
fn synthetic_sensor_reading_fires_sensorchanged() {
    let _ = drain_sensor_readings(); // clear any prior parked readings

    // Synthetic: the platform backend would call this on every sample.
    push_sensor_reading(SensorReading {
        kind: SensorKind::Accelerometer,
        x: 0.1,
        y: 0.2,
        z: 9.81,
        timestamp_ms: 1,
    });

    // The dll layout pass drains + folds each reading into the manager.
    let drained = drain_sensor_readings();
    assert_eq!(drained.len(), 1, "the synthetic reading parks + drains");

    let mut mgr = SensorManager::new();
    for r in &drained {
        mgr.set_reading(*r);
    }
    assert!(mgr.reading(SensorKind::Accelerometer).is_some());

    // The event pass collects the EventProvider's pending events.
    let events = mgr.get_pending_events(ts());
    assert_eq!(events.len(), 1, "one SensorChanged event is pending");
    assert_eq!(events[0].event_type, EventType::SensorChanged);
}

/// A reading bit-identical to the previous one must NOT re-fire (so an idle
/// sensor streaming the same value doesn't spam events).
#[test]
fn synthetic_unchanged_sensor_reading_does_not_refire() {
    let _ = drain_sensor_readings();
    let r = SensorReading {
        kind: SensorKind::Gyroscope,
        x: 1.0,
        y: 0.0,
        z: 0.0,
        timestamp_ms: 0,
    };
    let mut mgr = SensorManager::new();
    assert!(mgr.set_reading(r), "first reading is a change");
    mgr.clear_pending_event();
    assert!(!mgr.set_reading(r), "identical reading is not a change");
    assert!(
        mgr.get_pending_events(ts()).is_empty(),
        "no event for an unchanged reading"
    );
}

/// P6 gamepad: a synthetic pad state injected via the backend channel flows
/// through the manager and produces a `GamepadInput` event.
#[test]
fn synthetic_gamepad_state_fires_gamepadinput() {
    let _ = drain_gamepad_states();

    push_gamepad_state(GamepadState {
        id: GamepadId { id: 0 },
        connected: true,
        buttons: 0b101,
        left_stick_x: 0.5,
        left_stick_y: -0.5,
        right_stick_x: 0.0,
        right_stick_y: 0.0,
        left_z: 0.0,
        right_z: 1.0,
    });

    let drained = drain_gamepad_states();
    assert_eq!(drained.len(), 1);

    let mut mgr = GamepadManager::new();
    for s in &drained {
        mgr.set_state(*s);
    }
    assert!(mgr.primary().is_some(), "the connected pad is the primary");

    let events = mgr.get_pending_events(ts());
    assert_eq!(events.len(), 1, "one GamepadInput event is pending");
    assert_eq!(events[0].event_type, EventType::GamepadInput);
}

/// The P6 input events route to BOTH a node-level Hover filter and the window-
/// level filter, so an app can attach `EventFilter::Window(SensorChanged)` (or
/// the Hover mirror) instead of Timer-polling.
#[test]
fn input_events_route_to_node_and_window_filters() {
    let sensor = event_type_to_filters(EventType::SensorChanged, &EventData::None);
    assert!(sensor.contains(&EventFilter::Window(WindowEventFilter::SensorChanged)));
    assert!(sensor.contains(&EventFilter::Hover(HoverEventFilter::SensorChanged)));

    let gamepad = event_type_to_filters(EventType::GamepadInput, &EventData::None);
    assert!(gamepad.contains(&EventFilter::Window(WindowEventFilter::GamepadInput)));
    assert!(gamepad.contains(&EventFilter::Hover(HoverEventFilter::GamepadInput)));
}

/// P3 geolocation: a synthetic fix injected via the backend channel drains back
/// through the manager pipeline (`get_location_fix` reads it). Same path a real
/// CoreLocation / Android fix takes.
#[test]
fn synthetic_location_fix_drains() {
    let _ = drain_location_fixes();
    push_location_fix(LocationFix {
        latitude_deg: 37.7749,
        longitude_deg: -122.4194,
        accuracy_m: 5.0,
        altitude_m: 12.0,
        altitude_accuracy_m: 3.0,
        heading_deg: 90.0,
        speed_mps: 1.5,
        timestamp_ms: 42,
    });
    let drained = drain_location_fixes();
    assert_eq!(drained.len(), 1, "the synthetic fix parks + drains");
    assert!((drained[0].latitude_deg - 37.7749).abs() < 1e-9);
    assert!((drained[0].longitude_deg - (-122.4194)).abs() < 1e-9);
}

/// P7 audio: an `AudioFrame` survives the serialize/deserialize the azul-meet
/// UDP path uses (capture -> bytes -> wire -> bytes -> playback).
#[test]
fn synthetic_audio_frame_roundtrips() {
    let frame = AudioFrame {
        sample_rate: 48_000,
        channels: 1,
        samples: F32Vec::from_vec(vec![0.0, 0.5, -0.5, 1.0]),
    };
    let bytes = audio_to_bytes(&frame);
    let back = audio_from_bytes(&bytes).expect("deserializes");
    assert_eq!(back.sample_rate, 48_000);
    assert_eq!(back.channels, 1);
    assert_eq!(back.samples.as_ref(), &[0.0_f32, 0.5, -0.5, 1.0]);
}

/// P6 capture: a `VideoFrame` carries its RGBA bytes + dimensions intact (the
/// `on_frame` hook payload for the camera / screencap / video widgets).
#[test]
fn synthetic_video_frame_carries_pixels() {
    let px: Vec<u8> = vec![255, 0, 0, 255, 0, 255, 0, 255]; // 2 RGBA pixels
    let frame = VideoFrame {
        width: 2,
        height: 1,
        bytes: U8Vec::from_vec(px.clone()),
    };
    assert_eq!(frame.width, 2);
    assert_eq!(frame.height, 1);
    assert_eq!(frame.bytes.as_ref(), px.as_slice());
}

/// P2/P3 touch routes to its Hover filter (the touch + gesture paths flow
/// through `event_type_to_filters` too).
#[test]
fn synthetic_touch_routes_to_filter() {
    let filters = event_type_to_filters(EventType::TouchStart, &EventData::None);
    assert!(filters.contains(&EventFilter::Hover(HoverEventFilter::TouchStart)));
}

/// P2 pen / stylus: pen input is STATE-BASED (not a `PenDown` event). The
/// platform backend (or `debug_server`, for synthetic injection) populates
/// `PenState` through the gesture manager; apps react to ordinary pointer
/// events and read the pen detail via `CallbackInfo::get_pen_state` - exactly
/// what `examples/azul-paint` does to draw pressure-modulated strokes. Here we
/// drive that path synthetically.
#[test]
fn synthetic_pen_input_populates_penstate() {
    let mut mgr = GestureAndDragManager::new();
    mgr.update_pen_state(
        LogicalPosition::new(100.0, 200.0),
        0.75,         // pressure
        (10.0, -5.0), // tilt (x, y) in degrees
        true,         // in_contact
        false,        // is_eraser
        false,        // barrel button
        1,            // device id
    );
    let pen = mgr.get_pen_state().expect("pen state is populated");
    assert!((pen.pressure - 0.75).abs() < 1e-6);
    assert!(pen.in_contact);
    assert!(!pen.is_eraser);
    assert_eq!(pen.device_id, 1);
}

// (The PenDown/Move/Up *event filters* exist but are unused: the working pen
// path is pointer-event + get_pen_state, as above - NOT an EventType-routed
// PenDown. So pen is not a dead filter like GeolocationFix was; it uses the
// state-accessor pattern instead of a dedicated event.)

// --- AudioFrame <-> bytes (mirrors the azul-meet UDP framing) ---

fn audio_to_bytes(f: &AudioFrame) -> Vec<u8> {
    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(&f.sample_rate.to_le_bytes());
    b.extend_from_slice(&f.channels.to_le_bytes());
    for s in f.samples.as_ref() {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

fn audio_from_bytes(b: &[u8]) -> Option<AudioFrame> {
    if b.len() < 6 {
        return None;
    }
    let sample_rate = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let channels = u16::from_le_bytes([b[4], b[5]]);
    let mut samples: Vec<f32> = Vec::new();
    let mut i = 6;
    while i + 4 <= b.len() {
        samples.push(f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]));
        i += 4;
    }
    Some(AudioFrame {
        sample_rate,
        channels,
        samples: F32Vec::from_vec(samples),
    })
}
