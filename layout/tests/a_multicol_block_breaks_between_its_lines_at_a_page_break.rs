//! A multi-column block on paged media is fragmented column row by column row.
//!
//! CSS Multi-column Layout 1 §7 and CSS Fragmentation 3: in paged media the
//! columns of a multi-column block are as tall as the page has room for. A
//! container that needs more than one page lays its columns side by side
//! down to the bottom of the page - the first column filled before the
//! second starts - and the content that does not fit goes on in the columns
//! of the NEXT page, from its top. Only the columns of the last page are
//! balanced.
//!
//! The engine used to balance the columns over the whole container and cut
//! the result at the page height (CSS "paged layout" slices a continuous
//! canvas): a column taller than the room on its page was torn in the
//! middle - both columns at the same height - so the text of the left column
//! went on at the top of the NEXT page's left column, BEFORE the right
//! column of the page before it had been read; and a box straddling the cut
//! was split in two.
//!
//! Every box is 40px tall (or every line 20px) wherever a number is
//! asserted; the container follows a 60px block, so it starts MID-PAGE on
//! 200px pages: 140px are left for its first columns.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use std::collections::BTreeMap;

use azul_core::{dom::NodeId, geom::LogicalPosition};
use azul_layout::{solver3::LayoutNodeId, Solver3LayoutCache};

use crate::pagination_dom_breaks::{paginate, paginate_with_policy};

const PAGE: f32 = 200.0;
const LEAD: f32 = 60.0;

fn html(body: &str) -> String {
    html_of(&[body])
}

/// The containers `bodies`, one below the other, after the lead block.
fn html_of(bodies: &[&str]) -> String {
    let containers: String = bodies
        .iter()
        .map(|body| format!("<div class=\"cols\">{body}</div>"))
        .collect();
    format!(
        "<html><head><style>* {{ margin: 0; padding: 0; }} \
         #lead {{ height: {LEAD}px }} \
         .cols {{ column-count: 2; column-gap: 20px; width: 420px; line-height: 20px }} \
         .b {{ height: 40px }}</style></head>\
         <body><div id=\"lead\"></div>{containers}</body></html>"
    )
}

/// Twelve 40px boxes `{prefix}0` .. `{prefix}11`.
fn twelve(prefix: &str) -> String {
    (0..12)
        .map(|i| format!("<div id=\"{prefix}{i}\" class=\"b\"></div>"))
        .collect()
}

/// The layout box of the element `id`.
fn layout_box(
    cache: &Solver3LayoutCache,
    dom: &azul_core::styled_dom::StyledDom,
    id: &str,
) -> (LayoutNodeId, LogicalPosition) {
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
    (idx, cache.calculated_positions[idx.index()])
}

