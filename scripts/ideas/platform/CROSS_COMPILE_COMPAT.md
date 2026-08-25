# Azul Cross-Compilation & Retro OS Compatibility Report

Generated: 2026-04-11

## Executive Summary

Azul's desktop backends already use **dynamic loading (dlopen/LoadLibrary)** for
nearly all platform APIs, which is exactly what you need for old-OS support. The
codebase is close to running on very old systems, but there are some gaps.

**Current realistic minimum versions:**
- **Windows**: Windows XP SP2 (with rust9x toolchain) — all Win32 APIs are dlopen'd
- **macOS**: 10.10 Yosemite — but `NSVisualEffectView` use needs soft fallback
- **Linux**: Any with X11 (Xlib) — all libraries loaded via dlopen

---

## Windows Backend Analysis

### Dynamic Loading Architecture

All Win32 functions are loaded via `LoadLibraryW` + `GetProcAddress` in
`dll/src/desktop/shell2/windows/dlopen.rs`. **No static linking** to any Win32 DLL.

| DLL | Required? | Functions | Min Windows |
|-----|-----------|-----------|-------------|
| `user32.dll` | **Yes** (hard fail) | CreateWindowExW, ShowWindow, GetMessageW, DispatchMessageW, SetWindowPos, RegisterClassW, DefWindowProcW, etc. | **Windows 95** |
| `gdi32.dll` | **Yes** (hard fail) | CreateSolidBrush, DeleteObject, StretchDIBits, CreateRectRgn | **Windows 95** |
| `kernel32.dll` | Optional (`Option`) | SetThreadExecutionState, GetModuleHandleW | **Windows 98** (SetThreadExecutionState) |
| `shell32.dll` | Optional (`Option`) | DragAcceptFiles, DragQueryFileW, DragFinish | **Windows 95** |
| `imm32.dll` | Optional (`Option`) | ImmGetContext, ImmGetCompositionStringW, ImmSetCompositionWindow | **Windows 95** (IME) |
| `dwmapi.dll` | Optional (`Option`) | DwmSetWindowAttribute, DwmExtendFrameIntoClientArea, DwmEnableBlurBehindWindow, DwmFlush | **Windows Vista** |
| `opengl32.dll` | Optional | GL functions via wglGetProcAddress | **Windows 95** (with drivers) |

### DPI Functions (dpi.rs) — Excellent Fallback Chain

The DPI code already has a **perfect 3-tier fallback** from newest to oldest:

```
SetProcessDpiAwarenessContext   → Win10 1607+
  ↓ fallback
SetProcessDpiAwareness          → Win8.1+
  ↓ fallback
SetProcessDPIAware              → Vista+
  ↓ fallback
(no DPI awareness)              → Win95+
```

Same pattern for `hwnd_dpi()`: `GetDpiForWindow` → `GetDpiForMonitor` → `GetDeviceCaps`.

### DWM (dwmapi.dll) — Clean Optional

DWM functions (Mica, Acrylic, blur-behind) are all `Option<DwmapiFunctions>`.
On pre-Vista systems, `dwmapi.dll` simply won't load and these features are
silently unavailable. **No code changes needed.**

### Windows 98/XP via rust9x

Since the codebase:
- Does **NOT** use `ring` (pure-Rust TLS via `rustls` + `rustls-rustcrypto`)
- Does **NOT** have any `build.rs` that compiles C code
- Loads all Win32 APIs via `LoadLibraryW`/`GetProcAddress`
- Has cross-compilation stubs for non-Windows hosts

