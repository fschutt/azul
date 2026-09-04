//! Every commit that replaces a node's inline content is keyed to the caret's
//! IFC OWNER (`LayoutWindow::caret_text_target`), never to the focused host.
//!
//! Typing has been keyed that way since the Enter fix; Backspace/Delete
//! (`delete_selection`) still keyed its commit to the HOST. Two consequences:
//! the host-flattened blob was spliced at per-IFC cursor offsets (a
//! multi-paragraph editable deleted from the wrong place), and the app-facing
//! sync API handed out an edit whose node no `get_node_child_index_path` from
//! a block could reach — AzWriter's word count froze on Backspace.
//!
//! The contract: after focus on the host and a caret in the leaf, a deletion
//! lands in the overlay under the PARAGRAPH — the same key typing uses — and
//! `unsynced_text_edits` reports the paragraph.

use azul_core::dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId};
use azul_core::geom::LogicalSize;
use azul_core::resources::RendererResources;
use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
use azul_core::styled_dom::{NodeHierarchyItemId, StyledDom};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = "* { margin: 0; padding: 0; } \
                   body { font-size: 14px; width: 600px; } \
                   .host { display: block; } \
                   p { display: block; }";

/// body(0) > div.host(1, contenteditable) > p(2) > text(3) "hello".
const HOST: NodeId = NodeId::new(1);
const PARAGRAPH: NodeId = NodeId::new(2);
const TEXT: NodeId = NodeId::new(3);

fn editable_dom() -> StyledDom {
    let mut host =
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("host".into())].into());
    host.set_contenteditable(true);
    let host = host.with_child(
        Dom::create_p().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("hello")),
    );
    let mut dom = Dom::create_body().with_child(host);
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    StyledDom::create(&mut dom, css)
}

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug = Some(Vec::new());
    lw.layout_new_generation(
        editable_dom(),
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug,
    )
    .unwrap();
    lw
}

fn host_id() -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(HOST)),
    }
}

/// Focus on the contenteditable HOST (what a click sets), caret in the text
/// leaf at `byte` (what the editing session records).
fn session_at(lw: &mut LayoutWindow, byte: u32) {
    lw.focus_manager.set_focused_node(Some(host_id()));
    lw.text_edit_manager.initialize_editing(
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Leading,
        },
        DomId::ROOT_ID,
        TEXT,
        0,
    );
}

fn text_of(lw: &LayoutWindow, node: NodeId) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, node);
    lw.extract_text_from_inline_content(&content)
}

/// The overlay's edited IFC roots: (node, flattened text).
fn overlay_entries(lw: &LayoutWindow) -> Vec<(NodeId, String)> {
    lw.content_overlay
        .iter_text()
        .map(|(&(_, node), dirty)| {
            (node, azul_layout::overlay::flatten_inline_content(&dirty.content))
        })
        .collect()
}

#[test]
fn backspace_is_keyed_to_the_paragraph_like_typing() {
    let mut lw = window();
    session_at(&mut lw, 5); // caret after "hello"

    let affected = lw
        .delete_selection(host_id(), false)
        .expect("a backspace at the end of 'hello' deletes 'o'");
    assert_eq!(affected, vec![host_id()], "the host is what the caller re-renders");

    assert_eq!(
        overlay_entries(&lw),
        vec![(PARAGRAPH, "hell".to_string())],
        "the deletion lands under the caret's IFC owner, not the focused host"
    );
    assert_eq!(text_of(&lw, PARAGRAPH), "hell");

    let unsynced = lw.unsynced_text_edits();
    assert_eq!(unsynced.len(), 1);
    assert_eq!(
        unsynced[0].0.node.into_crate_internal(),
        Some(PARAGRAPH),
        "the app maps the edit to its block through the paragraph"
    );
    assert_eq!(unsynced[0].1, "hell");
    assert_eq!(
        lw.node_child_index_path(host_id(), unsynced[0].0),
        Some(vec![0]),
        "the edit node is reachable from the host by child-index path"
    );
}

#[test]
fn delete_forward_is_keyed_to_the_paragraph_too() {
    let mut lw = window();
    session_at(&mut lw, 0); // caret before "hello"

    lw.delete_selection(host_id(), true)
        .expect("Delete at the start of 'hello' deletes 'h'");

    assert_eq!(overlay_entries(&lw), vec![(PARAGRAPH, "ello".to_string())]);
}

#[test]
fn typing_then_deleting_share_one_overlay_entry() {
    // The two commits must agree on the key, otherwise the second one reads
    // stale content (the DOM's text instead of the typed overlay).
    let mut lw = window();
    session_at(&mut lw, 5);
    let _ = lw.record_text_input("!");
    let _ = lw.apply_text_changeset();
    assert_eq!(text_of(&lw, PARAGRAPH), "hello!");

    lw.delete_selection(host_id(), false)
        .expect("backspace after typing removes the '!'");
    assert_eq!(
        overlay_entries(&lw),
        vec![(PARAGRAPH, "hello".to_string())],
        "one entry, one key, the deletion read the typed text"
    );
    let unsynced = lw.unsynced_text_edits();
    assert_eq!(unsynced.len(), 1, "same node = one unsynced edit, not two");
    assert_eq!(unsynced[0].2, 2, "two commits = revision 2");
}
