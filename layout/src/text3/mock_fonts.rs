//! Built-in MOCK FONTS with fully controlled metrics.
//!
//! # Why these exist
//!
//! Every text assertion made against a *system* font is a guess: the engine
//! has no control over Arial's advances, and on a CI box Arial may not even
//! exist (see `register_named_font` — a family that fontconfig cannot find
//! silently falls back, which is exactly how the "8 families → 2 `FontIds`"
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
//! # Every glyph draws DIFFERENT ink
//!
//! Metrics are uniform; ink is not. Each glyph is the same box with a
//! codepoint-derived bite out of its interior (a constant frame plus a 3x3
//! grid of cells, cell `k` kept iff bit `k` of `codepoint - 0x20` is clear —
//! see `scripts/gen_mock_fonts.py`). All 94 inked glyphs therefore rasterise
//! to distinct pixels.
//!
//! This is load-bearing, not decoration. When every glyph was the identical
//! filled rectangle, two equal-length strings rendered bit-identically, so a
//! text edit like `"tick 1"` -> `"tick 2"` produced damage but no pixel
//! change. Any scenario combining a mock font with a pixel-liveness assertion
//! (`assert_changed`, `assert_damage_covers_changes`,
//! `assert_damage_sound{pixel_identity}`) was then asserting something the
//! font made impossible; the ones that passed did so only because their two
//! strings happened to differ in LENGTH.
//!
//! The frame is what keeps this free: it touches all four sides of the glyph
//! box, so the tight bounding box — and every metric derived from it — is
//! exactly what it was when the box was solid.
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
//! # Regenerating, and adding more mock fonts
//!
//! The `.ttf`s are built by a committed generator, not by hand:
//!
//! ```text
//! python3 scripts/gen_mock_fonts.py
//! ```
//!
//! It rewrites both copies — `assets/fonts/test/` (canonical) and the vendored
//! `layout/assets/fonts/test/` that `include_bytes!` below actually reads,
//! since that macro cannot reach outside the crate root — and it is
//! deterministic: every byte is derived from the `FONTS` table and the
//! codepoint, with no hashing, timestamps or iteration order. Re-running it
//! without changing the script leaves the working tree clean, so a diff after
//! a re-run means the committed fonts are stale.
//!
//! The script needs no third-party deps (the repo cannot assume `fonttools`).
//! To add a font, add an entry to its `FONTS` list — family name, upem,
//! advance, ascent, descent, codepoint range — re-run it, commit the `.ttf`,
//! and add it to [`BUILTIN_MOCK_FONTS`] below. An RTL mock is the same call with a
//! Hebrew/Arabic range; a missing-glyph mock is the same call with a
//! truncated range (uncovered chars then take the real fallback path); a
//! proportional mock is the same call with a wider advance.

use rust_fontconfig::UnicodeRange;

/// `Azul Mock Mono`: every ASCII glyph advances 0.5 em (10 px at 20 px).
pub const MOCK_MONO_TTF: &[u8] = include_bytes!("../../assets/fonts/test/azul-mock-mono.ttf");

/// `Azul Mock Wide`: every ASCII glyph advances 1.0 em (20 px at 20 px).
pub const MOCK_WIDE_TTF: &[u8] = include_bytes!("../../assets/fonts/test/azul-mock-wide.ttf");

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
/// costs ~30 KiB, and the families are only reachable if a stylesheet asks
/// for them by name.
pub const BUILTIN_MOCK_FONTS: &[(&str, &[u8])] = &[
    ("Azul Mock Mono", MOCK_MONO_TTF),
    ("Azul Mock Wide", MOCK_WIDE_TTF),
];