/// Where a box lies in the reading order of the pages: the page it is on,
/// the column of the page (0 left, 1 right) and the offset in the page.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
struct Slot {
    page: u32,
    column: u32,
    y: f32,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, positive
fn slot_of(x: f32, y: f32) -> Slot {
    Slot {
        page: (y / PAGE).floor() as u32,
        column: u32::from(x > 100.0),
        y: y % PAGE,
    }
}

#[test]
fn the_boxes_read_down_each_column_of_a_page_before_the_next_page() {
    let boxes: String = (0..12)
        .map(|i| format!("<div id=\"b{i}\" class=\"b\"></div>"))
        .collect();
    let (cache, dom, _) = paginate(&html(&boxes), 800.0, PAGE);

    let slots: Vec<Slot> = (0..12)
        .map(|i| {
            let (idx, pos) = layout_box(&cache, &dom, &format!("b{i}"));
            let h = cache.tree.as_deref().unwrap().get(idx).unwrap().used_size.unwrap().height;
            assert_eq!(
                (pos.y / PAGE).floor(),
                ((pos.y + h - 0.01) / PAGE).floor(),
                "#b{i} ({}..{}) straddles a page break",
                pos.y,
                pos.y + h
            );
            slot_of(pos.x, pos.y)
        })
        .collect();

    // 140px are left on the first page: three boxes fill the left column
    // (60..180), three the right one. The next page balances the other six.
    let columns: Vec<(u32, u32)> = slots.iter().map(|s| (s.page, s.column)).collect();
    assert_eq!(
        columns,
        vec![
            (0, 0), (0, 0), (0, 0), (0, 1), (0, 1), (0, 1),
            (1, 0), (1, 0), (1, 0), (1, 1), (1, 1), (1, 1),
        ],
        "page and column of every box, in source order: {slots:?}"
    );
    assert!(
        slots[3].y == LEAD && slots[6].y == 0.0 && slots[9].y == 0.0,
        "a column starts at the container's top, a page's at the page's top: {slots:?}"
    );
}

#[test]
fn the_lines_of_a_paragraph_read_down_each_column_of_a_page_before_the_next_page() {
    let lines: String = (0..30)
        .map(|i| format!("w{i}"))
        .collect::<Vec<_>>()
        .join("<br/>");
    let (cache, dom, _) = paginate(&html(&format!("<p id=\"p\">{lines}</p>")), 800.0, PAGE);
    let (idx, origin) = layout_box(&cache, &dom, "p");
    let tree = cache.tree.as_deref().unwrap();
    let layout = tree
        .materialized_inline_layout_for_node(idx.index())
        .expect("the paragraph keeps its lines");

    // The first item of every line: its pen's place on the canvas.
    let mut lines: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
    for item in &layout.items {
        lines
            .entry(item.line_index)
            .or_insert((origin.x + item.position.x, origin.y + item.position.y));
    }
    assert_eq!(lines.len(), 30, "every line is placed: {lines:?}");

    let slots: Vec<Slot> = lines.values().map(|&(x, y)| slot_of(x, y)).collect();
    for (i, w) in slots.windows(2).enumerate() {
        assert!(
            w[0] < w[1],
            "line {} ({:?}) is read before line {i} ({:?})",
            i + 1,
            w[1],
            w[0]
        );
    }
    for (i, &(_, y)) in lines.values().enumerate() {
        assert_eq!(
            (y / PAGE).floor(),
            ((y + 19.0) / PAGE).floor(),
            "line {i} (y {y}) straddles a page break"
        );
    }
    // 140px = seven lines per column on the first page; then 16 lines are
    // balanced over the two columns of the second.
    let at = |i: usize| (slots[i].page, slots[i].column);
    assert_eq!(
        (at(6), at(7), at(13), at(14), at(21), at(22), at(29)),
        ((0, 0), (0, 1), (0, 1), (1, 0), (1, 0), (1, 1), (1, 1)),
        "the lines that open a column: {slots:?}"
    );
}

#[test]
fn a_container_below_one_that_spilled_over_a_page_starts_where_that_one_ended() {
    // The first container fills the 140px of its first page and spills
    // onto the second, ending 20px further down than a balanced one would
    // (320, not 300): the second container stands at 320, with 80px left on
    // its page - two boxes a column - and carries the rest to the third.
    let (cache, dom, _) = paginate(&html_of(&[&twelve("a"), &twelve("b")]), 800.0, PAGE);
    let slots: Vec<Slot> = (0..12)
        .map(|i| {
            let (idx, pos) = layout_box(&cache, &dom, &format!("b{i}"));
            let h = cache.tree.as_deref().unwrap().get(idx).unwrap().used_size.unwrap().height;
            assert_eq!(
                (pos.y / PAGE).floor(),
                ((pos.y + h - 0.01) / PAGE).floor(),
                "#b{i} ({}..{}) straddles a page break",
                pos.y,
                pos.y + h
            );
            slot_of(pos.x, pos.y)
        })
        .collect();
    let columns: Vec<(u32, u32)> = slots.iter().map(|s| (s.page, s.column)).collect();
    assert_eq!(
        columns,
        vec![
            (1, 0), (1, 0), (1, 1), (1, 1),
            (2, 0), (2, 0), (2, 0), (2, 0), (2, 1), (2, 1), (2, 1), (2, 1),
        ],
        "page and column of every box of the second container: {slots:?}"
    );
}

#[test]
fn a_page_break_before_a_container_moves_the_page_its_columns_end_on() {
    // A forced break at 20px: the pages are 20..220, 220..420. The
    // container at 60 has 160px on its first page: four boxes a column.
    let html = format!(
        "<html><head><style>* {{ margin: 0; padding: 0; }} \
         #lead {{ height: 20px }} #brk {{ height: 40px; page-break-before: always }} \
         .cols {{ column-count: 2; column-gap: 20px; width: 420px }} \
         .b {{ height: 40px }}</style></head>\
         <body><div id=\"lead\"></div><div id=\"brk\"></div>\
         <div class=\"cols\">{}</div></body></html>",
        twelve("c")
    );
    let (cache, dom, _) = paginate(&html, 800.0, PAGE);
    let pos = |i: usize| layout_box(&cache, &dom, &format!("c{i}")).1;
    // 60..220 holds four boxes a column; the second page balances the rest.
    assert_eq!((pos(0).x, pos(0).y), (0.0, 60.0));
    assert_eq!((pos(3).x, pos(3).y), (0.0, 180.0));
    assert!(pos(4).x > 100.0 && pos(4).y == 60.0, "c4 tops the right column: {:?}", pos(4));
    assert!(pos(8).x == 0.0 && pos(8).y == 220.0, "c8 tops the next page: {:?}", pos(8));
}

#[test]
fn the_break_analysis_cuts_the_pages_at_the_rows_whatever_it_honours() {
    // A PDF is cut with widows, orphans and atomic lines honoured: with the
    // paragraphs' lines running across the columns the analysis pushed every
    // page's break up past the rows' ends, and the rows no longer stood on
    // their pages. A row after the first starts a page, verbatim.
    let policy = azul_layout::solver3::page_breaks::BreakPolicy {
        honor_break_inside: true,
        widows_orphans: true,
        atomic_lines: true,
        atomic_table_rows: true,
        repeat_table_headers: true,
        ..Default::default()
    };
    let text = "Der Papst ist an die göttliche Offenbarung gebunden und an die Tradition, \
                wie die Kirche seit jeher lehrt. "
        .repeat(4);
    let paragraphs: String = (0..16)
        .map(|i| format!("<p id=\"p{i}\">{text}</p>"))
        .collect();
    let html = format!(
        "<html><head><style>* {{ margin: 0; padding: 0; }} #lead {{ height: 60px }} \
         .cols {{ column-count: 2; column-gap: 20px; width: 420px; line-height: 20px; \
         font-size: 14px }} p {{ margin: 0 0 8px 0; orphans: 3; widows: 3; \
         text-align: justify }}</style></head><body><div id=\"lead\"></div>\
         <div class=\"cols\">{paragraphs}</div></body></html>"
    );
    let (_cache, _dom, pagination) = paginate_with_policy(&html, 800.0, PAGE, policy);
    let breaks: Vec<f32> = pagination.breaks.as_ref().iter().map(|b| b.y).collect();
    assert!(
        breaks.len() >= 3 && breaks.iter().enumerate().all(|(i, y)| *y == PAGE * (i + 1) as f32),
        "the pages break every {PAGE}px, as the rows are laid out: {breaks:?}"
    );
}
