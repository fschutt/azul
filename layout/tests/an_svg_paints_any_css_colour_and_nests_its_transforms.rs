//! The CPU SVG renderer (`ParsedSvg::render`) draws what a browser draws for
//! the paints and transforms a PDF page's SVG uses (PDF9): printpdf writes
//! every paint as `rgb(r, g, b)` and positions content with `transform`
//! attributes inside a page that is scaled to the target size. Before, an
//! `rgb()` paint drew NOTHING (a PDF page came out blank), an element's own
//! transform ran BEFORE its parent's scale instead of inside it, and a
//! transform list kept only its first function.

use azul_core::{
    resources::{RawImage, RawImageData},
    svg::{SvgParseOptions, SvgRenderOptions},
};
use azul_css::props::basic::{ColorU, LayoutSize, OptionColorU, OptionLayoutSize};
use azul_layout::xml::svg::ParsedSvg;

/// `svg` rasterised at `w` x `h` px on white.
fn render(svg: &str, w: isize, h: isize) -> RawImage {
    let parsed = ParsedSvg::from_string(svg, SvgParseOptions::default()).expect("SVG must parse");
    let options = SvgRenderOptions {
        target_size: OptionLayoutSize::Some(LayoutSize {
            width: w,
            height: h,
        }),
        background_color: OptionColorU::Some(ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
        ..SvgRenderOptions::default()
    };
    parsed.render(options).expect("SVG must render")
}

/// The RGBA of pixel (`x`, `y`).
fn px(image: &RawImage, x: usize, y: usize) -> [u8; 4] {
    let RawImageData::U8(bytes) = &image.pixels else {
        panic!("expected 8-bit pixels");
    };
    let bytes = bytes.as_ref();
    let i = (y * image.width + x) * 4;
    [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
}

fn is_red(p: [u8; 4]) -> bool {
    p[0] > 200 && p[1] < 60 && p[2] < 60
}

fn is_white(p: [u8; 4]) -> bool {
    p[0] > 240 && p[1] > 240 && p[2] > 240
}

#[test]
fn an_rgb_paint_fills_the_shape() {
    let image = render(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><path d="M0,0 L8,0 L8,8 L0,8 Z" fill="rgb(255, 0, 0)" stroke="none"/></svg>"#,
        8,
        8,
    );
    assert!(
        is_red(px(&image, 4, 4)),
        "rgb(255, 0, 0) must paint red, got {:?}",
        px(&image, 4, 4)
    );
}

#[test]
fn an_rgb_stroke_paints_the_outline() {
    let image = render(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M0,8 L16,8" fill="none" stroke="rgb(255, 0, 0)" stroke-width="4"/></svg>"#,
        16,
        16,
    );
    assert!(is_red(px(&image, 8, 8)), "got {:?}", px(&image, 8, 8));
    assert!(is_white(px(&image, 8, 1)));
}

#[test]
fn every_css_colour_keyword_is_an_svg_paint() {
    let image = render(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8" fill="rebeccapurple"/></svg>"#,
        8,
        8,
    );
    let p = px(&image, 4, 4);
    assert!(
        (p[0], p[1], p[2]) == (102, 51, 153),
        "rebeccapurple is #663399, got {p:?}"
    );
}

#[test]
fn an_elements_transform_applies_inside_its_parents_scale() {
    // viewBox 8x8 drawn at 16x16: the page is scaled 2x. The rect is moved
    // 4 user units right BEFORE that scale, so it covers x 8..16 px.
    let image = render(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="4" height="4" fill="red" transform="translate(4, 0)"/></svg>"#,
        16,
        16,
    );
    assert!(
        is_red(px(&image, 12, 4)),
        "the moved rect must cover x 8..16, got {:?} at x 12",
        px(&image, 12, 4)
    );
    assert!(
        is_white(px(&image, 5, 4)),
        "x 4..8 must stay empty (the parent's scale ran first), got {:?}",
        px(&image, 5, 4)
    );
}

#[test]
fn a_groups_transform_applies_outside_its_childrens() {
    // <g scale(2)> <rect translate(2,0) 2x2>: the rect is moved in the
    // group's units, then scaled - it covers x 4..8, y 0..4.
    let image = render(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><g transform="scale(2)"><rect width="2" height="2" fill="red" transform="translate(2, 0)"/></g></svg>"#,
        16,
        16,
    );
    assert!(is_red(px(&image, 6, 2)), "got {:?}", px(&image, 6, 2));
    assert!(is_white(px(&image, 3, 2)), "got {:?}", px(&image, 3, 2));
}

#[test]
fn a_transform_list_applies_every_function_right_to_left() {
    // "translate(8,0) scale(2)": scale first, then translate - a 2x2 rect
    // covers x 8..12, y 0..4.
    let image = render(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="2" height="2" fill="red" transform="translate(8, 0) scale(2)"/></svg>"#,
        16,
        16,
    );
    assert!(is_red(px(&image, 10, 2)), "got {:?}", px(&image, 10, 2));
    assert!(is_red(px(&image, 10, 3)), "the scale must apply: y 0..4");
    assert!(is_white(px(&image, 1, 1)), "got {:?}", px(&image, 1, 1));
    assert!(is_white(px(&image, 13, 2)), "got {:?}", px(&image, 13, 2));
}

#[test]
fn a_rotation_about_a_centre_turns_around_that_point() {
    // The bar x 8..12, y 8..10, turned 180 degrees about (8, 8), lands at
    // x 4..8, y 6..8 (a turn about the origin would put it off the image).
    let image = render(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect x="8" y="8" width="4" height="2" fill="red" transform="rotate(180 8 8)"/></svg>"#,
        16,
        16,
    );
    assert!(is_red(px(&image, 6, 7)), "got {:?}", px(&image, 6, 7));
    assert!(is_white(px(&image, 10, 9)), "got {:?}", px(&image, 10, 9));
}

#[test]
fn a_pdf_style_page_with_matrix_transforms_draws_where_the_page_says() {
    // A page in printpdf's form: 100 x 50 pt, a path element with its
    // paint as rgb() and its CTM as a space-separated matrix() that moves
    // the box 50 pt right.
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100px" height="50px" viewBox="0 0 100 50">
<g><path d="M10,40 L30,40 L30,30 L10,30 Z" fill="rgb(255, 0, 0)" fill-rule="nonzero" stroke="none" stroke-width="1" stroke-linejoin="miter" stroke-linecap="butt" transform="matrix(1 0 0 1 50 0)" /></g></svg>"#;
    // Drawn at 200 x 100: 2 px per point.
    let image = render(svg, 200, 100);
    // The box: x (10+50)..(30+50) = 60..80 pt -> 120..160 px; SVG y 30..40 -> 60..80 px.
    assert!(is_red(px(&image, 140, 70)), "got {:?}", px(&image, 140, 70));
    assert!(
        is_white(px(&image, 40, 70)),
        "the box must not stay at its untransformed place, got {:?}",
        px(&image, 40, 70)
    );
}
