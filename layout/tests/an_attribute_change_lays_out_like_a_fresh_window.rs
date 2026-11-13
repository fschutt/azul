//! An attribute change lays the page out as a fresh window does.
//!
//! The layout reads attributes of a node besides its style: a cell's
//! `colspan` and `rowspan` place it in its table's grid, the legacy `align`
//! centres the blocks below it, an SVG text sits at its `x`, an SVG group's
//! `transform` scales what it holds, and an SVG shape is stroked along its
//! geometry. An app that changes only one of them rebuilds its page with the
//! node's style, text and classes as they were, and the window must not keep
//! the layout or the display list of the old page.

use azul_core::{
    dom::{AttributeNameValue, AttributeType, Dom, DomId, SvgNodeData},
    path_parser::parse_svg_path_d,
};
use azul_layout::{solver3::display_list::DisplayListItem, window::LayoutWindow};

use crate::{
    a_measured_label_flows_its_text_only_at_a_new_width::{
        assert_lays_out_like_a_fresh_window, node_with_class, painted_glyphs, rebuild, rect,
    },
    a_one_box_slide_does_not_re_lay_out_the_page::window,
};

/// Rebuilds a window opened on `page(first)` with `page(second)`, then with
/// the first page and the second again: every pass lays out as a window
/// opened on its page does.
fn switch_back_and_forth<T: Copy + core::fmt::Debug>(page: impl Fn(T) -> Dom, first: T, second: T) {
    let mut lw = window(page(first));
    for value in [second, first, second] {
        rebuild(&mut lw, page(value));
        assert_lays_out_like_a_fresh_window(&lw, page(value), &format!("the page with {value:?}"));
    }
}

/// Where the box of class `class` starts in a window opened on `page`.
fn x_of(page: Dom, class: &str) -> f32 {
    let lw = window(page);
    rect(&lw, node_with_class(&lw, class)).origin.x
}

/// Where the first glyph a window opened on `page` paints starts.
fn first_glyph_x(page: Dom) -> f32 {
    painted_glyphs(&window(page))
        .first()
        .map(|glyph| glyph.1)
        .expect("harness: the page paints text")
}

// ---- tables ----

/// A cell 40 px wide holding `text`.
fn cell(text: &str, class: &str) -> Dom {
    Dom::create_td_with_text(text)
        .with_class(class.into())
        .with_css("width: 40px; padding: 0px;")
}

/// A table without spacing between its cells, holding `rows`.
fn table(rows: Vec<Dom>) -> Dom {
    Dom::create_html().with_child(
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_table_no_a11y()
                .with_css("border-spacing: 0px;")
                .with_child(Dom::create_tbody().with_children(rows.into())),
        ),
    )
}

/// A first row of cell `a`, spanning `span` columns, and cell `b`, over a
/// row of three cells.
fn spanning_columns(span: i32) -> Dom {
    table(vec![
        Dom::create_tr()
            .with_child(cell("A", "a").with_attribute(AttributeType::ColSpan(span)))
            .with_child(cell("B", "b")),
        Dom::create_tr()
            .with_child(cell("C", "c"))
            .with_child(cell("D", "d"))
            .with_child(cell("E", "e")),
    ])
}

/// A first row of cell `a`, spanning `span` rows, and cell `b`, over a row
/// of cells `c` and `d`.
fn spanning_rows(span: i32) -> Dom {
    table(vec![
        Dom::create_tr()
            .with_child(cell("A", "a").with_attribute(AttributeType::RowSpan(span)))
            .with_child(cell("B", "b")),
        Dom::create_tr()
            .with_child(cell("C", "c"))
            .with_child(cell("D", "d")),
    ])
}

/// A cell that comes to span two columns puts its neighbour in the third.
#[test]
fn a_cell_that_spans_another_column_moves_its_neighbour_on() {
    assert!(
        x_of(spanning_columns(2), "b") > x_of(spanning_columns(1), "b") + 30.0,
        "harness: a cell over two columns pushes `b` into the third"
    );
    switch_back_and_forth(spanning_columns, 1, 2);
}

/// A cell that comes to span two rows takes the first column of the row
/// below too: that row's cells move one column on.
#[test]
fn a_cell_that_spans_another_row_moves_the_row_below_on() {
    assert!(
        x_of(spanning_rows(2), "c") > x_of(spanning_rows(1), "c") + 30.0,
        "harness: a cell over two rows pushes `c` into the second column"
    );
    switch_back_and_forth(spanning_rows, 1, 2);
}

// ---- the legacy align ----

