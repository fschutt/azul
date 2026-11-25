//! HTML components that printpdf adds to azul's component map.
//!
//! When printpdf turns HTML into a PDF, azul parses the markup into a DOM and looks up each tag
//! in a `ComponentMap`. azul's builtin library already covers the standard HTML tags
//! (`html`, `body`, `div`, `h1` to `h6`, `p`, `span`, `table`, `a`, `ul`, `li` and so on), so this
//! module only defines two extra components, `<img>` and `<component>`, in a `"printpdf"`
//! library. [`printpdf_default_components`] returns the builtin map with that library appended.
//!
//! The definitions are descriptive: azul's `str_to_dom` maps tags to node types directly and does
//! not call `ComponentDef::render_fn`, so the render functions here only produce an empty
//! `<div>`. Images are drawn later, when the [`crate::html::bridge`] walks the display list.

use azul_core::{
    dom::{Dom, NodeType},
    styled_dom::StyledDom,
    xml::{
        ComponentDataField, ComponentDataModel, ComponentDef, ComponentFieldType, ComponentId,
        ComponentCodegen, ComponentLibrary, ComponentMap, ComponentSource,
        ComponentDefaultValue, OptionComponentDefaultValue, RenderDomError,
        ResultStyledDomRenderDomError,
    },
};
use azul_css::{css::Css, AzString};

// A field is required when it has no default value.
fn data_field(
    name: &str,
    ft: ComponentFieldType,
    default: Option<ComponentDefaultValue>,
    description: &str,
) -> ComponentDataField {
    let required = default.is_none();
    ComponentDataField {
        name: AzString::from(name),
        field_type: ft,
        default_value: match default {
            Some(d) => OptionComponentDefaultValue::Some(d),
            None => OptionComponentDefaultValue::None,
        },
        required,
        description: AzString::from(description),
    }
}

// `render_fn` only sees the typed data model, not the image bytes (those are in
// `XmlRenderOptions::images`), so it renders an empty `<div>`. The bridge draws the image.
fn img_render_fn(
    _def: &ComponentDef,
    _data: &ComponentDataModel,
    _component_map: &ComponentMap,
) -> ResultStyledDomRenderDomError {
    let mut dom = Dom::create_node(NodeType::Div);
    let r: Result<StyledDom, RenderDomError> = Ok(StyledDom::create(&mut dom, Css::empty()));
    r.into()
}

/// The `printpdf:img` component, the `<img>` element.
///
/// Its data model has a required `src` (the key into the image map) and an optional `alt`.
pub fn img_component_def() -> ComponentDef {
    ComponentDef {
        id: ComponentId::new("printpdf", "img"),
        display_name: AzString::from("Image"),
        description: AzString::from("HTML <img> element (embeds an image into the PDF)"),
        css: AzString::from(""),
        source: ComponentSource::Compiled,
        data_model: ComponentDataModel {
            name: AzString::from("ImgData"),
            description: AzString::from("Data model for <img>"),
            fields: vec![
                data_field(
                    "src",
                    ComponentFieldType::String,
                    None,
                    "Image source (URL or embedded image id)",
                ),
                data_field(
                    "alt",
                    ComponentFieldType::String,
                    Some(ComponentDefaultValue::String(AzString::from(""))),
                    "Alternate text",
                ),
            ]
            .into(),
        },
        render_fn: img_render_fn,
        codegen: ComponentCodegen::RenderFunction,
        render_fn_source: None.into(),
    }
}

// `render_fn` is a bare function pointer and the data model holds only typed field values, so
// it cannot carry an XML template to instantiate. It renders an empty `<div>`.
fn dynamic_xml_render_fn(
    _def: &ComponentDef,
    _data: &ComponentDataModel,
    _component_map: &ComponentMap,
) -> ResultStyledDomRenderDomError {
    let mut dom = Dom::create_node(NodeType::Div);
    let r: Result<StyledDom, RenderDomError> = Ok(StyledDom::create(&mut dom, Css::empty()));
    r.into()
}

/// The `printpdf:component` component, a user-defined reusable element
/// (`<component name="..." />`).
///
/// Its data model has a required `name` and an optional `args` string with the raw argument list.
pub fn dynamic_xml_component_def() -> ComponentDef {
    ComponentDef {
        id: ComponentId::new("printpdf", "component"),
        display_name: AzString::from("Component"),
        description: AzString::from("User-defined reusable XML component (<component name=\"…\" />)"),
        css: AzString::from(""),
        source: ComponentSource::UserDefined,
        data_model: ComponentDataModel {
            name: AzString::from("ComponentData"),
            description: AzString::from("Data model for a user-defined <component>"),
            fields: vec![
                data_field(
                    "name",
                    ComponentFieldType::String,
                    None,
                    "Name of the component, e.g. \"test\" for <component name=\"test\" />",
                ),
                data_field(
                    "args",
                    ComponentFieldType::String,
                    Some(ComponentDefaultValue::String(AzString::from(""))),
                    "Raw component argument string, e.g. \"a: String\"",
                ),
            ]
            .into(),
        },
        render_fn: dynamic_xml_render_fn,
        codegen: ComponentCodegen::RenderFunction,
        render_fn_source: None.into(),
    }
}

/// The `"printpdf"` component library, holding `img` and `component`.
pub fn printpdf_component_library() -> ComponentLibrary {
    ComponentLibrary {
        name: AzString::from("printpdf"),
        version: AzString::from("1.0.0"),
        description: AzString::from("printpdf custom components (img, component)"),
        exportable: false,
        modifiable: false,
        data_models: Vec::new().into(),
        enum_models: Vec::new().into(),
        components: vec![img_component_def(), dynamic_xml_component_def()].into(),
    }
}

/// Returns azul's builtin `ComponentMap` with [`printpdf_component_library`] appended.
///
/// The HTML renderer passes this map to azul when it parses a document.
pub fn printpdf_default_components() -> ComponentMap {
    let mut map = ComponentMap::with_builtin();
    let mut libs = std::mem::replace(&mut map.libraries, Vec::new().into()).into_library_owned_vec();
    libs.push(printpdf_component_library());
    map.libraries = libs.into();
    map
}
