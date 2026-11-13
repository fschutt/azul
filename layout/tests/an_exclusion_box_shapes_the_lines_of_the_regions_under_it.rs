//! An exclusion box shapes the lines of the regions under it: an absolutely
//! positioned box with `wrap-flow` other than `auto` is an exclusion, and
//! the lines a region of a named flow lays out in the box's containing
//! block flow around its area as `wrap-flow` says, unless the region, or a
//! box between it and that containing block, has `wrap-through: none`.
//!
//! CSS Exclusions 1. Neither property was parsed: a positioned box lay over
//! the text of a frame under it, the way a picture placed over a text frame
//! hides its words instead of pushing them aside.
//!
//! Glyphs here are Azul Mock Mono at 20px: 10px wide, in 20px line boxes.
//! The region is 200 x 100 at (100, 100); the box is 60 x 40 at (170, 100),
//! over the middle of the region's first two lines.

use crate::{
    mono_lines::{inked_pens, FONT_SIZE, GLYPH, LINE},
    table_markup::laid_out,
};

/// Eight words of four letters: a 200px line holds four of them.
const STORY: &str = "aaaa bbbb cccc dddd eeee ffff gggg hhhh";

/// The top left corners (x, line box top) of every inked glyph of the story
/// laid out through the region with `.x { <box_rule> }` on the box, and
/// `region_rule` on the region.
fn glyphs_with(box_rule: &str, region_rule: &str) -> Vec<(f32, f32)> {
    glyphs_with_story(box_rule, region_rule, "")
}

/// As [`glyphs_with`], with `story_rule` on the story's paragraph too.
fn glyphs_with_story(box_rule: &str, region_rule: &str, story_rule: &str) -> Vec<(f32, f32)> {
    let lw = laid_out(
        &format!(
            "<html><head><style>* {{ margin: 0; padding: 0 }} body {{ font-family: 'Azul Mock \
             Mono'; font-size: 20px; line-height: 20px }} .story {{ flow-into: art; \
             {story_rule} }} .pg {{ position: relative; width: 800px; height: 600px }} .r {{ \
             position: absolute; left: 100px; top: 100px; width: 200px; height: 100px; \
             flow-from: art; {region_rule} }} .x {{ position: absolute; left: 170px; top: 100px; \
             width: 60px; height: 40px; {box_rule} }}</style></head><body><div \
             class=\"story\">{STORY}</div><div class=\"pg\"><div class=\"r\"></div><div \
             class=\"x\"></div></div></body></html>"
        ),
        800.0,
        600.0,
    );
    inked_pens(&lw)
        .into_iter()
        .map(|(x, baseline)| (x, baseline - FONT_SIZE * 0.8))
        .collect()
}

/// The glyphs of `glyphs` that overlap the box's area, 170..230 x 100..140.
fn over_the_box(glyphs: &[(f32, f32)]) -> Vec<(f32, f32)> {
    glyphs
        .iter()
        .copied()
        .filter(|&(x, y)| {
            x < 230.0 - 0.01 && x + GLYPH > 170.0 + 0.01 && y < 140.0 - 0.01 && y + LINE > 100.0 + 0.01
        })
        .collect()
}

/// The glyphs of `glyphs` on the lines beside the box (the first two).
fn beside_the_box(glyphs: &[(f32, f32)]) -> Vec<(f32, f32)> {
    glyphs.iter().copied().filter(|&(_, y)| y < 140.0 - 0.01).collect()
}

#[test]
fn a_box_of_wrap_flow_auto_is_no_exclusion() {
    let g = glyphs_with("wrap-flow: auto", "");
    assert!(!over_the_box(&g).is_empty(), "the lines run under the box: {g:?}");
}

