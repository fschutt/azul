//! `background-origin` (CSS Backgrounds 3 s3.8) picks the box a background
//! image is positioned and sized in: the padding box by default, the border
//! box or the content box when it says so. In the `background` shorthand a
//! layer's one `<visual-box>` sets its origin and its clip, and with two boxes
//! the first is the origin and the second the clip.

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalRect, LogicalSize},
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::{DisplayList, DisplayListItem},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const WIDTH: f32 = 300.0;
const HEIGHT: f32 = 200.0;

/// A `w` x `h` image of one opaque colour.
fn solid(w: usize, h: usize, rgba: [u8; 4]) -> ImageRef {
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(rgba.repeat(w * h).into()),
        width: w,
        height: h,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    })
    .expect("an RGBA8 image")
}

/// One div styled `css` at the window's top left, with `tile` (10 x 10 red)
/// and `wide` (20 x 10 blue) in the image cache: the window and its display
/// list.
fn laid_out(css: &str) -> (LayoutWindow, DisplayList) {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.image_cache
        .add_css_image_id("tile".into(), solid(10, 10, [255, 0, 0, 255]));
    lw.image_cache
        .add_css_image_id("wide".into(), solid(20, 10, [0, 0, 255, 255]));
    let dom = Dom::create_body()
        .with_css("margin: 0px; padding: 0px; background: #ffffff;")
        .with_child(Dom::create_div().with_css(css));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the div lays out");
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .as_ref()
        .clone();
    (lw, dl)
}

/// `(x, y, width, height)` of every image item.
fn images(dl: &DisplayList) -> Vec<(f32, f32, f32, f32)> {
    dl.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Image { bounds, .. } => Some(xywh(bounds.inner())),
            _ => None,
        })
        .collect()
}

fn xywh(r: &LogicalRect) -> (f32, f32, f32, f32) {
    (r.origin.x, r.origin.y, r.size.width, r.size.height)
}

fn close(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.01
        && (a.1 - b.1).abs() < 0.01
        && (a.2 - b.2).abs() < 0.01
        && (a.3 - b.3).abs() < 0.01
}

/// A div with a 5px border and 10px padding: its border box at (0, 0), its
/// padding box at (5, 5), its content box at (15, 15).
const BOXED: &str = "width: 100px; height: 60px; border: 5px solid transparent; padding: 10px; \
                     background-image: url(tile); background-repeat: no-repeat;";

fn only_image(css: &str) -> (f32, f32, f32, f32) {
    let (_, dl) = laid_out(css);
    let found = images(&dl);
    assert_eq!(found.len(), 1, "one image: {found:?}");
    found[0]
}

#[test]
fn an_image_is_positioned_in_the_padding_box_by_default() {
    let at = only_image(BOXED);
    assert!(close(at, (5.0, 5.0, 10.0, 10.0)), "{at:?}");
}

#[test]
fn background_origin_border_box_positions_the_image_at_the_border_edge() {
    let at = only_image(&format!("{BOXED} background-origin: border-box;"));
    assert!(close(at, (0.0, 0.0, 10.0, 10.0)), "{at:?}");
}

#[test]
fn background_origin_content_box_positions_the_image_at_the_content_edge() {
    let at = only_image(&format!("{BOXED} background-origin: content-box;"));
    assert!(close(at, (15.0, 15.0, 10.0, 10.0)), "{at:?}");
}

#[test]
fn a_percentage_position_is_of_the_origin_box() {
    // 100% 100% puts the image's bottom right on the content box's.
    let at = only_image(&format!(
        "{BOXED} background-origin: content-box; background-position: 100% 100%;"
    ));
    assert!(close(at, (105.0, 65.0, 10.0, 10.0)), "{at:?}");
}

#[test]
fn one_box_in_the_shorthand_is_the_origin_and_the_clip() {
    let at = only_image(
        "width: 100px; height: 60px; border: 5px solid transparent; padding: 10px; \
         background: url(tile) content-box; background-repeat: no-repeat;",
    );
    assert!(close(at, (15.0, 15.0, 10.0, 10.0)), "{at:?}");
}

#[test]
fn of_two_boxes_in_the_shorthand_the_first_is_the_origin() {
    let at = only_image(
        "width: 100px; height: 60px; border: 5px solid transparent; padding: 10px; \
         background: url(tile) border-box padding-box; background-repeat: no-repeat;",
    );
    assert!(close(at, (0.0, 0.0, 10.0, 10.0)), "{at:?}");
}
