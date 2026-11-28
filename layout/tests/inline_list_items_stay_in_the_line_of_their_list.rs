//! `display: inline list-item` items of a list sit in one line (CSS Display 3 s2.4, CSS Lists 3):
//! each shows its marker before its text and the whole is one line box, as the items of
//! `tests/e2e/layout23_proof.json` ("an inline list item shows its marker before its text on the
//! same line"): two lists of two items each, in a box 380 px wide, are two lines. The items
//! stacked, one line each (a list of height 40, not 20).
//!
//! Not compiled by the author (house rule).

use crate::text_rig::{lay_out, painted, window, MONO};

/// The distinct baselines the glyphs of the page stand on, top to bottom.
fn baselines(markup: &str) -> Vec<f32> {
    let mut lw = window();
    let parsed = azul_layout::xml::parse_xml(markup).expect("the page parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let dl = lay_out(&mut lw, dom, 400.0);
    let mut ys: Vec<f32> = painted(&dl).iter().map(|g| g.y).collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    ys
}

fn page(items_css: &str) -> String {
    format!(
        r#"<style>
            body {{ margin: 0; font-family: "{MONO}"; }}
            #page {{ width: 380px; }}
            ol, ul {{ margin: 10px; padding: 0; font-size: 16px; line-height: 20px; }}
            .inline {{ {items_css} }}
        </style>
        <div id="page">
        <ol id="numbered"><li class="inline">apples</li> <li class="inline">pears</li></ol>
        <ul id="bulleted"><li class="inline">plums</li> <li class="inline">figs</li></ul>
        </div>"#
    )
}

/// The scenario of the e2e file: markers inside, a margin after each item.
#[test]
fn the_items_of_a_list_are_one_line_and_the_two_lists_are_two() {
    let lines = baselines(&page(
        "display: inline list-item; list-style-position: inside; margin-right: 12px;",
    ));
    assert_eq!(lines.len(), 2, "one line per list, not one per item: {lines:?}");
    assert!(
        lines[1] - lines[0] >= 20.0 - 0.5,
        "the second list is below the line of the first: {lines:?}"
    );
}

/// The same without the margin and the position: nothing of those makes the items stack.
#[test]
fn the_margin_and_the_marker_position_do_not_stack_the_items() {
    for css in [
        "display: inline list-item;",
        "display: inline list-item; list-style-position: inside;",
        "display: inline list-item; margin-right: 12px;",
    ] {
        let lines = baselines(&page(css));
        assert_eq!(lines.len(), 2, "`{css}`: one line per list: {lines:?}");
    }
}
