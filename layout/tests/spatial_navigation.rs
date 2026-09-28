//! Spatial (arrow-key) navigation against CSS Spatial Navigation Level 1,
//! measured on a REAL layout.
//!
//! Every spatial test in `focus_cursor.rs` runs on an empty layout tree, so
//! the geometry half of the resolver - which candidates lie "below", and which
//! of them is "nearest" - had no test at all. These lay out real boxes (no
//! text, so no font moves a number) and ask the same two questions the shells
//! ask: `resolve_focus_target(Directional)` for the target, and
//! `determine_keyboard_default_action` for what an arrow key does.
//!
//! Boxes are `display: block` buttons with a fixed size, positioned by
//! margins, so every rect below is known exactly.

use std::collections::BTreeSet;

use azul_core::{
    callbacks::{FocusDirection, FocusTarget},
    dom::{Dom, DomId, DomNodeId, NodeId, TabIndex},
    events::DefaultAction,
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{KeyboardState, VirtualKeyCode},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    default_actions::{default_action_to_focus_target, determine_keyboard_default_action},
    managers::focus_cursor::{resolve_focus_target, FocusResolution},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// One layout pass of `dom` in a `w`x`h` window, the way the shells run one.
/// The funnel also publishes the scroll state, so scroll containers are
/// registered with the scroll manager afterwards.
fn lay_out(dom: Dom, w: f32, h: f32) -> LayoutWindow {
    let styled = StyledDom::create_from_dom(dom);
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(w, h);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        styled,
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

/// A focusable box of `w`x`h` at the position its `margin` puts it: a
/// `tabindex=0` div, so no UA border or padding moves a number.
fn button(w: u32, h: u32, margin: &str) -> Dom {
    Dom::create_div()
        .with_tab_index(TabIndex::OverrideInParent(0))
        .with_css(
            format!("display: block; width: {w}px; height: {h}px; margin: {margin}; padding: 0;")
                .as_str(),
        )
}

fn arrow(key: VirtualKeyCode) -> KeyboardState {
    KeyboardState {
        current_virtual_keycode: Some(key).into(),
        pressed_virtual_keycodes: vec![key].into(),
        ..Default::default()
    }
}

/// Where spatial navigation sends the focus from `from`, as a focus target.
fn target_from(lw: &LayoutWindow, from: usize, dir: FocusDirection) -> Option<DomNodeId> {
    match resolve_focus_target(
        &FocusTarget::Directional(dir),
        &lw.layout_results,
        Some(dnid(from)),
        &BTreeSet::new(),
    ) {
        Ok(FocusResolution::Resolved(n)) => Some(n),
        _ => None,
    }
}

/// What an unmodified Down arrow does with the focus on `from`.
fn down_arrow_from(lw: &LayoutWindow, from: usize) -> DefaultAction {
    determine_keyboard_default_action(
        &arrow(VirtualKeyCode::Down),
        Some(dnid(from)),
        &lw.layout_results,
        false,
    )
    .action
}

// ---------------------------------------------------------------------------
// Selecting the best candidate (css-nav-1 §8.4)
// ---------------------------------------------------------------------------

/// `body(0) > [a(1), b(2)]`: `a` is 100x20 at (0,0); `b` is 100x20 at
/// (150,30), below `a` and further to the side than it is below.
///
/// css-nav-1 decides "below" by EDGES: `b`'s top edge (30) is below `a`'s
/// bottom edge (20), so `b` is a candidate for Down. The old resolver compared
/// CENTRES inside a 45-degree cone (150 across > 30 down) and never offered it,
/// so Down from `a` went nowhere.
#[test]
fn a_box_below_is_reachable_even_when_it_sits_further_to_the_side_than_below() {
    let lw = lay_out(
        Dom::create_body()
            .with_css("margin: 0; padding: 0;")
            .with_child(button(100, 20, "0"))
            .with_child(button(100, 20, "10px 0 0 150px")),
        800.0,
        600.0,
    );
    assert_eq!(
        target_from(&lw, 1, FocusDirection::Down),
        Some(dnid(2)),
        "the box whose top edge is below the focus's bottom edge is BELOW it",
    );
}

/// The css-nav-1 §9.3 example: `a` on top, `b` nearer but half a column to
/// the side, `c` further but straight below.
///
/// `body(0) > panel(1) > [a(2) (0,0 100x20), b(3) (100,110 100x20),
/// c(4) (0,600 100x20)]`. The panel is a `contain` container declaring
/// `spatial-navigation-function: grid`, so Down must prefer the ALIGNED
/// candidate `c` over the nearer `b`.
#[test]
fn grid_prefers_the_aligned_candidate_over_the_nearer_one() {
    let lw = lay_out(
        Dom::create_body()
            .with_css("margin: 0; padding: 0;")
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: block; margin: 0; padding: 0; spatial-navigation-contain: \
                         contain; spatial-navigation-function: grid;",
                    )
                    .with_child(button(100, 20, "0"))
                    .with_child(button(100, 20, "90px 0 0 100px"))
                    .with_child(button(100, 20, "470px 0 0 0")),
            ),
        800.0,
        800.0,
    );
    assert_eq!(
        target_from(&lw, 2, FocusDirection::Down),
        Some(dnid(4)),
        "`grid` must pick the candidate lined up below, not the nearer one to the side",
    );
}

