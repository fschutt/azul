//! `ParsedPdf`: PDF bytes -> page count, page sizes, page N as SVG text, the
//! text of a page and the outline (PDF9). The documents are built with
//! printpdf's own op API, so the sizes and the text are known exactly and no
//! system font or layout is involved.

use printpdf::{
    BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt, TextItem,
};

use super::{ParsedPdf, PdfPageSize};

/// A4 portrait in points (210 mm x 297 mm).
const A4_W_PT: f32 = 595.2756;
const A4_H_PT: f32 = 841.8898;

fn text_page(width: Mm, height: Mm, text: &str) -> PdfPage {
    let ops = vec![
        Op::StartTextSection,
        Op::SetTextCursor {
            pos: Point::new(Mm(20.0), Mm(270.0)),
        },
        Op::SetFont {
            font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
            size: Pt(12.0),
        },
        Op::ShowText {
            items: vec![TextItem::Text(text.to_string())],
        },
        Op::EndTextSection,
    ];
    PdfPage::new(width, height, ops)
}

fn save(doc: &PdfDocument) -> Vec<u8> {
    doc.save(&PdfSaveOptions::default(), &mut Vec::new())
}

/// Page 1: A4 portrait with "Hello page one"; page 2: A4 landscape, empty.
fn two_pages() -> Vec<u8> {
    let mut doc = PdfDocument::new("PDF9 sample");
    doc.pages
        .push(text_page(Mm(210.0), Mm(297.0), "Hello page one"));
    doc.pages
        .push(PdfPage::new(Mm(297.0), Mm(210.0), Vec::new()));
    save(&doc)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.1
}

#[test]
fn a_parsed_pdf_reports_its_page_count_and_page_sizes_in_points() {
    let pdf = ParsedPdf::from_bytes(&two_pages());
    assert!(pdf.is_valid(), "error: {}", pdf.get_error().as_str());
    assert_eq!(pdf.page_count(), 2);

    let first = pdf.page_size(0);
    assert!(
        close(first.width_pt, A4_W_PT) && close(first.height_pt, A4_H_PT),
        "page 1 must be A4 portrait in points, got {first:?}"
    );
    let second = pdf.page_size(1);
    assert!(
        close(second.width_pt, A4_H_PT) && close(second.height_pt, A4_W_PT),
        "page 2 must be A4 landscape in points, got {second:?}"
    );
}

#[test]
fn page_n_renders_as_a_standalone_svg_the_size_of_the_page() {
    let pdf = ParsedPdf::from_bytes(&two_pages());
    let svg = pdf
        .page_to_svg(0)
        .into_option()
        .expect("page 1 must render to SVG");
    let svg = svg.as_str();
    assert!(
        svg.starts_with("<svg"),
        "not a standalone SVG: {}",
        &svg[..svg.len().min(80)]
    );
    assert!(svg.trim_end().ends_with("</svg>"));
    assert!(
        svg.contains("viewBox=\"0 0 595.2"),
        "the SVG's user space must be the page's points: {}",
        &svg[..svg.len().min(200)]
    );
    assert!(
        svg.contains("Hello page one"),
        "page 1's text must be in its SVG"
    );

    let landscape = pdf
        .page_to_svg(1)
        .into_option()
        .expect("page 2 must render");
    assert!(landscape.as_str().contains("viewBox=\"0 0 841.8"));
}

#[test]
fn a_page_past_the_end_has_no_svg_no_text_and_a_zero_size() {
    let pdf = ParsedPdf::from_bytes(&two_pages());
    assert!(pdf.page_to_svg(2).is_none());
    assert!(pdf.page_to_svg(usize::MAX).is_none());
    assert_eq!(pdf.page_size(2), PdfPageSize::default());
    assert!(pdf.page_text(2).as_slice().is_empty());
}

#[test]
fn the_text_of_a_page_is_extracted_for_search() {
    let pdf = ParsedPdf::from_bytes(&two_pages());
    let text: Vec<String> = pdf
        .page_text(0)
        .as_slice()
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    assert!(
        text.join(" ").contains("Hello page one"),
        "page 1's text runs: {text:?}"
    );
    assert!(pdf.page_text(1).as_slice().is_empty(), "page 2 has no text");
}

