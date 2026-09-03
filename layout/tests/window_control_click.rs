//! A window control's click has to reach it — and its behaviour now lives
//! INSIDE a `VirtualView`.
//!
//! The maximize control renders through a view, because its glyph depends on
//! the live window frame, and the click it carries is rendered with the glyph
//! rather than attached to the button around it: one place produces both, so
//! they cannot drift apart. That moves the callback into a NESTED dom — a
//! document of its own, composited by the host — and "does a press in there
//! still dispatch?" is not something the widget code can answer.
//!
//! So this presses the middle of the control the way the shells do (CPU hit
//! test → `FullHitTest`) and asserts the press lands on a node that carries a
//! callback, in the view's nested dom. It also presses the button's CORNER:
//! the view fills the button precisely so the hit area is the whole control
//! and not just the glyph, which is the part a careless `width: 16px` would
//! silently take away.

use azul_core::{
    dom::{Dom, DomId, NodeId, NodeType},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    headless::{convert_cpu_hit_test_to_full, CpuHitTester},
    widgets::quick_access::QuickAccessBar,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn laid_out_band() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("LayoutWindow::new");
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(1280.0, 800.0);
    lw.current_window_state = window_state.clone();

    let mut dom = Dom::create_body()
        .with_css("display: block;")
        .with_child(QuickAccessBar::new("AzWriter".into()).dom());
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    lw.layout_and_generate_display_list(
        styled,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("layout");
    lw
}

/// The band's only `VirtualView` — the maximize control.
fn maximize_view_node(lw: &LayoutWindow) -> NodeId {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root dom laid out");
    let nd = lr.styled_dom.node_data.as_container();
    (0..nd.len())
        .map(NodeId::new)
        .find(|id| matches!(nd[*id].get_node_type(), NodeType::VirtualView))
        .expect("the band renders the maximize control as a view")
}

/// Window-space rect of a node, the way the rasteriser places it.
fn window_rect(lw: &LayoutWindow, dom: DomId, node: NodeId) -> (LogicalPosition, LogicalSize) {
    let lr = lw.get_layout_result(&dom).expect("layout result");
    let idx = *lr
        .layout_tree
        .dom_to_layout
        .get(&node)
        .and_then(|v| v.first())
        .expect("the node is in the layout tree");
    let pos = lr
        .calculated_positions
        .get(idx.index())
        .copied()
        .expect("a position");
    let size = lr.layout_tree.nodes[idx.index()]
        .used_size
        .expect("a used size");
    let lift = lw.window_space_offset_of_dom(dom);
    (
        LogicalPosition::new(pos.x + lift.x, pos.y + lift.y),
        size,
    )
}

/// The shells' CPU hit-test arm, at one point.
fn press_at(lw: &LayoutWindow, position: LogicalPosition) -> azul_layout::hit_test::FullHitTest {
    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
    let scroll_manager = &lw.scroll_manager;
    let gpu = &lw.gpu_state_manager;
    let resolve = |d: DomId, n: NodeId| scroll_manager.get_current_offset(d, n);
    let resolve_tf = |d: DomId, n: NodeId| {
        gpu.caches
            .get(&d)
            .and_then(|c| c.css_current_transform_values.get(&n))
            .copied()
    };
    let hits = tester.hit_test_scrolled(position, &resolve, &resolve_tf);
    convert_cpu_hit_test_to_full(
        &tester,
        &hits,
        None,
        &lw.layout_results,
        position,
        &resolve,
        &resolve_tf,
    )
}

/// Does the press land on a node that carries at least one callback?
fn hits_a_callback(lw: &LayoutWindow, hit: &azul_layout::hit_test::FullHitTest) -> bool {
    hit.hovered_nodes.iter().any(|(dom_id, per_dom)| {
        let Some(lr) = lw.get_layout_result(dom_id) else {
            return false;
        };
        let nd = lr.styled_dom.node_data.as_container();
        per_dom
            .regular_hit_test_nodes
            .keys()
            .any(|n| nd.get(*n).is_some_and(|d| !d.get_callbacks().as_ref().is_empty()))
    })
}

#[test]
fn pressing_the_maximize_control_reaches_the_callback_its_view_rendered() {
    let lw = laid_out_band();
    let view = maximize_view_node(&lw);
    let nested = lw
        .virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, view)
        .expect("the view materialized a nested dom");

    let (origin, size) = window_rect(&lw, DomId::ROOT_ID, view);
    assert!(
        size.width > 0.0 && size.height > 0.0,
        "the control has a box to press: {size:?}"
    );

    let centre = LogicalPosition::new(
        origin.x + size.width / 2.0,
        origin.y + size.height / 2.0,
    );
    let hit = press_at(&lw, centre);
    assert!(
        hit.hovered_nodes.contains_key(&nested),
        "a press on the control has to reach the view's nested dom {nested:?}; hit {:?}",
        hit.hovered_nodes.keys().collect::<Vec<_>>()
    );
    assert!(
        hits_a_callback(&lw, &hit),
        "and land on the callback the view rendered with the glyph"
    );
}

#[test]
fn the_whole_control_is_pressable_including_its_corners() {
    // The glyph moved into a view; the CLICK did not, and this is why. The
    // view was written to carry it - one place producing both what the control
    // looks like and what it does - and two replaced-element sizing bugs make
    // that unsafe: a view cannot be made to cover its button, so a click on it
    // shrinks the target to the glyph. See `maximize_icon_view` for the
    // measurements. This pins the property that decides it: every part of the
    // control is pressable, including the corners the glyph does not cover.
    let lw = laid_out_band();
    let view = maximize_view_node(&lw);
    let button = lw
        .get_layout_result(&DomId::ROOT_ID)
        .and_then(|lr| lr.styled_dom.node_hierarchy.as_container()[view].parent_id())
        .expect("the view sits inside the control's button");
    let (origin, size) = window_rect(&lw, DomId::ROOT_ID, button);

    // Inset by 2px so the samples clear the 1px chassis border.
    let inset = 2.0;
    for (dx, dy, where_) in [
        (size.width / 2.0, size.height / 2.0, "centre"),
        (inset, inset, "top-left"),
        (size.width - inset, inset, "top-right"),
        (inset, size.height - inset, "bottom-left"),
        (size.width - inset, size.height - inset, "bottom-right"),
    ] {
        let at = LogicalPosition::new(origin.x + dx, origin.y + dy);
        assert!(
            hits_a_callback(&lw, &press_at(&lw, at)),
            "a press at the control's {where_} ({at:?}) has to reach its callback"
        );
    }
}
