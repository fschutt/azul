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
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Debugging

> **WIP.** The flag set, the HTTP debug server, and the in-browser debugger UI all work today. Names of endpoints and env vars may shift.

Azul ships an HTTP debug server that runs inside your application process. Set `AZ_DEBUG=<port>` and a thread binds `127.0.0.1:<port>`, serves an inspector UI at `/`, and accepts JSON commands at `POST /` that drive the application as if a user were clicking it. The same channel powers programmatic E2E tests (covered in [End-to-End Testing](e2e-testing.md)) and memory probes (covered in [Memory and Profiling](memory-profiling.md)).

```bash
AZ_DEBUG=8765 ./my_app &
curl -s http://localhost:8765/health
curl -s -X POST http://localhost:8765/ -d '{"op":"get_dom_tree"}'
```

## Environment flags

Every flag is read once at process start. Unset means off. All are independent and can be combined.

| Flag | Type | Effect |
|---|---|---|
| `AZ_DEBUG=<port>` | `u16` | Bind HTTP debug server on `127.0.0.1:<port>`. Failure to bind exits the process. |
| `AZ_BACKEND=<mode>` | `auto` \| `gpu` \| `cpu` \| `headless` | Resolve the rendering backend. `headless` skips the OS window and is required by the E2E runner. Default `auto`. |
| `AZUL_HEADLESS=1` | bool | Legacy alias for `AZ_BACKEND=headless`. |
| `AZ_RECORD=<path>` | filepath | Append every internal log message to `<path>` as plain text. Implies enabling internal logging. |
| `AZ_E2E=<path>` | filepath | Read JSON tests from `<path>`, run them through the debug timer, exit `0` (all pass) or `1` (any fail). See [End-to-End Testing](e2e-testing.md). |
| `AZ_E2E_TEST=<path>` | filepath | Deterministic resize/tick scenario runner for memory regressions. Bypasses the normal event loop and exits when the scenario completes. Requires the `e2e-test` cargo feature. |
| `AZ_PROFILE=<tokens>` | csv | Per-frame instrumentation. See [Memory and Profiling](memory-profiling.md). |
| `AZ_PROFILE_OUT=<path>` | filepath | JSONL output destination paired with `AZ_PROFILE=heap,jsonl`. |
| `RUST_LOG=<filter>` | env\_logger filter | Standard `log` crate filter applied by `set_up_logging` in `dll/src/desktop/logging.rs:25`. |

`AZ_DEBUG` and `AZUL_HEADLESS` compose: a CI run with `AZUL_HEADLESS=1 AZ_DEBUG=8765 ./my_app` boots a windowless process you can drive over HTTP. This is the supported configuration for screenshot diffing in CI.

## The HTTP debug server

The server lives in `dll/src/desktop/shell2/common/debug_server.rs`. When `AZ_DEBUG=<port>` is set, `start_debug_server` (`debug_server.rs:2474`) binds the port, spawns a thread named `azul-debug-server`, and registers a per-window timer that drains the request channel during the normal event loop. Commands therefore execute on the same thread that runs the layout, callback, and render passes — no shared-state races, no need to think about thread safety in your callback.

| Route | Method | Purpose |
|---|---|---|
| `/` | GET | Serves the inspector UI (HTML + CSS + JS, brotli-compressed). |
| `/health` | GET | Status JSON: port, pending log count, recent log lines. |
| `/material-icons.ttf` | GET | Embedded Material Icons font used by the inspector. |
| `/` | POST | One JSON command, blocks until the timer responds. |
| `/debug/compile?lang=<rust\|cpp\|python>` | POST | Compile a CSS source body to a standalone project ZIP. |

A request body is one `DebugEvent` plus optional `window_id`, `wait_for_render`, and `timeout_secs`:

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

Each command's `op` field selects a `DebugEvent` variant defined in `debug_server.rs:1526`. Categories overlap with the in-browser inspector's panels.

