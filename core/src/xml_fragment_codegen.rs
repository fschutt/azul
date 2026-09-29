//! Compile a DOM FRAGMENT — a subtree of the builder's document, a component's
//! template — to a render FUNCTION in Rust, C, C++ or Python: the engine
//! behind AzBuilder's "Subtree → code" and "Component → code"
//! (`layout/src/e2e/export.rs`).
//!
//! The page walkers in `xml.rs` (`str_to_*_code`) turn a whole `<html>` page
//! into an app, each with its own string plumbing. This module lowers the XML
//! ONCE into a small IR — a constructor, inline CSS, id / classes, children —
//! and prints it with one printer per language, so the four outputs cannot
//! drift apart structurally. It reuses the page walkers' decisions: the
//! semantic / accessibility-aware constructor (`analyze_node_ctor`), the
//! zero-argument creator whitelist (`safe_container_tag`) and the CSS rule
//! matching (`get_css_blocks`), so a fragment and a page compile an element
//! the same way.
//!
//! **CSS.** Every rule of the stylesheet that matches a node, plus the node's
//! own `style` attribute (last, so it wins like an inline style), becomes that
//! node's `with_css(..)`. Nothing is emitted as a separate stylesheet: the
//! generated function returns a self-contained `Dom`.
//!
//! **Template placeholders.** When the fragment is a TEMPLATE (`params` is
//! `Some`), `{name}` in a text or in an attribute value, where `name` is one
//! of the parameters, becomes that string parameter of the function; `{{` /
//! `}}` are literal braces and an unknown `{x}` stays literal text — the same
//! rules as the builder's template substitution
//! (`layout/src/e2e/builder.rs::substitute`). Plain markup (`params: None`)
//! keeps every brace as text.
//!
//! **Whitespace.** Runs of whitespace in a text collapse to one space (except
//! under `<pre>`), a whitespace-only text is dropped, and an element's first /
//! last text child loses its leading / trailing space — `<p>Hello <b>you</b></p>`
//! keeps the space before `you`.
//!
//! Not emitted yet (documented in the export's report): `tabindex`,
//! `contenteditable`, `data-l10n`, images (`<img>` becomes a `div`, like in
//! the page walkers) and event handlers.

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::fmt::Write;

use azul_css::css::{Css, CssPathSelector, NodeTypeTag};

use super::{
    analyze_node_ctor, c_creator_suffix, camel_to_snake, element_draws_nothing, fmt_f32_lit,
    get_css_blocks, node_inline_css, normalize_casing, safe_container_tag, tag_to_node_type,
    tag_to_node_type_tag, CompileError, CompileTarget, CssMatcher, CtorArg, NodeCtor, XmlNode,
    XmlNodeChild, MAX_XML_NESTING_DEPTH,
};

// ===========================================================================
// Public API
// ===========================================================================

/// A `{name}` placeholder of a template fragment, compiled into a string
/// parameter of the generated render function.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FragmentParam {
    /// The placeholder's name (`text`, `href`, …), as the template writes it.
    pub name: String,
    /// The value the component uses when the parameter is not given. Python
    /// writes it as the keyword default; the other languages document it.
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

/// A DOM fragment compiled to one target language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledFragment {
    /// What the function needs above it in its file: imports / includes and,
    /// for C, the helpers it calls. Ends with a newline.
    pub header: String,
    /// The render function. Ends with a newline.
    pub function: String,
    /// The function's name as emitted (sanitised for the language).
    pub fn_name: String,
    /// Each parameter's identifier in the target language, in `params` order
    /// (a keyword gets a `_` suffix: `type` → `type_`).
    pub param_idents: Vec<String>,
}

impl CompiledFragment {
    /// Header and function as one source file.
    #[must_use]
    pub fn source(&self) -> String {
        format!("{}\n{}", self.header, self.function)
    }
}

/// Compile the XML `root_nodes` — a subtree or a component template, NOT a
/// whole page — into one render function `fn_name` for `target`.
///
/// `stylesheet` is CSS source whose matching rules become each node's
/// `with_css`. `params`: `Some` compiles a TEMPLATE (`{name}` placeholders
/// become the function's string parameters, `{{` / `}}` are literal braces);
/// `None` compiles plain markup (braces are text). Several roots, or a text
/// root, are wrapped in one `div`.
///
/// # Errors
///
/// Currently none: every fragment compiles (the stylesheet is parsed
/// leniently, unknown elements become `div`s). The `Result` is kept for the
/// same contract as `str_to_*_code`.
#[allow(clippy::result_large_err)] // the crate's #[repr(C,u8)] FFI error enum, see str_to_rust_code
pub fn compile_xml_fragment(
    root_nodes: &[XmlNodeChild],
    stylesheet: &str,
    target: &CompileTarget,
    fn_name: &str,
    params: Option<&[FragmentParam]>,
) -> Result<CompiledFragment, CompileError> {
    let root = lower_fragment(root_nodes, stylesheet, params)?;
    let names: Vec<String> = params
        .unwrap_or(&[])
        .iter()
        .map(|p| p.name.clone())
        .collect();
    let idents = param_idents(&names, target);
    let fn_name = ident_for(fn_name, target, "render");
    let (header, function) = {
        let mut p = Printer::new(target, &idents);
        let function = p.function(&root, &fn_name, params.unwrap_or(&[]));
        // After the function: the header lists what the function used.
        (p.header(), function)
    };
    Ok(CompiledFragment {
        header,
        function,
        fn_name,
        param_idents: idents,
    })
}

