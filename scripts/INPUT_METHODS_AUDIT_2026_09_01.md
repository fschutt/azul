# Input methods audit — 2026-09-01

Full report (matrix + rationale): https://claude.ai/code/artifact/9fe65fa2-54c7-447d-a8de-d570df41b262

Ground truth: `core/src/events.rs`, `api.json`, `dll/src/desktop/shell2/{windows,macos,linux,android,ios,headless}`, `dll/src/web/`.

## Headline

Device coverage is better than expected. Pen pressure+tilt is real on **all six** shells; the Wayland
tablet-pad protocol is fully bound incl. `_dial_v2`; macOS reads `NSEvent.phase`/`momentumPhase`;
gamepads + sensors have live per-OS backends.

The actual hole is one layer up: **32 event filter variants in the public API that no producer ever emits.**

## A. Dead API surface (fix first — mostly wiring, no new platform code)

| id | what | note |
|----|------|------|
| D1 | **all 4 `ApplicationEventFilter` variants** | `matches_filter_phase` returns `false` for `EventFilter::Application(_)` ("will be implemented in future"). Structurally unreachable. `EventType::MonitorConnected/Disconnected` never constructed either. **Implement or delete before 1.0.** |
| D2 | `CompositionStart/Update/End` ×2 families = **6 dead** | Native IME is wired end-to-end (Win32 `WM_IME_*`, macOS `NSTextInputClient`, Wayland `zwp_text_input_v3`, X11 XIM + `linux/common/compose.rs`) but terminates in `apply_preedit_to_text_cache`. See the `// Phase 2: OnCompositionStart callback` TODO in `windows/mod.rs:5347`. |
| D3 | `ScrollStart`/`ScrollEnd` ×3 families = **6 dead** | Engine already classifies `ScrollInputSource::{TrackpadContinuous,TrackpadMomentum,TrackpadEnd,WheelDiscrete}`. Just never emitted. |
| D4 | `PenSqueeze`/`PenDoubleTap`/`PenHover` ×2 = **6 dead** | `PenHover` nearly free — Wayland already delivers `proximity_in` + `distance`. Squeeze needs `UIPencilInteraction` on iOS. |
| D5 | `MouseOut`, `FocusIn`/`FocusOut` ×2, focus `Copy`/`Cut`/`Paste`, `ComponentEventFilter::{DefaultAction,Selected}` | Clipboard bypasses the filters via `SystemChange::*` + `KeyboardShortcut` → apps cannot intercept a paste. The two Component variants are simply absent from `matches_component_filter`. |
| D6 | **12 of 23 `AccessibilityAction` variants unrouted** | Adapters map only `Default, Focus, Blur, Collapse, Expand, Increment, Decrement, Scroll{Up,Down,Left,Right}`. Missing incl. `SetTextSelection`, `ReplaceSelectedText` — the two a screen reader / voice control needs in a text field. |

`SystemText{Single,Double,Triple}Click` are `#[doc(hidden)]` + `is_system_internal()` — deliberate, leave them.

Also no-producer `EventType`s: `KeyPress, Change, Submit, Reset, Invalid, Play, Pause, Ended, TimeUpdate, VolumeChange, MediaError`, and `ContextMenu` (web JS only).

## B. Real device gaps

| id | gap | fix |
|----|-----|-----|
| G1 | **Back/forward mouse buttons pumped but unsubscribable.** `MouseButton::Other(u8)` exists, `WM_XBUTTON*`/`otherMouseDown:`/X11 8-9 all route in — but no filter variant and `MouseState` has only L/R/M. | `Back/ForwardMouseDown/Up` filters + `MouseState.other_down: u8` |
| G2 | **Touchpad gestures dead on Win/X11/Wayland.** Pinch/rotate *do* fire there — but only via the in-process detector fed by touch sessions, i.e. touchscreen only. A touchpad gives no touch points. | bind `zwp_pointer_gestures_v1`; XI 2.4 `XI_GesturePinch*`/`XI_GestureSwipe*`; `WM_GESTURE`. **No api.json change.** |
| G3 | **Wayland seat capped at v7** (`seat_version = version.min(7)`) → `axis_value120` (v8) and `axis_relative_direction` (v9) unreachable. macOS `isDirectionInvertedFromDevice` unread. | raise cap to 9, add both listeners, keep `axis_discrete` as v5-v7 fallback |
| G4 | **No raw/relative pointer.** `is_cursor_locked` is a flag with nothing behind it. No `WM_INPUT`, no `zwp_relative_pointer_v1`+`pointer_constraints`, no `XI_RawMotion`. | `RawMouseMotion` window filter + lock request path |
| G5 | **`TouchPoint` is `{id, position, force}`.** Wayland `touch_shape_handler` / `touch_orientation_handler` are registered with **empty bodies** — data arrives, discarded. Win reads only `pressure` from `POINTER_TOUCH_INFO`. | `+ major, minor, orientation_rad, tool_type: TouchToolType` |
| G6 | **Pen tail is ragged; Force Touch absent.** `tangential_pressure = 0.0` on X11/Wayland/iOS; `barrel_button_pressed` hardcoded `false` on macOS; `tool_id = 0` almost everywhere. No `pressureChangeWithEvent:`. | `PenToolType` (= the `zwp_tablet_tool_v2` type enum), hover distance, the 3 macOS responder methods |
| G7 | **`WacomPadState` models 2 of 5 pad controls.** Both Linux shells already bind ring **and** strip **and** dial **and** mode-switch. | `+ strip, strip_active, dial_delta, mode, mode_count`; rename → `TabletPadState` |
| G8 | **Gamepad input-only + mobile stubs.** `gamepad/android.rs` = 16 lines, `apple.rs` = 17. No rumble, pad touchpad, pad IMU, battery, paddles. | `+ battery, touchpad, imu`; `Misc1/Paddle1-4/Touchpad`; `GamepadRumble`. Copy SDL3's taxonomy. |
| G9 | **One fused 163-entry keycode table.** No physical/logical split, no `ModifiersChanged`, no lock state, no repeat flag. Bites non-US layouts. | add positional `PhysicalKey`, `ModifiersChanged`, `KeyboardState += {modifiers, locks, is_repeat}` |

