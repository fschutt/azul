# Review: layout/src/icu.rs

## Summary
- Lines: 1693
- Public functions: ~60 (across IcuLocalizer, IcuLocalizerHandle, LayoutCallbackInfoIcuExt trait)
- Public structs/enums: 11 (IcuError, IcuResult, PluralCategory, ListType, DateTimeFieldSet, FormatLength, IcuDate, IcuTime, IcuDateTime, IcuLocalizerInner, IcuLocalizerHandle) + 1 trait + 1 type alias
- Findings: 1 high, 3 medium, 1 low

## Findings

### [HIGH] Unsafe Code — Manual reference counting in `IcuLocalizerHandle`
- **Location**: `icu.rs:926-961`
- **Details**: Manual `Arc`-like reference counting with raw pointers. Several issues:
  1. `Drop` sets `self.run_destructor = false` (line 952) but `run_destructor` is never checked — it's set but never read to guard anything. This field appears vestigial from FFI but is meaningless in the current Rust code.
  2. `clone()` uses `as_ref()` which returns `Option` — if `copies` is null, the `fetch_add` is silently skipped but `run_destructor` is still set to `true`, leading to a double-free when the clone is dropped.
  3. No protection against cloning after the refcount reaches 0 (use-after-free if a stale handle is cloned).
- **Evidence**: Lines 936-961. The `run_destructor` field is written at lines 945, 952, 983, 997 but never read in any conditional.
- **Recommendation**: Use `Arc<IcuLocalizerInner>` instead of manual refcounting. The FFI boundary can use `Arc::into_raw` / `Arc::from_raw`. If manual refcounting is required for C API reasons, add null checks and remove the dead `run_destructor` field.

### [MEDIUM] Dead Code — `DateTimeFieldSet` enum is unused
- **Location**: `icu.rs:156-169`
- **Details**: The `DateTimeFieldSet` enum is defined but never used in any logic. The actual datetime formatting in `format_date`/`format_time`/`format_datetime` uses `FormatLength` and ICU4X's own `YMD`/`T` fieldsets directly.
- **Evidence**: Grep for `DateTimeFieldSet` across the codebase returns only the definition in `icu.rs` and a re-export in `lib.rs`. No function accepts or matches on it.
- **Recommendation**: Remove the enum or integrate it into the formatting API if the intent was to support more field combinations.

### [MEDIUM] Performance — `LayoutCallbackInfoIcuExt` creates a new `IcuLocalizerHandle` on every call
- **Location**: `icu.rs:1476-1573`
- **Details**: Every method in the `LayoutCallbackInfoIcuExt` impl creates a brand-new `IcuLocalizerHandle` via `IcuLocalizerHandle::from_system_language(...)`, which allocates a new cache, uses it once, then drops it. This means:
  1. No caching benefit — formatters are created and destroyed on every call.
  2. Two heap allocations + deallocations per call (for the `Box<IcuLocalizerInner>` and `Box<AtomicUsize>`).
  3. If a user formats a date and a number in the same layout callback, formatters are created twice.
- **Evidence**: All 13 method impls at lines 1476-1573 follow the same pattern.
- **Recommendation**: Store the `IcuLocalizerHandle` in `LayoutCallbackInfo` (or retrieve a shared one from the application state) instead of creating a throwaway instance per call.

### [MEDIUM] Lossy casts — `as u8` for chrono values
- **Location**: `icu.rs:245-246`, `icu.rs:257-258`, `icu.rs:275-277`, `icu.rs:287-289`, `icu.rs:340-346`
- **Details**: `chrono::month()` returns `u32`, cast to `u8`. While months (1-12) and days (1-31) always fit in `u8`, hours/minutes/seconds also fit. These are technically safe but `try_into().unwrap()` would be more idiomatic and wouldn't silently mask a chrono API change.
- **Evidence**: 15 occurrences of `as u8` in chrono conversion code.
- **Recommendation**: Low priority — consider `u8::try_from(x).unwrap()` for defense-in-depth, or leave as-is since values are bounded by chrono's guarantees.

### [LOW] TODO in code — `ListType::Unit` fallback
- **Location**: `icu.rs:674`
- **Details**: Comment says `// TODO: Use ListFormatter::try_new_unit when available` and falls back to a simple comma join. This means `ListType::Unit` formatting is not locale-aware.
- **Recommendation**: Track as a known limitation. Check if `ListFormatter::try_new_unit` is now available in the ICU4X version being used.

## System Documentation
- System identified: yes — internationalization / ICU localization system
- Existing doc: none (no i18n or icu guide in `doc/guide/`)
- Doc needed: A guide document covering the ICU/i18n system — how localization works, the three backends (ICU4X, macOS Foundation, Windows NLS), the `IcuLocalizerHandle` caching layer, and how to use the `LayoutCallbackInfoIcuExt` trait in layout callbacks.
