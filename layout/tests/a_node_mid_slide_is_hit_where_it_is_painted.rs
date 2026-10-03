//! A node mid-slide is hit where it is painted - the animation channel's
//! transform moves its hit box as it moves its pixels.
//!
//! An engine-driven transition (the reconcile's FLIP slide, a keyframe
//! `transform`) moves a node that has no CSS `transform` of its own through
//! the GPU value cache's ANIMATION channel (`anim_transform_keys` /
//! `anim_current_transform_values`). The display list wraps such a node in a
//! reference frame bound to that channel, so the raster paints it moved. The
//! hit tester built its chains from the CSS channel only
//! (`css_transform_keys`) and both hosts resolved matrices from
//! `css_current_transform_values` only: a node mid-slide was hit at its
//! static place (HEADLESS6, 2026-10-03).
//!
//! The one lookup is `GpuStateManager::painted_transform_of` - the display
//! list's rule (CSS channel first, then the animation channel).

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::{RendererResources, TransformKey},
    styled_dom::StyledDom,
    transform::ComputedTransform3D,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, headless::CpuHitTester, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body(0) > slider(1) > box(2); body > below(3).
const SLIDER: NodeId = NodeId::new(1);
const BOX: NodeId = NodeId::new(2);
const BELOW: NodeId = NodeId::new(3);

/// `#slider` (no CSS transform) half way through a slide by (120, 80), the
/// way the animation channel carries it.
fn window_mid_slide() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let page = Dom::create_body()
        .with_css("margin: 0; padding: 0;")
        .with_child(
            Dom::create_div()
                .with_css("width: 100px; height: 50px;")
                .with_child(
                    Dom::create_div().with_css("width: 100px; height: 50px; background: red;"),
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css("position: absolute; left: 0px; top: 0px; width: 100px; height: 50px;"),
        );
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    let cache = lw.gpu_state_manager.get_or_create_cache(DomId::ROOT_ID);
    cache
        .anim_transform_keys
        .insert(SLIDER, TransformKey::unique());
    cache.anim_current_transform_values.insert(
        SLIDER,
        ComputedTransform3D::new_translation(120.0, 80.0, 0.0),
    );
    lw
}

/// The nodes of the root dom under `(x, y)`, topmost first, resolved the way
/// the hosts resolve them.
fn hits_at(lw: &LayoutWindow, x: f32, y: f32) -> Vec<NodeId> {
    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
    let resolve_scroll = |d: DomId, n: NodeId| lw.scroll_manager.get_current_offset(d, n);
    let resolve_tf = |d: DomId, n: NodeId| lw.gpu_state_manager.painted_transform_of(d, n);
    tester
        .hit_test_scrolled(LogicalPosition::new(x, y), &resolve_scroll, &resolve_tf)
        .into_iter()
        .filter(|(d, _, _)| *d == DomId::ROOT_ID)
        .map(|(_, n, _)| n)
        .collect()
}

#[test]
fn the_painted_transform_of_a_sliding_node_is_its_animation_matrix() {
    let lw = window_mid_slide();
    let t = lw
        .gpu_state_manager
        .painted_transform_of(DomId::ROOT_ID, SLIDER)
        .expect("the sliding node is painted through a reference frame");
    assert_eq!((t.m[3][0], t.m[3][1]), (120.0, 80.0));
}

#[test]
fn a_node_mid_slide_is_hit_where_it_is_painted() {
    let lw = window_mid_slide();
    let painted = hits_at(&lw, 160.0, 105.0);
    assert!(
        painted.contains(&BOX) && painted.contains(&SLIDER),
        "the sliding box is painted at (120,80)-(220,130): a press there hits it, got \
         {painted:?}"
    );
}

#[test]
fn a_node_mid_slide_is_not_hit_at_its_static_place() {
    let lw = window_mid_slide();
    let hits = hits_at(&lw, 50.0, 25.0);
    assert!(
        !hits.contains(&BOX) && !hits.contains(&SLIDER),
        "nothing of the sliding box is painted at its static place: got {hits:?}"
    );
    assert_eq!(
        hits.first(),
        Some(&BELOW),
        "the box painted there is hit, got {hits:?}"
    );
}
