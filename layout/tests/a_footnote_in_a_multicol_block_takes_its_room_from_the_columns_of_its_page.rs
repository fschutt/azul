//! A footnote anchored in a multi-column container takes its room from the
//! columns of the page it is on - from ALL of them.
//!
//! CSS Generated Content for Paged Media, "Footnotes": the footnote area is
//! at the bottom of the page, as wide as the page area. CSS Multicol 1 and
//! CSS Fragmentation 3: on paged media the columns of a container are as
//! tall as the page has room for, row by row, a row a page.
//!
//! So the columns of a page that carries notes end where its notes begin:
//! a note anchored in the right column shortens the left column too, the
//! content that no longer fits goes on in the columns of the next page, and
//! a line (or box) and its note stay on one page. The columns were laid out
//! for the whole page and the page cut where the notes begin - through both
//! columns at one height, the rest of the left column read after the right.
//!
//! Pages are 800 x 200. The container follows a 60px block: 140px are left
//! for its first columns. Two 200px columns 20px apart; every box is 40px
//! tall, every line 20px.

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

const PAGE: f32 = 200.0;

const STYLE: &str = "* { margin: 0; padding: 0; } \
     #lead { height: 60px; line-height: 20px } \
     .cols { column-count: 2; column-gap: 20px; width: 420px; line-height: 20px } \
     .b { height: 40px } \
     .fn { float: footnote; height: 40px; background: #ff0000 } \
     .fn::footnote-call { content: none } .fn::footnote-marker { content: none }";

fn html(body: &str) -> String {
    format!("<html><head><style>{STYLE}</style></head><body>{body}</body></html>")
}

const NOTE: &str = "<span id=\"n1\" class=\"fn\">note</span>";

/// `n` boxes `b0`.., the note written in box `with_note`.
fn boxes(n: usize, with_note: Option<usize>) -> String {
    (0..n)
        .map(|i| {
            let note = if with_note == Some(i) { NOTE } else { "" };
            format!("<div id=\"b{i}\" class=\"b\">{note}</div>")
        })
        .collect()
}

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

/// The red rects of `page` (the note), as (top, bottom).
fn note_on(page: &DisplayList) -> Vec<(f32, f32)> {
    page.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Rect { bounds, color, .. }
                if color.r == 255 && color.g == 0 && color.b == 0 && color.a > 0 =>
            {
                let b = bounds.inner();
                Some((b.origin.y, b.origin.y + b.size.height))
            }
            _ => None,
        })
        .collect()
}

fn break_ys(result: &PagedLayoutResult) -> Vec<f32> {
    result.breaks.iter().map(|b| b.y).collect()
}

/// Where the layout put the element `id` on the canvas.
fn at(cache: &Solver3LayoutCache, dom: &StyledDom, id: &str) -> (f32, f32) {
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
    let pos = cache.calculated_positions[idx.index()];
    (pos.x, pos.y)
}

/// Twelve boxes over two pages with the 40px note at the bottom of the
/// first: 100px are left for its columns, two boxes each; the other eight
/// are balanced on the second page, which starts where the first one's
/// columns end.
fn assert_two_boxes_a_column_on_the_first_page(
    result: &PagedLayoutResult,
    cache: &Solver3LayoutCache,
    dom: &StyledDom,
) {
    let pos = |i: usize| at(cache, dom, &format!("b{i}"));
    assert_eq!(
        (pos(0), pos(1), pos(2), pos(3)),
        ((0.0, 60.0), (0.0, 100.0), (220.0, 60.0), (220.0, 100.0)),
        "two boxes a column above the note"
    );
    assert_eq!(
        (pos(4), pos(7), pos(8), pos(11)),
        ((0.0, 160.0), (0.0, 280.0), (220.0, 160.0), (220.0, 280.0)),
        "the other eight on the second page, four a column"
    );
    assert_eq!(break_ys(result), vec![160.0], "the first page ends where its columns do");
    assert_eq!(result.pages.len(), 2);
    assert_eq!(note_on(&result.pages[0]), vec![(160.0, 200.0)]);
    assert_eq!(note_on(&result.pages[1]), vec![]);
}

#[test]
fn a_note_in_the_left_column_shortens_both_columns_of_its_page() {
    let (result, cache, dom) = pages(&html(&format!(
        "<div id=\"lead\"></div><div class=\"cols\">{}</div>",
        boxes(12, Some(1))
    )));
    assert_two_boxes_a_column_on_the_first_page(&result, &cache, &dom);
}

#[test]
fn a_note_in_the_right_column_shortens_the_left_column_too() {
    let (result, cache, dom) = pages(&html(&format!(
        "<div id=\"lead\"></div><div class=\"cols\">{}</div>",
        boxes(12, Some(3))
    )));
    assert_two_boxes_a_column_on_the_first_page(&result, &cache, &dom);
}

