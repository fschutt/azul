//! The phone's action sheet: which commands destroy, and when the sheet is there at all.

use azul::prelude::RefAny;

use super::*;
use crate::{fixtures_test::erp, store::Kind};

fn action(kind: ActionKind) -> ActionSpec {
    ActionSpec {
        label: String::from("x"),
        kind,
        icon: String::new(),
        primary: false,
        condition: None,
    }
}

#[test]
fn only_delete_destroys() {
    assert!(destroys(&action(ActionKind::Named(String::from("delete")))));
    assert!(!destroys(&action(ActionKind::Named(String::from(
        "check_in"
    )))));
    assert!(!destroys(&action(ActionKind::Link(String::from(
        "/accounting/assets/x/dispose"
    )))));
    assert!(!destroys(&action(ActionKind::Cancel)));
}

#[test]
fn a_desktop_or_a_closed_sheet_draws_nothing() {
    let mut s = erp();
    s.state.open("/accounting/assets");
    let id = s.state.book.assets[0].id.clone();
    s.state.select(Kind::Asset, &id);
    let app = RefAny::new(());
    assert!(sheet(&s, &app, true).is_none(), "the sheet is closed");
    s.sheet_open = true;
    assert!(sheet(&s, &app, false).is_none(), "a desktop has the ribbon");
    assert!(
        sheet(&s, &app, true).is_some(),
        "open on a phone for a selected record"
    );
    s.state.clear_selection();
    assert!(sheet(&s, &app, true).is_none(), "no record, no sheet");
}
