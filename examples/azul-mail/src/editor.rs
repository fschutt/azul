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
//! writes as text/plain and text/html. A link keeps its address where HTML keeps it, in its
//! `href` attribute (`NodeData::get_attribute`) - a link the app made and a pasted one alike.

use azul::{
    callbacks::DocumentChangeset,
    css::{BoxOrStaticString, DocOpWrapRange, DocumentOperation, NodePosition},
    dom::DomNodeId,
    misc::EditResumePoint,
    option::OptionString,
    prelude::*,
    time::Instant,
};

use crate::compose::{Block, BlockKind, MailDoc, Run};

/// The editor host's DOM id (scripts focus it as `#compose-body`).
pub const HOST_ID: &str = "compose-body";

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

/// An empty `<a>` for `href` (its address in its `href` attribute).
pub fn link_dom(href: &str) -> Dom {
    Dom::create_a_no_a11y(href, OptionString::None)
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

/// The address of a link (its `href` attribute): one the app made or a pasted one.
fn link_of(node: &Dom) -> Option<String> {
    node.root
        .get_attribute("href")
        .into_option()
        .map(|href| href.as_str().to_string())
        .filter(|href| !href.is_empty())
}

// ==== The engine's edits on the model ====

/// The editor host in the window whose callback runs, if it is there.
pub fn host_node(info: &CallbackInfo) -> Option<DomNodeId> {
    let dom = info.get_hit_node().dom;
    let node = info.get_node_id_by_id_attribute(dom, HOST_ID);
    (node.into_raw() != 0).then_some(DomNodeId { dom, node })
}

/// The child-index path from the host to `node` (empty: the host itself).
fn path_of(info: &CallbackInfo, host: DomNodeId, node: DomNodeId) -> Option<Vec<u32>> {
    if node == host {
        return Some(Vec::new());
    }
    info.get_node_child_index_path(host, node)
        .into_option()
        .map(|path| path.as_ref().to_vec())
}

/// The model node at `path`.
fn node_at_mut<'a>(model: &'a mut Dom, path: &[u32]) -> Option<&'a mut Dom> {
    let mut node = model;
    for &i in path {
        node = node.children.get_mut(i as usize)?;
    }
    Some(node)
}

/// Copies the text the engine edited in place into the model; `true` when something changed.
pub fn sync_text(model: &mut Dom, info: &mut CallbackInfo, host: DomNodeId) -> bool {
    let edits = info.get_unsynced_text_edits();
    let mut changed = false;
    let mut revision = 0u64;
    for edit in edits.as_ref() {
        revision = revision.max(edit.revision);
        let Some(path) = path_of(info, host, edit.node) else {
            continue;
        };
        let Some(node) = node_at_mut(model, &path) else {
            continue;
        };
        let text = Dom::create_text_do_not_use_without_block_level_wrapper(edit.text.as_str());
        if node.root.is_text_node() {
            *node = text;
        } else {
            // An element whose text the engine reports whole (a block typed into while
            // empty): its text becomes its one child.
            node.children = vec![text].into();
        }
        changed = true;
    }
    if revision > 0 {
        info.mark_text_revision_synced(revision);
    }
    if changed {
        model.fixup_children_estimated();
    }
    changed
}

/// Applies the structural edit the engine recorded (Enter, Backspace / Delete across blocks, a
/// paste, a delete over a selection) to the model with the engine's own applier and
/// acknowledges it with its inverse; `true` when the model changed (the window rebuilds).
pub fn apply_structural_edit(model: &mut Dom, info: &mut CallbackInfo, host: DomNodeId) -> bool {
    let Some(changeset) = info.get_document_edit_clone().into_option() else {
        return false;
    };
    // The node whose CHILDREN the operation edits: for a split or a merge the parent of the
    // node the resume point names (the applier reads the index from it), else the node the
    // operation names.
    let host_path = match &changeset.operation {
        DocumentOperation::SplitNode(_) | DocumentOperation::MergeNodes(_) => {
            let path = changeset.resume.node_path.as_ref();
            Some(path[..path.len().saturating_sub(1)].to_vec())
        }
        DocumentOperation::InsertChildren(op) => path_of(info, host, op.parent),
        DocumentOperation::RemoveChildren(op) => path_of(info, host, op.parent),
        DocumentOperation::ReplaceChildren(op) => path_of(info, host, op.parent),
        DocumentOperation::WrapRange(op) => path_of(info, host, op.node),
        DocumentOperation::UnwrapRange(op) => path_of(info, host, op.node),
    };
    let Some(host_path) = host_path else {
        return false;
    };
    match changeset.apply_to_dom(model, host_path).into_result() {
        Ok(applied) => {
            info.mark_document_edit_applied_with_inverse(changeset.id, applied.inverse);
            true
        }
        Err(_) => false,
    }
}

/// A resume point for an edit the app applies itself (the applier only reads its last index
/// for splits and merges, which the app never makes).
fn resume_at(path: &[u32]) -> EditResumePoint {
    EditResumePoint {
        anchor_key: 0,
        node_path: path.to_vec().into(),
        position: NodePosition {
            child_index: 0,
            text_byte: azul::option::OptionU32::None,
        },
    }
}

