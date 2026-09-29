//! A CSS value is cut into its components (at spaces) or its list items (at
//! commas) only at its TOP level: never inside a function's parentheses and
//! never inside a quoted string.
//!
//! `spaces_inside_parentheses_stay_in_one_value.rs` covers the shadows,
//! borders and scrollbar colours. This file covers the other families: the
//! comma lists (`font-family`, `animation`, background layers), quoted
//! strings, grid tracks, filter lists and `var()` fallbacks.

use azul_css::{
    css::{Css, CssDeclaration},
    props::{
        basic::{
            animation::{parse_style_animation_vec, AnimationTiming},
            color::{parse_css_color, ColorU},
            font::{parse_style_font_family, StyleFontFamily},
        },
        layout::grid::parse_grid_template,
        property::{parse_css_property, CssProperty, CssPropertyType},
        style::{
            background::{parse_style_background_content_multiple, StyleBackgroundContent},
            border::parse_style_border,
            filter::{parse_style_filter_vec, StyleFilter},
        },
    },
};

/// Every declaration of the stylesheet's rules, in source order.
fn declarations(css: &str) -> Vec<CssDeclaration> {
    let (css, warnings) = Css::from_string_with_warnings(css.to_string().into());
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    css.rules
        .as_ref()
        .iter()
        .flat_map(|rule| rule.declarations.as_ref().iter().cloned())
        .collect()
}

// ---------------------------------------------------------------------------
// font-family: quoted names
// ---------------------------------------------------------------------------

#[test]
fn a_font_family_list_keeps_a_quoted_name_with_a_comma_whole() {
    let families = parse_style_font_family("\"Foo, Bar\", serif").unwrap();
    assert_eq!(
        families.as_slice(),
        &[
            StyleFontFamily::System("Foo, Bar".into()),
            StyleFontFamily::System("serif".into()),
        ],
        "the comma inside the quotes is part of the name"
    );

    let single = parse_style_font_family("'Comma, Inc. Sans', \"Times New Roman\"").unwrap();
    assert_eq!(
        single.as_slice(),
        &[
            StyleFontFamily::System("Comma, Inc. Sans".into()),
            StyleFontFamily::System("Times New Roman".into()),
        ]
    );
}

#[test]
fn a_font_family_name_with_a_comma_prints_quoted_and_round_trips() {
    let family = StyleFontFamily::System("Foo,Bar".into());
    let printed = family.as_string();
    assert_eq!(
        printed, "\"Foo,Bar\"",
        "a name with a comma must be quoted, or it reads back as two families"
    );
    let parsed = parse_style_font_family(&printed).unwrap();
    assert_eq!(parsed.as_slice(), &[family]);
}

#[test]
fn a_font_family_fallback_inside_var_keeps_its_quoted_comma() {
    let decls = declarations(".a { font-family: var(--f, \"Foo, Bar\", serif); }");
    let fallback = decls
        .iter()
        .find_map(|decl| match decl {
            CssDeclaration::Dynamic(d) => Some(&d.default_value),
            _ => None,
        })
        .expect("font-family: var(..) is a dynamic declaration");
    let CssProperty::FontFamily(value) = fallback else {
        panic!("the fallback is a font-family, got {fallback:?}");
    };
    let families = value.get_property().expect("an exact fallback");
    assert_eq!(
        families.as_slice(),
        &[
            StyleFontFamily::System("Foo, Bar".into()),
            StyleFontFamily::System("serif".into()),
        ]
    );
}

// ---------------------------------------------------------------------------
// animation lists: cubic-bezier(a, b, c, d)
// ---------------------------------------------------------------------------

#[test]
fn an_animation_with_a_cubic_bezier_timing_parses() {
    let parsed = parse_css_property(
        CssPropertyType::Animation,
        "swoosh 1s cubic-bezier(0.4, 0, 0.2, 1)",
    )
    .expect("the commas inside cubic-bezier() do not separate animations");
    let CssProperty::Animation(value) = &parsed else {
        panic!("expected an animation, got {parsed:?}");
    };
    let list = value.get_property().expect("an exact value");
    let list = list.as_slice();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name.as_str(), "swoosh");
    match list[0].timing {
        AnimationTiming::CubicBezier(b) => {
            assert_eq!((b.x1, b.y1, b.x2, b.y2), (400, 0, 200, 1000));
        }
        other => panic!("expected a bezier, got {other:?}"),
    }
}

