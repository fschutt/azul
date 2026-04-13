# Review: layout/src/scroll_timer.rs

## Summary
- Lines: 519
- Public functions: 2 (`ScrollPhysicsState::new`, `scroll_physics_timer_callback`)
- Public structs/enums: 2 (`ScrollPhysicsState`, `NodeScrollPhysics`)
- Findings: 0 high, 1 medium, 1 low

## Findings

### [MEDIUM] `scroll_physics_timer_callback` is 284 lines (lines 134-418)
- **Location**: `scroll_timer.rs:134-418`
- **Details**: The main timer callback function is ~284 lines. It has clear phase comments (input processing, velocity integration, position application, terminate decision), but the velocity integration section (lines 240-344, ~104 lines) and the position application section (lines 351-406, ~55 lines) could each be extracted into helper functions for readability.
- **Recommendation**: Extract the velocity integration loop and the position-application loop into separate private functions. This would bring the main function closer to the ~100 LOC target.

### [LOW] Module doc could mention `scroll_physics_timer_callback` by name
- **Location**: `scroll_timer.rs:1-34`
- **Details**: The module doc is thorough and well-structured. Minor improvement: explicitly naming `scroll_physics_timer_callback` as the entry point would help readers find it quickly.
- **Recommendation**: No action needed. The doc is good.

## Items Checked — No Issues Found

- **Duplicated functionality**: `scroll_physics_timer_callback` is the only scroll physics timer in the codebase. No duplication.
- **File size**: 519 lines is appropriate for the scope of this module.
- **Outdated comments**: All references (e.g., `ScrollManager.record_scroll_input`, `SCROLL_MOMENTUM_TIMER`, `CallbackChange::ScrollTo`, `SystemStyle`) verified to exist in the codebase.
- **Obvious bugs**: Physics math looks correct. Spring-back uses critically-damped spring (no oscillation). Friction decay is well-formulated. Clamping logic is correct.
- **Stub code / vibe-coding hints**: No `TODO`, `FIXME`, `HACK`, `todo!()`, `unimplemented!()`, `placeholder`, `dummy`, or `stub` found.
- **`..Default::default()`**: Not used in this file.
- **Unsafe code / FFI**: The `extern "C"` on `scroll_physics_timer_callback` (line 134) is correct — it matches `TimerCallbackType` signature. No unsafe blocks in the file.
- **Known bug patterns**: Return values are properly used. No null/empty FFI confusion. No lossy type conversions.
- **Scripts directory**: `scripts/scroll3.md` through `scripts/scroll6_report.md` contain design docs for the scroll system. The implementation aligns with the described architecture.

## System Documentation
- System identified: yes — scroll physics / scroll timer system
- Existing doc: none (no dedicated guide in `doc/guide/` for the scroll system; `lifecycle.md` and `css-properties.md` mention scroll peripherally)
- Doc needed: A `doc/guide/scrolling.md` explaining the scroll architecture: `ScrollManager` + `ScrollInputQueue` + `scroll_physics_timer_callback` pipeline, per-node CSS properties (`overflow-scrolling`, `overscroll-behavior`), rubber-banding, and how platform event handlers feed into the system.
