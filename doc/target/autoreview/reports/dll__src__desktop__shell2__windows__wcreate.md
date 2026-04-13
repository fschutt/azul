# Review: dll/src/desktop/shell2/windows/wcreate.rs

## Summary
- Lines: 592
- Public functions: 4 (`register_window_class`, `create_hwnd`, `create_gl_context`, `get_client_rect`, `set_window_size`)
- Public structs/enums: 0
- Public constants: 1 (`CLASS_NAME`)
- Findings: 0 high, 3 medium, 1 low

## Findings

### [MEDIUM] Inconsistent FFI approach — mixes custom dlopen wrappers with direct `winapi` crate calls
- **Location**: `wcreate.rs:269`, `wcreate.rs:365`, `wcreate.rs:397`, `wcreate.rs:424-425`
- **Details**: `create_gl_context` uses the custom `Win32Libraries` dlopen wrappers for some functions (`GetDC`, `ReleaseDC`) but calls `winapi` crate functions directly for others (`SetPixelFormat`, `DescribePixelFormat`, `wglCreateContext`, `wglMakeCurrent`, `GetProcAddress`). This dual approach is inconsistent — if the goal of the dlopen wrappers is to avoid static linking, then using `winapi` directly defeats that purpose for those calls.
- **Recommendation**: Either add the missing functions to the dlopen wrapper set, or document why some functions use `winapi` directly (e.g., they are always available via opengl32.dll which is already loaded).

### [MEDIUM] Excessive trace logging bloats `create_gl_context` to 400+ lines
- **Location**: `wcreate.rs:137-537`
- **Details**: `create_gl_context` is ~400 lines, but roughly half of that is `log_trace!` calls. The function has approximately 30 trace-level log statements, many of which are redundant (e.g., logging before and after every single function call). This makes the actual logic hard to follow.
- **Recommendation**: Reduce logging to key decision points (context creation attempts, fallbacks, errors). The before/after pattern for every FFI call is excessive for trace-level logging.

### [MEDIUM] Refactoring — `create_gl_context` mixes pixel format, context creation, and GL info logging
- **Location**: `wcreate.rs:137-537`
- **Details**: This 400-line function handles: (1) loading WGL extensions, (2) choosing pixel format, (3) setting pixel format, (4) creating GL context with 3-level fallback, (5) making context current, (6) querying and logging GL vendor/renderer/version info, (7) setting vsync. These are logically separate concerns.
- **Recommendation**: Extract at least the GL info logging block (lines 422-499) into a separate helper like `log_gl_info(opengl32: HMODULE)`, and consider extracting pixel format selection into its own function.

### [LOW] Magic numbers for OpenGL constants
- **Location**: `wcreate.rs:438-439` (`GL_VENDOR = 0x1F00`, etc.), `wcreate.rs:470` (`GL_MAX_TEXTURE_SIZE = 0x0D33`)
- **Details**: OpenGL enum constants are defined inline as local `const` inside the `#[cfg(target_os = "windows")]` block. They are correct values but could be imported from a shared GL constants module if one exists.
- **Recommendation**: Minor — these are standard GL enums and the local const approach is acceptable for a diagnostic logging block.

## System Documentation
- System identified: **Windows windowing / Win32 shell** (part of `shell2/windows/`)
- Existing doc: none (no `doc/guide/windowing.md` or similar)
- Doc needed: A guide document covering the windowing system (`shell2/`) — how windows are created across platforms, the GL context initialization strategy, the dlopen approach for Win32 API loading, and the event loop integration. Many files in `shell2/` share this gap.
