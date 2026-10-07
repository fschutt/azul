//! An SVG `<image>` draws its picture where its attributes put it.
//!
//! The builtin renderer keeps `x` / `y` / `width` / `height` / `href` /
//! `transform` on the `SvgImage` node; paint maps the rectangle through the
//! transforms and the `<svg>`'s viewBox and fits the picture in (SVG's default
//! `xMidYMid meet`). A `data:` URI is decoded once. Before, an `<image>` was a
//! `<div>` and drew nothing (a PDF page's pictures were missing).

use azul_core::{dom::DomId, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A 2 x 1 picture, both pixels opaque red.
const RED_2X1: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8DwH4QBEfcD/ePF9e8AAAAASUVORK5CYII=";

/// `(x, y, width, height)` and the picture's size of every image the
/// `<svg>` content (`svg_px` square, viewBox 64 x 64) paints.
fn images(svg_px: u32, content: &str) -> Vec<((f32, f32, f32, f32), (f32, f32))> {
    let markup = format!(
        "<html><body style=\"margin: 0px\"><svg width=\"{svg_px}\" height=\"{svg_px}\" \
         viewBox=\"0 0 64 64\">{content}</svg></body></html>"
    );
    let parsed = azul_layout::xml::parse_xml(&markup).expect("the markup parses");
    let mut dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(256.0, 256.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .unwrap();
    lw.get_layout_result(&DomId::ROOT_ID)
        .unwrap()
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Image { bounds, image, .. } => {
                let r = bounds.inner();
                let size = image.get_size();
                Some((
                    (r.origin.x, r.origin.y, r.size.width, r.size.height),
                    (size.width, size.height),
                ))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn an_image_fills_its_rectangle() {
    let drawn = images(64, &format!(r#"<image x="10" y="20" width="30" height="15" href="{RED_2X1}"/>"#));
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    assert_eq!(drawn[0].0, (10.0, 20.0, 30.0, 15.0));
}

#[test]
fn an_image_moves_with_its_transform_and_scales_with_its_svg() {
    let drawn = images(
        128,
        &format!(
            r#"<g transform="translate(5 5)"><image x="0" y="0" width="20" height="10" href="{RED_2X1}"/></g>"#
        ),
    );
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    assert_eq!(drawn[0].0, (10.0, 10.0, 40.0, 20.0), "at (5, 5) user units, at twice the viewBox");
}

/// With a PNG decoder built in, the `data:` URI is the picture, and a 2:1
/// picture in a square keeps its proportions, centred.
#[cfg(feature = "png")]
#[test]
fn an_embedded_picture_is_decoded_and_keeps_its_proportions() {
    let drawn = images(64, &format!(r#"<image x="10" y="20" width="30" height="30" href="{RED_2X1}"/>"#));
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    let ((x, y, w, h), size) = drawn[0];
    assert_eq!(size, (2.0, 1.0), "the decoded picture");
    assert_eq!((x, w, h), (10.0, 30.0, 15.0));
    assert!((y - 27.5).abs() < 0.01, "centred: y {y}");
}
