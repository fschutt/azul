//! The empty states and the sample register's way in.

use super::*;
use crate::fixtures_test::{empty_erp, erp};

#[test]
fn every_view_type_has_a_name_for_the_line_that_says_it_has_no_screen() {
    let kinds = [
        ViewKind::Table,
        ViewKind::TableEmbedded,
        ViewKind::Form,
        ViewKind::FormModal,
        ViewKind::Detail,
        ViewKind::Wizard,
        ViewKind::Report,
        ViewKind::Dashboard,
        ViewKind::Kanban,
        ViewKind::Calendar,
        ViewKind::Inbox,
        ViewKind::SearchResults,
        ViewKind::Custom,
        ViewKind::Unknown,
    ];
    let names: Vec<&str> = kinds.iter().map(|k| kind_words(*k)).collect();
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), kinds.len(), "{names:?}");
    assert_eq!(kind_words(ViewKind::Dashboard), "dashboard");
    assert_eq!(kind_words(ViewKind::SearchResults), "search results");
}

#[test]
fn loading_the_sample_fills_an_empty_register_once() {
    let mut s = empty_erp();
    assert_eq!(s.state.book.assets.len(), 0);
    load_sample(&mut s);
    let n = s.state.book.assets.len();
    assert!(n >= 10, "{n} assets");
    let queued = s.state.queue.pending();
    assert!(queued > n, "every record is a file: {queued}");
    // the button is for an empty register: a second press adds nothing
    load_sample(&mut s);
    assert_eq!(s.state.book.assets.len(), n);
    assert_eq!(s.state.queue.pending(), queued);
}

#[test]
fn the_sample_never_overwrites_a_register_that_has_records() {
    let mut s = erp();
    let before = s.state.book.assets.len();
    let queued = s.state.queue.pending();
    load_sample(&mut s);
    assert_eq!(s.state.book.assets.len(), before);
    assert_eq!(s.state.queue.pending(), queued);
}
