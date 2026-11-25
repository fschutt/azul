//! Tests of what a style asks of a variable face (`getters.rs`): the coordinates a request derives
//! for the axes of a face, and the request a node reads from its properties through the shared
//! style cache.

use rust_fontconfig::{FcAxis, FcVariation};

use super::*;

/// An axis of a face: `(tag, min, default, max)`.
fn axis(tag: &[u8; 4], min: f32, default: f32, max: f32) -> FcAxis {
    FcAxis {
        tag: *tag,
        min,
        default,
        max,
        hidden: false,
    }
}

/// The axes of `azul-mock-variable.ttf`, in its `fvar` order.
fn mock_variation() -> FcVariation {
    FcVariation {
        axes: vec![
            axis(b"wght", 300.0, 400.0, 700.0),
            axis(b"wdth", 75.0, 100.0, 125.0),
            axis(b"slnt", -15.0, 0.0, 0.0),
            axis(b"ital", 0.0, 0.0, 1.0),
            axis(b"opsz", 8.0, 14.0, 144.0),
        ],
    }
}

/// A face with a `wght` axis only (Red Hat Mono).
fn weight_only() -> FcVariation {
    FcVariation {
        axes: vec![axis(b"wght", 300.0, 400.0, 700.0)],
    }
}

fn coordinates(request: &FontRequest, variation: &FcVariation) -> Vec<([u8; 4], f32)> {
    font_coordinates(request, variation)
}

/// No coordinate at all: the default instance.
fn nothing() -> Vec<([u8; 4], f32)> {
    Vec::new()
}

/// The initial request draws the default instance: no coordinate at all.
#[test]
fn the_default_request_asks_nothing_of_a_face() {
    assert_eq!(coordinates(&FontRequest::default(), &mock_variation()), nothing());
    assert_eq!(coordinates(&FontRequest::default(), &weight_only()), nothing());
    // a bucket that is the default one
    let normal = FontRequest::default().with_weight_bucket(FcWeight::Normal);
    assert_eq!(coordinates(&normal, &weight_only()), nothing());
}

/// The weight is the `wght` coordinate, exact (no bucket), clamped to the axis and rounded to the
/// step the cache keeps instances by.
#[test]
fn the_weight_is_clamped_to_the_axis_and_exact_between_the_buckets() {
    let at = |weight: f32| coordinates(&FontRequest::default().with_weight(weight), &weight_only());
    assert_eq!(at(650.0), [(*b"wght", 650.0)]);
    assert_eq!(at(350.0), [(*b"wght", 350.0)]);
    assert_eq!(at(450.5), [(*b"wght", 450.5)]);
    assert_eq!(at(700.0), [(*b"wght", 700.0)]);
    assert_eq!(at(1000.0), [(*b"wght", 700.0)], "clamped to the maximum of the axis");
    assert_eq!(at(1.0), [(*b"wght", 300.0)], "clamped to the minimum of the axis");
    assert_eq!(at(400.0), nothing(), "the default of the axis is no coordinate");
    // a step finer than the cache keeps is the same instance
    assert_eq!(at(350.004), [(*b"wght", 350.0)]);
    assert_eq!(at(400.004), nothing(), "and the default stays the default");
}

#[test]
fn the_stretch_is_the_width_axis_in_percent() {
    let at = |percent: f32| coordinates(&FontRequest::default().with_stretch(percent), &mock_variation());
    assert_eq!(at(87.5), [(*b"wdth", 87.5)]);
    assert_eq!(at(100.0), nothing());
    assert_eq!(at(50.0), [(*b"wdth", 75.0)], "ultra-condensed clamps to the axis");
    assert_eq!(at(200.0), [(*b"wdth", 125.0)], "ultra-expanded clamps to the axis");
}

