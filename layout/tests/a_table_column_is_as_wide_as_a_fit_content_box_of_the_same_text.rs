//! A table column sized by its text is exactly as wide as a shrink-to-fit box
//! of the same text: both are the text's intrinsic width, which is ceiled to a
//! 1/64 px grid (Chrome's LayoutUnit). The table measured its cells by the laid
//! out line instead and took the raw advance sum, so the next column started
//! up to 1/64 px further left and its text was anti-aliased differently from
//! the reference (WPT `css-tables/table-cell-width-0` on Linux, whose default
//! face has fractional advances).

use azul_core::{dom::DomId, geom::LogicalSize, resources::RendererResources};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The pen x of every glyph the page paints, in order.
fn glyph_xs(xml: &str) -> Vec<f32> {
    let styled = azul_layout::xml::parse_xml_to_styled_dom(xml).expect("the page parses");
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut config = azul_core::resources::AppConfig::create();
    for face in azul_fonts_calmet::faces() {
        config.add_font(azul_fonts_calmet::FAMILY.into(), face.to_vec().into());
    }
    lw.font_manager.register_app_fonts(config.bundled_fonts.as_ref());
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .iter()
        .flat_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } => glyphs.iter().map(|g| g.point.x).collect(),
            _ => Vec::new(),
        })
        .collect()
}

const FONT: &str = "font-family: 'Calmet UI'; font-size: 16px;";

#[test]
fn the_next_column_starts_where_the_next_flex_item_does() {
    let table = glyph_xs(&format!(
        "<html><body style=\"margin: 0px;\"><table style=\"border-collapse: collapse; \
         border: 2px solid black; {FONT}\"><tbody><tr><td style=\"width: 0px; padding: 1px;\">11</td>\
         <td style=\"padding: 1px;\">2</td></tr></tbody></table></body></html>"
    ));
    let flex = glyph_xs(&format!(
        "<html><body style=\"margin: 0px;\"><div style=\"display: flex; width: fit-content; \
         border: 2px solid black; {FONT}\"><div style=\"padding: 1px; width: fit-content;\">11</div>\
         <div style=\"padding: 1px; flex: 1;\">2</div></div></body></html>"
    ));
    assert_eq!(table.len(), 3, "premise: three glyphs in the table: {table:?}");
    assert_eq!(flex.len(), 3, "premise: three glyphs in the flex row: {flex:?}");
    let column = table[2] - table[0];
    let item = flex[2] - flex[0];
    assert!(
        (column * 64.0).fract().abs() < 1e-3 || (column * 64.0).fract() > 0.999,
        "the column's text starts on the 1/64 px grid: {column}"
    );
    assert!(
        (column - item).abs() < 1e-3,
        "the second column's text starts where the second item's does: {column} vs {item}"
    );
}
