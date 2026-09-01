# Input wiring — work queue

Branch `feat/input-event-wiring`, stacked on **PR #450** (`fix/tablet-and-clipboard-linux`, 28 commits)
rebased onto `origin/master` @ fcef148b2. Spec: `scripts/INPUT_METHODS_AUDIT_2026_09_01.md`.

## Absorbed from PR #450 (Linux-tested — do NOT redo)

- `PenState.report_rate_hz` — measured EMA of the sample interval; no protocol carries a nominal rate.
- `TabletDeviceInfo` + `TabletToolKind { Unknown, Stylus, Eraser, Pad, Touch }` + `TabletDeviceInfoVec`,
  `CallbackInfo::get_tablet_devices()`, backend-populated at window init and on hotplug. **Already in api.json.**
- Wayland: `handle_tablet_frame` drives the pointer pipeline (cursor, hover hit-test, tip=left, barrel=right);
  `tool_button`/`hardware_serial`/`capability`/`slider` listeners were noops and now feed `PenState`; per-tool
  identity applied on `proximity_in`; `proximity_out` clears pen state and releases synthesized buttons.
- X11: sparse XI2 valuators reuse previous pen state (absent axis = unchanged, not zero); tip contact tracks the
  tip BUTTON; barrel buttons reach `barrel_button_pressed`; `device_id` is the slave sourceid; `FocusOut` resets
  pad + pen state.
- So `tangential_pressure`, `barrel_button_pressed`, `tool_id` and `device_id` are **no longer ragged on
  Wayland/X11**. macOS and Win32 are untouched by #450.

⚠ **Interaction with item 1a:** #450's Wayland bridge exists precisely *because* pen events dispatch to nothing
today — it synthesizes `Mouse*` so something reacts. Once 1a lands, real `Pen*` events start dispatching too.
Check for double-handling (a node subscribed to both `MouseDown` and `PenDown` will now get both) and decide
whether the bridge should suppress its synthetic mouse events when a `Pen*` subscriber exists.

## Rules for this arc

- **NOTHING is removed from api.json.** Unemitted variants get wired, never deleted. (User ruling 2026-09-01.)
- **DO NOT COMPILE while iterating.** No `cargo check`, no `cargo build`, no `cargo test`. We fix up at the END.
- Commit after each item, even if it doesn't build. Message: `wip(input): <item id> <what>`.
- api.json changes go via `azul-doc autofix` only — never hand-edited. If autofix can't run without a compile,
  record the intended delta in `scripts/INPUT_WIRING_APIJSON_TODO.md` and move on.
- Every wiring fix that adds a dispatchable event should also add a `HeadlessEvent` variant so it can be tested
  at the end.
- **Four layers must agree** for a filter to fire: (1) shell constructs `EventType`, (2) `event_type_to_filters`
  returns the filter, (3) `matches_*_filter` accepts the pair, (4) `matches_filter_phase` passes the family.
  Touch all four or the work is invisible.

## Status legend

