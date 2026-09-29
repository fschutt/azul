//! A screen reader reads a node's OWN text, at the place it stands in.
//!
//! The text a node publishes and sets selections in (`ScopeText`) was the
//! block its text is in, or every block inside it one after the other:
//!
//! - an inline editing host (`<p>Name: <span contenteditable>Bob</span></p>`)
//!   read the whole paragraph as its value, and its offsets counted "Name: ";
//! - a block nested in another - an inline-block's, inside its paragraph - is
//!   laid out in its own block, but its text stands inside the paragraph's:
//!   it was read a second time after the paragraph, and a caret in it was at
//!   that second copy.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::TextBlock,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    block_content::FlatByte, callbacks::ExternalSystemCallbacks, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .p { display: block; }
    .ib { display: inline-block; }
"#;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
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

// ---------------------------------------------------------------------------
// An inline editing host
// ---------------------------------------------------------------------------

/// `body(0) > div.p(1) > ["Name: "(2), span[contenteditable](3) > "Bob"(4)]`
const PARAGRAPH: usize = 1;
const SPAN: usize = 3;
const BOB: usize = 4;

fn inline_host() -> LayoutWindow {
    layout(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_child(text("Name: "))
                .with_child(
                    Dom::create_span()
                        .with_contenteditable(true)
                        .with_child(text("Bob")),
                ),
        ),
    )
}

#[test]
fn an_inline_host_reads_its_own_text() {
    let lw = inline_host();
    assert_eq!(
        block_of(&lw, SPAN),
        block_of(&lw, PARAGRAPH),
        "premise: the span's text is laid out in the paragraph's block"
    );

    assert_eq!(lw.scope_text(dnid(SPAN)).text(), "Bob");
}

#[test]
fn a_caret_in_an_inline_host_is_read_in_its_own_text() {
    let mut lw = inline_host();
    let block = block_of(&lw, PARAGRAPH);
    // "B|ob", in the paragraph's own numbering of its runs.
    let caret = lw
        .caret_at_node_byte(block, NodeId::new(BOB), 1)
        .expect("premise: \"Bob\" is laid out in the paragraph");
    assert!(lw.start_editing_at(caret, DomId::ROOT_ID, NodeId::new(PARAGRAPH), 0));

    let read = lw.accessible_selection().expect("a session is open");
    assert_eq!(read.node, dnid(SPAN), "read on its editing host - the span, not the paragraph");
    assert_eq!(read.text.text(), "Bob");
    assert_eq!((read.anchor, read.focus), (FlatByte(1), FlatByte(1)));

    // And back: offset 2 of the host's text is "Bo|b".
    let (at_block, at) = read
        .text
        .caret_at(FlatByte(2))
        .expect("the host's text has a caret at 2");
    assert_eq!(at_block, block);
    assert_eq!(
        (at.cluster_id.source_run, at.cluster_id.start_byte_in_run),
        (caret.cluster_id.source_run, 2),
        "in \"Bob\"'s run, not byte 2 of \"Name: \""
    );
}

// ---------------------------------------------------------------------------
// An inline-block inside a paragraph
// ---------------------------------------------------------------------------

/// `body(0) > div.p[contenteditable](1) > div.p(2) > ["a "(3),
/// div.ib(4) > "inner"(5), " b"(6)]`
const HOST: usize = 1;
const IB: usize = 4;

fn inline_block() -> LayoutWindow {
    layout(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_contenteditable(true)
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(class("p"))
                        .with_child(text("a "))
                        .with_child(
                            Dom::create_div()
                                .with_ids_and_classes(class("ib"))
                                .with_child(text("inner")),
                        )
                        .with_child(text(" b")),
                ),
        ),
    )
}

#[test]
fn an_inline_block_is_read_where_it_stands_in_its_paragraph() {
    let lw = inline_block();
    assert_ne!(
        block_of(&lw, IB),
        block_of(&lw, 2),
        "premise: the inline-block's text is a block of its own"
    );

    let scope = lw.scope_text(dnid(HOST));
    assert_eq!(scope.text(), "a inner b", "read once, in its place");

    // "in|ner" is byte 4 of the host's text, and byte 4 is "in|ner".
    let inner = block_of(&lw, IB);
    let caret = lw
        .caret_at_node_byte(inner, NodeId::new(5), 2)
        .expect("premise: \"inner\" is laid out in its block");
    assert_eq!(scope.flat_byte_of(inner, &caret), Some(FlatByte(4)));
    let (at_block, at) = scope
        .caret_at(FlatByte(4))
        .expect("the host's text has a caret at 4");
    assert_eq!(at_block, inner, "inside the inline-block, its block");
    assert_eq!(at.cluster_id.start_byte_in_run, 2);
}
