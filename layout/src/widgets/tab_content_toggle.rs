//! The tab strip's chevron: ONE per window, at the top-left corner of the tab row, folds the content of
//! the tabs away (`^` while the content shows, `v` while it is folded).
//!
//! The state is the WINDOW's ([`azul_core::window::WindowFlags::tab_content_hidden`]), not a widget's: a
//! click on the chevron flips the flag through the window's state and asks for a new `layout()`; every
//! `TabContent` and every ribbon band built for the window with `with_content_toggle(true)` reads it while
//! it is built ([`azul_core::callbacks::build_tab_content_hidden`]) and shows nothing, so the list under the
//! strip gets the room. An app may start a window with the flag set, read it back from the window's state,
//! or flip it with the e2e op `toggle_tab_content`.
//!
//! A window has exactly one chevron whatever its page holds: a strip that asks for it
//! (`TabHeader::with_content_toggle`, `Ribbon::with_content_toggle`) takes it from the build
//! ([`azul_core::callbacks::build_take_tab_content_toggle`]), and the strips after the first are told no.

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClassVec, TabIndex},
    events::{EventFilter, FocusEventFilter, HoverEventFilter},
    refany::{OptionRefAny, RefAny},
};
use azul_css::{AzString, OptionString};

use crate::callbacks::CallbackInfo;

/// The class of the chevron.
pub const CONTENT_TOGGLE_CLASS: &str = "__azul-tab-content-toggle";

static CONTENT_TOGGLE_CLASSES: &[IdOrClass] = &[IdOrClass::Class(AzString::from_const_str(CONTENT_TOGGLE_CLASS))];

/// The chevron's width, px: the room a tab row keeps at its left end for it.
pub const CONTENT_TOGGLE_PX: f32 = 32.0;

/// The chevron for a tab row `row` px tall, if this strip of the build may draw it: the window's one
/// chevron goes to the first strip that asks ([`azul_core::callbacks::build_take_tab_content_toggle`]).
#[must_use]
pub(crate) fn chevron_for_row(row: f32) -> Option<Dom> {
    azul_core::callbacks::build_take_tab_content_toggle()
        .then(|| chevron(azul_core::callbacks::build_tab_content_hidden(), row))
}

/// The chevron, `row` px tall and [`CONTENT_TOGGLE_PX`] wide, drawn as `hidden` says (`v` while the
/// content is folded, `^` while it shows): an icon button with the button role, the name and the
/// expanded / collapsed state. It has no colour of its own: in a row of the layout it stands on the
/// row's leather, and a touch strip puts a stretch of leather under it ([`tabs`'s `chevron_over_leather`])
/// so a pill that slides under it is covered.
#[must_use]
pub(crate) fn chevron(hidden: bool, row: f32) -> Dom {
    let name = if hidden {
        "Show the tab content"
    } else {
        "Hide the tab content"
    };
    let mut node = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CONTENT_TOGGLE_CLASSES))
        .with_child(Dom::create_icon(AzString::from_const_str(if hidden {
            "expand_more"
        } else {
            "expand_less"
        })))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::PushButton,
            accessibility_name: OptionString::Some(AzString::from_const_str(name)),
            states: AccessibilityStateVec::from_vec(vec![if hidden {
                AccessibilityState::Collapsed
            } else {
                AccessibilityState::Expanded
            }]),
            ..Default::default()
        })
        .with_callbacks(
            vec![
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Click),
                    callback: CoreCallback {
                        cb: on_chevron_click as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: RefAny::new(()),
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    callback: CoreCallback {
                        cb: on_chevron_key as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: RefAny::new(()),
                },
            ]
            .into(),
        );
    node.root.set_css(&format!(
        "display: flex; align-items: center; justify-content: center; flex-shrink: 0; \
         position: relative; width: {CONTENT_TOGGLE_PX}px; height: {row}px; cursor: pointer; \
         color: system:text; font-size: 20px;"
    ));
    node
}

/// Fold the window's tab content away, or show it again: the flag flips in the window's state, and the
/// strip, the panels and the bands are built anew.
pub fn toggle(info: &mut CallbackInfo) -> Update {
    let mut window = info.get_current_window_state().clone();
    window.flags.tab_content_hidden = !window.flags.tab_content_hidden;
    info.modify_window_state(window);
    Update::RefreshDom
}

/// A click on the chevron.
extern "C" fn on_chevron_click(_data: RefAny, mut info: CallbackInfo) -> Update {
    toggle(&mut info)
}

/// Enter or Space on the focused chevron.
extern "C" fn on_chevron_key(_data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::VirtualKeyCode as K;
    match crate::widgets::roving::plain_key(&info.get_current_keyboard_state()) {
        Some(K::Return | K::Space) => {
            info.prevent_default();
            toggle(&mut info)
        }
        _ => Update::DoNothing,
    }
}

#[cfg(test)]
#[path = "tab_content_toggle_test.rs"]
mod tab_content_toggle_test;
