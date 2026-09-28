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

use azul_core::selection::{
    CursorAffinity, GraphemeClusterId, Selection, SelectionRange, TextBlock, TextCursor,
};

use crate::{
    solver3::layout_tree::LayoutNodeId, text3::cache::InlineContent, window::LayoutWindow,
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
    /// first text run. `None` for a block with no text item at all.
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
        last_end.or_else(|| first_run.map(|run| caret_in_run(run, "", 0)))
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
                    .map(|cache| cache.content.clone())
            })
            .unwrap_or_default();
        let generated = items
            .iter()
            .take_while(|item| matches!(item, InlineContent::Marker { .. }))
            .count();
        BlockContent::new(items, generated)
    }
}
