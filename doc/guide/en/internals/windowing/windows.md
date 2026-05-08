---
slug: windowing/windows
title: Windowing — Windows
language: en
canonical_slug: windowing/windows
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Windows shell - Win32 messages, DirectComposition, IME
prerequisites: [windowing/common]
tracked_files:
  - dll/src/desktop/shell2/windows/accessibility.rs
  - dll/src/desktop/shell2/windows/clipboard.rs
  - dll/src/desktop/shell2/windows/dlopen.rs
  - dll/src/desktop/shell2/windows/dpi.rs
  - dll/src/desktop/shell2/windows/gl.rs
  - dll/src/desktop/shell2/windows/menu.rs
  - dll/src/desktop/shell2/windows/mod.rs
  - dll/src/desktop/shell2/windows/registry.rs
  - dll/src/desktop/shell2/windows/system_style.rs
  - dll/src/desktop/shell2/windows/tooltip.rs
  - dll/src/desktop/shell2/windows/wcreate.rs
  - dll/src/desktop/shell2/windows/win_event.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
default-search-keys:
  - WindowCreateOptions
  - CoreMenuCallback
  - AppConfig
  - WindowSize
---

# Windowing — Windows

## Overview

*WIP — main lifecycle is stable, the per-monitor DPI path and IME composition window have rough edges.* The Windows backend is `Win32Window`. It uses the Win32 API loaded entirely via dlopen (`LoadLibraryA` + `GetProcAddress`) so the binary cross-compiles from macOS without a Win32 SDK present. The struct holds the `HWND`, `HINSTANCE`, render mode (GPU via `HGLRC` or CPU), the embedded `event::CommonWindowState`, the `Win32Libraries` function-pointer table, multi-window state (`pending_window_creates`), the menu bar and active context menu, the DPI helpers, the IME composition string, the tooltip window, and the optional accessibility adapter.

## Win32Libraries and dlopen

`Win32Libraries` declares one struct per DLL — `User32`, `Gdi32`, `Kernel32`, `OpenGL32`, `Imm32`, `Shcore`, `UxTheme`, `Dwmapi` — each holding `extern "system"` function pointers. `Win32Libraries::load` calls `load_dll("user32.dll")`, `load_dll("gdi32.dll")`, etc. and populates each struct. There is no shared `DynamicLibrary` trait implementation here — the autoreview report flags this as an inconsistency with the Linux backends; `load_first_available` is unreachable on Windows.

The Win32 type aliases map every opaque handle to `*mut c_void`:

```rust,ignore
pub type HWND       = *mut c_void;
pub type HDC        = *mut c_void;
pub type HGLRC      = *mut c_void;
pub type HMENU      = *mut c_void;
pub type HINSTANCE  = *mut c_void;
pub type HMONITOR   = *mut c_void;
pub type HIMC       = *mut c_void;   // IME context handle
pub type WPARAM     = usize;
pub type LPARAM     = isize;
pub type LRESULT    = isize;
```

Win32 structs (`MSG`, `WNDCLASSW`, `RECT`, `POINT`, `TRACKMOUSEEVENT`, `COMPOSITIONFORM`, etc.) are `#[repr(C)]` mirrors of the headers in `Windows.h`. Windows-specific dlopen also exposes `encode_wide(s: &str) -> Vec<u16>` for converting Rust strings to UTF-16 zero-terminated buffers (every Win32 API takes wide strings).

## Window creation

`Win32Window::new` is the constructor:

1. `Win32Libraries::load()` — load every required DLL.
2. `dpi::DpiFunctions::new()` — load the DPI APIs (`SetProcessDpiAwarenessContext`, `GetDpiForWindow`, `AdjustWindowRectExForDpi`, etc.). All five of these are version-gated; older Windows falls back to `SetProcessDpiAware` from XP / Vista.
3. `dpi.set_process_dpi_aware()` — register per-monitor V2 DPI awareness so the window won't get bilinearly scaled by the OS.
4. `wcreate::register_window_class(hinstance, window_proc, &win32)` — registers the `"AzulWindowClass"` window class with a null background brush (we paint the entire window with OpenGL or the CPU compositor; letting Windows paint a brush flashes black/white during creation).
5. `wcreate::create_hwnd(hinstance, options, parent, user_data, &win32)` — `CreateWindowExW` with `WS_OVERLAPPEDWINDOW`. The window is *not* shown yet — `ShowWindow` is deferred until after the first frame is presented.
6. `wcreate::create_gl_context(hwnd, &win32)` — see below.
7. WebRender + image cache + renderer setup, identical to other backends.
8. `SetWindowLongPtrW(hwnd, GWLP_USERDATA, window_ptr as isize)` — stash the `*mut Win32Window` in the window's user data so `WindowProc` can recover it.

The deferred `ShowWindow` call lives in `render_and_present` after the first `SwapBuffers` succeeds; this avoids the classic "window appears white then content paints" flash.

## GL context creation — three-step fallback

`wcreate::create_gl_context` tries three OpenGL versions in order:

