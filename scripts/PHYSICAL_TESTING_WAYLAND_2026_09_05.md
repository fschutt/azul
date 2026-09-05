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
| system style (KDE) | the app reads the desktop's own settings | **VERIFIED** | `settings_source kdeglobals`, `theme Dark`, real Breeze values (`background #1b1e20`, `accent #3daee9`, `close-hover #da4453`) — not generic Adwaita. The #461 source-resolution change behaves on a second desktop |
| system icons | the desktop's icon theme loads | **VERIFIED** | `[system-icons] icon theme 'breeze-dark': registered 18/17` — the widened lookup from #461 did not regress KDE |
| window theme | `get_theme()` follows the desktop | **BROKEN → FIXED** | portal-less session left `WindowTheme::default()` = LightMode on a Breeze **Dark** desktop. Paper measured `#ffffff` (light palette) before, `#e2e2e2` (dark palette) after. Commit "a portal-less desktop never told the window it was dark" |
| KDE extras source | icons/cursor/buttons come from KDE | **BROKEN → FIXED** | they were read from `org.gnome.desktop.*`; the Cinnamon schemas in the same session hold `Mint-Y-Sand` / `Mint-Y-Aqua` / `Bibata-Modern-Classic` against KDE's `breeze-dark` / `Breeze` / `breeze_cursors`. Titlebar layout now reports KDE's own `MS|HIAX` instead of GNOME's `icon:minimize,maximize,close` |
| CSD resize band | the 8 px band resizes, and only that | **BROKEN → FIXED** | Wayland had the X11 defect verbatim — no frame check, and its `return` precedes `record_input_sample`. Rule moved to the shared `csd_resize_edge_for_press`; **not** device-verified here (no pointer), it rests on the X11 half being measured |
| 4c — compositor-driven resize | a configure relayouts | **VERIFIED** | KWin script set `frameGeometry` to 1000x700: ribbon reflowed (gallery dropped to 2 cells, Editing group moved), status bar spans the new width. Screenshot `wl1-crop.png` |
| externally-triggered maximize | the app observes WM-driven state | **VERIFIED (by code) / PARTIAL (device)** | unlike X11 — which never reads `_NET_WM_STATE` back — `xdg_toplevel_configure_handler` parses the states array (1=maximized, 2=fullscreen) and applies it with `WindowStateSource::Os`. On the device, `setMaximize(false,false)` from KWin was a no-op because the window was not maximized in the compositor's view to begin with |
| diagnostics on closed stderr | a warning never kills the app | **BROKEN → FIXED** | `AzWriter 2>&1 \| head -4` aborted with `failed printing to stderr: Broken pipe`. After the fix the same command exits **124** (killed by its own `timeout`, i.e. still alive) instead of **101** (panic) |
| titlebar drag | dragging the client titlebar moves the window | **UNTESTABLE** | no pointer injection (see above). The shared gesture-session fix from #461 applies to this path but cannot be exercised here |
| double-click maximize | double-click toggles maximize/restore | **UNTESTABLE** | same |
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
4. **`kdotool`/`xdotool` are useless here; KWin scripting is not.**
   `org.kde.KWin /Scripting loadScript` + `start`, with output read back from
   `journalctl --user -u plasma-kwin_wayland.service | grep '^js:'`, drives
   window geometry and state from the compositor side. The loaded script has no
   `/Scripting/Script<id>` object in this KWin version — call `start` on
   `/Scripting` itself.

## Still open

- Everything needing a pointer, until an injection path exists (`ydotool` +
  `input` group, or a second seat).
- The portal short-circuit path from #461, until a session with a live portal.
- A real resize-repaint measurement (instrument `render_and_present`).
- From the X11 audit: no `_NET_FRAME_EXTENTS` / `_GTK_FRAME_EXTENTS`;
  `XI_KeyPress` selected on `XIAllMasterDevices` while primary-seat keys are
  dropped; pointer lock uses `XGrabPointer` under XI2.
