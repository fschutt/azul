//! Headless regression test for `GpuValueCache::synchronize` wiring.
//!
//! `core/src/gpu.rs::GpuValueCache::synchronize` is the sole producer of
//! `css_transform_keys`, `css_current_transform_values`, `opacity_keys` and
//! `current_opacity_values`. `layout/src/solver3/display_list.rs` reads
//! those maps while emitting reference frames and opacity stacking contexts.
//! If `synchronize` is never called during the relayout flow, the maps stay
//! empty and CSS `transform` / `opacity` never reach WebRender.
//!
//! This test layouts a DOM with a node carrying non-default `transform` and
//! `opacity`, then verifies that after one relayout:
//! - The GPU cache has entries for the node's transform and opacity keys.
//! - The `GpuEventChanges` accumulated on `GpuStateManager.pending_changes`
//!   contain `Added` events for both.
//!
//! After mutating the DOM with new values, relayout must produce `Changed`
//! events and update the stored values.

use azul_core::{
    dom::{Dom, DomId, IdOrClass, NodeId},
    geom::LogicalSize,
    gpu::{GpuOpacityKeyEvent, GpuTransformKeyEvent},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::css::Css;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn relayout(lw: &mut LayoutWindow, dom: Dom, css_str: &str) {
    let css = Css::from_string(css_str.into());
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
}

fn build_dom() -> Dom {
    // Body > .target
    // node ids: 0=body, 1=.target
    Dom::create_body().with_child(
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("target".into())].into()),
    )
}

#[test]
fn gpu_synchronize_populates_caches_and_emits_added_events() {
    let font_cache = FcFontCache::build();
    let mut lw = LayoutWindow::new(font_cache).unwrap();

    let css = r#"
        body { width: 400px; height: 300px; }
        .target { width: 50px; height: 50px; opacity: 0.5; transform: translateX(10px); }
    "#;

    relayout(&mut lw, build_dom(), css);

    let target = NodeId::new(1);
    let dom_id = DomId::ROOT_ID;

    // Caches are populated by synchronize()
    let cache = lw
        .gpu_state_manager
        .get_cache(dom_id)
        .expect("gpu cache for ROOT_ID");
    assert!(
        cache.css_transform_keys.contains_key(&target),
        "css_transform_keys should contain the target node after relayout"
    );
    assert!(
        cache.css_current_transform_values.contains_key(&target),
        "css_current_transform_values should contain the target node after relayout"
    );
    assert!(
        cache.opacity_keys.contains_key(&target),
        "opacity_keys should contain the target node after relayout"
    );
    let stored_opacity = *cache
        .current_opacity_values
        .get(&target)
        .expect("current_opacity_values should contain the target node");
    // Opacity is quantised to u8 by the compact cache, so compare with a
    // tolerance that covers one step of 1/254.
    assert!(
        (stored_opacity - 0.5).abs() < 0.01,
        "stored opacity should match CSS (0.5), got {stored_opacity}"
    );

    // Pending events contain an Added for both transform and opacity
    let pending = lw.gpu_state_manager.take_pending_changes();
    let has_added_transform = pending.transform_key_changes.iter().any(|e| {
        matches!(e, GpuTransformKeyEvent::Added(nid, _, _) if *nid == target)
    });
    let has_added_opacity = pending.opacity_key_changes.iter().any(|e| {
        matches!(e, GpuOpacityKeyEvent::Added(nid, _, _) if *nid == target)
    });
    assert!(
        has_added_transform,
        "expected GpuTransformKeyEvent::Added for target, got {:?}",
        pending.transform_key_changes
    );
    assert!(
        has_added_opacity,
        "expected GpuOpacityKeyEvent::Added for target, got {:?}",
        pending.opacity_key_changes
    );
}

#[test]
fn gpu_synchronize_emits_changed_events_on_mutation() {
    let font_cache = FcFontCache::build();
    let mut lw = LayoutWindow::new(font_cache).unwrap();

    let first_css = r#"
        body { width: 400px; height: 300px; }
        .target { width: 50px; height: 50px; opacity: 0.5; transform: translateX(10px); }
    "#;
    relayout(&mut lw, build_dom(), first_css);
    // Drain the initial Added events so the next round only contains deltas.
    let _ = lw.gpu_state_manager.take_pending_changes();

    let second_css = r#"
        body { width: 400px; height: 300px; }
        .target { width: 50px; height: 50px; opacity: 0.25; transform: translateX(40px); }
    "#;
    relayout(&mut lw, build_dom(), second_css);

    let target = NodeId::new(1);
    let dom_id = DomId::ROOT_ID;

    // The cached values reflect the new CSS
    let cache = lw
        .gpu_state_manager
        .get_cache(dom_id)
        .expect("gpu cache for ROOT_ID");
    let stored_opacity = *cache
        .current_opacity_values
        .get(&target)
        .expect("current_opacity_values should still contain the target node");
    // Opacity is quantised to u8 by the compact cache, so compare with a
    // tolerance that covers one step of 1/254.
    assert!(
        (stored_opacity - 0.25).abs() < 0.01,
        "stored opacity should reflect updated CSS (0.25), got {stored_opacity}"
    );

    // Pending changes contain Changed events, not Added
    let pending = lw.gpu_state_manager.take_pending_changes();
    let has_changed_transform = pending.transform_key_changes.iter().any(|e| {
        matches!(e, GpuTransformKeyEvent::Changed(nid, _, _, _) if *nid == target)
    });
    let has_changed_opacity = pending.opacity_key_changes.iter().any(|e| {
        matches!(e, GpuOpacityKeyEvent::Changed(nid, _, _, _) if *nid == target)
    });
    assert!(
        has_changed_transform,
        "expected GpuTransformKeyEvent::Changed for target, got {:?}",
        pending.transform_key_changes
    );
    assert!(
        has_changed_opacity,
        "expected GpuOpacityKeyEvent::Changed for target, got {:?}",
        pending.opacity_key_changes
    );
}
