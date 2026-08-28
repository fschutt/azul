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
use azul_layout::solver3::LayoutNodeId;
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
                    Dom::create_div()
                        .with_ids_and_classes(vec![IdOrClass::Class("grandchild".into())].into()),
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
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "some shaped text that must not be re-shaped",
        ))
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("child".into())].into())
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "a second paragraph of shaped text",
                )),
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
    assert!(
        cold_fresh > 0,
        "cold layout must create nodes (got {cold_fresh})"
    );
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

    // The ANON-WRAPPER half of the same contract. Wrappers have no dom id, so
    // they are invisible to the fresh/reused census above — but their damage
    // IS visible here: an unmatched wrapper flips its parent to
    // children_are_different, which lands the parent in intrinsic_dirty. This
    // DOM has two text paragraphs (=> two inline runs => wrappers), and
    // before ordinal-matching (try_reuse_anon_wrapper) every resize
    // re-dirtied their parents: last_intrinsic_dirty was non-zero on every
    // same-DOM resize. Zero means the wrappers matched and nothing was
    // re-measured.
    assert_eq!(
        layout_window.layout_cache.last_intrinsic_dirty, 0,
        "a same-DOM resize must not re-measure any intrinsics — a non-zero \
         count means anonymous wrappers failed to ordinal-match their old \
         selves (children_are_different flipped unconditionally again)"
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
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "vw sized text",
        ));

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
    assert_eq!(
        flag,
        Some(true),
        "5vw font-size must set uses_viewport_units"
    );
}

#[test]
fn px_only_document_does_not_set_uses_viewport_units() {
    let mut dom = Dom::create_div().with_child(
        Dom::create_text_do_not_use_without_block_level_wrapper("plain"),
    );
    let (css, _) =
        azul_css::parser2::new_from_str("* { margin: 0px; } div { width: 100%; font-size: 20px; }");
    let styled_dom = StyledDom::create(&mut dom, css);
    let flag = styled_dom
        .css_property_cache
        .ptr
        .compact_cache
        .as_ref()
        .map(|cc| cc.uses_viewport_units);
    assert_eq!(flag, Some(false), "px-only document must not set the flag");
}

/// The scrollbar-reflow counterpart to the reuse census: when a resize makes
/// a scrollbar appear or disappear, the layout loop re-runs — and it used to
/// mark EVERY node intrinsic-dirty on the way (`(0..len).collect()`),
/// re-measuring the whole document's min/max-content widths although a
/// scrollbar changes AVAILABLE SPACE, not content. 75 ms of the measured
/// 166 ms first-resize outlier on big.md, invisible to every pixel test
/// (recomputed intrinsics equal reused ones). `last_intrinsic_dirty` is the
/// observable; the negative control re-adds the blanket collect and watches
/// this go red.
#[test]
fn scrollbar_toggle_does_not_remeasure_all_intrinsics() {
    // Content tall enough to overflow 300px viewport height (scrollbar ON)
    // but not 800px (scrollbar OFF) — the resize crosses the toggle.
    let mut children = Vec::new();
    for i in 0..24 {
        children.push(Dom::create_text_do_not_use_without_block_level_wrapper(
            format!("line of overflow text number {i}"),
        ));
    }
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("scroller".into())].into())
        .with_children(children.into());

    let css_str = r#"
        * { margin: 0px; padding: 0px; }
        .scroller { width: 100%; height: 100%; overflow-y: auto; font-size: 16px; }
    "#;
    let (css, _) = azul_css::parser2::new_from_str(css_str);

    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);
    let styled_dom_first = styled_dom.clone();
    let node_count = styled_dom.node_data.as_ref().len();

    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0); // overflows -> scrollbar
    layout_window
        .layout_and_generate_display_list(
            styled_dom_first,
            &ws,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    // Resize across the scrollbar toggle. Same DOM => reconcile reuses all;
    // the reflow loop may run, but intrinsics must stay untouched.
    ws.size.dimensions = LogicalSize::new(500.0, 800.0); // fits -> scrollbar off
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &ws,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    let dirty = layout_window.layout_cache.last_intrinsic_dirty;
    let reused = layout_window.layout_cache.last_reconcile_reused;
    println!("scrollbar toggle: intrinsic_dirty={dirty} of {node_count} nodes (reused={reused})");
    assert_eq!(
        layout_window.layout_cache.last_reconcile_fresh, 0,
        "same-DOM resize must reuse the tree"
    );
    assert!(
        dirty < node_count / 2,
        "scrollbar toggle re-measured {dirty} of {node_count} intrinsics — the \
         blanket `(0..len).collect()` is back"
    );
}

