# Review: layout/src/event_determination.rs

## Summary
- Lines: ~820 (including ~160 lines of tests)
- Public functions: 1 (`determine_all_events`)
- Public structs/enums: 0
- Findings: 1 high, 3 medium, 0 low

## Findings

### [HIGH] Dead Code — `get_all_hovered_nodes` unused outside this file
- **Location**: `event_determination.rs:209-220`
- **Details**: Private function only called from within `determine_all_events` at line 432-433, which is fine. However, the function takes a `frame_index` parameter and does `.get_frame(&InputPointId::Mouse, frame_index)` — this pattern assumes `HoverManager::get_frame` supports arbitrary frame indexing. If the hover manager only stores current and previous frames, passing index values > 1 would silently return None.
- **Evidence**: Grep for `get_all_hovered_nodes` found only `layout/src/event_determination.rs` and `scripts/EVENT_ARCHITECTURE_ANALYSIS.md`. Call sites use indices 0 and 1, which are valid.
- **Recommendation**: This is not dead code per se (it's private and called), but the `frame_index` API is fragile. Consider adding a bounds check or using named constants (`CURRENT_FRAME = 0`, `PREVIOUS_FRAME = 1`).

### [MEDIUM] Module-Level Documentation — adequate
- **Location**: `event_determination.rs:1-5`
- **Details**: The file has a `//!` module doc block explaining its purpose. It is brief and accurate. No issues.

### [MEDIUM] Lossy Type Conversion — `phys_pos.x as f32`
- **Location**: `event_determination.rs:129-130` and `event_determination.rs:567-568`
- **Details**: Physical position coordinates are cast from their source type to `f32` using `as`. If the source type is `i32` or `f64`, this could lose precision for large values. While window positions are unlikely to exceed f32 range in practice, `try_into()` or explicit documentation would be safer.
- **Recommendation**: Low risk in practice but consider using `as f64` → `as f32` explicitly or adding a comment noting the acceptable precision loss.

### [MEDIUM] `determine_all_events` is ~615 LOC — consider splitting
- **Location**: `event_determination.rs:248-873`
- **Details**: The function body spans from line 259 to line 873 (~615 lines). While it is logically organized with section comments, extracting subsections (mouse button events, keyboard events, window state events, gesture events) into private helper functions would improve readability and testability.
- **Recommendation**: Extract at least the gesture event detection (lines 666-861, ~195 lines) and mouse button events (lines 306-393, ~87 lines) into separate functions.

## System Documentation
- System identified: yes — Event Loop / Event Determination system
- Existing doc: `doc/guide/lifecycle.md` covers the general lifecycle but not event determination specifically
- Doc needed: A guide document covering the event pipeline (how platform input flows through managers to `determine_all_events` to callback dispatch) would be valuable. The architecture diagram in the doc comment at lines 36-47 is a good starting point.
