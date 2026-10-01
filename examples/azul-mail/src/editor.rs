//! The compose window's rich editor: azul's "Path 2" (`DocumentChangeset::apply_to_dom`).
//!
//! The editor is one `contenteditable` host whose children the APP holds as a `Dom` (the model):
//! paragraphs (`<p>`), quotes (`<blockquote>`, nested by depth), lists (`<ul>` / `<ol>` of
//! `<li>`), and inline `<b>` `<i>` `<u>` `<a>` around text. azul edits it like a browser:
//!
//! - typing changes text in place and is reported as text edits ([`sync_text`] copies them into
//!   the model, `get_unsynced_text_edits` + `mark_text_revision_synced`);
//! - Enter, Backspace / Delete across blocks, a rich paste and a delete over a selection are
//!   STRUCTURAL edits the engine records ([`apply_structural_edit`] applies them to the model with
//!   the engine's own applier, acknowledges them with their inverse, and the window rebuilds);
//! - Ctrl/Cmd+B / I / U at a caret are the engine's typing style (`toggle_text_format`).
//!
//! The model comes from a [`MailDoc`] ([`doc_to_host`]: a reply's quote, a forward's header
//! block, a draft) and goes back to one on Save and Send ([`host_to_doc`]), which `compose.rs`
//! writes as text/plain and text/html. A link keeps its address in a class
//! (`azmail-href:<address>`): the generated API has no attribute getter on `NodeData`, so a
//! link the app made is read back from its classes; a pasted link keeps its text.

use azul::{
    callbacks::DocumentChangeset,
    css::{BoxOrStaticString, DocOpWrapRange, DocumentOperation, NodePosition},
    dom::{DomNodeId, IdOrClass},
    misc::EditResumePoint,
    option::OptionString,
    prelude::*,
    time::Instant,
};

use crate::compose::{Block, BlockKind, MailDoc, Run};

/// The editor host's DOM id (scripts focus it as `#compose-body`).
pub const HOST_ID: &str = "compose-body";
/// The class prefix that carries a link's address.
pub const LINK_CLASS_PREFIX: &str = "azmail-href:";

/// The host's own style: the body of a mail on white paper (like the reading pane's), whatever
/// the app's mode.
const HOST_CSS: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 160px; \
                        padding: 12px 14px; background: #ffffff; color: #1a1a1a; font-size: 14px; \
                        font-family: sans-serif; overflow-y: auto; cursor: text;";

