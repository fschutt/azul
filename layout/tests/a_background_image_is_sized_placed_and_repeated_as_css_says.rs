//! A background image is sized, placed and repeated the way CSS Backgrounds 3
//! says, on the CPU and on the GPU alike (both draw the display list's
//! tiles).
//!
//! The image takes `background-size` (its own size when none is given,
//! `contain` / `cover` keep its proportions, lengths and percentages of the
//! positioning area), sits where `background-position` puts it inside the
//! padding box (a percentage aligns that point of the image with the same
//! point of the box), and repeats from there over the painting area per
//! `background-repeat`, cut to the painting area. A gradient is an image
//! too: with a size it is tiled the same way. Without a size, position or
//! intrinsic size (a gradient, a material) a layer keeps filling its
//! painting area as one item.

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalRect, LogicalSize},
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions},
    glyph_cache::GlyphCache,
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

/// `(x, y, width, height)` of every linear-gradient item.
fn gradients(dl: &DisplayList) -> Vec<(f32, f32, f32, f32)> {
    dl.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::LinearGradient { bounds, .. } => Some(xywh(bounds.inner())),
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

#[test]
fn a_sized_image_that_does_not_repeat_sits_at_its_position_inside_the_padding_box() {
    let (_, dl) = laid_out(
        "width: 200px; height: 100px; border: 5px solid transparent; background: url(tile); \
         background-size: 40px 20px; background-position: 10px 5px; \
         background-repeat: no-repeat;",
    );
    let found = images(&dl);
    assert_eq!(found.len(), 1, "one image, not repeated: {found:?}");
    assert!(
        close(found[0], (15.0, 10.0, 40.0, 20.0)),
        "40 x 20, placed 10px / 5px into the padding box (whose corner is the border's \
         5px in): {found:?}"
    );
}

#[test]
fn an_image_without_a_size_repeats_at_its_own_size_from_the_padding_box() {
    let (_, dl) = laid_out("width: 30px; height: 20px; background: url(tile);");
    let mut found = images(&dl);
    found.sort_by(|a, b| (a.1, a.0).partial_cmp(&(b.1, b.0)).expect("finite"));
    let want = [
        (0.0, 0.0),
        (10.0, 0.0),
        (20.0, 0.0),
        (0.0, 10.0),
        (10.0, 10.0),
        (20.0, 10.0),
    ];
    assert_eq!(found.len(), want.len(), "a 10px image tiles 30 x 20: {found:?}");
    for (got, (x, y)) in found.iter().zip(want) {
        assert!(close(*got, (x, y, 10.0, 10.0)), "{found:?}");
    }
}

#[test]
fn contain_fits_the_image_inside_the_box_and_keeps_its_proportions() {
    let (_, dl) = laid_out(
        "width: 100px; height: 100px; background: url(wide); background-size: contain; \
         background-position: center; background-repeat: no-repeat;",
    );
    let found = images(&dl);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        close(found[0], (0.0, 25.0, 100.0, 50.0)),
        "a 2:1 image as wide as the square box, centred: {found:?}"
    );
}

#[test]
fn cover_fills_the_box_and_keeps_its_proportions() {
    let (_, dl) = laid_out(
        "width: 100px; height: 100px; background: url(wide); background-size: cover; \
         background-position: center; background-repeat: no-repeat;",
    );
    let found = images(&dl);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        close(found[0], (-50.0, 0.0, 200.0, 100.0)),
        "a 2:1 image as tall as the square box, centred: {found:?}"
    );
    let clipped = dl.items.iter().any(|item| match item {
        DisplayListItem::PushClip { bounds, .. } => {
            close(xywh(bounds.inner()), (0.0, 0.0, 100.0, 100.0))
        }
        _ => false,
    });
    assert!(clipped, "the overhanging image is cut to the box");
}

