//! A4 pages: where each page of the document starts.
//!
//! The document is laid out once at the A4 text width by azul's paged
//! pipeline (`Pdf::compute_pagination`, the engine the PDF export uses), on
//! an azul `Thread`; its breaks become the FIRST BLOCK of every page (a page
//! holds whole blocks: a break inside a paragraph moves the paragraph to the
//! next page, a forced page break starts one). The canvas then shows each
//! page as one editing host of the shared editor
//! (`RichTextEditor::page_doms`). Until the pages of the current generation
//! are known the last pages are used (block indices shift by an edit or two
//! at most), so typing never waits for the layout.

use azul::{
    callbacks::{CallbackInfo, RefAny, Update},
    dom::{Dom, StyledDom},
    font::FontCacheSnapshot,
    image::ImageCacheSnapshot,
    pdf::Pdf,
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
    widgets::{RichTextDoc, RichTextEditor, RichTextEditorState},
};

use crate::AppState;

/// An A4 sheet at 96 dpi, in logical pixels.
pub const A4_W: f32 = 794.0;
pub const A4_H: f32 = 1123.0;
/// The page margin (one inch).
pub const MARGIN: f32 = 96.0;
/// The body text size at 100 % zoom.
pub const FONT_PX: f32 = 15.0;
/// The space under a paragraph at 100 % zoom.
pub const SPACING_PX: f32 = 8.0;

/// The text area of a page.
#[must_use]
pub fn content_size() -> (f32, f32) {
    (A4_W - 2.0 * MARGIN, A4_H - 2.0 * MARGIN)
}

/// The font of the paper (the sheet on screen and the print add the ink:
/// the mode's text colour on screen, black on white in the PDF). AzWriter is
/// always flora (`start` pins it), whose running text is EB Garamond: named,
/// so the paginator on its thread (no theme there) measures the face the
/// sheet shows and the PDF embeds.
pub const PAPER_TEXT_CSS: &str = "font-family: 'EB Garamond', Georgia, serif;";

/// What is known about the pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pages {
    /// The document generation the pages were computed for (0 = none yet).
    pub generation: u64,
    /// The first block of every page, ascending, starting with 0.
    pub starts: Vec<u32>,
}

impl Default for Pages {
    fn default() -> Self {
        Self {
            generation: 0,
            starts: vec![0],
        }
    }
}

impl Pages {
    /// The page starts for a document of `blocks` blocks: the known ones,
    /// cut to the document (a page past its end goes).
    #[must_use]
    pub fn starts_for(&self, blocks: usize) -> Vec<u32> {
        let mut starts: Vec<u32> = self
            .starts
            .iter()
            .copied()
            .filter(|s| (*s as usize) < blocks.max(1))
            .collect();
        if starts.first() != Some(&0) {
            starts.insert(0, 0);
        }
        starts
    }

    /// The page block `block` is on.
    #[must_use]
    pub fn page_of(&self, block: usize) -> usize {
        self.starts
            .iter()
            .rposition(|s| (*s as usize) <= block)
            .unwrap_or(0)
    }
}

/// The page starts from the break paths of a pagination (each path a
/// child-index path from the document host: `[block, ...]`). Ascending,
/// unique, within the document, starting with 0.
#[must_use]
pub fn starts_from_breaks(paths: &[Vec<u32>], blocks: usize) -> Vec<u32> {
    let mut starts = vec![0u32];
    for path in paths {
        let Some(&block) = path.first() else {
            continue;
        };
        let last = starts.last().copied().unwrap_or(0);
        if block > last && (block as usize) < blocks {
            starts.push(block);
        }
    }
    starts
}