It should compile with [rust9x](https://github.com/rust9x/rust/) targeting
`i586-pc-windows-msvc` (Win9x) or `i686-pc-windows-msvc` (WinXP+).

**Potential issues for Win98/XP:**
1. `SetWindowLongPtrW` / `GetWindowLongPtrW` — these are **64-bit aware** versions
   that don't exist on 32-bit Win9x. On 32-bit, they're `#define`s to
   `SetWindowLongW`/`GetWindowLongW`. Since we dlopen by name, we'd need to fall
   back to the non-Ptr versions on 32-bit. **Needs a soft fallback.**
2. `CreateWindowExW` (Unicode) — exists since Win95, but Win9x Unicode support
   is via `unicows.dll`. With rust9x this is handled.
3. `TrackMouseEvent` — Windows 98+, not in Win95. Currently required. **Needs soft fallback for Win95.**

### Verdict: Windows

| Target | Status | Notes |
|--------|--------|-------|
| Windows 11 | Works | Full DWM/Mica/Acrylic |
| Windows 10 | Works | Full DPI awareness v2 |
| Windows 8.1 | Works | Per-monitor DPI via fallback |
| Windows Vista/7 | Works | DWM blur, basic DPI |
| Windows XP | **Nearly works** | Need `SetWindowLongW` fallback for 32-bit |
| Windows 98 | **Nearly works** | Need rust9x, `TrackMouseEvent` fallback |

---

## macOS Backend Analysis

### Dynamic Loading Architecture

| Library | Loading Method | Functions | Min macOS |
|---------|---------------|-----------|-----------|
| `OpenGL.framework` | `dlopen` + `dlsym` | All GL entry points (gl.rs) | **10.0** |
| `ApplicationServices.framework` | `libloading::Library` | CGMainDisplayID, CGDisplayBounds | **10.0** |
| `CoreVideo.framework` | `libloading::Library` | CVDisplayLink* (VSYNC) | **10.3** |
| `AppKit` (via objc2) | ObjC runtime (`msg_send!`) | NSWindow, NSView, NSEvent, etc. | **10.0** |

### ObjC API Version Requirements

| API | Min macOS | Used In | Has Fallback? |
|-----|-----------|---------|---------------|
| `NSWindow`, `NSView`, `NSEvent` | 10.0 | mod.rs | N/A (core) |
| `NSPanel`, `NSTextField` | 10.0 | tooltip.rs | N/A (core) |
| `NSMenu`, `NSMenuItem` | 10.0 | menu.rs | N/A (core) |
| `NSPasteboard` (clipboard) | 10.0 | clipboard.rs | N/A (core) |
| `NSOpenGLView`, `NSOpenGLContext` | 10.0 | mod.rs | N/A (core) |
| `NSOpenGLPixelFormat` | 10.0 | mod.rs | N/A (core) |
| `NSTrackingArea` | **10.5** | mod.rs | **No** — replaces old `addTrackingRect:` |
| `NSColor colorWithRed:green:blue:alpha:` | 10.0 | tooltip.rs | N/A |
| `NSScreen mainScreen` | 10.0 | tooltip.rs | N/A |
| **`NSVisualEffectView`** | **10.10** | mod.rs:4361 | **No fallback** — used for Mica/blur |
| `NSVisualEffectMaterial::Sidebar` | **10.11** | mod.rs:4408 | **No fallback** |
| `NSVisualEffectMaterial::HUDWindow` | **10.11** | mod.rs:4410 | **No fallback** |
| `NSWindowTitleVisibility` | **10.10** | mod.rs | Check usage |
| `NSEventPhase` (scroll momentum) | **10.7** | events.rs:329 | **No fallback** |
| `CVDisplayLink` | **10.3** | corevideo.rs | Returns `Err` if unavailable |
| `accesskit_macos` | **10.10+** | accessibility.rs | Feature-gated (`a11y`) |

### What Needs Soft Fallbacks for Old macOS

1. **`NSVisualEffectView`** (10.10+) — Currently used unconditionally for blur/Mica
   materials. On <10.10, `NSVisualEffectView::class()` returns nil, so the
   `isKindOfClass:` check would return false, but creating a new one would crash.
   **Needs:** Check `NSClassFromString("NSVisualEffectView")` before allocating.

2. **`NSTrackingArea`** (10.5+) — Used for mouse tracking. Pre-10.5 used
   `addTrackingRect:owner:userData:assumeInside:`. Realistically 10.5 is fine
   as a floor since Leopard is the oldest Intel macOS.

3. **`NSEventPhase`** (10.7+) — Used for scroll momentum tracking. Pre-10.7
   systems don't have momentum scrolling, so this should return 0/None. Check
   `respondsToSelector:`.

### Verdict: macOS

| Target | Status | Notes |
|--------|--------|-------|
| macOS 14+ (Sonoma) | Works | Full NSVisualEffect |
| macOS 10.10-13 | Works | NSVisualEffect available |
| macOS 10.7-10.9 | **Nearly works** | Need NSVisualEffectView nil-check |
| macOS 10.5-10.6 | **Nearly works** | + NSEventPhase fallback |
| macOS 10.4 (Tiger) | Unlikely | Last PPC-only, no NSTrackingArea |

**Realistic floor: macOS 10.7 (Lion)** with 1-2 soft fallbacks.
**Aspirational floor: macOS 10.5 (Leopard)** — oldest Intel macOS.

---

## Linux Backend Analysis

### Dynamic Loading Architecture

**Everything** is dlopen'd. No static linking to any system library.

| Library | Loading Method | Fallback? | Min Kernel/Distro |
|---------|---------------|-----------|-------------------|
| `libX11.so.6` / `libX11.so` | `libc::dlopen` | Tries both names | Any X11 system |
| `libEGL.so.1` / `libEGL.so` | `libc::dlopen` | Tries both names | Mesa 8+ / any EGL |
| `libxkbcommon.so.0` | `libc::dlopen` | Required for keyboard | Most modern distros |
| `libXrender.so.1` | `libc::dlopen` | **Optional** (`Option<Rc>`) | X11 + Xrender ext |
| `libgtk-3.so.0` | `libc::dlopen` | **Optional** | GTK3 for IME only |
| `libwayland-client.so` | `libc::dlopen` | X11 fallback | Wayland compositors |
| `libwayland-egl.so` | `libc::dlopen` | X11 fallback | Wayland compositors |
| `libwayland-cursor.so` | `libc::dlopen` | **Optional** (`Option`) | Wayland compositors |
| `libdbus-1.so` | `libc::dlopen` | **Optional** | GNOME menu integration |

### X11 Functions Used

All Xlib functions loaded are from the **original X11R6** API (1994):
- `XOpenDisplay`, `XCreateWindow`, `XMapWindow`, `XNextEvent`, etc.
- `XCreateGC`, `XFillRectangle`, `XPutImage` (CPU rendering)
- `XOpenIM`, `XCreateIC` (input methods) — X11R6
- `XCreateColormap`, `XMatchVisualInfo` (ARGB visuals)

**No Xlib extensions required** except:
- XRender (`libXrender.so.1`) — **optional**, used only for ARGB visual detection
- xkbcommon — for modern keyboard handling (Wayland/X11)

### Wayland Functions Used

Standard `wl_*` and `xdg_*` protocol functions. Wayland is inherently modern
(2012+), so no retro concern here. The key point is that Wayland is **optional** —
the code falls back to X11.

### Verdict: Linux

| Target | Status | Notes |
|--------|--------|-------|
| Modern distros (2020+) | Works | Wayland or X11 |
| Older distros (2010-2019) | Works | X11, may lack Wayland |
| Very old (2005-2010) | **Works** | X11, may lack xkbcommon/EGL |
| Ancient (pre-2005) | **Possible** | X11 core only, need SW rendering fallback |

**Realistic floor: Any Linux with X11** (essentially kernel 2.6+).

---

## Cross-Cutting Concerns

### Pointer Size (32-bit)

Key files to watch for 32-bit issues:

| Pattern | Risk | Location |
|---------|------|----------|
| `WPARAM = usize` / `LPARAM = isize` | **OK** — already `usize`/`isize` | windows/dlopen.rs |
| `*mut c_void` as window ID | **OK** — pointer-sized | All backends |
| `WindowId::as_i64()` → `ptr as usize as i64` | **OK** on 32-bit (usize fits i64) | macos/registry.rs |
| `SetWindowLongPtrW` | **FIXED** — falls back to `SetWindowLongW` | windows/dlopen.rs |

### No C Dependencies in Build Chain

Verified: No `cc` crate, no `bindgen`, no `build.rs` that compiles C.
The only `build.rs` (in `dll/`) just checks for pre-generated files and
configures dylib search paths.

### Pure-Rust TLS

HTTP feature uses `rustls` + `rustls-rustcrypto` (NOT `ring` or `aws-lc-rs`).
This means no assembly code, no C compiler needed — critical for cross-compilation
to exotic targets.

---

## Recommended Actions

### Quick Wins (no code changes, just CI)

- [x] Added cross-compilation `cargo check` CI job for 32-bit, ARM, RISC-V, etc.
- [x] Added `MACOSX_DEPLOYMENT_TARGET=10.13` / `10.15` / `11.0` CI checks
- [x] Created `scripts/cross_check.sh` for local testing

### Soft Fallbacks Implemented

1. **Windows: `SetWindowLongPtrW` → `SetWindowLongW`** on 32-bit -- **DONE**
   - In `dlopen.rs`, tries `SetWindowLongPtrW` first, falls back to `SetWindowLongW`
   - Same for `GetWindowLongPtrW` → `GetWindowLongW`

2. **macOS: `NSVisualEffectView` nil-check** (mod.rs) -- **DONE**
   - `has_visual_effect_view()` checks `AnyClass::get("NSVisualEffectView")`
   - Falls back to transparent background on pre-10.10

3. **macOS: `NSEventPhase` guard** (events.rs) -- **DONE**
   - Uses `respondsToSelector:` before calling `scrollingDeltaX`/`Y`
   - Falls back to legacy `deltaX`/`deltaY` on pre-10.7

### Nice-to-Have for Win98 (rust9x)

4. **`TrackMouseEvent` soft fallback** — Load via `GetProcAddress`, use `Option`
   like `imm32` / `shell32`. Pre-Win98 systems don't track mouse leave events.

5. **Add `i586-pc-windows-msvc` (Win9x) to cross_check.sh** — requires rust9x
   toolchain, so it's opt-in.

---

## API Introduction Dates Reference

### Windows

| API | Introduced |
|-----|-----------|
| CreateWindowExW, RegisterClassW | Windows 95 |
| SetWindowLongW/GetWindowLongW | Windows 95 |
| SetWindowLongPtrW/GetWindowLongPtrW | Windows XP (64-bit aware) |
| TrackMouseEvent | Windows 98 / IE 4.0 |
| DragAcceptFiles, Shell32 drag-drop | Windows 95 |
| ImmGetContext (IME) | Windows 95 |
| SetProcessDPIAware | Windows Vista |
| DwmEnableBlurBehindWindow | Windows Vista |
| SetProcessDpiAwareness | Windows 8.1 |
| GetDpiForWindow | Windows 10 1607 |
| SetProcessDpiAwarenessContext | Windows 10 1607 |
| DwmSetWindowAttribute (DWMWA_SYSTEMBACKDROP_TYPE) | Windows 11 22H2 |

### macOS

| API | Introduced |
|-----|-----------|
| NSWindow, NSView, NSEvent, NSMenu | 10.0 (2001) |
| NSOpenGLView, NSOpenGLContext | 10.0 (2001) |
| NSPasteboard | 10.0 (2001) |
| CVDisplayLink | 10.3 (2003) |
| NSTrackingArea | 10.5 (2007) |
| NSEventPhase (scroll momentum) | 10.7 (2011) |
| NSVisualEffectView | 10.10 (2014) |
| NSVisualEffectMaterial::Sidebar/HUDWindow | 10.11 (2015) |
| NSAppearance (dark mode) | 10.14 (2018) |

### Linux

| Library | Introduced |
|---------|-----------|
| Xlib (libX11) | 1987 (X11R1) |
| XRender | 2000 (X11R6.4) |
| EGL (libEGL) | 2004 (EGL 1.0) |
| libxkbcommon | 2012 |
| Wayland | 2012 |
| libdbus-1 | 2003 |
