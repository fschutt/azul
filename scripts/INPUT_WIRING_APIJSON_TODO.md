# api.json deltas owed — run `azul-doc autofix` at fix-up time

Never hand-edit api.json. Each entry names the Rust type that is the source of truth.

## From step 3c (composition events)

- `azul_core::events::CompositionEventData` — new struct `{ data: String, cursor_begin: usize,
  cursor_end: usize }`. Needs a C-ABI representation (`AzString` + two `usize`).
- `azul_layout::managers::text_edit::CompositionPhase` — new enum `{ Start, Update, End }`.
- `CallbackInfo::get_composition_text() -> OptionAzString`
- `CallbackInfo::get_composition_cursor() -> Option<(usize, usize)>` — the tuple needs a named struct for
  the C ABI; propose `CompositionCursor { begin: usize, end: usize }` rather than a tuple.
- `CallbackInfo::is_composing() -> bool`
- `EventData::Composition(CompositionEventData)` — appended at the END of `EventData` for ABI stability.
  `EventData` is Rust-internal today, so this may need no api.json entry at all; confirm before adding.

## From step 2c (scroll phase)

- `azul_layout::managers::scroll_state::ScrollPhaseTransition` — new enum `{ Started, Ended }`. Internal to
  the manager; only needs exposing if `CallbackInfo` grows a phase accessor.

## From step 5a/5c (new EventTypes)

`EventType` is Rust-internal (not in api.json), so these five may need no entry at all — confirm before
adding. They are appended at the END of the enum, after `DeviceDisconnected`:
`PenSqueeze`, `PenDoubleTap`, `PenHover`, `DefaultAction`, `Selected`.

The FILTER variants they serve (`HoverEventFilter::PenSqueeze/PenDoubleTap/PenHover`,
`WindowEventFilter::` the same three, `ComponentEventFilter::DefaultAction/Selected`) are already in
api.json and unchanged — this arc gave them producers, it did not add them.

## From step 8b (touch contact geometry)

- `azul_core::window::TouchToolType` — new `#[repr(C)]` enum
  `{ Unknown, Finger, Stylus, Eraser, Palm, Mouse }`.
- `azul_core::window::TouchPoint` — four new fields appended:
  `major: f32`, `minor: f32`, `orientation_rad: f32`, `tool_type: TouchToolType`.
  Already in api.json as `window.TouchPoint`, so this is a field-level update, not a new class.

## From step 8a (thumb buttons)

- `azul_core::window::MouseState` — new field `other_down: u8` (bitmask), plus `back_down()` /
  `forward_down()` accessors. Already in api.json as `dom.MouseState`; field-level update.
- `HoverEventFilter`, `FocusEventFilter`, `WindowEventFilter` — four variants appended to EACH:
  `BackMouseDown`, `BackMouseUp`, `ForwardMouseDown`, `ForwardMouseUp`. All three enums are in api.json
  (`css.*` and `window.WindowEventFilter`), so these are variant additions at the end.
- `azul_core::events::{MOUSE_BUTTON_BACK, MOUSE_BUTTON_FORWARD, MOUSE_OTHER_MASK_BACK,
  MOUSE_OTHER_MASK_FORWARD}` — consts; expose only if the C API needs to construct the mask itself.

## From step 8c/8d (pen + pad)

- `TabletToolKind` — five variants APPENDED after `Unknown`: `Brush`, `Pencil`, `Airbrush`, `Mouse`, `Lens`.
  Already in api.json from #450 (`gesture.TabletToolKind`); variant addition only.
- `PenState` — two fields appended: `hover_distance: f32`, `tool_kind: TabletToolKind`.
- `WacomPadState` — five fields appended: `strip: f32`, `strip_active: bool`, `dial_delta: f32`,
  `mode: u32`, `mode_count: u32`.
- `CallbackInfo::get_pen_hover_distance() -> OptionF32`, `CallbackInfo::get_pen_tool_kind() ->
  OptionTabletToolKind` (the Option wrapper may need generating).
- RENAME owed, not done: `WacomPadState` -> `TabletPadState`, with an alias for the old name. Needs autofix.

## From step 8e/8f (sensors + gamepad)

- `SensorKind` — eight variants APPENDED: `RotationVector`, `Gravity`, `LinearAcceleration`,
  `AmbientLight`, `Proximity`, `Barometer`, `StepCounter`, `HingeAngle`.
