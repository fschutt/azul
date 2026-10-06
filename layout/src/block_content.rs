//! What a caret indexes, and the byte offsets into a block's text.
//!
//! A `TextCursor` is minted by the shaped layout: its `source_run` numbers the
//! runs `solver3::fc` built for the block, and `fc` puts generated content - a
//! list item's `::marker` - IN FRONT of the block's own text. The edit model
//! (`LayoutWindow::get_text_before_textinput`, the DOM's text through the
//! content overlay) holds the text alone. A reader that indexes the one with a
//! caret from the other is one run off in every list item.
//!
//! [`BlockContent`] is the vector a caret may index: the block's text behind
//! the items the layout numbers first. The byte offsets the outside world
//! speaks - the app's `DocumentPosition`, the IME's `selectedRange`, an
//! accessibility text selection - are [`FlatByte`]s into the block's FLAT
//! text (`overlay::flatten_inline_content` of the text alone, the generated
//! items left out), and [`BlockContent::flat_byte_of`] /
//! [`BlockContent::caret_at`] are the one converter pair between the two:
//! affinity resolved, in logical order.
//!
//! A node that holds several blocks - an editing host with paragraphs -
//! speaks [`FlatByte`]s into its [`ScopeText`]: its blocks' flat texts in
//! document order, one `'\n'` between two of them. That is the text a screen
//! reader reads as the host's value and sets its selection in.

use alloc::collections::BTreeSet;

use azul_core::{
    dom::{DomNodeId, NodeId},
    selection::{
        CursorAffinity, GraphemeClusterId, Selection, SelectionRange, TextBlock, TextCursor,
    },
    styled_dom::NodeHierarchyItemId,
};

use crate::{
    solver3::layout_tree::LayoutNodeId,
    text3::cache::InlineContent,
    text_block::{enclosing_block, BlockFilter},
    window::LayoutWindow,
};

/// A byte offset into a FLAT text: a block's ([`BlockContent::flat_text`]) or
/// a node's that holds several blocks. What the app, the IME and a screen
/// reader speak - never a run index and a byte inside that run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct FlatByte(pub usize);

/// A byte offset inside ONE run's text, with the caret's affinity resolved: a
/// `Trailing` caret is after its grapheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct RunByte(pub usize);

/// How many bytes `item` contributes to a flat text - the accounting of
/// `overlay::flatten_inline_content`, which every flat text is made by.
#[must_use]
pub fn flat_len_of(item: &InlineContent) -> usize {
    match item {
        InlineContent::Text(run) => run.text.len(),
        InlineContent::Space(_) | InlineContent::LineBreak(_) | InlineContent::Tab { .. } => 1,
        InlineContent::Ruby { base, .. } => base.iter().map(flat_len_of).sum(),
        InlineContent::Marker { run, .. } => run.text.len(),
        InlineContent::Image(_) | InlineContent::Shape(_) => 0,
    }
}

/// The content a caret in one text block indexes: the block's text behind
/// the items the layout generates in front of it (a list item's `::marker`),
/// numbered as the layout numbers them.
#[derive(Debug, Clone, Default)]
pub struct BlockContent {
    items: Vec<InlineContent>,
    generated: usize,
}

impl BlockContent {
    /// `items`, whose first `generated` are generated content.
    #[must_use]
    pub fn new(items: Vec<InlineContent>, generated: usize) -> Self {
        let generated = generated.min(items.len());
        Self { items, generated }
    }

    /// Every item, in the carets' numbering.
    #[must_use]
    pub fn items(&self) -> &[InlineContent] {
        &self.items
    }

    /// How many items in front of the text are generated content.
    #[must_use]
    pub const fn generated(&self) -> usize {
        self.generated
    }

    /// The block's own text: the items behind the generated ones.
    #[must_use]
    pub fn text(&self) -> &[InlineContent] {
        &self.items[self.generated..]
    }

