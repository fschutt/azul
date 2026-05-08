---
slug: windowing/common
title: Windowing — Common
language: en
canonical_slug: windowing/common
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Shared shell infrastructure across platforms
prerequisites: [windowing]
tracked_files:
  - dll/src/desktop/shell2/mod.rs
  - dll/src/desktop/shell2/run.rs
  - dll/src/desktop/shell2/common/mod.rs
  - dll/src/desktop/shell2/common/compositor.rs
  - dll/src/desktop/shell2/common/cpu_compositor.rs
  - dll/src/desktop/shell2/common/dlopen.rs
  - dll/src/desktop/shell2/common/error.rs
  - dll/src/desktop/shell2/common/event.rs
  - dll/src/desktop/shell2/common/layout.rs
  - dll/src/desktop/shell2/common/gl_loader.rs
  - dll/src/desktop/shell2/common/debug_server.rs
  - dll/src/desktop/shell2/common/e2e_test.rs
  - dll/src/desktop/shell2/headless/mod.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
default-search-keys:
  - App
  - StyledDom
  - WindowCreateOptions
  - LayoutCallback
---

# Windowing — Common

## Overview

*WIP — the trait surface is settled but the CPU compositor and several headless-only paths still carry `TODO`s.* `shell2` is the per-OS window-and-event layer. The mod root declares one platform module per target (`macos`, `windows`, `linux`, `ios`, `headless`) and re-exports the active one as `Window` / `WindowEvent`. The `common/` subtree holds the platform-agnostic pieces every backend reuses: backend selection, the `PlatformWindow` trait, error types, dynamic-library loading, the GL function-pointer loader, the layout-regeneration workflow, the debug server, and the `e2e_test` scenario runner.

This page covers everything outside the platform-specific directories. Each backend gets its own page: [X11](linux-x11.md), [Wayland](linux-wayland.md), [DBus / GNOME menus](linux-dbus.md), [Windows](windows.md), [macOS](macos.md). The `headless` backend is documented at the bottom of this page since it lives outside the platform tree but consumes only common code.

## Crate layout

```text
shell2/
├── common/              ← this page
│   ├── compositor.rs    AzBackend, CompositorMode, Compositor trait
│   ├── cpu_compositor.rs CPU compositor stub
│   ├── debug_server.rs  HTTP debug API + E2E test executor
│   ├── dlopen.rs        DynamicLibrary trait + load_first_available
│   ├── e2e_test.rs      AZ_E2E_TEST scenario runner (feature-gated)
│   ├── error.rs         WindowError, CompositorError, DlError
│   ├── event.rs         PlatformWindow trait, CommonWindowState
│   ├── gl_loader.rs     load_gl_context — fills GenericGlContext
│   └── layout.rs        regenerate_layout, incremental_relayout
├── headless/            HeadlessWindow + CpuBackend + AZ_BACKEND=headless
├── run.rs               run() — per-OS event-loop entry point
├── linux/{x11,wayland,dbus,gnome_menu,common,registry,resources,timer}
├── macos/
├── windows/
└── ios/
```

The dispatch resolves to one `Window` / `WindowEvent` pair via `cfg_if`, picking the active platform module:

```rust,ignore
cfg_if::cfg_if! {
    if #[cfg(target_os = "macos")] {
        pub use macos::MacOSWindow as Window;
        pub use macos::MacOSEvent as WindowEvent;
    } else if #[cfg(target_os = "windows")] {
        pub use windows::Win32Window as Window;
        pub use windows::Win32Event as WindowEvent;
    } else if #[cfg(target_os = "linux")] {
        pub use linux::LinuxWindow as Window;
        pub use linux::LinuxEvent as WindowEvent;
    } else {
        pub use headless::HeadlessWindow as Window;
        pub use headless::HeadlessEvent as WindowEvent;
    }
}
```

## AzBackend resolution

`AzBackend` is the unified backend selector. It supersedes the older `AZUL_HEADLESS` / `AZUL_RENDERER` / `AZ_COMPOSITOR` trio.

```rust,ignore
pub enum AzBackend {
    Auto,                                        // default — try GPU, fall back
    Gpu,                                         // force GPU (OpenGL / Metal / D3D)
    Cpu,                                         // CPU rendering in a native window
    Headless,                                    // CPU + no native window
    #[cfg(feature = "web")] Web(SocketAddr),    // serve as HTML over HTTP
}
```

Resolution order, set by `AzBackend::resolve`:

1. `AZ_BACKEND` env var. Accepted values: `headless`, `cpu`, `gpu` / `opengl` / `gl`, `auto`, and (when the `web` feature is on) anything parseable by `web::config::parse_web_url`.
2. `WindowCreateOptions.renderer.hw_accel`: `HwAcceleration::Disabled → Cpu`, `Enabled → Gpu`, `DontCare → fall through`.
3. Default: `Auto`.

`run` calls `resolve_backend(&root_window)` once, then branches:

- `Web(addr)` → `crate::web::run_web` (HTTP server, no native window).
- `Headless` → `run_headless` builds a `HeadlessWindow` and enters its loop.
- Anything else → the OS-specific event loop.

## CompositorMode and the GPU blacklist

`CompositorMode` is the lower-level `GPU | CPU | Auto` choice consumed by `Compositor` impls. It deliberately duplicates a subset of `AzBackend` so a single window can flip between GPU and CPU at runtime via `Compositor::try_switch_mode` without touching the process-wide `AzBackend`.

`GpuInfo` is the populated GL string set (`GL_VENDOR`, `GL_RENDERER`, `GL_VERSION`, `GL_SHADING_LANGUAGE_VERSION`). `check_gpu_blacklist` returns `GpuCheckResult`. Patterns it flags:

