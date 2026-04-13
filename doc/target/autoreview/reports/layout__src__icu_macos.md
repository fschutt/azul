# Review: layout/src/icu_macos.rs

## Summary
- Lines: 480
- Public functions: 18 (on `IcuLocalizer`)
- Public structs/enums: 1 (`IcuLocalizer`)
- Findings: 0 high, 0 medium, 1 low

## Findings

### [LOW] `set_locale` always returns `true`
- **Location**: `icu_macos.rs:228-231`
- **Details**: `set_locale` always returns `true` regardless of input. No validation
  is performed on the locale string. The ICU4X backend (`icu.rs:475`) at least
  attempts to parse the locale and returns `false` on failure.
- **Recommendation**: Either validate the locale string or change the return type to
  `()` if validation is not needed on this platform.

## System Documentation
- System identified: yes — ICU / Localization system (number formatting, plural rules,
  date/time formatting, list formatting, locale-aware collation)
- Existing doc: none (`doc/guide/` has no ICU or localization guide)
- Doc needed: A `doc/guide/localization.md` covering the three-backend ICU architecture
  (ICU4X, macOS Foundation, Windows NLS), how `IcuLocalizerHandle` dispatches to
  per-locale `IcuLocalizer` instances, the CLDR plural rules implementation, and how
  the system integrates with the window/callback infrastructure.
