//! A caret in text that shares its editable container with a block - the
//! loose "loose" in `host[contenteditable] > ["loose", p > "para"]` - never
//! lets a keystroke rewrite the paragraph next to it.
//!
//! CSS wraps "loose" in an ANONYMOUS block box. A click can put the caret
//! there (it is a text block like any other), but an edit has no element to
//! be keyed to: the block's text is its container's inline run, not a node's
//! content. The edit fell back to the focused host, spliced the host's
//! flattened text ("loose" + "para") at the caret, and the reshape - which
//! looks for an inline layout below the host - found the paragraph's and
//! painted the whole blob into it.

use std::collections::BTreeMap;

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, text3::cache::ShapedItem, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .block { display: block; }
"#;

/// `body(0) > div.block[contenteditable](1) > [text(2) "loose",
/// div.block(3) > text(4) "para"]`
const HOST: usize = 1;
const LOOSE: usize = 2;
const PARA: usize = 4;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

fn editor() -> LayoutWindow {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("block"))
            .with_contenteditable(true)
            .with_child(text("loose"))
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("block"))
                    .with_child(text("para")),
            ),
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
    lw
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// The text a block's layout shows: its runs, in order.
fn shown_text(lw: &LayoutWindow, n: usize) -> String {
    let target = lw
        .text_target_at_node(dnid(n))
        .unwrap_or_else(|| panic!("node {n} is in a laid-out text block"));
    let mut runs: BTreeMap<u32, String> = BTreeMap::new();
    for item in target.layout.items.iter() {
        if let ShapedItem::Cluster(c) = &item.item {
            runs.entry(c.source_cluster_id.source_run)
                .or_insert_with(|| c.source_text.to_string());
        }
    }
    runs.into_values().collect()
}

#[test]
fn typing_beside_a_block_never_rewrites_the_paragraph_below() {
    let mut lw = editor();
    assert_eq!(shown_text(&lw, PARA), "para", "premise");

    // Click on "loose": the caret goes into its anonymous block.
    let host = lw
        .get_node_layout_rect(dnid(HOST))
        .expect("the host is laid out");
    lw.process_mouse_click_for_selection(
        LogicalPosition::new(host.origin.x + 2.0, host.origin.y + 6.0),
        0,
    )
    .expect("the click lands on \"loose\"");
    assert_eq!(
        lw.text_edit_manager.get_editing_block(),
        lw.text_block_of(dnid(LOOSE)),
        "premise: the caret is in the anonymous block around \"loose\""
    );
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));

    let _ = lw.record_text_input("x");
    let _ = lw.apply_text_changeset();

    assert_eq!(
        shown_text(&lw, PARA),
        "para",
        "the paragraph next to the caret's block shows its own text"
    );
}