#[test]
fn an_animation_list_splits_between_entries_not_inside_their_timing_functions() {
    let list = parse_style_animation_vec(
        "width 1s cubic-bezier(0.4,0,0.2,1), color 2s cubic-bezier(0, 0, 1, 1) 500ms",
    )
    .expect("two animations, each with a cubic-bezier() timing");
    let list = list.as_slice();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name.as_str(), "width");
    assert_eq!(list[0].duration.millis(), 1000);
    assert_eq!(list[1].name.as_str(), "color");
    assert_eq!(list[1].duration.millis(), 2000);
    assert_eq!(list[1].delay.millis(), 500);
    assert!(matches!(list[0].timing, AnimationTiming::CubicBezier(_)));
    assert!(matches!(list[1].timing, AnimationTiming::CubicBezier(_)));
}

// ---------------------------------------------------------------------------
// grid tracks
// ---------------------------------------------------------------------------

#[test]
fn a_grid_template_splits_its_tracks_at_tabs_and_newlines() {
    // CSS whitespace is space, tab and the newlines: a track list written
    // over several lines is still a list of tracks.
    assert_eq!(parse_grid_template("100px\t200px").unwrap().tracks.len(), 2);
    assert_eq!(
        parse_grid_template("100px\n  200px\n  1fr")
            .unwrap()
            .tracks
            .len(),
        3
    );
}

#[test]
fn a_grid_repeat_keeps_a_minmax_track_whole() {
    let tpl = parse_grid_template("repeat(2, minmax(100px, 1fr)) 200px").unwrap();
    assert_eq!(tpl.tracks.len(), 3);
}

// ---------------------------------------------------------------------------
// background layers
// ---------------------------------------------------------------------------

#[test]
fn a_background_layer_list_splits_after_a_quoted_url_that_contains_a_parenthesis() {
    let layers = parse_style_background_content_multiple("url(\"a).png\"), red")
        .expect("an image layer and a colour layer");
    assert_eq!(
        layers.as_slice(),
        &[
            StyleBackgroundContent::Image("a).png".into()),
            StyleBackgroundContent::Color(ColorU::RED),
        ],
        "the `)` inside the quotes does not close url(), so the comma after it \
         separates the layers"
    );
}

#[test]
fn background_layers_whose_gradient_stops_hold_rgba_colours_parse() {
    let layers = parse_style_background_content_multiple(
        "linear-gradient(to bottom, rgba(0, 0, 0, 0.5) 0%, rgba(0, 0, 0, 0) 100%), url(a.png)",
    )
    .expect("a gradient layer and an image layer");
    let layers = layers.as_slice();
    assert_eq!(layers.len(), 2);
    let StyleBackgroundContent::LinearGradient(gradient) = &layers[0] else {
        panic!("the first layer is the gradient, got {:?}", layers[0]);
    };
    assert_eq!(gradient.stops.as_slice().len(), 2);
    assert_eq!(layers[1], StyleBackgroundContent::Image("a.png".into()));
}

// ---------------------------------------------------------------------------
// borders, filters, var() fallbacks with functional colours
// ---------------------------------------------------------------------------

#[test]
fn a_border_with_an_hsl_colour_parses() {
    let side = parse_style_border("2px solid hsl(210, 20%, 50%)")
        .expect("hsl(210, 20%, 50%) is ONE border component");
    assert_eq!(
        side.border_color,
        parse_css_color("hsl(210, 20%, 50%)").unwrap()
    );
}

#[test]
fn a_filter_list_keeps_a_drop_shadow_with_an_rgba_colour_whole() {
    let filters = parse_style_filter_vec("blur(2px) drop-shadow(0 1px 2px rgba(0, 0, 0, 0.5))")
        .expect("two filter functions");
    let filters = filters.as_slice();
    assert_eq!(filters.len(), 2);
    assert!(matches!(filters[0], StyleFilter::Blur(_)));
    let StyleFilter::DropShadow(shadow) = &filters[1] else {
        panic!("the second filter is the drop shadow, got {:?}", filters[1]);
    };
    assert_eq!(shadow.color, parse_css_color("rgba(0, 0, 0, 0.5)").unwrap());
}

#[test]
fn a_text_shadow_fallback_inside_var_keeps_its_rgba_colour_whole() {
    let decls = declarations(".a { text-shadow: var(--s, 0 1px 2px rgba(0, 0, 0, 0.5)); }");
    let fallback = decls
        .iter()
        .find_map(|decl| match decl {
            CssDeclaration::Dynamic(d) => Some(d.default_value.clone()),
            _ => None,
        })
        .expect("text-shadow: var(..) is a dynamic declaration");
    assert_eq!(
        fallback,
        parse_css_property(CssPropertyType::TextShadow, "0 1px 2px rgba(0,0,0,0.5)").unwrap()
    );
}
