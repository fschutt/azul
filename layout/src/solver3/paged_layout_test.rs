//! The header, the footer and the page numbers of a paged layout, drawn with the
//! fonts of the document's font manager: no font is registered in
//! `RendererResources`, which is how a PDF or print caller lays a document out.
//!
//! The document is three empty boxes with a forced break after the first two:
//! three pages, and no text in the body, so every text item on a page is the
//! header or the footer.

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    dom::Dom,
    resources::{IdNamespace, ImageCache},
    task::GetSystemTimeCallback,
};
use azul_css::props::basic::FontRef;
use rust_fontconfig::FcFontCache;

use super::*;
use crate::{
    font_traits::FontManager,
    solver3::display_list::DisplayListItem,
    text3::{default::PathLoader, mock_fonts::mock_advance_px},
    xml::DomXmlExt,
};

const PAGE: LogicalSize = LogicalSize {
    width: 600.0,
    height: 400.0,
};

/// Three boxes of `100px`, the first two followed by a page break.
const THREE_PAGES: &str = "<html><body style=\"margin: 0\">\
     <div style=\"height: 100px; break-after: page\"></div>\
     <div style=\"height: 100px; break-after: page\"></div>\
     <div style=\"height: 100px\"></div>\
     </body></html>";

/// What a paged layout of `html` under `config` left: the result, the font
/// manager the document was laid out with, and the debug messages.
struct Laid {
    result: PagedLayoutResult,
    font_manager: FontManager<FontRef>,
    messages: Vec<LayoutDebugMessage>,
}

/// `html` on 600 x 400 pages under `config`, with a font manager that holds no
/// system font (only the faces azul bundles) and `font_loader` loading them.
fn lay_out_with<L>(html: &str, config: FakePageConfig, font_loader: L) -> Laid
where
    L: Fn(
        Arc<rust_fontconfig::FontBytes>,
        usize,
    ) -> std::result::Result<FontRef, crate::text3::cache::LayoutError>,
{
    let styled_dom = Dom::from_xml_string(html);
    let mut font_manager = FontManager::<FontRef>::new(FcFontCache::default()).expect("manager");
    let mut messages = Some(Vec::new());
    let result = layout_document_paged_v2(
        &mut LayoutCache::default(),
        &mut TextLayoutCache::new(),
        FragmentationContext::new_paged(PAGE),
        &styled_dom,
        LogicalRect::new(LogicalPosition::zero(), PAGE),
        &mut font_manager,
        &BTreeMap::new(),
        &mut messages,
        None,
        // Nothing registered with a renderer.
        &RendererResources::default(),
        IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        config,
        &ImageCache::default(),
        GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
    )
    .expect("the paged layout");
    Laid {
        result,
        font_manager,
        messages: messages.unwrap_or_default(),
    }
}

fn lay_out(html: &str, config: FakePageConfig) -> Laid {
    let loader = PathLoader::new();
    lay_out_with(html, config, move |bytes, index| {
        loader.load_font_shared(bytes, index)
    })
}

/// The glyph runs of a page, top to bottom: `(font hash, glyph x of each, baseline y)`.
fn text_items(page: &DisplayList) -> Vec<(u64, Vec<f32>, f32)> {
    page.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text {
                glyphs, font_hash, ..
            } => Some((
                font_hash.font_hash,
                glyphs.iter().map(|g| g.point.x).collect(),
                glyphs.first().map_or(f32::NAN, |g| g.point.y),
            )),
            _ => None,
        })
        .collect()
}

fn header_and_footer() -> FakePageConfig {
    FakePageConfig::new()
        .with_header_text("Quarterly report")
        .with_footer_page_numbers()
}

/// The repro of the bug: with no font registered in `RendererResources` every
/// page had no text item where its header and footer were expected, while the
/// room for them was reserved (the bands were there, and empty).
#[test]
fn the_header_and_the_page_numbers_are_drawn_when_no_font_is_registered() {
    let laid = lay_out(THREE_PAGES, header_and_footer());

    assert_eq!(laid.result.pages.len(), 3, "the page count is the document's");
    for (index, page) in laid.result.pages.iter().enumerate() {
        let runs = text_items(page);
        assert_eq!(
            runs.len(),
            2,
            "page {}: the header and the footer, two runs of text: {runs:?}",
            index + 1
        );
        // The header is in the header band at the top of the page, the footer
        // in the band at its bottom.
        let (header, footer) = (&runs[0], &runs[1]);
        assert!(
            (0.0..=30.0).contains(&header.2),
            "page {}: the header's baseline {} is in the band at the top",
            index + 1,
            header.2
        );
        assert!(
            (PAGE.height - 30.0..=PAGE.height).contains(&footer.2),
            "page {}: the footer's baseline {} is in the band at the bottom",
            index + 1,
            footer.2
        );
        assert!(!header.1.is_empty() && !footer.1.is_empty(), "glyphs were set");
        // Their faces are in the font manager the document was laid out with:
        // where a PDF writer or a renderer looks them up by hash.
        for (hash, _, _) in &runs {
            assert_ne!(*hash, 0, "a glyph run with a face");
            assert!(
                laid.font_manager.get_font_by_hash(*hash).is_some(),
                "page {}: the face {hash} of the text is loaded in the font manager",
                index + 1
            );
        }
    }
}