#[test]
fn a_percentage_position_aligns_that_point_of_the_image_with_that_point_of_the_box() {
    let (_, dl) = laid_out(
        "width: 100px; height: 50px; background: url(wide); \
         background-position: 100% 100%; background-repeat: no-repeat;",
    );
    let found = images(&dl);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        close(found[0], (80.0, 40.0, 20.0, 10.0)),
        "the image's bottom right corner on the box's: {found:?}"
    );
}

#[test]
fn a_sized_gradient_is_tiled_like_an_image() {
    // The WPT reference of `background-margin-root` draws its root's
    // gradient this way.
    let (_, dl) = laid_out(
        "width: 100px; height: 60px; background: linear-gradient(red, blue); \
         background-size: 30px 30px; background-position: 5px 5px;",
    );
    let found = gradients(&dl);
    assert_eq!(
        found.len(),
        15,
        "5 columns (from -25px) by 3 rows (from -25px) of 30px tiles: {found:?}"
    );
    assert!(found.iter().any(|g| close(*g, (5.0, 5.0, 30.0, 30.0))), "{found:?}");
    assert!(found.iter().all(|g| (g.2 - 30.0).abs() < 0.01 && (g.3 - 30.0).abs() < 0.01));
}

#[test]
fn a_gradient_without_a_size_still_fills_its_painting_area_as_one_item() {
    let (_, dl) = laid_out(
        "width: 100px; height: 60px; border: 5px solid #000000; \
         background: linear-gradient(red, blue);",
    );
    let found = gradients(&dl);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(close(found[0], (0.0, 0.0, 110.0, 70.0)), "{found:?}");
}

#[test]
fn the_cpu_draws_a_placed_image_where_the_display_list_puts_it() {
    let (lw, dl) = laid_out(
        "width: 60px; height: 40px; background: url(tile); background-size: 20px 20px; \
         background-position: 30px 10px; background-repeat: no-repeat;",
    );
    let mut cache = GlyphCache::new();
    let pixmap = cpurender::render_with_font_manager(
        &dl,
        &RendererResources::default(),
        &lw.font_manager,
        RenderOptions {
            width: WIDTH,
            height: HEIGHT,
            dpi_factor: 1.0,
        },
        &mut cache,
    )
    .expect("the div renders on the CPU");
    let px = |x: u32, y: u32| {
        let i = ((y * pixmap.width + x) * 4) as usize;
        let d = pixmap.data();
        [d[i], d[i + 1], d[i + 2]]
    };
    assert_eq!(px(40, 20), [255, 0, 0], "inside the placed image");
    assert_eq!(px(5, 5), [255, 255, 255], "left of it, the page");
    assert_eq!(px(55, 35), [255, 255, 255], "below it, the page");
}

/// One `background-size` value is the width; the height is `auto` and keeps
/// the image's proportions (CSS Backgrounds 3 s3.9, as Chrome draws it).
#[test]
fn one_size_value_is_the_width_and_the_height_keeps_the_images_proportions() {
    let (_, dl) = laid_out(
        "width: 100px; height: 100px; background: url(wide); background-size: 50%; \
         background-repeat: no-repeat;",
    );
    let found = images(&dl);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        close(found[0], (0.0, 0.0, 50.0, 25.0)),
        "half the box wide, the 2:1 image's height from that: {found:?}"
    );
}

/// `auto` on one axis follows the other through the image's proportions;
/// `auto` alone is the image's own size.
#[test]
fn an_auto_size_follows_the_image_from_the_other_axis() {
    let (_, dl) = laid_out(
        "width: 100px; height: 100px; background: url(wide); background-size: auto 40px; \
         background-repeat: no-repeat;",
    );
    let found = images(&dl);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        close(found[0], (0.0, 0.0, 80.0, 40.0)),
        "40px tall, twice as wide: {found:?}"
    );
    let (_, dl) = laid_out(
        "width: 100px; height: 100px; background: url(wide); background-size: auto; \
         background-repeat: no-repeat;",
    );
    let found = images(&dl);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(close(found[0], (0.0, 0.0, 20.0, 10.0)), "its own size: {found:?}");
}

