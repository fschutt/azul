//! A map's line lies on the ground and turns and tilts with the tiles; a
//! marker stands upright over the ground. On a turned and tilted map the two
//! must still meet: the marker stands on the screen point where the ground
//! shows its place. Both are laid out and painted by the CPU renderer here,
//! so the camera that tilts the line is the engine's own.

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::basic::color::ColorU;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender,
    widgets::map::{
        MapLatLon, MapLatLonVec, MapMarker, MapOverlay, MapOverlayVec, MapPolyline, MapTileLayer,
        MapViewport, MapWidget,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: usize = 640;
const H: usize = 480;

/// The map's middle (Berlin) and the place the marker and the line mark, 70 px
/// east and 77 px north of it at zoom 14 on the flat map.
const CENTRE: (f64, f64) = (52.5163, 13.3777);
const PLACE: (f64, f64) = (52.5203, 13.3837);

fn viewport(bearing: f32, pitch: f32) -> MapViewport {
    MapViewport {
        centre_lat_deg: CENTRE.0,
        centre_lon_deg: CENTRE.1,
        zoom: 14.0,
        bearing_deg: bearing,
        pitch_deg: pitch,
    }
}

fn at(p: (f64, f64)) -> MapLatLon {
    MapLatLon {
        lat_deg: p.0,
        lon_deg: p.1,
    }
}

/// A red dot on the ground at the place: a line half a metre long, 14 px
/// wide with round caps.
fn ground_dot() -> MapOverlay {
    MapOverlay::Polyline(
        MapPolyline::create(MapLatLonVec::from_vec(vec![
            at(PLACE),
            at((PLACE.0, PLACE.1 + 0.000_005)),
        ]))
        .with_color(ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        })
        .with_width(14.0),
    )
}

/// A green 14 px square standing on the place, its centre on it.
fn standing_square() -> MapOverlay {
    let icon = Dom::create_div().with_css("width: 14px; height: 14px; background: #00c800;");
    MapOverlay::Marker(MapMarker::create(1, at(PLACE), icon).with_anchor(7.0, 7.0))
}

/// The window's pixels with a 640 x 480 map at `viewport` showing `overlay`.
fn paint(viewport: MapViewport, overlay: MapOverlay) -> Vec<u8> {
    let map = MapWidget::create(MapTileLayer::default())
        .with_viewport(viewport)
        .with_overlays(MapOverlayVec::from_vec(vec![overlay]))
        .dom();
    let mut dom = Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(
            Dom::create_div()
                .with_css("position: relative; width: 640px; height: 480px; overflow: hidden;")
                .with_child(map),
        );
    let (css, _) = azul_css::parser2::new_from_str("");
    let styled = StyledDom::create(&mut dom, css);
    let mut window = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut state = FullWindowState::default();
    state.size.dimensions = LogicalSize::new(W as f32, H as f32);
    let resources = RendererResources::default();
    window
        .layout_and_generate_display_list(
            styled,
            &state,
            &resources,
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the layout pass");

    let root = window.layout_results.get(&DomId::ROOT_ID).expect("the root's layout");
    let views: BTreeMap<_, _> = window
        .layout_results
        .iter()
        .filter(|(id, _)| **id != DomId::ROOT_ID)
        .map(|(id, r)| (*id, Arc::clone(&r.display_list)))
        .collect();
    let state = cpurender::CpuRenderState::new(Default::default())
        .with_virtual_view_display_lists(views);
    let mut glyphs = azul_layout::glyph_cache::GlyphCache::new();
    let mut compositor = cpurender::CompositorState::new(W as u32, H as u32);
    compositor.allocate_layers_from_display_list(
        &root.display_list,
        1.0,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
    );
    compositor
        .render_layers(
            &root.display_list,
            1.0,
            &resources,
            &window.font_manager,
            &mut glyphs,
            &state,
        )
        .expect("the render");
    let mut out = cpurender::AzulPixmap::new(W as u32, H as u32).expect("a pixmap");
    compositor.composite_frame(&mut out, 1.0);
    out.data().to_vec()
}

/// The centre of the pixels `is` picks, `None` when there are none.
fn centre_of(pixels: &[u8], is: impl Fn(u8, u8, u8) -> bool) -> Option<(f64, f64)> {
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
    for y in 0..H {
        for x in 0..W {
            let i = (y * W + x) * 4;
            if is(pixels[i], pixels[i + 1], pixels[i + 2]) {
                sx += x as f64 + 0.5;
                sy += y as f64 + 0.5;
                n += 1.0;
            }
        }
    }
    (n > 0.0).then(|| (sx / n, sy / n))
}

fn red(r: u8, g: u8, b: u8) -> bool {
    r > 200 && g < 60 && b < 60
}

fn green(r: u8, g: u8, b: u8) -> bool {
    g > 150 && r < 60 && b < 60
}

#[test]
fn a_marker_stands_where_the_ground_shows_its_place_on_a_turned_and_tilted_map() {
    let flat = centre_of(&paint(viewport(0.0, 0.0), ground_dot()), red)
        .expect("the line is painted on the flat map");
    assert!(
        (flat.0 - 390.0).abs() < 2.0 && (flat.1 - 163.0).abs() < 3.0,
        "on the flat map the place is 70 px east and 77 px north of the middle: {flat:?}"
    );

    let camera = viewport(35.0, 40.0);
    let ground = centre_of(&paint(camera, ground_dot()), red)
        .expect("the line is painted on the turned map");
    let marker = centre_of(&paint(camera, standing_square()), green)
        .expect("the marker is painted on the turned map");
    assert!(
        (ground.0 - flat.0).hypot(ground.1 - flat.1) > 20.0,
        "the camera moves the place on the screen: {flat:?} -> {ground:?}"
    );
    assert!(
        (marker.0 - ground.0).hypot(marker.1 - ground.1) < 2.0,
        "the marker stands at {marker:?}, the ground shows its place at {ground:?}"
    );
}