/// `oblique <angle>` leans to the right, the `slnt` axis counts counter-clockwise: its value is
/// minus the angle. `italic` is `ital` 1; `normal` and `oblique` are `ital` 0, and `normal` and
/// `italic` are `slnt` 0 (the defaults, so no coordinate on a face that defaults them).
#[test]
fn the_style_is_the_slant_and_the_italic_axes() {
    let face = mock_variation();
    let style = |request: FontRequest| coordinates(&request, &face);
    let base = FontRequest::default();
    assert_eq!(style(base.clone().with_oblique(10.0)), [(*b"slnt", -10.0)]);
    assert_eq!(style(base.clone().with_style(FontStyle::Oblique)), [(*b"slnt", -14.0)]);
    assert_eq!(style(base.clone().with_oblique(40.0)), [(*b"slnt", -15.0)], "clamped");
    assert_eq!(
        style(base.clone().with_oblique(-10.0)),
        nothing(),
        "leaning left is +10, past the maximum 0 of the axis: the default"
    );
    assert_eq!(style(base.clone().with_style(FontStyle::Italic)), [(*b"ital", 1.0)]);
    assert_eq!(style(base.with_style(FontStyle::Normal)), nothing());
    // `italic` on a face with `slnt` and no `ital` stays upright: CSS maps italic to `ital` only
    let slant_only = FcVariation {
        axes: vec![axis(b"slnt", -15.0, 0.0, 0.0)],
    };
    assert_eq!(
        coordinates(&FontRequest::default().with_style(FontStyle::Italic), &slant_only),
        nothing()
    );
}

/// A face whose axis default is not the neutral value gets the neutral one asked for: `normal`
/// is `slnt` 0 and `ital` 0 (`font-style: normal` resets a face that is slanted by default).
#[test]
fn normal_resets_a_face_that_is_slanted_by_default() {
    let slanted = FcVariation {
        axes: vec![axis(b"slnt", -15.0, -10.0, 0.0), axis(b"ital", 0.0, 1.0, 1.0)],
    };
    assert_eq!(
        coordinates(&FontRequest::default(), &slanted),
        [(*b"slnt", 0.0), (*b"ital", 0.0)]
    );
    assert_eq!(
        coordinates(&FontRequest::default().with_oblique(10.0), &slanted),
        [(*b"ital", 0.0)],
        "oblique 10deg is the slant this face has by default: only `ital` is reset"
    );
}

#[test]
fn the_optical_size_is_the_font_size_in_css_px_clamped_to_the_axis() {
    let at = |px: f32| coordinates(&FontRequest::default().at_font_size(px), &mock_variation());
    assert_eq!(at(20.0), [(*b"opsz", 20.0)]);
    assert_eq!(at(14.0), nothing(), "the default of the axis");
    assert_eq!(at(8.0), [(*b"opsz", 8.0)]);
    assert_eq!(at(4.0), [(*b"opsz", 8.0)], "clamped to the minimum");
    assert_eq!(at(200.0), [(*b"opsz", 144.0)], "clamped to the maximum");
    assert_eq!(at(12.4), [(*b"opsz", 12.0)], "the whole px the request carries");
    assert_eq!(
        coordinates(&FontRequest::default(), &mock_variation()),
        nothing(),
        "no optical size (`font-optical-sizing: none`) asks nothing of the axis"
    );
}

/// The explicit settings are applied LAST: they win over the coordinate every property above
/// derives for the same axis, whatever the order of the declarations, and tags the face does not
/// have are ignored.
#[test]
fn the_explicit_settings_win_over_every_derived_coordinate() {
    let face = mock_variation();
    let base = FontRequest::default();
    let with = |request: FontRequest| coordinates(&request, &face);
    assert_eq!(
        with(base.clone().with_weight(300.0).with_setting(*b"wght", 650.0)),
        [(*b"wght", 650.0)]
    );
    assert_eq!(
        with(base.clone().with_style(FontStyle::Normal).with_setting(*b"ital", 1.0)),
        [(*b"ital", 1.0)],
        "an explicit \"ital\" 1 beats `font-style: normal`"
    );
    assert_eq!(
        with(base.clone().with_style(FontStyle::Italic).with_setting(*b"ital", 0.0)),
        nothing(),
        "and \"ital\" 0 beats `font-style: italic`"
    );
    assert_eq!(
        with(base.clone().at_font_size(8.0).with_setting(*b"opsz", 20.0)),
        [(*b"opsz", 20.0)],
        "\"opsz\" 20 beats the optical size of the font size"
    );
    assert_eq!(
        with(base.clone().with_setting(*b"wght", 900.0)),
        [(*b"wght", 700.0)],
        "an explicit value is clamped too"
    );
    assert_eq!(
        with(base.clone().with_weight(650.0).with_setting(*b"wght", 400.0)),
        nothing(),
        "an explicit value that is the default of the axis is no coordinate"
    );
    assert_eq!(
        with(base.clone().with_setting(*b"abcd", 5.0)),
        nothing(),
        "a tag the face does not have changes nothing"
    );
    // an axis the stylesheet has no property for, set by its tag
    let custom = FcVariation {
        axes: vec![axis(b"GRAD", 0.0, 0.0, 100.0)],
    };
    assert_eq!(
        coordinates(&base.with_setting(*b"GRAD", 50.0), &custom),
        [(*b"GRAD", 50.0)]
    );
}

