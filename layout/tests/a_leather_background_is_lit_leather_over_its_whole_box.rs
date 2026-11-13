//! A `builtin(leather)` background is rendered as lit leather for the box it
//! fills: the display list draws one frame of the box's own size, opaque, in
//! the leather's tint (a pale, warm nappa unless the node sets
//! `--azul-material-tint`), with the grain in it.
//!
//! The materials register themselves as callback images under their CSS
//! names, so this runs the whole path a widget's background takes: the CSS
//! parser, the image cache, the background-layer resolver and the shader.

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalRect, LogicalSize},
    resources::{ImageRef, RawImageData, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Lays out one 160 x 48 div with `css` in a 400 x 300 window and prepares
/// a frame, which renders the background layers' callback images.
fn lay_out(css: &str) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let dom = Dom::create_body().with_child(Dom::create_div().with_css(css));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the div lays out");
    lw.prepare_frame_content();
    lw
}

/// Every image item of the root display list: its rect and its image.
fn image_items(lw: &LayoutWindow) -> Vec<(LogicalRect, ImageRef)> {
    let lr = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    lr.display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Image { bounds, image, .. } => Some((*bounds.inner(), image.clone())),
            _ => None,
        })
        .collect()
}

/// The frame's pixels as straight RGBA, whatever channel order it is
/// stored in.
fn rgba_pixels(image: &ImageRef) -> Vec<[u8; 4]> {
    let raw = image.get_rawimage().expect("a rendered frame, not a callback");
    let RawImageData::U8(bytes) = &raw.pixels else {
        panic!("an 8-bit frame");
    };
    bytes
        .as_ref()
        .chunks_exact(4)
        .map(|p| match raw.data_format {
            RawImageFormat::BGRA8 => [p[2], p[1], p[0], p[3]],
            _ => [p[0], p[1], p[2], p[3]],
        })
        .collect()
}

/// The mean of each channel and the spread of the luma (its standard
/// deviation) over `pixels`.
fn mean_and_grain(pixels: &[[u8; 4]]) -> ([f32; 4], f32) {
    let n = pixels.len() as f32;
    let mut mean = [0.0_f32; 4];
    for p in pixels {
        for c in 0..4 {
            mean[c] += f32::from(p[c]) / n;
        }
    }
    let luma = |p: &[u8; 4]| 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]);
    let mean_luma = pixels.iter().map(luma).sum::<f32>() / n;
    let variance = pixels.iter().map(|p| (luma(p) - mean_luma).powi(2)).sum::<f32>() / n;
    (mean, variance.sqrt())
}

#[test]
fn a_leather_background_is_lit_leather_over_its_whole_box() {
    let lw = lay_out("width: 160px; height: 48px; background: builtin(leather);");
    let items = image_items(&lw);
    assert_eq!(items.len(), 1, "the div's one background layer: {items:?}");
    let (rect, image) = &items[0];
    assert_eq!((rect.size.width, rect.size.height), (160.0, 48.0));
    assert!(!image.is_callback(), "the display list draws the rendered frame");
    assert_eq!(image.get_size(), LogicalSize::new(160.0, 48.0), "a frame of the box's own size");

    let pixels = rgba_pixels(image);
    assert!(
        pixels.iter().all(|p| p[3] == 255),
        "a square box is covered to its edges"
    );
    let (mean, grain) = mean_and_grain(&pixels);
    assert!(
        mean[0] >= mean[1] && mean[1] >= mean[2] && mean[0] - mean[2] < 30.0,
        "nappa is a pale warm grey: red over green over blue, close together, got {mean:?}"
    );
    assert!(grain > 1.0, "the grain shows, not a flat fill (luma spread {grain})");
}

/// The luma of a mean colour.
fn luma(mean: [f32; 4]) -> f32 {
    0.2126 * mean[0] + 0.7152 * mean[1] + 0.0722 * mean[2]
}

#[test]
fn a_tinted_leather_keeps_its_grain_in_the_tint_colour() {
    let white = lay_out(
        "width: 160px; height: 48px; --azul-material-tint: #f4f0e8; background: builtin(leather);",
    );
    let items = image_items(&white);
    assert_eq!(items.len(), 1);
    let (mean, grain) = mean_and_grain(&rgba_pixels(&items[0].1));
    assert!(luma(mean) > 170.0, "white leather is light, got {mean:?}");
    assert!(mean[0] >= mean[2], "and as warm as its tint, got {mean:?}");
    // The design's nappa grain is fine and shallow: a pale tint shows it as
    // little as the default nappa does.
    assert!(grain > 1.0, "with the leather's grain in it (luma spread {grain})");

    let dark = lay_out(
        "width: 160px; height: 48px; --azul-material-tint: #3a322c; background: builtin(leather);",
    );
    let items = image_items(&dark);
    assert_eq!(items.len(), 1);
    let (mean, grain) = mean_and_grain(&rgba_pixels(&items[0].1));
    assert!(luma(mean) < 90.0, "dark leather is dark, got {mean:?}");
    // A dark tint shows the same grain in less light.
    assert!(grain > 0.5, "and grained too (luma spread {grain})");
}
