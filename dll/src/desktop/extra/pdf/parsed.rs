//! A PDF parsed once and read many times (PDF9): page count, page sizes,
//! page N as SVG text, the text of a page, the title and the outline.
//!
//! printpdf parses the bytes into its op model (`PdfDocument::parse`) and
//! renders ONE page of that model to a standalone SVG (`page_to_svg`). This
//! handle keeps the parse, so a viewer renders only the pages it shows - the
//! older `Pdf::to_svg_pages` renders every page in one call and is now built
//! on this handle (one render path).
//!
//! The handle is reference counted and immutable: clones share the parse,
//! and it is `Send + Sync`, so an app parses on a `Thread` and hands the
//! handle back to the UI thread, and page renders can run on threads too.
//!
//! The API is always present; without the `pdf` feature every handle is
//! invalid and says so in `get_error`.
//!
//! What the SVG carries (printpdf's renderer, `render.rs`): paths for
//! fills / strokes, `<text>` with the fonts as `@font-face` data URLs, and
//! `<image>` with data URLs. Colour spaces, inline images and shadings are
//! not rendered by printpdf yet.

use alloc::sync::Arc;
use core::ffi::c_void;

use azul_core::geom::LogicalSize;
use azul_css::{AzString, OptionString, StringVec};

/// CSS px per PDF point: a point is 1/72 inch, a CSS px 1/96 inch.
pub const PDF_PX_PER_PT: f32 = 96.0 / 72.0;

/// A page's size in PDF points (1/72 inch), from its MediaBox.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct PdfPageSize {
    /// The width in points.
    pub width_pt: f32,
    /// The height in points.
    pub height_pt: f32,
}

impl PdfPageSize {
    /// The size in CSS px (96 per inch): what the page measures at 100 %.
    #[must_use]
    pub fn to_logical_size(&self) -> LogicalSize {
        LogicalSize {
            width: self.width_pt * PDF_PX_PER_PT,
            height: self.height_pt * PDF_PX_PER_PT,
        }
    }
}

/// What a [`ParsedPdf`] shares between its clones. Immutable once built.
#[derive(Default)]
struct Inner {
    /// The printpdf op model (`None`: the parse failed or no `pdf` feature).
    #[cfg(feature = "pdf")]
    doc: Option<printpdf::PdfDocument>,
    /// `true` if the bytes parsed.
    parsed: bool,
    /// One size per page (the MediaBox), read at parse time.
    sizes: Vec<PdfPageSize>,
    /// The outline (bookmarks) in document order: title, 0-based page.
    outline: Vec<(String, usize)>,
    /// The Info dictionary's /Title.
    title: String,
    /// Why the parse failed (empty when it did not).
    error: String,
    /// What the parser skipped or guessed, one line each.
    warnings: Vec<String>,
}

/// A PDF parsed once: page count, page sizes, page N as SVG, the text of a
/// page, the title and the outline.
///
/// Reference counted (clones share one parse) and immutable, so it is
/// `Send + Sync`: parse on a `Thread`, hand the handle to the UI thread,
/// render pages on threads. Bytes that are not a PDF give a handle that is
/// not valid (`is_valid` false, `get_error` says why) with no pages - never
/// a null handle.
#[repr(C)]
#[derive(Debug)]
pub struct ParsedPdf {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

// SAFETY: `ptr` is an `Arc<Inner>` that is never mutated after construction,
// and `Inner` is `Send + Sync` (the printpdf model is asserted in `engine`).
unsafe impl Send for ParsedPdf {}
unsafe impl Sync for ParsedPdf {}

impl Clone for ParsedPdf {
    /// Shares the parse (a reference count), it does not parse again.
    fn clone(&self) -> Self {
        if !self.ptr.is_null() {
            // SAFETY: `ptr` came from `Arc::into_raw` and this handle still
            // owns one count of it.
            unsafe { Arc::increment_strong_count(self.ptr as *const Inner) };
        }
        Self {
            ptr: self.ptr,
            run_destructor: !self.ptr.is_null(),
        }
    }
}

impl Default for ParsedPdf {
    /// An empty handle: no pages, not valid.
    fn default() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl Drop for ParsedPdf {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            // SAFETY: balances the `into_raw` / `increment_strong_count`
            // that produced this handle.
            unsafe { Arc::decrement_strong_count(self.ptr as *const Inner) };
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl ParsedPdf {
    fn from_inner(inner: Inner) -> Self {
        Self {
            ptr: Arc::into_raw(Arc::new(inner)) as *mut c_void,
            run_destructor: true,
        }
    }

    fn inner(&self) -> Option<&Inner> {
        // SAFETY: a non-null `ptr` is a live `Arc<Inner>` this handle counts.
        unsafe { (self.ptr as *const Inner).as_ref() }
    }

    /// Parses PDF `bytes`. Slow for big documents: call it on a `Thread`.
    /// Bytes that are not a PDF give a handle with no pages whose
    /// `get_error` says why.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        #[cfg(feature = "pdf")]
        {
            Self::from_inner(engine::parse(bytes))
        }
        #[cfg(not(feature = "pdf"))]
        {
            let _ = bytes;
            super::announce_pdf_stub("ParsedPdf::from_bytes");
            Self::from_inner(Inner {
                error: String::from("this build of azul has no PDF support (the `pdf` feature)"),
                ..Inner::default()
            })
        }
    }

    /// `true` if the bytes parsed as a PDF.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.inner().is_some_and(|i| i.parsed)
    }

    /// Why the parse failed; empty when it did not.
    #[must_use]
    pub fn get_error(&self) -> AzString {
        self.inner()
            .map_or_else(String::new, |i| i.error.clone())
            .into()
    }

