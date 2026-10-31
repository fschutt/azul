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

// One notice per user action: a drop, a paste or a download raises one, which says what the
// action does and then, in its place, how it went.

use crate::{
    fileops::{SourceItem, TransferKind},
    transfer_actions::{Ended, TransferActions},
};

fn file(name: &str) -> SourceItem {
    SourceItem {
        key: name.to_string(),
        is_folder: false,
        size: Some(3),
    }
}

fn ran(done: usize) -> Ended {
    Ended::Ran {
        done,
        skipped: 0,
        cancelled: false,
        failed: false,
    }
}

#[test]
fn one_dropped_file_is_one_notice_from_its_upload_to_done() {
    let (mut actions, mut notices) = (TransferActions::default(), Notices::default());
    let drop = actions.begin();
    let started = notices.report(drop, actions.queued(drop, TransferKind::Upload, &[file("a.txt")], "docs"));
    assert_eq!(
        said(&notices),
        ["azdrive-transfer-label(kind=upload, count=1, name=a.txt, target=docs)…"]
    );
    notices.apply(drop, actions.ended(drop, ran(1)));
    assert_eq!(said(&notices), ["azdrive-transfer-done(count=1)"], "the same notice, done");
    assert_eq!(notices.list[0].message.kind, MessageKind::Success);
    // The "Uploading" toast's clock may still run out: it takes out its own id only.
    assert_ne!(notices.list[0].id, started);
    notices.dismiss(started);
    assert_eq!(notices.list.len(), 1);
}

#[test]
fn files_dropped_from_two_folders_are_one_notice() {
    let (mut actions, mut notices) = (TransferActions::default(), Notices::default());
    let drop = actions.begin();
    // One transfer per source folder.
    notices.report(drop, actions.queued(drop, TransferKind::Upload, &[file("a.txt"), file("b.txt")], "docs"));
    notices.report(drop, actions.queued(drop, TransferKind::Upload, &[file("c.txt")], "docs"));
    assert_eq!(
        said(&notices),
        ["azdrive-transfer-label(kind=upload, count=3, name=, target=docs)…"]
    );
    notices.apply(drop, actions.ended(drop, ran(2)));
    assert_eq!(notices.list.len(), 1, "the second folder's upload still runs");
    assert!(said(&notices)[0].starts_with("azdrive-transfer-label"));
    notices.apply(drop, actions.ended(drop, ran(1)));
    assert_eq!(said(&notices), ["azdrive-transfer-done(count=3)"]);
}

#[test]
fn a_paste_is_one_notice_and_another_action_its_own() {
    let (mut actions, mut notices) = (TransferActions::default(), Notices::default());
    let paste = actions.begin();
    notices.report(paste, actions.queued(paste, TransferKind::Copy, &[file("a"), file("b")], "docs"));
    let other = actions.begin();
    notices.report(other, actions.queued(other, TransferKind::Move, &[file("c")], "old"));
    notices.apply(paste, actions.ended(paste, ran(2)));
    assert_eq!(
        said(&notices),
        [
            "azdrive-transfer-done(count=2)",
            "azdrive-transfer-label(kind=move, count=1, name=c, target=old)…"
        ],
        "each action keeps its place"
    );
}

#[test]
fn a_download_says_once_where_it_went() {
    let (mut actions, mut notices) = (TransferActions::default(), Notices::default());
    let download = actions.begin();
    notices.report(
        download,
        actions.queued(download, TransferKind::Download, &[file("a.pdf")], "/home/u/Downloads"),
    );
    notices.apply(download, actions.ended(download, ran(1)));
    assert_eq!(
        said(&notices),
        ["azdrive-downloaded(count=1, folder=/home/u/Downloads)"]
    );
}

#[test]
fn a_failed_action_leaves_its_words_to_the_info_bar() {
    let (mut actions, mut notices) = (TransferActions::default(), Notices::default());
    let drop = actions.begin();
    notices.report(drop, actions.queued(drop, TransferKind::Upload, &[file("a")], "docs"));
    notices.apply(drop, actions.ended(drop, Ended::Failed));
    assert!(notices.is_empty());
}

#[test]
fn an_action_whose_notice_left_says_how_it_went_as_the_newest() {
    let (mut actions, mut notices) = (TransferActions::default(), Notices::default());
    let drop = actions.begin();
    let started = notices.report(drop, actions.queued(drop, TransferKind::Upload, &[file("a")], "docs"));
    notices.push(info("x"));
    // A long upload: its toast's time ran out.
    notices.dismiss(started);
    notices.apply(drop, actions.ended(drop, ran(1)));
    assert_eq!(said(&notices), ["x", "azdrive-transfer-done(count=1)"]);
}
