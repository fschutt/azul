//! What the user typed into a field stays with THAT field: when the app
//! replaces a page of fields with another page of the same shape (a wizard's
//! Next), the new fields show what the app renders for them, never the
//! typing of the field that stood at their place.
//!
//! Found in AzMail's Add Account wizard (MAIL6, 2026-10-03): page 1 has the
//! fields `#acct-name` and `#acct-email`, page 2 `#acct-imap-host` and
//! `#acct-imap-port` at the same tree positions. After Next, the IMAP host
//! field showed "Ada Lovelace" (typed into the name field), the port field
//! the e-mail address, and the next keystrokes went in at the old caret
//! ("Ada Lovelac127.0.0.1e"), which the app then received as the host. The
//! fields carry different CSS ids: different elements, so the old field's
//! uncommitted typing (the text overlay, the caret) must go with the old
//! field when it unmounts.

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

const CSS: &str = "* { margin: 0; padding: 0; } body { font-size: 14px; width: 600px; } \
                   .field { display: block; height: 20px; } p { display: block; }";

/// body(0) > div.form(1) > [div#<first>(2) > p(3) > text(4),
///                          div#<second>(5) > p(6) > text(7)]
const FIRST_FIELD: NodeId = NodeId::new(2);
const FIRST_PARAGRAPH: NodeId = NodeId::new(3);
const FIRST_TEXT: NodeId = NodeId::new(4);
const SECOND_PARAGRAPH: NodeId = NodeId::new(6);

/// The app renders the same value for every field (empty in AzMail; one
/// letter here so every field has a text run to put a caret in).
const VALUE: &str = "x";

fn field(id: &str) -> Dom {
    let ids: azul_core::dom::IdOrClassVec =
        vec![IdOrClass::Id(id.into()), IdOrClass::Class("field".into())].into();
    Dom::create_div()
        .with_ids_and_classes(ids)
        .with_contenteditable(true)
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper(VALUE),
        ))
}

/// A page of two fields with these ids.
fn page(first: &str, second: &str) -> StyledDom {
    let form: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("form".into())].into();
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(form)
            .with_child(field(first))
            .with_child(field(second)),
    );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    StyledDom::create(&mut dom, css)
}

/// The app renders a page: a new generation, installed the way the shells'
/// `regenerate_layout` (and the E2E runner) install one - the reconciliation
/// first (which old node became which new one: node-keyed state such as the
/// text overlay follows it), then the layout, then the reconciliation's
/// completion. (LAYOUT7: the test called `layout_new_generation` alone, a
/// path no app takes, where nothing ever moves or drops node-keyed state.)
fn render(lw: &mut LayoutWindow, first: &str, second: &str) {
    let window_state = lw.current_window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    let mut styled = page(first, second);
    let pending =
        lw.begin_reconciliation(DomId::ROOT_ID, &mut styled, azul_core::task::Instant::now());
    lw.layout_new_generation(
        styled,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
    lw.finish_reconciliation(DomId::ROOT_ID, &pending);
}

fn window(first: &str, second: &str) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;
    render(&mut lw, first, second);
    lw
}

/// Types `s` at the start of the first field, the way a keystroke does
/// (`record_text_input` + `apply_text_changeset`); the app does not adopt it
/// (it re-renders nothing until Next).
fn type_into_first_field(lw: &mut LayoutWindow, s: &str) {
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(FIRST_FIELD)),
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
        FIRST_TEXT,
        0,
    );
    let _ = lw.record_text_input(s);
    let _ = lw.apply_text_changeset();
}

/// What the paragraph of a field shows: the overlay's text while the typing
/// stands, the DOM's otherwise.
fn shown(lw: &LayoutWindow, paragraph: NodeId) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, paragraph);
    lw.extract_text_from_inline_content(&content)
}

#[test]
fn a_field_of_the_next_page_does_not_show_the_typing_of_the_field_at_its_place() {
    let mut lw = window("acct-name", "acct-email");
    type_into_first_field(&mut lw, "Ada");
    assert_eq!(
        shown(&lw, FIRST_PARAGRAPH),
        "Adax",
        "premise: the typing shows in the first field"
    );

    // Next: the app renders page 2 - two OTHER fields at the same places.
    render(&mut lw, "acct-imap-host", "acct-imap-port");

    assert_eq!(
        shown(&lw, FIRST_PARAGRAPH),
        VALUE,
        "#acct-imap-host is another element than #acct-name: it shows what the app renders \
         for it, not the name typed into the field that stood there"
    );
    assert_eq!(shown(&lw, SECOND_PARAGRAPH), VALUE);
    assert_eq!(
        lw.content_overlay.text_len(),
        0,
        "the typing went with its field when that field unmounted"
    );
}

#[test]
fn the_same_field_rendered_again_keeps_its_typing() {
    // The counterpart that must keep working: the same page re-rendered
    // without the app adopting the typing keeps it on screen.
    let mut lw = window("acct-name", "acct-email");
    type_into_first_field(&mut lw, "Ada");

    render(&mut lw, "acct-name", "acct-email");

    assert_eq!(shown(&lw, FIRST_PARAGRAPH), "Adax");
}