/// Compile `root_nodes` into a complete program that opens a window showing
/// them: the fragment's render function (`render_ui`) plus a data model, a
/// layout callback and `main`, modelled on `examples/*/hello-world`. A single
/// `<body>` root IS the window's body; anything else is put inside one.
///
/// # Errors
///
/// See [`compile_xml_fragment`].
#[allow(clippy::result_large_err)] // the crate's #[repr(C,u8)] FFI error enum, see str_to_rust_code
pub fn compile_xml_fragment_app(
    root_nodes: &[XmlNodeChild],
    stylesheet: &str,
    target: &CompileTarget,
    title: &str,
) -> Result<String, CompileError> {
    let frag = compile_xml_fragment(root_nodes, stylesheet, target, "render_ui", None)?;
    let is_body = single_element_root(root_nodes)
        .is_some_and(|n| normalize_casing(n.node_type.as_str()) == "body");
    Ok(app_source(target, &frag, is_body, title))
}

// ===========================================================================
// IR
// ===========================================================================

/// One piece of a string in generated code.
#[derive(Debug, Clone, PartialEq)]
enum Seg {
    Lit(String),
    /// Index into the parameter list.
    Param(usize),
}

/// A string in generated code: literal text and parameter holes.
#[derive(Debug, Clone, PartialEq, Default)]
struct Text(Vec<Seg>);

impl Text {
    fn lit(s: &str) -> Self {
        Self(vec![Seg::Lit(s.to_string())])
    }

