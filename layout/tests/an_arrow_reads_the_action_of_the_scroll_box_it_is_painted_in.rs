//! An arrow key reads the `spatial-navigation-action` of the scroll box its
//! focus is painted in.
//!
//! css-nav-1 §9.2: the value in force is the focused element's own if it is
//! a scroll container, else that of "its nearest scroll container
//! ancestor" - the box whose scrolling moves it. When the steps find
//! nothing to focus and nothing to scroll, `default_actions::
//! spatial_navigation_action` decides between a scroll fallback (`auto`)
//! and nothing (`focus`). It walked DOM parents. An `absolute` box whose
//! containing block is outside a non-positioned scroll box is painted
//! outside that box's frame and does not scroll with it, yet the box's
//! `focus` silenced every arrow on it.
//!
//! Boxes are fixed-size `tabindex=0` divs; the window is 800x600.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId, TabIndex},
    events::{DefaultAction, ScrollAmount, ScrollDirection},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{KeyboardState, VirtualKeyCode},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, default_actions::determine_keyboard_default_action,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn lay_out(dom: Dom) -> LayoutWindow {
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

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn down_arrow_from(lw: &LayoutWindow, from: usize) -> DefaultAction {
    let key = VirtualKeyCode::Down;
    determine_keyboard_default_action(
        &KeyboardState {
            current_virtual_keycode: Some(key).into(),
            pressed_virtual_keycodes: vec![key].into(),
            ..Default::default()
        },
        Some(dnid(from)),
        &lw.layout_results,
        false,
    )
    .action
}

/// body(0) > scroller(1) `overflow-y: auto; spatial-navigation-action:
/// focus`, NOT positioned > [400px(2), absolute button(3)]. The button's
/// containing block is the initial one: it sits at (250,150), outside the
/// scroller's frame. It is the only focusable, and the page fits the window,
/// so the steps find nothing to focus and nothing to scroll.
#[test]
fn an_arrow_on_a_box_that_escapes_a_focus_scroller_is_not_silenced_by_it() {
    let lw = lay_out(
        Dom::create_body()
            .with_css("margin: 0; padding: 0;")
            .with_child(
            Dom::create_div()
                .with_css(
                    "display: block; margin: 0; padding: 0; width: 200px; height: 100px; \
                     overflow-y: auto; spatial-navigation-action: focus;",
                )
                .with_child(Dom::create_div().with_css("height: 400px;"))
                .with_child(
                    Dom::create_div()
                        .with_tab_index(TabIndex::OverrideInParent(0))
                        .with_css(
                            "position: absolute; top: 150px; left: 250px; width: 50px; height: \
                             20px; margin: 0; padding: 0;",
                        ),
                ),
        ),
    );
    assert_eq!(
        down_arrow_from(&lw, 3),
        DefaultAction::ScrollFocusedContainer {
            direction: ScrollDirection::Down,
            amount: ScrollAmount::Line,
        },
        "the button is painted outside the scroller: no scroll box it lives in says `focus`, so \
         the arrow keeps the `auto` fallback"
    );
}
