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

use azul_core::events::{
    event_type_to_filters, EventData, EventFilter, EventProvider, EventType, HoverEventFilter,
    WindowEventFilter,
};
use azul_core::task::Instant;
use azul_layout::managers::gamepad::{
    drain_gamepad_states, push_gamepad_state, GamepadId, GamepadManager, GamepadState,
};
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
