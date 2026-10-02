//! A [`RichTextDoc`] as HTML and as plain text, and HTML / DOM back into
//! blocks.
//!
//! - [`doc_to_html`]: the HTML a mail's `text/html` part carries (AzMail's
//!   `MailDoc::to_html`, promoted and widened to every block kind): a `<div>`
//!   per paragraph (`<div><br></div>` for an empty one, as mail clients
//!   write it), quote levels nested as `<blockquote type="cite">`, list items
//!   in nested `<ul>` / `<ol>`, runs as `<b>` `<i>` `<u>` `<s>` `<code>` and
//!   `<a href>` (web and mail addresses only); text escaped.
//! - [`doc_to_plain_text`]: a mail's `text/plain` part - a line per block,
//!   `> ` per quote level, `- ` / `1. ` / `- [x] ` before list items, a
//!   link as `text <address>`.
//! - [`plain_text_to_doc`]: plain text back into paragraphs, `>` quote
//!   marks as quote depth (a reply's quote of a plain-text mail).
//! - [`html_to_doc`]: HTML into blocks through the engine's paste sanitizer
//!   (`paste_html::sanitize_html`: the lenient parser and ONE policy of
//!   what survives) and [`blocks_from_doms`] - a reopened draft keeps its
//!   bold, italic and links (scripts/DEDUP_EDITORS F3).
//! - [`blocks_from_doms`]: DOM subtrees into blocks - a sanitized paste,
//!   and the content of the engine's structural edits (a paste or a delete
//!   across blocks), which clones the editor's own block and run elements
//!   with the classes this module names.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use azul_core::dom::{AttributeType, Dom, NodeType};
use azul_css::AzString;

use super::doc::{
    push_run, PastedBlock, RichBlock, RichBlockKind, RichCheck, RichFormats, RichRun, RichTable,
    RichTableRow, RichTableRowVec, RichTextDoc,
};

// ==== The classes the editor's elements carry (read back from clones) ====

/// A bold run.
pub const RUN_BOLD_CLASS: &str = "__azul-rte-bold";
/// An italic run.
pub const RUN_ITALIC_CLASS: &str = "__azul-rte-italic";
/// An underlined run.
pub const RUN_UNDERLINE_CLASS: &str = "__azul-rte-underline";
/// A struck-through run.
pub const RUN_STRIKE_CLASS: &str = "__azul-rte-strike";
/// An inline-code run.
pub const RUN_CODE_CLASS: &str = "__azul-rte-code";
/// Every block element of the editor.
pub const BLOCK_CLASS: &str = "__azul-rte-block";
/// A bulleted list item (with [`indent_class`]).
pub const BULLET_CLASS: &str = "__azul-rte-bullet";
/// A numbered list item (with [`indent_class`]).
pub const NUMBERED_CLASS: &str = "__azul-rte-numbered";
/// A check item (with [`indent_class`]).
pub const CHECK_CLASS: &str = "__azul-rte-check";
/// A ticked check item.
pub const CHECKED_CLASS: &str = "__azul-rte-checked";
/// An element of the editor that is not text (a check item's box, an
/// image): `contenteditable=false`, skipped when content is read back.
pub const ISLAND_CLASS: &str = "__azul-rte-island";

/// The class of a list item at `indent`.
#[must_use]
pub fn indent_class(indent: u8) -> String {
    format!("__azul-rte-indent-{indent}")
}

/// The class of a block quoted `depth` levels deep (none for 0).
#[must_use]
pub fn quote_class(depth: u8) -> String {
    format!("__azul-rte-quote-{depth}")
}

/// The number a class of `prefix` carries (`__azul-rte-indent-2` -> 2).
fn class_number(dom: &Dom, prefix: &str) -> Option<u8> {
    dom.root.attributes().as_ref().iter().find_map(|attr| match attr {
        AttributeType::Class(c) => c.as_str().strip_prefix(prefix)?.parse().ok(),
        _ => None,
    })
}

impl RichTextDoc {
    /// The document as an HTML fragment (a mail's `text/html` body goes
    /// inside `<html><body>`; see [`doc_to_html`]).
    #[must_use]
    pub fn to_html(&self) -> AzString {
        AzString::from(doc_to_html(self))
    }

    /// The document as plain text (see [`doc_to_plain_text`]).
    #[must_use]
    pub fn to_plain_text(&self) -> AzString {
        AzString::from(doc_to_plain_text(self))
    }