    /// The items and the generated count, for an edit that splices the items
    /// and stores only the text.
    #[must_use]
    pub fn into_parts(self) -> (Vec<InlineContent>, usize) {
        (self.items, self.generated)
    }

    /// The block's flat text - what the app, the IME and a screen reader read.
    #[must_use]
    pub fn flat_text(&self) -> String {
        crate::overlay::flatten_inline_content(self.text())
    }

    /// The length of [`Self::flat_text`].
    #[must_use]
    pub fn flat_len(&self) -> usize {
        self.text().iter().map(flat_len_of).sum()
    }

    /// The part of the flat text laid out from the DOM nodes `holds`
    /// accepts: from the start of the first text run whose source node it
    /// accepts to the end of the last. `None` when it accepts none.
    #[must_use]
    pub fn flat_window_of(&self, holds: impl Fn(NodeId) -> bool) -> Option<(FlatByte, FlatByte)> {
        let mut acc = 0usize;
        let mut window: Option<(usize, usize)> = None;
        for item in self.text() {
            let len = flat_len_of(item);
            if let InlineContent::Text(run) = item {
                if run.source_node_id.is_some_and(&holds) {
                    let lo = window.map_or(acc, |(lo, _)| lo);
                    window = Some((lo, acc + len));
                }
            }
            acc += len;
        }
        window.map(|(lo, hi)| (FlatByte(lo), FlatByte(hi)))
    }

    /// The item `cursor`'s run is, and the byte inside it the caret stands
    /// at - when that run is text.
    #[must_use]
    pub fn run_byte_of(&self, cursor: &TextCursor) -> Option<(usize, RunByte)> {
        let run = cursor.cluster_id.source_run as usize;
        match self.items.get(run)? {
            InlineContent::Text(t) => Some((
                run,
                RunByte(
                    crate::text3::edit::cursor_byte_offset_in_run(&t.text, cursor)
                        .min(t.text.len()),
                ),
            )),
            _ => None,
        }
    }

