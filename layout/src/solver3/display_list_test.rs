//! The text of a page's header and footer in the slicing entry points that take
//! a `RendererResources` and no font manager: drawn from a font registered in
//! it, and never dropped without a word when none is.
//!
//! A paged layout draws the same text with the document's font manager
//! (`paged_layout_test.rs`).
//!
//! And the page cap of the slicer (`SlicerConfig::max_pages`): the pages after
//! the cap are not made, and a page that nothing of the canvas reaches is
//! handed on without being cut from it, as the page that cutting would have made.

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

/// A text of size zero, or none at all, or blank, is nothing to draw: not a
/// finding (the text of a blank header is not missing a font).
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
    for blank in ["", " ", "\t \n"] {
        assert!(
            setter
                .set_text(blank, band, &HeaderFooterConfig::default(), TextAlignment::Center)
                .is_empty(),
            "{blank:?}"
        );
    }
    assert!(!azul_core::diagnostics::any_contains(REPORT));
}

/// Blank text draws nothing where a font is registered, as it draws nothing
/// where none is (a run of space glyphs is no header).
#[test]
fn blank_text_draws_nothing_where_a_font_is_registered() {
    let resources = resources_with_a_font();
    let band = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(100.0, 30.0));
    let mut setter = RegisteredFontText(&resources);

    for blank in [" ", "   ", "\t \n"] {
        assert!(
            setter
                .set_text(blank, band, &HeaderFooterConfig::default(), TextAlignment::Center)
                .is_empty(),
            "{blank:?}"
        );
    }
}

/// A size that is not a number, or not finite, makes every coordinate of the
/// text NaN: nothing is drawn at it, with a font registered or not.
#[test]
fn a_size_that_is_no_size_draws_nothing_where_a_font_is_registered() {
    let resources = resources_with_a_font();
    let band = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(100.0, 30.0));
    let mut setter = RegisteredFontText(&resources);

    for size in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -3.0] {
        let sized = HeaderFooterConfig {
            font_size: size,
            ..HeaderFooterConfig::default()
        };
        assert!(
            setter
                .set_text("Header", band, &sized, TextAlignment::Center)
                .is_empty(),
            "size {size}"
        );
    }
}

/// What a size is: a positive, finite number of px.
#[test]
fn a_drawable_size_is_positive_and_finite() {
    assert!(is_drawable_size(10.0));
    assert!(is_drawable_size(f32::MIN_POSITIVE));
    for size in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(!is_drawable_size(size), "{size}");
    }
}

/// A tiny deterministic generator: these tests need no crate for their numbers.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }
}

fn black() -> ColorU {
    ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    }
}

fn rect_at(y: f32, height: f32) -> DisplayListItem {
    DisplayListItem::Rect {
        bounds: LogicalRect::new(LogicalPosition::new(0.0, y), LogicalSize::new(50.0, height))
            .into(),
        color: black(),
        border_radius: BorderRadius::default(),
    }
}

fn underline_at(y: f32, height: f32) -> DisplayListItem {
    DisplayListItem::Underline {
        bounds: LogicalRect::new(LogicalPosition::new(0.0, y), LogicalSize::new(50.0, height))
            .into(),
        color: black(),
        thickness: 1.0,
    }
}

/// A one-glyph text run whose baseline is at `y`, in a line box that is as tall
/// as the paragraph around it (a text run's can be).
fn text_at(y: f32) -> DisplayListItem {
    let paragraph = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(50.0, 1500.0));
    DisplayListItem::Text {
        glyphs: vec![GlyphInstance {
            index: 1,
            point: LogicalPosition::new(0.0, y),
            size: LogicalSize::zero(),
        }],
        font_hash: FontHash::from_hash(1),
        font_size_px: 10.0,
        color: black(),
        clip_rect: paragraph.into(),
        line_box: paragraph.into(),
        source_node_index: None,
    }
}

fn hit_test_at(y: f32, height: f32) -> DisplayListItem {
    DisplayListItem::HitTestArea {
        bounds: LogicalRect::new(LogicalPosition::new(0.0, y), LogicalSize::new(50.0, height))
            .into(),
        tag: (1, 0x0100),
    }
}