/// A PDF writer draws a paged list's text from the shaped layout
/// (`TextLayout`), not from the glyph runs: the header and the footer carry
/// one each, as the text of the document does.
#[test]
fn the_header_and_the_footer_carry_their_shaped_layout_for_a_pdf_writer() {
    let laid = lay_out(THREE_PAGES, header_and_footer());

    for page in &laid.result.pages {
        let layouts: Vec<&DisplayListItem> = page
            .items
            .iter()
            .filter(|item| matches!(item, DisplayListItem::TextLayout { .. }))
            .collect();
        assert_eq!(layouts.len(), 2, "one shaped layout each for the header and the footer");
        for item in layouts {
            let DisplayListItem::TextLayout { layout, .. } = item else {
                unreachable!()
            };
            assert!(
                layout
                    .downcast_ref::<crate::text3::cache::UnifiedLayout>()
                    .is_some_and(|l| !l.items.is_empty()),
                "the payload is the shaped layout a PDF writer reads"
            );
        }
    }
}

/// The font is chosen by `FakePageConfig::with_font_family`: a CSS family list,
/// resolved through the document's font manager. `Azul Mock Wide` advances one
/// em a glyph and `Azul Mock Mono` half of one, so where the glyphs stand says
/// which face drew them.
#[test]
fn the_font_family_option_chooses_the_font_of_the_header_and_the_footer() {
    let wide = lay_out(
        THREE_PAGES,
        header_and_footer().with_font_family("Azul Mock Wide"),
    );
    let mono = lay_out(
        THREE_PAGES,
        header_and_footer().with_font_family("Azul Mock Mono"),
    );
    let size = 10.0;
    let (wide_advance, mono_advance) = (
        mock_advance_px("Azul Mock Wide", size).expect("a mock font"),
        mock_advance_px("Azul Mock Mono", size).expect("a mock font"),
    );

    let (wide_runs, mono_runs) = (text_items(&wide.result.pages[0]), text_items(&mono.result.pages[0]));
    assert_eq!((wide_runs.len(), mono_runs.len()), (2, 2));
    for (runs, advance, who) in [
        (&wide_runs, wide_advance, "Azul Mock Wide"),
        (&mono_runs, mono_advance, "Azul Mock Mono"),
    ] {
        for (_, xs, _) in runs {
            for pair in xs.windows(2) {
                assert!(
                    (pair[1] - pair[0] - advance).abs() < 1e-3,
                    "{who}: every glyph advances {advance}px, got {}",
                    pair[1] - pair[0]
                );
            }
        }
        // Centred in the 600px page: the header is 16 glyphs wide.
        let header_width = 16.0 * advance;
        assert!(
            (runs[0].1[0] - (PAGE.width - header_width) / 2.0).abs() < 1e-3,
            "{who}: the header is centred, it starts at {}",
            runs[0].1[0]
        );
    }
    assert_ne!(
        wide_runs[0].0, mono_runs[0].0,
        "the two families are two faces: two font hashes"
    );
    // A face of the family the option names is loaded in the manager that laid
    // the document out, though no text of the document uses it.
    assert!(wide.font_manager.get_font_by_hash(wide_runs[0].0).is_some());
}

/// A family list is a fallback list as it is in the document: the first family
/// the system has draws the text.
#[test]
fn a_family_list_falls_back_to_the_first_family_that_exists() {
    let laid = lay_out(
        THREE_PAGES,
        header_and_footer().with_font_family("No Such Family, 'Azul Mock Wide', serif"),
    );
    let runs = text_items(&laid.result.pages[0]);
    assert_eq!(runs.len(), 2);
    let advance = mock_advance_px("Azul Mock Wide", 10.0).expect("a mock font");
    assert!((runs[0].1[1] - runs[0].1[0] - advance).abs() < 1e-3);
}

/// The space the header and the footer take, and so the page count, is what it
/// was: the font changes how the text is drawn, never how much room it has.
#[test]
fn the_font_does_not_change_the_room_the_header_and_the_footer_take() {
    let plain = lay_out(THREE_PAGES, FakePageConfig::new());
    let default = lay_out(THREE_PAGES, header_and_footer());
    let wide = lay_out(
        THREE_PAGES,
        header_and_footer().with_font_family("Azul Mock Wide"),
    );

    assert_eq!(plain.result.pages.len(), 3);
    assert_eq!(default.result.pages.len(), 3);
    assert_eq!(wide.result.pages.len(), 3);
    assert_eq!(default.result.breaks, wide.result.breaks);
    assert!((default.result.total_content_height - wide.result.total_content_height).abs() < 1e-3);
    // The plain document has no text item at all.
    assert!(plain.result.pages.iter().all(|page| text_items(page).is_empty()));
}

