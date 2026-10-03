//! A selection made from the KEYBOARD after a keyboard focus is painted.
//!
//! Tab (or a programmatic focus) into a text field opens the editing session
//! through `finalize_pending_focus_changes`, which seeds the caret at the end
//! of the host's last text node. Shift+Left or Ctrl+A then make a range in
//! that session - Copy finds it - but the display list got no highlight: the
//! range was handed to the painter under the node the session was opened on
//! (the text LEAF), and `paint_selections` looks the range up under the IFC
//! root it is painting (the `<p>` that owns the leaf's inline layout). The
//! same range made by a mouse click or drag was painted, because the click
//! path opens its session on the IFC root.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .host { display: block; }
    .p { display: block; }
"#;

/// `body(0) > div.host[contenteditable](1) > div.p(2) > text(3) "hello"` -
/// the shape of a `TextInput`: the host is focused, the value paragraph owns
/// the inline layout.
const HOST: usize = 1;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn text_field() -> LayoutWindow {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true)
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("p"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "hello",
                    )),
            ),
    );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// Tab into the field: the focus path, then the post-layout finalize that
/// seeds the caret.
fn tab_into_the_field(lw: &mut LayoutWindow) {
    let ws = lw.current_window_state.clone();
    let _ = lw.handle_focus_change_for_cursor_blink(Some(dnid(HOST)), &ws);
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    assert!(
        lw.finalize_pending_focus_changes(),
        "premise: the focus opened an editing session"
    );
    assert!(
        lw.text_edit_manager.get_primary_cursor().is_some(),
        "premise: the session has a caret"
    );
}

fn selection_rects(lw: &LayoutWindow) -> usize {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .display_list
        .items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::SelectionRect { .. }))
        .count()
}

fn copied_text(lw: &LayoutWindow) -> Option<String> {
    lw.get_selected_content_for_clipboard(&DomId::ROOT_ID)
        .map(|c| c.plain_text.as_str().to_string())
}

#[test]
fn shift_left_after_tab_paints_the_selected_character() {
    let mut lw = text_field();
    tab_into_the_field(&mut lw);

    let shift_left = SelectionOp::new(
        SelectionDirection::Backward,
        SelectionStep::Character,
        SelectionMode::Extend,
    );
    assert!(lw.apply_selection_op(dnid(HOST), &shift_left));
    assert_eq!(
        copied_text(&lw).as_deref(),
        Some("o"),
        "premise: Shift+Left selected the last character"
    );

    assert!(
        selection_rects(&lw) > 0,
        "the selected character must be highlighted"
    );
}

#[test]
fn select_all_after_tab_paints_the_whole_text() {
    let mut lw = text_field();
    tab_into_the_field(&mut lw);

    assert!(lw.select_all_text(dnid(HOST)), "Ctrl+A selects the field");
    assert_eq!(
        copied_text(&lw).as_deref(),
        Some("hello"),
        "premise: Ctrl+A selected the field's text"
    );

    assert!(
        selection_rects(&lw) > 0,
        "the selected text must be highlighted"
    );
}
