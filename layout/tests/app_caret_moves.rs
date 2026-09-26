//! The app's caret moves (`CallbackInfo`'s `MoveCursorLeft` ...
//! `MoveCursorToDocumentEnd`) do what the keys they are named after do.
//!
//! They read the node's STORED inline layout, which under the default dense
//! text path is the empty retirement sentinel - no cluster to move over, so
//! every one of them was a no-op - and only the node's OWN layout, so one
//! naming a text field's host (whose text is in a paragraph inside it) found
//! none at all.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::{CallbackChange, ExternalSystemCallbacks},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .p { display: block; }
"#;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn para(s: &str) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class("p"))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(s))
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

/// `body(0) > div.p[contenteditable](1) > "hello"(2)`, caret at the start.
fn one_field() -> LayoutWindow {
    let mut lw = layout(Dom::create_body().with_child(para("hello").with_contenteditable(true)));
    open_session_in(&mut lw, 1);
    lw
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

fn open_session_in(lw: &mut LayoutWindow, n: usize) {
    assert!(
        lw.start_editing_at(start(), DomId::ROOT_ID, NodeId::new(n), 0),
        "premise: a session opens in node {n}'s block"
    );
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn node(n: usize) -> (DomId, NodeId) {
    (DomId::ROOT_ID, NodeId::new(n))
}

#[test]
fn an_apps_move_right_moves_the_caret() {
    let mut lw = one_field();
    let (dom_id, node_id) = node(1);

    assert!(lw.apply_app_cursor_move(&CallbackChange::MoveCursorRight {
        dom_id,
        node_id,
        extend_selection: false,
    }));

    assert_eq!(lw.focused_caret_byte_offset(), Some(1));
}

#[test]
fn an_apps_document_end_moves_the_caret_to_the_end() {
    let mut lw = one_field();
    let (dom_id, node_id) = node(1);

    assert!(
        lw.apply_app_cursor_move(&CallbackChange::MoveCursorToDocumentEnd {
            dom_id,
            node_id,
            extend_selection: false,
        })
    );

    assert_eq!(lw.focused_caret_byte_offset(), Some("hello".len()));
}

#[test]
fn an_apps_extending_move_selects() {
    let mut lw = one_field();
    let (dom_id, node_id) = node(1);

    assert!(lw.apply_app_cursor_move(&CallbackChange::MoveCursorToLineEnd {
        dom_id,
        node_id,
        extend_selection: true,
    }));

    assert_eq!(lw.focused_selection_byte_range(), Some((0, "hello".len())));
}

#[test]
fn an_apps_move_naming_a_fields_host_moves_the_caret_inside_it() {
    // `body(0) > div.p[contenteditable](1) > div.p(2) > "hello"(3)`
    let mut lw = layout(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_contenteditable(true)
                .with_child(para("hello")),
        ),
    );
    open_session_in(&mut lw, 2);
    let (dom_id, node_id) = node(1);

    assert!(
        lw.apply_app_cursor_move(&CallbackChange::MoveCursorToDocumentEnd {
            dom_id,
            node_id,
            extend_selection: false,
        })
    );

    assert_eq!(lw.focused_caret_byte_offset(), Some("hello".len()));
}

#[test]
fn an_apps_move_in_another_paragraph_leaves_the_caret_alone() {
    // `body(0) > [div.p(1) > "one"(2), div.p(3) > "two"(4)]`
    let mut lw = layout(
        Dom::create_body()
            .with_child(para("one"))
            .with_child(para("two")),
    );
    open_session_in(&mut lw, 1);
    let (dom_id, node_id) = node(3);

    let _ = lw.apply_app_cursor_move(&CallbackChange::MoveCursorRight {
        dom_id,
        node_id,
        extend_selection: false,
    });

    assert_eq!(
        lw.text_edit_manager.get_editing_block(),
        lw.text_block_of(dnid(1))
    );
    assert_eq!(
        lw.text_edit_manager.get_primary_cursor(),
        Some(start()),
        "there is no caret in \"two\" to move; the one in \"one\" stays"
    );
}
