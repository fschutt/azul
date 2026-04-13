# Review: dll/src/desktop/shell2/run.rs

## Summary
- Lines: 1300 (141 blank)
- Public functions: 4 (`run` x4, one per platform via `#[cfg]`)
- Public structs/enums: 0
- Pub(super) statics: 1 (`INITIAL_OPTIONS`, iOS only)
- Findings: 1 high, 3 medium, 2 low

## Findings

### [HIGH] Memory Leak — macOS windows never freed
- **Location**: `run.rs:342` (Box::into_raw), `macos/mod.rs:1902` and `macos/mod.rs:4249` (unregister without free)
- **Details**: On macOS, windows are allocated with `Box::into_raw` at line 342. When windows close, `registry::unregister_window` is called at `macos/mod.rs:1902` and `macos/mod.rs:4249`, which returns `Option<*mut MacOSWindow>` — but the return value is **ignored** and `Box::from_raw` is never called. Compare with the Windows cleanup at `run.rs:909-912` which correctly calls `drop(Box::from_raw(win_ptr))`.
- **Evidence**: `registry::unregister_window` returns `Option<*mut MacOSWindow>` (registry.rs:99), but the call sites discard it.
- **Recommendation**: At both unregister call sites in `macos/mod.rs`, reclaim the pointer with `Box::from_raw` if it's returned. Also add cleanup at event loop exit (the `RunForever` path currently has no cleanup at all).

### [MEDIUM] Code Style — Deep nesting in event loops
- **Location**: `run.rs:418-583` (macOS loop body ~165 lines), `run.rs:759-904` (Windows loop ~145 lines), `run.rs:1027-1203` (Linux loop ~175 lines)
- **Details**: The inner event loop bodies are deeply nested (4-5 levels of indentation inside `autoreleasepool`, `loop`, `if let`, `unsafe`, `match`). The macOS `autoreleasepool` closure spans 165 lines. The Linux pending-window-create block (`run.rs:1080-1187`) is 107 lines of nested match arms.
- **Recommendation**: Extract the pending-window-create logic for each platform into a helper function (e.g. `process_pending_creates_macos()`, `process_pending_creates_linux()`).

### [LOW] Debug server handle dropped immediately
- **Location**: `run.rs:256`, `run.rs:259`, and 6 other identical sites
- **Details**: `let (_handle, rx) = debug_server::start_debug_server(port);` — the `_handle` (an `Arc<DebugServerHandle>`) is dropped at end of the `if` block. If the handle's `Drop` impl shuts down the server, this could be a problem. However, since the server thread is spawned and the handle is `Arc`-wrapped, the server likely continues running via the internal `Arc` clone. Low risk but worth verifying.
- **Recommendation**: Verify that dropping the `Arc<DebugServerHandle>` does not stop the debug server. If it does, the handle must be kept alive for the duration of the event loop.

### [LOW] `#[allow(deprecated)]` for `activateIgnoringOtherApps`
- **Location**: `run.rs:377-378`
- **Details**: `activateIgnoringOtherApps` is deprecated in macOS 14+. The `#[allow(deprecated)]` suppresses the warning but the replacement API (`activate()` or `NSRunningApplication.activate(options:)`) should be used for forward compatibility.
- **Recommendation**: Consider using the non-deprecated activation API with a fallback for older macOS versions.

## System Documentation
- **System identified**: Event loop / windowing system
- **Existing doc**: `doc/guide/lifecycle.md` covers app lifecycle at a high level
- **Doc needed**: A dedicated `doc/guide/event-loop.md` explaining the multi-platform event loop architecture, the PHASE-based tick structure, multi-window management via registries, headless mode, debug/E2E infrastructure, and backend resolution. The `scripts/EVENT_ARCHITECTURE_ANALYSIS_DOC.md` planning document already outlines many of these concepts and could serve as a starting point.
