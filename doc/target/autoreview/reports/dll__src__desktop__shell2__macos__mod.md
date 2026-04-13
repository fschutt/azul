# Review: dll/src/desktop/shell2/macos/mod.rs

## Summary
- Lines: 6490
- Public functions: ~40
- Public structs/enums: 7 (RenderBackend, GLViewIvars, GLView, CPUViewIvars, CPUView, MacOSWindow, MacOSEvent)
- Findings: 2 high, 5 medium, 1 low

## Findings

### [HIGH] Massive code duplication — GLView and CPUView are ~95% identical

- **Location**: GLView (lines 142-905) vs CPUView (lines 927-1708)
- **Details**: GLView and CPUView duplicate nearly every event handler method identically: `mouse_down`, `mouse_up`, `mouse_dragged`, `right_mouse_down`, `right_mouse_up`, `scroll_wheel`, `key_down`, `key_up`, `flags_changed`, `undo`, `redo`, `validate_user_interface_item`, `tick_timers`, `update_tracking_areas`, `mouse_entered`, `mouse_exited`, `mouse_moved`, plus all 10 NSTextInputClient protocol methods. The only difference is CPUView's `draw_rect` does CPU blitting and CPUView has `cached_bitmap` ivars.
- **Evidence**: ~780 lines of event handling code are copy-pasted between the two classes. The `insert_text` bug (finding #1) was caused by this duplication diverging.
- **Recommendation**: Extract the shared event handling into free functions or a helper module. The `define_class!` macro requires methods to be defined inside the macro, but each method body can delegate to a shared function (e.g. `fn handle_view_mouse_down(window_ptr, event, button) -> ...`).

### [HIGH] Unwrap cluster in GPU render path — panics if called before init

- **Location**: `mod.rs:3644-3652` and `mod.rs:5486-5569`
- **Details**: `generate_frame_if_needed()` and the GPU section of `render_and_present_in_draw_rect()` contain ~9 `.unwrap()` calls on `self.common.render_api` and `self.common.document_id`. These are `Option`s that are `None` in CPU mode. If the GPU code path is accidentally reached in CPU mode, or called before initialization completes, the window panics.
- **Evidence**: Lines 3644, 3645, 3651, 3652, 5486, 5501, 5519, 5520, 5569.
- **Recommendation**: Either guard the entire GPU block with `if self.backend == RenderBackend::OpenGL` (which it partially does at line 5156 but not for `generate_frame_if_needed`), or use `ok_or/ok_or_else` with proper error returns.

### [MEDIUM] Dead code — remaining public methods with zero external callers

- **Location**: Various
- **Details**: The following `pub` methods have zero call sites outside the `macos/` module:
  - `update_window_state`
  - `hide_cursor`
  - `show_cursor`
  - `reset_cursor`
  - `generate_frame_if_needed`
  - `RenderBackend` enum — `pub` but only used within `macos/`
- **Evidence**: Grep for each name outside `macos/` returned zero matches (verified by agent search).
- **Recommendation**: Reduce visibility to `pub(crate)` or `pub(super)` where appropriate.

### [MEDIUM] Missing module-level documentation completeness

- **Location**: `mod.rs:1-9`
- **Details**: The file has a `//!` module doc block (lines 1-9) that covers the high-level purpose. However, it doesn't mention key types (MacOSWindow, GLView, CPUView, WindowDelegate) or how the module fits into the shell2 architecture (called by `run.rs`, implements `PlatformWindow` trait).
- **Recommendation**: Expand the `//!` block to mention `MacOSWindow` as the main entry point, the dual GLView/CPUView rendering backends, and the `PlatformWindow` trait implementation.

### [MEDIUM] File size — 6490 lines with mixed concerns

- **Location**: Entire file
- **Details**: At 6490 lines the file is very large. The duplication between GLView and CPUView accounts for ~780 lines. The rendering pipeline (`render_and_present_in_draw_rect`, ~520 lines), window state sync (~170 lines), undo/redo (~150 lines), and accessibility (~150 lines) are reasonably cohesive with the window struct. The file is not egregiously mixing unrelated concerns — it's a platform window implementation — but the GLView/CPUView event forwarding boilerplate inflates it significantly.
- **Recommendation**: Extracting shared event handler logic (see finding #4) would remove ~700 lines and bring the file closer to 5800 lines — still large but more manageable.

### [MEDIUM] OpenGL pixel format attributes are bare integer literals

- **Location**: `mod.rs:2157-2165`
- **Details**: The `create_opengl_pixel_format` function uses raw integers (`5, 12, 24, 99, 0x3200, 8, 24, 11, 8, 73, 0`) for NSOpenGLPixelFormat attributes. While comments explain each, these should be named constants for maintainability.
- **Evidence**: Lines 2157-2165 contain comments like `// NSOpenGLPFADoubleBuffer`, `// NSOpenGLPFADepthSize(24)`, etc.
- **Recommendation**: Define named constants (e.g. `const NSGL_PFA_DOUBLE_BUFFER: u32 = 5;`) or reference objc2-app-kit's constants if available.

### [MEDIUM] Unresolved TODOs

- **Location**: Lines 2564, 2978, 3326, 6486
- **Details**:
  - Line 2564: `// TODO: Re-enable once objc2-open-gl feature is properly configured` — VSync via `setValues:forParameter:` disabled, entire function `configure_vsync` is a no-op.
  - Line 2978: `// TODO: Implement proper multi-monitor positioning after event loop starts`
  - Line 3326: `// TODO: build initial menu state from layout_window`
  - Line 6486: `// TODO: Could call invalidateMarkable or similar if needed`
- **Recommendation**: Triage — the VSync TODO (2564) means `configure_vsync()` does nothing, which is a functional gap even though CVDisplayLink provides a workaround.

### [MEDIUM] `configure_vsync` is a no-op function

- **Location**: `mod.rs:2561-2589`
- **Details**: The function computes `swap_interval` from the `Vsync` enum but never applies it — the actual `setValues:forParameter:` call is commented out (line 2564). The function only logs. This is called during window creation (line 2861) and in the display link fallback path (line 2673).
- **Evidence**: The function body after computing `swap_interval` only calls `log_debug!`.
- **Recommendation**: Either implement the call (fixing the objc2 type encoding issue) or remove the function and its call sites, since CVDisplayLink is the actual mechanism used.

### [LOW] `present()` duplicates buffer swap from `render_and_present_in_draw_rect`

- **Location**: `mod.rs:5780-5799` and `mod.rs:5599-5606`
- **Details**: `present()` calls `flushBuffer` via `msg_send!`, while `render_and_present_in_draw_rect` already calls `gl_context.flushBuffer()` (line 5603). If both are called in the same frame, the buffer is flushed twice.
- **Recommendation**: Clarify the calling convention — either `present()` is the public API and `render_and_present_in_draw_rect` shouldn't flush, or vice versa.

## System Documentation
- System identified: yes — macOS windowing/platform shell (part of the `shell2` cross-platform windowing system)
- Existing doc: `doc/guide/architecture.md` (general architecture); no macOS-specific windowing guide
- Doc needed: A `doc/guide/windowing.md` guide covering the shell2 platform abstraction layer, the `PlatformWindow` trait, and the macOS/Windows/Linux implementations. This would also document the dual GL/CPU rendering backends and the event dispatch flow.
