//! The HTML flavour of a paste, sanitized into what a rich-text editor
//! holds: text in four formats (bold, italic, underline, line-through),
//! line breaks, links, and the blocks a mail or a document is made of
//! (paragraphs, headings, quotes, lists, preformatted text).
//!
//! The markup is read by the XML loader's own parser
//! ([`crate::xml::parse_xml_string`]: its void elements, its auto-closing
//! rules, its entity decoding), and nothing of it is used that this module
//! does not name. What is not named goes one of two ways:
//!
//! - DROPPED with its content: everything that runs, loads, styles or asks -
//!   `script style link meta head title iframe frame frameset object embed
//!   applet form input button select textarea option optgroup datalist
//!   template noscript svg math canvas video audio img picture source`. A
//!   form in particular: the XML loader turns one into live widgets, and a
//!   paste must never bring a prefilled password field into a mail.
//! - UNWRAPPED, its content kept: every other element (`span`, `font`,
//!   `section`, `table`, ...). Formatting is read off the element's tag and
//!   its `style` attribute - `font-weight`, `font-style`, `text-decoration`,
//!   how Google Docs and Word spell it - and carried by the text itself.
//!
//! No attribute survives but an `<a>`'s `href`, and only an `http:`,
//! `https:` or `mailto:` one; a link with any other target is its text.
//! White space collapses as HTML's `white-space: normal` does (a rich
//! editing host is `pre-wrap`, where a source's indentation would show),
//! except inside `<pre>`.

use alloc::{string::String, vec::Vec};

use azul_core::{
    dom::Dom,
    xml::{XmlNode, XmlNodeChild},
};
use azul_css::{AzString, OptionString};

use crate::text3::edit::{FormatOverrides, FormatSpan};

/// How deep the markup is walked; below that, content is left out.
const MAX_DEPTH: usize = 64;

/// Elements dropped with everything in them.
const DROPPED: &[&str] = &[
    "script", "style", "link", "meta", "head", "title", "iframe", "frame", "frameset", "object",
    "embed", "applet", "form", "input", "button", "select", "textarea", "option", "optgroup",
    "datalist", "template", "noscript", "svg", "math", "canvas", "video", "audio", "img",
    "picture", "source", "base", "param", "track", "map", "area",
];

/// The four formats a pasted text carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PastedFormats {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

impl PastedFormats {
    /// As overrides of the style the text goes into: the formats the text
    /// has are set, the ones it has not are left as the target has them.
    #[must_use]
    pub fn as_overrides(self) -> FormatOverrides {
        FormatOverrides {
            bold: self.bold.then_some(true),
            italic: self.italic.then_some(true),
            underline: self.underline.then_some(true),
            strikethrough: self.strikethrough.then_some(true),
        }
    }
}

/// A block a paste can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PastedBlockKind {
    Paragraph,
    Division,
    Heading(u8),
    Quote,
    UnorderedList,
    OrderedList,
    ListItem,
    Preformatted,
}

/// One node of a sanitized paste.
#[derive(Debug, Clone, PartialEq)]
pub enum PastedNode {
    Text(String, PastedFormats),
    Break,
    Link(String, Vec<PastedNode>),
    Block(PastedBlockKind, Vec<PastedNode>),
}

/// A paste, sanitized: its top-level nodes in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PastedFragment {
    pub nodes: Vec<PastedNode>,
}

/// Sanitize `html` into a [`PastedFragment`]; `None` when the markup does
/// not parse (the paste then falls back to the plain-text flavour).
#[must_use]
pub fn sanitize_html(html: &str) -> Option<PastedFragment> {
    let roots = parse(html)?;
    let mut nodes = Vec::new();
    convert_children(&roots, PastedFormats::default(), false, &mut nodes, 0);
    tidy(&mut nodes, true);
    Some(PastedFragment { nodes })
}

