//! AzBuilder's quick exports — text in, text out, no zip:
//!
//! * **Compile CSS to…** — list the rules of a stylesheet (pasted text, the
//!   document's stylesheet, the selected node's style, a component's CSS) and
//!   compile all or some of them with a CSS code generator
//!   (`azul_css::codegen::backend_for`).
//! * **Subtree → code** — a subtree of the builder document as a render
//!   function (or a runnable app).
//! * **Component → code** — a component as code: its render function (a
//!   converted component's `{placeholders}` become parameters) and, where
//!   the language's printer spells it, its registration
//!   (`register_<library>_library`, a `ComponentDef` per component).
//! * **HTML → DOM (code)** — pasted HTML / XHTML (a fragment or a document,
//!   its `<style>` blocks and `style` attributes) as a render function or an
//!   app, optionally with its CSS as named styles; a parse error comes back
//!   with its line and column ([`html_to_code`]).
//!
//! Plus what Export > Code downloads ([`project`]): the app, its build file,
//! the exportable component libraries, the app's stylesheet as named styles
//! and a README.
//!
//! **No code generator and no assembly live here.** Everything is
//! `azul_core::codegen::project` (the DOM and the CSS generators together,
//! on top of the component API), printed by `azul_css::codegen`. This module
//! only picks the builder's markup (a document subtree, the document, the
//! live page; instances stay `<library:name ..>` tags,
//! [`builder::export_node_markup`]), its stylesheet, the components'
//! templates ([`template_markup`]) and the libraries, and answers JSON.
//!
//! The ops that call this live in `full.rs` (`get_codegen_languages`,
//! `get_css_rules`, `compile_css`, `html_to_code`, `export_subtree_code`,
//! `export_component_code`, `export_code`, `export_code_zip`).

use std::fmt::Write as _;

pub use azul_core::codegen::project::{AppMarkup, CodeExport, CodeMode};
use azul_core::{
    codegen::{
        backend,
        dom::{ComponentMarkup, Components},
        dom_warning,
        project::{fragment_code, html_code, library_code, project_files, ProjectSpec},
        render_fn_name,
    },
    xml::{ComponentDef, ComponentMap, XmlNodeChild},
};
use azul_css::{
    codegen::{all_backends, ir::Ident, GeneratedFile},
    css::{Css, CssDeclaration, CssPath, CssPathSelector, CssRuleBlock},
};

use super::builder::{self, BuilderDocument, BuilderNode, BuilderNodeKind, ROOT_UID};

// ===========================================================================
// Languages: ONE list, the code generators azul_css has
// ===========================================================================

/// `get_codegen_languages`: every code generator, in documentation order:
/// `{languages: [{id, label, ext, dom, no_dom_reason}]}` (`dom`: its printer
/// does DOM export; the CSS dialog offers them all; the DOM exports and
/// Export > Code (ZIP) list the others disabled with `no_dom_reason`, the
/// warning such an export carries - `azul_core::codegen::dom_warning`).
#[must_use]
pub fn languages_json() -> serde_json::Value {
    serde_json::json!({
        "languages": all_backends()
            .iter()
            .map(|b| serde_json::json!({
                "id": b.lang(),
                "label": b.display_name(),
                "ext": b.extension(),
                "dom": b.exports_dom(),
                "no_dom_reason": dom_warning(&**b),
            }))
            .collect::<Vec<_>>(),
    })
}


// ===========================================================================
// CSS: sources, rules, compile
// ===========================================================================

/// Where "Compile CSS to…" takes its stylesheet from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssSource {
    /// CSS text the user typed or pasted.
    Text(String),
    /// The builder document's stylesheet: every component stylesheet it
    /// uses, then its own (`builder_set_stylesheet`).
    Document,
    /// One document node's style: the document rules that apply to it, and
    /// its own `style` attribute as a rule of its own.
    Node(u64),
    /// A component's CSS.
    Component { library: String, name: String },
}

