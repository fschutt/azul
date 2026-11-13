//! A fixed-size widget in a column sits at the column's start edge, like the
//! other widgets do.
//!
//! Segmented and Pagination hug their content with `align-self: start`. The
//! Switch track and the Slider track said `align-self: center` - meant to
//! centre them vertically next to a label in a ROW, it centres them
//! HORIZONTALLY in a column. AzWidgets lays every widget out in a labelled
//! column, so those two sat in the middle of their cards (Switch at x=300,
//! Slider at x=220 in a card whose content starts at x=50) while every other
//! widget started at the left.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{slider::Slider, switch::Switch},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn x_of_class(lw: &LayoutWindow, class: &str) -> Option<f32> {
    let lr = lw.get_layout_result(&DomId::ROOT_ID)?;
    let container = lr.styled_dom.node_data.as_container();
    (0..container.len()).map(NodeId::new).find_map(|nid| {
        let has = container[nid].attributes().as_ref().iter().any(|a| {
            a.as_class().is_some_and(|c| {
                let s: &str = c;
                s == class
            })
        });
        if !has {
            return None;
        }
        let idx = *lr.layout_tree.dom_to_layout.get(&nid)?.first()?;
        Some(lr.calculated_positions.get(idx.index())?.x)
    })
}

/// `body > div(column, 400px) > widget`, laid out at 640x480.
fn laid_out_in_a_column(widget: Dom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = window_state.clone();
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; width: 400px;")
            .with_child(widget),
    );
    let (css, _) = azul_css::parser2::new_from_str("body { margin: 0; }");
    let styled = StyledDom::create(&mut dom, css);
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();
    lw
}

#[test]
fn a_switch_in_a_column_sits_at_its_start() {
    let lw = laid_out_in_a_column(Switch::create(false).dom());
    let x = x_of_class(&lw, "__azul-native-switch").expect("the switch track is laid out");
    assert_eq!(
        x, 0.0,
        "the 40px switch starts at the column's left edge, not in its middle"
    );
}

#[test]
fn a_slider_in_a_column_sits_at_its_start() {
    let lw = laid_out_in_a_column(Slider::create(50.0, 0.0, 100.0).dom());
    let x = x_of_class(&lw, "__azul-native-slider").expect("the slider track is laid out");
    assert_eq!(
        x, 0.0,
        "the 200px slider starts at the column's left edge, not in its middle"
    );
}