#[test]
fn a_box_of_wrap_flow_both_keeps_a_regions_lines_off_it() {
    let g = glyphs_with("wrap-flow: both", "");
    assert_eq!(g.len(), 32, "the whole story is drawn: {g:?}");
    assert_eq!(over_the_box(&g), Vec::new(), "no glyph lies under the box: {g:?}");
    let right_of_it = beside_the_box(&g)
        .into_iter()
        .filter(|&(x, _)| x >= 230.0 - 0.01)
        .count();
    assert_eq!(right_of_it, 8, "a word on each side of the box on both lines: {g:?}");
}

#[test]
fn a_box_of_wrap_flow_clear_leaves_no_line_beside_it() {
    let g = glyphs_with("wrap-flow: clear", "");
    assert_eq!(beside_the_box(&g), Vec::new(), "the lines start below the box: {g:?}");
}

#[test]
fn a_box_of_wrap_flow_start_leaves_its_end_side_empty() {
    let g = glyphs_with("wrap-flow: start", "");
    let beside = beside_the_box(&g);
    assert!(!beside.is_empty(), "{g:?}");
    assert!(
        beside.iter().all(|&(x, _)| x + GLYPH <= 170.0 + 0.01),
        "the lines beside the box keep to its left, the start side: {g:?}"
    );
}

#[test]
fn a_box_of_wrap_flow_end_leaves_its_start_side_empty() {
    let g = glyphs_with("wrap-flow: end", "");
    let beside = beside_the_box(&g);
    assert!(!beside.is_empty(), "{g:?}");
    assert!(
        beside.iter().all(|&(x, _)| x >= 230.0 - 0.01),
        "the lines beside the box keep to its right, the end side: {g:?}"
    );
}

#[test]
fn a_region_of_wrap_through_none_runs_under_the_boxes_around_it() {
    let g = glyphs_with("wrap-flow: both", "wrap-through: none");
    assert!(!over_the_box(&g).is_empty(), "the region's lines ignore the box: {g:?}");
}

#[test]
fn a_paragraph_of_wrap_through_none_runs_under_the_boxes_around_its_region() {
    let g = glyphs_with_story("wrap-flow: both", "", "wrap-through: none");
    assert!(!over_the_box(&g).is_empty(), "the paragraph's lines ignore the box: {g:?}");
}

#[test]
fn an_exclusion_boxs_shape_outside_is_its_area() {
    // Only the box's upper half is its area: the second line runs under it.
    let g = glyphs_with("wrap-flow: both; shape-outside: inset(0 0 20px 0)", "");
    let over = over_the_box(&g);
    assert!(!over.is_empty(), "{g:?}");
    assert!(
        over.iter().all(|&(_, y)| y >= 120.0 - 0.01),
        "only the second line runs under the box: {g:?}"
    );
}

#[test]
fn a_box_shapes_only_the_regions_in_its_containing_block() {
    // The box's containing block is another positioned box over the page:
    // the region is not in it, so the box does not touch its lines.
    let lw = laid_out(
        &format!(
            "<html><head><style>* {{ margin: 0; padding: 0 }} body {{ font-family: 'Azul Mock \
             Mono'; font-size: 20px; line-height: 20px }} .story {{ flow-into: art }} .layer {{ \
             position: absolute; left: 0px; top: 0px; width: 800px; height: 600px }} .r {{ \
             position: absolute; left: 100px; top: 100px; width: 200px; height: 100px; \
             flow-from: art }} .x {{ position: absolute; left: 170px; top: 100px; width: 60px; \
             height: 40px; wrap-flow: both }}</style></head><body><div \
             class=\"story\">{STORY}</div><div class=\"layer\"><div class=\"r\"></div></div><div \
             class=\"layer\"><div class=\"x\"></div></div></body></html>"
        ),
        800.0,
        600.0,
    );
    let g: Vec<(f32, f32)> = inked_pens(&lw)
        .into_iter()
        .map(|(x, baseline)| (x, baseline - FONT_SIZE * 0.8))
        .collect();
    assert!(!over_the_box(&g).is_empty(), "{g:?}");
}
