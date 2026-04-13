# Review: layout/src/callbacks.rs

## Summary
- Lines: 4454
- Public functions: ~277
- Public structs/enums: 17
- Findings: 5 high, 4 medium, 4 low

## Findings

### [HIGH] Dead Code — `SelectAllResult`, `DeleteResult`, FFI Result types unused outside module
- **Location**: `callbacks.rs:110` (`SelectAllResult`), `callbacks.rs:136` (`DeleteResult`), `callbacks.rs:4371` (`ResultU8VecString`), `callbacks.rs:4387` (`ResultVoidString`), `callbacks.rs:4403` (`ResultStringString`)
- **Details**: These five public types are defined and only referenced within `callbacks.rs` itself (and `api.json`). No other Rust source file imports or uses them.
- **Evidence**: `grep -r "SelectAllResult" --include="*.rs"` returns only `layout/src/callbacks.rs`. Same for `DeleteResult`, `ResultU8VecString`, `ResultVoidString`, `ResultStringString`.
- **Recommendation**: Remove if not needed for FFI/C API. If they are generated from `api.json`, mark with `#[doc(hidden)]` and a comment explaining their purpose.

### [HIGH] Dead Code — Many inspect/changeset methods have zero call sites
- **Location**: `callbacks.rs:3491` (`inspect_copy_changeset`), `callbacks.rs:3501` (`inspect_cut_changeset`), `callbacks.rs:3510` (`inspect_paste_target_range`), `callbacks.rs:3523` (`inspect_select_all_changeset`), `callbacks.rs:3569` (`inspect_delete_changeset`), `callbacks.rs:3645` (`get_undo_text`), `callbacks.rs:3655` (`get_redo_text`)
- **Details**: These public methods have no call sites outside `callbacks.rs` (only in `api.json`).
- **Evidence**: `grep -r "inspect_copy_changeset\|inspect_cut_changeset\|inspect_paste_target_range\|inspect_select_all_changeset\|inspect_delete_changeset" --include="*.rs"` returns only `layout/src/callbacks.rs`.
- **Recommendation**: These appear to be designed for the Debug API / C FFI. If not yet wired up, add `#[doc(hidden)]` or remove. If intended for future C API exposure, document that clearly.

### [HIGH] Dead Code — Deprecated methods `get_scroll_delta` and `had_scroll_activity`
- **Location**: `callbacks.rs:3422` (`get_scroll_delta`), `callbacks.rs:3432` (`had_scroll_activity`)
- **Details**: Both methods are deprecated stubs that always return `None`/`false`. No call sites exist in Rust code (only in `scripts/` markdown docs and `api.json`).
- **Evidence**: `grep -r "get_scroll_delta\|had_scroll_activity" --include="*.rs"` returns only `layout/src/callbacks.rs`.
- **Recommendation**: Remove. The deprecation comment says "kept for FFI backward compatibility" but they do nothing.

### [HIGH] Duplicate Code — `has_selection` / `has_any_selection` and duplicate selection range methods
- **Location**: `callbacks.rs:2234` (`has_selection`), `callbacks.rs:3035` (`has_any_selection`), `callbacks.rs:2249` (`get_selection_ranges`), `callbacks.rs:3772` (`get_node_selection_ranges`)
- **Details**: `has_selection(&self, _dom_id: &DomId)` and `has_any_selection(&self)` have identical implementations — both ignore parameters and check `multi_cursor`. Similarly, `get_selection_ranges` and `get_node_selection_ranges` have identical bodies that ignore their parameters. (`get_all_selection_ranges` was removed as dead code.)
- **Recommendation**: Consolidate each group into a single canonical method. Keep one and make the others thin wrappers or remove them.

### [HIGH] Stub Methods — `take_native_screenshot` always returns `Err`
- **Location**: `callbacks.rs:2912` (`take_native_screenshot`), `callbacks.rs:2928` (`take_native_screenshot_bytes`), `callbacks.rs:2953` (`take_native_screenshot_base64`)
- **Details**: `take_native_screenshot` always returns an error directing users to use a different trait. `take_native_screenshot_bytes` calls the stub and will always fail. `take_native_screenshot_base64` calls `take_native_screenshot_bytes` and will always fail.
- **Recommendation**: Since the real implementation lives in `dll/src/desktop/native_screenshot.rs` via the `NativeScreenshotExt` trait, consider removing these stubs or marking them `#[doc(hidden)]` to avoid confusion.