| Category | Representative ops |
|---|---|
| Mouse | `mouse_move`, `mouse_down`, `mouse_up`, `click`, `double_click`, `scroll` |
| Keyboard | `key_down`, `key_up`, `text_input` |
| Window | `resize`, `move`, `focus`, `blur`, `close`, `dpi_changed` |
| Queries | `get_state`, `get_dom_tree`, `get_node_hierarchy`, `get_layout_tree`, `get_display_list`, `get_html_string`, `hit_test`, `get_logs` |
| DOM mutation | `insert_node`, `delete_node`, `set_node_text`, `set_node_classes`, `set_node_css_override` |
| Scrolling | `get_scroll_states`, `get_scrollable_nodes`, `scroll_node_by`, `scroll_node_to`, `scroll_into_view` |
| Frame control | `wait_frame`, `wait`, `relayout`, `redraw` |
| Screenshots | `take_screenshot` (rasterised through the CPU compositor), `take_native_screenshot` (current GPU/CPU framebuffer of the running window) |
| Component / library introspection | `get_component_registry`, `get_libraries`, `get_library_components`, `get_function_pointers` |
| E2E | `run_e2e_tests` (also queued internally by `AZ_E2E` and the `/e2e/run` UI button) |

`click` accepts whichever of `selector`, `node_id`, `text`, or `(x, y)` you pass — internally it resolves to a node, mouse-downs and mouse-ups on its centre, fires `On::Click`, and triggers `RefreshDom` if your callback returns it. This is the building block every E2E `click` step uses.

`wait_frame` blocks until the next frame is rendered. After any command that mutates state (`click`, `resize`, `set_node_text`, …) call `wait_frame` before reading state back, otherwise queries can race the relayout pass.

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

This pattern — `AZ_DEBUG`, wait, drive, query — is the foundation for both ad-hoc debugging and the JSON-described E2E tests in [End-to-End Testing](e2e-testing.md).

## The in-browser inspector

Navigate to `http://localhost:<port>/` in any browser and the server returns the bundled inspector — DOM tree, layout box overlay, computed CSS, scroll-state monitor, log stream, and an E2E test designer. The same `POST /` endpoints power its panels, so anything you see in the browser can be reproduced from a script.

The inspector is a single HTML/JS bundle compiled into the binary via `build.rs` and served brotli-compressed (`Content-Encoding: br`). Disabling it means stripping the `AZ_DEBUG` codepath in the build — there is no runtime toggle.

## Internal log macros and categories

Inside the framework — and inside any callback that opts in — log lines are emitted through the macros defined in `core/src/debug.rs`:

```rust,ignore
use azul::{log_debug, log_info, log_warn};
use azul::core::debug::DebugCategory;

log_debug!(logger, Layout, "relayout took {} ms", elapsed_ms);
log_info!(logger, Window, "window {} resized to {}x{}", id, w, h);
log_warn!(logger, Resources, "font {} missing fallback", family);
```

Five severity levels (`Trace`, `Debug`, `Info`, `Warn`, `Error`) cross fifteen categories (`General`, `Window`, `EventLoop`, `Input`, `Layout`, `Text`, `DisplayList`, `SceneBuilding`, `Rendering`, `Resources`, `Callbacks`, `Timer`, `DebugServer`, `Platform`, `Icon`). The macros are no-ops when `logger` is `None`, so passing `&mut Option<Logger>` keeps the cost off the hot path.

The debug server keeps its own ring buffer of recent log entries; query it with `{"op":"get_logs"}` to see what fired during the last command. Pair with `AZ_RECORD=<file>` to mirror the same lines to disk.

## Crash handling

`set_up_panic_hooks` (`dll/src/desktop/logging.rs:62`) installs a panic handler that:

1. Captures and demangles the backtrace (frames pruned to file + line + symbol).
2. Logs the formatted panic at `error!` level — visible in stdout, in `RUST_LOG`, in `AZ_RECORD`, and in `{"op":"get_logs"}`.
3. If `SHOULD_ENABLE_PANIC_HOOK` is set, opens a native message box (`tinyfiledialogs`) summarising the failure for the end user. Linux escapes `<` and `>` via `escape_dialog_html` so Pango markup does not eat them.

Calling `set_up_panic_hooks()` early in `main` is part of `App::create` already; you only call it again if you build a custom embedding outside the standard entry point.

## When the timer is not running

`AZ_DEBUG` requires that the application reaches the event loop. If `App::run` is never called — for example, in a Rust unit test that builds a `Dom` and asserts its shape — the debug timer is never registered, and a `POST /` request hangs until `timeout_secs` elapses (default 30 s). For pure layout assertions, prefer the headless renderer covered in [End-to-End Testing](e2e-testing.md) or the reftest harness rather than `AZ_DEBUG`.
