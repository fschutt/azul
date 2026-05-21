---
slug: device-input
title: Device Input (Sensors, Gamepad, Geolocation)
language: en
canonical_slug: device-input
audience: external
maturity: beta
guide_order: 275
topic_only: false
short_desc: React to motion sensors, gamepads, and location as events - no polling
prerequisites: [events, callbacks]
tracked_files:
  - core/src/events.rs
  - core/src/sensors.rs
  - core/src/gamepad.rs
  - layout/src/managers/sensors.rs
  - layout/src/managers/gamepad.rs
  - layout/src/managers/geolocation.rs
last_generated_rev: 754b7f00e088960c14db598f64fa200dacc28bf1
generated_at: 2026-05-21T00:00:00Z
default-search-keys:
  - SensorChanged
  - GamepadInput
  - get_sensor_reading
  - get_primary_gamepad
  - get_gamepad_state
  - get_location_fix
  - create_geolocation_probe
  - WindowEventFilter
---

# Device Input (Sensors, Gamepad, Geolocation)

## Introduction

Motion sensors (accelerometer / gyroscope / magnetometer), gamepads, and
geolocation are **event-driven**: you attach a callback to a window-level event
filter and the framework invokes it when a new sample arrives. You do **not**
poll on a [Timer](timers.md). Inside the callback you read the current value
through an accessor on [`CallbackInfo`](callbacks.md). This is the same
"react to an event, then read the detail" shape as a key press or a mouse move.

This avoids the busy-loop a poll would cause and keeps the device's sample rate
in charge of how often your code runs.

## Motion sensors

Attach a callback to `WindowEventFilter::SensorChanged`; it fires when any
sensor reading advances. Read the value with `CallbackInfo::get_sensor_reading`:

```rust
let dom = Dom::create_body().with_callback(
    EventFilter::Window(WindowEventFilter::SensorChanged),
    state.clone(),
    on_sensor,
);

extern "C" fn on_sensor(mut data: RefAny, info: CallbackInfo) -> Update {
    if let OptionSensorReading::Some(r) = info.get_sensor_reading(SensorKind::Accelerometer) {
        if let Some(mut s) = data.downcast_mut::<MyState>() {
            s.tilt_x = r.x;  // m/s^2 on each axis; r.magnitude() for total
        }
    }
    Update::RefreshDom
}
```

There is also a node-level `HoverEventFilter::SensorChanged` mirror, for the
"redraw this node when a reading changes" pattern. Identical consecutive
readings do not re-fire (an idle sensor streaming the same value is silent).

## Gamepad

Attach to `WindowEventFilter::GamepadInput`; read state with
`CallbackInfo::get_primary_gamepad` (the first connected pad - the common
single-controller case) or `get_gamepad_state(id)`:

```rust
extern "C" fn on_gamepad(mut data: RefAny, info: CallbackInfo) -> Update {
    if let OptionGamepadState::Some(pad) = info.get_primary_gamepad() {
        let jump = pad.is_pressed(GamepadButton::South);   // A / Cross
        let move_x = pad.left_stick_x;                     // [-1, 1]
        // ... drive your game/menu state
    }
    Update::RefreshDom
}
```

`GamepadState` carries `connected`, a `buttons` bitset (read via `is_pressed`),
two sticks, and the triggers (`left_z` / `right_z`). A disconnect keeps the slot
with `connected = false` so you can observe it.

## Geolocation

Geolocation follows the WebAuthn-style permission model: mounting a
`Dom::create_geolocation_probe(...)` node *is* the permission request. Once a
backend delivers a fix you read it with `CallbackInfo::get_location_fix`:

```rust
// Mounting the probe asks the OS for location.
container.with_child(Dom::create_geolocation_probe(GeolocationProbeConfig {
    high_accuracy: true,
    background: false,
    max_accuracy_m: 0.0,
    min_interval_ms: 0,
}));

// Read the latest fix from any callback:
if let OptionLocationFix::Some(fix) = info.get_location_fix() {
    let (lat, lon) = (fix.latitude_deg, fix.longitude_deg);
}
```

`LocationFix` carries latitude/longitude, accuracy, altitude (+ accuracy),
heading, speed, and a timestamp. See the AzulMaps example for a live readout.

## Configurability

- **Sensors**: the manager keeps one reading per `SensorKind`; request rate /
  which sensors are platform-backend concerns (CoreMotion / Android
  `SensorManager`).
- **Gamepad**: deadzone + rumble are platform-backend features; the cross-
  platform surface gives you raw stick/trigger values and the button bitset.
- **Geolocation**: `GeolocationProbeConfig` configures accuracy, background
  delivery, max accuracy, and minimum interval at mount time.

## Testing without hardware

Every path above is exercised synthetically (no device) by
`layout/tests/synthetic_events.rs`: it injects readings/states/fixes through the
same `push_sensor_reading` / `push_gamepad_state` / `push_location_fix` channels
the real platform backends use, then asserts the manager folds them and the
`EventProvider` yields `SensorChanged` / `GamepadInput`. See
[e2e-testing](e2e-testing.md).

> Note: wacom **pen** events (`PenDown`/`Move`/`Up`) currently exist as event
> filters but are not yet dispatched (no `EventType` + routing), so they cannot
> be attached to or synthetically exercised yet - tracked for a follow-up that
> mirrors the sensor/gamepad wiring.

## See also

- [events](events.md) - the event-filter + dispatch model.
- [callbacks](callbacks.md) - `CallbackInfo` accessors + `RefAny`.
- [Realtime Media and Devices](realtime-media.md) - camera/mic capture.
