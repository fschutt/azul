//! A field the user typed into survives the app rendering a page with FEWER nodes before it: the layout
//! pass of the new page does not panic, and the typing shows where the field now is.
//!
//! Found by the whiteboard's end-to-end run of AzReview (E22, 2026-11-26): type a label into the label
//! editor's text area, then press Tab. The blur callback renders the page again, with a node less before
//! the editor's field; the field is the same field (the reconciliation matches it) and its typing, the text
//! overlay's entry, follows it to its new id. The runs of that entry name the text node they came from by
//! the id the node had in the page the typing was made in, and those did not follow: the node of the new
//! page that had that id (here: none, the id is one past the end of the page) was looked up by the display
//! list, `node_data[260]` of a page of 260 nodes, and the layout pass panicked in the main thread. The fatal
//! panic dialog of the hook then waited for a click nobody could give in a headless run, so the app hung.
//!
//! The page of the test is the smallest that shrinks before a field: `body > div.form > [div.junk,
//! div#target(contenteditable) > p > text]`, rendered again without the `div.junk`.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, IdOrClassVec, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, text3::cache::InlineContent, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = "* { margin: 0; padding: 0; } body { font-size: 14px; width: 600px; } \
                   .field, .junk { display: block; height: 20px; } p { display: block; }";

/// The page with the junk: body(0) > div.form(1) > [div.junk(2), div#target(3) > p(4) > text(5)]
const FIELD_WITH_JUNK: NodeId = NodeId::new(3);
const TEXT_WITH_JUNK: NodeId = NodeId::new(5);
/// The page without it: body(0) > div.form(1) > [div#target(2) > p(3) > text(4)]
const PARAGRAPH_WITHOUT_JUNK: NodeId = NodeId::new(3);
const TEXT_WITHOUT_JUNK: NodeId = NodeId::new(4);
const NODES_WITHOUT_JUNK: usize = 5;

fn field() -> Dom {
    let ids: IdOrClassVec =
        vec![IdOrClass::Id("target".into()), IdOrClass::Class("field".into())].into();
    Dom::create_div()
        .with_ids_and_classes(ids)
        .with_contenteditable(true)
        .with_child(
            Dom::create_p()
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("x")),
        )
}

/// The page: the junk before the field, or not.
fn page(with_junk: bool) -> StyledDom {
    let form: IdOrClassVec = vec![IdOrClass::Class("form".into())].into();
    let mut form_dom = Dom::create_div().with_ids_and_classes(form);
    if with_junk {
        let junk: IdOrClassVec = vec![IdOrClass::Class("junk".into())].into();
        form_dom = form_dom.with_child(Dom::create_div().with_ids_and_classes(junk));
    }
    let mut dom = Dom::create_body().with_child(form_dom.with_child(field()));
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    StyledDom::create(&mut dom, css)
}

/// The app renders a page: a new generation, installed the way the shells' `regenerate_layout` installs
/// one - the reconciliation first (which old node became which new one: the node-keyed state, the text
/// overlay among it, follows), then the layout, then the reconciliation's completion.
fn render(lw: &mut LayoutWindow, with_junk: bool) {
    let window_state = lw.current_window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    let mut styled = page(with_junk);
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

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.node_identity.id_is_key = true;
    lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;
    render(&mut lw, true);
    lw
}

/// Types `s` at the start of the field, the way a keystroke does (`record_text_input` +
/// `apply_text_changeset`); the app does not adopt it.
fn type_into_field(lw: &mut LayoutWindow, s: &str) {
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(FIELD_WITH_JUNK)),
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
        TEXT_WITH_JUNK,
        0,
    );
    let _ = lw.record_text_input(s);
    let _ = lw.apply_text_changeset();
}

/// The text nodes the runs of the typing name.
fn text_nodes_the_typing_names(lw: &LayoutWindow) -> Vec<NodeId> {
    let mut named = Vec::new();
    for (_, entry) in lw.content_overlay.iter_text() {
        for item in &entry.content {
            if let InlineContent::Text(run) = item {
                named.extend(run.source_node_id);
            }
        }
    }
    named
}

fn shown(lw: &LayoutWindow, paragraph: NodeId) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, paragraph);
    lw.extract_text_from_inline_content(&content)
}

fn nodes_of_the_page(lw: &LayoutWindow) -> usize {
    lw.layout_results
        .get(&DomId::ROOT_ID)
        .expect("the root DOM was laid out")
        .styled_dom
        .node_data
        .as_ref()
        .len()
}

/// The page shrinks by a node before the typed field: the layout of the new page holds, the typing follows
/// its field and every node its runs name is a node of the new page (the text node of the field, which had
/// the id 5 and has the id 4).
#[test]
fn a_page_that_shrinks_before_a_typed_field_lays_out_and_keeps_the_typing() {
    let mut lw = window();
    type_into_field(&mut lw, "Ada");
    assert_eq!(lw.content_overlay.text_len(), 1, "premise: the typing is in the overlay");
    assert_eq!(
        text_nodes_the_typing_names(&lw),
        vec![TEXT_WITH_JUNK],
        "premise: the runs of the typing name the text node of the field, by its id in this page"
    );

    // The blur callback renders the page again, without the junk before the field.
    render(&mut lw, false);

    assert_eq!(nodes_of_the_page(&lw), NODES_WITHOUT_JUNK);
    assert_eq!(lw.content_overlay.text_len(), 1, "the typing followed its field");
    assert_eq!(
        text_nodes_the_typing_names(&lw),
        vec![TEXT_WITHOUT_JUNK],
        "the runs name the text node of the field by its id in the new page"
    );
    assert_eq!(
        shown(&lw, PARAGRAPH_WITHOUT_JUNK),
        "Adax",
        "the typing shows in the field where it now is"
    );
}

/// The field itself leaves the page (the editor closes): its typing goes with it and the page lays out.
#[test]
fn a_field_that_leaves_the_page_takes_its_typing_with_it() {
    let mut lw = window();
    type_into_field(&mut lw, "Ada");
    assert_eq!(lw.content_overlay.text_len(), 1, "premise: the typing is in the overlay");

    // The page without the field: body > div.form > [div.junk]
    let window_state = lw.current_window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    let form: IdOrClassVec = vec![IdOrClass::Class("form".into())].into();
    let junk: IdOrClassVec = vec![IdOrClass::Class("junk".into())].into();
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(form)
            .with_child(Dom::create_div().with_ids_and_classes(junk)),
    );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let mut styled = StyledDom::create(&mut dom, css);
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

    assert_eq!(lw.content_overlay.text_len(), 0, "the typing went with its field");
}
