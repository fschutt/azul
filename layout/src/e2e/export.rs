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
//!
//! Plus what Export > Code downloads (`project_files`): the app, its build
//! file, the exportable component libraries and a README — and the
//! `compile_fn` of template components (`builder_template_compile_fn`).
//!
//! **No code generator lives here.** Markup is LOWERED to the
//! language-neutral codegen IR by `azul_core::xml::lower_xml_fragment` and
//! PRINTED by `azul_css::codegen` (one printer per binding language, the
//! same printers as the CSS export). This module only picks the markup (a
//! document subtree, a component's template or default rendering, the
//! document, the live page), its stylesheet, the parameters, and assembles
//! the files.
//!
//! The ops that call this live in `full.rs` (`get_codegen_languages`,
//! `get_css_rules`, `compile_css`, `export_subtree_code`,
//! `export_component_code`, `export_code`, `export_code_zip`).

use std::fmt::Write as _;

use azul_core::xml::{
    lower_xml_fragment, lower_xml_fragment_app, lower_xml_page_app, CompileTarget,
    ComponentDataModel, ComponentDef, ComponentDefaultValue, ComponentFieldType, ComponentMap,
    FragmentParam, OptionComponentDefaultValue, ResultStringCompileError, XmlNodeChild,
};
use azul_css::{
    codegen::{
        all_backends, backend_for,
        ir::{ComponentSpec, Ident, LibrarySpec, Module},
        supported_languages, CodegenBackend, GeneratedFile,
    },
    css::{Css, CssDeclaration, CssPath, CssPathSelector, CssRuleBlock},
    AzString,
};

use super::builder::{self, BuilderDocument, BuilderNode, BuilderNodeKind, ROOT_UID};

// ===========================================================================
// Results
// ===========================================================================

/// One file of an exported project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeFile {
    pub path: String,
    pub contents: String,
}

impl From<GeneratedFile> for CodeFile {
    fn from(f: GeneratedFile) -> Self {
        Self {
            path: f.path,
            contents: f.contents,
        }
    }
}

/// Generated code, as the dialogs show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeExport {
    /// The language id (`rust`, `c`, `cpp`, `python`, ...).
    pub language: String,
    /// The file name of `code` (for the download button).
    pub file_name: String,
    /// The code the dialog shows first.
    pub code: String,
    /// Every file, when the result is a project (an app); else empty.
    pub files: Vec<CodeFile>,
    pub warnings: Vec<String>,
}

impl CodeExport {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "language": self.language,
            "file_name": self.file_name,
            "code": self.code,
            "files": self
                .files
                .iter()
                .map(|f| serde_json::json!({ "path": f.path, "contents": f.contents }))
                .collect::<Vec<_>>(),
            "warnings": self.warnings,
        })
    }
}

// ===========================================================================
// Languages: ONE list, the code generators azul_css has
// ===========================================================================

/// `get_codegen_languages`: every code generator, in documentation order:
/// `{languages: [{id, label, ext, dom}]}` (`dom`: its printer does DOM
/// export; the CSS dialog offers them all).
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
            }))
            .collect::<Vec<_>>(),
    })
}

/// The code generator for `lang` (an id or an alias).
///
/// # Errors
/// An unknown language.
pub fn backend(lang: &str) -> Result<Box<dyn CodegenBackend>, String> {
    backend_for(lang).ok_or_else(|| {
        format!(
            "no code generator for {lang:?}; available: {}",
            supported_languages()
        )
    })
}

