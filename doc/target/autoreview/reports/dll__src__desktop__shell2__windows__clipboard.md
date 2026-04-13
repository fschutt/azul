# Review: dll/src/desktop/shell2/windows/clipboard.rs

## Summary
- Lines: 33
- Public functions: 2
- Public structs/enums: 0
- Findings: 1 high, 1 medium, 1 low

## Findings

### [HIGH] Dead Code — `sync_clipboard` has zero call sites on Windows
- **Location**: `clipboard.rs:12`
- **Details**: `sync_clipboard` is defined but never called anywhere in the Windows platform code. On Linux, `sync_clipboard` is wired through `x11/mod.rs:583` and `wayland/mod.rs:566`, but no equivalent call exists for Windows. The `scripts/PLATFORM_WINDOW_REFACTORING.md:70` explicitly notes: "`sync_clipboard()` — Never called from run.rs (handled internally)". Meanwhile, `common/event.rs:279-280` calls `clipboard_win::set_clipboard` directly instead of routing through this function.
- **Evidence**: `grep 'sync_clipboard' dll/src/desktop/shell2/windows/` returns only the definition at `clipboard.rs:12`. Zero call sites.
- **Recommendation**: Either wire `sync_clipboard` into the Windows event loop (matching what Linux does), or remove it and consolidate clipboard-set logic into `common/event.rs:set_system_clipboard`.

### [MEDIUM] File Size — 33 lines, candidate for merging
- **Location**: entire file
- **Details**: The file contains only 33 lines with two small functions. The macOS clipboard module (`macos/clipboard.rs`) has substantially more logic (pasteboard interaction, multiple helper functions). This Windows module is thin enough to be inlined into the parent `windows/mod.rs` or consolidated with the event.rs clipboard logic.
- **Recommendation**: Consider merging into `windows/mod.rs` or consolidating clipboard write/read into `common/event.rs` where the platform dispatch already lives.

### [LOW] Unconditional `clear()` in `sync_clipboard`
- **Location**: `clipboard.rs:22`
- **Details**: `clipboard_manager.clear()` clears both paste and copy content, even though this function only handles copy. If paste content is pending, it would be lost. The Linux clipboard modules also call `clear()`, so this is consistent across platforms, but it's worth noting as a potential gotcha.
- **Recommendation**: Consider using `clear_copy()` instead of `clear()` if paste content should be preserved.

## System Documentation
- System identified: yes — Clipboard / Windowing system (platform integration)
- Existing doc: none (no clipboard guide in `doc/guide/`)
- Doc needed: A `doc/guide/clipboard.md` covering the clipboard architecture (ClipboardManager, platform sync functions, copy/paste/cut flows, how `common/event.rs` dispatches to platform modules)