/// Nothing is drawn silently: a face of the family that cannot be loaded leaves
/// the text undrawn and the layout says so.
#[test]
fn a_header_that_cannot_be_drawn_says_so() {
    let laid = lay_out_with(THREE_PAGES, header_and_footer(), |_, _| {
        Err(crate::text3::cache::LayoutError::ShapingError(
            "no font file can be read".to_owned(),
        ))
    });

    assert_eq!(laid.result.pages.len(), 3);
    assert!(
        laid.result
            .pages
            .iter()
            .all(|page| text_items(page).is_empty()),
        "no face loaded: no glyph run"
    );
    let warnings: Vec<&str> = laid
        .messages
        .iter()
        .map(|m| m.message.as_str())
        .filter(|m| m.contains("[azul][paginate]"))
        .collect();
    assert!(
        warnings.iter().any(|m| m.contains("failed to load the font")),
        "the font that failed to load is named: {warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|m| m.contains("was not drawn") && m.contains("Quarterly report")),
        "the text that was not drawn is named: {warnings:?}"
    );
    assert_eq!(
        warnings.iter().filter(|m| m.contains("\"Quarterly report\"")).count(),
        1,
        "said once for the layout, not once per page: {warnings:?}"
    );
}

/// A family no font matches is reported (the text falls back as the
/// document's would, and is drawn).
#[test]
fn a_family_nothing_matches_is_reported_and_the_text_still_drawn() {
    let laid = lay_out(
        THREE_PAGES,
        header_and_footer().with_font_family("No Such Family"),
    );

    assert!(laid.result.pages.iter().all(|page| text_items(page).len() == 2));
    assert!(
        laid.messages
            .iter()
            .any(|m| m.message.as_str().contains("No Such Family")
                && m.message.as_str().contains("matches no installed or registered font")),
        "the family is named: {:?}",
        laid.messages
    );
}

/// "The document's default font" is the one a node without a `font-family` of
/// its own has: the root element's.
#[test]
fn the_default_font_is_the_one_the_documents_root_has() {
    let html = "<html style=\"font-family: 'Azul Mock Wide'\"><body style=\"margin: 0\">\
         <div style=\"height: 100px\"></div></body></html>";
    let laid = lay_out(html, header_and_footer());

    let runs = text_items(&laid.result.pages[0]);
    assert_eq!(runs.len(), 2);
    let advance = mock_advance_px("Azul Mock Wide", 10.0).expect("a mock font");
    assert!(
        (runs[0].1[1] - runs[0].1[0] - advance).abs() < 1e-3,
        "the header is set in the root's family, advancing {} a glyph",
        runs[0].1[1] - runs[0].1[0]
    );
}

/// Under a page sequence every page's text is set in the family of the setup
/// the page has, and every setup's family is resolved before the pages are.
#[test]
fn a_page_sequence_sets_each_pages_text_in_the_family_of_its_own_setup() {
    use crate::solver3::pagination::{
        HeaderFooterConfig, OptionPageSequence, OptionPageSetup, PageSequence, PageSetup,
    };

    let setup = |family: &str| PageSetup {
        header_footer: HeaderFooterConfig::with_page_numbers().with_font_family(family),
        page_size: PAGE,
        margins: crate::solver3::pagination::PageMargins::default(),
    };
    let mut sequence = PageSequence::uniform(setup("Azul Mock Mono"));
    sequence.first_page = OptionPageSetup::Some(setup("Azul Mock Wide"));
    let config = FakePageConfig {
        page_sequence: OptionPageSequence::Some(sequence),
        ..FakePageConfig::new()
    };
    let laid = lay_out(THREE_PAGES, config);

    assert_eq!(laid.result.pages.len(), 3);
    let advance_of = |page: usize| {
        let runs = text_items(&laid.result.pages[page]);
        assert_eq!(runs.len(), 1, "page {}: the footer only: {runs:?}", page + 1);
        runs[0].1[1] - runs[0].1[0]
    };
    let (wide, mono) = (
        mock_advance_px("Azul Mock Wide", 10.0).expect("a mock font"),
        mock_advance_px("Azul Mock Mono", 10.0).expect("a mock font"),
    );
    assert!((advance_of(0) - wide).abs() < 1e-3, "the first page has its own setup");
    assert!((advance_of(1) - mono).abs() < 1e-3, "the others have the default's");
    assert!((advance_of(2) - mono).abs() < 1e-3);
}
