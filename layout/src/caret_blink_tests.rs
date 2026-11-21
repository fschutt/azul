//! The caret blink is a patch of the display list, not a rebuild of it.
//!
//! The blink flipped the caret's alpha by regenerating the whole window's display
//! list every ~530 ms (`regenerate_display_list_for_dom`: every item of the DOM, a
//! fresh `Arc`) for a frame that changes one rect: a focused text field in an
//! idle window kept the editor rebuilding a 38 000-item list twice a second. The
//! caret items of a list are recorded when it is built
//! (`DisplayList::carets`), and the blink shows or hides them in place
//! (`LayoutWindow::apply_caret_visibility_in_place`).

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};

/// `body(0) > p(1) > ["Name: "(2), span[contenteditable](3) > "Ada"(4)]`
const SPAN: usize = 3;

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// A window whose span is focused and holds the caret.
fn window_with_a_caret() -> LayoutWindow {
    let mut span = Dom::create_span()
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Ada"));
    span.set_contenteditable(true);
    let mut dom = Dom::create_body().with_child(
        Dom::create_p()
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Name: "))
            .with_child(span),
    );
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { font-size: 14px; width: 400px; }",
    );
    let styled = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .unwrap();
    let _ = lw.handle_focus_change_for_cursor_blink(Some(dnid(SPAN)), &ws);
    lw.focus_manager.set_focused_node(Some(dnid(SPAN)));
    assert!(lw.finalize_pending_focus_changes(), "premise: an editing session opened");
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    lw
}

fn caret_alpha(lw: &LayoutWindow) -> Option<u8> {
    lw.get_layout_result(&DomId::ROOT_ID)?
        .display_list
        .items
        .iter()
        .rev()
        .find_map(|item| match item {
            DisplayListItem::CursorRect { color, .. } => Some(color.a),
            _ => None,
        })
}

#[test]
fn the_list_records_its_caret() {
    let lw = window_with_a_caret();
    let dl = &lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list;
    assert_eq!(dl.carets.len(), 1, "one caret, recorded when the list was built");
    let (index, shown) = dl.carets[0];
    assert!(matches!(dl.items[index], DisplayListItem::CursorRect { .. }));
    assert!(shown.a > 0, "the colour it has while it shows");
}

#[test]
fn the_blink_shows_and_hides_the_caret_with_no_display_list_rebuild() {
    let mut lw = window_with_a_caret();
    assert!(caret_alpha(&lw).is_some_and(|a| a > 0), "premise: the caret shows");
    lw.frame_report.reset_counters();
    let before = Arc::clone(&lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list);

    // The off phase: what `ToggleCursorVisibility` does to the blink state, and
    // the one call it makes after.
    lw.text_edit_manager.blink.set_visibility(false);
    assert_eq!(
        lw.apply_caret_visibility_in_place(DomId::ROOT_ID),
        Some(true),
        "the list is patched"
    );
    assert_eq!(caret_alpha(&lw), Some(0), "the caret is hidden");
    assert_eq!(lw.frame_report.dl_rebuilds, 0, "and nothing was rebuilt");
    let patched = Arc::clone(&lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list);
    assert!(
        !Arc::ptr_eq(&before, &patched),
        "a patched list is a new one: the renderer is owed it"
    );
    assert_eq!(
        patched.items.len(),
        before.items.len(),
        "the item count is the same in both phases: the damage diff is a caret-sized rect"
    );

    // The on phase.
    lw.text_edit_manager.blink.set_visibility(true);
    assert_eq!(lw.apply_caret_visibility_in_place(DomId::ROOT_ID), Some(true));
    assert!(caret_alpha(&lw).is_some_and(|a| a > 0), "the caret shows again");
    assert_eq!(lw.frame_report.dl_rebuilds, 0);

    // The phase it already shows: nothing to patch, nothing new.
    let same = Arc::clone(&lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list);
    assert_eq!(lw.apply_caret_visibility_in_place(DomId::ROOT_ID), Some(false));
    assert!(Arc::ptr_eq(
        &same,
        &lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list
    ));
}

/// A list that has no caret it knows of cannot be patched: the caller rebuilds.
#[test]
fn a_list_without_a_recorded_caret_is_left_to_a_rebuild() {
    let mut lw = window_with_a_caret();
    if let Some(r) = lw.layout_results
        .get_mut(&DomId::ROOT_ID) { Arc::make_mut(&mut r.display_list).carets.clear() }
    assert_eq!(lw.apply_caret_visibility_in_place(DomId::ROOT_ID), None);

    // A caret item the table does not know (spliced in from elsewhere): the same.
    let mut lw = window_with_a_caret();
    if let Some(r) = lw.layout_results.get_mut(&DomId::ROOT_ID) {
        let dl = Arc::make_mut(&mut r.display_list);
        let at = dl.items.len();
        dl.insert_item(
            at,
            DisplayListItem::CursorRect {
                bounds: azul_core::geom::LogicalRect::zero().into(),
                color: azul_css::props::basic::ColorU::WHITE,
            },
            None,
        );
    }
    lw.text_edit_manager.blink.set_visibility(false);
    assert_eq!(lw.apply_caret_visibility_in_place(DomId::ROOT_ID), None);
}

/// A rebuild after a patch paints the phase the blink is in, and records the
/// caret again.
#[test]
fn a_list_built_after_the_blink_hid_the_caret_hides_it_too() {
    let mut lw = window_with_a_caret();
    lw.text_edit_manager.blink.set_visibility(false);
    let _ = lw.apply_caret_visibility_in_place(DomId::ROOT_ID);
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    assert_eq!(caret_alpha(&lw), Some(0));
    assert_eq!(lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list.carets.len(), 1);
}
