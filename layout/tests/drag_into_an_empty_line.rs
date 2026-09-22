//! A caret can be dragged into an EMPTY line of an editing host.
//!
//! `layout_ifc` keeps a strut line box for an IFC root with no inline content
//! when it is inside a `contenteditable` host, precisely so a caret can stand
//! there - and `empty_editing_host_caret` is the fallback that turns a point
//! on that line into offset 0. The CLICK path calls it (both of its
//! branches); the DRAG path did not, on either of its two resolvers. So a
//! selection dragged across a blank paragraph - the shape a new document is
//! made entirely of - stopped dead at the paragraph before it.

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

/// body(0) > div.host[contenteditable](1) > [ p.p(2) > text(3), p.p(4) ]
///
/// The second paragraph holds nothing at all - no text node, no `<br>`. Its
/// only box is the editing strut.
fn layout_a_host_with_a_blank_line() -> LayoutWindow {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .host { display: block; }
        .p { display: block; }
    "#;
    let class =
        |name: &str| -> azul_core::dom::IdOrClassVec { vec![IdOrClass::Class(name.into())].into() };
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true)
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("p"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "alpha",
                    )),
            )
            .with_child(Dom::create_div().with_ids_and_classes(class("p"))),
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

fn rect_of(lw: &LayoutWindow, n: usize) -> azul_core::geom::LogicalRect {
    lw.get_node_layout_rect(azul_core::dom::DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    })
    .unwrap_or_else(|| panic!("node {n} is laid out"))
}

#[test]
fn a_drag_resolves_a_caret_on_a_blank_line_the_click_path_can_reach() {
    let lw = layout_a_host_with_a_blank_line();

    let blank = rect_of(&lw, 4);
    assert!(
        blank.size.height > 0.0,
        "premise: the blank paragraph has an editing strut to stand on, got {blank:?}"
    );
    let point = LogicalPosition::new(
        blank.origin.x + blank.size.width * 0.5,
        blank.origin.y + blank.size.height * 0.5,
    );

    // The CLICK path reaches it - this is the behaviour the drag has to match,
    // asserted here so the test fails loudly if the premise ever moves.
    let mut click_lw = layout_a_host_with_a_blank_line();
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
