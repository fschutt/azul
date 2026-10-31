//! A named flow runs through its regions: the content of `flow-into: x`
//! leaves the place where it stands and is laid out through the boxes with
//! `flow-from: x`, in document order, each with its own width and shape, a
//! paragraph going on from one to the next line by line.
//!
//! CSS Regions 1. `flow-into` and `flow-from` were parsed and never laid
//! out: the story stayed where it was written and the regions stayed empty.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.

use azul_layout::{solver3::display_list::DisplayListItem, window::LayoutWindow};

use crate::{
    mono_lines::{inked_pens, FONT_SIZE, GLYPH, LINE},
    table_markup::{items, laid_out},
};

/// `body` under `style` in an 800 x 600 window, every margin and padding
/// zeroed and all text in Azul Mock Mono at 20px on 20px lines.
fn page(style: &str, body: &str) -> LayoutWindow {
    laid_out(
        &format!(
            "<html><head><style>* {{ margin: 0; padding: 0 }} body {{ font-family: 'Azul Mock \
             Mono'; font-size: 20px; line-height: 20px }} .story {{ flow-into: art }} .r {{ \
             position: absolute; flow-from: art }} {style}</style></head><body>{body}</body></html>"
        ),
        800.0,
        600.0,
    )
}

/// The top left corners (x, line box top) of every inked glyph `lw` draws.
fn glyphs(lw: &LayoutWindow) -> Vec<(f32, f32)> {
    inked_pens(lw)
        .into_iter()
        .map(|(x, baseline)| (x, baseline - FONT_SIZE * 0.8))
        .collect()
}

/// The glyphs of `glyphs` whose boxes lie in the rectangle `x`, `y`,
/// `width` x `height`.
fn inside(glyphs: &[(f32, f32)], x: f32, y: f32, width: f32, height: f32) -> usize {
    glyphs
        .iter()
        .filter(|&&(gx, gy)| {
            gx >= x - 0.01
                && gx + GLYPH <= x + width + 0.01
                && gy >= y - 0.01
                && gy + LINE <= y + height + 0.01
        })
        .count()
}

/// Five words of four letters, two per line in a 100px region.
const STORY: &str = "aaaa bbbb cccc dddd eeee";

#[test]
fn a_story_fills_its_regions_in_document_order() {
    // The first region in the document is the lower right one.
    let lw = page(
        ".r { width: 100px; height: 40px } #first { left: 400px; top: 300px } #second { left: \
         50px; top: 50px }",
        &format!(
            "<div class=\"story\">{STORY}</div><div class=\"r\" id=\"first\"></div><div \
             class=\"r\" id=\"second\"></div>"
        ),
    );
    let g = glyphs(&lw);
    assert_eq!(g.len(), 20, "every letter of the story is drawn once: {g:?}");
    assert_eq!(
        inside(&g, 400.0, 300.0, 100.0, 40.0),
        16,
        "the first region holds the first two lines: {g:?}"
    );
    assert_eq!(
        inside(&g, 50.0, 50.0, 100.0, 40.0),
        4,
        "the second region holds the last word: {g:?}"
    );
}

#[test]
fn the_story_takes_no_room_where_it_stands() {
    let lw = page(
        ".r { width: 300px; height: 100px; left: 400px; top: 300px }",
        &format!(
            "<div class=\"story\">{STORY}</div><p>zz</p><div class=\"r\"></div>"
        ),
    );
    let g = glyphs(&lw);
    assert_eq!(
        inside(&g, 0.0, 0.0, 400.0, 20.0),
        2,
        "the first line of the page is the paragraph after the story, its two letters: {g:?}"
    );
}

#[test]
fn a_regions_own_children_are_not_shown() {
    let lw = page(
        ".r { width: 300px; height: 100px; left: 400px; top: 300px }",
        &format!("<div class=\"story\">{STORY}</div><div class=\"r\">xxxx</div>"),
    );
    let g = glyphs(&lw);
    assert_eq!(g.len(), 20, "only the story is drawn: {g:?}");
    assert_eq!(inside(&g, 400.0, 300.0, 300.0, 100.0), 20);
}