`[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (note why)

---

## Step 0 — ratchet

- [x] 0a `core/src/events_test.rs`: extend `event_type_to_filters_never_panics_and_stays_synced_with_the_hover_matcher`
      from 2 layers × Hover to 4 layers × {Hover, Focus, Window}. Assert planning emits the filter, matcher accepts,
      phase gate passes. Keep `KNOWN_DESYNC` as the subset allow-list; entries get deleted as items below land.
- [x] 0b Prune the 6 stale `KNOWN_DESYNC` entries (`MouseOut`, `FocusIn`, `FocusOut`, `Composition{Start,Update,End}`)
      — their matcher arms exist at `events.rs:1438-1443`, so the entries protect nothing.

## Step 1 — C1: planning omissions (14 variants, zero shell work)

- [x] 1a `event_type_to_filters_legacy_hint`: add `E::PenDown`, `E::PenMove`, `E::PenUp`, `E::PenEnter`, `E::PenLeave`
      arms emitting Hover + Focus (Down/Move/Up only) + Window filters.
- [x] 1b Same fn: add `E::DocumentEdit => vec![EF::Focus(F::DocumentEdit)]`.

### Found while doing 1a (same bug class, not originally listed)

- [x] 1c `matches_focus_filter` had **no Pen arms at all**, though `FocusEventFilter` has carried
      `PenDown`/`PenMove`/`PenUp` since it was introduced. Planning naming the filter would not have been enough.
- [x] 1d `E::TouchStart/Move/End/Cancel` planned only the Hover half, though `WindowEventFilter` owns all four
      with matching same-name arms in `matches_window_filter`. A window-level touch listener never fired.

## Step 2 — C2: planning de-sync

- [x] 2a Split `E::Scroll | E::ScrollStart | E::ScrollEnd => vec![EF::Hover(H::Scroll)]` into three arms, each
      emitting its own Hover + Focus + Window variant.
- [x] 2b Matcher arms: `(RightMouseDown, EventType::ContextMenu)`, `(TextInput, EventType::KeyPress)`,
      `(TextInput, EventType::Change)`.
- [x] 2c Emit `ScrollStart`/`ScrollEnd` from the `ScrollInputSource` transitions the physics timer already computes
      (`layout/src/managers/scroll_state.rs` + callers).
- [x] 2d Emit `ContextMenu` from right-button-up, the Menu/Apps key, and Shift+F10 on all four desktop shells.

## Step 3 — C3: missing producers

- [x] 3a Emit `EventType::MouseOut` alongside every `MouseLeave` site.
- [x] 3b Emit `EventType::FocusIn`/`FocusOut` alongside every `Focus`/`Blur` site.
- [x] 3c `CompositionEventData { data, cursor_begin, cursor_end }` + `EventData::Composition` variant +
      `CallbackInfo::get_composition_*` accessors.
- [x] 3d Emit `Composition*` at the IME sites: Win32 `WM_IME_STARTCOMPOSITION`/`COMPOSITION`/`ENDCOMPOSITION`,
      macOS `setMarkedText:`/`unmarkText`/`insertText:`, Wayland `preedit_string`/`commit_string`/`done`, X11 XIM.
- [x] 3e ALREADY DONE UPSTREAM (landed after the audit was written — verified at `common/event.rs:9114-9175`: the three SystemChanges are deferred past callback dispatch, the events are constructed and propagated, and `clip_prevented` gates `apply_system_change`). No change needed. Original item: dispatch `EventType::Copy`/`Cut`/`Paste` to the focused node BEFORE pushing
      `SystemChange::{CopyToClipboard, CutToClipboard, PasteFromClipboard}`; the existing
      `post_callback_filter_system_changes(prevent_default, …)` gate then makes them interceptable.

- [ ] 2c-iv `settle_scroll_gesture()` still has no caller. A discrete wheel has no end-of-gesture signal, so
      a `WheelDiscrete` gesture never fires `ScrollEnd`. The right place is the terminate branch of
      `scroll_physics_timer_callback` (`layout/src/scroll_timer.rs:1187`), but that callback only holds a
      downcast `ScrollPhysicsState` and does not obviously reach the `ScrollManager` — find the seam rather
      than guessing. Trackpad gestures are unaffected: `TrackpadEnd` closes those.

- [ ] 4f-i X11 RandR monitor hotplug: `XRRSelectInput(RRScreenChangeNotifyMask)` plus a count diff, the
      same shape as Win32/macOS. libXrandr is not currently dlopened at all, so this needs a loader entry
      first — unlike XI2, which was already loaded.

### Follow-ups opened by 10f

- [ ] 10f-i `com.azul.gamepad.AzulGamepad` does not exist. It owns the `InputManager.InputDeviceListener`
      and the source filtering — `InputDevice.getSources() & SOURCE_GAMEPAD/SOURCE_JOYSTICK` is Java-side
      API with no NDK equivalent — and calls the three `native*` entry points. Same class family as
      10a-i's `NativeTextBridge`.
- [ ] 10f-ii iOS `GCMotion` (pad gyro/accel) and `GCDeviceBattery` are readable but not read, because no
      platform fills those `GamepadState` fields yet (8f-i). Do them together so iOS is not the only
      backend reporting them.

### Follow-ups opened by 10e

- [ ] 10e-i Web equivalents: `PointerEvent.getCoalescedEvents()` and `getPredictedEvents()`. The wasm
      loader binds only `pointerdown` today (see the audit's web column), so this needs the pointer-event
      migration first. Windows has `GetPointerFrameInfoHistory` for the coalesced half; Wayland and X11
      have no equivalent — a compositor delivers every sample, so there is nothing to un-coalesce.

### Follow-ups opened by 10d

- [ ] 10d-i The `UIPencilInteraction` object is not CREATED or attached to the view. The delegate methods
      exist and are registered, but something has to `[[UIPencilInteraction alloc] init]`, set its
      delegate to the view and call `addInteraction:` — and gate it on iOS 12.1+ / 17.5+ respectively,
      since `didReceiveSqueeze:` does not exist on older SDKs.

### Follow-ups opened by 10c

- [ ] 10c-i iOS does not fill `keyboard`: it needs `UIKeyboardWillChangeFrameNotification` observed and
      `UIKeyboardFrameEndUserInfoKey` intersected with the view's frame (the raw frame is in screen
      coordinates and can be wider than the window on iPad split view).
- [ ] 10c-ii The Java side of `nativeOnWindowInsets` does not exist — `View.setOnApplyWindowInsetsListener`
      reading `Type.systemBars() | Type.displayCutout()` and `Type.ime()`. Same class as 10a-i.
- [ ] 10c-iii Nothing CONSUMES the keyboard inset in layout. `env(safe-area-inset-*)` already resolves from
      `SafeAreaInsets`; a matching `env(keyboard-inset-bottom)` would let CSS react without app code.

### Follow-ups opened by 10b

- [ ] 10b-i Full `UITextInput` conformance — ~25 methods over `UITextPosition` / `UITextRange` object
      graphs — buys marked text (a live preedit rendered by the app rather than only the committed
      result), the edit menu, and dictation. ⚠ Do NOT half-implement it: UIKit probes for the protocol and
      then calls methods that must return real `UITextPosition` objects, so returning nil CRASHES rather
      than degrades. That is why 10b shipped `UIKeyInput` instead.
- [ ] 10b-ii Nothing calls `becomeFirstResponder` on the view, so the keyboard never actually appears
      even though the view can now accept it. Should be driven by `take_soft_keyboard_request()` (10a-ii)
      — the same drain, on both mobile shells.
- [ ] 10b-iii `UIPress` modifier flags (`key.modifierFlags`) are not read, so Cmd-key shortcuts from a
      Magic Keyboard do not reach the accelerator path.

### Follow-ups opened by 10a

- [ ] 10a-i The Java side does not exist. `com.azul.text.NativeTextBridge` must own a
      `BaseInputConnection` on the activity's view, override `onCreateInputConnection` to return it, and
      forward `commitText` / `setComposingText` / `finishComposingText` / `deleteSurroundingText` to the
      five `native*` entry points added here. It also has to call `InputMethodManager.showSoftInput` /
      `hideSoftInputFromWindow` for the keyboard request. Mirror `NativeGestureBridge.java`.
- [ ] 10a-ii Nothing drains `take_soft_keyboard_request()` yet — the Android shell has to poll it each
      pass and call across to `NativeTextBridge`.
- [ ] 10a-iii `EditorInfo` hints (`inputType`, `imeOptions`) are how Android decides to show a numeric pad
      or a "Go" key instead of Enter. Needs an input-purpose attribute on the DOM node first, which is a
      design question, not plumbing.

### Follow-ups opened by 9h

- [ ] 9h-i macOS and Linux equivalents. macOS routes media keys as
      `NSEventTypeSystemDefined` subtype 8 (`NX_SYSDEFINED`), which needs an event-tap or the
      `MediaPlayer` framework's remote-command centre; Linux delivers them as ordinary X11/Wayland keysyms
      (`XF86AudioPlay` and friends) IF the desktop has not grabbed them, and as MPRIS over D-Bus if it has.
      Only the Win32 half is done.

### Follow-ups opened by 9f/9g

- [ ] 9f-i No backend enumerates HID devices or pushes reports. Win32 `WM_INPUT` with a HID usage page
      registration (the same `RegisterRawInputDevices` call 9d-i needs — do them together), Linux
      `/dev/hidraw*` or libudev, macOS `IOHIDManager`.
- [ ] 9g-i No backend plays haptics. macOS `NSHapticFeedbackManager.defaultPerformer` (trackpad only),
      Android `performHapticFeedback`, Win32 `SimpleHapticsController` via WinRT, gamepad rumble via SDL
      or raw HID output reports. The shell also has to DRAIN `haptic_manager.take_pending()` each pass.

### Follow-ups opened by 9e

- [ ] 9e-i No shell fills `KeyboardState.modifiers`, `.locks`, `.is_repeat` or `.current_physical_key`, so
      `ModifiersChanged` cannot fire yet and the accessors read defaults. Each backend has the data:
      Win32 `GetKeyboardState` (already called on focus gain) plus `VK_CAPITAL`/`VK_NUMLOCK` toggle bits and
      the `lParam` bit 30 repeat flag; macOS `NSEvent.modifierFlags` including `NSEventModifierFlagCapsLock`,
      and `isARepeat`; Wayland `wl_keyboard.modifiers` carries `mods_locked` and `repeat_info` gives the
      rate; X11 `XkbStateNotify` plus the `XKeyEvent.state` lock bits.
- [ ] 9e-ii `PhysicalKey` needs a per-platform scancode -> position map. The `ScanCode` is already captured
      on every backend, so this is a table per platform, not new plumbing.

### Follow-ups opened by 9d

- [ ] 9d-i Raw-motion producers for the other backends: Win32 `WM_INPUT` + `RegisterRawInputDevices`
      (`RIDEV_INPUTSINK` decides whether it arrives unfocused), Wayland `zwp_relative_pointer_v1` (a new
      protocol binding, same shape as the pointer-gestures work in 7a), web `movementX`/`movementY`.
- [ ] 9d-ii Nothing SETS `is_cursor_locked` — there is no request path. The flag gates raw delivery
      correctly, but an app cannot turn it on, so raw motion cannot actually be reached yet. Needs
      `CallbackInfo::set_pointer_lock(bool)` plus the per-platform grab: Win32 `ClipCursor` + hide,
      Wayland `zwp_pointer_constraints_v1.lock_pointer`, X11 `XGrabPointer`, web `requestPointerLock`.
      **9d-ii is the item that makes 9d usable — do it before the other producers.**

### Follow-ups opened by 9c

- [ ] 9c-i Other dial backends: Win32 `RadialController` (needs WinRT interop, and is the only one that
      reports `contact_position`), Android `SOURCE_ROTARY_ENCODER` for a Wear crown, Apple Digital Crown.
      The type and the accessor are in place; each is a backend, not a design question.
- [ ] 9c-ii No `DialRotate` / `DialClick` FILTERS yet — the dial is readable via
      `CallbackInfo::get_dial_state()` but not subscribable. Adding them means the full four layers plus
      two `EventType` variants appended at the end.

### Follow-ups opened by 9b

- [ ] 9b-i `pointer_source` is fed only on Wayland (from `axis_source` and the tablet bridge). Win32 can
      answer it from `GetPointerType` / `WM_POINTERDEVICE`, X11 from the XI2 slave device name or its
      valuator set, macOS from `NSEvent.subtype` + `hasPreciseScrollingDeltas`. All currently leave it
      `Unknown`, which is honest but means item G2's "tell a touchpad from a mouse" only works on Wayland.
- [ ] 9b-ii `MouseState` is still a single global, so multi-seat remains inexpressible — `device_id` now
      travels on the EVENT but the state does not fan out per seat. That is a bigger change than this arc.

### Follow-ups opened by 9a

- [ ] 9a-i Nothing DRIVES `FocusTarget::Directional` yet — it is reachable from the API
      (`CallbackInfo::set_focus`) but no key is bound to it. Arrow keys currently scroll, and stealing them
      unconditionally would break every scroll container, so this needs the CSS opt-out the audit describes
      (a `-azul-spatial-navigation: auto|none` or equivalent) before a default binding is safe. A D-pad or
      TV remote has no such conflict and could be bound now.
- [ ] 9a-ii `DefaultAction` has `FocusNext`/`FocusPrevious`/`FocusFirst`/`FocusLast` but no directional
      variants; add four once 9a-i decides the trigger.

### Follow-ups opened by 8e/8f

- [ ] 8f-i The new `GamepadState` fields (battery, touchpad, gyro/accel) are modelled but no backend fills
      them. gilrs exposes battery via `Gamepad::power_info`; the pad touchpad and IMU need either SDL or
      raw HID, since gilrs does not surface them. Until then they read as their "not reported" defaults,
      which is honest but inert.
- [ ] 8e-i The eight new `SensorKind` variants have no backend producing them. `dll/src/desktop/extra/
      sensors/{android,apple,linux,windows}.rs` fill only the original three; `HingeAngle` in particular
      needs Android `TYPE_HINGE_ANGLE` and is the one with a layout consequence.

### Follow-ups opened by 8c/8d

- [x] 8d-i `WacomPadState.dial_delta` is modelled but has NO producer: `get_tablet_pad_dial_v2_interface()`
      exists (from #450) yet no `zwp_tablet_pad_dial_v2` listener is ever registered, and the pad-group
      listener has no `dial` member. Bind it the way ring/strip are bound. This is also the field a future
      `DialState` (item 9c) should read, so do 8d-i first.
- [ ] 8d-ii The rename `WacomPadState` -> `TabletPadState` is NOT done — the protocol is not Wacom-specific
      and the type is in api.json, so the rename needs `azul-doc autofix` and a deprecation alias rather
      than a sed. Deferred to fix-up.
- [ ] 8c-i `PenState.tool_kind` is fed only on Wayland. Win32 can distinguish pen vs eraser from
      `PEN_FLAG_ERASER` and X11 from the device name; both currently leave it `Unknown`.

### Follow-ups opened by 7c

- [ ] 7c-i Windows TOUCHPAD pinch is not reachable through `WM_GESTURE`. A precision touchpad reports pan
      and zoom as `WM_MOUSEWHEEL` / `WM_MOUSEHWHEEL` (zoom as Ctrl+wheel, the convention browsers zoom on),
      and the raw finger geometry is only available through Direct Manipulation
      (`IDirectManipulationViewport`). Decide whether to synthesize a pinch from Ctrl+wheel — which is what
      most apps actually do — or take the DirectManipulation dependency.
- [ ] 7c-ii `screen_to_logical_client` is referenced by the `WM_GESTURE` arm and may not exist under that
      name; `ptsLocation` is in SCREEN coordinates while every other gesture path reports client-space.
      Reconcile at fix-up.

### Follow-ups opened by 5b

- [ ] 5b-i `PenState` has no `hover_distance` field yet (that is item 8c), so Wayland's `tool_distance` is
      captured into `TabletPenPending.distance` and stops there. Wire it through once 8c lands.
- [x] 5b-ii DONE by 10d — `PenSqueeze` / `PenDoubleTap` now have a producer. Original: they had no producer on any platform — they are `UIPencilInteraction`
      only, which is item 10d. The EventType, planning and matcher arms are in place waiting for it.

### Follow-ups opened by 4b/4c

- [x] 4c-i Register `DeviceEventManager` on `LayoutWindow` (field + `new()` + the destructure at
      `window.rs:830`) and add it to the `EventProvider` slice — same registration debt as 2c-ii and 3d-i.
      These three should land together.

### Follow-ups opened by 3c/3d

- [x] 3d-i Register `TextEditManager` in the `&[&dyn EventProvider]` slice, and clear
      `pending_composition` after the drain (same shape as 2c-ii/iii).
- [ ] 3d-ii X11 has no separate commit path — `Xutf8LookupString` returns committed text directly and the
      preedit is only ever set from the XIM callback, so `CompositionEnd` there currently comes from the
      cancel path with empty text. Confirm whether XIM preedit callbacks are installed at all
      (`XIMPreeditNothing` means the IM server draws its own window and the client sees no preedit).

### Follow-ups opened by 2c

- [x] 2c-i `note_scroll_phase` is called from `record_scroll_from_hit_test`, the single entry point every
      backend already funnels through with an already-classified `source` — so macOS, Wayland, X11 and Win32
      are all covered by one call and cannot drift on what starts a gesture. Original wording: call `ScrollManager::note_scroll_phase(source)` from every platform scroll path
      (macOS `scrollWheel:`, Wayland `pointer_axis*`, X11 scroll, Win32 `WM_MOUSEWHEEL`) and
      `settle_scroll_gesture()` from the physics timer when velocity reaches zero — a discrete wheel has no
      end-of-gesture signal, so without the settle call a `WheelDiscrete` gesture never closes.
- [x] 2c-ii Register `ScrollManager` in the `&[&dyn EventProvider]` slice passed to
      `determine_events_from_managers`, or the impl is never polled.
- [x] 2c-iii Clear `pending_scroll_phase` after the drain (`get_pending_events` takes `&self`; the other
      managers use a `pending_event` flag cleared elsewhere in the pass — match whatever they do).

## Step 4 — C4: open the Application phase

- [x] 4a `matches_filter_phase`: replace the `EventFilter::Application(_) => false` arm with a real
      `matches_application_filter(f, event, phase)`; write that fn.
- [x] 4b Producer: gilrs gamepad connect/disconnect → `EventType::DeviceConnected`/`DeviceDisconnected`
      (already pumped via `capability_pump`, all four desktops).
- [x] 4c Producer: Wayland — `wl_registry.global_remove` for `wl_output` → monitor events;
      `wl_seat.capabilities` + `zwp_tablet_seat_v2` add/remove → device events. All handlers already exist.
- [x] 4d Producer: Win32 `WM_DISPLAYCHANGE` monitor-list diff; add `WM_DEVICECHANGE` handling.
- [x] 4e Producer: macOS `windowDidChangeScreen:` diff + observe `NSApplicationDidChangeScreenParameters`.
- [x] 4f Producer: X11 (XI_HierarchyChanged done; RandR monitor hotplug still owed — see 4f-i) RandR `XRRSelectInput` + `XI_HierarchyChanged`.

## Step 5 — C5: new EventTypes and missing match arms

- [x] 5a Append `EventType::PenSqueeze`, `PenDoubleTap`, `PenHover` at the END of the enum (ABI stability, same
      convention `Copy`/`Cut`/`Paste`/`DocumentEdit` used). Add planning arms + Hover/Window matcher arms.
- [x] 5b `PenHover` producers: Wayland `proximity_in`/`distance`, Win32 `POINTER_FLAG_INRANGE`, Android
      `ACTION_HOVER_MOVE` (already handled), macOS `NSEventSubtype::TabletProximity` on the existing mouse path.
- [x] 5c `matches_component_filter`: add the missing `DefaultAction` and `Selected` arms.

## Step 6 — C7: accessibility actions

- [x] 6a NO WORK NEEDED — the audit was WRONG on this point. There is a shared
      `azul_layout::managers::a11y::map_accesskit_action` that every platform adapter calls through
      `poll_action`, and it is exhaustive over `accesskit::Action` (no `_` arm). `LayoutWindow::
      process_accessibility_action` likewise handles all 23 `AccessibilityAction` variants. The audit's
      "12 of 23 have no adapter arm" came from reading the per-platform files, which delegate. Original item:
      route the 12 unmapped `AccessibilityAction` variants in the accesskit adapters
      (`{windows,linux/x11,macos,android,ios}/accessibility.rs`) to the engine fns that already exist:
      `ScrollIntoView`→`scroll_node_into_view`, `ScrollToPoint`/`SetScrollOffset`→`scroll_to`/`scroll_to_unclamped`,
      `SetTextSelection`→`TextOpSetSelection`, `ReplaceSelectedText`→text-edit manager,
      `ShowContextMenu`→`open_menu_for_node`, `Show`/`HideTooltip`→`show`/`hide_tooltip_from_callback`,
      `SetValue`/`SetNumericValue`→widget setters, `SetSequentialFocusNavigationStartingPoint`→focus engine,
      `CustomAction`→callback passthrough.

## Step 7 — shell wiring, no API change

- [x] 7a Wayland: bind `zwp_pointer_gestures_v1` (swipe, pinch, hold) → existing pinch/rotate/swipe filters.
- [x] 7b X11: XInput 2.4 `XI_GesturePinch*` / `XI_GestureSwipe*`.
- [x] 7c Windows: TOUCHSCREEN pinch/rotate via WM_GESTURE. The touchpad half is NOT this API — see 7c-i. Original item: touchpad pinch — handle `WM_GESTURE`, or recognise from the pointer stream.
- [x] 7d Wayland: raise `seat_version` cap from `min(7)` to 9; add `axis_value120` and `axis_relative_direction`
      listeners; keep `axis_discrete` as the v5–v7 fallback.
- [x] 7e macOS: read `isDirectionInvertedFromDevice` (natural-scroll flag).
- [x] 7f macOS: add `pressureChangeWithEvent:` (Force Touch `stage` / `stageTransition`).
- [x] 7g Wayland: fill the empty `touch_shape_handler` / `touch_orientation_handler` bodies (needs 8b first).
- [x] 7h NO WORK NEEDED — the audit was wrong. `WM_MOUSEWHEEL` already divides as `delta as f32 / WHEEL_DELTA as f32`, so the fractional remainder is preserved; there was no truncation to fix.

## Step 8 — api.json deltas (record intent; run `azul-doc autofix` at fix-up time)

- [x] 8a `MouseState.other_down: u8` + `Back`/`Forward` `MouseDown`/`MouseUp` on Hover/Focus/Window; extend the
      `button_specific_down` helper. All four layers.
- [x] 8b `TouchPoint += { major, minor, orientation_rad, tool_type }` + `TouchToolType { Unknown, Finger, Stylus,
      Eraser, Palm, Mouse }`.
- [x] 8c `PenState.hover_distance` (proximity Z). The ragged tail is DONE on Wayland/X11 via #450 — what remains
      is macOS + Win32 parity. Do NOT invent `PenToolType`: #450 shipped `TabletToolKind { Unknown, Stylus,
      Eraser, Pad, Touch }`; either widen that toward the 8-value `zwp_tablet_tool_v2` set (Brush, Pencil,
      Airbrush, Lens) or leave it — but reuse it, don't duplicate it.
- [x] 8d (struct grown + strip/mode wired; dial has no producer yet — see 8d-i) `WacomPadState` → `TabletPadState` + `{ strip, strip_active, dial_delta, mode, mode_count }`.
      #450 left this struct at 2 of 5 pad controls, so it is still fully open. `TabletToolKind::Pad` and
      `TabletDeviceInfo.button_count` now exist to hang it off.
- [x] 8e `SensorKind += RotationVector, Gravity, LinearAcceleration, AmbientLight, Proximity, Barometer,
      StepCounter, HingeAngle`.
- [x] 8f (state + buttons done; `GamepadRumble` is OUTPUT and is 9g's haptics item, and the backends do not yet fill the new fields — see 8f-i) `GamepadState += { battery, touchpad, imu }`; buttons `Misc1, Paddle1..4, Touchpad`; `GamepadRumble`.

## Step 9 — new capability

- [x] 9a `FocusTarget += Directional(FocusDirection)`, `FocusDirection { Up, Down, Left, Right }`, geometric
      nearest-neighbour over the existing focusable set. No shell code.
- [x] 9b (types + Wayland producers done; other backends leave it Unknown — see 9b-i) `PointerSource { Unknown, Mouse, Touchpad, Trackball, Trackpoint, Touchscreen, Pen, Eraser }` on pointer
      events; `device_id` on mouse and key events. NOTE: #450 already delivered the *tablet* half
      (`TabletDeviceInfo`, matching `device_id` on `PenState`/`WacomPadState`) — this item is now the
      mouse/keyboard half plus the per-event `PointerSource` discriminator. Model it on `TabletDeviceInfo`.
- [x] 9c (type + Wayland producer done; Surface Dial / crown backends are 9c-i) `DialState { device_id, delta_rad, detent_count, pressed, contact_position }` + `DialRotate`/`DialClick`
      filters; wire Wayland `zwp_tablet_pad_dial_v2` (already bound) as the first producer.
- [x] 9d (types + X11 producer done; Win32/Wayland/web are 9d-i, and locking itself is 9d-ii) `RawMouseMotion` window filter + pointer-lock request path; Win32 `WM_INPUT`,
      Wayland `zwp_relative_pointer_v1` + `zwp_pointer_constraints_v1`, X11 `XI_RawMotion`.
- [x] 9e (types + ModifiersChanged + accessors done; the shells do not fill the new state yet — see 9e-i) `PhysicalKey` positional enum + `ModifiersChanged` filter + `KeyboardState += { modifiers, locks,
      is_repeat }`.
- [x] 9f (types + manager + accessors; no backend enumerates yet — 9f-i) `HidDevice { vendor_id, product_id, usage_page, usage, name }` + `HidReport { bytes }`.
- [x] 9g (types + manager + `CallbackInfo::play_haptic`; no backend plays yet — 9g-i) `Haptic::play(pattern)` — macOS `NSHapticFeedbackManager`, Win32 `SimpleHapticsController`,
      Android `performHapticFeedback`.
- [x] 9h Win32 `WM_APPCOMMAND` → media/browser app-command channel.

## Step 10 — mobile parity

- [x] 10a (native side + JNI entry points + keyboard request; the Java class is 10a-i) Android `InputConnection` (JNI bridge) → text input + the composition events from 3d.
- [x] 10b (UIKeyInput + UIPress done; full UITextInput is 10b-i) iOS `UITextInput` → text input; `UIPress`/`UIKeyCommand` for hardware keyboard.
- [x] 10c (keyboard inset modelled + Android bridge; safe-area itself already existed on macOS/iOS. iOS keyboard notifications are 10c-i) Insets / safe area / keyboard avoidance as a layout input (Android `WindowInsets`, iOS safe area).
- [x] 10d iOS `UIPencilInteraction` → `PenSqueeze` + `PenDoubleTap`.
- [x] 10e (iOS done; the web equivalent is 10e-i) `coalescedTouches` / `predictedTouches` (iOS) and the equivalent elsewhere.
- [x] 10f (both written; the Java half is 10f-i) Real gamepad backends to replace `gamepad/android.rs` (16 lines) and `gamepad/apple.rs` (17 lines).

## Step 11 — C6: full-stack stragglers

- [ ] 11a `Submit` off the existing `DefaultAction::SubmitForm`; `Change` as commit-on-blur off
      `TextInputOnFocusLost`. Add the filter variants first (all four layers), then update the
      `events_test.rs` unmapped pin.
- [ ] 11b `Reset` / `Invalid` — needs a validation concept; design then wire.
- [ ] 11c Media: `Play`/`Pause`/`Ended`/`TimeUpdate`/`VolumeChange`/`MediaError`. BLOCKED on a real playback state
      machine — `dll/src/unified/audio.rs:62` is `pub fn play(&self, _frame: AudioFrame) {}`. Build the state
      machine first, then emit.

## Step 12 — headless/test surface

- [ ] 12a `HeadlessEvent`: add scroll-phase, pen, gesture, gamepad, sensor and composition injection variants so
      everything above is reachable from the e2e runner.

## Step 13 — FIX-UP (only after everything above)

- [ ] 13a `cargo check --workspace` — fix compile errors.
- [ ] 13b `cargo run --release -p azul-doc codegen all` (target/codegen is wiped by `cargo clean`).
- [ ] 13c `azul-doc autofix` for every api.json delta recorded in step 8.
- [ ] 13d `cargo test --release --lib` per crate.
- [ ] 13e Full e2e (`--test all`) ONCE.
- [ ] 13f Drive the step-0 ratchet allow-list to empty; anything left is a real remaining gap.