// ---------------------------------------------------------------------------
// spatial-navigation-action (css-nav-1 §9.2)
// ---------------------------------------------------------------------------

/// `body(0) > scroller(1) > [a(2), b(3), c(4)]`, each 40px tall in a 100px
/// `overflow-y: auto` box that says `spatial-navigation-action: focus`.
///
/// With the focus on the LAST item there is nothing below it anywhere. `focus`
/// means "the scroll container is not scrolled, and the search continues up
/// the ancestry chain instead" - so the arrow does nothing.
///
/// The property is read off the focused element if IT is the scroll container,
/// else off its nearest scroll container ancestor. The old lookup took "has
/// `scrollbar_info`" for "is a scroll container", and layout gives EVERY box a
/// `scrollbar_info`, so it always stopped at the focused button, read the
/// button's `auto`, and scrolled the `focus` container.
#[test]
fn an_arrow_with_nowhere_to_go_does_not_scroll_a_focus_container() {
    let lw = lay_out(
        Dom::create_body().with_css("margin: 0; padding: 0;").with_child(
            Dom::create_div()
                .with_css(
                    "display: block; margin: 0; padding: 0; width: 200px; height: 100px; \
                     overflow-y: auto; spatial-navigation-action: focus;",
                )
                .with_child(button(100, 40, "0"))
                .with_child(button(100, 40, "0"))
                .with_child(button(100, 40, "0")),
        ),
        800.0,
        600.0,
    );
    assert_eq!(
        down_arrow_from(&lw, 4),
        DefaultAction::None,
        "`spatial-navigation-action: focus` on the scroll container: nothing to focus means \
         nothing happens, never a scroll",
    );
}

/// `body(0) > [scroller(1) > [a0(2), a1(3), a2(4), a3(5)], outside(6)]`.
///
/// The scroller is 80px tall with `overflow-y: auto`; its four 40px items
/// fill 160px, so `a0` and `a1` are visible and `a2`, `a3` are scrolled out
/// of view. `outside` is a button right under the scroller, visible.
///
/// Down from `a1`: there is no VISIBLE candidate below it inside the scroller,
/// and the scroller can still scroll down. css-nav-1 (`auto`, §8.3 and §9.2):
/// "Otherwise, the scroll container is scrolled in the direction requested."
/// The old resolver searched every candidate everywhere and moved the focus
/// out of the half-read list to `outside`.
#[test]
fn auto_scrolls_the_container_instead_of_leaving_it_while_it_still_has_content_below() {
    let lw = lay_out(
        Dom::create_body()
            .with_css("margin: 0; padding: 0;")
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: block; margin: 0; padding: 0; width: 200px; height: 80px; \
                         overflow-y: auto;",
                    )
                    .with_child(button(100, 40, "0"))
                    .with_child(button(100, 40, "0"))
                    .with_child(button(100, 40, "0"))
                    .with_child(button(100, 40, "0")),
            )
            .with_child(button(100, 20, "0")),
        800.0,
        600.0,
    );
    let action = down_arrow_from(&lw, 3);
    assert!(
        default_action_to_focus_target(&action).is_none() && action != DefaultAction::None,
        "Down at the bottom of the visible part of a scroll box must SCROLL it, not move the \
         focus (got {action:?})",
    );
}

