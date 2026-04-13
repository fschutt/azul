# Review: layout/src/widgets/frame.rs

## Summary
- Lines: 448
- Public functions: 5 (`create`, `swap_with_default`, `set_flex_grow`, `with_flex_grow`, `dom`)
- Public structs/enums: 1 (`Frame`)
- Findings: 0 high, 1 medium, 0 low

## Findings

### [MEDIUM] Dead Code — `Frame` widget has zero external call sites
- **Location**: `frame.rs:241` (`pub struct Frame`)
- **Details**: Grep for `Frame::create` across the entire codebase returns zero results. Grep for `frame::Frame` returns only a webrender doc comment unrelated to this widget. The `Frame` widget is exported via `layout/src/widgets/mod.rs:133` but never used by any code in `dll/`, `examples/`, or `tests/`. Note: Frame is referenced in `api.json`, so it is part of the public FFI API.
- **Evidence**: `Grep "Frame::create" *.rs` → 0 results. `Grep "frame::Frame" *.rs` → 1 result (webrender comment, unrelated).
- **Recommendation**: Either wire the Frame widget into examples/tests/dll API, or mark it as `#[doc(hidden)]` / remove if unused.

## System Documentation
- System identified: yes — Widgets system (native-look widget library)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (guide already exists, though Frame is likely not documented in it)
