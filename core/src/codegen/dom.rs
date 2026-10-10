//! Lower MARKUP — a subtree of the builder's document, a component's
//! template, a page, pasted HTML — to the language-neutral codegen IR
//! ([`azul_css::codegen::ir`]), which every binding language's printer
//! ([`azul_css::codegen::lang`]) turns into source: AzBuilder's
//! "Subtree → code", "Component → code", "HTML → DOM (code)" and Export > Code.
//!
//! This module only DECIDES; it prints nothing. The decisions: the semantic /
//! accessibility-aware constructor of an element (`analyze_node_ctor`), the
//! zero-argument creator whitelist (`safe_container_tag`), and the CSS rule
//! matching ([`CssMatcher`]) plus the `style` attribute (`inline_css`).
//!
//! An element becomes `Dom::create_<x>(..)` followed by builder methods:
//! `with_css` (every matching rule of the stylesheet, then the node's own
//! `style` attribute), `with_id`, `with_class`, then one `with_child` per
//! child. A link is always `Dom::create_a(href, text, SmallAriaInfo::label(..))`
//! (its text is its accessible name): the `create_a_no_a11y(href,
//! OptionString)` form cannot be called from every binding (Python has no
//! `OptionString` constructor).
//!
//! **Template placeholders.** When the fragment is a TEMPLATE (`params` is
//! `Some`), `{name}` in a text or in an attribute value, where `name` is one
//! of the parameters, becomes [`Expr::Param`] (inside [`Expr::Concat`] when
//! mixed with text); `{{` / `}}` are literal braces and an unknown `{x}`
//! stays literal text — the builder's template rules
//! (`layout/src/e2e/builder.rs::substitute`). Plain markup (`params: None`)
//! keeps every brace as text.
//!
//! **Whitespace.** Runs of whitespace in a text collapse to one space (except
//! under `<pre>`), a whitespace-only text is dropped, and an element's first /
//! last text child loses its leading / trailing space — `<p>Hello <b>you</b></p>`
//! keeps the space before `you`.
//!
//! **Component boundaries.** With [`Components`], a component instance
//! (`<library:name ..>`) is a CALL of that component's own function, never
//! its tree inlined; every component used is lowered once into its own item
//! ([`lower_components_fragment`], [`lower_components_app`],
//! [`lower_component_library`]). How code builds a component is its
//! `ComponentDef::codegen` (`ComponentCodegen`).
//!
//! **Attributes.** What an attribute sets on a node comes from the ONE table
//! the XML loaders use (`azul_core::xml::attributes`): ids / classes /
//! `style` (+ `dir`) as above, the focus as `.with_tab_index(..)`,
//! `contenteditable` as `.with_contenteditable(true)`, the typed attributes as
//! `.with_attribute(AttributeType::..)` (after the element's constructor,
//! without the attributes the constructor took). What code cannot say yet (a
//! `data-l10n` text, a callback named in markup) is a note in the item's doc.
//!
//! Not lowered yet: images (`<img>` becomes a `div`).

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::cell::RefCell;

use azul_css::{
    codegen::ir::{
        AppSpec, ComponentSpec, EnumShape, Expr, Ident, Item, ItemParam, LibrarySpec, Module,
        Prim,
    },
    css::{
        Css, CssDeclaration, CssPath, CssPathPseudoSelector, CssPathSelector, CssRuleBlock,
        NodeTypeTag,
    },
    AzString,
};

use super::render_fn_name;
use crate::{
    dom::{AttributeNameValue, AttributeType, NodeType, TabIndex},
    id::NodeId,
    styled_dom::StyledDom,
    window::{AzStringPair, StringPairVec},
    xml::{
        attributes::{self, NodeSetting},
        collect_style_text, element_draws_nothing, get_body_node, get_html_node, normalize_casing,
        tag_to_node_type, tag_to_node_type_tag, CompileError, ComponentCallCodegen,
        ComponentCodegen, ComponentDataField, ComponentDataModel, ComponentDef,
        ComponentDefaultValue, ComponentFieldType, ComponentMap, OptionComponentDefaultValue,
        ResultStyledDomRenderDomError, XmlAttributeMap, XmlNode, XmlNodeChild,
        MAX_XML_NESTING_DEPTH,
    },
};

// ===========================================================================
// Public API
// ===========================================================================

/// A `{name}` placeholder of a template fragment: a string parameter of the
/// generated render function.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FragmentParam {
    /// The placeholder's name (`text`, `href`, …), as the template writes it.
    pub name: String,
    /// The value the component was made with (a keyword default in the
    /// languages that have them; the registration's fallback).
    pub default: String,
}

impl FragmentParam {
    /// A parameter with its default value.
    #[must_use]
    pub fn new(name: &str, default: &str) -> Self {
        Self {
            name: name.to_string(),
            default: default.to_string(),
        }
    }
}

/// Lower `root_nodes` — a subtree or a component template, NOT a whole page
/// — to one IR item `fn_name` that builds it as a `Dom`, every tag an
/// element (see [`lower_components_fragment`] for markup with component
/// instances).
///
/// `stylesheet` is CSS source whose matching rules become each node's
/// `with_css`. `params`: `Some` lowers a TEMPLATE (`{name}` placeholders
/// become the item's `String` parameters, `{{` / `}}` literal braces);
/// `None` lowers plain markup (braces are text). Several roots, or a text
/// root, are wrapped in one `div`. `doc`: comment lines for the item.
#[must_use]
pub fn lower_xml_fragment(
    root_nodes: &[XmlNodeChild],
    stylesheet: &str,
    fn_name: &str,
    params: Option<&[FragmentParam]>,
    doc: Vec<String>,
) -> Module {
    Module {
        items: vec![lower_item(
            root_nodes,
            stylesheet,
            Ident::from_text(fn_name),
            params,
            doc,
            None,
        )],
        ..Module::default()
    }
}

/// Lower `root_nodes` to an app ([`Module::app`]): one item `render_ui`,
/// which a printer's `emit_project_files` puts in a window titled `title`.
/// A single `<body>` root IS the window's body; anything else goes inside
/// one.
#[must_use]
pub fn lower_xml_fragment_app(root_nodes: &[XmlNodeChild], stylesheet: &str, title: &str) -> Module {
    let mut m = lower_xml_fragment(root_nodes, stylesheet, APP_ROOT, None, Vec::new());
    m.app = Some(app_spec(root_nodes, title, false));
    m
}

/// Lower a whole page (`<html>` with a `<head><style>` and a `<body>`, or a
/// fragment the parser wraps into one) to an app: its body with its
/// stylesheet (every `<style>` block, [`document_style_text`]).
///
/// # Errors
///
/// A document without a body the parser can find.
#[allow(clippy::result_large_err)] // the crate's #[repr(C,u8)] FFI error enum (xml.rs CompileError)
pub fn lower_xml_page_app(root_nodes: &[XmlNodeChild], title: &str) -> Result<Module, CompileError> {
    let html = get_html_node(root_nodes)?;
    let body = get_body_node(html.children.as_ref())?.clone();
    let style = document_style_text(root_nodes);
    Ok(lower_xml_fragment_app(
        &[XmlNodeChild::Element(body)],
        &style,
        title,
    ))
}

/// Every `<style>` block's text in `root_nodes` (the document's head and
/// body), in document order: the stylesheet a pasted page or fragment
/// carries.
#[must_use]
pub fn document_style_text(root_nodes: &[XmlNodeChild]) -> String {
    let mut out: Vec<String> = Vec::new();
    for c in root_nodes {
        if let XmlNodeChild::Element(n) = c {
            if normalize_casing(n.node_type.as_str()) == "style" {
                let t = n.get_text_content();
                if !t.is_empty() {
                    out.push(t);
                }
            } else {
                collect_style_text(n, &mut out, 0);
            }
        }
    }
    out.join("\n")
}

/// Markup as what code builds: a whole document (an `<html>` root, or a
/// root `<body>` / `<head>`) is its `<body>`, anything else the markup
/// itself; with every `<style>` block's text as the stylesheet, and whether
/// it was a document.
///
/// # Errors
/// A document without a body.
pub fn markup_parts(
    root_nodes: &[XmlNodeChild],
) -> Result<(Vec<XmlNodeChild>, String, bool), String> {
    let css = document_style_text(root_nodes);
    let is_page = root_nodes.iter().any(|c| {
        matches!(c, XmlNodeChild::Element(n)
            if ["html", "body", "head"].iter().any(|t| n.node_type.as_str().eq_ignore_ascii_case(t)))
    });
    if !is_page {
        return Ok((root_nodes.to_vec(), css, false));
    }
    let html = get_html_node(root_nodes).map_err(|e| e.to_string())?;
    let body = get_body_node(html.children.as_ref())
        .map_err(|e| e.to_string())?
        .clone();
    Ok((vec![XmlNodeChild::Element(body)], css, true))
}

/// The name a render function of `root_nodes` gets:
/// `render_<first id | first class | tag>` of the only element root, else
/// `render_ui` (also for a lone component instance, whose own function is
/// `render_<name>`).
#[must_use]
pub fn default_fn_name(root_nodes: &[XmlNodeChild]) -> String {
    let Some(n) = single_element_root(root_nodes, true) else {
        return render_fn_name(APP_ROOT_BASE);
    };
    let raw = n.node_type.as_str();
    // A lone component instance is a CALL of the component's own function
    // (`render_<name>`); the fragment around it must not take that name.
    if component_tag(raw).is_some() {
        return render_fn_name(APP_ROOT_BASE);
    }
    let first = |k: &str| {
        n.attributes
            .get_key(k)
            .and_then(|v| v.as_str().split_whitespace().next().map(ToString::to_string))
    };
    let base = first("id")
        .or_else(|| first("class"))
        .unwrap_or_else(|| normalize_casing(raw));
    render_fn_name(&base)
}

