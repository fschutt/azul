//! A screen reader reads, and sets, a selection in an editing host that holds
//! paragraphs, in ONE text: the host's paragraphs in document order, one
//! line break between two of them.
//!
//! The offsets an accessibility `SetTextSelection` carries are character
//! indices into the text the tree published for the node. On a host with
//! paragraphs they were resolved against its FIRST paragraph alone (and as
//! bytes): an offset in the second paragraph clamped to the end of the first.
//! The tree itself published no value and no selection for such a host at
//! all - the caret sits in a paragraph, and the offsets it did publish were the
//! raw `start_byte_in_run` of the caret's cluster, without its run or its
//! affinity.

use azul_core::{
    dom::{AccessibilityAction, Dom, DomId, DomNodeId, IdOrClass, NodeId, TextSelectionStartEnd},
    geom::LogicalSize,
    resources::RendererResources,
    selection::TextBlock,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
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

/// `body(0) > div.p[contenteditable](1) > [div.p(2) > "one"(3),
/// div.p(4) > "two"(5)]` - the host's text is "one\ntwo".
const HOST: usize = 1;
const ONE: usize = 2;
const TWO: usize = 4;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn para(s: &str) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class("p"))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(s))
}

fn host_with_two_paragraphs() -> LayoutWindow {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("p"))
            .with_contenteditable(true)
            .with_child(para("one"))
            .with_child(para("two")),
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
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
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

fn select_by_a11y(lw: &mut LayoutWindow, n: usize, selection_start: usize, selection_end: usize) {
    let _ = lw.process_accessibility_action(
        DomId::ROOT_ID,
        NodeId::new(n),
        AccessibilityAction::SetTextSelection(TextSelectionStartEnd {
            selection_start,
            selection_end,
        }),
        Instant::from(std::time::Instant::now()),
    );
}

#[test]
fn a_selection_on_the_host_lands_in_the_paragraph_its_offsets_name() {
    let mut lw = host_with_two_paragraphs();

    // "one\ntwo": characters 5..6 are the "w" of "two".
    select_by_a11y(&mut lw, HOST, 5, 6);

    assert_eq!(
        lw.text_edit_manager.get_editing_block(),
        Some(block_of(&lw, TWO)),
        "offsets past the first paragraph are in the second"
    );
    assert_eq!(lw.focused_selection_byte_range(), Some((1, 2)));
}

#[test]
fn a_selection_on_the_host_across_its_paragraphs_selects_both() {
    let mut lw = host_with_two_paragraphs();

    // "o|ne\ntw|o"
    select_by_a11y(&mut lw, HOST, 1, 6);

    let ends = lw
        .text_edit_manager
        .get_cross_block_selection()
        .map(|cb| (cb.anchor.block, cb.focus.block));
    assert_eq!(
        ends,
        Some((block_of(&lw, ONE), block_of(&lw, TWO))),
        "the selection runs from the first paragraph into the second"
    );
    assert_eq!(
        lw.get_selected_content_for_clipboard(&DomId::ROOT_ID)
            .map(|c| c.plain_text.as_str().to_string())
            .as_deref(),
        Some("ne\ntw")
    );
}

#[cfg(feature = "a11y")]
#[test]
fn the_host_publishes_its_paragraphs_and_the_caret_in_them() {
    use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};

    let mut lw = host_with_two_paragraphs();
    // "t|wo": the caret after the "t" of "two", character 5 of "one\ntwo".
    assert!(lw.start_editing_at(
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Trailing,
        },
        DomId::ROOT_ID,
        NodeId::new(TWO),
        0,
    ));

    lw.update_a11y_tree();
    let update = lw
        .a11y_manager
        .take_pending()
        .expect("the tree update is published");
    let host_id = accesskit::NodeId((0u64 << 32) | (HOST as u64 + 1));
    let host = update
        .nodes
        .iter()
        .find(|(id, _)| *id == host_id)
        .map(|(_, node)| node)
        .expect("the host is in the tree");

    assert_eq!(host.value(), Some("one\ntwo"));
    let selection = host
        .text_selection()
        .expect("the host carries the caret");
    assert_eq!(selection.anchor.character_index, 5);
    assert_eq!(selection.focus.character_index, 5);
}