1. **OpenGL 3.2 Core via `wglCreateContextAttribsARB`** — preferred. Requires WGL extensions, which require a *dummy* GL 1.x context to exist first so `wglGetProcAddress("wglCreateContextAttribsARB")` returns non-null. The dummy context is created on a hidden window, queried for the extension, then destroyed.
2. **OpenGL 3.0 via `wglCreateContextAttribsARB`** — same path, `WGL_CONTEXT_MAJOR_VERSION_ARB = 3`, `MINOR = 0`.
3. **Legacy `wglCreateContext`** — produces a 1.x compatibility context. WebRender then runs in compatibility mode.

If all three fail, the path falls through to CPU rendering (`RenderMode::Cpu`) and the window paints via `StretchDIBits` — see the `cpurender` feature flag.

`gl::GlFunctions::initialize` loads `opengl32.dll`. The fill closure passed to `common::gl_loader::load_gl_context` first asks `wglGetProcAddress(name)`; if that returns null, it falls back to `GetProcAddress(opengl32_dll, name)` — `wglGetProcAddress` is the only way to get GL >= 1.2 functions, but `GetProcAddress` is the only way to get GL 1.0–1.1 functions, so both lookups are needed.

## DPI awareness

`DpiFunctions` runtime-loads the entire DPI API surface because it spans Windows Vista (`SetProcessDPIAware`) through Windows 10 1703 (`SetProcessDpiAwarenessContext` with `PER_MONITOR_AWARE_V2`). Each `Option<fn>` is `Some` only on Windows versions where the symbol exists in user32.dll.

The activation order in `Win32Window::new`:

```rust,ignore
if let Some(f) = dpi.set_process_dpi_awareness_context {
    unsafe { f(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2); }
} else if let Some(f) = dpi.set_process_dpi_awareness {
    unsafe { f(ProcessDpiAwareness::PROCESS_PER_MONITOR_DPI_AWARE); }
} else if let Some(f) = dpi.set_process_dpi_aware {
    unsafe { f(); }
}
```

Per-window DPI is queried via `GetDpiForWindow(hwnd)` on each `WM_DPICHANGED` message; the layout viewport is updated and `regenerate_layout` fires.

## Event loop — WindowProc + main loop

The Win32 message dispatch is split:

- **`WindowProc`** is the `extern "system"` function registered with the window class. Win32 calls it for every message *and* re-enters the application during certain APIs (`SetWindowPos`, `MoveWindow`, etc.). Inside `WindowProc` the window pointer is recovered via `GetWindowLongPtrW(hwnd, GWLP_USERDATA)`, and the standard `process_window_events()` runs after updating the relevant fields on `current_window_state`.
- **The main loop** uses `PeekMessageW(PM_REMOVE)` to drain pending messages for each window, then `WaitMessage()` to block when idle (zero CPU when nothing is happening).

Per-message handling lives in `WindowProc`'s `match msg {}` arms:

- **`WM_LBUTTONDOWN` / `WM_RBUTTONDOWN` / `WM_MBUTTONDOWN` (and `_UP`).** Updates `mouse_state.button_down` and fires `process_window_events`.
- **`WM_MOUSEMOVE`.** Updates the cursor position. The first move triggers `TrackMouseEvent(TME_LEAVE)` so `WM_MOUSELEAVE` is delivered later.
- **`WM_MOUSEWHEEL` / `WM_MOUSEHWHEEL`.** Converts to a scroll delta and pushes it to `scroll_manager`.
- **`WM_KEYDOWN` / `WM_KEYUP` / `WM_CHAR`.** Calls `win_event::handle_key` for VK code translation, then `process_window_events`.
- **`WM_SIZE`.** Updates `current_window_state.size` and calls `incremental_relayout`.
- **`WM_DPICHANGED`.** Refreshes per-window DPI and runs the full `regenerate_layout`.
- **`WM_PAINT`.** Calls `render_and_present()` and then `ValidateRect`.
- **`WM_COMMAND` (low word equals menu command ID).** Looks up `CoreMenuCallback` in the menu bar's `command_id → callback` map.
- **`WM_CLOSE`.** Sets `is_open = false` and returns 0 to defeat the default `DestroyWindow`.
- **`WM_IME_*`.** Composition string handling. See below.

`win_event` (adapted from winit, Apache-2.0 licensed) handles the cluster of small Win32 quirks around extended keys, scancode-based disambiguation (left vs right Shift, numeric keypad keys), AltGr emulation on European keyboards, and the dance between `WM_KEYDOWN` (virtual key code) and `WM_CHAR` (translated UTF-16 character).

`request_redraw` calls `InvalidateRect(hwnd, NULL, FALSE)` followed by `UpdateWindow(hwnd)` — Win32 will deliver the resulting `WM_PAINT` before the next `PeekMessage` returns.

## IME — Imm32

The `Imm32` library functions are dlopen-loaded alongside the rest. The flow:

1. `WM_IME_STARTCOMPOSITION` — `ImmGetContext(hwnd)` returns a `HIMC`; `ImmSetCompositionWindow(himc, &CompositionForm { ... })` positions the candidate window at the caret.
2. `WM_IME_COMPOSITION` — preedit string is read with `ImmGetCompositionStringW(himc, GCS_COMPSTR, ...)`. Buffered into `ime_composition: Option<String>` on the window.
3. On commit (`GCS_RESULTSTR`) the result string is fed into `process_text_input`.
4. `WM_IME_ENDCOMPOSITION` — clear `ime_composition`, `ImmReleaseContext(hwnd, himc)`.

