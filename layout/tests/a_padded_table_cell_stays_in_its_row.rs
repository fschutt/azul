//! A padded table cell's text clip stays inside its own row.
//!
//! The display list clips an overflow-visible node's inline content to its
//! live content extent rather than its tight content box, so a TextArea whose
//! IFC outgrew its stale box still repaints every moved line. That extent
//! comes from `get_scroll_content_size`, which floors at the node's own
//! BORDER box, while the clip keeps its content-box origin. For a `td` with
//! `padding: 3px` and nothing overflowing, the clip was therefore padding-box
//! TALL yet started 3px down: it reached 3px into the next row.
//!
//! The paginator unions every display item a `<tr>` owns into that row's
//! keep-together range, so every row overlapped the top of the next, and the
//! break-snapping pass climbed row after row until its push budget (a third
//! of the page) was spent - azul#478, every page of a long table cut a third
//! short. A box's own padding and border are not overflow; only an extent
//! beyond the border box widens the clip now.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::{display_list::DisplayListItem, pagination::collect_table_row_ranges},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const ROWS: usize = 6;
const PADDING: f32 = 3.0;

/// The reporter's table, reduced: single-line cells, `padding: 3px 6px`,
/// collapsed borders, no `break-*` CSS.
fn lay_out_table() -> LayoutWindow {
    let css = format!(
        "body {{ margin: 0; padding: 0; font-family: sans-serif; }}
         table {{ border-collapse: collapse; }}
         td {{ font-size: 9pt; padding: {PADDING}px 6px; }}"
    );
    let (css, _) = azul_css::parser2::new_from_str(&css);

    let mut tbody = Dom::create_tbody();
    for i in 0..ROWS {
        tbody.add_child(
            Dom::create_tr()
                .with_child(Dom::create_td().with_child(Dom::create_span_with_text(format!("Row {i}").as_str()))),
        );
    }
    let mut dom = Dom::create_body().with_child(Dom::create_table_no_a11y().with_child(tbody));
    let styled = StyledDom::create(&mut dom, css);

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 400.0);
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();
    lw
}

#[test]
fn a_padded_cells_text_clip_stays_inside_its_row() {
    let lw = lay_out_table();
    let result = lw.get_layout_result(&DomId::ROOT_ID).unwrap();

    let mut rows = collect_table_row_ranges(&result.display_list, &result.styled_dom);
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(rows.len(), ROWS, "one keep-together range per <tr>: {rows:?}");

    // Rows stack: each ends where the next begins, none reaches into the next.
    for pair in rows.windows(2) {
        let (_, bottom) = pair[0];
        let (top, _) = pair[1];
        assert!(
            bottom <= top + 0.01,
            "a row's range runs {} px into the next row: {rows:?}",
            bottom - top
        );
    }

    // And each cell's text clip is content-box tall: the row minus the
    // vertical padding, never the row itself.
    let pitch = rows[1].0 - rows[0].0;
    let mut clips = 0;
    for item in &result.display_list.items {
        if let DisplayListItem::Text { clip_rect, .. } = item {
            let r = clip_rect.inner();
            assert!(
                r.size.height <= pitch - 2.0 * PADDING + 0.01,
                "text clip {r:?} is not content-box tall in a {pitch} px row"
            );
            clips += 1;
        }
    }
    assert_eq!(clips, ROWS, "one text run per cell");
}