impl CssSource {
    /// From the op's fields: `source` is `text` (default), `document`, `node`
    /// or `component`.
    ///
    /// # Errors
    /// An unknown source, or one missing the field it needs.
    pub fn from_op(
        source: Option<&str>,
        css: Option<&str>,
        node: Option<u64>,
        library: Option<&str>,
        name: Option<&str>,
    ) -> Result<Self, String> {
        match source.unwrap_or("text") {
            "text" => Ok(Self::Text(css.unwrap_or_default().to_string())),
            "document" => Ok(Self::Document),
            "node" => node
                .map(Self::Node)
                .ok_or_else(|| "source \"node\" needs \"node\" (a document uid)".to_string()),
            "component" => match (library, name) {
                (Some(l), Some(n)) => Ok(Self::Component {
                    library: l.to_string(),
                    name: n.to_string(),
                }),
                _ => Err("source \"component\" needs \"library\" and \"name\"".to_string()),
            },
            other => Err(format!(
                "unknown CSS source {other:?}; use text, document, node or component"
            )),
        }
    }
}

/// The CSS text of a source.
///
/// # Errors
/// An unknown node / component, a text node.
pub fn resolve_css(
    source: &CssSource,
    doc: &BuilderDocument,
    map: &ComponentMap,
) -> Result<String, String> {
    match source {
        CssSource::Text(t) => Ok(t.clone()),
        CssSource::Document => Ok(document_css(doc, map)),
        CssSource::Component { library, name } => map
            .get(library, name)
            .map(|d| d.css.as_str().to_string())
            .ok_or_else(|| format!("Component '{name}' not found in library '{library}'")),
        CssSource::Node(uid) => node_css(doc, map, *uid),
    }
}

/// "The selected node's style": the rules of the document's stylesheet whose
/// last compound selector the node satisfies (its type, classes and id;
/// ancestors are not checked), then its `style` attribute as a rule for its
/// own selector. A component instance's style is its component's CSS.
fn node_css(doc: &BuilderDocument, map: &ComponentMap, uid: u64) -> Result<String, String> {
    let node = doc.node(uid)?;
    let classes: Vec<&str> = node
        .attrs
        .get("class")
        .map(|c| c.split_whitespace().collect())
        .unwrap_or_default();
    let ids: Vec<&str> = node
        .attrs
        .get("id")
        .map(|c| c.split_whitespace().collect())
        .unwrap_or_default();
    let mut out = String::new();
    let own_selector = match &node.kind {
        BuilderNodeKind::Text { .. } => {
            return Err(format!(
                "node {uid} is a text node; its style is its element's (select the element)"
            ))
        }
        BuilderNodeKind::Element { tag } => {
            let sheet = document_css(doc, map);
            let (css, _) = azul_css::parser2::new_from_str(&sheet);
            for rule in css.rules.as_ref() {
                if rule_matches(&rule.path, tag, &classes, &ids) {
                    out.push_str(&rule_text(rule));
                }
            }
            element_selector(tag, &classes, &ids)
        }
        BuilderNodeKind::Component { library, name } => {
            if let Some(def) = map.get(library, name) {
                let css = def.css.as_str().trim();
                if !css.is_empty() {
                    let _ = writeln!(out, "/* {library}:{name} */\n{css}");
                }
            }
            if classes.is_empty() && ids.is_empty() {
                String::new()
            } else {
                element_selector("", &classes, &ids)
            }
        }
    };
    if let Some(style) = node.attrs.get("style") {
        let style = style.trim();
        if !style.is_empty() && !own_selector.is_empty() {
            let _ = writeln!(out, "{own_selector} {{ {style} }}");
        }
    }
    Ok(out)
}

/// The document's stylesheet as the window applies it: the CSS of every
/// component it uses, then the document's own (B5), which comes last.
fn document_css(doc: &BuilderDocument, map: &ComponentMap) -> String {
    with_own_stylesheet(builder::export_node_xml(&doc.root, map).1, doc)
}

/// `css` followed by the document's own stylesheet.
fn with_own_stylesheet(mut css: String, doc: &BuilderDocument) -> String {
    if !doc.stylesheet.trim().is_empty() {
        if !css.is_empty() && !css.ends_with('\n') {
            css.push('\n');
        }
        css.push_str(&doc.stylesheet);
        css.push('\n');
    }
    css
}

/// `.a.b` if the node has classes, else `#id`, else its tag.
fn element_selector(tag: &str, classes: &[&str], ids: &[&str]) -> String {
    if !classes.is_empty() {
        classes.iter().map(|c| format!(".{c}")).collect()
    } else if let Some(id) = ids.first() {
        format!("#{id}")
    } else {
        tag.to_string()
    }
}

