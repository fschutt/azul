# Review: css/src/dynamic_selector.rs

## Summary
- Lines: 1777
- Public functions: ~28 (including methods and free functions)
- Public structs/enums: 16 (PseudoStateFlags, DynamicSelector, MinMaxRange, BoolCondition, OsCondition, OsVersionCondition, OsVersion, OsFamily, LinuxDesktopEnv, MediaType, ThemeCondition, OrientationType, LanguageCondition, PseudoStateType, DynamicSelectorContext, CssPropertyWithConditions)
- Findings: 3 high, 1 medium, 0 low

## Findings

### [HIGH] Inconsistent Ord/PartialEq — Ord violates consistency requirement
- **Location**: `dynamic_selector.rs:938-954`
- **Details**: `PartialEq` is derived (compares all fields structurally), but `Ord` only compares `apply_if.len()`. This violates the Rust requirement that `a == b` implies `a.cmp(&b) == Equal` and vice versa. Two structurally different `CssPropertyWithConditions` with the same condition count will be `Ord::Equal` but `PartialEq::ne`. This can cause incorrect behavior in sorted collections and BTreeMaps.
- **Evidence**: Line 922: `#[derive(..., PartialEq)]` vs Lines 949-953: `self.apply_if.as_slice().len().cmp(&other.apply_if.as_slice().len())`
- **Recommendation**: Either implement `PartialEq` manually to match `Ord`, or implement a proper `Ord` that is consistent with derived `PartialEq`.

### [HIGH] Lossy Hash — CssPropertyWithConditions hashes only condition count
- **Location**: `dynamic_selector.rs:1112-1118`
- **Details**: The `Hash` impl hashes the property and the *length* of `apply_if`, but not the actual selector contents. Two properties with the same CSS property and same condition count but different conditions will produce the same hash, causing excessive hash collisions.
- **Evidence**: Line 1116: `self.apply_if.as_slice().len().hash(state);`
- **Recommendation**: Implement `Hash` for `DynamicSelector` and hash each element of `apply_if`, or document that this type should not be used as a HashMap/HashSet key.

### [HIGH] Duplicate Type — PseudoStateFlags duplicates StyledNodeState
- **Location**: `dynamic_selector.rs:10-24` vs `core/src/styled_dom.rs:186-206`
- **Details**: `PseudoStateFlags` has the exact same 10 boolean fields as `StyledNodeState`. The doc comment acknowledges this: "See azul_core::styled_dom::StyledNodeState for the main type." These must be kept in sync manually, creating a field-drift risk.
- **Evidence**: Both structs: hover, active, focused, disabled, checked, focus_within, visited, backdrop, dragging, drag_over.
- **Recommendation**: Consider a shared type or a compile-time layout assertion test to catch drift.

### [MEDIUM] Questionable OsVersion::unknown() Defaults to Linux
- **Location**: `dynamic_selector.rs:411-416`
- **Details**: `OsVersion::unknown()` uses `OsFamily::Linux` as the OS. An unknown version will compare as a Linux version with `version_id: 0`, which could produce incorrect cross-OS version comparisons. While `compare()` returns `None` for cross-OS comparisons, `is_at_least`/`is_at_most` will return `false` for cross-OS but `true/false` for same-OS Linux comparisons, even when the OS is genuinely unknown.
- **Evidence**: Line 413: `os: OsFamily::Linux` with comment "Fallback, but version_id 0 means unknown"
- **Recommendation**: Consider adding an `Unknown` variant to `OsFamily` or a dedicated `is_unknown()` check.

## System Documentation
- System identified: yes — CSS Dynamic Selectors (part of the CSS styling/evaluation pipeline)
- Existing doc: `doc/guide/css-styling.md` and `doc/guide/styling-system.md` exist but neither mentions dynamic selectors, `@os`, `@media`, `@container`, `@theme`, or conditional CSS features
- Doc needed: Dynamic selectors system needs documentation — either in `doc/guide/css-styling.md` (section on conditional/responsive styles) or a dedicated `doc/guide/dynamic-selectors.md` covering supported at-rules (`@os`, `@os-version`, `@media`, `@container`, `@theme`, `@lang`, `@prefers-reduced-motion`, `@prefers-high-contrast`) and pseudo-classes
