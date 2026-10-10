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

// ---------------------------------------------------------------------------
// The LIVE path: painted geometry and the scroll boundary
// ---------------------------------------------------------------------------

/// The `auto_scrolls_...` layout: an 80px `overflow-y: auto` scroller(1)
/// holding a0(2)..a3(5), 40px each, and `outside`(6) right under it.
fn scroll_list() -> LayoutWindow {
    lay_out(
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
    )
}

fn scroll_list_to(lw: &mut LayoutWindow, y: f32) {
    let now: azul_core::task::Instant = std::time::Instant::now().into();
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(1),
        azul_core::geom::LogicalPosition::new(0.0, y),
        now,
    );
}

fn live_down_from(lw: &LayoutWindow, from: usize) -> DefaultAction {
    lw.keyboard_default_action(&arrow(VirtualKeyCode::Down), Some(dnid(from)), false, None)
        .action
}

/// The arrow names the container it scrolls: the scroller, one line down.
#[test]
fn the_scroll_names_the_container_the_steps_picked() {
    let lw = scroll_list();
    assert_eq!(
        live_down_from(&lw, 3),
        DefaultAction::ScrollContainer {
            container: dnid(1),
            direction: azul_core::events::ScrollDirection::Down,
            amount: azul_core::events::ScrollAmount::Line,
        },
    );
}

/// Scrolled down by 40px, `a2` is on screen right under `a1`, and Down
/// focuses it. The STATIC geometry still has `a2` below the fold - the answer
/// the old resolver's unscrolled rects gave.
#[test]
fn an_item_scrolled_into_view_is_focused_where_it_is_painted() {
    let mut lw = scroll_list();
    scroll_list_to(&mut lw, 40.0);
    assert_eq!(live_down_from(&lw, 3), DefaultAction::FocusDown);
    assert_eq!(
        lw.resolve_focus_target_live(
            &FocusTarget::Directional(FocusDirection::Down),
            Some(dnid(3))
        ),
        Ok(FocusResolution::Resolved(dnid(4))),
    );
}

// ---------------------------------------------------------------------------
// The css-nav-1 JS API (§5.2): getSpatialNavigationContainer(),
// focusableAreas(), spatialNavigationSearch()
// ---------------------------------------------------------------------------

/// `getSpatialNavigationContainer()`: the nearest ANCESTOR that is a spatial
/// navigation container - never the element itself - or the document (the
/// DOM's root node) when that is the viewport.
#[test]
fn the_container_of_a_list_item_is_its_scroll_box_and_of_the_box_the_document() {
    let lw = scroll_list();
    assert_eq!(lw.get_spatial_navigation_container(dnid(3)), Some(dnid(1)));
    assert_eq!(
        lw.get_spatial_navigation_container(dnid(1)),
        Some(dnid(0)),
        "a container's own container is its nearest container ANCESTOR",
    );
    assert_eq!(lw.get_spatial_navigation_container(dnid(6)), Some(dnid(0)));
}

/// `focusableAreas({ mode })`: the focusable DESCENDANTS, in document order;
/// `visible` keeps the ones inside every scrollport above them.
#[test]
fn focusable_areas_are_the_visible_ones_or_all_of_them() {
    use azul_core::callbacks::FocusableAreaSearchMode;

    let lw = scroll_list();
    let ids = |v: &[usize]| v.iter().map(|n| dnid(*n)).collect::<Vec<_>>();
    assert_eq!(
        lw.get_focusable_areas(dnid(1), FocusableAreaSearchMode::Visible),
        ids(&[2, 3]),
        "a2 and a3 are scrolled out of the 80px box",
    );
    assert_eq!(
        lw.get_focusable_areas(dnid(1), FocusableAreaSearchMode::All),
        ids(&[2, 3, 4, 5]),
    );
    assert_eq!(
        lw.get_focusable_areas(dnid(0), FocusableAreaSearchMode::Visible),
        ids(&[2, 3, 6]),
    );
    assert_eq!(
        lw.get_focusable_areas(dnid(0), FocusableAreaSearchMode::All),
        ids(&[2, 3, 4, 5, 6]),
    );
}

/// `spatialNavigationSearch(dir, options)`.
///
/// No options: the VISIBLE areas of the nearest container only, and - the
/// spec's note - no climbing further up, so Down from `a1` finds nothing
/// (`a2` is scrolled out). With the document as the container, `outside`.
/// With explicit candidates, the best of exactly those, visible or not.
#[test]
fn spatial_navigation_search_searches_the_container_or_the_candidates_it_is_given() {
    use azul_core::{
        callbacks::SpatialNavigationSearchOptions,
        dom::{OptionDomNodeId, OptionDomNodeIdVec},
    };

    let lw = scroll_list();
    assert_eq!(
        lw.spatial_navigation_search(
            dnid(3),
            FocusDirection::Down,
            &SpatialNavigationSearchOptions::default()
        ),
        None,
    );
    assert_eq!(
        lw.spatial_navigation_search(
            dnid(3),
            FocusDirection::Down,
            &SpatialNavigationSearchOptions {
                candidates: OptionDomNodeIdVec::None,
                container: OptionDomNodeId::Some(dnid(0)),
            }
        ),
        Some(dnid(6)),
    );
    assert_eq!(
        lw.spatial_navigation_search(
            dnid(3),
            FocusDirection::Down,
            &SpatialNavigationSearchOptions {
                candidates: OptionDomNodeIdVec::Some(vec![dnid(5), dnid(4)].into()),
                container: OptionDomNodeId::None,
            }
        ),
        Some(dnid(4)),
    );
}

/// At the bottom of the list (offset 80, its maximum) nothing below `a3` is
/// left to scroll to: `navnotarget`, then the document, where `outside` is.
#[test]
fn at_the_bottom_of_a_scrolled_list_an_arrow_leaves_it() {
    let mut lw = scroll_list();
    scroll_list_to(&mut lw, 80.0);
    assert_eq!(live_down_from(&lw, 5), DefaultAction::FocusDown);
    assert_eq!(
        lw.resolve_focus_target_live(
            &FocusTarget::Directional(FocusDirection::Down),
            Some(dnid(5))
        ),
        Ok(FocusResolution::Resolved(dnid(6))),
    );
}