/// `render_ui`'s base.
const APP_ROOT_BASE: &str = "ui";

// ===========================================================================
// Component boundaries
// ===========================================================================
//
// With a [`Components`], a tag `<library:name ..>` is an INSTANCE of the
// component `map.get(library, name)`, lowered the way the component's
// `ComponentDef::codegen` says:
//
// - `RenderFunction`: a CALL (`Expr::ItemCall`) of the component's own
//   render function `render_<name>(<fields>)`, which the module defines ONCE
//   from the component's template (placeholders = parameters, nested
//   instances = calls inside it) or, without one, from what it renders. The
//   arguments are the instance's attributes (the builder stores the field
//   values there), a `text` field takes the instance's text, a missing
//   field its default. `class` / `id` / `style` attributes that are not
//   fields style the returned `Dom`; the instance's children are appended.
// - `Element`: the HTML element of that name.
// - `Call`: the widget's constructor in api.json vocabulary
//   (`Button::create(label).dom()`), arguments typed by the data model.
//
// Items come callees first (C and C++ need a function defined before its
// use), the root last. A component that uses itself is cut (an empty div
// with a note), a missing component is an empty div with a note.

/// A component's markup, as the code export reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentMarkup {
    /// The template WITH its `{placeholders}` and nested instances as tags
    /// (`is_template`), or what the component renders with its default data.
    pub nodes: Vec<XmlNodeChild>,
    /// The CSS its elements take (the component's own stylesheet).
    pub css: String,
    pub is_template: bool,
}

impl ComponentMarkup {
    /// What `def` renders with its default data (its `render_fn`), as
    /// markup ([`styled_dom_markup`]).
    ///
    /// # Errors
    /// A failing `render_fn`.
    pub fn rendered(def: &ComponentDef, map: &ComponentMap) -> Result<Self, String> {
        match (def.render_fn)(def, &def.data_model, map) {
            ResultStyledDomRenderDomError::Ok(sd) => Ok(Self {
                nodes: styled_dom_markup(&sd),
                css: def.css.as_str().to_string(),
                is_template: false,
            }),
            ResultStyledDomRenderDomError::Err(e) => Err(format!(
                "render_fn failed for `{}`: {e:?}",
                def.id.qualified_name()
            )),
        }
    }
}

/// What the lowering knows about components: the component map (a tag
/// `<library:name>` is an instance of `map.get(library, name)`) and where a
/// component's TEMPLATE comes from. Core has no XML parser: a caller that
/// has templates (AzBuilder's) parses them; `template` answers `None` for a
/// component without one, which is then exported as what it renders.
pub struct Components<'a> {
    pub map: &'a ComponentMap,
    pub template: &'a dyn Fn(&ComponentDef) -> Option<Result<ComponentMarkup, String>>,
}

impl<'a> Components<'a> {
    /// No templates: every component is exported as what it renders.
    #[must_use]
    pub fn rendered_only(map: &'a ComponentMap) -> Self {
        Self {
            map,
            template: &no_template,
        }
    }
}

fn no_template(_: &ComponentDef) -> Option<Result<ComponentMarkup, String>> {
    None
}

/// Lower `root_nodes` (plain markup that may hold component instances) to a
/// module: one item per component it uses (each lowered once, callees
/// first), then item `fn_name` that builds the markup and calls them.
#[must_use]
pub fn lower_components_fragment(
    root_nodes: &[XmlNodeChild],
    stylesheet: &str,
    fn_name: &str,
    doc: Vec<String>,
    components: &Components<'_>,
) -> Module {
    let reg = Registry::new(components, fn_name);
    let root = lower_item(
        root_nodes,
        stylesheet,
        Ident::from_text(fn_name),
        None,
        doc,
        Some(&reg),
    );
    Module {
        items: reg.finish(root),
        ..Module::default()
    }
}

/// [`lower_components_fragment`] as an app ([`Module::app`], root
/// `render_ui`, see [`lower_xml_fragment_app`]).
#[must_use]
pub fn lower_components_app(
    root_nodes: &[XmlNodeChild],
    stylesheet: &str,
    title: &str,
    components: &Components<'_>,
) -> Module {
    let mut m = lower_components_fragment(root_nodes, stylesheet, APP_ROOT, Vec::new(), components);
    m.app = Some(app_spec(root_nodes, title, true));
    m
}

/// The component library `library`: one item per component of `defs` (its
/// render function) plus the components they use, and the
/// [`LibrarySpec`] the printers register `defs` from. `warnings`: a
/// component without a template, one that fails to render.
#[must_use]
pub fn lower_component_library(
    library: &str,
    version: &str,
    defs: &[&ComponentDef],
    components: &Components<'_>,
    warnings: &mut Vec<String>,
) -> Module {
    let reg = Registry::new(components, "");
    let mut specs = Vec::new();
    for def in defs {
        let Some(d) = reg.define(def) else {
            continue;
        };
        let (_, descriptions) = component_params(&def.data_model);
        specs.push(ComponentSpec {
            item: d.item,
            name: def.id.name.as_str().to_string(),
            display_name: def.display_name.as_str().to_string(),
            description: def.description.as_str().to_string(),
            data_model: def.data_model.name.as_str().to_string(),
            data_model_description: def.data_model.description.as_str().to_string(),
            field_descriptions: if d.params.is_empty() {
                Vec::new()
            } else {
                descriptions
            },
        });
    }
    warnings.extend(reg.warnings.take());
    Module {
        items: reg.items.into_inner(),
        app: None,
        library: Some(LibrarySpec {
            name: library.to_string(),
            version: version.to_string(),
            components: specs,
        }),
    }
}

/// A component's parameters: every data-model field that is a value (not a
/// callback, not a child slot), with its default as text, and each field's
/// description. A render function takes them as strings, in this order (the
/// builder's templates substitute text).
#[must_use]
pub fn component_params(dm: &ComponentDataModel) -> (Vec<FragmentParam>, Vec<String>) {
    dm.fields
        .as_ref()
        .iter()
        .filter(|f| {
            !matches!(
                f.field_type,
                ComponentFieldType::Callback(_) | ComponentFieldType::StyledDom
            )
        })
        .map(|f| {
            (
                FragmentParam::new(f.name.as_str(), &default_text(&f.default_value)),
                f.description.as_str().to_string(),
            )
        })
        .unzip()
}

/// A field's default value as text (the way a template substitutes it).
#[must_use]
pub fn default_text(v: &OptionComponentDefaultValue) -> String {
    match v {
        OptionComponentDefaultValue::Some(d) => match d {
            ComponentDefaultValue::String(s)
            | ComponentDefaultValue::CallbackFnPointer(s)
            | ComponentDefaultValue::Json(s) => s.as_str().to_string(),
            ComponentDefaultValue::Bool(b) => b.to_string(),
            ComponentDefaultValue::I32(n) => n.to_string(),
            ComponentDefaultValue::I64(n) => n.to_string(),
            ComponentDefaultValue::U32(n) => n.to_string(),
            ComponentDefaultValue::U64(n) => n.to_string(),
            ComponentDefaultValue::Usize(n) => n.to_string(),
            ComponentDefaultValue::F32(n) => n.to_string(),
            ComponentDefaultValue::F64(n) => n.to_string(),
            ComponentDefaultValue::ColorU(c) => {
                format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
            }
            ComponentDefaultValue::None | ComponentDefaultValue::ComponentInstance(_) => {
                String::new()
            }
        },
        OptionComponentDefaultValue::None => String::new(),
    }
}

/// A rendered `StyledDom` as markup, the code export's view of a DOM: every
/// element with its tag, classes and ids, and its text (whitespace-only text
/// is left out). What [`ComponentMarkup::rendered`] lowers; a caller with a
/// DOM instead of markup lowers this.
#[must_use]
pub fn styled_dom_markup(sd: &StyledDom) -> Vec<XmlNodeChild> {
    let mut budget = sd.node_data.as_ref().len();
    if budget == 0 {
        return Vec::new();
    }
    markup_node(sd, NodeId::new(0), 0, &mut budget)
        .into_iter()
        .collect()
}

fn markup_node(sd: &StyledDom, id: NodeId, depth: usize, budget: &mut usize) -> Option<XmlNodeChild> {
    let nd = sd.node_data.as_ref().get(id.index())?;
    if let NodeType::Text(t) = nd.get_node_type() {
        let s = t.as_str();
        return (!s.trim().is_empty()).then(|| XmlNodeChild::Text(AzString::from(s)));
    }
    let mut classes: Vec<&str> = Vec::new();
    let mut ids: Vec<&str> = Vec::new();
    for attr in nd.attributes().as_ref() {
        if let Some(c) = attr.as_class() {
            classes.push(c);
        } else if let Some(i) = attr.as_id() {
            ids.push(i);
        }
    }
    let mut attrs: Vec<AzStringPair> = Vec::new();
    if !ids.is_empty() {
        attrs.push(AzStringPair::create(
            AzString::from("id"),
            AzString::from(ids.join(" ").as_str()),
        ));
    }
    if !classes.is_empty() {
        attrs.push(AzStringPair::create(
            AzString::from("class"),
            AzString::from(classes.join(" ").as_str()),
        ));
    }
    let mut children: Vec<XmlNodeChild> = Vec::new();
    if depth < MAX_XML_NESTING_DEPTH {
        let hierarchy = sd.node_hierarchy.as_ref();
        let mut next = hierarchy.get(id.index()).and_then(|h| h.first_child_id(id));
        while let Some(c) = next {
            // Bounded by the node count: a corrupted sibling chain must not
            // loop forever.
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            if let Some(x) = markup_node(sd, c, depth + 1, budget) {
                children.push(x);
            }
            next = hierarchy.get(c.index()).and_then(|h| h.next_sibling_id());
        }
    }
    Some(XmlNodeChild::Element(XmlNode {
        node_type: nd.get_node_type().get_path().to_string().into(),
        attributes: XmlAttributeMap::from(StringPairVec::from_vec(attrs)),
        children: children.into(),
    }))
}