    /// What the parser skipped or guessed, one line each.
    #[must_use]
    pub fn get_warnings(&self) -> StringVec {
        self.inner()
            .map_or_else(Vec::new, |i| i.warnings.clone())
            .into()
    }

    /// The document's title (the Info dictionary's /Title); empty if none.
    #[must_use]
    pub fn get_title(&self) -> AzString {
        self.inner()
            .map_or_else(String::new, |i| i.title.clone())
            .into()
    }

    /// The number of pages.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.inner().map_or(0, |i| i.sizes.len())
    }

    /// The size of page `index` (0-based) in points; zero past the end.
    #[must_use]
    pub fn page_size(&self, index: usize) -> PdfPageSize {
        self.inner()
            .and_then(|i| i.sizes.get(index).copied())
            .unwrap_or_default()
    }

    /// Page `index` (0-based) as a standalone SVG document whose user space
    /// is the page in points (`viewBox="0 0 <width_pt> <height_pt>"`).
    /// `None` past the end. Renders on every call: cache the result, and call
    /// it on a `Thread` for big pages.
    #[must_use]
    pub fn page_to_svg(&self, index: usize) -> OptionString {
        #[cfg(feature = "pdf")]
        {
            self.inner()
                .and_then(|i| i.doc.as_ref())
                .and_then(|doc| engine::page_svg(doc, index))
                .map(AzString::from)
                .into()
        }
        #[cfg(not(feature = "pdf"))]
        {
            let _ = index;
            OptionString::None
        }
    }

    /// The text of page `index` (0-based), one entry per text block in
    /// content order (lines end in `\r\n`) - for search and copy. Empty past
    /// the end or for a page without text (a scan).
    #[must_use]
    pub fn page_text(&self, index: usize) -> StringVec {
        #[cfg(feature = "pdf")]
        {
            self.inner()
                .and_then(|i| i.doc.as_ref())
                .and_then(|doc| {
                    doc.pages
                        .get(index)
                        .map(|page| page.extract_text(&doc.resources))
                })
                .unwrap_or_default()
                .into()
        }
        #[cfg(not(feature = "pdf"))]
        {
            let _ = index;
            Vec::<String>::new().into()
        }
    }

    /// The number of outline entries (the PDF's top-level bookmarks).
    #[must_use]
    pub fn outline_count(&self) -> usize {
        self.inner().map_or(0, |i| i.outline.len())
    }

    /// The title of outline entry `index`; empty past the end.
    #[must_use]
    pub fn outline_title(&self, index: usize) -> AzString {
        self.inner()
            .and_then(|i| i.outline.get(index))
            .map_or_else(String::new, |(title, _)| title.clone())
            .into()
    }

    /// The page (0-based) outline entry `index` jumps to; 0 past the end.
    #[must_use]
    pub fn outline_page(&self, index: usize) -> usize {
        self.inner()
            .and_then(|i| i.outline.get(index))
            .map_or(0, |(_, page)| *page)
    }
}

#[cfg(feature = "pdf")]
mod engine {
    use printpdf::{PdfDocument, PdfParseOptions, PdfToSvgOptions, PdfWarnMsg};

    use super::{Inner, PdfPageSize};

    /// `ParsedPdf`'s `Send + Sync` rests on the printpdf model being both.
    const _: fn() = || {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PdfDocument>();
    };

    /// Parses `bytes` into the shared state of a `ParsedPdf`.
    pub(super) fn parse(bytes: &[u8]) -> Inner {
        let mut warnings: Vec<PdfWarnMsg> = Vec::new();
        let parsed = PdfDocument::parse(bytes, &PdfParseOptions::default(), &mut warnings);
        let warnings = warnings.iter().map(warning_line).collect();
        match parsed {
            Ok(doc) => Inner {
                parsed: true,
                sizes: doc
                    .pages
                    .iter()
                    .map(|page| PdfPageSize {
                        width_pt: page.media_box.width.0,
                        height_pt: page.media_box.height.0,
                    })
                    .collect(),
                outline: outline(&doc),
                title: doc.metadata.info.document_title.clone(),
                error: String::new(),
                warnings,
                doc: Some(doc),
            },
            Err(error) => Inner {
                error: if error.is_empty() {
                    String::from("the bytes are not a PDF")
                } else {
                    error
                },
                warnings,
                ..Inner::default()
            },
        }
    }

    /// Page `index` (0-based) as SVG. printpdf numbers pages from 1.
    pub(super) fn page_svg(doc: &PdfDocument, index: usize) -> Option<String> {
        let page = index.checked_add(1)?;
        let mut warnings: Vec<PdfWarnMsg> = Vec::new();
        doc.page_to_svg(page, &PdfToSvgOptions::default(), &mut warnings)
    }

    /// The bookmarks in document order. printpdf keys them `bookmark_<n>` in
    /// a `BTreeMap`, whose STRING order puts `bookmark_10` before
    /// `bookmark_2`: sort by `<n>`. Their pages are 1-based.
    fn outline(doc: &PdfDocument) -> Vec<(String, usize)> {
        let mut entries: Vec<(usize, String, usize)> = doc
            .bookmarks
            .map
            .iter()
            .map(|(id, mark)| {
                let order =
                    id.0.rsplit('_')
                        .next()
                        .and_then(|n| n.parse::<usize>().ok())
                        .unwrap_or(usize::MAX);
                (order, mark.name.clone(), mark.page.saturating_sub(1))
            })
            .collect();
        entries.sort_by_key(|(order, _, _)| *order);
        entries
            .into_iter()
            .map(|(_, title, page)| (title, page))
            .collect()
    }

    fn warning_line(w: &PdfWarnMsg) -> String {
        format!(
            "{:?} (page {}, op {}): {}",
            w.severity,
            w.page + 1,
            w.op_id,
            w.msg
        )
    }
}
