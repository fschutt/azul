# Review: dll/src/desktop/shell2/macos/clipboard.rs

## Summary
- Lines: 123
- Public functions: 3 (`sync_clipboard`, `get_clipboard_content`, `write_to_clipboard`)
- Public structs/enums: 1 (`ClipboardError`)
- Findings: 1 high, 0 medium, 1 low

## Findings

### [HIGH] Dead code — `sync_clipboard` is never called from macOS window code
- **Location**: `clipboard.rs:27`
- **Details**: `sync_clipboard` is defined but never called from `macos/mod.rs`. Other platforms (X11, Wayland, Windows) call their `clipboard::sync_clipboard` from their respective window modules, but the macOS module only declares `pub mod clipboard;` without ever invoking `sync_clipboard`. Only `get_clipboard_content` and `write_to_clipboard` are called (from `common/event.rs:258,284`).
- **Evidence**: `grep "clipboard::sync_clipboard\|clipboard::get_clipboard\|clipboard::write_to"` in `macos/mod.rs` returns no matches. The function has zero call sites outside its own file.
- **Recommendation**: Either wire `sync_clipboard` into the macOS event loop (as other platforms do) or remove it if macOS clipboard sync uses a different mechanism via `get_clipboard_content`/`write_to_clipboard`.

### [LOW] Code style — `write_to_clipboard` is a trivial wrapper
- **Location**: `clipboard.rs:52-54`
- **Details**: `write_to_clipboard` just delegates to `write_to_pasteboard` with no added logic. It exists as a public API name normalization, which is fine, but could be simplified to a `pub use` or inline.
- **Recommendation**: Minor — acceptable as-is for API consistency across platforms.

## System Documentation
- System identified: yes — clipboard / windowing system (platform-specific clipboard integration)
- Existing doc: none (no clipboard or windowing guide in `doc/guide/`)
- Doc needed: A windowing/platform-integration guide covering clipboard, IME, accessibility, and other per-platform subsystems would help. Multiple platform clipboard files exist (`macos/clipboard.rs`, `windows/clipboard.rs`, `x11/clipboard.rs`, `wayland/clipboard.rs`) following the same pattern.
