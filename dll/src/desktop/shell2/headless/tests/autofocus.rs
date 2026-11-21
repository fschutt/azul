//! The `autofocus` attribute: a node that asks for focus gets it when it mounts.
//!
//! Every node mounts on a window's first layout, so a window opens with its first
//! `autofocus` node focused (AzWriter's page takes the keys, with a caret). A node that
//! mounts later takes focus too: an overlay that opens (AzDrive's tab overview) answers
//! the arrow keys at once, even though the search box held the focus. The focus is not
//! indicated: no ring, as after a click.

use azul_core::{
    dom::{DomId, DomNodeId, NodeId, TabIndex},
    styled_dom::NodeHierarchyItemId,
};

use super::*;

fn node(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn focused(window: &HeadlessWindow) -> Option<DomNodeId> {
    window
        .common
        .layout_window
        .as_ref()
        .and_then(|lw| lw.focus_manager.get_focused_node().copied())
}

/// `body(0) > [button(1), field(2, autofocus)]`: two focusable nodes, the second asks.
extern "C" fn form_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::dom::AttributeType;
    Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_css("height: 20px;")
                .with_tab_index(TabIndex::Auto),
        )
        .with_child(
            Dom::create_div()
                .with_css("height: 20px;")
                .with_tab_index(TabIndex::Auto)
                .with_attribute(AttributeType::Autofocus),
        )
}

#[test]
fn a_window_opens_with_its_autofocus_node_focused() {
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, form_layout);
    window.regenerate_layout().expect("initial layout");

    assert_eq!(
        focused(&window),
        Some(node(2)),
        "the node with `autofocus` has the focus when the window opens"
    );
    assert!(
        window
            .common
            .layout_window
            .as_ref()
            .is_some_and(|lw| !lw.focus_manager.focus_is_visible),
        "autofocus is not indicated: no ring, as after a click"
    );
}

/// Whether the overlay is open.
#[derive(Debug, Clone, Copy)]
struct Overlay {
    open: bool,
}

/// `body(0) > [search(1), overlay(2, autofocus)?]`: the search box always, the overlay
/// list while it is open.
extern "C" fn overlay_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::dom::AttributeType;
    let open = data.downcast_ref::<Overlay>().is_some_and(|o| o.open);
    let body = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("height: 20px;")
            .with_tab_index(TabIndex::Auto),
    );
    if !open {
        return body;
    }
    body.with_child(
        Dom::create_div()
            .with_css("height: 100px;")
            .with_tab_index(TabIndex::Auto)
            .with_attribute(AttributeType::Autofocus),
    )
}

#[test]
fn an_autofocus_node_that_mounts_takes_the_focus() {
    let state = Arc::new(RefCell::new(RefAny::new(Overlay { open: false })));
    let mut window = make_window_with(&state, overlay_layout);
    window.regenerate_layout().expect("initial layout");
    if let Some(lw) = window.common.layout_window.as_mut() {
        lw.focus_manager.set_focused_node(Some(node(1)));
    }

    {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        if let Some(mut overlay) = r.downcast_mut::<Overlay>() {
            overlay.open = true;
        };
    }
    window.regenerate_layout().expect("the overlay opens");
    assert_eq!(
        focused(&window),
        Some(node(2)),
        "the overlay that opened takes the keys from the search box"
    );

    // A rebuild that keeps the overlay mounts nothing: the focus stays where the user
    // moved it.
    if let Some(lw) = window.common.layout_window.as_mut() {
        lw.focus_manager.set_focused_node(Some(node(1)));
    }
    window.regenerate_layout().expect("an unrelated rebuild");
    assert_eq!(
        focused(&window),
        Some(node(1)),
        "a node that stayed mounted never takes the focus again"
    );
}

/// `body(0) > div[contenteditable, autofocus](1) > "Dear"`: a document page.
extern "C" fn page_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::dom::AttributeType;
    Dom::create_body().with_child(
        Dom::create_div()
            .with_contenteditable(true)
            .with_attribute(AttributeType::Autofocus)
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "Dear",
            )),
    )
}

#[test]
fn an_editable_page_with_autofocus_opens_with_a_caret() {
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, page_layout);
    window.regenerate_layout().expect("initial layout");

    assert_eq!(focused(&window), Some(node(1)), "the page has the focus");
    let lw = window
        .common
        .layout_window
        .as_ref()
        .expect("the window has a layout");
    assert!(
        lw.text_edit_manager.get_primary_cursor().is_some(),
        "a caret stands in the page"
    );
    assert!(
        lw.timers
            .contains_key(&azul_core::task::CURSOR_BLINK_TIMER_ID),
        "and it blinks"
    );
}
