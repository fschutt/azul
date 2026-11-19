//! `float: footnote` on paged media: the note goes to the bottom of the
//! page its place in the flow lands on.
//!
//! CSS Generated Content for Paged Media, "Footnotes": "An element becomes
//! a footnote by applying `float: footnote` to that element. [...] The
//! footnote element is removed from the flow"; the footnote area is at the
//! bottom of the page. `footnote-policy: line`, this engine's `auto`: a
//! line whose note does not fit under it goes to the next page with it.
//!
//! The engine parsed `float: footnote` as an error, so the note stayed in
//! its paragraph, in the middle of the sentence.
//!
//! Pages are 800 x 200. Every line is 20px (a `<br/>` ends each), every
//! note has a height of its own and a background colour of its own to find
//! it by on the page; the flow's blocks are blue.

use std::collections::{BTreeMap, HashMap};

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    font::loading::build_font_cache,
    font_traits::{FontManager, TextLayoutCache},
    paged::FragmentationContext,
    solver3::{
        display_list::{DisplayList, DisplayListItem},
        paged_layout::{
            layout_document_paged_streaming, layout_document_paged_v2, PagedLayoutResult,
        },
        pagination::FakePageConfig,
    },
    text3::default::PathLoader,
    xml::DomXmlExt,
    Solver3LayoutCache,
};

use crate::{pagination_dom_breaks::paginate, table_markup::laid_out};

const PAGE: f32 = 200.0;
const RED: (u8, u8, u8) = (255, 0, 0);
const GREEN: (u8, u8, u8) = (0, 255, 0);
const BLUE: (u8, u8, u8) = (0, 0, 255);

const STYLE: &str = "* { margin: 0; padding: 0; } \
     p { line-height: 20px } \
     .b { background: #0000ff } \
     .fn { float: footnote } \
     #n1 { background: #ff0000 } #n2 { background: #00ff00 }";

fn html(body: &str) -> String {
    format!("<html><head><style>{STYLE}</style></head><body>{body}</body></html>")
}

/// A note `id`, `height` px tall.
fn note(id: &str, height: u32) -> String {
    format!("<span id=\"{id}\" class=\"fn\" style=\"height: {height}px\">note</span>")
}

/// `n` blue blocks, each `height` px tall.
fn blocks(n: usize, height: u32) -> String {
    (0..n)
        .map(|_| format!("<div class=\"b\" style=\"height: {height}px\"></div>"))
        .collect()
}

/// `html` laid out on 800 x 200 pages (no page decoration, the plain break
/// algorithm): the pages, the breaks, the cache and the styled DOM.
pub(crate) fn pages(html: &str) -> (PagedLayoutResult, Solver3LayoutCache, StyledDom) {
    pages_with(html, FakePageConfig::new(), false)
}

/// The page setup of an HTML-to-PDF export: every break rule on
/// (`break-inside: avoid`, widows and orphans, atomic lines and table rows,
/// repeated table heads).
fn export_config() -> FakePageConfig {
    let mut config = FakePageConfig::new();
    config.break_policy = azul_layout::solver3::page_breaks::BreakPolicy {
        honor_break_inside: true,
        widows_orphans: true,
        atomic_lines: true,
        atomic_table_rows: true,
        repeat_table_headers: true,
        ..Default::default()
    };
    config
}

/// `html` on 800 x 200 pages under `config`; `streaming` hands each page to
/// a sink as it is sliced (`layout_document_paged_streaming`, the entry
/// point of an export) instead of collecting them.
fn pages_with(
    html: &str,
    config: FakePageConfig,
    streaming: bool,
) -> (PagedLayoutResult, Solver3LayoutCache, StyledDom) {
    let styled_dom = Dom::from_xml_string(html);
    let mut font_manager = FontManager::new(build_font_cache()).expect("FontManager");
    let mut cache = Solver3LayoutCache {
        tree: None,
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: HashMap::new(),
        scroll_id_to_node_id: HashMap::new(),
        counters: HashMap::new(),
        float_cache: HashMap::new(),
        cache_map: Default::default(),
        previous_positions: Vec::new(),
        cached_display_list: None,
        prev_dom_ptr: 0,
        prev_viewport: LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::zero(),
        },
        ..Default::default()
    };
    let mut text_cache = TextLayoutCache::new();
    let content_size = LogicalSize::new(800.0, PAGE);
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    let time = azul_core::task::GetSystemTimeCallback {
        cb: azul_core::task::get_system_time_libstd,
    };
    let result = if streaming {
        let mut pages = Vec::new();
        let streamed = layout_document_paged_streaming(
            &mut cache,
            &mut text_cache,
            FragmentationContext::new_paged(content_size),
            &styled_dom,
            viewport,
            &mut font_manager,
            &BTreeMap::new(),
            &mut Some(Vec::new()),
            None,
            &RendererResources::default(),
            azul_core::resources::IdNamespace(0),
            DomId::ROOT_ID,
            font_loader,
            config,
            &azul_core::resources::ImageCache::default(),
            time,
            false,
            &mut |page| {
                pages.push(page);
                Ok(())
            },
        )
        .expect("the document paginates");
        PagedLayoutResult {
            pages,
            breaks: streamed.breaks,
            total_content_height: streamed.total_content_height,
        }
    } else {
        layout_document_paged_v2(
            &mut cache,
            &mut text_cache,
            FragmentationContext::new_paged(content_size),
            &styled_dom,
            viewport,
            &mut font_manager,
            &BTreeMap::new(),
            &mut Some(Vec::new()),
            None,
            &RendererResources::default(),
            azul_core::resources::IdNamespace(0),
            DomId::ROOT_ID,
            font_loader,
            config,
            &azul_core::resources::ImageCache::default(),
            time,
            false,
        )
        .expect("the document paginates")
    };
    (result, cache, styled_dom)
}