    fn is_blank(&self) -> bool {
        self.0.iter().all(|s| match s {
            Seg::Lit(l) => l.trim().is_empty(),
            Seg::Param(_) => false,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Arg {
    Str(Text),
    Aria(Text),
    Float(f32),
    OptSome(Text),
    OptNone,
}

#[derive(Debug, Clone, PartialEq)]
enum Ctor {
    /// A zero-argument per-tag creator, by its `NodeType` debug name (`"Div"`).
    Tag(&'static str),
    /// A semantic constructor: CamelCase suffix (`PWithText`, `ButtonNoA11y`)
    /// and its arguments.
    Semantic { suffix: String, args: Vec<Arg> },
}

#[derive(Debug, Clone, PartialEq)]
struct Element {
    ctor: Ctor,
    /// Inline CSS: the matched rules, then the `style` attribute.
    css: String,
    ids: Vec<String>,
    classes: Vec<String>,
    children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Element(Element),
    Text(Text),
}

// ===========================================================================
// Lowering: XML → IR
// ===========================================================================

/// Elements that are document plumbing, not content.
const SKIPPED_TAGS: &[&str] = &[
    "style", "script", "head", "title", "meta", "link", "base", "template",
];

struct Lower<'a> {
    css: Css,
    /// Placeholder names; `None` = plain markup.
    params: Option<&'a [FragmentParam]>,
}

#[allow(clippy::result_large_err)]
fn lower_fragment(
    root_nodes: &[XmlNodeChild],
    stylesheet: &str,
    params: Option<&[FragmentParam]>,
) -> Result<Element, CompileError> {
    let mut css = if stylesheet.trim().is_empty() {
        Css::empty()
    } else {
        azul_css::parser2::new_from_str(stylesheet).0
    };
    css.sort_by_specificity();
    let lower = Lower { css, params };

    // The fragment sits where it would in a page: inside `<body>`, so a rule
    // like `body .card` or `.card > p` matches the way it does when mounted.
    let base = CssMatcher {
        path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
        indices_in_parent: vec![0],
        children_length: vec![1],
    };

    if let Some(root) = single_element_root(root_nodes) {
        let m = child_matcher(&base, 0, 1);
        if let Some(Node::Element(e)) = lower.element(root, m, 0, false) {
            return Ok(e);
        }
    }
    // Several roots, a text root, or nothing: one `div` around them.
    let children = lower.children(root_nodes, &base, 0, false);
    Ok(Element {
        ctor: Ctor::Tag("Div"),
        css: String::new(),
        ids: Vec::new(),
        classes: Vec::new(),
        children,
    })
}

/// The only element among `roots` when there is exactly one and no
/// non-blank text beside it.
fn single_element_root(roots: &[XmlNodeChild]) -> Option<&XmlNode> {
    let mut found = None;
    for c in roots {
        match c {
            XmlNodeChild::Element(n) => {
                let tag = normalize_casing(n.node_type.as_str());
                if SKIPPED_TAGS.contains(&tag.as_str())
                    || element_draws_nothing(n.node_type.as_str(), &tag)
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

impl Lower<'_> {
    /// A text or attribute value as a [`Text`]: placeholders only in a template.
    fn text(&self, raw: &str) -> Text {
        match self.params {
            None => Text::lit(raw),
            Some(params) => split_template(raw, params),
        }
    }

    fn arg(&self, a: &CtorArg) -> Arg {
        match a {
            CtorArg::Str(s) => Arg::Str(self.text(s)),
            CtorArg::Aria(s) => Arg::Aria(self.text(s)),
            CtorArg::Float(f) => Arg::Float(*f),
            CtorArg::OptSome(s) => Arg::OptSome(self.text(s)),
            CtorArg::OptNone => Arg::OptNone,
        }
    }

    fn element(
        &self,
        node: &XmlNode,
        mut matcher: CssMatcher,
        depth: usize,
        in_pre: bool,
    ) -> Option<Node> {
        let raw_tag = node.node_type.as_str();
        let tag = normalize_casing(raw_tag);
        if SKIPPED_TAGS.contains(&tag.as_str()) || element_draws_nothing(raw_tag, &tag) {
            return None;
        }

        let analysed = analyze_node_ctor(&tag, node);
        let (ctor, consumes_text, skip_caption) = match &analysed {
            NodeCtor::Plain => {
                let dbg = format!("{:?}", tag_to_node_type(&tag));
                (Ctor::Tag(safe_container_tag(&dbg)), false, false)
            }
            NodeCtor::Semantic {
                suffix,
                args,
                consumes_text,
                skip_caption,
            } => (
                Ctor::Semantic {
                    suffix: suffix.clone(),
                    args: args.iter().map(|a| self.arg(a)).collect(),
                },
                *consumes_text,
                *skip_caption,
            ),
        };

        let ids = split_words(node.attributes.get_key("id"));
        let classes = split_words(node.attributes.get_key("class"));
        matcher
            .path
            .push(CssPathSelector::Type(tag_to_node_type_tag(&tag)));
        matcher
            .path
            .extend(ids.iter().map(|i| CssPathSelector::Id(i.clone().into())));
        matcher
            .path
            .extend(classes.iter().map(|c| CssPathSelector::Class(c.clone().into())));

        let css = node_inline_css(&get_css_blocks(&self.css, &matcher), node);

        let mut children = Vec::new();
        if depth < MAX_XML_NESTING_DEPTH {
            let in_pre = in_pre || tag == "pre";
            let kids = node.children.as_ref();
            let mut caption_skipped = false;
            let visible: Vec<&XmlNodeChild> = kids
                .iter()
                .filter(|c| match c {
                    XmlNodeChild::Text(_) => !consumes_text,
                    XmlNodeChild::Element(e) => {
                        if skip_caption
                            && !caption_skipped
                            && e.node_type.as_str().eq_ignore_ascii_case("caption")
                        {
                            caption_skipped = true;
                            false
                        } else {
                            true
                        }
                    }
                })
                .collect();
            children = self.lower_children(&visible, &matcher, depth + 1, in_pre);
        }

        Some(Node::Element(Element {
            ctor,
            css,
            ids,
            classes,
            children,
        }))
    }

    fn children(
        &self,
        kids: &[XmlNodeChild],
        parent: &CssMatcher,
        depth: usize,
        in_pre: bool,
    ) -> Vec<Node> {
        let visible: Vec<&XmlNodeChild> = kids.iter().collect();
        self.lower_children(&visible, parent, depth, in_pre)
    }

    /// `kids`: the children to emit. Structural selectors (`:first-child`,
    /// `:nth-child`) count ELEMENTS, as in CSS, so the matcher indexes the
    /// element children only.
    fn lower_children(
        &self,
        kids: &[&XmlNodeChild],
        parent: &CssMatcher,
        depth: usize,
        in_pre: bool,
    ) -> Vec<Node> {
        let last = kids.len().saturating_sub(1);
        let elements = kids
            .iter()
            .filter(|c| matches!(c, XmlNodeChild::Element(_)))
            .count();
        let mut element_idx = 0;
        let mut out = Vec::new();
        for (i, c) in kids.iter().enumerate() {
            match c {
                XmlNodeChild::Element(e) => {
                    let m = child_matcher(parent, element_idx, elements);
                    element_idx += 1;
                    if let Some(n) = self.element(e, m, depth, in_pre) {
                        out.push(n);
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
                    if !text.is_blank() {
                        out.push(Node::Text(text));
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

/// `{name}` → the parameter, `{{` / `}}` → a literal brace, anything else
/// literal (the builder's `substitute` rules).
fn split_template(raw: &str, params: &[FragmentParam]) -> Text {
    let mut segs = Vec::new();
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
                if let Some(idx) = params.iter().position(|p| p.name == name) {
                    if !lit.is_empty() {
                        segs.push(Seg::Lit(core::mem::take(&mut lit)));
                    }
                    segs.push(Seg::Param(idx));
                    rest = &tail[end + 1..];
                    continue;
                }
            }
        }
        lit.push_str(&tail[..1]);
        rest = &tail[1..];
    }
    lit.push_str(rest);
    if !lit.is_empty() || segs.is_empty() {
        segs.push(Seg::Lit(lit));
    }
    Text(segs)
}

// ===========================================================================
// Identifiers
// ===========================================================================

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod",
    "move", "mut", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true",
    "try", "type", "unsafe", "use", "where", "while", "yield", "abstract", "become", "box", "do",
    "final", "macro", "override", "priv", "typeof", "unsized", "virtual",
];

const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
    "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long", "register",
    "restrict", "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef",
    "union", "unsigned", "void", "volatile", "while", "bool", "true", "false", "main", "data",
    "info",
];

const CPP_KEYWORDS: &[&str] = &[
    "alignas", "alignof", "and", "asm", "catch", "class", "concept", "constexpr", "consteval",
    "constinit", "decltype", "delete", "explicit", "export", "friend", "mutable", "namespace",
    "new", "noexcept", "not", "nullptr", "operator", "or", "private", "protected", "public",
    "requires", "template", "this", "throw", "try", "typeid", "typename", "using", "virtual",
    "xor", "azul", "std",
];

const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
    "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
    "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try",
    "while", "with", "yield", "azul", "data", "info",
];

fn is_keyword(s: &str, target: &CompileTarget) -> bool {
    match target {
        CompileTarget::Rust => RUST_KEYWORDS.contains(&s),
        CompileTarget::C => C_KEYWORDS.contains(&s),
        CompileTarget::Cpp => C_KEYWORDS.contains(&s) || CPP_KEYWORDS.contains(&s),
        CompileTarget::Python => PYTHON_KEYWORDS.contains(&s),
    }
}

/// `name` as an identifier of `target`: `[a-z0-9_]`, not starting with a
/// digit, not a keyword (a keyword gets a `_` suffix), never empty.
fn ident_for(name: &str, target: &CompileTarget, fallback: &str) -> String {
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
    if s.is_empty() || s.chars().all(|c| c == '_') {
        s = fallback.to_string();
    }
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert_str(0, "p_");
    }
    if is_keyword(&s, target) {
        s.push('_');
    }
    s
}

/// One identifier per parameter, unique, and never `nN` (the C printer's
/// locals).
fn param_idents(names: &[String], target: &CompileTarget) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(names.len());
    for n in names {
        let mut id = ident_for(n, target, "arg");
        let is_local = id.len() > 1
            && id.starts_with('n')
            && id[1..].chars().all(|c| c.is_ascii_digit());
        if is_local {
            id.push('_');
        }
        while out.contains(&id) {
            id.push('_');
        }
        out.push(id);
    }
    out
}

// ===========================================================================
// String literals
// ===========================================================================

/// Escape for a double-quoted Rust / C / C++ / Python string literal.
fn esc(s: &str, target: &CompileTarget) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                let _ = match target {
                    CompileTarget::Rust => write!(out, "\\u{{{:x}}}", c as u32),
                    // Octal: a hex escape would swallow a following hex digit.
                    CompileTarget::C | CompileTarget::Cpp => write!(out, "\\{:03o}", c as u32),
                    CompileTarget::Python => write!(out, "\\x{:02x}", c as u32),
                };
            }
            c => out.push(c),
        }
    }
    out
}

/// Double the braces of a literal inside a Rust `format!` / Python f-string.
fn brace_escape(s: &str) -> String {
    s.replace('{', "{{").replace('}', "}}")
}

// ===========================================================================
// Printing
// ===========================================================================

const IND: &str = "    ";

fn ind(depth: usize) -> String {
    IND.repeat(depth)
}

struct Printer<'a> {
    target: &'a CompileTarget,
    idents: &'a [String],
    used: Vec<bool>,
    uses_aria: bool,
    uses_concat: bool,
    /// C: the next local variable number.
    next_var: usize,
}

impl<'a> Printer<'a> {
    fn new(target: &'a CompileTarget, idents: &'a [String]) -> Self {
        Self {
            target,
            idents,
            used: vec![false; idents.len()],
            uses_aria: false,
            uses_concat: false,
            next_var: 0,
        }
    }

