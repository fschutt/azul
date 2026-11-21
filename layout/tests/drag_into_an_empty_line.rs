//! A selection can reach, and start from, an EMPTY line of an editing host.
//!
//! `layout_ifc` keeps a strut line box for an IFC root with no inline content
//! when it is (inside) a `contenteditable` host, precisely so a caret can stand
//! there - and the blank-line caret (`TextTarget::blank_line_caret`) turns a point
//! on that line into offset 0. The CLICK path calls it (both of its
//! branches); the DRAG path did not, on either of its two resolvers. So a
//! selection dragged across a blank paragraph - the shape a new document is
//! made entirely of - stopped dead at the paragraph before it.
//!
//! The same line has no FIRST or LAST cluster either, and two more selection
//! paths asked for one: `set_cross_block_selection` needs the end of the
//! block a selection starts in, and Ctrl+A needs the start of the first block
//! and the end of the last. A blank line at either place - right after Enter
//! at the end of a document, or above its first paragraph - made both give up.

use azul_core::{
    dom::{Dom, DomId, IdOrClass, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `body(0) > div.host[contenteditable](1) > [ one p.p per entry ]`, in
/// order: `Some(text)` is a paragraph holding that text (the text node
/// follows it), `None` a paragraph holding nothing at all - no text node, no
/// `<br>`. Its only box is the editing strut.
fn layout_a_host_with(paragraphs: &[Option<&str>]) -> LayoutWindow {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .host { display: block; }
        .p { display: block; }
    "#;
    let class =
        |name: &str| -> azul_core::dom::IdOrClassVec { vec![IdOrClass::Class(name.into())].into() };
    let mut host = Dom::create_div()
        .with_ids_and_classes(class("host"))
        .with_contenteditable(true);
    for paragraph in paragraphs {
        let mut p = Dom::create_div().with_ids_and_classes(class("p"));
        if let Some(text) = paragraph {
            p = p.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                *text,
            ));
        }
        host = host.with_child(p);
    }
    let mut dom = Dom::create_body().with_child(host);
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

const HOST: usize = 1;

fn dnid(n: usize) -> azul_core::dom::DomNodeId {
    azul_core::dom::DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn rect_of(lw: &LayoutWindow, n: usize) -> azul_core::geom::LogicalRect {
    lw.get_node_layout_rect(dnid(n))
        .unwrap_or_else(|| panic!("node {n} is laid out"))
}

fn middle_of(lw: &LayoutWindow, n: usize) -> LogicalPosition {
    let r = rect_of(lw, n);
    assert!(
        r.size.height > 0.0,
        "premise: block {n} has a line to stand on, got {r:?}"
    );
    LogicalPosition::new(
        r.origin.x + r.size.width * 0.5,
        r.origin.y + r.size.height * 0.5,
    )
}

/// The blocks the document selection spans, by node index.
fn spanned(lw: &LayoutWindow) -> Option<Vec<usize>> {
    lw.text_edit_manager
        .get_cross_block_selection()
        .map(|s| {
            s.affected_blocks
                .keys()
                .map(|b| b.first_node().index())
                .collect()
        })
}

/// `host(1) > [ p(2) > "alpha"(3), p(4) ]`
#[test]
fn a_drag_resolves_a_caret_on_a_blank_line_the_click_path_can_reach() {
    let lw = layout_a_host_with(&[Some("alpha"), None]);
    let point = middle_of(&lw, 4);

    // The CLICK path reaches it - this is the behaviour the drag has to match,
    // asserted here so the test fails loudly if the premise ever moves.
    let mut click_lw = layout_a_host_with(&[Some("alpha"), None]);
    assert!(
        click_lw.process_mouse_click_for_selection(point, 0).is_some(),
        "premise: a CLICK on the blank line places a caret"
    );

    assert!(
        lw.hittest_text_position_global(DomId::ROOT_ID, point)
            .is_some(),
        "a drag arriving on the blank line must resolve the same caret the click does"
    );
}

/// `host(1) > [ p(2), p(3) > "alpha"(4) ]`: the press on the blank line
/// anchors the selection there, the drag goes on into "alpha".
#[test]
fn a_drag_that_starts_on_a_blank_line_extends_into_the_next_paragraph() {
    let mut lw = layout_a_host_with(&[None, Some("alpha")]);
    let blank = middle_of(&lw, 2);
    let alpha = middle_of(&lw, 3);

    lw.process_mouse_click_for_selection(blank, 0)
        .expect("premise: a click on the blank line places a caret");
    assert_eq!(
        lw.text_edit_manager.get_editing_node_id(),
        Some(NodeId::new(2)),
        "premise: the caret is on the blank line"
    );

    lw.process_mouse_drag_for_selection(blank, alpha)
        .expect("the drag is handled");
    assert_eq!(
        spanned(&lw),
        Some(vec![2, 3]),
        "the selection runs from the blank line into the paragraph below it"
    );
}

/// `host(1) > [ p(2) > "alpha"(3), p(4) ]` - a document right after Enter at
/// its end.
#[test]
fn select_all_covers_a_document_that_ends_in_a_blank_line() {
    let mut lw = layout_a_host_with(&[Some("alpha"), None]);
    assert!(
        lw.select_all_text(dnid(HOST)),
        "Ctrl+A selects the whole document"
    );
    assert_eq!(spanned(&lw), Some(vec![2, 4]));
}

/// `host(1) > [ p(2), p(3) > "alpha"(4) ]` - a blank line above the text.
#[test]
fn select_all_covers_a_document_that_starts_with_a_blank_line() {
    let mut lw = layout_a_host_with(&[None, Some("alpha")]);
    assert!(
        lw.select_all_text(dnid(HOST)),
        "Ctrl+A selects the whole document"
    );
    assert_eq!(spanned(&lw), Some(vec![2, 3]));
}
