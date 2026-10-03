//! The pages of a chapter: the engine's pagination of the reading column.
//!
//! The chapter's DOM ([`crate::content::Chapter::xml`] through `Dom::create_from_parsed_xml`)
//! sits in the READING COLUMN - a box exactly the page's text width with the reader's type
//! (font, size, line height, alignment) - and the column is laid out once by azul's paged
//! layout at the page's text height. Its breaks are the page starts ([`PageMap`]). The SAME
//! column is what the reader shows, clipped per page: the pagination and the screen lay out
//! one box the same way.
//!
//! A reader's pages end between two lines, never inside one: the pagination runs the
//! READING POLICY ([`reading_policy`]: whole lines, widows and orphans, pictures and table rows
//! kept whole), not the default plain slicing.

use azul::{
    dom::StyledDom,
    font::FontCacheSnapshot,
    image::ImageCacheSnapshot,
    pdf::{BreakPolicy, Pdf},
};

use crate::position::PageMap;

/// How a reader's pages break: never inside a line, two lines at least at a page's top and
/// bottom (CSS `widows` / `orphans`), a box that asks to stay whole (`break-inside: avoid`:
/// a picture, a heading with the reader's sheet) and a table row kept whole.
#[must_use]
pub fn reading_policy() -> BreakPolicy {
    BreakPolicy {
        max_push_distance: 0.33,
        honor_break_inside: true,
        widows_orphans: true,
        atomic_lines: true,
        atomic_table_rows: true,
        repeat_table_headers: false,
    }
}

/// The pages of `column` (a styled reading column) on pages of `width` x `height` text area.
#[must_use]
pub fn page_map(
    column: StyledDom,
    width: f32,
    height: f32,
    fonts: FontCacheSnapshot,
    images: ImageCacheSnapshot,
) -> PageMap {
    let snapshot = Pdf::create().compute_pagination_with_policy(
        column,
        width,
        height,
        fonts,
        images,
        reading_policy(),
    );
    let breaks: Vec<f32> = (0..snapshot.break_count())
        .map(|i| snapshot.break_y(i))
        .collect();
    PageMap::from_breaks(&breaks, snapshot.total_content_height())
}

#[cfg(test)]
mod tests {
    use azul::dom::Dom;

    use super::*;

    #[test]
    fn a_page_of_text_never_ends_inside_a_line() {
        // Twenty paragraphs of three to four lines of 20 px on pages 105 px high: the plain
        // slicing would cut at 105, 210, ... - through a line every time.
        let mut column = Dom::create_div().with_css(
            "display: block; width: 300px; font-size: 10px; line-height: 20px; \
             font-family: sans-serif; margin: 0px; padding: 0px;",
        );
        for i in 0..20 {
            column.add_child(
                Dom::create_p_with_text(format!(
                    "Paragraph {i}: it was the best of times, it was the worst of times, it \
                     was the age of wisdom, it was the age of foolishness."
                ))
                .with_css("margin: 0px; padding: 0px;"),
            );
        }
        let pages = page_map(
            StyledDom::create_from_dom(column),
            300.0,
            105.0,
            FontCacheSnapshot::empty(),
            ImageCacheSnapshot::empty(),
        );
        assert!(pages.page_count() >= 3, "{} pages", pages.page_count());
        for page in 1..pages.page_count() {
            let (top, height) = pages.span(page);
            let into_line = top % 20.0;
            assert!(
                into_line < 0.01 || into_line > 19.99,
                "page {page} starts {into_line} px into a line (at {top})"
            );
            assert!(
                height <= 105.0 + 0.01,
                "page {page} is {height} px, the page 105"
            );
        }
    }
}
