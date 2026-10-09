//! Text blocks as DOM subtrees again.
//!
//! An edit of a block's TEXT lives in the content overlay as styled runs
//! (`text3::cache::StyledRun`: the text, its style, the DOM text node it came
//! from). An edit of the document's STRUCTURE - a document selection deleted
//! and its two ends joined, a rich paste - is recorded as a
//! `DocumentChangeset` whose payload is a `Dom` subtree the app applies
//! (`LayoutWindow::replace_cross_block_selection`). This module is the one
//! way between the two: a block's runs rebuilt into the inline element tree
//! they were laid out from - every run inside clones of the elements between
//! its text node and its block (`<b>`, `<a href>`, `<span class>`), adjacent
//! runs sharing them, a run whose style the typing changed wrapped in the
//! elements that make the difference (`<b>`, `<i>`, `<u>`, `<s>`, or a span
//! unsetting what it lost).
//!
//! Flattening them to one text run (what the first cut of the
//! cross-block delete did) lost every `<b>` of the two blocks it joined.

use alloc::{string::String, sync::Arc, vec::Vec};

use azul_core::{
    dom::{Dom, DomId, NodeData, NodeId, NodeType},
    selection::{CursorAffinity, TextCursor},
};

use crate::{
    block_content::BlockContent,
    text3::{
        cache::{InlineContent, StyleProperties, StyledRun},
        edit::{cursor_byte_offset_in_run, FormatOverrides},
    },
    window::LayoutWindow,
};

/// One node of an [`InlineTree`].
#[derive(Debug, Clone)]
enum InlineNode {
    /// An element: a clone of the DOM element `source` (its children left
    /// out), or - `source: None` - one made up to carry a format.
    Element {
        source: Option<NodeId>,
        // Boxed: a `NodeData` is hundreds of bytes, a text or break node a few.
        data: Box<NodeData>,
        children: Vec<InlineNode>,
    },
    Text(String),
    Break,
}

/// Where a position stands in an [`InlineTree`]: inside a text node (the
/// child-index path to it and the byte in it), or between the children of
/// an element (the path to the element and the child index).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlinePosition {
    InText { path: Vec<u32>, byte: u32 },
    Before { path: Vec<u32>, index: u32 },
}

/// A block's inline content as it is being rebuilt: runs pushed one after
/// the other, each inside its element chain; [`Self::end`] names the point
/// right after what was pushed last - the caret after a join.
#[derive(Debug, Clone, Default)]
pub struct InlineTree {
    nodes: Vec<InlineNode>,
    end: Option<InlinePosition>,
}

/// One element of a run's chain: the DOM element it clones, or a made-up one.
#[derive(Debug, Clone)]
pub struct ChainLink {
    pub source: Option<NodeId>,
    pub data: NodeData,
}

impl InlineTree {
    /// Nothing pushed yet.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The point right after what was pushed last; `None` before anything
    /// was.
    #[must_use]
    pub fn end(&self) -> Option<InlinePosition> {
        self.end.clone()
    }

    /// Push `text` inside the elements of `chain`, outermost first. The
    /// elements already open at the end of the tree are shared with the
    /// chain as far as the two agree (the same DOM element, or the same
    /// made-up format element), and text joins text.
    pub fn push_text(&mut self, chain: &[ChainLink], text: &str) {
        if text.is_empty() {
            return;
        }
        let mut path: Vec<u32> = Vec::new();
        let mut level = &mut self.nodes;
        for link in chain {
            let shared = matches!(
                level.last(),
                Some(InlineNode::Element { source, data, .. })
                    if match (source, link.source) {
                        (Some(a), Some(b)) => *a == b,
                        (None, None) => {
                            data.get_node_type() == link.data.get_node_type()
                                && !matches!(data.get_node_type(), NodeType::Span)
                        }
                        _ => false,
                    }
            );
            if !shared {
                level.push(InlineNode::Element {
                    source: link.source,
                    data: Box::new(link.data.clone()),
                    children: Vec::new(),
                });
            }
            path.push(u32::try_from(level.len() - 1).unwrap_or(u32::MAX));
            level = match level.last_mut() {
                Some(InlineNode::Element { children, .. }) => children,
                // Just pushed or matched as an element above.
                _ => return,
            };
        }
        let byte = if let Some(InlineNode::Text(existing)) = level.last_mut() {
            existing.push_str(text);
            existing.len()
        } else {
            level.push(InlineNode::Text(String::from(text)));
            text.len()
        };
        path.push(u32::try_from(level.len() - 1).unwrap_or(u32::MAX));
        self.end = Some(InlinePosition::InText {
            path,
            byte: u32::try_from(byte).unwrap_or(u32::MAX),
        });
    }