- **Mesa software rasteriser.** `llvmpipe` or `softpipe` in `GL_RENDERER`. `cpurender` is faster.
- **NVIDIA driver without GLSL.** NVIDIA vendor with an empty `GL_SHADING_LANGUAGE_VERSION`. The driver loads but cannot compile shaders (tracked as azul#220).
- **Old Intel GL.** Intel vendor with GL major version `< 3`. WebRender requires GL 3.0+.

`check_gpu_blacklist` has no production call site yet (autoreview report flagged this as `[HIGH]` dead code). It is wired up to be called after a successful GL context creation in `Auto` mode; the call site is pending.

## The Compositor trait

```rust,ignore
pub trait Compositor {
    fn new(context: RenderContext, mode: CompositorMode) -> Result<Self, CompositorError>
        where Self: Sized;
    fn render(&mut self, display_list: &DisplayList) -> Result<(), CompositorError>;
    fn resize(&mut self, new_size: PhysicalSizeU32) -> Result<(), CompositorError>;
    fn get_mode(&self) -> CompositorMode;
    fn try_switch_mode(&mut self, mode: CompositorMode) -> Result<(), CompositorError>;
    fn flush(&mut self);
    fn present(&mut self) -> Result<(), CompositorError>;
}
```

`RenderContext` carries platform-specific GPU handles as raw pointers (`OpenGL`, `Metal`, `D3D11`) or `u64` Vulkan handles. `Send`/`Sync` are unsafely implemented; the caller must keep cross-thread access to these contexts synchronised via `wglMakeCurrent` / `glXMakeCurrent` / `CGLSetCurrentContext`.

`CpuCompositor` is the only concrete impl in `common/`. Today it only allocates an RGBA8 framebuffer and clears it to white in `rasterize`; the autoreview reports flag the whole file as a stub (`HIGH` finding). The real GPU path lives in WebRender via `wr_translate2`, not in this trait.

## PlatformWindow and CommonWindowState

The shared event-processing logic lives in `common/event.rs`. Every backend embeds a `CommonWindowState` field (named `common`) that holds the layout window, current/previous `FullWindowState`, hit-tester, render API, document/pipeline IDs, image cache, renderer resources, fc_cache, icon provider, and frame-regeneration flags. A backend implements `PlatformWindow` by providing a small set of getters via the `impl_platform_window_getters!` macro (used by every native window).

`PlatformWindow` then provides default impls for:

- `process_window_events()` — state-diffing between `previous_window_state` and `current_window_state` (via `azul_layout::window_state::create_events_from_states`), callback dispatch, and result handling.
- `dispatch_events_propagated()` — recursive event propagation.
- `update_hit_test()` — pushes the hit-test result into the `HoverManager` keyed by `InputPointId`.
- Scrollbar interaction — `perform_scrollbar_hit_test`, `handle_scrollbar_click`, `handle_scrollbar_drag`.
- Pre-event processing for scroll physics, text input, and a11y change recording.

The lifecycle for a native event handler (mouse, key, resize, scroll) is:

1. Update the relevant fields in `current_window_state`.
2. Call `update_hit_test()` if cursor moved.
3. Call `process_window_events()` and react to the returned `ProcessEventResult` (request redraw, regenerate layout, close, etc.).

Per-OS notes on where to call this — modifier handling, IME quirks, coordinate translation — are in the module-level doc-comment of `common/event.rs`.

## Layout regeneration

`common/layout.rs` exports two free functions instead of trait methods. The free-function shape is intentional: it sidesteps borrow-checker issues that arise when `regenerate_layout` would otherwise want `&mut self` on a trait object whose fields the function also needs to borrow individually.

- **`regenerate_layout`.** Full rebuild. Runs the user `LayoutCallback`, recomputes the StyledDom, runs the cascade, lays out every DOM, registers scroll nodes, and generates the frame.
- **`incremental_relayout`.** Cheap path for resize. Re-runs layout against the existing StyledDom and skips the user callback.
- **`generate_frame`.** Translates `DisplayList` to a WebRender `Transaction` and submits it.

`incremental_relayout` is what fires from `WM_SIZE` / `ConfigureNotify` / `xdg_surface::configure` / `windowDidResize:`. The full rebuild fires on DOM changes (`Update::RefreshDom`), font-cache invalidation, and viewport breakpoint crossings (the per-OS handlers in [X11](linux-x11.md), [Wayland](linux-wayland.md), [Windows](windows.md), [macOS](macos.md) compare the `DynamicSelectorContext` against `CSS_BREAKPOINTS`).

## DynamicLibrary trait

```rust,ignore
pub trait DynamicLibrary {
    fn load(name: &str) -> Result<Self, DlError> where Self: Sized;
    unsafe fn get_symbol<T>(&self, name: &str) -> Result<T, DlError>;
    fn unload(&mut self);
}
```

`load_first_available::<L>(&["libX11.so.6", "libX11.so"])` walks a name list and returns the first one that loads, with a `DlError::LibraryNotFound { name, tried, suggestion }` aggregating the errors otherwise. Linux backends use this with `Library` (a thin wrapper over `libc::dlopen` / `dlsym` / `dlclose`, defined in the X11 dlopen module and re-exported by Wayland and dbus). The Windows backend defines its own non-trait `DynamicLibrary` struct; the autoreview report flags the resulting inconsistency as `[MEDIUM]` — both implementations work but `load_first_available` is unreachable on Windows.

The `load_symbol!` macro wraps the unsafe `get_symbol` call with early-return error propagation; the entire mechanical part of every `Xlib::new` / `Wayland::new` / `Egl::new` is hundreds of lines of `load_symbol!(...)` invocations.

## Error types

`common/error.rs` defines three enums every backend converts into:

```rust,ignore
pub enum WindowError {
    PlatformError(String),
    ContextCreationFailed,
    WindowClosed,
    InvalidState(String),
    NoBackendAvailable,           // Linux: neither X11 nor Wayland
    Unsupported(String),
}

pub enum CompositorError {
    NoGPU, ShaderError(String), OutOfMemory, ContextLost,
    UnsupportedMode(String), RenderFailed(String), ResizeFailed(String),
}

pub enum DlError {
    LibraryNotFound { name: String, tried: Vec<String>, suggestion: String },
    SymbolNotFound { symbol: String, library: String, suggestion: String },
    InvalidLibrary(String),
    VersionMismatch { found: String, required: String },
}
```

All three are `Clone + Display + Error`. `WindowError` is the type returned from every `*::new` constructor; the `run()` entry point propagates it back up.

## GL function loading

`common/gl_loader.rs` exports a single function:

```rust,ignore
pub fn load_gl_context(get_func: impl Fn(&str) -> *mut c_void) -> GenericGlContext;
```

The body is roughly 800 lines of `glFoo: get_func("glFoo")` field assignments into `GenericGlContext`. Each backend supplies a closure that resolves GL symbols through the platform's preferred mechanism: `eglGetProcAddress` on Linux, `dlsym` over `OpenGL.framework` on macOS, `wglGetProcAddress` on Windows. Keeping the closure caller-supplied is how this single function services every backend without `cfg`-gating.

## run() — per-OS event loop entry

`run.rs` exposes one `pub fn run(...)` per `target_os`. The first ~30 lines of every variant are identical: read `AZUL_DEBUG` / `AZ_DEBUG` (which port to start the debug server on), read `AZ_E2E` (JSON file of E2E tests), build the channel + component map. Then:

- Headless or Web → delegate to `run_headless` / `run_web`.
- Otherwise call into the OS-specific window construction (`MacOSWindow::new_with_fc_cache`, `Win32Window::new`, `LinuxWindow::new_with_resources`).

What differs is the loop body. Each backend's loop is documented on its own page; common phases are:

1. Drain native events (non-blocking).
2. Process `pending_window_creates` (popup menus, dialogs, child windows).
3. Render windows that flagged `frame_needs_regeneration`.
4. Block until the next event with the OS-native idle primitive (`NSRunLoop.runMode`, `WaitMessage`, `select(2)` on the X11 fd, `Condvar` for headless).

## The headless backend

`HeadlessWindow` is a fully functional implementation of `PlatformWindow` with no GPU and no native window. Selected by `AZ_BACKEND=headless` (or the legacy `AZUL_HEADLESS=1`).

Layout, callbacks, timers, scroll physics, and the debug server all work — only rendering is replaced. Where a native backend reaches WebRender, headless reaches `CpuBackend`:

```text
WebRender path:   DisplayList → WrRenderApi → Renderer (GPU) → swapBuffers
CpuBackend path:  DisplayList → cpurender   → Pixmap  (CPU)  → (no-op / PNG)
```

The event loop blocks on a `Condvar` signalled when:

- An event is injected via `inject_event` / debug server.
- The earliest timer deadline elapses.
- A background `Thread` completes.

If none of those can ever fire, the loop blocks indefinitely and prints a warning. This mirrors the behaviour of a real window nobody interacts with.

The autoreview report on the headless backend lists three public test-API methods that have no in-tree callers (`inject_events`, `has_active_timers`, `pending_window_count`); they are intended for external test harnesses, not internal use.

## AZ_E2E_TEST scenario runner

`common/e2e_test.rs` (gated by the `e2e-test` cargo feature) is a deterministic resize/tick harness used to reproduce memory leaks without standing up a real window. Activated by setting `AZ_E2E_TEST=path/to/scenario.json`.

The JSON schema (the `Step` enum):

- **`resize`.** Updates dimensions and calls `incremental_relayout` for the fast path.
- **`resize_full`.** Updates dimensions and calls `regenerate_layout` for a full rebuild.
- **`tick`.** Calls `regenerate_layout` only.
- **`sleep_ms`.** Calls `std::thread::sleep`.

A scenario can wrap its steps in a `loop { iterations: N, steps_range: [a, b) }` and configure `rss_probes` to:

- Sample RSS every `every_n_iterations` (default 100).
- Skip `warmup_skip` early probes.
- Fail the run if growth exceeds `assert_growth_mib_max` MiB or the absolute RSS exceeds `assert_absolute_mib_max` MiB.
- With `memory_breakdown: true`, emit a flat `mem` JSONL event per probe attributing bytes to every `StyledDom` / `Solver3LayoutCache` / `TextLayoutCache` / manager field that exposes a count or `memory_report()`.

`run_e2e_scenario` bypasses `NSApplication` / `select(2)` entirely — it constructs a `HeadlessWindow`, runs warmup ticks, then drives the scripted steps in-thread and exits the process with code 0 (pass) or 1 (RSS budget breached).

This is separate from `AZ_E2E=` (debug-server-dispatched assertion scenarios that run alongside a normal window).

## What lives where for a contributor

- **Add a new backend selector value.** Edit the `AzBackend` enum and update the dispatch in `run.rs`.
- **Add a GPU blacklist entry.** Extend the pattern matches in `check_gpu_blacklist`.
- **Add a window error variant.** Add it to the `WindowError` enum.
- **Add a default `PlatformWindow` method.** Add it to the trait with a default body.
- **Tweak the layout-regeneration order.** Edit the orchestrator functions in `common/layout.rs`.
- **Add a new debug-server event.** Extend the message switch in `process_debug_event`.
- **Add a leak-test scenario.** Author a JSON scenario under `research/calc-regression-triage/leak-deep-dive/scripts/` and run with `AZ_E2E_TEST`.

## Coming Up Next

- [Windows](windows.md) — Windows shell - Win32 messages, DirectComposition, IME
- [macOS](macos.md) — macOS shell - Cocoa, AppKit, IME, a11y
- [Linux Wayland](linux-wayland.md) — Linux Wayland shell - wl_surface, xdg-shell, libinput
- [Linux X11](linux-x11.md) — Linux X11 shell - Xlib, GLX, XInput2
- [Windowing Overview](../windowing.md) — Per-window aggregate, headless variant, and the platform shell layer
