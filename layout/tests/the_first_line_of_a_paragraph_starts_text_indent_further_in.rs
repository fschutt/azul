//! The first line of a paragraph starts `text-indent` further in.
//!
//! Reported 2026-10-02 by the PDF OCR work: `text-indent: 177pt` left the
//! first line at the same x as the others, upright or italic, so its
//! renderer indented first lines with no-break spaces instead. CSS Text 3
//! §8.1: the indent is applied to the first line of the block container
//! (a margin on the line box's start edge); `text-indent` is inherited.

use std::collections::{BTreeMap, HashMap};

use azul_core::{
    dom::DomId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
};
use azul_layout::{
    font::loading::build_font_cache,
    font_traits::{FontManager, TextLayoutCache},
    paged::FragmentationContext,
    solver3::{
        display_list::DisplayListItem, paged_layout::layout_document_paged_with_config,
        pagination::FakePageConfig,
    },
    text3::default::PathLoader,
    window::LayoutWindow,
    xml::DomXmlExt,
    Solver3LayoutCache,
};

use crate::table_markup::{glyph_runs, laid_out, prose};

/// `body` laid out in an 800 x 600 window under `style`, margins and padding zeroed.
fn page(style: &str, body: &str) -> LayoutWindow {
    laid_out(
        &format!(
            "<html><head><style>* {{ margin: 0; padding: 0; }} {style}</style></head>\
             <body>{body}</body></html>"
        ),
        800.0,
        600.0,
    )
}

/// The leftmost glyph pen of each line, top to bottom.
fn line_starts(lw: &LayoutWindow) -> Vec<f32> {
    let mut lines: Vec<(f32, f32)> = Vec::new(); // (y, min x)
    for (x, y) in glyph_runs(lw).into_iter().flatten() {
        match lines.iter_mut().find(|(ly, _)| (*ly - y).abs() < 2.0) {
            Some(line) => line.1 = line.1.min(x),
            None => lines.push((y, x)),
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    lines.into_iter().map(|(_, x)| x).collect()
}

fn assert_indented(lw: &LayoutWindow, indent_px: f32, what: &str) {
    let starts = line_starts(lw);
    assert!(starts.len() >= 2, "{what}: premise, the prose wraps: {starts:?}");
    assert!(
        (starts[0] - indent_px).abs() <= 1.0,
        "{what}: the first line starts {indent_px}px in: {starts:?}"
    );
    assert!(
        starts[1..].iter().all(|x| x.abs() <= 1.0),
        "{what}: the other lines start at the edge: {starts:?}"
    );
}

#[test]
fn a_paragraph_indents_its_first_line() {
    // 177pt = 236px.
    let lw = page(
        "p { width: 600px; text-indent: 177pt }",
        &format!("<p>{}</p>", prose(60)),
    );
    assert_indented(&lw, 236.0, "upright");
}

#[test]
fn an_italic_paragraph_indents_its_first_line() {
    let lw = page(
        "p { width: 600px; text-indent: 177pt; font-style: italic }",
        &format!("<p>{}</p>", prose(60)),
    );
    assert_indented(&lw, 236.0, "italic");
}

#[test]
fn a_paragraph_inherits_the_indent_of_its_container() {
    let lw = page(
        "div { text-indent: 40px } p { width: 600px }",
        &format!("<div><p>{}</p></div>", prose(60)),
    );
    assert_indented(&lw, 40.0, "inherited");
}

#[test]
fn an_inline_style_indents_the_first_line() {
    let lw = page(
        "",
        &format!("<p style=\"width: 600px; text-indent: 177pt\">{}</p>", prose(60)),
    );
    assert_indented(&lw, 236.0, "inline style");
}

/// The same, on paper: the paged layout printpdf's HTML renderer draws
/// from. Every glyph pen of the paged display lists, page by page.
pub(crate) fn paged_pens(style: &str, body: &str) -> Vec<(f32, f32)> {
    let html = format!(
        "<html><head><style>* {{ margin: 0; padding: 0; }} {style}</style></head>\
         <body>{body}</body></html>"
    );
    let styled_dom = azul_core::dom::Dom::from_xml_string(&html);
    let mut font_manager = FontManager::new(build_font_cache()).expect("a font manager");
    let mut layout_cache = Solver3LayoutCache {
        scroll_ids: HashMap::new(),
        ..Default::default()
    };
    let mut text_cache = TextLayoutCache::new();
    let content_size = LogicalSize::new(800.0, 600.0);
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let display_lists = layout_document_paged_with_config(
        &mut layout_cache,
        &mut text_cache,
        FragmentationContext::new_paged(content_size),
        &styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &mut None,
        None,
        &RendererResources::default(),
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        FakePageConfig::new(),
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
    )
    .expect("the page lays out");
    display_lists
        .iter()
        .flat_map(|dl| dl.items.iter())
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => {
                Some(glyphs.iter().map(|g| (g.point.x, g.point.y)).collect::<Vec<_>>())
            }
            _ => None,
        })
        .flatten()
        .collect()
}