## C. No representation at all

| id | class | verdict |
|----|-------|---------|
| N1 | **Directional (spatial) focus nav** — D-pad/TV/console/IVI/switch-access | **stub + implement.** `FocusTarget += Directional(FocusDirection)`. Geometric nearest-neighbour over the existing focusable set — *zero shell code*. |
| N2 | **Device identity / `PointerSource`** — only Pen/Gamepad/WacomPad carry `device_id`; `MouseState` is a single global → multi-seat inexpressible; trackball/trackpoint indistinguishable; can't tell touchpad-synthesised pointer from a real mouse (which is what G2 needs) | **do it.** `PointerSource {Unknown,Mouse,Touchpad,Trackball,Trackpoint,Touchscreen,Pen,Eraser}` = web `pointerType` ∩ `GdkInputSource` ∩ Qt `DeviceType` |
| N3 | **Dial / rotary encoder** — Surface Dial (`RadialController`), `zwp_tablet_pad_dial_v2` (already bound!), `SOURCE_ROTARY_ENCODER`, Digital Crown | **stub.** `DialState {device_id, delta_rad, detent_count, pressed, contact_position}` covers all four |
| N4 | **Generic HID / joystick** — flight sticks, wheels+pedals, 6-DOF SpaceMouse, foot pedals, Stream Deck, MIDI, barcode wedge | **stub.** `HidDevice {vid,pid,usage_page,usage,name}` + `HidReport {bytes}` escape hatch (= WebHID's shape) |
| N5 | **Haptic output**; **media keys as a channel** (`WM_APPCOMMAND` unhandled; MPRIS on Linux) | stub `Haptic::play`; media keys partly work already via `VirtualKeyCode::{PlayPause,NextTrack,...}` |
| N6 | **XR/spatial, voice, eye+head tracking** | **skip — no stub.** transient-pointer folds into N2 later; voice/gaze arrive as synthetic pointer + a11y actions, so the real work is D6 |

## D. Mobile shells

Not toys — both do multi-touch, stylus w/ tilt, 5 native gesture recognizers, lifecycle, full a11y bridge.
**Android** (1462 lines): `ToolType::Stylus`/`Eraser` + `Axis::Tilt`/`Orientation` work. Missing: `InputConnection`
(no soft-keyboard text at all — needs a JNI bridge), `WindowInsets`, handwriting delegation, predictive back, `FoldingFeature`.
**iOS** (1626 lines): Apple Pencil `force`/`altitudeAngle`/`azimuth`/`rollAngle` all read. Missing: `UITextInput`,
`UIPress`/`UIKeyCommand`, safe-area insets, `UIPencilInteraction`, `coalescedTouches`/`predictedTouches`, `indirectPointer`.

Highest-value item on both = **text input**, and it pairs with D2 (one project, not two).

## E. Testability constraint

`HeadlessEvent` has 12 variants; scroll is **always** `WheelDiscrete`. No pen/gamepad/sensor/gesture injection
(only `inject_touch_points`). **D3 cannot be regression-tested today.** Add a `HeadlessEvent` variant in the same
commit as anything above.

## Work order

- **A.** Batch A above — make the existing API tell the truth. Mostly wiring, biggest behaviour delta per line.
- **B.** Shell wiring, no API change: G2, G3, G6 (macOS responders), G5 (Wayland stubs).
- **C.** Small api.json deltas: G1, G5, G6, G7, G8, `SensorKind` → ~12 incl. `HingeAngle`.
- **D.** New capability: N1, N2, G4, G9.
- **E.** Mobile parity: text input, insets, `UIPencilInteraction`, coalesced/predicted, real gamepad backends.
- **Stub only:** N3, N4, N5. **Don't:** N6.