/// The rects painted in exactly `color` on `page`, as (top, bottom, left,
/// width).
pub(crate) fn rects(page: &DisplayList, (r, g, b): (u8, u8, u8)) -> Vec<(f32, f32, f32, f32)> {
    page.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Rect { bounds, color, .. }
                if color.r == r && color.g == g && color.b == b && color.a > 0 =>
            {
                let b = bounds.inner();
                Some((
                    b.origin.y,
                    b.origin.y + b.size.height,
                    b.origin.x,
                    b.size.width,
                ))
            }
            _ => None,
        })
        .collect()
}

fn break_ys(result: &PagedLayoutResult) -> Vec<f32> {
    result.breaks.iter().map(|b| b.y).collect()
}

/// Where the layout put the box of the element `id` on the canvas.
fn canvas_y(cache: &Solver3LayoutCache, dom: &StyledDom, id: &str) -> f32 {
    let n = dom
        .node_data
        .as_ref()
        .iter()
        .position(|nd| {
            nd.get_ids_and_classes().iter().any(
                |c| matches!(c, azul_core::dom::IdOrClass::Id(s) if s.as_str() == id),
            )
        })
        .unwrap_or_else(|| panic!("no element with id {id}"));
    let tree = cache.tree.as_deref().expect("a layout tree");
    let idx = *tree
        .dom_to_layout
        .get(&NodeId::new(n))
        .and_then(|v| v.first())
        .expect("the element has a layout box");
    cache.calculated_positions[idx.index()].y
}

#[test]
fn a_note_is_drawn_at_the_bottom_of_the_page_its_line_is_on() {
    // Three lines (60px) and six 40px blocks: 300px of flow. The note is
    // written in the second line.
    let (result, _, _) = pages(&html(&format!(
        "<p>one<br/>two{}<br/>three</p>{}",
        note("n1", 30),
        blocks(6, 40)
    )));
    assert_eq!(result.pages.len(), 2, "breaks: {:?}", break_ys(&result));
    assert_eq!(
        rects(&result.pages[0], RED),
        vec![(170.0, 200.0, 0.0, 800.0)],
        "the note ends at the bottom edge of the first page, as wide as the page"
    );
    assert_eq!(rects(&result.pages[1], RED), vec![], "and is drawn once");
    // Its text goes with its box: three lines of the paragraph above 60px,
    // the note's line inside the note, nothing in between.
    let text_tops: Vec<f32> = result.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { line_box, .. } => Some(line_box.inner().origin.y),
            _ => None,
        })
        .collect();
    assert!(
        text_tops.iter().any(|y| *y >= 170.0 && *y < 200.0),
        "the note's text is drawn in the note: {text_tops:?}"
    );
    assert!(
        text_tops.iter().all(|y| *y < 60.0 || *y >= 170.0),
        "and not where the note was taken out of the flow: {text_tops:?}"
    );
}

#[test]
fn the_flow_of_a_page_ends_above_its_notes() {
    let (result, _, _) = pages(&html(&format!(
        "<p>one<br/>two{}<br/>three</p>{}",
        note("n1", 30),
        blocks(6, 40)
    )));
    // 200px less the 30px note.
    assert_eq!(break_ys(&result), vec![170.0]);
    let flow_bottom = rects(&result.pages[0], BLUE)
        .into_iter()
        .map(|(_, bottom, _, _)| bottom)
        .fold(0.0_f32, f32::max);
    assert!(
        flow_bottom > 100.0 && flow_bottom <= 170.0,
        "the blocks of the first page end above its note: {flow_bottom}"
    );
    // The rest of the flow: 300 - 170 = 130px on the second page.
    let rest = rects(&result.pages[1], BLUE)
        .into_iter()
        .map(|(_, bottom, _, _)| bottom)
        .fold(0.0_f32, f32::max);
    assert_eq!(rest, 130.0);
}