/// Whether a node with this tag / classes / ids satisfies the last compound
/// selector of `path` (the part after the last combinator).
fn rule_matches(path: &CssPath, tag: &str, classes: &[&str], ids: &[&str]) -> bool {
    let sels = path.selectors.as_ref();
    let start = sels
        .iter()
        .rposition(|s| {
            matches!(
                s,
                CssPathSelector::Children
                    | CssPathSelector::DirectChildren
                    | CssPathSelector::AdjacentSibling
                    | CssPathSelector::GeneralSibling
            )
        })
        .map_or(0, |i| i + 1);
    let compound = &sels[start..];
    !compound.is_empty()
        && compound.iter().all(|s| match s {
            CssPathSelector::Global | CssPathSelector::PseudoSelector(_) => true,
            CssPathSelector::Type(t) => t.to_string().eq_ignore_ascii_case(tag),
            CssPathSelector::Class(c) => classes.contains(&c.as_str()),
            CssPathSelector::Id(i) => ids.contains(&i.as_str()),
            _ => false,
        })
}

fn declaration_text(d: &CssDeclaration) -> String {
    // `key: value;`, a `var()` / `env()` with its fallback, a custom-property
    // definition as `--name: value;` - the one formatter the css crate has.
    d.format_css()
}

fn rule_text(rule: &CssRuleBlock) -> String {
    let decls: Vec<String> = rule
        .declarations
        .as_ref()
        .iter()
        .map(declaration_text)
        .collect();
    format!("{} {{ {} }}\n", rule.path, decls.join(" "))
}

/// `get_css_rules`: the source's CSS and its rules, in source order:
/// `{css, rules: [{index, selector, declarations, classes}], warnings}`.
#[must_use]
pub fn css_rules_json(css_text: &str) -> serde_json::Value {
    let (css, warnings) = azul_css::parser2::new_from_str(css_text);
    let rules: Vec<serde_json::Value> = css
        .rules
        .as_ref()
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            let classes: Vec<String> = rule
                .path
                .selectors
                .as_ref()
                .iter()
                .filter_map(|s| match s {
                    CssPathSelector::Class(c) => Some(c.as_str().to_string()),
                    _ => None,
                })
                .collect();
            serde_json::json!({
                "index": index,
                "selector": rule.path.to_string(),
                "declarations": rule
                    .declarations
                    .as_ref()
                    .iter()
                    .map(declaration_text)
                    .collect::<Vec<_>>()
                    .join(" "),
                "classes": classes,
                "conditional": !rule.conditions.as_ref().is_empty(),
            })
        })
        .collect();
    serde_json::json!({
        "css": css_text,
        "rules": rules,
        "warnings": warnings
            .iter()
            .map(|w| format!("{:?}", w.warning))
            .collect::<Vec<_>>(),
    })
}


/// `compile_css`: the CSS (all its rules, or the ones at `rules`) through
/// the code generator for `language` (named styles, `emit_css`), and how
/// many rules went in.
///
/// # Errors
/// An unknown language; a rule index past the end.
pub fn compile_css(
    css_text: &str,
    language: &str,
    rules: Option<&[usize]>,
) -> Result<(CodeExport, usize), String> {
    let b = backend(language)?;
    let (css, parse_warnings) = azul_css::parser2::new_from_str(css_text);
    let mut warnings: Vec<String> = parse_warnings
        .iter()
        .map(|w| format!("{:?}", w.warning))
        .collect();
    let all: &[CssRuleBlock] = css.rules.as_ref();
    let chosen = match rules {
        None => css.clone(),
        Some(idx) => {
            if let Some(bad) = idx.iter().find(|i| **i >= all.len()) {
                return Err(format!(
                    "rule {bad} does not exist (the stylesheet has {} rules)",
                    all.len()
                ));
            }
            let mut sorted: Vec<usize> = idx.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            if !css.keyframes.as_ref().is_empty() {
                warnings.push(
                    "@keyframes are left out when only some rules are compiled".to_string(),
                );
            }
            Css::new(sorted.iter().map(|i| all[*i].clone()).collect())
        }
    };
    let rule_count = chosen.rules.as_ref().len();
    Ok((
        CodeExport {
            language: b.lang().to_string(),
            file_name: format!("styles.{}", b.extension()),
            code: b.emit_css(&chosen),
            files: Vec::new(),
            warnings,
        },
        rule_count,
    ))
}

