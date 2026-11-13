//! A pane opens and closes by animating its width, the ribbon's panel by
//! animating its height - on the design's curve, with the boxes after them
//! moving along, and every frame cheap.
//!
//! AzDrive's navigation pane (228px) closes to 0 and opens again over 260ms,
//! the ribbon's panel opens from 0 to its 98px (SPEC 3.2: both 260ms on
//! `--ease`, `cubic-bezier(.25, .46, .45, .94)`), and the leaf beside the
//! pane / the rule under the panel follow every frame. A frame of such a
//! tween relays out the box and its ancestors - the leaf beside it is placed
//! again but none of its hundreds of rows is measured again.
//!
//! The frame driver is the loop the accordion test uses: tick, and relay out
//! when a layout property moved. The values at a quarter, a half and the
//! end of the tween are the curve's (solved by bisection on the curve's x):
//! ease(0.25) = 0.45338, ease(0.5) = 0.77132.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::props::{
    layout::{LayoutHeight, LayoutWidth},
    property::CssProperty,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, overlay::ContentChange, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const EASE: &str = "cubic-bezier(.25,.46,.45,.94)";
const DURATION_S: f32 = 0.26;
const PANE: f32 = 228.0;
const PANEL: f32 = 98.0;
const EASE_QUARTER: f32 = 0.453_376;
const EASE_HALF: f32 = 0.771_324;

fn window(dom: Dom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the window lays out");
    lw
}

/// Relayout over the window's own styled DOM - what the frame driver does
/// when a transition moved a layout property.
fn relayout(lw: &mut LayoutWindow) {
    let result = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("harness: laid out");
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        result.styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the window lays out again");
}

/// One frame `dt` seconds after the last.
fn frame(lw: &mut LayoutWindow, dt: f32) {
    lw.tick_animations(dt);
    if lw.take_transition_relayout() {
        relayout(lw);
    }
}

fn rect(lw: &LayoutWindow, index: usize) -> LogicalRect {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .unwrap_or_else(|| panic!("node {index} is laid out"))
}

fn assert_near(what: &str, got: f32, want: f32) {
    assert!((got - want).abs() < 0.5, "{what}: {want:.2} wanted, {got:.2}");
}

const NAV: usize = 2;
const NAV_ROWS: usize = 10;
/// The leaf after the pane: body (0) > row (1) > [nav (2) > NAV_ROWS x (div,
/// text), leaf > LEAF_ROWS x (div, text)].
const LEAF: usize = NAV + 1 + 2 * NAV_ROWS;
const LEAF_ROWS: usize = 200;

/// The pane beside the leaf, `nav_width` wide, tweening its width.
fn shell(nav_width: f32) -> LayoutWindow {
    let row = |i: usize| {
        Dom::create_div()
            .with_css("height: 20px;")
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(format!(
                "item {i}"
            )))
    };
    let mut nav = Dom::create_div().with_css(&format!(
        "width: {nav_width}px; flex-shrink: 0; overflow: hidden; \
         animation: width 260ms {EASE};"
    ));
    for i in 0..NAV_ROWS {
        nav = nav.with_child(row(i));
    }
    let mut leaf = Dom::create_div().with_css("flex-grow: 1; overflow: hidden;");
    for i in 0..LEAF_ROWS {
        leaf = leaf.with_child(row(i));
    }
    window(
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_div()
                .with_css("display: flex; width: 800px; height: 500px;")
                .with_child(nav)
                .with_child(leaf),
        ),
    )
}

fn set_nav_width(lw: &mut LayoutWindow, px: isize) {
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: NodeId::new(NAV),
        props: vec![CssProperty::const_width(LayoutWidth::const_px(px))],
        override_only: false,
    });
    relayout(lw);
}

#[test]
fn closing_the_navigation_pane_walks_its_width_down_the_designs_curve() {
    let mut lw = shell(PANE);
    set_nav_width(&mut lw, 0);
    assert_near("t = 0: the pane is still open", rect(&lw, NAV).size.width, PANE);
    assert_near("the leaf starts at the pane's edge", rect(&lw, LEAF).origin.x, PANE);

    frame(&mut lw, DURATION_S / 4.0);
    let quarter = rect(&lw, NAV).size.width;
    assert_near("a quarter in", quarter, PANE * (1.0 - EASE_QUARTER));
    assert_near("the leaf follows the pane's edge", rect(&lw, LEAF).origin.x, quarter);

    frame(&mut lw, DURATION_S / 4.0);
    assert_near("half way", rect(&lw, NAV).size.width, PANE * (1.0 - EASE_HALF));

    frame(&mut lw, DURATION_S / 2.0);
    assert_near("the end: closed", rect(&lw, NAV).size.width, 0.0);
    assert_near("the leaf takes the whole row", rect(&lw, LEAF).size.width, 800.0);
    assert!(lw.css_transitions.is_empty(), "the tween is over");
}

#[test]
fn opening_the_navigation_pane_walks_its_width_up_the_same_curve() {
    let mut lw = shell(0.0);
    set_nav_width(&mut lw, PANE as isize);
    assert_near("t = 0: still closed", rect(&lw, NAV).size.width, 0.0);
    frame(&mut lw, DURATION_S / 2.0);
    assert_near("half way", rect(&lw, NAV).size.width, PANE * EASE_HALF);
    frame(&mut lw, DURATION_S / 2.0);
    assert_near("the end: open", rect(&lw, NAV).size.width, PANE);
}

#[test]
fn a_frame_of_the_pane_tween_measures_the_pane_and_its_ancestors_only() {
    let mut lw = shell(PANE);
    set_nav_width(&mut lw, 0);
    let passes = lw.frame_report.layout_passes;
    frame(&mut lw, DURATION_S / 4.0);
    assert_eq!(lw.frame_report.layout_passes, passes + 1, "one layout pass per frame");
    let measured = lw.layout_cache.last_intrinsic_dirty;
    assert!(
        measured <= 4,
        "a frame re-measures the pane and its ancestors (body, the row), not the leaf's \
         {LEAF_ROWS} rows: {measured} nodes"
    );
    assert_eq!(lw.layout_cache.last_reconcile_fresh, 0, "no box is built anew");
}

#[test]
fn the_ribbon_panel_opens_by_animating_its_height_and_the_rule_follows() {
    // body (0) > [panel (1) > content (2), rule (3)]
    let mut lw = window(
        Dom::create_body()
            .with_css("margin: 0px;")
            .with_child(
                Dom::create_div()
                    .with_css(&format!(
                        "height: 0px; overflow: hidden; animation: height 260ms {EASE};"
                    ))
                    .with_child(Dom::create_div().with_css(&format!("height: {PANEL}px;"))),
            )
            .with_child(Dom::create_div().with_css("height: 2px;")),
    );
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: NodeId::new(1),
        props: vec![CssProperty::const_height(LayoutHeight::const_px(PANEL as isize))],
        override_only: false,
    });
    relayout(&mut lw);
    assert_near("t = 0: folded", rect(&lw, 1).size.height, 0.0);
    frame(&mut lw, DURATION_S / 2.0);
    let half = rect(&lw, 1).size.height;
    assert_near("half way", half, PANEL * EASE_HALF);
    assert_near("the rule sits at the panel's foot", rect(&lw, 3).origin.y, half);
    frame(&mut lw, DURATION_S / 2.0);
    assert_near("the end: open", rect(&lw, 1).size.height, PANEL);
    assert_near("the rule under the open panel", rect(&lw, 3).origin.y, PANEL);
}
