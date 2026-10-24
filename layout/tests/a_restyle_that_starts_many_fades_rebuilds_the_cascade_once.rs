//! A restyle that starts many fades rebuilds the cascade once.
//!
//! Each fade a state change or a rebuild starts holds its node at the value
//! it fades from, through the user-override layer, and the cascade's derived
//! tables (the compact cache, the inheritance) must then agree with that
//! layer. They were rebuilt for the whole DOM once per fade: a pointer on a
//! flora button starts five (its face and four borders), and every open,
//! close or switch of AzDrive's ribbon content started its own, so a hover
//! or a rebuild paid the whole window's cascade as many times again. The
//! overrides of one restyle are written together and the tables rebuilt
//! once; reading what is under a running fade's override (to start the next
//! one from it) rebuilds nothing.
//!
//! The count is `compact_cache_rebuilds_on_this_thread`, not a time.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{compact_cache_rebuilds_on_this_thread, HoverChange, StyledDom},
    task::Instant,
    window::{DarkLightMode, OptionDarkLightMode},
};
use azul_css::{system::defaults, AzString};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        button::{Button, ButtonType},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn window_state() -> FullWindowState {
    let mut ws = FullWindowState {
        mode: DarkLightMode::Light,
        ..Default::default()
    };
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    ws
}

fn laid_out(dom: Dom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(defaults::macos_modern_light()));
    lw.mode = OptionDarkLightMode::None;
    let ws = window_state();
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    lw
}

fn node_with_class(lw: &LayoutWindow, class: &str) -> NodeId {
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    (0..styled.node_data.as_ref().len())
        .map(NodeId::new)
        .find(|n| {
            styled.node_data.as_container()[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == class))
        })
        .unwrap_or_else(|| panic!("harness: a node with class {class}"))
}

/// The pointer enters `node`, as the shell applies it; returns the cascade
/// rebuilds the transitions' seeding cost.
fn hover(lw: &mut LayoutWindow, node: NodeId) -> usize {
    let before = lw.node_states(DomId::ROOT_ID, [node]);
    let restyled = lw
        .layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom
        .restyle_on_state_change(
            None,
            Some(HoverChange {
                left_nodes: Vec::new(),
                entered_nodes: vec![node],
            }),
            None,
        );
    let rebuilds = compact_cache_rebuilds_on_this_thread();
    let _ = lw.seed_state_change_transitions(DomId::ROOT_ID, &before, &restyled.changed_nodes);
    compact_cache_rebuilds_on_this_thread() - rebuilds
}

#[test]
fn a_hover_that_starts_many_fades_rebuilds_the_cascade_once() {
    let button = Button::with_type(AzString::from("Save"), ButtonType::Default)
        .with_theme(UiTheme::Flora);
    let mut lw = laid_out(Dom::create_body().with_css("margin: 0;").with_child(button.dom()));
    let node = node_with_class(&lw, "__azul-native-button");
    let rebuilds = hover(&mut lw, node);
    let fades = lw.css_transitions.iter().filter(|t| t.node == node).count();
    assert!(fades >= 2, "harness: the hover starts several fades, got {fades}");
    assert!(
        rebuilds <= 1,
        "the hover started {fades} fades and rebuilt the cascade {rebuilds} times - once is \
         enough for all of them"
    );

    // The pointer leaves and comes back while the fades run: every fade is
    // read under its override and restarted from where it stands.
    let leave = {
        let before = lw.node_states(DomId::ROOT_ID, [node]);
        let restyled = lw
            .layout_results
            .get_mut(&DomId::ROOT_ID)
            .expect("laid out")
            .styled_dom
            .restyle_on_state_change(
                None,
                Some(HoverChange {
                    left_nodes: vec![node],
                    entered_nodes: Vec::new(),
                }),
                None,
            );
        let rebuilds = compact_cache_rebuilds_on_this_thread();
        let _ =
            lw.seed_state_change_transitions(DomId::ROOT_ID, &before, &restyled.changed_nodes);
        compact_cache_rebuilds_on_this_thread() - rebuilds
    };
    assert!(
        leave <= 1,
        "leaving mid-fade turned {fades} fades around and rebuilt the cascade {leave} times"
    );
}

/// A box declaring fades of four properties, at the values of `step`.
fn faded_page(step: u32, all_four: bool) -> StyledDom {
    let (width, height, opacity, color) = match (step, all_four) {
        (0, _) => (100, 20, "1", "#000000"),
        (_, true) => (160, 40, "0.5", "#ff0000"),
        (_, false) => (160, 20, "1", "#000000"),
    };
    StyledDom::create_from_dom(Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div().with_css(&format!(
            "width: {width}px; height: {height}px; opacity: {opacity}; color: {color}; \
             animation: width 200ms linear, height 200ms linear, opacity 200ms linear, \
             color 200ms linear;"
        )),
    ))
}

/// The cascade rebuilds a rebuild of the page costs in its reconciliation,
/// and the fades it started.
fn rebuild_cost(all_four: bool) -> (usize, usize) {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let ws = window_state();
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        faded_page(0, all_four),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    let mut next = faded_page(1, all_four);
    let rebuilds = compact_cache_rebuilds_on_this_thread();
    let pending = lw.begin_reconciliation(DomId::ROOT_ID, &mut next, Instant::now());
    let rebuilds = compact_cache_rebuilds_on_this_thread() - rebuilds;
    lw.layout_new_generation(
        next,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the next DOM lays out");
    lw.finish_reconciliation(DomId::ROOT_ID, &pending);
    (rebuilds, lw.css_transitions.len())
}

#[test]
fn a_rebuild_that_starts_four_fades_rebuilds_the_cascade_no_more_often_than_one_that_starts_one() {
    let (one, one_fades) = rebuild_cost(false);
    let (four, four_fades) = rebuild_cost(true);
    assert_eq!(one_fades, 1, "harness: the width alone fades");
    assert_eq!(four_fades, 4, "harness: all four properties fade");
    assert_eq!(
        four, one,
        "a rebuild starting four fades rebuilt the cascade {four} times, one starting one fade \
         {one} times: the fades of one rebuild are written together"
    );
}
