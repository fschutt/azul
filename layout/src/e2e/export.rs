//! AzBuilder's quick exports — text in, text out, no zip:
//!
//! * **Compile CSS to…** — list the rules of a stylesheet (pasted text, the
//!   document's stylesheet, the selected node's style, a component's CSS) and
//!   compile all or some of them with a CSS code generator
//!   (`azul_css::codegen::CodegenBackend`, looked up by language name).
//! * **Subtree → code** — a subtree of the builder document as a render
//!   function (or a runnable app) in Rust / C / C++ / Python
//!   (`azul_core::xml::compile_xml_fragment`).
//! * **Component → code** — a component as code: its render function (a
//!   converted component's `{placeholders}` become parameters), a
//!   default-arguments wrapper and, for Rust / C / C++, the registration
//!   (`register_<library>_library`, a `ComponentDef` per component).
//!
//! Plus what Export > Code downloads (`project_files`): the app, its build
//! file, the exportable component libraries and a README — and the
//! `compile_fn` of template components (`builder_template_compile_fn`).
//!
//! The ops that call this live in `full.rs` (`get_codegen_languages`,
//! `get_css_rules`, `compile_css`, `export_subtree_code`,
//! `export_component_code`, `export_code`, `export_code_zip`).

use std::{collections::BTreeSet, fmt::Write as _};

use azul_core::xml::{
    compile_xml_fragment, compile_xml_fragment_app, CompileTarget, CompiledFragment,
    ComponentDataModel, ComponentDef, ComponentDefaultValue, ComponentFieldType, ComponentMap,
    FragmentParam, OptionComponentDefaultValue, ResultStringCompileError, XmlNodeChild,
};
use azul_css::{
    css::{Css, CssDeclaration, CssPath, CssPathSelector, CssRuleBlock},
    AzString,
};

use super::builder::{self, BuilderDocument, BuilderNode, BuilderNodeKind, ROOT_UID};

// ===========================================================================
// Results
// ===========================================================================

/// One piece of generated code, as the dialogs show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeExport {
    /// The language id (`rust`, `c`, `cpp`, `python`, or a CSS backend's id).
    pub language: String,
    /// A file name for the download button.
    pub file_name: String,
    pub code: String,
    pub warnings: Vec<String>,
}

impl CodeExport {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "language": self.language,
            "file_name": self.file_name,
            "code": self.code,
            "warnings": self.warnings,
        })
    }
}

/// One file of an exported project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeFile {
    pub path: String,
    pub contents: String,
}

// ===========================================================================
// Languages
// ===========================================================================

/// A language the DOM export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomLanguage {
    pub id: &'static str,
    pub label: &'static str,
    pub ext: &'static str,
}

/// Every language `CompileTarget` has.
pub const DOM_LANGUAGES: [DomLanguage; 4] = [
    DomLanguage {
        id: "rust",
        label: "Rust",
        ext: "rs",
    },
    DomLanguage {
        id: "c",
        label: "C",
        ext: "c",
    },
    DomLanguage {
        id: "cpp",
        label: "C++",
        ext: "cpp",
    },
    DomLanguage {
        id: "python",
        label: "Python",
        ext: "py",
    },
];

/// `lang` (`rust`, `c`, `cpp` / `c++`, `python` / `py`) as a target.
///
/// # Errors
/// A language with no DOM code generator.
pub fn dom_language(lang: &str) -> Result<(CompileTarget, DomLanguage), String> {
    let id = match lang.trim().to_ascii_lowercase().as_str() {
        "rust" | "rs" => "rust",
        "c" => "c",
        "cpp" | "c++" | "cxx" => "cpp",
        "python" | "py" => "python",
        _ => {
            return Err(format!(
                "no DOM code generator for {lang:?}; available: rust, c, cpp, python"
            ))
        }
    };
    let target = match id {
        "rust" => CompileTarget::Rust,
        "c" => CompileTarget::C,
        "cpp" => CompileTarget::Cpp,
        _ => CompileTarget::Python,
    };
    let l = DOM_LANGUAGES
        .iter()
        .copied()
        .find(|l| l.id == id)
        .unwrap_or(DOM_LANGUAGES[0]);
    Ok((target, l))
}

/// Names to ask the CSS code generators for (`azul_css::codegen::backend_for`).
/// Every name a backend answers to is listed once, under the backend's own
/// `lang()` — so a new backend shows up in the dialog as soon as
/// `backend_for` knows its name.
const CSS_LANGUAGE_CANDIDATES: &[&str] = &[
    "rust", "c", "cpp", "python", "csharp", "java", "kotlin", "swift", "go", "zig", "odin",
    "d", "nim", "lua", "ruby", "php", "javascript", "typescript", "dart", "haskell", "ocaml",
    "fsharp", "julia", "fortran", "ada", "pascal", "crystal", "v", "racket", "lisp", "perl",
    "elixir", "scala", "r", "vb6", "smalltalk",
];

/// The CSS code generator for `lang`, if there is one. The ONE place that
/// knows how CSS backends are found (B2 is rewriting `azul_css::codegen`;
/// only this function has to follow it).
fn css_backend(lang: &str) -> Option<Box<dyn azul_css::codegen::CodegenBackend>> {
    azul_css::codegen::backend_for(lang.trim().to_ascii_lowercase().as_str())
}

