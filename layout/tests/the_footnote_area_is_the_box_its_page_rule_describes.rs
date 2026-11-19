//! `@page { @footnote { ... } }` styles the footnote area.
//!
//! CSS Generated Content for Paged Media, "The footnote area": "A page area
//! that can be used to display footnotes is described in the page context
//! using an @footnote rule. This rule defines a box that, if used, will
//! contain all the footnote elements that appear on that page." Its
//! `border-top` is the footnote rule; "The separator (and the entire
//! footnote area) cannot be rendered on a page with no footnotes"; "The
//! max-height property on the footnote area limits the size of this area".
//!
//! The stylesheet parser skipped `@page` rules whole.
//!
//! Pages are 800 x 200; lines are 20px.

use azul_layout::solver3::display_list::{DisplayList, DisplayListItem};

use crate::a_footnote_is_drawn_at_the_bottom_of_the_page_of_its_anchor::{
    blocks, break_ys, long_note, note, pages, rects, STYLE,
};

const RED: (u8, u8, u8) = (255, 0, 0);

fn html(footnote_rule: &str, body: &str) -> String {
    format!(
        "<html><head><style>{STYLE} @page {{ @footnote {{ {footnote_rule} }} }}</style></head>\
         <body>{body}</body></html>"
    )
}

/// The borders drawn on `page`, as (top, height, left, width).
fn borders(page: &DisplayList) -> Vec<(f32, f32, f32, f32)> {
    page.items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Border { bounds, .. } => {
                let b = bounds.inner();
                Some((b.origin.y, b.size.height, b.origin.x, b.size.width))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_rule_above_the_notes_is_drawn_on_a_page_with_notes_and_on_no_other() {
    // 300px of flow, the note in its first line: two pages, the second
    // without notes.
    let (result, _, _) = pages(&html(
        "border-top: 2px solid #00ff00;",
        &format!("<p>one{}</p>{}", note("n1", 30), blocks(7, 40)),
    ));
    assert_eq!(result.pages.len(), 2);
    // The rule takes its 2px from the flow too.
    assert_eq!(break_ys(&result), vec![168.0]);
    assert_eq!(rects(&result.pages[0], RED), vec![(170.0, 200.0, 0.0, 800.0)]);
    assert_eq!(
        borders(&result.pages[0]),
        vec![(168.0, 32.0, 0.0, 800.0)],
        "the area's border box: the rule above the note, as wide as the page"
    );
    assert_eq!(borders(&result.pages[1]), vec![], "no rule on a page without notes");
}

#[test]
fn the_areas_margin_and_padding_move_the_notes_and_take_their_room_from_the_flow() {
    let (result, _, _) = pages(&html(
        "margin-top: 10px; padding-top: 4px; padding-bottom: 6px; padding-left: 20px; \
         padding-right: 30px;",
        &format!("<p>one{}</p>{}", note("n1", 30), blocks(7, 40)),
    ));
    // 200 - (10 + 4 + 30 + 6).
    assert_eq!(break_ys(&result), vec![150.0]);
    assert_eq!(
        rects(&result.pages[0], RED),
        vec![(164.0, 194.0, 20.0, 750.0)],
        "the note stands inside the area's padding"
    );
    assert_eq!(borders(&result.pages[0]), vec![], "no rule was asked for");
}

#[test]
fn a_max_height_splits_a_note_the_default_would_hold() {
    // Five lines (100px): under the default (four fifths of the page) the
    // note stands whole on its page.
    let body = format!("<p>one{}</p>{}", long_note("n1", 5), blocks(4, 50));
    let (whole, _, _) = pages(&html("", &body));
    assert_eq!(rects(&whole.pages[0], RED), vec![(100.0, 200.0, 0.0, 800.0)]);
    assert_eq!(rects(&whole.pages[1], RED), vec![]);
    // With `max-height: 60px` the area holds three lines: the other two go
    // on on the next page.
    let (split, _, _) = pages(&html("max-height: 60px;", &body));
    assert_eq!(break_ys(&split)[0], 140.0);
    assert_eq!(rects(&split.pages[0], RED), vec![(140.0, 200.0, 0.0, 800.0)]);
    assert_eq!(rects(&split.pages[1], RED), vec![(160.0, 200.0, 0.0, 800.0)]);
}

#[test]
fn a_document_without_a_footnote_rule_is_laid_out_as_before() {
    let body = format!("<p>one{}</p>{}", note("n1", 30), blocks(7, 40));
    let (result, _, _) = pages(&format!(
        "<html><head><style>{STYLE} @page {{ margin: 2cm; }}</style></head><body>{body}</body></html>"
    ));
    assert_eq!(break_ys(&result), vec![170.0]);
    assert_eq!(rects(&result.pages[0], RED), vec![(170.0, 200.0, 0.0, 800.0)]);
    assert_eq!(borders(&result.pages[0]), vec![]);
}