/// The warning for a DOM export in a language whose printer does not do it.
fn dom_warning(b: &dyn CodegenBackend) -> Option<String> {
    (!b.exports_dom()).then(|| {
        format!(
            "the {} printer does not print DOM construction yet: the code says why instead of \
             building the UI",
            b.display_name()
        )
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
    /// Every component stylesheet the builder document uses.
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
        CssSource::Document => Ok(builder::export_node_xml(&doc.root, map).1),
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
            let sheet = builder::export_node_xml(&doc.root, map).1;
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
// DOM: subtree → code
// ===========================================================================

/// What "Subtree → code" produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtreeMode {
    /// One render function (and what it needs above it).
    Function,
    /// A complete program that shows the subtree in a window.
    App,
}

impl SubtreeMode {
    /// `function` (default) or `app`.
    ///
    /// # Errors
    /// Anything else.
    pub fn parse(s: Option<&str>) -> Result<Self, String> {
        match s.unwrap_or("function") {
            "function" | "fn" => Ok(Self::Function),
            "app" | "program" => Ok(Self::App),
            other => Err(format!("unknown mode {other:?}; use function or app")),
        }
    }
}

fn parse_xml(xml: &str) -> Result<Vec<XmlNodeChild>, String> {
    crate::xml::parse_xml_string(xml)
        .map_err(|e| format!("the exported markup does not parse: {e:?}"))
}

/// `render_<id | first class | tag>`, `render_document` for the root.
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
    format!("render_{}", Ident::from_text(&base).snake())
}

/// The project file a dialog shows first: the entry point.
fn main_file(files: &[CodeFile]) -> Option<&CodeFile> {
    files
        .iter()
        .find(|f| {
            let name = f.path.rsplit('/').next().unwrap_or(&f.path);
            name.starts_with("main.") || name.starts_with("Main.") || name.starts_with("app.")
        })
        .or_else(|| files.first())
}

/// The files of an app module ([`Module::app`]): the printer's project
/// (build file, the module, a `main` that opens the window). A printer that
/// does not do DOM export has no app to write: its module alone (which says
/// why, item by item), not the CSS harness `emit_project_files` would make.
fn app_files(b: &dyn CodegenBackend, m: &Module) -> Vec<CodeFile> {
    if b.exports_dom() {
        b.emit_project_files(m)
            .into_iter()
            .map(CodeFile::from)
            .collect()
    } else {
        vec![CodeFile {
            path: format!("ui.{}", b.extension()),
            contents: b.emit_module(m),
        }]
    }
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
    mode: SubtreeMode,
    fn_name: Option<&str>,
) -> Result<CodeExport, String> {
    let b = backend(language)?;
    let node = doc.node(uid)?;
    let (xml, css) = builder::export_node_xml(node, map);
    let nodes = parse_xml(&xml)?;
    let warnings: Vec<String> = dom_warning(&*b).into_iter().collect();
    match mode {
        SubtreeMode::Function => {
            let name = fn_name
                .filter(|n| !n.trim().is_empty())
                .map_or_else(|| default_fn_name(node), str::to_string);
            let m = lower_xml_fragment(&nodes, &css, &name, None, Vec::new());
            Ok(CodeExport {
                language: b.lang().to_string(),
                file_name: format!("{}.{}", m.items[0].name.snake(), b.extension()),
                code: b.emit_module(&m),
                files: Vec::new(),
                warnings,
            })
        }
        SubtreeMode::App => {
            let m = lower_xml_fragment_app(&nodes, &css, "AzBuilder app");
            let files = app_files(&*b, &m);
            let first = main_file(&files).cloned().unwrap_or(CodeFile {
                path: String::new(),
                contents: String::new(),
            });
            Ok(CodeExport {
                language: b.lang().to_string(),
                file_name: first.path.rsplit('/').next().unwrap_or("").to_string(),
                code: first.contents,
                files,
                warnings,
            })
        }
    }
}

// ===========================================================================
// DOM: component → code
// ===========================================================================

fn default_string(v: &OptionComponentDefaultValue) -> String {
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

/// A template's parameters: every data-model field that is a value (not a
/// callback, not a child slot), with its default as a string, and each
/// field's description.
fn template_params(dm: &ComponentDataModel) -> (Vec<FragmentParam>, Vec<String>) {
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
                FragmentParam::new(f.name.as_str(), &default_string(&f.default_value)),
                f.description.as_str().to_string(),
            )
        })
        .unzip()
}