/// Every CSS code generator the server has: `[{id, label, ext}]`.
#[must_use]
pub fn css_languages() -> Vec<serde_json::Value> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for cand in CSS_LANGUAGE_CANDIDATES {
        if let Some(b) = css_backend(cand) {
            let id = b.lang();
            if seen.insert(id) {
                out.push(serde_json::json!({
                    "id": id,
                    "label": language_label(id),
                    "ext": language_ext(id),
                }));
            }
        }
    }
    out
}

/// `get_codegen_languages`: `{dom: [...], css: [...]}`.
#[must_use]
pub fn languages_json() -> serde_json::Value {
    serde_json::json!({
        "dom": DOM_LANGUAGES
            .iter()
            .map(|l| serde_json::json!({ "id": l.id, "label": l.label, "ext": l.ext }))
            .collect::<Vec<_>>(),
        "css": css_languages(),
    })
}

fn language_label(id: &str) -> String {
    let known = match id {
        "rust" => "Rust",
        "c" => "C",
        "cpp" => "C++",
        "python" => "Python",
        "csharp" => "C#",
        "java" => "Java",
        "kotlin" => "Kotlin",
        "swift" => "Swift",
        "go" => "Go",
        "zig" => "Zig",
        "odin" => "Odin",
        "d" => "D",
        "nim" => "Nim",
        "lua" => "Lua",
        "ruby" => "Ruby",
        "php" => "PHP",
        "javascript" => "JavaScript",
        "typescript" => "TypeScript",
        "dart" => "Dart",
        "haskell" => "Haskell",
        "ocaml" => "OCaml",
        "fsharp" => "F#",
        "julia" => "Julia",
        _ => return id.to_string(),
    };
    known.to_string()
}

fn language_ext(id: &str) -> &'static str {
    match id {
        "rust" => "rs",
        "c" => "c",
        "cpp" => "cpp",
        "python" => "py",
        "csharp" => "cs",
        "java" => "java",
        "kotlin" => "kt",
        "swift" => "swift",
        "go" => "go",
        "zig" => "zig",
        "odin" => "odin",
        "d" => "d",
        "nim" => "nim",
        "lua" => "lua",
        "ruby" => "rb",
        "php" => "php",
        "javascript" => "js",
        "typescript" => "ts",
        "dart" => "dart",
        "haskell" => "hs",
        "ocaml" => "ml",
        "fsharp" => "fs",
        "julia" => "jl",
        _ => "txt",
    }
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
    match d {
        CssDeclaration::Static(p) => format!("{}: {};", p.key(), p.value()),
        CssDeclaration::Dynamic(dy) => format!(
            "{}: var(--{}, {});",
            dy.default_value.key(),
            dy.dynamic_id.as_str(),
            dy.default_value.value()
        ),
    }
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
/// the CSS code generator for `language`, and how many rules went in.
///
/// # Errors
/// No generator for the language; a rule index past the end.
pub fn compile_css(
    css_text: &str,
    language: &str,
    rules: Option<&[usize]>,
) -> Result<(CodeExport, usize), String> {
    let backend = css_backend(language).ok_or_else(|| {
        let have: Vec<String> = css_languages()
            .iter()
            .filter_map(|l| l.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        format!(
            "no CSS code generator for {language:?}; available: {}",
            have.join(", ")
        )
    })?;
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
    let id = backend.lang();
    let rule_count = chosen.rules.as_ref().len();
    Ok((
        CodeExport {
            language: id.to_string(),
            file_name: format!("styles.{}", language_ext(id)),
            code: backend.emit_css(&chosen),
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
    crate::xml::parse_xml_string(xml).map_err(|e| format!("the exported markup does not parse: {e:?}"))
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
    format!("render_{}", snake(&base))
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
    let (target, lang) = dom_language(language)?;
    let node = doc.node(uid)?;
    let (xml, css) = builder::export_node_xml(node, map);
    let nodes = parse_xml(&xml)?;
    let (code, file_name) = match mode {
        SubtreeMode::Function => {
            let name = fn_name
                .filter(|n| !n.trim().is_empty())
                .map_or_else(|| default_fn_name(node), str::to_string);
            let f = compile_xml_fragment(&nodes, &css, &target, &name, None)
                .map_err(|e| format!("codegen: {e}"))?;
            let file = format!("{}.{}", f.fn_name, lang.ext);
            (f.source(), file)
        }
        SubtreeMode::App => {
            let code = compile_xml_fragment_app(&nodes, &css, &target, "AzBuilder app")
                .map_err(|e| format!("codegen: {e}"))?;
            (code, app_file_name(&lang).to_string())
        }
    };
    Ok(CodeExport {
        language: lang.id.to_string(),
        file_name,
        code,
        warnings: Vec::new(),
    })
}

fn app_file_name(lang: &DomLanguage) -> &'static str {
    match lang.id {
        "rust" => "main.rs",
        "c" => "main.c",
        "cpp" => "main.cpp",
        _ => "main.py",
    }
}

// ===========================================================================
// DOM: component → code
// ===========================================================================

/// `snake_case` identifier from a component / library name.
fn snake(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        s.push('x');
    }
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, 'c');
    }
    s
}

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
/// callback, not a child slot), with its default as a string.
fn template_params(dm: &ComponentDataModel) -> Vec<FragmentParam> {
    dm.fields
        .as_ref()
        .iter()
        .filter(|f| {
            !matches!(
                f.field_type,
                ComponentFieldType::Callback(_) | ComponentFieldType::StyledDom
            )
        })
        .map(|f| FragmentParam::new(f.name.as_str(), &default_string(&f.default_value)))
        .collect()
}