/// A 300 px region with centred text that states `align`, holding a line of
/// text and a block, and in that block a 50 px box. The block centres its
/// box when the region says `align="center"`: the region's alignment reaches
/// the boxes below it, the box is not the region's own child.
fn centred_the_legacy_way(align: &'static str) -> Dom {
    Dom::create_html().with_child(
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_div()
                .with_attribute(AttributeType::Custom(AttributeNameValue {
                    attr_name: "align".into(),
                    value: align.into(),
                }))
                .with_css("width: 300px; text-align: center;")
                .with_child(Dom::create_div_with_text("Files"))
                .with_child(
                    Dom::create_div().with_child(
                        Dom::create_div()
                            .with_class("box".into())
                            .with_css("width: 50px; height: 10px;"),
                    ),
                ),
        ),
    )
}

/// The box moves to the left edge when the region stops centring blocks, and
/// back to the middle when it centres them again.
#[test]
fn a_box_centred_by_an_align_above_it_moves_when_the_align_changes() {
    assert!(
        x_of(centred_the_legacy_way("center"), "box")
            > x_of(centred_the_legacy_way("left"), "box") + 100.0,
        "harness: `align=\"center\"` centres the box, `align=\"left\"` does not"
    );
    switch_back_and_forth(centred_the_legacy_way, "center", "left");
}

// ---- SVG ----

/// `body` in a page, loaded by the XML loader as an app's markup is.
fn markup(body: &str) -> Dom {
    let page = format!("<html><body style=\"margin: 0px\">{body}</body></html>");
    azul_layout::xml::dom_from_parsed_xml(
        azul_layout::xml::parse_xml(&page).expect("the markup parses"),
    )
}

/// An SVG text whose baseline starts at `x`.
fn svg_text_at(x: u32) -> Dom {
    markup(&format!(
        "<svg width=\"200\" height=\"100\" viewBox=\"0 0 200 100\">\
         <text x=\"{x}\" y=\"50\" font-size=\"20\">Hi</text></svg>"
    ))
}

/// An SVG text in a group that scales it by `scale`.
fn svg_text_scaled(scale: u32) -> Dom {
    markup(&format!(
        "<svg width=\"200\" height=\"100\" viewBox=\"0 0 200 100\">\
         <g transform=\"scale({scale})\"><text x=\"10\" y=\"40\" font-size=\"20\">Hi</text></g>\
         </svg>"
    ))
}

/// The text moves to its new `x`.
#[test]
fn an_svg_text_moves_to_its_new_x() {
    assert!(
        first_glyph_x(svg_text_at(100)) > first_glyph_x(svg_text_at(10)) + 50.0,
        "harness: the text starts at its `x`"
    );
    switch_back_and_forth(svg_text_at, 10, 100);
}

/// The text a group holds is laid out again at the group's new scale.
#[test]
fn an_svg_text_takes_its_groups_new_scale() {
    assert!(
        first_glyph_x(svg_text_scaled(2)) > first_glyph_x(svg_text_scaled(1)) + 5.0,
        "harness: the group's scale moves the text's `x`"
    );
    switch_back_and_forth(svg_text_scaled, 1, 2);
}

/// A 100 x 50 box stroked along the path `d`, in window pixels: a chart's
/// series is drawn so.
fn stroked(d: &'static str) -> Dom {
    let path = parse_svg_path_d(d).expect("the path parses");
    Dom::create_html().with_child(
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_div()
                .with_css("width: 100px; height: 50px; border: 2px solid #ff0000;")
                .with_svg_data(SvgNodeData::Path(path)),
        ),
    )
}

/// Every path the window strokes.
fn stroked_paths(lw: &LayoutWindow) -> Vec<String> {
    lw.layout_results[&DomId::ROOT_ID]
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::StrokedPath { path, .. } => Some(format!("{path:?}")),
            _ => None,
        })
        .collect()
}

/// A box whose path changes is stroked along the new path, as a chart whose
/// values change draws its new series.
#[test]
fn a_box_is_stroked_along_its_new_path() {
    let (rising, falling) = ("M 0 50 L 100 0", "M 0 0 L 100 50");
    let first = stroked_paths(&window(stroked(rising)));
    assert!(!first.is_empty(), "harness: the box is stroked along its path");
    assert_ne!(
        first,
        stroked_paths(&window(stroked(falling))),
        "harness: the two paths are stroked apart"
    );
    let mut lw = window(stroked(rising));
    for d in [falling, rising, falling] {
        rebuild(&mut lw, stroked(d));
        assert_eq!(
            stroked_paths(&lw),
            stroked_paths(&window(stroked(d))),
            "the box is stroked along `{d}`"
        );
    }
}
