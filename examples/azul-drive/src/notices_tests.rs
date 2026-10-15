//! The toast stack's notices: ids that never repeat, a dismiss by id, and only the newest few
//! kept.

use azul_appkit::l10n::Text;

use crate::{Message, MessageKind, Notices};

fn info(text: &str) -> Message {
    Message {
        kind: MessageKind::Info,
        text: Text::plain(text),
    }
}

fn said(notices: &Notices) -> Vec<String> {
    notices
        .list
        .iter()
        .map(|n| format!("{}", n.message.text))
        .collect()
}

#[test]
fn each_notice_takes_a_new_id() {
    let mut notices = Notices::default();
    let a = notices.push(info("a"));
    let b = notices.push(info("b"));
    assert_ne!(a, b);
    notices.dismiss(a);
    let c = notices.push(info("c"));
    assert!(c != a && c != b, "an id is never given twice");
}

#[test]
fn a_dismiss_takes_out_only_its_own_notice() {
    let mut notices = Notices::default();
    let a = notices.push(info("a"));
    notices.push(info("b"));
    notices.dismiss(a);
    assert_eq!(said(&notices), ["b"]);
    // Its timer and its close button may both fire: the second dismiss changes nothing.
    notices.dismiss(a);
    assert_eq!(said(&notices), ["b"]);
}

#[test]
fn only_the_newest_notices_are_kept() {
    let mut notices = Notices::default();
    for i in 0..10 {
        notices.push(info(&i.to_string()));
    }
    assert_eq!(said(&notices), ["4", "5", "6", "7", "8", "9"]);
}