UTF-16 surrogate pairs from `WM_CHAR` are reassembled via `high_surrogate: Option<u16>` (set on the high half, consumed on the low half).

## Menus

`menu::WindowsMenuBar` wraps a Win32 `HMENU`:

- `CreateMenu()` for the bar root.
- For each menu item, `AppendMenuW` with either `MF_STRING | command_id` (leaf) or `MF_POPUP | submenu_handle` (submenu).
- The unique command IDs come from the global atomic `WINDOWS_UNIQUE_COMMAND_ID_GENERATOR`; the `command_id → CoreMenuCallback` map is stored on `WindowsMenuBar`.

`set_menu_bar(hwnd, &WindowsMenuBar)` calls `SetMenu(hwnd, hmenu)`. A hash-based diff (`menu.hash`) skips reconstruction when the menu hasn't changed between layouts.

Context menus go through a separate path: `TrackPopupMenu(hmenu, ..., hwnd, NULL)` is called in response to a right-click; the resulting `WM_COMMAND` looks up the callback in `Win32Window.context_menu`.

## Tooltips

`TooltipWindow` creates a tooltip via the Win32 `TOOLTIPS_CLASS` window class — a real native tooltip, not a custom popup. `TTM_ADDTOOL` registers the rect; `TTM_TRACKACTIVATE` shows it; `TTM_TRACKPOSITION` moves it on cursor motion. Visuals match the user's Windows theme automatically.

## Multi-window registry

The Windows registry is a thread-local `BTreeMap<HWND, *mut Win32Window>`. The pattern matches the Linux and macOS registries documented on those pages — `register_window`, `unregister_window`, `get_window`, `get_all_window_handles`. The pointer is leaked via `Box::into_raw` from the run loop; `unregister_window` returns the pointer for `Box::from_raw` cleanup.

The main loop iterates `get_all_window_handles()` each tick. Multi-window fan-out for popup menus and dialogs flows through `Win32Window.pending_window_creates`, drained by the main loop after event processing.

## Clipboard

The Windows clipboard module uses the `clipboard-win` crate. `set_clipboard(formats::Unicode, &text)` on copy; `get_clipboard::<String, _>(formats::Unicode)` on paste. The manager's `clear()` is only called on successful write so a transient clipboard-busy error doesn't drop the user's selection.

## Accessibility

The Windows accessibility module uses `accesskit_windows::SubclassingAdapter` when the `a11y` feature is on. The adapter window-procs the HWND to intercept `WM_GETOBJECT(OBJID_CLIENT)` and present the AzulRoot accessibility tree to UIA. `accesskit_windows::SubclassingAdapter::new` is wrapped in `catch_unwind` for the same reasons as the Linux variant — some COM/UIA initialisation failures panic.

## CPU rendering path

When `RenderMode::Cpu` is active, `render_and_present` rasterises through `cpurender` into `retained_pixmap: AzulPixmap`, converts to BGRA into the cached `bgra_buffer: Vec<u8>`, and uses `StretchDIBits` to blit onto the window's `HDC`. The `glyph_cache` is held on the window so successive frames reuse rasterised glyphs.

`gpu_damage_rects: Vec<LogicalRect>` is also tracked in GPU mode — when non-empty, only the listed regions need painting; the WebRender transaction sets the same rects so the GPU compositor can skip unchanged tiles.

## system_style and dynamic theming

`SystemStyle::detect_windows` reads from the registry (`HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`) plus `DwmGetColorizationColor` to populate the `azul_css::system::SystemStyle` struct: dark/light theme, accent colour, window background, system fonts. The result is plumbed into `current_window_state.system_style` and influences both `:dark` / `:light` CSS pseudo-classes and any `system-ui` font references.

Theme changes arrive via `WM_SETTINGCHANGE` with `lParam == "ImmersiveColorSet"` — the message handler re-runs detection and triggers a full `regenerate_layout` so theme-conditional CSS re-evaluates.

## Known issues / TODOs

- The `system_style` reader reads only the `Personalize` keys; some accent variants (e.g., contrast themes) need additional handling.
- `WM_NCHITTEST` for custom titlebar regions is not yet wired — client-side decoration drag handles need this to make windows draggable from non-titlebar regions.
- `WM_INPUT` for raw mouse / pen / touch is not yet supported; only `WM_MOUSE*` and `WM_TOUCH` are processed.
- The CPU path's `bgra_buffer` is cached but not damage-clipped — it always rebuilds the full BGRA from the pixmap each frame.

## Coming Up Next

- [Common](common.md) — Shared shell infrastructure across platforms
- [macOS](macos.md) — macOS shell - Cocoa, AppKit, IME, a11y
- [Menus and CSD](menus-and-csd.md) — Menus and client-side decorations across platforms
- [Windowing Overview](../windowing.md) — Per-window aggregate, headless variant, and the platform shell layer
