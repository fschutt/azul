//! A caret in a list item moves with text the APP's new generation inserts
//! before it, as a caret in a paragraph does (U3-b).
//!
//! The generation diff compares the session block's text with the text the
//! engine last saw and shifts the carets by the change in each run. It read
//! the edit model (`get_text_before_textinput`), where a list item's text is
//! run 0; every caret in the item names run 1, behind the `::marker`. The
//! change was applied to run 0 - no caret there - and the caret stayed at
//! its old byte, now in the middle of other text.

use azul_core::{
    dom::{Dom, DomId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .host { display: block; }
    .li { display: list-item; }
"#;

/// `body(0) > div.host[contenteditable](1) > div.li(2) > text(3)`
const ITEM: usize = 2;
const TEXT: usize = 3;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

/// The DOM the app renders from a model holding `text`.
fn list_item_dom(text: &str) -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true)
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("li"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        text,
                    )),
            ),
    );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    StyledDom::create(&mut dom, css)
}

/// The app re-renders from its model: a new generation.
fn re_render(lw: &mut LayoutWindow, text: &str) {
    let window_state = lw.current_window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_new_generation(
        list_item_dom(text),
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
}

fn window(text: &str) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;
    re_render(&mut lw, text);
    lw
}

fn caret(lw: &LayoutWindow) -> TextCursor {
    lw.text_edit_manager
        .get_primary_cursor()
        .expect("the session survives the generation")
}

#[test]
fn a_list_item_caret_moves_with_text_the_app_inserts_before_it() {
    let mut lw = window("alpha");
    let item = azul_core::dom::DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(ITEM))),
    };
    let block = lw
        .text_block_of(item)
        .expect("the list item is a text block");
    // "alp|ha", in the layout's own numbering of the item's runs.
    let at = lw
        .caret_at_node_byte(block, NodeId::new(TEXT), 3)
        .expect("premise: the item's text is laid out");
    assert_eq!(
        at.cluster_id.source_run, 1,
        "premise: the marker is run 0 of the item's layout, so its text is run 1"
    );
    assert!(lw.start_editing_at(at, DomId::ROOT_ID, NodeId::new(ITEM), 0));

    // The engine sees the item's text as it is ...
    re_render(&mut lw, "alpha");
    assert_eq!(
        caret(&lw),
        at,
        "premise: an unchanged generation moves nothing"
    );

    // ... then the app's model gets "XX" in front of it (a remote
    // participant's insert) and renders that.
    re_render(&mut lw, "XXalpha");

    let moved = caret(&lw);
    assert_eq!(
        (
            moved.cluster_id.source_run,
            moved.cluster_id.start_byte_in_run,
            moved.affinity
        ),
        (1, 5, CursorAffinity::Leading),
        "the caret stays before \"ha\": \"XXalp|ha\""
    );
}
