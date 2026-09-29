//! Lower a DOM FRAGMENT — a subtree of the builder's document, a component's
//! template, a page body — to the language-neutral codegen IR
//! ([`azul_css::codegen::ir`]), which every binding language's printer
//! ([`azul_css::codegen::lang`]) turns into source: AzBuilder's
//! "Subtree → code", "Component → code" and Export > Code
//! (`layout/src/e2e/export.rs`).
//!
//! This module only DECIDES; it prints nothing. It keeps the page walkers'
//! decisions (in `xml.rs`), so a fragment and a page compile an element the
//! same way: the semantic / accessibility-aware constructor
//! (`analyze_node_ctor`), the zero-argument creator whitelist
//! (`safe_container_tag`), and the CSS rule matching plus the `style`
//! attribute (`node_inline_css`).
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
//! Not lowered yet (listed in the export's report): `tabindex`,
//! `contenteditable`, `data-l10n`, images (`<img>` becomes a `div`, like in
//! the page walkers) and event handlers.

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use azul_css::{
    codegen::{
        backend_for,
        ir::{AppSpec, Expr, Ident, Item, ItemParam, Module},
        supported_languages, GeneratedFile,
    },
    css::{Css, CssPathSelector, NodeTypeTag},
};

use super::{
    analyze_node_ctor, camel_to_snake, element_draws_nothing, get_css_blocks, get_body_node,
    get_html_node, node_inline_css, normalize_casing, safe_container_tag, tag_to_node_type,
    tag_to_node_type_tag, CompileError, CssMatcher, CtorArg, NodeCtor, XmlNode, XmlNodeChild,
    MAX_XML_NESTING_DEPTH,
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
/// — to one IR item `fn_name` that builds it as a `Dom`.
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
    let css = parse_stylesheet(stylesheet);
    let value = lower_roots(root_nodes, &css, params);
    Module {
        items: vec![Item {
            name: Ident::from_text(fn_name),
            doc,
            ty: "Dom".to_string(),
            params: params
                .unwrap_or(&[])
                .iter()
                .map(|p| ItemParam::string(&p.name, &p.default))
                .collect(),
            value,
        }],
        ..Module::default()
    }
}

/// Lower `root_nodes` to an app ([`Module::app`]): one item `render_ui`,
/// which a printer's `emit_project_files` puts in a window titled `title`.
/// A single `<body>` root IS the window's body; anything else goes inside
/// one.
#[must_use]
pub fn lower_xml_fragment_app(root_nodes: &[XmlNodeChild], stylesheet: &str, title: &str) -> Module {
    let is_body = single_element_root(root_nodes)
        .is_some_and(|n| normalize_casing(n.node_type.as_str()) == "body");
    let mut m = lower_xml_fragment(root_nodes, stylesheet, "render_ui", None, Vec::new());
    m.app = Some(AppSpec {
        title: title.to_string(),
        root: Ident::from_text("render_ui"),
        is_body,
    });
    m
}

/// Lower a whole page (`<html>` with a `<head><style>` and a `<body>`, or a
/// fragment the parser wraps into one) to an app: its body with its
/// stylesheet.
///
/// # Errors
///
/// A document without a body the parser can find.
#[allow(clippy::result_large_err)] // the crate's #[repr(C,u8)] FFI error enum, see str_to_rust_code
pub fn lower_xml_page_app(root_nodes: &[XmlNodeChild], title: &str) -> Result<Module, CompileError> {
    let html = get_html_node(root_nodes)?;
    let body = get_body_node(html.children.as_ref())?.clone();
    let style = super::head_style_text(&html);
    Ok(lower_xml_fragment_app(
        &[XmlNodeChild::Element(body)],
        &style,
        title,
    ))
}

/// [`lower_xml_fragment`] printed by the code generator for `language`
/// (any id or alias of `azul_css::codegen::supported_languages()`).
///
/// # Errors
///
/// An unknown language.
pub fn compile_xml_fragment(
    root_nodes: &[XmlNodeChild],
    stylesheet: &str,
    language: &str,
    fn_name: &str,
    params: Option<&[FragmentParam]>,
) -> Result<String, String> {
    let backend = backend(language)?;
    let m = lower_xml_fragment(root_nodes, stylesheet, fn_name, params, Vec::new());
    Ok(backend.emit_module(&m))
}

/// [`lower_xml_fragment_app`] as a project (build file, the module, a
/// `main` that opens the window) for `language`.
///
/// # Errors
///
/// An unknown language.
pub fn compile_xml_fragment_app(
    root_nodes: &[XmlNodeChild],
    stylesheet: &str,
    language: &str,
    title: &str,
) -> Result<Vec<GeneratedFile>, String> {
    let backend = backend(language)?;
    Ok(backend.emit_project_files(&lower_xml_fragment_app(root_nodes, stylesheet, title)))
}

fn backend(language: &str) -> Result<alloc::boxed::Box<dyn azul_css::codegen::CodegenBackend>, String> {
    backend_for(language).ok_or_else(|| {
        format!(
            "no code generator for {language:?}; available: {}",
            supported_languages()
        )
    })
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

struct Lower<'a> {
    css: &'a Css,
    params: Option<&'a [FragmentParam]>,
}

fn lower_roots(root_nodes: &[XmlNodeChild], css: &Css, params: Option<&[FragmentParam]>) -> Expr {
    let lower = Lower { css, params };
    // The fragment sits where it would in a page: inside `<body>`, so a rule
    // like `body .card` or `.card > p` matches the way it does when mounted.
    let base = CssMatcher {
        path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
        indices_in_parent: vec![0],
        children_length: vec![1],
    };
    if let Some(root) = single_element_root(root_nodes) {
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
    fn text(&self, raw: &str) -> Expr {
        text_expr(raw, self.params)
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

    fn element(
        &self,
        node: &XmlNode,
        mut matcher: CssMatcher,
        depth: usize,
        in_pre: bool,
    ) -> Option<Expr> {
        let raw_tag = node.node_type.as_str();
        let tag = normalize_casing(raw_tag);
        if SKIPPED_TAGS.contains(&tag.as_str()) || element_draws_nothing(raw_tag, &tag) {
            return None;
        }

        let analysed = analyze_node_ctor(&tag, node);
        let (consumes_text, skip_caption) = match &analysed {
            NodeCtor::Plain => (false, false),
            NodeCtor::Semantic {
                consumes_text,
                skip_caption,
                ..
            } => (*consumes_text, *skip_caption),
        };
        let mut e = self.ctor(&tag, &analysed);

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

        let css = node_inline_css(&get_css_blocks(self.css, &matcher), node);
        if !css.is_empty() {
            e = with(e, "with_css", vec![Expr::str(&css)]);
        }
        for id in &ids {
            e = with(e, "with_id", vec![Expr::str(id)]);
        }
        for class in &classes {
            e = with(e, "with_class", vec![Expr::str(class)]);
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