/// The blocks' style, scoped to the host. An empty paragraph keeps one line (a caret can stand
/// in it); a quote is the interop form's bar.
const BLOCK_CSS: &str = "
    p { margin: 0px; min-height: 18px; }
    blockquote { margin: 4px 0px 4px 4px; padding-left: 10px; border-left: 2px solid #1a73e8;
                 color: #3c4043; }
    blockquote blockquote { border-left: 2px solid #188038; }
    blockquote blockquote blockquote { border-left: 2px solid #e37400; }
    ul, ol { margin: 0px 0px 0px 24px; }
    li { min-height: 18px; }
    a { color: #0b57d0; text-decoration: underline; }
";

/// The text of a text node's payload.
fn box_str(s: &BoxOrStaticString) -> &str {
    // SAFETY: both variants point at a live `AzString` the node owns (as AzWriter reads them).
    unsafe {
        match s {
            BoxOrStaticString::Boxed(p) => (**p).as_str(),
            BoxOrStaticString::Static(p) => (**p).as_str(),
        }
    }
}

// ==== MailDoc -> Dom ====

/// The editor host for `doc`: the `contenteditable` div with its blocks as children.
pub fn doc_to_host(doc: &MailDoc) -> Dom {
    let mut host = Dom::create_div()
        .with_id(HOST_ID)
        .with_contenteditable(true)
        .with_css(HOST_CSS);
    host.css = vec![Css::from_string(BLOCK_CSS)].into();
    for child in blocks_to_doms(&doc.blocks, 0) {
        host.add_child(child);
    }
    if host.children.as_ref().is_empty() {
        host.add_child(Dom::create_p());
    }
    host.fixup_children_estimated();
    host
}

/// `blocks` at quote depth `depth`: deeper blocks go into one `<blockquote>` (recursively),
/// consecutive list items of one kind into one list.
fn blocks_to_doms(blocks: &[Block], depth: u8) -> Vec<Dom> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < blocks.len() {
        if blocks[i].quote > depth {
            let start = i;
            while i < blocks.len() && blocks[i].quote > depth {
                i += 1;
            }
            let mut quote = Dom::create_blockquote();
            for child in blocks_to_doms(&blocks[start..i], depth + 1) {
                quote.add_child(child);
            }
            out.push(quote);
            continue;
        }
        match blocks[i].kind {
            BlockKind::Paragraph => {
                out.push(block_dom(Dom::create_p(), &blocks[i].runs));
                i += 1;
            }
            kind => {
                let mut list = if kind == BlockKind::Bullet {
                    Dom::create_ul()
                } else {
                    Dom::create_ol()
                };
                while i < blocks.len() && blocks[i].quote == depth && blocks[i].kind == kind {
                    list.add_child(block_dom(Dom::create_li(), &blocks[i].runs));
                    i += 1;
                }
                out.push(list);
            }
        }
    }
    out
}

/// A block element with its runs.
fn block_dom(mut node: Dom, runs: &[Run]) -> Dom {
    for run in runs.iter().filter(|r| !r.text.is_empty()) {
        node.add_child(run_dom(run));
    }
    node
}

/// A run: its text inside `<u>`, `<i>`, `<b>` and the link, inside out.
pub fn run_dom(run: &Run) -> Dom {
    let mut node = Dom::create_text_do_not_use_without_block_level_wrapper(run.text.as_str());
    if run.underline {
        node = Dom::create_u().with_child(node);
    }
    if run.italic {
        node = Dom::create_i().with_child(node);
    }
    if run.bold {
        node = Dom::create_b().with_child(node);
    }
    if let Some(link) = run.link.as_deref() {
        node = link_dom(link).with_child(node);
    }
    node
}

/// An empty `<a>` for `href`, its address kept in a class.
pub fn link_dom(href: &str) -> Dom {
    Dom::create_a_no_a11y(href, OptionString::None).with_class(format!("{LINK_CLASS_PREFIX}{href}"))
}

// ==== Dom -> MailDoc ====

/// The mail body the host holds now.
pub fn host_to_doc(host: &Dom) -> MailDoc {
    let mut collector = Collector::default();
    for child in host.children.as_ref() {
        collector.node(child, &RunStyle::default());
    }
    collector.flush(false);
    if collector.out.is_empty() {
        MailDoc::empty()
    } else {
        MailDoc {
            blocks: collector.out,
        }
    }
}

/// The inline style a run inherits from the elements around it.
#[derive(Debug, Clone, Default)]
struct RunStyle {
    bold: bool,
    italic: bool,
    underline: bool,
    link: Option<String>,
}

/// Walks the host's subtree into blocks.
#[derive(Default)]
struct Collector {
    out: Vec<Block>,
    /// The runs of the block being read.
    runs: Vec<Run>,
    depth: u8,
    kind: BlockKind,
}

impl Collector {
    /// Ends the block being read; an empty one only when `keep_empty` (an empty `<p>` is an
    /// empty line, nothing between two blocks is nothing).
    fn flush(&mut self, keep_empty: bool) {
        if self.runs.is_empty() && !keep_empty {
            return;
        }
        self.out.push(Block {
            quote: self.depth,
            kind: self.kind,
            runs: std::mem::take(&mut self.runs),
        });
    }

    /// A block element: its content is one block of `kind` (a block inside it splits it).
    fn block(&mut self, node: &Dom, kind: BlockKind) {
        self.flush(false);
        let outer = self.kind;
        self.kind = kind;
        for child in node.children.as_ref() {
            self.node(child, &RunStyle::default());
        }
        self.flush(true);
        self.kind = outer;
    }

    /// The children of `node` with `style`.
    fn children(&mut self, node: &Dom, style: &RunStyle) {
        for child in node.children.as_ref() {
            self.node(child, style);
        }
    }

    fn node(&mut self, node: &Dom, style: &RunStyle) {
        match &node.root.node_type {
            NodeType::Text(text) => self.text(box_str(text), style),
            NodeType::Br => self.flush(true),
            NodeType::B | NodeType::Strong => {
                let s = RunStyle {
                    bold: true,
                    ..style.clone()
                };
                self.children(node, &s);
            }
            NodeType::I | NodeType::Em | NodeType::Cite => {
                let s = RunStyle {
                    italic: true,
                    ..style.clone()
                };
                self.children(node, &s);
            }
            NodeType::U | NodeType::Ins => {
                let s = RunStyle {
                    underline: true,
                    ..style.clone()
                };
                self.children(node, &s);
            }
            NodeType::A => {
                let s = RunStyle {
                    link: link_of(node).or_else(|| style.link.clone()),
                    ..style.clone()
                };
                self.children(node, &s);
            }
            NodeType::BlockQuote => {
                self.flush(false);
                self.depth = self.depth.saturating_add(1);
                self.children(node, &RunStyle::default());
                self.flush(false);
                self.depth = self.depth.saturating_sub(1);
            }
            NodeType::Ul | NodeType::Ol => {
                self.flush(false);
                let kind = if matches!(node.root.node_type, NodeType::Ol) {
                    BlockKind::Numbered
                } else {
                    BlockKind::Bullet
                };
                for item in node.children.as_ref() {
                    if matches!(item.root.node_type, NodeType::Li) {
                        self.block(item, kind);
                    } else {
                        self.node(item, &RunStyle::default());
                    }
                }
            }
            NodeType::P
            | NodeType::Div
            | NodeType::Li
            | NodeType::H1
            | NodeType::H2
            | NodeType::H3
            | NodeType::H4
            | NodeType::H5
            | NodeType::H6
            | NodeType::Pre
            | NodeType::Address
            | NodeType::Section
            | NodeType::Article => self.block(node, BlockKind::Paragraph),
            _ => self.children(node, style),
        }
    }

    /// A text node: a run, joined to the one before when it has the same style.
    fn text(&mut self, text: &str, style: &RunStyle) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.runs.last_mut() {
            if last.bold == style.bold
                && last.italic == style.italic
                && last.underline == style.underline
                && last.link == style.link
            {
                last.text.push_str(text);
                return;
            }
        }
        self.runs.push(Run {
            text: text.to_string(),
            bold: style.bold,
            italic: style.italic,
            underline: style.underline,
            link: style.link.clone(),
        });
    }
}

/// The address of a link the app made (from its class).
fn link_of(node: &Dom) -> Option<String> {
    node.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .find_map(|ic| match ic {
            IdOrClass::Class(c) => c
                .as_str()
                .strip_prefix(LINK_CLASS_PREFIX)
                .map(str::to_string),
            IdOrClass::Id(_) => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> MailDoc {
        MailDoc {
            blocks: vec![
                Block::paragraph(0, ""),
                Block {
                    quote: 0,
                    kind: BlockKind::Paragraph,
                    runs: vec![
                        Run::plain("See "),
                        Run {
                            text: String::from("the plan"),
                            bold: true,
                            link: Some(String::from("https://example.org/plan")),
                            ..Run::default()
                        },
                        Run::plain("."),
                    ],
                },
                Block {
                    quote: 0,
                    kind: BlockKind::Bullet,
                    runs: vec![Run::plain("bulbs")],
                },
                Block {
                    quote: 0,
                    kind: BlockKind::Bullet,
                    runs: vec![Run {
                        text: String::from("gloves"),
                        italic: true,
                        underline: true,
                        ..Run::default()
                    }],
                },
                Block::paragraph(0, "On Wed, Ben wrote:"),
                Block::paragraph(1, "Hi Ada,"),
                Block::paragraph(2, "Last year it came early."),
                Block::paragraph(1, "Bring gloves."),
                Block {
                    quote: 0,
                    kind: BlockKind::Numbered,
                    runs: vec![Run::plain("one")],
                },
            ],
        }
    }

    #[test]
    fn a_mail_body_survives_the_editor_model_both_ways() {
        let host = doc_to_host(&doc());
        assert_eq!(host_to_doc(&host), doc());
    }

    #[test]
    fn the_host_is_an_editable_div_with_quotes_nested_and_lists_grouped() {
        let host = doc_to_host(&doc());
        assert!(host.root.is_contenteditable());
        assert!(host.root.has_id(HOST_ID));
        let kinds: Vec<&'static str> = host
            .children
            .as_ref()
            .iter()
            .map(|c| match c.root.node_type {
                NodeType::P => "p",
                NodeType::Ul => "ul",
                NodeType::Ol => "ol",
                NodeType::BlockQuote => "blockquote",
                _ => "?",
            })
            .collect();
        assert_eq!(kinds, vec!["p", "p", "ul", "p", "blockquote", "ol"]);
        let quote = &host.children.as_ref()[4];
        assert_eq!(quote.children.as_ref().len(), 3, "p, the inner quote, p");
    }

    #[test]
    fn an_empty_body_is_one_empty_paragraph_both_ways() {
        let host = doc_to_host(&MailDoc::empty());
        assert_eq!(host.children.as_ref().len(), 1);
        assert_eq!(host_to_doc(&host), MailDoc::empty());
        assert_eq!(host_to_doc(&Dom::create_div()), MailDoc::empty());
    }

    #[test]
    fn markup_the_engine_or_a_paste_made_reads_back_as_blocks() {
        // `<p>a<br>b</p><div><b>c</b></div>` + a link without the class (a pasted one).
        let host = Dom::create_div()
            .with_child(
                Dom::create_p()
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("a"))
                    .with_child(Dom::create_br())
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("b")),
            )
            .with_child(Dom::create_div().with_child(
                Dom::create_b().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("c")),
            ))
            .with_child(Dom::create_p().with_child(
                Dom::create_a_no_a11y("https://x.example", OptionString::None)
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("pasted")),
            ));
        let doc = host_to_doc(&host);
        let texts: Vec<String> = doc.blocks.iter().map(Block::text).collect();
        assert_eq!(texts, vec!["a", "b", "c", "pasted"]);
        assert!(doc.blocks[2].runs[0].bold);
        assert_eq!(doc.blocks[3].runs[0].link, None, "a pasted link keeps its text");
    }
}
