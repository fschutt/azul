//! Ctrl+A selects the whole editing host it was pressed in - every text block
//! of it, and only that host's.
//!
//! Ctrl+A found its blocks with a walk of its own: a DOM descent that stopped
//! at elements owning an inline layout and skipped text children. Text beside
//! a block (`host > ["Item", p("sub")]`) is in an ANONYMOUS block, which no
//! element owns - the walk never saw it, so the selection started below it.
//! And a host that is one block had its range written into whatever session
//! was open, in whichever block that was.

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

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .p { display: block; }
"#;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

fn para(s: &str) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class("p"))
        .with_child(text(s))
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

fn start() -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 0,
        },
        affinity: CursorAffinity::Leading,
    }
}

fn open_session_in(lw: &mut LayoutWindow, n: usize) {
    assert!(
        lw.start_editing_at(start(), DomId::ROOT_ID, NodeId::new(n), 0),
        "premise: a session opens in node {n}'s block"
    );
}

fn copied(lw: &LayoutWindow) -> Option<String> {
    lw.get_selected_content_for_clipboard(&DomId::ROOT_ID)
        .map(|c| c.plain_text.as_str().to_string())
}

#[test]
fn select_all_starts_at_text_beside_a_block() {
    // `body(0) > div.p[contenteditable](1) > ["Item"(2), div.p(3) > "sub"(4)]`
    let mut lw = layout(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(class("p"))
                .with_contenteditable(true)
                .with_child(text("Item"))
                .with_child(para("sub")),
        ),
    );
    open_session_in(&mut lw, 3);

    assert!(lw.select_all_text(dnid(1)), "Ctrl+A selects the host");

    assert_eq!(copied(&lw).as_deref(), Some("Item\nsub"));
}

#[test]
fn select_all_in_a_field_selects_that_field() {
    // `body(0) > [div.p[contenteditable](1) > "alpha"(2),
    // div.p[contenteditable](3) > "beta"(4)]`
    let mut lw = layout(
        Dom::create_body()
            .with_child(para("alpha").with_contenteditable(true))
            .with_child(para("beta").with_contenteditable(true)),
    );
    open_session_in(&mut lw, 1);

    assert!(lw.select_all_text(dnid(3)), "Ctrl+A selects the field");

    assert_eq!(
        copied(&lw).as_deref(),
        Some("beta"),
        "the field's own text, not its range laid over the other field's"
    );
    assert_eq!(
        lw.text_edit_manager.get_editing_block(),
        lw.text_block_of(dnid(3))
    );
}