/// The resize-only reconcile SKIP (the hinted fast path). When the dll's
/// resize latch fires, the StyledDom object is by construction unchanged —
/// walking 1209 nodes to rediscover "everything reused" was ~9.6 ms per drag
/// frame, plus an identity remap of every per-node cache entry. With the
/// hint set, solver3 must take the retained tree AS-IS (census says
/// skipped), still re-run the top-down pass at the new size (root width
/// must track the viewport), and consume the hint (one-shot: the next
/// un-hinted pass reconciles normally).
#[test]
fn resize_only_hint_skips_reconcile_but_still_resizes() {
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "skip-path paragraph one",
        ))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "skip-path paragraph two",
        ));

    let css_str = r#"
        * { margin: 0px; padding: 0px; }
        .root { width: 100%; height: 100%; }
    "#;
    let (css, _) = azul_css::parser2::new_from_str(css_str);

    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);
    let styled_dom_first = styled_dom.clone();
    let styled_dom_third = styled_dom.clone();

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
    assert!(
        !layout_window.layout_cache.last_reconcile_was_skipped,
        "cold pass must reconcile"
    );

    // Hinted resize: reconcile skipped, sizes still track the viewport.
    layout_window.layout_cache.resize_only_hint = true;
    ws.size.dimensions = LogicalSize::new(900.0, 700.0);
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &ws,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();
    assert!(
        layout_window.layout_cache.last_reconcile_was_skipped,
        "the hinted resize must take the reconcile-skip branch"
    );
    let root_width = {
        let lr = layout_window
            .layout_results
            .get(&DomId::ROOT_ID)
            .expect("layout result");
        let tree = &lr.layout_tree;
        tree.get(LayoutNodeId::new(tree.root))
            .and_then(|n| n.used_size)
            .map(|s| s.width)
            .expect("root used_size")
    };
    assert!(
        (root_width - 900.0).abs() < 1.0,
        "skip must NOT skip the layout itself: root width {root_width} != 900 — \
         the top-down pass did not run at the new viewport"
    );

    // The hint is one-shot: an un-hinted follow-up reconciles normally.
    ws.size.dimensions = LogicalSize::new(910.0, 700.0);
    layout_window
        .layout_and_generate_display_list(
            styled_dom_third,
            &ws,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();
    assert!(
        !layout_window.layout_cache.last_reconcile_was_skipped,
        "the hint must be consumed by exactly one pass"
    );
}

/// GRANULAR DIFF channel (task #15b): when the produce side proves nodes
/// unchanged (pre-cascade fingerprints, self+ancestors, both tiers),
/// reconcile must REUSE their old fingerprints instead of re-hashing —
/// "the structural diff is used in later stages and not thrown away"
/// (user directive). The dll computes the clean vector; this test injects
/// it directly (same-DOM passes make every node provably clean) and reads
/// the census. Without the hint the census must be zero — that asymmetry
/// is the test's own negative control.
#[test]
fn dom_diff_clean_hint_skips_fingerprint_recompute() {
    let dom = || {
        Dom::create_div()
            .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "granular diff paragraph one",
            ))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "granular diff paragraph two",
            ))
    };
    let css_str = r#"* { margin: 0px; } .root { width: 100%; }"#;

    let build = |d: Dom| {
        let (css, _) = azul_css::parser2::new_from_str(css_str);
        let mut d = d;
        StyledDom::create(&mut d, css)
    };

    let font_cache = FcFontCache::build();
    let mut lw = LayoutWindow::new(font_cache).unwrap();
    let rr = RendererResources::default();
    let cb = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);

    lw.layout_and_generate_display_list(build(dom()), &ws, &rr, &cb, &mut dbg)
        .unwrap();

    // Pass 2 WITHOUT the hint: full re-fingerprinting, census zero.
    lw.layout_and_generate_display_list(build(dom()), &ws, &rr, &cb, &mut dbg)
        .unwrap();
    assert_eq!(
        lw.layout_cache.last_fingerprint_skips, 0,
        "no hint => no skips (the negative control)"
    );

    // Pass 3 WITH the hint: every DOM-backed node's fingerprint is reused.
    let n = lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .unwrap()
        .styled_dom
        .node_data
        .as_ref()
        .len();
    lw.layout_cache.dom_diff_clean = Some(vec![true; n]);
    lw.layout_and_generate_display_list(build(dom()), &ws, &rr, &cb, &mut dbg)
        .unwrap();
    let skips = lw.layout_cache.last_fingerprint_skips;
    assert!(
        skips >= n.saturating_sub(1),
        "with an all-clean hint on an identical DOM, (nearly) every node \
         must skip fingerprint recompute — got {skips} of {n}"
    );
}