    /// Push a hard line break (`<br>`), at the block's own level.
    pub fn push_break(&mut self) {
        self.nodes.push(InlineNode::Break);
        self.end = Some(InlinePosition::Before {
            path: Vec::new(),
            index: u32::try_from(self.nodes.len()).unwrap_or(u32::MAX),
        });
    }

    /// Push `dom` - a subtree of a fragment (a rich paste) - as it is.
    pub fn push_dom(&mut self, dom: &Dom) {
        let node = Self::node_of_dom(dom);
        self.nodes.push(node);
        self.end = Some(InlinePosition::Before {
            path: Vec::new(),
            index: u32::try_from(self.nodes.len()).unwrap_or(u32::MAX),
        });
        // Inside the last text of what was pushed, when there is one: the
        // caret after a paste stands at the end of its text.
        let mut path = vec![u32::try_from(self.nodes.len() - 1).unwrap_or(u32::MAX)];
        let mut node = self.nodes.last();
        while let Some(InlineNode::Element { children, .. }) = node {
            match children.last() {
                Some(last) => {
                    path.push(u32::try_from(children.len() - 1).unwrap_or(u32::MAX));
                    node = Some(last);
                }
                None => return,
            }
        }
        if let Some(InlineNode::Text(text)) = node {
            self.end = Some(InlinePosition::InText {
                path,
                byte: u32::try_from(text.len()).unwrap_or(u32::MAX),
            });
        }
    }

    fn node_of_dom(dom: &Dom) -> InlineNode {
        match dom.root.get_node_type() {
            NodeType::Text(t) => InlineNode::Text(String::from(t.as_str())),
            NodeType::Br => InlineNode::Break,
            _ => InlineNode::Element {
                source: None,
                data: Box::new(dom.root.clone()),
                children: dom
                    .children
                    .as_ref()
                    .iter()
                    .map(Self::node_of_dom)
                    .collect(),
            },
        }
    }

    /// The tree as `Dom` subtrees, the block's children.
    #[must_use]
    pub fn into_doms(self) -> Vec<Dom> {
        self.nodes.into_iter().map(Self::dom_of_node).collect()
    }

    fn dom_of_node(node: InlineNode) -> Dom {
        match node {
            InlineNode::Text(text) => Dom::create_text_do_not_use_without_block_level_wrapper(text),
            InlineNode::Break => Dom::create_br(),
            InlineNode::Element { data, children, .. } => {
                let mut dom = Dom {
                    root: *data,
                    children: Vec::new().into(),
                    css: Vec::new().into(),
                    estimated_total_children: 0,
                };
                for child in children {
                    dom.add_child(Self::dom_of_node(child));
                }
                dom
            }
        }
    }
}

impl LayoutWindow {
    /// The items block `block` keeps on one side of a cut at `cut` - a caret
    /// in the block's own numbering ([`Self::element_content`]): everything
    /// before it (`head`) or after it. Text runs are cut at the caret; the
    /// cut run's head piece is kept even when empty, so text put in at the
    /// cut can take its style. Generated items (a list item's marker) are
    /// no part of either side.
    #[must_use]
    pub fn kept_block_items(
        &self,
        dom_id: DomId,
        block: NodeId,
        cut: &TextCursor,
        head: bool,
    ) -> Vec<InlineContent> {
        let (items, generated) = self.element_content(dom_id, block).into_parts();
        let cut = BlockContent::past_generated(*cut, generated);
        let cut_run = cut.cluster_id.source_run as usize;
        let mut out = Vec::new();
        for (i, item) in items.into_iter().enumerate().skip(generated) {
            if i == cut_run {
                match item {
                    InlineContent::Text(mut run) => {
                        let byte = cursor_byte_offset_in_run(&run.text, &cut).min(run.text.len());
                        let kept: Arc<str> = if head {
                            Arc::from(&run.text[..byte])
                        } else {
                            Arc::from(&run.text[byte..])
                        };
                        run.text = kept;
                        out.push(InlineContent::Text(run));
                    }
                    // A caret on an item that is not text stands before it,
                    // or with `Trailing` after it.
                    other => {
                        if head == (cut.affinity == CursorAffinity::Trailing) {
                            out.push(other);
                        }
                    }
                }
            } else if (i < cut_run) == head {
                out.push(item);
            }
        }
        out
    }

