//! The sheet of a document row: which row asks for what.

use std::path::PathBuf;

use super::*;
use crate::model::DocumentModel;

#[test]
fn the_rows_show_close_and_switch_collaboration_for_the_document_the_sheet_is_for() {
    assert_eq!(command_of(SHOW, 2, false), Some(Command::Switch(2)));
    assert_eq!(command_of(CLOSE, 2, true), Some(Command::Close(2)));
    assert_eq!(command_of(0, 2, false), None);
    assert_eq!(command_of(99, 2, false), None);
}

#[test]
fn the_collaboration_row_asks_for_the_state_the_document_is_not_in() {
    assert_eq!(command_of(COLLAB, 1, false), Some(Command::Collab(1, true)));
    assert_eq!(command_of(COLLAB, 1, true), Some(Command::Collab(1, false)));
    assert_ne!(collab_key(true), collab_key(false), "the row says what it would do");
}

#[test]
fn a_desktop_has_no_sheet_and_a_phone_has_one_only_while_a_row_was_pressed_and_held() {
    crate::l10n::in_english();
    let mut st = AppState::new(RefAny::new(()), PathBuf::from("data"), false);
    st.open.push(DocumentModel::from_markdown("a".to_string(), "# Alpha\n"));
    assert!(sheet(&st, &RefAny::new(()), true).is_none(), "no row was pressed");
    st.sheet_doc = Some(0);
    assert!(sheet(&st, &RefAny::new(()), false).is_none(), "a desktop has the ribbon and the keys");
    assert!(sheet(&st, &RefAny::new(()), true).is_some());
    st.sheet_doc = Some(5);
    assert!(sheet(&st, &RefAny::new(()), true).is_none(), "a row that is not there");
}
