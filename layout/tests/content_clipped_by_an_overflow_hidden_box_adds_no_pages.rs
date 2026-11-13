//! Content clipped by an `overflow: hidden` box adds no pages.
//!
//! The paged extent - how far down the document the page count reaches -
//! was the bottom of every display-list item, clip or no clip
//! (`display_list::calculate_display_list_height`, which the break analysis,
//! the slicer and `PagedLayoutResult::total_content_height` all measure by).
//! An absolutely positioned block whose text ran past the bottom of a
//! fixed-height `overflow: hidden` page box made a second page, though
//! nothing on it could ever be painted (printpdf's HTML-to-PDF:
//! `.page { position: relative; height: <page height>; overflow: hidden }`,
//! with `overflow: hidden` on the blocks too).
//!
//! The page box is exactly one page tall and the block's text runs some
//! thousand pixels below it.
//!
//! Not compiled by the author (house rule); expected RED before the fix
//! (the control test stays green either way).

use crate::{pagination_dom_breaks::paginate, table_markup::prose};

/// A 400 x 300 page box holding one absolutely positioned 200 x 60 block of
/// 300 words, each box with its own extra declarations.
fn page_box(page_style: &str, block_style: &str) -> String {
    format!(
        "<html><head><style>\
         * {{ margin: 0; padding: 0; }}\
         body {{ font-size: 16px; }}\
         .page {{ position: relative; width: 400px; height: 300px; {page_style} }}\
         .block {{ position: absolute; left: 20px; top: 200px; width: 200px; height: 60px; \
         {block_style} }}\
         </style></head><body>\
         <div class=\"page\"><div class=\"block\">{}</div></div>\
         </body></html>",
        prose(300)
    )
}

/// Pages of a 400 x 300 page size the document lays out on.
fn page_count(html: &str) -> usize {
    let (_cache, _dom, pagination) = paginate(html, 400.0, 300.0);
    pagination.page_count
}

#[test]
fn an_abspos_block_of_long_text_in_a_clipped_page_box_stays_on_one_page() {
    let pages = page_count(&page_box("overflow: hidden;", "overflow: hidden;"));
    assert_eq!(
        pages, 1,
        "the page box and its block both clip the text: one page, got {pages}"
    );
}

#[test]
fn a_clipped_page_box_keeps_the_text_of_its_abspos_blocks_off_further_pages() {
    // Only the page box clips: it is the block's containing block, so its
    // clip applies to the block's overflowing text all the same.
    let pages = page_count(&page_box("overflow: hidden;", ""));
    assert_eq!(
        pages, 1,
        "the page box clips its absolutely positioned block: one page, got {pages}"
    );
}

#[test]
fn text_that_nothing_clips_still_runs_on_to_further_pages() {
    let pages = page_count(&page_box("", ""));
    assert!(
        pages >= 2,
        "unclipped text far below the page box is painted, so it is paged: got {pages}"
    );
}