/// The markup as the XML loader's parser reads it; `None` when it does not
/// parse - or, without the `xml` feature, always (the paste then falls back
/// to the plain text).
#[cfg(feature = "xml")]
fn parse(html: &str) -> Option<Vec<XmlNodeChild>> {
    crate::xml::parse_xml_string(html).ok()
}

#[cfg(not(feature = "xml"))]
fn parse(_html: &str) -> Option<Vec<XmlNodeChild>> {
    None
}

impl PastedFragment {
    /// Nothing to insert.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Whether a link is anywhere in it.
    #[must_use]
    pub fn has_links(&self) -> bool {
        fn any(nodes: &[PastedNode]) -> bool {
            nodes.iter().any(|n| match n {
                PastedNode::Link(..) => true,
                PastedNode::Block(_, children) => any(children),
                _ => false,
            })
        }
        any(&self.nodes)
    }

    /// Whether it holds more than inline content: a block that is not one
    /// plain paragraph (one paragraph of inline content is the text it
    /// holds, merged into the paragraph at the caret).
    #[must_use]
    pub fn has_structure(&self) -> bool {
        let blocks = self
            .nodes
            .iter()
            .filter(|n| matches!(n, PastedNode::Block(..)))
            .count();
        match blocks {
            0 => false,
            1 if self.nodes.len() == 1 => !matches!(
                &self.nodes[0],
                PastedNode::Block(PastedBlockKind::Paragraph | PastedBlockKind::Division, children)
                    if children.iter().all(|c| !matches!(c, PastedNode::Block(..)))
            ),
            _ => true,
        }
    }

    /// The text of it with the formatted spans of that text - what the
    /// text pipeline inserts when the fragment is inline: a `<br>` and the
    /// boundary between two blocks become a line break.
    #[must_use]
    pub fn text_and_spans(&self) -> (String, Vec<FormatSpan>) {
        fn walk(nodes: &[PastedNode], text: &mut String, spans: &mut Vec<FormatSpan>) {
            for node in nodes {
                match node {
                    PastedNode::Text(t, formats) => {
                        let start = text.len();
                        text.push_str(t);
                        let overrides = formats.as_overrides();
                        if !overrides.is_empty() {
                            spans.push(FormatSpan {
                                start,
                                end: text.len(),
                                formats: overrides,
                            });
                        }
                    }
                    PastedNode::Break => text.push('\n'),
                    PastedNode::Link(_, children) => walk(children, text, spans),
                    PastedNode::Block(_, children) => {
                        if !text.is_empty() && !text.ends_with('\n') {
                            text.push('\n');
                        }
                        walk(children, text, spans);
                    }
                }
            }
        }
        let mut text = String::new();
        let mut spans = Vec::new();
        walk(&self.nodes, &mut text, &mut spans);
        (text, spans)
    }

    /// Its nodes as `Dom` subtrees: formats as `<b>` / `<i>` / `<u>` / `<s>`
    /// around the text (in that order, outermost first), a link as
    /// `<a href>`, a block as its element.
    #[must_use]
    pub fn to_doms(&self) -> Vec<Dom> {
        self.nodes.iter().map(PastedNode::to_dom).collect()
    }
}

impl PastedNode {
    /// Whether this is a block.
    #[must_use]
    pub const fn is_block(&self) -> bool {
        matches!(self, Self::Block(..))
    }

    /// A paragraph (or a division) holding inline content only: what merges
    /// into the paragraph at the caret when it is the first or the last of
    /// a paste.
    #[must_use]
    pub fn is_inline_paragraph(&self) -> bool {
        matches!(
            self,
            Self::Block(PastedBlockKind::Paragraph | PastedBlockKind::Division, children)
                if !children.iter().any(PastedNode::is_block)
        )
    }

    /// This node as a `Dom` subtree (see [`PastedFragment::to_doms`]).
    #[must_use]
    pub fn to_dom(&self) -> Dom {
        dom_of(self)
    }
}