/// One component, compiled.
struct Part<'a> {
    def: &'a ComponentDef,
    snake: String,
    frag: CompiledFragment,
    /// `Some`: a template (the function takes these); `None`: the default
    /// rendering of a component without a template.
    params: Option<Vec<FragmentParam>>,
}

fn compile_part<'a>(
    def: &'a ComponentDef,
    map: &ComponentMap,
    target: &CompileTarget,
    warnings: &mut Vec<String>,
) -> Result<Part<'a>, String> {
    let (xml, css, is_template) = builder::component_export_xml(def, map)?;
    let nodes = parse_xml(&xml)?;
    let snake = snake(def.id.name.as_str());
    let params = is_template.then(|| template_params(&def.data_model));
    if !is_template {
        warnings.push(format!(
            "{}:{} has no template (it was not made in AzBuilder): exported what it renders with \
             its default data",
            def.id.collection.as_str(),
            def.id.name.as_str()
        ));
    }
    let frag = compile_xml_fragment(
        &nodes,
        &css,
        target,
        &format!("render_{snake}"),
        params.as_deref(),
    )
    .map_err(|e| format!("codegen: {e}"))?;
    Ok(Part {
        def,
        snake,
        frag,
        params,
    })
}

/// `export_component_code`: one component as code (its library's
/// registration holds just this component).
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
        snake(library),
        snake(name),
        dom_language(language)?.1.ext
    );
    Ok(out)
}

/// Components of one library as one source file: per component its render
/// function and a default-arguments wrapper; for Rust, C and C++ also a
/// `ComponentDef` per component and `register_<library>_library()`.
///
/// # Errors
/// Unknown language; a component whose markup does not parse.
pub fn library_code(
    map: &ComponentMap,
    library: &str,
    defs: &[&ComponentDef],
    language: &str,
) -> Result<CodeExport, String> {
    let (target, lang) = dom_language(language)?;
    let mut warnings = Vec::new();
    let parts: Vec<Part<'_>> = defs
        .iter()
        .map(|d| compile_part(*d, map, &target, &mut warnings))
        .collect::<Result<_, _>>()?;
    let version = map
        .libraries
        .iter()
        .find(|l| l.name.as_str() == library)
        .map_or_else(|| "0.1.0".to_string(), |l| l.version.as_str().to_string());
    let lib = LibraryInfo {
        name: library,
        snake: snake(library),
        version: &version,
    };
    let code = match target {
        CompileTarget::Rust => rust_library(&lib, &parts),
        CompileTarget::C => c_library(&lib, &parts, false),
        CompileTarget::Cpp => c_library(&lib, &parts, true),
        CompileTarget::Python => python_library(&lib, &parts),
    };
    Ok(CodeExport {
        language: lang.id.to_string(),
        file_name: format!("{}.{}", lib.snake, lang.ext),
        code,
        warnings,
    })
}

struct LibraryInfo<'a> {
    name: &'a str,
    snake: String,
    version: &'a str,
}

