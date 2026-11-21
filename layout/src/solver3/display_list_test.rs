//! The text of a page's header and footer in the slicing entry points that take
//! a `RendererResources` and no font manager: drawn from a font registered in
//! it, and never dropped without a word when none is.
//!
//! A paged layout draws the same text with the document's font manager
//! (`paged_layout_test.rs`).

use azul_core::resources::{FontKey, IdNamespace};

use super::*;
use crate::text3::mock_fonts::MOCK_MONO_TTF;

const REPORT: &str = "this slicing entry point draws it with a font registered in `RendererResources`";

/// Renderer resources with the mock mono font registered: every ASCII glyph
/// advances half an em.
fn resources_with_a_font() -> RendererResources {
    let font = crate::font::parsed::ParsedFont::from_bytes(MOCK_MONO_TTF, 0, &mut Vec::new())
        .expect("the mock face parses");
    let font_ref = crate::parsed_font_to_font_ref(font);
    let mut resources = RendererResources::default();
    let key = FontKey::unique(IdNamespace(0));
    resources
        .font_hash_map
        .insert(crate::font_ref_to_parsed_font(&font_ref).hash, key);
    resources
        .currently_registered_fonts
        .insert(key, (font_ref, BTreeMap::default()));
    resources
}

fn page_with_header(resources: &RendererResources) -> DisplayList {
    let mut config = SlicerConfig::simple(200.0);
    config.header_footer = HeaderFooterConfig::default().with_header_text("Header");
    let mut pages =
        paginate_display_list_with_slicer_and_breaks(DisplayList::default(), &config, resources)
            .expect("the pages");
    assert_eq!(pages.len(), 1);
    pages.remove(0)
}

fn text_runs(page: &DisplayList) -> usize {
    page.items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::Text { .. }))
        .count()
}

/// With no font registered there is nothing to draw the text with, and the
/// slicer says so (it used to leave the band empty and say nothing).
#[test]
fn page_text_without_a_registered_font_is_reported_not_dropped_silently() {
    let _lock = azul_core::diagnostics::test_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    azul_core::diagnostics::clear();

    let page = page_with_header(&RendererResources::default());

    assert_eq!(text_runs(&page), 0, "nothing can draw the text");
    assert!(
        azul_core::diagnostics::any_contains(REPORT),
        "the finding is reported: {:?}",
        azul_core::diagnostics::recorded()
    );
}

/// With a font registered the text is drawn as before - one run of glyphs
/// advancing as the font says - and nothing is reported.
#[test]
fn page_text_with_a_registered_font_is_drawn_and_not_reported() {
    let _lock = azul_core::diagnostics::test_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    azul_core::diagnostics::clear();

    let page = page_with_header(&resources_with_a_font());

    let runs: Vec<&DisplayListItem> = page
        .items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::Text { .. }))
        .collect();
    assert_eq!(runs.len(), 1);
    let DisplayListItem::Text {
        glyphs, font_size_px, ..
    } = runs[0]
    else {
        unreachable!()
    };
    assert_eq!(glyphs.len(), "Header".chars().count());
    for pair in glyphs.windows(2) {
        assert!(
            (pair[1].point.x - pair[0].point.x - font_size_px * 0.5).abs() < 1e-3,
            "the mock mono font advances half an em"
        );
    }
    assert!(!azul_core::diagnostics::any_contains(REPORT));
}

/// A text of size zero, or none at all, is nothing to draw: not a finding.
#[test]
fn no_text_and_no_size_are_not_reported() {
    let _lock = azul_core::diagnostics::test_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    azul_core::diagnostics::clear();
    let resources = RendererResources::default();
    let band = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(100.0, 30.0));
    let mut setter = RegisteredFontText(&resources);

    let sized = HeaderFooterConfig {
        font_size: 0.0,
        ..HeaderFooterConfig::default()
    };
    assert!(setter
        .set_text("Header", band, &sized, TextAlignment::Center)
        .is_empty());
    assert!(setter
        .set_text("", band, &HeaderFooterConfig::default(), TextAlignment::Center)
        .is_empty());
    assert!(!azul_core::diagnostics::any_contains(REPORT));
}