    /// Push the `items` of block `block` onto `tree` (see the module docs):
    /// each text run in clones of the elements between its text node and
    /// the block, plus the format elements its style gained or lost against
    /// that node's own style; a line break as `<br>`.
    pub fn push_block_items(
        &self,
        tree: &mut InlineTree,
        dom_id: DomId,
        block: NodeId,
        items: &[InlineContent],
    ) {
        for item in items {
            match item {
                InlineContent::Text(run) => {
                    let chain = self.run_chain(dom_id, block, run);
                    tree.push_text(&chain, &run.text);
                }
                InlineContent::LineBreak(_) => tree.push_break(),
                _ => {}
            }
        }
    }

    /// The elements `run` is laid out inside, outermost first: the DOM
    /// elements between its text node and `block`, then the format elements
    /// that make the difference between the text node's own style and the
    /// run's (typing changed it).
    fn run_chain(&self, dom_id: DomId, block: NodeId, run: &StyledRun) -> Vec<ChainLink> {
        let mut chain = Vec::new();
        let Some(lr) = self.layout_results.get(&dom_id) else {
            return chain;
        };
        let node_data = lr.styled_dom.node_data.as_container();
        let hierarchy = lr.styled_dom.node_hierarchy.as_container();
        let natural = match run.source_node_id {
            Some(text_node) => {
                // The text node's ancestors below the block; none when it is
                // not inside the block at all.
                let mut up: Vec<NodeId> = Vec::new();
                let mut current = hierarchy.get(text_node).and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
                let mut inside = false;
                while let Some(n) = current {
                    if n == block {
                        inside = true;
                        break;
                    }
                    up.push(n);
                    current = hierarchy.get(n).and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
                }
                if inside {
                    for n in up.into_iter().rev() {
                        if let Some(data) = node_data.get(n) {
                            chain.push(ChainLink {
                                source: Some(n),
                                data: data.clone(),
                            });
                        }
                    }
                }
                self.get_text_style_for_node(dom_id, text_node)
            }
            None => self.get_text_style_for_node(dom_id, block),
        };
        chain.extend(format_links(&natural, &run.style));
        chain
    }
}

/// The made-up elements that turn text in `natural` style into text in
/// `style`, for the four formats an editor toggles: `<b>` / `<i>` / `<u>` /
/// `<s>` for what it gained, one `<span>` unsetting what it lost.
fn format_links(natural: &StyleProperties, style: &StyleProperties) -> Vec<ChainLink> {
    use azul_core::events::TextFormat;

    let made = |node_type: NodeType| ChainLink {
        source: None,
        data: NodeData::create_node(node_type),
    };
    let has = |s: &StyleProperties, f: TextFormat| FormatOverrides::style_has(s, f);
    let mut links = Vec::new();
    let mut unset = String::new();
    if has(style, TextFormat::Bold) && !has(natural, TextFormat::Bold) {
        links.push(made(NodeType::B));
    } else if !has(style, TextFormat::Bold) && has(natural, TextFormat::Bold) {
        unset.push_str("font-weight: normal;");
    }
    if has(style, TextFormat::Italic) && !has(natural, TextFormat::Italic) {
        links.push(made(NodeType::I));
    } else if !has(style, TextFormat::Italic) && has(natural, TextFormat::Italic) {
        unset.push_str("font-style: normal;");
    }
    let (u, s) = (
        has(style, TextFormat::Underline),
        has(style, TextFormat::Strikethrough),
    );
    let (nu, ns) = (
        has(natural, TextFormat::Underline),
        has(natural, TextFormat::Strikethrough),
    );
    if (nu && !u) || (ns && !s) {
        // A decoration went: the span says which ones are left.
        unset.push_str(match (u, s) {
            (true, true) => "text-decoration: underline line-through;",
            (true, false) => "text-decoration: underline;",
            (false, true) => "text-decoration: line-through;",
            (false, false) => "text-decoration: none;",
        });
    } else {
        if u && !nu {
            links.push(made(NodeType::U));
        }
        if s && !ns {
            links.push(made(NodeType::S));
        }
    }
    if !unset.is_empty() {
        let mut span = NodeData::create_node(NodeType::Span);
        span.set_css(&unset);
        links.insert(
            0,
            ChainLink {
                source: None,
                data: span,
            },
        );
    }
    links
}

impl InlinePosition {
    /// This position as a resume point below the block the tree is rebuilt
    /// for: the child-index path from the block to the node the position is
    /// in, and the position inside that node.
    #[must_use]
    pub fn as_resume(&self) -> (Vec<u32>, crate::managers::changeset::NodePosition) {
        use crate::managers::changeset::NodePosition;
        match self {
            Self::InText { path, byte } => {
                let (last, parent) = path.split_last().map_or((0, &[][..]), |(l, p)| (*l, p));
                (parent.to_vec(), NodePosition::in_text_child(last, *byte))
            }
            Self::Before { path, index } => (path.clone(), NodePosition::before_child(*index)),
        }
    }
}
