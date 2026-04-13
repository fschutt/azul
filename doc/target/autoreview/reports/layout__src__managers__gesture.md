# Review: layout/src/managers/gesture.rs

## Summary
- Lines: 1429
- Public functions: 1 (`allocate_event_id`)
- Public structs/enums: 10 (`GestureDetectionConfig`, `InputSample`, `InputSession`, `DetectedDrag`, `DetectedLongPress`, `GestureDirection`, `DetectedPinch`, `DetectedRotation`, `PenState`, `GestureAndDragManager`)
- Public type aliases: 0
- Public constants: 2 (`MAX_SAMPLES_PER_SESSION`, `DEFAULT_SAMPLE_TIMEOUT_MS`)
- Re-exports: 0
- Findings: 1 high, 2 medium, 0 low

## Findings

### [HIGH] Dead Code — Numerous public methods with zero external call sites
- **Location**: Multiple methods on `GestureAndDragManager`
- **Details**: The following public methods have zero call sites outside `gesture.rs`:
  - `mark_long_press_callback_invoked` (line 716)
  - `get_drag_direction` (line 832)
  - `get_gesture_velocity` (line 857)
  - `is_swipe` (line 876)
  - `get_current_mouse_position` (line 1028)
  - `get_window_position_at_session_start` (line 1104)
  - `activate_text_selection_drag` (line 1124)
  - `activate_scrollbar_drag` (line 1140)
  - `update_active_drag_positions` (line 1204)
  - `update_auto_scroll_direction` (line 1226)
  - `is_text_selection_dragging` (line 1257)
  - `is_scrollbar_dragging` (line 1262)
  - `session_count` (line 1298)
  - `current_session_id` (line 1303)
  - `get_window_drag_delta` (line 1375)
  - `get_window_position_from_drag` (line 1390)
  - `get_scrollbar_scroll_offset` (line 1408)
  - `clear_old_sessions` (line 580)
  - `clear_all_sessions` (line 602)
  - `update_pen_state` (line 610)
  - `clear_pen_state` (line 635)
  - `with_config` (line 416)
- **Evidence**: Grepped for each method name across the codebase; zero results outside `gesture.rs`.
- **Recommendation**: Many of these appear to be API surface built speculatively. Consider removing or marking `pub(crate)` for methods not yet integrated. The text-selection and scrollbar drag activation methods are particularly notable — they suggest the unified drag system is only partially wired up.

### [MEDIUM] Dead Code — Public types with zero or near-zero external usage
- **Location**: Multiple structs
- **Details**: The following types have zero external call sites:
  - `GestureDetectionConfig` (line 95) — never referenced outside this file
  - `InputSession` (line 173) — only in `doc/src/autofix/module_map.rs`
  - `DetectedDrag` (line 243)
  - `DetectedLongPress` (line 261)
  - `DetectedPinch` (line 284)
  - `DetectedRotation` (line 299)
  - `allocate_event_id` function (line 48)
- **Evidence**: Grepped for each type/function name; zero results outside `gesture.rs` (except `InputSession` in a doc tooling file).
- **Recommendation**: These are return types of detection methods or internal config. Consider `pub(crate)` visibility.

### [MEDIUM] Missing Module-Level Documentation for `start_file_drop`
- **Location**: `gesture.rs:1198`
- **Details**: `start_file_drop` has zero external call sites. It accepts `Vec<AzString>` for files, but there is no corresponding platform code calling it. The file-drop path appears to go through `DragContext::file_drop` directly elsewhere. This suggests the method exists but the OS file-drop events are not routed through `GestureAndDragManager.start_file_drop()`.
- **Recommendation**: Verify whether file drops should route through this method. If not, remove it.

### [MEDIUM] Documentation — `// +spec` comments absent; several doc comments are verbose
- **Location**: Various
- **Details**: The module doc (lines 1–14) is good but slightly verbose. Most public items have appropriate doc comments. The doc on `get_drag_delta_screen_incremental` (lines 1069–1086) includes a 7-line explanation with ASCII diagram — this is somewhat verbose but acceptable given the subtlety of incremental vs. total delta.
- **Recommendation**: No action required; docs are generally appropriate.

## System Documentation
- System identified: **Input / gesture detection system** (part of the event handling pipeline)
- Existing doc: `doc/guide/architecture.md` mentions drag/gesture briefly
- Doc needed: No dedicated guide exists for the gesture/drag system. A `doc/guide/gestures.md` covering the input session model, gesture detection pipeline, unified drag context, and how platform events flow into `GestureAndDragManager` would be valuable. This system spans `layout/src/managers/gesture.rs`, `azul_core/src/drag.rs`, `layout/src/managers/drag_drop.rs`, `layout/src/event_determination.rs`, and `dll/src/desktop/shell2/common/event.rs`.