/// The document as the paginator lays it out: the editor's blocks on ONE
/// host (no padding, the page's text width), in the paper's font.
#[must_use]
pub fn measure_dom(doc: &RichTextDoc) -> Dom {
    let editor = RichTextEditor::create(RichTextEditorState::create(doc.clone()))
        .with_read_only(true)
        .with_font_size(FONT_PX)
        .with_body_points(crate::ribbon::DEFAULT_FONT_SIZE)
        .with_paragraph_spacing(SPACING_PX);
    let pages = editor.page_doms(vec![0u32], 0, 1);
    pages
        .as_slice()
        .first()
        .cloned()
        .unwrap_or_else(Dom::create_div)
        .with_css(PAPER_TEXT_CSS)
}

/// Lays `doc` out on A4 text areas and returns where each page starts.
#[must_use]
pub fn compute_starts(doc: &RichTextDoc, fonts: FontCacheSnapshot) -> Vec<u32> {
    let (w, h) = content_size();
    let styled = StyledDom::create_from_dom(measure_dom(doc));
    let snapshot = Pdf::create().compute_pagination(styled, w, h, fonts, ImageCacheSnapshot::empty());
    let paths: Vec<Vec<u32>> = (0..snapshot.break_count())
        .map(|i| snapshot.break_path(i).as_ref().to_vec())
        .collect();
    starts_from_breaks(&paths, doc.block_count())
}

// ==== On a Thread ====

struct PaginateInit {
    doc: Option<RichTextDoc>,
    generation: u64,
    fonts: FontCacheSnapshot,
}

struct PaginateDone {
    generation: u64,
    starts: Vec<u32>,
}

extern "C" fn paginate_worker(mut init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let Some((doc, generation, fonts)) = init.downcast_mut::<PaginateInit>().and_then(|mut i| {
        let doc = i.doc.take()?;
        Some((doc, i.generation, i.fonts.clone()))
    }) else {
        return;
    };
    let starts = compute_starts(&doc, fonts);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_paginated,
        RefAny::new(PaginateDone { generation, starts }),
    )));
}

/// Starts the pagination of the open document when its pages are not known
/// for its generation and none is on the way (one at a time: the answer
/// starts the next if the document changed meanwhile).
pub fn ensure(state: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    let Some(doc) = state.doc.as_ref() else {
        return;
    };
    if state.pages.generation == doc.generation || state.paginating.is_some() {
        return;
    }
    let Some(fonts) = state.fonts.clone() else {
        return;
    };
    let generation = doc.generation;
    let init = RefAny::new(PaginateInit {
        doc: Some(doc.doc().clone()),
        generation,
        fonts,
    });
    info.add_thread(ThreadId::unique(), Thread::create(init, app.clone(), paginate_worker));
    state.paginating = Some(generation);
}

