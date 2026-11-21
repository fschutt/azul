//! A person without a name is "Guest" in the window's language, while the name the service and
//! the peers get stays the service's one word.

use azul_appkit::l10n::set_locale;

use crate::{chatroom, ui};

#[test]
fn a_nameless_person_is_called_guest_in_the_windows_language() {
    crate::l10n::in_english();
    assert_eq!(ui::person_name(chatroom::GUEST), "Guest");
    assert_eq!(ui::shown_name(chatroom::GUEST, false), "Guest");
    set_locale("de-DE");
    assert_eq!(ui::person_name(chatroom::GUEST), "Gast");
    assert_eq!(ui::shown_name(chatroom::GUEST, false), "Gast");
    assert_eq!(ui::person_name("Ada"), "Ada", "a name given is shown as given");
    set_locale("en-US");
}

#[test]
fn the_name_sent_for_a_nameless_person_stays_the_services_word() {
    crate::l10n::in_english();
    set_locale("de-DE");
    assert_eq!(chatroom::clean_name("  "), "Guest");
    set_locale("en-US");
}
