//! An SVG `<text>` is laid out from its own attributes through its `<svg>`.
//!
//! The builtin SVG renderers keep `x` / `y` / `font-size` / `transform` on the
//! `SvgText` node (its characters are its `Text` children). Layout scales the
//! text's sizes by the `<svg>`'s current mapping (viewBox onto its box, then
//! the transforms) and places its first baseline at the mapped `(x, y)` -
//! crisp at any size, and right when the `<svg>` is drawn at another size
//! than its viewBox. Before, `<text>` was a `<div>` in flow at the top of the
//! `<svg>`.

use azul_core::{dom::DomId, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `(x, baseline y, font size px)` of every glyph the markup's `<svg>`
/// content draws, in paint order, at the window's origin (no body margin).
fn glyphs(svg: &str) -> Vec<(f32, f32, f32)> {
    let markup = format!("<html><body style=\"margin: 0px\">{svg}</body></html>");
    let parsed = azul_layout::xml::parse_xml(&markup).expect("the markup parses");
    let mut dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .unwrap();
    let dl = &lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list;
    dl.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text {
                glyphs,
                font_size_px,
                ..
            } => Some(
                glyphs
                    .iter()
                    .map(|g| (g.point.x, g.point.y, *font_size_px))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The first glyph's `(x, baseline y, font size px)`.
fn first_glyph(svg: &str) -> (f32, f32, f32) {
    *glyphs(svg).first().expect("a glyph is drawn")
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.75
}

#[test]
fn a_text_starts_its_baseline_at_its_x_and_y() {
    let (x, y, size) = first_glyph(
        r#"<svg width="200" height="100" viewBox="0 0 200 100"><text x="10" y="50" font-size="20">Hello</text></svg>"#,
    );
    assert!(close(x, 10.0) && close(y, 50.0), "the baseline starts at (10, 50): ({x}, {y})");
    assert!(close(size, 20.0), "at its font size: {size}");
}

#[test]
fn a_text_in_an_svg_drawn_at_twice_its_view_box_is_twice_as_big() {
    let (x, y, size) = first_glyph(
        r#"<svg width="400" height="200" viewBox="0 0 200 100"><text x="10" y="50" font-size="20">Hello</text></svg>"#,
    );
    assert!(close(x, 20.0) && close(y, 100.0), "the baseline at (20, 100): ({x}, {y})");
    assert!(close(size, 40.0), "the font scaled with it: {size}");
}

#[test]
fn a_text_under_a_matrix_is_where_printpdf_puts_it() {
    // printpdf's text: x / y 0, the position and size in the matrix.
    let (x, y, size) = first_glyph(
        r#"<svg width="200" height="100" viewBox="0 0 200 100"><g transform="translate(5 5)"><text x="0" y="0" font-size="1" transform="matrix(12 0 0 12 30 40)">Hello</text></g></svg>"#,
    );
    assert!(close(x, 35.0) && close(y, 45.0), "the baseline at (35, 45): ({x}, {y})");
    assert!(close(size, 12.0), "a 1-unit font scaled to 12 px: {size}");
}

#[test]
fn a_tspans_dx_moves_its_first_character_and_scales_with_the_svg() {
    // printpdf's kerning: `<tspan dx>` before the next characters.
    let second_x = |width: u32, dx: &str| {
        glyphs(&format!(
            r#"<svg width="{width}" height="{h}" viewBox="0 0 200 100"><text x="10" y="50" font-size="20">A<tspan dx="{dx}">B</tspan></text></svg>"#,
            h = width / 2
        ))[1]
            .0
    };
    let moved = second_x(200, "30") - second_x(200, "0");
    assert!(close(moved, 30.0), "dx 30 moves B by 30 px: {moved}");
    let scaled = second_x(400, "30") - second_x(400, "0");
    assert!(close(scaled, 60.0), "twice that at twice the size: {scaled}");
    let back = second_x(200, "-5") - second_x(200, "0");
    assert!(close(back, -5.0), "a negative dx (kerning) moves it back: {back}");
}

#[test]
fn a_point_on_a_word_of_a_scaled_text_hits_that_word() {
    // printpdf's text: the page drawn at 1.5x its viewBox. "AzPdf" is the
    // 15th..19th character; a point in its middle must hit a cursor inside
    // it, not the start of the text.
    let markup = r#"<html><body style="margin: 0px"><svg width="918" height="1188" viewBox="0 0 612 792"><text x="0" y="0" font-family="Helvetica, Arial, sans-serif" font-size="28" transform="matrix(1 -0 -0 1 72 92)">Page 1 of the AzPdf test</text></svg></body></html>"#;
    let parsed = azul_layout::xml::parse_xml(markup).expect("the markup parses");
    let mut dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(1000.0, 1200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .unwrap();
    let xs: Vec<(f32, f32)> = {
        let dl = &lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list;
        dl.items
            .iter()
            .filter_map(|item| match item {
                DisplayListItem::Text { glyphs, .. } => {
                    Some(glyphs.iter().map(|g| (g.point.x, g.point.y)).collect::<Vec<_>>())
                }
                _ => None,
            })
            .flatten()
            .collect()
    };
    // The middle of "AzPdf": between its 2nd and 3rd glyph, a little above
    // the baseline.
    let (x, y) = ((xs[15].0 + xs[16].0) / 2.0 + 0.5 * (xs[17].0 - xs[16].0), xs[15].1 - 10.0);
    let (block, cursor) = lw
        .hittest_text_position_global(DomId::ROOT_ID, azul_core::geom::LogicalPosition::new(x, y))
        .expect("the point hits the text");
    let byte = lw.byte_offset_of_cursor(block, &cursor).expect("a byte of the text");
    assert!(
        (14..=19).contains(&byte),
        "a point on \"AzPdf\" ({x}, {y}) hits a cursor inside it, not byte {byte}"
    );
}
