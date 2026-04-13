# Review: layout/src/icu_windows.rs

## Summary
- Lines: 596
- Public functions: 17 (on `IcuLocalizer`)
- Public structs/enums: 1 (`IcuLocalizer`)
- Findings: 0 high, 2 medium, 1 low

## Findings

### [MEDIUM] Missing Documentation — no doc comments on private helpers and internal types
- **Location**: `icu_windows.rs:22` (`HMODULE`), `icu_windows.rs:103` (`NlsFns`), `icu_windows.rs:116` (`nls()`), `icu_windows.rs:147` (`to_wide`), `icu_windows.rs:185` (`plural_for`), `icu_windows.rs:283-327` (`conjunction_and`, `conjunction_or`, `join_list`)
- **Details**: While these are private items (acceptable to lack docs), the public API methods on `IcuLocalizer` also have no doc comments. The macOS backend similarly lacks them, and the ICU4X backend (`icu.rs`) has full docs on every method. The Windows and macOS backends should match.
- **Recommendation**: Add brief doc comments to the public methods on `IcuLocalizer`, matching the style in `icu.rs`.

### [MEDIUM] Unsafe — `core::mem::transmute` for function pointer casts
- **Location**: `icu_windows.rs:132`
- **Details**: The `sym!` macro transmutes `*mut c_void` to typed function pointers. While this is necessary for `GetProcAddress` results, the transmute is unchecked — if the function signature doesn't match the actual Win32 export, this is UB. The signatures look correct against Win32 docs, but transmute is inherently dangerous.
- **Evidence**: Line 132: `unsafe { core::mem::transmute(ptr) }`.
- **Recommendation**: This is a standard pattern for dynamic loading and the signatures match Win32 declarations. No change needed, but consider adding a safety comment on each type alias confirming it matches the MSDN declaration.

### [LOW] Resource Leak — `LoadLibraryW` without `FreeLibrary`
- **Location**: `icu_windows.rs:120`
- **Details**: `LoadLibraryW("kernel32.dll")` bumps the refcount on kernel32 but never calls `FreeLibrary`. The comment on line 118 explains this is intentional (kernel32 is always mapped), and the handle lives in a `OnceLock` for the process lifetime. This is correct behavior — just noting for completeness.
- **Recommendation**: None needed. The comment adequately explains the intent.

## System Documentation
- System identified: yes — ICU / internationalization system
- Existing doc: none (no `doc/guide/internationalization.md` or similar)
- Doc needed: A guide document covering the ICU system architecture — how the three backends (ICU4X, macOS Foundation, Windows NLS) are selected via feature flags, the `IcuLocalizer` / `IcuLocalizerHandle` API, and how locale data flows through the system. This file is one of three platform backends behind `layout/src/icu.rs`.