// ===========================================================================
// DOM: the builder's markup through `azul_core::codegen::project`
// ===========================================================================

fn parse_xml(xml: &str) -> Result<Vec<XmlNodeChild>, String> {
    crate::xml::parse_xml_string(xml)
        .map_err(|e| format!("the exported markup does not parse: {e:?}"))
}

/// A component's TEMPLATE as markup for the component-aware lowering
/// (AzBuilder's templates: `{placeholders}`, nested instances as
/// `<library:name ..>` tags), with the component's CSS; `None` for a
/// component without one (the lowering then exports what it renders).
#[must_use]
pub fn template_markup(def: &ComponentDef) -> Option<Result<ComponentMarkup, String>> {
    let template = builder::template_of(def)?;
    Some(parse_xml(template).map(|nodes| ComponentMarkup {
        nodes,
        css: def.css.as_str().to_string(),
        is_template: true,
    }))
}

/// The components of `map`, with the builder's templates.
fn components(map: &ComponentMap) -> Components<'_> {
    Components {
        map,
        template: &template_markup,
    }
}

/// An export answer as the dialogs read it:
/// `{language, file_name, code, files: [{path, contents}], warnings}`.
#[must_use]
pub fn code_json(c: &CodeExport) -> serde_json::Value {
    serde_json::json!({
        "language": c.language,
        "file_name": c.file_name,
        "code": c.code,
        "files": c
            .files
            .iter()
            .map(|f| serde_json::json!({ "path": f.path, "contents": f.contents }))
            .collect::<Vec<_>>(),
        "warnings": c.warnings,
    })
}

/// `render_<id | first class | tag>`, `render_document` for the root, a
/// component instance's `render_<name>`.
fn default_fn_name(node: &BuilderNode) -> String {
    if node.uid == ROOT_UID {
        return "render_document".to_string();
    }
    let base = match &node.kind {
        BuilderNodeKind::Component { name, .. } => name.clone(),
        BuilderNodeKind::Text { .. } => "text".to_string(),
        BuilderNodeKind::Element { tag } => node
            .attrs
            .get("id")
            .and_then(|i| i.split_whitespace().next())
            .or_else(|| {
                node.attrs
                    .get("class")
                    .and_then(|c| c.split_whitespace().next())
            })
            .unwrap_or(tag.as_str())
            .to_string(),
    };
    render_fn_name(&base)
}

/// The builder's markup of `node` for the code export (instances as tags)
/// and the CSS of every component it uses plus the document's own
/// stylesheet (global on the page, as when it is mounted).
fn builder_markup(
    doc: &BuilderDocument,
    node: &BuilderNode,
    map: &ComponentMap,
) -> Result<(Vec<XmlNodeChild>, String), String> {
    let (_, css) = builder::export_node_xml(node, map);
    Ok((
        parse_xml(&builder::export_node_markup(node))?,
        with_own_stylesheet(css, doc),
    ))
}

/// `export_subtree_code`: the document subtree at `uid` as code.
///
/// # Errors
/// An unknown uid or language.
pub fn subtree_code(
    doc: &BuilderDocument,
    map: &ComponentMap,
    uid: u64,
    language: &str,
    mode: CodeMode,
    fn_name: Option<&str>,
) -> Result<CodeExport, String> {
    let node = doc.node(uid)?;
    let (nodes, css) = builder_markup(doc, node, map)?;
    let name = fn_name
        .filter(|n| !n.trim().is_empty())
        .map_or_else(|| default_fn_name(node), str::to_string);
    fragment_code(
        &nodes,
        &css,
        language,
        mode,
        &name,
        "AzBuilder app",
        &components(map),
    )
}

/// `export_component_code`: one component as code — its render function
/// (and the components it uses) and, where the language's printer spells
/// it, its library's registration (holding just this component).
///
/// # Errors
/// Unknown component or language.
pub fn component_code(
    map: &ComponentMap,
    library: &str,
    name: &str,
    language: &str,
) -> Result<CodeExport, String> {
    let def = map
        .get(library, name)
        .ok_or_else(|| format!("Component '{name}' not found in library '{library}'"))?;
    let mut out = library_code(library, &[def], language, &components(map))?;
    out.file_name = format!(
        "{}_{}.{}",
        Ident::from_text(library).snake(),
        Ident::from_text(name).snake(),
        backend(language)?.extension()
    );
    Ok(out)
}

