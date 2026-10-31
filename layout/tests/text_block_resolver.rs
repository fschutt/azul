//! `LayoutWindow::text_block_of` - the one rule that says which text block a
//! node's text lives in.
//!
//! Every node that can carry text in a paragraph resolves to the paragraph;
//! a block container that owns no inline layout resolves to nothing; a node
//! that generated no box resolves through its nearest boxed ancestor; and the
//! anonymous block box around "Item" in `li > ["Item", ul]` has a name of its
//! own that round-trips through its layout node.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::TextBlockKey,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .block { display: block; }
"#;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

fn block() -> Dom {
    Dom::create_div().with_ids_and_classes(class("block"))
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

/// `body(0) > div.host[contenteditable](1) > div.p(2) > [text(3) "Hello ",
/// b(4) > text(5) "world"]`
#[test]
fn every_node_of_a_paragraph_resolves_to_the_paragraph() {
    let lw = layout(
        Dom::create_body().with_child(
            block().with_contenteditable(true).with_child(
                block()
                    .with_child(text("Hello "))
                    .with_child(Dom::create_b().with_child(text("world"))),
            ),
        ),
    );
    let paragraph = lw
        .text_block_of(dnid(2))
        .expect("the paragraph owns an inline layout");
    assert_eq!(paragraph.key(), TextBlockKey::Element(NodeId::new(2)));
    for node in [3, 4, 5] {
        assert_eq!(
            lw.text_block_of(dnid(node)),
            Some(paragraph),
            "node {node} (a text leaf, the <b>, the text inside it) is in the paragraph's block"
        );
    }
    assert_eq!(
        lw.text_block_of(dnid(1)),
        None,
        "the host is a block container: its text is in the paragraph, not in a block of its own"
    );
    let root = lw
        .text_block_layout_index(paragraph)
        .expect("the paragraph is laid out");
    assert_eq!(
        lw.text_block_at_layout_index(DomId::ROOT_ID, root),
        Some(paragraph),
        "the block's layout node names the block"
    );
}

/// `body(0) > div.host[contenteditable](1) > div.p(2) > text(3) ""` - an empty
/// text node generates no box; it belongs to its paragraph's block.
#[test]
fn a_node_without_a_box_resolves_through_its_ancestor() {
    let lw = layout(
        Dom::create_body()
            .with_child(block().with_contenteditable(true).with_child(block().with_child(text("")))),
    );
    let paragraph = lw
        .text_block_of(dnid(2))
        .expect("the empty editable line owns an inline layout");
    assert_eq!(lw.text_block_of(dnid(3)), Some(paragraph));
}

/// `body(0) > div.item(1) > [text(2) "Item", div.sub(3) > text(4) "sub"]`:
/// "Item" shares its container with a block, so CSS wraps it in an
/// ANONYMOUS block box.
#[test]
fn text_beside_a_block_is_named_by_its_anonymous_block() {
    let lw = layout(
        Dom::create_body().with_child(
            block()
                .with_child(text("Item"))
                .with_child(block().with_child(text("sub"))),
        ),
    );
    let item = lw
        .text_block_of(dnid(2))
        .expect("the anonymous block around \"Item\" is a text block");
    assert_eq!(
        item.key(),
        TextBlockKey::Anonymous {
            parent: NodeId::new(1),
            first_child: NodeId::new(2),
        }
    );
    assert_eq!(
        lw.text_block_of(dnid(1)),
        None,
        "the container holds a block: it owns no inline layout itself"
    );

    let root = lw
        .text_block_layout_index(item)
        .expect("the anonymous block is laid out");
    assert_eq!(
        lw.text_block_at_layout_index(DomId::ROOT_ID, root),
        Some(item),
        "the anonymous block's name round-trips through its layout node"
    );

    let sub = lw.text_block_of(dnid(4)).expect("the nested block");
    assert_eq!(sub.key(), TextBlockKey::Element(NodeId::new(3)));
    assert!(item < sub, "\"Item\" comes first in the document");
}
