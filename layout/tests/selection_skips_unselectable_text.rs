//! A selection only ever covers text a selection may cover - the rule the
//! painter already followed.
//!
//! `paint_selections` skips a block whose text is `user-select: none`, but the
//! selection itself did not: a document selection over it listed it among its
//! blocks (Copy put a button label on the clipboard that was never
//! highlighted), a drag could end inside it, and Ctrl+A could start or end on
//! it. And a drag that starts inside an editing host - a text field - ran out
//! of the field into the page text around it.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextBlock, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .p { display: block; }
    .chrome { display: block; user-select: none; }
"#;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn para(class_name: &str, text: &str) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class(class_name))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            text,
        ))
}

fn layout(mut dom: Dom) -> LayoutWindow {
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

fn block_of(lw: &LayoutWindow, n: usize) -> TextBlock {
    lw.text_block_of(dnid(n))
        .unwrap_or_else(|| panic!("node {n} is in a text block"))
}

fn rect_of(lw: &LayoutWindow, n: usize) -> LogicalRect {
    lw.get_node_layout_rect(dnid(n))
        .unwrap_or_else(|| panic!("node {n} is laid out"))
}

fn inside(r: LogicalRect) -> LogicalPosition {
    LogicalPosition::new(r.origin.x + 2.0, r.origin.y + r.size.height * 0.5)
}

fn start() -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 0,
        },
        affinity: CursorAffinity::Leading,
    }
}

fn copied(lw: &LayoutWindow) -> Option<String> {
    lw.get_selected_content_for_clipboard(&DomId::ROOT_ID)
        .map(|c| c.plain_text.as_str().to_string())
}

/// `body(0) > [div.p(1) > "one"(2), div.chrome(3) > "label"(4),
/// div.p(5) > "two"(6)]`
fn with_chrome_between() -> LayoutWindow {
    layout(
        Dom::create_body()
            .with_child(para("p", "one"))
            .with_child(para("chrome", "label"))
            .with_child(para("p", "two")),
    )
}

#[test]
fn a_selection_over_unselectable_text_does_not_copy_it() {
    let mut lw = with_chrome_between();
    let (one, two) = (block_of(&lw, 1), block_of(&lw, 5));
    let end_of_two = lw
        .text_target(two)
        .and_then(|t| t.last_caret())
        .expect("premise: \"two\" has a last caret");
    assert!(lw.set_cross_block_selection(one, start(), two, end_of_two));

    assert_eq!(copied(&lw).as_deref(), Some("one\ntwo"));
}

#[test]
fn a_drag_does_not_end_in_unselectable_text() {
    let mut lw = with_chrome_between();
    let from = inside(rect_of(&lw, 1));
    lw.process_mouse_click_for_selection(from, 0)
        .expect("premise: the press lands on \"one\"");

    lw.process_mouse_drag_for_selection(from, inside(rect_of(&lw, 3)))
        .expect("the drag is handled");

    let focus = lw
        .text_edit_manager
        .get_cross_block_selection()
        .map(|sel| sel.focus.block);
    assert_ne!(
        focus,
        Some(block_of(&lw, 3)),
        "the drag's far end must not be in the unselectable label"
    );
}

#[test]
fn select_all_does_not_start_on_unselectable_text() {
    // `body(0) > div.host[contenteditable](1) > [div.chrome(2) > "label"(3),
    // div.p(4) > "text"(5)]`
    let mut lw = layout(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_contenteditable(true)
                .with_child(para("chrome", "label"))
                .with_child(para("p", "text")),
        ),
    );
    // Tab into the host: the focus path opens the session Ctrl+A acts on.
    let ws = lw.current_window_state.clone();
    let _ = lw.handle_focus_change_for_cursor_blink(Some(dnid(1)), &ws);
    lw.focus_manager.set_focused_node(Some(dnid(1)));
    assert!(
        lw.finalize_pending_focus_changes(),
        "premise: the focus opened a session"
    );

    assert!(lw.select_all_text(dnid(1)), "Ctrl+A selects the host");
    assert_eq!(copied(&lw).as_deref(), Some("text"));
}

#[test]
fn a_drag_that_starts_in_a_text_field_stays_in_it() {
    // `body(0) > [div.host[contenteditable](1) > div.p(2) > "field"(3),
    // div.p(4) > "page"(5)]`
    let mut lw = layout(
        Dom::create_body()
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("p"))
                    .with_contenteditable(true)
                    .with_child(para("p", "field")),
            )
            .with_child(para("p", "page")),
    );
    let from = inside(rect_of(&lw, 2));
    lw.process_mouse_click_for_selection(from, 0)
        .expect("premise: the press lands in the field");

    lw.process_mouse_drag_for_selection(from, inside(rect_of(&lw, 4)))
        .expect("the drag is handled");

    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_none(),
        "the selection must not run out of the field into the page, got {:?}",
        lw.text_edit_manager
            .get_cross_block_selection()
            .map(|sel| sel.focus.block)
    );
}
