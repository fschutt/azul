# Review: dll/src/desktop/shell2/windows/mod.rs

## Summary
- Lines: 4080
- Public functions: 26
- Public structs/enums: 2 (Win32Window, Win32Event)
- Findings: 0 high, 4 medium, 3 low

## Findings

### [MEDIUM] Dead Code — get_window_display_info, get_window_rect, get_window_dpi only used internally

- **Location**: `mod.rs:3590` (`get_window_rect`), `mod.rs:3600` (`get_window_dpi`), `mod.rs:3605` (`get_window_display_info`)
- **Details**: `get_window_rect` and `get_window_dpi` are only called by `get_window_display_info` (lines 3608, 3632, 3661). `get_window_display_info` itself has no external callers — only its definition exists in each platform module.
- **Evidence**: Grep shows definitions in all 4 platform mods but no call sites.
- **Recommendation**: Either wire these into the public API or make them private / remove them.

### [MEDIUM] Refactoring — window_proc is ~1400 LOC

- **Location**: `mod.rs:1957-3368`
- **Details**: The `window_proc` function spans ~1400 lines. While structured as a match on message types, the individual arms (especially WM_MOUSEMOVE ~90 LOC, WM_LBUTTONDOWN ~70 LOC, WM_MOUSEWHEEL ~100 LOC, WM_COMMAND ~100 LOC) contain substantial duplicated patterns for hit-testing, state saving, and redraw requests.
- **Recommendation**: Extract repeated patterns into helper methods: `save_previous_state()`, `update_hit_test(logical_pos)`, `invalidate_if_needed(result)`. Extract large arms into named handler functions like `handle_wm_size()`, `handle_wm_mousewheel()`, etc.

### [MEDIUM] Refactoring — render_and_present is ~400 LOC

- **Location**: `mod.rs:720-1121`
- **Details**: `render_and_present` mixes CPU rendering (~200 LOC) and GPU rendering (~200 LOC) in a single function. The CPU path (lines 722-920) and GPU path (lines 922-1121) share almost no code except the final scrollbar-fade and CI-exit checks.
- **Recommendation**: Extract `render_and_present_cpu()` and `render_and_present_gpu()` as separate methods.

### [MEDIUM] Refactoring — Win32Window::new is ~470 LOC

- **Location**: `mod.rs:181-666`
- **Details**: The constructor handles library loading, DPI init, window creation, GL context setup, WebRender init, layout window creation, accessibility init, material application, first frame render, and callback invocation. Each of these could be a named setup step.
- **Recommendation**: Extract initialization phases into helper methods for readability.

### [LOW] TODO comments — 3 unfinished items

- **Location**: `mod.rs:432`, `mod.rs:436`, `mod.rs:462`
- **Details**:
  - Line 432: `// TODO: Menu bar needs to be extracted from window state` — menu_bar always set to None
  - Line 436: `// TODO: size_to_content needs to be implemented with new layout API` — commented-out code block
  - Line 462: `// TODO: Use monitor_id to look up actual Monitor from global state` — uses `Monitor::default()` instead
- **Recommendation**: Implement or remove the commented-out code. The line-462 TODO may cause windows to always appear on the primary monitor regardless of monitor_id.

### [LOW] Missing Documentation — several public methods lack doc comments

- **Location**: `mod.rs:458` (`poll_event`), `mod.rs:469` (`present`), `mod.rs:476` (`is_open`), `mod.rs:480` (`close`)
- **Details**: The lifecycle methods block (lines 3457-3512) has minimal or no doc comments on individual methods.
- **Recommendation**: Add brief doc comments explaining the public API contract.

### [LOW] Compiler Warnings — `get_default_pfd` only compiles on Windows

- **Location**: `mod.rs:3399-3430` (`get_default_pfd`)
- **Details**: The function uses `winapi` types directly without `#[cfg(target_os = "windows")]` guards. It will fail to compile on non-Windows platforms (the rest of the file uses dynamic loading via `dlopen` to support cross-compilation).
- **Recommendation**: Add `#[cfg(target_os = "windows")]` guard (it is called from gl.rs via `super::get_default_pfd()`).

## System Documentation
- System identified: yes — Windows windowing/platform shell (part of the `shell2` windowing system)
- Existing doc: none (no windowing guide in doc/guide/)
- Doc needed: A `doc/guide/windowing.md` guide explaining the shell2 architecture, platform window trait, event loop, and how the Windows/macOS/Linux/Wayland backends integrate. The `scripts/PLATFORM_WINDOW_REFACTORING.md` planning document provides useful architectural context.