/// `html_to_code`: pasted HTML / XHTML as code ("HTML → DOM (code)",
/// `azul_core::codegen::project::html_code`), component instances
/// (`<library:name ..>`) as calls of the app's components. The answer is
/// [`code_json`] plus `errors`: markup that does not parse answers no code and
/// its error with line and column ([`parse_error_json`]); that is an answer,
/// not a refusal.
///
/// # Errors
/// An unknown language; a document without a body.
pub fn html_to_code(
    html: &str,
    map: &ComponentMap,
    language: &str,
    mode: CodeMode,
    fn_name: Option<&str>,
    with_styles: bool,
) -> Result<serde_json::Value, String> {
    let b = backend(language)?;
    let nodes = match crate::xml::parse_xml_string(html) {
        Ok(nodes) => nodes,
        Err(e) => {
            return Ok(serde_json::json!({
                "language": b.lang(),
                "file_name": "",
                "code": "",
                "files": [],
                "warnings": [],
                "errors": [parse_error_json(&e)],
            }))
        }
    };
    let out = html_code(&nodes, b.lang(), mode, fn_name, with_styles, &components(map))?;
    let mut json = code_json(&out);
    if let Some(obj) = json.as_object_mut() {
        obj.insert("errors".into(), serde_json::json!([]));
    }
    Ok(json)
}

/// A markup parse error as the dialogs read it: `{message, line, column}`
/// (1-based; `line` / `column` are `null` when the parser gave no position).
#[must_use]
pub fn parse_error_json(e: &azul_core::xml::XmlError) -> serde_json::Value {
    let (line, column) = xml_error_position(e).map_or((None, None), |p| (Some(p.row), Some(p.col)));
    serde_json::json!({
        "message": e.to_string(),
        "line": line,
        "column": column,
    })
}

/// Where the parser stopped, if it said.
fn xml_error_position(e: &azul_core::xml::XmlError) -> Option<azul_core::xml::XmlTextPos> {
    use azul_core::xml::{XmlError as E, XmlParseError as P, XmlStreamError as S};
    let stream_pos = |s: &S| match s {
        S::NonXmlChar(x) => Some(x.pos),
        S::InvalidChar(x) => Some(x.pos),
        S::InvalidCharMultiple(x) => Some(x.pos),
        S::InvalidQuote(x) => Some(x.pos),
        S::InvalidSpace(x) => Some(x.pos),
        S::InvalidString(x) => Some(x.pos),
        _ => None,
    };
    match e {
        E::ParserError(p) => match p {
            P::InvalidDeclaration(t)
            | P::InvalidComment(t)
            | P::InvalidPI(t)
            | P::InvalidDoctype(t)
            | P::InvalidEntity(t)
            | P::InvalidElement(t)
            | P::InvalidAttribute(t)
            | P::InvalidCdata(t)
            | P::InvalidCharData(t) => stream_pos(&t.stream_error).or(Some(t.pos)),
            P::UnknownToken(pos) => Some(*pos),
        },
        E::InvalidXmlPrefixUri(p)
        | E::UnexpectedXmlUri(p)
        | E::UnexpectedXmlnsUri(p)
        | E::InvalidElementNamePrefix(p)
        | E::UnexpectedEntityCloseTag(p)
        | E::MalformedEntityReference(p)
        | E::EntityReferenceLoop(p)
        | E::InvalidAttributeValue(p)
        | E::UnexpectedDeclaration(p)
        | E::InvalidName(p)
        | E::NonXmlChar(p)
        | E::InvalidChar(p)
        | E::InvalidChar2(p)
        | E::InvalidString(p)
        | E::InvalidExternalID(p)
        | E::InvalidComment(p)
        | E::InvalidCharacterData(p)
        | E::UnknownToken(p) => Some(*p),
        E::DuplicatedNamespace(x) => Some(x.pos),
        E::UnknownNamespace(x) => Some(x.pos),
        E::UnexpectedCloseTag(x) => Some(x.pos),
        E::UnknownEntityReference(x) => Some(x.pos),
        E::DuplicatedAttribute(x) => Some(x.pos),
        _ => None,
    }
}

// ===========================================================================
// Export > Code: the project
// ===========================================================================

