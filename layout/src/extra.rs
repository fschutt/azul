//! Utility functions for color parsing and XML DOM construction.

use azul_core::dom::Dom;
use azul_css::props::basic::color::ColorU;

/// Parses a CSS color string (e.g. `"#ff0000"`, `"red"`) into a `ColorU`.
/// Returns `ColorU::BLACK` if the string cannot be parsed.
pub fn coloru_from_str(s: &str) -> ColorU {
    azul_css::props::basic::color::parse_css_color(s)
        .ok()
        .unwrap_or(ColorU::BLACK)
}

/// Create a Dom (with CSS attached but not applied) from an already-parsed Xml structure.
///
/// Returns an unstyled `Dom` suitable for use in layout callbacks (which return `Dom`,
/// not `StyledDom`). The CSS from `<style>` tags is attached to the `Dom.css` field
/// and will be applied during the cascade pass.
#[cfg(not(feature = "xml"))]
pub fn dom_from_parsed_xml(_xml: azul_core::xml::Xml) -> Dom {
    Dom::create_body()
        .with_children(
            vec![Dom::create_text(format!(
                "library was not compiled with --feature=\"xml\""
            ))]
            .into(),
        )
}

/// Create a Dom (with CSS attached but not applied) from an already-parsed Xml structure.
#[cfg(feature = "xml")]
pub fn dom_from_parsed_xml(xml: azul_core::xml::Xml) -> Dom {
    use azul_core::xml::{str_to_dom_unstyled, ComponentMap};

    let component_map = ComponentMap::with_builtin();
    match str_to_dom_unstyled(xml.root.as_ref(), &component_map) {
        Ok(dom) => dom,
        Err(e) => {
            Dom::create_body()
                .with_children(vec![Dom::create_text(format!("{}", e))].into())
        }
    }
}