/// Wraps every selected piece of text in a copy of `wrapper` (`<b>`, `<i>`, `<u>`, a link) with
/// the engine's applier (`WrapRange`); `true` when there was a selection.
pub fn wrap_selection(model: &mut Dom, info: &CallbackInfo, host: DomNodeId, wrapper: &Dom) -> bool {
    let spans = info.get_document_selection();
    let mut targets: Vec<(Vec<u32>, u32, u32)> = spans
        .as_ref()
        .iter()
        .filter(|span| span.end_byte > span.start_byte)
        .filter_map(|span| {
            path_of(info, host, span.node)
                .filter(|path| !path.is_empty())
                .map(|path| (path, span.start_byte, span.end_byte))
        })
        .collect();
    // The last piece first: wrapping one splits its text node, which shifts the indices of
    // the nodes after it, never of those before.
    targets.sort_by(|a, b| b.0.cmp(&a.0));
    let mut changed = false;
    for (path, start, end) in targets {
        let (parent, index) = path.split_at(path.len() - 1);
        let op = DocumentOperation::WrapRange(DocOpWrapRange {
            node: host,
            start: NodePosition {
                child_index: index[0],
                text_byte: Some(start).into(),
            },
            end: NodePosition {
                child_index: index[0],
                text_byte: Some(end).into(),
            },
            wrapper: wrapper.clone(),
        });
        let changeset = DocumentChangeset::create(host, op, resume_at(parent), Instant::now());
        changed |= changeset.apply_to_dom(model, parent.to_vec()).into_result().is_ok();
    }
    changed
}

/// Whether a model node is a block that holds text (where a caret stands).
fn is_text_block(node: &Dom) -> bool {
    matches!(
        node.root.node_type,
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
    )
}

/// The path of the innermost text block on `path` (where the caret is).
fn block_path(model: &Dom, path: &[u32]) -> Option<Vec<u32>> {
    let mut node = model;
    let mut found = None;
    for (depth, &i) in path.iter().enumerate() {
        node = node.children.as_ref().get(i as usize)?;
        if is_text_block(node) {
            found = Some(path[..=depth].to_vec());
        }
    }
    found
}

/// The path of the block the caret is in.
fn caret_block(model: &Dom, info: &CallbackInfo, host: DomNodeId) -> Option<Vec<u32>> {
    let caret = info.get_document_caret().into_option()?;
    let path = path_of(info, host, caret.node)?;
    block_path(model, &path)
}

/// A list of `ordered` kind holding `items`.
fn list_dom(ordered: bool, items: Vec<Dom>) -> Dom {
    let list = if ordered {
        Dom::create_ol()
    } else {
        Dom::create_ul()
    };
    list.with_children(items)
}

/// The toolbar's Bullets / Numbering on the caret's block: a paragraph becomes a list item; an
/// item of a list of that kind takes its whole list back to paragraphs; an item of the other
/// kind turns its list into this kind. `true` when the model changed.
pub fn toggle_list(model: &mut Dom, info: &CallbackInfo, host: DomNodeId, ordered: bool) -> bool {
    let Some(block) = caret_block(model, info, host) else {
        return false;
    };
    let Some((&index, parent_path)) = block.split_last() else {
        return false;
    };
    let index = index as usize;
    let Some(parent) = node_at_mut(model, parent_path) else {
        return false;
    };
    let parent_is_list = matches!(parent.root.node_type, NodeType::Ul | NodeType::Ol);
    let parent_ordered = matches!(parent.root.node_type, NodeType::Ol);
    if !parent_is_list {
        // A paragraph: the same content as the one item of a new list.
        let Some(slot) = parent.children.get_mut(index) else {
            return false;
        };
        let content: Vec<Dom> = slot.children.as_ref().to_vec();
        *slot = list_dom(ordered, vec![Dom::create_li().with_children(content)]);
    } else if parent_ordered != ordered {
        let items: Vec<Dom> = parent.children.as_ref().to_vec();
        *parent = list_dom(ordered, items);
    } else {
        // Back to paragraphs: the list's items replace the list in ITS parent.
        let Some((&list_index, outer_path)) = parent_path.split_last() else {
            return false;
        };
        let paragraphs: Vec<Dom> = parent
            .children
            .as_ref()
            .iter()
            .map(|item| Dom::create_p().with_children(item.children.as_ref().to_vec()))
            .collect();
        let Some(outer) = node_at_mut(model, outer_path) else {
            return false;
        };
        let mut children: Vec<Dom> = outer.children.as_ref().to_vec();
        let at = list_index as usize;
        if at >= children.len() {
            return false;
        }
        children.splice(at..=at, paragraphs);
        outer.children = children.into();
    }
    model.fixup_children_estimated();
    true
}

/// Insert Link: the selection becomes a link to `href`; with no selection, `text` (the address
/// when empty) is added as a link at the end of the caret's block. `true` when the model changed.
pub fn insert_link(model: &mut Dom, info: &CallbackInfo, host: DomNodeId, href: &str, text: &str) -> bool {
    if wrap_selection(model, info, host, &link_dom(href)) {
        return true;
    }
    let Some(block) = caret_block(model, info, host).or_else(|| {
        // No caret: the end of the first block.
        model
            .children
            .as_ref()
            .first()
            .map(|_| vec![0u32])
    }) else {
        return false;
    };
    let Some(node) = node_at_mut(model, &block) else {
        return false;
    };
    let label = if text.trim().is_empty() { href } else { text };
    node.add_child(run_dom(&Run {
        text: label.to_string(),
        link: Some(href.to_string()),
        ..Run::default()
    }));
    model.fixup_children_estimated();
    true
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