/// The builder document (`<body>`, instances as tags, the component CSS it
/// uses and its own stylesheet - the app's stylesheet, which
/// `project_files` writes as named styles) as the app of a project.
///
/// # Errors
/// The document's markup does not parse.
pub fn document_app(doc: &BuilderDocument, map: &ComponentMap) -> Result<AppMarkup, String> {
    let (nodes, css) = builder_markup(doc, &doc.root, map)?;
    Ok(AppMarkup::Fragment { nodes, css })
}

/// The live page (`StyledDom::get_html_string`: a `<head><style>` and a
/// body whose nodes carry their computed style) as the app of a project.
///
/// # Errors
/// The HTML does not parse.
pub fn live_page_app(html: &str) -> Result<AppMarkup, String> {
    Ok(AppMarkup::Page(parse_xml(html)?))
}

/// Everything Export > Code writes for `language`
/// (`azul_core::codegen::project::project_files`): the app, every
/// exportable component library (or only `library_filter`) as a file of its
/// own, the app's stylesheet as named styles, a README.
///
/// # Errors
/// Unknown language; a page without a body.
pub fn project(
    language: &str,
    app: AppMarkup,
    title: &str,
    map: &ComponentMap,
    library_filter: Option<&str>,
) -> Result<(Vec<GeneratedFile>, Vec<String>), String> {
    let components = components(map);
    let spec = ProjectSpec {
        title,
        app,
        components: &components,
        libraries: map
            .get_exportable_libraries()
            .into_iter()
            .map(|l| l.name.as_str().to_string())
            .filter(|n| library_filter.map_or(true, |f| f == n.as_str()))
            .collect(),
    };
    project_files(language, &spec)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use azul_core::xml::{
        ComponentDataField, ComponentDefaultValue, ComponentFieldType, ComponentMap,
        OptionComponentDefaultValue,
    };
    use azul_css::AzString;

    use super::*;

    fn string_field(name: &str, default: &str) -> ComponentDataField {
        ComponentDataField {
            name: AzString::from(name),
            field_type: ComponentFieldType::String,
            default_value: OptionComponentDefaultValue::Some(ComponentDefaultValue::String(
                AzString::from(default),
            )),
            required: false,
            description: AzString::from(""),
        }
    }

    /// The builtins plus `user:badge`, a template component with CSS.
    fn badge_map() -> ComponentMap {
        let mut map = ComponentMap::with_builtin();
        let def = builder::template_component_def(
            "user",
            "badge",
            "Badge",
            "",
            ".badge { margin-top: 2px; }",
            "<span class=\"badge\">{text}</span>",
            vec![string_field("text", "New")],
        );
        builder::add_component(&mut map, "user", def).expect("added");
        map
    }

    #[test]
    fn the_dialogs_get_one_language_list_the_code_generators_and_whether_they_do_dom() {
        let v = languages_json();
        let langs = v["languages"].as_array().expect("one list");
        assert_eq!(langs.len(), all_backends().len());
        let dom: Vec<&str> = langs
            .iter()
            .filter(|l| l["dom"] == true)
            .filter_map(|l| l["id"].as_str())
            .collect();
        let printers: Vec<&str> = all_backends()
            .iter()
            .filter(|b| b.exports_dom())
            .map(|b| b.lang())
            .collect();
        assert_eq!(dom, printers, "`dom` is each printer's exports_dom()");
        for lang in ["rust", "c", "cpp", "python", "java", "go", "swift"] {
            assert!(dom.contains(&lang), "{lang} exports a DOM: {dom:?}");
        }
    }

    #[test]
    fn a_template_components_css_lands_on_its_nodes_and_its_text_is_a_parameter() {
        let map = badge_map();
        let rust = component_code(&map, "user", "badge", "rust").expect("rust").code;
        assert!(rust.contains("pub fn render_badge(text: &str) -> Dom {"), "{rust}");
        assert!(
            rust.contains("Dom::create_span_with_text(azul::str::String::from(text))"),
            "{rust}"
        );
        assert!(
            rust.contains(".with_css(azul::str::String::from(\"margin-top: 2px;\"))"),
            "{rust}"
        );
        assert!(rust.contains("pub extern \"C\" fn register_user_library()"), "{rust}");
        let py = component_code(&map, "user", "badge", "python")
            .expect("python")
            .code;
        assert!(py.contains("def render_badge(text=\"New\"):"), "{py}");
    }

    #[test]
    fn a_component_without_a_template_exports_its_default_rendering_and_says_so() {
        let map = ComponentMap::with_builtin();
        let out = component_code(&map, "builtin", "button", "rust").expect("rust");
        assert!(out.code.contains("Dom::create_button_no_a11y("), "{}", out.code);
        assert!(out.warnings.iter().any(|w| w.contains("no template")));
    }

    #[test]
    fn a_language_without_dom_export_says_why_instead_of_printing_a_ui() {
        // Any printer that does not export a DOM (none left: nothing to say).
        let Some(lang) = all_backends()
            .iter()
            .find(|b| !b.exports_dom())
            .map(|b| b.lang())
        else {
            return;
        };
        let map = badge_map();
        let out = component_code(&map, "user", "badge", lang).expect(lang);
        assert!(
            out.warnings.iter().any(|w| w.contains("does not print DOM")),
            "{lang}: {:?}",
            out.warnings
        );
    }

    #[test]
    fn a_wrapper_layer_language_exports_the_component_and_says_why_it_cannot_register_it() {
        let map = badge_map();
        let out = component_code(&map, "user", "badge", "java").expect("java");
        assert!(out.warnings.iter().all(|w| !w.contains("does not print DOM")));
        assert!(out.code.contains("public static Dom renderBadge(String text) {"), "{}", out.code);
        assert!(out.code.contains("is not registered here"), "{}", out.code);
    }

    #[test]
    fn a_template_component_is_code_through_its_render_function_in_any_language() {
        let map = badge_map();
        let def = map.get("user", "badge").expect("badge");
        assert_eq!(
            def.codegen,
            azul_core::xml::ComponentCodegen::RenderFunction
        );
        let c = component_code(&map, "user", "badge", "c").expect("c").code;
        assert!(c.contains("static AzDom render_badge(const char* text) {"), "{c}");
        assert!(template_markup(def).is_some_and(|m| m.is_ok()));
    }

    #[test]
    fn a_nodes_style_is_the_rules_its_classes_match_and_its_style_attribute() {
        let map = badge_map();
        let mut doc = BuilderDocument::new();
        // An instance puts `.badge` into the document's stylesheet…
        doc.insert(
            0,
            None,
            BuilderNodeKind::Component {
                library: "user".into(),
                name: "badge".into(),
            },
            BTreeMap::new(),
        )
        .expect("instance");
        // …which this plain span matches by its class.
        let mut attrs = BTreeMap::new();
        attrs.insert("class".to_string(), "badge".to_string());
        attrs.insert("style".to_string(), "color: red".to_string());
        let span = doc
            .insert(
                0,
                None,
                BuilderNodeKind::Element {
                    tag: "span".into(),
                },
                attrs,
            )
            .expect("span");
        let css = resolve_css(&CssSource::Node(span), &doc, &map).expect("css");
        assert!(css.contains("margin-top"), "{css}");
        assert!(css.contains(".badge { color: red }"), "{css}");
        assert!(resolve_css(&CssSource::Node(0), &doc, &map)
            .expect("body")
            .is_empty());
    }

    #[test]
    fn a_rule_matches_a_node_by_its_last_compound_selector_only() {
        let (css, _) = azul_css::parser2::new_from_str(
            "div .card { margin-top: 1px; } .other { margin-top: 2px; } p.card { margin-top: 3px; }",
        );
        let rules = css.rules.as_ref();
        assert!(rule_matches(&rules[0].path, "span", &["card"], &[]));
        assert!(!rule_matches(&rules[1].path, "span", &["card"], &[]));
        assert!(!rule_matches(&rules[2].path, "span", &["card"], &[]));
        assert!(rule_matches(&rules[2].path, "p", &["card", "x"], &[]));
    }

    #[test]
    fn a_rule_index_past_the_end_is_refused_and_a_subset_counts_its_rules() {
        let css = ".a { margin-top: 1px; } .b { margin-top: 2px; }";
        assert!(compile_css(css, "rust", Some(&[2])).is_err());
        let (_, n) = compile_css(css, "rust", Some(&[1, 1, 0])).expect("compiles");
        assert_eq!(n, 2, "indices are de-duplicated");
        assert!(compile_css(css, "klingon", None)
            .unwrap_err()
            .contains("available:"));
    }
}