fn qualified(def: &ComponentDef) -> String {
    format!(
        "{}:{}",
        def.id.collection.as_str(),
        def.id.name.as_str()
    )
}

/// `render_<name>`.
fn render_fn_name(def: &ComponentDef) -> String {
    format!("render_{}", Ident::from_text(def.id.name.as_str()).snake())
}

/// Components of one library as an IR module: one item per component (its
/// template with its parameters, or what it renders with its default data)
/// and the [`LibrarySpec`] the printers register them from.
///
/// # Errors
/// A component whose template does not parse, or whose `render_fn` fails.
pub fn library_module(
    map: &ComponentMap,
    library: &str,
    defs: &[&ComponentDef],
    warnings: &mut Vec<String>,
) -> Result<Module, String> {
    let mut m = Module::default();
    let mut components = Vec::new();
    for def in defs {
        let (xml, css, is_template) = builder::component_export_xml(def, map)?;
        let nodes = parse_xml(&xml)?;
        let (params, descriptions) = if is_template {
            template_params(&def.data_model)
        } else {
            warnings.push(format!(
                "{} has no template (it was not made in AzBuilder): exported what it renders \
                 with its default data",
                qualified(def)
            ));
            (Vec::new(), Vec::new())
        };
        let fn_name = render_fn_name(def);
        let doc = vec![format!(
            "`{}` ({}){}",
            qualified(def),
            def.display_name.as_str(),
            if is_template {
                ""
            } else {
                ": its default rendering (the component has no template)"
            }
        )];
        let mut lowered = lower_xml_fragment(
            &nodes,
            &css,
            &fn_name,
            is_template.then_some(params.as_slice()),
            doc,
        );
        components.push(ComponentSpec {
            item: Ident::from_text(&fn_name),
            name: def.id.name.as_str().to_string(),
            display_name: def.display_name.as_str().to_string(),
            description: def.description.as_str().to_string(),
            data_model: def.data_model.name.as_str().to_string(),
            data_model_description: def.data_model.description.as_str().to_string(),
            field_descriptions: descriptions,
        });
        m.items.append(&mut lowered.items);
    }
    let version = map
        .libraries
        .iter()
        .find(|l| l.name.as_str() == library)
        .map_or_else(|| "0.1.0".to_string(), |l| l.version.as_str().to_string());
    m.library = Some(LibrarySpec {
        name: library.to_string(),
        version,
        components,
    });
    Ok(m)
}

/// `export_component_code`: one component as code — its render function
/// and, where the language's printer spells it, its library's registration
/// (holding just this component).
///
/// # Errors
/// Unknown component or language; a template that does not parse.
pub fn component_code(
    map: &ComponentMap,
    library: &str,
    name: &str,
    language: &str,
) -> Result<CodeExport, String> {
    let def = map
        .get(library, name)
        .ok_or_else(|| format!("Component '{name}' not found in library '{library}'"))?;
    let mut out = library_code(map, library, &[def], language)?;
    out.file_name = format!(
        "{}_{}.{}",
        Ident::from_text(library).snake(),
        Ident::from_text(name).snake(),
        backend(language)?.extension()
    );
    Ok(out)
}

/// Components of one library as one source file.
///
/// # Errors
/// Unknown language; a component whose markup does not parse.
pub fn library_code(
    map: &ComponentMap,
    library: &str,
    defs: &[&ComponentDef],
    language: &str,
) -> Result<CodeExport, String> {
    let b = backend(language)?;
    let mut warnings: Vec<String> = dom_warning(&*b).into_iter().collect();
    let m = library_module(map, library, defs, &mut warnings)?;
    Ok(CodeExport {
        language: b.lang().to_string(),
        file_name: format!("{}.{}", Ident::from_text(library).snake(), b.extension()),
        code: b.emit_module(&m),
        files: Vec::new(),
        warnings,
    })
}