- `GamepadButton` — six variants APPENDED: `Misc1`, `Paddle1`..`Paddle4`, `Touchpad`.
  ⚠ `GamepadState::buttons` is a bitset indexed by DISCRIMINANT, so these MUST stay at the end.
- `GamepadState` — ten fields appended: `battery`, `touchpad_x/y/active`, `gyro_x/y/z`, `accel_x/y/z`.
  `battery` uses -1.0 as "not reported" rather than an Option, because the struct crosses the C ABI.
- `GamepadState` gained a `Default` impl; check whether api.json needs it declared.

## From step 9a (spatial navigation)

- `azul_core::callbacks::FocusDirection` — new `#[repr(C)]` enum `{ Up, Down, Left, Right }`.
- `azul_core::callbacks::FocusTarget` — one variant APPENDED: `Directional(FocusDirection)`.
  Already in api.json as `dom.FocusTarget`; variant addition at the end.

## From step 9b (pointer source + device identity)

- `azul_core::events::PointerSource` — new `#[repr(C)]` enum
  `{ Unknown, Mouse, Touchpad, Trackball, Trackpoint, Touchscreen, Pen, Eraser }`.
- `MouseState` — two fields appended: `pointer_source: PointerSource`, `pointer_device_id: u64`.
- `MouseEventData` — two fields appended: `source: PointerSource`, `device_id: u64`.
  `KeyboardEventData` — one appended: `device_id: u64`. Both are Rust-internal today; confirm.
- `CallbackInfo::get_pointer_source()`, `CallbackInfo::get_pointer_device_id()`.
- `MouseEventData` and `KeyboardEventData` gained `Default` impls.

## From step 9d (raw pointer motion)

- `azul_core::events::RawMotionEventData` — new struct `{ dx: f64, dy: f64, device_id: u64 }`.
- `EventData::RawMotion(RawMotionEventData)` — appended at the END.
- `EventType::RawMouseMotion` — appended at the END.
- `WindowEventFilter::RawMouseMotion` — appended at the END. In api.json as `window.WindowEventFilter`.
- `CallbackInfo::get_raw_mouse_motion() -> Option<(f64, f64)>` — the tuple needs a named struct for the
  C ABI; propose reusing `RawMotionEventData` as the return type instead.

## From step 9e (keyboard)

- `azul_core::window::PhysicalKey` — new `#[repr(C)]` enum, ~130 positional variants (W3C `code` names)
  plus `Unidentified`. Needs `OptionPhysicalKey` generating.
- `azul_core::window::KeyLocks` — new `#[repr(C)]` struct `{ caps_lock, num_lock, scroll_lock }`.
- `KeyboardState` — four fields appended: `modifiers: KeyModifiers`, `locks: KeyLocks`,
  `is_repeat: bool`, `current_physical_key: OptionPhysicalKey`. In api.json as `dom.KeyboardState`.
- `EventType::ModifiersChanged` and `WindowEventFilter::ModifiersChanged` — appended at the END.
- `CallbackInfo::{get_key_modifiers, get_key_locks, get_physical_key, is_key_repeat}`.
- ⚠ `KeyModifiers` is currently Rust-internal (`azul_core::events`); exposing `get_key_modifiers` means
  it needs an api.json entry too.

## From step 9f/9g (HID + haptics)

- `azul_core::hid::{HidDevice, HidReport}` — new `#[repr(C)]` structs. `HidReport.bytes` is a `U8Vec`.
- `azul_core::haptics::{HapticPattern, HapticTarget, HapticRequest}` — new `#[repr(C)]` enums/struct.
- `EventType::HidReport` + `WindowEventFilter::HidReport` — appended at the END.
- `CallbackInfo::{get_hid_reports, get_hid_devices, play_haptic}`. The two slice returns need Vec
  wrappers for the C ABI (`HidReportVec`, `HidDeviceVec`).

## From step 10a (mobile text input)

- `CallbackInfo::request_soft_keyboard(visible: bool)`.
- `TextEditManager.pending_soft_keyboard: Option<bool>` — internal, likely no api.json entry.

## From step 10c/10e (insets + touch sampling)

- `azul_css::system::SafeAreaInsets` — one field appended: `keyboard: OptionPixelValue`.
- `azul_core::window::TouchState` — two fields appended: `coalesced_points: TouchPointVec`,
  `predicted_points: TouchPointVec`.
- `CallbackInfo::{get_keyboard_inset, get_coalesced_touches, get_predicted_touches}`.
  The two slice returns need `TouchPointVec` for the C ABI rather than `&[TouchPoint]`.
