//! A superscript and a subscript are drawn off the baseline in the PDF
//! payload.
//!
//! `<sup>`, `<sub>` and `vertical-align: super` move their text up or down
//! (CSS 2.2 10.8.1; Chrome: by the parent's font size / 3 + 1px up, / 5 + 1px
//! down). printpdf draws a page's text from the `TextLayout` payloads of the
//! paged layout, so the shift has to be in the clusters' positions there, not
//! only in the window's glyph runs: Calmet's Dictionary had every footnote
//! mark of a line drawn on its baseline (pdfocr engine issue 1).
//!
//! Font-independent: every mark is compared with the same paragraph whose
//! marks only have the smaller font size, relative to the paragraph's "C".

use azul_core::geom::LogicalPosition;

use crate::paged_payload::{cluster_at, has_cluster, paged_text_layouts};

/// `Cock`, a mark `a`, `and Job`, a mark `z`, `then x`, a mark `2`, the
/// marks written as `sup`, `super` and `sub`.
fn paragraph(sup: &str, sup_end: &str, sup_style: &str, sub: &str, sub_end: &str) -> String {
    format!(
        "<p style=\"margin: 0 0 40px 0; font-size: 20px\">Cock{sup}a{sup_end} and Job<span \
         style=\"font-size: 0.6em; {sup_style}\">z</span> then x{sub}2{sub_end}</p>"
    )
}

/// The top of the marks a, z and 2 below the top of the paragraph's "C".
fn mark_drops(items: &[azul_layout::text3::cache::PositionedItem]) -> [f32; 3] {
    let c: LogicalPosition = cluster_at(items, "C");
    ["a", "z", "2"].map(|mark| cluster_at(items, mark).y - c.y)
}

#[test]
fn a_superscript_and_a_subscript_are_drawn_off_the_baseline_in_the_pdf_payload() {
    let control = paragraph(
        "<span style=\"font-size: smaller\">",
        "</span>",
        "",
        "<span style=\"font-size: smaller\">",
        "</span>",
    );
    let marked = paragraph(
        "<sup>",
        "</sup>",
        "vertical-align: super",
        "<sub>",
        "</sub>",
    );
    let html =
        format!("<html><head></head><body style=\"margin: 0\">{control}{marked}</body></html>");
    let layouts: Vec<_> = paged_text_layouts(&html)
        .into_iter()
        .filter(|(_, items)| has_cluster(items, "z"))
        .collect();
    assert_eq!(layouts.len(), 2, "one text layout per paragraph");
    let plain = mark_drops(&layouts[0].1);
    let shifted = mark_drops(&layouts[1].1);
    let [sup, sup_style, sub] = [0, 1, 2].map(|i| shifted[i] - plain[i]);
    assert!(
        sup < -2.0,
        "<sup>a</sup> is drawn higher than the same small \"a\" on the baseline: {sup:.2} px \
         (plain {plain:?}, marked {shifted:?})"
    );
    assert!(
        sup_style < -2.0,
        "vertical-align: super draws the \"z\" higher: {sup_style:.2} px (plain {plain:?}, \
         marked {shifted:?})"
    );
    assert!(
        sub > 2.0,
        "<sub>2</sub> is drawn lower than the same small \"2\" on the baseline: {sub:.2} px \
         (plain {plain:?}, marked {shifted:?})"
    );
}