#[test]
fn a_note_above_the_container_on_its_page_takes_room_from_its_columns() {
    let (result, cache, dom) = pages(&html(&format!(
        "<p id=\"lead\">one{NOTE}</p><div class=\"cols\">{}</div>",
        boxes(12, None)
    )));
    assert_two_boxes_a_column_on_the_first_page(&result, &cache, &dom);
}

#[test]
fn a_container_that_fits_its_page_with_its_note_stays_in_one_row() {
    // Four boxes, two a column: the container ends at 140, the note begins
    // at 160.
    let (result, cache, dom) = pages(&html(&format!(
        "<div id=\"lead\"></div><div class=\"cols\">{}</div>",
        boxes(4, Some(1))
    )));
    let pos = |i: usize| at(&cache, &dom, &format!("b{i}"));
    assert_eq!(
        (pos(0), pos(1), pos(2), pos(3)),
        ((0.0, 60.0), (0.0, 100.0), (220.0, 60.0), (220.0, 100.0))
    );
    assert_eq!(result.pages.len(), 1, "breaks: {:?}", break_ys(&result));
    assert_eq!(note_on(&result.pages[0]), vec![(160.0, 200.0)]);
}

#[test]
fn a_container_that_fits_its_page_only_without_its_note_goes_on_on_the_next() {
    // Six boxes, three a column, would end at 180 - under the page's end,
    // but not above the note that begins at 160: two a column stay, the
    // last two go to the next page, balanced.
    let (result, cache, dom) = pages(&html(&format!(
        "<div id=\"lead\"></div><div class=\"cols\">{}</div>",
        boxes(6, Some(1))
    )));
    let pos = |i: usize| at(&cache, &dom, &format!("b{i}"));
    assert_eq!(
        (pos(0), pos(1), pos(2), pos(3)),
        ((0.0, 60.0), (0.0, 100.0), (220.0, 60.0), (220.0, 100.0))
    );
    assert_eq!((pos(4), pos(5)), ((0.0, 160.0), (220.0, 160.0)));
    assert_eq!(break_ys(&result), vec![160.0]);
    assert_eq!(note_on(&result.pages[0]), vec![(160.0, 200.0)]);
}

#[test]
fn a_note_in_a_line_of_a_paragraph_keeps_the_lines_of_both_columns_above_it() {
    // Twelve lines, a 50px inline block each. Without the note seven fit a
    // 140px column; with the 40px note written in the second line five do:
    // ten lines on the first page, the last two balanced on the second.
    let lines: String = (0..12)
        .map(|i| {
            let note = if i == 1 { NOTE } else { "" };
            let end = if i == 11 { "" } else { "<br/>" };
            format!(
                "<i id=\"w{i}\" style=\"display: inline-block; width: 50px; height: 10px\"></i>\
                 {note}{end}"
            )
        })
        .collect();
    let (result, cache, dom) = pages(&html(&format!(
        "<div id=\"lead\"></div><div class=\"cols\"><p>{lines}</p></div>"
    )));
    let pos = |i: usize| at(&cache, &dom, &format!("w{i}"));
    let first = pos(0).1;
    assert!((60.0..80.0).contains(&first), "the first line is the container's first: {first}");
    assert_eq!((pos(4).0, pos(4).1), (0.0, first + 80.0), "five lines in the left column");
    assert_eq!((pos(5).0, pos(5).1), (220.0, first), "the sixth tops the right column");
    assert_eq!((pos(9).0, pos(9).1), (220.0, first + 80.0));
    assert_eq!((pos(10).0, pos(10).1), (0.0, first + 100.0), "the eleventh tops the next page");
    assert_eq!((pos(11).0, pos(11).1), (220.0, first + 100.0));
    assert_eq!(break_ys(&result), vec![160.0]);
    assert_eq!(note_on(&result.pages[0]), vec![(160.0, 200.0)]);
    assert_eq!(note_on(&result.pages[1]), vec![]);
}

#[test]
fn a_note_on_a_later_page_of_a_container_shortens_the_columns_of_that_page() {
    // Twenty-four boxes. The first page has 140px: three a column. The
    // second has 200px less the 40px note written in its second box: four
    // a column, and the page ends where they do. The last ten are balanced
    // on the third page, five a column.
    let (result, cache, dom) = pages(&html(&format!(
        "<div id=\"lead\"></div><div class=\"cols\">{}</div>",
        boxes(24, Some(7))
    )));
    let pos = |i: usize| at(&cache, &dom, &format!("b{i}"));
    assert_eq!((pos(0), pos(2), pos(3), pos(5)), ((0.0, 60.0), (0.0, 140.0), (220.0, 60.0), (220.0, 140.0)));
    assert_eq!(
        (pos(6), pos(9), pos(10), pos(13)),
        ((0.0, 200.0), (0.0, 320.0), (220.0, 200.0), (220.0, 320.0)),
        "four boxes a column above the note of the second page"
    );
    assert_eq!(
        (pos(14), pos(18), pos(19), pos(23)),
        ((0.0, 360.0), (0.0, 520.0), (220.0, 360.0), (220.0, 520.0)),
        "the third page starts where the second one's columns end"
    );
    assert_eq!(break_ys(&result), vec![200.0, 360.0]);
    assert_eq!(result.pages.len(), 3);
    assert_eq!(note_on(&result.pages[0]), vec![]);
    assert_eq!(note_on(&result.pages[1]), vec![(160.0, 200.0)]);
    assert_eq!(note_on(&result.pages[2]), vec![]);
}

