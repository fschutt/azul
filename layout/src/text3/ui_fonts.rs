//! The UI FONTS azul bundles: faces a built-in widget theme names by family,
//! so a theme draws the same on every machine whether or not the face is
//! installed.
//!
//! | family        | faces                | for                                         |
//! |---------------|----------------------|---------------------------------------------|
//! | `EB Garamond` | 400 and 700, upright | flora: its capitals labels (buttons, group  |
//! |               |                      | and section titles, dialog headers), and    |
//! |               |                      | running text                                |
//!
//! flora.css sets "every label in capitals" in Garamond (`--font-caps`), and
//! the faces here are the website's own: static instances of its latin slice
//! of the variable font (`doc/fonts/EBGaramond-Variable.woff2`, SIL OFL 1.1 -
//! the licence sits next to the bundle), cut by
//! `scripts/make_ebgaramond_ui_fonts.py`. Static, because azul measures a
//! WOFF2's decompressed tables as zero-width and registering a variable font
//! bakes one instance per weight bucket in every `FontManager`; brotli at
//! quality 11 (about 54 KB for both faces, like the Material Icons bundle),
//! decompressed once per process.
//!
//! Registered in every `FontManager` (next to the mock fonts, the same four
//! constructors) in the FALLBACK tier: an installed EB Garamond - a full one,
//! with true small caps - wins on a desktop that has it; everywhere else
//! these are there. The slice covers Latin-1 and the typographic extras
//! ([`eb_garamond_ranges`]); any other character falls back per glyph
//! through the rest of the `font-family` list.
//!
//! The slice has no `smcp` feature (and azul's CSS no `font-variant`), so
//! flora sets its "small capitals" the way it already does: uppercase, a
//! size step down, tracked out (`letter-spacing`).

use std::sync::OnceLock;

use rust_fontconfig::UnicodeRange;

/// The family flora's chrome is set in.
pub const EB_GARAMOND: &str = "EB Garamond";

const EB_GARAMOND_REGULAR_BR: &[u8] =
    include_bytes!("../../assets/fonts/ui/EBGaramond-Regular.ttf.br");
const EB_GARAMOND_BOLD_BR: &[u8] = include_bytes!("../../assets/fonts/ui/EBGaramond-Bold.ttf.br");

/// `packed`, brotli-decompressed; empty if it does not decompress (the face
/// is then skipped - a font path never panics over a bundled file).
fn decompress(packed: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if brotli_decompressor::BrotliDecompress(&mut &packed[..], &mut out).is_err() {
        out.clear();
    }
    out
}

/// Every bundled face as `(family, TrueType bytes)`, decompressed on first
/// use and kept for the process.
#[must_use]
pub fn bundled_ui_fonts() -> &'static [(&'static str, Vec<u8>)] {
    static FACES: OnceLock<Vec<(&'static str, Vec<u8>)>> = OnceLock::new();
    FACES.get_or_init(|| {
        alloc::vec![
            (EB_GARAMOND, decompress(EB_GARAMOND_REGULAR_BR)),
            (EB_GARAMOND, decompress(EB_GARAMOND_BOLD_BR)),
        ]
    })
}

/// The codepoints the bundled EB Garamond covers (its cmap): printable
/// ASCII, Latin-1, the few Latin Extended letters and combining marks the
/// website's slice keeps, and the typographic punctuation.
#[must_use]
pub fn eb_garamond_ranges() -> Vec<UnicodeRange> {
    const RANGES: &[(u32, u32)] = &[
        (0x0020, 0x007E),
        (0x00A0, 0x00FF),
        (0x0102, 0x0102),
        (0x0131, 0x0131),
        (0x0152, 0x0153),
        (0x02BB, 0x02BC),
        (0x02C6, 0x02C6),
        (0x02DA, 0x02DA),
        (0x02DC, 0x02DC),
        (0x0300, 0x0301),
        (0x0303, 0x0304),
        (0x0308, 0x0309),
        (0x0323, 0x0323),
        (0x0329, 0x0329),
        (0x2002, 0x2002),
        (0x2009, 0x2009),
        (0x200B, 0x200B),
        (0x2013, 0x2014),
        (0x2018, 0x201A),
        (0x201C, 0x201E),
        (0x2022, 0x2022),
        (0x2026, 0x2026),
        (0x2032, 0x2033),
        (0x2039, 0x203A),
        (0x2044, 0x2044),
        (0x20AC, 0x20AC),
        (0x2122, 0x2122),
        (0x2191, 0x2191),
        (0x2193, 0x2193),
        (0x2212, 0x2212),
        (0x2215, 0x2215),
    ];
    RANGES
        .iter()
        .map(|&(start, end)| UnicodeRange { start, end })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_garamond_faces_decompress_to_truetype() {
        let faces = bundled_ui_fonts();
        assert_eq!(faces.len(), 2);
        for (family, bytes) in faces {
            assert_eq!(*family, EB_GARAMOND);
            assert_eq!(bytes.get(..4), Some(&[0, 1, 0, 0][..]), "a TrueType (glyf) face");
        }
    }

    #[test]
    fn the_garamond_faces_are_the_regular_and_the_bold() {
        let weights: Vec<_> = bundled_ui_fonts()
            .iter()
            .map(|(family, bytes)| {
                let faces = rust_fontconfig::FcParseFontBytes(bytes, family).expect("parses");
                let (pattern, _) = faces.into_iter().next().expect("one face");
                pattern.weight
            })
            .collect();
        assert_eq!(
            weights,
            [rust_fontconfig::FcWeight::Normal, rust_fontconfig::FcWeight::Bold]
        );
    }
}
