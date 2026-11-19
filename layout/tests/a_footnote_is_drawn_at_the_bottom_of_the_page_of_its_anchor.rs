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
        paged_layout::{layout_document_paged_v2, PagedLayoutResult},
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
fn pages(html: &str) -> (PagedLayoutResult, Solver3LayoutCache, StyledDom) {
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
    let result = layout_document_paged_v2(
        &mut cache,
        &mut text_cache,
        FragmentationContext::new_paged(content_size),
        &styled_dom,
        LogicalRect {
            origin: LogicalPosition::zero(),
            size: content_size,
        },
        &mut font_manager,
        &BTreeMap::new(),
        &mut Some(Vec::new()),
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
    .expect("the document paginates");
    (result, cache, styled_dom)
}

/// The rects painted in exactly `color` on `page`, as (top, bottom, left,
/// width).
fn rects(page: &DisplayList, (r, g, b): (u8, u8, u8)) -> Vec<(f32, f32, f32, f32)> {
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
