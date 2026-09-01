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
