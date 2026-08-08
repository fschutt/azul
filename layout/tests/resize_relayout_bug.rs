//! Regression test for #9 "grey on resize" (azul-maps).
//!
//! The map widget's tile container is an absolutely-positioned node with
//! `top/left/right/bottom: 0` inside a `flex-grow:1; position:relative` parent
//! (see `layout/src/widgets/map.rs` + `examples/azul-maps`). On a window resize
//! / maximize the incremental layout cache reused the cached subtree, so the
//! out-of-flow container kept its OLD viewport size — tiles rendered only in the
//! original rect and the newly-exposed area was grey.
//!
//! The fix (layout/src/solver3/cache.rs `reconcile_and_invalidate`) drops the
//! cached layout tree whenever the viewport SIZE changes, forcing a fresh layout
//! against the new viewport. This test lays the same DOM out at 640x480, then
//! again at 1920x1080 reusing the same `LayoutWindow` (== same cache), and
//! asserts the absolutely-positioned grandchild grows to fill the new viewport.
//! Without the fix the second layout leaves it at 480 tall (the bug).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

#[test]
fn absolute_inset_child_grows_on_viewport_resize() {
    // root (fills viewport, flex column) > child (flex-grow:1, relative) >
    // grandchild (absolute, inset:0) — mirrors the MapWidget VirtualView nesting.
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("child".into())].into())
                .with_child(
                    Dom::create_div().with_ids_and_classes(
                        vec![IdOrClass::Class("grandchild".into())].into(),
                    ),
                ),
        );

    let css_str = r#"
        * { margin: 0px; padding: 0px; }
        .root { width: 100%; height: 100%; display: flex; flex-direction: column; }
        .child { flex-grow: 1; position: relative; }
        .grandchild { position: absolute; top: 0px; left: 0px; right: 0px; bottom: 0px; }
    "#;
    let (css, _) = azul_css::parser2::new_from_str(css_str);

    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);
    // layout_and_generate_display_list consumes the StyledDom; we lay out twice,
    // so keep a clone for the first (smaller) pass.
    let styled_dom_small = styled_dom.clone();

    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    let root_id = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    };

    // --- Pass 1: 640x480 ---
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    layout_window
        .layout_and_generate_display_list(
            styled_dom_small,
            &ws,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    let child_id = layout_window.get_first_child(root_id).expect("child");
    let gc_id = layout_window.get_first_child(child_id).expect("grandchild");
    let gc_small = layout_window
        .get_node_layout_rect(gc_id)
        .expect("grandchild rect @640x480");
    println!("grandchild @640x480 = {gc_small:?}");

    // --- Pass 2: 1920x1080, SAME layout_window => same cache (the resize path) ---
    ws.size.dimensions = LogicalSize::new(1920.0, 1080.0);
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &ws,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    let gc_large = layout_window
        .get_node_layout_rect(gc_id)
        .expect("grandchild rect @1920x1080");
    println!("grandchild @1920x1080 = {gc_large:?}");

    // Sanity: the absolute inset:0 child fills the viewport on the first pass.
    assert!(
        (gc_small.size.height - 480.0).abs() < 4.0,
        "expected ~480 tall at 640x480, got {}",
        gc_small.size.height
    );
    assert!(
        (gc_small.size.width - 640.0).abs() < 4.0,
        "expected ~640 wide at 640x480, got {}",
        gc_small.size.width
    );

    // The actual #9 regression: after the resize the out-of-flow container must
    // grow to the new viewport, not stay stuck at the old 640x480.
    assert!(
        gc_large.size.height > 1000.0,
        "#9 regression: absolute child did not grow on resize — height {} \
         (expected ~1080; stuck near 480 means the cached tree was reused)",
        gc_large.size.height
    );
    assert!(
        gc_large.size.width > 1800.0,
        "#9 regression: absolute child did not grow on resize — width {} \
         (expected ~1920)",
        gc_large.size.width
    );
}

