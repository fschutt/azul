---
slug: headless-rendering
title: Headless Rendering
language: en
canonical_slug: headless-rendering
audience: external
maturity: wip
guide_order: 290
topic_only: false
short_desc: Running the pipeline without a window
prerequisites: [dom]
tracked_files:
  - layout/src/cpurender.rs
  - dll/src/desktop/native_screenshot.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Headless Rendering

> **WIP.** Wayland and XCB native screenshot support is incomplete.

The same binary that opens a desktop window can run without one. There is no display server, no OpenGL context, no visible window. Set an environment variable, the framework swaps in a CPU-only backend, and the rest of the pipeline (layout, callbacks, timers, re-renders) runs unchanged. Output is captured through the HTTP debug API.

```bash
AZ_BACKEND=headless ./my_app
```

This is the standard configuration for screenshot diffing in CI, smoke tests, and pre-rendering content for caching.

```azul-render screenshot=headless-hello width=400 height=200 subtitle="A free-standing render — no window, no display server"
<body style="background: white; font-family: sans-serif;">
  <p style="font-size: 28px; padding: 24px; color: #1d4f8b;">Hello from headless</p>
</body>
```

## Selecting the backend

Two environment variables, set before the process starts:

| Variable | Effect |
|---|---|
| `AZ_BACKEND=headless` | Force the headless backend even when a display is available. |
| `AZUL_HEADLESS=1` | Legacy alias for the same. |
| `AZ_DEBUG=<port>` | Start the HTTP debug server (event injection plus screenshot capture). |

```bash
AZ_BACKEND=headless ./my_azul_app
AZ_BACKEND=headless AZ_DEBUG=8765 ./my_azul_app
```

A binary built for desktop runs unchanged in a server-side container with these flags set.

## Capturing screenshots over HTTP

When the process boots with `AZ_DEBUG=<port>`, the debug server (covered in [Debugging](debugging.md)) exposes two screenshot ops:

| Op | Returns |
|---|---|
| `take_screenshot` | CPU-rasterised PNG of the current DOM, no window decorations. |
| `take_native_screenshot` | Current framebuffer with whatever the OS is drawing. |

In headless mode there is no OS framebuffer, so prefer `take_screenshot`. Both return a base64 data URI in the `data.value` field of the response envelope:

```bash
AZ_BACKEND=headless AZ_DEBUG=8765 ./my_app &
sleep 0.2
curl -s -X POST http://127.0.0.1:8765/ \
  -d '{"op":"take_screenshot"}' \
  | jq -r '.data.value' \
  | sed 's|^data:image/png;base64,||' \
  | base64 -d > screenshot.png
```

This is the pattern the screenshot harness in `scripts/screenshot.sh` uses to render `azul-render` fences in the documentation.

## Driving a headless app

Drive interactions through the same HTTP API documented in [Debugging](debugging.md). A headless run blocks on a wait condition just like a normal window blocks on `WaitMessage()` / `XNextEvent()` / `NSEvent`, so an idle process uses zero CPU. Inject events, query state, capture pixels:

```bash
post() { curl -s -X POST "http://127.0.0.1:8765/" -d "$1"; }

post '{"op":"wait_frame"}'
post '{"op":"resize","width":1024,"height":768}'
post '{"op":"click","selector":".increment-btn"}'
post '{"op":"wait_frame"}'
post '{"op":"take_screenshot"}' | jq -r '.data.value' > out.b64
```

For repeatable scenarios crystallised into a JSON file, see [End-to-End Testing](e2e-testing.md).

## Determinism and CI

CPU rendering is consistent in concept across platforms, but pixel-exact output differs at sub-pixel positioning, antialiasing, and font hinting boundaries. For CI screenshot diffing:

- Use `assert_screenshot` with a `max_diff_ratio` tolerance rather than byte-equality.
- Pin the harness to one platform (Linux is the cheapest in CI) and treat baselines from other platforms as informational.
- Bundle a font (DejaVu, Noto) with your test assets and reference it explicitly via `AppConfig.bundled_fonts`. Relying on the host's fontconfig produces different glyph metrics on different machines.

## Platform notes

- macOS, Windows, X11: native screenshot capture works.
- Wayland: the compositor does not expose other windows' contents to applications. Use the headless backend rather than trying to capture a visible window.

The reftest harness in `layout/tests/` is the reference implementation of these patterns.
