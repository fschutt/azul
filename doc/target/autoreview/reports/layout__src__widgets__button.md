# Review: layout/src/widgets/button.rs

## Summary
- Lines: 388
- Public functions: 9 (`class_name`, `create`, `with_type`, `set_button_type`, `with_button_type`, `swap_with_default`, `set_image`, `set_on_click`, `with_on_click`, `dom`)
- Public structs/enums: 3 (`Button`, `ButtonType`, `ButtonOnClickCallbackType` type alias + macro-generated types)
- Findings: 0 high, 0 medium, 2 low

## Findings

### [LOW] `ButtonType::class_name()` is only used internally
- **Location**: `button.rs:54-65`
- **Details**: `class_name()` is `pub` but only called in `dom()` (line 1013). No external callers found.
- **Evidence**: Grep for `class_name` in `layout/src/widgets/` returns only `button.rs`.
- **Recommendation**: Consider making it `pub(crate)` or keeping it `pub` if it's part of the intended API.

### [LOW] `set_image()` has no external callers
- **Location**: `button.rs:317`
- **Details**: Grep for `set_image` outside `button.rs` returns only `webrender/core/src/resource_cache.rs` (different method). No callers of `Button::set_image`.
- **Evidence**: Grep `set_image` finds only the definition in `button.rs` and an unrelated method in `resource_cache.rs`.
- **Recommendation**: Now that image rendering is wired up in `dom()`, this method is functional but has no external callers yet.

## System Documentation
- System identified: yes — Widget system (button widget)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (guide exists)