    // ── strings ──

    /// A string VALUE of the target's string-argument type: Rust `&str` /
    /// `String` (every builder argument is `Into<AzString>`), C `AzString`,
    /// C++ `azul::String`, Python `str`.
    fn str_expr(&mut self, t: &Text) -> String {
        let segs: &[Seg] = &t.0;
        for s in segs {
            if let Seg::Param(i) = s {
                if let Some(u) = self.used.get_mut(*i) {
                    *u = true;
                }
            }
        }
        let all_lit: Option<String> = segs.iter().try_fold(String::new(), |mut acc, s| {
            if let Seg::Lit(l) = s {
                acc.push_str(l);
                Some(acc)
            } else {
                None
            }
        });
        let single_param = match segs {
            [Seg::Param(i)] => Some(self.idents[*i].clone()),
            _ => None,
        };
        match self.target {
            CompileTarget::Rust => {
                if let Some(l) = all_lit {
                    format!("\"{}\"", esc(&l, self.target))
                } else if let Some(p) = single_param {
                    p
                } else {
                    let mut f = String::from("format!(\"");
                    for s in segs {
                        match s {
                            Seg::Lit(l) => f.push_str(&brace_escape(&esc(l, self.target))),
                            Seg::Param(i) => {
                                let _ = write!(f, "{{{}}}", self.idents[*i]);
                            }
                        }
                    }
                    f.push_str("\")");
                    f
                }
            }
            CompileTarget::C => {
                if let Some(l) = all_lit {
                    format!("AZ_STR(\"{}\")", esc(&l, self.target))
                } else if let Some(p) = single_param {
                    format!("AZ_STR({p})")
                } else {
                    self.uses_concat = true;
                    let mut parts: Vec<String> = segs
                        .iter()
                        .map(|s| match s {
                            Seg::Lit(l) => format!("\"{}\"", esc(l, self.target)),
                            Seg::Param(i) => self.idents[*i].clone(),
                        })
                        .collect();
                    parts.push("(const char*)NULL".to_string());
                    format!("az_concat({})", parts.join(", "))
                }
            }
            CompileTarget::Cpp => {
                if let Some(l) = all_lit {
                    format!("String(\"{}\")", esc(&l, self.target))
                } else if let Some(p) = single_param {
                    format!("String({p})")
                } else {
                    let parts: Vec<String> = segs
                        .iter()
                        .map(|s| match s {
                            Seg::Lit(l) => format!("std::string(\"{}\")", esc(l, self.target)),
                            Seg::Param(i) => self.idents[*i].clone(),
                        })
                        .collect();
                    format!("String({})", parts.join(" + "))
                }
            }
            CompileTarget::Python => {
                if let Some(l) = all_lit {
                    format!("\"{}\"", esc(&l, self.target))
                } else if let Some(p) = single_param {
                    p
                } else {
                    let mut f = String::from("f\"");
                    for s in segs {
                        match s {
                            Seg::Lit(l) => f.push_str(&brace_escape(&esc(l, self.target))),
                            Seg::Param(i) => {
                                let _ = write!(f, "{{{}}}", self.idents[*i]);
                            }
                        }
                    }
                    f.push('"');
                    f
                }
            }
        }
    }