/// The root item of an app.
const APP_ROOT: &str = "render_ui";

fn app_spec(root_nodes: &[XmlNodeChild], title: &str, instances: bool) -> AppSpec {
    let is_body = single_element_root(root_nodes, instances)
        .is_some_and(|n| normalize_casing(n.node_type.as_str()) == "body");
    AppSpec {
        title: title.to_string(),
        root: Ident::from_text(APP_ROOT),
        is_body,
    }
}

fn parse_stylesheet(stylesheet: &str) -> Css {
    let mut css = if stylesheet.trim().is_empty() {
        Css::empty()
    } else {
        azul_css::parser2::new_from_str(stylesheet).0
    };
    css.sort_by_specificity();
    css
}

/// One item: `nodes` lowered with `stylesheet` (and `params` for a
/// template); the notes of the lowering (a missing component, a cut
/// recursion) join its doc.
fn lower_item(
    nodes: &[XmlNodeChild],
    stylesheet: &str,
    name: Ident,
    params: Option<&[FragmentParam]>,
    mut doc: Vec<String>,
    reg: Option<&Registry<'_>>,
) -> Item {
    let css = parse_stylesheet(stylesheet);
    let notes = RefCell::new(Vec::new());
    let value = lower_roots(nodes, &css, params, reg, &notes);
    for n in notes.into_inner() {
        if !doc.contains(&n) {
            doc.push(n);
        }
    }
    Item {
        name,
        doc,
        ty: "Dom".to_string(),
        params: params
            .unwrap_or(&[])
            .iter()
            .map(|p| ItemParam::string(&p.name, &p.default))
            .collect(),
        value,
    }
}

/// A component's item, once lowered: its name and its parameters (what a
/// call passes, in order).
#[derive(Debug, Clone)]
struct Def {
    item: Ident,
    params: Vec<FragmentParam>,
}

/// Every component a module calls, lowered once each into its own item.
struct Registry<'a> {
    components: &'a Components<'a>,
    /// qualified name → its item; `None` while it is being lowered (a use
    /// inside it is a recursion).
    defined: RefCell<BTreeMap<String, Option<Def>>>,
    /// Item names taken → the component that has it (`""`: the root).
    names: RefCell<BTreeMap<String, String>>,
    /// Finished items, callees first.
    items: RefCell<Vec<Item>>,
    warnings: RefCell<Vec<String>>,
}

impl<'a> Registry<'a> {
    fn new(components: &'a Components<'a>, root: &str) -> Self {
        let mut names = BTreeMap::new();
        if !root.is_empty() {
            names.insert(Ident::from_text(root).snake(), String::new());
        }
        Self {
            components,
            defined: RefCell::new(BTreeMap::new()),
            names: RefCell::new(names),
            items: RefCell::new(Vec::new()),
            warnings: RefCell::new(Vec::new()),
        }
    }

    /// The item of `def`, lowered first if needed. `None` while `def` is
    /// being lowered: a component that uses itself.
    fn define(&self, def: &ComponentDef) -> Option<Def> {
        let key = def.id.qualified_name();
        if let Some(state) = self.defined.borrow().get(&key) {
            return state.clone();
        }
        self.defined.borrow_mut().insert(key.clone(), None);
        let item_name = self.item_name(def, &key);
        let title = format!("`{key}` ({})", def.display_name.as_str());
        let markup = (self.components.template)(def)
            .unwrap_or_else(|| ComponentMarkup::rendered(def, self.components.map));
        let (params, item) = match markup {
            Ok(m) => {
                let mut doc = vec![title];
                let params = if m.is_template {
                    component_params(&def.data_model).0
                } else {
                    doc.push("what it renders with its default data (it has no template)".to_string());
                    self.warnings.borrow_mut().push(format!(
                        "{key} has no template (it was not made in AzBuilder): exported what it \
                         renders with its default data"
                    ));
                    Vec::new()
                };
                let item = lower_item(
                    &m.nodes,
                    &m.css,
                    item_name.clone(),
                    m.is_template.then_some(params.as_slice()),
                    doc,
                    Some(self),
                );
                (params, item)
            }
            Err(e) => {
                self.warnings.borrow_mut().push(format!("{key}: {e}"));
                (
                    Vec::new(),
                    Item {
                        name: item_name.clone(),
                        doc: vec![title, format!("could not be exported: {e}")],
                        ty: "Dom".to_string(),
                        params: Vec::new(),
                        value: dom_call("create_div", Vec::new()),
                    },
                )
            }
        };
        self.items.borrow_mut().push(item);
        let d = Def {
            item: item_name,
            params,
        };
        self.defined.borrow_mut().insert(key, Some(d.clone()));
        Some(d)
    }

    /// `render_<name>`, or `render_<library>_<name>` when another component
    /// (or the root) already has that name.
    fn item_name(&self, def: &ComponentDef, key: &str) -> Ident {
        let short = render_fn_name(def.id.name.as_str());
        let mut names = self.names.borrow_mut();
        let name = match names.get(&short) {
            Some(owner) if owner != key => render_fn_name(&format!(
                "{}_{}",
                def.id.collection.as_str(),
                def.id.name.as_str()
            )),
            _ => short,
        };
        names.insert(name.clone(), key.to_string());
        Ident::from_text(&name)
    }

    /// The module's items: every component's, then `root`.
    fn finish(self, root: Item) -> Vec<Item> {
        let mut items = self.items.into_inner();
        items.push(root);
        items
    }
}

/// `(library, name)` of an instance tag `<library:name>` (`svg:` / `html:`
/// / `xhtml:` are namespaces of elements, not libraries).
fn component_tag(raw: &str) -> Option<(&str, &str)> {
    let (library, name) = raw.split_once(':')?;
    if library.is_empty()
        || name.is_empty()
        || matches!(library.to_ascii_lowercase().as_str(), "svg" | "html" | "xhtml")
    {
        return None;
    }
    Some((library, name))
}

/// The value of a data-model field `f` as the instance's text `raw` says,
/// typed by the field (a `Call` codegen passes typed arguments).
fn typed_value(lower: &Lower<'_, '_>, f: &ComponentDataField, raw: &str) -> Expr {
    let r = raw.trim();
    match &f.field_type {
        ComponentFieldType::String => lower.text(raw),
        ComponentFieldType::Bool => r.parse::<bool>().map_or_else(|_| default_value(f), Expr::Bool),
        ComponentFieldType::I32 => r
            .parse::<i32>()
            .map_or_else(|_| default_value(f), |n| Expr::int(i128::from(n), Prim::I32)),
        ComponentFieldType::I64 => r
            .parse::<i64>()
            .map_or_else(|_| default_value(f), |n| Expr::int(i128::from(n), Prim::I64)),
        ComponentFieldType::U32 => r
            .parse::<u32>()
            .map_or_else(|_| default_value(f), |n| Expr::int(i128::from(n), Prim::U32)),
        ComponentFieldType::U64 => r
            .parse::<u64>()
            .map_or_else(|_| default_value(f), |n| Expr::int(i128::from(n), Prim::U64)),
        ComponentFieldType::Usize => r.parse::<u64>().map_or_else(
            |_| default_value(f),
            |n| Expr::int(i128::from(n), Prim::Usize),
        ),
        ComponentFieldType::F32 => r.parse::<f32>().map_or_else(|_| default_value(f), Expr::f32),
        ComponentFieldType::F64 => r.parse::<f64>().map_or_else(|_| default_value(f), Expr::f64),
        other => Expr::unsupported(&format!(
            "a {other} value for `{}` (only text, bool and number fields are passed)",
            f.name.as_str()
        )),
    }
}

/// A field's default as a typed value.
fn default_value(f: &ComponentDataField) -> Expr {
    match &f.default_value {
        OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) => Expr::str(s.as_str()),
        OptionComponentDefaultValue::Some(ComponentDefaultValue::Bool(b)) => Expr::Bool(*b),
        OptionComponentDefaultValue::Some(ComponentDefaultValue::I32(n)) => {
            Expr::int(i128::from(*n), Prim::I32)
        }
        OptionComponentDefaultValue::Some(ComponentDefaultValue::I64(n)) => {
            Expr::int(i128::from(*n), Prim::I64)
        }
        OptionComponentDefaultValue::Some(ComponentDefaultValue::U32(n)) => {
            Expr::int(i128::from(*n), Prim::U32)
        }
        OptionComponentDefaultValue::Some(ComponentDefaultValue::U64(n)) => {
            Expr::int(i128::from(*n), Prim::U64)
        }
        OptionComponentDefaultValue::Some(ComponentDefaultValue::Usize(n)) => {
            Expr::int(i128::try_from(*n).unwrap_or_default(), Prim::Usize)
        }
        OptionComponentDefaultValue::Some(ComponentDefaultValue::F32(n)) => Expr::f32(*n),
        OptionComponentDefaultValue::Some(ComponentDefaultValue::F64(n)) => Expr::f64(*n),
        _ => match f.field_type {
            ComponentFieldType::String => Expr::str(""),
            ComponentFieldType::Bool => Expr::Bool(false),
            _ => Expr::unsupported(&format!("`{}` has no default value", f.name.as_str())),
        },
    }
}