#[test]
fn the_notes_of_a_page_stack_in_the_order_of_their_anchors() {
    let (result, _, _) = pages(&html(&format!(
        "<p>one{}<br/>two{}<br/>three</p>{}",
        note("n1", 30),
        note("n2", 20),
        blocks(6, 40)
    )));
    assert_eq!(break_ys(&result), vec![150.0]);
    assert_eq!(rects(&result.pages[0], RED), vec![(150.0, 180.0, 0.0, 800.0)]);
    assert_eq!(rects(&result.pages[0], GREEN), vec![(180.0, 200.0, 0.0, 800.0)]);
}

#[test]
fn a_note_anchored_on_the_second_page_is_drawn_at_its_bottom() {
    // 250px of blocks, then the paragraph: its line is on the second page.
    let (result, _, _) = pages(&html(&format!(
        "{}<p>one{}</p>",
        blocks(5, 50),
        note("n1", 30)
    )));
    assert_eq!(break_ys(&result), vec![200.0], "the first page is not shortened");
    assert_eq!(rects(&result.pages[0], RED), vec![]);
    assert_eq!(rects(&result.pages[1], RED), vec![(170.0, 200.0, 0.0, 800.0)]);
}

#[test]
fn a_line_whose_note_does_not_fit_under_it_goes_to_the_next_page_with_it() {
    // The line stands at 150..170: it fits the page, but not above its
    // 50px note (170 > 200 - 50).
    let (result, _, _) = pages(&html(&format!(
        "{}<p>one{}</p>{}",
        blocks(3, 50),
        note("n1", 50),
        blocks(1, 40)
    )));
    assert_eq!(break_ys(&result), vec![150.0], "the page ends before the line");
    assert_eq!(rects(&result.pages[0], RED), vec![]);
    assert_eq!(rects(&result.pages[1], RED), vec![(150.0, 200.0, 0.0, 800.0)]);
}

#[test]
fn a_block_level_note_takes_no_room_in_the_flow() {
    let (result, cache, dom) = pages(&html(
        "<div id=\"a\" class=\"b\" style=\"height: 40px\"></div>\
         <div id=\"n1\" class=\"fn\" style=\"height: 30px\">note</div>\
         <div id=\"c\" class=\"b\" style=\"height: 40px\"></div>",
    ));
    assert_eq!(canvas_y(&cache, &dom, "c"), 40.0, "the block after the note follows the one before");
    assert_eq!(result.pages.len(), 1);
    assert_eq!(rects(&result.pages[0], RED), vec![(170.0, 200.0, 0.0, 800.0)]);
}

#[test]
fn a_note_in_the_last_line_makes_no_page_of_its_own() {
    // The note is parked at its line, at the end of the flow: its box
    // reaches below the last line, which is no content to cut pages from.
    let (result, _, _) = pages(&html(&format!("{}<p>one{}</p>", blocks(2, 50), note("n1", 150))));
    assert_eq!(break_ys(&result), vec![100.0], "the line moves over with its note");
    assert_eq!(result.pages.len(), 2);
    assert_eq!(rects(&result.pages[1], RED), vec![(50.0, 200.0, 0.0, 800.0)]);
}

#[test]
fn a_document_without_notes_is_cut_as_it_was() {
    // The same flow without `float: footnote` anywhere: the plain interval
    // breaks, every 200px.
    let (_, _, pagination) = paginate(
        &html(&format!("<p>one<br/>two<br/>three</p>{}", blocks(11, 40))),
        800.0,
        PAGE,
    );
    let ys: Vec<f32> = pagination.breaks.as_ref().iter().map(|b| b.y).collect();
    assert_eq!(ys, vec![200.0, 400.0]);
    assert_eq!(pagination.page_count, 3);
    assert!(pagination
        .breaks
        .as_ref()
        .iter()
        .all(|b| b.kind == azul_layout::BreakKind::Interval));
}

#[test]
fn in_a_window_a_footnote_stays_in_the_flow() {
    // Continuous media: `float: footnote` behaves as `float: none` (a choice
    // of this engine; the module is for paged media).
    let lw = laid_out(
        &html(
            "<div id=\"a\" class=\"b\" style=\"height: 40px\"></div>\
             <div id=\"n1\" class=\"fn\" style=\"height: 30px\">note</div>\
             <div id=\"c\" class=\"b\" style=\"height: 40px\"></div>",
        ),
        800.0,
        600.0,
    );
    let (n1, c) = (
        crate::table_markup::rect(&lw, "n1"),
        crate::table_markup::rect(&lw, "c"),
    );
    assert_eq!((n1.origin.y, n1.size.height), (40.0, 30.0));
    assert_eq!(c.origin.y, 70.0);
}

