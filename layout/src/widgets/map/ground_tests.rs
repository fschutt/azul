//! The ground under the tiles - what shows before the first tile arrives,
//! between tiles and beyond the poles - is the land colour of the map's look,
//! light or dark as the window is, and switches with it.

use alloc::{string::String, vec::Vec};

use azul_core::dom::{Dom, OptionDom};
use azul_css::{dynamic_selector::DynamicSelectorVec, props::property::CssProperty, AzString};

use super::{
    autotest_generated::{cache_at, with_virtual_view_info},
    map_ground_css, map_widget_render, MapTheme, MapTileLayer, MapWidget,
};

/// The background declarations of a node, with their conditions: its inline
/// style and its own sheet (`with_css` gives a node a sheet of its own).
fn grounds(dom: &Dom) -> Vec<(CssProperty, DynamicSelectorVec)> {
    dom.root
        .style
        .iter_inline_properties()
        .chain(
            dom.css
                .as_ref()
                .iter()
                .flat_map(|sheet| sheet.iter_inline_properties()),
        )
        .filter(|(p, _)| p.get_type().to_str().starts_with("background"))
        .map(|(p, c)| (p.clone(), c.clone()))
        .collect()
}

/// What a node styled with nothing but `css` declares as its background.
fn grounds_of_css(css: &str) -> Vec<(CssProperty, DynamicSelectorVec)> {
    grounds(&Dom::create_div().with_css(css))
}

fn layer(theme: MapTheme) -> MapTileLayer {
    MapTileLayer {
        theme,
        ..MapTileLayer::default()
    }
}

#[test]
fn the_ground_is_the_light_looks_land_colour_and_the_dark_looks_in_dark_mode() {
    assert_eq!(
        map_ground_css(&layer(MapTheme::Positron)),
        "background: #f2f3f0; @media (prefers-color-scheme: dark) { background: #0c0c0c; }",
        "Positron's land, and Dark Matter's in dark mode"
    );
    assert_eq!(
        map_ground_css(&layer(MapTheme::Google)),
        "background: #f2efe9; @media (prefers-color-scheme: dark) { background: #242f3e; }"
    );
}

#[test]
fn a_custom_sheet_grounds_both_modes_in_its_own_canvas() {
    let mut custom = layer(MapTheme::Custom);
    custom.style_css = AzString::from("canvas { fill: #102030; } water { fill: #000000; }");
    assert_eq!(
        map_ground_css(&custom),
        "background: #102030; @media (prefers-color-scheme: dark) { background: #102030; }"
    );
}

#[test]
fn the_widget_and_its_view_paint_the_ground_before_any_tile_arrives() {
    let positron = layer(MapTheme::Positron);
    let expected = grounds_of_css(&map_ground_css(&positron));
    assert_eq!(expected.len(), 2, "a light ground and a dark one");

    let widget = MapWidget::create(positron).dom();
    assert_eq!(grounds(&widget), expected, "the widget's own box");

    let mut cache = cache_at(0.0, 0.0, 3.0);
    cache.layer = layer(MapTheme::Positron);
    let dataset = azul_core::refany::RefAny::new(cache);
    let ret = with_virtual_view_info(640.0, 480.0, |info| map_widget_render(dataset.clone(), info));
    let OptionDom::Some(view_root) = &ret.dom else {
        panic!("a finite box renders its view");
    };
    assert_eq!(grounds(view_root), expected, "the view, under its tiles");
}

#[test]
fn a_tile_still_loading_is_see_through_with_no_grey_box_and_no_text() {
    let dataset = azul_core::refany::RefAny::new(cache_at(0.0, 0.0, 3.0));
    let ret = with_virtual_view_info(640.0, 480.0, |info| map_widget_render(dataset.clone(), info));
    let OptionDom::Some(view_root) = &ret.dom else {
        panic!("a finite box renders its view");
    };
    let grid = view_root.children.as_slice().first().expect("the grid");
    assert!(!grid.children.as_slice().is_empty());
    for tile_box in grid.children.as_slice() {
        let chrome: Vec<String> = tile_box
            .root
            .style
            .iter_inline_properties()
            .map(|(p, _)| String::from(p.get_type().to_str()))
            .filter(|name| name.starts_with("background") || name.starts_with("border"))
            .collect();
        assert!(chrome.is_empty(), "a loading tile paints {chrome:?} over the ground");
        assert!(
            tile_box.children.as_slice().is_empty(),
            "a loading tile shows no text over the map"
        );
    }
}