// ===========================================================================
// compile_fn of template components
// ===========================================================================

/// `compile_fn` of a component made in AzBuilder: its template as a render
/// function for `target` (what the Components view's "compile_fn" shows),
/// printed by the same code generators. Nested components of other user
/// libraries are unknown here (a compile_fn gets no component map) and
/// compile to a visible placeholder; `export_component_code` has the map.
#[must_use]
pub fn builder_template_compile_fn(
    def: &ComponentDef,
    target: &CompileTarget,
    data: &ComponentDataModel,
    _indent: usize,
) -> ResultStringCompileError {
    let lang = match target {
        CompileTarget::Rust => "rust",
        CompileTarget::C => "c",
        CompileTarget::Cpp => "cpp",
        CompileTarget::Python => "python",
    };
    let map = ComponentMap::with_builtin();
    let compiled = (|| -> Result<String, String> {
        let b = backend(lang)?;
        let (xml, css, is_template) = builder::component_export_xml(def, &map)?;
        let nodes = parse_xml(&xml)?;
        let (params, _) = template_params(data);
        let m = lower_xml_fragment(
            &nodes,
            &css,
            &render_fn_name(def),
            is_template.then_some(params.as_slice()),
            vec![format!("`{}`", qualified(def))],
        );
        Ok(b.emit_module(&m))
    })();
    let text = match compiled {
        Ok(src) => src,
        Err(e) => {
            let cmt = if matches!(target, CompileTarget::Python) {
                "#"
            } else {
                "//"
            };
            format!(
                "{cmt} cannot compile {}: {}\n",
                qualified(def),
                e.replace(|c: char| c == '\n' || c == '\r', " ")
            )
        }
    };
    ResultStringCompileError::Ok(AzString::from(text))
}

// ===========================================================================
// Export > Code: the project
// ===========================================================================

/// The builder document (`<body>` and its component CSS) as an app
/// project for `language`.
///
/// # Errors
/// Unknown language; the document's markup does not parse.
pub fn document_app(
    doc: &BuilderDocument,
    map: &ComponentMap,
    language: &str,
) -> Result<Vec<CodeFile>, String> {
    let b = backend(language)?;
    let (xml, css) = builder::export_node_xml(&doc.root, map);
    let nodes = parse_xml(&xml)?;
    let m = lower_xml_fragment_app(&nodes, &css, "AzBuilder app");
    Ok(app_files(&*b, &m))
}

/// The live page (`StyledDom::get_html_string`: a `<head><style>` and a
/// body whose nodes carry their computed style) as an app project.
///
/// # Errors
/// Unknown language; the HTML does not parse or has no body.
pub fn live_page_app(html: &str, language: &str) -> Result<Vec<CodeFile>, String> {
    let b = backend(language)?;
    let nodes = parse_xml(html)?;
    let m = lower_xml_page_app(&nodes, "Azul app").map_err(|e| format!("codegen: {e}"))?;
    Ok(app_files(&*b, &m))
}

