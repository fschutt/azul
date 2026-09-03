//! Contract tests for the app-facing document coordinates
//! (`DocumentSelectionSpan` / `DocumentPosition` — the CallbackInfo
//! selection/caret/sync API), with the hard text cases spelled out:
//! multi-byte chars, decomposed combining marks, ZWJ emoji families, and
//! Arabic (logical byte order regardless of visual direction).
//!
//! The contract under test: byte offsets index the node's FLATTENED text
//! content (what `get_node_text_content` returns), affinity is resolved by
//! grapheme segmentation (a trailing cursor lands past the WHOLE cluster),
//! and spans are normalized `start <= end` in logical order.

use azul_core::dom::{Dom, DomId, DomNodeId, IdOrClass, NodeType};
use azul_core::geom::LogicalSize;
use azul_core::id::NodeId;
use azul_core::selection::{
    CursorAffinity, GraphemeClusterId, IdentifiedSelection, MultiCursorState, Selection,
    SelectionId, SelectionRange, TextCursor,
};
use azul_core::styled_dom::{NodeHierarchyItemId, StyledDom};
use azul_layout::callbacks::ExternalSystemCallbacks;
use azul_layout::window::LayoutWindow;
use azul_layout::window_state::FullWindowState;
use azul_core::resources::RendererResources;
use rust_fontconfig::FcFontCache;

fn editable_paragraph(text: &str) -> (LayoutWindow, DomId, NodeId, NodeId) {
    let mut host = Dom::create_div().with_ids_and_classes(
        vec![IdOrClass::Class("host".into())].into(),
    );
    host.set_contenteditable(true);
    let host = host.with_child(
        Dom::create_p().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
    );
    let mut dom = Dom::create_body().with_child(host);

    let (css, _) = azul_css::parser2::new_from_str("body { font-size: 14px; }");
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

    // Find the text leaf and its host.
    let lr = lw.layout_results.get(&DomId::ROOT_ID).unwrap();
    let nodes = lr.styled_dom.node_data.as_container();
    let mut text_node = None;
    let mut host_node = None;
    for i in 0..nodes.len() {
        let nid = NodeId::new(i);
        match nodes.get(nid).map(azul_core::dom::NodeData::get_node_type) {
            Some(NodeType::Text(_)) => text_node = Some(nid),
            Some(_) if nodes.get(nid).is_some_and(|n| n.is_contenteditable()) => {
                host_node = Some(nid)
            }
            _ => {}
        }
    }
    (lw, DomId::ROOT_ID, host_node.unwrap(), text_node.unwrap())
}

fn cursor(byte: u32, affinity: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity,
    }
}

/// Install an editing session on the text leaf with one RANGE selection.
fn select(lw: &mut LayoutWindow, dom: DomId, node: NodeId, start: TextCursor, end: TextCursor) {
    let dom_node = DomNodeId {
        dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    };
    let mut mc = MultiCursorState::new_with_cursor(start, dom_node, 0);
    mc.selections = vec![IdentifiedSelection {
        id: SelectionId::new(),
        selection: Selection::Range(SelectionRange { start, end }),
    }];
    lw.text_edit_manager.multi_cursor = Some(mc);
}

#[test]
fn ascii_selection_resolves_to_plain_bytes() {
    let (mut lw, dom, _host, text) = editable_paragraph("hello world");
    select(
        &mut lw,
        dom,
        text,
        cursor(6, CursorAffinity::Leading),
        cursor(10, CursorAffinity::Trailing), // trailing on 'd' = past it
    );
    let spans = lw.document_selection_spans();
    assert_eq!(spans.len(), 1);
    assert_eq!((spans[0].start_byte, spans[0].end_byte), (6, 11));
}

#[test]
fn backward_drag_normalizes_to_logical_order() {
    let (mut lw, dom, _host, text) = editable_paragraph("hello world");
    // Anchor AFTER focus (a right-to-left drag): start > end.
    select(
        &mut lw,
        dom,
        text,
        cursor(10, CursorAffinity::Trailing),
        cursor(6, CursorAffinity::Leading),
    );
    let spans = lw.document_selection_spans();
    assert_eq!(
        (spans[0].start_byte, spans[0].end_byte),
        (6, 11),
        "a backward drag reads the same as a forward one"
    );
}

#[test]
fn multibyte_umlaut_trailing_lands_past_both_bytes() {
    // "grüße": g r ü(2) ß(2) e — 'ü' starts at byte 2, 2 bytes wide.
    let (mut lw, dom, _host, text) = editable_paragraph("grüße");
    select(
        &mut lw,
        dom,
        text,
        cursor(0, CursorAffinity::Leading),
        cursor(2, CursorAffinity::Trailing),
    );
    let spans = lw.document_selection_spans();
    assert_eq!(
        (spans[0].start_byte, spans[0].end_byte),
        (0, 4),
        "trailing on 'ü' includes the whole 2-byte char, never half of it"
    );
}

