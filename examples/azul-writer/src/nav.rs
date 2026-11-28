//! What the sidebar's own sections list and how its rows are told apart: the OUTLINE (the
//! headings of the shown document), the PAGES (of the print layout) and the STYLES. Plain
//! planning over the document, no widgets: the tags of the rows, what a tag stands for, how deep
//! a heading stands. `ui_sidebar` turns it into the sidebar's rows and runs what it reports.
//!
//! The open documents' rows and their tags are azul-appkit's (`azul_appkit::docs`); everything
//! here starts at its `APP_TAG_BASE`, in ranges of their own that do not meet.

use azul::widgets::{RichBlockKind, RichTextDoc};
use azul_appkit::docs::{shown_name, APP_TAG_BASE};

/// The tag of the first style row; the n-th is `STYLE_BASE + n`.
pub const STYLE_BASE: usize = APP_TAG_BASE;
/// The tag of the first outline row.
pub const OUTLINE_BASE: usize = APP_TAG_BASE + 1_000;
/// The tag of the first page row.
pub const PAGE_BASE: usize = APP_TAG_BASE + 2_000;

/// The most headings the outline lists (a document of thousands of headings shows the first ones
/// and says how many it leaves out).
pub const MAX_OUTLINE: usize = 500;
/// The most pages the sidebar lists.
pub const MAX_PAGE_ROWS: usize = 500;

/// One heading of the document, as a row of the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    /// The heading's block in the document: what a click on the row scrolls to.
    pub block: usize,
    /// How deep the row stands: its level less the shallowest level of the document, so a
    /// document that starts at a second-level heading is not indented from the start.
    pub depth: usize,
    /// The heading's text, cut.
    pub text: String,
}

/// The outline of a document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outline {
    /// The headings listed, in the order of the document.
    pub rows: Vec<OutlineRow>,
    /// The headings the cap left out.
    pub hidden: usize,
}

/// The headings of `doc` with a text, as the sidebar's outline lists them. `untitled` is the
/// words of a heading with no text (it is listed so the outline matches the document: an empty
/// heading is still a place).
#[must_use]
pub fn outline(doc: &RichTextDoc, untitled: &str) -> Outline {
    let headings: Vec<(usize, u8, String)> = doc
        .blocks
        .as_slice()
        .iter()
        .enumerate()
        .filter_map(|(block, b)| match b.kind {
            RichBlockKind::Heading(level) => Some((block, level, b.get_text().as_str().to_string())),
            _ => None,
        })
        .collect();
    let shallowest = headings.iter().map(|(_, level, _)| *level).min().unwrap_or(1);
    let hidden = headings.len().saturating_sub(MAX_OUTLINE);
    let rows = headings
        .into_iter()
        .take(MAX_OUTLINE)
        .map(|(block, level, text)| OutlineRow {
            block,
            depth: usize::from(level.saturating_sub(shallowest)),
            text: shown_name(&text, untitled),
        })
        .collect();
    Outline { rows, hidden }
}

/// The tag of the style row `index`.
#[must_use]
pub const fn style_tag(index: usize) -> usize {
    STYLE_BASE + index
}

/// The style a tag names (an index into the ribbon's styles); `None` for any other tag.
#[must_use]
pub const fn style_of_tag(tag: usize, styles: usize) -> Option<usize> {
    match tag.checked_sub(STYLE_BASE) {
        Some(index) if index < styles && tag < OUTLINE_BASE => Some(index),
        _ => None,
    }
}

/// The tag of the outline row `index`.
#[must_use]
pub const fn outline_tag(index: usize) -> usize {
    OUTLINE_BASE + index
}

/// The outline row a tag names; `None` for any other tag.
#[must_use]
pub const fn outline_of_tag(tag: usize) -> Option<usize> {
    match tag.checked_sub(OUTLINE_BASE) {
        Some(index) if index < MAX_OUTLINE => Some(index),
        _ => None,
    }
}

/// The tag of the page row `index`.
#[must_use]
pub const fn page_tag(index: usize) -> usize {
    PAGE_BASE + index
}

/// The page a tag names; `None` for any other tag.
#[must_use]
pub const fn page_of_tag(tag: usize) -> Option<usize> {
    match tag.checked_sub(PAGE_BASE) {
        Some(index) if index < MAX_PAGE_ROWS => Some(index),
        _ => None,
    }
}

/// The block a page starts with, as the paginator knows it: page `index` of `starts`
/// (`(block, byte)` pairs, ascending); `None` past the last page.
#[must_use]
pub fn page_block(starts: &[(u32, u32)], index: usize) -> Option<usize> {
    starts.get(index).map(|(block, _)| *block as usize)
}

#[cfg(test)]
#[path = "nav_test.rs"]
mod nav_test;