extern "C" fn on_paginated(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some((generation, starts)) = msg
        .downcast_mut::<PaginateDone>()
        .map(|mut d| (d.generation, core::mem::take(&mut d.starts)))
    else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let state = &mut *guard;
    state.paginating = None;
    let changed = state.pages.starts != starts;
    state.pages = Pages { generation, starts };
    println!("AZWRITER_PAGES {}", state.pages.starts.len());
    ensure(state, &mut info, &handle);
    if changed {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paper_is_set_in_floras_garamond() {
        // flora's running text, named (the paginator's thread has no theme to
        // resolve a role or a generic family by).
        assert!(PAPER_TEXT_CSS.contains("font-family: 'EB Garamond'"), "{PAPER_TEXT_CSS}");
        assert!(!PAPER_TEXT_CSS.contains("sans-serif"));
    }

    fn between(path: Vec<u32>) -> Break {
        Break {
            path,
            text_path: Vec::new(),
            text_byte: None,
        }
    }

    fn inside(path: Vec<u32>, text_path: Vec<u32>, byte: u32) -> Break {
        Break {
            path,
            text_path,
            text_byte: Some(byte),
        }
    }

    /// Every block one run of ten bytes, block 3 the runs "Hello " / "world" / "again".
    fn runs(block: usize) -> Vec<usize> {
        if block == 3 {
            vec![6, 5, 5]
        } else {
            vec![10]
        }
    }

    #[test]
    fn a_page_starts_at_the_block_a_break_falls_in() {
        let breaks = vec![
            between(vec![4, 0, 2]),
            between(vec![9]),
            between(vec![9, 1]),
            between(vec![30]),
        ];
        assert_eq!(
            starts_from_breaks(&breaks, &runs, 20),
            vec![(0, 0), (4, 0), (9, 0)],
            "unique, within the document"
        );
        assert_eq!(starts_from_breaks(&[], &runs, 3), vec![(0, 0)]);
        assert_eq!(
            starts_from_breaks(&[between(vec![0, 3])], &runs, 3),
            vec![(0, 0)],
            "a break in the first block"
        );
    }

    #[test]
    fn a_break_inside_a_paragraph_starts_the_next_page_at_its_line() {
        // The next page's first line begins at "rld" of the run "world" (a bare text node, or
        // the text inside the run's span): six bytes of "Hello " before it, then two.
        let breaks = vec![
            between(vec![2]),
            inside(vec![4], vec![3, 1], 2),
            between(vec![5]),
        ];
        assert_eq!(
            starts_from_breaks(&breaks, &runs, 20),
            vec![(0, 0), (2, 0), (3, 8), (5, 0)]
        );
        let wrapped = vec![inside(vec![4], vec![3, 1, 0], 2)];
        assert_eq!(starts_from_breaks(&wrapped, &runs, 20), vec![(0, 0), (3, 8)]);
        // A line that begins at the block's very start: the page starts with the whole block.
        let at_start = vec![inside(vec![4], vec![3, 0], 0)];
        assert_eq!(starts_from_breaks(&at_start, &runs, 20), vec![(0, 0), (3, 0)]);
        // A text node the runs do not account for: the block the break path names.
        let unknown = vec![inside(vec![4], vec![3, 7], 1)];
        assert_eq!(starts_from_breaks(&unknown, &runs, 20), vec![(0, 0), (4, 0)]);
    }

    #[test]
    fn known_pages_fit_a_shorter_document_and_name_the_page_of_a_block() {
        let pages = Pages {
            generation: 3,
            starts: vec![(0, 0), (4, 0), (9, 0)],
        };
        assert_eq!(pages.starts_for(20), vec![(0, 0), (4, 0), (9, 0)]);
        assert_eq!(pages.starts_for(6), vec![(0, 0), (4, 0)], "a page past the end goes");
        assert_eq!(pages.page_of(0, 0), 0);
        assert_eq!(pages.page_of(5, 3), 1);
        assert_eq!(pages.page_of(12, 0), 2);
    }

    #[test]
    fn a_caret_in_a_paragraph_a_page_cuts_is_on_the_page_of_its_part() {
        let pages = Pages {
            generation: 3,
            starts: vec![(0, 0), (3, 8)],
        };
        assert_eq!(pages.page_of(3, 2), 0, "the head of block 3 ends page 1");
        assert_eq!(pages.page_of(3, 8), 1, "its tail starts page 2");
        assert_eq!(pages.page_of(4, 0), 1);
    }

    #[test]
    fn a_long_document_spans_pages_and_a_page_break_starts_one() {
        let mut md = String::from("# Title\n\n");
        for i in 0..60 {
            md.push_str(&format!(
                "Paragraph {i} with a reasonable amount of text in it, so that sixty of them fill \
                 more than one A4 page of text.\n\n"
            ));
        }
        let doc = RichTextDoc::create_from_markdown(md.as_str());
        let starts = compute_starts(&doc, FontCacheSnapshot::empty());
        assert!(starts.len() >= 2, "sixty paragraphs span pages: {starts:?}");
        let short = RichTextDoc::create_from_markdown("one\n\n<!-- pagebreak -->\n\ntwo\n");
        let starts = compute_starts(&short, FontCacheSnapshot::empty());
        assert_eq!(starts.len(), 2, "a page break starts a page: {starts:?}");
    }
}
