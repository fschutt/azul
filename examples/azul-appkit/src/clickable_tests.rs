//! A clickable row or chip is a control the keyboard reaches: a tab stop with a name, whose
//! callback runs on a click - a press and release on it, or Enter / Space while it has the
//! focus. A bare MouseUp callback answered the mouse only: keyboard activation dispatches a
//! click, which a MouseUp callback never hears.

use azul::{dom::TabIndex, option::OptionTabIndex, prelude::*};

use crate::pieces::clickable;

extern "C" fn nothing(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

#[test]
fn a_clickable_piece_is_a_tab_stop_that_hears_clicks_from_the_mouse_and_the_keyboard() {
    let dom = clickable(Dom::create_div(), "Tokyo", RefAny::new(()), nothing);
    assert!(
        matches!(dom.root.get_tab_index(), OptionTabIndex::Some(TabIndex::Auto)),
        "Tab must reach it"
    );
    let events: Vec<bool> = dom
        .root
        .callbacks
        .as_slice()
        .iter()
        .map(|cb| matches!(cb.event, EventFilter::Hover(HoverEventFilter::Click)))
        .collect();
    assert_eq!(events, vec![true], "one Click callback, no MouseUp beside it");
}