/// The canvas of `items`, in the order given.
fn canvas_of(items: &[DisplayListItem]) -> DisplayList {
    let mut list = DisplayList::default();
    for item in items {
        list.items.push(item.clone());
        list.node_mapping.push(None);
    }
    list
}

/// What cutting every item of `canvas` against each page of `breaks` gives, page
/// by page, as text: the slicer's own clipping and nothing else.
fn pages_by_clipping(
    canvas: &DisplayList,
    config: &SlicerConfig,
    breaks: &[page_breaks::PageBreakPosition],
) -> Vec<String> {
    let total = paged_flow_height(canvas, &config.footnotes);
    let mut spans = page_breaks::page_spans(breaks, total);
    if spans.is_empty() {
        let constraints = page_breaks::PageConstraints::from_slicer_config(config);
        spans.push((0.0, total.max(constraints.first_page_content_height)));
    }
    spans
        .iter()
        .map(|&(top, bottom)| {
            let items: Vec<DisplayListItem> = canvas
                .items
                .iter()
                .filter_map(|item| clip_and_offset_display_item(item, top, bottom))
                .collect();
            format!("{items:?}")
        })
        .collect()
}

/// The pages of `canvas` as `paginate_pages_impl` hands them on, as text, with
/// its report.
fn pages_by_slicing(
    canvas: DisplayList,
    config: &SlicerConfig,
    breaks: &[page_breaks::PageBreakPosition],
) -> (Vec<String>, PageSlicingReport) {
    let resources = RendererResources::default();
    let mut pages = Vec::new();
    let report = paginate_pages_impl(
        canvas,
        config,
        breaks,
        &mut RegisteredFontText(&resources),
        None,
        |page| {
            pages.push(format!("{:?}", page.items));
            Ok(())
        },
    )
    .expect("the pages");
    (pages, report)
}

/// A page that no item of the canvas reaches is handed on empty without being
/// cut from it, and every page is the page that clipping every item against it
/// gives: items that touch a page's edge, items with no height, items on no page.
#[test]
fn skipped_pages_are_the_pages_slicing_would_have_made() {
    let mut rng = Lcg(7);
    let mut skipped_somewhere = false;
    for round in 0..60 {
        let mut items = Vec::new();
        for _ in 0..1 + rng.next() % 8 {
            // A grid of a quarter page: items touch the edges of pages exactly.
            let y = (rng.next() % 60) as f32 * 25.0;
            let height = (rng.next() % 6) as f32 * 25.0;
            items.push(match rng.next() % 4 {
                0 => rect_at(y, height),
                1 => underline_at(y, height),
                2 => text_at(y),
                // State the slicer does not carry over: no page shows it, and no
                // page is cut for it.
                _ => DisplayListItem::PopFilter,
            });
        }
        let config = SlicerConfig::simple(100.0);
        let canvas = canvas_of(&items);
        let breaks = page_breaks::compute_page_breaks_from_display_list(
            &canvas,
            &page_breaks::PageConstraints::from_slicer_config(&config),
        );
        let (pages, report) = pages_by_slicing(canvas.clone(), &config, &breaks);
        assert_eq!(pages, pages_by_clipping(&canvas, &config, &breaks), "round {round}");
        assert_eq!(report.sliced + report.skipped, pages.len(), "round {round}");
        assert!(!report.truncated, "round {round}");
        skipped_somewhere |= report.skipped > 0;
    }
    assert!(skipped_somewhere, "harness: some canvas has an empty page between its items");
}

/// An item whose box is not a number is on every page (the clipping cannot tell
/// it is not), so no page is skipped.
#[test]
fn an_item_with_no_position_is_on_every_page_and_nothing_is_skipped() {
    let items = [rect_at(0.0, 10.0), rect_at(f32::NAN, 10.0), rect_at(950.0, 10.0)];
    let config = SlicerConfig::simple(100.0);
    let canvas = canvas_of(&items);
    let breaks = page_breaks::compute_page_breaks_from_display_list(
        &canvas,
        &page_breaks::PageConstraints::from_slicer_config(&config),
    );
    let (pages, report) = pages_by_slicing(canvas.clone(), &config, &breaks);
    assert_eq!(pages, pages_by_clipping(&canvas, &config, &breaks));
    assert_eq!(report.skipped, 0);
    assert_eq!(report.sliced, pages.len());
}