// ---------------------------------------------------------------------------
// Guards: behaviour that must not move
// ---------------------------------------------------------------------------

/// The same three boxes as the `grid` test, in a panel that does NOT ask for
/// `grid`: the spec's distance function takes the nearer `b` (190 against
/// 675), so the property is what makes the difference.
#[test]
fn normal_takes_the_nearer_box_when_the_panel_does_not_ask_for_grid() {
    let lw = lay_out(
        Dom::create_body()
            .with_css("margin: 0; padding: 0;")
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: block; margin: 0; padding: 0; spatial-navigation-contain: \
                         contain;",
                    )
                    .with_child(button(100, 20, "0"))
                    .with_child(button(100, 20, "90px 0 0 100px"))
                    .with_child(button(100, 20, "470px 0 0 0")),
            ),
        800.0,
        800.0,
    );
    assert_eq!(target_from(&lw, 2, FocusDirection::Down), Some(dnid(3)));
}

/// `body(0) > scroller(1) > [a(2), b(3), c(4)]` with
/// `spatial-navigation-action: scroll` on the scroller. css-nav-1 §9.2: "If
/// the currently focused element is not itself a scroll container, this value
/// on an ancestor scroll container has the same effect as auto." Down from
/// `a` focuses the visible `b`.
#[test]
fn scroll_on_an_ancestor_scroll_container_behaves_like_auto() {
    let lw = lay_out(
        Dom::create_body().with_css("margin: 0; padding: 0;").with_child(
            Dom::create_div()
                .with_css(
                    "display: block; margin: 0; padding: 0; width: 200px; height: 100px; \
                     overflow-y: auto; spatial-navigation-action: scroll;",
                )
                .with_child(button(100, 40, "0"))
                .with_child(button(100, 40, "0"))
                .with_child(button(100, 40, "0")),
        ),
        800.0,
        600.0,
    );
    assert_eq!(down_arrow_from(&lw, 2), DefaultAction::FocusDown);
    assert_eq!(target_from(&lw, 2, FocusDirection::Down), Some(dnid(3)));
}

/// The same scroller, focusable itself and focused: `scroll` on the FOCUSED
/// scroll container scrolls it, "without changing which element is in
/// focus, regardless of the presence of focusable descendants".
#[test]
fn scroll_on_the_focused_scroll_container_scrolls_it() {
    let lw = lay_out(
        Dom::create_body().with_css("margin: 0; padding: 0;").with_child(
            Dom::create_div()
                .with_tab_index(TabIndex::OverrideInParent(0))
                .with_css(
                    "display: block; margin: 0; padding: 0; width: 200px; height: 100px; \
                     overflow-y: auto; spatial-navigation-action: scroll;",
                )
                .with_child(button(100, 40, "0"))
                .with_child(button(100, 40, "0"))
                .with_child(button(100, 40, "0")),
        ),
        800.0,
        600.0,
    );
    let action = down_arrow_from(&lw, 1);
    assert!(
        default_action_to_focus_target(&action).is_none() && action != DefaultAction::None,
        "a focused `scroll` container scrolls on Down (got {action:?})",
    );
}
