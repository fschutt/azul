//! A block taller than a page is split across pages (LAYOUT7 item 10;
//! WRITER6 "Left": AzWriter's "pages hold whole blocks, so a block taller
//! than a page overflows its sheet").
//!
//! CSS Fragmentation 3 s4.4 / s5: a block container is fragmentable - a
//! paragraph taller than the page breaks between its line boxes and goes on
//! over the next pages. A MONOLITHIC box (a replaced element, a box with
//! `overflow: hidden`) cannot be broken, but one taller than the page "may
//! be sliced" in paged media - and that is what the engine does (the slicer:
//! content never moves, a box taller than the page is torn, page_breaks.rs's
//! monolith rule).
//!
//! The engine's pagination already splits the tall paragraph and knows
//! where (`StructuralBreak::line_start`, the run and byte of the first line
//! that moves, pinned by pagination_dom_breaks.rs). What an app could NOT
//! get is the block that break splits: `break_path` addresses the first
//! block whose TOP is at/after the break - for a break inside the LAST
//! block, nothing at all - and the FFI handle (`PaginationSnapshot`, what
//! AzWriter's `compute_starts` reads through `Pdf::compute_pagination`)
//! carried neither. So AzWriter put every page start at a block and a tall
//! block's later pages never existed. Each break now names the block it
//! splits and the line it splits at, through the handle.
//!
//! Not compiled by the author (house rule). RED before the fix: the handle's
//! `break_line_path` / `break_line_start_*` do not exist yet (the test does
//! not build), and `StructuralBreak` has no `line_path`.

use azul_layout::{
    resource_handles::{PaginationAnalysis, PaginationSnapshot},
    solver3::paged_layout::pagination_to_dom_breaks,
};

use crate::pagination_dom_breaks::paginate;

#[test]
fn a_paragraph_taller_than_a_page_names_itself_and_the_line_on_every_page_break() {
    // A short block, then one paragraph of some 25 lines of 20px on 200px
    // pages: every break lands inside the paragraph, the LAST block.
    let long = "wrap ".repeat(400);
    let html = format!(
        "<html><head><style>* {{ margin: 0; padding: 0; }} body {{ font-size: 16px; \
         line-height: 20px; width: 300px; }} .p {{ display: block; width: 300px; }}\
         </style></head><body><div class=\"p\">short</div><div class=\"p\">{long}</div>\
         </body></html>"
    );
    let (cache, styled_dom, pagination) = paginate(&html, 300.0, 200.0);
    assert!(
        pagination.page_count >= 3,
        "the paragraph spans pages: {} pages",
        pagination.page_count
    );
    let structural =
        pagination_to_dom_breaks(&cache, &styled_dom, &pagination).expect("tree and positions");
    let snapshot = PaginationSnapshot::from_analysis(PaginationAnalysis {
        info: pagination.clone(),
        structural,
    });
    assert!(snapshot.break_count() >= 2);

    let mut previous: Option<(u32, u32)> = None;
    for i in 0..snapshot.break_count() {
        let path = snapshot.break_line_path(i);
        let path = path.as_ref();
        assert!(
            path.len() >= 2 && path.last() == Some(&1),
            "break {i} (y {}) splits the tall paragraph, the body's second block: its path \
             names it, got {path:?}",
            snapshot.break_y(i)
        );
        let run = snapshot.break_line_start_run(i).into_option();
        let byte = snapshot.break_line_start_byte(i).into_option();
        let (Some(run), Some(byte)) = (run, byte) else {
            panic!("break {i} names the first line of its next page: run {run:?} byte {byte:?}");
        };
        assert!(
            (run, byte) > (0, 0),
            "a split inside the paragraph never starts at its first byte"
        );
        if let Some(prev) = previous {
            assert!(
                (run, byte) > prev,
                "page after page the split moves down the text: {prev:?} then {:?}",
                (run, byte)
            );
        }
        previous = Some((run, byte));
    }
}

#[test]
fn a_break_between_blocks_names_no_split() {
    let html = "<html><head><style>* { margin: 0; padding: 0; } .p { height: 200px; }\
                </style></head><body><div class=\"p\">one</div><div class=\"p\">two</div>\
                <div class=\"p\">three</div></body></html>";
    let (cache, styled_dom, pagination) = paginate(html, 800.0, 200.0);
    let structural =
        pagination_to_dom_breaks(&cache, &styled_dom, &pagination).expect("tree and positions");
    let snapshot = PaginationSnapshot::from_analysis(PaginationAnalysis {
        info: pagination.clone(),
        structural,
    });
    for i in 0..snapshot.break_count() {
        assert!(
            snapshot.break_line_path(i).as_ref().is_empty()
                && snapshot.break_line_start_byte(i).into_option().is_none(),
            "a break at a block boundary splits no block (the whole next block moves): break {i}"
        );
    }
}

#[test]
fn a_monolithic_box_taller_than_a_page_is_sliced_across_pages() {
    // `overflow: hidden` makes the box monolithic (css-break-3 s4.4); at 500px
    // on 200px pages it cannot fit any page, so it is sliced in place - the
    // engine's choice, which paged media permits - and the block after it
    // follows on the third page.
    let html = "<html><head><style>* { margin: 0; padding: 0; } body { font-size: 16px; \
                line-height: 20px; } .m { height: 500px; overflow: hidden; } \
                .p { height: 20px; }</style></head><body><div class=\"m\">x</div>\
                <div class=\"p\">after</div></body></html>";
    let (_cache, _dom, pagination) = paginate(html, 800.0, 200.0);
    assert_eq!(
        pagination.page_count, 3,
        "520px of content on 200px pages, the monolith sliced at 200 and 400: {:?}",
        pagination.breaks
    );
    let ys: Vec<f32> = pagination.breaks.iter().map(|b| b.y).collect();
    assert!(
        ys.len() == 2 && (ys[0] - 200.0).abs() < 1.0 && (ys[1] - 400.0).abs() < 1.0,
        "the slices are the page height: {ys:?}"
    );
}