/// The hit-test area of a box that spans pages paints nothing: a page that shows
/// nothing else is an empty page, and one with something painted on it keeps the
/// hit-test areas it had.
#[test]
fn a_page_with_only_a_hit_test_area_is_an_empty_page() {
    // The area of `<body>`, from the top of the canvas to the bottom; a small
    // painted box on the first page and another on the last.
    let canvas = canvas_of(&[
        hit_test_at(0.0, 1000.0),
        rect_at(10.0, 10.0),
        rect_at(950.0, 10.0),
    ]);
    let config = SlicerConfig::simple(100.0);
    let breaks = page_breaks::compute_page_breaks_from_display_list(
        &canvas,
        &page_breaks::PageConstraints::from_slicer_config(&config),
    );
    let (pages, report) = pages_by_slicing(canvas.clone(), &config, &breaks);
    assert_eq!(pages.len(), 10);
    assert_eq!(pages[1], "[]", "the areas of the pages between show nothing");
    assert_eq!(pages[5], "[]");
    assert!(pages[0].contains("HitTestArea") && pages[0].contains("Rect"), "{}", pages[0]);
    assert!(pages[9].contains("HitTestArea") && pages[9].contains("Rect"), "{}", pages[9]);
    assert!(report.skipped >= 6, "{report:?}");
    assert_eq!(report.sliced + report.skipped, 10);
}

/// Two small items far apart: the pages between them are not cut from the canvas
/// (a page at each edge of a gap is: an item on the edge of a page may be on it).
#[test]
fn the_pages_of_a_gap_are_not_cut_from_the_canvas() {
    let canvas = canvas_of(&[rect_at(0.0, 10.0), rect_at(5000.0, 10.0)]);
    let config = SlicerConfig::simple(100.0);
    let breaks = page_breaks::compute_page_breaks_from_display_list(
        &canvas,
        &page_breaks::PageConstraints::from_slicer_config(&config),
    );
    let (pages, report) = pages_by_slicing(canvas.clone(), &config, &breaks);
    assert_eq!(pages.len(), 51);
    assert_eq!(pages, pages_by_clipping(&canvas, &config, &breaks));
    assert!(report.sliced <= 4, "{report:?}");
    assert_eq!(report.skipped, pages.len() - report.sliced);
}

/// The cap drops the pages after it from a break analysis made without one: the
/// pages that are made are the first pages of the uncapped slicing.
#[test]
fn pages_after_the_cap_are_not_made() {
    let items: Vec<DisplayListItem> = (0..30).map(|i| rect_at(i as f32 * 100.0, 50.0)).collect();
    let canvas = canvas_of(&items);
    let uncapped = SlicerConfig::simple(100.0).with_max_pages(0);
    let breaks = page_breaks::compute_page_breaks_from_display_list(
        &canvas,
        &page_breaks::PageConstraints::from_slicer_config(&uncapped),
    );
    let (all, report) = pages_by_slicing(canvas.clone(), &uncapped, &breaks);
    assert_eq!(all.len(), 30);
    assert!(!report.truncated);

    let capped = SlicerConfig::simple(100.0).with_max_pages(5);
    let (first, report) = pages_by_slicing(canvas, &capped, &breaks);
    assert_eq!(first, all[..5].to_vec());
    assert!(report.truncated);
}

/// The slicing entry point that makes its own breaks makes as many as the cap
/// allows and no more, for a canvas of an absurd height, and says so once.
#[test]
fn the_slicer_that_makes_its_own_breaks_stops_at_the_cap() {
    let _lock = azul_core::diagnostics::test_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    azul_core::diagnostics::clear();

    let canvas = canvas_of(&[rect_at(0.0, 1.0e12)]);
    let config = SlicerConfig::simple(100.0).with_max_pages(8);
    let pages = paginate_display_list_with_slicer_and_breaks(
        canvas,
        &config,
        &RendererResources::default(),
    )
    .expect("the pages");
    assert_eq!(pages.len(), 8);
    assert!(
        azul_core::diagnostics::any_contains("SlicerConfig::max_pages"),
        "the cut is reported: {:?}",
        azul_core::diagnostics::recorded()
    );
}
