# Physical testing — Linux / Wayland — 2026-09-05

**Device.** KDE Plasma **Wayland** (`XDG_SESSION_TYPE=wayland`,
`DESKTOP_SESSION=plasmawayland`, `WAYLAND_DISPLAY=wayland-0`), Linux Mint 22.2,
kwin_wayland, NVIDIA GTX 960 (`OpenGL ES 3.2 NVIDIA 580.159.03`), 1920x1080.
Colour scheme **BreezeDark**. The backend was forced with `AZ_BACKEND=wayland`
and `DISPLAY` unset; the app log confirms it each run:

    [Linux] run() entry — AZ_BACKEND=Some("wayland") WAYLAND_DISPLAY=Some("wayland-0") DISPLAY=None

Branch `fix/wayland-desktop-integration`, stacked on `fix/x11-desktop-integration`
(PR #461) so the X11 round's shared fixes are present. Binary: `AzWriter`
(`link-dynamic`) against a `build-dll` libazul.

## THE CONSTRAINT THAT SHAPES THIS REPORT

**No pointer input can be synthesised on this machine under Wayland.** There is
no XTEST; `/dev/uinput` is `crw------- root root` and the user is not in
`input`; `ydotool`, `wtype`, `dotool`, `evemu-play` are all absent. Installing
one, or adding the user to a group, is a system change made while the machine's
owner was asleep, so it was not done.

Everything below that needs a real pointer is therefore **UNTESTABLE**, and is
recorded as such rather than assumed. What replaced it where possible:
**KWin scripting over D-Bus** (`org.kde.KWin /Scripting loadScript`), which
drives *window state* from the compositor without touching the pointer — enough
to exercise the configure path, but not the click path.

## Results

| id / item | promise (one line) | verdict | evidence |
|---|---|---|---|
| 7d | seat bound past v7 so `axis_value120` is reachable | **VERIFIED** | `Bound wl_seat v8 (offered v8) - high-res wheel (axis_value120) available`. The cap is `version.min(9)` and both v8/v9 listener slots are wired; KWin offers v8, so high-res wheel works and v9's `axis_relative_direction` is simply not offered by this compositor. The old cap of 7 would have lost both |
| 7a | `zwp_pointer_gestures_v1` bound | **VERIFIED (bind)** | `Bound zwp_pointer_gestures_v1 v3 - touchpad pinch/swipe/hold` — v3 is the version that has hold |
| 9d-i-a / 9d-ii-a | relative pointer + pointer constraints bound | **VERIFIED (bind)** | `Bound zwp_relative_pointer_manager_v1`, `Bound zwp_pointer_constraints_v1` |
| 3d | Wayland IME producer live | **VERIFIED (activation)** | `Bound zwp_text_input_v3`, then on focus `text_input_v3: enabled for contenteditable focus` / `enter - IME activated for surface` / `done serial=1`. Composition delivery itself needs an IME actually composing |
| monitor enumeration | the app knows its real outputs | **BROKEN → FIXED** | four CLI spawns that all fail on KWin; now read from `wl_output` (see below) |
| system style (KDE) | the app reads the desktop's own settings | **VERIFIED** | `settings_source kdeglobals`, `theme Dark`, real Breeze values (`background #1b1e20`, `accent #3daee9`, `close-hover #da4453`) — not generic Adwaita. The #461 source-resolution change behaves on a second desktop |
| system icons | the desktop's icon theme loads | **VERIFIED** | `[system-icons] icon theme 'breeze-dark': registered 18/17` — the widened lookup from #461 did not regress KDE |
| window theme | `get_theme()` follows the desktop | **BROKEN → FIXED** | portal-less session left `WindowTheme::default()` = LightMode on a Breeze **Dark** desktop. Paper measured `#ffffff` (light palette) before, `#e2e2e2` (dark palette) after. Commit "a portal-less desktop never told the window it was dark" |
| KDE extras source | icons/cursor/buttons come from KDE | **BROKEN → FIXED** | they were read from `org.gnome.desktop.*`; the Cinnamon schemas in the same session hold `Mint-Y-Sand` / `Mint-Y-Aqua` / `Bibata-Modern-Classic` against KDE's `breeze-dark` / `Breeze` / `breeze_cursors`. Titlebar layout now reports KDE's own `MS|HIAX` instead of GNOME's `icon:minimize,maximize,close` |
| CSD resize band | the 8 px band resizes, and only that | **BROKEN → FIXED** | Wayland had the X11 defect verbatim — no frame check, and its `return` precedes `record_input_sample`. Rule moved to the shared `csd_resize_edge_for_press`; **not** device-verified here (no pointer), it rests on the X11 half being measured |
| 4c — compositor-driven resize | a configure relayouts | **VERIFIED** | KWin script set `frameGeometry` to 1000x700: ribbon reflowed (gallery dropped to 2 cells, Editing group moved), status bar spans the new width. Screenshot `wl1-crop.png` |
| externally-triggered maximize | the app observes WM-driven state | **VERIFIED (by code) / PARTIAL (device)** | unlike X11 — which never reads `_NET_WM_STATE` back — `xdg_toplevel_configure_handler` parses the states array (1=maximized, 2=fullscreen) and applies it with `WindowStateSource::Os`. On the device, `setMaximize(false,false)` from KWin was a no-op because the window was not maximized in the compositor's view to begin with |
| diagnostics on closed stderr | a warning never kills the app | **BROKEN → FIXED** | `AzWriter 2>&1 \| head -4` aborted with `failed printing to stderr: Broken pipe`. After the fix the same command exits **124** (killed by its own `timeout`, i.e. still alive) instead of **101** (panic) |
| titlebar drag | dragging the client titlebar moves the window | **VERIFIED (X11 via XWayland)** | every press from y=10 to y=27 enters the interactive-move path — no alternation, so #461's gesture-session fix holds. Not testable on the Wayland backend |
| double-click maximize | double-click toggles maximize/restore | **VERIFIED (X11 via XWayland)** | `0,0 1920x1036` -> `200,218 1200x800` on a double-click at the title bar |
| CSD band gate | the band is off while maximized | **VERIFIED (X11 via XWayland)** | band ON (move-path=0) when the app knows it is Normal, OFF (move-path=1) when its `flags.frame` is stale — see below |
| WM-initiated un-maximize | the client notices | **BROKEN -> FIXED** | X11 never read `_NET_WM_STATE` back. Same probe before/after: `move-path=1` (band dead) -> `move-path=0` (band alive), with `_NET_WM_STATE -> Normal (was Maximized) — WM-driven, adopting` in the log |
| portal short-circuit (#461) | the portal must not replace desktop discovery | **UNEXERCISED** | this session logs `xdg-desktop-portal unavailable`, so the portal path never runs. Needs a session where the portal answers |
| resize repaint rate | resize repaints at refresh rate | **NOT MEASURED** | see below |

## On the X11 "~23 fps" figure — a correction

The X11 report states 44.2 ms median between frames during an interactive
resize. That number came from the *gaps between `ConfigureNotify` log lines*
while the resize was driven by synthetic input, so it measures the arrival rate
of resize events, **not** how long a repaint takes. It should not be quoted as
a frame rate.

The equivalent measurement was attempted here and abandoned honestly: KWin
coalesces rapid `frameGeometry` writes into very few configures, so a
compositor-driven resize produces too few samples to time, and the app's own
`xdg_toplevel.configure` trace sits behind `AZ_WL_TRACE` at Debug level.

The right instrument for both platforms is a span around `render_and_present`,
reporting the paint itself. Until that exists, neither platform has a
trustworthy resize-repaint number.

## Idle repaint is the caret, at the configured rate — and a regression I caused and caught

Idle, with a caret in the document, the app generates a WebRender frame per
caret blink and nothing else. Measured:

| | frames | over | rate | gap (median) |
|---|---|---|---|---|
| with the bug | 67 | 33.9 s | 1.97 /s | **530 ms** |
| fixed | 34 | 38.4 s | 0.88 /s | **1200 ms** |

530 ms is `text_edit::CURSOR_BLINK_INTERVAL_MS`, azul's built-in default;
1200 ms is what this session actually configures and what
`AZ_DUMP_SYSTEM_STYLE=1` reports. So the caret plumbing works end to end —
`caret_blink_interval_for` reads `SystemStyle::input::caret_blink_rate_ms`
correctly, and the earlier suspicion that it was an ordering problem around
`set_system_style` was wrong.

The 530 ms was **caused by the `kde_extras` commit in this PR**. That commit
returned early from `discover_linux_extras` once KDE's own reader had
answered — and the tail of that function is not desktop-specific at all: the
animation, sound, menu/toolbar and caret-blink blocks are asked of gsettings on
every desktop. Returning early took `caret_blink_ms` from 1200 to the built-in
530, and the app quietly repainted twice a second at a rate nobody configured.

Fixed by splitting the function honestly: `discover_gsettings_appearance` is
the icon/cursor/theme/button block that a desktop-specific reader REPLACES, and
`discover_shared_behaviour` is the tail that always runs. The idle cadence
returning to exactly 1200 ms is the proof.

The lesson, which is the general one from this whole arc: an early `return`
added to serve one desktop silently deleted work that belonged to all of them,
and no test covered the tail. The device measurement caught it, which is the
argument for running the thing rather than only compiling it.

## The resize repaint, finally measured — the fast path IS hit

`AZ_LOG="debug,+platform"` plus `AZ_LOG_FILE` turns on the Wayland platform
trace, which times the repaint itself rather than the gaps between events:

    [WL] xdg_toplevel.configure resize 1280x800 -> 1920x1036: fast relayout
    [WL] resize_surface logical=1920x1036 buffer=1920x1036 scale=1 took=1.78ms

Driving 12 spaced resizes from a KWin script (so each produces its own
configure) gives **13 samples: min 1.75 ms, median 3.20 ms, max 17.45 ms** —
and the 17.45 ms is the FIRST resize after startup, cold. Every warm sample is
between 1.75 and 3.56 ms:

    17.45  2.09  2.63  1.75  3.43  2.63  2.63  3.42  3.56  3.39  3.20  3.24  3.17

So on Wayland the fast resize path is confirmed taken (the trace says
`fast relayout`, and no full DOM regeneration appears), and the repaint costs
~2-3.5 ms warm, comfortably inside a 16.7 ms frame. **Resize paint cost is not
a source of jitter here.**

This also settles the X11 "~23 fps" figure from #461: if the paint is ~3 ms,
the 44 ms gaps measured there were the arrival rate of resize EVENTS under
synthetic pacing, never the cost of painting. The corrected claim is in that
report too.

## Wayland protocols this compositor actually gives us

Every optional protocol the ledger items depend on binds on KWin (Plasma 6).
The bind sites were silent, so this needed a log line each - added in this PR,
in the same `[Wayland] Bound ...` style the other arms already used:

| protocol | bound | serves |
|---|---|---|
| `zwp_text_input_v3` | yes | 3d (IME), 9b-ii-a-i-d-ii-c |
| `zwp_pointer_gestures_v1` | **v3** | 7a — v3 means pinch, swipe AND hold |
| `zwp_relative_pointer_manager_v1` | yes | 9d-i-a (raw motion) |
| `zwp_pointer_constraints_v1` | yes | 9d-ii-a (pointer lock) |
| `zwp_tablet_manager_v2` | yes | 8c, 5b, 9b-ii-b-i-b-i (pen) |
| `zwp_primary_selection_device_manager_v1` | yes | 9b-ii-b-i-b-i-a-i (middle-click paste) |
| `zxdg_decoration_manager_v1` | yes | CSD/SSD negotiation |
| `wp_viewporter`, `wp_fractional_scale_manager_v1` | yes | scaling |

That verifies the BINDING half of those items on real hardware. The
event-delivery half still needs a pointer, a pen or a second seat, and remains
untested for the reasons above.

The IME path goes further than binding - it activates on focus with no input
needed:

    [Wayland] text_input_v3: enabled for contenteditable focus
    [Wayland] text_input_v3: enter - IME activated for surface
    [Wayland] text_input_v3: done serial=1

so 3d's Wayland producer is live; what is untested is `preedit_string` /
`commit_string` delivery, which needs an IME actually composing.

## Monitor enumeration never asked the compositor

Buried in the platform trace:

    [display] All Wayland detection methods failed. Falling back to default display.

`DETECTION_CHAIN` on Wayland is `swaymsg`, `hyprctl`, `kscreen-doctor`,
`wlr-randr` — four EXTERNAL PROCESS SPAWNS, tried in order, **on the UI
thread**. On KDE Plasma Wayland not one of them answers (this box also has
`kscreen-doctor` SEGFAULTING twice in `dmesg`), so every monitor query returned
a hardcoded 1920x1080 guess with a guessed panel height and a default refresh.
The chain's own comment records the hazard: a 2-second per-tool deadline added
after "the fourth per-tick detection call on KDE Wayland never returned" froze
the app so the window could not even close.

Meanwhile the compositor had already told us everything. The backend binds
`wl_output` and its handlers fill a `MonitorState` with position, mode, scale,
make and model — and `wl_output_mode_handler` then **discarded the refresh
rate**, which is the one number the frame pacer divides by.

Fixed: `MonitorState` keeps `refresh_mhz`, `wl_output.done` (the protocol's
atomic commit for an output) publishes the set to the display layer, and
`get_displays()` consults that before anything else. Measured on the device:

    [display] compositor described 1 output(s): output-48 1920x1080@60000mHz scale=1

Correct mode, correct refresh, correct scale — from the protocol, with no
process spawned.

The startup ordering is inherent — window creation seeds the monitor list ~1 s
BEFORE the first `wl_output.done` arrives — so `wl_output.done` now re-seeds it
the first time the description actually changes, exactly as the monitor-REMOVAL
path already did. Measured end to end:

    2032.2ms  All Wayland detection methods failed. Falling back to default display.
    3028.4ms  compositor described 1 output(s): output-48 1920x1080@60000mHz scale=1
    3028.4ms  1 output(s) from the compositor (wl_output)

The one startup fallback is unavoidable (nothing has described an output yet);
a second later the app's memoised list is the compositor's own, instead of
keeping the 1920x1080 guess until someone plugs a monitor in.

## Open, measured, NOT fixed: 20 installed font families report as unresolved

Running AzWriter on this session prints, once per family:

    [azul][font] UNRESOLVED font-family "DejaVu Sans": no font file and no
    registered in-memory font matches this family.

for **20 families**: C059, Cantarell, DejaVu Sans, DejaVu Sans Mono, DejaVu
Serif, Droid Sans Fallback, Liberation Sans, Nimbus Mono PS, Nimbus Roman,
Nimbus Sans, Nimbus Sans Narrow, Noto Mono, Noto Sans Mono, Noto Serif, P052,
Sans, Standard Symbols PS, URW Bookman, URW Gothic, Ubuntu.

Cross-checked against the system, and most of them ARE installed:

    DejaVu Sans        1 match   (fc-list : family)
    Noto Serif         1 match
    Liberation Sans    1 match
    Nimbus Roman       1 match
    Cantarell          0 matches  <- the only genuinely absent one

The mechanism is documented in `prune_absent_alias_candidates`: every CSS
generic is expanded to the system's fontconfig `<alias><prefer>` families
ahead of the generic itself, so a stack of three generics reaches the resolver
as ~150 concrete names, and the ones the cache cannot serve are pruned. These
20 survived pruning and then failed to resolve anyway.

This is NOT diagnosed here, deliberately: it sits squarely in the font-fallback
area that the rust-fontconfig 5 migration (PR #457) exists to rework, this
branch is master-based and does not contain that migration, and evaluating one
against the other needs both trees built and compared rather than a guess at
07:00. Recorded with the cross-check so the next session starts from evidence.

What WAS fixed is how the warning is delivered: it used a bare `eprintln!`,
which panics when stderr has gone away (the crash class fixed earlier in this
PR) and is invisible to the diagnostics ring, so no test could assert that a
missing family was reported at all. It now goes through
`azul_core::diagnostics::emit` like every other azul warning.

## The repo's own tripwire was red before any of this started

`scripts/check_feature_matrix.sh` exists to be run before a push. On this
branch's base it reported **4 of 8 combinations FAILING TO COMPILE**, and CI's
Feature Matrix job runs the same `azul-layout --no-default-features --features
"text_layout"` check, so it was red there too.

None of it came from this work — every failing file (`display_list.rs`,
`tray_icon.rs`, `changeset.rs`, `dialogs/`) is one nothing here had touched,
which `git diff --name-only` against the base confirms.

Four gates, all the same shape — an item that needs a feature, reached from a
place that does not require it:

| what | why it broke |
|---|---|
| `rasterize_svg_clip_to_r8` | reaches `agg_rust` ungated, while its sibling `rasterize_svg_stroke_to_r8` AND its only caller both already carry `#[cfg(feature = "cpurender")]` |
| `pub mod tray_icon` | imports `crate::cpurender` unconditionally; driving the CPU renderer is the module's whole job |
| `DocumentChangeset::apply_to_dom` | returns a `crate::document_edit` type, but that module is gated on `text_layout + xml` — a `text_layout`-only build failed on the return type alone |
| `dialogs::report_problem` | built on the cpurender-gated `dialogs::report` and decodes via `cpurender::AzulPixmap`; gating it then required gating `invoke_system_dialog`'s `ReportProblem` arm, which now mirrors the `updater` arm beside it |

Now **8/8, "Feature matrix clean"**, with the default-feature tests unaffected.

Worth knowing: the first two are the same gates already fixed on **#457**, which
is blocked indefinitely behind the crates.io publish chain — so those fixes
were stranded on an unmergeable branch while master stayed broken. They belong
somewhere that can merge. If #457 is rebased later, git drops the duplicates.

## The X11 backend under XWayland — a partial way past "no pointer input"

This session has XWayland (`DISPLAY=:0`) and XTEST works there, so the X11
backend CAN be driven with `xdotool` even though the Wayland one cannot. Run
with `AZ_BACKEND=x11`, the window maps, `wmctrl` sees it, and
`xdotool getmouselocation` reports the pointer inside it.

**Regression check — PASS.** The shared `csd_resize_edge_for_press` refactor in
this PR rewrote the X11 press path, and the X11 backend had not been run since.
It starts, maps, lays out and responds.

**The gesture-session fix holds — no alternation.** Sweeping a press-and-drag
down the title bar and counting how often the interactive-move path is entered
(`AZ_HIT_DEBUG=1` prints `begin_interactive_move clears buttons`):

    y=10 DRAG   y=14 DRAG   y=18 DRAG   y=21 DRAG   y=24 DRAG   y=27 DRAG

Every attempt, where the pre-fix behaviour on the real X11 session was every
OTHER attempt (`y=4 drag, y=10 dead, y=14 drag, y=18 dead ...`). Alternation is
a property of stale SESSION state, not of position, so this result survives the
coordinate caveat below.

**The band gate IS verifiable here — and it exposed a second defect.**

The first attempt looked like a 16 px addressing error (`windowmove` to
(200,200) read back as (200,216)). It was not: repeating the move on a settled
window lands at exactly (200,200) every time, so the 216 was the window still
settling out of a maximize. With that ruled out, the real result appears.

Undecorated (`_MOTIF_WM_HINTS = 0x2,0x0,...`, KWin `noBorder=true`), window at
a verified (200,200), pressing at window-relative y and counting entries into
the interactive-move path:

| how the window was un-maximized | app's `flags.frame` | press at rel_y=2 |
|---|---|---|
| `wmctrl` (the WM did it) | still **Maximized** — stale | move-path=**1** (band OFF) |
| double-click on the title bar (the APP did it) | **Normal** — known | move-path=**0** (band ON) |

Both rows are `csd_resize_edge_for_press` behaving exactly as written: the band
is live on a Normal window and disabled on a Maximized one. So the band gate is
**VERIFIED on the X11 backend**, and so is **double-click maximize/restore** —
the double-click took the window from `0,0 1920x1036` to `200,218 1200x800`.

The first row is the interesting one. X11 never reads `_NET_WM_STATE` back
(there is no `PropertyNotify` handler at all — the X11 audit's item b2), so
after the WINDOW MANAGER un-maximizes a window the client keeps believing it is
maximized. That abstract gap now has a concrete, user-visible symptom:

> Un-maximize an azul window from outside the app — Alt+F10, the window menu,
> dragging it off the top edge — and its resize edges stop working, because the
> client still thinks it is maximized and a maximized window has no edges.

That is a real bug, and it IS fixed now - the reproducibility is what made it
worth doing. X11 selects `PropertyChangeMask` and handles `PropertyNotify` for
`_NET_WM_STATE`, reading the property back and applying the frame with
`WindowStateSource::Os` so the baseline advances and `sync_window_state` does
not echo it to the WM.

Same probe, same three commands, before and after:

    un-maximize with wmctrl, press rel_y=2:
      before   move-path=1   band OFF - client still believed Maximized
      after    move-path=0   band ON  - resize edges work again

and the handler says so:

    [X11] _NET_WM_STATE -> Normal (was Maximized) — WM-driven, adopting

`window_frame_from_net_wm_state` is the pure half and carries the ranking the
atoms need: HIDDEN outranks FULLSCREEN outranks maximize, and maximize means
BOTH axes - a vertically-maximized window still has left and right edges, so it
is Normal.

## X11 repaint, now instrumented — and the "23 fps" claim finally settled

X11 had no equivalent of Wayland's `resize_surface ... took=`, which is exactly
why #461's resize figure was inferred from the gaps between ConfigureNotify
events and had to be retracted. `render_and_present` now times itself
(`AZ_LOG="debug,+window"`).

Driving 12 window resizes on the X11 backend, **1058 samples**:

    present cost              min 0.94   median 16.51   p90 17.19   max 317.41 (cold first)
    present-to-present gap    median 16.65 ms  ->  60.0 fps

Two things follow, and the second one killed a hypothesis of mine.

**The present loop runs at exactly refresh rate.** 16.65 ms between presents on
a 60 Hz panel is 60.0 fps. Whatever "resize lags" is, it is not the present
loop failing to keep up.

**The 16.51 ms is the vsync block, not paint work.** The MINIMUM present is
0.94 ms, and the median sits a hair under one refresh interval — the shape of a
swap that waits for vblank. Wayland's number is the same story from the other
side: `resize_surface` there measures the repaint WITHOUT the wait and reports
3.20 ms median.

I expected to find double-pacing here — the frame pacer stamps
`last_present_at` AFTER the present returns, and its own comment says it was
written when presents were "sub-ms frames", so a present that now blocks a full
frame looked like it would make the pacer add a second one. The measurement
says otherwise: 60.0 fps, no doubled interval. The pacer change I had drafted
would have "fixed" something that is not broken, and measuring before changing
is the only reason it was not made.

So #461's "~23 fps" is fully retired: the paint costs ~1 ms of work, the swap
waits for vblank, and the loop delivers 60 fps. Those 44 ms gaps were the rate
synthetic resize EVENTS were arriving at.

## Keyboard input on X11 — the audit's XI2 worry is latent, not live

The X11 audit flagged that `XI_KeyPress`/`XI_KeyRelease` are selected on
`XIAllMasterDevices` while the handler DROPS every primary-seat key, and that
per XI2proto "if the event has been delivered, event processing stops" this
should suppress core key delivery for the client. It also said, correctly, to
**verify on the box before changing anything**.

Verified: **keys work**. Clicking into the document and pressing keys through
XWayland puts text in the document and moves the word count off zero. So
practical delivery differs from the spec text here, the concern is latent
rather than live, and the false comment at `mod.rs:1483-1486` is a
documentation bug rather than a broken path. Nothing changed.

**A correction to my own first reading.** `xdotool type "Hello azul"` rendered
`Hello ayul` — the `z` came out as `y`. On a `de-DE` session that looks exactly
like a QWERTZ/QWERTY layout bug, and I went looking for azul interpreting
keycodes with a German keymap. It is not that. The X layout here is `us`
(keycode 52 = `z`, 29 = `y`), and pressing the keys explicitly with
`xdotool key` produced `y z q w` — every one correct, `z` included.

The difference is the tool: `xdotool type` synthesises arbitrary characters by
temporarily REMAPPING a spare keycode, which races the client's view of the
keymap. `xdotool key <keysym>` presses a key that already carries the keysym
and has no such race. The bug was in my harness, and one more screenshot would
have been reported as an azul defect.

## Traps found

1. **`build-dll` and `link-dynamic` fight over `target/release/libazul.so`, in
   both directions.** Building libazul after the app deletes
   `target/release/AzWriter` outright ("Datei oder Verzeichnis nicht gefunden");
   building the app after libazul replaces the 402 MB library with a 5.6 MB stub
   and the app dies with "file too short". The only stable arrangement is: build
   the app, then libazul, then copy the library somewhere `cargo` does not own
   (`target/azul-lib/`) and run against that.
2. **A pipeline hides how a process died.** `app | head -4; sleep 15; pgrep` was
   read twice as "the app died", when the pipeline had simply blocked until the
   app's own `timeout` killed it. `${PIPESTATUS[0]}` settles it in one line:
   124 = timeout, 101 = panic, 141 = SIGPIPE.
3. **A Wayland resize logs nothing.** X11 prints `ConfigureNotify resize -> WxH`
   at INFO for every step; the Wayland handler's equivalent is `wl_trace!`,
   gated behind `AZ_WL_TRACE` *and* Platform/Debug. Half an hour went into
   "did the app even see that configure?" before a screenshot answered it.
4. **The Wayland platform trace needs THREE things, not one.** `AZ_WL_TRACE=1`
   alone prints nothing: `wl_trace!` emits at Debug into the `Platform`
   category, which `AZ_LOG=debug` does not enable on its own. The combination
   that works is `AZ_LOG="debug,+platform"` — and to be sure of capturing it,
   `AZ_LOG_FILE=<path>`, which `log_gate::emit_at` writes to unconditionally,
   bypassing every stderr gate. Hours of "did the app even see that configure?"
   collapse to one grep once this is on.
5. **The repo has a pre-push tripwire and it was already red.**
   `scripts/check_feature_matrix.sh` catches exactly the "a gated subsystem
   does not compile" class, takes minutes, and reported 4/8 failing before any
   change here. Run it before pushing; a green `cargo test` says nothing about
   the seven other feature combinations CI builds.
6. **`xdotool type` lies; `xdotool key` does not.** `type` remaps a spare
   keycode to synthesise characters, which races the client's keymap: typing
   "azul" rendered "ayul", which on a de-DE session reads exactly like a
   QWERTZ layout bug in the app. Pressing the same keys with
   `xdotool key <keysym>` produced every character correctly. Use `key` for
   anything you intend to draw a conclusion from.
7. **`kdotool`/`xdotool` are useless here; KWin scripting is not.**
   `org.kde.KWin /Scripting loadScript` + `start`, with output read back from
   `journalctl --user -u plasma-kwin_wayland.service | grep '^js:'`, drives
   window geometry and state from the compositor side. The loaded script has no
   `/Scripting/Script<id>` object in this KWin version — call `start` on
   `/Scripting` itself.

## Still open

Ordered by what the next session can actually act on.

**Needs hardware or a second seat**
- Anything driven by the WAYLAND pointer: this machine has no injection path
  (no XTEST under Wayland, `/dev/uinput` is root-only, no `ydotool`/`wtype`).
  The X11 backend IS drivable through XWayland, which is how titlebar drag,
  double-click maximize and the CSD band gate got verified — but that exercises
  the X11 code, not the Wayland pointer path.
- Multi-seat (9b-ii and everything under it) needs a second `wl_seat`; pen
  items need a tablet; pad items need a pad. None are present.
- Event DELIVERY for the protocols that are confirmed BOUND — pointer
  gestures, relative pointer, pointer constraints, tablet, primary selection.
  Binding is verified; nothing has sent an event through them.

**Needs a different session**
- The portal short-circuit fix from #461: this session logs
  `xdg-desktop-portal unavailable`, so that path never runs here.

**Code, no hardware needed**
- 20 font families report unresolved while `fc-list` finds most of them
  installed. Belongs with the font-fallback rework in #457; the cross-check is
  recorded above so the next session starts from evidence.
- ~~`render_and_present` is not instrumented~~ DONE - it times itself now, and
  the X11 loop measures 60.0 fps with a 0.94 ms floor. Both platforms have a
  repaint number.
- From the X11 audit, still unfixed: no `_NET_FRAME_EXTENTS` /
  `_GTK_FRAME_EXTENTS` (and xfwm4#603 says the extents must be cleared BEFORE
  a maximize transition, not after); `XI_KeyPress` is selected on
  `XIAllMasterDevices` while primary-seat keys are dropped - VERIFIED LATENT
  on the device (keys work), so what is left there is the false comment at
  `mod.rs:1483-1486`, not a broken path; pointer lock uses `XGrabPointer` while XI2 is
  selected, so during a lock the pointer path silently switches to the core
  fallback while `XI_RawMotion` keeps arriving.
- `_NET_WM_MOVERESIZE` hardening the audit lists: no `_NET_SUPPORTED` probe, no
  `_NET_WM_MOVERESIZE_CANCEL` on a post-handoff `ButtonRelease`, and
  `XUngrabPointer` releases only the core pointer where XI2 wants
  `XIUngrabDevice`.

**Closed during this round** (kept so the next session does not re-open them):
the window theme on a portal-less desktop, KDE reading GNOME's schemas, the
shared-tail regression that followed it, the CSD band on Wayland, the
diagnostics/`log_gate` stderr panic, monitor enumeration via `wl_output`, the
unresolved-font warning bypassing the ring, the 4/8 feature matrix, and the
`_NET_WM_STATE` readback (X11 audit item b2).
