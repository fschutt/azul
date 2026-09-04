//! `@theme(dark)` in an APP stylesheet has to re-resolve per pass.
//!
//! The mechanism AzWriter's document sheet depends on: the page CSS is baked
//! into a CACHED content DOM (the source of truth its edit loop mutates),
//! which a theme switch does not rebuild - the document did not change, only
//! the desktop did. So the dark colours cannot be chosen in Rust at parse
//! time; they have to be an `@theme(dark)` override that the CASCADE picks up
//! from the live `SystemStyle` on whatever pass comes next.
//!
//! This pins that end to end: one `StyledDom`, two system styles, two answers -
//! and the light one asserted too, because a rule that "worked" by being
//! applied unconditionally would pass a dark-only assertion.

use std::sync::Arc;

use azul_core::{id::NodeId, styled_dom::StyledDom};
use azul_css::{
    props::basic::{color::ColorU, PhysicalSize},
    system::{SystemStyle, Theme},
};

const LIGHT_INK: ColorU = ColorU {
    r: 26,
    g: 26,
    b: 26,
    a: 255,
};
const DARK_INK: ColorU = ColorU {
    r: 239,
    g: 240,
    b: 241,
    a: 255,
};

fn style_for(theme: Theme) -> Arc<SystemStyle> {
    let mut s = SystemStyle::default();
    s.theme = theme;
    Arc::new(s)
}

/// Resolve `node`'s colour the way a layout pass does: hand the DOM the
/// window's dynamic-selector context FIRST (author `@`-rule conditions are
/// baked at CASCADE time - `set_dynamic_selector_context` re-runs the author
/// cascade when the context moves), then read the property.
fn text_color_at(styled_dom: &mut StyledDom, node: NodeId, theme: Theme) -> ColorU {
    let style = style_for(theme);
    let ctx = azul_css::dynamic_selector::DynamicSelectorContext::from_system_style(&style)
        .with_viewport(800.0, 600.0);
    styled_dom.set_dynamic_selector_context(ctx);
    azul_layout::solver3::getters::get_style_properties(
        styled_dom,
        node,
        Some(&style),
        PhysicalSize::new(800.0, 600.0),
    )
    .color
}

#[test]
fn a_theme_dark_block_overrides_the_base_rule_only_in_the_dark_theme() {
    // Exactly the shape of azwriter's document stylesheet: a base rule, then
    // a `@theme(dark)` block that re-states the same property.
    // `with_css` on a Dom takes INLINE declarations, not a selector sheet, so
    // the stylesheet goes through the XML `<style>` path the app uses.
    let component_map = azul_core::xml::ComponentMap::default();
    let mut styled_dom = azul_layout::xml::domxml_from_str(
        "<html><head><style>
            div { color: #1a1a1a; }
            @theme(dark) { div { color: #eff0f1; } }
        </style></head><body><div></div></body></html>",
        &component_map,
    )
    .parsed_dom;
    let root = styled_dom
        .node_data
        .as_container()
        .linear_iter()
        .filter(|id| {
            matches!(
                styled_dom.node_data.as_container()[*id].get_node_type(),
                azul_core::dom::NodeType::Div
            )
        })
        .last()
        .expect("the parsed document has a <div>");

    // Order matters as a control: light FIRST, then dark, then light again -
    // a context change that only ever moves one way would pass the first two.
    let light = text_color_at(&mut styled_dom, root, Theme::Light);
    let dark = text_color_at(&mut styled_dom, root, Theme::Dark);
    let light_again = text_color_at(&mut styled_dom, root, Theme::Light);
    assert_eq!(
        light, light_again,
        "switching back to light must restore the base rule, not strand the \
         document in the dark palette"
    );

    assert_eq!(
        light, LIGHT_INK,
        "the base rule must survive in the light theme - an override that \
         applies unconditionally is not a theme override"
    );
    assert_eq!(
        dark, DARK_INK,
        "the @theme(dark) block must win in the dark theme; the SAME StyledDom \
         is asked twice, because a theme switch does NOT rebuild the cached \
         document DOM this stylesheet lives in"
    );
}

/// The same, through the `<style>` element of a parsed XML document - the
/// path `azwriter::document::markdown_to_content_dom` actually takes. A
/// stylesheet that parses standalone but is dropped by the XML head parser
/// would leave the page light in a dark session, silently.
#[test]
fn a_theme_dark_block_survives_the_xml_style_element() {
    let xml = "<html><head><style>
        p { color: #1a1a1a; }
        @theme(dark) { p { color: #eff0f1; } }
    </style></head><body><p>ink</p></body></html>";

    let component_map = azul_core::xml::ComponentMap::default();
    // The XML path styles internally: the page's <style> is applied here.
    let mut styled_dom = azul_layout::xml::domxml_from_str(xml, &component_map).parsed_dom;

    // The <p> is the PARENT of the only text node.
    let p = styled_dom
        .node_data
        .as_container()
        .linear_iter()
        .find(|id| styled_dom.node_data.as_container()[*id].is_text_node())
        .and_then(|id| styled_dom.node_hierarchy.as_container()[id].parent_id())
        .expect("the parsed document has a <p> with text");

    assert_eq!(
        text_color_at(&mut styled_dom, p, Theme::Light),
        LIGHT_INK,
        "base rule in the light theme"
    );
    assert_eq!(
        text_color_at(&mut styled_dom, p, Theme::Dark),
        DARK_INK,
        "the @theme(dark) block must survive <style> parsing too"
    );
}

/// The APP path, which is not the `domxml_from_str` one: azwriter parses the
/// markdown to XML, calls `Dom::create_from_parsed_xml` (which attaches the
/// `<style>` sheet to `Dom.css` for the cascade to apply later) and hands the
/// resulting Dom back from `layout()`. The engine styles it itself.
///
/// Same stylesheet, one more hop - and the hop is where a conditional block
/// can quietly not survive, because the scoped sheet is applied at flatten
/// time while the dynamic context arrives afterwards.
#[test]
fn a_theme_dark_block_survives_the_dom_from_parsed_xml_path() {
    let xml = "<html><head><style>
        p { color: #1a1a1a; }
        @theme(dark) { p { color: #eff0f1; } }
    </style></head><body><p>ink</p></body></html>";

    let parsed = azul_layout::xml::parse_xml(xml).expect("the fixture parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let mut styled_dom = StyledDom::create_from_dom(dom);

    let p = styled_dom
        .node_data
        .as_container()
        .linear_iter()
        .find(|id| styled_dom.node_data.as_container()[*id].is_text_node())
        .and_then(|id| styled_dom.node_hierarchy.as_container()[id].parent_id())
        .expect("the parsed document has a <p> with text");

    assert_eq!(
        text_color_at(&mut styled_dom, p, Theme::Light),
        LIGHT_INK,
        "base rule in the light theme"
    );
    assert_eq!(
        text_color_at(&mut styled_dom, p, Theme::Dark),
        DARK_INK,
        "the @theme(dark) block must survive create_from_parsed_xml + \
         create_from_dom - this is the path the document sheet takes"
    );
}
