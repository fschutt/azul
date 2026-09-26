---
slug: debugging
title: Debugging
language: en
canonical_slug: debugging
audience: external
maturity: wip
guide_order: 200
topic_only: false
short_desc: Debug overlays, the inspector, and structured logging
prerequisites: [hello-world]
tracked_files:
  - core/src/debug.rs
  - dll/src/desktop/logging.rs
  - dll/src/desktop/shell2/common/debug_server.rs
  - dll/src/desktop/shell2/common/x11_host.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
default-search-keys:
  - App
  - CallbackInfo
  - take_screenshot
  - take_native_screenshot
  - Dom
---

# Debugging

## Introduction

*WIP.* The flag set, the HTTP debug server, and the in-browser debugger all work today; names of endpoints and env vars may shift.

Azul ships an HTTP debug server that runs inside your application process. Set `AZ_DEBUG=<port>` and a thread binds `127.0.0.1:<port>`, serves an inspector UI at `/`, and accepts JSON commands at `POST /` that drive the application as if a user were clicking it. The same channel powers programmatic E2E tests (covered in [End-to-End Testing](debugging/e2e-testing.md)) and memory probes (covered in [Memory and Profiling](debugging/profiling.md)).

```bash
AZ_DEBUG=8765 ./my_app &
curl -s http://localhost:8765/health
curl -s -X POST http://localhost:8765/ -d '{"op":"get_dom_tree"}'
```

## Environment flags

Every flag is read once at process start. Unset means off — **except `AZ_LOG`, which is ON by default** (see below). All are independent and can be combined.

