//! Built-in MOCK FONTS with fully controlled metrics.
//!
//! # Why these exist
//!
//! Every text assertion made against a *system* font is a guess: the engine
//! has no control over Arial's advances, and on a CI box Arial may not even
//! exist (see `register_named_font` — a family that fontconfig cannot find
//! silently falls back, which is exactly how the "8 families → 2 FontIds"
//! bug hid for so long). So text tests degenerate into "roughly this wide".
//!
//! The mock fonts fix that by making text layout ARITHMETIC:
//!
//! | family            | advance | ascent | descent | glyphs        |
//! |-------------------|---------|--------|---------|---------------|
//! | `Azul Mock Mono`  | 0.5 em  | 0.8 em | 0.2 em  | ASCII 0x20-7E |
//! | `Azul Mock Wide`  | 1.0 em  | 0.8 em | 0.2 em  | ASCII 0x20-7E |
//!
//! At `font-size: 20px`, `Azul Mock Mono` advances exactly 10 px per glyph:
//! a 5-character string is exactly 50 px wide and its line box is exactly
//! 20 px tall. Caret offsets, selection rectangles, line-break positions and
//! bidi run widths become exact integers a test can write down.
//!
//! # Registration path
//!
//! These are registered as ordinary rust-fontconfig **memory fonts** in the
//! shared [`rust_fontconfig::FcFontCache`] (see
//! [`crate::text3::cache::FontManager::register_named_font`]) — the same
//! mechanism an embedder uses for a bundled font. They therefore travel the
//! *real* resolution path: CSS `font-family` → font-stack collection →
//! chain resolution → `FontId` → `load_missing_for_chains` → shaping. There
//! is no test-only bypass, which is the point: a test using them exercises
//! font resolution rather than skipping it.
//!
//! # Adding more mock fonts
//!
//! `scripts/gen_mock_fonts.py` builds the `.ttf`s (no third-party deps). Add
//! an entry to its `FONTS` list — family name, upem, advance, ascent,
//! descent, codepoint range — re-run it, commit the `.ttf`, and add it to
//! [`BUILTIN_MOCK_FONTS`] below. An RTL mock is the same call with a
//! Hebrew/Arabic range; a missing-glyph mock is the same call with a
//! truncated range (uncovered chars then take the real fallback path); a
//! proportional mock is the same call with a wider advance.

use rust_fontconfig::UnicodeRange;

/// `Azul Mock Mono`: every ASCII glyph advances 0.5 em (10 px at 20 px).
pub const MOCK_MONO_TTF: &[u8] = include_bytes!("../../../assets/fonts/test/azul-mock-mono.ttf");

/// `Azul Mock Wide`: every ASCII glyph advances 1.0 em (20 px at 20 px).
pub const MOCK_WIDE_TTF: &[u8] = include_bytes!("../../../assets/fonts/test/azul-mock-wide.ttf");

/// Codepoints the mock fonts cover (printable ASCII). Anything outside this
/// range is deliberately *not* covered, so it exercises real fallback.
#[must_use]
pub fn mock_font_ranges() -> Vec<UnicodeRange> {
    vec![UnicodeRange {
        start: 0x20,
        end: 0x7E,
    }]
}

/// The mock fonts registered into every `FontManager`: `(family, bytes)`.
///
/// Registering them unconditionally (rather than behind a test-only flag)
/// is intentional: it keeps the production and test font paths identical,
/// costs ~10 KiB, and the families are only reachable if a stylesheet asks
/// for them by name.
pub const BUILTIN_MOCK_FONTS: &[(&str, &[u8])] = &[
    ("Azul Mock Mono", MOCK_MONO_TTF),
    ("Azul Mock Wide", MOCK_WIDE_TTF),
];

/// Advance of one glyph of `family` at `font_size_px`, or `None` if the
/// family is not a mock font. Test helper: lets a test compute the expected
/// width of a string without hardcoding the em fraction twice.
#[must_use]
pub fn mock_advance_px(family: &str, font_size_px: f32) -> Option<f32> {
    match family {
        "Azul Mock Mono" => Some(font_size_px * 0.5),
        "Azul Mock Wide" => Some(font_size_px),
        _ => None,
    }
}