    /// The range over bytes `start..end` of text run `run`, shaped like a
    /// word range (`text3::selection::select_word_at_cursor`): `Leading` on
    /// the grapheme that begins at `start`, `Trailing` on the last grapheme
    /// that begins before `end`. An empty span is a caret at `start`. `None`
    /// when `run` is not a text run.
    #[must_use]
    pub fn run_range(&self, run: usize, start: RunByte, end: RunByte) -> Option<SelectionRange> {
        use unicode_segmentation::UnicodeSegmentation;

        let InlineContent::Text(t) = self.items.get(run)? else {
            return None;
        };
        let text: &str = &t.text;
        let first = caret_in_run(run, text, start.0);
        if end.0 <= start.0 {
            return Some(SelectionRange {
                start: first,
                end: first,
            });
        }
        let end_byte = end.0.min(text.len());
        let last = text
            .grapheme_indices(true)
            .map(|(at, _)| at)
            .take_while(|&at| at < end_byte)
            .last()
            .unwrap_or(0);
        Some(SelectionRange {
            start: first,
            end: TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: u32::try_from(run).unwrap_or(u32::MAX),
                    start_byte_in_run: u32::try_from(last).unwrap_or(u32::MAX),
                },
                affinity: CursorAffinity::Trailing,
            },
        })
    }

    /// Where `cursor` stands in the block's flat text.
    ///
    /// A caret on a generated item stands before the text; a caret on an item
    /// that is not text (a line break, an image) before it, or with `Trailing`
    /// after it; a caret past the content (a stale one) at the end.
    #[must_use]
    pub fn flat_byte_of(&self, cursor: &TextCursor) -> FlatByte {
        let run = cursor.cluster_id.source_run as usize;
        if run < self.generated {
            return FlatByte(0);
        }
        let mut acc = 0usize;
        for (i, item) in self.items.iter().enumerate().skip(self.generated) {
            if i == run {
                let inside = match item {
                    InlineContent::Text(t) => {
                        crate::text3::edit::cursor_byte_offset_in_run(&t.text, cursor)
                            .min(t.text.len())
                    }
                    other if cursor.affinity == CursorAffinity::Trailing => flat_len_of(other),
                    _ => 0,
                };
                return FlatByte(acc + inside);
            }
            acc += flat_len_of(item);
        }
        FlatByte(acc)
    }

    /// The caret at `at` in the block's flat text - the inverse of
    /// [`Self::flat_byte_of`].
    ///
    /// Inside a text run: `Leading` on the grapheme that begins there (an
    /// offset inside a grapheme snaps back to its start). At the end of a run
    /// that no text run follows directly: `Trailing` on its last grapheme -
    /// the end of a line before a break, the end of the block. Past the end:
    /// the end. On a blank block, the one position it has: offset 0 of its
    /// first text run, or - with no text item at all, an empty editable - of
    /// the run its first keystroke will be seeded as (the one a blank line's
    /// caret names). Always `Some`; an `Option` so a caller may refuse.
    #[must_use]
    pub fn caret_at(&self, at: FlatByte) -> Option<TextCursor> {
        let want = at.0;
        let mut acc = 0usize;
        let mut last_end: Option<TextCursor> = None;
        let mut first_run: Option<usize> = None;
        for i in self.generated..self.items.len() {
            let item = &self.items[i];
            let len = flat_len_of(item);
            if let InlineContent::Text(t) = item {
                if first_run.is_none() {
                    first_run = Some(i);
                }
                let text_follows = matches!(
                    self.items.get(i + 1),
                    Some(InlineContent::Text(next)) if !next.text.is_empty()
                );
                if want < acc + len || (want == acc + len && !text_follows) {
                    return Some(caret_in_run(i, &t.text, want.saturating_sub(acc)));
                }
                if len > 0 {
                    last_end = Some(caret_in_run(i, &t.text, len));
                }
            }
            acc += len;
        }
        Some(last_end.unwrap_or_else(|| {
            caret_in_run(first_run.unwrap_or(self.generated), "", 0)
        }))
    }

    /// `cursor`, moved off a generated item: a caret the layout minted ON the
    /// marker (a click on it) stands at the start of the block's own text, so
    /// nothing typed or deleted goes into the marker.
    #[must_use]
    pub fn past_generated(cursor: TextCursor, generated: usize) -> TextCursor {
        let first_text = u32::try_from(generated).unwrap_or(u32::MAX);
        if cursor.cluster_id.source_run < first_text {
            TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: first_text,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Leading,
            }
        } else {
            cursor
        }
    }

    /// [`Self::past_generated`] for every end of every selection.
    #[must_use]
    pub fn selections_past_generated(selections: Vec<Selection>, generated: usize) -> Vec<Selection> {
        if generated == 0 {
            return selections;
        }
        selections
            .into_iter()
            .map(|sel| match sel {
                Selection::Cursor(c) => Selection::Cursor(Self::past_generated(c, generated)),
                Selection::Range(r) => Selection::Range(SelectionRange {
                    start: Self::past_generated(r.start, generated),
                    end: Self::past_generated(r.end, generated),
                }),
            })
            .collect()
    }
}

/// The caret at byte `byte` of run `run`, whose text is `text`: `Leading` on
/// the grapheme that begins at (or contains) `byte`, `Trailing` on the last
/// grapheme at or past the end.
fn caret_in_run(run: usize, text: &str, byte: usize) -> TextCursor {
    use unicode_segmentation::UnicodeSegmentation;

    let at = |start: usize, affinity: CursorAffinity| TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: u32::try_from(run).unwrap_or(u32::MAX),
            start_byte_in_run: u32::try_from(start).unwrap_or(u32::MAX),
        },
        affinity,
    };
    if byte >= text.len() {
        return match text.grapheme_indices(true).next_back() {
            Some((start, _)) => at(start, CursorAffinity::Trailing),
            None => at(0, CursorAffinity::Leading),
        };
    }
    let start = text
        .grapheme_indices(true)
        .map(|(start, _)| start)
        .take_while(|&start| start <= byte)
        .last()
        .unwrap_or(0);
    at(start, CursorAffinity::Leading)
}

