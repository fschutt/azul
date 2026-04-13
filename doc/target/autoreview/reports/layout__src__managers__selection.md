# Review: layout/src/managers/selection.rs

## Summary
- Lines: 93
- Public functions: 1
- Public structs/enums: 2 (`StyledTextRun`, `ClipboardContent`)
- Findings: 1 high, 1 medium, 1 low

## Findings

### [HIGH] Dead Code — `StyledTextRunVec` / `OptionStyledTextRun` / `OptionClipboardContent` FFI types partially unused

- **Location**: `selection.rs:449-470`
- **Details**: `StyledTextRunVec` is only referenced in `api.json` and `selection.rs` itself (definition). `OptionClipboardContent` is also only in `api.json` and `selection.rs`. These FFI wrapper types exist for the C API but `to_html()` (the only method using styled runs richly) has no call sites in production code.
- **Evidence**: `grep -r 'StyledTextRunVec' --include='*.rs'` returns only `selection.rs`. `grep -r 'OptionClipboardContent' --include='*.rs'` returns only `selection.rs`. Both appear in `api.json` for FFI generation.
- **Recommendation**: Keep if needed for FFI/C API stability. Flag as potentially dead if the C API isn't shipping these types yet.

### [MEDIUM] Misplaced Types — `ClipboardContent` / `StyledTextRun` belong in `clipboard.rs`

- **Location**: `selection.rs:432-513`
- **Details**: `ClipboardContent` and `StyledTextRun` are clipboard-related types that are imported by `clipboard.rs:28` and `changeset.rs:32` via `use crate::managers::selection::ClipboardContent`. They have no logical relationship with selection management.
- **Recommendation**: Move `ClipboardContent`, `StyledTextRun`, and their FFI macro invocations to `clipboard.rs` where they semantically belong.

### [LOW] `#[repr(C)]` on types that may be dead

- **Location**: `selection.rs:433` (`StyledTextRun`), `selection.rs:457` (`ClipboardContent`)
- **Details**: Both types have `#[repr(C)]` for FFI. If these types are not actually exposed through the C API boundary, the repr is unnecessary overhead. They are referenced in `api.json` so this may be intentional.
- **Recommendation**: Verify these are actually used in the generated C API. If not, remove `#[repr(C)]`.

## System Documentation
- System identified: yes — Text Selection / Input system
- Existing doc: none (no dedicated selection/text-input guide in `doc/guide/`)
- Doc needed: A guide covering the text input, selection, and cursor management system — how `TextEditManager`, `multi_cursor`, `CursorManager`, and clipboard interact. The current `selection.rs` is largely dead; the real system lives in `text_edit.rs` and `focus_cursor.rs`.