/// Advance of one glyph of `family` at `font_size_px`, or `None` if the
/// family is not a mock font.
///
/// Test helper: lets a test compute the expected width of a string without
/// hardcoding the em fraction twice.
#[must_use]
pub fn mock_advance_px(family: &str, font_size_px: f32) -> Option<f32> {
    match family {
        "Azul Mock Mono" => Some(font_size_px * 0.5),
        "Azul Mock Wide" => Some(font_size_px),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod autotest_generated {
    use super::*;

    // ---------------------------------------------------------------------
    // Minimal, panic-free sfnt reader used by the round-trip tests below.
    //
    // Deliberately written with `get()`/`Option` everywhere: a test helper
    // that panics on malformed input would report "the font is broken" as a
    // helper bug, and worse, an out-of-bounds index would abort the test
    // process rather than fail one assertion.
    // ---------------------------------------------------------------------

    fn be_u16(data: &[u8], off: usize) -> Option<u16> {
        let b = data.get(off..off.checked_add(2)?)?;
        Some(u16::from_be_bytes([b[0], b[1]]))
    }

    fn be_i16(data: &[u8], off: usize) -> Option<i16> {
        be_u16(data, off).map(|v| v as i16)
    }

    fn be_u32(data: &[u8], off: usize) -> Option<u32> {
        let b = data.get(off..off.checked_add(4)?)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Locate a top-level sfnt table by tag, clamped to the file end.
    fn sfnt_table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
        let num_tables = be_u16(data, 4)? as usize;
        for i in 0..num_tables {
            let rec = 12usize.checked_add(i.checked_mul(16)?)?;
            let record = data.get(rec..rec.checked_add(16)?)?;
            if record.get(..4)? == &tag[..] {
                let off = be_u32(data, rec + 8)? as usize;
                let len = be_u32(data, rec + 12)? as usize;
                let end = off.checked_add(len)?.min(data.len());
                return data.get(off..end);
            }
        }
        None
    }

    /// `(units_per_em, ascender, descender, num_glyphs, advances)` —
    /// `advances` holds one entry per `numberOfHMetrics`.
    fn font_metrics(data: &[u8]) -> Option<(u16, i16, i16, u16, Vec<u16>)> {
        let head = sfnt_table(data, b"head")?;
        let hhea = sfnt_table(data, b"hhea")?;
        let maxp = sfnt_table(data, b"maxp")?;
        let hmtx = sfnt_table(data, b"hmtx")?;

        let upem = be_u16(head, 18)?;
        let ascender = be_i16(hhea, 4)?;
        let descender = be_i16(hhea, 6)?;
        let num_h_metrics = be_u16(hhea, 34)? as usize;
        let num_glyphs = be_u16(maxp, 4)?;

        let mut advances = Vec::with_capacity(num_h_metrics);
        for i in 0..num_h_metrics {
            advances.push(be_u16(hmtx, i.checked_mul(4)?)?);
        }
        Some((upem, ascender, descender, num_glyphs, advances))
    }

    /// Codepoint coverage from the first `cmap` format-4 subtable, with the
    /// mandatory `0xFFFF` sentinel segment dropped.
    fn cmap_coverage(data: &[u8]) -> Option<Vec<(u32, u32)>> {
        let cmap = sfnt_table(data, b"cmap")?;
        let num_encodings = be_u16(cmap, 2)? as usize;
        for i in 0..num_encodings {
            let rec = 4usize.checked_add(i.checked_mul(8)?)?;
            let sub_off = be_u32(cmap, rec.checked_add(4)?)? as usize;
            let sub = cmap.get(sub_off..)?;
            if be_u16(sub, 0)? != 4 {
                continue;
            }
            let seg_count_x2 = be_u16(sub, 6)? as usize;
            let seg_count = seg_count_x2 / 2;
            let mut out = Vec::with_capacity(seg_count);
            for s in 0..seg_count {
                let end = be_u16(sub, 14usize.checked_add(s.checked_mul(2)?)?)?;
                let start = be_u16(sub, 16usize.checked_add(seg_count_x2)?.checked_add(s * 2)?)?;
                if start == 0xFFFF && end == 0xFFFF {
                    continue; // required terminator, not real coverage
                }
                out.push((u32::from(start), u32::from(end)));
            }
            return Some(out);
        }
        None
    }

    /// `name` table nameID 1 (family) for the Windows/UTF-16BE record.
    fn family_name(data: &[u8]) -> Option<String> {
        let name = sfnt_table(data, b"name")?;
        let count = be_u16(name, 2)? as usize;
        let string_off = be_u16(name, 4)? as usize;
        for i in 0..count {
            let rec = 6usize.checked_add(i.checked_mul(12)?)?;
            let platform = be_u16(name, rec)?;
            let name_id = be_u16(name, rec.checked_add(6)?)?;
            if platform != 3 || name_id != 1 {
                continue;
            }
            let len = be_u16(name, rec.checked_add(8)?)? as usize;
            let off = be_u16(name, rec.checked_add(10)?)? as usize;
            let start = string_off.checked_add(off)?;
            let bytes = name.get(start..start.checked_add(len)?)?;
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            return String::from_utf16(&units).ok();
        }
        None
    }

    /// Family names that must NOT resolve — near misses of the two real ones.
    const NEAR_MISSES: &[&str] = &[
        "",
        " ",
        "Azul",
        "Azul Mock",
        "Azul Mock ",
        " Azul Mock Mono",
        "Azul Mock Mono ",
        "Azul Mock Mon",
        "Azul Mock Monospace",
        "Azul Mock Wid",
        "Azul Mock Wider",
        "Azul  Mock Mono",
        "Azul Mock  Mono",
        "AzulMockMono",
        "Azul\tMock Mono",
        "Azul\nMock Mono",
        "Azul-Mock-Mono",
        "azul mock mono",
        "AZUL MOCK MONO",
        "Azul mock Mono",
        "\"Azul Mock Mono\"",
        "'Azul Mock Mono'",
        "Azul Mock Mono;garbage",
        "Azul Mock Mono, sans-serif",
        "sans-serif",
        "monospace",
        "Arial",
        "Azul Mock Mono\u{0}",
        "\u{0}Azul Mock Mono",
        "Azul Mock Mono\u{FEFF}",
    ];

    // ---------------------------------------------------------------------
    // mock_advance_px — positive control
    // ---------------------------------------------------------------------

    #[test]
    fn mock_advance_px_valid_minimal_matches_module_doc() {
        // The module header promises: at 20 px, Mono advances exactly 10 px
        // and Wide exactly 20 px, so a 5-char Mono string is exactly 50 px.
        assert_eq!(mock_advance_px("Azul Mock Mono", 20.0), Some(10.0));
        assert_eq!(mock_advance_px("Azul Mock Wide", 20.0), Some(20.0));
        assert_eq!(
            mock_advance_px("Azul Mock Mono", 20.0).unwrap() * 5.0,
            50.0,
            "5-char Mono string must be exactly 50 px at 20 px"
        );
    }

    #[test]
    fn mock_advance_px_accepts_every_builtin_family() {
        for (family, _) in BUILTIN_MOCK_FONTS {
            assert!(
                mock_advance_px(family, 16.0).is_some(),
                "registered family {family:?} has no advance"
            );
        }
    }

    // ---------------------------------------------------------------------
    // mock_advance_px — malformed / hostile family strings
    // ---------------------------------------------------------------------

    #[test]
    fn mock_advance_px_empty_input_is_none() {
        assert_eq!(mock_advance_px("", 20.0), None);
        assert_eq!(mock_advance_px("", 0.0), None);
        assert_eq!(mock_advance_px("", f32::NAN), None);
    }

    #[test]
    fn mock_advance_px_whitespace_only_is_none() {
        for family in ["   ", "\t", "\n", "\r\n", "\t\n", "\u{A0}", "\u{2003}"] {
            assert_eq!(
                mock_advance_px(family, 20.0),
                None,
                "whitespace-only {family:?} must not resolve"
            );
        }
    }

    #[test]
    fn mock_advance_px_garbage_never_panics() {
        // Every byte value that is a valid single-char &str, plus some
        // classic parser-breaker payloads.
        for b in 0u8..=127 {
            let s = (b as char).to_string();
            assert_eq!(mock_advance_px(&s, 20.0), None);
        }
        for family in [
            "%s%s%s%n",
            "../../etc/passwd",
            "\u{0}\u{1}\u{2}\u{7F}",
            "\\x00\\xff",
            "{}[]()<>",
            "\u{FFFD}",
        ] {
            assert_eq!(mock_advance_px(family, 20.0), None, "{family:?}");
        }
    }

    #[test]
    fn mock_advance_px_near_miss_families_are_all_rejected() {
        for family in NEAR_MISSES {
            assert_eq!(
                mock_advance_px(family, 20.0),
                None,
                "{family:?} must not be treated as a mock font (matching is \
                 exact and case-sensitive: no trimming, no normalisation)"
            );
        }
    }

    #[test]
    fn mock_advance_px_extremely_long_input_terminates() {
        let huge = "a".repeat(1_000_000);
        assert_eq!(mock_advance_px(&huge, 20.0), None);

        // Valid name as a prefix of a 1M-char string: still not a match.
        let mut prefixed = String::from("Azul Mock Mono");
        prefixed.push_str(&"x".repeat(1_000_000));
        assert_eq!(mock_advance_px(&prefixed, 20.0), None);

        // The valid name repeated: not a match either.
        assert_eq!(
            mock_advance_px(&"Azul Mock Mono".repeat(50_000), 20.0),
            None
        );
    }

    #[test]
    fn mock_advance_px_unicode_input_is_none() {
        let many_marks = "\u{0301}".repeat(10_000); // 10k combining marks
        for family in [
            "\u{1F600}",
            "Azul Mock Mono\u{1F600}",
            "A\u{301}zul Mock Mono", // NFD: 'A' + combining acute
            "Ａzul Mock Mono",       // fullwidth A
            "\u{202E}Azul Mock Mono",
            "אזול מוק מונו",
            "\u{200B}Azul Mock Mono",
            "Azul\u{200D}Mock\u{200D}Mono",
            "🅰🅱🅲",
            many_marks.as_str(),
        ] {
            assert_eq!(mock_advance_px(family, 20.0), None, "{family:?}");
        }
    }

    #[test]
    fn mock_advance_px_deeply_nested_input_does_not_stack_overflow() {
        let nested = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        assert_eq!(mock_advance_px(&nested, 20.0), None);
        let nested_parens = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert_eq!(mock_advance_px(&nested_parens, 20.0), None);
    }

    #[test]
    fn mock_advance_px_numeric_looking_families_are_none() {
        for family in [
            "0",
            "-0",
            "NaN",
            "inf",
            "-inf",
            "9223372036854775807",
            "1e309",
            "0x20",
        ] {
            assert_eq!(mock_advance_px(family, 20.0), None, "{family:?}");
        }
    }

    // ---------------------------------------------------------------------
    // mock_advance_px — numeric edge cases on font_size_px
    // ---------------------------------------------------------------------

    #[test]
    fn mock_advance_px_zero_preserves_sign() {
        assert_eq!(mock_advance_px("Azul Mock Mono", 0.0), Some(0.0));
        assert_eq!(mock_advance_px("Azul Mock Wide", 0.0), Some(0.0));

        // -0.0 * 0.5 == -0.0, and Wide returns the input verbatim.
        assert!(mock_advance_px("Azul Mock Mono", -0.0)
            .unwrap()
            .is_sign_negative());
        assert!(mock_advance_px("Azul Mock Wide", -0.0)
            .unwrap()
            .is_sign_negative());
    }

    #[test]
    fn mock_advance_px_nan_propagates_without_panicking() {
        for family in ["Azul Mock Mono", "Azul Mock Wide"] {
            assert!(
                mock_advance_px(family, f32::NAN).unwrap().is_nan(),
                "{family}"
            );
            assert!(
                mock_advance_px(family, -f32::NAN).unwrap().is_nan(),
                "{family}"
            );
        }
    }

    #[test]
    fn mock_advance_px_infinities_propagate() {
        assert_eq!(
            mock_advance_px("Azul Mock Mono", f32::INFINITY),
            Some(f32::INFINITY)
        );
        assert_eq!(
            mock_advance_px("Azul Mock Mono", f32::NEG_INFINITY),
            Some(f32::NEG_INFINITY)
        );
        assert_eq!(
            mock_advance_px("Azul Mock Wide", f32::INFINITY),
            Some(f32::INFINITY)
        );
        assert_eq!(
            mock_advance_px("Azul Mock Wide", f32::NEG_INFINITY),
            Some(f32::NEG_INFINITY)
        );
    }

    #[test]
    fn mock_advance_px_extremes_do_not_overflow() {
        // Halving can never overflow, and Wide is the identity, so every
        // finite input must map to a finite output.
        for size in [
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            1e38,
            -1e38,
            i64::MAX as f32,
            i64::MIN as f32,
            u64::MAX as f32,
        ] {
            for family in ["Azul Mock Mono", "Azul Mock Wide"] {
                let got = mock_advance_px(family, size).unwrap();
                assert!(got.is_finite(), "{family} @ {size:e} produced {got:e}");
            }
        }
    }

    #[test]
    fn mock_advance_px_smallest_subnormal_underflows_to_zero_not_garbage() {
        let tiny = f32::from_bits(1); // ~1e-45, smallest positive subnormal
        let mono = mock_advance_px("Azul Mock Mono", tiny).unwrap();
        assert!(mono.is_finite() && !mono.is_sign_negative());
        assert!(
            mono <= tiny,
            "halving a subnormal must not grow it: {mono:e}"
        );
        assert_eq!(mock_advance_px("Azul Mock Wide", tiny), Some(tiny));
    }

    #[test]
    fn mock_advance_px_negative_sizes_are_passed_through_unclamped() {
        // Documents current behaviour rather than asserting a clamp: the
        // helper is a pure arithmetic mirror of font-size and performs no
        // validation, so a negative size yields a negative advance.
        assert_eq!(mock_advance_px("Azul Mock Mono", -20.0), Some(-10.0));
        assert_eq!(mock_advance_px("Azul Mock Wide", -20.0), Some(-20.0));
    }

    // ---------------------------------------------------------------------
    // mock_advance_px — invariants
    // ---------------------------------------------------------------------

    #[test]
    fn mock_advance_px_wide_is_exactly_double_mono() {
        for size in [0.0, 0.1, 1.0, 12.0, 16.0, 20.0, 1234.5, 1e30, -7.5] {
            let mono = mock_advance_px("Azul Mock Mono", size).unwrap();
            let wide = mock_advance_px("Azul Mock Wide", size).unwrap();
            assert_eq!(wide, mono * 2.0, "at size {size}");
            // Halving is exact in binary floating point: round-trip must be
            // bit-identical, with no accumulated error.
            assert_eq!(mono * 2.0, size, "mono round-trip at size {size}");
        }
    }

    #[test]
    fn mock_advance_px_scales_linearly_and_monotonically() {
        let sizes = [0.0, 0.5, 1.0, 8.0, 12.0, 16.0, 20.0, 64.0, 1000.0, 1e20];
        for family in ["Azul Mock Mono", "Azul Mock Wide"] {
            let mut prev = f32::NEG_INFINITY;
            for size in sizes {
                let got = mock_advance_px(family, size).unwrap();
                assert!(got >= prev, "{family} not monotonic at {size}");
                prev = got;
                assert_eq!(
                    mock_advance_px(family, size * 2.0).unwrap(),
                    got * 2.0,
                    "{family} not linear at {size}"
                );
            }
        }
    }

    #[test]
    fn mock_advance_px_is_deterministic() {
        let inputs = ["Azul Mock Mono", "Azul Mock Wide", "Arial", ""];
        for family in inputs {
            for size in [20.0, 0.0, -1.0, f32::NAN, f32::INFINITY] {
                let a = mock_advance_px(family, size);
                let b = mock_advance_px(family, size);
                assert_eq!(
                    a.map(f32::to_bits),
                    b.map(f32::to_bits),
                    "{family:?} @ {size} not pure"
                );
            }
        }
    }

    // ---------------------------------------------------------------------
    // mock_font_ranges — getter invariants
    // ---------------------------------------------------------------------

    #[test]
    fn mock_font_ranges_is_exactly_printable_ascii() {
        let ranges = mock_font_ranges();
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            ranges[0],
            UnicodeRange {
                start: 0x20,
                end: 0x7E
            }
        );
    }

    #[test]
    fn mock_font_ranges_holds_structural_invariants() {
        let ranges = mock_font_ranges();
        assert!(!ranges.is_empty(), "an empty range set would cover nothing");

        let mut prev_end: Option<u32> = None;
        for r in &ranges {
            assert!(r.start <= r.end, "inverted range {r:?}");
            assert!(r.end <= u32::from(char::MAX), "range past U+10FFFF: {r:?}");
            assert!(
                char::from_u32(r.start).is_some() && char::from_u32(r.end).is_some(),
                "range endpoints are not valid scalar values: {r:?}"
            );
            // Endpoints must not be surrogates and must not overflow when the
            // caller iterates start..=end and adds one.
            assert!(r.end.checked_add(1).is_some(), "end+1 overflows: {r:?}");
            if let Some(prev) = prev_end {
                assert!(r.start > prev, "ranges overlap or are unsorted: {r:?}");
            }
            prev_end = Some(r.end);
        }
    }

    #[test]
    fn mock_font_ranges_covers_only_printable_ascii_codepoints() {
        let ranges = mock_font_ranges();
        let mut count = 0u32;
        for r in &ranges {
            for cp in r.start..=r.end {
                let c = char::from_u32(cp).expect("covered codepoint must be a scalar value");
                assert!(
                    c == ' ' || c.is_ascii_graphic(),
                    "U+{cp:04X} ({c:?}) is covered but is not printable ASCII"
                );
                count += 1;
            }
        }
        assert_eq!(count, 95, "0x20..=0x7E is 95 codepoints");
    }

    #[test]
    fn mock_font_ranges_deliberately_excludes_everything_else() {
        let ranges = mock_font_ranges();
        let covers = |cp: u32| ranges.iter().any(|r| cp >= r.start && cp <= r.end);

        // Boundary neighbours first — the classic off-by-one.
        assert!(!covers(0x1F), "0x1F (just below) must not be covered");
        assert!(covers(0x20), "0x20 (first) must be covered");
        assert!(covers(0x7E), "0x7E (last) must be covered");
        assert!(!covers(0x7F), "0x7F DEL (just above) must not be covered");

        for cp in [
            0x00, 0x09, 0x0A, 0x80, 0xA0, 0x5D0,  // Hebrew
            0x627,  // Arabic
            0x4E00, // CJK
            0xFFFD, 0x1_0000, 0x1_F600,  // emoji
            0x10_FFFF, // char::MAX
        ] {
            assert!(!covers(cp), "U+{cp:04X} must fall through to real fallback");
        }
    }

    #[test]
    fn mock_font_ranges_returns_an_independent_owned_vec() {
        let mut first = mock_font_ranges();
        first.clear();
        first.push(UnicodeRange {
            start: 0,
            end: 0x10_FFFF,
        });

        let second = mock_font_ranges();
        assert_eq!(
            second,
            vec![UnicodeRange {
                start: 0x20,
                end: 0x7E
            }],
            "mutating a returned Vec must not affect later calls"
        );
        assert_eq!(second, mock_font_ranges(), "not deterministic");
    }

    // ---------------------------------------------------------------------
    // Round-trip: declared constants vs. the bytes actually embedded
    // ---------------------------------------------------------------------

    #[test]
    fn builtin_mock_fonts_table_is_self_consistent() {
        assert_eq!(BUILTIN_MOCK_FONTS.len(), 2);

        let mut seen: Vec<&str> = Vec::new();
        for (family, bytes) in BUILTIN_MOCK_FONTS {
            assert!(!family.is_empty(), "empty family name");
            assert!(!seen.contains(family), "duplicate family {family:?}");
            seen.push(*family);
            assert!(!bytes.is_empty(), "{family:?} has no font bytes");
            assert!(
                mock_advance_px(family, 20.0).is_some(),
                "{family:?} is registered but mock_advance_px does not know it"
            );
        }

        assert_eq!(BUILTIN_MOCK_FONTS[0], ("Azul Mock Mono", MOCK_MONO_TTF));
        assert_eq!(BUILTIN_MOCK_FONTS[1], ("Azul Mock Wide", MOCK_WIDE_TTF));

        // Same byte length, different content: a copy-paste of one .ttf over
        // the other would slip past a length-only check.
        assert_ne!(
            MOCK_MONO_TTF, MOCK_WIDE_TTF,
            "the two mock fonts must not be the same file"
        );
    }

    #[test]
    fn mock_ttf_bytes_are_well_formed_sfnt() {
        for (family, data) in BUILTIN_MOCK_FONTS {
            assert_eq!(
                be_u32(data, 0),
                Some(0x0001_0000),
                "{family:?} is not a TrueType sfnt"
            );
            let num_tables = be_u16(data, 4).unwrap() as usize;
            assert!(
                num_tables > 0 && num_tables < 64,
                "{family:?}: {num_tables} tables"
            );
            assert!(
                data.len() >= 12 + num_tables * 16,
                "{family:?}: table directory is truncated"
            );
            for tag in [
                b"head", b"hhea", b"hmtx", b"maxp", b"cmap", b"glyf", b"loca", b"name",
            ] {
                assert!(
                    sfnt_table(data, tag).is_some(),
                    "{family:?} is missing the {} table",
                    std::str::from_utf8(tag).unwrap()
                );
            }
            let head = sfnt_table(data, b"head").unwrap();
            assert_eq!(
                be_u32(head, 12),
                Some(0x5F0F_3CF5),
                "{family:?}: bad head magic"
            );
        }
    }

    #[test]
    fn mock_ttf_advances_match_mock_advance_px() {
        for (family, data) in BUILTIN_MOCK_FONTS {
            let (upem, _, _, num_glyphs, advances) =
                font_metrics(data).unwrap_or_else(|| panic!("{family:?}: unparseable metrics"));

            assert!(upem > 0, "{family:?}: units_per_em is zero");
            assert_eq!(
                advances.len(),
                usize::from(num_glyphs),
                "{family:?}: not every glyph has an explicit advance"
            );
            assert!(!advances.is_empty(), "{family:?}: hmtx has no entries");
            let first = advances[0];
            assert!(
                advances.iter().all(|a| *a == first),
                "{family:?} is not monospaced — advances differ across glyphs"
            );

            // The em fraction the font actually encodes must equal the one
            // mock_advance_px hardcodes, at every size, exactly.
            let em_fraction = f32::from(first) / f32::from(upem);
            for size in [0.0, 1.0, 12.0, 16.0, 20.0, 100.0, 1e20] {
                assert_eq!(
                    mock_advance_px(family, size),
                    Some(em_fraction * size),
                    "{family:?} @ {size} px: .ttf says {em_fraction} em"
                );
            }
        }
        // And the documented table itself.
        assert_eq!(mock_advance_px("Azul Mock Mono", 1.0), Some(0.5));
        assert_eq!(mock_advance_px("Azul Mock Wide", 1.0), Some(1.0));
    }

    #[test]
    fn mock_ttf_vertical_metrics_match_module_doc() {
        // Doc table: ascent 0.8 em, descent 0.2 em for both families, so a
        // line box at 20 px is exactly 20 px tall.
        for (family, data) in BUILTIN_MOCK_FONTS {
            let (upem, ascender, descender, _, _) = font_metrics(data).unwrap();
            let upem = f32::from(upem);
            assert_eq!(f32::from(ascender) / upem, 0.8, "{family:?}: ascent");
            assert_eq!(f32::from(descender) / upem, -0.2, "{family:?}: descent");
            assert_eq!(
                (f32::from(ascender) - f32::from(descender)) / upem * 20.0,
                20.0,
                "{family:?}: line box at 20 px is not exactly 20 px"
            );
        }
    }

    #[test]
    fn mock_ttf_cmap_coverage_matches_mock_font_ranges() {
        let declared: Vec<(u32, u32)> = mock_font_ranges()
            .iter()
            .map(|r| (r.start, r.end))
            .collect();

        for (family, data) in BUILTIN_MOCK_FONTS {
            let actual = cmap_coverage(data)
                .unwrap_or_else(|| panic!("{family:?}: no format-4 cmap subtable"));
            assert_eq!(
                actual, declared,
                "{family:?}: cmap coverage disagrees with mock_font_ranges(), so \
                 registration would claim glyphs the font does not have (or hide \
                 ones it does)"
            );

            // .notdef plus one glyph per covered codepoint.
            let (_, _, _, num_glyphs, _) = font_metrics(data).unwrap();
            let covered: u32 = actual.iter().map(|(s, e)| e - s + 1).sum();
            assert_eq!(
                u32::from(num_glyphs),
                covered + 1,
                "{family:?}: glyph count does not match cmap coverage + .notdef"
            );
        }
    }

    /// Every glyph's raw `glyf` entry, indexed by glyph id, via `loca`.
    ///
    /// Assumes the long `loca` format, which these fonts declare
    /// (`head.indexToLocFormat == 1`); the assert below states that rather
    /// than silently misparsing if the generator ever switches.
    fn glyph_outlines(data: &[u8]) -> Option<Vec<&[u8]>> {
        let head = sfnt_table(data, b"head")?;
        assert_eq!(be_i16(head, 50)?, 1, "expected long loca format");
        let loca = sfnt_table(data, b"loca")?;
        let glyf = sfnt_table(data, b"glyf")?;
        let num_glyphs = be_u16(sfnt_table(data, b"maxp")?, 4)? as usize;

        let mut out = Vec::with_capacity(num_glyphs);
        for gid in 0..num_glyphs {
            let start = be_u32(loca, gid.checked_mul(4)?)? as usize;
            let end = be_u32(loca, gid.checked_add(1)?.checked_mul(4)?)? as usize;
            out.push(glyf.get(start..end.min(glyf.len()))?);
        }
        Some(out)
    }

    #[test]
    fn mock_ttf_every_inked_glyph_draws_distinct_ink() {
        // THE regression guard for the reason the ink is codepoint-derived at
        // all. These fonts used to give every glyph the identical filled
        // rectangle, which made two equal-length strings rasterise to
        // BIT-IDENTICAL pixels: a text edit then produced damage but no pixel
        // change, and every pixel-liveness assertion over a mock-font text
        // mutation (`assert_changed` and friends) was asserting something the
        // font made impossible. Collapse the outlines back onto one shape and
        // this fails instead of silently making those assertions vacuous.
        for (family, data) in BUILTIN_MOCK_FONTS {
            let outlines =
                glyph_outlines(data).unwrap_or_else(|| panic!("{family:?}: cannot read glyf/loca"));

            let blank: Vec<usize> = outlines
                .iter()
                .enumerate()
                .filter(|(_, o)| o.is_empty())
                .map(|(gid, _)| gid)
                .collect();
            assert_eq!(
                blank,
                vec![0, 1],
                "{family:?}: exactly .notdef (gid 0) and U+0020 (gid 1) may be \
                 blank — they still advance, they just draw nothing"
            );

            let inked: Vec<&[u8]> = outlines.iter().copied().filter(|o| !o.is_empty()).collect();
            let distinct: std::collections::BTreeSet<&[u8]> = inked.iter().copied().collect();
            assert_eq!(
                distinct.len(),
                inked.len(),
                "{family:?}: {} of {} inked glyphs share an outline with another \
                 glyph, so the characters they encode are indistinguishable in \
                 pixels",
                inked.len() - distinct.len(),
                inked.len()
            );
        }
    }

    #[test]
    fn mock_ttf_glyph_boxes_are_identical_across_glyphs() {
        // The companion to the test above, and the reason varying the ink is
        // FREE: the per-glyph pattern lives strictly INSIDE a frame that
        // touches all four sides of the box, so every glyph declares the same
        // bounding box and it still equals `head`'s global one. If a future
        // pattern let ink drive the box, glyph extents would start varying by
        // character and the mock fonts would stop being arithmetic.
        for (family, data) in BUILTIN_MOCK_FONTS {
            let head = sfnt_table(data, b"head").unwrap();
            let global = (
                be_i16(head, 36).unwrap(),
                be_i16(head, 38).unwrap(),
                be_i16(head, 40).unwrap(),
                be_i16(head, 42).unwrap(),
            );

            for (gid, outline) in glyph_outlines(data).unwrap().iter().enumerate() {
                if outline.is_empty() {
                    continue; // blank glyphs carry no bbox at all
                }
                let bbox = (
                    be_i16(outline, 2).unwrap(),
                    be_i16(outline, 4).unwrap(),
                    be_i16(outline, 6).unwrap(),
                    be_i16(outline, 8).unwrap(),
                );
                assert_eq!(
                    bbox, global,
                    "{family:?}: glyph {gid} has bbox {bbox:?} but head declares \
                     {global:?} — glyph extents must not vary by character"
                );
            }
        }
    }

    #[test]
    fn mock_ttf_family_name_matches_its_registration_key() {
        // If the embedded name table disagreed with the key in
        // BUILTIN_MOCK_FONTS, resolution would silently fall back — exactly
        // the failure mode the module doc warns about.
        for (family, data) in BUILTIN_MOCK_FONTS {
            let embedded = family_name(data)
                .unwrap_or_else(|| panic!("{family:?}: no Windows family name record"));
            assert_eq!(
                embedded.as_str(),
                *family,
                "font self-identifies as {embedded:?} but is registered as {family:?}"
            );
        }
    }
}

#[cfg(all(test, feature = "font_loading"))]
mod distinguishable_glyphs {
    use std::collections::BTreeMap;

    use super::*;
    use crate::font::parsed::ParsedFont;

    /// The decoded ink of one character: contour end indices plus the raw
    /// outline points, in font units.
    ///
    /// Two characters whose signatures compare equal cover exactly the same
    /// pixels at every size and every transform, so no rasteriser can tell
    /// them apart. Reading the DECODED outline (rather than a rendered bitmap)
    /// keeps this free of a rasteriser while still being the thing a
    /// rasteriser consumes.
    fn ink_of(font: &ParsedFont, ch: char) -> (Vec<u16>, Vec<(i16, i16)>) {
        let gid = font
            .lookup_glyph_index(ch as u32)
            .unwrap_or_else(|| panic!("{ch:?} is in the mock font's ASCII range"));
        let glyph = font
            .get_or_decode_glyph(gid)
            .unwrap_or_else(|| panic!("{ch:?} (gid {gid}) must decode"));
        let ends = glyph.raw_contour_ends.clone().unwrap_or_else(|| {
            panic!(
                "{ch:?} (gid {gid}) decoded with no contour list — every inked mock glyph is a \
                 simple TrueType glyph, so this means the outline was not read at all"
            )
        });
        (ends, glyph.raw_points.clone().unwrap_or_default())
    }

    /// Two different ASCII characters must not decode to the same ink.
    ///
    /// This is the property `mock_ttf_every_inked_glyph_draws_distinct_ink`
    /// pins in the raw `glyf` bytes, asserted one layer further down: through
    /// the real font parser, on the outline a rasteriser actually consumes. A
    /// generator emitting distinct `glyf` entries that nonetheless decoded to
    /// the same contours would satisfy the byte-level test and still make text
    /// edits invisible.
    ///
    /// Why it is load-bearing: when every glyph was the identical filled
    /// rectangle, two equal-length strings rasterised bit-identically, so a
    /// text edit like `"tick 1"` -> `"tick 2"` produced damage but NO pixel
    /// change. Every scenario combining a mock font with a pixel-liveness
    /// assertion (`assert_changed`, `assert_damage_covers_changes`,
    /// `assert_damage_sound`) was then asserting something the font made
    /// impossible; the ones that passed did so only because their two strings
    /// differed in LENGTH — an accident, not a property. The gen-e2e prompt
    /// pushes mock fonts hard and is right to (deterministic metrics, no OS
    /// font dependency, and real family names collapse onto one shared
    /// `FontId` on a CI box, which makes font-identity and leak assertions
    /// vacuously green), so the font is what had to change.
    ///
    /// Note what this deliberately does NOT assert on: GLYPH IDS. The broken
    /// fonts already gave every codepoint its own id — `'A'` was gid 34 and
    /// `'B'` gid 35 before the ink was fixed exactly as after it — while all
    /// 94 inked glyphs shared ONE outline. An id-based check passes just as
    /// happily on a font whose glyphs are indistinguishable, so it cannot fail
    /// for the reason stated above; only the outline can.
    #[test]
    fn mock_font_glyphs_differ_between_characters() {
        for (family, bytes) in BUILTIN_MOCK_FONTS {
            let mut warnings = Vec::new();
            let font = ParsedFont::from_bytes(bytes, 0, &mut warnings)
                .unwrap_or_else(|| panic!("{family:?} must parse"));

            // The pair from the original bug report, named explicitly so a
            // failure reads as characters rather than glyph indices.
            assert_ne!(
                ink_of(&font, 'A'),
                ink_of(&font, 'B'),
                "{family:?}: 'A' and 'B' decode to the SAME outline, so every string of a given \
                 length renders identically and no pixel assertion over a text edit can hold"
            );

            // ...and the whole inked range is pairwise distinct, not just that
            // one pair. U+0020 is excluded: it is blank by design (it still
            // advances, it just draws nothing).
            let mut seen: BTreeMap<(Vec<u16>, Vec<(i16, i16)>), char> = BTreeMap::new();
            for cp in 0x21..=0x7E_u32 {
                let ch = char::from_u32(cp).expect("printable ASCII is a scalar value");
                if let Some(prev) = seen.insert(ink_of(&font, ch), ch) {
                    panic!(
                        "{family:?}: {ch:?} and {prev:?} decode to the same outline, so the two \
                         characters are indistinguishable in pixels"
                    );
                }
            }
            assert_eq!(seen.len(), 94, "{family:?}: 0x21..=0x7E is 94 inked glyphs");
        }
    }

    /// The companion property, and the reason varying the ink is FREE: the ink
    /// varies, the METRICS DO NOT.
    ///
    /// The per-glyph pattern lives strictly inside a frame that touches all
    /// four sides of the glyph box, so every glyph decodes to the same tight
    /// bounding box and the same advance. `mock_ttf_glyph_boxes_are_identical_across_glyphs`
    /// checks the bbox each glyph DECLARES in its `glyf` header; this checks
    /// the one the parser COMPUTES from the outline, which is what layout and
    /// rasterisation actually use. A future pattern that let ink escape the
    /// frame would keep the declared box intact and still make glyph extents
    /// vary by character — that is the gap this closes.
    #[test]
    fn mock_font_ink_varies_without_moving_any_metric() {
        for (family, bytes) in BUILTIN_MOCK_FONTS {
            let mut warnings = Vec::new();
            let font = ParsedFont::from_bytes(bytes, 0, &mut warnings)
                .unwrap_or_else(|| panic!("{family:?} must parse"));

            let mut advances = BTreeMap::new();
            let mut boxes = BTreeMap::new();
            for cp in 0x20..=0x7E_u32 {
                let ch = char::from_u32(cp).expect("printable ASCII is a scalar value");
                let gid = font
                    .lookup_glyph_index(cp)
                    .unwrap_or_else(|| panic!("{family:?}: {ch:?} has no glyph"));
                advances
                    .entry(font.get_horizontal_advance(gid))
                    .or_insert(ch);
                if cp == 0x20 {
                    continue; // blank: its bbox is the seeded one, not ink
                }
                let glyph = font
                    .get_or_decode_glyph(gid)
                    .unwrap_or_else(|| panic!("{family:?}: {ch:?} must decode"));
                let bbox = glyph.bounding_box;
                boxes
                    .entry((bbox.min_x, bbox.min_y, bbox.max_x, bbox.max_y))
                    .or_insert(ch);
            }

            assert_eq!(
                advances.len(),
                1,
                "{family:?}: advances differ across characters ({advances:?}) — the mock fonts \
                 exist to make text layout arithmetic, which requires one advance for all of them"
            );
            assert_eq!(
                boxes.len(),
                1,
                "{family:?}: the DECODED tight bounding box varies by character ({boxes:?}), so \
                 ink escaped the constant frame and glyph extents are no longer deterministic"
            );
        }
    }
}
