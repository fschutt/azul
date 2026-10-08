//! The UI FONTS azul bundles: faces a built-in widget theme names by family,
//! so a theme draws the same on every machine whether or not the face is
//! installed.
//!
//! | family        | faces                            | for                             |
//! |---------------|----------------------------------|---------------------------------|
//! | `EB Garamond` | 400 and 700, upright and italic  | flora: its capitals labels      |
//! |               |                                  | (buttons, group and section     |
//! |               |                                  | titles, dialog headers), its    |
//! |               |                                  | chrome and its running text     |
//! |               |                                  | (the `system:` text roles under |
//! |               |                                  | flora, [`theme_font_families`]) |
//!
//! flora.css sets running text and "every label in capitals" in Garamond
//! (`--font-serif`, `--font-caps`), and the faces here are the website's own:
//! static instances of its latin slices of the variable font
//! (`doc/fonts/EBGaramond-Variable.woff2` and `-Italic.woff2`, byte-identical
//! to Google Fonts' EB Garamond v33 latin slices, "Version 1.003"; SIL OFL
//! 1.1 - the licence sits next to the bundle), cut by
//! `scripts/make_ebgaramond_ui_fonts.py`, which checks the sources' SHA-256.
//! Static, because azul measures a WOFF2's decompressed tables as zero-width
//! and registering a variable font bakes one instance per weight bucket in
//! every `FontManager`; brotli at quality 11 - about 112 KB for the four
//! faces (the two variable fonts would be 96 KB, and a baked instance per
//! weight in every window on top) - decompressed once per process.
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

/// Flora's hand, as a family list: `--font-serif` / `--font-caps`,
/// `'EB Garamond', Georgia, 'Times New Roman', serif`. The bundled face
/// first, so it holds on every machine; a character outside its slice falls
/// back per glyph through the rest.
pub const FLORA_HAND: &[&str] = &[EB_GARAMOND, "Georgia", "Times New Roman", "serif"];

/// The families the app theme whose structural theme is `theme` (`flora`
/// for `flora:green`; `None` outside every theme) sets the `system:` font
/// role `role` in, where that theme has a hand of its own - else `None`, and
/// the role takes the platform's face (`SystemFontType::get_fallback_chain`,
/// `system-ui` for the UI).
///
/// Flora writes every TEXT role in Garamond: running text (`--font-serif`)
/// and the chrome - the user's ruling for flora on the desktop, and what
/// flora.css's own UI hand (`--font-ui: 'Grenze', 'EB Garamond', Georgia,
/// serif`) renders as wherever Grenze is not installed, which is every
/// desktop: azul bundles Garamond, not Grenze. So `system:ui`, `system:serif`,
/// `system:title`, `system:menu` and `system:small` (and their bold
/// spellings) are [`FLORA_HAND`]; monospace keeps the platform's code face.
/// Every other theme (flat, native) keeps the platform's faces.
#[must_use]
pub fn theme_font_families(
    theme: Option<&str>,
    role: azul_css::system::SystemFontType,
) -> Option<&'static [&'static str]> {
    use azul_css::system::SystemFontType as Role;
    match (theme?, role) {
        ("flora", Role::Monospace | Role::MonospaceBold | Role::MonospaceItalic) => None,
        ("flora", _) => Some(FLORA_HAND),
        _ => None,
    }
}

const EB_GARAMOND_REGULAR_BR: &[u8] =
    include_bytes!("../../assets/fonts/ui/EBGaramond-Regular.ttf.br");
const EB_GARAMOND_BOLD_BR: &[u8] = include_bytes!("../../assets/fonts/ui/EBGaramond-Bold.ttf.br");
const EB_GARAMOND_ITALIC_BR: &[u8] =
    include_bytes!("../../assets/fonts/ui/EBGaramond-Italic.ttf.br");
const EB_GARAMOND_BOLD_ITALIC_BR: &[u8] =
    include_bytes!("../../assets/fonts/ui/EBGaramond-BoldItalic.ttf.br");

/// `packed`, brotli-decompressed; empty if it does not decompress (the face
/// is then skipped - a font path never panics over a bundled file).
fn decompress(packed: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if brotli_decompressor::BrotliDecompress(&mut &packed[..], &mut out).is_err() {
        out.clear();
    }
    out
}

/// One bundled face: its family, its TrueType bytes, and whether it is the
/// italic cut (whose cmap is a little smaller: [`eb_garamond_italic_ranges`]).
#[derive(Debug)]
pub struct BundledUiFace {
    /// The family the face registers under ([`EB_GARAMOND`]).
    pub family: &'static str,
    /// The TrueType bytes, decompressed (empty if the bundle is damaged).
    pub bytes: Vec<u8>,
    /// The italic cut.
    pub italic: bool,
}

impl BundledUiFace {
    /// The codepoints the face covers: what it is registered with, so a
    /// character it lacks falls back to the next family.
    #[must_use]
    pub fn ranges(&self) -> Vec<UnicodeRange> {
        if self.italic {
            eb_garamond_italic_ranges()
        } else {
            eb_garamond_ranges()
        }
    }
}

