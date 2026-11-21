//! The software titlebar wraps the app's DOM: a container takes the
//! titlebar's and the app's styled DOMs as children
//! (`StyledDom::append_child`, `dll::desktop::csd`). Each DOM's stylesheet
//! goes with its nodes, so every later cascade still styles them - a rule
//! anchored on an ancestor's state (`.btn:hover .ink`), a viewport or theme
//! change - and only them. The container kept only its own empty sheet, so
//! none of those reached the app's DOM in a CSD window.

use azul_core::{
    dom::Dom,
    id::NodeId,
    styled_dom::{HoverChange, StyledDom},
};
use azul_css::{
    css::Css,
    dynamic_selector::DynamicSelectorContext,
    props::{
        basic::color::ColorU,
        property::{CssProperty, CssPropertyType},
        style::StyleBackgroundContent,
    },
};

const RED: ColorU = ColorU { r: 255, g: 0, b: 0, a: 255 };
const GREEN: ColorU = ColorU { r: 0, g: 255, b: 0, a: 255 };
const BLUE: ColorU = ColorU { r: 0, g: 0, b: 255, a: 255 };

fn css(source: &str) -> Css {
    azul_css::parser2::new_from_str(source).0
}

fn with_class(styled: &StyledDom, class: &str) -> NodeId {
    (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| styled.node_data.as_container()[*id].has_class(class))
        .unwrap_or_else(|| panic!("no .{class}"))
}

fn color(styled: &StyledDom, node: NodeId) -> Option<ColorU> {
    let state = styled.styled_nodes.as_container()[node].styled_node_state;
    let node_data = &styled.node_data.as_container()[node];
    match styled.css_property_cache.ptr.get_property(
        node_data,
        &node,
        &state,
        &CssPropertyType::TextColor,
    ) {
        Some(CssProperty::TextColor(v)) => v.get_property().map(|c| c.inner),
        _ => None,
    }
}

fn background(styled: &StyledDom, node: NodeId) -> Option<ColorU> {
    let state = styled.styled_nodes.as_container()[node].styled_node_state;
    azul_layout::solver3::getters::get_background_contents(styled, node, &state)
        .into_iter()
        .find_map(|bg| match bg {
            StyleBackgroundContent::Color(c) => Some(c),
            _ => None,
        })
}

/// The window's container, as the CSD shell makes it: an empty sheet, the
/// titlebar, then the app.
fn window(titlebar: StyledDom, app: StyledDom) -> StyledDom {
    let mut container = StyledDom::create(&mut Dom::create_html(), Css::empty());
    container.append_child(titlebar);
    container.append_child(app);
    container
}

fn titlebar() -> StyledDom {
    StyledDom::create(
        &mut Dom::create_div().with_class("bar".into()),
        css(".bar { color: #0000ff; }"),
    )
}

#[test]
fn an_ancestors_hover_restyles_the_apps_nodes_in_a_csd_window() {
    let app = StyledDom::create(
        &mut Dom::create_body().with_child(
            Dom::create_div()
                .with_class("btn".into())
                .with_child(Dom::create_div().with_class("ink".into())),
        ),
        css(".ink { color: #00ff00; } .btn:hover .ink { color: #ff0000; }"),
    );
    let mut sd = window(titlebar(), app);
    let btn = with_class(&sd, "btn");
    let ink = with_class(&sd, "ink");
    assert_eq!(color(&sd, ink), Some(GREEN), "harness: resting");
    let _ = sd.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: Vec::new(),
            entered_nodes: vec![btn],
        }),
        None,
    );
    assert_eq!(color(&sd, ink), Some(RED));
}

#[test]
fn a_viewport_change_recascades_the_apps_nodes_and_only_them() {
    let app = StyledDom::create(
        &mut Dom::create_body().with_child(Dom::create_div().with_class("ink".into())),
        css(
            "div { background: #ff0000; }
             .ink { color: #00ff00; }
             @media (max-width: 500px) { .ink { color: #ff0000; } }",
        ),
    );
    let mut sd = window(titlebar(), app);
    sd.set_dynamic_selector_context(DynamicSelectorContext {
        viewport_width: 400.0,
        ..Default::default()
    });
    let ink = with_class(&sd, "ink");
    let bar = with_class(&sd, "bar");
    assert_eq!(color(&sd, ink), Some(RED), "the app's narrow rule");
    assert_eq!(background(&sd, ink), Some(RED), "the app's own rules stay");
    assert_eq!(color(&sd, bar), Some(BLUE), "the titlebar keeps its rule");
    assert_eq!(background(&sd, bar), None, "the app's `div` rule stays with the app");
}