    /// A literal string VALUE (CSS, id, class).
    fn lit_expr(&mut self, s: &str) -> String {
        self.str_expr(&Text::lit(s))
    }

    fn arg_expr(&mut self, a: &Arg) -> String {
        match a {
            Arg::Str(t) => self.str_expr(t),
            Arg::Aria(t) => {
                self.uses_aria = true;
                let s = self.str_expr(t);
                match self.target {
                    CompileTarget::Rust | CompileTarget::Cpp => format!("SmallAriaInfo::label({s})"),
                    CompileTarget::C => format!("AzSmallAriaInfo_label({s})"),
                    CompileTarget::Python => format!("azul.SmallAriaInfo.label({s})"),
                }
            }
            Arg::Float(f) => match self.target {
                CompileTarget::C | CompileTarget::Cpp => format!("{}f", fmt_f32_lit(*f)),
                CompileTarget::Rust | CompileTarget::Python => fmt_f32_lit(*f),
            },
            // Python has no way to build an `OptionString` (the binding only
            // offers `OptionString.None()`, which is not even valid syntax);
            // `ctor_expr` routes the one constructor that takes one (`<a>`)
            // around it, so these two Python arms are a fallback only.
            Arg::OptSome(t) => {
                let s = self.str_expr(t);
                match self.target {
                    CompileTarget::Rust | CompileTarget::Cpp => format!("OptionString::some({s})"),
                    CompileTarget::C => format!("AzOptionString_some({s})"),
                    CompileTarget::Python => s,
                }
            }
            Arg::OptNone => match self.target {
                CompileTarget::Rust | CompileTarget::Cpp => "OptionString::none()".to_string(),
                CompileTarget::C => "AzOptionString_none()".to_string(),
                CompileTarget::Python => "\"\"".to_string(),
            },
        }
    }

