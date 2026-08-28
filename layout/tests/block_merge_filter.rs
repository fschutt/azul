//! C13: `block_sibling` block-level filtering.
//!
//! Backspace at the start of a block determines `MergeWithPrevious`; the
//! merge partner must be a real flow-block container. The naive version
//! returned the raw previous sibling, so a block could be merged INTO an
//! XML whitespace text node, across a `<pagebreak/>`, or into an inline.

use azul_core::dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId};
use azul_core::events::DefaultAction;
use azul_core::geom::LogicalSize;
use azul_core::resources::RendererResources;
use azul_core::styled_dom::{NodeHierarchyItemId, StyledDom};
use azul_layout::managers::changeset::DocumentOperation;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .p { display: block; }
    .li { display: list-item; }
    .inline { display: inline; }
"#;

fn cls(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn para(text: &str) -> Dom {
    Dom::create_div().with_ids_and_classes(cls("p")).with_child(
        Dom::create_text_do_not_use_without_block_level_wrapper(text),
    )
}

fn layout(mut dom: Dom) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
    lw
}

fn dom_node(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// Records `MergeWithPrevious { target }`; returns the merge partner
/// (`MergeNodes.first`) if a changeset was recorded.
fn merge_partner(lw: &mut LayoutWindow, target: usize) -> Option<usize> {
    let id = lw.record_structural_default_action(&DefaultAction::MergeWithPrevious {
        target: dom_node(target),
    })?;
    let changeset = lw.get_pending_document_edit().expect("recorded").clone();
    assert_eq!(changeset.id, id);
    let DocumentOperation::MergeNodes(ref m) = changeset.operation else {
        panic!("expected MergeNodes, got {:?}", changeset.operation);
    };
    m.first.node.into_crate_internal().map(|n| n.index())
}

#[test]
fn plain_block_neighbors_still_merge() {
    // body=0, p1=1 (text=2), p2=3 (text=4)
    let mut lw = layout(
        Dom::create_body()
            .with_child(para("first"))
            .with_child(para("second")),
    );
    assert_eq!(
        merge_partner(&mut lw, 3),
        Some(1),
        "the classic Backspace merge keeps working"
    );
}

#[test]
fn whitespace_text_between_blocks_is_skipped() {
    // body=0, p1=1 (text=2), ws=3, p2=4 (text=5) — the XML pretty-printing
    // case: the whitespace run must be SKIPPED, not become the merge target.
    let mut lw = layout(
        Dom::create_body()
            .with_child(para("first"))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "\n    ",
            ))
            .with_child(para("second")),
    );
    assert_eq!(
        merge_partner(&mut lw, 4),
        Some(1),
        "whitespace-only text nodes are formatting, not merge partners"
    );
}

#[test]
fn first_list_item_has_no_merge_partner_inside_the_list() {
    // list=1 > [ws=2, li1=3 (text=4), li2=5 (text=6)]: Backspace at the
    // start of the FIRST li must be a no-op, not a merge into the
    // whitespace (or anything outside the list).
    let list = Dom::create_div()
        .with_ids_and_classes(cls("p"))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "\n  ",
        ))
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(cls("li"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "one",
                )),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(cls("li"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "two",
                )),
        );
    let mut lw = layout(Dom::create_body().with_child(list));
    assert_eq!(
        merge_partner(&mut lw, 3),
        None,
        "no previous block INSIDE the list: Backspace is a no-op"
    );
    assert!(lw.get_pending_document_edit().is_none());
    // ...while the SECOND li still merges into the first (list-item is a
    // flow block).
    assert_eq!(merge_partner(&mut lw, 5), Some(3));
}

#[test]
fn a_real_inline_text_run_is_not_a_merge_partner() {
    // body > [text("intro")=1, p=2]: a block cannot merge INTO a bare text
    // run (it has no child list) — engine default is a safe no-op.
    let mut lw = layout(
        Dom::create_body()
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "intro text",
            ))
            .with_child(para("para")),
    );
    assert_eq!(merge_partner(&mut lw, 2), None);
}

#[test]
fn an_inline_element_between_blocks_stops_the_merge() {
    // body > [p1=1, span=3 (inline, real content), p2=5]: the merge stops at
    // the first real obstacle — it does not jump over the inline to p1.
    let mut lw = layout(
        Dom::create_body()
            .with_child(para("first"))
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(cls("inline"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "inline island",
                    )),
            )
            .with_child(para("second")),
    );
    assert_eq!(merge_partner(&mut lw, 5), None);
}

#[test]
fn a_page_break_marker_blocks_the_merge() {
    // body > [p1=1, pagebreak=3, p2=4]: merging across a page break is an
    // app-level decision, never the engine default.
    let mut lw = layout(
        Dom::create_body()
            .with_child(para("first"))
            .with_child(Dom::create_page_break())
            .with_child(para("second")),
    );
    assert_eq!(merge_partner(&mut lw, 4), None);
}

// ---------------------------------------------------------------------------
// C12: IME × structural interlock — no structural records mid-composition
// ---------------------------------------------------------------------------

#[test]
fn active_ime_composition_suppresses_structural_records() {
    let mut lw = layout(
        Dom::create_body()
            .with_child(para("first"))
            .with_child(para("second")),
    );

    // Composition active: the IME owns the keys — a leaked Backspace must
    // not record a merge against text containing uncommitted preedit.
    lw.text_edit_manager.set_preedit("にほ".to_string(), -1, -1);
    assert_eq!(
        merge_partner(&mut lw, 3),
        None,
        "structural records are suppressed while preedit is active"
    );
    assert!(lw.get_pending_document_edit().is_none());

    // Commit/cancel clears the preedit: the same action records again.
    lw.text_edit_manager.clear_preedit();
    assert_eq!(merge_partner(&mut lw, 3), Some(1));
}

#[test]
fn empty_preedit_string_does_not_lock_editing() {
    // Some IMEs send an EMPTY preedit update on focus — that is not an
    // active composition and must not suppress editing.
    let mut lw = layout(
        Dom::create_body()
            .with_child(para("first"))
            .with_child(para("second")),
    );
    lw.text_edit_manager.set_preedit(String::new(), -1, -1);
    assert_eq!(merge_partner(&mut lw, 3), Some(1));
}
