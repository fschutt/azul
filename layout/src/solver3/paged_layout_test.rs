//! The header, the footer and the page numbers of a paged layout, drawn with the
//! fonts of the document's font manager: no font is registered in
//! `RendererResources`, which is how a PDF or print caller lays a document out.
//!
//! The document is three empty boxes with a forced break after the first two:
//! three pages, and no text in the body, so every text item on a page is the
//! header or the footer.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul_core::{
    dom::Dom,
    resources::{IdNamespace, ImageCache},
    task::GetSystemTimeCallback,
};
use azul_css::props::basic::{ColorU, FontRef};
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

/// `html` on 600 x 400 pages under `config`, laid out in `font_manager` with
/// `font_loader` loading the faces: the result and the debug messages.
fn lay_out_in<L>(
    font_manager: &mut FontManager<FontRef>,
    html: &str,
    config: FakePageConfig,
    font_loader: L,
) -> (PagedLayoutResult, Vec<LayoutDebugMessage>)
where
    L: Fn(
        Arc<rust_fontconfig::FontBytes>,
        usize,
    ) -> std::result::Result<FontRef, crate::text3::cache::LayoutError>,
{
    lay_out_in_sized(font_manager, html, PAGE, config, font_loader)
}

/// [`lay_out_in`] on pages of `page` size.
fn lay_out_in_sized<L>(
    font_manager: &mut FontManager<FontRef>,
    html: &str,
    page: LogicalSize,
    config: FakePageConfig,
    font_loader: L,
) -> (PagedLayoutResult, Vec<LayoutDebugMessage>)
where
    L: Fn(
        Arc<rust_fontconfig::FontBytes>,
        usize,
    ) -> std::result::Result<FontRef, crate::text3::cache::LayoutError>,
{
    let styled_dom = Dom::from_xml_string(html);
    let mut messages = Some(Vec::new());
    let result = layout_document_paged_v2(
        &mut LayoutCache::default(),
        &mut TextLayoutCache::new(),
        FragmentationContext::new_paged(page),
        &styled_dom,
        LogicalRect::new(LogicalPosition::zero(), page),
        font_manager,
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
    (result, messages.unwrap_or_default())
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
    let mut font_manager = FontManager::<FontRef>::new(FcFontCache::default()).expect("manager");
    let (result, messages) = lay_out_in(&mut font_manager, html, config, font_loader);
    Laid {
        result,
        font_manager,
        messages,
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

/// The findings of the page furniture a layout was told, as text.
fn findings(messages: &[LayoutDebugMessage]) -> Vec<&str> {
    messages
        .iter()
        .map(|m| m.message.as_str())
        .filter(|m| m.contains("[azul][paginate]"))
        .collect()
}

/// A loader that finds no font file readable.
fn no_font_can_be_read(
    _bytes: Arc<rust_fontconfig::FontBytes>,
    _index: usize,
) -> std::result::Result<FontRef, crate::text3::cache::LayoutError> {
    Err(crate::text3::cache::LayoutError::ShapingError(
        "no font file can be read".to_owned(),
    ))
}

/// Holds the shared diagnostics ring for one test and starts it empty: a test
/// that makes a layout report to the process, or reads what the process was told.
fn quiet_diagnostics() -> std::sync::MutexGuard<'static, ()> {
    let lock = azul_core::diagnostics::test_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    azul_core::diagnostics::clear();
    lock
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
    let _lock = quiet_diagnostics();
    let laid = lay_out_with(THREE_PAGES, header_and_footer(), no_font_can_be_read);

    assert_eq!(laid.result.pages.len(), 3);
    assert!(
        laid.result
            .pages
            .iter()
            .all(|page| text_items(page).is_empty()),
        "no face loaded: no glyph run"
    );
    let warnings = findings(&laid.messages);
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
    let _lock = quiet_diagnostics();
    let laid = lay_out(
        THREE_PAGES,
        header_and_footer().with_font_family("No Such Family"),
    );

    assert!(laid.result.pages.iter().all(|page| text_items(page).len() == 2));
    assert!(
        laid.messages
            .iter()
            .any(|m| m.message.as_str().contains("No Such Family")
                && m.message.as_str().contains("matches an installed or registered font")),
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

/// A second layout in the font manager of the first finds the faces of the page
/// furniture loaded: no face is loaded again, and the text is set as before.
#[test]
fn a_second_layout_in_the_same_font_manager_loads_no_face_again() {
    let loader = PathLoader::new();
    let loads = AtomicUsize::new(0);
    let counting = |bytes, index| {
        loads.fetch_add(1, Ordering::Relaxed);
        loader.load_font_shared(bytes, index)
    };
    let mut font_manager = FontManager::<FontRef>::new(FcFontCache::default()).expect("manager");
    let config = header_and_footer().with_font_family("Azul Mock Wide");

    let (first, _) = lay_out_in(&mut font_manager, THREE_PAGES, config.clone(), counting);
    let loaded_by_the_first = loads.load(Ordering::Relaxed);
    let (second, _) = lay_out_in(&mut font_manager, THREE_PAGES, config, counting);

    assert!(
        loaded_by_the_first > 0,
        "the first layout loads the faces it sets the text in"
    );
    assert_eq!(
        loads.load(Ordering::Relaxed),
        loaded_by_the_first,
        "the second layout loads no face again"
    );
    let runs = |result: &PagedLayoutResult| result.pages.iter().map(text_items).collect::<Vec<_>>();
    assert_eq!(
        runs(&first),
        runs(&second),
        "the same text in the same faces in the same places"
    );
    assert!(runs(&second).iter().all(|page| page.len() == 2));
}

/// A blank header is nothing to draw and nothing to say: no text on any page,
/// and no word about a font that is missing.
#[test]
fn a_blank_header_draws_nothing_and_says_nothing() {
    let laid = lay_out(THREE_PAGES, FakePageConfig::new().with_header_text(" \t "));

    assert_eq!(laid.result.pages.len(), 3);
    for (index, page) in laid.result.pages.iter().enumerate() {
        assert!(
            !page.items.iter().any(|item| matches!(
                item,
                DisplayListItem::Text { .. } | DisplayListItem::TextLayout { .. }
            )),
            "page {}: the blank header is not drawn",
            index + 1
        );
    }
    assert_eq!(findings(&laid.messages), Vec::<&str>::new());
}

/// The text is set in the colour and the size of the config: the glyph runs a
/// renderer draws and the shaped layouts a PDF writer draws from, both.
#[test]
fn the_text_carries_the_colour_and_the_size_of_the_config() {
    let color = ColorU {
        r: 200,
        g: 30,
        b: 10,
        a: 255,
    };
    let laid = lay_out(
        THREE_PAGES,
        header_and_footer().with_font_size(14.0).with_text_color(color),
    );

    for (index, page) in laid.result.pages.iter().enumerate() {
        let mut seen = 0;
        for item in &page.items {
            if let DisplayListItem::Text {
                font_size_px,
                color: drawn,
                ..
            }
            | DisplayListItem::TextLayout {
                font_size_px,
                color: drawn,
                ..
            } = item
            {
                assert!(
                    (font_size_px - 14.0).abs() < 1e-3,
                    "page {}: the size is the config's, not {font_size_px}",
                    index + 1
                );
                assert_eq!(*drawn, color, "page {}", index + 1);
                seen += 1;
            }
        }
        assert_eq!(
            seen,
            4,
            "page {}: a glyph run and a shaped layout each, for the header and the footer",
            index + 1
        );
    }
}

/// `skip_first_page` leaves the first page without a header or a footer, and the
/// others with both.
#[test]
fn skip_first_page_leaves_the_first_page_without_text_and_the_others_with_it() {
    let laid = lay_out(THREE_PAGES, header_and_footer().skip_first_page(true));

    assert_eq!(laid.result.pages.len(), 3);
    let runs: Vec<usize> = laid
        .result
        .pages
        .iter()
        .map(|page| text_items(page).len())
        .collect();
    assert_eq!(runs, [0, 2, 2]);
}

/// A header wider than its band does not wrap onto the page below and does not
/// start before the page: it starts at the start of the band and runs on past
/// its end, as a line too long for its box does in CSS.
#[test]
fn a_header_wider_than_its_band_starts_at_the_start_of_the_band() {
    // 150 glyphs of a face that advances a full em: 1500px of text in a 600px band.
    let header = "word ".repeat(30);
    let laid = lay_out(
        THREE_PAGES,
        FakePageConfig::new()
            .with_header_text(header)
            .with_font_family("Azul Mock Wide"),
    );

    assert_eq!(laid.result.pages.len(), 3);
    for (index, page) in laid.result.pages.iter().enumerate() {
        let mut glyphs = Vec::new();
        for item in &page.items {
            if let DisplayListItem::Text { glyphs: run, .. } = item {
                glyphs.extend(run.iter().map(|g| (g.point.x, g.point.y)));
            }
        }
        assert!(glyphs.len() >= 120, "page {}: {} glyphs", index + 1, glyphs.len());
        assert!(
            glyphs[0].0.abs() < 1e-3,
            "page {}: the text starts at the start of the band, not at {}",
            index + 1,
            glyphs[0].0
        );
        assert!(
            glyphs.iter().all(|g| g.0 >= -1e-3),
            "page {}: nothing starts before the page",
            index + 1
        );
        assert!(
            glyphs.last().is_some_and(|g| g.0 > PAGE.width),
            "page {}: the text runs on past the band",
            index + 1
        );
        assert!(
            glyphs.iter().all(|g| (g.1 - glyphs[0].1).abs() < 1e-3),
            "page {}: one line, it does not wrap",
            index + 1
        );
        assert!(
            (0.0..=30.0).contains(&glyphs[0].1),
            "page {}: the line is in the header band",
            index + 1
        );
    }
}

/// `font-family` is inherited downward only: the default font of the header is
/// the root element's, and a family set on `<body>` does not reach it.
#[test]
fn a_font_family_on_body_does_not_reach_the_header() {
    let html = "<html style=\"font-family: 'Azul Mock Mono'\">\
         <body style=\"margin: 0; font-family: 'Azul Mock Wide'\">\
         <div style=\"height: 100px\"></div></body></html>";
    let laid = lay_out(html, header_and_footer());

    let runs = text_items(&laid.result.pages[0]);
    assert_eq!(runs.len(), 2);
    let mono = mock_advance_px("Azul Mock Mono", 10.0).expect("a mock font");
    assert!(
        (runs[0].1[1] - runs[0].1[0] - mono).abs() < 1e-3,
        "the header is set in the root's family, advancing {} a glyph, not in the body's",
        runs[0].1[1] - runs[0].1[0]
    );
}

/// `list` laid out: nothing is said about it, and the header is set in the mock
/// face `Azul Mock Wide` (every glyph a full em wide) the list has a name for.
fn assert_set_in_the_face_the_list_names_and_said_nothing(list: &str) {
    let advance = mock_advance_px("Azul Mock Wide", 10.0).expect("a mock font");
    let laid = lay_out(THREE_PAGES, header_and_footer().with_font_family(list));

    assert_eq!(findings(&laid.messages), Vec::<&str>::new(), "{list}");
    let runs = text_items(&laid.result.pages[0]);
    assert_eq!(runs.len(), 2, "{list}");
    assert!(
        (runs[0].1[1] - runs[0].1[0] - advance).abs() < 1e-3,
        "{list}: set in the family that exists"
    );
}

/// A family list with a name that is a font is a normal CSS list: a name of it
/// that is missing, behind the one that exists, is said nothing about.
#[test]
fn a_family_list_with_a_missing_name_behind_one_that_exists_is_not_reported() {
    assert_set_in_the_face_the_list_names_and_said_nothing("Azul Mock Wide, No Such Family, serif");
}

/// A missing first choice is what a family list is for.
#[test]
fn a_family_list_with_a_missing_first_choice_is_not_reported() {
    assert_set_in_the_face_the_list_names_and_said_nothing("No Such Family, Azul Mock Wide, serif");
}

/// A trailing comma adds an empty name to the list: it names no family, so none
/// is missing.
#[test]
fn a_trailing_comma_in_a_family_list_names_no_family() {
    assert_set_in_the_face_the_list_names_and_said_nothing("Azul Mock Wide,");
}

/// A list none of whose names is a font is set in the fallback of its stack,
/// and says so once, naming the list (the generics it ends in are not names).
#[test]
fn a_family_list_none_of_whose_names_exists_is_reported_once_with_the_list() {
    let _lock = quiet_diagnostics();
    let laid = lay_out(
        THREE_PAGES,
        header_and_footer().with_font_family("No Such Family, 'Other Missing', serif"),
    );

    assert!(
        laid.result.pages.iter().all(|page| text_items(page).len() == 2),
        "the text falls back, and is drawn"
    );
    let found: Vec<&str> = findings(&laid.messages)
        .into_iter()
        .filter(|m| m.contains("of the header and footer matches"))
        .collect();
    assert_eq!(found.len(), 1, "said once: {found:?}");
    assert!(
        found[0].contains("\"No Such Family, Other Missing\""),
        "the whole list is named: {found:?}"
    );
    assert!(
        found[0].contains("none of the font families"),
        "the message says that none of them matched: {found:?}"
    );
}

/// The process is told each kind of finding once, whatever the families that ran
/// into it (its table of once-only findings is bounded: a service that lays out
/// documents in arbitrary families must not fill it with their names), and every
/// layout's own messages still name its own family.
#[test]
fn the_process_is_told_each_kind_of_finding_once_and_each_layout_its_own() {
    let _lock = quiet_diagnostics();

    for family in ["Alpha Missing", "Beta Missing", "Gamma Missing"] {
        let laid = lay_out_with(
            THREE_PAGES,
            header_and_footer().with_font_family(family),
            no_font_can_be_read,
        );
        let said = findings(&laid.messages);
        assert!(
            said.iter()
                .any(|m| m.contains(family) && m.contains("of the header and footer matches")),
            "{family}: {said:?}"
        );
        assert!(
            said.iter()
                .any(|m| m.contains(family) && m.contains("was not drawn: no face of")),
            "{family}: {said:?}"
        );
        assert!(
            said.iter().any(|m| m.contains("failed to load the font")),
            "{family}: {said:?}"
        );
    }

    let lines = azul_core::diagnostics::recorded();
    let in_the_process = |needle: &str| lines.iter().filter(|line| line.contains(needle)).count();
    assert_eq!(in_the_process("of the header and footer matches"), 1, "{lines:?}");
    assert_eq!(in_the_process("was not drawn: no face of"), 1, "{lines:?}");
    assert_eq!(in_the_process("failed to load the font"), 1, "{lines:?}");
}

// ---------------------------------------------------------------------------
// The page cap: a box of an absurd height is one page of text and a bounded
// number of empty ones, not a billion pages.
// ---------------------------------------------------------------------------

/// A4 in CSS px: the page of the document a PDF caller lays out.
const A4: LogicalSize = LogicalSize {
    width: 595.0,
    height: 842.0,
};

/// `html` on A4 pages under `config`, laid out in a font manager that holds the
/// faces azul bundles.
fn lay_out_a4(
    html: &str,
    config: FakePageConfig,
) -> (PagedLayoutResult, Vec<LayoutDebugMessage>) {
    let loader = PathLoader::new();
    let mut font_manager = FontManager::<FontRef>::new(FcFontCache::default()).expect("manager");
    lay_out_in_sized(&mut font_manager, html, A4, config, move |bytes, index| {
        loader.load_font_shared(bytes, index)
    })
}

/// One `<p>x</p>` with `style`.
fn one_box(style: &str) -> String {
    format!("<html><body><p style=\"{style}\">x</p></body></html>")
}

/// How many of the pages carry an item.
fn pages_with_items(result: &PagedLayoutResult) -> usize {
    result.pages.iter().filter(|p| !p.items.is_empty()).count()
}

/// How many of the pages show something: have an item that paints (the hit-test
/// areas and the markers around them paint nothing).
fn painted_pages(result: &PagedLayoutResult) -> usize {
    result
        .pages
        .iter()
        .filter(|page| {
            page.items.iter().any(|item| {
                !matches!(item, DisplayListItem::HitTestArea { .. })
                    && !item.is_push_marker()
                    && !item.is_pop_marker()
            })
        })
        .count()
}

/// `n` paragraphs of text: the long document of the tests below.
fn paragraphs(n: usize) -> String {
    let mut body = String::new();
    for i in 0..n {
        body.push_str(&format!(
            "<p>Paragraph {i}: the quick brown fox jumps over the lazy dog, again and again, \
             until the line wraps and the page fills up with text.</p>"
        ));
    }
    format!("<html><body>{body}</body></html>")
}

/// The page counts of the documents below on the commit before the cap.
const PAGES_OF_1E5PX: usize = 119;
const PAGES_OF_1E6PX: usize = 1188;
const PAGES_OF_3000_PARAGRAPHS: usize = 172;
const PAGES_OF_3000_SECTIONS: usize = 3108;
const PAGES_OF_THE_GAP: usize = 119;

/// A box of 1e5px and one of 1e6px are the pages they were before the cap: it
/// is far above them. (The counts were taken on the commit before the cap.)
#[test]
fn a_box_of_1e5px_and_one_of_1e6px_have_the_pages_they_always_had() {
    for (style, pages) in [
        ("height:1e5px", PAGES_OF_1E5PX),
        ("height:1e6px", PAGES_OF_1E6PX),
    ] {
        let (result, _) = lay_out_a4(&one_box(style), FakePageConfig::new());
        assert_eq!(result.pages.len(), pages, "{style}");
        assert_eq!(painted_pages(&result), 1, "{style}: only the page of the text shows anything");
        assert!(!result.pages[0].items.is_empty(), "{style}: the text is on the first page");
    }
}

/// A real long document is cut into the pages it always was: its page count is
/// the one of the commit before the cap.
#[test]
fn a_long_document_of_text_has_the_pages_it_always_had() {
    let (result, messages) = lay_out_a4(&paragraphs(3000), FakePageConfig::new());
    assert_eq!(result.pages.len(), PAGES_OF_3000_PARAGRAPHS);
    assert_eq!(painted_pages(&result), result.pages.len(), "every page has text");
    assert!(
        !messages.iter().any(|m| m.message.contains("max_pages")),
        "a document under the cap is not told about it"
    );
}

/// Three thousand sections, each ended by a page break: the pages of the commit
/// before the cap, three thousand of them and more, every one with its text.
#[test]
fn three_thousand_sections_have_the_pages_they_always_had() {
    let mut body = String::new();
    for i in 0..3000 {
        body.push_str(&format!("<div style=\"break-after: page\"><p>Section {i}</p></div>"));
    }
    let (result, _) = lay_out_a4(
        &format!("<html><body>{body}</body></html>"),
        FakePageConfig::new(),
    );
    assert_eq!(result.pages.len(), PAGES_OF_3000_SECTIONS);
    assert!(painted_pages(&result) >= 3000, "{} pages with text", painted_pages(&result));
}

/// A box of 1e5px between two paragraphs: the pages between them are empty pages
/// and stay pages; the two paragraphs are on the first and on the last page.
#[test]
fn a_gap_between_two_pages_of_content_keeps_its_pages() {
    let html = "<html><body><p>first</p><div style=\"height:1e5px\"></div><p>last</p></body></html>";
    let (result, _) = lay_out_a4(html, FakePageConfig::new());
    let n = result.pages.len();
    assert_eq!(n, PAGES_OF_THE_GAP);
    assert_eq!(painted_pages(&result), 2);
    assert!(!result.pages[0].items.is_empty());
    assert!(!result.pages[n - 1].items.is_empty());
}

/// The document of `height:1e12px` is its one page with the text, and no more
/// pages than the cap lets a layout have.
#[test]
fn a_box_of_1e12px_is_the_page_of_its_text_and_a_bounded_number_of_empty_ones() {
    let (result, _) = lay_out_a4(&one_box("height:1e12px"), FakePageConfig::new());
    assert!(result.pages.len() <= 10_000, "{} pages", result.pages.len());
    assert!(!result.pages[0].items.is_empty(), "the text is on the first page");
    assert_eq!(painted_pages(&result), 1, "the page of the text is the only one that shows anything");
}

/// [`lay_out`] on the 600 x 400 pages of this file: the result and the messages.
fn lay_out_plain(html: &str, config: FakePageConfig) -> (PagedLayoutResult, Vec<LayoutDebugMessage>) {
    let laid = lay_out(html, config);
    (laid.result, laid.messages)
}

/// The page cap is the default of a config, and a builder sets it.
#[test]
fn the_page_cap_is_ten_thousand_unless_set() {
    assert_eq!(FakePageConfig::new().max_pages, 10_000);
    assert_eq!(FakePageConfig::default().max_pages, 10_000);
    assert_eq!(FakePageConfig::new().with_max_pages(7).max_pages, 7);
    assert_eq!(FakePageConfig::new().with_unlimited_pages().max_pages, 0);
    // The other fields are where they were.
    let plain = FakePageConfig::new().with_footer_page_numbers();
    let mut capped = plain.clone().with_max_pages(3);
    capped.max_pages = plain.max_pages;
    assert_eq!(capped, plain);
}

/// A box of 1e12px has its one page with the text, the pages end at the cap, and
/// the layout says so: a warning, `truncated`.
#[test]
fn a_box_of_1e12px_is_cut_at_the_cap_with_a_warning() {
    let (result, messages) = lay_out_a4(&one_box("height:1e12px"), FakePageConfig::new());
    assert_eq!(result.pages.len(), 10_000);
    assert_eq!(
        result.breaks.len(),
        10_000,
        "the breaks end with the one that ends the last page made"
    );
    assert!(result.truncated);
    assert_eq!(pages_with_items(&result), 1);
    assert!(!result.pages[0].items.is_empty());
    assert!(result.total_content_height > 1.0e11, "the height of the document is still said");
    let warnings: Vec<&LayoutDebugMessage> = messages
        .iter()
        .filter(|m| m.message.contains("max_pages") && m.message.contains("10000"))
        .collect();
    assert_eq!(warnings.len(), 1, "{messages:?}");
    assert!(
        matches!(
            warnings[0].message_type,
            azul_css::LayoutDebugMessageType::Warning
        ),
        "a warning, not an info"
    );
}

/// The pages built are bounded by the cap, and of them only the page of the text
/// is cut from the canvas: the empty ones are handed on without that pass.
#[test]
fn the_display_lists_built_are_bounded_and_the_empty_pages_are_not_cut() {
    for style in ["height:1e12px", "height:1e39px", "min-height:1e39px"] {
        let (result, _) = lay_out_a4(&one_box(style), FakePageConfig::new());
        let slicing = result.slicing;
        assert_eq!(slicing.sliced + slicing.skipped, result.pages.len(), "{style}");
        assert!(result.pages.len() <= 10_000, "{style}: {} pages", result.pages.len());
        assert!(slicing.sliced <= 3, "{style}: {} pages cut from the canvas", slicing.sliced);
    }
    // Whatever the box, a page that is cut or handed on is one of the pages, and
    // there are never more than the cap's.
    for style in [
        "font-size:1e30px",
        "line-height:1e39px",
        "position:absolute;top:1e39px",
    ] {
        let (result, _) = lay_out_a4(&one_box(style), FakePageConfig::new());
        let slicing = result.slicing;
        assert_eq!(slicing.sliced + slicing.skipped, result.pages.len(), "{style}");
        assert!(result.pages.len() <= 10_000, "{style}: {} pages", result.pages.len());
    }
}

/// The pages between two pages of content are empty pages: they stay pages (the
/// count is the one it was, `a_gap_between_two_pages_of_content_keeps_its_pages`)
/// and are handed on without being cut from the canvas.
#[test]
fn the_empty_pages_between_two_pages_of_content_are_not_cut_from_the_canvas() {
    let html = "<html><body><p>first</p><div style=\"height:1e5px\"></div><p>last</p></body></html>";
    let (result, _) = lay_out_a4(html, FakePageConfig::new());
    let n = result.pages.len();
    assert_eq!(n, PAGES_OF_THE_GAP);
    // The two pages with content, and at most the page before each (an item on
    // the edge of a page may be on it).
    assert!(result.slicing.sliced <= 4, "{:?}", result.slicing);
    assert_eq!(result.slicing.skipped, n - result.slicing.sliced);
    assert!(!result.truncated);
}

/// A page with a footer is not an empty page: nothing is skipped, whatever the
/// body says.
#[test]
fn a_page_with_a_footer_is_not_skipped() {
    let (result, _) = lay_out_a4(
        &one_box("height:1e5px"),
        FakePageConfig::new().with_footer_page_numbers(),
    );
    assert_eq!(result.slicing.skipped, 0);
    assert_eq!(result.slicing.sliced, result.pages.len());
    assert!(result.pages.iter().all(|p| !p.items.is_empty()), "every page has its footer");
}

/// The cap cuts a long document after the pages it allows, those pages are the
/// first pages of the document without the cap, and the layout says so.
#[test]
fn a_document_over_the_cap_ends_with_the_pages_the_cap_allows() {
    let html = "<html><body style=\"margin: 0\"><div style=\"height:5000px; background:#ccc\"></div></body></html>";
    let (full, _) = lay_out_a4(html, FakePageConfig::new().with_unlimited_pages());
    assert!(full.pages.len() > 5);
    assert!(!full.truncated);
    let (capped, messages) = lay_out_a4(html, FakePageConfig::new().with_max_pages(3));
    assert_eq!(capped.pages.len(), 3);
    assert!(capped.truncated);
    assert_eq!(capped.breaks, full.breaks[..3].to_vec());
    for (page, whole) in capped.pages.iter().zip(&full.pages) {
        assert_eq!(
            format!("{:?}", page.items),
            format!("{:?}", whole.items),
            "a page of the cut document is its page of the whole"
        );
    }
    assert!(messages
        .iter()
        .any(|m| m.message.contains("max_pages") && m.message.contains("3 pages")));
}

/// A document of exactly the cap's pages is not cut and is not told about it;
/// one page more is.
#[test]
fn a_document_of_exactly_the_cap_is_not_truncated() {
    let (exact, messages) = lay_out_plain(THREE_PAGES, FakePageConfig::new().with_max_pages(3));
    assert_eq!(exact.pages.len(), 3);
    assert!(!exact.truncated);
    assert!(!messages.iter().any(|m| m.message.contains("max_pages")), "{messages:?}");

    let (cut, messages) = lay_out_plain(THREE_PAGES, FakePageConfig::new().with_max_pages(2));
    assert_eq!(cut.pages.len(), 2);
    assert!(cut.truncated);
    assert!(messages.iter().any(|m| m.message.contains("max_pages")));

    let (one, _) = lay_out_plain(THREE_PAGES, FakePageConfig::new().with_max_pages(1));
    assert_eq!(one.pages.len(), 1);
    assert_eq!(one.breaks.len(), 1);
    assert!(one.truncated);

    let (all, _) = lay_out_plain(THREE_PAGES, FakePageConfig::new().with_unlimited_pages());
    assert_eq!(all.pages.len(), 3);
    assert!(!all.truncated);
}

/// The precalculation-only entry has the cap too: its page count is the cap's.
#[test]
fn the_pagination_without_pages_is_capped_too() {
    let styled_dom = Dom::from_xml_string(one_box("height:1e12px"));
    let loader = PathLoader::new();
    let mut font_manager = FontManager::<FontRef>::new(FcFontCache::default()).expect("manager");
    let info = compute_document_pagination(
        &mut LayoutCache::default(),
        &mut TextLayoutCache::new(),
        FragmentationContext::new_paged(A4),
        &styled_dom,
        LogicalRect::new(LogicalPosition::zero(), A4),
        &mut font_manager,
        &BTreeMap::new(),
        &mut Some(Vec::new()),
        None,
        &RendererResources::default(),
        IdNamespace(0),
        DomId::ROOT_ID,
        move |bytes, index| loader.load_font_shared(bytes, index),
        FakePageConfig::new(),
        &ImageCache::default(),
        GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
    )
    .expect("the pagination");
    assert_eq!(info.page_count, 10_000);
}
