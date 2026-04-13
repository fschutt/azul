# Review: layout/src/widgets/file_input.rs

## Summary
- Lines: 212
- Public functions: 7 (`create`, `swap_with_default`, `set_default_text`, `with_default_text`, `set_on_path_change`, `with_on_path_change`, `dom`)
- Public structs/enums: 3 (`FileInput`, `FileInputStateWrapper`, `FileInputState`)
- Public type aliases: 1 (`FileInputOnPathChangeCallbackType`)
- Findings: 0 high, 1 medium, 0 low

## Findings

### [MEDIUM] Dead Code — `set_default_text` / `with_default_text` have no external callers
- **Location**: `file_input.rs:125-133`
- **Details**: `set_default_text` and `with_default_text` are only defined in this file and referenced in `api.json` (for codegen), but have zero Rust call sites outside this module.
- **Evidence**: Grep for `set_default_text|with_default_text` returned only `api.json` and `file_input.rs`.
- **Recommendation**: Low priority — these are API surface methods likely used by C/Python bindings generated from `api.json`. Flag for awareness but no action needed if bindings use them.

## System Documentation
- System identified: yes — Widgets system
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (guide exists)