/// Every bundled face, decompressed on first use and kept for the process:
/// EB Garamond 400 and 700, upright and italic.
#[must_use]
pub fn bundled_ui_fonts() -> &'static [BundledUiFace] {
    static FACES: OnceLock<Vec<BundledUiFace>> = OnceLock::new();
    FACES.get_or_init(|| {
        let face = |packed: &[u8], italic: bool| BundledUiFace {
            family: EB_GARAMOND,
            bytes: decompress(packed),
            italic,
        };
        alloc::vec![
            face(EB_GARAMOND_REGULAR_BR, false),
            face(EB_GARAMOND_BOLD_BR, false),
            face(EB_GARAMOND_ITALIC_BR, true),
            face(EB_GARAMOND_BOLD_ITALIC_BR, true),
        ]
    })
}

/// The codepoints the bundled italic EB Garamond covers: the upright's
/// ([`eb_garamond_ranges`]) but the superscript one, two and three (U+00B9,
/// U+00B2, U+00B3), which the website's italic slice does not carry.
#[must_use]
pub fn eb_garamond_italic_ranges() -> Vec<UnicodeRange> {
    eb_garamond_ranges()
        .into_iter()
        .flat_map(|r| {
            if r.start == 0x00A0 && r.end == 0x00FF {
                alloc::vec![
                    UnicodeRange { start: 0x00A0, end: 0x00B1 },
                    UnicodeRange { start: 0x00B4, end: 0x00B8 },
                    UnicodeRange { start: 0x00BA, end: 0x00FF },
                ]
            } else {
                alloc::vec![r]
            }
        })
        .collect()
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
    fn flora_sets_every_text_role_in_garamond_and_keeps_the_code_face() {
        use azul_css::system::SystemFontType as Role;
        for role in [
            Role::Ui,
            Role::UiBold,
            Role::Title,
            Role::TitleBold,
            Role::Menu,
            Role::Small,
            Role::Serif,
            Role::SerifBold,
        ] {
            assert_eq!(theme_font_families(Some("flora"), role), Some(FLORA_HAND), "{role:?}");
            assert_eq!(theme_font_families(Some("flat"), role), None, "flat: {role:?}");
            assert_eq!(theme_font_families(None, role), None, "no theme: {role:?}");
        }
        for role in [Role::Monospace, Role::MonospaceBold, Role::MonospaceItalic] {
            assert_eq!(theme_font_families(Some("flora"), role), None, "{role:?}");
        }
        assert_eq!(FLORA_HAND.first(), Some(&EB_GARAMOND), "the bundled face first");
        assert_eq!(FLORA_HAND.last(), Some(&"serif"), "a serif to the end, as flora.css");
    }

    #[test]
    fn every_garamond_face_decompresses_to_truetype() {
        let faces = bundled_ui_fonts();
        assert_eq!(faces.len(), 4);
        for face in faces {
            assert_eq!(face.family, EB_GARAMOND);
            assert_eq!(face.bytes.get(..4), Some(&[0, 1, 0, 0][..]), "a TrueType (glyf) face");
        }
    }

    /// Running text has emphasis: the bundle is the regular and the bold,
    /// each upright and italic, so an italic run in flora's hand is a real
    /// italic and not the upright face.
    #[test]
    fn the_garamond_faces_are_the_regular_and_the_bold_upright_and_italic() {
        use rust_fontconfig::{FcWeight, PatternMatch};
        let styles: Vec<_> = bundled_ui_fonts()
            .iter()
            .map(|face| {
                let faces =
                    rust_fontconfig::FcParseFontBytes(&face.bytes, face.family).expect("parses");
                let (pattern, _) = faces.into_iter().next().expect("one face");
                (pattern.weight, pattern.italic == PatternMatch::True, face.italic)
            })
            .collect();
        assert_eq!(
            styles,
            [
                (FcWeight::Normal, false, false),
                (FcWeight::Bold, false, false),
                (FcWeight::Normal, true, true),
                (FcWeight::Bold, true, true),
            ]
        );
    }

    /// Running text has emphasis: under flora the `system:` text roles are
    /// Garamond, so an italic run needs an italic Garamond - the upright face
    /// would set the emphasis upright (azul draws no synthetic oblique).
    #[test]
    fn the_garamond_bundle_has_an_italic_regular_and_bold() {
        let italic_weights: Vec<_> = bundled_ui_fonts()
            .iter()
            .filter_map(|face| {
                let faces =
                    rust_fontconfig::FcParseFontBytes(&face.bytes, face.family).expect("parses");
                let (pattern, _) = faces.into_iter().next().expect("one face");
                (pattern.italic == rust_fontconfig::PatternMatch::True).then_some(pattern.weight)
            })
            .collect();
        assert_eq!(
            italic_weights,
            [rust_fontconfig::FcWeight::Normal, rust_fontconfig::FcWeight::Bold]
        );
    }

    #[test]
    fn the_italic_ranges_leave_out_the_superscript_figures_the_italic_lacks() {
        let covers = |ranges: &[UnicodeRange], c: u32| ranges.iter().any(|r| r.start <= c && c <= r.end);
        let upright = eb_garamond_ranges();
        let italic = eb_garamond_italic_ranges();
        for c in [0x00B2_u32, 0x00B3, 0x00B9] {
            assert!(covers(&upright, c) && !covers(&italic, c), "U+{c:04X}");
        }
        for c in (0x0020_u32..0x2216).filter(|c| ![0x00B2, 0x00B3, 0x00B9].contains(c)) {
            assert_eq!(covers(&upright, c), covers(&italic, c), "U+{c:04X}");
        }
    }
}