/// A double-quoted string literal (the same escapes in Rust, C, C++ and
/// Python for what a name, a default or a description can contain).
fn q(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\{:03o}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One line for a comment (no `*/`, no newline).
fn comment_line(s: &str) -> String {
    s.replace("*/", "* /")
        .replace(|c: char| c == '\n' || c == '\r', " ")
}

fn qualified(def: &ComponentDef) -> String {
    format!(
        "{}:{}",
        def.id.collection.as_str(),
        def.id.name.as_str()
    )
}

fn params_doc(part: &Part<'_>) -> String {
    match &part.params {
        None => "Its default rendering (the component has no template).".to_string(),
        Some(p) if p.is_empty() => "It takes no parameters.".to_string(),
        Some(p) => format!(
            "Parameters (defaults): {}.",
            p.iter()
                .map(|f| format!("{} = {}", f.name, q(&f.default)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The literals of the defaults, in parameter order.
fn default_args(part: &Part<'_>) -> Vec<String> {
    part.params
        .as_ref()
        .map(|p| p.iter().map(|f| q(&f.default)).collect())
        .unwrap_or_default()
}

// ── Rust ──

const RUST_IMPORTS: &str = "use azul::component::{
    CompileTarget, ComponentDataField, ComponentDataModel, ComponentDef, ComponentDefaultValue,
    ComponentFieldType, ComponentId, ComponentLibrary, ComponentMap, ComponentSource,
};
use azul::error::{ResultStringCompileError, ResultStyledDomRenderDomError};
use azul::option::OptionComponentDefaultValue;
use azul::str::String as AzString;
use azul::vec::{ComponentDataFieldVec, ComponentDataModelVec, ComponentEnumModelVec};
";

/// What the Rust registration calls when a component takes parameters.
const RUST_HELPERS: &str = r#"
/// The String value of the data-model field `name`, or `default`.
fn model_string(model: &ComponentDataModel, name: &str, default: &str) -> String {
    for field in model.fields.as_slice() {
        if field.name.as_str() == name {
            if let OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) =
                &field.default_value
            {
                return s.as_str().to_string();
            }
        }
    }
    default.to_string()
}

/// A String field of a component's data model.
fn string_field(name: &str, default: &str, description: &str) -> ComponentDataField {
    ComponentDataField {
        name: AzString::from(name),
        field_type: ComponentFieldType::String,
        default_value: OptionComponentDefaultValue::Some(ComponentDefaultValue::String(
            AzString::from(default),
        )),
        required: false,
        description: AzString::from(description),
    }
}
"#;

fn field_description(def: &ComponentDef, name: &str) -> String {
    def.data_model
        .fields
        .as_ref()
        .iter()
        .find(|d| d.name.as_str() == name)
        .map(|d| d.description.as_str().to_string())
        .unwrap_or_default()
}

fn has_params(p: &Part<'_>) -> bool {
    p.params.as_ref().is_some_and(|v| !v.is_empty())
}

fn rust_library(lib: &LibraryInfo<'_>, parts: &[Part<'_>]) -> String {
    let names: Vec<String> = parts.iter().map(|p| qualified(p.def)).collect();
    let lib_q = q(lib.name);
    let lsn = &lib.snake;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "//! Component library `{}` — exported by AzBuilder: {}.",
        lib.name,
        names.join(", ")
    );
    s.push_str("//!\n");
    s.push_str("//! Each `render_*` function builds its component's DOM; call it from a layout\n");
    let _ = writeln!(
        s,
        "//! callback. `register_{lsn}_library` registers the components, so XML and"
    );
    s.push_str("//! AzBuilder can use them:\n//!\n//! ```ignore\n");
    let _ = writeln!(
        s,
        "//! config.add_component_library({lib_q}, register_{lsn}_library);"
    );
    s.push_str("//! ```\n#![allow(dead_code, unused_imports)]\n\nuse azul::prelude::*;\n");
    if parts.iter().any(|p| p.frag.header.contains("SmallAriaInfo")) {
        s.push_str("use azul::dom::SmallAriaInfo;\n");
    }
    s.push_str(RUST_IMPORTS);

    for p in parts {
        let def = p.def;
        let sn = &p.snake;
        let qn = qualified(def);
        let _ = writeln!(s, "\n// ── {qn} ──\n");
        let _ = writeln!(
            s,
            "/// `{qn}` ({}). {}",
            comment_line(def.display_name.as_str()),
            params_doc(p)
        );
        s.push_str(&p.frag.function);

        let _ = writeln!(s, "\n/// `{qn}` with its default arguments.");
        let _ = writeln!(s, "pub fn render_{sn}_default() -> Dom {{");
        let _ = writeln!(s, "    render_{sn}({})", default_args(p).join(", "));
        s.push_str("}\n");

        // render_fn: the data model's values → the render function.
        let _ = writeln!(s, "\nextern \"C\" fn {sn}_render_fn(");
        s.push_str("    _def: &ComponentDef,\n    model: &ComponentDataModel,\n");
        s.push_str("    _map: &ComponentMap,\n) -> ResultStyledDomRenderDomError {\n");
        let mut call_args = Vec::new();
        match &p.params {
            Some(params) if !params.is_empty() => {
                for (param, ident) in params.iter().zip(p.frag.param_idents.iter()) {
                    let _ = writeln!(
                        s,
                        "    let {ident} = model_string(model, {}, {});",
                        q(&param.name),
                        q(&param.default)
                    );
                    call_args.push(format!("&{ident}"));
                }
            }
            _ => s.push_str("    let _ = model;\n"),
        }
        let _ = writeln!(
            s,
            "    ResultStyledDomRenderDomError::Ok(StyledDom::create_from_dom(render_{sn}({})))",
            call_args.join(", ")
        );
        s.push_str("}\n");

        let _ = writeln!(s, "\nextern \"C\" fn {sn}_compile_fn(");
        s.push_str("    _def: &ComponentDef,\n    _target: &CompileTarget,\n");
        s.push_str("    _model: &ComponentDataModel,\n    _indent: usize,\n");
        s.push_str(") -> ResultStringCompileError {\n");
        let _ = writeln!(
            s,
            "    ResultStringCompileError::Ok(AzString::from(\"render_{sn}_default()\"))"
        );
        s.push_str("}\n");

        let _ = writeln!(s, "\nfn {sn}_def() -> ComponentDef {{");
        s.push_str("    ComponentDef {\n");
        let _ = writeln!(
            s,
            "        id: ComponentId::create({lib_q}, {}),",
            q(def.id.name.as_str())
        );
        let _ = writeln!(
            s,
            "        display_name: AzString::from({}),",
            q(def.display_name.as_str())
        );
        let _ = writeln!(
            s,
            "        description: AzString::from({}),",
            q(def.description.as_str())
        );
        let _ = writeln!(s, "        // The CSS is applied per node by render_{sn}.");
        s.push_str("        css: AzString::from(\"\"),\n");
        s.push_str("        source: ComponentSource::UserDefined,\n");
        s.push_str("        data_model: ComponentDataModel {\n");
        let _ = writeln!(
            s,
            "            name: AzString::from({}),",
            q(def.data_model.name.as_str())
        );
        let _ = writeln!(
            s,
            "            description: AzString::from({}),",
            q(def.data_model.description.as_str())
        );
        match &p.params {
            Some(params) if !params.is_empty() => {
                s.push_str("            fields: vec![\n");
                for f in params {
                    let _ = writeln!(
                        s,
                        "                string_field({}, {}, {}),",
                        q(&f.name),
                        q(&f.default),
                        q(&field_description(def, &f.name))
                    );
                }
                s.push_str("            ]\n            .into(),\n");
            }
            _ => s.push_str("            fields: ComponentDataFieldVec::create(),\n"),
        }
        s.push_str("        },\n");
        let _ = writeln!(s, "        render_fn: {sn}_render_fn,");
        let _ = writeln!(s, "        compile_fn: {sn}_compile_fn,");
        s.push_str("        render_fn_source: OptionString::none(),\n");
        s.push_str("        compile_fn_source: OptionString::none(),\n");
        s.push_str("    }\n}\n");
    }

    let defs: Vec<String> = parts.iter().map(|p| format!("{}_def()", p.snake)).collect();
    s.push_str("\n// ── registration ──\n\n");
    let _ = writeln!(s, "/// The component library `{}`:", lib.name);
    let _ = writeln!(
        s,
        "/// `config.add_component_library({lib_q}, register_{lsn}_library);`"
    );
    let _ = writeln!(
        s,
        "pub extern \"C\" fn register_{lsn}_library() -> ComponentLibrary {{"
    );
    s.push_str("    ComponentLibrary {\n");
    let _ = writeln!(s, "        name: AzString::from({lib_q}),");
    let _ = writeln!(s, "        version: AzString::from({}),", q(lib.version));
    s.push_str("        description: AzString::from(\"Exported from AzBuilder\"),\n");
    let _ = writeln!(s, "        components: vec![{}].into(),", defs.join(", "));
    s.push_str("        exportable: true,\n        modifiable: false,\n");
    s.push_str("        data_models: ComponentDataModelVec::create(),\n");
    s.push_str("        enum_models: ComponentEnumModelVec::create(),\n");
    s.push_str("    }\n}\n");

    if parts.iter().any(has_params) {
        s.push_str(RUST_HELPERS);
    }
    s
}

// ── C and C++ ──

/// What the C / C++ registration calls when a component takes parameters.
/// Placed above the components: C needs a declaration before the first use.
const C_HELPERS: &str = r#"
/* The String value of the data-model field `name` as a NUL-terminated copy
 * (free() it), or a copy of `fallback`. */
static char* az_model_string(const AzComponentDataModel* model, const char* name, const char* fallback) {
    size_t name_len = strlen(name);
    const char* src = fallback;
    size_t len = strlen(fallback);
    for (size_t i = 0; i < model->fields.len; i++) {
        const AzComponentDataField* f = &model->fields.ptr[i];
        if (f->name.vec.len == name_len && memcmp(f->name.vec.ptr, name, name_len) == 0
            && f->default_value.Some.tag == AzOptionComponentDefaultValue_Tag_Some
            && f->default_value.Some.payload.String.tag == AzComponentDefaultValue_Tag_String) {
            const AzString* s = &f->default_value.Some.payload.String.payload;
            src = (const char*)s->vec.ptr;
            len = s->vec.len;
            break;
        }
    }
    char* out = (char*)malloc(len + 1);
    memcpy(out, src, len);
    out[len] = 0;
    return out;
}

/* A String field of a component's data model. */
static AzComponentDataField az_string_field(const char* name, const char* value, const char* description) {
    AzComponentDataField f;
    f.name = AZ_STR(name);
    f.field_type = AzComponentFieldType_string();
    f.default_value = AzOptionComponentDefaultValue_some(AzComponentDefaultValue_string(AZ_STR(value)));
    f.required = false;
    f.description = AZ_STR(description);
    return f;
}
"#;

/// C, or C++ (`cpp`): the same registration code — it uses only the C API,
/// which azul20.hpp includes — around each language's render function.
fn c_library(lib: &LibraryInfo<'_>, parts: &[Part<'_>], cpp: bool) -> String {
    let names: Vec<String> = parts.iter().map(|p| qualified(p.def)).collect();
    let lib_q = q(lib.name);
    let lsn = &lib.snake;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "/* Component library `{}` — exported by AzBuilder: {}.",
        lib.name,
        comment_line(&names.join(", "))
    );
    s.push_str(" *\n");
    let _ = writeln!(
        s,
        " * Each render_* function builds its component's DOM; register_{lsn}_library()"
    );
    s.push_str(" * registers the components, so XML and AzBuilder can use them:\n");
    let _ = writeln!(
        s,
        " *     AzAppConfig_addComponentLibrary(&config, AZ_STR({lib_q}), register_{lsn}_library);"
    );
    if cpp {
        let _ = writeln!(
            s,
            " * Build: c++ -std=c++20 -c {lsn}.cpp -I <azul>/target/codegen"
        );
    } else {
        let _ = writeln!(s, " * Build: cc -c {lsn}.c -I <azul>/target/codegen");
    }
    s.push_str(" */\n");
    if cpp {
        s.push_str("#include \"azul20.hpp\"\n#include <string>\n#include <stdlib.h>\n");
        s.push_str("#include <string.h>\n\nusing namespace azul;\n");
    } else {
        s.push_str("#include \"azul.h\"\n#include <stdlib.h>\n#include <string.h>\n");
        // A text mixing literals and parameters needs the fragment's joiner.
        if let Some(header) = parts
            .iter()
            .map(|p| p.frag.header.as_str())
            .find(|h| h.contains("az_concat"))
        {
            if let Some(at) = header.find("#include <stdarg.h>") {
                s.push_str(&header[at..]);
            }
        }
    }
    if parts.iter().any(has_params) {
        s.push_str(C_HELPERS);
    }

    for p in parts {
        let def = p.def;
        let sn = &p.snake;
        let qn = qualified(def);
        let _ = writeln!(s, "\n/* ── {qn} ── */\n");
        let _ = writeln!(
            s,
            "/* `{qn}` ({}). {} */",
            comment_line(def.display_name.as_str()),
            comment_line(&params_doc(p))
        );
        s.push_str(&p.frag.function);

        let _ = writeln!(s, "\n/* `{qn}` with its default arguments. */");
        if cpp {
            let _ = writeln!(s, "Dom render_{sn}_default() {{");
        } else {
            let _ = writeln!(s, "AzDom render_{sn}_default(void) {{");
        }
        let _ = writeln!(s, "    return render_{sn}({});", default_args(p).join(", "));
        s.push_str("}\n");

        // render_fn: the data model's values → the render function.
        let _ = writeln!(
            s,
            "\nstatic AzResultStyledDomRenderDomError {sn}_render_fn(const AzComponentDef* def, \
             const AzComponentDataModel* model, const AzComponentMap* map) {{"
        );
        s.push_str("    (void)def;\n    (void)map;\n");
        let mut call_args = Vec::new();
        match &p.params {
            Some(params) if !params.is_empty() => {
                for (param, ident) in params.iter().zip(p.frag.param_idents.iter()) {
                    let _ = writeln!(
                        s,
                        "    char* {ident} = az_model_string(model, {}, {});",
                        q(&param.name),
                        q(&param.default)
                    );
                    call_args.push(ident.clone());
                }
            }
            _ => s.push_str("    (void)model;\n"),
        }
        let call = format!("render_{sn}({})", call_args.join(", "));
        if cpp {
            // The C++ render function returns an owning azul::Dom.
            let _ = writeln!(s, "    AzDom dom = {call}.release();");
        } else {
            let _ = writeln!(s, "    AzDom dom = {call};");
        }
        for ident in &call_args {
            let _ = writeln!(s, "    free({ident});");
        }
        s.push_str(
            "    return AzResultStyledDomRenderDomError_ok(AzStyledDom_createFromDom(dom));\n}\n",
        );

        let _ = writeln!(
            s,
            "\nstatic AzResultStringCompileError {sn}_compile_fn(const AzComponentDef* def, const \
             AzCompileTarget* target, const AzComponentDataModel* model, size_t indent) {{"
        );
        s.push_str("    (void)def;\n    (void)target;\n    (void)model;\n    (void)indent;\n");
        let _ = writeln!(
            s,
            "    return AzResultStringCompileError_ok(AZ_STR(\"render_{sn}_default()\"));"
        );
        s.push_str("}\n");

        let _ = writeln!(s, "\nstatic AzComponentDef {sn}_def(void) {{");
        s.push_str("    AzComponentDef def;\n");
        let _ = writeln!(
            s,
            "    def.id = AzComponentId_create(AZ_STR({lib_q}), AZ_STR({}));",
            q(def.id.name.as_str())
        );
        let _ = writeln!(
            s,
            "    def.display_name = AZ_STR({});",
            q(def.display_name.as_str())
        );
        let _ = writeln!(
            s,
            "    def.description = AZ_STR({});",
            q(def.description.as_str())
        );
        let _ = writeln!(s, "    /* The CSS is applied per node by render_{sn}. */");
        s.push_str("    def.css = AZ_STR(\"\");\n");
        s.push_str("    def.source = AzComponentSource_UserDefined;\n");
        let _ = writeln!(
            s,
            "    def.data_model.name = AZ_STR({});",
            q(def.data_model.name.as_str())
        );
        let _ = writeln!(
            s,
            "    def.data_model.description = AZ_STR({});",
            q(def.data_model.description.as_str())
        );
        match &p.params {
            Some(params) if !params.is_empty() => {
                let n = params.len();
                let _ = writeln!(s, "    AzComponentDataField fields[{n}];");
                for (i, f) in params.iter().enumerate() {
                    let _ = writeln!(
                        s,
                        "    fields[{i}] = az_string_field({}, {}, {});",
                        q(&f.name),
                        q(&f.default),
                        q(&field_description(def, &f.name))
                    );
                }
                let _ = writeln!(
                    s,
                    "    def.data_model.fields = AzComponentDataFieldVec_copyFromPtr(fields, {n});"
                );
                s.push_str("    /* copyFromPtr cloned them. */\n");
                let _ = writeln!(
                    s,
                    "    for (size_t i = 0; i < {n}; i++) AzComponentDataField_delete(&fields[i]);"
                );
            }
            _ => s.push_str("    def.data_model.fields = AzComponentDataFieldVec_create();\n"),
        }
        let _ = writeln!(s, "    def.render_fn = {sn}_render_fn;");
        let _ = writeln!(s, "    def.compile_fn = {sn}_compile_fn;");
        s.push_str("    def.render_fn_source = AzOptionString_none();\n");
        s.push_str("    def.compile_fn_source = AzOptionString_none();\n");
        s.push_str("    return def;\n}\n");
    }

    let n = parts.len();
    s.push_str("\n/* ── registration ── */\n\n");
    let _ = writeln!(s, "AzComponentLibrary register_{lsn}_library(void) {{");
    let _ = writeln!(s, "    AzComponentDef defs[{n}];");
    for (i, p) in parts.iter().enumerate() {
        let _ = writeln!(s, "    defs[{i}] = {}_def();", p.snake);
    }
    s.push_str("    AzComponentLibrary lib;\n");
    let _ = writeln!(s, "    lib.name = AZ_STR({lib_q});");
    let _ = writeln!(s, "    lib.version = AZ_STR({});", q(lib.version));
    s.push_str("    lib.description = AZ_STR(\"Exported from AzBuilder\");\n");
    let _ = writeln!(
        s,
        "    lib.components = AzComponentDefVec_copyFromPtr(defs, {n});"
    );
    s.push_str("    /* copyFromPtr cloned them. */\n");
    let _ = writeln!(
        s,
        "    for (size_t i = 0; i < {n}; i++) AzComponentDef_delete(&defs[i]);"
    );
    s.push_str("    lib.exportable = true;\n    lib.modifiable = false;\n");
    s.push_str("    lib.data_models = AzComponentDataModelVec_create();\n");
    s.push_str("    lib.enum_models = AzComponentEnumModelVec_create();\n");
    s.push_str("    return lib;\n}\n");
    s
}

