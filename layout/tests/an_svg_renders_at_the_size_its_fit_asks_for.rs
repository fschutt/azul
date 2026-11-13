//! `ParsedSvg::render` draws at the size `SvgRenderOptions::fit` asks for
//! when no `target_size` is given (PDF9: a PDF page drawn 560 px wide for a
//! preview, or at a zoom for the viewer). Before, `fit` was ignored and every
//! render without a target size was 800 x 600 px, the page letterboxed inside.

use azul_core::{
    resources::{RawImage, RawImageData},
    svg::{SvgFitTo, SvgParseOptions, SvgRenderOptions},
};
use azul_css::props::basic::{LayoutSize, OptionLayoutSize};
use azul_layout::xml::svg::ParsedSvg;

/// 200 x 100 user units, all red.
const WIDE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200px" height="100px" viewBox="0 0 200 100"><rect width="200" height="100" fill="red"/></svg>"#;

fn render(svg: &str, options: SvgRenderOptions) -> RawImage {
    ParsedSvg::from_string(svg, SvgParseOptions::default())
        .expect("SVG must parse")
        .render(options)
        .expect("SVG must render")
}

fn fit(fit: SvgFitTo) -> SvgRenderOptions {
    SvgRenderOptions {
        fit,
        ..SvgRenderOptions::default()
    }
}

#[test]
fn fit_width_draws_that_wide_with_the_svgs_aspect_ratio() {
    let image = render(WIDE, fit(SvgFitTo::Width(50)));
    assert_eq!((image.width, image.height), (50, 25));
}

#[test]
fn fit_height_draws_that_high_with_the_svgs_aspect_ratio() {
    let image = render(WIDE, fit(SvgFitTo::Height(40)));
    assert_eq!((image.width, image.height), (80, 40));
}

#[test]
fn fit_zoom_scales_the_natural_size() {
    let image = render(WIDE, fit(SvgFitTo::Zoom(1.5)));
    assert_eq!((image.width, image.height), (300, 150));
}

#[test]
fn fit_original_draws_at_the_natural_size() {
    let image = render(WIDE, fit(SvgFitTo::Original));
    assert_eq!((image.width, image.height), (200, 100));
}

#[test]
fn an_explicit_target_size_wins_over_the_fit() {
    let options = SvgRenderOptions {
        target_size: OptionLayoutSize::Some(LayoutSize {
            width: 30,
            height: 30,
        }),
        fit: SvgFitTo::Width(50),
        ..SvgRenderOptions::default()
    };
    let image = render(WIDE, options);
    assert_eq!((image.width, image.height), (30, 30));
}

#[test]
fn a_pdf_page_fitted_by_width_fills_the_image_edge_to_edge() {
    // A4 in points, as printpdf writes a page: fractional px sizes.
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="595.2756px" height="841.8898px" viewBox="0 0 595.2756 841.8898"><rect width="595.2756" height="841.8898" fill="red"/></svg>"#;
    let image = render(svg, fit(SvgFitTo::Width(100)));
    assert_eq!((image.width, image.height), (100, 141));
    let RawImageData::U8(bytes) = &image.pixels else {
        panic!("expected 8-bit pixels");
    };
    let bytes = bytes.as_ref();
    // One pixel in from the corner: the rounded height (141.42 -> 141) leaves
    // the last column a fraction of a pixel short.
    let corner = ((image.height - 2) * image.width + (image.width - 2)) * 4;
    assert!(
        bytes[corner] > 200 && bytes[corner + 3] > 200,
        "the bottom-right is the page's, not a letterbox: {:?}",
        &bytes[corner..corner + 4]
    );
}

#[test]
fn an_svg_without_any_size_is_still_drawn_as_wide_as_asked() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10" fill="red"/></svg>"#;
    let image = render(svg, fit(SvgFitTo::Width(50)));
    assert_eq!(
        image.width, 50,
        "no natural size: the width is still the asked one"
    );
}
