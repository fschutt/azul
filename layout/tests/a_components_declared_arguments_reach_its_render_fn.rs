//! A component's declared arguments reach its render fn.
//!
//! User ruling (MAILVIEW, item 1): `<a href>` keeps its link through the
//! `a` COMPONENT's arguments - its data model declares `href`, `target` and
//! `rel` (`azul_core::xml::builtin_data_model`) - not through an ad-hoc
//! entry of the XML attribute table. Until now nothing used them: the XML
//! loaders never filled a component's declared fields from an element's
//! attributes (`xml_attrs_to_data_model` existed and had no caller), and the
//! builtin render fn (`builtin_dom`) read only `text`, so an `a` rendered
//! from its component knew no link.
//!
//! ONE code path fills the declared fields from attributes, for every
//! component - builtin and user alike, each value parsed to its field's type
//! (`azul_core::xml::data_model_with_attributes`) - and the builtin render
//! fn and both XML loaders land the arguments on the node.
//!
//! Not compiled by the author (house rule); expected RED before the fix
//! (the first test fails, the second does not compile yet).

use azul_core::{
    dom::{AttributeNameValue, AttributeType, NodeData, NodeType},
    xml::{ComponentDefaultValue, ComponentMap, ResultStyledDomRenderDomError},
};

fn rendered_node(
    map: &ComponentMap,
    tag: &str,
    data: &azul_core::xml::ComponentDataModel,
    node_type: NodeType,
) -> NodeData {
    let def = map
        .get("builtin", tag)
        .unwrap_or_else(|| panic!("the builtin component <{tag}>"));
    let styled = match (def.render_fn)(def, data, map) {
        ResultStyledDomRenderDomError::Ok(sd) => sd,
        ResultStyledDomRenderDomError::Err(e) => panic!("<{tag}> renders: {e:?}"),
    };
    styled
        .node_data
        .as_ref()
        .iter()
        .find(|n| n.node_type == node_type)
        .cloned()
        .unwrap_or_else(|| panic!("the rendered <{tag}> has its node"))
}

#[test]
fn the_builtin_link_renders_with_its_href_target_and_rel() {
    let map = ComponentMap::with_builtin();
    let def = map.get("builtin", "a").expect("the builtin <a>");
    let data = def
        .data_model
        .clone()
        .with_default(
            "href",
            ComponentDefaultValue::String("https://example.org/q3".into()),
        )
        .with_default("target", ComponentDefaultValue::String("_blank".into()))
        .with_default("rel", ComponentDefaultValue::String("noopener".into()));
    let a = rendered_node(&map, "a", &data, NodeType::A);
    let attrs = a.attributes().as_ref().to_vec();
    assert!(
        attrs.contains(&AttributeType::Href("https://example.org/q3".into())),
        "the render fn of <a> sets its `href` argument: {attrs:?}"
    );
    assert!(
        attrs.contains(&AttributeType::Target("_blank".into())),
        "...and `target`: {attrs:?}"
    );
    assert!(
        attrs.contains(&AttributeType::Rel("noopener".into())),
        "...and `rel`: {attrs:?}"
    );
}

#[test]
fn an_elements_attributes_fill_its_components_declared_fields_by_type() {
    let map = ComponentMap::with_builtin();
    let textarea = map
        .get("builtin", "textarea")
        .expect("the builtin <textarea>");
    // `rows` is declared as an I32 (default 2), `name` as a String;
    // `onclick` is declared nowhere and stays out.
    let args = azul_core::xml::data_model_with_attributes(
        &textarea.data_model,
        [("rows", "7"), ("NAME", "body"), ("onclick", "x()")],
    );
    let field = |name: &str| {
        args.fields
            .as_ref()
            .iter()
            .find(|f| f.name.as_str() == name)
            .map(|f| f.default_value.clone())
    };
    assert_eq!(
        field("rows"),
        Some(azul_core::xml::OptionComponentDefaultValue::Some(
            ComponentDefaultValue::I32(7)
        )),
        "a number attribute is parsed to the field's type"
    );
    assert_eq!(
        field("name"),
        Some(azul_core::xml::OptionComponentDefaultValue::Some(
            ComponentDefaultValue::String("body".into())
        )),
        "attribute names match the declared field case-insensitively, as in HTML"
    );
    assert!(
        field("onclick").is_none(),
        "an undeclared attribute adds no field"
    );
    // ...and the argument reaches the render fn.
    let node = rendered_node(&map, "textarea", &args, NodeType::TextArea);
    let attrs = node.attributes().as_ref().to_vec();
    assert!(
        attrs.contains(&AttributeType::Custom(AttributeNameValue {
            attr_name: "rows".into(),
            value: "7".into(),
        })) || attrs.contains(&AttributeType::Name("body".into())),
        "the filled arguments are the ones the render fn applies: {attrs:?}"
    );
}
