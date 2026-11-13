//! A date picker's week rows are one cell tall, its numbers centred in them.
//!
//! The user's screenshot (date-picker-broken.png) shows the grid spread out:
//! every day cell laid out 33 px tall where the widget declares 28 px rows.
//! The cell asked for `height: 28px` and `padding-top: 5px` to push a 13 px
//! number down to the middle, and with the default `box-sizing: content-box`
//! the padding comes on top of the height - in Chrome too. The blank cells
//! that open a month (no padding) stay 28 px, so the first row and the rest
//! disagree. A line box as tall as the cell centres the number instead.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{date_picker::DatePicker, themes::UiTheme},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The row height every calendar cell declares (`date_picker::CELL_H`).
const ROW: f32 = 28.0;

/// An inline June 2026 calendar (it opens on a Monday: one blank before the
/// 1st) in `theme`, laid out.
fn calendar(theme: UiTheme) -> LayoutWindow {
    let mut dom = Dom::create_body().with_css("margin: 0;").with_child(
        DatePicker::create(2026, 6, 23)
            .with_inline(true)
            .with_theme(theme)
            .dom(),
    );
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 400.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the calendar lays out");
    lw
}

/// The laid-out height and top of every node carrying `class`, in tree order.
fn boxes(lw: &LayoutWindow, class: &str) -> Vec<(f32, f32)> {
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .filter(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .filter_map(|n| {
            lw.get_node_layout_rect(DomNodeId {
                dom: DomId::ROOT_ID,
                node: Some(n).into(),
            })
        })
        .map(|r| (r.size.height, r.origin.y))
        .collect()
}

fn assert_rows_are_one_cell_tall(theme: UiTheme) {
    let lw = calendar(theme);
    let weeks = boxes(&lw, "__azul-native-date-picker-week");
    let days = boxes(&lw, "__azul-native-date-picker-day");
    assert!(
        weeks.len() >= 5 && days.len() >= 30,
        "harness: a month of weeks and days, {} / {}",
        weeks.len(),
        days.len()
    );
    for (height, _) in &days {
        assert!(
            (height - ROW).abs() < 0.5,
            "{theme:?}: a day cell is {height} px tall, its row is {ROW} px"
        );
    }
    for (height, _) in &weeks {
        assert!(
            (height - ROW).abs() < 0.5,
            "{theme:?}: a week row is {height} px tall, its cells are {ROW} px"
        );
    }
    for pair in weeks.windows(2) {
        let pitch = pair[1].1 - pair[0].1;
        assert!(
            (pitch - ROW).abs() < 0.5,
            "{theme:?}: the weeks are {pitch} px apart, a row is {ROW} px"
        );
    }
}

#[test]
fn a_flat_date_pickers_rows_are_one_cell_tall() {
    assert_rows_are_one_cell_tall(UiTheme::Flat);
}

#[test]
fn a_flora_date_pickers_rows_are_one_cell_tall() {
    assert_rows_are_one_cell_tall(UiTheme::Flora);
}
