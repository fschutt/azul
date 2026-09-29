//! A spatial navigation container under `auto` is a scroll box the node is
//! painted in - by containing block, not by DOM parent.
//!
//! css-nav-1 makes a scroll container a spatial navigation container, and
//! "visible" means inside the scrollport of every scroll container above
//! the box. `focus_cursor::spatial_navigation_containers` and `is_visible`
//! walked DOM parents. An `absolute` box whose containing block is outside
//! a non-positioned scroll box is painted outside that box's frame, is not
//! clipped or scrolled by it - and was searched inside it and called
//! invisible wherever it was painted.
//!
//! Boxes are fixed-size `tabindex=0` divs; the window is 800x600.

use azul_core::{
    callbacks::FocusableAreaSearchMode,
    dom::{Dom, DomId, DomNodeId, NodeId, TabIndex},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn focusable(css: &str) -> Dom {
    Dom::create_div()
        .with_tab_index(TabIndex::OverrideInParent(0))
        .with_css(css)
}

/// body(0) > scroller(1) `overflow-y: auto`, 200x100, NOT positioned >
/// [in-flow button(2) at (0,0), 400px(3), absolute button(4) at (250,150)].
/// The absolute button's containing block is the initial one: it is painted
/// outside the scroller, and on screen. The page fits the window.
fn page() -> LayoutWindow {
    let dom = Dom::create_body()
        .with_css("margin: 0; padding: 0;")
        .with_child(
        Dom::create_div()
            .with_css(
                "display: block; margin: 0; padding: 0; width: 200px; height: 100px; overflow-y: \
                 auto;",
            )
            .with_child(focusable(
                "display: block; width: 100px; height: 40px; margin: 0; padding: 0;",
            ))
            .with_child(Dom::create_div().with_css("height: 400px;"))
            .with_child(focusable(
                "position: absolute; top: 150px; left: 250px; width: 50px; height: 20px; margin: \
                 0; padding: 0;",
            )),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

/// `getSpatialNavigationContainer()`: the nearest container the node is
/// painted in, else the document.
#[test]
fn the_container_of_a_box_that_escapes_a_scroll_box_is_not_that_box() {
    let lw = page();
    assert_eq!(
        lw.get_spatial_navigation_container(dnid(2)),
        Some(dnid(1)),
        "control: an in-flow child's container is its scroll box"
    );
    assert_eq!(
        lw.get_spatial_navigation_container(dnid(4)),
        Some(dnid(0)),
        "the absolute button is painted outside the scroller: its container is the document"
    );
}

/// `focusableAreas({ mode: 'visible' })` of the document: both buttons are
/// on screen.
#[test]
fn a_box_that_escapes_a_scroll_box_is_not_clipped_by_it() {
    let lw = page();
    assert_eq!(
        lw.get_focusable_areas(dnid(0), FocusableAreaSearchMode::Visible),
        vec![dnid(2), dnid(4)],
        "the absolute button at (250,150) is painted outside the 200x100 scroller, which does \
         not clip it"
    );
}