#[test]
fn each_region_lays_its_lines_out_at_its_own_width() {
    // The first region holds a line of two words; the second, 300px wide,
    // takes the other three on one line.
    let lw = page(
        ".r { height: 20px } #a { width: 100px; left: 0px; top: 300px } #b { width: 300px; \
         left: 300px; top: 300px }",
        &format!(
            "<div class=\"story\">{STORY}</div><div class=\"r\" id=\"a\"></div><div class=\"r\" \
             id=\"b\"></div>"
        ),
    );
    let g = glyphs(&lw);
    assert_eq!(inside(&g, 0.0, 300.0, 100.0, 20.0), 8, "{g:?}");
    assert_eq!(
        inside(&g, 300.0, 300.0, 300.0, 20.0),
        12,
        "the rest re-wraps on the wider region's one line: {g:?}"
    );
}

#[test]
fn a_regions_shape_inside_shapes_its_lines() {
    // A 200 x 60 frame with a hole in the middle of its first two lines.
    let lw = page(
        ".r { width: 200px; height: 60px; left: 100px; top: 100px; shape-inside: \
         path(evenodd, 'M0 0H200V60H0Z M80 0H120V40H80Z') }",
        &format!("<div class=\"story\">{STORY} ffff gggg</div><div class=\"r\"></div>"),
    );
    let g = glyphs(&lw);
    assert_eq!(g.len(), 28, "the frame holds the story: {g:?}");
    for &(x, y) in &g {
        assert!(
            y >= 140.0 - 0.01 || x + GLYPH <= 180.0 + 0.01 || x >= 220.0 - 0.01,
            "a glyph at {x}, {y} lies in the hole"
        );
        assert!(
            (100.0..=300.0 - GLYPH).contains(&x) && (100.0..=160.0 - LINE).contains(&y),
            "a glyph at {x}, {y} lies outside the frame"
        );
    }
}

#[test]
fn a_forced_region_break_starts_the_next_region() {
    let lw = page(
        ".r { width: 300px; height: 100px } #a { left: 0px; top: 300px } #b { left: 400px; \
         top: 300px } .next { break-before: region }",
        "<div class=\"story\"><p>aaaa</p><p class=\"next\">bbbb</p></div><div class=\"r\" \
         id=\"a\"></div><div class=\"r\" id=\"b\"></div>",
    );
    let g = glyphs(&lw);
    assert_eq!(inside(&g, 0.0, 300.0, 300.0, 100.0), 4, "{g:?}");
    assert_eq!(
        inside(&g, 400.0, 300.0, 300.0, 100.0),
        4,
        "the paragraph after the break starts the second region: {g:?}"
    );
}

/// A one-line paragraph, then one of three lines, through a first region
/// three lines tall and a second one.
fn widows_and_orphans(rule: &str) -> (usize, usize) {
    let lw = page(
        &format!(
            ".r {{ width: 50px; height: 60px }} #a {{ left: 0px; top: 300px }} #b {{ left: \
             400px; top: 300px; height: 200px }} p {{ {rule} }}"
        ),
        "<div class=\"story\"><p>aaaa</p><p>bbbb cccc dddd</p></div><div class=\"r\" \
         id=\"a\"></div><div class=\"r\" id=\"b\"></div>",
    );
    let g = glyphs(&lw);
    (
        inside(&g, 0.0, 300.0, 50.0, 60.0),
        inside(&g, 400.0, 300.0, 50.0, 200.0),
    )
}

#[test]
fn a_paragraph_keeps_two_lines_on_each_side_of_a_region_break() {
    // Two lines of the second paragraph would fit the first region, but the
    // third alone would be a widow; one line alone there would be an orphan:
    // the paragraph goes to the next region whole (widows and orphans: 2).
    assert_eq!(widows_and_orphans(""), (4, 12));
}

#[test]
fn widows_and_orphans_of_one_let_the_paragraph_break_anywhere() {
    assert_eq!(widows_and_orphans("widows: 1; orphans: 1"), (12, 4));
}

#[test]
fn the_flows_text_is_painted_in_its_regions_place() {
    // The story comes first in the document, the page with its background
    // and the region after it: the text is painted over the background.
    let lw = page(
        ".pg { position: relative; width: 400px; height: 300px; background: #ffeeee } .r { \
         width: 300px; height: 100px; left: 50px; top: 50px }",
        &format!("<div class=\"story\">{STORY}</div><div class=\"pg\"><div class=\"r\"></div></div>"),
    );
    let all = items(&lw);
    let background = all
        .iter()
        .position(|item| matches!(item, DisplayListItem::Rect { .. }))
        .expect("the page's background is painted");
    let text = all
        .iter()
        .position(|item| matches!(item, DisplayListItem::Text { .. }))
        .expect("the story is painted");
    assert!(
        background < text,
        "the background ({background}) is painted before the story's text ({text})"
    );
}
