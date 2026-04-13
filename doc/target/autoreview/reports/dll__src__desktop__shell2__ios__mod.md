# Review: dll/src/desktop/shell2/ios/mod.rs

## Summary
- Lines: 306
- Public functions: 1 (`launch_app`)
- Public structs/enums: 3 (`RenderBackend`, `IOSEvent`, `IOSWindow`)
- Public methods: 6 (`new`, `poll_event`, `present`, `is_open`, `close`, `request_redraw`)
- Findings: 4 high, 2 medium, 1 low

## Findings

### [HIGH] Stub Code — `draw_rect` draws hardcoded blue, not actual UI
- **Location**: `ios/mod.rs:65-74`
- **Details**: The `drawRect:` callback just fills the screen solid blue. Comment on line 69 says "In a real app, this is where you'd get the pixel buffer from your CPU compositor." This is a non-functional placeholder.
- **Evidence**: Lines 69-73 contain hardcoded `CGContextSetRGBFillColor(context, 0.0, 0.0, 1.0, 1.0)`.
- **Recommendation**: Implement CPU rendering pipeline as described in `scripts/IOS_IMPLEMENTATION_PLAN.md` Phase 2.

### [HIGH] Stub Code — `touches_began` only logs, does not process events
- **Location**: `ios/mod.rs:77-83`
- **Details**: The touch handler logs "Touches Began!" but does not translate touch coordinates into Azul events or call `process_window_events()`. Touch input is non-functional.
- **Evidence**: Lines 79-82 — only a `log_debug!` call, no state mutation.
- **Recommendation**: Implement touch-to-event translation as described in `scripts/IOS_IMPLEMENTATION_PLAN.md` Phase 3.

### [HIGH] Stub Code — Missing touch handlers (touchesMoved, touchesEnded, touchesCancelled)
- **Location**: `ios/mod.rs:85`
- **Details**: Line 85 is a comment: `// ... Implement touchesMoved, touchesEnded, touchesCancelled similarly ...` — these are never implemented. Only `touchesBegan` is registered on the view class (line 96). Touch lifecycle is incomplete.
- **Evidence**: Grep for `touchesMoved|touchesEnded|touchesCancelled` in the file returns zero results (only the comment).
- **Recommendation**: Implement all four touch handlers and register them in `get_or_create_view_class`.

### [HIGH] Stub Code — `create_gl_context` always returns Err
- **Location**: `ios/mod.rs:266-271`
- **Details**: The function is documented as "Placeholder" and unconditionally returns `Err("GPU rendering not yet implemented for iOS")`. This forces CPU-only rendering which is itself also a stub (see `draw_rect` finding).
- **Recommendation**: Acceptable as a deliberate fallback for now, but the CPU path it falls back to must actually work first.

### [MEDIUM] Unsafe — Global mutable static `AZUL_IOS_WINDOW` without synchronization
- **Location**: `ios/mod.rs:60`
- **Details**: `static mut AZUL_IOS_WINDOW: *mut IOSWindow = ptr::null_mut()` is accessed from multiple `extern "C"` callbacks (`draw_rect`, `touches_began`, `did_finish_launching`). While iOS UI callbacks all run on the main thread, this is still UB under Rust's safety model. The pattern also leaks the `IOSWindow` (line 118: `Box::into_raw` with no corresponding `Box::from_raw` cleanup).
- **Evidence**: The `IOS_IMPLEMENTATION_PLAN.md` acknowledges this as "Unsafe, but simple for this minimal example."
- **Recommendation**: Consider wrapping in a `static` with `OnceCell<*mut IOSWindow>` or at minimum document the main-thread-only invariant. The memory leak is acceptable for an app-lifetime singleton.

### [MEDIUM] Dead Code — `RenderBackend` enum defined locally, shadows macOS version
- **Location**: `ios/mod.rs:173`
- **Details**: `RenderBackend` is also defined in `macos/mod.rs:123` with different variants (`Gpu`/`Cpu` vs `OpenGL`/`CPU`). The iOS version is only used internally within `ios/mod.rs`. The inconsistent naming could cause confusion.
- **Evidence**: Grep for `RenderBackend` shows 16 files; macOS uses `RenderBackend::OpenGL`/`RenderBackend::CPU`.
- **Recommendation**: Consider a shared `RenderBackend` enum in `common/` or align variant names.

### [LOW] Magic Number — Hardcoded RGBA color values
- **Location**: `ios/mod.rs:71`
- **Details**: `CGContextSetRGBFillColor(context, 0.0, 0.0, 1.0, 1.0)` — blue color is hardcoded. This is acceptable since the entire `draw_rect` is a placeholder, but worth noting.
- **Recommendation**: Will be resolved when the real rendering pipeline is implemented.

## System Documentation
- System identified: yes — iOS windowing/platform backend (part of the cross-platform shell2 windowing system)
- Existing doc: none (no `doc/guide/` file for platform backends or iOS)
- Doc needed: A `doc/guide/platform-backends.md` or `doc/guide/windowing.md` covering the shell2 platform abstraction (`PlatformWindow` trait, `CommonWindowState`, per-platform modules, event flow). This would cover all backends (macOS, Windows, Linux/X11, Linux/Wayland, iOS, headless) in one document. The `scripts/IOS_IMPLEMENTATION_PLAN.md` contains good architectural context that should eventually be distilled into such a guide.