/// The COUNTERPART invariant to `absolute_inset_child_grows_on_viewport_resize`:
/// correctness on resize must not be bought by rebuilding the tree from
/// scratch. A same-DOM viewport resize must RECONCILE every node against its
/// previous self (cloning warm shaped-text + intrinsic caches forward), with
/// ZERO fresh nodes.
///
/// This is the regression test for the `old_tree = None`-on-resize sledgehammer
/// in `reconcile_and_invalidate`: with it, every resize re-shaped 917
/// paragraphs and re-measured 1112 intrinsic widths on big.md (~130 of 246 ms)
/// while EVERY pixel-level test still passed — rebuilt-from-scratch and reused
/// produce identical output, just slower. The reuse census on `LayoutCache`
/// (`last_reconcile_reused` / `last_reconcile_fresh`) exists precisely so this
/// difference is assertable. The two tests in this file TOGETHER pin the
/// resize contract: #9 says viewport-dependent sizes must update; this one
/// says everything else must be reused.
#[test]
fn viewport_resize_reuses_every_reconciled_node() {
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(Dom::create_text("some shaped text that must not be re-shaped"))
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("child".into())].into())
                .with_child(Dom::create_text("a second paragraph of shaped text")),
        );

    let css_str = r#"
        * { margin: 0px; padding: 0px; }
        .root { width: 100%; height: 100%; }
        .child { width: 50%; }
    "#;
    let (css, _) = azul_css::parser2::new_from_str(css_str);

    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);
    let styled_dom_first = styled_dom.clone();

    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    layout_window
        .layout_and_generate_display_list(
            styled_dom_first,
            &ws,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    // Cold pass: everything is fresh by definition.
    let cold_fresh = layout_window.layout_cache.last_reconcile_fresh;
    assert!(cold_fresh > 0, "cold layout must create nodes (got {cold_fresh})");
    assert_eq!(
        layout_window.layout_cache.last_reconcile_reused, 0,
        "cold layout has nothing to reuse"
    );

    // Resize pass: SAME DOM content, new viewport.
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &ws,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    let reused = layout_window.layout_cache.last_reconcile_reused;
    let fresh = layout_window.layout_cache.last_reconcile_fresh;
    println!("resize reconcile: reused={reused} fresh={fresh} (cold created {cold_fresh})");

    assert_eq!(
        fresh, 0,
        "a same-DOM viewport resize built {fresh} nodes FRESH — warm shaped-text \
         and intrinsic caches were thrown away (the old_tree=None-on-resize bug)"
    );
    assert_eq!(
        reused, cold_fresh,
        "every node the cold pass created must be reconciled-and-reused on resize"
    );
}

/// The viewport-units side of the collect-cache contract — and the discovery
/// it forced. `uses_viewport_units` lets solver3 skip per-resize invalidation
/// of every inline collection for documents that never mention vw/vh; this
/// test pins the DETECTOR (author-CSS `5vw` must set the flag; the negative
/// control breaks the detector and watches this go red).
///
/// DISCOVERED WHILE WRITING THE STRICT VERSION: viewport units do not
/// actually RESOLVE anywhere in the pipeline — `font-size: 5vw` lays out as a
/// ~5px font at every window size (measured: line height 6.81px at BOTH 400w
/// and 1200w; 5vw should be 20px vs 60px). The compact encoder and the slow
/// path both treat the number as raw pixels. So the viewport fold this flag
/// gates was ALWAYS protecting a non-functional feature, at the cost of
/// re-collecting every IFC on every resize for everyone.
///
/// WHEN vw RESOLUTION IS IMPLEMENTED, the same commit MUST extend this test:
/// lay out at 400w and 1200w and assert the auto-height container grows ~3x —
/// at that point the assertion also becomes the run-and-see-red control for
/// the fc-gate consumer (a wrongly-skipped invalidation freezes the height).
#[test]
fn vw_font_size_sets_the_viewport_units_flag() {
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(Dom::create_text("vw sized text"));

    let css_str = r#"
        * { margin: 0px; padding: 0px; }
        .root { width: 100%; font-size: 5vw; }
    "#;
    let (css, _) = azul_css::parser2::new_from_str(css_str);

    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);

    let flag = styled_dom
        .css_property_cache
        .ptr
        .compact_cache
        .as_ref()
        .map(|cc| cc.uses_viewport_units);
    assert_eq!(flag, Some(true), "5vw font-size must set uses_viewport_units");
}

#[test]
fn px_only_document_does_not_set_uses_viewport_units() {
    let mut dom = Dom::create_div().with_child(Dom::create_text("plain"));
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0px; } div { width: 100%; font-size: 20px; }",
    );
    let styled_dom = StyledDom::create(&mut dom, css);
    let flag = styled_dom
        .css_property_cache
        .ptr
        .compact_cache
        .as_ref()
        .map(|cc| cc.uses_viewport_units);
    assert_eq!(flag, Some(false), "px-only document must not set the flag");
}
