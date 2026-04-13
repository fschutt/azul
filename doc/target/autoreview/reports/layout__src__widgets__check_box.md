# Review: layout/src/widgets/check_box.rs

## Summary
- Lines: 310
- Public functions: 5 (`create`, `swap_with_default`, `set_on_toggle`, `with_on_toggle`, `dom`)
- Public structs/enums: 3 (`CheckBox`, `CheckBoxStateWrapper`, `CheckBoxState`)
- Findings: 1 high, 2 medium, 1 low

## Findings

### [HIGH] Function Pointer Cast to `usize` — potential UB
- **Location**: `check_box.rs:233`
- **Details**: `self::input::default_on_checkbox_clicked as usize` casts a function pointer directly to `usize`. The `CoreCallback.cb` field is typed as `CoreCallbackType = usize` (see `core/src/callbacks.rs:765`). This is an intentional architectural pattern used across the codebase to break circular dependencies between `azul-core` and `azul-layout`. While this is a known design decision, casting a function pointer directly to an integer (without going through `as *const ()` first) can trigger compiler warnings and is technically implementation-defined behavior.
- **Evidence**: `core/src/callbacks.rs:765` defines `pub type CoreCallbackType = usize;`, and the same pattern is used in other widgets (button, text_input, etc.).
- **Recommendation**: Consider casting via `as *const () as usize` to silence potential compiler warnings and make the intent clearer. This is a codebase-wide pattern, not specific to this file.

### [MEDIUM] Hard-Coded Magic Numbers in Default Styles
- **Location**: `check_box.rs:92-93` (width/height 14px), `check_box.rs:96-106` (padding 2px), `check_box.rs:109-119` (border 1px), `check_box.rs:156-157` (content 8px)
- **Details**: The checkbox dimensions (14px container, 8px content, 2px padding, 1px border) are hard-coded as numeric literals throughout the static style arrays. While these are `const` expressions, the relationship between them is not documented (14 = 8 + 2*2 + 2*1).
- **Recommendation**: Define named constants for the checkbox geometry (e.g., `CHECKBOX_SIZE`, `CHECKBOX_CONTENT_SIZE`, `CHECKBOX_PADDING`, `CHECKBOX_BORDER_WIDTH`) to make the dimensional relationship explicit and easier to maintain.

### [MEDIUM] `..Default::default()` Usage — Safe but Worth Noting
- **Location**: `check_box.rs:177`
- **Details**: `CheckBoxStateWrapper { inner: ..., ..Default::default() }` defaults the `on_toggle: OptionCheckBoxOnToggle` field. `CheckBoxStateWrapper` derives `Default`, and `OptionCheckBoxOnToggle` defaults to `None`, which is the correct behavior here (no callback by default).
- **Recommendation**: No change needed — this usage is safe. The defaulted field (`on_toggle`) is genuinely safe at its default value.

### [LOW] Redundant Style Arrays — Checked vs Unchecked Differ Only in Opacity
- **Location**: `check_box.rs:155-169`
- **Details**: `DEFAULT_CHECKBOX_CONTENT_STYLE_CHECKED` and `DEFAULT_CHECKBOX_CONTENT_STYLE_UNCHECKED` are identical except for opacity (100 vs 0). This is 14 lines of near-duplication.
- **Recommendation**: Consider using a single base style array and applying the opacity difference dynamically in `CheckBox::create`, similar to how the toggle callback already sets opacity dynamically at runtime (`check_box.rs:291-299`).

## System Documentation
- System identified: yes — Widget system (built-in widgets)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (guide already exists)