#[test]
fn every_page_of_a_long_container_makes_room_for_its_own_notes() {
    // Sixty boxes, a 40px note in every eighth: eight pages, each cut where
    // its columns end, none through a box, each note on the page of its box.
    let boxes: String = (0..60)
        .map(|i| {
            let note = if i % 8 == 1 {
                "<span class=\"fn\">note</span>"
            } else {
                ""
            };
            format!("<div id=\"b{i}\" class=\"b\">{note}</div>")
        })
        .collect();
    let (result, cache, dom) = pages(&html(&format!(
        "<div id=\"lead\"></div><div class=\"cols\">{boxes}</div>"
    )));
    let breaks = break_ys(&result);
    let page_of = |y: f32| breaks.iter().take_while(|b| **b <= y).count();
    // 140px less a note on the first page: four boxes; 200px less a note on
    // every other: eight. 4 + 7 * 8 = 60.
    assert_eq!(result.pages.len(), 8, "breaks: {breaks:?}");
    // The boxes read down the left column of a page, then down its right
    // column, then on the next page.
    let slots: Vec<(usize, bool, f32)> = (0..60)
        .map(|i| {
            let (x, y) = at(&cache, &dom, &format!("b{i}"));
            (page_of(y), x > 100.0, y)
        })
        .collect();
    assert!(
        slots.windows(2).all(|w| w[0] < w[1]),
        "page, column and offset of every box, in source order: {slots:?}"
    );
    for i in 0..60 {
        let (_, y) = at(&cache, &dom, &format!("b{i}"));
        assert_eq!(
            page_of(y),
            page_of(y + 39.9),
            "#b{i} ({y}..{}) is cut by a page break: {breaks:?}",
            y + 40.0
        );
    }
    let mut notes_drawn = 0;
    for (p, page) in result.pages.iter().enumerate() {
        let start = if p == 0 { 0.0 } else { breaks[p - 1] };
        let flow = breaks.get(p).copied().unwrap_or(result.total_content_height) - start;
        let notes = note_on(page);
        let expected = (0..60)
            .filter(|i| i % 8 == 1 && page_of(at(&cache, &dom, &format!("b{i}")).1) == p)
            .count();
        assert_eq!(notes.len(), expected, "the notes of page {p}: {notes:?}");
        notes_drawn += notes.len();
        if let Some((top, _)) = notes.first() {
            assert!(
                flow <= *top + 0.01,
                "the flow of page {p} ({flow}px) ends above its notes ({top}): {breaks:?}"
            );
            assert_eq!(notes.last().map(|n| n.1), Some(PAGE), "the last note ends at the bottom edge");
        }
    }
    assert_eq!(notes_drawn, 8, "every note is drawn once");
}

#[test]
fn a_note_that_fits_no_page_takes_the_footnote_area_of_the_pages_of_the_columns_it_crosses() {
    // Eight boxes at the top of the page, a note of fifteen 20px lines
    // (300px) written in the second. A page gives its footnote area 160px
    // at the most: the first page has 40px for its columns, a box each,
    // above eight lines of the note; the second 60px, a box each again,
    // above the other seven; the last four boxes are balanced on the third.
    let lines: Vec<String> = (0..15).map(|i| format!("l{i}")).collect();
    let long = format!(
        "<span class=\"fn\" style=\"height: auto; line-height: 20px\">{}</span>",
        lines.join("<br/>")
    );
    let boxes: String = (0..8)
        .map(|i| {
            let note = if i == 1 { long.as_str() } else { "" };
            format!("<div id=\"b{i}\" class=\"b\">{note}</div>")
        })
        .collect();
    let (result, cache, dom) = pages(&html(&format!("<div class=\"cols\">{boxes}</div>")));
    let pos = |i: usize| at(&cache, &dom, &format!("b{i}"));
    assert_eq!((pos(0), pos(1)), ((0.0, 0.0), (220.0, 0.0)));
    assert_eq!((pos(2), pos(3)), ((0.0, 40.0), (220.0, 40.0)));
    assert_eq!(
        (pos(4), pos(5), pos(6), pos(7)),
        ((0.0, 100.0), (0.0, 140.0), (220.0, 100.0), (220.0, 140.0))
    );
    assert_eq!(break_ys(&result), vec![40.0, 100.0]);
    assert_eq!(result.pages.len(), 3);
    assert_eq!(note_on(&result.pages[0]), vec![(40.0, 200.0)]);
    assert_eq!(note_on(&result.pages[1]), vec![(60.0, 200.0)]);
    assert_eq!(note_on(&result.pages[2]), vec![]);
}
