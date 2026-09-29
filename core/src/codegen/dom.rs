//! Lower MARKUP — a subtree of the builder's document, a component's
//! template, a page, pasted HTML — to the language-neutral codegen IR
//! ([`azul_css::codegen::ir`]), which every binding language's printer
//! ([`azul_css::codegen::lang`]) turns into source: AzBuilder's
//! "Subtree → code", "Component → code", "HTML → DOM (code)" and Export > Code.
//!
//! This module only DECIDES; it prints nothing. The decisions: the semantic /
//! accessibility-aware constructor of an element (`analyze_node_ctor`), the
//! zero-argument creator whitelist (`safe_container_tag`), and the CSS rule
//! matching ([`CssMatcher`]) plus the `style` attribute (`node_inline_css`).
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
//! Not lowered yet: `tabindex`, `contenteditable`, `data-l10n`, images
//! (`<img>` becomes a `div`) and event handlers.

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use azul_css::{
    codegen::ir::{AppSpec, Expr, Ident, Item, ItemParam, Module},
    css::{
        Css, CssDeclaration, CssPath, CssPathPseudoSelector, CssPathSelector, CssRuleBlock,
        NodeTypeTag,
    },
};

use crate::xml::{
    element_draws_nothing, get_body_node, get_html_node, head_style_text, normalize_casing,
    tag_to_node_type, tag_to_node_type_tag, CompileError, XmlNode, XmlNodeChild,
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
#[allow(clippy::result_large_err)] // the crate's #[repr(C,u8)] FFI error enum (xml.rs CompileError)
pub fn lower_xml_page_app(root_nodes: &[XmlNodeChild], title: &str) -> Result<Module, CompileError> {
    let html = get_html_node(root_nodes)?;
    let body = get_body_node(html.children.as_ref())?.clone();
    let style = head_style_text(&html);
    Ok(lower_xml_fragment_app(
        &[XmlNodeChild::Element(body)],
        &style,
        title,
    ))
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
    a: &[&CssPathSelector],
    b: &[&CssPathSelector],
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
                if !b.iter().any(|t| **t == Type(*tag)) {
                    return false;
                }
            }
            Class(class) => {
                if !b.iter().any(|t| **t == Class(class.clone())) {
                    return false;
                }
            }
            Id(id) => {
                if !b.iter().any(|t| **t == Id(id.clone())) {
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
/// `Dom::with_css(...)`. `with_css` parses via `Css::parse_inline`, which runs
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
            .map(|d| {
                let prop = match d {
                    CssDeclaration::Static(s) => s,
                    CssDeclaration::Dynamic(dy) => &dy.default_value,
                };
                format!("{}: {};", prop.key(), prop.value())
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
/// (`css_blocks_to_inline_string`), then its own `style` attribute — last, so
/// it wins like an inline style does. The attribute's whitespace is collapsed
/// to single spaces: a newline inside a C / C++ / Python string literal would
/// not compile.
fn node_inline_css(blocks: &[CssBlock], node: &XmlNode) -> String {
    let mut css = css_blocks_to_inline_string(blocks);
    if let Some(style) = node.attributes.get_key("style") {
        let style = style.as_str().split_whitespace().collect::<Vec<_>>().join(" ");
        if !style.is_empty() {
            if !css.is_empty() {
                css.push(' ');
            }
            css.push_str(&style);
        }
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

#[cfg(test)]
#[path = "dom_test.rs"]
mod dom_test;