// ===========================================================================
// Strings
// ===========================================================================

/// A text or attribute value: [`Expr::Str`], [`Expr::Param`], or an
/// [`Expr::Concat`] of both — placeholders only in a template.
fn text_expr(raw: &str, params: Option<&[FragmentParam]>) -> Expr {
    match params {
        None => Expr::str(raw),
        Some(params) => split_template(raw, params),
    }
}

/// `{name}` → the parameter, `{{` / `}}` → a literal brace, anything else
/// literal (the builder's `substitute` rules).
fn split_template(raw: &str, params: &[FragmentParam]) -> Expr {
    let mut parts: Vec<Expr> = Vec::new();
    let mut lit = String::new();
    let mut rest = raw;
    while let Some(pos) = rest.find(|c: char| c == '{' || c == '}') {
        lit.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        if tail.starts_with("{{") {
            lit.push('{');
            rest = &tail[2..];
            continue;
        }
        if tail.starts_with("}}") {
            lit.push('}');
            rest = &tail[2..];
            continue;
        }
        if tail.starts_with('{') {
            if let Some(end) = tail.find('}') {
                let name = &tail[1..end];
                if params.iter().any(|p| p.name == name) {
                    if !lit.is_empty() {
                        parts.push(Expr::str(&core::mem::take(&mut lit)));
                    }
                    parts.push(Expr::param(name));
                    rest = &tail[end + 1..];
                    continue;
                }
            }
        }
        lit.push_str(&tail[..1]);
        rest = &tail[1..];
    }
    lit.push_str(rest);
    if !lit.is_empty() || parts.is_empty() {
        parts.push(Expr::str(&lit));
    }
    Expr::concat(parts)
}

/// `true` if the expression is only whitespace text.
fn is_blank(e: &Expr) -> bool {
    match e {
        Expr::Str(s) => s.trim().is_empty(),
        Expr::Concat(parts) => parts.iter().all(is_blank),
        _ => false,
    }
}

// ===========================================================================
// Lowering
// ===========================================================================

/// Elements that are document plumbing, not content.
const SKIPPED_TAGS: &[&str] = &[
    "style", "script", "head", "title", "meta", "link", "base", "template",
];

fn dom_call(method: &str, args: Vec<Expr>) -> Expr {
    Expr::call("Dom", method, args)
}

fn with(recv: Expr, method: &str, args: Vec<Expr>) -> Expr {
    Expr::method(recv, "Dom", method, args)
}

fn text_node(text: Expr) -> Expr {
    dom_call("create_text_do_not_use_without_block_level_wrapper", vec![text])
}

struct Lower<'a, 'r> {
    css: &'a Css,
    params: Option<&'a [FragmentParam]>,
    /// `Some`: tags `<library:name>` are component instances.
    reg: Option<&'a Registry<'r>>,
    /// Notes for the item's doc (a missing component, a cut recursion).
    notes: &'a RefCell<Vec<String>>,
}

fn lower_roots(
    root_nodes: &[XmlNodeChild],
    css: &Css,
    params: Option<&[FragmentParam]>,
    reg: Option<&Registry<'_>>,
    notes: &RefCell<Vec<String>>,
) -> Expr {
    let lower = Lower {
        css,
        params,
        reg,
        notes,
    };
    // The fragment sits where it would in a page: inside `<body>`, so a rule
    // like `body .card` or `.card > p` matches the way it does when mounted.
    let base = CssMatcher {
        path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
        indices_in_parent: vec![0],
        children_length: vec![1],
    };
    if let Some(root) = single_element_root(root_nodes, reg.is_some()) {
        if let Some(e) = lower.element(root, child_matcher(&base, 0, 1), 0, false) {
            return e;
        }
    }
    // Several roots, a text root, or nothing: one `div` around them.
    let visible: Vec<&XmlNodeChild> = root_nodes.iter().collect();
    lower
        .children(&visible, &base, 0, false)
        .into_iter()
        .fold(dom_call("create_div", Vec::new()), |acc, c| {
            with(acc, "with_child", vec![c])
        })
}

/// The only element among `roots` when there is exactly one and no
/// non-blank text beside it. `instances`: a component instance
/// (`<library:name>`) counts as an element.
fn single_element_root(roots: &[XmlNodeChild], instances: bool) -> Option<&XmlNode> {
    let mut found = None;
    for c in roots {
        match c {
            XmlNodeChild::Element(n) => {
                let raw = n.node_type.as_str();
                let tag = normalize_casing(raw);
                let is_instance = instances && component_tag(raw).is_some();
                if !is_instance
                    && (SKIPPED_TAGS.contains(&tag.as_str()) || element_draws_nothing(raw, &tag))
                {
                    continue;
                }
                if found.is_some() {
                    return None;
                }
                found = Some(n);
            }
            XmlNodeChild::Text(t) => {
                if !t.as_str().trim().is_empty() {
                    return None;
                }
            }
        }
    }
    found
}

fn child_matcher(parent: &CssMatcher, idx: usize, len: usize) -> CssMatcher {
    let mut m = parent.clone();
    m.path.push(CssPathSelector::Children);
    m.indices_in_parent.push(idx);
    m.children_length.push(len);
    m
}

fn split_words(v: Option<&azul_css::AzString>) -> Vec<String> {
    v.map(|s| s.as_str().split_whitespace().map(ToString::to_string).collect())
        .unwrap_or_default()
}

impl Lower<'_, '_> {
    fn text(&self, raw: &str) -> Expr {
        text_expr(raw, self.params)
    }

    fn note(&self, note: String) {
        let mut notes = self.notes.borrow_mut();
        if !notes.contains(&note) {
            notes.push(note);
        }
    }

    /// The constructor call of an element.
    fn ctor(&self, tag: &str, analysed: &NodeCtor) -> Expr {
        match analysed {
            NodeCtor::Plain => {
                let dbg = format!("{:?}", tag_to_node_type(tag));
                let t = safe_container_tag(&dbg);
                dom_call(&format!("create_{}", t.to_lowercase()), Vec::new())
            }
            NodeCtor::Semantic { suffix, args, .. } => {
                if suffix == "ANoA11y" {
                    // `create_a(href, text, label(text))`: see the module docs.
                    let href = match args.first() {
                        Some(CtorArg::Str(s)) => self.text(s),
                        _ => Expr::str(""),
                    };
                    let text = match args.get(1) {
                        Some(CtorArg::OptSome(s)) => self.text(s),
                        _ => href.clone(),
                    };
                    let label = Expr::call("SmallAriaInfo", "label", vec![text.clone()]);
                    return dom_call("create_a", vec![href, text, label]);
                }
                let args = args
                    .iter()
                    .map(|a| match a {
                        CtorArg::Str(s) => self.text(s),
                        CtorArg::Aria(s) => {
                            Expr::call("SmallAriaInfo", "label", vec![self.text(s)])
                        }
                        CtorArg::Float(f) => Expr::f32(*f),
                        // Only `ANoA11y` takes an OptionString (handled above).
                        CtorArg::OptSome(s) => self.text(s),
                        CtorArg::OptNone => Expr::str(""),
                    })
                    .collect();
                dom_call(&format!("create_{}", camel_to_snake(suffix)), args)
            }
        }
    }

    /// An element, or a component instance (with components).
    fn element(
        &self,
        node: &XmlNode,
        matcher: CssMatcher,
        depth: usize,
        in_pre: bool,
    ) -> Option<Expr> {
        let raw_tag = node.node_type.as_str();
        if let Some(reg) = self.reg {
            if let Some((library, name)) = component_tag(raw_tag) {
                return Some(self.instance(reg, library, name, node, matcher, depth, in_pre));
            }
        }
        self.html_element(node, raw_tag, &normalize_casing(raw_tag), matcher, depth, in_pre)
    }