#[test]
fn decomposed_combining_mark_is_one_cluster() {
    // "e\u{301}" = decomposed é: 'e' (1 byte) + COMBINING ACUTE (2 bytes) —
    // ONE grapheme cluster of 3 bytes.
    let (mut lw, dom, _host, text) = editable_paragraph("e\u{301}x");
    select(
        &mut lw,
        dom,
        text,
        cursor(0, CursorAffinity::Leading),
        cursor(0, CursorAffinity::Trailing),
    );
    let spans = lw.document_selection_spans();
    assert_eq!(
        (spans[0].start_byte, spans[0].end_byte),
        (0, 3),
        "trailing must clear the combining mark, not stop between e and \u{301}"
    );
}

#[test]
fn zwj_emoji_family_is_never_split() {
    // "a👨‍👩‍👧b": the family = MAN(4) ZWJ(3) WOMAN(4) ZWJ(3) GIRL(4) = 18
    // bytes starting at byte 1 — one grapheme cluster.
    let family = "👨\u{200D}👩\u{200D}👧";
    assert_eq!(family.len(), 18);
    let text_str = format!("a{family}b");
    let (mut lw, dom, _host, text) = editable_paragraph(&text_str);
    select(
        &mut lw,
        dom,
        text,
        cursor(1, CursorAffinity::Leading),
        cursor(1, CursorAffinity::Trailing),
    );
    let spans = lw.document_selection_spans();
    assert_eq!(
        (spans[0].start_byte, spans[0].end_byte),
        (1, 19),
        "trailing affinity on the family cluster extends past ALL ZWJ joins"
    );
}

#[test]
fn arabic_selection_is_logical_byte_order() {
    // "مرحبا بالعالم" — every Arabic letter is 2 bytes here. The SECOND
    // word starts after "مرحبا " = 5*2 + 1 = byte 11, and is 8 letters =
    // 16 bytes. Visual order is right-to-left; the span must be the
    // LOGICAL byte range, unaffected by display direction.
    let text_str = "مرحبا بالعالم";
    let second_word_start = "مرحبا ".len() as u32;
    let second_word_end = text_str.len() as u32;
    let (mut lw, dom, _host, text) = editable_paragraph(text_str);
    select(
        &mut lw,
        dom,
        text,
        cursor(second_word_start, CursorAffinity::Leading),
        cursor(second_word_end, CursorAffinity::Leading),
    );
    let spans = lw.document_selection_spans();
    assert_eq!(
        (spans[0].start_byte, spans[0].end_byte),
        (second_word_start, second_word_end),
        "RTL text selects in logical bytes"
    );
    // The bytes really do cover the second word.
    assert_eq!(
        &text_str[spans[0].start_byte as usize..spans[0].end_byte as usize],
        "بالعالم"
    );
}

#[test]
fn caret_resolves_and_clamps() {
    let (mut lw, dom, _host, text) = editable_paragraph("hi");
    let dom_node = DomNodeId {
        dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(text)),
    };
    lw.text_edit_manager.multi_cursor = Some(MultiCursorState::new_with_cursor(
        cursor(1, CursorAffinity::Trailing),
        dom_node,
        0,
    ));
    let caret = lw.document_caret().expect("session active");
    assert_eq!(caret.text_byte, 2, "trailing on 'i' = end of text");

    // A stale cursor beyond the text clamps to the end instead of lying.
    lw.text_edit_manager.multi_cursor = Some(MultiCursorState::new_with_cursor(
        cursor(999, CursorAffinity::Trailing),
        dom_node,
        0,
    ));
    let caret = lw.document_caret().expect("session active");
    assert_eq!(caret.text_byte, 2, "past-the-end clamps to len");
}

#[test]
fn child_index_path_walks_ancestor_to_node() {
    let (lw, dom, host, text) = editable_paragraph("x");
    let host_id = DomNodeId {
        dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(host)),
    };
    let text_id = DomNodeId {
        dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(text)),
    };
    // host > p > text: the path is [0, 0].
    assert_eq!(lw.node_child_index_path(host_id, text_id), Some(vec![0, 0]));
    // Reflexive: [].
    assert_eq!(lw.node_child_index_path(host_id, host_id), Some(vec![]));
    // Not a descendant: None.
    assert_eq!(lw.node_child_index_path(text_id, host_id), None);
}