    /// Plain text as paragraphs, one per line, `>` quote marks as quote
    /// depth plus `extra_quote` (a reply quotes the original one deeper).
    #[must_use]
    pub fn from_plain_text(text: AzString, extra_quote: u8) -> Self {
        plain_text_to_doc(text.as_str(), extra_quote)
    }

    /// HTML (a mail body, a paste) as a document (see [`html_to_doc`]).
    #[must_use]
    pub fn from_html(html: AzString) -> Self {
        html_to_doc(html.as_str())
    }
}

// ==== HTML ====

/// `text` escaped for HTML (`&`, `<`, `>`, and `"` for attribute values).
fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Whether a link may go into HTML: web and mail addresses only.
#[must_use]
pub fn is_safe_link(link: &str) -> bool {
    let lower = link.trim().to_ascii_lowercase();
    ["https://", "http://", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
}

/// A run as HTML: escaped, then `<code>`, `<s>`, `<u>`, `<i>`, `<b>` and
/// the link around it, inside out.
fn run_html(run: &RichRun) -> String {
    let mut html = escape_html(run.as_str()).replace('\n', "<br>");
    let f = run.formats;
    if f.code {
        html = format!("<code>{html}</code>");
    }
    if f.strike {
        html = format!("<s>{html}</s>");
    }
    if f.underline {
        html = format!("<u>{html}</u>");
    }
    if f.italic {
        html = format!("<i>{html}</i>");
    }
    if f.bold {
        html = format!("<b>{html}</b>");
    }
    if let Some(link) = run.link_str().filter(|l| is_safe_link(l)) {
        html = format!("<a href=\"{}\">{html}</a>", escape_html(link.trim()));
    }
    html
}

/// A block's runs as inline HTML.
fn runs_html(runs: &[RichRun]) -> String {
    runs.iter().map(run_html).collect()
}

/// The `style` attribute of a block's alignment ("" for left).
fn align_attr(block: &RichBlock) -> String {
    match block.align {
        super::doc::RichAlign::Left => String::new(),
        other => format!(" style=\"text-align: {}\"", other.css()),
    }
}

/// Closes every open list (each still holds its open `<li>`).
fn close_lists(out: &mut String, lists: &mut Vec<&'static str>) {
    while let Some(tag) = lists.pop() {
        out.push_str("</li></");
        out.push_str(tag);
        out.push('>');
    }
}

/// The document as an HTML fragment (see the module docs).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn doc_to_html(doc: &RichTextDoc) -> String {
    let mut out = String::new();
    let mut depth: u8 = 0;
    // The open lists, outermost first; the innermost item of each is open.
    let mut lists: Vec<&'static str> = Vec::new();
    for block in doc.blocks() {
        if !block.kind.is_list() || block.quote_depth != depth {
            close_lists(&mut out, &mut lists);
        }
        while depth > block.quote_depth {
            out.push_str("</blockquote>");
            depth -= 1;
        }
        while depth < block.quote_depth {
            out.push_str("<blockquote type=\"cite\">");
            depth += 1;
        }
        let align = align_attr(block);
        let inner = runs_html(block.runs.as_ref());
        match &block.kind {
            RichBlockKind::Paragraph if inner.is_empty() => {
                out.push_str(&format!("<div{align}><br></div>"));
            }
            RichBlockKind::Paragraph => out.push_str(&format!("<div{align}>{inner}</div>")),
            RichBlockKind::Heading(level) => {
                let level = (*level).clamp(1, 6);
                out.push_str(&format!("<h{level}{align}>{inner}</h{level}>"));
            }
            RichBlockKind::Bullet(_) | RichBlockKind::Numbered(_) | RichBlockKind::Check(_) => {
                let tag = if matches!(block.kind, RichBlockKind::Numbered(_)) {
                    "ol"
                } else {
                    "ul"
                };
                let level = usize::from(block.kind.indent()) + 1;
                while lists.len() > level {
                    if let Some(open) = lists.pop() {
                        out.push_str(&format!("</li></{open}>"));
                    }
                }
                if lists.len() == level {
                    if lists.last().copied() == Some(tag) {
                        out.push_str("</li>");
                    } else if let Some(open) = lists.pop() {
                        out.push_str(&format!("</li></{open}>"));
                    }
                }
                while lists.len() < level {
                    out.push_str(&format!("<{tag}>"));
                    lists.push(tag);
                    if lists.len() < level {
                        // A skipped level: an item to hold the deeper list.
                        out.push_str("<li>");
                    }
                }
                let mark = match &block.kind {
                    RichBlockKind::Check(c) if c.checked => "&#9745; ",
                    RichBlockKind::Check(_) => "&#9744; ",
                    _ => "",
                };
                out.push_str(&format!("<li{align}>{mark}{inner}"));
            }
            RichBlockKind::Code(lang) => {
                let class = if lang.as_str().is_empty() {
                    String::new()
                } else {
                    format!(" class=\"language-{}\"", escape_html(lang.as_str()))
                };
                out.push_str(&format!(
                    "<pre><code{class}>{}</code></pre>",
                    escape_html(&block.flat())
                ));
            }
            RichBlockKind::Rule => out.push_str("<hr>"),
            RichBlockKind::Image(image) => out.push_str(&format!(
                "<img src=\"{}\" alt=\"{}\">",
                escape_html(image.src.as_str()),
                escape_html(image.alt.as_str())
            )),
            RichBlockKind::PageBreak => {
                out.push_str("<div style=\"break-after: page\"></div>");
            }
            RichBlockKind::Table(table) => {
                out.push_str("<table>");
                for (r, row) in table.rows.as_ref().iter().enumerate() {
                    let cell_tag = if table.has_header && r == 0 { "th" } else { "td" };
                    out.push_str("<tr>");
                    for cell in row.cells.as_ref() {
                        out.push_str(&format!(
                            "<{cell_tag}>{}</{cell_tag}>",
                            escape_html(cell.as_str())
                        ));
                    }
                    out.push_str("</tr>");
                }
                out.push_str("</table>");
            }
        }
    }
    close_lists(&mut out, &mut lists);
    for _ in 0..depth {
        out.push_str("</blockquote>");
    }
    out
}

