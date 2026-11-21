//! Calmet UI, the typeface of AzDrive's design, for an app to register at
//! startup.
//!
//! Calmet UI is traced from the 1732 print of Augustin Calmet's Commentaire
//! litteral and made fit for a UI: the book's own glyphs (never redrawn), the
//! umlauts and the sharp s composed from them, the long s mapped, a few marks
//! re-spaced, and the features `liga`, `dlig` (ct), `ss01` (the long s) and
//! `tnum`. It has about a hundred code points, no bold and no small caps, so
//! a stylesheet names EB Garamond (which azul bundles) after it: a character
//! Calmet lacks falls to Garamond one code point at a time ([`STACK`]).
//!
//! The faces live here and not in libazul, so the engine's library carries no
//! app's typeface: an app registers them on its `AppConfig` ([`register`],
//! the `azul` feature), and every window it opens draws with them.
//! `make_fonts.py` packs a new build of the faces.

use std::sync::OnceLock;

/// The family the faces register under: what a stylesheet names.
pub const FAMILY: &str = "Calmet UI";

/// The design's font stack: Calmet UI, EB Garamond for the characters Calmet
/// lacks, then the serifs a desktop has.
pub const STACK: &str = "\"Calmet UI\", \"EB Garamond\", \"Hoefler Text\", Georgia, serif";

const REGULAR_BR: &[u8] = include_bytes!("../fonts/CalmetUI-Regular.ttf.br");
const ITALIC_BR: &[u8] = include_bytes!("../fonts/CalmetUI-Italic.ttf.br");

/// The TrueType bytes of the regular and the italic, in that order,
/// unpacked on first use and kept for the process. A face that does not
/// unpack is empty (a damaged package is skipped, never a panic).
#[must_use]
pub fn faces() -> &'static [Vec<u8>] {
    static FACES: OnceLock<[Vec<u8>; 2]> = OnceLock::new();
    &FACES.get_or_init(|| [unpack(REGULAR_BR), unpack(ITALIC_BR)])[..]
}

fn unpack(packed: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if brotli_decompressor::BrotliDecompress(&mut &packed[..], &mut out).is_err() {
        out.clear();
    }
    out
}

/// Registers Calmet UI's faces on the app's `config`, before the app runs:
/// every window then draws a text whose stack names [`FAMILY`] in it.
#[cfg(feature = "azul")]
pub fn register(config: &mut azul::prelude::AppConfig) {
    for face in faces().iter().filter(|face| !face.is_empty()) {
        config.add_font(FAMILY, face.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_faces_unpack_to_truetype() {
        let faces = faces();
        assert_eq!(faces.len(), 2);
        for face in faces {
            assert_eq!(face.get(..4), Some(&[0, 1, 0, 0][..]), "a TrueType (glyf) face");
        }
    }

    #[test]
    fn the_stack_starts_with_the_family() {
        assert!(STACK.starts_with(&format!("\"{FAMILY}\"")));
    }
}
