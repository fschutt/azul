//! Validates the primitives behind `CallbackInfo::get_loaded_fonts` /
//! `get_loaded_font_bytes`: a font loaded into a `FontManager` via the
//! production lazy loader must be enumerable, its `font_hash` must match the
//! hash carried by display-list glyph runs (`ParsedFont::hash`), and its
//! source bytes must be retrievable for embedding.
//!
//! The callback methods are thin wrappers that lock `font_manager.parsed_fonts`,
//! deref each `FontRef` to its `ParsedFont`, and read `hash` / `font_name` /
//! `num_glyphs` / `source_bytes_for_subset()` — exactly the chain exercised
//! here. Constructing a full `CallbackInfo` requires a live `LayoutWindow` plus
//! ~12 manager references, so we test the underlying logic directly.

#![cfg(feature = "text_layout")]

use azul_layout::font::parsed::ParsedFont;
use azul_layout::text3::cache::FontManager;
use azul_layout::text3::default::PathLoader;
use azul_css::props::basic::FontRef;
// FontManager keys its `parsed_fonts` map by `rust_fontconfig::FontId`, not the
// `azul_layout::font_traits::FontId` newtype.
use rust_fontconfig::FontId;

const KOHO_LIGHT: &[u8] = include_bytes!("../../examples/assets/fonts/KoHo-Light.ttf");

/// Deref a FontRef to its ParsedFont — same helper the callback methods use
/// (`azul_layout::font_ref_to_parsed_font`).
fn parsed(font_ref: &FontRef) -> &ParsedFont {
    azul_layout::font_ref_to_parsed_font(font_ref)
}

#[test]
fn loaded_font_enumerate_and_retrieve_bytes() {
    // Build a FontManager with an empty system cache (matches the WASM /
    // headless path — no system font discovery required).
    let fm: FontManager<FontRef> =
        FontManager::new(rust_fontconfig::FcFontCache::default()).expect("FontManager::new");

    // Load the bundled font via the PRODUCTION lazy loader. This is the exact
    // closure `load_fonts_from_disk` feeds during a real layout pass, and it
    // sets `original_bytes` so `source_bytes_for_subset()` works.
    let loader = PathLoader;
    let bytes_arc = std::sync::Arc::new(rust_fontconfig::FontBytes::Owned(
        std::sync::Arc::from(KOHO_LIGHT),
    ));
    let font_ref = loader
        .load_font_shared(bytes_arc, 0)
        .expect("load_font_shared must parse KoHo-Light");

    // The display-list glyph-run hash (`ParsedFont::hash`) is the correlation
    // key the callback returns in `LoadedFont::font_hash`.
    let expected_hash = parsed(&font_ref).hash;
    assert_ne!(expected_hash, 0, "parsed font hash should be non-zero");

    fm.insert_font(FontId::new(), font_ref.clone());

    // --- get_loaded_fonts() logic: enumerate parsed_fonts -> LoadedFont descriptors ---
    {
        let guard = fm.parsed_fonts.lock().unwrap();
        assert_eq!(guard.len(), 1, "exactly one font is loaded");
        let fr = guard.values().next().unwrap();
        let p = parsed(fr);
        assert_eq!(p.hash, expected_hash, "enumerated hash matches glyph-run hash");
        assert!(p.num_glyphs > 0, "font reports a glyph count");
        // KoHo-Light has a PostScript name in its NAME table.
        assert!(
            p.font_name.is_some(),
            "KoHo-Light should expose a PostScript/family name"
        );
        assert!(
            p.source_bytes_for_subset().is_some(),
            "production-loaded font retains its source bytes (has_bytes == true)"
        );
    }

    // --- get_loaded_font_bytes(hash) logic: lookup by hash -> raw bytes ---
    let found = fm
        .get_font_by_hash(expected_hash)
        .expect("font is findable by its glyph-run hash");
    let retrieved = parsed(&found)
        .source_bytes_for_subset()
        .expect("source bytes retrievable for embedding");
    // Bytes round-trip: what a callback would embed equals the input font file.
    assert_eq!(
        retrieved.as_slice(),
        KOHO_LIGHT,
        "retrieved font bytes must equal the original font file"
    );

    // A hash that doesn't correspond to any loaded font yields nothing
    // (the callback returns OptionU8Vec::None in this case).
    assert!(
        fm.get_font_by_hash(expected_hash.wrapping_add(1)).is_none(),
        "unknown hash must not resolve to a font"
    );
}
