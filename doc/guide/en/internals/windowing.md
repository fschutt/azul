---
slug: windowing
title: Windowing
language: en
canonical_slug: windowing
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Per-window aggregate, headless variant, and the platform shell layer
prerequisites: [code-organization]
tracked_files:
  - layout/src/window.rs
  - layout/src/headless.rs
  - dll/src/desktop/shell2/mod.rs
  - dll/src/desktop/shell2/run.rs
  - dll/src/desktop/shell2/common/mod.rs
  - dll/src/desktop/shell2/common/event.rs
  - dll/src/desktop/shell2/common/layout.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
default-search-keys:
  - WindowCreateOptions
  - FullWindowState
  - StyledDom
  - LayoutCallback
  - AccessibilityInfo
---

# Windowing

## Overview

*WIP.* The windowing subsystem is the integration boundary between platform input and the layout engine. Two pieces sit at the centre: `LayoutWindow` is the per-window aggregate that drives layout each frame, and `shell2` is the per-OS layer that owns the native window, drains events, and presents rendered output. Every desktop backend (Windows, macOS, Linux X11, Linux Wayland, headless) embeds a `LayoutWindow` and pumps events through the same shared pipeline in `shell2/common`.

The split exists so that everything platform-independent — the layout solver, text engine, scroll/focus/cursor managers, callback dispatch, accessibility tree construction — lives in `azul-layout` and runs identically everywhere, while everything platform-specific — the actual `HWND` / `NSWindow` / `wl_surface`, IME bridge, GL/Vulkan context creation, native menu bar, screen-reader adapter — is isolated to one directory per OS.

The `HeadlessWindow` is the off-screen counterpart: it implements the same `PlatformWindow` contract as the native shells but skips WebRender, GL, and any actual window. It exists to drive E2E tests, screenshot reftests, and memory-leak scenarios on CI runners with no display server.

## LayoutWindow — the per-window aggregate

`LayoutWindow` (defined in `layout/src/window.rs`) is the per-window state that the platform shells drive each frame. It owns the `StyledDom` after the cascade, the `LayoutCache` from the previous frame, every per-window manager (`ScrollManager`, `FocusManager`, `SelectionManager`, `CursorManager`, `IFrameManager`, `GpuStateManager`, `GestureManager`, `A11yManager`), and the cached rendering resources (font metrics, image refs, scrollbar geometry).

Two entry points dominate the per-frame lifecycle:

- **`layout_and_generate_display_list`** — full rebuild. Runs the user `LayoutCallback`, recomputes the `StyledDom`, runs the cascade, lays out every nested DOM, registers scroll nodes, and generates a `DisplayList`. Fired on `Update::RefreshDom`, font-cache invalidation, and viewport breakpoint crossings.
- **`incremental_relayout`** — cheap path for resize. Re-runs layout against the existing `StyledDom` and skips the user callback. Fired from `WM_SIZE` / `ConfigureNotify` / `xdg_surface::configure` / `windowDidResize:`.

Every per-window manager lives directly on `LayoutWindow`. The full table is in [`code-organization`](code-organization.md); the [accessibility manager](windowing/accessibility.md) and per-platform IME state are documented on the per-OS pages.

## HeadlessWindow — off-screen variant

`HeadlessWindow` (in `layout/src/headless.rs`, with the platform-shell glue under `dll/src/desktop/shell2/headless/`) is the implementation of `PlatformWindow` that runs without a display server. It is selected by `AZ_BACKEND=headless` (or the legacy `AZUL_HEADLESS=1`).

Layout, callbacks, timers, scroll physics, accessibility tree construction, and the debug server all behave identically to a native window — only the rendering tail is replaced. Where a native backend reaches WebRender, headless reaches `cpurender`:

```text
WebRender path:   DisplayList -> WrRenderApi -> Renderer (GPU) -> swapBuffers
CpuBackend path:  DisplayList -> cpurender   -> Pixmap  (CPU)  -> (no-op or PNG)
```

This is what powers the reftest harness, the leak-deep-dive `AZ_E2E_TEST` scenarios, and Python-driven E2E tests against the public C API.

## The shell2 layer

`shell2` is the per-OS window-and-event layer. Each platform module owns one struct that implements the `PlatformWindow` trait, embedding a `CommonWindowState` with the `LayoutWindow` and shared book-keeping (current/previous `FullWindowState`, hit-tester, render API, document/pipeline IDs, image cache, renderer resources, font cache, frame-regeneration flags).

Per-platform sub-pages:

- [Common](windowing/common.md) — `AzBackend`, the `Compositor` trait, `PlatformWindow`, `CommonWindowState`, layout-regeneration helpers, GL function loader, error types, debug server, `AZ_E2E_TEST` runner, and the headless event loop.
- [Windows](windowing/windows.md) — `Win32Window`, `WindowProc`, IME via Imm32, DPI awareness, `HMENU` menu bar, UIA accessibility.
- [macOS](windowing/macos.md) — `MacOSWindow`, `NSApplication`, `NSOpenGLContext`, `NSTextInputClient` IME, `CVDisplayLink` VSYNC, `NSAccessibility` adapter.
- [Linux X11](windowing/linux-x11.md) — `X11Window`, Xlib + EGL via runtime dlopen, XKB keyboard, XIM IME with GTK fallback, AT-SPI accessibility.
- [Linux Wayland](windowing/linux-wayland.md) — `WaylandWindow`, `xdg-shell`, `text-input v3` IME, KDE blur, `wl_subsurface` tooltips.
- [Linux DBus](windowing/linux-dbus.md) — generic libdbus-1 dlopen, GNOME global menu via `org.gtk.Menus` / `org.gtk.Actions`.
- [Menus and CSD](windowing/menus-and-csd.md) — the unified `show_menu` pipeline, client-side decorations, the `csd-*` stylesheet.
- [Accessibility](windowing/accessibility.md) — the three-layer pipeline from `AccessibilityInfo` through `A11yManager` to the per-OS adapter.

## Cross-platform abstractions

Every backend implements the same handful of contracts in `shell2/common`. The `PlatformWindow` trait provides default implementations for the shared logic so each platform only needs to wire getters via the `impl_platform_window_getters!` macro and call into the trait methods at the right points. The pieces every backend reuses:

- **`process_window_events`** — state-diffing between `previous_window_state` and `current_window_state`, callback dispatch, and result handling. Implemented once in `common/event.rs` as a default trait method.
- **`apply_system_change`** — applies pre- and post-callback system effects: clipboard paste, focus changes, scroll-into-view, text changesets. Backends only need to implement the platform-specific calls (e.g., `SetClipboardText` on Win32, `NSPasteboard` on macOS); the dispatch is shared.
- **IME contracts** — every backend feeds composition strings into `LayoutWindow.cursor_manager.preedit` and commits via the same `process_text_input` entry point. The platform code differs (Imm32, NSTextInputClient, XIM, text-input v3), the layout-side code is identical.
- **GL / Vulkan context** — `common/gl_loader.rs` exports `load_gl_context(get_func)` which fills a `GenericGlContext` from a caller-supplied symbol resolver. Each backend supplies a closure that delegates to its native loader (`wglGetProcAddress`, `dlsym` over `OpenGL.framework`, `eglGetProcAddress`).
- **Accessibility tree push** — `A11yManager::update_tree` produces an `accesskit::TreeUpdate` that each platform adapter forwards to the OS bridge (`accesskit_unix`, `accesskit_macos`, `accesskit_windows`). Action requests come back through the same channel and are decoded by `A11yManager::handle_action_request`.
- **Layout regeneration** — `regenerate_layout` and `incremental_relayout` are free functions in `common/layout.rs`, intentionally not trait methods so they can borrow disjoint fields of the platform window.

## Backend selection and event loop entry

`run.rs` exposes one `pub fn run(...)` per `target_os`. Every variant resolves the `AzBackend` once via `AzBackend::resolve` (driven by `AZ_BACKEND`, `WindowCreateOptions.renderer.hw_accel`, and finally `Auto`), then dispatches:

- `Web(addr)` — delegate to `crate::web::run_web` (HTTP server, no native window).
- `Headless` — build a `HeadlessWindow` and enter its loop.
- Anything else — call into the OS-specific window construction (`MacOSWindow::new_with_fc_cache`, `Win32Window::new`, `LinuxWindow::new_with_resources`).

The OS-specific loop bodies differ in detail but share the same phases each tick: drain native events, process `pending_window_creates` (popup menus, dialogs, child windows), render windows that flagged `frame_needs_regeneration`, and block on the OS idle primitive (`NSRunLoop.runMode`, `WaitMessage`, `select(2)` on the X11 fd, `Condvar` for headless). The per-platform pages cover the loop body in detail.

## Coming Up Next

- [Common](windowing/common.md) — Shared shell infrastructure across platforms
- [Windows](windowing/windows.md) — Win32 messages, DirectComposition, IME
- [macOS](windowing/macos.md) — Cocoa, AppKit, NSTextInputClient, accessibility
- [Linux X11](windowing/linux-x11.md) — Xlib, EGL, XKB, XIM
- [Linux Wayland](windowing/linux-wayland.md) — wl_surface, xdg-shell, text-input v3
- [Linux DBus](windowing/linux-dbus.md) — DBus integration for GNOME menus
- [Menus and CSD](windowing/menus-and-csd.md) — Unified menu pipeline and client-side decorations
- [Accessibility](windowing/accessibility.md) — Per-platform a11y bridges
- [Events](events.md) — Hit-testing, callback invocation, the Update protocol
- [Rendering](rendering.md) — From `DisplayList` to pixels
