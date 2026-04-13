# Review: layout/src/text3/script.rs

## Summary
- Lines: 876
- Public functions: 28 (detect_script, detect_char_script, script_to_language, is_stop_char, 24 is_* character check functions)
- Public structs/enums: 2 (Script, Language — Language is conditional on feature flag)
- Findings: 1 high, 2 medium, 2 low

## Findings

### [HIGH] Questionable Language Fallbacks — `script_to_language` maps scripts to unrelated languages
- **Location**: `script.rs:700-709`
- **Details**: Several script-to-language mappings are semantically wrong:
  - `Script::Myanmar => Language::Thai` — Myanmar script is for Burmese, not Thai
  - `Script::Khmer => Language::Thai` — Khmer is Cambodian, not Thai
  - `Script::Sinhala => Language::Hindi` — Sinhala is Sri Lankan, not Hindi
  - `Script::Arabic => Language::Chinese` — Arabic mapped to Chinese
  - `Script::Hebrew => Language::Chinese`
  - `Script::Hangul => Language::Chinese`
  - `Script::Hiragana => Language::Chinese`
  - `Script::Katakana => Language::Chinese`

  These are clearly used as "no hyphenation" fallback values since the `Language` enum (from the `hyphenation` crate) lacks variants for these languages. However, this is confusing and fragile — if the hyphenation crate adds support for any of these languages, these mappings would silently produce wrong results.
- **Evidence**: Comments on lines 699 and 704-705 acknowledge this: "not directly matchable" and "no classical hyphenation behaviour".
- **Recommendation**: Add doc comments on `script_to_language` explaining the fallback strategy explicitly. Consider returning `Option<Language>` instead of mapping to unrelated languages, letting callers decide on default behavior.

### [MEDIUM] Missing Module-Level Documentation
- **Location**: `script.rs:1`
- **Details**: The file has no `//!` module doc comment. The top of the file has license/attribution comments but nothing explaining the module's purpose. This module provides Unicode script detection and script-to-language mapping for text shaping and hyphenation.
- **Recommendation**: Add a `//!` block explaining: what the module does (script detection for text layout), key exports (`Script`, `Language`, `detect_script`, `script_to_language`), and its role in the text3 pipeline.

### [MEDIUM] Missing Documentation on Public Functions
- **Location**: Multiple
- **Details**: Most public `is_*` functions lack doc comments. `detect_char_script` (line 518), `script_to_language` (line 680), and `is_stop_char` (line 435) also lack docs. Only `detect_script` (line 442) and `get_unicode_ranges` (line 133) have doc comments.
- **Recommendation**: Add brief doc comments to `detect_char_script`, `script_to_language`, and `is_stop_char` at minimum. The `is_*` functions are self-explanatory but `script_to_language` especially needs documentation about its fallback behavior.

### [LOW] Individual `is_*` Functions — No External Callers
- **Location**: `script.rs:713-875` (all `is_*` functions)
- **Details**: The 24 individual `is_*` functions (e.g., `is_latin`, `is_cyrillic`, etc.) are only used within `script.rs` itself (in the `script_counters` arrays and `detect_*_language` functions) and in `tests/src/script.rs`. They are `pub` but have no callers in the rest of the codebase.
- **Evidence**: Grep for `is_latin|is_cyrillic|...` across `*.rs` returns only `script.rs`, `cache.rs` (which has its own `is_arabic_cluster` — different function), and `tests/src/script.rs`.
- **Recommendation**: Consider making these `pub(crate)` or `pub(super)` since they appear to be internal implementation details. If they are part of the intended public API for external consumers, document them.

### [LOW] `detect_latin_language` — Accented Characters Map to Spanish Instead of Setting Flags
- **Location**: `script.rs:657`
- **Details**: `'á' | 'é' | 'í' | 'ó' | 'ú' => return Language::Spanish` immediately returns Spanish for any text containing common Romance accent characters. These characters appear in French, Portuguese, Italian, and many other Latin-script languages. Text like "café" (French) would be detected as Spanish.
- **Recommendation**: This is inherited from the upstream whatlang-rs library and is a known limitation of the heuristic approach. Worth a comment noting the limitation.

## System Documentation
- System identified: yes — text shaping / text layout pipeline (`text3` module)
- Existing doc: none (no guide doc for text shaping/layout)
- Doc needed: A `doc/guide/text-shaping.md` or `doc/guide/text-layout.md` guide explaining the text3 pipeline: script detection → language detection → font selection → shaping → line breaking → glyph positioning. This module handles the first two stages.
