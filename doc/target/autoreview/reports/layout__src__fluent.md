# Review: layout/src/fluent.rs

## Summary
- Lines: 904
- Public functions: 5 (`check_fluent_syntax`, `check_fluent_syntax_bytes`, `create_fluent_zip`, `create_fluent_zip_from_strings`, `export_to_zip`)
- Public structs/enums: 5 (`FluentError`, `FluentSyntaxCheckResult`, `FluentLanguageInfo`, `FluentZipLoadResult`, `FluentLocalizerHandle`)
- Public traits: 0
- Public type aliases: 1 (`FluentLanguageInfoVec`)
- Findings: 1 high, 1 medium, 2 low

## Findings

### [HIGH] Lossy Type Conversion — `u64`/`i64` cast to `f64`
- **Location**: `fluent.rs:222-227`
- **Details**: `FmtValue::Slong(n)` and `FmtValue::Ulong(n)` are cast via `*n as f64`. Since `f64` has only 53 bits of mantissa precision, values above 2^53 (for u64, roughly 9 × 10^15) will silently lose precision. This is a known bug pattern (lossy type conversions).
- **Evidence**: Lines 223 (`FmtValue::Slong(n) => ... *n as f64`), 226 (`FmtValue::Ulong(n) => ... *n as f64`).
- **Recommendation**: For integer values that may exceed 2^53, consider using `FluentValue::from` with a string representation of the number, or document the precision limitation.

### [MEDIUM] Unsafe Code — Manual reference-counted handle without safety documentation
- **Location**: `fluent.rs:301-336`
- **Details**: `FluentLocalizerHandle` implements manual reference counting with raw pointers (`ptr`, `copies`) and `unsafe impl Send + Sync`. The `Drop` impl (line 326) sets `run_destructor = false` on every drop — this field appears to serve no purpose since it's set unconditionally. The `Clone` impl uses `as_ref()` which could return `None` if `copies` is null, silently skipping the increment (creating a double-free).
- **Evidence**: Line 327: `self.run_destructor = false;` — unconditional, making the field unused. Line 313: `.as_ref().map(...)` — if copies is null, ref count is not incremented but a clone is still returned.
- **Recommendation**: Document the safety invariants. Remove `run_destructor` if unused. Assert that `copies` is non-null rather than silently ignoring null.

### [LOW] Poisoned Mutex Handling — silently swallowing lock failures
- **Location**: Throughout the file (lines 385, 401, 418, 664, 696, 705, 712, 719, 732, 739)
- **Details**: All `Mutex::lock()` calls use `if let Ok(...)` or `.ok()`, silently ignoring poisoned mutex states. If a panic occurs while a lock is held, all subsequent operations will silently fail or return defaults.
- **Recommendation**: Consider using `.lock().expect("...")` or at least logging when a mutex is poisoned, as silent failures can be very hard to debug.

### [LOW] Documentation Verbosity — Some doc comments on simple methods are excessive
- **Location**: `fluent.rs:438-467` (`load_from_zip_with_locale`)
- **Details**: The ZIP structure examples in the doc comments are helpful but the overall doc block is 30 lines for a method whose behavior is straightforward. This is borderline — the examples are useful for FFI consumers.
- **Recommendation**: No change needed; the examples justify the length.

## System Documentation
- System identified: yes — Localization / i18n system (Project Fluent integration)
- Existing doc: none (no guide covers localization/i18n/fluent)
- Doc needed: A `doc/guide/localization.md` covering the Fluent-based localization system, how to load translations, the fallback chain, ZIP archive format, and integration with the widget system via `FmtArgVec`.
