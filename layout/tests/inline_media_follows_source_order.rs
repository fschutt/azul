//! A nested `@media` block in an inline style cascades in SOURCE ORDER.
//!
//! Every theme-aware inline style is written as its light value with a dark
//! block after it: `color: #202020; @media (prefers-color-scheme: dark) {
//! color: system:text; }`. Both declarations have the same specificity, so
//! under a dark theme the LATER one - the block's - must win, and a plain
//! declaration after the block must win over the block in turn. The parser
//! emitted the block first and every plain declaration of the rule after it,
//! so the light value always won: the ColorInput picker panel stayed white in
//! dark mode under its own dark text.

use azul_core::{
    dom::{Dom, NodeId},
    styled_dom::StyledDom,
};
use azul_css::{
    dynamic_selector::DynamicSelectorContext,
    props::basic::{ColorU, PhysicalSize},
};
use azul_layout::solver3::getters;
use std::sync::Arc;

fn ink(css: &str, dark: bool) -> ColorU {
    let style = Arc::new(if dark {
        azul_css::system::defaults::macos_modern_dark()
    } else {
        azul_css::system::defaults::macos_modern_light()
    });
    let ctx = DynamicSelectorContext::from_system_style(&style).with_viewport(800.0, 600.0);
    let sd = StyledDom::create_from_dom_with_context(
        Dom::create_body().with_child(Dom::create_div().with_css(css)),
        Some(ctx),
    );
    getters::get_style_properties(&sd, NodeId::new(1), None, PhysicalSize::new(800.0, 600.0))
        .color
}

const RED: ColorU = ColorU { r: 255, g: 0, b: 0, a: 255 };
const BLUE: ColorU = ColorU { r: 0, g: 0, b: 255, a: 255 };
const GREEN: ColorU = ColorU { r: 0, g: 255, b: 0, a: 255 };

#[test]
fn a_dark_block_after_a_plain_declaration_wins_under_the_dark_theme() {
    let css = "color: #ff0000; @media (prefers-color-scheme: dark) { color: #0000ff; }";
    assert_eq!(ink(css, true), BLUE, "the later, matching dark block wins");
    assert_eq!(ink(css, false), RED, "a dark block does not apply under light");
}

#[test]
fn a_plain_declaration_after_a_dark_block_wins_over_it() {
    let css = "@media (prefers-color-scheme: dark) { color: #0000ff; } color: #00ff00;";
    assert_eq!(ink(css, true), GREEN, "the later plain declaration wins over the block");
}

/// The colour the display list PAINTS a text node in: its live re-resolve
/// reads the cascade's `color` for the text node itself (`get_text_color`,
/// the slow path - inherited through `cascaded_props` / `computed_values`,
/// not the compact tier `get_style_properties` answers from).
fn painted_text_ink(css: &str, dark: bool) -> ColorU {
    let style = Arc::new(if dark {
        azul_css::system::defaults::macos_modern_dark()
    } else {
        azul_css::system::defaults::macos_modern_light()
    });
    let ctx = DynamicSelectorContext::from_system_style(&style).with_viewport(800.0, 600.0);
    let sd = StyledDom::create_from_dom_with_context(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_css(css)
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("ink")),
        ),
        Some(ctx),
    );
    let text = NodeId::new(2);
    let nd = &sd.node_data.as_container()[text];
    let state = sd.styled_nodes.as_container()[text].styled_node_state;
    let color = sd
        .get_css_property_cache()
        .get_text_color(nd, &text, &state)
        .and_then(|v| v.get_property().copied())
        .map(|c| c.inner)
        .expect("the text node has a `color`");
    getters::system_colors_resolved(&sd, color)
}

/// The TEXT inside inherits the declaration that WON on its parent, not the
/// first one the parent listed. The restyle's inheritance step read the
/// parent's declarations in source order before they were deduplicated and
/// kept the FIRST per property - so once the dark block sat after the plain
/// value (4d3ba32fe), every `color: <light>; @media (dark) { color: .. }`
/// painted its text in the light value: the demo's "Azul Widget Showcase"
/// heading and its titlebar title came out dark text on the dark page,
/// while the div's own `color` (and the compact tier) said the right thing.
#[test]
fn the_text_inside_paints_the_declaration_that_won() {
    let css = "color: #ff0000; @media (prefers-color-scheme: dark) { color: #0000ff; }";
    assert_eq!(
        painted_text_ink(css, true),
        BLUE,
        "the text inherits the dark block, which won on its parent"
    );
    assert_eq!(painted_text_ink(css, false), RED, "and the plain value under light");

    let css = "@media (prefers-color-scheme: dark) { color: #0000ff; } color: #00ff00;";
    assert_eq!(
        painted_text_ink(css, true),
        GREEN,
        "a plain declaration after the block wins on the parent, so the text inherits it"
    );
}
