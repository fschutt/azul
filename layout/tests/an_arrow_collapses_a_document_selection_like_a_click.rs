//! A plain arrow over a selection that spans paragraphs collapses it onto one
//! of its ends - and the caret there is opened the way every other caret is
//! (`LayoutWindow::open_session`): keyed on its editing host, in the focus
//! scope it now sits in.
//!
//! `collapse_document_selection_for_move` called
//! `TextEditManager::initialize_editing` directly, so the caret tween kept the
//! focus scope of wherever the session had been before - a text field, say -
//! and the caret glided out of that field into the paragraph instead of
//! jumping, as a caret crossing into another scope does everywhere else.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId, TabIndex},
    events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep},
    geom::LogicalSize,
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
"#;

/// `body(0) > [div.p[contenteditable, tabindex](1) > "field"(2),
/// div.p(3) > "one"(4), div.p(5) > "two"(6)]`
const FIELD: usize = 1;
const P1: usize = 3;
const P2: usize = 5;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn para(s: &str) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class("p"))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(s))
}

fn page() -> LayoutWindow {
    let mut dom = Dom::create_body()
        .with_child(
            para("field")
                .with_contenteditable(true)
                .with_tab_index(TabIndex::Auto),
        )
        .with_child(para("one"))
        .with_child(para("two"));
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

fn at(byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

#[test]
fn an_arrow_over_a_document_selection_opens_the_caret_in_its_own_focus_scope() {
    let mut lw = page();
    // A caret in the field first: the tween's scope is the field.
    assert!(lw.add_app_cursor(dnid(FIELD), at(1)));
    let field_scope = lw.text_edit_manager.tween.focus_scope;
    assert_eq!(field_scope, Some(dnid(FIELD)), "premise: the field is a focus scope");

    // A selection across the two paragraphs outside it.
    let (one, two) = (block_of(&lw, P1), block_of(&lw, P2));
    assert!(lw.set_cross_block_selection(one, at(1), two, at(2)));

    // Right: the selection collapses onto its end, in "two".
    assert!(lw.apply_selection_op(
        dnid(P1),
        &SelectionOp::new(
            SelectionDirection::Forward,
            SelectionStep::Character,
            SelectionMode::Move,
        ),
    ));
    assert_eq!(lw.text_edit_manager.get_editing_block(), Some(two));

    let paragraph_scope = lw.find_focusable_ancestor(two.container_dom_node());
    assert_ne!(paragraph_scope, field_scope, "premise: the paragraph is outside the field");
    assert_eq!(
        lw.text_edit_manager.tween.focus_scope, paragraph_scope,
        "the caret now sits in the paragraph's scope, not the field's"
    );
}
