//! The window's one dialog: a dialog that shows something only this once (a new drive's
//! recovery code, a join code, the shares made for trusted contacts) closes only when its user
//! closes it. Whatever the app does meanwhile - a busy setup that hands over to its sheet late,
//! the answer of other work, a question of the sync, a close meant for the dialog before - waits
//! behind it or passes it by.

use azul_storage::crypto::Zeroizing;

use crate::{
    encryption::{Dialog, Sheet},
    popup_slot::PopupSlot,
    recovery_contacts::Page,
    Popup,
};

fn busy() -> Popup {
    Popup::Encryption(Dialog::Busy {
        title: String::from("Setting up the drive"),
        text: String::from("Making the keys"),
    })
}

fn sheet(drive_id: &str) -> Popup {
    let code = Zeroizing::new(String::from("ABCDE-FGHJK-MNPQR-STVWX-YZ2345"));
    Popup::Encryption(Dialog::Sheet(Sheet::new(drive_id, code).for_new_drive()))
}

fn message(title: &str) -> Popup {
    Popup::Encryption(Dialog::Message {
        title: String::from(title),
        text: String::new(),
    })
}

fn shows_sheet_of(slot: &PopupSlot, drive_id: &str) -> bool {
    matches!(slot.as_ref(), Some(Popup::Encryption(Dialog::Sheet(s))) if s.drive_id == drive_id)
}

fn title_of(slot: &PopupSlot) -> Option<&str> {
    match slot.as_ref() {
        Some(Popup::Encryption(Dialog::Message { title, .. })) => Some(title.as_str()),
        _ => None,
    }
}

#[test]
fn a_new_drives_recovery_sheet_stays_until_its_user_closes_it() {
    let mut slot = PopupSlot::default();
    slot.open(busy());
    // The setup's answer: its sheet in place of the busy dialog.
    slot.open_when_free(sheet("d_1"));
    assert!(shows_sheet_of(&slot, "d_1"));
    // What the app does meanwhile: a close meant for the busy dialog, a take of whatever shows,
    // the answer of other work, a question of the sync.
    slot.close();
    assert!(slot.take().is_none(), "the sheet is not taken to answer another dialog");
    slot.open(message("Restored"));
    slot.open(message("A conflict"));
    assert!(shows_sheet_of(&slot, "d_1"), "nothing but its user closes the sheet");
    assert!(!slot.show_waiting(), "nothing shows over the sheet");
    slot.user_closed();
    assert!(slot.show_waiting());
    assert_eq!(title_of(&slot), Some("Restored"), "what waited shows next, oldest first");
    slot.close();
    assert!(slot.show_waiting());
    assert_eq!(title_of(&slot), Some("A conflict"));
    slot.close();
    assert!(!slot.show_waiting());
    assert!(slot.is_none());
}

#[test]
fn a_late_hand_over_from_the_busy_setup_still_shows_the_sheet_until_its_user_closes_it() {
    // The setup ran long under load: meanwhile other work ended, closed the busy dialog it took
    // for its own and showed its answer. The sheet comes after that.
    let mut slot = PopupSlot::default();
    slot.open(busy());
    slot.close();
    slot.open(message("Moved into the encryption"));
    slot.open_when_free(sheet("d_1"));
    assert_eq!(
        title_of(&slot),
        Some("Moved into the encryption"),
        "the answer showing is not taken away by the sheet either"
    );
    slot.close();
    assert!(slot.show_waiting());
    assert!(shows_sheet_of(&slot, "d_1"), "the sheet shows once the answer is closed");
    // A progress dialog opened behind the sheet does not wait for it: its work answers on its
    // own, and a busy dialog shown after its work ended would never close.
    slot.open(busy());
    slot.close();
    assert!(shows_sheet_of(&slot, "d_1"));
    slot.user_closed();
    assert!(slot.is_none());
    assert!(!slot.show_waiting());
}

#[test]
fn the_busy_setup_hands_its_place_to_its_recovery_sheet_at_once() {
    // The sheet takes the busy dialog's place in the same callback: the page never shows the
    // slot empty between them, so the dialog's node (one fixed id) keeps its window.
    let mut slot = PopupSlot::default();
    slot.open(busy());
    slot.open_when_free(sheet("d_1"));
    assert!(shows_sheet_of(&slot, "d_1"));
    assert_eq!(slot.waiting().count(), 0, "nothing waits: the sheet is the busy dialog's answer");
}

#[test]
fn a_second_sheet_waits_for_the_first() {
    let mut slot = PopupSlot::default();
    slot.open_when_free(sheet("d_1"));
    slot.open_when_free(sheet("d_2"));
    assert!(shows_sheet_of(&slot, "d_1"));
    assert!(slot.waiting().any(|p| matches!(
        p,
        Popup::Encryption(Dialog::Sheet(s)) if s.drive_id == "d_2"
    )));
    slot.user_closed();
    assert!(slot.show_waiting());
    assert!(shows_sheet_of(&slot, "d_2"));
}

#[test]
fn the_dialogs_that_show_a_secret_only_once_must_be_seen() {
    assert!(sheet("d_1").must_be_seen());
    assert!(Popup::Encryption(Dialog::Invite {
        drive_id: String::from("d_1"),
        code: Zeroizing::new(String::from("join")),
        note: String::new(),
    })
    .must_be_seen());
    assert!(Popup::Encryption(Dialog::Contacts(Page::Made {
        drive_id: String::from("d_1"),
        shares: Vec::new(),
        note: String::new(),
    }))
    .must_be_seen());
    assert!(Popup::Encryption(Dialog::Contacts(Page::Rebuilt {
        drive_id: String::from("d_1"),
        code: Zeroizing::new(String::from("ABCDE")),
        until: None,
        note: String::new(),
    }))
    .must_be_seen());
    assert!(!busy().must_be_seen());
    assert!(!message("Done").must_be_seen());
    assert!(!Popup::Transfers { auto: true }.must_be_seen());
}
