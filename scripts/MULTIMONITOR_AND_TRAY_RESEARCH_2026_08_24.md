# Multi-monitor & System Tray — what's left to call azul a desktop toolkit

**Date:** 2026-08-24
**Branch at time of writing:** `transient-window`
**Method:** six parallel agents — three reading the tree (core/API model, macOS+Windows shells, Linux shells), one architecture bug-sweep, two doing web research on 2026 platform APIs. Every HIGH-severity claim below was re-verified by hand against the quoted line.

---

## 0. Verdict

Two very different situations.

**Multi-monitor is not missing — it is unwired and unit-confused.** The types, the FFI surface, the per-platform enumeration and even the event kinds all exist and are mostly well designed. What's missing is the wiring between them, and what's *wrong* is that three different coordinate/unit conventions have been quietly mixed in the popup and cursor paths. This is a **bug-fixing and plumbing job**, not a feature build. It is also the more urgent of the two, because the bugs are live today on any HiDPI or multi-head machine.

**System tray is genuinely absent.** Zero lines on all platforms, no commit in history. It is a **from-scratch feature build**, roughly 1500–1800 LOC across three platforms, of which Linux is ~60%. It also drags in two prerequisites azul does not have: a macOS `.app` bundle story, and an app-level (not per-window) object to own the tray.

Neither is blocked on anything architectural. But there is one genuine architecture defect — `WindowPosition`'s unit ambiguity — that should be fixed *before* either workstream, because both build on top of it.

---

## PART I — MULTI-MONITOR

### 1.1 What already exists and works

Credit where due; this is further along than the "missing feature" framing suggests.