// ── Python ──

fn python_library(lib: &LibraryInfo<'_>, parts: &[Part<'_>]) -> String {
    let names: Vec<String> = parts.iter().map(|p| qualified(p.def)).collect();
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# Component library `{}` — exported by AzBuilder: {}.",
        lib.name,
        names.join(", ")
    );
    s.push_str(
        "#\n# Each render_* function builds its component's DOM (its parameters default to\n# \
         the values the component was made with). Registering the components for XML\n# needs \
         a ComponentDef with native render callbacks, which the Python binding\n# cannot build \
         yet: register them from the Rust or C export.\n",
    );
    s.push_str("import azul\n");
    for p in parts {
        let _ = write!(
            s,
            "\n# ── {} ──\n# `{}` ({}). {}\n{}",
            qualified(p.def),
            qualified(p.def),
            comment_line(p.def.display_name.as_str()),
            comment_line(&params_doc(p)),
            p.frag.function
        );
    }
    s
}

// ===========================================================================
// compile_fn of template components
// ===========================================================================

/// `compile_fn` of a component made in AzBuilder: its template compiled to a
/// render function for `target` (what the Components view's "compile_fn"
/// shows). Nested components of other user libraries are unknown here (a
/// compile_fn gets no component map) and compile to a visible placeholder;
/// `export_component_code` has the whole map.
#[must_use]
pub fn builder_template_compile_fn(
    def: &ComponentDef,
    target: &CompileTarget,
    data: &ComponentDataModel,
    _indent: usize,
) -> ResultStringCompileError {
    let map = ComponentMap::with_builtin();
    let compiled = (|| -> Result<String, String> {
        let (xml, css, is_template) = builder::component_export_xml(def, &map)?;
        let nodes = parse_xml(&xml)?;
        let params = is_template.then(|| template_params(data));
        let f = compile_xml_fragment(
            &nodes,
            &css,
            target,
            &format!("render_{}", snake(def.id.name.as_str())),
            params.as_deref(),
        )
        .map_err(|e| format!("codegen: {e}"))?;
        Ok(f.source())
    })();
    let text = match compiled {
        Ok(src) => src,
        Err(e) => {
            let cmt = if matches!(target, CompileTarget::Python) {
                "#"
            } else {
                "//"
            };
            format!("{cmt} cannot compile {}: {}\n", qualified(def), comment_line(&e))
        }
    };
    ResultStringCompileError::Ok(AzString::from(text))
}