fn dom_of(node: &PastedNode) -> Dom {
    match node {
        PastedNode::Text(text, formats) => {
            let mut dom =
                Dom::create_text_do_not_use_without_block_level_wrapper(AzString::from(text.as_str()));
            if formats.strikethrough {
                dom = Dom::create_s().with_child(dom);
            }
            if formats.underline {
                dom = Dom::create_u().with_child(dom);
            }
            if formats.italic {
                dom = Dom::create_i().with_child(dom);
            }
            if formats.bold {
                dom = Dom::create_b().with_child(dom);
            }
            dom
        }
        PastedNode::Break => Dom::create_br(),
        PastedNode::Link(href, children) => {
            let mut dom = Dom::create_a_no_a11y(AzString::from(href.as_str()), OptionString::None);
            for child in children {
                dom.add_child(dom_of(child));
            }
            dom
        }
        PastedNode::Block(kind, children) => {
            let mut dom = match kind {
                PastedBlockKind::Paragraph => Dom::create_p(),
                PastedBlockKind::Division => Dom::create_div(),
                PastedBlockKind::Heading(1) => Dom::create_h1(),
                PastedBlockKind::Heading(2) => Dom::create_h2(),
                PastedBlockKind::Heading(3) => Dom::create_h3(),
                PastedBlockKind::Heading(4) => Dom::create_h4(),
                PastedBlockKind::Heading(5) => Dom::create_h5(),
                PastedBlockKind::Heading(_) => Dom::create_h6(),
                PastedBlockKind::Quote => Dom::create_blockquote(),
                PastedBlockKind::UnorderedList => Dom::create_ul(),
                PastedBlockKind::OrderedList => Dom::create_ol(),
                PastedBlockKind::ListItem => Dom::create_li(),
                PastedBlockKind::Preformatted => Dom::create_pre(),
            };
            for child in children {
                dom.add_child(dom_of(child));
            }
            dom
        }
    }
}

/// The value of attribute `name` of `node` (the name in any case).
fn attribute<'a>(node: &'a XmlNode, name: &str) -> Option<&'a str> {
    node.attributes
        .as_ref()
        .iter()
        .find(|pair| pair.key.as_str().eq_ignore_ascii_case(name))
        .map(|pair| pair.value.as_str())
}

/// The formats inside `node`: its tag's, then its `style` attribute's.
fn formats_of(node: &XmlNode, tag: &str, mut formats: PastedFormats) -> PastedFormats {
    match tag {
        "b" | "strong" => formats.bold = true,
        "i" | "em" | "cite" | "var" | "dfn" => formats.italic = true,
        "u" | "ins" => formats.underline = true,
        "s" | "strike" | "del" => formats.strikethrough = true,
        _ => {}
    }
    let Some(style) = attribute(node, "style") else {
        return formats;
    };
    for declaration in style.split(';') {
        let Some((name, value)) = declaration.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim().to_ascii_lowercase();
        let value = value.trim_end_matches("!important").trim();
        match name.as_str() {
            "font-weight" => {
                formats.bold = matches!(value, "bold" | "bolder")
                    || value.parse::<u32>().is_ok_and(|w| w >= 600);
            }
            "font-style" => {
                formats.italic = value.starts_with("italic") || value.starts_with("oblique");
            }
            "text-decoration" | "text-decoration-line" => {
                formats.underline = value.contains("underline");
                formats.strikethrough = value.contains("line-through");
            }
            _ => {}
        }
    }
    formats
}

/// A link target a paste may keep: `http:`, `https:` and `mailto:`.
fn safe_href(href: &str) -> Option<String> {
    let href = href.trim();
    let lower = href.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:"))
        .then(|| String::from(href))
}

/// Runs of HTML white space as one space.
fn collapse_white_space(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for ch in text.chars() {
        if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(ch);
            in_space = false;
        }
    }
    out
}

