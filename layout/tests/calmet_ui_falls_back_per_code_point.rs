//! An app registers its fonts at startup, and a stack that starts with one
//! falls back one code point at a time.
//!
//! The engine bundles only the faces its built-in themes need (EB Garamond);
//! an app ships its own typeface and registers the bytes on its `AppConfig`
//! (`AppConfig::add_font`, one call per face; weight and style come from the
//! face's own tables), and every window's font manager takes them in, with
//! exactly the code points of each face's cmap.
//!
//! AzDrive's typeface is Calmet UI (the `azul-fonts-calmet` crate, linked by
//! the app, not by libazul): traced from the 1732 print of Calmet's
//! Commentaire litteral and cut down to the letters a UI needs - about a
//! hundred code points. AzDrive sets every text in `"Calmet UI", "EB
//! Garamond", serif`, as the design does in Chrome, where the face's
//! `unicode-range` is exactly its cmap: a character Calmet has is drawn in
//! Calmet, and only the characters it lacks (`#`, `@`, `/`, the euro, `e`
//! with an accent) fall to the next face. A whole word never switches faces
//! because one of its letters is missing.
//!
//! The window's font cache is memory-only, so the faces in play are the ones
//! azul bundles, the ones the test registers, and nothing the machine has
//! installed.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::{AppConfig, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, font::parsed::ParsedFont,
    solver3::display_list::DisplayListItem, text3::ui_fonts::bundled_ui_fonts,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CALMET_UI: &str = azul_fonts_calmet::FAMILY;
const EB_GARAMOND: &str = "EB Garamond";

/// An app's config with Calmet UI registered the way AzDrive registers it:
/// every face, through `AppConfig::add_font`.
fn calmet_config() -> AppConfig {
    let mut config = AppConfig::create();
    for face in azul_fonts_calmet::faces() {
        config.add_font(CALMET_UI.into(), face.to_vec().into());
    }
    config
}

/// A window whose font manager took the app's fonts in, as the shell gives
/// them to every window it opens.
fn window_with(config: &AppConfig) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.font_manager.register_app_fonts(config.bundled_fonts.as_ref());
    lw
}

fn hash_of(bytes: &[u8]) -> u64 {
    ParsedFont::from_bytes(bytes, 0, &mut Vec::new())
        .expect("the face parses")
        .hash
}

/// The content hash of Calmet UI's face (`italic`: the italic one): the hash
/// the display list names a glyph run's face by.
fn calmet_hash(italic: bool) -> u64 {
    hash_of(&azul_fonts_calmet::faces()[usize::from(italic)])
}

/// The content hash of the bundled upright EB Garamond.
fn garamond_hash() -> u64 {
    let face = bundled_ui_fonts()
        .iter()
        .find(|f| f.family == EB_GARAMOND && !f.italic)
        .expect("EB Garamond is bundled");
    hash_of(&face.bytes)
}

/// The face hash of every glyph the display list paints for `text` set in
/// the stack, under the extra declarations `css`.
fn painted_faces_with(css: &str, text: &str) -> Vec<u64> {
    let css = format!(
        "margin: 0px; font-family: \"{CALMET_UI}\", \"{EB_GARAMOND}\", serif; font-size: 20px; \
         {css}"
    );
    let dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css(&css)
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
    );
    let mut lw = window_with(&calmet_config());
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

fn painted_faces(text: &str) -> Vec<u64> {
    painted_faces_with("", text)
}

/// libazul carries only the faces its themes need: an app's typeface lives
/// in the app (the user's ruling - the dylib is big enough).
#[test]
fn the_engine_bundles_no_calmet_ui() {
    assert!(
        bundled_ui_fonts().iter().all(|f| f.family != CALMET_UI),
        "Calmet UI is the app's font, registered at startup, not one of the engine's"
    );
}

#[test]
fn the_calmet_crate_ships_the_regular_and_the_italic_as_truetype() {
    let faces = azul_fonts_calmet::faces();
    assert_eq!(faces.len(), 2, "the regular and the italic");
    for face in faces {
        assert_eq!(face.get(..4), Some(&[0, 1, 0, 0][..]), "a TrueType (glyf) face");
    }
}

/// The ranges a registered face enters the chain with are its cmap, code
/// point for code point: a range wider than the cmap would take a character
/// the face draws as .notdef, a narrower one would send a letter it has to
/// the next face. The same holds for the faces azul bundles.
#[test]
fn every_registered_and_bundled_face_covers_exactly_the_code_points_of_its_cmap() {
    let lw = window_with(&calmet_config());
    let registered = lw.font_manager.memory_families
        [&rust_fontconfig::utils::normalize_family_name(CALMET_UI)]
        .iter()
        .map(|face| face.font_match.unicode_ranges.clone());
    let bundled = bundled_ui_fonts().iter().map(|face| face.ranges());
    let faces: Vec<(&[u8], Vec<rust_fontconfig::UnicodeRange>)> = azul_fonts_calmet::faces()
        .iter()
        .map(Vec::as_slice)
        .zip(registered)
        .chain(bundled_ui_fonts().iter().map(|f| f.bytes.as_slice()).zip(bundled))
        .collect();
    assert_eq!(faces.len(), 2 + bundled_ui_fonts().len(), "both Calmet faces registered");
    for (bytes, ranges) in faces {
        let parsed = ParsedFont::from_bytes(bytes, 0, &mut Vec::new()).expect("parses");
        for cp in 0x20_u32..0x3000 {
            let registered = ranges.iter().any(|r| r.start <= cp && cp <= r.end);
            let in_cmap = parsed.lookup_glyph_index(cp).is_some_and(|g| g != 0);
            assert_eq!(
                registered, in_cmap,
                "U+{cp:04X}: registered {registered}, in the cmap {in_cmap} (font {:x})",
                parsed.hash
            );
        }
    }
}

#[test]
fn a_word_calmet_covers_is_drawn_in_calmet_alone() {
    let calmet = calmet_hash(false);
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
    let calmet = calmet_hash(false);
    let garamond = garamond_hash();
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

/// A family takes several faces, one `add_font` each: the style comes from
/// the face's own tables, so an italic run is drawn in the italic.
#[test]
fn an_italic_run_is_drawn_in_the_italic_face_the_app_registered() {
    let italic = calmet_hash(true);
    let faces = painted_faces_with("font-style: italic;", "Gardasee");
    assert!(
        !faces.is_empty() && faces.iter().all(|h| *h == italic),
        "every glyph in Calmet UI Italic ({italic:x}): {faces:x?}"
    );
}
