//! What the File tab's location says.

use super::*;

fn mailbox(name: &str) -> Option<(usize, String)> {
    Some((0, name.to_string()))
}

#[test]
fn a_folder_is_the_name_and_its_mailbox_the_parent_line() {
    let place = place_of(0, "Mail", mailbox("ada@example.org"), Some(String::from("Inbox")));
    assert_eq!(place.name, "Inbox");
    assert_eq!(place.mailbox, mailbox("ada@example.org"));
}

#[test]
fn a_module_other_than_mail_is_the_name_under_the_mailbox() {
    let place = place_of(1, "Calendar", mailbox("ada@example.org"), Some(String::from("Inbox")));
    assert_eq!(place.name, "Calendar", "not the folder the mail view last showed");
    assert_eq!(place.mailbox, mailbox("ada@example.org"));
}

#[test]
fn a_mailbox_with_no_folder_is_its_own_name_and_has_no_parent_line_to_repeat_it() {
    let place = place_of(0, "Mail", mailbox("ada@example.org"), None);
    assert_eq!(place.name, "ada@example.org");
    assert_eq!(place.mailbox, None, "the name is the mailbox: a parent line would say it twice");
}

#[test]
fn a_window_with_no_mailbox_is_azmail() {
    let place = place_of(0, "Mail", None, None);
    assert_eq!(place, Place { mailbox: None, name: String::from("AzMail") });
    let calendar = place_of(2, "Contacts", None, None);
    assert_eq!(calendar.name, "Contacts");
}