    fn ctor_expr(&mut self, c: &Ctor) -> String {
        // Python: `Dom.create_a_no_a11y(href, label: OptionString)` cannot be
        // called (no `OptionString` constructor in the binding), so a link is
        // `Dom.create_a(href, text, SmallAriaInfo.label(text))` — its text IS
        // its accessible name, as a screen reader would announce it anyway.
        if matches!(self.target, CompileTarget::Python) {
            if let Ctor::Semantic { suffix, args } = c {
                if suffix == "ANoA11y" {
                    let href = match args.first() {
                        Some(Arg::Str(t)) => t.clone(),
                        _ => Text::lit(""),
                    };
                    let text = match args.get(1) {
                        Some(Arg::OptSome(t)) => t.clone(),
                        _ => href.clone(),
                    };
                    self.uses_aria = true;
                    let h = self.str_expr(&href);
                    let t = self.str_expr(&text);
                    return format!("azul.Dom.create_a({h}, {t}, azul.SmallAriaInfo.label({t}))");
                }
            }
        }
        let (name, args): (String, Vec<String>) = match c {
            Ctor::Tag(t) => (t.to_string(), Vec::new()),
            Ctor::Semantic { suffix, args } => (
                suffix.clone(),
                args.iter().map(|a| self.arg_expr(a)).collect(),
            ),
        };
        let args = args.join(", ");
        match self.target {
            // `AzDom_createBlockquote`: first letter kept, the rest as the
            // C API spells it (`c_creator_suffix` for tags; semantic
            // suffixes are already the C spelling).
            CompileTarget::C => match c {
                Ctor::Tag(t) => format!("AzDom_create{}()", c_creator_suffix(t)),
                Ctor::Semantic { .. } => format!("AzDom_create{name}({args})"),
            },
            CompileTarget::Rust | CompileTarget::Cpp => match c {
                Ctor::Tag(t) => format!("Dom::create_{}()", t.to_lowercase()),
                Ctor::Semantic { .. } => format!("Dom::create_{}({args})", camel_to_snake(&name)),
            },
            CompileTarget::Python => match c {
                Ctor::Tag(t) => format!("azul.Dom.create_{}()", t.to_lowercase()),
                Ctor::Semantic { .. } => {
                    format!("azul.Dom.create_{}({args})", camel_to_snake(&name))
                }
            },
        }
    }

    fn text_node_expr(&mut self, t: &Text) -> String {
        let s = self.str_expr(t);
        match self.target {
            CompileTarget::Rust | CompileTarget::Cpp => {
                format!("Dom::create_text_do_not_use_without_block_level_wrapper({s})")
            }
            CompileTarget::C => format!("AzDom_createTextDoNotUseWithoutBlockLevelWrapper({s})"),
            CompileTarget::Python => {
                format!("azul.Dom.create_text_do_not_use_without_block_level_wrapper({s})")
            }
        }
    }

    // ── fluent expressions (Rust, C++, Python) ──

    /// The expression building `node`. Its first line has no indentation; its
    /// continuation lines are indented for an expression that starts at
    /// nesting `depth`.
    fn fluent(&mut self, node: &Node, depth: usize) -> String {
        let e = match node {
            Node::Text(t) => return self.text_node_expr(t),
            Node::Element(e) => e,
        };
        let mut s = self.ctor_expr(&e.ctor);
        let chain = ind(depth + 1);
        if !e.css.is_empty() {
            let v = self.lit_expr(&e.css);
            let _ = write!(s, "\n{chain}.with_css({v})");
        }
        for id in &e.ids {
            let v = self.lit_expr(id);
            let _ = write!(s, "\n{chain}.with_id({v})");
        }
        for class in &e.classes {
            let v = self.lit_expr(class);
            let _ = write!(s, "\n{chain}.with_class({v})");
        }
        let rust_vec = matches!(self.target, CompileTarget::Rust) && e.children.len() > 1;
        if rust_vec {
            let item = ind(depth + 2);
            let _ = write!(s, "\n{chain}.with_children(vec![");
            for c in &e.children {
                let cs = self.fluent(c, depth + 2);
                let _ = write!(s, "\n{item}{cs},");
            }
            let _ = write!(s, "\n{chain}])");
        } else {
            for c in &e.children {
                let cs = self.fluent(c, depth + 1);
                let _ = write!(s, "\n{chain}.with_child({cs})");
            }
        }
        s
    }

    // ── imperative C ──

