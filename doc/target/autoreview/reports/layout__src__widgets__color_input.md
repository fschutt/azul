# Review: layout/src/widgets/color_input.rs

## Summary
- Lines: 186
- Public functions: 5 (`create`, `set_on_value_change`, `with_on_value_change`, `swap_with_default`, `dom`)
- Public structs/enums: 3 (`ColorInput`, `ColorInputStateWrapper`, `ColorInputState`)
- Public type aliases: 1 (`ColorInputOnValueChangeCallbackType`)
- Findings: 1 high, 0 medium, 0 low

## Findings

### [HIGH] Known Bug Pattern — function pointer cast directly to usize
- **Location**: `color_input.rs:152`
- **Details**: `on_color_input_clicked as usize` casts a function pointer directly to `usize`. While this is a codebase-wide pattern used consistently in all widgets (text_input, tabs, drop_down, ribbon, titlebar, node_graph), it relies on the assumption that function pointers fit in `usize` and that `CoreCallbackType` is `usize` (`core/src/callbacks.rs:765`). The cast bypasses type safety — the compiler cannot verify the function signature matches expectations. If `on_color_input_clicked`'s signature ever drifts from what the callback dispatch expects, this will be a silent runtime bug.
- **Evidence**: Pattern used in all widgets: `text_input.rs:832`, `tabs.rs:1340`, `drop_down.rs:252`, `ribbon.rs:205`, `titlebar.rs:377`. `CoreCallbackType` is defined as `usize` at `core/src/callbacks.rs:765`.
- **Recommendation**: This is an architectural decision (FFI compatibility). No file-local fix, but worth noting as a systemic risk. The `node_graph.rs:913` file even has a double-cast `as usize as usize` which is a code smell.

## System Documentation
- System identified: yes — Widget system
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (guide already exists)
