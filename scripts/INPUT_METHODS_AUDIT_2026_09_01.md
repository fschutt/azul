# Input methods audit — 2026-09-01 (rev 2)

Full report: https://claude.ai/code/artifact/9fe65fa2-54c7-447d-a8de-d570df41b262

Ground truth: `core/src/events.rs`, `core/src/events_test.rs`, `api.json`,
`dll/src/desktop/shell2/{windows,macos,linux,android,ios,headless}`, `dll/src/web/`.

**Policy: nothing is removed from api.json. Everything gets wired.**

## The mechanism

A filter is alive only when four layers agree:

1. a shell constructs the `EventType`
2. `event_type_to_filters` (dispatch **planning**) returns that filter — this is how nodes get looked up
3. `matches_*_filter` (**phase matching**) accepts the pair
4. `matches_filter_phase` lets the family through

Disagree on any one → the callback is silently never called. The codebase already names this
(`events.rs:3091`: *"`event_type_to_filters` is what DISPATCH PLANNING uses; the `matches_hover_filter` table is
what phase matching uses. Both must agree"*) and has a ratchet test with a `KNOWN_DESYNC` allow-list — but it only
compares **layers 2↔3, Hover only**. Every failure below is in a layer it doesn't watch.

**46 of 196 filter variants cannot fire today.**

## Failure classes

### C1 — producer ✓, matcher ✓, **no planning arm** (14 variants) ← biggest win, smallest diff

`event_type_to_filters_legacy_hint` has **no `E::Pen*` arm and no `E::DocumentEdit` arm**; they fall through
`_ => vec![]`, so planning returns an empty list and no node is ever looked up.

- `EventType::PenDown` produced at `layout/src/event_determination.rs:862`
- matcher arms exist: `events.rs:1405` (hover), `:1566` (window)
- dead: `PenDown/Move/Up/Enter/Leave` × Hover(5) + Focus(3) + Window(5) = 13, plus `FocusEventFilter::DocumentEdit`

**Pen pressure/tilt/twist/eraser is implemented on all six shells and every pen event dispatches to nothing.**
`CallbackInfo::get_pen_state()` still works (polling), but you cannot subscribe.

Fix: add six arms to `event_type_to_filters_legacy_hint`. No shell work, no api.json change.

### C2 — planning points at a **different filter** than the matcher accepts (9 variants)

`events.rs:3180`: `E::Scroll | E::ScrollStart | E::ScrollEnd => vec![EF::Hover(H::Scroll)]`
→ nodes on `Hover(ScrollStart)` never looked up; nodes on `Hover(Scroll)` looked up then rejected by
`(Scroll, EventType::Scroll)`. Dead from both directions. No Focus/Window planning entry either, though both
families have the variants **and** the matcher arms (`:1393-1394` hover, focus rel. 30-31).

Same shape: `E::ContextMenu → Hover(RightMouseDown)` (matcher wants `EventType::MouseDown` + right-button payload);
`E::KeyPress → Focus(TextInput)` and `E::Change → Focus(TextInput)` (matcher has only `(TextInput, EventType::Input)`).

Fix: split the scroll arm into three (Hover+Focus+Window each); add matcher arms
`(RightMouseDown, EventType::ContextMenu)`, `(TextInput, EventType::KeyPress)`, `(TextInput, EventType::Change)`.
→ ContextMenu then makes the Menu/Apps key, Shift+F10 and a11y `ShowContextMenu` reach existing right-click
handlers with **no new filter**.

### C3 — planning ✓, matcher ✓, **no producer** (12 variants)

| what | note |
|---|---|
| `Composition*` ×6 (Hover+Focus) | IME wired end-to-end (Win32 `WM_IME_*`, macOS `NSTextInputClient`, Wayland `zwp_text_input_v3`, X11 XIM + `linux/common/compose.rs`) but terminates in `apply_preedit_to_text_cache`. `// Phase 2: OnCompositionStart callback` TODO at `windows/mod.rs:5347`. |
| `Copy`/`Cut`/`Paste` ×3 (Focus) | Design already written in the enum doc comment (`events.rs:2278`): *"fire on the focused element BEFORE the OS default action, which preventDefault suppresses"*. Machinery exists — `post_callback_filter_system_changes(prevent_default, …)` already has the `if prevent_default { …only focus passes… return }` gate. Dispatch before pushing `SystemChange::{CopyToClipboard,…}` and interception works for free. |
| `MouseOut` ×1, `FocusIn`/`FocusOut` ×4 | Emit alongside `MouseLeave` / `Focus` / `Blur`. |

⚠ **Their `KNOWN_DESYNC` entries are STALE** — the comments say "matcher has no arm" but the arms were added later
at `events.rs:1438-1443`. Six of the ten ratchet entries are currently protecting nothing.

### C4 — **phase gate closed** (4 variants)

`matches_filter_phase` returns `false` for `EventFilter::Application(_)` ("will be implemented in future").
Planning is already correct (`E::MonitorConnected → EF::Application(MonitorConnected)`).

Producers are cheap — most signals already arrive:
- **gilrs** already reports gamepad connect/disconnect via the capability pump → free `DeviceConnected` on all 4 desktops
- **Wayland** `wl_registry.global_remove`, `wl_seat.capabilities`, `zwp_tablet_seat_v2` add/remove — all handled
- **Win32** `WM_DISPLAYCHANGE` handled (diff the monitor list)
- **macOS** `windowDidChangeScreen:` handled
- genuinely new: X11 RandR `XRRSelectInput` + `XI_HierarchyChanged`, Win32 `WM_DEVICECHANGE`, macOS
  `NSApplicationDidChangeScreenParameters`

### C5 — filter exists, **no `EventType` to carry it** (8 variants)

`EventType::PenSqueeze/PenDoubleTap/PenHover` **do not exist** (0 occurrences) though the filter variants do on
Hover+Window (6) and are handled in the Window→Hover map at `:2502-2504`.
`ComponentEventFilter::{DefaultAction, Selected}` (2) are absent from `matches_component_filter`'s arms.

Fix: append 3 `EventType` variants at the end of the enum for ABI stability (same convention `Copy`/`Cut`/`Paste`/
`DocumentEdit` used), sync via **`azul-doc autofix`** — never a hand-edited api.json patch.
`PenHover` is nearly free: Wayland `proximity_in`+`distance`, Win32 `POINTER_FLAG_INRANGE`, Android
`ACTION_HOVER_MOVE` (already handled), macOS `NSEventSubtype::TabletProximity` on the existing mouse-event path
(same trick `feed_tablet_pen` uses).

### C6 — no filter at all, **explicitly pinned unmapped** (9 EventTypes)

`Submit, Reset, Invalid, Play, Pause, Ended, TimeUpdate, VolumeChange, MediaError`.
`events_test.rs:2243` asserts they *must* yield no filters — wiring them updates that pin deliberately.

- **Form half is tractable**: `DefaultAction::SubmitForm` already exists for `Submit`; `Change` is commit-on-blur and
  `TextInputOnFocusLost` is already the site. `Reset`/`Invalid` need a validation concept first.
- **Media half is blocked**: no playback state machine yet — `dll/src/unified/audio.rs:62` is
  `pub fn play(&self, _frame: AudioFrame) {}`, an empty stub. Sequence last.

### C7 — 12 of 23 a11y actions unrouted (pure plumbing)

Adapters map `Default, Focus, Blur, Collapse, Expand, Increment, Decrement, Scroll{Up,Down,Left,Right}`.
Nearly every unrouted one already has an engine function:

`ScrollIntoView`→`scroll_node_into_view` · `ScrollToPoint`/`SetScrollOffset`→`scroll_to`/`scroll_to_unclamped` ·
`SetTextSelection`→`TextOpSetSelection` · `ReplaceSelectedText`→text-edit manager ·
`ShowContextMenu`→`open_menu_for_node` · `Show/HideTooltip`→`show/hide_tooltip_from_callback` ·
`SetValue`/`SetNumericValue`→widget setters · `SetSequentialFocusNavigationStartingPoint`→focus engine ·
`CustomAction`→callback passthrough

`SetTextSelection` + `ReplaceSelectedText` are what a screen reader / voice control need in a text field.

## Device gaps (G-series) — unchanged from rev 1

G1 back/forward mouse buttons pumped but unsubscribable (`MouseButton::Other(u8)` routes; no filter, no
`MouseState` field; planning even documents it: `MouseButton::Other(_) => None`) ·
G2 touchpad pinch/rotate dead on Win/X11/Wayland (in-process detector needs touch points = touchscreen only;
`zwp_pointer_gestures_v1` unbound, XI 2.4 gestures unhandled, `WM_GESTURE` pre-empted by `WM_POINTER*`) ·
G3 Wayland seat capped `min(7)` → no `axis_value120`/`axis_relative_direction`; macOS
`isDirectionInvertedFromDevice` unread ·
G4 no raw/relative pointer (`is_cursor_locked` is a flag with nothing behind it) ·
G5 `TouchPoint` = 3 fields; Wayland `touch_shape_handler`/`touch_orientation_handler` are **empty bodies** ·
G6 pen tail ragged (`tangential_pressure=0.0` X11/Wayland/iOS, `barrel_button_pressed=false` macOS, `tool_id=0`),
no `pressureChangeWithEvent:` ·
G7 `WacomPadState` models 2 of 5 pad controls both Linux shells already bind (incl. `_dial_v2`) ·
G8 gamepad input-only, `android.rs`/`apple.rs` are 16/17-line stubs ·
G9 one fused 163-entry keycode table, no physical/logical split, no `ModifiersChanged`, no lock state

## No representation yet (N-series)

N1 directional focus nav (**zero shell code** — geometric nearest-neighbour over the existing focusable set) ·
N2 `PointerSource` + device identity (unblocks multi-seat; also what G2 needs to tell touchpad from mouse) ·
N3 `DialState` — one type for Surface Dial / `zwp_tablet_pad_dial_v2` (already bound!) / Wear crown / Digital Crown ·
N4 `HidDevice`/`HidReport` escape hatch (= WebHID's shape) ·
N5 `Haptic::play`; media keys as a channel (`WM_APPCOMMAND` unhandled, MPRIS on Linux) ·
N6 XR/voice/eye-tracking — **deferred, not removed**: they arrive as pointer events + a11y actions, so they are
covered by N2 and C7, both of which are on the list

## Work order

**Step 0 — turn the ratchet into the work queue.**
Extend `event_type_to_filters_never_panics_and_stays_synced_with_the_hover_matcher` from 2 layers × 1 family to
**4 layers × 3 families**. It goes red with ~46 entries — *that list is the backlog*, and every fix deletes a line.
Also prune the 6 stale `KNOWN_DESYNC` entries (they hide future regressions today).

1. **C1** planning omissions → 14 variants alive, no shell work, no api.json change
2. **C2** planning de-sync (split the scroll arm; 3 matcher arms) then emit `ScrollStart`/`ScrollEnd`/`ContextMenu`
3. **C3** missing producers: `MouseOut`, `FocusIn/Out`, `Composition*` (+`CompositionEventData`), `Copy/Cut/Paste`
4. **C4** open the Application phase; producers cheapest-first (gilrs → Wayland → Win32 → macOS → X11)
5. **C5** append the 3 pen `EventType`s (autofix), add the 2 Component arms
6. **C7** route the 12 a11y actions
7. **Shell wiring, no API change**: G2, G3, G6 (macOS responders), G5 (Wayland stubs)
8. **api.json deltas via `azul-doc autofix`**: G1, G5, G6, G7, G8, `SensorKind`→~12 incl. `HingeAngle`
9. **New capability**: N1, N2, N3, G4, G9, N4, N5
10. **Mobile parity**: text input (pairs with C3), insets, `UIPencilInteraction`, coalesced/predicted, real gamepad
11. **C6 stragglers**: form events, then media (after the media backend has real state)

## Constraints

- **api.json changes go through `azul-doc autofix` only** — never hand-curated patches. The in-source comment at
  `events.rs:2280` even says "sync to api.json via azul-doc autofix".
- **`HeadlessEvent` has 12 variants; scroll is always `WheelDiscrete`; no pen/gamepad/gesture injection.**
  Add a `HeadlessEvent` variant in the same commit as each wiring fix, or it can't be regression-tested.
