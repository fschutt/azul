# Review: dll/src/desktop/shell2/windows/dpi.rs

## Summary
- Lines: 230
- Public functions: 6 (`init`, `become_dpi_aware`, `enable_non_client_dpi_scaling`, `get_monitor_dpi`, `hwnd_dpi`, `dpi_to_scale_factor`)
- Public structs/enums: 2 (`DpiFunctions`, `ProcessDpiAwareness`, `MonitorDpiType`) + 7 type aliases
- Findings: 1 high, 1 medium, 1 low

## Findings

### [HIGH] Dead Code — `get_monitor_dpi` method has no callers
- **Location**: `dpi.rs:156`
- **Details**: The public method `get_monitor_dpi` is defined but never called from outside `dpi.rs`.
- **Evidence**: Grep for `get_monitor_dpi` across the entire codebase returns only `dpi.rs:156`.
- **Recommendation**: Wire into monitor enumeration code or remove.

### [MEDIUM] Dead Code — `adjust_window_rect_ex_for_dpi` field loaded but never used
- **Location**: `dpi.rs:60`, `dpi.rs:89-91`
- **Details**: The `adjust_window_rect_ex_for_dpi` field is populated during `init()` by loading the `AdjustWindowRectExForDpi` symbol, but no method exposes it and no code outside dpi.rs accesses it.
- **Evidence**: Grep for `adjust_window_rect_ex_for_dpi` in `mod.rs` returns no matches. All matches are within dpi.rs.
- **Recommendation**: Add a public method that uses it for DPI-correct window rect adjustment, or remove the field.

### [LOW] Duplicated `load_dll` / `encode_ascii` Helpers
- **Location**: `dpi.rs:81` (calls `super::load_dll`), `dpi.rs:116` (calls `super::encode_ascii`)
- **Details**: `load_dll` exists in `mod.rs:3385`, `gl.rs:16`, and `dlopen.rs` (slightly different return types). `encode_ascii` exists in `mod.rs:3373`, `gl.rs:19`, and `dlopen.rs:134` (returning `Vec<u8>` vs `Vec<i8>`). This is a codebase-wide pattern, not specific to this file.
- **Evidence**: Grep for `fn encode_ascii` returns 3 definitions; grep for `fn load_dll` returns 2 definitions.
- **Recommendation**: Consolidate into a single shared utility (low priority, cross-cutting concern).

## System Documentation
- System identified: yes — Windows DPI / windowing subsystem
- Existing doc: none (no `doc/guide/windowing.md` or `doc/guide/dpi.md`)
- Doc needed: A windowing system guide covering platform-specific window creation, DPI handling, and event loop integration would be valuable. This file is part of the Windows shell2 backend.