#[test]
fn the_document_title_comes_from_the_info_dictionary() {
    let pdf = ParsedPdf::from_bytes(&two_pages());
    assert_eq!(pdf.get_title().as_str(), "PDF9 sample");
}

#[test]
fn bytes_that_are_not_a_pdf_give_an_invalid_handle_with_an_error_and_no_pages() {
    let pdf = ParsedPdf::from_bytes(b"this is not a PDF");
    assert!(!pdf.is_valid());
    assert!(
        !pdf.get_error().as_str().is_empty(),
        "the parse error must be reported"
    );
    assert_eq!(pdf.page_count(), 0);
    assert!(pdf.page_to_svg(0).is_none());

    let empty = ParsedPdf::from_bytes(&[]);
    assert!(!empty.is_valid());
    assert_eq!(empty.page_count(), 0);
}

#[test]
fn a_default_handle_is_empty_and_safe_to_query() {
    let pdf = ParsedPdf::default();
    assert!(!pdf.is_valid());
    assert_eq!(pdf.page_count(), 0);
    assert_eq!(pdf.page_size(0), PdfPageSize::default());
    assert!(pdf.page_to_svg(0).is_none());
    assert_eq!(pdf.outline_count(), 0);
    let copy = pdf.clone();
    assert_eq!(copy.page_count(), 0);
}

#[test]
fn clones_share_one_parse_and_outlive_the_original() {
    let pdf = ParsedPdf::from_bytes(&two_pages());
    let copy = pdf.clone();
    assert_eq!(
        pdf.ptr, copy.ptr,
        "a clone shares the parse, it does not re-parse"
    );
    drop(pdf);
    assert_eq!(copy.page_count(), 2);
    assert!(copy.page_to_svg(0).is_some());
}

#[test]
fn a_parsed_pdf_can_be_read_from_another_thread() {
    let pdf = ParsedPdf::from_bytes(&two_pages());
    let copy = pdf.clone();
    let count = std::thread::spawn(move || {
        let svg = copy.page_to_svg(1).into_option().map(|s| s.as_str().len());
        (copy.page_count(), svg)
    })
    .join()
    .expect("the worker must not panic");
    assert_eq!(count.0, 2);
    assert!(count.1.is_some_and(|len| len > 0));
}

#[test]
fn the_outline_lists_the_bookmarks_in_document_order_with_zero_based_pages() {
    let mut doc = PdfDocument::new("outline");
    for i in 0..12 {
        doc.pages
            .push(text_page(Mm(210.0), Mm(297.0), &format!("Body {}", i + 1)));
    }
    // printpdf's bookmark pages are 1-based; twelve entries, so a sort by the
    // entries' string ids ("bookmark_10" < "bookmark_2") would scramble them.
    for i in 0..12 {
        doc.add_bookmark(&format!("Section {:02}", i + 1), i + 1);
    }
    let pdf = ParsedPdf::from_bytes(&save(&doc));
    assert!(pdf.is_valid(), "error: {}", pdf.get_error().as_str());
    assert_eq!(pdf.outline_count(), 12);
    for i in 0..12 {
        assert_eq!(
            pdf.outline_title(i).as_str(),
            format!("Section {:02}", i + 1),
            "entry {i} out of order"
        );
        assert_eq!(
            pdf.outline_page(i),
            i,
            "entry {i} must jump to page index {i}"
        );
    }
    assert_eq!(pdf.outline_title(12).as_str(), "");
}

#[test]
fn the_legacy_all_pages_call_and_the_handle_render_the_same_svg() {
    let bytes = two_pages();
    let all = super::pdf_to_svg_pages(&bytes);
    let pdf = ParsedPdf::from_bytes(&bytes);
    assert_eq!(all.len(), pdf.page_count());
    for (i, svg) in all.iter().enumerate() {
        let one = pdf.page_to_svg(i).into_option().expect("each page renders");
        assert_eq!(
            svg.as_str(),
            one.as_str(),
            "page {i}: one render path, not two"
        );
    }
}
