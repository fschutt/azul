//! THE golden gate for DL patching (task 12 round 2): a PATCHED resize pass
//! must produce a display list ITEM-IDENTICAL to a full rebuild — same
//! items, same order, same geometry, same DOM attribution. Anything less
//! means the patcher invented or lost pixels. Two independent LayoutWindows
//! run the same cold-layout + hinted-resize sequence, one with patching
//! forced ON and one forced OFF; their second-pass display lists are
//! compared exactly.

use azul_core::{
    dom::{Dom, IdOrClass, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::set_dl_patching_enabled,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn test_dom() -> StyledDom {
    let dom = Dom::create_node(NodeType::Div)
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(
            Dom::create_node(NodeType::Div)
                .with_ids_and_classes(vec![IdOrClass::Class("page".into())].into())
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "the first paragraph of golden text",
                ))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "a second, slightly longer paragraph",
                )),
        )
        .with_child(
            Dom::create_node(NodeType::Div)
                .with_ids_and_classes(vec![IdOrClass::Class("bar".into())].into())
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "chrome that tracks the window",
                )),
        );
    let css_str = r#"
        * { margin: 0px; padding: 0px; }
        .root { width: 100%; height: 100%; background: #eee; }
        .page { width: 400px; height: 300px; background: #fff;
                border: 1px solid #888; box-shadow: 2px 2px 6px #0008; }
        .bar  { width: 100%; height: 24px; background: #ccd; }
    "#;
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    StyledDom::create(&mut dom, css)
}

/// Runs cold @640x480 then a hinted resize @900x700; returns the second
/// pass's display list.
fn run_sequence() -> std::sync::Arc<azul_layout::solver3::display_list::DisplayList> {
    let font_cache = FcFontCache::build();
    let mut lw = LayoutWindow::new(font_cache).unwrap();
    let rr = RendererResources::default();
    let cb = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;

    let styled = test_dom();
    let styled2 = styled.clone();

    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.layout_and_generate_display_list(styled, &ws, &rr, &cb, &mut dbg)
        .unwrap();

    lw.layout_cache.resize_only_hint = true;
    ws.size.dimensions = LogicalSize::new(900.0, 700.0);
    lw.layout_and_generate_display_list(styled2, &ws, &rr, &cb, &mut dbg)
        .unwrap();
    assert!(
        lw.layout_cache.last_reconcile_was_skipped,
        "harness: the second pass must take the resize-skip branch"
    );

    lw.layout_cache
        .cached_display_list
        .as_ref()
        .map(|(_, _, _, dl)| dl.clone())
        .expect("second pass must cache a display list")
}

#[test]
fn patched_resize_display_list_is_item_identical_to_a_full_rebuild() {
    set_dl_patching_enabled(true);
    let patched = run_sequence();

    set_dl_patching_enabled(false);
    let full = run_sequence();

    // Same DOM attribution invariants on both.
    assert_eq!(patched.items.len(), patched.node_mapping.len());
    assert_eq!(full.items.len(), full.node_mapping.len());

    assert_eq!(
        patched.items.len(),
        full.items.len(),
        "patched pass emitted a different ITEM COUNT than a full rebuild"
    );
    for (i, (a, b)) in patched.items.iter().zip(full.items.iter()).enumerate() {
        assert_eq!(
            format!("{a:?}"),
            format!("{b:?}"),
            "item {i} differs between the patched and full pass"
        );
    }
    assert_eq!(
        patched.node_mapping, full.node_mapping,
        "DOM attribution differs between the patched and full pass"
    );

    // Restore the default for any test that runs after us in-process.
    set_dl_patching_enabled(true);
}