/// Everything Export > Code writes for `language`: the app (`app`, from
/// [`document_app`] or [`live_page_app`]: build file + module + main),
/// every exportable component library (or only `library_filter`) as a file
/// of its own, and a README.
///
/// # Errors
/// Unknown language. (A component library that does not compile is left out
/// with a warning.)
pub fn project_files(
    language: &str,
    app: Vec<CodeFile>,
    map: &ComponentMap,
    library_filter: Option<&str>,
    warnings: &mut Vec<String>,
) -> Result<Vec<CodeFile>, String> {
    let b = backend(language)?;
    let mut files = app;
    let mut component_paths: Vec<String> = Vec::new();
    let mut rust_mods: Vec<String> = Vec::new();
    for lib in map.get_exportable_libraries() {
        if library_filter.is_some_and(|f| f != lib.name.as_str()) {
            continue;
        }
        let defs: Vec<&ComponentDef> = lib.components.iter().collect();
        if defs.is_empty() {
            continue;
        }
        // One library that does not compile must not cost the user the app.
        match library_code(map, lib.name.as_str(), &defs, b.lang()) {
            Ok(code) => {
                warnings.extend(code.warnings.iter().cloned());
                let sn = Ident::from_text(lib.name.as_str()).snake();
                let path = if b.lang() == "rust" {
                    rust_mods.push(sn.clone());
                    format!("src/components/{sn}.rs")
                } else {
                    format!("components/{sn}.{}", b.extension())
                };
                component_paths.push(path.clone());
                files.push(CodeFile {
                    path,
                    contents: code.code,
                });
            }
            Err(e) => warnings.push(format!(
                "component library '{}' left out: {e}",
                lib.name.as_str()
            )),
        }
    }
    if !rust_mods.is_empty() {
        let mut m = String::new();
        for sn in &rust_mods {
            let _ = writeln!(m, "pub mod {sn};");
        }
        files.push(CodeFile {
            path: "src/components/mod.rs".to_string(),
            contents: m,
        });
        if let Some(main) = files.iter_mut().find(|f| f.path == "src/main.rs") {
            main.contents
                .push_str("\n#[allow(dead_code)]\nmod components;\n");
        }
    }
    let listed: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    files.push(CodeFile {
        path: "README.md".to_string(),
        contents: readme(&*b, &listed, &component_paths),
    });
    Ok(files)
}

fn readme(b: &dyn CodegenBackend, files: &[String], component_paths: &[String]) -> String {
    let build = match b.lang() {
        "rust" => "cargo run --release\n# the bindings link against libazul: export \
                   AZ_LINK_PATH=<the directory holding libazul>"
            .to_string(),
        "c" | "cpp" => "make AZUL_INCLUDE=<azul>/target/codegen AZUL_LIB=<azul>/target/release\n./app"
            .to_string(),
        "python" => "python3 main.py   # needs the azul extension module next to it".to_string(),
        // The printer's project files carry their build steps as header comments.
        _ if b.exports_dom() => format!(
            "# see the comments at the top of the {} build file and main file above",
            b.display_name()
        ),
        _ => format!(
            "# see the {} files above; the {} printer does not write a runnable app yet",
            b.display_name(),
            b.display_name()
        ),
    };
    let comps = if component_paths.is_empty() {
        String::new()
    } else {
        format!(
            "\nThe component libraries are in {}: each `render_*` function builds one \
             component; where the language can spell it, `register_<library>_library` \
             registers them for XML / AzBuilder.\n",
            component_paths
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "# AzBuilder export ({})\n\nFiles: {}.\n{comps}\n## Build and run\n\n```sh\n{build}\n```\n\n\
         `<azul>` is an azul checkout with the bindings generated (`cargo run --release -p \
         azul-doc -- codegen all`) and libazul built (`cargo build --release -p azul-dll \
         --features build-dll`).\n",
        b.display_name(),
        files
            .iter()
            .map(|p| format!("`{p}`"))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use azul_core::xml::{
        ComponentDataField, ComponentDefaultValue, ComponentFieldType, ComponentMap,
        OptionComponentDefaultValue,
    };

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
    fn the_compile_fn_of_a_template_component_is_the_template_as_a_function() {
        let map = badge_map();
        let def = map.get("user", "badge").expect("badge");
        match (def.compile_fn)(def, &CompileTarget::C, &def.data_model, 0) {
            ResultStringCompileError::Ok(s) => {
                assert!(
                    s.as_str().contains("static AzDom render_badge(const char* text) {"),
                    "{}",
                    s.as_str()
                );
            }
            ResultStringCompileError::Err(e) => panic!("{e:?}"),
        }
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
