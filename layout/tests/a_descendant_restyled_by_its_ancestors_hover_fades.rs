//! A rule that names a state LEFT of its subject (`.btn:hover .ink`) restyles
//! the subject when the anchor's state changes, and a subject that declares
//! an `animation` for the property fades into the new value - the same fade
//! it gets for a state of its own, as a CSS transition does in a browser.
//! The restyle reports the subject's change (its previous and current
//! value), but the transitions were seeded only for the nodes whose own
//! state changed: the ink snapped.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{HoverChange, StyledDom},
};
use azul_css::props::property::CssPropertyType;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = "
    .btn { width: 100px; height: 20px; }
    .ink { width: 10px; height: 10px; opacity: 0.2; animation: opacity 150ms linear; }
    .btn:hover .ink { opacity: 1; }
";

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 200.0);
    lw.current_window_state = ws.clone();
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_class("btn")
            .with_child(Dom::create_div().with_class("ink")),
    );
    lw.layout_and_generate_display_list(
        StyledDom::create(&mut dom, azul_css::parser2::new_from_str(CSS).0),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    lw
}

fn with_class(lw: &LayoutWindow, class: &str) -> NodeId {
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| styled.node_data.as_container()[*id].has_class(class))
        .unwrap_or_else(|| panic!("no .{class}"))
}

#[test]
fn the_ink_of_a_hovered_button_fades_in_as_its_rule_declares() {
    let mut lw = window();
    let (btn, ink) = (with_class(&lw, "btn"), with_class(&lw, "ink"));

    // The pointer enters the button, as the shell applies it.
    let before = lw.node_states(DomId::ROOT_ID, [btn]);
    let restyled = lw
        .layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .restyle_on_state_change(
            None,
            Some(HoverChange {
                left_nodes: Vec::new(),
                entered_nodes: vec![btn],
            }),
            None,
        );
    assert!(
        restyled.changed_nodes.get(&ink).is_some_and(|props| props
            .iter()
            .any(|c| c.current_prop.get_type() == CssPropertyType::Opacity)),
        "harness: the hover restyles the ink's opacity through `.btn:hover .ink`"
    );
    let _ = lw.seed_state_change_transitions(DomId::ROOT_ID, &before, &restyled.changed_nodes);

    assert!(
        lw.css_transitions
            .iter()
            .any(|t| t.node == ink && t.prop_type == CssPropertyType::Opacity),
        "the ink declares `animation: opacity 150ms`: it fades in, it does not snap"
    );
}
