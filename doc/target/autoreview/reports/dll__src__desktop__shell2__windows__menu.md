# Review: dll/src/desktop/shell2/windows/menu.rs

## Summary
- Lines: 168
- Public functions: 2 (`WindowsMenuBar::new`, `recursive_construct_menu`, `set_menu_bar`)
- Public structs/enums: 1 (`WindowsMenuBar`)
- Findings: 0 high, 1 medium, 0 low

## Findings

### [MEDIUM] Duplicated Functionality — multiple UTF-8→UTF-16 functions across codebase

- **Location**: `dlopen.rs:144`, `layout/src/icu_windows.rs:147`
- **Details**: The codebase has at least 2 remaining UTF-8→UTF-16 conversion functions (menu.rs now uses `dlopen::encode_wide`, and the duplicate in `mod.rs` was removed). Consider consolidating the remaining `layout/src/icu_windows.rs` duplicate in a follow-up.
- **Recommendation**: Consolidate remaining duplicates across codebase.

## System Documentation
- System identified: yes — Windows windowing / shell system (`dll/src/desktop/shell2/windows/`)
- Existing doc: none (no `doc/guide/` document covers the windowing shell system)
- Doc needed: A windowing system guide covering the shell2 architecture (platform-specific window creation, event loops, menu handling, tooltips, clipboard, accessibility) would benefit developers working across the platform backends.