/// The coordinates come in the order of the face's `fvar` table, whatever the order the request
/// was made in, and a face without an axis ignores that part of the request.
#[test]
fn the_coordinates_follow_the_order_of_the_axes_and_ignore_the_axes_a_face_lacks() {
    let request = FontRequest::default()
        .at_font_size(20.0)
        .with_oblique(10.0)
        .with_stretch(87.5)
        .with_weight(650.0);
    assert_eq!(
        coordinates(&request, &mock_variation()),
        [
            (*b"wght", 650.0),
            (*b"wdth", 87.5),
            (*b"slnt", -10.0),
            (*b"opsz", 20.0)
        ]
    );
    assert_eq!(coordinates(&request, &weight_only()), [(*b"wght", 650.0)]);
    assert_eq!(
        coordinates(&request, &FcVariation { axes: Vec::new() }),
        nothing(),
        "a face without axes asks nothing"
    );
}

/// A document with numeric weights builds the styles its nodes ask for through the shared style
/// cache - the nodes that read the same share a style, the others do not - and the memo's own
/// check (every hit is built the long way too) holds.
#[test]
fn numeric_weights_build_their_own_styles_through_the_style_cache() {
    use azul_core::dom::Dom;

    let block = |css: &str| {
        Dom::create_div()
            .with_css(css)
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("text"))
    };
    let dom = Dom::create_body()
        .with_child(block("font-weight: 350"))
        .with_child(block("font-weight: 650"))
        .with_child(block("font-weight: 650"))
        .with_child(block("font-weight: 400"))
        .with_child(block("font-weight: bold"))
        .with_child(block("font-weight: 650; font-stretch: 87.5%"))
        .with_child(block("font-style: oblique 10deg"))
        .with_child(block("font-style: oblique 10deg"));
    let styled = StyledDom::create_from_dom(dom);
    let viewport = PhysicalSize::new(800.0, 600.0);
    let mut cache = StyleCache::default();
    let request_of_text = |cache: &mut StyleCache, block: usize| -> FontRequest {
        // body(0), then each block and its text: block n is node 1 + 2 * n, its text the next
        let text = NodeId::new(2 + 2 * block);
        let style = get_style_properties_cached(cache, &styled, text, None, viewport);
        let direct = get_style_properties(&styled, text, None, viewport);
        assert_eq!(*style, direct, "block {block}: the cached style is the built one");
        style
            .font_stack
            .first_selector()
            .expect("a font stack")
            .request
            .clone()
    };
    let requests: Vec<FontRequest> = (0..8).map(|n| request_of_text(&mut cache, n)).collect();
    let weight = |n: usize| requests[n].weight.to_f32();
    assert_eq!(weight(0), 350.0);
    assert_eq!(weight(1), 650.0);
    assert_eq!(weight(2), 650.0);
    assert_eq!(weight(3), 400.0);
    assert_eq!(weight(4), 700.0);
    assert_eq!(weight(5), 650.0);
    assert_eq!(requests[5].stretch.to_f32(), 87.5);
    assert_eq!(requests[1], requests[2], "the same declarations ask the same");
    assert_ne!(requests[1], requests[5], "the stretch is part of the request");
    assert_eq!(requests[6].style, FontStyle::Oblique);
    assert_eq!(requests[6].oblique_degrees.to_f32(), 10.0);
    assert_eq!(requests[6], requests[7]);
    // the same nodes asked again are served from the cache, unchanged
    let again: Vec<FontRequest> = (0..8).map(|n| request_of_text(&mut cache, n)).collect();
    assert_eq!(again, requests);
}
