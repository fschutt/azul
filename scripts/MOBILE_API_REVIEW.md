# Mobile API Review — P2-P7 user-facing surface (2026-05-20)

A design review of every feature added in SUPER_PLAN_2 §4 P2-P7, measured against
"the Azul way": `f(State, &mut cache) -> UI` + event→callback→State→re-render +
backreference dependency-injection hooks + **no process-global state**.

## Verdict

**Structurally sound, systematically half-built.** No feature uses a user-facing
global; the widget pattern + per-window managers fit `f(State,&cache)->UI`. But
one gap repeats across *nearly every* feature:

> **Data flows one way and must be POLLED — it is never PUSHED to user code.**
> Azul's own backreference hook (`set_on_X`, as in `button.rs`/`number_input.rs`)
> is absent from every P2-P7 feature.

Concretely:
- **Capture widgets** (camera/screencap/video) are one-way streets: frame → GL
  texture, *never* to the app. No `on_frame`. → **azul-meet is impossible** today
  (can't read frames out to send; can't push frames in to display a remote stream —
  `present_frame`/`upload_rgba` are crate-internal, `capture_common.rs:52,97`).
- **Input devices** (sensors/gamepad/wacom-pad) are poll-only accessors with no
  event → the app must run a `Timer` and return `Update::RefreshDom` *every tick*
  (a relayout busy-loop; `azul-gamepad/src/main.rs:159-169` says so explicitly).
  The `changed` bool is already computed and **thrown away** (`sensors.rs:51`→log
  `layout.rs:852`; `gamepad.rs:58`→`layout.rs:879`).
- **Async system services** (biometric/keyring/pdf-export/permission) return results
  poll-only → the "tap Unlock twice" wart (`azul-vault/src/main.rs:9-13`); PDF
  export's success `ok` bool is **discarded** (`layout.rs:819`).

The fix is two seams — both pure-azul, both no-globals, both also the exact
azul-meet enablers:
1. **Push, don't poll.** EventFilter for DOM/window-level producers; backreference
   hook for widget-level producers.
2. **Backreference hooks wherever a widget produces data.**

## Reference patterns already in the tree (copy these)

| Pattern | Where | What it nails |
|---|---|---|
| **Widget structure** | `MapWidget` (`widgets/map.rs`) | POD mirrors api.json (no fn-ptr in the `#[repr(C)]` struct, `map.rs:133-144`); builder→`.dom()`; `RefAny` dataset + merge-callback (survives relayout); `AfterMount` bootstrap; `Thread`+writeback; pure unit-tested core. **But missing the `set_on_X` hooks.** |
| **Input** | Pen (`gesture.rs` + `events.rs:1561-1584`) | Real `EventFilter`s (`PenDown/Move/Up/...`), per-window manager, detail via `CallbackInfo` accessor *inside* the event callback. Push, not poll. |
| **Async sensor** | Geolocation (`managers/geolocation.rs`) | permission-as-DOM probe + per-window manager + `EventFilter` (`GeolocationFix/Error`) + sync accessor. Has **both** a push event and a poll accessor. `take_pending_events()` (`:109`) is the precedent for sensors/gamepad. |
| **DI hook** | `Button` (`button.rs:988-1009,100-109`) | `OptionButtonOnClick` field + `set_on_click(data,cb)`/`with_on_click` + `impl_managed_callback!` FFI thunk. The exact idiom every producer should expose. |

## Per-feature scorecard

| Feature | State lives in | Delivery to user | DI hook? | Top gap |
|---|---|---|---|---|
| Camera / Screencap / Video | widget `RefAny` ✅ | **none** (→texture only) | ❌ | no `on_frame`; no frame-in; config ignored post-mount |
| Audio (POD only) | n/a | n/a | ❌ | zero plumbing; not in api.json; doc describes a *global-channel* design (rejected) |
| Sensors | per-window mgr ✅ | poll accessor | ❌ | no `SensorChanged` event → Timer busy-loop |
| Gamepad | per-window mgr ✅ | poll accessor | ❌ | no button/connect events; no enumerate-all accessor; no rumble |
| Wacom **pen** | per-window mgr ✅ | **EventFilter** ✅ | (events) | model citizen — done right |
| Wacom **pad** | per-window mgr ✅ | poll accessor | ❌ | **dead**: no backend on any platform → always `None` |
| MapWidget | widget `RefAny` ✅ | merge-callback only | ❌ | no `on_pin_tap`/`on_viewport_changed`; projection helpers private → demo duplicates them |
| Geolocation | per-window mgr ✅ | accessor **+ event** ✅ | (events) | good; one-shot convenience missing |
| Biometric / Keyring | per-window mgr ✅ | poll accessor | ❌ | no completion event ("tap twice"); keyring's 3 ops share 1 result slot |
| PDF export | global channel | **nothing** | ❌ | `ok` bool discarded → no Saved✓/Failed UI possible |
| Permission | per-window mgr ✅ | (indirect) | ❌ | `get_permission_status` documented, **not implemented**; types **absent from api.json**; only `GeolocationProbe` mountable |
| Db / SQLite | handle (re-opened per call) | return value ✅ | n/a | **blocking on UI thread**; `DbRows`/`DbValue` accessors absent from api.json |