/// The flat text of a node that may hold several text blocks, and where each
/// block's text stands in it: the blocks' flat texts in document order, one
/// `'\n'` between two of them. A node whose own text is in ONE block (a
/// paragraph, a flat editable, a text leaf) reads that block's text alone -
/// and a node INSIDE a block (an inline editing host,
/// `<p>Name: <span contenteditable>Bob</span></p>`) only its own part of it.
/// A block nested in another (an inline-block's, inside its paragraph) is
/// read where its text stands in the other's, not after it.
///
/// What a screen reader reads as a host's value and indexes its selection in:
/// the offsets the accessibility tree publishes and the ones a
/// `SetTextSelection` brings back are one space, and an offset in the
/// host's second paragraph names the second paragraph.
#[derive(Debug, Clone, Default)]
pub struct ScopeText {
    text: String,
    blocks: Vec<ScopeEntry>,
}

/// One block's part of a [`ScopeText`].
#[derive(Debug, Clone)]
struct ScopeEntry {
    block: TextBlock,
    /// Where the text of `lo..hi` starts in the scope's text.
    start: FlatByte,
    content: BlockContent,
    /// The part of the block's flat text the scope reads (byte offsets into
    /// [`BlockContent::flat_text`]): all of it, or an inline host's own.
    lo: usize,
    hi: usize,
    /// A block nested in another of the scope's: its text stands inside
    /// that block's, at `start`.
    nested: bool,
}

impl ScopeText {
    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The blocks: the ones read one after the other in document order,
    /// then the ones nested in them.
    pub fn blocks(&self) -> impl Iterator<Item = TextBlock> + '_ {
        self.blocks.iter().map(|entry| entry.block)
    }

    /// Read `lo..hi` of `block`'s flat text after the text so far, one
    /// line break after the block before it.
    fn push_block(&mut self, block: TextBlock, content: BlockContent, lo: usize, hi: usize) {
        if self.blocks.iter().any(|entry| !entry.nested) {
            self.text.push('\n');
        }
        let start = FlatByte(self.text.len());
        self.text
            .push_str(content.flat_text().get(lo..hi).unwrap_or_default());
        self.blocks.push(ScopeEntry {
            block,
            start,
            content,
            lo,
            hi,
            nested: false,
        });
    }

    /// Where `cursor`, a caret in `block`, stands in the text. `None` when
    /// `block` is not one of this scope's.
    #[must_use]
    pub fn flat_byte_of(&self, block: TextBlock, cursor: &TextCursor) -> Option<FlatByte> {
        let entry = self.blocks.iter().find(|entry| entry.block == block)?;
        let at = entry
            .content
            .flat_byte_of(cursor)
            .0
            .clamp(entry.lo, entry.hi.max(entry.lo));
        Some(FlatByte(entry.start.0 + at - entry.lo))
    }

    /// The block and the caret at `at` in the text. Strictly inside a
    /// nested block's text, in that block; otherwise in the block read at
    /// `at` - the line break between two blocks is the end of the first,
    /// past the end is the end of the last. `None` for a scope that holds
    /// no block.
    #[must_use]
    pub fn caret_at(&self, at: FlatByte) -> Option<(TextBlock, TextCursor)> {
        // The innermost: nested blocks come in document order.
        let nested = self
            .blocks
            .iter()
            .filter(|entry| {
                entry.nested
                    && entry.start.0 < at.0
                    && at.0 < entry.start.0 + entry.hi.saturating_sub(entry.lo)
            })
            .last();
        let entry = match nested {
            Some(entry) => entry,
            None => {
                let mut read = self.blocks.iter().filter(|entry| !entry.nested);
                let mut chosen = read.next()?;
                for entry in read {
                    if entry.start.0 > at.0 {
                        break;
                    }
                    chosen = entry;
                }
                chosen
            }
        };
        let local = (at.0.saturating_sub(entry.start.0) + entry.lo).min(entry.hi.max(entry.lo));
        let caret = entry.content.caret_at(FlatByte(local))?;
        Some((entry.block, caret))
    }

    /// The [`FlatByte`] of CHARACTER `index` (what accessibility speaks); past
    /// the end, the end.
    #[must_use]
    pub fn flat_byte_at_char(&self, index: usize) -> FlatByte {
        FlatByte(
            self.text
                .char_indices()
                .nth(index)
                .map_or(self.text.len(), |(byte, _)| byte),
        )
    }
}

