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

use azul_core::{
    geom::LogicalSize,
    pdf_form::{PdfFieldValueVec, PdfFormFieldVec, PdfPageSvgOptions, PdfStampVec},
};
use azul_css::{AzString, OptionString, StringVec};
use azul_layout::callbacks::ResultU8VecString;

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
    /// The PDF's own bytes: a form is filled on them (printpdf's model drops
    /// the form).
    #[cfg(feature = "pdf")]
    bytes: Vec<u8>,
    /// The form's fields, read at parse time.
    #[cfg(feature = "pdf")]
    fields: Vec<printpdf::forms::FormField>,
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
    pub fn create_from_bytes(bytes: &[u8]) -> Self {
        #[cfg(feature = "pdf")]
        {
            Self::from_inner(engine::parse(bytes))
        }
        #[cfg(not(feature = "pdf"))]
        {
            let _ = bytes;
            super::announce_pdf_stub("ParsedPdf::create_from_bytes");
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

    /// [`Self::page_to_svg`] with `options`: the form fields' values drawn on
    /// the page (`include_form_fields`) or not - a viewer that overlays live
    /// inputs renders the page without them.
    #[must_use]
    pub fn page_to_svg_with(&self, index: usize, options: PdfPageSvgOptions) -> OptionString {
        #[cfg(feature = "pdf")]
        {
            self.inner()
                .and_then(|i| engine::page_svg_with(i, index, options))
                .map(AzString::from)
                .into()
        }
        #[cfg(not(feature = "pdf"))]
        {
            let _ = (index, options);
            OptionString::None
        }
    }

    /// The fields of the PDF's interactive form (AcroForm), in document order;
    /// none for a PDF without a form. Their rects are in points from each
    /// page's top-left corner (the page SVG's user space).
    #[must_use]
    pub fn form_fields(&self) -> PdfFormFieldVec {
        #[cfg(feature = "pdf")]
        {
            self.inner()
                .map(engine::form_fields)
                .unwrap_or_default()
                .into()
        }
        #[cfg(not(feature = "pdf"))]
        {
            PdfFormFieldVec::from_vec(Vec::new())
        }
    }

    /// The PDF with its form filled: `values` set by field name, `stamps`
    /// (drawings - a signature) put onto their pages. With `flatten` the
    /// fields are drawn into the pages and the form removed (a "printed"
    /// copy); without, the fields keep the values and stay editable. A value
    /// for a name the form does not have is ignored. Slow for big documents:
    /// call it on a `Thread`.
    #[must_use]
    pub fn fill_form(
        &self,
        values: PdfFieldValueVec,
        stamps: PdfStampVec,
        flatten: bool,
    ) -> ResultU8VecString {
        #[cfg(feature = "pdf")]
        {
            match self.inner() {
                Some(inner) if inner.parsed => engine::fill_form(
                    inner,
                    values.as_ref(),
                    stamps.as_ref(),
                    flatten,
                )
                .map_err(AzString::from)
                .into(),
                Some(inner) => ResultU8VecString::from(Err(AzString::from(inner.error.clone()))),
                None => ResultU8VecString::from(Err(AzString::from("an empty ParsedPdf"))),
            }
        }
        #[cfg(not(feature = "pdf"))]
        {
            let _ = (values, stamps, flatten);
            ResultU8VecString::from(Err(AzString::from(
                "this build of azul has no PDF support (the `pdf` feature)",
            )))
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
                bytes: bytes.to_vec(),
                fields: printpdf::forms::parse_form_fields(bytes).unwrap_or_default(),
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

    /// [`page_svg`] with the form fields' values drawn on it when `options`
    /// asks: printpdf's `fields_svg` inserted before the closing tag.
    pub(super) fn page_svg_with(
        inner: &Inner,
        index: usize,
        options: azul_core::pdf_form::PdfPageSvgOptions,
    ) -> Option<String> {
        let mut svg = page_svg(inner.doc.as_ref()?, index)?;
        if options.include_form_fields {
            let height = inner.sizes.get(index)?.height_pt;
            let fields = printpdf::forms::fields_svg(&inner.fields, index, height);
            if let Some(end) = svg.rfind("</svg>") {
                svg.insert_str(end, &fields);
            }
        }
        Some(svg)
    }

    /// The form's fields as the API hands them out: rects from the page's
    /// top-left corner.
    pub(super) fn form_fields(inner: &Inner) -> Vec<azul_core::pdf_form::PdfFormField> {
        use azul_core::pdf_form::{
            PdfFormField, PdfFormFieldKind as Kind, PdfFormWidget, PdfRect, PdfTextAlign,
        };
        use printpdf::forms::FormFieldKind;

        inner
            .fields
            .iter()
            .map(|field| PdfFormField {
                name: field.name.clone().into(),
                kind: match field.kind {
                    FormFieldKind::Text => Kind::Text,
                    FormFieldKind::CheckBox => Kind::CheckBox,
                    FormFieldKind::RadioButton => Kind::RadioButton,
                    FormFieldKind::ComboBox => Kind::ComboBox,
                    FormFieldKind::ListBox => Kind::ListBox,
                    FormFieldKind::PushButton => Kind::PushButton,
                    FormFieldKind::Signature => Kind::Signature,
                },
                value: field.value.clone().into(),
                default_value: field.default_value.clone().into(),
                options: field.options.clone().into(),
                widgets: field
                    .widgets
                    .iter()
                    .map(|w| {
                        let [llx, lly, urx, ury] = w.rect;
                        let height = inner.sizes.get(w.page).map_or(0.0, |s| s.height_pt);
                        PdfFormWidget {
                            page: w.page,
                            rect: PdfRect {
                                x: llx,
                                y: height - ury,
                                width: urx - llx,
                                height: ury - lly,
                            },
                            on_state: w.on_state.clone().into(),
                            hidden: w.hidden,
                        }
                    })
                    .collect::<Vec<_>>()
                    .into(),
                read_only: field.read_only,
                required: field.required,
                multiline: field.multiline,
                password: field.password,
                max_length: field.max_len.unwrap_or(0),
                font_size_pt: field.font_size,
                alignment: match field.alignment {
                    1 => PdfTextAlign::Center,
                    2 => PdfTextAlign::Right,
                    _ => PdfTextAlign::Left,
                },
            })
            .collect()
    }

    /// printpdf's `fill_form` on the PDF's bytes, the API's values and
    /// stamps (a stamp's SVG read into paths).
    pub(super) fn fill_form(
        inner: &Inner,
        values: &[azul_core::pdf_form::PdfFieldValue],
        stamps: &[azul_core::pdf_form::PdfStamp],
        flatten: bool,
    ) -> Result<Vec<u8>, String> {
        let values: Vec<printpdf::forms::FieldValue> = values
            .iter()
            .map(|v| printpdf::forms::FieldValue {
                name: v.name.as_str().to_string(),
                value: v.value.as_str().to_string(),
            })
            .collect();
        let stamps: Vec<printpdf::forms::FormStamp> = stamps
            .iter()
            .filter_map(|stamp| {
                let height = inner.sizes.get(stamp.page)?.height_pt;
                let (view_box, paths) = super::super::stamp::svg_paths(stamp.svg.as_str())?;
                let r = stamp.rect;
                Some(printpdf::forms::FormStamp {
                    page: stamp.page,
                    rect: [r.x, height - r.y - r.height, r.x + r.width, height - r.y],
                    view_box,
                    paths,
                })
            })
            .collect();
        printpdf::forms::fill_form(&inner.bytes, &values, &stamps, flatten)
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

#[cfg(all(test, feature = "pdf"))]
mod page_to_svg_tests {
    use super::ParsedPdf;

    /// A one-page PDF (612 x 792 pt) around the content stream `content`,
    /// with `/F1` the standard Helvetica (not embedded).
    fn tiny_pdf(content: &str) -> Vec<u8> {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_string(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }

    /// The page's SVG drawn 612 px wide (1 px per pt), as AzPdf draws it.
    fn picture(pdf: &ParsedPdf) -> azul_core::resources::RawImage {
        use azul_core::svg::{SvgFitTo, SvgParseOptions, SvgRenderOptions};
        use azul_css::props::basic::color::{ColorU, OptionColorU};
        let svg = pdf.page_to_svg(0).into_option().expect("an SVG of page 1");
        let parsed = azul_layout::xml::svg::ParsedSvg::from_string(
            svg.as_str(),
            SvgParseOptions::default(),
        )
        .expect("the SVG parses");
        parsed
            .render(SvgRenderOptions {
                fit: SvgFitTo::Width(612),
                background_color: OptionColorU::Some(ColorU::WHITE),
                ..SvgRenderOptions::default()
            })
            .expect("the SVG draws")
    }

    /// The colour at (`x`, `y`) of a page picture, top-left origin.
    fn at(image: &azul_core::resources::RawImage, x: usize, y: usize) -> (u8, u8, u8) {
        let azul_core::resources::RawImageData::U8(bytes) = &image.pixels else {
            panic!("an 8-bit picture");
        };
        let bytes = bytes.as_ref();
        let i = (y * image.width + x) * 4;
        let px = (bytes[i], bytes[i + 1], bytes[i + 2]);
        match image.data_format {
            azul_core::resources::RawImageFormat::BGRA8 => (px.2, px.1, px.0),
            _ => px,
        }
    }

    #[test]
    fn a_shape_drawn_under_a_scaling_cm_is_where_the_pdf_puts_it() {
        // A 100 x 100 pt red square at (100, 100) in a half-scale user space:
        // on the page it covers x 50..100, y (from the top) 692..742. The SVG
        // export flipped the points AND the translation, so it sat d * H = 396
        // pt too low - below the page.
        let pdf = ParsedPdf::create_from_bytes(&tiny_pdf(
            "q 0.5 0 0 0.5 0 0 cm 1 0 0 rg 100 100 100 100 re f Q",
        ));
        assert!(pdf.is_valid(), "{:?}", pdf.get_error());
        let image = picture(&pdf);
        assert_eq!(at(&image, 75, 717), (255, 0, 0), "inside the square");
        assert_eq!(at(&image, 75, 640), (255, 255, 255), "above it");
    }

    #[test]
    fn a_clip_rectangle_is_not_painted_and_a_rectangle_after_it_is() {
        // `re W n` sets a clip and paints nothing; it came out as a rectangle
        // filled in the current colour (black: a page-sized black box over
        // most real PDFs, which clip their content area first).
        let pdf = ParsedPdf::create_from_bytes(&tiny_pdf(
            "q 0 0 400 400 re W n 0 0.5 0 rg 10 10 50 50 re f Q",
        ));
        assert!(pdf.is_valid(), "{:?}", pdf.get_error());
        let image = picture(&pdf);
        assert_eq!(at(&image, 200, 592), (255, 255, 255), "the clip area stays paper");
        assert_eq!(at(&image, 35, 757), (0, 128, 0), "the green square inside it");
    }

    #[test]
    fn a_cubic_curve_is_drawn_as_one() {
        // A filled shape whose top edge is a cubic bulging up to y 700 pt
        // (a control polygon at 760): (100, 600) -> curve -> (300, 600) ->
        // down to 500 and back. Read as a quadratic through the second handle
        // the curve ended at that handle and the shape lost its right half.
        let pdf = ParsedPdf::create_from_bytes(&tiny_pdf(
            "0 0 1 rg 100 600 m 150 760 250 760 300 600 c 300 500 l 100 500 l h f",
        ));
        assert!(pdf.is_valid(), "{:?}", pdf.get_error());
        let image = picture(&pdf);
        // (280, 550) pt is inside the right half; y from the top: 792 - 550.
        assert_eq!(at(&image, 280, 242), (0, 0, 255), "the right half is filled");
        // The bulge: (200, 690) pt lies under the curve's apex (720).
        assert_eq!(at(&image, 200, 102), (0, 0, 255), "under the apex");
        // Above the curve at x 250 (it passes 680 there): the quadratic ran
        // up to the second handle, (250, 760), and filled this.
        assert_eq!(at(&image, 250, 52), (255, 255, 255), "above the curve");
    }
}
