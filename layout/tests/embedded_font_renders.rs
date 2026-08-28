//! Regression: text whose `font-family` is an EMBEDDED `FontRef` must rasterize.
//!
//! This is the shipped 0.2.0 defect behind
//! `[cpurender] Font hash 11053151937467401179 not found in FontManager`: every
//! widget icon (`Dom::create_icon` → `StyleFontFamily::Ref(material_icons)`) is
//! shaped with a face the DOM handed to the `FontManager` directly, which lands in
//! `embedded_fonts` — NOT in `parsed_fonts`. The CPU renderer searched only
//! `parsed_fonts`, so layout measured and positioned the icon, the display list
//! carried its hash, and the rasterizer then dropped the run on the floor. The
//! WebRender path searched both, so this was CPU-only — which is exactly the split
//! the unified `FontManager::resolve_font_by_hash` removes.
//!
//! The test asserts the contract that makes that impossible: a hash that layout
//! emitted must resolve at render time, i.e. the glyphs must actually paint.

#![cfg(all(
    feature = "cpurender",
    feature = "text_layout",
    feature = "font_loading"
))]

use azul_core::dom::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_core::dom::{Dom, IdOrClass};
use azul_css::props::basic::{FontRef, StyleFontFamily, StyleFontFamilyVec};
use azul_css::props::property::CssProperty;
use azul_layout::cpurender::{render_dom_to_image, AzulPixmap};

/// A real, glyph-bearing face to stand in for Material Icons. Any face works — the
/// bug is about WHERE the face is registered, not which face it is.
const KOHO_LIGHT: &[u8] = include_bytes!("../../examples/assets/fonts/KoHo-Light.ttf");

fn dark_pixels(pm: &AzulPixmap) -> usize {
    pm.data()
        .chunks_exact(4)
        .filter(|p| p[0] < 128 && p[1] < 128 && p[2] < 128)
        .count()
}

fn embedded_font() -> FontRef {
    let parsed = azul_layout::font::parsed::ParsedFont::from_bytes(KOHO_LIGHT, 0, &mut Vec::new())
        .expect("the bundled test face must parse");
    azul_layout::parsed_font_to_font_ref(parsed)
}

#[test]
fn text_in_an_embedded_fontref_family_rasterizes() {
    let font = embedded_font();

    // Exactly what `layout/src/icon.rs::create_font_icon_from_original` builds for
    // every widget icon: a text node whose font-family is `StyleFontFamily::Ref`.
    let mut text = Dom::create_text_do_not_use_without_block_level_wrapper("HELLO");
    text.root
        .set_css_props(CssPropertyWithConditionsVec::from_vec(vec![
            CssPropertyWithConditions::simple(CssProperty::font_family(
                StyleFontFamilyVec::from_vec(vec![StyleFontFamily::Ref(font)]),
            )),
        ]));

    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("t".to_string().into())].into())
        .with_child(text);

    let (css, _) = azul_css::parser2::new_from_str(
        ".t { font-size: 48px; color: #000000; background: #ffffff; }",
    );

    let png = render_dom_to_image(dom, css, 400.0, 100.0, 1.0).expect("render_dom_to_image");
    let pm = AzulPixmap::decode_png(&png).expect("decode png");
    let dark = dark_pixels(&pm);

    assert!(
        dark > 50,
        "text in an EMBEDDED FontRef family painted {dark} dark pixels — the \
         renderer resolved no font for a hash layout had already shaped with \
         (this is the shipped icon-drop regression)"
    );
}

/// The same contract stated directly on the manager: whatever pool a face lives
/// in, the ONE lookup every renderer uses must find it by the hash layout stamps
/// onto its glyphs.
#[test]
fn resolve_font_by_hash_finds_embedded_and_loaded_faces_alike() {
    use azul_layout::text3::cache::FontManager;

    let fm: FontManager<FontRef> =
        FontManager::new(rust_fontconfig::FcFontCache::default()).expect("FontManager::new");

    let embedded = embedded_font();
    let hash = azul_layout::font_ref_to_parsed_font(&embedded).hash;

    // Not registered anywhere yet: nothing to find.
    assert!(fm.resolve_font_by_hash(hash).is_none());

    // Registered ONLY as an embedded font (the `StyleFontFamily::Ref` path) —
    // `parsed_fonts` stays empty, which is precisely the state the CPU renderer
    // used to declare "not found in FontManager".
    fm.register_embedded_font(&embedded);
    assert!(fm.parsed_fonts.lock().unwrap().is_empty());
    assert_eq!(
        fm.resolve_font_by_hash(hash)
            .map(|f| azul_layout::font_ref_to_parsed_font(&f).hash),
        Some(hash),
        "an embedded face must resolve by the hash its glyphs carry"
    );

    // And the ordinary loaded-face pool still resolves.
    let loaded = embedded_font();
    let loaded_hash = azul_layout::font_ref_to_parsed_font(&loaded).hash;
    let fm2: FontManager<FontRef> =
        FontManager::new(rust_fontconfig::FcFontCache::default()).expect("FontManager::new");
    fm2.insert_font(rust_fontconfig::FontId::new(), loaded);
    assert_eq!(
        fm2.resolve_font_by_hash(loaded_hash)
            .map(|f| azul_layout::font_ref_to_parsed_font(&f).hash),
        Some(loaded_hash)
    );
}
