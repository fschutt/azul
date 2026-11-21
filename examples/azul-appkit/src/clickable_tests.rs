//! A clickable row or chip is a control the keyboard reaches: a tab stop with a name, whose
//! callback runs on a click - a press and release on it, or Enter / Space while it has the
//! focus. A bare MouseUp callback answered the mouse only: keyboard activation dispatches a
//! click, which a MouseUp callback never hears. A screen reader says what it is: a button,
//! unless the app names another role (a navigation entry is a link or a tab, a menu's row a
//! menu item).

use azul::{
    dom::{AccessibilityInfo, AccessibilityRole, TabIndex},
    option::{OptionString, OptionTabIndex},
    prelude::*,
};

use crate::pieces::{clickable, clickable_as, Clickable};

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

/// What the node declares to a screen reader. The binding keeps it behind an opaque pointer:
/// the engine's boxed `AccessibilityInfo`, whose layout the binding's struct shares.
fn accessibility(dom: &Dom) -> Option<&AccessibilityInfo> {
    // SAFETY: the pointer is null or the node's live `Box<AccessibilityInfo>`, which lives as
    // long as `dom`; the binding's `AccessibilityInfo` is its `repr(C)` twin.
    unsafe { dom.root.accessibility.cast::<AccessibilityInfo>().as_ref() }
}

/// The role and name the node declares (`Unknown` and no name when it declares nothing).
fn role_and_name(dom: &Dom) -> (AccessibilityRole, Option<String>) {
    accessibility(dom).map_or((AccessibilityRole::Unknown, None), |info| {
        let name = match &info.accessibility_name {
            OptionString::Some(name) => Some(name.as_str().to_string()),
            OptionString::None => None,
        };
        (info.role, name)
    })
}

#[test]
fn a_clickable_piece_is_announced_as_a_button_with_its_name() {
    let dom = clickable(Dom::create_div(), "Tokyo", RefAny::new(()), nothing);
    assert_eq!(
        role_and_name(&dom),
        (AccessibilityRole::PushButton, Some("Tokyo".to_string())),
        "a clickable row with no role reads as a name with nothing to say what it is"
    );
}

#[test]
fn a_clickable_piece_can_be_announced_as_the_role_the_app_names() {
    let link = clickable_as(
        Dom::create_div(),
        "Servers",
        AccessibilityRole::Link,
        RefAny::new(()),
        nothing,
    );
    assert_eq!(role_and_name(&link), (AccessibilityRole::Link, Some("Servers".to_string())));
    let tab = Dom::create_div().clickable_as(
        "Mail",
        AccessibilityRole::PageTab,
        RefAny::new(()),
        nothing,
    );
    assert_eq!(role_and_name(&tab), (AccessibilityRole::PageTab, Some("Mail".to_string())));
    assert!(
        matches!(tab.root.get_tab_index(), OptionTabIndex::Some(TabIndex::Auto)),
        "another role keeps the tab stop"
    );
}