// ===========================================================================
// Export > Code: the project
// ===========================================================================

/// The app entry file for `language` from the builder document: the whole
/// document (`<body>`) as a runnable program.
///
/// # Errors
/// Unknown language.
pub fn document_app(
    doc: &BuilderDocument,
    map: &ComponentMap,
    language: &str,
) -> Result<CodeFile, String> {
    let c = subtree_code(doc, map, ROOT_UID, language, SubtreeMode::App, None)?;
    let path = if c.language == "rust" {
        "src/main.rs".to_string()
    } else {
        c.file_name
    };
    Ok(CodeFile {
        path,
        contents: c.code,
    })
}

/// Everything Export > Code writes for `language`: the app (`app`, from
/// [`document_app`] or the live-page export), the build file, every
/// exportable component library (or only `library_filter`) as its own file,
/// and a README with the build commands.
///
/// # Errors
/// Unknown language. (A component library that does not compile is left out
/// with a warning.)
pub fn project_files(
    language: &str,
    app: CodeFile,
    map: &ComponentMap,
    library_filter: Option<&str>,
    warnings: &mut Vec<String>,
) -> Result<Vec<CodeFile>, String> {
    let (_, lang) = dom_language(language)?;
    let mut files = Vec::new();
    let mut component_files = Vec::new();
    for lib in map.get_exportable_libraries() {
        if library_filter.is_some_and(|f| f != lib.name.as_str()) {
            continue;
        }
        let defs: Vec<&ComponentDef> = lib.components.iter().collect();
        if defs.is_empty() {
            continue;
        }
        // One library that does not compile must not cost the user the app.
        match library_code(map, lib.name.as_str(), &defs, lang.id) {
            Ok(code) => {
                warnings.extend(code.warnings.iter().cloned());
                component_files.push((snake(lib.name.as_str()), code.code));
            }
            Err(e) => warnings.push(format!(
                "component library '{}' left out: {e}",
                lib.name.as_str()
            )),
        }
    }

    let mut app = app;
    match lang.id {
        "rust" => {
            if !component_files.is_empty() {
                app.contents.push_str("\n#[allow(dead_code)]\nmod components;\n");
                let mut m = String::new();
                for (sn, code) in &component_files {
                    let _ = writeln!(m, "pub mod {sn};");
                    files.push(CodeFile {
                        path: format!("src/components/{sn}.rs"),
                        contents: code.clone(),
                    });
                }
                files.push(CodeFile {
                    path: "src/components/mod.rs".to_string(),
                    contents: m,
                });
            }
            files.push(CodeFile {
                path: "Cargo.toml".to_string(),
                contents: CARGO_TOML.to_string(),
            });
        }
        _ => {
            for (sn, code) in &component_files {
                files.push(CodeFile {
                    path: format!("components/{sn}.{}", lang.ext),
                    contents: code.clone(),
                });
            }
        }
    }
    let component_paths: Vec<String> = files
        .iter()
        .filter(|f| f.path.contains("components/") && !f.path.ends_with("mod.rs"))
        .map(|f| f.path.clone())
        .collect();
    files.push(CodeFile {
        path: "README.md".to_string(),
        contents: readme(&lang, &component_paths),
    });
    files.insert(0, app);
    Ok(files)
}

