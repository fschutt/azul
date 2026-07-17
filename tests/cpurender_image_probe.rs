//! Regression: cpurender must rasterize a `new_rawimage` `<img>` node — at both
//! native (1:1) and scaled sizes. This guards the decoded-video display path
//! (azul-video plays Big Buck Bunny by feeding decoded RGBA frames into `<img>`
//! nodes that the CPU renderer blits). A 0-height/blank regression here is what
//! made video appear not to play.

#![cfg(all(feature = "cpurender", feature = "text_layout", feature = "font_loading"))]

use azul_core::dom::{Dom, IdOrClass};
use azul_core::resources::{ImageRef, RawImage, RawImageData, RawImageFormat};
use azul_css::U8Vec;
use azul_layout::cpurender::{render_dom_to_image, AzulPixmap};

fn green(format: RawImageFormat, w: usize, h: usize) -> ImageRef {
    // (0,255,0,255) is green read as RGBA *or* BGRA (G is the middle byte either way).
    let px: Vec<u8> = (0..w * h).flat_map(|_| [0u8, 255, 0, 255]).collect();
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(U8Vec::from_vec(px)),
        width: w,
        height: h,
        premultiplied_alpha: false,
        data_format: format,
        tag: U8Vec::from_vec(Vec::new()),
    })
    .expect("new_rawimage")
}

fn img_dom(format: RawImageFormat) -> Dom {
    Dom::create_image(green(format, 100, 100))
        .with_ids_and_classes(vec![IdOrClass::Class("t".to_string().into())].into())
}

fn css(decls: &str) -> azul_css::css::Css {
    let (c, _) = azul_css::parser2::new_from_str(&format!(".t {{ {decls} }}"));
    c
}

/// Assert pixel (x,y) of an RGBA pixmap is green (G high, R+B low).
fn assert_green(pm: &AzulPixmap, x: u32, y: u32, ctx: &str) {
    let (w, _h) = (pm.width(), pm.height());
    let d = pm.data();
    let off = ((y * w + x) * 4) as usize;
    let (r, g, b) = (d[off], d[off + 1], d[off + 2]);
    assert!(
        g > 200 && r < 90 && b < 90,
        "{ctx}: expected green at ({x},{y}), got rgba=({r},{g},{b}) — cpurender did not blit the image",
    );
}

fn render(dom: Dom, style: azul_css::css::Css, w: f32, h: f32) -> AzulPixmap {
    let png = render_dom_to_image(dom, style, w, h, 1.0).expect("render_dom_to_image");
    AzulPixmap::decode_png(&png).expect("decode png")
}

#[test]
fn cpurender_blits_rawimage_native() {
    let pm = render(img_dom(RawImageFormat::RGBA8), css("width:100px;height:100px;"), 100.0, 100.0);
    assert_green(&pm, 50, 50, "rgba native");
    let pm = render(img_dom(RawImageFormat::BGRA8), css("width:100px;height:100px;"), 100.0, 100.0);
    assert_green(&pm, 50, 50, "bgra native");
}

#[test]
fn cpurender_blits_rawimage_scaled() {
    // 100x100 source downscaled into a 60x60 box at the origin — pixel (20,20) is
    // inside it. Guards image scaling (cf. #7 low-res images).
    let pm = render(img_dom(RawImageFormat::RGBA8), css("width:60px;height:60px;"), 100.0, 100.0);
    assert_green(&pm, 20, 20, "rgba scaled 60x60");
}