fn convert_children(
    children: &[XmlNodeChild],
    formats: PastedFormats,
    pre: bool,
    out: &mut Vec<PastedNode>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    for child in children {
        match child {
            XmlNodeChild::Text(text) => {
                let text = if pre {
                    String::from(text.as_str())
                } else {
                    collapse_white_space(text.as_str())
                };
                if !text.is_empty() {
                    out.push(PastedNode::Text(text, formats));
                }
            }
            XmlNodeChild::Element(node) => convert_element(node, formats, pre, out, depth + 1),
        }
    }
}

fn convert_element(
    node: &XmlNode,
    formats: PastedFormats,
    pre: bool,
    out: &mut Vec<PastedNode>,
    depth: usize,
) {
    let tag = node.node_type.as_str().to_ascii_lowercase();
    if DROPPED.contains(&tag.as_str()) || tag.contains(':') && tag != "o:p" {
        return;
    }
    let formats = formats_of(node, &tag, formats);
    let children = node.children.as_ref();
    let block = match tag.as_str() {
        "br" => {
            out.push(PastedNode::Break);
            return;
        }
        "p" | "o:p" | "tr" | "dt" | "dd" | "address" | "center" | "caption" => {
            Some(PastedBlockKind::Paragraph)
        }
        "div" | "section" | "article" | "header" | "footer" | "main" | "nav" | "aside"
        | "figure" | "figcaption" | "dl" => Some(PastedBlockKind::Division),
        "h1" => Some(PastedBlockKind::Heading(1)),
        "h2" => Some(PastedBlockKind::Heading(2)),
        "h3" => Some(PastedBlockKind::Heading(3)),
        "h4" => Some(PastedBlockKind::Heading(4)),
        "h5" => Some(PastedBlockKind::Heading(5)),
        "h6" => Some(PastedBlockKind::Heading(6)),
        "blockquote" => Some(PastedBlockKind::Quote),
        "ul" | "menu" | "dir" => Some(PastedBlockKind::UnorderedList),
        "ol" => Some(PastedBlockKind::OrderedList),
        "li" => Some(PastedBlockKind::ListItem),
        "pre" => Some(PastedBlockKind::Preformatted),
        "a" => {
            let mut inner = Vec::new();
            convert_children(children, formats, pre, &mut inner, depth);
            match attribute(node, "href").and_then(safe_href) {
                Some(href) => out.push(PastedNode::Link(href, inner)),
                None => out.extend(inner),
            }
            return;
        }
        // A cell's content, a space after it.
        "td" | "th" => {
            convert_children(children, formats, pre, out, depth);
            out.push(PastedNode::Text(String::from(" "), formats));
            return;
        }
        _ => None,
    };
    match block {
        Some(kind) => {
            let pre = pre || kind == PastedBlockKind::Preformatted;
            let mut inner = Vec::new();
            convert_children(children, formats, pre, &mut inner, depth);
            if !pre {
                tidy(&mut inner, false);
            }
            // A division that only holds blocks is no block of its own (a
            // mail's `<div dir="ltr">` around everything).
            if kind == PastedBlockKind::Division
                && !inner.is_empty()
                && inner.iter().all(|n| matches!(n, PastedNode::Block(..)))
            {
                out.extend(inner);
            } else {
                out.push(PastedNode::Block(kind, inner));
            }
        }
        // Anything else: its content, in its formats.
        None => convert_children(children, formats, pre, out, depth),
    }
}

/// White space between blocks goes, and a block's content starts and ends
/// without it (a source's line breaks and indentation around its tags).
fn tidy(nodes: &mut Vec<PastedNode>, top_level: bool) {
    let has_block = nodes.iter().any(|n| matches!(n, PastedNode::Block(..)));
    if has_block {
        nodes.retain(|n| !matches!(n, PastedNode::Text(t, _) if t.trim().is_empty()));
    }
    let _ = top_level;
    if let Some(PastedNode::Text(t, _)) = nodes.first_mut() {
        *t = String::from(t.trim_start());
    }
    if let Some(PastedNode::Text(t, _)) = nodes.last_mut() {
        *t = String::from(t.trim_end());
    }
    nodes.retain(|n| !matches!(n, PastedNode::Text(t, _) if t.is_empty()));
}
