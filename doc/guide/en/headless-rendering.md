---
slug: headless-rendering
title: Headless Rendering
language: en
canonical_slug: headless-rendering
audience: external
maturity: wip
guide_order: 290
topic_only: false
prerequisites: [dom]
tracked_files:
  - layout/src/cpurender.rs
  - dll/src/desktop/native_screenshot.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T17:30:00Z
---

> **WIP.** Free-standing CPU rendering APIs (`render_dom_to_image`,
> `render_svg_to_png`, `render_component_preview`) are stable. The
> `HeadlessWindow` event loop and `NativeScreenshotExt` trait are still
> evolving — Wayland/XCB native screenshot support is incomplete.

Azul's CPU renderer (`layout/src/cpurender.rs`, backed by
[agg-rust](https://github.com/fschutt/agg-rust)) can be used without a
display server, an OpenGL context, or even a window. Three usage
modes:

| Mode | Use when | Entry point |
|---|---|---|
| Free-standing render | One DOM → one PNG, no event loop | `render_dom_to_image` |
| Component preview | A `StyledDom` → PNG with sizing options | `render_component_preview` |
| Full headless app | A real `App` running without a window | `HeadlessWindow` (`AZUL_HEADLESS=1`) |

```azul-render screenshot=headless-hello width=400 height=200 subtitle="A free-standing render — no window, no display server"
<body style="background: white; font-family: sans-serif;">
  <p style="font-size: 28px; padding: 24px; color: #1d4f8b;">Hello from cpurender</p>
</body>
```

## `render_dom_to_image` — DOM + CSS to PNG

The simplest entry point. Given a `Dom`, a `Css`, and a target size,
returns PNG bytes:

```rust,ignore
pub fn render_dom_to_image(
    dom: azul_core::dom::Dom,
    css: azul_css::css::Css,
    width: f32,
    height: f32,
    dpi: f32,
) -> Result<Vec<u8>, String>
```

```rust,no_run
# use azul_core::dom::Dom;
# use azul_css::css::Css;
# use azul_layout::cpurender::render_dom_to_image;
let dom = Dom::body().with_child(
    Dom::create_text("Hello, headless world.")
);
let css = Css::empty();
let png_bytes = render_dom_to_image(dom, css, 400.0, 200.0, 1.0)?;
std::fs::write("hello.png", png_bytes)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`render_dom_to_image` builds a `StyledDom`, runs one layout pass with
a fresh `FontManager` (system fonts via fontconfig), rasterizes the
display list to an `AzulPixmap`, and PNG-encodes the buffer. It is
gated behind the `std`, `text_layout`, and `font_loading` Cargo
features.

`dpi` is the device-pixel multiplier: `1.0` produces `width × height`
pixels, `2.0` produces `2*width × 2*height` (Retina), and so on. The
logical dimensions stay the same; only resolution changes.

## `render_component_preview` — rendering a `StyledDom`

When you already have a `StyledDom` (e.g. from a widget builder),
prefer `render_component_preview`. It accepts a pre-built
`FontManager` and an optional `SystemStyle`, and lets the content size
itself to fit:

```rust,ignore
pub struct ComponentPreviewOptions {
    pub width: Option<f32>,    // None = size to content (max 4096)
    pub height: Option<f32>,
    pub dpi_factor: f32,
    pub background_color: ColorU,
}

pub struct ComponentPreviewResult {
    pub png_data: Vec<u8>,
    pub content_width: f32,
    pub content_height: f32,
}

pub fn render_component_preview(
    styled_dom: StyledDom,
    font_manager: &FontManager<FontRef>,
    opts: ComponentPreviewOptions,
    system_style: Option<Arc<SystemStyle>>,
) -> Result<ComponentPreviewResult, String>;
```

If both `width` and `height` are `Some`, the renderer obeys them
exactly. If either is `None`, the renderer measures the laid-out
content and fits to it (up to a hard cap of 4096 px). The returned
`content_width` / `content_height` are the actual logical pixel
dimensions used.

This is the API the screenshot harness (`scripts/screenshot.sh`) uses
to render `azul-render` fences in the documentation.

## `render_svg_to_png` — direct SVG to PNG

For SVG payloads that do not need CSS layout, `render_svg_to_png`
parses the XML directly and rasterizes via agg-rust without going
through the layout solver:

```rust,ignore
pub fn render_svg_to_png(
    svg_data: &[u8],
    target_width: u32,
    target_height: u32,
) -> Result<Vec<u8>, String>;
```

Use this when you want SVG rendering as a feature of your application
(thumbnail generation, server-side icon rendering) without paying for
the full HTML/CSS engine. It is gated behind the `std` and `xml`
features.

For SVG embedded in an HTML page, use `<svg>` inside the DOM — the
normal layout/render path handles it.

## `HeadlessWindow` — a full app loop without a display server

`HeadlessWindow` (`dll/src/desktop/shell2/headless/mod.rs`) implements
the full `PlatformWindow` trait without GPU or windowing-system
dependencies. The DOM is laid out, callbacks fire, timers tick, and
state changes drive re-renders — but there is no actual window on
screen. Output is captured by calling the screenshot APIs from
inside a callback.

```text
WebRender path:   DisplayList → WrRenderApi → Renderer (GPU) → swapBuffers
CpuBackend path:  DisplayList → cpurender   → Pixmap  (CPU)  → (no-op / PNG)
```

The event loop blocks on a `Condvar` that is signalled when an event
is injected, a timer fires, or a background thread completes — so an
idle headless app uses zero CPU, just like a native window waiting on
`WaitMessage()` / `XNextEvent()` / `NSEvent`. If nothing can wake the
loop, it blocks indefinitely (intentional: a real window with no
interaction would do the same).

Programmatic event injection is the primary way to drive a headless
app:

```rust,ignore
pub enum HeadlessEvent {
    Close,
    MouseMove { x: f32, y: f32 },
    MouseDown { button: MouseButton },
    MouseUp { button: MouseButton },
    KeyDown { virtual_keycode: VirtualKeyCode },
    KeyUp { virtual_keycode: VirtualKeyCode },
    TextInput { text: String },
    Resize { width: f32, height: f32 },
    Scroll { delta_x: f32, delta_y: f32 },
}
```

Inject events with `HeadlessWindow::inject_event` or via the debug
server (`AZUL_DEBUG=1`). End-to-end tests in
`dll/tests/headless_*.rs` exercise this path.

## Selecting the headless backend at runtime

Two environment variables, set before the process starts:

| Variable | Effect |
|---|---|
| `AZUL_HEADLESS=1` | Force the headless backend even when a display is available. |
| `AZ_BACKEND=headless` | Same effect, in the unified backend selector. |
| `AZUL_DEBUG=1` | Start the debug server (HTTP API for event injection + screenshots). |

```bash
AZUL_HEADLESS=1 ./my_azul_app
AZUL_HEADLESS=1 AZUL_DEBUG=1 ./my_azul_app
```

The same binary that runs your GUI on a desktop can be deployed to a
server-side container with `AZUL_HEADLESS=1` and used to render
screenshots, run integration tests, or pre-render content for caching.

## `HeadlessConfig` — viewport and DPI

```rust,ignore
pub struct HeadlessConfig {
    pub width: f32,        // logical pixels (default 800.0)
    pub height: f32,       // logical pixels (default 600.0)
    pub dpi_factor: f32,   // 1.0 = 96 DPI, 2.0 = Retina
    pub enable_rendering: bool, // false = layout-only (no pixels)
    pub max_iterations: Option<usize>, // safety cap (default 1000)
}
```

`enable_rendering = false` skips rasterization entirely — useful for
unit tests that only verify the layout / event handling and do not
need pixels. `max_iterations` is a safety cap for tests so a runaway
event loop terminates instead of hanging the suite. In a long-running
production headless deployment, set it to `None`.

## Capturing the running window with `NativeScreenshotExt`

`NativeScreenshotExt` (`dll/src/desktop/native_screenshot.rs:29`)
extends `CallbackInfo` with three methods that capture the current
window — including OS-drawn decorations (title bar, borders, drop
shadow):

```rust,ignore
pub trait NativeScreenshotExt {
    fn take_native_screenshot(&self, path: &str) -> Result<(), AzString>;
    fn take_native_screenshot_bytes(&self) -> Result<Vec<u8>, AzString>;
    fn take_native_screenshot_base64(&self) -> Result<AzString, AzString>;
}
```

These are usable from any callback context. Call from a button click
to save a screenshot when the user presses a key, or from a debug
server route to expose a `/screenshot` endpoint.

```rust,ignore
use azul_dll::desktop::native_screenshot::NativeScreenshotExt;

extern "C" fn on_click(_: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let _ = info.take_native_screenshot("/tmp/window.png");
    Update::DoNothing
}
```

## `take_native_screenshot_bytes` — in-memory PNG

`take_native_screenshot_bytes` returns the PNG payload directly with
no temporary file. Use it when you want to ship the bytes over a
socket, embed them in HTML via `take_native_screenshot_base64`, or
hash them for diffing against a baseline:

```rust,ignore
let png = info.take_native_screenshot_bytes()?;
let data_uri = info.take_native_screenshot_base64()?;
```

## Platform support table

| Platform | Native screenshot API | Status |
|---|---|---|
| macOS | `CGWindowListCreateImage` (Core Graphics) | shipped |
| Windows | `PrintWindow` (BitBlt from window DC) | shipped |
| Linux / X11 (Xlib) | `XGetImage` via `dlopen` | shipped |
| Linux / X11 (XCB) | (planned, not implemented) | stub |
| Linux / Wayland | (not supported — use the Xlib backend) | n/a |

The Xlib path uses runtime `dlopen` to avoid a static link against
libX11, so a binary built on a Wayland-only system still loads. On
Wayland, fall back to in-process CPU rendering via
`render_component_preview` — Wayland does not let arbitrary
applications read each other's window contents.

## Determinism and CI

CPU rendering is the same across platforms in concept, but pixel-exact
output differs at sub-pixel positioning, antialiasing, and font
hinting boundaries. For CI screenshot diffing:

- Use `cpurender::pixel_diff` /
  `cpurender::compare_against_reference` rather than byte-equality.
- Pin the harness to one platform (Linux is the cheapest in CI) and
  treat baselines from other platforms as informational.
- Bundle a font (DejaVu, Noto) with your test assets and pass it
  explicitly to `render_component_preview` — relying on the host's
  fontconfig produces different glyph metrics on different machines.

The reftest harness in `layout/tests/` is the reference implementation
of this pattern.