    /// Emit the statements building `node` into `out`; returns the variable.
    fn c_statements(&mut self, e: &Element, out: &mut String) -> String {
        let var = format!("n{}", self.next_var);
        self.next_var += 1;
        let ctor = self.ctor_expr(&e.ctor);
        let _ = writeln!(out, "{IND}AzDom {var} = {ctor};");
        if !e.css.is_empty() {
            let v = self.lit_expr(&e.css);
            let _ = writeln!(out, "{IND}{var} = AzDom_withCss({var}, {v});");
        }
        for id in &e.ids {
            let v = self.lit_expr(id);
            let _ = writeln!(out, "{IND}{var} = AzDom_withId({var}, {v});");
        }
        for class in &e.classes {
            let v = self.lit_expr(class);
            let _ = writeln!(out, "{IND}{var} = AzDom_withClass({var}, {v});");
        }
        for c in &e.children {
            match c {
                Node::Element(ce) => {
                    let cv = self.c_statements(ce, out);
                    let _ = writeln!(out, "{IND}AzDom_addChild(&{var}, {cv});");
                }
                Node::Text(t) => {
                    let tn = self.text_node_expr(t);
                    let _ = writeln!(out, "{IND}AzDom_addChild(&{var}, {tn});");
                }
            }
        }
        var
    }

    // ── the function ──

    fn unused_params(&self) -> Vec<String> {
        self.idents
            .iter()
            .zip(self.used.iter())
            .filter(|(_, u)| !**u)
            .map(|(i, _)| i.clone())
            .collect()
    }

