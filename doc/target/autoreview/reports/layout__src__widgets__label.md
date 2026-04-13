# Review: layout/src/widgets/label.rs

## Summary
- Lines: 133
- Public functions: 3 (`create`, `swap_with_default`, `dom`)
- Public structs/enums: 1 (`Label`)
- Findings: 1 high, 0 medium, 0 low

## Findings

### [HIGH] Dead Code — Label struct never used outside its own module
- **Location**: `label.rs:21` (`pub struct Label`), `label.rs:97` (`pub fn create`)
- **Details**: `Label::create` is never called outside `label.rs` itself (the only call site is `swap_with_default` at line 113). No file in the codebase imports `label::Label` or `widgets::Label`. The struct appears in the codegen reexports list (`doc/src/codegen/v2/lang_reexports.rs:287`) as a string name for FFI/binding generation, but there are zero Rust call sites that construct or use a `Label`.
- **Evidence**: `Grep pattern="Label::create" type=rust` → only match is `label.rs:113`. `Grep pattern="label::Label|widgets::Label|use.*Label" type=rust` → zero matches. `Grep pattern="Label\b" type=rust output_mode=files_with_matches` → 21 files, but all references are to unrelated items (`NodeType::Label`, HTML `<label>`, menu labels, etc.).
- **Recommendation**: Either wire `Label` into actual widget usage (examples, other widgets) or mark the module as pending integration. The widget exists in the API surface but is unused in practice.

## System Documentation
- System identified: yes — Widget system (label widget)
- Existing doc: `doc/guide/widgets.md`
- Doc needed: n/a (guide exists)