/// A selection as a screen reader sees it: the node it is read on, that
/// node's [`ScopeText`], and the anchor and the focus in it
/// ([`LayoutWindow::accessible_selection`]).
#[derive(Debug, Clone)]
pub struct AccessibleSelection {
    /// The node the text and the selection are published on: the session's
    /// editing host, else the element of the session's block.
    pub node: DomNodeId,
    /// Whether `node` is an editing host (the text is its value).
    pub is_host: bool,
    /// `node`'s text.
    pub text: ScopeText,
    /// The anchor, in `text`.
    pub anchor: FlatByte,
    /// The focus, in `text`.
    pub focus: FlatByte,
}

impl LayoutWindow {
    /// The content a caret in `block` indexes ([`BlockContent`]).
    ///
    /// An element's: its text (overlay first) behind the layout's generated
    /// items ([`Self::element_content`]). An anonymous block has no element
    /// whose text an edit could change: its content is the layout's own
    /// collection, which is what its carets were minted from.
    #[must_use]
    pub fn block_content(&self, block: TextBlock) -> BlockContent {
        if let Some(element) = block.element() {
            return self.element_content(block.dom(), element);
        }
        let items = self
            .layout_results
            .get(&block.dom())
            .and_then(|lr| {
                let tree = &lr.layout_tree;
                let root = tree.text_block_root(block.key())?;
                tree.warm(LayoutNodeId::new(root))
                    .and_then(|w| w.inline_content_cache.as_deref())
                    .map(|cache| cache.content.to_vec())
            })
            .unwrap_or_default();
        let generated = items
            .iter()
            .take_while(|item| matches!(item, InlineContent::Marker { .. }))
            .count();
        BlockContent::new(items, generated)
    }

    /// `node`'s [`ScopeText`]: its own text in the block it is in, when it
    /// is in one (all of the block's for the block's element, an inline
    /// host's own part of it); else every block inside it, in document
    /// order. A block nested in one of those is read where its text stands.
    #[must_use]
    pub fn scope_text(&self, node: DomNodeId) -> ScopeText {
        let mut scope = ScopeText::default();
        let Some(node_id) = node.node.into_crate_internal() else {
            return scope;
        };
        let Some(tree) = self.layout_results.get(&node.dom).map(|lr| &lr.layout_tree) else {
            return scope;
        };
        let inside = |of: NodeId| move |n: NodeId| self.node_is_self_or_descendant(node.dom, n, of);
        let within = self.text_block_roots(
            node.dom,
            BlockFilter {
                within: Some(node),
                ..BlockFilter::ALL
            },
        );
        let own = self.text_block_of(node);
        // The blocks read one after the other, with their layout nodes.
        let read: Vec<(TextBlock, Option<usize>)> = match own {
            Some(block) => vec![(
                block,
                self.text_block_layout_index(block).map(LayoutNodeId::index),
            )],
            None => {
                let roots: BTreeSet<usize> = within.iter().map(|(_, idx)| idx.index()).collect();
                within
                    .iter()
                    .filter(|(_, idx)| {
                        enclosing_block(tree, idx.index(), |p| roots.contains(&p)).is_none()
                    })
                    .map(|(block, idx)| (*block, Some(idx.index())))
                    .collect()
            }
        };
        // (layout node, entry) of every block placed so far.
        let mut placed: Vec<(usize, usize)> = Vec::new();
        for (block, index) in read {
            let content = self.block_content(block);
            let (lo, hi) = if own.is_some() && block.element() != Some(node_id) {
                // An inline host's own text; none laid out: an empty value.
                content
                    .flat_window_of(inside(node_id))
                    .map_or((0, 0), |(lo, hi)| (lo.0, hi.0))
            } else {
                (0, content.flat_len())
            };
            if let Some(index) = index {
                placed.push((index, scope.blocks.len()));
            }
            scope.push_block(block, content, lo, hi);
        }
        // Blocks nested in those, where their text stands in the outer one's
        // content; one whose text is not in it is read after the rest.
        for (block, idx) in &within {
            if scope.blocks.iter().any(|entry| entry.block == *block) {
                continue;
            }
            let outer = enclosing_block(tree, idx.index(), |p| placed.iter().any(|&(i, _)| i == p))
                .and_then(|p| placed.iter().find(|&&(i, _)| i == p).map(|&(_, e)| e));
            let content = self.block_content(*block);
            let len = content.flat_len();
            let start = match (outer, block.element()) {
                (Some(e), Some(element)) => scope.blocks.get(e).and_then(|outer| {
                    let (lo, hi) = outer.content.flat_window_of(inside(element))?;
                    (lo.0 >= outer.lo && hi.0 <= outer.hi)
                        .then(|| FlatByte(outer.start.0 + lo.0 - outer.lo))
                }),
                _ => None,
            };
            placed.push((idx.index(), scope.blocks.len()));
            match start {
                Some(start) => scope.blocks.push(ScopeEntry {
                    block: *block,
                    start,
                    content,
                    lo: 0,
                    hi: len,
                    nested: true,
                }),
                None => scope.push_block(*block, content, 0, len),
            }
        }
        scope
    }

