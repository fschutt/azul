//! A value the APP sets replaces what the user typed - HTML's `input.value =
//! ..` - also when the app sets it by rendering it.
//!
//! The engine keeps the user's uncommitted typing in its text overlay, over
//! the app's (stale) DOM, until the app's model catches up: a new generation
//! that renders the typed text (or an app ack) retires it. That rule could not
//! tell "the app has not adopted the typing yet" from "the app set another
//! value": a generation rendering a DIFFERENT text than the one the user
//! typed over kept the typing on screen, so the app's value never showed.
//!
//! The distinction the engine can make: the text the user typed OVER is what
//! the app rendered at that node. A generation that renders the same text
//! again has not caught up (the typing stays); one that renders something
//! else has set the value (the app's text wins, and the carets move with it).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = "* { margin: 0; padding: 0; } body { font-size: 14px; width: 600px; } .host { \
                   display: block; } p { display: block; }";

/// body(0) > div.host(1, contenteditable) > p(2) > text(3).
const HOST: NodeId = NodeId::new(1);
const PARAGRAPH: NodeId = NodeId::new(2);
const TEXT: NodeId = NodeId::new(3);

/// The DOM the app renders from a model holding `text`.
fn editable_dom(text: &str) -> StyledDom {
    let class: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("host".into())].into();
    let host = Dom::create_div()
        .with_ids_and_classes(class)
        .with_contenteditable(true)
        .with_child(
            Dom::create_p().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                text,
            )),
        );
    let mut dom = Dom::create_body().with_child(host);
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
        editable_dom(text),
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
}

fn window(text: &str) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;
    re_render(&mut lw, text);
    lw
}

/// Type `s` at the start of the paragraph and commit it, the way a keystroke
/// does (`record_text_input` + `apply_text_changeset`).
fn type_at_start(lw: &mut LayoutWindow, s: &str) {
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(HOST)),
    }));
    lw.start_editing_at(
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        },
        DomId::ROOT_ID,
        TEXT,
        0,
    );
    let _ = lw.record_text_input(s);
    let _ = lw.apply_text_changeset();
}

/// What the field shows: the overlay's text if the user's typing still
/// stands, the DOM's otherwise.
fn shown(lw: &LayoutWindow) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, PARAGRAPH);
    lw.extract_text_from_inline_content(&content)
}

#[test]
fn a_generation_that_renders_another_value_replaces_what_the_user_typed() {
    let mut lw = window("hello");
    type_at_start(&mut lw, "X");
    assert_eq!(shown(&lw), "Xhello", "premise: the typing shows");

    // The app sets another value (a "clear and fill", an autocomplete, a
    // formatter) and renders it.
    re_render(&mut lw, "bye");

    assert_eq!(
        shown(&lw),
        "bye",
        "the app rendered a new value: it must replace the typing"
    );
    let caret = lw
        .text_edit_manager
        .get_primary_cursor()
        .expect("the editing session stays");
    assert!(
        caret.cluster_id.start_byte_in_run <= 3,
        "the caret must stay inside the app's value, got byte {}",
        caret.cluster_id.start_byte_in_run
    );
}

#[test]
fn a_generation_that_renders_the_value_typed_over_keeps_the_typing() {
    // The app did not adopt the keystroke (a raw input, a widget without a
    // hook): it renders what it rendered before, and the typing must stand.
    let mut lw = window("hello");
    type_at_start(&mut lw, "X");

    re_render(&mut lw, "hello");

    assert_eq!(shown(&lw), "Xhello");
}

#[test]
fn a_generation_that_renders_the_typed_text_retires_the_typing() {
    // The app adopted the keystroke: the DOM says what the overlay says.
    let mut lw = window("hello");
    type_at_start(&mut lw, "X");

    re_render(&mut lw, "Xhello");

    assert_eq!(shown(&lw), "Xhello");
    assert_eq!(
        lw.content_overlay.text_len(),
        0,
        "a converged entry is dropped (the DOM is the whole truth again)"
    );
}
