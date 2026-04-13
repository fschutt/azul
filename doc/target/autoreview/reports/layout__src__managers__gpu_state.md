# Review: layout/src/managers/gpu_state.rs

## Summary
- Lines: 379
- Public functions: 6 (`new`, `tick`, `record_scroll_activity`, `get_cache`, `get_or_create_cache`, `update_scrollbar_transforms`)
- Public structs/enums: 3 (`GpuStateManager`, `ScrollbarFadeState`, `GpuTickResult`)
- Findings: 2 high, 0 medium, 0 low

## Findings

### [HIGH] Dead Code — `tick()` is never called
- **Location**: `gpu_state.rs:113`
- **Details**: `GpuStateManager::tick()` is defined but never invoked anywhere in the codebase. All `.tick()` calls found are on `scroll_manager`, not `gpu_state_manager`. The entire fade-state tick system (including `ScrollbarFadeState`, `GpuTickResult`, `calculate_fade_opacity`, and `fade_states`) is unwired.
- **Evidence**: Grep for `gpu_state_manager\.tick\(|gpu_state\.tick\(|\.gpu_state.*\.tick` returned zero results. Grep for `GpuTickResult` only matches within this file. Grep for `ScrollbarFadeState` only matches within this file.
- **Recommendation**: Either wire `tick()` into the platform render loops (where `scroll_manager.tick(now)` is already called) or remove the dead fade-tick infrastructure. The `scrollbar_fade_active` field IS read by platform code but is only set by `synchronize_scrollbar_opacity` in `window.rs`, never by `tick()`.

### [HIGH] Dead Code — `record_scroll_activity()` never called
- **Location**: `gpu_state.rs:196`
- **Details**: No external call site found. Without this being called, `fade_states` is always empty, making `tick()` a no-op even if it were wired up.
- **Evidence**: Grep for `record_scroll_activity` only matches the definition at `gpu_state.rs:196`.
- **Recommendation**: Wire into scroll event handling or remove.


## System Documentation
- System identified: yes — GPU rendering pipeline / scrollbar animation subsystem
- Existing doc: none (no `doc/guide/` file for rendering pipeline or GPU state)
- Doc needed: A guide document covering the rendering pipeline, including GPU value caches, scrollbar transform/opacity management, and how `GpuStateManager` / `synchronize_scrollbar_opacity` / display list generation interact.
