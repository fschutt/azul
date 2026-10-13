//! A column of cards in a scrolling column keeps every card as tall as its
//! content.
//!
//! A box is as tall as its content laid out at its width: in the block axis
//! Chrome has one intrinsic size, whether a flex container asks an item for
//! its min-content or its max-content height. taffy asks a column's items
//! for their min-content height to find how far they may shrink, and a
//! wrapping column answered with every option in a column of its own. So the
//! page of cards in a scroller shrank to that height instead of scrolling,
//! every card was squeezed and wrapped its options beside each other, and an
//! animation that changed one card's height changed all the others too
//! (AzWidgets' settings page). Option boxes have fixed sizes, so the numbers
//! do not depend on the fonts.

use crate::table_markup::{body, near, rect};

fn option() -> &'static str {
    "<div style=\"width: 50px; height: 20px; flex-shrink: 0\"></div>"
}

#[test]
fn a_column_of_cards_in_a_scroller_keeps_each_card_as_tall_as_its_options_stacked() {
    let cards: String = (0..10)
        .map(|i| {
            format!(
                "<div id=\"card{i}\" style=\"display: flex; flex-direction: column\">\
                 <div style=\"display: flex; flex-direction: column; flex-wrap: wrap\">\
                 {o}{o}{o}</div></div>",
                o = option()
            )
        })
        .collect();
    let lw = body(&format!(
        "<div style=\"display: flex; flex-direction: column; height: 300px; overflow-y: auto\">\
         <div id=\"page\" style=\"display: flex; flex-direction: column\">{cards}</div></div>"
    ));
    for i in 0..10 {
        let card = rect(&lw, &format!("card{i}"));
        assert!(
            near(card.size.height, 60.0, 0.5),
            "card {i} is its three options stacked, 60px, not squeezed: {}",
            card.size.height
        );
    }
    let page = rect(&lw, "page");
    assert!(
        near(page.size.height, 600.0, 0.5),
        "the page is as tall as its cards and scrolls: {}",
        page.size.height
    );
}