    /// The editing session's selection as a screen reader reads it
    /// ([`AccessibleSelection`]): on the session's editing host - the text
    /// field, the document - with the host's [`ScopeText`], or on the
    /// block's own element outside any host. A document selection has its
    /// two ends in two blocks of that text; one that does not lie in it is
    /// read as the session's own selection. `None` with no session.
    ///
    /// The tree published the caret on the session's BLOCK with the raw
    /// `start_byte_in_run` of its cluster - no run, no affinity - and a host
    /// with paragraphs published neither a value nor a selection.
    #[must_use]
    pub fn accessible_selection(&self) -> Option<AccessibleSelection> {
        let mc = self.text_edit_manager.multi_cursor.as_ref()?;
        let block = mc.block;
        // The host of the caret's own text: an inline host
        // (`<p>Name: <span contenteditable>`) lies INSIDE the paragraph that
        // is the caret's block, and asked of the block it was never found.
        let caret_node = mc
            .get_primary_cursor()
            .and_then(|cursor| self.caret_text_node(block, cursor))
            .map(|n| DomNodeId {
                dom: block.dom(),
                node: NodeHierarchyItemId::from_crate_internal(Some(n)),
            });
        let host = caret_node
            .and_then(|n| self.find_contenteditable_host(n))
            .or_else(|| self.find_contenteditable_host(block.container_dom_node()));
        let node = host.map_or_else(
            || block.container_dom_node(),
            crate::text_block::EditHost::dom_node,
        );
        let text = self.scope_text(node);
        let document = self
            .text_edit_manager
            .get_cross_block_selection()
            .and_then(|cb| {
                Some((
                    text.flat_byte_of(cb.anchor.block, &cb.anchor.cursor)?,
                    text.flat_byte_of(cb.focus.block, &cb.focus.cursor)?,
                ))
            });
        let (anchor, focus) = match document {
            Some(ends) => ends,
            None => {
                let (a, f) = match mc.get_primary()?.selection {
                    Selection::Cursor(c) => (c, c),
                    Selection::Range(r) => (r.start, r.end),
                };
                (text.flat_byte_of(block, &a)?, text.flat_byte_of(block, &f)?)
            }
        };
        Some(AccessibleSelection {
            node,
            is_host: host.is_some(),
            text,
            anchor,
            focus,
        })
    }
}