    fn function(&mut self, root: &Element, fn_name: &str, params: &[FragmentParam]) -> String {
        let node = Node::Element(root.clone());
        let mut out = String::new();
        match self.target {
            CompileTarget::Rust => {
                let expr = self.fluent(&node, 1);
                let sig: Vec<String> = self.idents.iter().map(|i| format!("{i}: &str")).collect();
                let _ = writeln!(out, "pub fn {fn_name}({}) -> Dom {{", sig.join(", "));
                for u in self.unused_params() {
                    let _ = writeln!(out, "{IND}let _ = {u};");
                }
                let _ = writeln!(out, "{IND}{expr}");
                out.push_str("}\n");
            }
            CompileTarget::C => {
                let mut body = String::new();
                let var = self.c_statements(root, &mut body);
                let sig = if self.idents.is_empty() {
                    "void".to_string()
                } else {
                    self.idents
                        .iter()
                        .map(|i| format!("const char* {i}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let _ = writeln!(out, "AzDom {fn_name}({sig}) {{");
                for u in self.unused_params() {
                    let _ = writeln!(out, "{IND}(void){u};");
                }
                out.push_str(&body);
                let _ = writeln!(out, "{IND}return {var};");
                out.push_str("}\n");
            }
            CompileTarget::Cpp => {
                let expr = self.fluent(&node, 1);
                let sig: Vec<String> = self
                    .idents
                    .iter()
                    .map(|i| format!("const std::string& {i}"))
                    .collect();
                let _ = writeln!(out, "Dom {fn_name}({}) {{", sig.join(", "));
                for u in self.unused_params() {
                    let _ = writeln!(out, "{IND}(void){u};");
                }
                let _ = writeln!(out, "{IND}return {expr};");
                out.push_str("}\n");
            }
            CompileTarget::Python => {
                let expr = self.fluent(&node, 2);
                let sig: Vec<String> = self
                    .idents
                    .iter()
                    .zip(params.iter())
                    .map(|(i, p)| format!("{i}=\"{}\"", esc(&p.default, self.target)))
                    .collect();
                let _ = writeln!(out, "def {fn_name}({}):", sig.join(", "));
                let _ = writeln!(out, "{IND}return (");
                let _ = writeln!(out, "{IND}{IND}{expr}");
                let _ = writeln!(out, "{IND})");
            }
        }
        out
    }

    fn header(&self) -> String {
        match self.target {
            CompileTarget::Rust => {
                let mut h = String::from("use azul::prelude::*;\n");
                if self.uses_aria {
                    h.push_str("use azul::dom::SmallAriaInfo;\n");
                }
                h
            }
            CompileTarget::C => {
                let mut h = String::from("#include \"azul.h\"\n#include <string.h>\n");
                if self.uses_concat {
                    h.push_str(C_CONCAT_HELPER);
                }
                h
            }
            CompileTarget::Cpp => {
                "#include \"azul20.hpp\"\n#include <string>\n\nusing namespace azul;\n".to_string()
            }
            CompileTarget::Python => "import azul\n".to_string(),
        }
    }
}

/// C has no string formatting in the azul API: a text that mixes literal
/// parts and parameters is joined by this helper (emitted only when used).
const C_CONCAT_HELPER: &str = "#include <stdarg.h>
#include <stdlib.h>

/* Joins NUL-terminated strings (the last argument is NULL) into an AzString. */
static AzString az_concat(const char* first, ...) {
    size_t len = 0;
    va_list ap;
    va_start(ap, first);
    for (const char* s = first; s != NULL; s = va_arg(ap, const char*)) len += strlen(s);
    va_end(ap);
    char* buf = (char*)malloc(len + 1);
    size_t at = 0;
    va_start(ap, first);
    for (const char* s = first; s != NULL; s = va_arg(ap, const char*)) {
        size_t n = strlen(s);
        memcpy(buf + at, s, n);
        at += n;
    }
    va_end(ap);
    AzString out = AzString_copyFromBytes((const uint8_t*)buf, 0, len);
    free(buf);
    return out;
}
";

// ===========================================================================
// Apps
// ===========================================================================

fn app_source(target: &CompileTarget, frag: &CompiledFragment, is_body: bool, title: &str) -> String {
    let title = esc(title, target);
    let f = &frag.fn_name;
    match target {
        CompileTarget::Rust => {
            let body = if is_body {
                format!("{f}()")
            } else {
                format!("Dom::create_body().with_child({f}())")
            };
            format!(
                "//! {title} — generated by AzBuilder.\n//! Build and run: `cargo run --release` \
                 (see Cargo.toml).\n\n{header}\n{function}\nstruct AppData {{}}\n\nextern \"C\" \
                 fn layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {{\n{IND}{body}\n}}\n\n\
                 fn main() {{\n{IND}let app = App::create(RefAny::new(AppData {{}}), \
                 AppConfig::create());\n{IND}let mut window = \
                 WindowCreateOptions::create(layout);\n{IND}window.window_state.title = \
                 \"{title}\".into();\n{IND}app.run(window);\n}}\n",
                header = frag.header,
                function = frag.function,
            )
        }
        CompileTarget::C => {
            let body = if is_body {
                format!("{IND}return {f}();\n")
            } else {
                format!(
                    "{IND}AzDom body = AzDom_createBody();\n{IND}AzDom_addChild(&body, \
                     {f}());\n{IND}return body;\n"
                )
            };
            format!(
                "/* {title} — generated by AzBuilder.\n * Build: cc main.c -I \
                 <azul>/target/codegen -L <azul>/target/release -lazul -o app\n */\n{header}\n\
                 {function}\ntypedef struct {{ int unused; }} AppData;\nstatic void \
                 AppData_destructor(void* p) {{ (void)p; }}\nAZ_REFLECT(AppData, \
                 AppData_destructor);\n\nAzDom layout(AzRefAny data, AzLayoutCallbackInfo info) \
                 {{\n{IND}(void)data;\n{IND}(void)info;\n{body}}}\n\nint main(void) {{\n{IND}\
                 AppData model = {{ 0 }};\n{IND}AzRefAny data = AppData_upcast(model);\n{IND}\
                 AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);\n{IND}\
                 window.window_state.title = AZ_STR(\"{title}\");\n{IND}AzApp app = \
                 AzApp_create(data, AzAppConfig_create());\n{IND}AzApp_run(&app, window);\n{IND}\
                 AzApp_delete(&app);\n{IND}return 0;\n}}\n",
                header = frag.header,
                function = frag.function,
            )
        }
        CompileTarget::Cpp => {
            let body = if is_body {
                format!("{f}()")
            } else {
                format!("Dom::create_body().with_child({f}())")
            };
            format!(
                "// {title} — generated by AzBuilder.\n// Build: c++ -std=c++20 main.cpp -I \
                 <azul>/target/codegen -L <azul>/target/release -lazul -o app\n{header}\n\
                 {function}\nstruct AppData {{}};\n\nffi::Dom layout(ffi::RefAny data, \
                 ffi::LayoutCallbackInfo info) {{\n{IND}RefAny adopted(data);\n{IND}(void)info;\n\
                 {IND}return {body};\n}}\n\nint main() {{\n{IND}RefAny data = \
                 RefAny::create(AppData{{}});\n{IND}WindowCreateOptions window = \
                 WindowCreateOptions::create(layout);\n{IND}App app = App::create(std::move(data), \
                 AppConfig::create());\n{IND}app.run(std::move(window));\n{IND}return 0;\n}}\n",
                header = frag.header,
                function = frag.function,
            )
        }
        CompileTarget::Python => {
            let body = if is_body {
                format!("{f}()")
            } else {
                format!("azul.Dom.create_body().with_child({f}())")
            };
            // One blank line between top-level definitions, like the Python
            // examples (house style).
            format!(
                "# {title} — generated by AzBuilder.\n# Run: python3 main.py\n{header}\n\
                 {function}\nclass AppData:\n{IND}pass\n\ndef layout(data, info):\n{IND}return \
                 {body}\n\ndef main():\n{IND}app = azul.App.create(AppData(), \
                 azul.AppConfig.create())\n{IND}window = \
                 azul.WindowCreateOptions.create(layout)\n{IND}app.run(window)\n\nif __name__ == \
                 \"__main__\":\n{IND}main()\n",
                header = frag.header,
                function = frag.function,
            )
        }
    }
}
