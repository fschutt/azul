# Review: dll/src/desktop/shell2/linux/x11/mod.rs

## Summary
- Lines: 3251
- Public functions: 15
- Public structs/enums: 2 (X11Window, X11Event)
- Findings: 1 high, 5 medium, 3 low

## Findings

### [HIGH] Dead Code — `get_window_display_info` never called
- **Location**: `mod.rs:2540-2598`
- **Details**: Defined on all 4 platforms but never called anywhere in the codebase.
- **Evidence**: `grep 'get_window_display_info()' dll/src` returns zero results (only definitions).
- **Recommendation**: Remove or wire into the platform abstraction.

### [HIGH] Unsafe — XSetICValues argument nesting likely incorrect
- **Location**: `mod.rs:2872-2884`
- **Details**: The XIM spec requires `XNPreeditAttributes` to wrap a nested attribute list (with its own null terminator containing `XNSpotLocation`). Current code passes flat arguments: `[preedit_attr_name, spot_location_name, spot_ptr, null]` instead of `[XNPreeditAttributes, XVaCreateNestedList(XNSpotLocation, &spot, NULL), NULL]`. This may silently fail on strict XIM servers.
- **Recommendation**: Use `XVaCreateNestedList` (if available via dlopen) or verify the flat-argument form works with the target XIM implementations.

### [MEDIUM] Missing Docs — X11Window struct fields
- **Location**: `mod.rs:254-322`
- **Details**: `X11Window` has 30+ fields, many public. Only a handful have doc comments. Fields like `xlib`, `egl`, `xkb`, `display`, `window`, `is_open` lack documentation.
- **Recommendation**: Add brief doc comments to public fields.

### [MEDIUM] Code Style — `render_and_present` is 393 lines
- **Location**: `mod.rs:1567-1960`
- **Details**: This function handles CPU rendering (with incremental damage), GPU rendering (with lightweight transactions, virtual view updates, WebRender render/swap), CI exit, and scrollbar fade scheduling. Well over the 60-100 LOC target.
- **Recommendation**: Extract CPU rendering path (~L1580-1782) into `render_cpu()` and GPU path (~L1784-1960) into `render_gpu()`.

### [MEDIUM] Code Style — `new_with_resources` is 510 lines
- **Location**: `mod.rs:610-1121`
- **Details**: Contains GL context setup, WebRender initialization, window struct construction, GNOME menu setup, registry registration, callback invocation, and initial state application.
- **Recommendation**: Extract sub-steps: `init_gl_and_renderer()`, `init_gnome_menus()`, `invoke_create_callback()`.

### [MEDIUM] Stub — TODO comments indicate unfinished work
- **Location**: `mod.rs:705`, `mod.rs:2709`
- **Details**:
  - Line 705: `let monitor_id = 0; // TODO: Get from options or detect primary monitor`
  - Line 2709: `// TODO: Show GNOME native menu via DBus`
- **Recommendation**: Implement or track in issue tracker.

### [MEDIUM] Unsafe — `std::process::exit(0)` called 3 times, bypassing Drop
- **Location**: `mod.rs:539`, `mod.rs:1778`, `mod.rs:1956`
- **Details**: All guarded by `AZUL_EXIT_SUCCESS_AFTER_FRAME_RENDER` env var (CI use). `std::process::exit` skips all destructors, leaking X11 display, GL contexts, and file descriptors. Even in CI, this is bad practice.
- **Recommendation**: Use a return-based exit path or set a flag that the event loop checks.

### [LOW] Magic Number — hardcoded work area panel height
- **Location**: `mod.rs:2579`
- **Details**: `(height_px - 24).max(0)` uses `24` as assumed panel height. Should query `_NET_WORKAREA` atom.
- **Recommendation**: Query `_NET_WORKAREA` or use full screen size as work area.

### [LOW] Unsafe — XRandR library intentionally leaked
- **Location**: `mod.rs:136`
- **Details**: `std::mem::forget(xrandr_lib)` prevents the library from being unloaded so function pointers remain valid. Correct approach but leaks memory on each call.
- **Recommendation**: Use `static OnceLock<Library>` or `Box::leak` to make the intent explicit and prevent multiple leaks.

### [LOW] Inefficiency — DBusLib loaded twice in `set_prevent_system_sleep`
- **Location**: `mod.rs:3018` (inhibit path) and `mod.rs:3176` (uninhibit path)
- **Details**: Each call dynamically loads `libdbus` anew. Not a leak (drops on scope exit) but wasteful.
- **Recommendation**: Cache `DBusLib` as a field or use `OnceLock`.

## System Documentation
- System identified: yes - Windowing / Platform Shell (X11 backend)
- Existing doc: none (no windowing guide in `doc/guide/`)
- Doc needed: A `doc/guide/windowing.md` covering the platform abstraction layer (`PlatformWindow` trait, `CommonWindowState`, per-platform backends), event loop architecture, and rendering pipeline. Multiple files (X11, Wayland, macOS, Windows) belong to this system.
