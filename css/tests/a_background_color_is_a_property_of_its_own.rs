//! `background-color` is a property of its own (CSS Backgrounds 3 s3.2):
//! `background-image` leaves it alone and the `background` shorthand sets it,
//! to its final layer's colour or to transparent.

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::color::ColorU,
        property::{
            get_css_key_map, parse_combined_css_property, CombinedCssPropertyType, CssProperty,
            CssPropertyType,
        },
        style::StyleBackgroundContent,
    },
};

fn shorthand(key: &str, value: &str) -> Vec<CssProperty> {
    let map = get_css_key_map();
    let key = CombinedCssPropertyType::from_str(key, &map).expect("a shorthand");
    parse_combined_css_property(key, value).expect("parses")
}

fn colour_of(props: &[CssProperty]) -> Option<StyleBackgroundContent> {
    props.iter().find_map(|p| match p {
        CssProperty::BackgroundColor(CssPropertyValue::Exact(c)) => Some(c.clone()),
        _ => None,
    })
}

const RED: StyleBackgroundContent = StyleBackgroundContent::Color(ColorU::RED);

#[test]
fn background_color_sets_only_the_colour() {
    let props = shorthand("background-color", "red");
    assert_eq!(props.len(), 1, "{props:?}");
    assert_eq!(props[0].get_type(), CssPropertyType::BackgroundColor);
    assert_eq!(colour_of(&props), Some(RED));
}

#[test]
fn background_image_sets_only_the_layers() {
    for value in ["none", "url(a.png)", "linear-gradient(red, blue)"] {
        let props = shorthand("background-image", value);
        assert!(
            props.iter().all(|p| p.get_type() == CssPropertyType::BackgroundContent),
            "{value}: {props:?}"
        );
    }
}

#[test]
fn the_shorthand_sets_the_colour_or_resets_it() {
    assert_eq!(colour_of(&shorthand("background", "red")), Some(RED));
    assert_eq!(colour_of(&shorthand("background", "url(a.png) no-repeat red")), Some(RED));
    assert_eq!(colour_of(&shorthand("background", "url(a.png), red")), Some(RED));
    assert_eq!(
        colour_of(&shorthand("background", "url(a.png)")),
        Some(StyleBackgroundContent::Color(ColorU::TRANSPARENT))
    );
    // The image layers carry no colour layer.
    let props = shorthand("background", "url(a.png) red");
    let layers = props
        .iter()
        .find_map(|p| match p {
            CssProperty::BackgroundContent(CssPropertyValue::Exact(v)) => Some(v.clone()),
            _ => None,
        })
        .expect("the layers");
    assert!(matches!(layers.as_slice(), [StyleBackgroundContent::Image(_)]), "{layers:?}");
}

#[test]
fn fill_is_the_background_colour() {
    let props = shorthand("fill", "red");
    assert_eq!(colour_of(&props), Some(RED));
}