### [MEDIUM] Dead Code — `get_selection` always returns `None`
- **Location**: `callbacks.rs:2227`
- **Details**: `get_selection` ignores its `_dom_id` parameter and unconditionally returns `None` with a comment: "SelectionManager removed; multi_cursor is the source of truth. SelectionState is a legacy type; return None."
- **Recommendation**: Remove this dead method. Callers should use `get_selection_ranges` or `get_primary_cursor` instead.

### [MEDIUM] Code Style — `get_node_attribute` is 100 lines of match arms
- **Location**: `callbacks.rs:2078–2178`
- **Details**: This method is ~100 lines, mostly a large match statement mapping attribute names to `AttributeType` variants. While each arm is trivial, the function is long.
- **Recommendation**: Consider extracting the match into a helper or using a generated lookup table. Not urgent since each arm is simple.

### [MEDIUM] Code Style — `inspect_move_cursor_*` methods are highly repetitive
- **Location**: `callbacks.rs:3807–3895` (four methods: `inspect_move_cursor_left/right/up/down`)
- **Details**: All four methods follow the identical pattern: get cursor, get layout, call a different `move_cursor_*` method, check if it moved. The only variation is the method called on `layout`.
- **Recommendation**: Extract a common helper like `inspect_cursor_move(target, |layout, cursor| layout.move_cursor_left(cursor, &mut None))` to eliminate the repetition.

### [MEDIUM] Code Style — `move_cursor_*` override methods are repetitive
- **Location**: `callbacks.rs:3989–4058` (eight methods)
- **Details**: All eight `move_cursor_*` / `move_cursor_to_*` methods share the same structure: extract dom_id/node_id from target, unwrap_or NodeId::ZERO, push a change. Only the `CallbackChange` variant differs.
- **Recommendation**: Extract a helper that takes the `CallbackChange` constructor, or use a macro.

### [MEDIUM] Dead Code — `get_node_id_by_id_attribute` and `get_children_count` unused
- **Location**: `callbacks.rs:1089` (`get_node_id_by_id_attribute`), `callbacks.rs:1200` (`get_children_count`)
- **Details**: No call sites in Rust code outside `callbacks.rs`.
- **Evidence**: `grep -r "get_node_id_by_id_attribute\|get_children_count" --include="*.rs"` returns only `layout/src/callbacks.rs`.
- **Recommendation**: These may be for the C/FFI API via `api.json`. If so, document that. Otherwise remove.

### [LOW] File Size — 4454 lines is large but cohesive
- **Location**: Entire file
- **Details**: The file is large but cohesive — it contains `CallbackInfo`, `CallbackChange`, and related FFI types that all serve the callback system. The methods on `CallbackInfo` are the public API surface for user callbacks.
- **Recommendation**: No split needed. The file is well-organized with section comments. The ICU methods (lines 2482–2695) could potentially be extracted to a separate `callbacks_icu.rs` if desired, but it's not necessary.

### [LOW] Code Style — `OptionCallback` and `OptionMenuCallback` are manually duplicated
- **Location**: `callbacks.rs:613–653` (`OptionCallback`), `callbacks.rs:4169–4204` (`OptionMenuCallback`)
- **Details**: Both follow identical patterns (None/Some variants, `into_option`, `is_some`, `is_none`, From impls). The `impl_option!` macro is used for simpler types but not here (perhaps because `Callback` is not `Copy`).
- **Recommendation**: Consider using `impl_option!` with `copy = false` (which is already used for `SelectAllResult` at line 126), or a shared macro to reduce boilerplate.

### [LOW] Module Doc — Present and adequate
- **Location**: `callbacks.rs:1–5`
- **Details**: The module has a `//!` doc comment explaining its purpose. It correctly states why callbacks live in `azul-layout` rather than `azul-core`.
- **Recommendation**: No change needed.

### [LOW] `remove_selection_by_id` always returns `true`
- **Location**: `callbacks.rs:1655–1659`
- **Details**: The method always returns `true` with comment "Actual removal happens deferred; assume success." This is technically correct for the transaction model but could mislead callers.
- **Recommendation**: Change return type to `()` since the return value is meaningless, or document the limitation prominently.

## System Documentation
- System identified: yes — Callback / event handling system
- Existing doc: `doc/guide/lifecycle.md` covers the event lifecycle partially
- Doc needed: A dedicated `doc/guide/callbacks.md` would be valuable, covering:
  - The transaction-based `CallbackChange` system
  - How `CallbackInfo` provides read-only queries + deferred mutations
  - The relationship between `Callback` / `CoreCallback` (usize storage for FFI)
  - How to use the text editing, clipboard, drag-drop, and scroll APIs from callbacks
