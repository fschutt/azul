//! A9, memory and allocation.
//!
//! Count-based guards: what a long session keeps stays bounded, what a frame allocates is
//! what it needs. Each test names its finding (`audit/A9.md`).

/// A9-01: a window whose frames are never `Full` (the macOS GPU path records WebRender's
/// dirty rects every frame) and whose counters nobody resets (every real session) keeps its
/// accumulated damage bounded, and the union it reports still covers every frame's damage.
#[cfg(feature = "text_layout")]
#[test]
fn a9_01_accumulated_damage_stays_bounded_over_a_long_gpu_session() {
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

    use crate::window::{FrameDamage, FrameReport, MAX_ACCUMULATED_DAMAGE_RECTS};

    let rect = |x: f32, y: f32, w: f32, h: f32| {
        LogicalRect::new(LogicalPosition { x, y }, LogicalSize { width: w, height: h })
    };
    let covered_by = |acc: &FrameDamage, r: &LogicalRect| match acc {
        FrameDamage::Full => true,
        FrameDamage::None => false,
        FrameDamage::Rects(list) => list.iter().any(|k| {
            k.origin.x <= r.origin.x
                && k.origin.y <= r.origin.y
                && r.origin.x + r.size.width <= k.origin.x + k.size.width
                && r.origin.y + r.size.height <= k.origin.y + k.size.height
        }),
    };

    // One hour at 60 fps of the same whole-window dirty rect: 216,000 frames.
    let window = rect(0.0, 0.0, 2560.0, 1440.0);
    let mut report = FrameReport::default();
    for _ in 0..216_000 {
        report.record_frame_at_generation(
            0,
            FrameDamage::Rects(vec![window]),
            FrameDamage::Rects(vec![window]),
        );
    }
    assert!(
        report.accumulated_paint_damage.rect_count() <= MAX_ACCUMULATED_DAMAGE_RECTS,
        "paint: {} rects after an hour (mutant: no compaction keeps all 216,000)",
        report.accumulated_paint_damage.rect_count()
    );
    assert!(report.accumulated_present_damage.rect_count() <= MAX_ACCUMULATED_DAMAGE_RECTS);
    // The same rect every frame is exactly one rect: the union is unchanged.
    assert!(covered_by(&report.accumulated_paint_damage, &window));

    // A rect that moves every frame (a cursor's damage): bounded too, and every frame's rect
    // is still inside what is reported (the bounding box is a superset).
    let mut report = FrameReport::default();
    let mut seen = Vec::new();
    for i in 0..20_000u32 {
        let r = rect((i % 2500) as f32, (i % 1400) as f32, 8.0, 8.0);
        seen.push(r);
        report.record_frame_at_generation(0, FrameDamage::Rects(vec![r]), FrameDamage::None);
        assert!(report.accumulated_paint_damage.rect_count() <= MAX_ACCUMULATED_DAMAGE_RECTS);
    }
    for r in &seen {
        assert!(covered_by(&report.accumulated_paint_damage, r), "{r:?} lost");
    }

    // Under the bound nothing changes: rects are concatenated as before (the E2E damage
    // assertions count them).
    let mut report = FrameReport::default();
    for _ in 0..5 {
        report.record_frame_at_generation(0, FrameDamage::Rects(vec![window]), FrameDamage::None);
    }
    assert_eq!(report.accumulated_paint_damage.rect_count(), 5);
}

/// A9-04: a rebuild that unmounts a VirtualView's host drops what the window keeps per child
/// DOM (its fingerprints and its layout cache), as it already dropped the child's GPU value
/// cache: a remounted view gets a new nested `DomId`, so the old entries were dead weight,
/// one set per unmount, for the life of the window.
#[cfg(feature = "text_layout")]
#[test]
fn a9_04_a_dropped_child_leaves_no_fingerprints_or_layout_cache() {
    use azul_core::dom::{DomId, NodeId};
    use rust_fontconfig::FcFontCache;

    use crate::{managers::NodeIdMap, window::LayoutWindow};

    let fingerprints = |n: usize| azul_core::diff::DomFingerprints {
        structure: vec![1; n],
        style: vec![2; n],
        structure_root: 1,
        style_root: 2,
    };
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let host = NodeId::new(3);
    let child = lw
        .virtual_view_manager
        .get_or_create_nested_dom_id(DomId::ROOT_ID, host);
    lw.child_dom_fingerprints.insert(child, fingerprints(12_500));
    lw.child_layout_caches.insert(child, Default::default());

    // A rebuild of the root that keeps the host keeps the child's state.
    lw.remap_node_ids(DomId::ROOT_ID, &NodeIdMap::from_pairs([(host, host)]));
    assert!(lw.child_dom_fingerprints.contains_key(&child));
    assert!(lw.child_layout_caches.contains_key(&child));

    // One that unmounts it drops it (mutant: without the two removes both stay).
    lw.remap_node_ids(DomId::ROOT_ID, &NodeIdMap::default());
    assert!(!lw.child_dom_fingerprints.contains_key(&child), "fingerprints of a dead child");
    assert!(!lw.child_layout_caches.contains_key(&child), "layout cache of a dead child");

    // A hundred mounts and unmounts of the view leave nothing behind.
    for _ in 0..100 {
        let child = lw
            .virtual_view_manager
            .get_or_create_nested_dom_id(DomId::ROOT_ID, host);
        lw.child_dom_fingerprints.insert(child, fingerprints(64));
        lw.child_layout_caches.insert(child, Default::default());
        lw.remap_node_ids(DomId::ROOT_ID, &NodeIdMap::default());
    }
    assert_eq!(lw.child_dom_fingerprints.len(), 0);
    assert_eq!(lw.child_layout_caches.len(), 0);
}
