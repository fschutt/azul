//! An SVG with absurd geometry renders in bounded time and memory, and the
//! part of it that is on screen renders exactly.
//!
//! Every SVG shape is painted through a clip mask the display list rasterises
//! with AGG, sized from the shape's box, and every stroke through a second
//! one. The inputs here are the kind a fuzzer feeds printpdf's HTML-to-PDF
//! path: a box a billion pixels wide (a mask whose byte count does not fit a
//! u32), NaN coordinates (a cubic the flattener subdivides 2^33 times),
//! geometry scaled 1e32 times (the same), edges past AGG's fixed-point range
//! (they saturate and bend), and a dash pattern a million times finer than
//! its line. Each render runs on a worker thread with a deadline, so a hang
//! fails the test instead of the run.

use azul_css::{css::Css, props::basic::color::ColorU};
use azul_layout::cpurender::{render_dom_to_rgba, ComponentPreviewResult};

const TRANSPARENT: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

/// The side of the rendered frame, in logical px.
const SIDE: f32 = 100.0;

/// Renders `markup` (an `<svg>` fragment, laid out as a block) into a
/// SIDE x SIDE frame on a worker thread, failing the test when it has not
/// finished after `seconds`.
fn render_within(seconds: u64, markup: &str) -> ComponentPreviewResult {
    let markup = format!(
        "<style>body {{ margin: 0; padding: 0; }} svg {{ display: block; }}</style>{markup}"
    );
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let parsed = azul_layout::xml::parse_xml(&markup).expect("the fixture parses");
        let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
        let _ = tx.send(render_dom_to_rgba(dom, Css::empty(), SIDE, SIDE, 1.0, TRANSPARENT));
    });
    rx.recv_timeout(std::time::Duration::from_secs(seconds))
        .expect("the render finishes in time (and does not panic)")
        .expect("the render succeeds")
}

fn render(markup: &str) -> ComponentPreviewResult {
    render_within(60, markup)
}

/// How many pixels of the frame are painted at all.
fn painted(r: &ComponentPreviewResult) -> usize {
    r.rgba.chunks_exact(4).filter(|p| p[3] > 0).count()
}

// The fuzzer's input: a box a billion pixels wide. Its clip mask asked for
// 2e9 x 110 pixels, 8.8e11 bytes of scratch, computed in u32.
#[test]
fn an_svg_a_billion_pixels_wide_rasterises_its_clip_mask_without_overflow() {
    let r = render(
        "<svg xs=\"vg\" width=\"1e9\" height=\"55\" vx=\"0N\">\
         <circle cx=\"1\" r=\"0.6\" fill=\"red\"/></svg>",
    );
    assert_eq!(r.rgba.len(), (r.pixel_width * r.pixel_height * 4) as usize);
}

#[test]
fn a_one_pixel_tall_svg_a_billion_pixels_wide_rasterises_its_clip_mask() {
    let r = render("<svg xs=\"vg\" width=\"1e9\" height=\"1\" x=\"0 \"><path d=\"M1 7Z\" s=\"d\"/></svg>");
    assert_eq!(r.rgba.len(), (r.pixel_width * r.pixel_height * 4) as usize);
}

#[test]
fn a_stroke_on_an_svg_a_billion_pixels_wide_rasterises_its_mask_without_overflow() {
    let r = render(
        "<svg width=\"1e9\" height=\"55\">\
         <line x1=\"0\" y1=\"20\" x2=\"90\" y2=\"20\" stroke=\"red\" stroke-width=\"4\"/></svg>",
    );
    assert!(painted(&r) > 0, "the stroke is still drawn");
}

// The mask of a large but sane SVG is capped in size, not dropped: the shape
// still paints, at a lower resolution.
#[test]
fn a_shape_on_an_svg_larger_than_the_mask_cap_still_paints() {
    let r = render(
        "<svg width=\"5000\" height=\"5000\">\
         <rect width=\"5000\" height=\"5000\" fill=\"red\"/></svg>",
    );
    assert_eq!(painted(&r), (r.pixel_width * r.pixel_height) as usize, "the frame is covered");
}

// NaN is not an SVG number: like any invalid geometry attribute it counts as
// not given (SVG 2, as Chrome renders it). Before, it reached the cubic
// flattener, which subdivided the rounded corner 2^33 times.
#[test]
fn a_rect_at_y_nan_renders_like_a_rect_without_y() {
    let shape = |y: &str| {
        format!(
            "<svg width=\"100\" height=\"100\"><rect x=\"-1\" {y} width=\"76\" height=\"24\" \
             fill=\"red\" rx=\"7\"/></svg>"
        )
    };
    let nan = render(&shape("y=\"NaN\""));
    let absent = render(&shape(""));
    assert!(painted(&absent) > 0);
    assert_eq!(nan.rgba, absent.rgba);
}

#[test]
fn an_ellipse_centred_at_nan_renders_like_one_without_that_centre() {
    let shape = |cy: &str| {
        format!(
            "<svg width=\"100\" height=\"100\"><ellipse cx=\"53\" {cy} rx=\"40\" ry=\"20\" \
             fill=\"red\"/></svg>"
        )
    };
    let nan = render(&shape("cy=\"NaN\""));
    let absent = render(&shape(""));
    assert!(painted(&absent) > 0);
    assert_eq!(nan.rgba, absent.rgba);
}

// A viewBox 1e-30 units wide scales its geometry by 2e32: this circle's
// control points lie 1e32 pixels out, far past the frame, where it paints
// nothing. Flattened whole, the curve needs more subdivisions than the
// flattener's depth limit allows, so it walked all 2^33 of them.
#[test]
fn a_circle_scaled_1e32_times_renders_in_bounded_time() {
    let r = render(
        "<svg width=\"100\" height=\"100\" viewBox=\"0 0 1e-30 1e-30\">\
         <circle cx=\"1\" cy=\"1\" r=\"1\" fill=\"red\"/></svg>",
    );
    assert_eq!(painted(&r), 0, "the circle lies far outside the frame");
}

// A shape reaching a billion pixels out paints the frame exactly as its part
// near the frame does. Past AGG's fixed-point range (about eight million
// pixels) a coordinate saturates on each axis on its own, which bends an edge
// that is not at 45 degrees: here the slope 1/2 became 1.
#[test]
fn a_triangle_reaching_a_billion_pixels_out_paints_the_frame_exactly() {
    let triangle = |far: &str| {
        format!(
            "<svg width=\"100\" height=\"100\"><polygon points=\"0,0 {far} 0,1000000\" \
             fill=\"red\"/></svg>"
        )
    };
    let far = render(&triangle("1000000000,500000000"));
    let near = render(&triangle("1000,500"));
    assert!(painted(&near) > 0);
    assert_eq!(far.rgba, near.rgba);
}

// A dash pattern a thousandth of a unit long on a line a billion units long
// is five hundred billion dashes; AGG's dasher walks them one by one. Past a
// million dashes the stroke is drawn solid.
#[test]
fn a_dash_pattern_far_finer_than_its_line_is_drawn_solid_in_bounded_time() {
    let r = render(
        "<svg width=\"100\" height=\"100\">\
         <line x1=\"0\" y1=\"50\" x2=\"1000000000\" y2=\"50\" stroke=\"red\" stroke-width=\"4\" \
         stroke-dasharray=\"0.001 0.001\"/></svg>",
    );
    assert!(painted(&r) > 0, "the line is drawn");
}
