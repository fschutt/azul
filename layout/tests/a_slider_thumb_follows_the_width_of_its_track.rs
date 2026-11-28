//! A slider an app gives another width than the design 200 px has its thumb at the value's fraction of that width.
//!
//! The track is 200 px wide and the thumb sat at `fraction * 184` px: a number worked out from the design width,
//! written into the DOM. An app that stretched the track (a page scrubber under a phone's page is as wide as the
//! row it is in: AzReader) had the thumb at that fraction of 200 px, so the thumb stopped at two thirds of the
//! rail of a 300 px track until the first touch moved it. The thumb's `margin-left` is a percentage of the room it
//! has (the track's width less the thumb's) now, so it is at the right place at any width.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{slider::Slider, themes::UiTheme},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const TRACK: &str = "__azul-native-slider";
const THUMB: &str = "__azul-native-slider-thumb";
const THUMB_SIZE: f32 = 16.0;

/// `body > div(column, 300px) > slider`, the slider's own root given `track_css`, laid out at 640 x 480.
fn laid_out(theme: UiTheme, value: f32, track_css: &str) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = window_state.clone();
    let slider = Slider::create(value, 0.0, 100.0).with_theme(theme).dom().with_css(track_css);
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; width: 300px;")
            .with_child(slider),
    );
    let (css, _) = azul_css::parser2::new_from_str("body { margin: 0; }");
    let styled = StyledDom::create(&mut dom, css);
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .expect("layout");
    lw
}

/// The `(x, width)` of the node with `class`.
fn x_and_width(lw: &LayoutWindow, class: &str) -> (f32, f32) {
    let lr = lw.get_layout_result(&DomId::ROOT_ID).expect("a layout");
    let container = lr.styled_dom.node_data.as_container();
    (0..container.len())
        .map(NodeId::new)
        .find_map(|nid| {
            let has = container[nid].attributes().as_ref().iter().any(|a| {
                a.as_class().is_some_and(|c| {
                    let s: &str = c;
                    s == class
                })
            });
            if !has {
                return None;
            }
            let idx = *lr.layout_tree.dom_to_layout.get(&nid)?.first()?;
            let x = lr.calculated_positions.get(idx.index())?.x;
            let width = lr.layout_tree.get(idx)?.used_size?.width;
            Some((x, width))
        })
        .unwrap_or_else(|| panic!("a node with the class {class}"))
}

/// How far the thumb is from the left end of its track, and how far it can go.
fn thumb_offset_and_travel(lw: &LayoutWindow) -> (f32, f32) {
    let (track_x, track_width) = x_and_width(lw, TRACK);
    let (thumb_x, thumb_width) = x_and_width(lw, THUMB);
    assert!((thumb_width - THUMB_SIZE).abs() < 0.01, "the thumb keeps its {THUMB_SIZE} px: {thumb_width}");
    (thumb_x - track_x, track_width - THUMB_SIZE)
}

const THEMES: [UiTheme; 2] = [UiTheme::Flat, UiTheme::Flora];

#[test]
fn the_design_track_keeps_its_200_px_and_the_thumb_its_place_on_it() {
    for theme in THEMES {
        for (value, at) in [(0.0, 0.0), (50.0, 92.0), (100.0, 184.0)] {
            let lw = laid_out(theme, value, "");
            let (_, width) = x_and_width(&lw, TRACK);
            assert!((width - 200.0).abs() < 0.01, "{theme:?}: the track is {width} px");
            let (offset, _) = thumb_offset_and_travel(&lw);
            assert!((offset - at).abs() < 0.6, "{theme:?} value {value}: the thumb is {offset} px along, not {at}");
        }
    }
}

#[test]
fn a_track_that_fills_its_row_has_the_thumb_at_the_values_fraction_of_it() {
    for theme in THEMES {
        for value in [0.0_f32, 25.0, 50.0, 75.0, 100.0] {
            let lw = laid_out(theme, value, "width: 100%;");
            let (_, width) = x_and_width(&lw, TRACK);
            assert!((width - 300.0).abs() < 0.01, "{theme:?}: the track fills the 300 px row: {width}");
            let (offset, travel) = thumb_offset_and_travel(&lw);
            let expected = value / 100.0 * travel;
            assert!(
                (offset - expected).abs() < 0.6,
                "{theme:?} value {value}: the thumb is {offset} px along a track it can travel {travel} px on, not {expected}"
            );
        }
    }
}

#[test]
fn the_thumb_is_not_squeezed_at_the_right_end() {
    for theme in THEMES {
        let lw = laid_out(theme, 100.0, "width: 100%;");
        let (offset, travel) = thumb_offset_and_travel(&lw);
        assert!((offset - travel).abs() < 0.6, "{theme:?}: the thumb is at {offset}, the end is {travel}");
    }
}