/// The leftmost pen of each line of `pens`, top to bottom.
fn starts_of(pens: Vec<(f32, f32)>) -> Vec<f32> {
    let mut lines: Vec<(f32, f32)> = Vec::new();
    for (x, y) in pens {
        match lines.iter_mut().find(|(ly, _)| (*ly - y).abs() < 2.0) {
            Some(line) => line.1 = line.1.min(x),
            None => lines.push((y, x)),
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    lines.into_iter().map(|(_, x)| x).collect()
}

#[test]
fn a_paragraph_indents_its_first_line_on_paper() {
    for italic in [false, true] {
        let style = if italic { "font-style: italic;" } else { "" };
        let starts = starts_of(paged_pens(
            &format!("p {{ width: 600px; text-indent: 177pt; {style} }}"),
            &format!("<p>{}</p>", prose(60)),
        ));
        assert!(starts.len() >= 2, "italic={italic}: premise, the prose wraps: {starts:?}");
        assert!(
            (starts[0] - 236.0).abs() <= 1.0,
            "italic={italic}: the first line starts 177pt (236px) in on paper: {starts:?}"
        );
        assert!(
            starts[1..].iter().all(|x| x.abs() <= 1.0),
            "italic={italic}: the other lines start at the edge: {starts:?}"
        );
    }
}

/// The markup pdfocr's html2pdf writes: an absolutely positioned region on a
/// clipped page box, justified when it holds three lines or more, around a
/// paragraph with an inline `text-indent`.
#[test]
fn a_justified_paragraph_in_a_positioned_region_indents_its_first_line_on_paper() {
    for (align, italic) in [("left", false), ("justify", false), ("justify", true)] {
        let style = ".page { position: relative; width: 595pt; height: 842pt; overflow: hidden; } \
                     .region { position: absolute; color: #000; font-family: Helvetica, Arial, \
                     sans-serif; overflow: hidden; }";
        let italic_css = if italic { " font-style: italic;" } else { "" };
        let body = format!(
            "<div class=\"page\"><div class=\"region\" style=\"left: 10%; top: 10%; width: 80%; \
             font-size: 11pt; line-height: 14pt; text-align: {align};{italic_css}\">\
             <p style=\"text-indent: 177.0pt;\">{}</p></div></div>",
            prose(60)
        );
        let starts = starts_of(paged_pens(style, &body));
        // The region's left edge: 10% of the 595pt page = 59.5pt = 79.33px.
        let edge = 595.0 * 0.1 * 96.0 / 72.0;
        assert!(starts.len() >= 3, "{align} italic={italic}: premise, the prose wraps: {starts:?}");
        assert!(
            (starts[0] - (edge + 236.0)).abs() <= 1.0,
            "{align} italic={italic}: the first line starts 177pt in from the region's edge: {starts:?}"
        );
        assert!(
            starts[1..].iter().all(|x| (x - edge).abs() <= 1.0),
            "{align} italic={italic}: the other lines start at the region's edge: {starts:?}"
        );
    }
}