const CARGO_TOML: &str = "[package]
name = \"azul-app\"
version = \"0.1.0\"
edition = \"2021\"

[dependencies]
# The azul crate, linked against the prebuilt libazul (see README.md).
# To build against a local checkout instead:
#   azul = { path = \"<azul>/dll\", package = \"azul-dll\", default-features = false, features = [\"link-dynamic\"] }
azul = { git = \"https://github.com/fschutt/azul\", package = \"azul-dll\", default-features = false, features = [\"link-dynamic\"] }
";

fn readme(lang: &DomLanguage, component_paths: &[String]) -> String {
    let comps = component_paths.join(" ");
    let build = match lang.id {
        "rust" => "cargo run --release\n# link-dynamic needs libazul at link and run time:\n# \
                   AZ_LINK_PATH=<azul>/target/release cargo run --release"
            .to_string(),
        "c" => format!(
            "cc main.c {comps} -I <azul>/target/codegen -L <azul>/target/release -lazul -o \
             app\n./app"
        ),
        "cpp" => format!(
            "c++ -std=c++20 main.cpp {comps} -I <azul>/target/codegen -L \
             <azul>/target/release -lazul -o app\n./app"
        ),
        _ => "python3 main.py   # needs the azul Python module on PYTHONPATH".to_string(),
    };
    let comps_md = if component_paths.is_empty() {
        String::new()
    } else {
        format!(
            "\nThe component libraries are in {}: each `render_*` function builds one \
             component, and `register_<library>_library` registers them for XML / AzBuilder \
             (Rust, C, C++).\n",
            component_paths
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "# AzBuilder export ({})\n\n`{}` is the app: it opens a window with the UI you built.\n{}\
         \n## Build and run\n\n```sh\n{}\n```\n\n`<azul>` is an azul checkout with the bindings \
         generated (`cargo run --release -p azul-doc -- codegen all`) and libazul built (`cargo \
         build --release -p azul-dll --features build-dll`).\n",
        lang.label,
        match lang.id {
            "rust" => "src/main.rs",
            "c" => "main.c",
            "cpp" => "main.cpp",
            _ => "main.py",
        },
        comps_md,
        build
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
    fn every_alias_of_a_dom_language_names_one_target() {
        for (alias, id) in [
            ("rust", "rust"),
            ("RS", "rust"),
            ("c", "c"),
            ("c++", "cpp"),
            ("cpp", "cpp"),
            ("py", "python"),
            (" Python ", "python"),
        ] {
            assert_eq!(dom_language(alias).map(|(_, l)| l.id), Ok(id), "{alias}");
        }
        assert!(dom_language("cobol").is_err());
    }

    #[test]
    fn a_template_components_css_lands_on_its_nodes_and_its_text_is_a_parameter() {
        let map = badge_map();
        let rust = component_code(&map, "user", "badge", "rust").expect("rust").code;
        assert!(rust.contains("pub fn render_badge(text: &str) -> Dom {"), "{rust}");
        assert!(rust.contains("Dom::create_span_with_text(text)"), "{rust}");
        assert!(rust.contains(".with_css(\"margin-top: 2px;\")"), "{rust}");
        assert!(rust.contains(".with_class(\"badge\")"), "{rust}");
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
    fn the_compile_fn_of_a_template_component_is_the_template_as_a_function() {
        let map = badge_map();
        let def = map.get("user", "badge").expect("badge");
        match (def.compile_fn)(def, &CompileTarget::C, &def.data_model, 0) {
            ResultStringCompileError::Ok(s) => {
                assert!(
                    s.as_str().contains("AzDom render_badge(const char* text) {"),
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
    }
}