/// The text blocks of `page` as a PDF backend reads them: the bounds of
/// every `TextLayout` item and how many shaped items its payload holds as a
/// `UnifiedLayout` - `None` when the payload is anything else, which such a
/// backend cannot draw.
fn text_blocks(page: &DisplayList) -> Vec<(LogicalRect, Option<usize>)> {
    page.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::TextLayout { layout, bounds, .. } => Some((
                *bounds.inner(),
                layout
                    .downcast_ref::<azul_layout::text3::cache::UnifiedLayout>()
                    .map(|l| l.items.len()),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn an_export_draws_the_note_at_the_bottom_of_its_page_above_nothing() {
    // The way an HTML-to-PDF export lays a document out: the streaming entry
    // point, paged media, every break rule on. A PDF is drawn from the
    // `TextLayout` items of each page, and only from those whose payload is
    // a `UnifiedLayout` - the note's kept the cached form of the canvas,
    // and its text was in no page of the PDF.
    let (result, _, _) = pages_with(
        &html(&format!(
            "<p>one<br/>two<span class=\"fn\"><sup>1</sup> A first note</span><br/>three</p>{}",
            blocks(6, 40)
        )),
        export_config(),
        true,
    );
    assert_eq!(result.pages.len(), 2, "breaks: {:?}", break_ys(&result));
    let flow_end = break_ys(&result)[0];
    let blocks_of_page = text_blocks(&result.pages[0]);
    for (bounds, items) in &blocks_of_page {
        assert!(
            items.is_some_and(|n| n > 0),
            "a text block of the page ({bounds:?}) is not in the form a PDF backend draws"
        );
    }
    // The paragraph above, the note below: its 14 characters, from where
    // the page's flow ends to the bottom edge at the latest.
    let notes: Vec<&(LogicalRect, Option<usize>)> = blocks_of_page
        .iter()
        .filter(|(b, _)| b.origin.y >= flow_end - 0.01)
        .collect();
    assert_eq!(notes.len(), 1, "one text block below the flow: {blocks_of_page:?}");
    let (note, items) = notes[0];
    assert_eq!(*items, Some(14), "`1 A first note`");
    assert!(
        (note.origin.y - flow_end).abs() < 0.01 && note.origin.y + note.size.height <= PAGE + 0.01,
        "the note stands from the end of the flow ({flow_end}) to the bottom edge: {note:?}"
    );
    assert!(flow_end < PAGE - 10.0 && flow_end > 150.0, "room was made for it: {flow_end}");
    // The flow of the page ends above the note: its blocks ...
    let blue_bottom = rects(&result.pages[0], BLUE)
        .into_iter()
        .map(|(_, bottom, _, _)| bottom)
        .fold(0.0_f32, f32::max);
    assert!(blue_bottom <= flow_end + 0.01, "{blue_bottom} > {flow_end}");
    // ... and its text.
    assert!(
        blocks_of_page
            .iter()
            .filter(|(b, _)| b.origin.y < flow_end - 0.01)
            .all(|(b, _)| b.origin.y + b.size.height <= flow_end + 0.01),
        "{blocks_of_page:?}"
    );
    // Nothing of the note on the second page.
    assert!(text_blocks(&result.pages[1]).is_empty());
}

#[test]
fn an_export_draws_a_repeated_table_head_in_the_form_a_pdf_backend_reads() {
    // The same holds for everything a page gets besides its slice of the
    // canvas: the table head repeated on a continuation page.
    let rows: String = (0..12)
        .map(|i| format!("<tr><td style=\"height: 30px\">row {i}</td></tr>"))
        .collect();
    let (result, _, _) = pages_with(
        &html(&format!(
            "<table><thead><tr><th style=\"height: 30px\">head</th></tr></thead>\
             <tbody>{rows}</tbody></table>"
        )),
        export_config(),
        true,
    );
    assert!(result.pages.len() >= 2, "breaks: {:?}", break_ys(&result));
    for (p, page) in result.pages.iter().enumerate() {
        let blocks_of_page = text_blocks(page);
        assert!(!blocks_of_page.is_empty(), "page {p} has text");
        for (bounds, items) in &blocks_of_page {
            assert!(
                items.is_some_and(|n| n > 0),
                "a text block of page {p} ({bounds:?}) is not in the form a PDF backend draws"
            );
        }
    }
    // The head is on the second page too: `head` at its top.
    assert!(
        text_blocks(&result.pages[1])
            .iter()
            .any(|(b, items)| b.origin.y < 30.0 && *items == Some(4)),
        "{:?}",
        text_blocks(&result.pages[1])
    );
}