- `AZ_LOG=<level>`. Controls Azul's built-in stderr logger, **enabled by default**. Azul installs a logger automatically at `App::create` so the platform layer (windowing, event loop, layout, device backends) is never silent — if your app exits unexpectedly, the reason is on stderr. Levels: `off`/`0`/`false` silences it entirely; `error`, `warn`, `info`, `debug` (the default), `trace` (everything, including per-frame). It honors `NO_COLOR` and only colorizes a TTY. If your host already installs a logger (Python's `pyo3-log`, Android's `android_logger`, your own `env_logger`), Azul's logger steps aside and does not override it.
- `AZ_DEBUG=<port>`. Binds the HTTP debug server on `127.0.0.1:<port>`. A bind failure exits the process.
- `AZ_BACKEND=<mode>`. One of `auto`, `gpu`, `cpu`, or `headless`. Resolves the rendering backend. `headless` skips the OS window and is required by the E2E runner. Default `auto`.
- `AZUL_HEADLESS=1`. Legacy alias for `AZ_BACKEND=headless`.
- `AZ_WINDOW=<x11|wayland|auto>`. The windowing system, a separate axis from the renderer: on Linux it overrides the `WAYLAND_DISPLAY`/`DISPLAY` detection (`AZ_BACKEND=x11|wayland` is the older spelling). On macOS, `x11` opens X11 windows through XQuartz (the `x11-macos` feature, part of `build-dll`), see [Reproducing X11 bugs on macOS](#reproducing-x11-bugs-on-macos-xquartz).
- `AZ_RECORD=<path>`. Appends every internal log message to `<path>` as plain text.
- `AZ_E2E=<path>`. Reads JSON tests from `<path>`, runs them, exits `0` (all pass) or `1` (any fail). See [End-to-End Testing](debugging/e2e-testing.md).
- `AZ_PROFILE=<tokens>`. Comma-separated profiler tokens for per-frame instrumentation. See [Memory and Profiling](debugging/profiling.md).
- `AZ_PROFILE_OUT=<path>`. JSONL output destination paired with `AZ_PROFILE=heap,jsonl`.
- `RUST_LOG=<filter>`. Standard `log` crate filter (env_logger syntax).

`AZ_DEBUG` and `AZUL_HEADLESS` compose: a CI run with `AZUL_HEADLESS=1 AZ_DEBUG=8765 ./my_app` boots a windowless process you can drive over HTTP. This is the supported configuration for screenshot diffing in CI.

## The HTTP debug server

When `AZ_DEBUG=<port>` is set, the server binds the port and registers a per-window timer that drains the request channel during the normal event loop. Commands therefore execute on the same thread that runs the layout, callback, and render passes. No shared-state races, no need to think about thread safety in your callback.

- `GET /`. Serves the inspector UI.
- `GET /health`. Status JSON: port, pending log count, recent log lines.
- `GET /material-icons.ttf`. Embedded Material Icons font used by the inspector.
- `POST /`. One JSON command. Blocks until the timer responds.
- `POST /debug/compile?lang=<rust|cpp|python>`. Compiles a CSS source body to a standalone project ZIP.

A request body is one debug event plus optional `window_id`, `wait_for_render`, and `timeout_secs`:

```json
{
  "op": "click",
  "selector": ".increment-btn",
  "wait_for_render": true,
  "timeout_secs": 30
}
```

The response is wrapped in a `{ "status": "ok" | "error", "request_id": <u64>, "data": {...}, "window_state": {...} }` envelope. The server pretty-prints `application/json` with `Connection: close`, so `curl` and `jq` work without ceremony:

```bash
curl -s -X POST http://localhost:8765/ \
     -H 'Content-Type: application/json' \
     -d '{"op":"click","selector":"button"}' | jq
```

## The command vocabulary

Each command's `op` field selects one debug event variant. Categories overlap with the in-browser inspector's panels.

- **Mouse.** `mouse_move`, `mouse_down`, `mouse_up`, `click`, `double_click`, `scroll` (programmatic `scroll_to` on the node under the point), `wheel` (a hardware wheel notch through the shells' hit-test + scroll-physics path).
- **Keyboard.** `key_down`, `key_up`, `text_input`.
- **Window.** `resize`, `move`, `focus`, `blur`, `close`, `dpi_changed`.
- **Queries.** `get_state`, `get_dom_tree`, `get_node_hierarchy`, `get_layout_tree`, `get_display_list`, `get_html_string`, `hit_test`, `get_logs`.
- **DOM mutation.** `insert_node`, `delete_node`, `set_node_text`, `set_node_classes`, `set_node_css_override`.
- **Scrolling.** `get_scroll_states`, `get_scrollable_nodes`, `scroll_node_by`, `scroll_node_to`, `scroll_into_view`.
- **Frame control.** `wait_frame`, `wait`, `relayout`, `redraw`.
- **Screenshots.** `take_screenshot` (CPU compositor), `take_native_screenshot` (current framebuffer).
- **Component and library introspection.** `get_component_registry`, `get_libraries`, `get_library_components`, `get_function_pointers`.
- **E2E.** `run_e2e_tests`.

`click` accepts whichever of `selector`, `node_id`, `text`, or `(x, y)` you pass. It resolves to a node, fires the click, and triggers a refresh if your callback returns one. This is the building block every E2E `click` step uses.

`wait_frame` is a barrier: the next step runs only after the window has prepared a frame that follows the request (the frame clock is the per-window `ContentJournal::frame_seq`, bumped once per frame on every backend). After any command that mutates state (`click`, `resize`, `set_node_text`, `text_input`, …) call `wait_frame` before reading state back, otherwise queries can race the relayout pass or the virtual-view re-renders queued for the next paint. A scenario step cannot hang on it: after ~2 s without a frame the barrier logs a warning and opens.

## A simple driver script

Drive a running app from bash. The Hello World sample's [`tests/e2e/hello-world.sh`](https://github.com/fschutt/azul/blob/master/tests/e2e/hello-world.sh) is built on the same five primitives:

```bash
#!/usr/bin/env bash
set -e
PORT=8765
APP=./target/release/hello-world
AZ_DEBUG=$PORT "$APP" &
APP_PID=$!
trap 'kill $APP_PID 2>/dev/null || true' EXIT

post() { curl -s -X POST "http://127.0.0.1:$PORT/" -d "$1"; }

# 1. Wait for the server to come up
until post '{"op":"get_state"}' >/dev/null 2>&1; do sleep 0.1; done

# 2. Wait for the first frame
post '{"op":"wait_frame"}' >/dev/null

# 3. Click a button by CSS selector
post '{"op":"click","selector":"button"}' | jq -r '.status'

# 4. Read the rendered HTML back
post '{"op":"get_html_string"}' | jq -r '.data.value.html'

# 5. Capture a PNG (base64 data URI)
post '{"op":"take_native_screenshot"}' \
  | jq -r '.data.value' \
  | sed 's|^data:image/png;base64,||' \
  | base64 -d > out.png
```

This pattern — `AZ_DEBUG`, wait, drive, query — is the foundation for both ad-hoc debugging and the JSON-described E2E tests in [End-to-End Testing](debugging/e2e-testing.md).

## The in-browser inspector

Navigate to `http://localhost:<port>/` in any browser and the server returns the bundled inspector: DOM tree, layout box overlay, computed CSS, scroll-state monitor, log stream, and an E2E test designer. The same `POST /` endpoints power its panels, so anything you see in the browser can be reproduced from a script.

The inspector is a single HTML/JS bundle compiled into the binary and served brotli-compressed. Disabling it means stripping the `AZ_DEBUG` codepath in the build. There is no runtime toggle.

## Logging and crash handling

By default Azul installs a built-in stderr logger at `App::create` (the `AZ_LOG` flag above), so every `log`-crate message — including the platform-layer traces (`App::run` entry, the selected display backend and `WAYLAND_DISPLAY`/`DISPLAY` on Linux, window creation, each layout pass, font loading) — prints to stderr without any extra setup. This is why an app that previously "just exited with no error" now tells you exactly which step it reached. Set `AZ_LOG=trace` for the full firehose, `AZ_LOG=off` to silence it, or install your own logger (which takes precedence). `RUST_LOG` further filters by target/level once a logger is installed, and `AZ_RECORD=<path>` mirrors every internal message (including the debug-server categories) to disk. The debug server also keeps its own ring buffer of recent entries; query it with `{"op":"get_logs"}` to see what fired during the last command.

If `App::run` returns an error (e.g. no display server could be opened), it is always written to stderr on every platform — on Linux the GUI message box silently no-ops without `zenity`/`kdialog`, so stderr is the guaranteed channel.

`App::create` installs a panic handler that captures and demangles the backtrace, logs the formatted panic at error level (visible in stdout, in `RUST_LOG`, in `AZ_RECORD`, and in `{"op":"get_logs"}`), and optionally opens a native `MsgBox` summarising the failure for the end user.

## When the timer is not running

`AZ_DEBUG` requires that the application reaches the event loop. If `App::run` is never called — for example, in a Rust unit test that builds a `Dom` and asserts its shape — the debug timer is never registered, and a `POST /` request hangs until `timeout_secs` elapses (default 30 s). For pure layout assertions, prefer the headless renderer covered in [End-to-End Testing](debugging/e2e-testing.md) or the reftest harness rather than `AZ_DEBUG`.

## Reproducing X11 bugs on macOS (XQuartz)

The Linux X11 backend also runs on a Mac, against XQuartz. It is the same backend and the same window loop that ship on Linux, not an emulation of them, so X11 behaviour - event handling, override-redirect menus and popups, focus and pointer grabs, keyboard mapping, the frame rules behind `WindowDecorations` - reproduces without a Linux machine. It is opt-in twice: compiled in by a cargo feature, and selected per run by an environment variable. A build with the feature and without the variable behaves exactly like a build without either.

```bash
# 1. The X server. After installing, log out and back in once so launchd
#    exports DISPLAY to new processes.
brew install --cask xquartz
open -a XQuartz

# 2. libazul - every macOS build-dll has the X11 backend (`x11-macos` is part of it;
#    a link-static app adds `--features x11-macos`).
cargo build --release -p azul-dll --features build-dll

# 3. Any app on that libazul, with X11 windows instead of AppKit ones.
cargo build --release -p AzWidgets
AZ_BACKEND=x11 ./target/release/AzWidgets
```

- `AZ_BACKEND=x11` and `AZ_WINDOW=x11` are equivalent, with `AZ_WINDOW` winning when both are set, and the windowing choice combines with a render one: `AZ_WINDOW=x11 AZ_BACKEND=gpu`. There is no auto-detection - XQuartz exports `DISPLAY` to every process in the session - so only an explicit `x11` leaves AppKit. A build without the feature says so on stderr and opens an AppKit window.
- `DISPLAY` is XQuartz's launchd socket (`/private/tmp/com.apple.launchd.…/org.xquartz:0`); `DISPLAY=:0` works too while XQuartz runs. The X libraries are loaded from `/opt/X11/lib`. To use other builds of them, put their directory in `DYLD_LIBRARY_PATH`, which dyld consults first. The same variable finds libazul itself if it is not next to the binary: `DYLD_LIBRARY_PATH=target/release`.
- Rendering is the CPU path (`XPutImage`), the desktop default. XQuartz ships no libEGL, so `AZ_BACKEND=gpu` falls back to it.
- XQuartz draws one X pixel per point. The window scale is X11's own answer, as on Linux: `Xft.dpi` if set, else an estimate from the screen's size in millimetres, which can round to 1.25 on a laptop panel. Pin it with `echo "Xft.dpi: 96" | xrdb -merge` before starting the app, or set `192` to exercise the HiDPI paths.
- Shortcuts follow Linux while X11 draws the windows: Ctrl+C, Ctrl+V, Ctrl+A, Ctrl+Z and Ctrl+arrow word jumps. XQuartz delivers the Command key as `Meta`, which X11 reads as Alt.
- Text goes through libX11's own input method, which composes dead keys from the locale's Compose file. libxkbcommon is optional (XQuartz has none; `brew install libxkbcommon` adds the compose table the backend falls back to when no input method opens). Without an input method, a character the locale cannot spell is read from its keysym.
- An XQuartz window is a real macOS window, owned by the XQuartz process rather than by the app. `scripts/cgevent_input.py` drives it unchanged when given XQuartz's PID where it expects the app's - `./scripts/cgevent_input.py windows $(pgrep -x X11.bin)` lists the windows with their ids and origins, then `moveto`, `click`, `wheel` and `key` work as usual - and `screencapture -l <window id> shot.png` captures one. XQuartz turns key CODES into X keys and ignores the Unicode string an event carries, so `type` reaches it only for the characters the script maps to a physical key (letters, digits and the punctuation in its table); anything else goes through `key`. Turn on *Click-through Inactive Windows* in XQuartz's window settings, or the first click into an inactive window only activates it; *Option keys send Alt_L and Alt_R* in its input settings makes Option an Alt key.

What does not behave like Linux:

- The clipboard goes through `NSPasteboard`, which XQuartz mirrors to the X `CLIPBOARD` selection. `PRIMARY` (select, then middle-click) works inside the app and is not visible to other X clients.
- No Wayland, and none of the Linux desktop's D-Bus services (GNOME global menu, the portal's colour scheme, screensaver inhibition, the tray) or AT-SPI accessibility. `App::set_tray` and `App::set_app_icon` are skipped with a warning.
- The system style is the Mac's, detected at startup; a later light/dark switch is not followed.
- The window manager is quartz-wm, whose EWMH support is partial: interactive move/resize hand-off, `_NET_WM_STATE` changes and window-type hints may be ignored where a Linux window manager would honour them.
