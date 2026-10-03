//! A thin scrollbar is pressed where it is painted - buttons, thumb and all.
//!
//! Paint used the style's `scroll_button_size_px` for a classic bar's arrow
//! buttons while the hit test measured buttons of the bar's own thickness,
//! so author CSS that narrowed a classic bar (`scrollbar-width: thin`) drew
//! 12px buttons on an 8px bar and was pressed on 8px ones: a press on the
//! lower part of a painted button reached the track. The scrollbar-presence
//! work (af9d720c7) made paint, the GPU thumb and the hit test read one
//! `ScrollbarPresence`; this guard pins it for the thin bar.
//!
//! The page: a 200x100 `overflow-y: scroll; scrollbar-width: thin` box over
//! 400px, unscrolled, in a 400x300 window.

use azul_core::{
    dom::{Dom, DomId, NodeId, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollbarHitId,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::scroll_state::ScrollbarComponent,
    solver3::display_list::{DisplayListItem, ScrollbarDrawInfo},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body(0) > box(1) > content(2).
const BOX: NodeId = NodeId::new(1);

fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let page = Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div()
            .with_css(
                "width: 200px; height: 100px; overflow-y: scroll; scrollbar-width: thin;",
            )
            .with_child(Dom::create_div().with_css("height: 400px;")),
    );
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

fn painted_bar(lw: &LayoutWindow) -> ScrollbarDrawInfo {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out");
    lr.display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::ScrollBarStyled { info }
                if info.hit_id == Some(ScrollbarHitId::VerticalThumb(DomId::ROOT_ID, BOX)) =>
            {
                Some((**info).clone())
            }
            _ => None,
        })
        .expect("the box paints a vertical bar")
}

fn centre(r: LogicalRect) -> LogicalPosition {
    LogicalPosition::new(
        r.origin.x + r.size.width / 2.0,
        r.origin.y + r.size.height / 2.0,
    )
}

fn pressed(lw: &LayoutWindow, at: LogicalPosition) -> Option<ScrollbarComponent> {
    lw.scroll_manager
        .hit_test_scrollbars(at)
        .filter(|hit| hit.dom_id == DomId::ROOT_ID && hit.node_id == BOX)
        .map(|hit| hit.component)
}

#[test]
fn a_thin_scrollbar_is_pressed_where_it_is_painted() {
    let lw = window();
    let painted = painted_bar(&lw);
    let track = *painted.track_bounds.inner();
    assert!(
        (track.size.width - 8.0).abs() < 0.01,
        "harness: `scrollbar-width: thin` paints an 8px bar, got {track:?}"
    );
    let measured = lw
        .scroll_manager
        .get_scrollbar_state(DomId::ROOT_ID, BOX, ScrollbarOrientation::Vertical)
        .expect("the box has a bar to press");
    let same = |a: LogicalRect, b: LogicalRect| {
        (a.origin.x - b.origin.x).abs() < 0.01
            && (a.origin.y - b.origin.y).abs() < 0.01
            && (a.size.width - b.size.width).abs() < 0.01
            && (a.size.height - b.size.height).abs() < 0.01
    };
    assert!(
        same(track, measured.track_rect),
        "the track is pressed where it is painted: painted {track:?}, measured {:?}",
        measured.track_rect
    );

    // The thumb (unscrolled: its transform is the identity).
    let thumb = *painted.thumb_bounds.inner();
    assert_eq!(
        pressed(&lw, centre(thumb)),
        Some(ScrollbarComponent::Thumb),
        "a press on the painted thumb {thumb:?} grabs it"
    );

    // The arrow buttons, where the bar has them - painted and measured
    // square, the bar's own thickness.
    for (button, component) in [
        (painted.button_decrement_bounds, ScrollbarComponent::TopButton),
        (painted.button_increment_bounds, ScrollbarComponent::BottomButton),
    ] {
        let Some(button) = button else { continue };
        let button = *button.inner();
        assert!(
            (button.size.height - measured.button_size).abs() < 0.01,
            "the painted button {button:?} is as tall as the pressed one ({})",
            measured.button_size
        );
        // Near its far edge from the track, where a larger painted button
        // used to overhang the measured one.
        let near_the_end = LogicalPosition::new(
            button.origin.x + button.size.width / 2.0,
            match component {
                ScrollbarComponent::TopButton => button.origin.y + 1.0,
                _ => button.origin.y + button.size.height - 1.0,
            },
        );
        assert_eq!(pressed(&lw, near_the_end), Some(component));
        assert_eq!(pressed(&lw, centre(button)), Some(component));
    }
}
