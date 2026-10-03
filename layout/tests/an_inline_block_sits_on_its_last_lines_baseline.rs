//! An inline-block sits on its last line's baseline (CSS 2.2 s10.8.1: "The
//! baseline of an 'inline-block' is the baseline of its last line box in the
//! normal flow").
//!
//! WPT sweep (scripts/REFCI_2026_09_30.md, E-INLINE: "an inline-block breaks
//! onto its own line and its baseline is wrong", inline-block-baseline-001 /
//! 002 / 005): `p { line-height: 5 } span { display: inline-block }` drew the
//! span's word some 40px ABOVE the line's other words, on the 2026-10-03
//! prebuilt dylib. Two causes, both read off the code:
//!
//! - a block container never reported a baseline (`layout_bfc` set
//!   `output.baseline = None`, "would happen here in a full implementation"),
//!   so an inline-block whose text sits in a block child was aligned by its
//!   bottom edge;
//! - an inline formatting context's `last_baseline()` returned the last item's
//!   ASCENT, not where its baseline lies: it ignored the item's position, so
//!   the half-leading of a tall line (and every line above the last) was
//!   missing.
//!
//! Font-independent: an empty inline-block (whose baseline is its bottom
//! edge) marks the line's baseline; the text's ascent and descent differ by
//! well under 20px at 15px, so the centre of a 75px line box around the text
//! lies within 10px of the baseline.
//! Not compiled by the author (house rule); expected RED.

use crate::painted::{painted, BLUE, RED};

fn centre_of_red_against_the_baseline(inline_block: &str) -> (f32, f32) {
    let page = painted(
        &format!(
            "<html><head><style>body {{ margin: 0; font-size: 15px; }} \
             p {{ margin: 0; line-height: 5; }} \
             .ib {{ display: inline-block; background: red; }} \
             .mark {{ display: inline-block; width: 10px; height: 10px; background: blue; }}\
             </style></head><body><p>X{inline_block}<i class=\"mark\"></i></p></body></html>"
        ),
        240,
        200,
    );
    let (_, ry0, _, ry1) = page
        .bounds_of(RED, 10)
        .expect("the red inline-block is painted");
    let (_, _, _, baseline) = page.bounds_of(BLUE, 10).expect("the blue mark is painted");
    ((ry0 + ry1) as f32 / 2.0, baseline as f32)
}

#[test]
fn an_inline_block_of_text_shares_the_lines_baseline() {
    let (centre, baseline) = centre_of_red_against_the_baseline("<span class=\"ib\">words</span>");
    assert!(
        (centre - baseline).abs() < 10.0,
        "the 75px line box of the inline-block's text is centred on the baseline (within the \
         font's ascent-descent difference): centre y={centre}, baseline y={baseline}"
    );
}

#[test]
fn an_inline_block_whose_text_is_in_a_block_shares_the_lines_baseline() {
    let (centre, baseline) =
        centre_of_red_against_the_baseline("<span class=\"ib\"><div>words</div></span>");
    assert!(
        (centre - baseline).abs() < 10.0,
        "the last line box is found inside the inline-block's block child: centre y={centre}, \
         baseline y={baseline}"
    );
}