## The "globals" nuance

The process-global static channels (`PENDING_READINGS`, `PENDING_STATES`,
`PENDING_REQUESTS/RESULTS`, `PENDING_EXPORTS`, `PENDING_FIXES`, `ASYNC_RESULTS`)
are **internal transport**, not user-facing state: `azul-layout` can't link
platform code, and the OS reply thread has no window handle. They're a deliberate
house style copied from geolocation. *But* they have a real bug: the transport is
**per-process** while the consuming manager is **per-window** → two windows share
one result queue (cross-window bleed). The **pen path shows the clean fix**: where
the producer holds the window handle it mutates the window manager *directly* and
skips the channel entirely (`ios/mod.rs:416`→`update_pen_state_full`). So "no
globals" is reachable even for transport — follow the pen precedent where possible.

## Redesign plan (tiered)

### T1 — azul-meet prerequisites (== "continue audio/video", do now)
1. **Move `VideoFrame` → `azul-core`, FFI-ready** (`#[repr(C)]`, `U8Vec` not
   `Vec<u8>`) — mirrors `AudioFrame`. Prereq for typed frame hooks.
2. **`set_on_frame(data, cb)` backreference hook** on Camera/ScreenCapture/Video
   (payload `VideoFrame`) + Microphone (payload `AudioFrame`), via
   `impl_widget_callback!` + `impl_managed_callback!{extra_args:[frame: …]}` — the
   `NumberInput::on_value_change` recipe. The private writeback invokes it → user
   does effects / save / **send over UDP**. (The single highest-value addition.)
3. **Frame-IN path** for displaying a *remote* stream. Design sub-decision (resolve
   when reached): `VideoSource::ExternalFrames` + a push method, vs a pull hook
   `set_on_need_frame(data, cb) -> OptionVideoFrame` the worker calls ~30/s, vs
   making `present_frame` public for a user-built display node.
4. **Audio widgets**: `MicrophoneWidget` (capture + `on_frame`) + `AudioWidget`
   (playback). Fix the rejected global-channel doc note in `audio.rs:5-7`.

### T2 — ergonomic retrofit ("easier to use")
5. **Input as events** (copy geolocation's `take_pending_events` + the discarded
   `changed` bool): `SensorChanged`, `GamepadInput`/`Connected`/`Disconnected`,
   pad `ExpressKey` events. Removes the Timer busy-loop everywhere.
6. **MapWidget hooks**: `set_on_pin_tap(data,cb)` + `set_on_viewport_changed`;
   make lat/lon↔pixel projection public (demo duplicates `tap_to_latlon`).
7. **Completion events for async services**: `BiometricResult`, `KeyringResult`
   (carry the originating key/request-id), `PdfExportDone(ok)` (the `ok` bool
   already exists, just discarded). Kills "tap twice".

### T3 — completeness / cleanup
8. **Permission**: expose types in api.json + implement `get_permission_status`;
   add camera/mic mountable probe NodeTypes.
9. **Db**: expose `DbRows`/`DbValue` accessors in api.json; offer a
   `Thread`-offloaded async query (so a remote/slow query doesn't block the frame).
10. Fix dead doc in `core/src/camera.rs` (references a non-existent
    `managers::camera`/`CameraStream`/`start_camera`).
11. Wacom pad backend (Wintab/libwacom/NSEvent) — on-machine batch.

## azul-meet readiness

**Today: 0%** — no path for frame/sample data to leave or enter a widget.
**After T1: feasible** over the public, no-global API:
capture → `on_frame` → encode → **AzUdp** (P8) → decode → push-into-widget.
**T1 is the critical path**; T2/T3 are quality, not blockers.
