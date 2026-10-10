//! Text over a layer is not stamped on its ancestor's colour.
//!
//! The display list hands a text run a "uniform background" when an
//! ancestor paints an opaque colour (`compute_uniform_text_bg`), and the
//! raster then stamps pre-blended glyph tiles over that colour. But the
//! ancestor's colour is only the text's backdrop when nothing paints between
//! the two: AzPlayer's black body under a positioned blue gradient and its
//! light rays gave every glyph of the start strip a BLACK box (user,
//! 2026-10-08). The raster has the real backdrop in its pixels.

use azul_core::dom::Dom;
use azul_css::{css::Css, props::basic::color::ColorU};
use azul_layout::cpurender::render_dom_to_rgba;

const W: f32 = 240.0;
const H: f32 = 80.0;

#[test]
fn text_over_a_positioned_layer_keeps_the_layers_colour_around_its_glyphs() {
    let dom = Dom::create_body()
        .with_css("margin: 0px; width: 240px; height: 80px; background: #000000;")
        .with_child(Dom::create_div().with_css(
            "position: absolute; left: 0px; top: 0px; width: 240px; height: 80px; \
             background: linear-gradient(to bottom, #2050a0, #2050a0);",
        ))
        .with_child(
            Dom::create_p_with_text("music")
                .with_css(
                    "position: absolute; left: 10px; top: 10px; margin: 0px; font-size: 38px; \
                     color: #ffffff;",
                ),
        );
    let r = render_dom_to_rgba(dom, Css::empty(), W, H, 1.0, ColorU::WHITE).expect("renders");
    let w = r.pixel_width as usize;
    // Every pixel of the text's line box is the layer's blue, white glyph
    // ink, or a blend of the two - never black (a blend toward black would be
    // the ancestor's colour baked under the glyphs).
    let mut black = 0usize;
    for y in 12..56usize {
        for x in 12..120usize {
            let p = &r.rgba[(y * w + x) * 4..(y * w + x) * 4 + 4];
            let (rr, gg, bb) = (u32::from(p[0]), u32::from(p[1]), u32::from(p[2]));
            if rr + gg + bb < 60 {
                black += 1;
            }
        }
    }
    assert_eq!(black, 0, "{black} near-black pixels in the text's box over a blue layer");
}
