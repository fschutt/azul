# Review: layout/src/widgets/drop_down.rs

## Summary
- Lines: 338
- Public functions: 5 (`new`, `set_on_choice_change`, `with_on_choice_change`, `swap_with_default`, `dom`)
- Public structs/enums: 1 (`DropDown`), plus macro-generated types (`DropDownOnChoiceChange`, `OptionDropDownOnChoiceChange`, `DropDownOnChoiceChangeCallback`)
- Findings: 1 high, 1 medium, 1 low

## Findings

### [HIGH] Dead Code — DropDown widget has zero external call sites
- **Location**: `drop_down.rs:187` (`pub struct DropDown`)
- **Details**: `DropDown` and all its public API (`new`, `dom`, `set_on_choice_change`, `with_on_choice_change`, `swap_with_default`) are never used outside this file. No examples, no tests, and no other module references these types. The module is declared `pub mod drop_down` in `layout/src/widgets/mod.rs:131` but is never re-exported or consumed.
- **Evidence**: `Grep pattern="DropDown" glob="*.rs"` returned only hits in `layout/src/widgets/drop_down.rs`. `Grep pattern="drop_down" glob="*.rs"` returned only `drop_down.rs` and `mod.rs` (the module declaration).
- **Recommendation**: Either wire this widget into examples/tests/the public API, or remove it if it is not planned for use.

### [MEDIUM] Repetitive gradient definitions
- **Location**: `drop_down.rs:57-112` (`NORMAL_BG_ITEMS`, `HOVER_BG_ITEMS`, `ACTIVE_BG_ITEMS`)
- **Details**: Three nearly identical gradient definitions that differ only in the two color values. Each is ~17 lines. The pattern is repeated 3 times for ~51 lines total.
- **Recommendation**: Not easily extractable to a helper since these are `const` values, but worth noting. If `const fn` support allows, a helper like `const_vertical_gradient(top: ColorU, bottom: ColorU)` would reduce this to 3 lines.

### [LOW] Unused `mut` on `info` parameter
- **Location**: `drop_down.rs:318` — `on_choice_selected(mut refany: RefAny, info: CallbackInfo)`
- **Details**: The `info` parameter is not marked `mut` but `refany` is. This is fine since `info` is only cloned, not mutated. However, in `on_dropdown_click` (line 287), `mut info` is used and `info.open_menu_for_hit_node` is called — consistent. No actual issue, just noting the asymmetry is intentional.

## System Documentation
- System identified: yes — Widget system (built-in widgets)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (guide already exists; the DropDown widget should be documented within it once the widget is actually wired up)
