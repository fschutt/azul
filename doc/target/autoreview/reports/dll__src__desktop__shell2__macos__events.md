# Review: dll/src/desktop/shell2/macos/events.rs

## Summary
- Lines: 1325
- Public functions: 13 (handle_mouse_down, handle_mouse_up, handle_mouse_move, handle_mouse_entered, handle_mouse_exited, handle_scroll_wheel, handle_key_down, handle_key_up, handle_text_input, handle_flags_changed, handle_resize, handle_file_drop, recursive_build_nsmenu)
- Public structs/enums: 1 (EventProcessResult)
- Re-exports: 1 (HitTestNode)
- Findings: 1 high, 3 medium, 0 low

## Findings

### [HIGH] Dead Code — `handle_file_drop` is never called
- **Location**: `events.rs:753`
- **Details**: Public method `handle_file_drop` has zero call sites. No drag-and-drop delegate methods (`draggingEntered:`, `performDragOperation:`, etc.) exist in `mod.rs`.
- **Evidence**: `grep -r "\.handle_file_drop(" dll/src/` returns zero results.
- **Recommendation**: Implement drag-and-drop delegate methods in `mod.rs` that call this, or remove if the feature is not yet needed.

### [MEDIUM] Missing Module Documentation
- **Location**: `events.rs:1`
- **Details**: The file has a `//!` module doc comment (`//! macOS Event handling - converts NSEvent to Azul events and dispatches callbacks.`) but it is minimal. It does not describe key types (`EventProcessResult`), entry points, or how it fits into the larger windowing system (relationship to `PlatformWindow` trait, `common/event.rs`, `mod.rs` delegate methods).
- **Recommendation**: Expand to mention `EventProcessResult`, the `PlatformWindow` trait dependency, and that `mod.rs` delegate methods are the callers.

### [MEDIUM] Redundant `EventProcessResult` enum
- **Location**: `events.rs:59-70`
- **Details**: All other platforms (X11, Wayland, Windows) use `azul_core::events::ProcessEventResult` directly. The macOS code defines a local `EventProcessResult` with 5 variants and a `convert_process_result` mapping function (line 75). This adds indirection without clear benefit — the mapping loses information (e.g., `ShouldIncrementalRelayout` and `ShouldUpdateDisplayListCurrentWindow` both map to `UpdateDisplayList`).
- **Recommendation**: Consider using `ProcessEventResult` directly, as all other platforms do.

### [MEDIUM] Magic Numbers — scroll and resize thresholds
- **Location**: `events.rs:324` (`0.01`), `events.rs:704` (`0.001`), `events.rs:711` (`96.0`), `events.rs:721-724` (`0.5`), `events.rs:733` (breakpoints array)
- **Details**: Multiple hard-coded numeric literals for scroll dead zone, DPI epsilon, base DPI, resize debounce, and CSS breakpoints.
- **Recommendation**: Extract to named constants: `SCROLL_DEAD_ZONE`, `DPI_CHANGE_EPSILON`, `BASE_DPI`, `VIEWPORT_RESIZE_EPSILON`, `CSS_BREAKPOINTS`.

## System Documentation
- System identified: yes — macOS windowing/event handling (part of the cross-platform windowing system)
- Existing doc: none (no `doc/guide/windowing.md` or `doc/guide/event-handling.md`)
- Doc needed: A guide covering the windowing and event-handling system — how platform-specific event handlers (macOS/Windows/X11/Wayland) delegate to the common `PlatformWindow` trait, the event flow from OS to callback dispatch, and the role of `EventProcessResult` / `ProcessEventResult`.