// ==== Plain text ====

/// A run in plain text: its text, a link as `text <address>` (just the
/// address when the text is the address).
fn run_plain(run: &RichRun) -> String {
    match run.link_str() {
        Some(link) if run.as_str().trim().is_empty() || run.as_str().trim() == link => {
            link.to_string()
        }
        Some(link) => format!("{} <{link}>", run.as_str()),
        None => run.as_str().to_string(),
    }
}

/// The document as plain text: a line per block (a block's line breaks
/// kept), `> ` per quote level, list markers, a table's cells split by
/// ` | `. Every line ends in `\n`.
#[must_use]
pub fn doc_to_plain_text(doc: &RichTextDoc) -> String {
    let mut out = String::new();
    for (index, block) in doc.blocks().iter().enumerate() {
        let text: String = block.runs.as_ref().iter().map(run_plain).collect();
        let indent = "  ".repeat(usize::from(block.kind.indent()));
        let body = match &block.kind {
            RichBlockKind::Paragraph | RichBlockKind::Heading(_) | RichBlockKind::Code(_) => text,
            RichBlockKind::Bullet(_) => format!("{indent}- {text}"),
            RichBlockKind::Numbered(_) => format!("{indent}{}. {text}", doc.number_of(index)),
            RichBlockKind::Check(c) => {
                format!("{indent}- [{}] {text}", if c.checked { "x" } else { " " })
            }
            RichBlockKind::Rule => "----------".to_string(),
            RichBlockKind::Image(image) => {
                if image.alt.as_str().is_empty() {
                    format!("[{}]", image.src.as_str())
                } else {
                    format!("[{}]", image.alt.as_str())
                }
            }
            RichBlockKind::PageBreak => String::new(),
            RichBlockKind::Table(table) => table
                .rows
                .as_ref()
                .iter()
                .map(|row| {
                    row.cells
                        .as_ref()
                        .iter()
                        .map(AzString::as_str)
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .collect::<Vec<_>>()
                .join("\n"),
        };
        let marks = "> ".repeat(usize::from(block.quote_depth));
        for line in body.split('\n') {
            let line = format!("{marks}{line}");
            out.push_str(line.trim_end_matches(' ').trim_end_matches('\t'));
            out.push('\n');
        }
    }
    out
}

/// Plain text as paragraphs, one per line: `>` marks (`>>`, `> >`) are the
/// line's quote depth, plus `extra_quote`. No text is one empty paragraph.
#[must_use]
pub fn plain_text_to_doc(text: &str, extra_quote: u8) -> RichTextDoc {
    let body = text.strip_suffix('\n').unwrap_or(text);
    if body.is_empty() {
        return RichTextDoc::create();
    }
    let blocks: Vec<RichBlock> = body
        .split('\n')
        .map(|raw| {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            let mut level = 0u8;
            let mut rest = line;
            loop {
                if let Some(after) = rest.strip_prefix('>') {
                    level = level.saturating_add(1);
                    rest = after;
                    continue;
                }
                // `> > text`: one space between marks.
                if level > 0 {
                    if let Some(after) = rest.strip_prefix(' ') {
                        if after.starts_with('>') {
                            rest = after;
                            continue;
                        }
                    }
                }
                break;
            }
            if level > 0 {
                rest = rest.strip_prefix(' ').unwrap_or(rest);
            }
            RichBlock::paragraph(rest).with_quote_depth(level.saturating_add(extra_quote))
        })
        .collect();
    RichTextDoc::from_blocks(blocks)
}

// ==== HTML / DOM -> blocks ====

/// HTML as a document: sanitized by the engine's paste policy (scripts,
/// styles, forms, images and every attribute but a safe `href` go), then
/// read into blocks. Empty HTML is one empty paragraph.
#[must_use]
pub fn html_to_doc(html: &str) -> RichTextDoc {
    let doms = crate::paste_html::sanitize_html(html)
        .map(|fragment| fragment.to_doms())
        .unwrap_or_default();
    let blocks: Vec<RichBlock> = blocks_from_doms(&doms)
        .into_iter()
        .map(|part| {
            RichBlock::new(part.kind.unwrap_or(RichBlockKind::Paragraph), part.runs)
                .with_quote_depth(part.quote_depth)
        })
        .collect();
    RichTextDoc::from_blocks(blocks)
}

/// The inline style a run takes from the elements around it.
#[derive(Debug, Clone, Default)]
struct Inline {
    formats: RichFormats,
    link: Option<String>,
}

impl Inline {
    fn run(&self, text: &str) -> RichRun {
        let run = RichRun::plain(text).with_formats(self.formats);
        match &self.link {
            Some(url) => run.with_link(AzString::from(url.as_str())),
            None => run,
        }
    }
}

/// Walks DOM subtrees into blocks.
#[derive(Default)]
struct Collector {
    out: Vec<PastedBlock>,
    /// The runs of the block being read.
    runs: Vec<RichRun>,
    /// The kind of the block being read (`None`: no block element named
    /// one - inline content at the top).
    kind: Option<RichBlockKind>,
    depth: u8,
    /// The open lists, outermost first: ordered or not.
    lists: Vec<bool>,
}

impl Collector {
    /// Ends the block being read; an empty one only when `keep_empty` (an
    /// empty `<p>` is an empty line, nothing between two blocks is
    /// nothing). One trailing line break is the block's placeholder
    /// (`<div><br></div>`), not text.
    fn flush(&mut self, keep_empty: bool) {
        if let Some(last) = self.runs.last_mut() {
            if let Some(kept) = last.as_str().strip_suffix('\n') {
                let kept = kept.to_string();
                last.set_text(kept);
                if last.as_str().is_empty() {
                    self.runs.pop();
                }
            }
        }
        if self.runs.is_empty() && !keep_empty {
            return;
        }
        self.out.push(PastedBlock {
            kind: self.kind.clone(),
            quote_depth: self.depth,
            runs: core::mem::take(&mut self.runs),
        });
    }

    /// A block element: its content is one block of `kind` (a block inside
    /// it splits it).
    fn block(&mut self, dom: &Dom, kind: RichBlockKind) {
        self.flush(false);
        let outer = self.kind.replace(kind);
        for child in dom.children.as_ref() {
            self.node(child, &Inline::default());
        }
        self.flush(true);
        self.kind = outer;
    }

    /// The children of `dom` with `style`.
    fn children(&mut self, dom: &Dom, style: &Inline) {
        for child in dom.children.as_ref() {
            self.node(child, style);
        }
    }

    /// The kind a list item stands for: the editor's own classes, else the
    /// list it sits in.
    fn item_kind(&self, dom: &Dom) -> RichBlockKind {
        let indent = class_number(dom, "__azul-rte-indent-").unwrap_or_else(|| {
            u8::try_from(self.lists.len().saturating_sub(1)).unwrap_or(u8::MAX)
        });
        if dom.root.has_class(CHECK_CLASS) {
            return RichBlockKind::Check(RichCheck {
                indent,
                checked: dom.root.has_class(CHECKED_CLASS),
            });
        }
        if dom.root.has_class(NUMBERED_CLASS) {
            return RichBlockKind::Numbered(indent);
        }
        if dom.root.has_class(BULLET_CLASS) {
            return RichBlockKind::Bullet(indent);
        }
        if self.lists.last().copied().unwrap_or(false) {
            RichBlockKind::Numbered(indent)
        } else {
            RichBlockKind::Bullet(indent)
        }
    }

    #[allow(clippy::too_many_lines)]
    fn node(&mut self, dom: &Dom, style: &Inline) {
        if dom.root.has_class(ISLAND_CLASS) {
            return;
        }
        let mut style = style.clone();
        // The editor's own run classes (a clone of a run of the editor).
        for (class, format) in [
            (RUN_BOLD_CLASS, super::doc::RichFormat::Bold),
            (RUN_ITALIC_CLASS, super::doc::RichFormat::Italic),
            (RUN_UNDERLINE_CLASS, super::doc::RichFormat::Underline),
            (RUN_STRIKE_CLASS, super::doc::RichFormat::Strike),
            (RUN_CODE_CLASS, super::doc::RichFormat::Code),
        ] {
            if dom.root.has_class(class) {
                style.formats.set(format, true);
            }
        }
        // A block of the editor quoted deeper than the blocks around it.
        let quoted = class_number(dom, "__azul-rte-quote-");
        let outer_depth = self.depth;
        if let Some(depth) = quoted {
            self.flush(false);
            self.depth = outer_depth.max(depth);
        }
        match dom.root.get_node_type() {
            NodeType::Text(text) => {
                let text = text.as_ref().as_str();
                if !text.is_empty() {
                    push_run(&mut self.runs, style.run(text));
                }
            }
            NodeType::Br => push_run(&mut self.runs, style.run("\n")),
            NodeType::B | NodeType::Strong => {
                style.formats.bold = true;
                self.children(dom, &style);
            }
            NodeType::I | NodeType::Em | NodeType::Cite => {
                style.formats.italic = true;
                self.children(dom, &style);
            }
            NodeType::U | NodeType::Ins => {
                style.formats.underline = true;
                self.children(dom, &style);
            }
            NodeType::S | NodeType::Del => {
                style.formats.strike = true;
                self.children(dom, &style);
            }
            NodeType::Code => {
                style.formats.code = true;
                self.children(dom, &style);
            }
            NodeType::A => {
                let href = dom.root.attributes().as_ref().iter().find_map(|attr| match attr {
                    AttributeType::Href(h) => Some(h.as_str().to_string()),
                    _ => None,
                });
                if let Some(href) = href {
                    style.link = Some(href);
                }
                self.children(dom, &style);
            }
            NodeType::BlockQuote => {
                self.flush(false);
                self.depth = self.depth.saturating_add(1);
                self.children(dom, &Inline::default());
                self.flush(false);
                self.depth = self.depth.saturating_sub(1);
            }
            NodeType::Ul | NodeType::Ol => {
                self.flush(false);
                self.lists.push(matches!(dom.root.get_node_type(), NodeType::Ol));
                for item in dom.children.as_ref() {
                    if matches!(item.root.get_node_type(), NodeType::Li) {
                        let kind = self.item_kind(item);
                        self.block(item, kind);
                    } else {
                        self.node(item, &Inline::default());
                    }
                }
                self.lists.pop();
            }
            NodeType::Li => {
                let kind = self.item_kind(dom);
                self.block(dom, kind);
            }
            NodeType::H1 => self.block(dom, RichBlockKind::Heading(1)),
            NodeType::H2 => self.block(dom, RichBlockKind::Heading(2)),
            NodeType::H3 => self.block(dom, RichBlockKind::Heading(3)),
            NodeType::H4 => self.block(dom, RichBlockKind::Heading(4)),
            NodeType::H5 => self.block(dom, RichBlockKind::Heading(5)),
            NodeType::H6 => self.block(dom, RichBlockKind::Heading(6)),
            NodeType::Pre => self.block(dom, RichBlockKind::Code(AzString::from_const_str(""))),
            NodeType::P
            | NodeType::Div
            | NodeType::Address
            | NodeType::Section
            | NodeType::Article
            | NodeType::Header
            | NodeType::Footer
            | NodeType::Main => self.block(dom, RichBlockKind::Paragraph),
            NodeType::Hr | NodeType::PageBreak => {
                self.flush(false);
                let kind = if matches!(dom.root.get_node_type(), NodeType::Hr) {
                    RichBlockKind::Rule
                } else {
                    RichBlockKind::PageBreak
                };
                self.out.push(PastedBlock {
                    kind: Some(kind),
                    quote_depth: self.depth,
                    runs: Vec::new(),
                });
            }
            NodeType::Table => {
                self.flush(false);
                let mut rows: Vec<RichTableRow> = Vec::new();
                let mut has_header = false;
                table_rows(dom, &mut rows, &mut has_header);
                self.out.push(PastedBlock {
                    kind: Some(RichBlockKind::Table(RichTable {
                        rows: RichTableRowVec::from_vec(rows),
                        has_header,
                    })),
                    quote_depth: self.depth,
                    runs: Vec::new(),
                });
            }
            // An image, an island the editor made (a check box): not text.
            NodeType::Image(_) | NodeType::Icon(_) => {}
            _ => self.children(dom, &style),
        }
        if quoted.is_some() {
            self.flush(false);
            self.depth = outer_depth;
        }
    }
}

/// The rows of a table element (`tr` under it or under a `thead` /
/// `tbody` / `tfoot`): each cell's text; a `th` in the first row marks the
/// header.
fn table_rows(dom: &Dom, rows: &mut Vec<RichTableRow>, has_header: &mut bool) {
    for child in dom.children.as_ref() {
        match child.root.get_node_type() {
            NodeType::Tr => {
                let mut cells: Vec<AzString> = Vec::new();
                for cell in child.children.as_ref() {
                    if matches!(cell.root.get_node_type(), NodeType::Th | NodeType::Td) {
                        if rows.is_empty() && matches!(cell.root.get_node_type(), NodeType::Th) {
                            *has_header = true;
                        }
                        let mut text = String::new();
                        dom_text(cell, &mut text);
                        cells.push(AzString::from(text.trim().to_string()));
                    }
                }
                rows.push(RichTableRow::create(azul_css::StringVec::from_vec(cells)));
            }
            NodeType::THead | NodeType::TBody | NodeType::TFoot => {
                table_rows(child, rows, has_header);
            }
            _ => {}
        }
    }
}

/// The text of a DOM subtree, `<br>` as `\n`.
pub fn dom_text(dom: &Dom, out: &mut String) {
    match dom.root.get_node_type() {
        NodeType::Text(text) => out.push_str(text.as_ref().as_str()),
        NodeType::Br => out.push('\n'),
        _ => {}
    }
    for child in dom.children.as_ref() {
        dom_text(child, out);
    }
}

/// DOM subtrees as blocks: each block element one block (its kind from
/// the element and the editor's classes), inline content at the top one
/// block without a kind, `<blockquote>` (or the editor's quote class) a
/// quote level, lists nested as indent levels.
#[must_use]
pub fn blocks_from_doms(doms: &[Dom]) -> Vec<PastedBlock> {
    let mut collector = Collector::default();
    for dom in doms {
        collector.node(dom, &Inline::default());
    }
    collector.flush(false);
    collector.out
}

/// The runs of ONE DOM subtree's inline content (a block element's
/// children, or a fragment of inline elements), formats read off the
/// elements and the editor's classes.
#[must_use]
pub fn runs_from_dom(dom: &Dom) -> Vec<RichRun> {
    let mut collector = Collector::default();
    collector.children(dom, &Inline::default());
    let mut runs = collector.runs;
    for block in collector.out {
        for run in block.runs {
            push_run(&mut runs, run);
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::rich_text::doc::RichFormat;

    fn plain(text: &str) -> RichRun {
        RichRun::plain(text)
    }

    fn bold(text: &str) -> RichRun {
        RichRun::plain(text).with_format(RichFormat::Bold)
    }

    /// AzMail's compose sample (compose.rs `sample_doc`).
    fn sample_doc() -> RichTextDoc {
        let link = plain("the list").with_link(AzString::from("https://example.org/a?b=1&c=2"));
        let gloves = plain("gloves")
            .with_format(RichFormat::Italic)
            .with_format(RichFormat::Underline);
        RichTextDoc::from_blocks(vec![
            RichBlock::new(
                RichBlockKind::Paragraph,
                vec![plain("Thanks, "), bold("bold"), plain(" & "), link],
            ),
            RichBlock::text(RichBlockKind::Bullet(0), "bulbs"),
            RichBlock::new(RichBlockKind::Bullet(0), vec![gloves]),
            RichBlock::paragraph(""),
            RichBlock::paragraph("Bring <gloves>.").with_quote_depth(1),
            RichBlock::paragraph("Deeper.").with_quote_depth(2),
            RichBlock::paragraph("Back up.").with_quote_depth(1),
            RichBlock::text(RichBlockKind::Numbered(0), "one"),
            RichBlock::text(RichBlockKind::Numbered(0), "two"),
        ])
    }

    #[test]
    fn the_text_part_quotes_with_marks_and_writes_lists_and_links_readably() {
        assert_eq!(
            doc_to_plain_text(&sample_doc()),
            "Thanks, bold & the list <https://example.org/a?b=1&c=2>\n\
             - bulbs\n\
             - gloves\n\
             \n\
             > Bring <gloves>.\n\
             > > Deeper.\n\
             > Back up.\n\
             1. one\n\
             2. two\n"
        );
        let link_is_text = RichTextDoc::from_blocks(vec![RichBlock::new(
            RichBlockKind::Paragraph,
            vec![plain("https://example.org").with_link(AzString::from("https://example.org"))],
        )]);
        assert_eq!(doc_to_plain_text(&link_is_text), "https://example.org\n");
    }

    #[test]
    fn the_html_part_nests_quotes_and_lists_and_escapes_text() {
        let html = doc_to_html(&sample_doc());
        assert!(
            html.contains(
                "<div>Thanks, <b>bold</b> &amp; <a href=\"https://example.org/a?b=1&amp;c=2\">the list</a></div>"
            ),
            "{html}"
        );
        assert!(
            html.contains("<ul><li>bulbs</li><li><i><u>gloves</u></i></li></ul>"),
            "{html}"
        );
        assert!(html.contains("<div><br></div>"), "an empty line: {html}");
        assert!(
            html.contains(
                "<blockquote type=\"cite\"><div>Bring &lt;gloves&gt;.</div><blockquote type=\"cite\"><div>Deeper.</div></blockquote><div>Back up.</div></blockquote>"
            ),
            "{html}"
        );
        assert!(html.contains("<ol><li>one</li><li>two</li></ol>"), "{html}");
    }

    #[test]
    fn nested_list_items_nest_their_lists_and_every_kind_has_its_element() {
        let doc = RichTextDoc::from_blocks(vec![
            RichBlock::text(RichBlockKind::Bullet(0), "a"),
            RichBlock::text(RichBlockKind::Bullet(1), "a.1"),
            RichBlock::text(RichBlockKind::Bullet(0), "b"),
            RichBlock::text(RichBlockKind::Heading(2), "Head")
                .with_align(super::super::doc::RichAlign::Center),
            RichBlock::text(RichBlockKind::Code(AzString::from("rust")), "let x = 1 < 2;"),
            RichBlock::new(RichBlockKind::Rule, vec![]),
            RichBlock::new(
                RichBlockKind::Table(RichTable {
                    rows: RichTableRowVec::from_vec(vec![
                        RichTableRow::create(azul_css::StringVec::from(vec![
                            String::from("Q"),
                            String::from("Goal"),
                        ])),
                        RichTableRow::create(azul_css::StringVec::from(vec![
                            String::from("Q4"),
                            String::from("Ship"),
                        ])),
                    ]),
                    has_header: true,
                }),
                vec![],
            ),
        ]);
        let html = doc_to_html(&doc);
        assert!(
            html.starts_with("<ul><li>a<ul><li>a.1</li></ul></li><li>b</li></ul>"),
            "{html}"
        );
        assert!(
            html.contains("<h2 style=\"text-align: center\">Head</h2>"),
            "{html}"
        );
        assert!(
            html.contains("<pre><code class=\"language-rust\">let x = 1 &lt; 2;</code></pre>"),
            "{html}"
        );
        assert!(html.contains("<hr>"), "{html}");
        assert!(
            html.contains("<table><tr><th>Q</th><th>Goal</th></tr><tr><td>Q4</td><td>Ship</td></tr></table>"),
            "{html}"
        );
    }

    #[test]
    fn an_unsafe_link_is_written_as_its_text() {
        let doc = RichTextDoc::from_blocks(vec![RichBlock::new(
            RichBlockKind::Paragraph,
            vec![plain("click").with_link(AzString::from("javascript:alert(1)"))],
        )]);
        assert_eq!(doc_to_html(&doc), "<div>click</div>");
    }

    #[test]
    fn plain_text_becomes_paragraphs_with_quote_depths() {
        let doc = plain_text_to_doc("Hi\n> quoted\n>> deeper\n> > also deeper\n", 1);
        let lines: Vec<(u8, String)> = doc
            .blocks()
            .iter()
            .map(|b| (b.quote_depth, b.flat()))
            .collect();
        assert_eq!(
            lines,
            vec![
                (1, String::from("Hi")),
                (2, String::from("quoted")),
                (3, String::from("deeper")),
                (3, String::from("also deeper")),
            ]
        );
        assert_eq!(
            plain_text_to_doc("", 0),
            RichTextDoc::create(),
            "no text is one empty line"
        );
    }

    #[test]
    fn a_mail_body_written_as_html_reads_back_with_its_formats_links_and_quotes() {
        // A reopened draft came back from text/plain and lost its formats
        // (DEDUP_EDITORS F3): its HTML part reads back whole.
        let html = format!("<html><body>{}</body></html>", doc_to_html(&sample_doc()));
        let back = html_to_doc(&html);
        assert_eq!(back, sample_doc());
    }

    #[test]
    fn markup_a_paste_or_the_engine_made_reads_back_as_blocks() {
        // `<p>a<br>b</p><div><b>c</b></div>` + a link and the editor's own
        // classes on a cloned run.
        let host = vec![
            Dom::create_p()
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("a"))
                .with_child(Dom::create_br())
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("b")),
            Dom::create_div().with_child(
                Dom::create_b()
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("c")),
            ),
            Dom::create_p().with_child(
                Dom::create_a_no_a11y(
                    AzString::from("https://x.example"),
                    azul_css::OptionString::None,
                )
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "pasted",
                )),
            ),
            Dom::create_li()
                .with_class(AzString::from(CHECK_CLASS))
                .with_class(AzString::from(CHECKED_CLASS))
                .with_class(AzString::from(indent_class(1)))
                .with_class(AzString::from(quote_class(2)))
                .with_child(
                    Dom::create_span()
                        .with_class(AzString::from(RUN_ITALIC_CLASS))
                        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                            "done",
                        )),
                ),
        ];
        let blocks = blocks_from_doms(&host);
        let texts: Vec<String> = blocks
            .iter()
            .map(|b| super::super::doc::flatten(&b.runs))
            .collect();
        assert_eq!(texts, vec!["a\nb", "c", "pasted", "done"]);
        assert_eq!(blocks[1].runs, vec![bold("c")]);
        assert_eq!(
            blocks[2].runs[0].link_str(),
            Some("https://x.example"),
            "a pasted link keeps its address"
        );
        assert_eq!(
            blocks[3].kind,
            Some(RichBlockKind::Check(RichCheck {
                indent: 1,
                checked: true
            }))
        );
        assert_eq!(blocks[3].quote_depth, 2);
        assert_eq!(blocks[3].runs, vec![plain("done").with_format(RichFormat::Italic)]);
    }

    #[test]
    fn a_pasted_table_and_rule_are_blocks() {
        let host = vec![
            Dom::create_hr(),
            Dom::create_table_no_a11y().with_child(
                Dom::create_tr()
                    .with_child(Dom::create_td_with_text("x"))
                    .with_child(Dom::create_td_with_text("y")),
            ),
        ];
        let blocks = blocks_from_doms(&host);
        assert_eq!(blocks[0].kind, Some(RichBlockKind::Rule));
        let Some(RichBlockKind::Table(table)) = &blocks[1].kind else {
            panic!("a table: {blocks:?}");
        };
        assert!(!table.has_header);
        assert_eq!(table.rows.as_ref()[0].cell(1), "y");
    }
}
