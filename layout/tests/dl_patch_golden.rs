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
    test_dom_with("the first paragraph of golden text")
}

fn test_dom_with(first_paragraph: &str) -> StyledDom {
    let dom = Dom::create_node(NodeType::Div)
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(
            Dom::create_node(NodeType::Div)
                .with_ids_and_classes(vec![IdOrClass::Class("page".into())].into())
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    first_paragraph,
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
        .map(|(_, _, _, _, _, dl)| dl.clone())
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

/// Cold pass in `first`, then the app's `layout()` runs again: a NEW
/// StyledDom of the same structure (one paragraph edited), laid out with the
/// window theme set to `second`. Returns the second pass's display list.
fn run_theme_flip(
    first: azul_core::window::WindowTheme,
    second: azul_core::window::WindowTheme,
) -> std::sync::Arc<azul_layout::solver3::display_list::DisplayList> {
    let font_cache = FcFontCache::build();
    let mut lw = LayoutWindow::new(font_cache).unwrap();
    let rr = RendererResources::default();
    let cb = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;

    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    ws.theme = first;
    lw.layout_and_generate_display_list(test_dom(), &ws, &rr, &cb, &mut dbg)
        .unwrap();

    // The new generation also edits one paragraph, as AzWriter's rebuild
    // re-flows text. An otherwise identical DOM takes the clean-reconcile
    // exit, which re-emits wholesale and never reaches the patch gate.
    ws.theme = second;
    lw.layout_and_generate_display_list(
        test_dom_with("the first paragraph of golden text, edited"),
        &ws,
        &rr,
        &cb,
        &mut dbg,
    )
    .unwrap();
    assert!(
        lw.layout_cache.last_reconcile_structure_preserved,
        "harness: the second pass must be a structure-preserving reconcile"
    );

    lw.layout_cache
        .cached_display_list
        .as_ref()
        .map(|(_, _, _, _, _, dl)| dl.clone())
        .expect("second pass must cache a display list")
}

fn item_strings(dl: &azul_layout::solver3::display_list::DisplayList) -> Vec<String> {
    dl.items.iter().map(|i| format!("{i:?}")).collect()
}

/// A theme flip re-runs `layout()`, so the second pass sees a DIFFERENT
/// StyledDom whose cascade epoch can equal the first one's: both started at
/// 0 and were bumped equally often. The patch gate compared only that
/// epoch, so it spliced the light list into the dark tree. In AzWriter this
/// showed as page sheets painted over the ribbon, with 23 chrome items
/// missing, after a live dark->light switch. The list after the flip must be
/// the full build's, item for item.
#[test]
fn a_theme_flip_rebuild_is_item_identical_to_a_full_rebuild() {
    use azul_core::window::WindowTheme::{DarkMode, LightMode};

    // The fixture must express the difference, or this gate proves nothing.
    set_dl_patching_enabled(false);
    let full_light = run_theme_flip(LightMode, LightMode);
    let full_dark = run_theme_flip(LightMode, DarkMode);
    assert_ne!(
        item_strings(&full_light),
        item_strings(&full_dark),
        "harness: the theme must change what this DOM paints"
    );

    set_dl_patching_enabled(true);
    let patched_dark = run_theme_flip(LightMode, DarkMode);
    set_dl_patching_enabled(true);

    assert_eq!(
        item_strings(&patched_dark),
        item_strings(&full_dark),
        "the display list after a theme flip differs from a full rebuild"
    );
    assert_eq!(patched_dark.node_mapping, full_dark.node_mapping);
}

/// A nested DOM (AzWriter's pages are a VirtualView child) re-generated on
/// its own, e.g. by a caret blink or a keystroke in the page.
fn child_dom() -> StyledDom {
    let mut dom = Dom::create_node(NodeType::Div)
        .with_ids_and_classes(vec![IdOrClass::Class("sheet".into())].into())
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "page text in a nested dom",
        ))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "a second line on the page",
        ));
    let (css, _) = azul_css::parser2::new_from_str(
        r#"* { margin: 0px; padding: 0px; }
           .sheet { width: 300px; height: 200px; background: #e2e2e2;
                    box-shadow: 0px 1px 4px #0006; border: 1px solid #31363b; }"#,
    );
    let mut sd = StyledDom::create(&mut dom, css);
    sd.dom_id = azul_core::dom::DomId { inner: 1 };
    sd
}

/// Root pass, a nested DOM laid out and then RE-GENERATED on its own, then
/// a structure-preserving root pass (one paragraph edited). Returns the root
/// display list of that last pass and whether the root's DL cache slot still
/// held a ROOT list right after the nested regeneration.
fn run_after_child_regeneration() -> (
    std::sync::Arc<azul_layout::solver3::display_list::DisplayList>,
    bool,
) {
    let font_cache = FcFontCache::build();
    let mut lw = LayoutWindow::new(font_cache).unwrap();
    let rr = RendererResources::default();
    let cb = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);

    lw.layout_and_generate_display_list(test_dom(), &ws, &rr, &cb, &mut dbg)
        .unwrap();
    // The way a VirtualView lays out its nested DOM during the root pass:
    // the public funnel would start a new generation and clear the root.
    lw.layout_dom_recursive_with_viewport(
        child_dom(),
        &ws,
        &rr,
        &cb,
        &mut dbg,
        Some(azul_core::geom::LogicalRect::new(
            azul_core::geom::LogicalPosition::new(100.0, 60.0),
            LogicalSize::new(320.0, 240.0),
        )),
    )
    .unwrap();
    let child = azul_core::dom::DomId { inner: 1 };
    assert!(
        lw.layout_results.contains_key(&child),
        "harness: the nested DOM must have a layout result"
    );
    lw.regenerate_display_list_for_dom(child);

    let root_items = item_strings(
        &lw.layout_results[&azul_core::dom::DomId::ROOT_ID].display_list,
    );
    let slot_is_root = lw
        .layout_cache
        .cached_display_list
        .as_ref()
        .is_some_and(|(_, _, _, _, _, dl)| item_strings(dl) == root_items);

    lw.layout_and_generate_display_list(
        test_dom_with("the first paragraph of golden text, edited"),
        &ws,
        &rr,
        &cb,
        &mut dbg,
    )
    .unwrap();
    let dl = lw.layout_results[&azul_core::dom::DomId::ROOT_ID]
        .display_list
        .clone();
    (dl, slot_is_root)
}

/// The root's DL cache slot is the patch arm's source. Regenerating a NESTED
/// DOM wrote that DOM's list into it: `regenerate_display_list_for_dom`
/// refreshed `self.layout_cache`, and outside a child layout pass that is the
/// ROOT's cache. The next structure-preserving root pass then spliced page
/// items into the window's list. AzWriter showed that as the page sheet
/// painted over the ribbon and the chrome fills missing, for a frame after a
/// keystroke (a caret regenerate plus a line-growth relayout) and for good
/// after a theme switch.
#[test]
fn regenerating_a_nested_dom_never_feeds_the_roots_patch() {
    set_dl_patching_enabled(false);
    let (full, _) = run_after_child_regeneration();

    set_dl_patching_enabled(true);
    let (patched, slot_is_root) = run_after_child_regeneration();

    assert!(
        slot_is_root,
        "regenerating DOM 1 replaced the ROOT's cached display list with DOM 1's"
    );
    assert_eq!(
        item_strings(&patched),
        item_strings(&full),
        "the root list after a nested regeneration differs from a full rebuild"
    );
}
