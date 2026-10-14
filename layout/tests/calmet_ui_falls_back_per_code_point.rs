//! Calmet UI is bundled, and a stack that starts with it falls back one code
//! point at a time.
//!
//! Calmet UI is the typeface traced from the 1732 print of Calmet's
//! Commentaire litteral, cut down to the letters a UI needs: about a hundred
//! code points (Latin letters, figures, the German umlauts, the long s, a few
//! quotes and dashes). AzDrive sets every text in
//! `"Calmet UI", "EB Garamond", serif`, as the design does in Chrome, where
//! the face's `unicode-range` is exactly its cmap: a character Calmet has is
//! drawn in Calmet, and only the characters it lacks (`#`, `@`, `/`, the
//! euro, `e` with an accent) fall to the next face. A whole word must never
//! switch faces because one of its letters is missing.
//!
//! The window's font cache is memory-only, so the faces in play are the ones
//! azul bundles and nothing the machine has installed.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, font::parsed::ParsedFont,
    solver3::display_list::DisplayListItem, text3::ui_fonts::bundled_ui_fonts,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CALMET_UI: &str = "Calmet UI";
const EB_GARAMOND: &str = "EB Garamond";

/// The content hash of the bundled upright face of `family`: the hash the
/// display list names a glyph run's face by.
fn face_hash(family: &str, italic: bool) -> u64 {
    let face = bundled_ui_fonts()
        .iter()
        .find(|f| f.family == family && f.italic == italic)
        .unwrap_or_else(|| panic!("{family} (italic: {italic}) is bundled"));
    ParsedFont::from_bytes(&face.bytes, 0, &mut Vec::new())
        .unwrap_or_else(|| panic!("the bundled {family} parses"))
        .hash
}

/// The face hash of every glyph the display list paints, in paint order.
fn painted_faces(text: &str) -> Vec<u64> {
    let css = format!(
        "margin: 0px; font-family: \"{CALMET_UI}\", \"{EB_GARAMOND}\", serif; font-size: 20px;"
    );
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css(&css)
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the paragraph lays out");
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .items
        .iter()
        .flat_map(|item| match item {
            DisplayListItem::Text {
                glyphs, font_hash, ..
            } => vec![font_hash.font_hash; glyphs.len()],
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn calmet_ui_is_bundled_upright_and_italic_as_truetype() {
    let faces: Vec<_> = bundled_ui_fonts()
        .iter()
        .filter(|f| f.family == CALMET_UI)
        .collect();
    assert_eq!(
        faces.iter().map(|f| f.italic).collect::<Vec<_>>(),
        [false, true],
        "the regular and the italic"
    );
    for face in faces {
        assert_eq!(face.bytes.get(..4), Some(&[0, 1, 0, 0][..]), "a TrueType (glyf) face");
    }
}

/// The ranges a face registers with are its cmap, code point for code point:
/// a range wider than the cmap would take a character the face draws as
/// .notdef, a narrower one would send a letter it has to the next face.
#[test]
fn every_bundled_face_registers_exactly_the_code_points_of_its_cmap() {
    for face in bundled_ui_fonts() {
        let parsed = ParsedFont::from_bytes(&face.bytes, 0, &mut Vec::new())
            .unwrap_or_else(|| panic!("{} parses", face.family));
        let ranges = face.ranges();
        for cp in 0x20_u32..0x3000 {
            let registered = ranges.iter().any(|r| r.start <= cp && cp <= r.end);
            let in_cmap = parsed.lookup_glyph_index(cp).is_some_and(|g| g != 0);
            assert_eq!(
                registered, in_cmap,
                "{} (italic: {}): U+{cp:04X} registered {registered}, in the cmap {in_cmap}",
                face.family, face.italic
            );
        }
    }
}

#[test]
fn a_word_calmet_covers_is_drawn_in_calmet_alone() {
    let calmet = face_hash(CALMET_UI, false);
    for word in ["Gardasee", "Schöne Grüße", "2026-10-14 16:20", "„Fünf“ – ſo"] {
        let faces = painted_faces(word);
        assert!(!faces.is_empty(), "{word:?} paints");
        assert!(
            faces.iter().all(|h| *h == calmet),
            "{word:?}: every glyph in Calmet UI ({calmet:x}): {faces:x?}"
        );
    }
}

#[test]
fn a_character_calmet_lacks_falls_to_the_next_face_and_only_that_character() {
    let calmet = face_hash(CALMET_UI, false);
    let garamond = face_hash(EB_GARAMOND, false);
    for lacking in ['#', '$', '%', '+', '/', '<', '=', '>', '@', '[', ']', '{', '}', '~', 'é', '€'] {
        let text = format!("a{lacking}b");
        let faces = painted_faces(&text);
        assert_eq!(
            faces,
            [calmet, garamond, calmet],
            "{text:?}: a and b in Calmet UI ({calmet:x}), {lacking:?} in EB Garamond ({garamond:x})"
        );
    }
}
