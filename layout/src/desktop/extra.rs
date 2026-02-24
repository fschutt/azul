use azul_core::styled_dom::StyledDom;
use azul_css::props::basic::color::ColorU;

pub fn coloru_from_str(s: &str) -> ColorU {
    azul_css::props::basic::color::parse_css_color(s)
        .ok()
        .unwrap_or(ColorU::BLACK)
}

// extra functions that can't be implemented in azul_core
#[cfg(not(feature = "xml"))]
pub fn styled_dom_from_file(_: &str) -> StyledDom {
    use azul_core::dom::Dom;
    use azul_css::css::Css;

    Dom::create_body()
        .with_children(
            vec![Dom::create_text(format!(
                "library was not compiled with --feature=\"xml\""
            ))]
            .into(),
        )
        .style(Css::empty())
}

#[cfg(feature = "xml")]
pub fn styled_dom_from_file(path: &str) -> StyledDom {
    use crate::xml::ComponentMap;
    crate::xml::domxml_from_file(path, &ComponentMap::with_builtin()).parsed_dom
}

#[cfg(not(feature = "xml"))]
pub fn styled_dom_from_str(_: &str) -> StyledDom {
    use azul_core::dom::Dom;
    use azul_css::css::Css;

    Dom::create_body()
        .with_children(
            vec![Dom::create_text(format!(
                "library was not compiled with --feature=\"xml\""
            ))]
            .into(),
        )
        .style(Css::empty())
}

#[cfg(feature = "xml")]
pub fn styled_dom_from_str(s: &str) -> StyledDom {
    use crate::xml::ComponentMap;
    crate::xml::domxml_from_str(s, &ComponentMap::with_builtin()).parsed_dom
}

/// Create a StyledDom from an already-parsed Xml structure.
/// This avoids re-parsing the XML string.
#[cfg(not(feature = "xml"))]
pub fn styled_dom_from_parsed_xml(_xml: azul_core::xml::Xml) -> StyledDom {
    use azul_core::dom::Dom;
    use azul_css::css::Css;

    Dom::create_body()
        .with_children(
            vec![Dom::create_text(format!(
                "library was not compiled with --feature=\"xml\""
            ))]
            .into(),
        )
        .style(Css::empty())
}

/// Create a StyledDom from an already-parsed Xml structure.
/// This avoids re-parsing the XML string.
#[cfg(feature = "xml")]
pub fn styled_dom_from_parsed_xml(xml: azul_core::xml::Xml) -> StyledDom {
    use azul_core::xml::{str_to_dom, ComponentMap};
    use azul_core::dom::Dom;
    use azul_css::css::Css;
    
    let component_map = ComponentMap::with_builtin();
    match str_to_dom(xml.root.as_ref(), &component_map, None) {
        Ok(styled_dom) => styled_dom,
        Err(e) => {
            Dom::create_body()
                .with_children(vec![Dom::create_text(format!("{}", e))].into())
                .style(Css::empty())
        }
    }
}
