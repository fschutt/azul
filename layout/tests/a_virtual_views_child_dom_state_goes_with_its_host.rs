//! A `VirtualView`'s child DOM goes with its host node: when a rebuild
//! unmounts the host, every manager drops what it held for the child DOM -
//! the GPU value cache keyed by that DOM included.
//!
//! Found by the /e2e corpus run against a real app (HEADLESS6, 2026-10-03):
//! AzPaint's status bar shows a ProgressBar, which renders through a
//! `VirtualView` (child DOM 1). Every scenario starts with `mount`, which
//! replaces the app's DOM, and 13 of the 62 scenarios then failed
//! `assert_manager_invariants` with "X10 gpu_state: a GPU value cache is
//! still held for DOM 1 which no longer exists". The remap that runs after
//! every reconciliation (`LayoutWindow::remap_node_ids`) already computed the
//! child DOMs the rebuild takes down (`nested_doms_dropped_by`), but only the
//! thread owners heard about them; the managers keyed by those DOMs kept
//! their state forever.

use azul_core::{
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    task::{Instant, SystemTick},
};
use azul_layout::{managers::NodeIdMap, window::LayoutWindow};
use rust_fontconfig::FcFontCache;

fn rect(w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(w, h))
}

#[test]
fn a_virtual_views_child_dom_state_goes_with_its_host() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");

    // The root DOM hosts a VirtualView at node 12 (AzPaint's progress bar);
    // its content is child DOM `child`, which holds a GPU value cache and a
    // scroll state of its own.
    let host = NodeId::new(12);
    let child = lw
        .virtual_view_manager
        .get_or_create_nested_dom_id(DomId::ROOT_ID, host);
    assert_ne!(child, DomId::ROOT_ID);
    let _ = lw.gpu_state_manager.get_or_create_cache(child);
    lw.scroll_manager.update_node_bounds(
        child,
        NodeId::new(0),
        rect(100.0, 10.0),
        rect(100.0, 40.0),
        Instant::Tick(SystemTick { tick_counter: 0 }),
    );
    assert!(lw.gpu_state_manager.caches.contains_key(&child));
    assert!(lw
        .scroll_manager
        .state_keys()
        .contains(&(child, NodeId::new(0))));

    // A rebuild of the root DOM in which no old node survived (the e2e
    // `mount` op, or any app whose new DOM shares nothing with the old one):
    // the host is unmounted, and with it the child DOM.
    lw.remap_node_ids(DomId::ROOT_ID, &NodeIdMap::default());

    assert_eq!(
        lw.virtual_view_manager
            .get_nested_dom_id(DomId::ROOT_ID, host),
        None,
        "the unmounted host's VirtualView state is dropped"
    );
    assert!(
        !lw.gpu_state_manager.caches.contains_key(&child),
        "the GPU value cache of the unmounted VirtualView's child DOM {} must go with it",
        child.inner
    );
    assert!(
        !lw.scroll_manager
            .state_keys()
            .iter()
            .any(|(dom, _)| *dom == child),
        "the scroll states of the unmounted VirtualView's child DOM {} must go with it",
        child.inner
    );
}

#[test]
fn a_surviving_virtual_view_keeps_its_child_dom_state() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");

    let host = NodeId::new(12);
    let child = lw
        .virtual_view_manager
        .get_or_create_nested_dom_id(DomId::ROOT_ID, host);
    let _ = lw.gpu_state_manager.get_or_create_cache(child);

    // The host survives the rebuild (moved from index 12 to 3): the child DOM
    // is the same document, its state stays.
    lw.remap_node_ids(
        DomId::ROOT_ID,
        &NodeIdMap::from_pairs([(NodeId::new(0), NodeId::new(0)), (host, NodeId::new(3))]),
    );

    assert_eq!(
        lw.virtual_view_manager
            .get_nested_dom_id(DomId::ROOT_ID, NodeId::new(3)),
        Some(child)
    );
    assert!(lw.gpu_state_manager.caches.contains_key(&child));
}
