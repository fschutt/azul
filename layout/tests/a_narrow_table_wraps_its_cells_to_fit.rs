//! A narrow table wraps its cells to fit.
//!
//! R1_MAIL_RENDER's open items (scripts/R1_MAIL_RENDER_2026_09_30.md, "What
//! is left"):
//!
//! - a cell's MIN-content measurement returned its max-content width: the
//!   `TableCell` arm of `calculate_used_size_for_node` sized an auto-width
//!   cell at its max-content width under a min-content constraint too, so the
//!   text inside never wrapped during the measurement, and no column could
//!   shrink below its longest line - a table of prose in a narrow reading
//!   pane ran past its own width;
//! - the table's own intrinsic sizes (`calculate_table_intrinsic_sizes`)
//!   counted a spanning cell in its FIRST column only, so a table under a
//!   shrink-to-fit parent came out as wide as the spanning cell PLUS every
//!   other column.
//!
//! CSS 2.2 17.5.2.2 (automatic table layout): a column's minimum is the
//! largest minimum content width of its cells; a spanning cell's widths are
//! spread over the columns it spans.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use azul_core::{
    dom::{DomId, DomNodeId, IdOrClass, NodeData, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn laid_out(markup: &str) -> LayoutWindow {
    let parsed = azul_layout::xml::parse_xml(markup).expect("the mail parses");
    let styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(760.0, 400.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the mail lays out");
    lw
}

/// Every text run's glyph pens `(x, y)`.
fn runs(lw: &LayoutWindow) -> Vec<Vec<(f32, f32)>> {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } if !glyphs.is_empty() => {
                Some(glyphs.iter().map(|g| (g.point.x, g.point.y)).collect())
            }
            _ => None,
        })
        .collect()
}

fn node(lw: &LayoutWindow, id: &str) -> DomNodeId {
    let sd = &lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    let n = sd
        .node_data
        .as_ref()
        .iter()
        .position(|nd: &NodeData| {
            nd.get_ids_and_classes()
                .iter()
                .any(|c| matches!(c, IdOrClass::Id(s) if s.as_str() == id))
        })
        .unwrap_or_else(|| panic!("no element with id {id}"));
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

#[test]
fn prose_cells_wrap_inside_a_220px_table() {
    let lw = laid_out(
        "<html><head></head><body style=\"margin: 0\">\
         <table id=\"t\" style=\"width: 220px\"><tr>\
         <td>alpha beta gamma delta epsilon</td><td>zeta eta theta iota kappa</td>\
         </tr></table></body></html>",
    );
    let t = node(&lw, "t");
    let left = lw.get_node_position(t).expect("the table is placed").x;
    let width = lw.get_node_size(t).expect("the table has a size").width;
    assert!(
        (width - 220.0).abs() < 1.0,
        "the table keeps its 220px: {width}"
    );
    let runs = runs(&lw);
    let pens: Vec<(f32, f32)> = runs.iter().flatten().copied().collect();
    assert!(!pens.is_empty(), "the cells paint");
    for (x, _) in &pens {
        assert!(
            *x < left + width,
            "every word starts inside the 220px table (the columns shrink to their \
             min-content and the text wraps): a pen at x={x}, table {left}..{}",
            left + width
        );
    }
    let mut lines: Vec<i32> = pens.iter().map(|(_, y)| y.round() as i32).collect();
    lines.sort_unstable();
    lines.dedup();
    assert!(
        lines.len() >= 3,
        "the prose wraps onto several lines: {lines:?}"
    );
}

#[test]
fn a_spanning_header_widens_the_columns_it_spans_not_only_the_first() {
    let lw = laid_out(
        "<html><head></head><body style=\"margin: 0\">\
         <div id=\"shrink\" style=\"display: inline-block\"><table style=\"border-spacing: 0\">\
         <tr><td colspan=\"2\">a spanning header wider than both columns together</td></tr>\
         <tr><td>left cell</td><td>right cell</td></tr>\
         </table></div></body></html>",
    );
    let runs = runs(&lw);
    let header = runs
        .iter()
        .find(|r| r.len() == "a spanning header wider than both columns together".len())
        .unwrap_or_else(|| panic!("the header paints: {runs:?}"));
    let header_extent = header.last().map_or(0.0, |p| p.0) - header[0].0;
    let wrapper = lw
        .get_node_size(node(&lw, "shrink"))
        .expect("the wrapper has a size")
        .width;
    // Paddings (UA: 1px per cell side) and the header's last letter fit in
    // 30px; the old sum added a whole second column ("right cell", ~70px).
    assert!(
        wrapper < header_extent + 30.0,
        "the shrink-to-fit wrapper is as wide as the spanning header, not the header \
         plus the second column: {wrapper} vs header {header_extent}"
    );
}
