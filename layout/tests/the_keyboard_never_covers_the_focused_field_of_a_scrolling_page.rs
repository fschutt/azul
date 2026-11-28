//! The on-screen keyboard never covers the focused field of a scrolling page.
//!
//! The keyboard overlays the window and is no part of any scroll container, so a field in the
//! lower half of a scrolling page counted as visible while it was under the keyboard, and nothing
//! scrolled it into sight (a phone's alarm label, a note, a mail's body: every typing app). The
//! shell hands the insets over through `LayoutWindow::set_safe_area_insets`, which tells the
//! scroll manager where the keyboard's top edge is, and a keyboard that comes up (or changes its
//! height) with a node focused reveals that node above it.
//!
//! Every page is a phone's 400x300 window without margins; the field is 40 px tall.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_css::{
    props::basic::pixel::{OptionPixelValue, PixelValue},
    system::SafeAreaInsets,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, managers::scroll_into_view::ScrollIntoViewOptions,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A window with the page laid out: body(0) > [filler(1), field(2), tail(3)] with the field at
/// `field_top` px from the page's top and a tail that makes the page scroll.
fn phone_with_a_field_at(field_top: f32) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let dom = Dom::create_body()
        .with_css("margin: 0;")
        .with_child(Dom::create_div().with_css(format!("height: {field_top}px;").as_str()))
        .with_child(Dom::create_div().with_css("height: 40px;"))
        .with_child(Dom::create_div().with_css("height: 1000px;"));
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

fn now() -> Instant {
    Instant::from(std::time::Instant::now())
}

fn field() -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
    }
}

fn keyboard(px: f32) -> SafeAreaInsets {
    SafeAreaInsets {
        keyboard: OptionPixelValue::Some(PixelValue::px(px)),
        ..SafeAreaInsets::default()
    }
}

fn page_offset(lw: &LayoutWindow) -> f32 {
    lw.scroll_manager
        .get_current_offset(DomId::ROOT_ID, NodeId::new(0))
        .map_or(0.0, |o| o.y)
}

/// Where the field's bottom edge is on the screen now (the page is the viewport's scroller).
fn field_bottom_on_screen(lw: &LayoutWindow, field_top: f32) -> f32 {
    field_top + 40.0 - page_offset(lw)
}

/// A field at y = 200..240 in a 300 px window is on the screen. The keyboard is 120 px: it covers
/// y >= 180, and the field has to be scrolled up by at least 60 px to end above it.
///
/// Mutation: the shell's insets stored with no word to the scroll manager (the state before this
/// change): the field is "visible" at y = 240 and the page stays where it is.
#[test]
fn a_keyboard_that_comes_up_scrolls_the_focused_field_above_it() {
    let mut lw = phone_with_a_field_at(200.0);
    lw.focus_manager.set_focused_node(Some(field()));
    assert_eq!(page_offset(&lw), 0.0, "harness: nothing scrolled yet");

    let moved = lw.set_safe_area_insets(keyboard(120.0), now());

    assert!(moved, "a scroll container moved");
    let bottom = field_bottom_on_screen(&lw, 200.0);
    assert!(
        bottom <= 180.0 + 0.5,
        "the field's bottom is at y = {bottom}, under the keyboard that starts at y = 180"
    );
}

/// Nothing is focused: a keyboard that comes up moves nothing (the keyboard of a system dialog, a
/// focus that is not the page's).
#[test]
fn a_keyboard_with_nothing_focused_scrolls_nothing() {
    let mut lw = phone_with_a_field_at(200.0);
    assert!(!lw.set_safe_area_insets(keyboard(120.0), now()));
    assert_eq!(page_offset(&lw), 0.0);
}

/// A field that already ends above the keyboard is where it should be: no scroll.
#[test]
fn a_field_above_the_keyboard_stays_where_it_is() {
    let mut lw = phone_with_a_field_at(20.0);
    lw.focus_manager.set_focused_node(Some(field()));
    assert!(!lw.set_safe_area_insets(keyboard(120.0), now()));
    assert_eq!(page_offset(&lw), 0.0);
}

/// A keyboard that grows (a suggestion bar, another layout) moves the field again, and one that
/// goes away asks for no scroll: the page is left where the person put it.
#[test]
fn a_taller_keyboard_scrolls_further_and_a_keyboard_that_leaves_scrolls_nothing() {
    let mut lw = phone_with_a_field_at(200.0);
    lw.focus_manager.set_focused_node(Some(field()));
    assert!(lw.set_safe_area_insets(keyboard(120.0), now()));
    let first = page_offset(&lw);

    assert!(lw.set_safe_area_insets(keyboard(200.0), now()), "100 px covered now: the field must move up by 40 px more");
    let second = page_offset(&lw);
    assert!(second > first + 30.0, "the page scrolled further ({first} -> {second})");
    assert!(field_bottom_on_screen(&lw, 200.0) <= 100.0 + 0.5, "above y = 100");

    assert!(!lw.set_safe_area_insets(SafeAreaInsets::default(), now()), "a keyboard that leaves asks for no scroll");
    assert_eq!(page_offset(&lw), second, "the page stays where it is");
    assert_eq!(lw.scroll_manager.covered_below(), None, "and nothing is covered any more");
}

/// The same inset reported again (the keyboard's animation sends one per frame, and the Java side
/// hands the same numbers over twice) is no new reveal.
#[test]
fn the_same_keyboard_reported_again_scrolls_nothing_more() {
    let mut lw = phone_with_a_field_at(200.0);
    lw.focus_manager.set_focused_node(Some(field()));
    assert!(lw.set_safe_area_insets(keyboard(120.0), now()));
    let once = page_offset(&lw);
    assert!(!lw.set_safe_area_insets(keyboard(120.0), now()));
    assert_eq!(page_offset(&lw), once);
}

/// A field focused while the keyboard is ALREADY up (tab to the next field of a form): the focus
/// path's own reveal aims above the keyboard too, with no word from the shell.
#[test]
fn a_reveal_with_the_keyboard_up_aims_above_it() {
    let mut lw = phone_with_a_field_at(200.0);
    lw.set_safe_area_insets(keyboard(120.0), now());
    lw.scroll_node_into_view(field(), ScrollIntoViewOptions::nearest(), now());
    assert!(field_bottom_on_screen(&lw, 200.0) <= 180.0 + 0.5);
}

/// No keyboard (a desktop, a phone before the keyboard): a reveal is what it always was.
#[test]
fn without_a_keyboard_a_visible_field_is_not_scrolled() {
    let mut lw = phone_with_a_field_at(200.0);
    let adjustments = lw.scroll_node_into_view(field(), ScrollIntoViewOptions::nearest(), now());
    assert!(adjustments.is_empty());
    assert_eq!(page_offset(&lw), 0.0);
}