    /// An HTML element `tag` (`raw_tag` as written: it decides whether the
    /// element draws at all).
    fn html_element(
        &self,
        node: &XmlNode,
        raw_tag: &str,
        tag: &str,
        mut matcher: CssMatcher,
        depth: usize,
        in_pre: bool,
    ) -> Option<Expr> {
        if SKIPPED_TAGS.contains(&tag) || element_draws_nothing(raw_tag, tag) {
            return None;
        }

        let analysed = analyze_node_ctor(tag, node);
        let (consumes_text, skip_caption) = match &analysed {
            NodeCtor::Plain => (false, false),
            NodeCtor::Semantic {
                consumes_text,
                skip_caption,
                ..
            } => (*consumes_text, *skip_caption),
        };
        let mut e = self.ctor(tag, &analysed);

        // What the attributes set, from the table the loaders use; the ones
        // the constructor took are not set twice.
        let taken = ctor_attributes(tag, &analysed);
        let settings = attributes::ordered(node.attributes.as_slice().iter().filter_map(|p| {
            let name = p.key.as_str().trim().to_ascii_lowercase();
            if taken.contains(&name.as_str()) {
                return None;
            }
            attributes::setting_of(tag, &name, p.value.as_str())
        }));
        let mut ids: Vec<String> = Vec::new();
        let mut classes: Vec<String> = Vec::new();
        let mut direction = None;
        let mut style: Option<String> = None;
        let mut calls: Vec<(&'static str, Expr)> = Vec::new();
        for setting in settings {
            match setting {
                NodeSetting::Ids(v) => ids.extend(v.iter().map(|x| x.as_str().to_string())),
                NodeSetting::Classes(v) => {
                    classes.extend(v.iter().map(|x| x.as_str().to_string()));
                }
                NodeSetting::Direction(d) => direction = Some(d),
                NodeSetting::Style(v) => style = Some(v.as_str().to_string()),
                NodeSetting::TabIndex(t) => calls.push(("with_tab_index", tab_index_expr(t))),
                NodeSetting::Editable => calls.push(("with_contenteditable", Expr::Bool(true))),
                NodeSetting::Attribute(a) => calls.push(("with_attribute", attribute_expr(&a))),
                NodeSetting::NotExported(why) => self.note(format!("<{tag}> {}", why.as_str())),
            }
        }
        matcher
            .path
            .push(CssPathSelector::Type(tag_to_node_type_tag(tag)));
        matcher
            .path
            .extend(ids.iter().map(|i| CssPathSelector::Id(i.clone().into())));
        matcher
            .path
            .extend(classes.iter().map(|c| CssPathSelector::Class(c.clone().into())));

        let css = inline_css(
            &get_css_blocks(self.css, &matcher),
            direction,
            style.as_deref(),
        );
        if !css.is_empty() {
            e = with(e, "with_css", vec![Expr::str(&css)]);
        }
        for id in &ids {
            e = with(e, "with_id", vec![Expr::str(id)]);
        }
        for class in &classes {
            e = with(e, "with_class", vec![Expr::str(class)]);
        }
        for (method, arg) in calls {
            e = with(e, method, vec![arg]);
        }

        if depth < MAX_XML_NESTING_DEPTH {
            let in_pre = in_pre || tag == "pre";
            let kids = node.children.as_ref();
            let mut caption_skipped = false;
            let visible: Vec<&XmlNodeChild> = kids
                .iter()
                .filter(|c| match c {
                    XmlNodeChild::Text(_) => !consumes_text,
                    XmlNodeChild::Element(el) => {
                        if skip_caption
                            && !caption_skipped
                            && el.node_type.as_str().eq_ignore_ascii_case("caption")
                        {
                            caption_skipped = true;
                            false
                        } else {
                            true
                        }
                    }
                })
                .collect();
            for c in self.children(&visible, &matcher, depth + 1, in_pre) {
                e = with(e, "with_child", vec![c]);
            }
        }
        Some(e)
    }

    /// A component instance `<library:name ..>`: see "Component boundaries".
    #[allow(clippy::too_many_arguments)]
    fn instance(
        &self,
        reg: &Registry<'_>,
        library: &str,
        name: &str,
        node: &XmlNode,
        mut matcher: CssMatcher,
        depth: usize,
        in_pre: bool,
    ) -> Expr {
        let key = format!("{library}:{name}");
        let Some(def) = reg.components.map.get(library, name) else {
            self.note(format!("missing component `{key}`: an empty div stands for it"));
            return dom_call("create_div", Vec::new());
        };
        let text = node_direct_text(node);
        let (mut e, text_used) = match &def.codegen {
            ComponentCodegen::Element => {
                return self
                    .html_element(node, name, &normalize_casing(name), matcher, depth, in_pre)
                    .unwrap_or_else(|| dom_call("create_div", Vec::new()));
            }
            ComponentCodegen::Call(call) => self.widget(call, def, node, &text),
            ComponentCodegen::RenderFunction => {
                let Some(d) = reg.define(def) else {
                    self.note(format!(
                        "cut a recursive use of `{key}`: an empty div stands for it"
                    ));
                    return dom_call("create_div", Vec::new());
                };
                let mut text_used = false;
                let args = d
                    .params
                    .iter()
                    .map(|p| {
                        if let Some(v) = node.attributes.get_key(&p.name) {
                            self.text(v.as_str())
                        } else if p.name == "text" && !text.is_empty() {
                            text_used = true;
                            self.text(&text)
                        } else {
                            Expr::str(&p.default)
                        }
                    })
                    .collect();
                (
                    Expr::ItemCall {
                        item: d.item,
                        params: d.params.iter().map(|p| Ident::from_text(&p.name)).collect(),
                        args,
                    },
                    text_used,
                )
            }
        };

        // `class` / `id` / `style` that are not fields style the returned
        // Dom (the builder passes them through to the component's root).
        let is_field = |k: &str| def.data_model.get_field(k).is_some();
        let ids = if is_field("id") {
            Vec::new()
        } else {
            split_words(node.attributes.get_key("id"))
        };
        let classes = if is_field("class") {
            Vec::new()
        } else {
            split_words(node.attributes.get_key("class"))
        };
        matcher
            .path
            .push(CssPathSelector::Type(tag_to_node_type_tag(&normalize_casing(name))));
        matcher
            .path
            .extend(ids.iter().map(|i| CssPathSelector::Id(i.clone().into())));
        matcher
            .path
            .extend(classes.iter().map(|c| CssPathSelector::Class(c.clone().into())));
        let style = if is_field("style") {
            None
        } else {
            node.attributes.get_key("style").map(|s| s.as_str())
        };
        let css = inline_css(&get_css_blocks(self.css, &matcher), None, style);
        if !css.is_empty() {
            e = with(e, "with_css", vec![Expr::str(&css)]);
        }
        for id in &ids {
            e = with(e, "with_id", vec![Expr::str(id)]);
        }
        for class in &classes {
            e = with(e, "with_class", vec![Expr::str(class)]);
        }

        // The instance's children are appended to what the component builds.
        if depth < MAX_XML_NESTING_DEPTH {
            let kids: Vec<&XmlNodeChild> = node
                .children
                .as_ref()
                .iter()
                .filter(|c| !(text_used && matches!(c, XmlNodeChild::Text(_))))
                .collect();
            for c in self.children(&kids, &matcher, depth + 1, in_pre) {
                e = with(e, "with_child", vec![c]);
            }
        }
        e
    }

    /// A widget's constructor in api.json vocabulary (`ComponentCodegen::Call`):
    /// its arguments typed by the data model, a setter per field the instance
    /// sets, then the finishing method. Also whether it took the instance's
    /// text (a `text` argument).
    fn widget(
        &self,
        call: &ComponentCallCodegen,
        def: &ComponentDef,
        node: &XmlNode,
        text: &str,
    ) -> (Expr, bool) {
        let class = call.class.as_str();
        let mut text_used = false;
        let mut value = |field: &str| -> Expr {
            let raw = match node.attributes.get_key(field) {
                Some(v) => Some(v.as_str().to_string()),
                None if field == "text" && !text.is_empty() => {
                    text_used = true;
                    Some(text.to_string())
                }
                None => None,
            };
            match (def.data_model.get_field(field), raw) {
                (Some(f), Some(r)) => typed_value(self, f, &r),
                (Some(f), None) => default_value(f),
                (None, Some(r)) => self.text(&r),
                (None, None) => Expr::str(""),
            }
        };
        let args: Vec<Expr> = call.args.as_ref().iter().map(|a| value(a.as_str())).collect();
        let mut e = Expr::call(class, call.constructor.as_str(), args);
        for setter in call.setters.as_ref() {
            let field = setter.key.as_str();
            if node.attributes.get_key(field).is_some() {
                e = Expr::method(e, class, setter.value.as_str(), vec![value(field)]);
            }
        }
        if !call.finish.as_str().is_empty() {
            e = Expr::method(e, class, call.finish.as_str(), Vec::new());
        }
        (e, text_used)
    }

    /// `kids`: the children to emit. Structural selectors (`:first-child`,
    /// `:nth-child`) count ELEMENTS, as in CSS, so the matcher indexes the
    /// element children only.
    fn children(
        &self,
        kids: &[&XmlNodeChild],
        parent: &CssMatcher,
        depth: usize,
        in_pre: bool,
    ) -> Vec<Expr> {
        let last = kids.len().saturating_sub(1);
        let elements = kids
            .iter()
            .filter(|c| matches!(c, XmlNodeChild::Element(_)))
            .count();
        let mut element_idx = 0;
        let mut out = Vec::new();
        for (i, c) in kids.iter().enumerate() {
            match c {
                XmlNodeChild::Element(el) => {
                    let m = child_matcher(parent, element_idx, elements);
                    element_idx += 1;
                    if let Some(x) = self.element(el, m, depth, in_pre) {
                        out.push(x);
                    }
                }
                XmlNodeChild::Text(t) => {
                    let mut s = if in_pre {
                        t.as_str().to_string()
                    } else {
                        collapse_whitespace(t.as_str())
                    };
                    if !in_pre {
                        if i == 0 {
                            s = s.trim_start().to_string();
                        }
                        if i == last {
                            s = s.trim_end().to_string();
                        }
                    }
                    let text = self.text(&s);
                    if !is_blank(&text) {
                        out.push(text_node(text));
                    }
                }
            }
        }
        out
    }
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out
}

// ===========================================================================
// Shared decisions: CSS matching and the constructor of an element
// ===========================================================================

#[derive(Debug, Clone)]
pub struct CssMatcher {
    path: Vec<CssPathSelector>,
    indices_in_parent: Vec<usize>,
    children_length: Vec<usize>,
}

impl CssMatcher {
    fn matches(&self, path: &CssPath) -> bool {
        use azul_css::css::CssPathSelector::*;

        use crate::style::{CssGroupIterator, CssGroupSplitReason};

        if self.path.is_empty() {
            return false;
        }
        if path.selectors.as_ref().is_empty() {
            return false;
        }

        // self_matcher is only ever going to contain "Children" selectors, never "DirectChildren"
        let mut path_groups = CssGroupIterator::new(path.selectors.as_ref()).collect::<Vec<_>>();
        path_groups.reverse();

        if path_groups.is_empty() {
            return false;
        }
        let mut self_groups = CssGroupIterator::new(self.path.as_ref()).collect::<Vec<_>>();
        self_groups.reverse();
        if self_groups.is_empty() {
            return false;
        }

        if self.indices_in_parent.len() != self_groups.len() {
            return false;
        }
        if self.children_length.len() != self_groups.len() {
            return false;
        }

        // self_groups = [ // HTML
        //     "body",
        //     "div.__azul_native-ribbon-container"
        //     "div.__azul_native-ribbon-tabs"
        //     "p.home"
        // ]
        //
        // path_groups = [ // CSS
        //     ".__azul_native-ribbon-tabs"
        //     "div.after-tabs"
        // ]

        // get the first path group and see if it matches anywhere in the self group
        let mut cur_selfgroup_scan = 0;
        let mut cur_pathgroup_scan = 0;
        let mut valid = false;
        let mut path_group = path_groups[cur_pathgroup_scan].clone();

        while cur_selfgroup_scan < self_groups.len() {
            let mut advance = None;

            // scan all remaining path groups
            for (id, cg) in self_groups[cur_selfgroup_scan..].iter().enumerate() {
                let gm = group_matches(
                    &path_group.0,
                    &self_groups[cur_selfgroup_scan + id].0,
                    self.indices_in_parent[cur_selfgroup_scan + id],
                    self.children_length[cur_selfgroup_scan + id],
                );

                if gm {
                    // ok: ".__azul_native-ribbon-tabs" was found within self_groups
                    // advance the self_groups by n
                    advance = Some(id);
                    break;
                }
            }

            match advance {
                Some(n) => {
                    // group was found in remaining items
                    // advance cur_pathgroup_scan by 1 and cur_selfgroup_scan by n
                    if cur_pathgroup_scan == path_groups.len() - 1 {
                        // last path group
                        return cur_selfgroup_scan + n == self_groups.len() - 1;
                    }
                    cur_pathgroup_scan += 1;
                    cur_selfgroup_scan += n;
                    path_group = path_groups[cur_pathgroup_scan].clone();
                }
                None => return false, // group was not found in remaining items
            }
        }

        // only return true if all path_groups matched
        cur_pathgroup_scan == path_groups.len() - 1
    }
}

// does p.home match div.after-tabs?
// a: div.after-tabs
fn group_matches(
    a: &[CssPathSelector],
    b: &[CssPathSelector],
    idx_in_parent: usize,
    parent_children: usize,
) -> bool {
    use azul_css::css::{
        CssNthChildSelector, CssPathPseudoSelector,
        CssPathSelector::{Class, Global, Id, PseudoSelector, Type},
    };

    for selector in a {
        match selector {
            // always matches
            Global
            | PseudoSelector(
                CssPathPseudoSelector::Hover
                | CssPathPseudoSelector::Active
                | CssPathPseudoSelector::Focus
                | CssPathPseudoSelector::SeatFocus,
            ) => {}

            Type(tag) => {
                if !b.iter().any(|t| *t == Type(*tag)) {
                    return false;
                }
            }
            Class(class) => {
                if !b.iter().any(|t| *t == Class(class.clone())) {
                    return false;
                }
            }
            Id(id) => {
                if !b.iter().any(|t| *t == Id(id.clone())) {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::First) => {
                if idx_in_parent != 0 {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::Last) => {
                if idx_in_parent != parent_children.saturating_sub(1) {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Number(i))) => {
                if idx_in_parent != *i as usize {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Even)) => {
                if !idx_in_parent.is_multiple_of(2) {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Odd)) => {
                if idx_in_parent.is_multiple_of(2) {
                    return false;
                }
            }
            PseudoSelector(CssPathPseudoSelector::NthChild(CssNthChildSelector::Pattern(p))) => {
                if !idx_in_parent
                    .saturating_sub(p.offset as usize)
                    .is_multiple_of(p.pattern_repeat as usize)
                {
                    return false;
                }
            }

            _ => return false, // can't happen
        }
    }

    true
}

struct CssBlock {
    ending: Option<CssPathPseudoSelector>,
    block: CssRuleBlock,
}

/// Serialize the CSS blocks matched for a node into one inline CSS string for
/// `Dom::with_css(...)`. `with_css` parses via `Css::parse_scoped`, which runs
/// the full selector+nesting machinery, so `:hover`/`:active`/`:focus` are
/// emitted as nested pseudo blocks and round-trip faithfully; plain rules are
/// emitted flat as `key: value;` (via `CssProperty::key()` / `value()`).
fn css_blocks_to_inline_string(blocks: &[CssBlock]) -> String {
    fn decls_of(block: &CssBlock) -> Vec<String> {
        block
            .block
            .declarations
            .as_ref()
            .iter()
            .map(|d| match d {
                CssDeclaration::Static(s) => format!("{}: {};", s.key(), s.value()),
                // A `var()` keeps its reference and a definition stays a
                // definition: the cascade resolves them where the exported
                // `with_css` string is loaded. An `env()` keeps its fallback,
                // as it always did.
                CssDeclaration::Dynamic(_) if d.var_reference().is_some() => d.format_css(),
                CssDeclaration::Dynamic(dy) => {
                    format!("{}: {};", dy.default_value.key(), dy.default_value.value())
                }
                CssDeclaration::CustomProperty(_) => d.format_css(),
            })
            .collect()
    }

    let mut normal: Vec<String> = Vec::new();
    let mut pseudo: Vec<String> = Vec::new();
    for block in blocks {
        let pseudo_sel = match block.ending {
            Some(CssPathPseudoSelector::Hover) => Some(":hover"),
            Some(CssPathPseudoSelector::Active) => Some(":active"),
            Some(CssPathPseudoSelector::Focus) => Some(":focus"),
            Some(CssPathPseudoSelector::SeatFocus) => Some(":seat-focus"),
            _ => None,
        };
        match pseudo_sel {
            None => normal.extend(decls_of(block)),
            Some(sel) => pseudo.push(format!("{} {{ {} }}", sel, decls_of(block).join(" "))),
        }
    }

    let mut parts = normal;
    parts.extend(pseudo);
    parts.join(" ")
}

/// The inline CSS of an exported node: the stylesheet rules that match it
/// (`css_blocks_to_inline_string`), its writing direction (`dir`), then its
/// own `style` attribute — last, so it wins like an inline style does (the
/// order the XML loaders apply them in, `xml::attributes::apply_settings`).
/// The attribute's whitespace is collapsed to single spaces: a newline inside
/// a C / C++ / Python string literal would not compile.
fn inline_css(
    blocks: &[CssBlock],
    direction: Option<azul_css::props::style::StyleDirection>,
    style: Option<&str>,
) -> String {
    let mut css = css_blocks_to_inline_string(blocks);
    let mut push = |part: &str| {
        if !part.is_empty() {
            if !css.is_empty() {
                css.push(' ');
            }
            css.push_str(part);
        }
    };
    if let Some(d) = direction {
        push(&attributes::direction_css(d));
    }
    if let Some(style) = style {
        push(&style.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    css
}

fn get_css_blocks(css: &Css, matcher: &CssMatcher) -> Vec<CssBlock> {
    let mut blocks = Vec::new();

    for css_block in css.rules.as_ref() {
        if matcher.matches(&css_block.path) {
            let ending = match css_block.path.selectors.as_ref().last() {
                Some(CssPathSelector::PseudoSelector(p)) => Some(p.clone()),
                _ => None,
            };

            blocks.push(CssBlock {
                ending,
                block: css_block.clone(),
            });
        }
    }

    blocks
}

/// Tags with a zero-arg per-tag creator (`create_<tag>()` / `AzDom_create<Tag>()`
/// / `create_node(NodeType::<Tag>)`). Interactive / data elements (Button, Input,
/// Img, Select, Textarea, Label, A, Table, …) take constructor arguments, so an
/// exported page maps them to a plain `div` container (structure preserved; the
/// user re-wires behavior). Keep these CamelCase to match `NodeTypeTag` debug names.
const SAFE_CONTAINER_TAGS: &[&str] = &[
    // These must match the real `NodeType` Debug names exactly (the lookup below is a
    // string compare against `{:?}`). Six used to be mis-cased — "Blockquote",
    // "Colgroup", "Figcaption", "Tbody", "Tfoot", "Thead" — so those tags silently
    // degraded to "Div".
    "Abbr",
    "Acronym",
    "Address",
    "Article",
    "Aside",
    "B",
    "Bdi",
    "Bdo",
    "Big",
    "BlockQuote",
    "Body",
    "Br",
    "Caption",
    "Cite",
    "Code",
    "ColGroup",
    "Dd",
    "Del",
    "Dfn",
    "Dir",
    "Div",
    "Dl",
    "Dt",
    "Em",
    "Embed",
    "FigCaption",
    "Figure",
    "Footer",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "Head",
    "Header",
    "Hr",
    "Html",
    "I",
    "Ins",
    "Kbd",
    "Li",
    "Link",
    "Main",
    "Map",
    "Mark",
    "Meta",
    "Nav",
    "Object",
    "Ol",
    "P",
    "Pre",
    "Q",
    "Rp",
    "Rt",
    "Rtc",
    "Ruby",
    "S",
    "Samp",
    "Script",
    "Section",
    "Small",
    "Span",
    "Strong",
    "Style",
    "Sub",
    "Sup",
    "Svg",
    "TBody",
    "Td",
    "TFoot",
    "Th",
    "THead",
    "Title",
    "Tr",
    "U",
    "Ul",
    "Var",
    "Wbr",
];

/// The CamelCase tag to actually emit a creator for: the tag itself if it has a
/// zero-arg creator, else `"Div"`.
fn safe_container_tag(tag_dbg: &str) -> &'static str {
    SAFE_CONTAINER_TAGS
        .iter()
        .copied()
        .find(|t| *t == tag_dbg)
        .unwrap_or("Div")
}

// ───────────────────────────────────────────────────────────────────────────
// Semantic / accessibility-aware constructor selection.
//
// Instead of mapping every element to a plain `div`, an exported live page
// picks the *most specific* Azul constructor so the generated app keeps the
// page's semantics + accessibility tree:
//
//   • Tier A  `create_<tag>_with_text(text)` — a tag with a single text child
//             and no element children (P, Span, H1-H6, Li, Td, Code, …).
//   • Tier B  aria-only / void widgets (Details, Summary, Form, Canvas, Area,
//             …) — `create_<tag>(SmallAriaInfo::label(..))` when `aria-label`
//             is present, else `create_<tag>_no_a11y()`.
//   • Tier C  multi-arg widgets (Button, A, Label, Input, Select, Option,
//             Optgroup, Textarea, Table) — args pulled from HTML attributes.
//   • Tier D  scalar-driven widgets (Progress, Meter, Dialog) — the `*_no_a11y`
//             form with extracted numeric args (the full aria structs are
//             complex; the NoA11y form is simplest + correct).
//
// Every constructor chosen here exists in api.json (`Dom::create_*`); anything
// else falls back to `safe_container_tag` (`div`). The lowering turns the
// choice into IR (`Lower::ctor`); the printers spell it per language.
// ───────────────────────────────────────────────────────────────────────────

/// A single positional argument of a semantic constructor. String payloads are
/// RAW — the printers escape them.
#[derive(Debug, Clone, PartialEq)]
enum CtorArg {
    /// Plain string literal (`AzString` / `String` / `"…"`).
    Str(String),
    /// `SmallAriaInfo` built from an accessible label.
    Aria(String),
    /// `f32` numeric literal.
    Float(f32),
    /// `OptionString::Some(text)`.
    OptSome(String),
    /// `OptionString::None`.
    OptNone,
}

/// The constructor chosen for an element node.
#[derive(Debug, Clone, PartialEq)]
enum NodeCtor {
    /// Plain container: `create_<tag>()` (or `create_div()`, see
    /// [`safe_container_tag`]).
    Plain,
    /// A specific semantic constructor.
    Semantic {
        /// Canonical CamelCase suffix after `create` / `AzDom_create`
        /// (e.g. `Button`, `ButtonNoA11y`, `PWithText`, `A`, `ANoA11y`).
        suffix: String,
        args: Vec<CtorArg>,
        /// The node's direct text is folded into the ctor — skip text children
        /// in the walk so it isn't emitted twice.
        consumes_text: bool,
        /// The table aria form injects its own `<caption>` child — drop the
        /// first literal `<caption>` element so it isn't duplicated.
        skip_caption: bool,
    },
}

/// Uppercase the first character (`button` → `Button`, `h1` → `H1`). HTML tags
/// are single lowercase tokens, so this yields the exact `AzDom_create<Suffix>`
/// spelling.
fn cap_first(tag: &str) -> String {
    let mut c = tag.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

/// CamelCase → `snake_case` for the C++/Python/Rust method names
/// (`ButtonNoA11y` → `button_no_a11y`, `PWithText` → `p_with_text`,
/// `ANoA11y` → `a_no_a11y`, `H1WithText` → `h1_with_text`).
fn camel_to_snake(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(char::is_ascii_lowercase);
            if prev.is_ascii_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_ascii_uppercase() && next_lower)
            {
                out.push('_');
            }
        }
        out.extend(ch.to_lowercase());
    }
    out
}

/// Joined, trimmed text of a node's *direct* text children (`"  Go  "` → `"Go"`).
fn node_direct_text(node: &XmlNode) -> String {
    node.children
        .as_ref()
        .iter()
        .filter_map(|c| match c {
            XmlNodeChild::Text(t) => {
                let t = t.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            }
            XmlNodeChild::Element(_) => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Non-empty `aria-label` attribute value, if present.
fn node_aria_label(node: &XmlNode) -> Option<String> {
    node.attributes.get_key("aria-label").and_then(|v| {
        let v = v.as_str().trim();
        if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        }
    })
}

/// Attribute value, or `default` when absent.
fn node_attr_or(node: &XmlNode, key: &str, default: &str) -> String {
    node.attributes
        .get_key(key)
        .map_or_else(|| default.to_string(), |v| v.as_str().to_string())
}

/// Attribute parsed as `f32`, or `default` when absent / unparsable.
fn node_attr_f32(node: &XmlNode, key: &str, default: f32) -> f32 {
    node.attributes
        .get_key(key)
        .and_then(|v| v.as_str().trim().parse::<f32>().ok())
        .unwrap_or(default)
}

/// Text of the node's first `<caption>` element child, if any (non-empty).
fn first_caption_text(node: &XmlNode) -> Option<String> {
    node.children.as_ref().iter().find_map(|c| match c {
        XmlNodeChild::Element(e) if e.node_type.as_str().eq_ignore_ascii_case("caption") => {
            let t = e.get_text_content();
            let t = t.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        _ => None,
    })
}

/// Tags with a single-arg `create_<tag>_with_text(text)` constructor (Tier A).
const WITH_TEXT_TAGS: &[&str] = &[
    "acronym",
    "b",
    "bdi",
    "bdo",
    "big",
    "blockquote",
    "cite",
    "code",
    "del",
    "dfn",
    "em",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "i",
    "ins",
    "kbd",
    "li",
    "mark",
    "p",
    "pre",
    "rp",
    "rt",
    "s",
    "samp",
    "small",
    "span",
    "strong",
    "style",
    "sub",
    "sup",
    "td",
    "th",
    "title",
    "u",
    "var",
];

/// Pick the semantic constructor for `tag` (lowercase HTML tag) + `node`.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one
                                 // branch per input variant)
fn analyze_node_ctor(tag: &str, node: &XmlNode) -> NodeCtor {
    // Helper for the common "no caption skip" case.
    fn sem(suffix: impl Into<String>, args: Vec<CtorArg>, consumes_text: bool) -> NodeCtor {
        NodeCtor::Semantic {
            suffix: suffix.into(),
            args,
            consumes_text,
            skip_caption: false,
        }
    }

    let aria = node_aria_label(node);
    let has_aria = aria.is_some();
    let label = aria.unwrap_or_default();
    // `has_only_text_children()` is also true for childless nodes; pair it with
    // `has_text` so empty elements stay plain containers.
    let pure_text = node.has_only_text_children();
    let text = node_direct_text(node);
    let has_text = !text.is_empty();
    let cap = cap_first(tag);

    // Tier A — *_with_text (single text child, no element children).
    if WITH_TEXT_TAGS.contains(&tag) {
        if pure_text && has_text {
            return sem(format!("{cap}WithText"), vec![CtorArg::Str(text)], true);
        }
        return NodeCtor::Plain;
    }

    match tag {
        // Tier B — aria-only / void widgets.
        "details" | "form" | "fieldset" | "legend" | "menu" | "output" | "datalist" | "canvas"
        | "audio" | "video" | "area" => {
            if has_aria {
                sem(cap, vec![CtorArg::Aria(label)], false)
            } else {
                sem(format!("{cap}NoA11y"), vec![], false)
            }
        }
        // Summary is Tier B but also has a WithText form for a single text child.
        "summary" => {
            if pure_text && has_text {
                if has_aria {
                    sem(
                        "SummaryWithText",
                        vec![CtorArg::Str(text), CtorArg::Aria(label)],
                        true,
                    )
                } else {
                    sem("SummaryWithTextNoA11y", vec![CtorArg::Str(text)], true)
                }
            } else if has_aria {
                sem("Summary", vec![CtorArg::Aria(label)], false)
            } else {
                sem("SummaryNoA11y", vec![], false)
            }
        }

        // Tier C — multi-arg widgets (args from HTML attributes).
        "button" => {
            if has_aria {
                sem(
                    "Button",
                    vec![CtorArg::Str(text), CtorArg::Aria(label)],
                    true,
                )
            } else {
                sem("ButtonNoA11y", vec![CtorArg::Str(text)], true)
            }
        }
        "a" => {
            let href = node_attr_or(node, "href", "");
            if has_aria {
                sem(
                    "A",
                    vec![CtorArg::Str(href), CtorArg::Str(text), CtorArg::Aria(label)],
                    true,
                )
            } else {
                let lbl = if has_text {
                    CtorArg::OptSome(text)
                } else {
                    CtorArg::OptNone
                };
                sem("ANoA11y", vec![CtorArg::Str(href), lbl], true)
            }
        }
        "label" => {
            let for_id = node_attr_or(node, "for", "");
            if has_aria {
                sem(
                    "Label",
                    vec![
                        CtorArg::Str(for_id),
                        CtorArg::Str(text),
                        CtorArg::Aria(label),
                    ],
                    true,
                )
            } else {
                sem(
                    "LabelNoA11y",
                    vec![CtorArg::Str(for_id), CtorArg::Str(text)],
                    true,
                )
            }
        }
        "input" => {
            let ty = node_attr_or(node, "type", "text");
            let name = node_attr_or(node, "name", "");
            if has_aria {
                sem(
                    "Input",
                    vec![
                        CtorArg::Str(ty),
                        CtorArg::Str(name),
                        CtorArg::Str(label.clone()),
                        CtorArg::Aria(label),
                    ],
                    false,
                )
            } else {
                sem(
                    "InputNoA11y",
                    vec![CtorArg::Str(ty), CtorArg::Str(name), CtorArg::Str(label)],
                    false,
                )
            }
        }
        "textarea" => {
            let name = node_attr_or(node, "name", "");
            if has_aria {
                sem(
                    "Textarea",
                    vec![
                        CtorArg::Str(name),
                        CtorArg::Str(label.clone()),
                        CtorArg::Aria(label),
                    ],
                    false,
                )
            } else {
                sem(
                    "TextareaNoA11y",
                    vec![CtorArg::Str(name), CtorArg::Str(label)],
                    false,
                )
            }
        }
        "select" => {
            let name = node_attr_or(node, "name", "");
            if has_aria {
                sem(
                    "Select",
                    vec![
                        CtorArg::Str(name),
                        CtorArg::Str(label.clone()),
                        CtorArg::Aria(label),
                    ],
                    false,
                )
            } else {
                sem(
                    "SelectNoA11y",
                    vec![CtorArg::Str(name), CtorArg::Str(label)],
                    false,
                )
            }
        }
        "option" => {
            let value = node_attr_or(node, "value", "");
            if has_aria {
                sem(
                    "Option",
                    vec![
                        CtorArg::Str(value),
                        CtorArg::Str(text),
                        CtorArg::Aria(label),
                    ],
                    true,
                )
            } else {
                sem(
                    "OptionNoA11y",
                    vec![CtorArg::Str(value), CtorArg::Str(text)],
                    true,
                )
            }
        }
        "optgroup" => {
            let lbl = node_attr_or(node, "label", "");
            if has_aria {
                sem(
                    "Optgroup",
                    vec![CtorArg::Str(lbl), CtorArg::Aria(label)],
                    false,
                )
            } else {
                sem("OptgroupNoA11y", vec![CtorArg::Str(lbl)], false)
            }
        }
        "table" => {
            if has_aria {
                // The aria form injects a caption child, so take the caption from
                // the literal <caption> (or the aria label) and drop the literal.
                let caption = first_caption_text(node).unwrap_or_else(|| label.clone());
                NodeCtor::Semantic {
                    suffix: "Table".to_string(),
                    args: vec![CtorArg::Str(caption), CtorArg::Aria(label)],
                    consumes_text: false,
                    skip_caption: true,
                }
            } else {
                sem("TableNoA11y", vec![], false)
            }
        }

        // Tier D — scalar-driven widgets (NoA11y form with extracted numbers).
        "progress" => sem(
            "ProgressNoA11y",
            vec![
                CtorArg::Float(node_attr_f32(node, "value", 0.0)),
                CtorArg::Float(node_attr_f32(node, "max", 1.0)),
            ],
            false,
        ),
        "meter" => sem(
            "MeterNoA11y",
            vec![
                CtorArg::Float(node_attr_f32(node, "value", 0.0)),
                CtorArg::Float(node_attr_f32(node, "min", 0.0)),
                CtorArg::Float(node_attr_f32(node, "max", 1.0)),
            ],
            false,
        ),
        "dialog" => sem("DialogNoA11y", vec![], false),

        _ => NodeCtor::Plain,
    }
}

// The lowering reads these flags by matching; the unit tests ask.
#[cfg(test)]
impl NodeCtor {
    const fn consumes_text(&self) -> bool {
        matches!(
            self,
            Self::Semantic {
                consumes_text: true,
                ..
            }
        )
    }
    const fn skip_caption(&self) -> bool {
        matches!(
            self,
            Self::Semantic {
                skip_caption: true,
                ..
            }
        )
    }
}

// ===========================================================================
// Attributes as builder calls
// ===========================================================================

/// The attributes an element's constructor takes as arguments
/// (`analyze_node_ctor`): the table's settings of them are not written
/// again as builder calls.
fn ctor_attributes(tag: &str, ctor: &NodeCtor) -> &'static [&'static str] {
    if matches!(ctor, NodeCtor::Plain) {
        return &[];
    }
    match tag {
        "a" => &["href", "aria-label"],
        "label" => &["for", "aria-label"],
        "input" => &["type", "name", "aria-label"],
        "textarea" | "select" => &["name", "aria-label"],
        "option" => &["value", "aria-label"],
        "optgroup" => &["label", "aria-label"],
        "progress" => &["value", "max"],
        "meter" => &["value", "min", "max"],
        _ => &["aria-label"],
    }
}

/// A `TabIndex` value in api.json vocabulary.
fn tab_index_expr(t: TabIndex) -> Expr {
    match t {
        TabIndex::Auto => Expr::unit("TabIndex", EnumShape::Tagged, "Auto"),
        TabIndex::OverrideInParent(n) => Expr::variant(
            "TabIndex",
            EnumShape::Tagged,
            "OverrideInParent",
            vec![Expr::int(i128::from(n), Prim::U32)],
        ),
        TabIndex::NoKeyboardFocus => Expr::unit("TabIndex", EnumShape::Tagged, "NoKeyboardFocus"),
    }
}

/// An `AttributeType` value in api.json vocabulary.
fn attribute_expr(a: &AttributeType) -> Expr {
    fn v(variant: &str, args: Vec<Expr>) -> Expr {
        Expr::variant("AttributeType", EnumShape::Tagged, variant, args)
    }
    fn unit(variant: &str) -> Expr {
        Expr::unit("AttributeType", EnumShape::Tagged, variant)
    }
    fn text(variant: &str, s: &AzString) -> Expr {
        v(variant, vec![Expr::str(s.as_str())])
    }
    fn pair(variant: &str, nv: &AttributeNameValue) -> Expr {
        v(
            variant,
            vec![Expr::strukt(
                "AttributeNameValue",
                vec![
                    ("attr_name", Expr::str(nv.attr_name.as_str())),
                    ("value", Expr::str(nv.value.as_str())),
                ],
            )],
        )
    }
    fn int(variant: &str, n: i32) -> Expr {
        v(variant, vec![Expr::int(i128::from(n), Prim::I32)])
    }
    match a {
        AttributeType::Id(s) => text("Id", s),
        AttributeType::Class(s) => text("Class", s),
        AttributeType::AriaLabel(s) => text("AriaLabel", s),
        AttributeType::AriaLabelledBy(s) => text("AriaLabelledBy", s),
        AttributeType::AriaDescribedBy(s) => text("AriaDescribedBy", s),
        AttributeType::AriaRole(s) => text("AriaRole", s),
        AttributeType::AriaState(nv) => pair("AriaState", nv),
        AttributeType::AriaProperty(nv) => pair("AriaProperty", nv),
        AttributeType::Href(s) => text("Href", s),
        AttributeType::Rel(s) => text("Rel", s),
        AttributeType::Target(s) => text("Target", s),
        AttributeType::Src(s) => text("Src", s),
        AttributeType::Alt(s) => text("Alt", s),
        AttributeType::Title(s) => text("Title", s),
        AttributeType::Name(s) => text("Name", s),
        AttributeType::Value(s) => text("Value", s),
        AttributeType::InputType(s) => text("InputType", s),
        AttributeType::Placeholder(s) => text("Placeholder", s),
        AttributeType::Required => unit("Required"),
        AttributeType::Disabled => unit("Disabled"),
        AttributeType::Readonly => unit("Readonly"),
        AttributeType::CheckedTrue => unit("CheckedTrue"),
        AttributeType::CheckedFalse => unit("CheckedFalse"),
        AttributeType::Selected => unit("Selected"),
        AttributeType::Max(s) => text("Max", s),
        AttributeType::Min(s) => text("Min", s),
        AttributeType::Step(s) => text("Step", s),
        AttributeType::Pattern(s) => text("Pattern", s),
        AttributeType::MinLength(n) => int("MinLength", *n),
        AttributeType::MaxLength(n) => int("MaxLength", *n),
        AttributeType::Autocomplete(s) => text("Autocomplete", s),
        AttributeType::Scope(s) => text("Scope", s),
        AttributeType::ColSpan(n) => int("ColSpan", *n),
        AttributeType::RowSpan(n) => int("RowSpan", *n),
        AttributeType::TabIndex(n) => int("TabIndex", *n),
        AttributeType::Focusable => unit("Focusable"),
        AttributeType::Autofocus => unit("Autofocus"),
        AttributeType::Lang(s) => text("Lang", s),
        AttributeType::Dir(s) => text("Dir", s),
        AttributeType::ContentEditable(b) => v("ContentEditable", vec![Expr::Bool(*b)]),
        AttributeType::Draggable(b) => v("Draggable", vec![Expr::Bool(*b)]),
        AttributeType::Hidden => unit("Hidden"),
        AttributeType::Data(nv) => pair("Data", nv),
        AttributeType::Custom(nv) => pair("Custom", nv),
    }
}

#[cfg(test)]
#[path = "dom_test.rs"]
mod dom_test;