| Piece | Where | State |
|---|---|---|
| `Monitor` struct — id, name, size, position, scale_factor, work_area, video_modes, is_primary | `core/src/window.rs:896` | Complete, well-designed |
| `MonitorId { index, hash }` + `from_properties` | `core/src/window.rs:819` | Exists, well unit-tested (`:2764-2870`) |
| FFI export of the whole model | `api.json` (48 refs) | Complete — reachable from every binding language |
| `CallbackInfo::get_monitors()` | `layout/src/callbacks.rs:3310` | Works wherever the cache is seeded |
| `App::get_monitors()` | `dll/src/desktop/app.rs:181` | Works (but see bug #15) |
| `LayoutCallbackInfo::get_max_monitor_size()` | `core/src/callbacks.rs:1097` | Works — bounds first-layout DOM size |
| Windows enumeration — `EnumDisplayMonitors` + `GetMonitorInfoW` + `GetDpiForMonitor` | `display.rs:154-432` | Real, correct |
| macOS enumeration — `NSScreen.screens` | `display.rs:434-516` | Real, correct |
| X11 enumeration — XRandR CRTC walk | `display.rs:606-818` | Real, RandR 1.2-era |
| Windows `WM_DPICHANGED` — uses the suggested RECT verbatim | `windows/mod.rs:5643-5734` | **Exemplary.** Correctly documents why the rect is frame-inclusive |
| Windows per-monitor-v2 awareness | `windows/dpi.rs:135-154` | Declared programmatically (but see bug #4) |
| macOS primary-screen Y-flip convention ("MWA-B9") | `macos/mod.rs:154-157, 226-235` | **Exemplary.** Every flip site uses `primary_screen_height()`, each with a comment naming the bug it fixed |
| Wayland `wp_fractional_scale_v1` + `wp_viewporter` | `wayland/events.rs:338, 652, 674` | Real, correct — CPU path |
| Wayland `wl_surface.enter/leave` → scale | `wayland/events.rs:228, 303` | Correct — the only right way to do this |
| Wayland `xdg_positioner` with `set_constraint_adjustment` | `wayland/mod.rs:7099-7230` | Correct — compositor does edge-clamping |
| Wayland honestly refuses to position windows | `wayland/mod.rs:2321-2335`, `:7879` | Correct design, well documented |
| Per-monitor `CVDisplayLink` on macOS | `macos/mod.rs:3931-3948` | Correct — bound to the window's actual display, re-created on change |

The macOS coordinate handling and the Windows DPI-change handling are both *better* than most hand-rolled toolkits. The Wayland backend is the most modern of the four.

### 1.2 The bug ledger

Severity is mine; `file:line` verified by hand.

#### Tier 1 — architecture defect, fix first

**A1. `WindowPosition` and `RelativeToParentWindow` carry two different units.** HIGH

`popup_window_state` takes a `LogicalPosition` and stores it in a `PhysicalPosition` field:
```rust
// dll/src/desktop/shell2/common/transient.rs:365
#[allow(clippy::cast_possible_truncation)] // whole logical pixels
window_state.position = WindowPosition::RelativeToParentWindow(PhysicalPosition::new(
    origin.x.round() as i32,
    origin.y.round() as i32,
));
```
The consumer adds it to a physical origin from `GetWindowRect` under a per-monitor-DPI-aware process:
```rust
// dll/src/desktop/shell2/windows/mod.rs:6689
Some((px, py)) => (px + offset.x, py + offset.y),
```
**Effect:** on a 200% display every `<transient-window>` popup — colour picker, and everything that migrates to this path — lands at half its intended offset from the parent. macOS is accidentally correct because AppKit points *are* logical. X11 is affected.

Same class, same file: tear-off placement `transient.rs:283-290` and tear-off *drag* `transient.rs:713-719` add logical deltas to physical origins, so a torn-off window drags at `1/scale` speed.

**A2. `WindowPosition::Initialized` means monitor-relative at creation and absolute everywhere else.** HIGH

`core/src/window.rs:978` documents it as absolute virtual-screen. Creation treats it as monitor-relative on all three desktop backends (`windows/mod.rs:6662`, `x11/mod.rs:2780`, `macos/mod.rs:8121`), while `sync_window_state` treats it as absolute (`windows/mod.rs:2398`, `macos/mod.rs:5405`). A window created at `Initialized(x,y)` on a secondary monitor lands at `monitor.origin + (x,y)`, then teleports to absolute `(x,y)` on the first programmatic move. Masked today only because the target is nearly always the primary at (0,0) — see A6.

**A3. `DisplayInfo.bounds`/`work_area` are physical px on Windows/X11 and logical points on macOS — same `LogicalRect` type.** HIGH

`display.rs:285-301` (Windows, `rcMonitor` device px), `display.rs:685-697` (X11, raw CRTC px), `display.rs:461-479` (macOS, `NSScreen.frame()` points). The doc contradicts itself three ways: `core/src/window.rs:901` says "Physical size … in logical pixels"; `:907` says work area "in logical pixels"; `core/src/callbacks.rs:1095` says `get_max_monitor_size` returns "physical px".

Downstream, `transient.rs:314-336` (`placement_bounds`) mixes all of it: it relabels a physical `pos` as logical with a bare `as f32`, passes physical-position + logical-size to `get_window_display` (which computes a window *centre* — so the wrong monitor is picked near boundaries), and hands a physical work-area rect to `resolve_within`, which works in logical space. At 150% the clamp box is ~1.5× too big and popups still run off the edge.

#### Tier 2 — per-platform wiring that is simply absent

**A4. `get_current_monitor()` is non-functional on three of four backends.** HIGH

It matches `ws.monitor_id` (a `u32` *index*) against the cache:
```rust
// layout/src/callbacks.rs:3331
if m.monitor_id.index == monitor_index {
```
- **macOS** writes a `CGDirectDisplayID` (~69733382) into that index field — `macos/mod.rs:3863`, with the comment *"For now, use display_id as index (not perfect but reasonable)"*. It never matches.
- **X11** never writes it: `x11/mod.rs:2101` — `let monitor_id = 0; // TODO: Get from options or detect primary monitor`, and `ws.monitor_id` stays `None` for the window's life.
- **Wayland** never writes it. It *has* a correct `get_current_monitor_id()` at `wayland/mod.rs:7047` — with **zero callers**. Dead code.
- **Windows** is the only correct one: `MonitorFromWindow` → match cache by position → store the enumeration index (`windows/mod.rs:4056-4098`).

Consequence: `WindowMonitorChanged` can only ever fire on Windows (the event rule at `event_determination.rs:777` needs both sides `Some`).

**A5. macOS misses same-DPI monitor changes.** MEDIUM-HIGH

`detect_current_monitor()` is reachable only from window creation (`macos/mod.rs:4933`) and `handle_dpi_change()` (`:5191`), and the latter runs only from `windowDidChangeBackingProperties:` (`:3071`). AppKit posts that notification only when the backing scale or colour space changes. `windowDidChangeScreen:` (`:3077-3092`) refreshes the monitor *list* but never re-detects the current monitor. So dragging between two identical Retina displays leaves `monitor_id` stale, fires no event, and leaves the `CVDisplayLink` bound to the old display. (The comment at `:5222` claims this path handles same-DPI moves — it isn't reached.)

**A6. Windows ignores the requested monitor at window creation.** HIGH
```rust
// dll/src/desktop/shell2/windows/mod.rs:627
// TODO: Use monitor_id to look up actual Monitor from global state
position_window_on_monitor(hwnd, Monitor::default().monitor_id, ...)
```
`Monitor::default().monitor_id == MonitorId::PRIMARY`. X11 has the same hole (`x11/mod.rs:2101`). Only macOS honours `options.window_state.monitor_id` (`macos/mod.rs:4357`) — and A4 makes even that unreachable in practice.

**A7. macOS explicit window position is never Y-flipped.** HIGH
```rust
// dll/src/desktop/shell2/macos/mod.rs:8120
WindowPosition::Initialized(pos) => {
    // Explicit position requested - use it relative to monitor
    // Note: macOS y-axis is flipped (0 at bottom)
    (screen_frame.origin.x + pos.x as f64,
     screen_frame.origin.y + pos.y as f64)
}
```
The comment says the axis is flipped, then doesn't flip it. `pos` is top-left logical; `screen_frame.origin` is bottom-left; the result goes to `setFrame_display:` which wants a bottom-left origin. A window asked to open at `(0,0)` opens at the *bottom* of the screen — and this is the one site in the whole macOS backend that breaks its own MWA-B9 convention.

**A8. Windows loads two DPI symbols from the wrong DLL.** HIGH
```rust
// dll/src/desktop/shell2/windows/dpi.rs:101, 113
get_dpi_for_monitor:       Self::get_func(user32_dll, "GetDpiForMonitor")
set_process_dpi_awareness: Self::get_func(user32_dll, "SetProcessDpiAwareness")
```
Both live in **shcore.dll**; only `user32.dll` is loaded (`dpi.rs:89`). Both are permanently `None`. The other five symbols in that block genuinely are user32, so this is a two-line fix. Effect: on Windows 8.1 / pre-1607 Win10 the process silently degrades to `SetProcessDPIAware()` — system-DPI-aware, not per-monitor — and `hwnd_dpi`'s middle branch is dead. (`display.rs:229` links `GetDpiForMonitor` from shcore correctly, so the two paths disagree with each other.)

#### Tier 3 — Linux-specific

**A9. Linux work area is a hardcoded 24px guess.** HIGH
```rust
// dll/src/desktop/display.rs:524
const FALLBACK_PANEL_HEIGHT: f32 = 24.0;
```
Applied unconditionally at `:695, :804, :828` (X11) and `:1031, :1103, :1183, :1305` (all four Wayland providers), always subtracted from the *bottom*. `_NET_WORKAREA` and `_NET_WM_STRUT_PARTIAL` are **never read anywhere in the tree** (zero hits). A top panel (GNOME), a left dock (Plasma/Unity), or no panel at all are all modelled as "24px missing at the bottom". This feeds real popup clamping (`x11/mod.rs:4362`, `transient.rs:331`, `menu.rs:76`). Maximize is unaffected — X11 goes through EWMH `_NET_WM_STATE_MAXIMIZED_*` so the WM applies real struts.

**A10. X11 assigns one screen-wide DPI to every monitor.** HIGH
```rust
// dll/src/desktop/display.rs:659-667
let width_mm = (xlib.XDisplayWidthMM)(display, screen);
let screen_dpi = ((xlib.XDisplayWidth)(display, screen) as f32 / width_mm as f32) * 25.4;
base_scale = screen_dpi / 96.0;
```
`XDisplayWidth`/`WidthMM` describe the whole *virtual* screen; that single value is written to every CRTC at `:728`. `XRROutputInfo.mm_width` is never read. Consequence: the careful per-monitor DPI-change path at `x11/mod.rs:3805` compares a value that is identical for every monitor — it is dead code on real mixed-DPI X11.

Also: X11 uses `XRRGetScreenResourcesCurrent` + CRTC walk (RandR 1.2 era), never RandR 1.5's `XRRGetMonitors`; monitor names are fabricated as `"CRTC-{i}"` (`display.rs:725`); primary is guessed as `i == 0` rather than `XRRGetOutputPrimary` (`:729`); and `XRRUpdateConfiguration` is never called.

**A11. X11 re-opens an X connection per drag pixel.** MEDIUM (perf)

`x11/mod.rs:3804` calls `get_display_at_point` inside the `ConfigureNotify` arm. That leads to `display.rs:620` — `XOpenDisplay` + `XRRGetScreenResourcesCurrent` + N × `XRRGetCrtcInfo` + `XCloseDisplay`, **uncached**: the 15s `DISPLAY_CACHE` is declared inside `mod wayland` (`display.rs:862-870`) and covers nothing else. A window move delivers one `ConfigureNotify` per pixel of pointer travel. The same file works hard 150 lines earlier (`:3640-3660`) to avoid *one* `XTranslateCoordinates` round-trip per configure.

**A12. Wayland has two disconnected monitor models, and exposes the worse one.** HIGH

The backend already tracks real `wl_output` geometry (`wayland/mod.rs:335-345`, `MonitorState { proxy, name, scale, x, y, width, height, make, model }`). What the app sees instead comes from `display::get_monitors()` → `display.rs:856`:
```rust
const DETECTION_CHAIN: &[DisplayProvider] =
    &[try_swaymsg, try_hyprctl, try_kscreen_doctor, try_wlr_randr];
```
Four subprocess spawns **on the UI thread**. The code is honest about the scar (`display.rs:864`): *"a tool that never exits (observed live: the fourth per-tick detection call on KDE Wayland never returned) froze the whole app — the window could no longer even close."* Mitigated with a 2s per-tool timeout and 15s cache. **On GNOME/Mutter none of the four tools exists** → fabricated single 1920×1080 @ scale 1.0.

`xdg_output` (logical geometry + connector names) is never bound — which is exactly the data the shell-out is trying to recover.

**A13. Wayland never handles monitor hot-unplug.** MEDIUM
```rust
// dll/src/desktop/shell2/linux/wayland/events.rs:756
pub(super) extern "C" fn registry_global_remove_handler(
    _data: *mut c_void, _registry: *mut wl_registry, _name: u32,
) {}
```
`known_outputs` grows monotonically across replug cycles, shifting every index — which matters because `get_current_monitor()` at `wayland/mod.rs:7014` correlates `known_outputs` (registry advertisement order) against `display::get_displays()` (CLI text output order) *by index*. Nothing ties those two orderings together.

**A14. Wayland GPU path has no scale handling.** HIGH
```rust
// dll/src/desktop/shell2/linux/wayland/mod.rs:6905
RenderMode::Gpu(gl_context, _gl_functions) => {
    gl_context.resize(&self.wayland, width, height);   // LOGICAL
}
```
`buf_w/buf_h/scale` from `cpu_buffer_spec` are discarded on this branch; neither `set_buffer_scale` nor `wp_viewport.set_destination` is called. Meanwhile the present path renders at physical size (`mod.rs:5318`). On HiDPI + GPU the EGL buffer is `logical×1` while WebRender draws a `logical×2` viewport into it — content clipped to the top-left quadrant. The CPU path is correct.

#### Tier 4 — user-facing API correctness

**A15. `get_cursor_position_screen()` returns a mixed-unit number.** HIGH
```rust
// layout/src/callbacks.rs:3229
WindowPosition::Initialized(pos) => OptionScreenPosition::Some(ScreenPosition::new(
    pos.x as f32 + cursor_local.x,
    pos.y as f32 + cursor_local.y,
)),
```
`pos` is `PhysicalPositionI32`; `cursor_local` is logical. This is a public FFI API. `ScreenPosition` is documented as "logical pixels, relative to primary monitor origin" (`core/src/geom.rs:619`) — a space that isn't well-defined under mixed DPI at all.

**A16. Eyedropper captures the primary monitor and scales it by the wrong window's DPI.** HIGH
```rust
// dll/src/desktop/eyedropper/windows.rs:26, 75-76
let width = (user32.GetSystemMetrics)(SM_CXSCREEN);   // primary only
origin: LogicalPosition::zero(),
scale: window.common.current_window_state().size.get_hidpi_factor()...,  // asking window's scale
```
App on a 200% secondary, primary at 100% → the loupe is created at half the primary's size and every `cursor * shot.scale` lookup reads the wrong pixel. X11 is the same shape but captures the whole virtual screen (`eyedropper/x11.rs:21`) then divides by one window's scale. macOS is fine — it uses the system `NSColorSampler`.

**A17. `MonitorId` has two incompatible hash implementations, and neither is fit for its documented purpose.** MEDIUM

`core/src/window.rs:875` hashes name + **position** + size with FNV-1a. `macos/coregraphics.rs:89` hashes display_id + size with `DefaultHasher`, *deliberately excluding* position — with the comment *"We don't hash position because it can change when monitors are rearranged"*. They disagree on inputs, on algorithm, and on intent.

Both are wrong for the stated use case (`core/src/window.rs:816`: *"stable across app restarts and monitor reconfigurations"*): the FNV one changes when you rearrange displays, which is the common case; and `DefaultHasher` is explicitly **not stable across Rust releases**, so persisting it is unsound. Latent today — nothing persists geometry (see §1.3) — but the primitive doesn't do what it says.

**A18. Monitor hotplug events are declared, exported, and never emitted.** MEDIUM

`ApplicationEventFilter::MonitorConnected` / `MonitorDisconnected` exist at `core/src/events.rs:2517`, are mapped at `:2822`, are unit-tested at `:5747`, and are exported through `api.json:37823`. **No platform shell ever emits them** (zero hits in `dll/`). A user can subscribe from any binding language and wait forever.

**A19. The monitor cache is per-window, not app-global.** MEDIUM

`LayoutWindow.monitors: Arc<Mutex<MonitorVec>>` (`layout/src/window.rs:1344`), created fresh per window (`:1719`). N windows hold N private snapshots, each refreshed only by whichever window receives the platform event. The doc claims it's "shared… across all `CallbackInfoRefData`" — true within one window, not across windows.

**A20. `App::get_monitors()` panics off the main thread on macOS.** MEDIUM
```rust
// dll/src/desktop/display.rs:443
let mtm = MainThreadMarker::new().expect("Must be called on main thread");
```
`transient.rs:322` guards against this; the public `App::get_monitors()` (`app.rs:181`) does not.

**A21. Animation velocity assumes 60Hz.** MEDIUM
```rust
// dll/src/desktop/shell2/common/layout.rs:1717
let changes = layout_window.run_track_frames(1.0 / 60.0, ...)
```
`dt` is used as a velocity divisor (`layout/src/window.rs:10293`). On a 144Hz display the reported velocity is 2.4× too small. The event path (`event.rs:3623`) does pass a real `dt`, so this is one of two paths. `VideoMode.refresh_rate` is collected on every platform and never read by any pacing code. Linux's event loop also hard-caps at 16ms for all windows (`run.rs:1926`); macOS is the exception and does per-window `CVDisplayLink` correctly.

### 1.3 What is missing outright

- **Move an existing window to a monitor.** `monitor_id` is deliberately never OS-synced — `common/event.rs:2487` lists it among fields `sync_window_state()` never diffs. Users must read `get_monitors()`, do the arithmetic, and set an absolute position.
- **Fullscreen on a chosen monitor.** Always the window's current monitor. No `_NET_WM_FULLSCREEN_MONITORS` (zero hits), no `SetWindowPos` to a monitor rect on Windows — fullscreen there is `SW_MAXIMIZE` with the style stripped (`windows/mod.rs:2458`), with no saved pre-fullscreen rect, and at creation it collapses to plain maximize (`:1360`) so an app that *starts* fullscreen starts maximized with a title bar.
- **Window geometry persistence.** Nothing anywhere. This is the documented purpose of `MonitorId.hash` (A17).
- **Real video mode enumeration.** `video_modes` always has exactly one element (the current mode) on every platform; `bit_depth` is hardcoded 32 on macOS/X11.
- **Monitor names on X11 and Wayland.** Fabricated (`"CRTC-{i}"`, `"output-{id}"`).
- **`MonitorId` constructors in the FFI.** No `constructors` block in `api.json` — non-Rust callers can read a `MonitorId` but cannot construct one or re-derive a hash.
- **Tooltip edge clamping on all three desktop platforms.** `macos/tooltip.rs:164`, `windows/tooltip.rs:189`, `x11/tooltip.rs:128` — none consult a work area. (Transient windows and window-based menus *do* clamp correctly against the right monitor; it's specifically tooltips that don't.)

### 1.4 Testability — nothing here is testable today

- `layout/src/e2e/runner.rs:238` hands callbacks an **empty** `MonitorVec`. Same empty literal in every widget test harness.
- The headless backend never touches `layout_window.monitors` — the word "monitor" does not appear in `headless/mod.rs`.
- **0 of 54** e2e scenarios mention monitors. `op-dpi-changed.json` simulates the DPI *outcome* in the headless runner; it asserts nothing about monitors, about `WM_DPICHANGED`'s suggested RECT, or about position after a monitor change. Its own description says it "does what WM_DPICHANGED and the X11 DPI path do" — i.e. it never exercises them.
- `event_determination.rs:2355` tests `WindowMonitorChanged` by poking `monitor_id` directly — which no X11 or Wayland backend ever does in production.

**Prerequisite for the whole workstream:** an injectable `MonitorVec` in the headless harness, plus e2e ops to declare a monitor topology and move a window between monitors. Without these, every fix below is unverifiable in CI.

### 1.5 Platform reference — what correct looks like in 2026

Distilled from the web research; only the parts that bear on decisions azul has to make.

#### Windows

- **Declare PMv2 in a manifest, not an API call.** `SetProcessDpiAwarenessContext` returns FALSE/`ERROR_ACCESS_DENIED` if awareness was already set "via a previous API call **or within the application manifest**", and MS states plainly: *"It is recommended that you set the process-default DPI awareness via application manifest, not an API call… Setting the process-default DPI awareness via API call can lead to unexpected application behavior."* The manifest applies before any code runs — before the CRT, static initializers, and any third-party `DllMain` that creates a window. azul currently does the API-call-only form (`windows/dpi.rs:135`) with **no manifest anywhere** (`dll/build.rs` has no `winres`/`embed-resource`/`.rc`). Ship `<dpiAwareness>PerMonitorV2, PerMonitor, System</dpiAwareness>` plus legacy `<dpiAware>true/PM</dpiAware>`, via the [`embed-manifest`](https://docs.rs/embed-manifest/) crate in `build.rs` (works when cross-compiling, needs no MSVC tools). Keep the API call as a fallback **only** for the cdylib-in-a-foreign-host case, and expect ACCESS_DENIED there.
- **`GetDpiForMonitor` is classified by Microsoft as the *non*-DPI-aware call** — its own doc says it "should not be used if the calling thread is per-monitor DPI aware", and the migration table lists `GetDpiForWindow` as its replacement. Use `GetDpiForMonitor(MDT_EFFECTIVE_DPI)` **only** for enumerating a monitor that has no window on it. azul already prefers `GetDpiForWindow` (`dpi.rs:188`), which is right — fixing A8 just restores the fallback tiers.
- **`WM_GETDPISCALEDSIZE` (0x02E4)** is the sanctioned hook for non-linear sizing, sent to top-level PMv2 windows *before* `WM_DPICHANGED`. `lParam`'s in-`SIZE` is *"the pending size of the window after a user-initiated move"* — **not** the current size. Windows Terminal shipped a bug from reading the current size ([terminal#18268](https://github.com/microsoft/terminal/pull/18268)). azul doesn't handle it; it's optional, but note it exists if text-grid snapping ever matters.
- **PMv2 also gives child-window DPI notifications** (`WM_DPICHANGED_BEFOREPARENT` 0x02E2 bottom-up, `WM_DPICHANGED_AFTERPARENT` 0x02E3 top-down) and makes `EnableNonClientDpiScaling` unnecessary. azul calls the latter from `WM_NCCREATE` (`windows/mod.rs:3604`) — harmless, but redundant under a real PMv2 manifest.
- **Signed 16-bit coordinates.** *"the coordinates of the virtual screen are represented by a signed 16-bit value"*. In Rust, `(lparam & 0xFFFF) as i32` is wrong on any monitor left of or above the primary; the correct `GET_X_LPARAM` semantics are `(lparam as u32 & 0xFFFF) as i16 as i32`. azul does this correctly (`windows/mod.rs:2258, 4037, 4125` all use `as i16 as i32`) — worth a lint so it stays that way.
- **Stable identity:** `szDevice` (`\\.\DISPLAY1`) is a session ordinal, `HMONITOR` is explicitly invalidated by `WM_DISPLAYCHANGE`. Real identity is `QueryDisplayConfig` → `DISPLAYCONFIG_TARGET_DEVICE_NAME` (`monitorDevicePath`, `monitorFriendlyDeviceName`, `edidManufactureId`, `edidProductCodeId`), joined back to `HMONITOR` via `DISPLAYCONFIG_SOURCE_DEVICE_NAME.viewGdiDeviceName == MONITORINFOEXW.szDevice`. Cheaper alternative: `EnumDisplayDevicesW(..., EDD_GET_DEVICE_INTERFACE_NAME)` → `DeviceID`.
- **Persistence: use `GetWindowPlacement`/`SetWindowPlacement`, not raw rects.** It stores min/max/normal atomically and auto-corrects a completely-off-screen restore. **Gotcha:** for non-`WS_EX_TOOLWINDOW` top-levels these are *workspace* coordinates, not screen coordinates — never feed them to `SetWindowPos`.
- **Don't fight Windows 11's arrangement memory.** It keys on display *topology* (connection set, positions, scaling, primary). On `WM_DISPLAYCHANGE` the OS is already moving windows; intervene only if, after debounce, a window intersects **no** monitor.
- **Refresh rate:** `DwmGetCompositionTimingInfo` **cannot** be used per-window — since Windows 8.1 the `hwnd` parameter *must* be NULL or it returns `E_INVALIDARG`. Per-monitor rate comes from `EnumDisplaySettingsW(szDevice, ENUM_CURRENT_SETTINGS).dmDisplayFrequency` (integer) or `QueryDisplayConfig`'s rational `refreshRate`. For real pacing, DXGI — `DXGI_OUTPUT_DESC.Monitor` *is* an `HMONITOR`, so outputs join to your monitor list.

#### macOS

- **`NSScreen.main` is the *key window's* screen, not the menu-bar screen.** `screens[0]` is the menu-bar screen and the coordinate-space anchor. azul's MWA-B9 convention already gets this right and `macos/tooltip.rs:148` documents the exact trap.
- **The Y-flip anchor is the primary screen's height**, and it *changes* when the user moves the menu bar to another display or changes its resolution — so `h0` must be recomputed on every screen-parameter change, and stored logical positions re-derived. azul recomputes it per call (`primary_screen_height()`), which is correct but means it depends on a non-stale `NSScreen.screens`.
- **`NSWindow.frame`/`NSScreen.frame` are always in points; `backingScaleFactor` never changes them.** This is why macOS has no `WM_DPICHANGED` equivalent: dragging 2×→1× doesn't move or resize the window, it only invalidates the backing store. What you must do in `viewDidChangeBackingProperties` is set `layer.contentsScale` and the drawable size. Prefer `convertRectToBacking:` over multiplying by `backingScaleFactor` — Apple: *"The `backingScaleFactor` method should not be used except in the rare case when the explicit scale factor is needed."*
- **macOS "scaled" resolutions render at 2× then downsample; `backingScaleFactor` stays 2.0.** You never see a fractional value. Fractional-scaling bugs are a Windows/X11/Wayland problem only.
- **Use both reconfiguration hooks.** `NSApplicationDidChangeScreenParametersNotification` is the primary one — it is the only thing that fires for `visibleFrame` changes (Dock moved/resized, menu-bar auto-hide), which CoreGraphics does not report at all. `CGDisplayRegisterReconfigurationCallback` gives you *which* display and *how*, plus a `kCGDisplayBeginConfigurationFlag` phase to tear down GL/Metal before a mode switch. azul registers **neither** (A5); `windowDidChangeScreenProfile:` is the ColorSync notification and additionally needs the deprecated `displaysWhenScreenProfileChanges`, which is never set — so it effectively never fires.
- **Stable identity is `CGDisplayCreateUUIDFromDisplayID`** — unique and consistent across reboots even when `CGDirectDisplayID` changes. Two traps: (a) Rust has repeatedly broken linking to it ([rust#91372](https://github.com/rust-lang/rust/issues/91372), [winit#2078](https://github.com/rust-windowing/winit/pull/2078)) — `dlsym` it or link `ApplicationServices` explicitly; (b) **it fails during a display-*removal* callback, even at begin-configuration**, so build the UUID map at *add* time.
- **`CGDirectDisplayID` changes on GPU switch** on dual-GPU MacBooks. GLFW works around it by matching on `CGDisplayUnitNumber` instead, with the comment *"HACK: Compare unit numbers instead of display IDs to work around display replacement on machines with automatic graphics switching."* Relevant to azul because `macos/mod.rs:3863` keys on the raw display id.
- **Fullscreen on a chosen monitor requires move-then-toggle-then-verify.** There is no `toggleFullScreenOnScreen:`. And with "Displays have separate Spaces" **off**, fullscreening one window blanks the other displays — you can't read that setting via public API, so never assume `visibleFrame.height == frame.height` on a secondary.
- **`CVDisplayLink` is deprecated as of macOS 14.** The replacement `-[NSView displayLinkWithTarget:selector:]` **auto-retargets when the view changes display** — exactly the multi-monitor correctness azul currently hand-rolls at `macos/mod.rs:5194-5217`. Worth adopting behind a version check.

#### X11

- **Use RandR 1.5 `XRRGetMonitors`.** It exists specifically to fix the cases the CRTC walk gets wrong *in both directions*: a tiled MST 4K/5K panel appears as two outputs/CRTCs (Dell UP2414Q "misdetected as being multiple monitors"), and `xrandr --setmonitor` lets a user split one physical panel into two logical monitors — only 1.5 reports the split. `XRRMonitorInfo` gives `name` (an Atom), `primary`, `automatic`, `x/y/width/height`, `mwidth/mheight`, and the output list. azul uses the 1.2-era `XRRGetScreenResourcesCurrent` + CRTC walk (A10).
- **`XRRUpdateConfiguration` is mandatory**, on *every* event: *"Clients must call back into Xlib using `XRRUpdateConfiguration` when screen configuration change notify events are generated… to update Xlib's view of the resolution, size, rotation, reflection or subpixel order."* Skip it and `DisplayWidth()`/`DisplayHeight()` silently return pre-change values forever. azul never calls it.
- **Per-monitor DPI on X11 is not achievable, and a new toolkit should not pretend otherwise.** The server exposes one DPI. GTK's `GDK_SCALE` is integer and process-wide; `GDK_DPI_SCALE` fine-tunes text only; Qt's `QT_SCREEN_SCALE_FACTORS` works only because Qt scales entirely in userspace above a physical-pixel X11. **Recommendation: expose the same `scale_factor` for every X11 monitor and document it.** Derivation order: explicit env override → `Xft.dpi` from `RESOURCE_MANAGER` → XSETTINGS `Gdk/WindowScalingFactor`+`Xft/DPI` → physical mm, *clamped hard* (winit shipped [#1983 "nonsensical scaling factor (featuring insane xrandr data)"](https://github.com/rust-windowing/winit/issues/1983) from trusting `mwidth`). azul's chain is already `Xft.dpi` → mm-estimate → 96; the fix for A10 is to **stop pretending it's per-monitor**, not to compute a better per-CRTC value.
- **`_NET_WORKAREA` is per virtual desktop, not per monitor, and WMs don't agree on it** — xfwm4 uses the largest feasible rectangle, metacity the largest square area, KWin the square sum *ignoring struts*. The per-monitor `_NET_WORKAREAS` was proposed for years and never adopted; KWin is X11-feature-frozen and won't add it. **GTK4 removed `gdk_monitor_get_workarea()` from the cross-platform API for exactly this reason.** So computing per-monitor work areas yourself is mandatory: walk `_NET_CLIENT_LIST`, read `_NET_WM_STRUT_PARTIAL` (12 CARDINALs, with `_start`/`_end` ranges — this is what makes it usable in multi-monitor at all), fall back to `_NET_WM_STRUT`, and subtract each strut's reserved band from the monitors it intersects. Note struts are measured from the **root window** edges, not monitor edges — a known EWMH deficiency.
- **Watch `PropertyNotify` on root** for `RESOURCE_MANAGER` (live `Xft.dpi` change) and `_NET_CLIENT_LIST`/`_NET_WORKAREA` (re-run the strut computation).
- **`_NET_WM_FULLSCREEN_MONITORS`** must be sent as a *client message to the root*, not `XChangeProperty`'d. Its indices are **Xinerama's**, not RandR's. WM support is uneven (i3, Hyprland-XWayland, awesome all shipped it broken) — always have a move-then-fullscreen fallback.

#### Wayland

- **`xdg_output` is only *partially* deprecated, and azul needs the part that isn't.** `name`/`description`/`done` are superseded by `wl_output` v4 and `wl_output.done`. But **`logical_position` and `logical_size` have no `wl_output` equivalent**: a 3840×2160 mode at fractional scale 1.5 advertises a logical size of 2560×1440, while `wl_output.mode` reports 3840×2160 and `wl_output.scale` reports the *rounded integer* 2 — so logical size is **not derivable** from `wl_output` alone. Binding `xdg_output` is exactly what would let azul delete the four-subprocess detection chain (A12).
- **`wl_output.geometry`'s x/y are explicitly untrustworthy** — the spec warns compositors "might fake this information" and redirects clients to `xdg_output.logical_position`. Likewise `make`/`model` → `name`/`description`, and `wl_output.transform` → `wl_surface.preferred_buffer_transform`.
- **`wl_surface.preferred_buffer_scale` (wl_compositor v6) supersedes the enter/leave union** for integer scale. azul binds `wl_compositor` at `min(4, …)` (`events.rs:390`) and `wl_output` at `min(3, …)` (`events.rs:520`) — both version caps block the modern events, and the caps exist because the listener structs are sized for the older versions (`defines.rs:507`, `:535`). Raising them requires extending the listener arrays; doing it without extending them is a buffer overrun.
- **`wp_fractional_scale_v1` + `wp_viewporter` is the target, and support is effectively universal in 2026** — Mutter 49, KWin 6.6, Sway 1.11, Hyprland 0.52, COSMIC, niri, river, Wayfire, Weston, labwc, Jay, GameScope. The correct sequence: buffer at `round_half_away_from_zero(logical × n/120)`, `viewport.set_destination(logical_w, logical_h)`, **`set_buffer_scale(1)`**, attach + damage + commit **in one commit**. azul's CPU path does exactly this (`mod.rs:3921-3926`); the GPU path does none of it (A14). Note SDL shipped a `bad_value` client-kill from setting destination and attaching in *different* commits ([SDL#9283](https://github.com/libsdl-org/SDL/issues/9283)).
- **GNOME still gates fractional scaling** behind `gsettings set org.gnome.mutter experimental-features "['scale-monitor-framebuffer']"` as of the GNOME 49 line.
- **`xdg_toplevel.configure_bounds` (xdg_shell v4)** is Wayland's work-area analogue: *"the compositor can tell the client the maximum recommended window size… can for example correspond to the size of a monitor excluding any panels."* Sent before `configure`; `0,0` means unknown. This is the right source for `Monitor::work_area()` on Wayland — not `xdg_output.logical_size`.
- **Manual popup clamping is wrong on Wayland for three independent reasons:** you don't know where you are; the constraining region isn't the output rect (it's output minus panels minus exclusive layer-shell zones, or the *tile* under a tiling compositor); and you'd double-constrain with the compositor's own adjustment. azul already does this correctly via `xdg_positioner`. The one remaining gap is honouring `xdg_popup.configure(x,y,w,h)` — a popup that ignores it and draws at its requested size is the Wayland version of the wrong-monitor bug.
- **Session restore exists: `xx-session-management-v1`.** Merged in KWin, shipped in **KDE Plasma 6.4** (June 2025); Chromium has merged support. Still flagged experimental (*"Backwards incompatible major versions of the protocol are to be expected"*). The key insight for azul's persistence design: **on Wayland the restore key is a compositor-issued opaque token you persist, not a monitor identity you compute.**

### 1.6 Recommended design

Four decisions, in dependency order.

**D1 — Make the unit lie impossible in the type system.** This is the root fix for A1/A2/A3/A15 and the prerequisite for everything else.

`PhysicalPositionI32` already exists. The bug is that `LogicalPosition` values are hand-cast into it in five places (`transient.rs:283, 365, 645, 716`, `callbacks.rs:3231`). Introduce a `LogicalPositionI32` (or make `RelativeToParentWindow` carry `LogicalPosition`) and every one of those becomes a compile error rather than a silent 2× bug. Then pick **one** meaning for `WindowPosition::Initialized` — absolute virtual-screen physical px, matching the existing doc — and fix the three creation paths to match `sync_window_state`, not the other way round.

**D2 — Normalise `DisplayInfo`/`Monitor` to one unit convention.** Carry the monitor rect in **backend-native physical px** plus `scale_factor`, and let macOS convert points→physical on the way in (it's the only backend where they differ). Fix the three contradictory doc comments. Do **not** build a global *logical* virtual-screen: that's what produces Qt's "islands-of-screens" gaps and Chromium's accumulating floor/ceil drift.

**D3 — Make the public API honest about what each backend can answer.** Follow what GTK4 and Qt6 deliberately *dropped*:

- `get_current_monitor()` → keep returning `OptionMonitor`, but make it actually work: resolve by **largest intersection** (the `MonitorFromWindow` rule), not by point containment. This is the fix for the wrong-monitor popup class of bug — it's exactly what Firefox's `GetConstrainedRect` got wrong for years ([bug 575328](https://bugzilla.mozilla.org/show_bug.cgi?id=575328)).
- Add `Window::request_position()` as explicitly best-effort, and keep position getters `Option` — **never return `Some((0,0))` on Wayland**. azul already models this correctly (`wayland/mod.rs:2321`, `:7879`); just don't regress it.
- `Monitor::work_area()` should be `Option` — on X11 you may genuinely not know it, and on Wayland it comes from `configure_bounds`.
- Reconsider `is_primary_monitor`: GTK4 dropped the concept because there is no primary on Wayland. Keeping it is defensible for azul (three of four backends have one) but it must not be load-bearing.
- Drop the pretence of per-monitor `scale_factor` on X11 — return the same global value for every monitor and say so in the doc.

**D4 — One app-global monitor cache, one debounced event.** Replace the N per-window `Arc<Mutex<MonitorVec>>` (A19) with a single app-level cache, refreshed on a debounced (100–250ms) reconfiguration signal, and emit `MonitorConnected`/`MonitorDisconnected` from it — closing A18. Dock/undock and wake-from-sleep produce bursts of add/remove including phantom displays on **every** platform; acting per-event produces visible window thrash.

Stable identity per platform (for the persistence story that doesn't exist yet):

| Platform | Key |
|---|---|
| Windows | `QueryDisplayConfig` → `monitorDevicePath` + `edidManufactureId`/`edidProductCodeId` |
| macOS | `CGDisplayCreateUUIDFromDisplayID`, cached at *add* time |
| X11 | EDID from `XRRGetOutputProperty("EDID")` + RandR monitor/connector name |
| Wayland | `xx-session-management-v1` token if available; else `wl_output.name` + `description` (weak — names may be reused after a global is destroyed) |

Match on restore with a **scored fallback** (exact key 100 → connector+resolution 80 → mfr+model+resolution 60 → resolution+arrangement position 40 → 0 = clamp to primary work area), and store the whole *arrangement*, not one monitor — Windows 11's own feature keys on topology for this reason.

### 1.7 Work plan — multi-monitor

Ordered by dependency. Sizes are rough.

**Phase 0 — testability (prerequisite, ~2–3 days).** Inject a `MonitorVec` into the headless shell and `layout/src/e2e/runner.rs:238`; add e2e ops to declare a monitor topology and to move a window between monitors; add a `monitor_changed` op. Without this every phase below is unverifiable. This also lets the existing `op-dpi-changed.json` be extended into a real cross-monitor DPI test.

**Phase 1 — the unit fix (D1), ~3–4 days.** `LogicalPositionI32` newtype; fix the five cast sites; unify `WindowPosition::Initialized` semantics across the three creation paths; fix `get_cursor_position_screen` (A15). Fixes A1, A2, A15, and most of A3's downstream damage. Bug-class test: a popup at a known offset under a synthetic 2× scale.

**Phase 2 — cheap correctness wins, ~1–2 days.** All independent, all small:
- A8: move two symbols to `shcore.dll` (2 lines) + add the PMv2 manifest via `embed-manifest`.
- A7: add the missing Y-flip at `macos/mod.rs:8120`.
- A4/macOS: store the enumeration index, not the `CGDirectDisplayID`.
- A6: pass `options.window_state.monitor_id` through on Windows and X11.
- A20: make `App::get_monitors()` non-panicking off-main-thread on macOS.

**Phase 3 — wire `get_current_monitor()` everywhere (~3 days).** X11: set `ws.monitor_id` at map time and on `ConfigureNotify` (and fix A11 by caching — the current per-drag-pixel `XOpenDisplay` is the reason this is expensive). Wayland: call the existing `get_current_monitor_id()` and delete the index-correlation in `get_current_monitor()` (A13). macOS: call `detect_current_monitor()` from `windowDidChangeScreen:` (A5). Then A18's events can actually fire.

**Phase 4 — Linux truth (~4–5 days).** X11: switch to `XRRGetMonitors`, add `XRRUpdateConfiguration`, read real connector names and `XRRGetOutputPrimary`, implement `_NET_WM_STRUT_PARTIAL` work-area computation (A9), and **stop claiming per-monitor DPI** (A10). Wayland: bind `xdg_output` and delete the subprocess detection chain (A12); implement `registry.global_remove` (A13); raise the `wl_output`/`wl_compositor` version caps *with* correspondingly extended listener structs.

**Phase 5 — the GPU scale bug (A14, ~1–2 days).** Wire buffer scale / viewport into the Wayland EGL path. Currently HiDPI + GPU renders a `logical×2` viewport into a `logical×1` buffer.

**Phase 6 — features (~1 week).** Move-window-to-monitor; fullscreen-on-a-chosen-monitor; window geometry persistence with the identity scheme from D4; tooltip edge clamping on all three desktop backends. Optionally: per-monitor frame pacing (A21) — `refresh_rate` is already collected on every platform and read by nothing.

**Rough total: 3–4 weeks** for a correct, tested multi-monitor story. Phases 0–3 (~1.5 weeks) get you the majority of the user-visible fix.

---

## PART II — SYSTEM TRAY

### 2.1 Current state

Nothing, on every platform. Zero hits repo-wide for `NSStatusBar`, `NSStatusItem`, `Shell_NotifyIcon`, `NOTIFYICONDATA`, `StatusNotifierItem`, `org.kde.StatusNotifierWatcher`, `com.canonical.dbusmenu`, `libappindicator`, `_NET_SYSTEM_TRAY_S0`, `XEMBED`. No tray commit in history. The only references anywhere are two aspirational doc lines (`scripts/ARCH_TODO.md:320`, `scripts/CALLBACK_INVOCATION_UNIFICATION.md:347`), and `scripts/SCRIPTS_AUDIT_2026_08_01.md:2411` already marks the latter as speculative: *"§5's speculative callback sources (notification / tray / file-watcher) do not exist."*

**What azul already has that a tray reuses:**

| Piece | Where | Value to a tray |
|---|---|---|
| Platform-agnostic `Menu`/`MenuItem`/`CoreMenuCallback` | `core/src/menu.rs:126, 243, 387` | The whole menu model, unchanged |
| `create_nsmenu()` | `macos/menu.rs:236` | macOS tray menu, verbatim |
| `recursive_construct_menu()` → `HMENU` | `windows/menu.rs:58` | Windows tray menu, verbatim |
| Deferred `TrackPopupMenu` + `SetForegroundWindow` | `windows/mod.rs:5755-5783` | Already implements the two-part dismissal workaround |
| `WindowIcon` — RGBA small/large + `IconKey` | `core/src/window.rs:2109-2145` | The tray icon type |
| `DBusLib` dlopen wrapper (72 symbols) | `linux/dbus/dlopen.rs` | Transport for SNI |
| `get_shared_dbus_lib()` `OnceLock` singleton | `gnome_menu/shared_dbus.rs:18` | Reusable as-is |
| Deferred-callback mailbox (`queue_menu_callback`/`drain_pending_menu_callbacks`) | `gnome_menu/actions_protocol.rs:31-63` | Exactly the pattern a tray needs — the D-Bus handler thread can't hold a `CallbackInfo` |
| `dbus_bus_name_has_owner` availability probe | `dbus/mod.rs:25-47` | Retarget one string literal to probe `org.kde.StatusNotifierWatcher` |

**Correction to an assumption worth stating plainly:** `linux/gnome_menu/` implements **`org.gtk.Menus` + `org.gtk.Actions`** (`protocol_impl.rs:164, 326`), *not* `com.canonical.dbusmenu`. The only `com.canonical.*` reference in the tree is a presence check for `com.canonical.AppMenu.Registrar`. SNI's `Menu` property points at a **dbusmenu** object — a recursive `(id, a{sv}, av)` tree with `GetLayout(parentId, recursionDepth, propertyNames)` — which is a different model from `org.gtk.Menus`' flat `(group_id, menu_id)` scheme. **The protocol layer is a rewrite, not an adaptation.** Roughly 30% of the Linux work is reusable, and it's the boring 30%.

**Blockers in the existing D-Bus layer** (none are in `dbus/dlopen.rs` today):
- **`dbus_message_new_signal` — hard blocker.** SNI requires emitting `NewIcon`/`NewStatus`/`NewAttentionIcon`/`NewToolTip`; dbusmenu requires `LayoutUpdated`/`ItemsPropertiesUpdated`. Today the code can only *reply*, never *emit*.
- `dbus_bus_add_match` + `dbus_connection_add_filter` — to watch `NameOwnerChanged` and re-register when the tray applet restarts (plasmashell/waybar restarts are routine).
- No `org.freedesktop.DBus.Properties` handler anywhere — **SNI is ~90% properties**. Also no `Introspectable`, which several hosts call first.
- `dbus_message_iter_append_fixed_array` — for `IconPixmap` (`a(iiay)`); doable byte-at-a-time but that's 4 calls per pixel.
- `GnomeMenuManager` is not a generic service host: bus name and object path are hardcoded to the GTK convention (`manager.rs:79-80`), and `new()` fuses connect + `request_name` + register-two-fixed-interfaces into one non-parameterisable function. There is no `register_service(bus_name, path, vtable)` to extract.

**Two pre-existing bugs in that D-Bus code, found on the way** (worth fixing regardless of tray work):
- **`gnome-menus` is not a declared Cargo feature** in any manifest, so `x11_properties.rs:32` always compiles to the `NotImplemented` branch (`:91`), `manager.rs:255` propagates the `Err`, and `x11/mod.rs:2622` logs a warning and drops the manager. **The X11 GNOME global menu can never activate.** The test at `x11_properties.rs:166` asserts this failure as correct behaviour.
- **Wayland never dispatches its D-Bus connection.** `process_messages()` has exactly one caller (`x11/mod.rs:1589`). Wayland constructs a manager (`wayland/mod.rs:2168`), stores it (`:2186`), syncs menus into it (`:2708`), drains callbacks (`:2743`) — and never calls `process_messages`, so `dbus_connection_read_write_dispatch` never runs and no incoming `org.gtk.Menus.Start` is ever answered.

### 2.2 Platform reference

The three platforms share nothing below the "TrayIcon + retained Menu tree" level.

| | macOS | Windows | Linux |
|---|---|---|---|
| Icon owner | your process (AppKit object) | the shell (you hand it an `HICON`) | your process (D-Bus properties the panel reads) |
| Menu | real `NSMenu`, AppKit draws it | real `HMENU`, you call `TrackPopupMenu` | **a remote model** over D-Bus — the panel draws it |
| Re-registration trigger | n/a | `TaskbarCreated` broadcast | `NameOwnerChanged` on the watcher |
| Icon format | `NSImage`, template | `HICON` at per-DPI size | ARGB32 **big-endian**, inline over D-Bus |
| "Is a tray available?" | always | practically always | **must be probed** |

**The single most-forgotten thing is the re-registration event.** Build it in on day one, and make it the *same* code path as initial creation so it can't rot.

**The second is that the Linux menu is not a popup.** The panel calls back into you asking for the layout. If the API is designed as `show_context_menu_at(x, y)` it will have to be redone — it must be a **retained tree with stable integer ids and a revision counter**.

#### macOS — `NSStatusItem`

```objc
NSStatusItem *item = [[NSStatusBar systemStatusBar]
                        statusItemWithLength:NSSquareStatusItemLength];  // -2.0
item.button.image = img;        // 18×18 pt, isTemplate=YES, black+alpha only
item.button.imagePosition = NSImageOnly;
item.autosaveName = @"org.example.app.tray";
item.menu = menu;               // AppKit opens on mouse-DOWN, both buttons
```

- **The retain rule is load-bearing.** Apple: *"the system status bar … cannot retain references to each application's status item objects … you must retain the object returned by `statusItemWithLength:`"*. In Rust the `Retained<NSStatusItem>` must outlive the app loop; dropping it *is* the removal API. Main-thread only.
- Everything moved to `item.button` (`NSStatusBarButton`) in 10.10; the old `NSStatusItem` image/title/action/target/toolTip/`sendActionOn:` are deprecated (objc2 marks them so).
- **Template images** are the dark-mode mechanism: alpha-only mask, AppKit tints per appearance/highlight. Author black + alpha; a coloured icon will look broken and that's not a bug.
- Attaching a menu means `button.action` **never fires** — the menu swallows the click. For click-vs-menu discrimination use `sendActionOn:` and read `NSApp.currentEvent`. `popUpStatusItemMenu:` is deprecated since 10.14; the supported idiom is assign-menu → `performClick:` → unassign.
- **Set `autosaveName`.** It persists the item's bar position; without one the system invents one, which is why third-party items lost their order on Big Sur ([FB8732253](https://github.com/feedback-assistant/reports/issues/151)).
- **There is no API for the notch/overflow problem.** Items are silently dropped when the bar runs out of room right of the notch; `isVisible` reports *intent*, not on-screen visibility, and returns `true` for a hidden item. macOS 26 "Tahoe" made this worse — there are field reports of a `NSStatusItemChangeVisibilityAction` ping-pong between the app and ControlCenter every ~200ms. **Practical rule: set `isVisible` once, never poll-and-reassert, and never build a feature that assumes the icon is on screen.**
- **Tray-first apps want `NSApplicationActivationPolicyAccessory`**, ideally via `Info.plist` `LSUIElement=1` to avoid a Dock-icon flash. azul hardcodes `Regular` at `run.rs:607` and `macos/mod.rs:4096` — this needs to become configurable.

#### Windows — `Shell_NotifyIconW`

```c
nid.cbSize = sizeof(NOTIFYICONDATAW);
nid.uFlags = NIF_MESSAGE|NIF_ICON|NIF_TIP|NIF_SHOWTIP;
Shell_NotifyIconW(NIM_ADD, &nid);
nid.uVersion = NOTIFYICON_VERSION_4;
Shell_NotifyIconW(NIM_SETVERSION, &nid);      // AFTER every ADD, including re-adds
```

- **Do NOT use `HWND_MESSAGE`.** Message-only windows are children of `HWND_MESSAGE`, therefore not top-level, therefore **invisible to broadcast messages** ([Raymond Chen](https://devblogs.microsoft.com/oldnewthing/20171218-00/?p=97595)). `TaskbarCreated` is a broadcast — your icon would never come back after an Explorer restart. Use a **real top-level window that is never shown**: `WS_EX_TOOLWINDOW`, `WS_OVERLAPPED`, no `WS_VISIBLE`, never `ShowWindow`. azul has neither `HWND_MESSAGE` nor `RegisterWindowMessage` anywhere today, so this is new infrastructure — but it fits the existing window machinery.
- **`TaskbarCreated` handling is mandatory**: `RegisterWindowMessage(TEXT("TaskbarCreated"))` in `WM_CREATE`, re-add on receipt. Note it *also* fires when the primary display's DPI changes on Windows 10 — so it's your cue to rebuild the `HICON` too.
- **v4 message encoding — note the swap:** `LOWORD(lParam)` = event, `HIWORD(lParam)` = icon id (**16-bit only**), and `GET_X_LPARAM(wParam)`/`GET_Y_LPARAM(wParam)` = **screen** coords. Getting `wParam`/`lParam` backwards is the classic v4 bug. Dispatch `NIN_SELECT`/`NIN_KEYSELECT` → activate, `WM_CONTEXTMENU` (0x007B) → menu (covers keyboard invocation too), `WM_MBUTTONUP` → secondary.
- **The two-part menu workaround:** `SetForegroundWindow(hwnd)` before `TrackPopupMenu` (or the menu won't dismiss on outside-click), and `PostMessage(hwnd, WM_NULL, 0, 0)` after it returns (or the *next* invocation flashes open and closes). azul's existing deferred-menu path at `windows/mod.rs:5755-5783` already has the `SetForegroundWindow` half.
- **Icon DPI:** size to `GetSystemMetricsForDpi(SM_CXSMICON, dpi)` (16/20/24/32 at 100/125/150/200%). Generate from RGBA via `CreateDIBSection` + `CreateIconIndirect` — you own the `HICON`; `NIM_ADD` doesn't take ownership.
- **Avoid `NIF_GUID`.** GUID identity is bound to the **exe path**; move the binary and the registration is lost (unless both are Authenticode-signed by the same company). For a toolkit whose apps run from `target/release/`, from Downloads, from per-version dirs, it's a footgun. Use `hWnd + uID`.
- **Windows 11 hides new tray icons by default** (22H2+ removed "Always show all icons"); visibility is per-icon user state keyed on the executable, so **an app update re-hides the icon**. Consequence: don't treat the tray as a discoverable first-run entry point, and don't write the undocumented `IsPromoted` key yourself.
- **Notifications:** `NIF_INFO` balloons need zero registration and work on 10 and 11 (though Win11 made them transient rather than Action-Center-persistent). Modern toasts need an AUMID **and a Start Menu shortcut**, or the WinAppSDK runtime. For a dlopen-only toolkit: **ship `NIF_INFO` as the built-in path, expose an optional AUMID hook** for apps that have an installer.

#### Linux — StatusNotifierItem

Registration sequence:
1. Own `org.kde.StatusNotifierItem-<pid>-<n>` on the session bus.
2. Export `org.kde.StatusNotifierItem` at `/StatusNotifierItem`.
3. Export `com.canonical.dbusmenu` (conventionally `/MenuBar`), point the `Menu` property at it.
4. Call `org.kde.StatusNotifierWatcher.RegisterStatusNotifierItem` at `/StatusNotifierWatcher`.
5. **Watch `NameOwnerChanged` for the watcher and redo step 4 when it returns.**

- **Use `org.kde.*`, not `org.freedesktop.*`.** The published fd.o text says `org.freedesktop.StatusNotifierItem-PID-ID`, but the reference implementation and the entire KDE stack use `org.kde.`. This is not academic: Electron 43+ switched to well-known `org.freedesktop.*` names and **Waybar stopped showing those icons** ([Waybar#5240](https://github.com/Alexays/Waybar/issues/5240)).
- The watcher accepts either a bus name (path defaults to `/StatusNotifierItem`) or a path (it then uses your unique `:1.x` sender). Belt-and-braces: own the well-known name; if no icon appears, re-register passing `"/StatusNotifierItem"`.
- **Availability = watcher name owned AND `IsStatusNotifierHostRegistered == true`.** Watcher-owned-but-no-host is a real state (the watcher can race the session — [cinnamon#13740](https://github.com/linuxmint/cinnamon/issues/13740)), so **poll/retry rather than deciding once at startup**.
- **Prefer `IconPixmap` over `IconName`.** `IconName` needs the icon installed in an XDG theme; `IconThemePath` is non-standard and ignored by several hosts. `IconPixmap` (`a(iiay)`, ARGB32 **network byte order** — on little-endian emit `[A,R,G,B]` per pixel) is self-contained: no install, no theme, no filesystem. Send 2–3 sizes (22/24/48) and let the host pick. This maps directly onto azul's existing `WindowIcon` RGBA bytes.
- **Emit both the `New*` signals and `PropertiesChanged`** — several hosts (notably KDE historically) don't react to `PropertiesChanged` for SNI.
- **dbusmenu is a retained remote model.** `GetLayout(parentId, recursionDepth, propertyNames) → (revision, (ia{sv}av))`, `Event(id, eventId, data, timestamp)` with eventIds `clicked`/`hovered`/`opened`/`closed`, `AboutToShow(id) → needUpdate`. Root id is 0. **Bump `revision` and emit `LayoutUpdated` on every tree change** — hosts cache aggressively; Electron documents the user-visible symptom (*"you have to call `setContextMenu` again"*). Note `icon-data` here is **PNG bytes**, unlike SNI's raw ARGB — different encoding in the same feature.
- **Desktop matrix 2026:** KDE Plasma native; **vanilla GNOME has no watcher at all** — registration fails silently and nothing appears, so azul must detect this and degrade to *something*; GNOME + the AppIndicator extension works; XFCE 4.16+ works; Cinnamon works via `xapp-sn-watcher` (with a documented startup race causing duplicate icons); Waybar/sway is a real host; Hyprland delegates to whatever bar the user runs.
- **Skip XEmbed.** It's X11-only (dead under Wayland), makes you responsible for rendering into a reparented window with the tray's `_NET_SYSTEM_TRAY_VISUAL`, and duplicates icons where a bridge already exists. Document `snixembed` in the README instead. (Caveat worth knowing: GNOME's *official* status-icons extension is XEmbed-only — but that path still requires the user to enable an extension, and if they're doing that, AppIndicator is the better one to point them at.)
- **There is no Wayland tray protocol.** D-Bus SNI is display-server-agnostic, which is exactly why it won. The only Wayland-adjacent piece is `ProvideXdgActivationToken` for focus transfer when a tray click should raise a window.
- **Model to copy: [`ksni`](https://github.com/iovxw/ksni)** — pure D-Bus, no GTK, correct watcher-offline/online handling, used by Thunderbird. **Model to avoid: `tray-icon`** (Tauri) — requires GTK + libxdo + libayatana, and its bug list is a catalogue of why: [#336](https://github.com/tauri-apps/tray-icon/issues/336) is "icon never appears on Plasma 6/Wayland and `.build()` returns no error." Tauri has an open PR to switch to `ksni`.

### 2.3 Recommended design

```rust
// One RGBA source, per-platform conversion inside.
TrayIcon::new(rgba: &[u8], w: u32, h: u32, tooltip: &str, id: &str) -> Result<TrayIcon>

// A RETAINED tree with stable u32 ids and a revision counter.
// Not show_menu_at(x, y) — Linux needs to query this.
TrayIcon::set_menu(&mut self, menu: &azul_core::menu::Menu)

// Events. Note ContextMenu is a REQUEST, not a command:
// on Linux the panel may draw the menu itself.
enum TrayEvent { Activate, SecondaryActivate, ContextMenu, Scroll { delta, axis }, MenuItem(u32) }

// Means something on Linux, true elsewhere.
TrayIcon::is_available() -> bool
```

Plus an internal "host restarted, re-publish everything" path wired to `TaskbarCreated` (Windows) and `NameOwnerChanged` (Linux) — **the same code path as initial creation**.

Two structural additions azul needs:
- **An app-level object to own the tray.** macOS state currently all hangs off `MacOSWindow`; Windows has no hidden top-level window. The natural homes are `run.rs:607` (macOS, where `NSApplication` is configured before any window exists) and `run.rs:1077` (Windows).
- **`Shell_NotifyIconW` in `Shell32Functions`** (`windows/dlopen.rs:623`) — shell32 is already loaded for drag-and-drop, on an optional/graceful-degradation path.

Also note the API must **not** document a precise click semantic on Linux. Electron's docs put it well: *"the StatusNotifierItem spec does not specify which action would cause an activation; for some environments it is left mouse click, but for some it might be double left mouse click."*

### 2.4 Work plan — system tray

**Build order is Windows → macOS → Linux**, easiest and most-specified first.

**Phase 1 — Windows (~400 LOC, 3–4 days).** Hidden top-level `WS_EX_TOOLWINDOW` window; `Shell_NotifyIconW` in `Shell32Functions`; `NIM_ADD` + `NIM_SETVERSION(4)`; `TaskbarCreated` re-registration; RGBA→`HICON` via `CreateDIBSection`+`CreateIconIndirect`; route a new `WM_APP_TRAY` in `window_proc`; reuse `recursive_construct_menu` + the existing deferred `TrackPopupMenu` path for the context menu.

**Phase 2 — macOS (~350 LOC, 3 days).** `NSStatusBar`/`NSStatusItem`/`NSStatusBarButton` at `run.rs:607`, with the `Retained<>` stored in a new app-level struct; RGBA→`NSImage` via `NSBitmapImageRep` with `isTemplate`; reuse `create_nsmenu` verbatim; `autosaveName`; make `setActivationPolicy` configurable so tray-first apps can be `Accessory`. Build nothing on `isVisible`.

**Phase 3 — Linux (~900 LOC, 1.5–2 weeks).** The bulk. In order:
1. Extend `dbus/dlopen.rs`: `dbus_message_new_signal`, `dbus_bus_add_match`, `dbus_connection_add_filter`, `dbus_message_iter_append_fixed_array`, `dbus_message_get_path`/`_get_sender`, `dbus_connection_get_unix_fd`.
2. Extract a generic `register_service(bus_name, path, vtable)` from `GnomeMenuManager::new` — and while there, fix the two pre-existing bugs (undeclared `gnome-menus` feature; Wayland never calling `process_messages`).
3. Add an `org.freedesktop.DBus.Properties` handler (+ `Introspectable`) — SNI is ~90% properties.
4. Implement `org.kde.StatusNotifierItem` with `IconPixmap` from `WindowIcon`'s RGBA.
5. Implement `com.canonical.dbusmenu` as a retained tree over `azul_core::menu::Menu`, with revision + `LayoutUpdated`.
6. Watcher availability probe + `NameOwnerChanged` re-registration.

**Phase 4 — adjacent (optional, ~1 week).** Desktop notifications (`org.freedesktop.Notifications` on Linux; `NIF_INFO` on Windows; macOS **blocked on bundling**) and autostart (XDG `.desktop` / portal `RequestBackground`; `SMAppService`; `Run` key — respecting `StartupApproved\Run` and **never** re-adding on every launch, which resurrects a user-disabled entry and reads as malware).

**Rough total: 3–4 weeks**, of which Linux is more than half.

### 2.5 The macOS bundling prerequisite

This is a real blocker that sits outside the tray work, and it should be decided before Phase 4.

**A non-bundled binary cannot post macOS notifications, at all.** `UNUserNotificationCenter.current()` raises `NSInternalInconsistencyException: "Invalid parameter not satisfying: bundleIdentifier != nil"` when the process has no bundle identifier, and the framework reads the *real, signed* identity — it cannot be swizzled. On top of that the app must be **code-signed** for the authorization prompt to appear at all, even in a debug build.

azul has **no macOS `.app` bundle tooling**: only iOS has an `Info.plist` (`scripts/ios/Info.plist`), and CI codesigns dylibs but never builds a bundle. This also blocks `LSUIElement` (tray-without-Dock-icon), which is a bundle key.

The recommendation is to **make "produce a `.app`" the default macOS output rather than an afterthought** — ad-hoc-signed (`codesign -s -`) for dev, Developer-ID-signed for distribution. Note it does *not* affect the tray icon itself: `NSStatusItem` works fine unbundled. Only notifications and `LSUIElement` need it.

---

## PART III — PRIORITIES

If the goal is "credible desktop toolkit", the ordering is not "monitors then tray" — it's **correctness first**, because the unit bugs are live today on hardware people actually own.

**Tier 1 — fix now, regardless of either feature.** The `WindowPosition` unit ambiguity (A1/A2/A15). Every HiDPI Windows or X11 user hits the popup-offset bug today, and both workstreams build on top of this. Roughly 3–4 days, and it makes the whole class impossible via the type system.

**Tier 2 — the cheap correctness wins.** A8 (two lines + a manifest), A7 (one missing flip), A6, A4/macOS, A20. Perhaps two days total for five real bugs.

**Tier 3 — pick one workstream.** Multi-monitor Phases 0–3 (~1.5 weeks) gets `get_current_monitor()` working everywhere, monitor hotplug events actually firing, and — critically — makes the whole area testable. Tray Phases 1–2 (~1 week) gets Windows + macOS tray, which is the majority of the user-visible "it's a real desktop app" impression, and defers the expensive Linux D-Bus work.

**Tier 4 — the long tails.** Linux monitor truth (Phase 4), the Wayland GPU scale bug, Linux SNI, and the macOS bundling story.

**A note on framing:** the two areas are not equally "missing". Multi-monitor is a *quality* problem — azul does multi-monitor today, just incorrectly in specific, enumerable ways. System tray is a *presence* problem. If the goal is the claim "desktop GUI toolkit", the tray is the more visible gap; if the goal is "doesn't embarrass itself on a developer's two-monitor mixed-DPI desk", the bug ledger is more urgent. The Tier 1/2 work serves both.

---

## Appendix — verification index

Every claim above with a `file:line` was checked against the tree at the time of writing. The ones most worth re-checking if this document ages:

| Claim | Location |
|---|---|
| Logical value stored in a `PhysicalPosition` field | `dll/src/desktop/shell2/common/transient.rs:365` |
| Physical parent origin + that offset | `dll/src/desktop/shell2/windows/mod.rs:6689` ← `:6623` |
| Physical + logical added in a public API | `layout/src/callbacks.rs:3229` |
| Two DPI symbols from the wrong DLL | `dll/src/desktop/shell2/windows/dpi.rs:101, 113` |
| Comment says "flipped", code doesn't flip | `dll/src/desktop/shell2/macos/mod.rs:8120` |
| `CGDirectDisplayID` used as an array index | `dll/src/desktop/shell2/macos/mod.rs:3863` |
| X11 monitor id hardcoded 0 | `dll/src/desktop/shell2/linux/x11/mod.rs:2101` |
| Wayland current-monitor is dead code | `dll/src/desktop/shell2/linux/wayland/mod.rs:7047` (0 callers) |
| Requested monitor discarded on Windows | `dll/src/desktop/shell2/windows/mod.rs:627` |
| Hardcoded Linux work area | `dll/src/desktop/display.rs:524`, used `:695 :804 :828 :1031 :1103 :1183 :1305` |
| One X11 scale for all CRTCs | `dll/src/desktop/display.rs:661` → `:728` |
| Wayland hot-unplug is an empty stub | `dll/src/desktop/shell2/linux/wayland/events.rs:756` |
| Wayland GPU path drops buffer scale | `dll/src/desktop/shell2/linux/wayland/mod.rs:6905` |
| `DISPLAY_CACHE` is Wayland-only | `dll/src/desktop/display.rs:862-870` |
| `get_displays()` panics off main thread | `dll/src/desktop/display.rs:443` |
| Hardcoded 60Hz animation dt | `dll/src/desktop/shell2/common/layout.rs:1717` |
| Monitor hotplug events never emitted | `core/src/events.rs:2517` declared; 0 hits in `dll/` |
| Per-window monitor cache | `layout/src/window.rs:1344`, created `:1719` |
| `gnome-menus` feature undeclared | `x11_properties.rs:32` vs every `Cargo.toml` |
| Wayland never dispatches D-Bus | `gnome_menu/manager.rs:317`, sole caller `x11/mod.rs:1589` |
| Empty monitor list in tests | `layout/src/e2e/runner.rs:238` |
| No monitor e2e coverage | 0 of 54 files in `e2e/` |

**Research method note.** Six agents ran in parallel: three read-only codebase surveys (core/API model; macOS+Windows shells; Linux shells), one architecture bug-sweep, and two web-research passes (tray APIs; multi-monitor APIs). The web agents flagged where search corrected a prior belief — the notable ones are folded into §1.5 and §2.2, especially `DwmGetCompositionTimingInfo` being unusable per-window, `xdg_output` being only *partially* deprecated, and Microsoft classifying `GetDpiForMonitor` as the non-DPI-aware call.
